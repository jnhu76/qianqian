#!/usr/bin/env python3
"""closeout3 decode-seek C1-C4 counterfactual runner.

Each cell has one source mutation that breaks the property the cell claims.
The runner applies the mutation, recompiles/runs the SAME cell, and requires a
precise RED: the test executed (non-zero execution), failed inside an
`assert!`/`panic!` with the named signature, and did NOT report PASS. Build
errors, zero-execution and watchdog timeouts are rejected as invalid REDs.
The mutated source is then restored byte-for-byte (sha256 checked) and the cell
is rerun to prove the restoration is green.

Run from anywhere; paths are absolute.
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time

WORKTREE = os.environ.get("QIANQIAN_NATIVE_WORKTREE", "")
if not WORKTREE:
    raise SystemExit("set QIANQIAN_NATIVE_WORKTREE to the scratch checkout")
RECEIPTS = os.environ.get(
    "QIANQIAN_NATIVE_RECEIPTS", os.path.join(WORKTREE, "native-seek-receipts")
)
RAW = os.path.join(RECEIPTS, "raw")
MANIFEST = os.path.join(WORKTREE, "crates/qianqian-decode-songcore/Cargo.toml")
TEST_NAME = "decode_seek_controls"

# cell -> (source file, exact old bytes, exact new bytes, expected RED marker)
MUTATIONS = {
    "c1": {
        "test": "c1_blocked_sink",
        "file": "crates/qianqian-decode-songcore/src/lib.rs",
        "old": "fn emit_decode_fault(fault: &DecodeFault) {\n    let record = Record::new(DIAGNOSTIC_SITE, Level::Error)",
        "new": "fn emit_decode_fault(fault: &DecodeFault) {\n    let _ = qianqian_diagnostics::write_stderr_line(\"[COUNTERFACTUAL] producer synchronous stderr write\");\n    let record = Record::new(DIAGNOSTIC_SITE, Level::Error)",
        "red": "C1 ISOLATION: the producer returned only after the blocked sink was released",
        "property": "producer path does not synchronously write the blocked sink",
    },
    "c2": {
        "test": "c2_stopped_consumer",
        "file": "crates/qianqian-diagnostics/src/lib.rs",
        "old": "                return ShutdownReport {\n                    drained: false,\n                    abandoned: true,",
        "new": "                return ShutdownReport {\n                    drained: true,\n                    abandoned: false,",
        "red": "C2 HONEST-ACCOUNTING: a stopped drainer must NEVER be reported drained",
        "property": "bounded shutdown reports abandoned (never drained) when the consumer is stopped",
    },
    "c3": {
        "test": "c3_capacity_exhaustion",
        "file": "crates/qianqian-diagnostics/src/pool.rs",
        "old": "        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {\n            self.rejected.fetch_add(1, Ordering::Relaxed);\n            return false;\n        }",
        "new": "        // COUNTERFACTUAL: the full-pool rejection (drop) path was removed.",
        "red": "C3 ISOLATION: the real producer's records were rejected at the full α pool",
        "property": "full α pool drops+counts instead of accepting into the bounded pool",
    },
    "c4": {
        "test": "c4_write_failure",
        "file": "crates/qianqian-diagnostics/src/lib.rs",
        "old": "inner.export_failed.fetch_add(1, Ordering::Relaxed);",
        "new": "inner.export_failed.fetch_add(0, Ordering::Relaxed);",
        "red": "C4 LOSS-ACCOUNTING: the failed export attempts are counted as export_failed",
        "property": "an export failure is counted as a loss, never as delivered",
    },
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def run_cell(cell, log_path, timeout=240):
    cmd = [
        "cargo", "test",
        "--manifest-path", MANIFEST,
        "--test", TEST_NAME,
        MUTATIONS[cell]["test"],
        "--", "--exact", "--nocapture", "--test-threads=1",
    ]
    env = dict(os.environ)
    env["CARGO_TERM_COLOR"] = "never"
    started = time.time()
    try:
        proc = subprocess.run(
            cmd, cwd=WORKTREE, env=env, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, timeout=timeout,
        )
        out = proc.stdout.decode("utf-8", "replace")
        code = proc.returncode
        timed_out = False
    except subprocess.TimeoutExpired as exc:
        out = (exc.stdout or b"").decode("utf-8", "replace")
        code = None
        timed_out = True
    elapsed = time.time() - started
    header = [
        "# cmd: " + " ".join(cmd),
        f"# cell: {cell} test: {MUTATIONS[cell]['test']}",
        f"# artifact: {sha256(os.path.join(WORKTREE, 'native/build/artifacts/libsongcore.a'))}",
        f"# date: {time.strftime('%Y-%m-%dT%H:%M:%S%z')}",
        f"# wall_seconds: {elapsed:.3f}",
    ]
    with open(log_path, "w", encoding="utf-8") as f:
        f.write("\n".join(header) + "\n")
        f.write(out)
        f.write(f"\n# EXIT={code} TIMED_OUT={timed_out}\n")
    return out, code, timed_out


def main():
    os.makedirs(RAW, exist_ok=True)
    results = []
    for cell, mut in MUTATIONS.items():
        path = os.path.join(WORKTREE, mut["file"])
        original = open(path, "rb").read()
        original_sha = hashlib.sha256(original).hexdigest()
        old = mut["old"].encode()
        new = mut["new"].encode()
        if original.count(old) < 1:
            print(f"FATAL {cell}: mutation anchor absent in {path}")
            return 2
        mutated = original.replace(old, new)
        variant_log = os.path.join(RAW, f"{cell}-counterfactual-red.log")
        restored_log = os.path.join(RAW, f"{cell}-restored-green.log")
        print(f"== {cell}: applying mutation ({mut['property']})")
        restored_ok = True
        try:
            open(path, "wb").write(mutated)
            out, code, timed_out = run_cell(cell, variant_log)
        finally:
            open(path, "wb").write(original)
            restored_sha = sha256(path)
            restored_ok = restored_sha == original_sha
        if not restored_ok:
            print(f"FATAL {cell}: restoration sha mismatch")
            return 3
        executed = ("running 1 test" in out) and (f"CELL={cell.upper()}" in out)
        assertion_red = ("panicked at" in out) and (mut["red"] in out)
        test_failed = "test result: FAILED" in out
        no_pass = f"{cell.upper()}-PASS" not in out
        invalid_markers = ("error[E", "error: could not compile", "TIMED_OUT=True")
        no_bad = not any(m in out for m in invalid_markers) and not timed_out
        precise_red = executed and assertion_red and test_failed and no_pass and no_bad
        print(f"   RED: executed={executed} assertion={assertion_red} "
              f"failed={test_failed} no_pass={no_pass} no_build_or_watchdog={no_bad} "
              f"=> {'PRECISE_RED' if precise_red else 'INVALID_RED'}")
        # Restored baseline must be green again.
        out_g, code_g, to_g = run_cell(cell, restored_log)
        restored_green = (
            code_g == 0 and not to_g and "test result: ok. 1 passed" in out_g
            and f"{cell.upper()}-PASS" in out_g
        )
        print(f"   restored GREEN: {restored_green}")
        results.append({
            "cell": cell,
            "property_broken": mut["property"],
            "mutation_file": mut["file"],
            "base_file_sha256": original_sha,
            "restored_file_sha256": restored_sha,
            "red_log": os.path.relpath(variant_log, RECEIPTS),
            "green_log": os.path.relpath(restored_log, RECEIPTS),
            "executed": executed,
            "assertion_red": assertion_red,
            "test_result_failed": test_failed,
            "no_pass_marker": no_pass,
            "no_build_error_or_watchdog": no_bad,
            "precise_red": precise_red,
            "restored_green": restored_green,
            "red_signature": mut["red"],
            "exit": code,
            "timed_out": timed_out,
        })
        if not (precise_red and restored_green):
            print(json.dumps(results[-1], indent=2))
            return 1
    with open(os.path.join(RECEIPTS, "counterfactuals.json"), "w") as f:
        json.dump(results, f, indent=2)
    print("ALL COUNTERFACTUALS: PRECISE_RED, restorations GREEN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
