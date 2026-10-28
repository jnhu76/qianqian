# Replay commands (Linux x86_64)

These are the commands used for the post-#320 replay on
`main@61d834ad`. Paths use `$WT` for the scratch worktree and `$PKG` for this
evidence package. The harnesses are test-only files copied into the scratch
worktree; the product tree is not modified.

## 0. Setup

```bash
cd /path/to/qianqian
git worktree add --detach "$WT" origin/main
cd "$WT"
mkdir -p native/build/artifacts
cp -a /path/to/verified-artifact-tree/native/build/artifacts/. native/build/artifacts/
sha256sum native/build/artifacts/libsongcore.a
# must be a0a3baf8eb61be8e991517d9653156a3efe0f67c9ba09fff1deb30187e5fe056

cp "$PKG/harness/native_ffmpeg_bridge_controls.rs" crates/qianqian-decode-songcore/tests/
cp "$PKG/harness/native_decode_read_controls.rs"   crates/qianqian-decode-songcore/tests/
cp "$PKG/harness/decode_seek_controls.rs"          crates/qianqian-decode-songcore/tests/

export QIANQIAN_NATIVE_DIR="$WT/native"
MANIFEST="$WT/crates/qianqian-decode-songcore/Cargo.toml"
```

Build:

```bash
for t in native_ffmpeg_bridge_controls native_decode_read_controls decode_seek_controls; do
  cargo test --manifest-path "$MANIFEST" --test "$t" --no-run || exit 1
done
```

## 1. Healthy cells (one fresh process each)

```bash
for c in C1 C2 C3 C4; do
  QIANQIAN_FFMPEG_LOG=1 QFB_CELL=$c timeout -k 5 90 \
    cargo test --manifest-path "$MANIFEST" --test native_ffmpeg_bridge_controls -- \
      --nocapture --test-threads=1 qfb_native_ffmpeg_bridge_cell
done

for c in c1_blocked_sink_native_decode_read \
         c2_stopped_consumer_native_decode_read \
         c3_capacity_exhaustion_native_decode_read \
         c4_write_failure_native_decode_read; do
  timeout -k 5 90 cargo test --manifest-path "$MANIFEST" \
    --test native_decode_read_controls -- --ignored --exact "$c" \
    --nocapture --test-threads=1
done

for c in c1_blocked_sink c2_stopped_consumer c3_capacity_exhaustion c4_write_failure; do
  timeout -k 5 90 cargo test --manifest-path "$MANIFEST" \
    --test decode_seek_controls -- --exact "$c" --nocapture --test-threads=1
done
```

Expected: every cell exits 0, prints its `*-PASS`/`*-VERDICT=PASS` line and
`test result: ok. 1 passed`.

## 2. Counterfactuals

Every counterfactual must be an assertion RED: the test binary runs, fails at
the named assertion, exits 101, and prints no PASS marker. Restore the source
byte-for-byte and re-run the same cell GREEN. Logs are in
`raw/main-replay/{red,green}/`.

### ffmpeg-bridge

`harness/mutate.py` applies C1–C4 (it loads `harness/base/base-pool.rs` and
`base-lib.rs`). Run from `$WT`:

```bash
for c in C1 C2 C3 C4; do
  python3 "$PKG/harness/mutate.py" $c
  QIANQIAN_FFMPEG_LOG=1 QFB_CELL=$c timeout -k 5 90 \
    cargo test --manifest-path "$MANIFEST" --test native_ffmpeg_bridge_controls -- \
      --nocapture --test-threads=1 qfb_native_ffmpeg_bridge_cell
  # expect exit 101 and the QFB-MUTANT-Cx signature
  cp "$PKG/harness/base/base-pool.rs" crates/qianqian-diagnostics/src/pool.rs
  cp "$PKG/harness/base/base-lib.rs"  crates/qianqian-diagnostics/src/lib.rs
  # rerun the same command: expect PASS
done
```

### decode-read

Apply each mutation to the named file, run the cell with `--ignored --exact`,
then restore with `git checkout -- <file>` and rerun:

```text
C1  crates/qianqian-decode-songcore/src/lib.rs
    after "fn emit_decode_fault(fault: &DecodeFault) {" insert
    "let _ = qianqian_diagnostics::write_stderr_line(...);" before the Record.
    RED: C1-PRODUCER-PROGRESS

C2  crates/qianqian-diagnostics/src/lib.rs
    "if started.elapsed() >= deadline {" -> "if false && started.elapsed() >= deadline {"
    RED: C2-TEARDOWN bounded-deadline assertion

C3  crates/qianqian-diagnostics/src/pool.rs
    full-pool branch "rejected += 1; return false;" -> "return true;"
    RED: C3-DROP-ACCOUNTING left rejected=0 right 4

C4  crates/qianqian-diagnostics/src/lib.rs
    Err(error) export branch: add "exported += 1" as well as "export_failed += 1"
    RED: C4-WRITE-FAILURE left exported=1 right 0
```

### decode-seek

Run the runner with the scratch checkout and a receipt directory:

```bash
QIANQIAN_NATIVE_WORKTREE="$WT" QIANQIAN_NATIVE_DIR="$WT/native" \
  python3 "$PKG/harness/counterfactual_runner.py"
```

It applies each mutation, requires `PRECISE_RED`, restores the original bytes,
verifies the SHA, and reruns each cell GREEN. Expected final line:
`ALL COUNTERFACTUALS: PRECISE_RED, restorations GREEN`.

## 3. Cleanup

```bash
cd /path/to/qianqian
git worktree remove --force "$WT"
```

Any child used for the blocked-sink controls is killed by the harness; the
`timeout -k 5 90` is a fail-closed watchdog, never the oracle.

## 4. Windows

Not replayable here. Windows native 12/12 is NOT_RUN until #172 S2 provides a
verified Windows artifact and runtime evidence.
