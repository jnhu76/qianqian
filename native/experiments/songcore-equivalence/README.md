# SongCore port-equivalence experiment

Native-only proof that the selectively ported SongCore (current main) is
the same Media→PCM mechanism as the historical evidence substrate
(`evidence/native-repro-corrective-1` @ fb3a3ba), measured against the
corrected decode baseline (#112 @ f3ca50a).

## What it proves

1. **Correctness (hard gate)** — for every fixture (MP3 / FLAC / ALAC):
   open → probe → decode to clean EOF; canonical Float32 PCM SHA-256,
   PCM frame count and terminal semantics must match the frozen
   historical reference exactly. One mismatch fails the run.
2. **ABI gate** — `libsongcore.so` must export exactly the 15 ABI v1
   functions, with zero `av_*`/`ff_*`/`swr_*` symbols and zero FFmpeg
   types in `songcore.h`.
3. **Performance shape** — whole-file decode throughput (×realtime),
   time-to-first-PCM and `song_read_pcm` p50/p99/max at block 1024,
   compared with the frozen #112 B1 (public ABI layer) medians; a
   throughput regression beyond 10% flags `PERF_REGRESSION_SUSPECT`.

## Result (2026-09-12, same host as #112)

Correctness: **PASS 4/4** — PCM SHA-256, frame counts and terminal
semantics are bit-exact against the historical mechanism (MP3 / FLAC /
ALAC, short + long fixtures).

Performance, same-day apples-to-apples A/B (the historical benchmark
driver compiled once, linked against the historical and the ported
archive, interleaved on the pinned CPU, `results/same-day-ab.json`):
median deltas **−0.04% … +2.14%**, max |delta| ≈ 2.14%, with identical
canonical PCM hashes on every fixture.

Verdict: **performance equivalent within measurement noise. No
optimization claim.**

The frozen #112 cross-day medians remain in `reference.json` and are
still compared as a regression guard only (a throughput drop beyond 10%
flags `PERF_REGRESSION_SUSPECT`). Cross-session numbers for this port
were several percent faster, but that is a cross-session / host-state
measurement difference — not evidence of optimization and not the
equivalence verdict.

Long real-media (the 224 s MP3 used by the #112 evidence corpus) is
deliberately not part of the committed fixture set. Long-file
performance remains available in the #112 evidence; the file itself
lives in a local/optional corpus, not in this repository.

An earlier measurement round flagged ~-50% "regressions"; that was
measurement contamination (a stray historical benchmark process was
still running on the pinned CPU). After killing it, all numbers
recovered to parity. Recorded here so the false alarm is not silently
forgotten.

## Running it

```bash
cd native
xmake ffmpeg-import && xmake f -m release -y && xmake build songcore
python3 experiments/songcore-equivalence/run.py            # full run
python3 experiments/songcore-equivalence/run.py --skip-perf  # gates only
```

Everything is native: C harness + Python driver, no Rust, no Cargo, no
historical worktree. Fixtures are sha256-frozen copies owned by this
experiment (`reference.json`); the harness compiles against
`build/artifacts/libsongcore.a` at `-O2`.

Output: `results/songcore-equivalence.json` (machine-readable) plus this
summary.
