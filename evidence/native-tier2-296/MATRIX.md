# 12-cell matrix

Artifact: `libsongcore.a` `a0a3baf8eb61be8e991517d9653156a3efe0f67c9ba09fff1deb30187e5fe056`.
FFmpeg pin: `n9.0.1 bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`.
Main replay: `61d834ad63bd5b0195496cf9de3a77395dd12a71` (post-#320).
One fresh process per cell; the facility and `av_log` callback are process-global.

| Arm | Cell | Main replay | Key evidence (main log) | Exact counterfactual RED (main) |
|---|---|---|---|---|
| ffmpeg-bridge | C1 BLOCKED_SINK | PASS | real stderr lock held; drainer entered real export; `attempted=4 accepted=2 rejected=2`; `shutdown drained=false abandoned=true undelivered=2` | producer waits on full pool → `PRODUCER-NOT-BACKPRESSURED`, exit 101 |
| ffmpeg-bridge | C2 STOPPED_CONSUMER | PASS | `beta_queued=1 beta_consumed=1`; producer completed; `shutdown abandoned=true undelivered=3` | stuck drainer claims `drained=true` → `TEARDOWN-OUTCOME`, exit 101 |
| ffmpeg-bridge | C3 CAPACITY_EXHAUSTION | PASS | fill `attempts=3 accepted=2 rejected=1`; target `attempts=4 accepted=0 rejected=4`; `outstanding=2`; no block | rejected counter removed → `REJECT-ACCOUNTING left 0 right 4`, exit 101 |
| ffmpeg-bridge | C4 WRITE_FAILURE | PASS | `/dev/full` canary Err; real export fails; `copied_out=2 = exported=0 + export_failed=2`; `self_diagnostics=1`; drained | failed export counted delivered → `LOSS-ACCOUNTING`, exit 101 |
| decode-read | C1 BLOCKED_SINK | PASS | real stderr lock; drainer blocked in real `StderrExporter`; producer returned `attempted=4 accepted=2 rejected=2 outstanding=2`; `shutdown abandoned=true undelivered=3` | synchronous producer `write_stderr_line` → `C1-PRODUCER-PROGRESS`, exit 101 |
| decode-read | C2 STOPPED_CONSUMER | PASS | drainer held in export; producer returned; `attempted=4 accepted=2 rejected=2`; `shutdown abandoned=true undelivered=3` | shutdown deadline branch disabled → `C2-TEARDOWN` bounded-deadline assertion, exit 101 |
| decode-read | C3 CAPACITY_EXHAUSTION | PASS | fill plus target: `attempted=6 accepted=2 rejected=4 outstanding=2`; every rejection counted; no block | full-pool rejection path removed → `C3-DROP-ACCOUNTING left 0 right 4`, exit 101 |
| decode-read | C4 WRITE_FAILURE | PASS | real failing sink; `copied_out=2 exported=0 export_failed=2 self_diagnostics=1`; drained | failed export counted delivered → `C4-WRITE-FAILURE left 1 right 0`, exit 101 |
| decode-seek | C1 BLOCKED_SINK | PASS | producer `finished_while_held=true sink_still_blocked=true elapsed_ms=1`; `attempted=8 accepted=2 rejected=6`; post-release `exported=3` | synchronous producer write on blocked sink → `C1 ISOLATION`, exit 101 |
| decode-seek | C2 STOPPED_CONSUMER | PASS | `beta_queued=1 beta_consumed=1`; `shutdown elapsed_ms=50 drained=false abandoned=true undelivered=3` | stopped drainer reported drained → `C2 HONEST-ACCOUNTING`, exit 101 |
| decode-seek | C3 CAPACITY_EXHAUSTION | PASS | `pool_full_seen=true`; `attempted=11 accepted=2 rejected=9 outstanding=2`; no block | rejected counter removed → `C3 REJECT-ACCOUNTING`, exit 101 |
| decode-seek | C4 WRITE_FAILURE | PASS | healthy export then real failing export: `copied_out+beta_consumed = exported+export_failed`; `self_diagnostics=1`; drained | failed export counted delivered → `C4 LOSS-ACCOUNTING`, exit 101 |

## Producer definitions

```text
ffmpeg-bridge   real FFmpeg av_log from a real SongCore decode of a committed
                FLAC fixture truncated to 100000 bytes
                (sha256 0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f),
                QIANQIAN_FFMPEG_LOG=1, real Rust bridge installed by plugin activation,
                real class-alpha admission.

decode-read     committed FLAC fixture (sha256 d8062668...), byte 76000 ^= 0x01
                inside a frame body; real song_read_pcm returns the native
                decode error ("pending decode error"); C probe attributes
                SONG_ERR_DECODE_ERROR (108), native=-1094995529.

decode-seek     committed FLAC fixture with three frame BODY bits flipped
                (73_502 / 77_042 / 80_375 each ^= 1; headers intact), seek 2.0 s
                lands on the corrupt body; ProviderSeekOutcome::MutatedThenFailed
                ("could not reach a landing point"); post-seek read returns
                "decode error"; best-effort class-alpha fault record emitted.
```

## RED validity (all 12)

Each main-replay RED satisfies: the test binary executed (non-zero test count),
failed inside an `assert!`/`panic!` with the named signature, exited 101, emitted
no compile error, and did not print a CELL-PASS marker. The mutation source was
then restored byte-for-byte and the same cell re-ran GREEN. Logs:
`raw/main-replay/red/` and `raw/main-replay/green/`.

## NOT_RUN

- Windows native 12/12: #172 S2 artifact/runtime absent. No cross-platform
  inference from these Linux rows.
- Production Playback Session worker registration is not mounted in these
  harnesses; the harness thread plays the isolation-obligated producer role over
  the real plugin/adapter/bridge/facility.
- Typed `(status, native_code)` attribution is not observable in the ffmpeg
  cells whose records were rejected/undelivered; closeout2's C probe owns that
  receipt.
- Allocator instrumentation: not performed (see C3 limit in README).
