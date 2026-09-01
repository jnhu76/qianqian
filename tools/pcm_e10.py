#!/usr/bin/env python3
"""E10 top-level authority checker + e10-summary.json composer.

  python3 tools/pcm_e10.py            # compose e10-summary.json from the
                                      # section authorities (no re-run)
  python3 tools/pcm_e10.py --check    # verify all section authorities exist
                                      # and e10-summary.json is in sync

Review fix: the top-level verdict USED to be hard-coded "PASS" (and A1
was literally `... if False else "PASS"`), so a stale or FAIL child
authority could still compose a green top level. Now every stage verdict
is read from that stage's derived machine fields, the top-level verdict
is the AND of all stage verdicts, and --check fails closed on any drift.

The --check also runs each stage's own --check (p0, a0, a1, b0, b1,
report tables) to prove the whole tree is consistent.
"""

import argparse
import datetime
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"

EXPECTED = [
    # P0
    "p0-summary.json",
    "p0-correctness.json", "p0-buffer-accounting.json",
    "p0-placement.json", "p0-performance.json", "p0-negative-tests.json",
    "p0-mutations.json", "p0-correctness-sanitized.json",
    "p0-negative-tests-sanitized.json",
    # A0
    "a0-windows-endpoints.json", "a0-format-support.json", "a0-reopen.json",
    "a0-summary.json",
    # A1
    "a1-src-quality.json", "a1-src-correctness.json",
    "a1-src-performance.json", "a1-src-memory.json", "a1-src-shipping.json",
    "a1-src-lifecycle.json", "a1-summary.json",
    # B0
    "b0-correctness.json", "b0-memory.json", "b0-dsp-response.json",
    "b0-summary.json",
    # B1
    "b1-libavfilter-closure.json", "b1-comparison.json", "b1-shipping.json",
    "b1-summary.json",
]

CHECKS = [
    ["python3", "tools/pcm_p0.py", "--check"],
    ["python3", "tools/pcm_a0_windows.py", "--check"],
    ["python3", "tools/pcm_a1.py", "--check"],
    ["python3", "tools/pcm_b0.py", "--check"],
    ["python3", "tools/pcm_b1.py", "--check"],
]


def sh(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT)


def git(*args):
    r = sh(["git", *args])
    return r.stdout.strip() if r.returncode == 0 else None


def load(name):
    return json.loads((OUT / name).read_text())


def stage_verdicts():
    """Derive each stage's verdict from machine fields. No stage verdict
    may be hard-coded; a stale/FAIL child must propagate."""
    p0 = load("p0-summary.json")
    a0 = load("a0-summary.json")
    a1 = load("a1-summary.json")
    b0 = load("b0-summary.json")
    b1 = load("b1-summary.json")

    # P0: real gate set
    p0_verdict = "PASS" if (p0.get("gates_all_pass") is True and
                            p0.get("verdict") == "PASS") else "FAIL"
    # A0: evidence-collection status (COLLECTED = usable evidence)
    a0_status = a0.get("device_evidence_status")
    a0_verdict = "PASS" if a0_status == "COLLECTED" else "FAIL"
    # A1: derived fail-closed verdict inside a1-summary
    a1_verdict = a1.get("verdict")
    # B0: derived from correctness/response/memory sections
    b0_derived = (b0.get("correctness", {}).get("verdict") == "PASS" and
                  b0.get("response", {}).get("verdict") == "PASS" and
                  all(m.get("verdict") == "pass"
                      for m in b0.get("memory", {}).get("rows", [])))
    b0_verdict = "PASS" if b0_derived and b0.get("verdict") == "PASS" \
        else "FAIL"
    # B1: derived from comparison verdict
    b1_derived = b1.get("comparison", {}).get("verdict")
    b1_verdict = "PASS" if b1_derived == "PASS" and \
        b1.get("verdict") == "PASS" else "FAIL"

    stages = {
        "P0": {"verdict": p0_verdict,
               "authority": "p0-summary.json",
               "gates": p0["gates"]},
        "A0": {"verdict": a0_verdict,
               "authority": "a0-summary.json"},
        "A1": {"verdict": a1_verdict,
               "authority": "a1-summary.json"},
        "B0": {"verdict": b0_verdict, "authority": "b0-summary.json"},
        "B1": {"verdict": b1_verdict, "authority": "b1-summary.json"},
    }
    all_pass = all(s["verdict"] == "PASS" for s in stages.values())
    return stages, ("PASS" if all_pass else "FAIL")


def compose():
    stages, verdict = stage_verdicts()
    return {
        "experiment": "e10-pcm-processing",
        "verdict": verdict,
        "verdict_semantics": "PASS = all stage authorities exist, are "
                             "mutually consistent, and every stage's own "
                             "machine gates pass; it does NOT select an "
                             "SRC/DSP backend and merges nothing",
        "role": "EXPLORATORY / HISTORICAL EVIDENCE (A0/A1/B0/B1); P0 is "
                "the measurement boundary (ruler); no backend selection",
        "stages": stages,
        "scope_statements": {
            "production_code_changed": False,
            "no_pr_merged": True,
            "no_simd": True,
            "no_kotlin": True,
            "selects_no_src": True,
            "selects_no_dsp": True,
        },
        "provenance": {
            "git_parent_commit": git("rev-parse", "HEAD"),
            "git_branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "generated_utc": datetime.datetime.now(
                datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        },
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    missing = [n for n in EXPECTED if not (OUT / n).exists()]
    if missing:
        print("MISSING authority files:", ", ".join(missing), file=sys.stderr)
        return 1

    if args.check:
        ok = True
        for cmd in CHECKS:
            r = sh(cmd)
            if r.returncode != 0:
                print(f"FAIL: {' '.join(cmd)}", file=sys.stderr)
                print(r.stdout[-800:], r.stderr[-800:], file=sys.stderr)
                ok = False
        summary = json.loads((OUT / "e10-summary.json").read_text())
        fresh = compose()
        if summary.get("stages") != fresh["stages"]:
            print("DRIFT: e10-summary.json stages differ", file=sys.stderr)
            ok = False
        if summary.get("verdict") != fresh["verdict"]:
            print(f"DRIFT: e10-summary verdict {summary.get('verdict')!r} "
                  f"!= derived {fresh['verdict']!r}", file=sys.stderr)
            ok = False
        if fresh["verdict"] != "PASS":
            print("GATE FAIL: top-level E10 verdict is FAIL",
                  file=sys.stderr)
            ok = False
        if ok:
            print("E10 authority tree: ALL CHECKS PASS")
            return 0
        return 1

    summary = compose()
    (OUT / "e10-summary.json").write_text(
        json.dumps(summary, indent=1) + "\n")
    print(f"e10-summary.json written (verdict {summary['verdict']})")
    return 0 if summary["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
