# windows-native-tier2-324 — Windows native 12-cell fault matrix

```text
TRUTH CLASS   executed verification evidence / implementation-record input
              NOT architecture authority
SUBJECT       #324: the 12 FFmpeg/decode Tier-2 cells transferred from #296
              ffmpeg-bridge / decode-read / decode-seek ×
              C1 BLOCKED_SINK / C2 STOPPED_CONSUMER /
              C3 CAPACITY_EXHAUSTION / C4 WRITE_FAILURE
PLATFORM      Windows 10.0.26300 x86_64 (real host), rustc/cargo 1.96.0,
              target x86_64-pc-windows-gnu, WinLibs GCC 16.2.0,
              FFmpeg n9.0.1 static closure (target-specific, mingw32)
ARTIFACT      native/build/artifacts/libsongcore.a
              sha256 90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2
              280/280 COFF pe-x86-64 members, 15/15 ABI v1 symbols
VERDICT       WINDOWS_NATIVE_TIER2 = 12/12 DISPOSITIONED
              each cell: healthy PASS, counterfactual PRECISE_RED,
              restored GREEN
```

## What is claimed

Each of the twelve cells has, **on a real Windows host, in its own fresh
process**:

- a real native producer — a real FFmpeg `av_log` callback from a real SongCore
  decode, a real `song_read_pcm` decode fault, or a real `song_seek` landing
  fault — never a fabricated `DecodeError` or a mock writer;
- the real `qianqian-diagnostics` facility (class-α pool + single drainer +
  exporter) and, for the FFmpeg arm, the real opt-in Rust bridge installed by
  real plugin activation. C1/C4 write through the real `StderrExporter`; C2/C3
  intentionally use a test-owned exporter that parks the sole drainer inside
  `export` (they claim a stopped consumer and pool accounting, not a real sink
  write);
- an independent premise witness: real OS pipe backpressure (two parked raw
  writes) or the real process-wide std stderr lock for C1; a real controlled
  drainer stop for C2; a real full two-slot pool for C3; a real
  `ERROR_NO_DATA (232)` write failure on the same handle the exporter uses for
  C4;
- assertions on producer progress, reliable result, terminal/teardown outcome
  and loss accounting that do not depend on sink health;
- one minimal source mutation that breaks exactly the claimed property,
  producing an assertion RED (exit 101, named assertion, no compile failure, no
  zero-test arm, no watchdog kill), followed by byte-restored GREEN.

## What is *not* claimed

- The full Playback Session / D11 terminal Fact is not mounted. These harnesses
  run at the producer-thread + plugin/adapter/bridge/facility level. See
  MATRIX.md "NOT CLAIMED".
- No allocator instrumentation. C3's boundedness is structural plus the
  observed `outstanding ≤ 2`.
- No Windows arm64 / MSVC / shared-DLL / release-packaging claim. The artifact
  is verification-grade only; #172 owns release provenance.
- No product behavior change: this campaign modified no production source, no
  ABI, and no build recipe.

## Contents

```text
README.md          this index
ENVIRONMENT.md     Windows host, toolchains, measured sink primitives,
                   Windows-vs-Linux differences, out-of-scope build findings
ARTIFACT.md        the verified windows-x86_64 artifact, closure provenance,
                   ABI gate and real C consumer smoke
MATRIX.md          the 12-cell table, RED validity, Windows confirmations, limits
ORACLE-CLASS.md    why four REDs are bounded-liveness assertions: no in-process
                   product-visible state oracle exists for their mutations, and
                   the out-of-process thread-context option is documented but
                   not adopted
EXECUTION_DAGS.md  happens-before structure per arm (SW / SCHED / LIVE)
TEST-QUALITY.md    six-question authoring record + CI-coverage disclosure
COMMANDS.md        exact replay procedure on a Windows host
REVIEW.md          independent review (A artifact / B mechanism / C oracle / D contract)
RECEIPTS.sha256    checksums of every evidence file
harness/           the three fault harnesses, the child runner, the
                   counterfactual orchestrator, the smoke suite, the C consumer
raw/smoke/         environment, artifact identity, sink primitives, S1/S2/S3
raw/healthy/       12 fresh-process healthy receipts
raw/mutants/       12 mutation receipts (assertion RED)
raw/restored/      12 restored-GREEN receipts
raw/builds/        cargo build transcripts for every arm
raw/healthy.json   machine-readable healthy verdicts
raw/counterfactuals.json
                   machine-readable mutation/RED/restore records + hashes
```

## Reproduce

`COMMANDS.md` is the procedure. Short form, from a clean detached worktree at
`9ffcaba` with the Windows mingw toolchain on `PATH`:

```bash
python3 evidence/windows-native-tier2-324/harness/counterfactual_runner.py
bash    evidence/windows-native-tier2-324/harness/run_smoke.sh
```

The runner prints `ALL 12 COUNTERFACTUALS: PRECISE_RED, restorations GREEN` on
success and writes every receipt above.
