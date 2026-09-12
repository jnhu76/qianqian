# SongCore call-comparison experiment

Measures the cost of crossing the SongCore C ABI from Rust through
`crates/qianqian-songcore-sys`, compared with a C caller that links and calls
the exact same artifact.

The one question this experiment answers:

> After Rust crosses the current SongCore C ABI, how much more performance
> does it cost compared with a direct C caller?

It does not design plugins, PCM transport, or decoder capabilities.

## Layout

```text
experiments/songcore-call-comparison/
├── c/
│   ├── caller.c          C caller: correct / surface / steady / latency / ttfp / floor
│   └── layout_probe.c    C ABI layout probe (compiled against the real songcore.h)
├── src/
│   ├── main.rs           driver: gates, interleaved A/B, results JSON
│   ├── rust_caller.rs    Rust caller via qianqian-songcore-sys (mirrors caller.c)
│   ├── layout.rs         Rust ABI layout probe (mirrors layout_probe.c)
│   ├── sha256.rs         SHA-256 for PCM identity
│   └── compare.rs        median / percentile / verdict statistics
└── results/
    ├── c-vs-rust-ffi-cost.json   committed evidence for one clean run
    ├── layout-c.txt              C probe output of that run
    └── layout-rust.txt           Rust probe output of that run
```

The same `caller.c` is built twice: a standalone C binary (with `main`) and a
main-less object linked into the driver so the C caller and the Rust caller
can be interleaved inside one pinned process. The standalone binary exists so
reviewers can run the C side independently.

## Reproduce

```bash
cd native
xmake ffmpeg-import
xmake f -m release -y
xmake build songcore

cd ../experiments/songcore-call-comparison
cargo run --release -- layout
cargo run --release -- all          # writes results/, run under taskset -c <cpu>
```

The canonical measurement command is:

```bash
taskset -c 2 cargo run --release -- all
```

Both crates build-link `native/build/artifacts/libsongcore.a` and fail closed
if it is absent. No system SongCore, no alternate FFmpeg, no downloaded
binaries.

## Gates

1. **Layout gate** — the C probe and the Rust probe each print, from their own
   compiler: every crossing struct's size/alignment, every field's offset and
   field size, every status/channel/scope/role constant, the callback pointer
   sizes, and the ABI version. The driver compares the two outputs
   line-for-line; any difference fails. Field *size* lines matter: a type
   change that padding absorbs (e.g. `u64` → `u32` in a padded position) keeps
   size/offset identical and is only visible as a field-size difference.
2. **Surface gate** — every one of the 15 exported symbols is executed from
   C and from Rust on every fixture (open, probe, stream enumeration,
   select_stream, metadata, artwork, last_error, read, seek, close,
   abi_version via floor); all reported statuses, seek landing positions and
   first-frame counts must be identical.
3. **Correctness gate** — full decode to clean EOF on MP3 / FLAC / ALAC /
   ALAC-long with SHA-256 over the decoded Float32 PCM. The hash must equal
   the value in `native/experiments/songcore-equivalence/reference.json`
   (which was produced by the #114 C equivalence harness) for both callers.

## A/B protocol

- Same static `libsongcore.a` (FFmpeg closure merged by the native build) for
  both callers; the artifact SHA is recorded in the results.
- Balanced interleaving: every iteration runs BOTH callers, and the order
  alternates C→Rust, Rust→C, ... including warmup iterations.
- Primary metric: steady decode wall time — the `song_read_pcm` loop to EOF
  only, with a fresh open+probe per iteration outside the timed window.
  Primary block size 1024 frames; 256 and 4096 run as a sweep to confirm the
  tax does not scale strangely with call frequency.
- Per-iteration sample gates: frame count must equal the reference frame
  count and the terminal status must be clean `SONG_EOF`, otherwise the
  sample (and the run) is rejected. PCM SHA is verified per fixture and per
  caller by the correctness gate that brackets the measurement; hashing is
  deliberately kept out of the timed loop so hash implementation speed cannot
  masquerade as FFI tax.
- Secondary metrics: time-to-first-PCM (open+probe+first read, reported per
  component) and read-call latency percentiles (per-call timing collected in
  dedicated interleaved passes).
- Verdict rule: `INVESTIGATE` when Rust is slower by more than 5% regardless
  of spread (a real slowdown is never absorbed by a wide band);
  `NO_MEASURABLE_FFI_TAX` when |delta| is inside the combined inter-quartile
  noise band of both sides; otherwise `MEASURABLE_FFI_TAX = ±X%`. The overall
  verdict and the `all` exit code (2) account for every measured block size,
  not only the primary one.
- Host discipline: pinned CPU (`taskset -c 2`, matching the #112/#114
  baseline affinity), monotonic clocks, CPU/kernel/toolchain/artifact identity
  recorded in the results. A visibly throttling or loaded host invalidates
  the run — discard and re-measure.

One optional micro diagnostic measures the bare `songcore_abi_version()`
call floor on both sides (~1.3 ns/call each, dominated by loop overhead); it
is a diagnostic only and never represents decode-path tax.

## Known caller-side difference (not FFI)

The C caller hosts SongSource on `FILE*` stdio (`fread`/`fseek`/`ftell`); the
Rust caller hosts it on `std::fs::File` reads/seeks with identical semantics.
The steady-decode timed window does include host IO (the decoder pulls
compressed data through the callbacks), so this difference is technically
inside the primary window. It is bounded well below the noise bands in
practice: fixtures are 65–890 KB served from a warm page cache and glibc
`fread` bypasses its buffer for large requests, so the syscall/memcpy pattern
difference is single-digit microseconds against 2–12 ms decode windows
(≲0.5%). The observed steady deltas are also mixed-sign across codecs, which
is inconsistent with a systematic one-sided host advantage driving the
result. The most visible effect is in I/O-bound phases (the TTFP `song_open`
component is consistently a few percent faster on Rust). Any true FFI tax
estimate here is therefore conservative.

## Negative controls (run, then reverted; never committed)

- **NC1 timing sensitivity** — a temporary 30 µs sleep per Rust
  `song_read_pcm` call moved the reported delta from ≈0% to **+906%**
  (`INVESTIGATE`).
- **NC2 correctness sensitivity** — (a) a temporary `frames += got + 1` in
  the Rust steady loop was rejected by the per-iteration gate
  (`performance sample rejected ... frames=176573 (expected 176400)`);
  (b) a temporary hash byte-count corruption flipped the correctness gate to
  FAIL on the PCM SHA comparison.
- **NC3 ABI layout sensitivity** — a temporary `song_info.channel_mask`
  `u64`→`u32` edit in the sys crate failed the layout gate with
  `fieldsize song_info.channel_mask 8 vs 4`. This mutation is invisible to
  size/offset comparison because padding absorbs it, which is why the probes
  also compare per-field sizes.

## Observed result (2026-09-12, see results/)

Overall verdict: **NO_MEASURABLE_FFI_TAX**. All 12 steady comparisons
(4 fixtures × blocks 256/1024/4096) stayed inside their noise bands
(|delta| ≤ 1.6%, bands 1.5–6.0% at the primary block), with no codec-specific
anomaly and no block-size scaling of the delta. At the worst observed primary
delta the equivalent cost on the 224 s reference track is ≈2.4 ms — a value
that is itself inside the run's noise band and therefore not a resolvable
cost, which is why it is reported as no measurable tax rather than a
percentage.
