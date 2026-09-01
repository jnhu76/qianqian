#!/usr/bin/env python3
"""E10-P0 driver: build + run the PCM-pipeline harness, enforce gates,
write bench/results/pcm-processing/p0-summary.json (the single machine
authority) plus the machine
evidence files p0-negative-tests.json, p0-mutations.json and the
sanitizer results.

  python3 tools/pcm_p0.py                # build, run, write authority JSONs
  python3 tools/pcm_p0.py --check        # verify summary matches section
                                         # JSONs (no re-run)

The harness (bench/pcm/qn_pcm_p0_harness.c) owns all measurements; this
driver only compiles it, runs it, runs the sanitizer and mutation builds,
and composes provenance + gates.

Mutations are deterministic compile-time fault injections (QN_MUT_* in
bench/pcm/pcm_pipeline.c). Each mutation is compiled and run into an
isolated build dir; the driver asserts that the mutation's targeted
negative checks flip to FAIL. A mutation expected to fail but passing is
a top-level gate failure.
"""

import argparse
import datetime
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"
BUILD = ROOT / "build/pcm-p0"
BIN = BUILD / "qn_pcm_p0_harness"

SOURCES = [
    "bench/pcm/pcm_pipeline.c",
    "bench/pcm/qn_pcm_p0_harness.c",
]
FLAGS = ["-std=c11", "-O2", "-Wall", "-Wextra"]
SAN_FLAGS = ["-std=c11", "-O1", "-Wall", "-Wextra",
             "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
LINK = ["-lm"]

SECTIONS = [
    "p0-correctness.json",
    "p0-buffer-accounting.json",
    "p0-placement.json",
    "p0-performance.json",
]
NEG_SECTION = "p0-negative-tests.json"
SAN_CORR = "p0-correctness-sanitized.json"
SAN_NEG = "p0-negative-tests-sanitized.json"
MUT_JSON = "p0-mutations.json"

PRODUCTION_PATHS = [
    "src/",
    "include/",
    "xmake.lua",
    "package-manifest.json",
    "corpus/",
]

# mutation name -> (compile define, [negative-check ids that must FAIL])
# `allow_bypass_rate_mismatch` also drives the 4 rate-rejection checks.
MUTATIONS = {
    "allow_bypass_rate_mismatch": (
        "QN_MUT_RATE_MISMATCH_ACCEPT",
        ["bypass_rejects_rate_mismatch_44100_48000",
         "bypass_rejects_rate_mismatch_48000_44100"]),
    "remove_bypass_memcpy": (
        "QN_MUT_REMOVE_BYPASS_MEMCPY",
        ["bypass_memcpy_present"]),
    "weaken_zero_copy_guard": (
        "QN_MUT_WEAK_ZEROCOPY_GUARD",
        ["zero_copy_guard_with_dsp_stage"]),
    "remove_state_epoch_bump": (
        "QN_MUT_NO_EPOCH_BUMP",
        ["state_epoch_bump_on_prepare_and_reset"]),
    "allow_stale_queue_commit": (
        "QN_MUT_STALE_COMMIT_ALLOWED",
        ["stale_commit_after_flush_rejected"]),
    "break_queue_bound": (
        "QN_MUT_QUEUE_BOUND_BREAK",
        ["queue_bound_enforced"]),
}

# The rate-rejection checks that must all pass for the gate.
RATE_REJECT_CHECKS = [
    "bypass_rejects_rate_mismatch_44100_48000",
    "bypass_rejects_rate_mismatch_48000_44100",
    "bypass_rejects_zero_in_rate",
    "bypass_rejects_negative_in_rate",
]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def git(*args):
    r = sh(["git", "-C", str(ROOT), *args])
    return r.stdout.strip() if r.returncode == 0 else None


def compile_harness(cc, flags, defines, out_path):
    out_path.parent.mkdir(parents=True, exist_ok=True)
    r = sh([cc, *flags, *defines, "-I", "bench/pcm", *SOURCES,
            "-o", str(out_path), *LINK],
           cwd=ROOT)
    if r.returncode != 0:
        print(r.stderr, file=sys.stderr)
        raise SystemExit(f"compile failed: {out_path.name}")
    ver = sh([cc, "--version"]).stdout.splitlines()
    return cc, ver[0] if ver else "unknown"


def run_harness(bin_path, out_dir, sections=None):
    cmd = [str(bin_path), str(out_dir)]
    if sections:
        cmd += ["--sections", sections]
    r = sh(cmd, cwd=ROOT)
    print(r.stdout, end="")
    if r.returncode != 0:
        print(r.stderr, file=sys.stderr)
        raise SystemExit(f"harness reported failures: {bin_path.name}")


def load_sections(dirpath=None):
    d = Path(dirpath) if dirpath else OUT
    sec = {name: json.loads((d / name).read_text()) for name in SECTIONS}
    sec[NEG_SECTION] = json.loads((d / NEG_SECTION).read_text())
    return sec


def neg_check(sec, check_id):
    for c in sec[NEG_SECTION]["checks"]:
        if c["id"] == check_id:
            return c["verdict"] == "pass"
    return False


def compute_gates(sec):
    corr = sec["p0-correctness.json"]
    acct = sec["p0-buffer-accounting.json"]
    place = sec["p0-placement.json"]
    perf = sec["p0-performance.json"]
    neg = sec[NEG_SECTION]
    cases = corr["cases"]
    odd_ok = all(c["out_of_range_preserved"] for c in cases)
    frames_ok = all(c["frames_preserved"] for c in cases)
    # Rate semantics gate: BYPASS means in_rate == out_rate, both > 0.
    # The old `c["sample_rate"] == c["sample_rate"]` was tautological (P0-1).
    rates_ok = all(
        c.get("requested_in_rate", 0) == c.get("requested_out_rate", -1) and
        c.get("requested_in_rate", 0) > 0 and
        c["consumed_frames"] == c["produced_frames"] for c in cases)
    lc = corr["lifecycle"]
    perf_rows = perf["rows"]
    return {
        "bypass_pcm_bit_identical":
            corr["verdict"] == "PASS",
        "no_implicit_clipping_gain_rematrix_rate_change":
            corr["verdict"] == "PASS" and odd_ok and frames_ok and rates_ok,
        "bypass_rate_mismatch_rejected":
            all(neg_check(sec, cid) for cid in RATE_REJECT_CHECKS),
        "post_prepare_allocation_gate":
            acct["allocation_gate"]["verdict"] == "PASS",
        "buffer_bound":
            acct["buffer_bound"]["verdict"] == "PASS" and
            acct["buffer_bound"].get("ownership_conservation") == "pass",
        "queue_ownership_conservation":
            neg_check(sec, "queue_ownership_conservation") and
            acct["buffer_bound"].get("ownership_conservation") == "pass",
        "queue_bound_enforced":
            neg_check(sec, "queue_bound_enforced") and
            acct["buffer_bound"]["verdict"] == "PASS",
        "stale_token_rejected":
            neg_check(sec, "stale_commit_after_flush_rejected"),
        "reset_lifecycle_same_instance":
            lc["reset_determinism"]["verdict"] == "pass" and
            lc["reset_determinism"].get("same_pipeline_instance") is True,
        "reprepare_rate_transition_semantics":
            lc["reprepare_rate_transition"]["verdict"] == "pass",
        "zero_copy_guard_with_dsp_stage":
            neg_check(sec, "zero_copy_guard_with_dsp_stage") and
            lc["zero_copy_guard_with_dsp_stage"]["verdict"] == "pass",
        "copy_memory_pass_accounting_present":
            len(acct["modes"]) == 2 and all(
                "full_memory_passes" in m and
                "logical_copy_bytes" in m and
                "estimated_memory_traffic_bytes" in m
                for m in acct["modes"]),
        "rt_vs_worker_placement_evidence_present":
            place["verdict"] == "PASS" and len(place["shapes"]) == 3,
        "bypass_overhead_measured":
            perf["verdict"] == "PASS" and len(perf_rows) > 0 and
            all(r.get("throughput_sanity_ok") for r in perf_rows),
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
    # Field semantics: True only when production paths were actually
    # touched (the old value stored "unchanged" under a "changed" name).
    return (len(touched) > 0), touched


def key_numbers(sec, mut_result, san_result):
    corr = sec["p0-correctness.json"]
    acct = sec["p0-buffer-accounting.json"]
    place = sec["p0-placement.json"]
    perf = sec["p0-performance.json"]
    neg = sec[NEG_SECTION]
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
        "negative_tests": neg["summary"],
        "negative_checks": neg["checks"],
        "mutations": mut_result,
        "sanitizer": san_result,
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
        "sanitize_flags": SAN_FLAGS,
        "sources": SOURCES,
        "cpu_model": cpu,
        "os": f"{platform.system()} {platform.release()}",
        "machine": platform.machine(),
        "generated_utc": datetime.datetime.now(
            datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "note": "timings are single-host; compare via recorded distributions",
    }


def run_sanitizer(cc):
    """Compile with ASan+UBSan and run the correctness + negative sections
    into an isolated dir; copy results into OUT under -sanitized names.
    Performance/placement stay non-sanitized by design (P0-4)."""
    san_dir = BUILD / "sanitized"
    san_dir.mkdir(parents=True, exist_ok=True)
    san_bin = san_dir / "qn_pcm_p0_harness_san"
    compile_harness(cc, SAN_FLAGS, [], san_bin)
    run_harness(san_bin, san_dir, sections="correctness,negative")
    for src, dst in [("p0-correctness.json", SAN_CORR),
                     (NEG_SECTION, SAN_NEG)]:
        shutil.copyfile(san_dir / src, OUT / dst)
    corr = json.loads((OUT / SAN_CORR).read_text())
    neg = json.loads((OUT / SAN_NEG).read_text())
    return {
        "correctness_verdict": corr["verdict"],
        "negative_verdict": neg["verdict"],
        "verdict": "PASS" if (corr["verdict"] == "PASS" and
                              neg["verdict"] == "PASS") else "FAIL",
        "compiler": cc,
        "sanitize_flags": SAN_FLAGS,
    }


def run_mutations(cc):
    """Compile each QN_MUT_* build and assert its targeted negative checks
    flip to FAIL. Returns the mutation result list for p0-mutations.json."""
    results = []
    all_caught = True
    for name, (define, expect_fail) in MUTATIONS.items():
        mut_dir = BUILD / "mutations" / name
        mut_bin = mut_dir / "qn_pcm_p0_harness_mut"
        compile_harness(cc, FLAGS, [f"-D{define}"], mut_bin)
        # A mutation run is EXPECTED to fail its targeted checks, so a
        # nonzero harness exit is the point, not a driver error.
        r = sh([str(mut_bin), str(mut_dir)], cwd=ROOT)
        if r.returncode != 0:
            print(f"[mutation {name}] harness exit {r.returncode} "
                  f"(expected; targeted negative checks should fail)")
        sec = load_sections(mut_dir)
        failed = [c["id"] for c in sec[NEG_SECTION]["checks"]
                  if c["verdict"] == "FAIL"]
        missed = [cid for cid in expect_fail if cid not in failed]
        caught = not missed
        if not caught:
            all_caught = False
        results.append({
            "mutation": name,
            "define": define,
            "expected_fail_checks": expect_fail,
            "actually_failed_checks": failed,
            "unexpected_passing_checks": missed,
            "caught": caught,
        })
    return {"mutations": results, "all_caught": all_caught}


def build_summary(sec, mut_result, san_result, prov):
    gates = compute_gates(sec)
    gates["sanitizer_clean"] = san_result["verdict"] == "PASS"
    gates["mutation_tests_caught"] = mut_result["all_caught"]
    all_pass = all(gates.values())
    return {
        "experiment": "e10-p0-pcm-processing",
        "authority_files": SECTIONS + [NEG_SECTION, MUT_JSON,
                                       SAN_CORR, SAN_NEG],
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
        "key_numbers": key_numbers(sec, mut_result, san_result),
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
        mut_result = json.loads((OUT / MUT_JSON).read_text())
        corr_san = json.loads((OUT / SAN_CORR).read_text())
        neg_san = json.loads((OUT / SAN_NEG).read_text())
        san_result = {
            "correctness_verdict": corr_san["verdict"],
            "negative_verdict": neg_san["verdict"],
            "verdict": "PASS" if (corr_san["verdict"] == "PASS" and
                                  neg_san["verdict"] == "PASS") else "FAIL",
        }
        gates["sanitizer_clean"] = san_result["verdict"] == "PASS"
        gates["mutation_tests_caught"] = mut_result["all_caught"]
        existing = json.loads((OUT / "p0-summary.json").read_text())
        drift = existing.get("gates") != {
            k: ("PASS" if v else "FAIL") for k, v in gates.items()}
        if drift:
            print("DRIFT: p0-summary.json differs from section JSONs; "
                  "run tools/pcm_p0.py to regenerate")
            return 1
        if not all(gates.values()):
            print("GATE FAIL: see gates in p0-summary.json")
            return 1
        print("p0-summary.json in sync with section JSONs; gates PASS")
        return 0

    cc_name, cc_version = compile_harness(
        os.environ.get("CC", "cc"), FLAGS, [], BIN)
    pre_run_git = {
        "head": git("rev-parse", "HEAD"),
        "branch": git("rev-parse", "--abbrev-ref", "HEAD"),
        "dirty": git("status", "--porcelain") not in (None, ""),
    }
    run_harness(BIN, OUT)
    sec = load_sections()
    san_result = run_sanitizer(cc_name)
    mut_result = run_mutations(cc_name)
    prov = provenance(cc_name, cc_version, pre_run_git)
    summary = build_summary(sec, mut_result, san_result, prov)
    (OUT / "p0-summary.json").write_text(
        json.dumps(summary, indent=1) + "\n")
    (OUT / MUT_JSON).write_text(json.dumps(mut_result, indent=1) + "\n")
    print(f"summary written: {OUT / 'p0-summary.json'} "
          f"(verdict {summary['verdict']})")
    print(f"sanitizer: {san_result['verdict']}; "
          f"mutations all caught: {mut_result['all_caught']}")
    if not summary["gates_all_pass"]:
        failed = [k for k, v in summary["gates"].items() if v != "PASS"]
        print("failed gates:", ", ".join(failed), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
