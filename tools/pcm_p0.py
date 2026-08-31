#!/usr/bin/env python3
"""E10-P0 driver: build + run the PCM-pipeline harness, enforce gates,
write bench/results/pcm-processing/p0-summary.json (the single machine
authority consumed by tools/pcm_report_tables.py).

  python3 tools/pcm_p0.py                # build, run, write authority JSONs
  python3 tools/pcm_p0.py --check        # verify summary matches the four
                                         # section JSONs (no re-run)

The harness (bench/pcm/qn_pcm_p0_harness.c) owns all measurements; this
driver only compiles it, runs it, and composes provenance + gates.
"""

import argparse
import datetime
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"
BIN = ROOT / "build/pcm-p0/qn_pcm_p0_harness"

SOURCES = [
    "bench/pcm/pcm_pipeline.c",
    "bench/pcm/qn_pcm_p0_harness.c",
]
FLAGS = ["-std=c11", "-O2", "-Wall", "-Wextra"]

SECTIONS = [
    "p0-correctness.json",
    "p0-buffer-accounting.json",
    "p0-placement.json",
    "p0-performance.json",
]

PRODUCTION_PATHS = [
    "src/",
    "include/",
    "xmake.lua",
    "package-manifest.json",
    "corpus/",
]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def git(*args):
    r = sh(["git", "-C", str(ROOT), *args])
    return r.stdout.strip() if r.returncode == 0 else None


def compile_harness():
    cc = os.environ.get("CC", "cc")
    BIN.parent.mkdir(parents=True, exist_ok=True)
    r = sh([cc, *FLAGS, "-I", "bench/pcm", *SOURCES, "-o", str(BIN)],
           cwd=ROOT)
    if r.returncode != 0:
        print(r.stderr, file=sys.stderr)
        raise SystemExit("compile failed")
    ver = sh([cc, "--version"]).stdout.splitlines()
    return cc, ver[0] if ver else "unknown"


def run_harness():
    OUT.mkdir(parents=True, exist_ok=True)
    r = sh([str(BIN), str(OUT)], cwd=ROOT)
    print(r.stdout, end="")
    if r.returncode != 0:
        print(r.stderr, file=sys.stderr)
        raise SystemExit("harness reported failures")


def load_sections():
    return {name: json.loads((OUT / name).read_text()) for name in SECTIONS}


def compute_gates(sec):
    corr = sec["p0-correctness.json"]
    acct = sec["p0-buffer-accounting.json"]
    place = sec["p0-placement.json"]
    perf = sec["p0-performance.json"]
    cases = corr["cases"]
    odd_ok = all(c["out_of_range_preserved"] for c in cases)
    frames_ok = all(c["frames_preserved"] for c in cases)
    rates_ok = all(
        c["sample_rate"] == c["sample_rate"] and
        c["consumed_frames"] == c["produced_frames"] for c in cases)
    return {
        "bypass_pcm_bit_identical":
            corr["verdict"] == "PASS",
        "no_implicit_clipping_gain_rematrix_rate_change":
            corr["verdict"] == "PASS" and odd_ok and frames_ok and rates_ok,
        "post_prepare_allocation_gate":
            acct["allocation_gate"]["verdict"] == "PASS",
        "buffer_bound":
            acct["buffer_bound"]["verdict"] == "PASS",
        "reset_deterministic":
            corr["lifecycle"]["reset_determinism"]["verdict"] == "pass",
        "reprepare_rate_transition_semantics":
            corr["lifecycle"]["reprepare_rate_transition"]["verdict"] == "pass",
        "copy_memory_pass_accounting_present":
            len(acct["modes"]) == 2 and all(
                "full_memory_passes" in m for m in acct["modes"]),
        "rt_vs_worker_placement_evidence_present":
            place["verdict"] == "PASS" and len(place["shapes"]) == 3,
        "bypass_overhead_measured":
            perf["verdict"] == "PASS" and len(perf["rows"]) > 0,
    }


def production_changed():
    base = git("merge-base", "HEAD", "origin/main") or git(
        "merge-base", "HEAD", "main")
    if not base:
        return None, []
    changed = git("diff", "--name-only", f"{base}..HEAD") or ""
    touched = [p for p in changed.splitlines()
               if any(p.startswith(pref) or p == pref.rstrip("/")
                      for pref in PRODUCTION_PATHS)]
    return (len(touched) == 0), touched


def key_numbers(sec):
    corr = sec["p0-correctness.json"]
    acct = sec["p0-buffer-accounting.json"]
    place = sec["p0-placement.json"]
    perf = sec["p0-performance.json"]
    return {
        "correctness": {
            "cases_total": corr["summary"]["cases_total"],
            "cases_passed": corr["summary"]["cases_passed"],
            "bytes_compared_total": corr["summary"]["bytes_compared_total"],
        },
        "accounting_modes": acct["modes"],
        "allocation_gate": acct["allocation_gate"],
        "buffer_bound": acct["buffer_bound"],
        "lifecycle": corr["lifecycle"],
        "placement_schedule": place["schedule"],
        "placement_shapes": place["shapes"],
        "performance_rows": perf["rows"],
    }


def provenance(cc_name, cc_version, pre_run_git):
    cpu = ""
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    prod_ok, prod_touched = production_changed()
    return {
        # snapshot taken BEFORE the harness rewrites result JSONs: this is
        # the parent commit the canonical run was produced from
        "git_parent_commit": pre_run_git["head"],
        "git_branch": pre_run_git["branch"],
        "worktree_dirty_at_run_start": pre_run_git["dirty"],
        "git_base": git("merge-base", "HEAD", "origin/main") or git(
            "merge-base", "HEAD", "main"),
        "production_code_changed": prod_ok,
        "production_paths_touched": prod_touched,
        "compiler": cc_name,
        "compiler_version": cc_version,
        "compile_flags": FLAGS,
        "sources": SOURCES,
        "cpu_model": cpu,
        "os": f"{platform.system()} {platform.release()}",
        "machine": platform.machine(),
        "generated_utc": datetime.datetime.now(
            datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "note": "timings are single-host; compare via recorded distributions",
    }


def build_summary(sec, prov):
    gates = compute_gates(sec)
    all_pass = all(gates.values())
    return {
        "experiment": "e10-p0-pcm-processing",
        "authority_files": SECTIONS,
        "scope_statements": {
            "selects_no_src": True,
            "selects_no_dsp_implementation": True,
            "selects_no_libavfilter_architecture": True,
            "selects_no_simd_strategy": True,
            "makes_no_production_placement_decision": True,
        },
        "gates": {k: ("PASS" if v else "FAIL") for k, v in gates.items()},
        "gates_all_pass": all_pass,
        "verdict": "PASS" if all_pass else "FAIL",
        "key_numbers": key_numbers(sec),
        "provenance": prov,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="verify p0-summary.json matches the section JSONs")
    args = ap.parse_args()

    if args.check:
        sec = load_sections()
        gates = compute_gates(sec)
        kn = key_numbers(sec)
        existing = json.loads((OUT / "p0-summary.json").read_text())
        drift = existing.get("gates") != {
            k: ("PASS" if v else "FAIL") for k, v in gates.items()} or \
            existing.get("key_numbers") != kn
        if drift:
            print("DRIFT: p0-summary.json differs from section JSONs; "
                  "run tools/pcm_p0.py to regenerate")
            return 1
        if not all(gates.values()):
            print("GATE FAIL: see gates in p0-summary.json")
            return 1
        print("p0-summary.json in sync with section JSONs; gates PASS")
        return 0

    cc_name, cc_version = compile_harness()
    pre_run_git = {
        "head": git("rev-parse", "HEAD"),
        "branch": git("rev-parse", "--abbrev-ref", "HEAD"),
        "dirty": git("status", "--porcelain") not in (None, ""),
    }
    run_harness()
    sec = load_sections()
    prov = provenance(cc_name, cc_version, pre_run_git)
    summary = build_summary(sec, prov)
    (OUT / "p0-summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(f"summary written: {OUT / 'p0-summary.json'} "
          f"(verdict {summary['verdict']})")
    if not summary["gates_all_pass"]:
        failed = [k for k, v in summary["gates"].items() if v != "PASS"]
        print("failed gates:", ", ".join(failed), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
