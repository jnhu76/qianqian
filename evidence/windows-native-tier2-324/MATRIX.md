# MATRIX — 12-cell Windows native disposition

```text
ARTIFACT   libsongcore.a  sha256 90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2
           (windows-x86_64, mingw-w64 COFF; see ARTIFACT.md)
FFMPEG     n9.0.1  bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa
HOST       Windows 10.0.26300 x64, real host; one FRESH process per cell
SOURCE     9ffcaba945eb8f47b9e30dddc67c805a5bda078a (detached git worktree)
```

| Arm | Cell | Healthy | Key evidence (healthy receipt) | Counterfactual | Exact RED | Restored |
|---|---|---|---|---|---|---|
| ffmpeg-bridge | C1 | PASS | `SINK_HOLD_ENTERED queued_bytes=65536 flood_bytes=65536 flood_outstanding=1 second_write_outstanding=1`; `EXPORTER_ENTERED=1 SINK_STILL_BLOCKED=1 exported=0 export_failed=0`; `REGISTRATION attempted=4 accepted=2 rejected=2 outstanding=2`; `SHUTDOWN drained=false abandoned=true undelivered_at_exit=2` | exit 101, assertion fired, no PASS marker, not timed out | `PRODUCER-NOT-BACKPRESSURED` | GREEN |
| ffmpeg-bridge | C2 | PASS | `REGISTRATION attempted=4 accepted=2 rejected=2 outstanding=2`; `SHUTDOWN drained=false abandoned=true undelivered_at_exit=3` | exit 101, assertion fired, no PASS marker, not timed out | `TEARDOWN-OUTCOME: a stopped consumer must yield an honest abandoned shutdown` | GREEN |
| ffmpeg-bridge | C3 | PASS | `fill_attempts=3 accepted=2 rejected=1 first_reject_attempt=Some(3)`; `TARGET attempts=4 accepted=0 rejected=4`; `SHUTDOWN drained=false abandoned=true undelivered_at_exit=3` | exit 101, assertion fired, no PASS marker, not timed out | `REJECT-ACCOUNTING: every full-pool attempt must be rejected and counted` | GREEN |
| ffmpeg-bridge | C4 | PASS | `BROKEN-STDERR-CONFIRMED raw_os_error=232`; `COUNTERS copied_out=2 exported=0 export_failed=2 self_diagnostics=1`; `SHUTDOWN drained=true abandoned=false undelivered_at_exit=0` | exit 101, assertion fired, no PASS marker, not timed out | `LOSS-ACCOUNTING: a failed write must never be counted as delivered` | GREEN |
| decode-read | C1 | PASS | `SINK_HOLD_ENTERED … flood_outstanding=1 second_write_outstanding=1`; `POOL attempted=4 accepted=2 rejected=2 copied_out=0 outstanding=2`; `SHUTDOWN drained=false abandoned=true undelivered_at_exit=3` | exit 101, assertion fired, no PASS marker, not timed out | `C1-PRODUCER-PROGRESS` | GREEN |
| decode-read | C2 | PASS | `POOL attempted=4 accepted=2 rejected=2 copied_out=0 outstanding=2`; `SHUTDOWN drained=false abandoned=true undelivered_at_exit=3` | exit 101, assertion fired, no PASS marker, not timed out | `C2-TEARDOWN: shutdown did not return inside its bounded deadline` | GREEN |
| decode-read | C3 | PASS | `POOL attempted=6 accepted=2 rejected=4 copied_out=0 outstanding=2`; `SHUTDOWN drained=false abandoned=true undelivered_at_exit=3` | exit 101, assertion fired, no PASS marker, not timed out | `C3-DROP-ACCOUNTING: every presentation over capacity is rejected+counted` | GREEN |
| decode-read | C4 | PASS | `BROKEN-STDERR-CONFIRMED raw_os_error=232`; `COUNTERS copied_out=2 exported=0 export_failed=2 self_diagnostics=1`; `SHUTDOWN drained=true abandoned=false undelivered_at_exit=0` | exit 101, assertion fired, no PASS marker, not timed out | `C4-WRITE-FAILURE: a failed export is never counted as delivered` | GREEN |
| decode-seek | C1 | PASS | `PRODUCER diag="could not reach a landing point" post="decode error" finished_while_held=true sink_still_blocked=true`; `ALPHA attempted=8 accepted=2 rejected=6 outstanding=2`; `SHUTDOWN drained=true abandoned=false undelivered=0 exported=3` | exit 101, assertion fired, no PASS marker, not timed out | `C1 ISOLATION: the producer returned only after the blocked sink was released` | GREEN |
| decode-seek | C2 | PASS | `ALPHA attempted=8 accepted=2 rejected=6 outstanding=2`; `SHUTDOWN elapsed_ms=50 drained=false abandoned=true undelivered=3` | exit 101, assertion fired, no PASS marker, not timed out | `C2 HONEST-ACCOUNTING: a stopped drainer must NEVER be reported drained` | GREEN |
| decode-seek | C3 | PASS | `fill accepted=2 pool_full_seen=true`; `ALPHA attempted=11 accepted=2 rejected=9 outstanding=2`; `SHUTDOWN drained=false abandoned=true undelivered=3` | exit 101, assertion fired, no PASS marker, not timed out | `C3 ISOLATION: the real producer's records were rejected at the full` | GREEN |
| decode-seek | C4 | PASS | `BROKEN-STDERR-CONFIRMED raw_os_error=232`; `FAILING diag=… post=… counters copied_out=2 exported=4 export_failed=0 outstanding=2`; `SHUTDOWN drained=true abandoned=false copied_out=4 exported=4 export_failed=2 self_diagnostics=1` | exit 101, assertion fired, no PASS marker, not timed out | `C4: ACCOUNTING-ORACLE at quiescence` | GREEN |

Machine-readable records: `raw/healthy.json` (12 healthy verdicts) and
`raw/counterfactuals.json` (12 mutation/RED/restore records with base, mutated
and restored file hashes, mutant/restored test-binary hashes, and every
classification field). Logs: `raw/healthy/`, `raw/mutants/`, `raw/restored/`,
build transcripts in `raw/builds/`.

## Producer definitions (real paths, same as the Linux arm)

```text
ffmpeg-bridge   real FFmpeg av_log from a real SongCore decode of the committed
                FLAC fixture truncated to 100,000 bytes
                (sha256 0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f),
                QIANQIAN_FFMPEG_LOG=1, real Rust bridge installed by plugin
                activation, real class-α admission; first fault at 115,200
                frames (25 x 4608), "pending decode error"

decode-read     committed FLAC fixture (sha256 d8062668…), byte 76000 ^= 0x01
                inside a frame body; real song_read_pcm returns the native decode
                error; the adapter's typed fault (status 108) is emitted

decode-seek     committed FLAC fixture with three frame BODY bits flipped
                (73,502 / 77,042 / 80,375 each ^= 1; headers intact), seek 2.0 s
                lands on the corrupt body; reliable results are
                "could not reach a landing point" and a post-seek "decode error"
```

## RED validity (all 12)

Each mutant receipt satisfies all of:

```text
executed_one            the test binary printed "running 1 test"
exit_101                the test failed, not the build
assertion_red           the harness's stdout panic hook printed ASSERTION and the
                        named assertion text appears in the log
test_result_failed      "test result: FAILED"
no_pass_marker          the cell's PASS marker is absent
no_build_error          no "error[E…" / "could not compile"
not_timed_out           the OUTER 180 s watchdog did not kill the child
```

Two RED styles are used, and they are labelled rather than conflated:

```text
STATE      the assertion reads an illegal state/counter (8 cells)
LIVENESS   the cell's OWN bounded wait for a progress event expires while the
           property that forbids that wait is the mutation (4 cells:
           ffmpeg-C1 PRODUCER-NOT-BACKPRESSURED, read-C1 PRODUCER-PROGRESS,
           read-C2 TEARDOWN bounded deadline, seek-C1 ISOLATION after the
           fail-closed lock valve releases at 10 s)
```

A LIVENESS RED is still an in-test named assertion that exits 101 — not the
outer watchdog, not a build error, not a zero-test arm. The alternative (an
unbounded wait) would be a hang, which the task forbids as evidence. See
REVIEW.md finding D-1 for the strict-vs-scoped reading, and `ORACLE-CLASS.md`
for the oracle-class analysis: no in-process product-visible state oracle
exists for those four, while an out-of-process thread-context oracle exists in
principle for three of them (one mutation-aware only) and is documented there
as an option that was not adopted.

seek-C4's counterfactual zeroes the *first* failure counter; the disabled-
exporter path still counts later losses, so the exact RED is the drain-boundary
identity (`C4: ACCOUNTING-ORACLE at quiescence`, left 6 / right 5), not the
line-1048 `export_failed` assertion. `raw/counterfactuals.json` records the
signature that actually fired.

No arm was killed by the watchdog, and no arm failed during compilation. The
mutation-applied witness is the before/after sha256 pair of the mutated file
(`raw/counterfactuals.json`), and every restored file hash equals the base hash
exactly (LF worktree, byte-level replacement).

## Windows-specific confirmations

- C4's failure is `ERROR_NO_DATA (232)` from a real anonymous pipe whose read
  end was closed — on the same process stderr handle the real
  `StderrExporter` writes to (canary before producer work). `NUL` is never used
  as a failing sink.
- C1 uses two distinct, labelled premises: OS pipe backpressure for
  ffmpeg-bridge/decode-read (a parked 64 KiB write plus an independent parked
  1-byte write), and the process-wide std stderr lock for decode-seek (which is
  released after the producer so the drainer can settle to a real complete
  drain). They are not reported as each other.
- The number of real av_log records the Windows artifact produces is the same
  as Linux: two (`attempted=4 = 2 av_log + 2 fault`), and C3 fills exactly the
  same six presentations (`attempted=6 accepted=2 rejected=4`).

## NOT CLAIMED

- A full Playback Session / D11 terminal Fact is **not** mounted in these
  harnesses. The producer thread plays the isolation-obligated class-α role
  over the real plugin/adapter/bridge/facility; terminal-outcome claims stay at
  the facility's `ShutdownReport` and the adapter's reliable results.
- Allocator instrumentation is not performed. C3's "no unbounded allocation" is
  structural (two preallocated slots, one bounded publish) plus the observed
  `outstanding ≤ 2`.
- No Windows arm64, MSVC, DLL/shared, or packaged-release artifact is claimed.
- No audible/device output claim is made; this campaign covers decode/diagnostic
  outlets only.
