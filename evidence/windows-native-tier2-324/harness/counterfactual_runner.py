#!/usr/bin/env python3
"""#324 Windows native Tier-2 counterfactual runner.

For each of the twelve cells:
  1. build the test binary from the current sources and run the HEALTHY cell
     through `run_cell.ps1` (one fresh Windows process per cell);
  2. apply ONE minimal source mutation that breaks exactly the property the
     cell claims, rebuild, run the SAME cell and require a PRECISE RED
     (the binary executed, the named assertion fired, exit 101, no PASS
     marker, no compile error, no watchdog kill);
  3. restore the source bytes exactly (sha256-verified) and rerun the cell to
     prove the restoration is GREEN.

Windows notes that differ from the Linux runner (evidence/native-tier2-296):
  * `run_cell.ps1` owns process spawn and the independent stderr pipe;
  * the C1 mutation's bounded producer wait is 30 s (the cell's fail-closed
    await deadline is 10 s), so "the producer never returns" is decided by the
    cell's own assertion, not by the outer watchdog;
  * C3's counterfactual zeroes the rejection counter (`fetch_add(0)`) instead
    of deleting the admission branch, so the pool bound and the loss
    accounting are separated cleanly.

Run from anywhere with QIANQIAN_NATIVE_WORKTREE set (or let it default to the
worktree containing this evidence directory).
"""

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time

EVIDENCE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKTREE = os.environ.get("QIANQIAN_NATIVE_WORKTREE") or os.path.dirname(
    os.path.dirname(EVIDENCE)
)
RAW = os.path.join(EVIDENCE, "raw")
MANIFEST = os.path.join(WORKTREE, "crates/qianqian-decode-songcore/Cargo.toml")
TARGET = "x86_64-pc-windows-gnu"
RUN_PS = os.path.join(EVIDENCE, "harness", "run_cell.ps1")
CARGO = "cargo"
ARTIFACT = os.path.join(WORKTREE, "native", "build", "artifacts", "libsongcore.a")

POOL = "crates/qianqian-diagnostics/src/pool.rs"
DIAG = "crates/qianqian-diagnostics/src/lib.rs"
DECODE = "crates/qianqian-decode-songcore/src/lib.rs"

FULL_REJECT = (
    "        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {\n"
    "            self.rejected.fetch_add(1, Ordering::Relaxed);\n"
    "            return false;\n"
    "        }\n"
)

SHUTDOWN_DEADLINE = (
    "                return ShutdownReport {\n"
    "                    drained: false,\n"
    "                    abandoned: true,\n"
)

DELIVER_ERR = (
    "        Err(error) => {\n"
    "            inner.export_failed.fetch_add(1, Ordering::Relaxed);\n"
    "            if !*disabled {\n"
)

EMIT_FAULT = (
    "fn emit_decode_fault(fault: &DecodeFault) {\n"
    "    let record = Record::new(DIAGNOSTIC_SITE, Level::Error)\n"
)

# --- the twelve cells -------------------------------------------------------

CELLS = []
for cell in ("C1", "C2", "C3", "C4"):
    CELLS.append(
        {
            "id": "ffmpeg-%s" % cell,
            "arm": "ffmpeg-bridge",
            "cell": cell,
            "test_bin": "native_ffmpeg_bridge_controls",
            "test": "qfb_native_ffmpeg_bridge_cell",
            "mode": "ignore",
            "env": {"QIANQIAN_FFMPEG_LOG": "1", "Q324_CELL": cell},
            "pass_marker": "QFB-%s-CELL-PASS" % cell,
        }
    )
for cell, name in (
    ("C1", "c1_blocked_sink_native_decode_read"),
    ("C2", "c2_stopped_consumer_native_decode_read"),
    ("C3", "c3_capacity_exhaustion_native_decode_read"),
    ("C4", "c4_write_failure_native_decode_read"),
):
    CELLS.append(
        {
            "id": "read-%s" % cell,
            "arm": "decode-read",
            "cell": cell,
            "test_bin": "native_decode_read_controls",
            "test": name,
            "mode": "ignore",
            "env": {"QIANQIAN_FFMPEG_LOG": "1"},
            "ignored": True,
            "pass_marker": "%s-VERDICT=PASS" % cell,
        }
    )
for cell, name, mode in (
    ("C1", "c1_blocked_sink", "drain"),
    ("C2", "c2_stopped_consumer", "ignore"),
    ("C3", "c3_capacity_exhaustion", "ignore"),
    ("C4", "c4_write_failure", "drain"),
):
    CELLS.append(
        {
            "id": "seek-%s" % cell,
            "arm": "decode-seek",
            "cell": cell,
            "test_bin": "decode_seek_controls",
            "test": name,
            "mode": mode,
            "env": {"QIANQIAN_FFMPEG_LOG": "1"},
            "pass_marker": "%s-PASS" % cell,
        }
    )

# --- the twelve counterfactuals --------------------------------------------

MUTATIONS = {
    # C1: put the diagnostic sink write back on the producer path / make the
    # producer wait for the blocked consumer.
    "ffmpeg-C1": {
        "file": POOL,
        "old": FULL_REJECT,
        "new": (
            "        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {\n"
            "            // Q324-MUTANT-ffmpeg-C1: the producer waits for the blocked\n"
            "            // consumer instead of dropping the record (isolation broken).\n"
            "            let start = std::time::Instant::now();\n"
            "            while self\n"
            "                .pub_gen\n"
            "                .load(Ordering::Acquire)\n"
            "                .wrapping_sub(self.cons_gen.load(Ordering::Acquire))\n"
            "                == 2\n"
            "                && start.elapsed() < std::time::Duration::from_secs(30)\n"
            "            {\n"
            "                std::thread::yield_now();\n"
            "            }\n"
            "            if self\n"
            "                .pub_gen\n"
            "                .load(Ordering::Acquire)\n"
            "                .wrapping_sub(self.cons_gen.load(Ordering::Acquire))\n"
            "                == 2\n"
            "            {\n"
            "                self.rejected.fetch_add(1, Ordering::Relaxed);\n"
            "                return false;\n"
            "            }\n"
            "        }\n"
        ),
        "red": "PRODUCER-NOT-BACKPRESSURED",
        "property": "a blocked sink must not backpressure the native producer",
    },
    "read-C1": {
        "file": DECODE,
        "old": EMIT_FAULT,
        "new": (
            "fn emit_decode_fault(fault: &DecodeFault) {\n"
            "    // Q324-MUTANT-read-C1: synchronous sink write on the producer path.\n"
            "    let _ = qianqian_diagnostics::write_stderr_line(\"[COUNTERFACTUAL] producer synchronous stderr write\");\n"
            "    let record = Record::new(DIAGNOSTIC_SITE, Level::Error)\n"
        ),
        "red": "C1-PRODUCER-PROGRESS",
        "property": "the decode producer must not synchronously write a blocked sink",
    },
    "seek-C1": {
        "file": DECODE,
        "old": EMIT_FAULT,
        "new": (
            "fn emit_decode_fault(fault: &DecodeFault) {\n"
            "    // Q324-MUTANT-seek-C1: synchronous sink write on the producer path.\n"
            "    let _ = qianqian_diagnostics::write_stderr_line(\"[COUNTERFACTUAL] producer synchronous stderr write\");\n"
            "    let record = Record::new(DIAGNOSTIC_SITE, Level::Error)\n"
        ),
        "red": "C1 ISOLATION: the producer returned only after the blocked sink was released",
        "property": "the seek producer must not synchronously write a blocked sink",
    },
    # C2: claim a complete drain while the drainer is still stuck.
    "ffmpeg-C2": {
        "file": DIAG,
        "old": SHUTDOWN_DEADLINE,
        "new": (
            "                return ShutdownReport {\n"
            "                    // Q324-MUTANT-C2: claim a complete drain while stuck.\n"
            "                    drained: true,\n"
            "                    abandoned: false,\n"
        ),
        "red": "TEARDOWN-OUTCOME: a stopped consumer must yield an honest abandoned shutdown",
        "property": "bounded shutdown reports abandoned, never drained, when the consumer is stopped",
    },
    "read-C2": {
        "file": DIAG,
        "old": "            if started.elapsed() >= deadline {\n",
        "new": "            if false && started.elapsed() >= deadline {\n",
        "red": "C2-TEARDOWN: shutdown did not return inside its bounded deadline",
        "property": "shutdown is bounded while the consumer is stopped",
    },
    "seek-C2": {
        "file": DIAG,
        "old": SHUTDOWN_DEADLINE,
        "new": (
            "                return ShutdownReport {\n"
            "                    // Q324-MUTANT-C2: claim a complete drain while stuck.\n"
            "                    drained: true,\n"
            "                    abandoned: false,\n"
        ),
        "red": "C2 HONEST-ACCOUNTING: a stopped drainer must NEVER be reported drained",
        "property": "bounded shutdown reports abandoned, never drained, when the consumer is stopped",
    },
    # C3: lose the full-pool rejection count (the pool bound itself stays).
    "ffmpeg-C3": {
        "file": POOL,
        "old": FULL_REJECT,
        "new": FULL_REJECT.replace(
            "self.rejected.fetch_add(1, Ordering::Relaxed);",
            "// Q324-MUTANT-C3: the full-pool rejection is no longer counted.\n"
            "            self.rejected.fetch_add(0, Ordering::Relaxed);",
        ),
        "red": "REJECT-ACCOUNTING: every full-pool attempt must be rejected and counted",
        "property": "a full α pool drops AND counts every over-capacity presentation",
    },
    "read-C3": {
        "file": POOL,
        "old": FULL_REJECT,
        "new": FULL_REJECT.replace(
            "self.rejected.fetch_add(1, Ordering::Relaxed);",
            "// Q324-MUTANT-C3: the full-pool rejection is no longer counted.\n"
            "            self.rejected.fetch_add(0, Ordering::Relaxed);",
        ),
        "red": "C3-DROP-ACCOUNTING: every presentation over capacity is rejected+counted",
        "property": "a full α pool drops AND counts every over-capacity presentation",
    },
    "seek-C3": {
        "file": POOL,
        "old": FULL_REJECT,
        "new": FULL_REJECT.replace(
            "self.rejected.fetch_add(1, Ordering::Relaxed);",
            "// Q324-MUTANT-C3: the full-pool rejection is no longer counted.\n"
            "            self.rejected.fetch_add(0, Ordering::Relaxed);",
        ),
        "red": "C3 ISOLATION: the real producer's records were rejected at the full",
        "property": "a full α pool drops AND counts every over-capacity presentation",
    },
    # C4: count a failed write as delivered / lose the failure count.
    "ffmpeg-C4": {
        "file": DIAG,
        "old": DELIVER_ERR,
        "new": (
            "        Err(error) => {\n"
            "            // Q324-MUTANT-C4: a failed write is also reported as delivered.\n"
            "            inner.exported.fetch_add(1, Ordering::Relaxed);\n"
            "            inner.export_failed.fetch_add(1, Ordering::Relaxed);\n"
            "            if !*disabled {\n"
        ),
        "red": "LOSS-ACCOUNTING: a failed write must never be counted as delivered",
        "property": "an export failure is charged as a loss, never as delivery",
    },
    "read-C4": {
        "file": DIAG,
        "old": DELIVER_ERR,
        "new": (
            "        Err(error) => {\n"
            "            // Q324-MUTANT-C4: a failed write is also reported as delivered.\n"
            "            inner.exported.fetch_add(1, Ordering::Relaxed);\n"
            "            inner.export_failed.fetch_add(1, Ordering::Relaxed);\n"
            "            if !*disabled {\n"
        ),
        "red": "C4-WRITE-FAILURE: a failed export is never counted as delivered",
        "property": "an export failure is charged as a loss, never as delivery",
    },
    "seek-C4": {
        "file": DIAG,
        "old": DELIVER_ERR,
        "new": DELIVER_ERR.replace(
            "inner.export_failed.fetch_add(1, Ordering::Relaxed);",
            "// Q324-MUTANT-C4: the export failure is no longer counted.\n"
            "            inner.export_failed.fetch_add(0, Ordering::Relaxed);",
        ),
        "red": [
            "C4 LOSS-ACCOUNTING: the failed export attempts are counted as export_failed",
            "C4: ACCOUNTING-ORACLE at quiescence",
        ],
        "property": "an export failure is charged as a loss, never as delivery",
    },
}


SOURCE_FILES = (POOL, DIAG, DECODE)


def source_identities():
    """Hashes of every product file the campaign reads or mutates."""
    return {f: sha256(os.path.join(WORKTREE, f)) for f in SOURCE_FILES}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def build(test_bin, log_path):
    cmd = [
        CARGO,
        "test",
        "--manifest-path",
        MANIFEST,
        "--target",
        TARGET,
        "--test",
        test_bin,
        "--no-run",
        "--message-format=json",
    ]
    env = dict(os.environ)
    env["CARGO_TERM_COLOR"] = "never"
    started = time.time()
    proc = subprocess.run(
        cmd, cwd=WORKTREE, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    elapsed = time.time() - started
    exe = None
    for raw_line in proc.stdout.decode("utf-8", "replace").splitlines():
        try:
            msg = json.loads(raw_line)
        except ValueError:
            continue
        if msg.get("reason") == "compiler-artifact" and msg.get("executable"):
            target = msg.get("target") or {}
            if target.get("name") == test_bin and "test" in (target.get("kind") or []):
                exe = msg["executable"]
    with open(log_path, "w", encoding="utf-8") as f:
        f.write("# cmd: " + " ".join(cmd) + "\n")
        f.write(proc.stderr.decode("utf-8", "replace"))
        f.write("\n# EXIT=%d WALL=%.2fs EXE=%s\n" % (proc.returncode, elapsed, exe))
    if proc.returncode != 0 or not exe:
        raise SystemExit(
            "build failed for %s (exit %d); see %s" % (test_bin, proc.returncode, log_path)
        )
    return exe, elapsed


def run_cell(cell, exe, log_path, timeout=180):
    envfile = tempfile.NamedTemporaryFile(
        mode="w", suffix=".env", delete=False, encoding="utf-8"
    )
    env = {
        "QIANQIAN_NATIVE_DIR": os.path.join(WORKTREE, "native"),
        "CARGO_TERM_COLOR": "never",
    }
    env.update(cell["env"])
    with envfile:
        for key, value in env.items():
            envfile.write("%s=%s\n" % (key, value))
    cmd = [
        "powershell",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        RUN_PS,
        "-Exe",
        exe,
        "-Cell",
        cell["id"],
        "-Test",
        cell["test"],
        "-Mode",
        cell["mode"],
        "-EnvFile",
        envfile.name,
        "-TimeoutSec",
        str(timeout),
        "-OutFile",
        log_path,
    ]
    if cell.get("ignored"):
        cmd.append("-Ignored")
    started = time.time()
    proc = subprocess.run(
        cmd, cwd=WORKTREE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    elapsed = time.time() - started
    try:
        os.unlink(envfile.name)
    except OSError:
        pass
    text = ""
    if os.path.exists(log_path):
        with open(log_path, "r", encoding="utf-8-sig", errors="replace") as f:
            text = f.read()
    if not text:
        text = proc.stdout.decode("utf-8", "replace")
    meta = {"exit": None, "timed_out": None, "stdout": ""}
    in_stdout = False
    captured = []
    for line_str in text.splitlines():
        if line_str.startswith("--- stdout ---"):
            in_stdout = True
            continue
        if line_str.startswith("--- stderr ---"):
            in_stdout = False
            continue
        if line_str.startswith("exit_code="):
            meta["exit"] = int(line_str.split("=", 1)[1])
        elif line_str.startswith("timed_out="):
            meta["timed_out"] = line_str.split("=", 1)[1].strip() == "True"
        elif in_stdout:
            captured.append(line_str)
    meta["stdout"] = "\n".join(captured)
    meta["wall_seconds"] = elapsed
    meta["receipt"] = text
    return meta


def classify_green(cell, meta):
    out = meta["stdout"]
    return {
        "exit_zero": meta["exit"] == 0,
        "not_timed_out": meta["timed_out"] is False,
        "executed_one": "running 1 test" in out,
        "pass_marker": cell["pass_marker"] in out,
        "test_result_ok": "test result: ok. 1 passed" in out,
    }


def classify_red(cell, mutation, meta):
    out = meta["stdout"]
    signatures = mutation["red"]
    if isinstance(signatures, str):
        signatures = [signatures]
    matched = [sig for sig in signatures if sig in out]
    result = {
        "exit_101": meta["exit"] == 101,
        "not_timed_out": meta["timed_out"] is False,
        "executed_one": "running 1 test" in out,
        "assertion_red": ("ASSERTION" in out) and bool(matched),
        "red_signature_matched": matched[0] if matched else None,
        "test_result_failed": "test result: FAILED" in out,
        "no_pass_marker": cell["pass_marker"] not in out,
        "no_build_error": ("error[E" not in out) and ("could not compile" not in out),
    }
    result["precise_red"] = all(result.values())
    return result


def write_json(path, data):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
        f.write("\n")


def main():
    only = os.environ.get("Q324_ONLY")
    # A single-cell replay must never overwrite the 12-cell machine-readable
    # records: it writes its own suffixed files instead.
    suffix = ("-only-%s" % re.sub(r"[^A-Za-z0-9_.-]", "_", only)) if only else ""
    healthy_json = os.path.join(RAW, "healthy%s.json" % suffix)
    counterfactual_json = os.path.join(RAW, "counterfactuals%s.json" % suffix)
    os.makedirs(os.path.join(RAW, "healthy"), exist_ok=True)
    os.makedirs(os.path.join(RAW, "mutants"), exist_ok=True)
    os.makedirs(os.path.join(RAW, "restored"), exist_ok=True)
    os.makedirs(os.path.join(RAW, "builds"), exist_ok=True)

    artifact_sha = sha256(ARTIFACT)
    print("worktree      :", WORKTREE)
    print("artifact      :", ARTIFACT, artifact_sha)
    print("cells         :", len(CELLS))

    selected = [c for c in CELLS if not only or only in c["id"]]
    exes = {}
    for cell in selected:
        exe, build_seconds = build(
            cell["test_bin"], os.path.join(RAW, "builds", "%s-build.log" % cell["id"])
        )
        exes[cell["id"]] = exe
        print("built %-14s %s (%.1fs)" % (cell["test_bin"], exe, build_seconds))

    healthy = []
    for cell in selected:
        log = os.path.join(RAW, "healthy", "%s.log" % cell["id"])
        meta = run_cell(cell, exes[cell["id"]], log)
        green = classify_green(cell, meta)
        ok = all(green.values())
        print("healthy %-12s %s %s" % (cell["id"], "PASS" if ok else "FAIL", green))
        healthy.append(
            {
                "cell": cell["id"],
                "arm": cell["arm"],
                "control": cell["cell"],
                "test_binary": os.path.basename(exes[cell["id"]]),
                "test_binary_sha256": sha256(exes[cell["id"]]),
                "mode": cell["mode"],
                "artifact_sha256": artifact_sha,
                "source_sha256": source_identities(),
                "log": os.path.relpath(log, EVIDENCE),
                "exit": meta["exit"],
                "timed_out": meta["timed_out"],
                "verdict": "PASS" if ok else "FAIL_ORACLE",
                "checks": green,
            }
        )
        if not ok:
            write_json(healthy_json, healthy)
            return 1
    write_json(healthy_json, healthy)

    results = []
    for cell in selected:
        mutation = MUTATIONS[cell["id"]]
        path = os.path.join(WORKTREE, mutation["file"])
        original = open(path, "rb").read()
        original_sha = hashlib.sha256(original).hexdigest()
        old = mutation["old"].encode()
        new = mutation["new"].encode()
        if original.count(old) < 1:
            print("FATAL %s: mutation anchor absent in %s" % (cell["id"], path))
            return 2
        mutated = original.replace(old, new, 1)
        mutated_sha = hashlib.sha256(mutated).hexdigest()
        print(
            "== %s applying mutation (%s) %s -> %s"
            % (cell["id"], mutation["property"], original_sha[:12], mutated_sha[:12])
        )
        red_meta = None
        mutant_exe = None
        restored_ok = False
        try:
            with open(path, "wb") as f:
                f.write(mutated)
            mutant_exe, _ = build(
                cell["test_bin"],
                os.path.join(RAW, "builds", "%s-mutant-build.log" % cell["id"]),
            )
            mutant_sha = sha256(mutant_exe)
            red_log = os.path.join(RAW, "mutants", "%s-red.log" % cell["id"])
            red_meta = run_cell(cell, mutant_exe, red_log)
            red = classify_red(cell, mutation, red_meta)
        finally:
            with open(path, "wb") as f:
                f.write(original)
            restored_ok = sha256(path) == original_sha
        if not restored_ok:
            print("FATAL %s: restoration sha mismatch" % cell["id"])
            return 3
        print(
            "   RED: %s => %s"
            % (red, "PRECISE_RED" if red["precise_red"] else "INVALID_RED")
        )

        exe_g, _ = build(
            cell["test_bin"],
            os.path.join(RAW, "builds", "%s-restored-build.log" % cell["id"]),
        )
        restored_exe_sha = sha256(exe_g)
        green_log = os.path.join(RAW, "restored", "%s-green.log" % cell["id"])
        green_meta = run_cell(cell, exe_g, green_log)
        green = classify_green(cell, green_meta)
        restored_green = all(green.values())
        print("   restored GREEN: %s %s" % (restored_green, green))

        results.append(
            {
                "cell": cell["id"],
                "arm": cell["arm"],
                "control": cell["cell"],
                "property_broken": mutation["property"],
                "mutation_file": mutation["file"],
                "base_file_sha256": original_sha,
                "mutated_file_sha256": mutated_sha,
                "restored_file_sha256": sha256(path),
                "mutation_applied_witness": mutated_sha != original_sha,
                "mutant_test_binary": os.path.basename(mutant_exe),
                "mutant_test_binary_sha256": mutant_sha,
                "restored_test_binary": os.path.basename(exe_g),
                "restored_test_binary_sha256": restored_exe_sha,
                "red_log": os.path.relpath(red_log, EVIDENCE),
                "green_log": os.path.relpath(green_log, EVIDENCE),
                "red_exit": red_meta["exit"],
                "red_timed_out": red_meta["timed_out"],
                "red_checks": red,
                "precise_red": red["precise_red"],
                "red_signature": mutation["red"],
                "red_signature_matched": red.get("red_signature_matched"),
                "restored_green": restored_green,
                "restored_checks": green,
            }
        )
        if not (red["precise_red"] and restored_green):
            write_json(counterfactual_json, results)
            return 1
    write_json(counterfactual_json, results)
    print("ALL 12 COUNTERFACTUALS: PRECISE_RED, restorations GREEN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
