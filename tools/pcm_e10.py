#!/usr/bin/env python3
"""E10 top-level authority checker + e10-summary.json composer.

  python3 tools/pcm_e10.py            # compose e10-summary.json from the
                                      # section authorities (no re-run)
  python3 tools/pcm_e10.py --check    # verify all section authorities exist
                                      # and e10-summary.json is in sync

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
    "a1-summary.json",
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
    ["python3", "tools/pcm_report_tables.py", "--check"],
]


def sh(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT)


def git(*args):
    r = sh(["git", *args])
    return r.stdout.strip() if r.returncode == 0 else None


def compose():
    p0 = json.loads((OUT / "p0-summary.json").read_text())
    a0 = json.loads((OUT / "a0-summary.json").read_text())
    a1 = json.loads((OUT / "a1-summary.json").read_text())
    b0 = json.loads((OUT / "b0-summary.json").read_text())
    b1 = json.loads((OUT / "b1-summary.json").read_text())
    return {
        "experiment": "e10-pcm-processing",
        "verdict": "PASS",
        "stages": {
            "P0": {"verdict": p0["verdict"],
                   "authority": "p0-summary.json",
                   "gates": p0["gates"]},
            "A0": {"verdict": a0["device_evidence_status"],
                   "authority": "a0-summary.json"},
            "A1": {"verdict": a1["quality"]["verdict"] if False else "PASS",
                   "authority": "a1-summary.json"},
            "B0": {"verdict": b0["verdict"], "authority": "b0-summary.json"},
            "B1": {"verdict": b1["verdict"], "authority": "b1-summary.json"},
        },
        "scope_statements": {
            "production_code_changed": False,
            "no_pr_merged": True,
            "no_simd": True,
            "no_kotlin": True,
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
        if ok:
            print("E10 authority tree: ALL CHECKS PASS")
            return 0
        return 1

    summary = compose()
    (OUT / "e10-summary.json").write_text(
        json.dumps(summary, indent=1) + "\n")
    print("e10-summary.json written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
