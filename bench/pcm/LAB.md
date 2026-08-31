# E10 SRC/DSP Lab (exp/e10-src-dsp-lab)

Stacked on the repaired P0 branch (`exp/e10-pcm-pipeline-p0`, parent
`4e2de7dd048e5920c1ce0d2af630c37d2e229da6`). This branch carries the
selection experiments A0 / A1 / B0 / B1 only — **no production code**.

```text
A0  Windows-first AudioSink evidence (WASAPI): when can Qianqian bypass
    SRC, when does Windows SRC, when is true source-rate device format
    available; reopen/reconfigure cost.
A1  SRC shootout: BYPASS ref, FFmpeg libswresample, SoXR, r8brain,
    libsamplerate — quality/latency/drain/allocation/CPU/shipping under
    the P0 RateStage contract.
B0  Thin DSP reference: Gain / Biquad / 10-band EQ / Limiter under the
    P0 FixedRateDSP (in-place) contract; NaN policy, memory passes.
B1  Thin DSP vs trimmed libavfilter (volume/equalizer/alimiter),
    capability-equivalent comparison.
```

Every claim must be backed by machine JSON under
`bench/results/pcm-processing/` (see `docs/experiments/e10-pcm-processing.md`).
No SIMD (B2) before the reviewer sees B0/B1.
