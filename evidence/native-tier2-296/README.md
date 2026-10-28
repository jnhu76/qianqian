# native-tier2-296 — Linux native 12-cell fault matrix (executed evidence)

```text
TRUTH CLASS   executed verification evidence / implementation-record input
              NOT architecture authority
SUBJECT       #296: the 12 native runtime cells previously recorded NOT_RUN
              ffmpeg-bridge / decode-read / decode-seek ×
              C1 BLOCKED_SINK / C2 STOPPED_CONSUMER /
              C3 CAPACITY_EXHAUSTION / C4 WRITE_FAILURE
PLATFORM      Linux x86_64 (WSL2 kernel 6.18.54.1), rustc/cargo 1.97.1,
              gcc 15.2.0, FFmpeg n9.0.1 static closure
ARTIFACT      libsongcore.a
              sha256 a0a3baf8eb61be8e991517d9653156a3efe0f67c9ba09fff1deb30187e5fe056
              (4,067,772 bytes; built from the fixed C gate)
VERDICT       12/12 PASS on Linux; 12/12 precise counterfactual RED with
              restored GREEN; Windows native 12/12 NOT_RUN (#172 S2)
```

## What is claimed

Each cell has an executed Linux native run and a killed mutation:

- the producer is a real SongCore/FFmpeg path (committed FLAC fixture, real
  corruption/seek, real `av_log` callback), not a mock;
- the diagnostic outlet is the real `qianqian-diagnostics` facility;
- each cell asserts producer progress, reliable result, terminal/teardown
  outcome and loss accounting independently of sink health;
- each cell has a source mutation that breaks exactly the claimed property and
  produces an assertion RED (test executed, exit 101, no compile error, no
  zero-test run, no watchdog kill), followed by byte-restored GREEN.

## Replay on post-#320 main

The closeout-3 execution was at `a343abb` (PR #320 head). This package was
re-executed on `main@61d834ad` after #320 merged:

```text
main native/src/songcore_ffmpeg.c sha256 = 55eab243...  (identical to a343abb)
artifact used for replay                 = a0a3baf8...  (the verified closeout2 artifact)
12/12 healthy cells                      = PASS on main
12/12 counterfactuals                    = PRECISE_RED + restored GREEN on main
```

A fresh rebuild in a different checkout can produce a different `libsongcore.a`
archive hash because the merged FFmpeg objects embed absolute source paths.
That does not change the C gate: `songcore_ffmpeg.c.o` is byte-identical between
builds (independently checked in the #320 delta review). The replay therefore
uses the verified archive whose C source hash is identical on main, and records
the archive identity explicitly; it does not infer a hash-equality claim for
path-dependent rebuilds.

## Contents

```text
README.md       this index
MATRIX.md       the 12-cell table with main-replay evidence and limits
COMMANDS.md     exact setup/run/counterfactual commands on main
harness/        the three test-only harnesses and mutation runners
raw/main-replay/healthy/    12 fresh main-run logs
raw/main-replay/red/        12 main counterfactual RED logs
raw/main-replay/green/      12 restored-GREEN logs
raw/closeout3/  original a343abb ten-tuples/results/cells/status/environment
RECEIPTS.sha256
```

## Scope and limits

- **Windows native: 12/12 NOT_RUN.** The #172 S2 verified Windows artifact and
  runtime evidence are absent. No Linux PASS is transferred to Windows.
- The ffmpeg-bridge cells exercise the real `av_log` producer and the real
  bridge/admission/facility at the producer-thread level; a Playback Session is
  not mounted there.
- The decode-read/seek cells exercise the real native adapter and diagnostics
  facility at the adapter level; no Playback Session/D11 terminal Fact is
  involved.
- C1's blocking premise is a real std stderr lock held by a helper and the real
  `StderrExporter` observed blocked inside its write — not a sleep inference.
- C3's "no unbounded allocation" is supported structurally (two preallocated
  slots, one bounded copy, single-attempt publish, no loop/CAS/lock/allocation
  on the fault path) plus the observed `outstanding <= 2`; it is **not** an
  allocator-instrumented measurement.
- C4 uses a real OS write failure (`/dev/full` ENOSPC / closed read end), not a
  mock writer.
- The harnesses are evidence-only and are copied into a scratch worktree; they
  are not part of the product build or CI.
- These results do not close #296 by themselves: the three KNOWN_RESIDUAL
  owner dispositions and the Windows platform obligation remain separate.

## Original records

`raw/closeout3/` preserves the closeout-3 artifacts byte-for-byte:
`ffmpeg-bridge` status/ten-tuples/artifact identity/validation, `decode-read`
results JSON, `decode-seek` cells/counterfactuals/environment/receipts. The
fresh `raw/main-replay/` logs are the post-#320 replay.
