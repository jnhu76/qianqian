# Benchmark Results

Layout:

```text
bench/results/
  README.md        this file
  schema.json      field contract for the four result documents
  baseline/        TRACKED canonical run for this branch (small JSON only)
  runs/            UNTRACKED local runs (bench/results/runs is gitignored)
```

## Tracked vs generated

| Path | Status |
|---|---|
| `bench/results/schema.json` | tracked |
| `bench/results/baseline/` | tracked — canonical baseline, updated deliberately when a profile or corpus changes |
| `bench/results/runs/**` | untracked — every local run |

`manifest.json` records the full environment (FFmpeg tag/SHA, configure-args
hash per profile, compiler, corpus id, qianqian git SHA). A baseline without
those fields is not acceptable.

## Files per run

- `manifest.json` — environment + metadata, incl. `swr_bypass_pcm_identical`
- `correctness.json` — per profile × corpus case: status (`pass`,
  `degraded_pass`, `fail`) + per-check evidence and typed failure causes
- `size.json` — per profile: static libs (per-lib bytes + defined symbols),
  linked bench binary, stripped, xz-compressed
- `throughput.json` — per profile × throughput case: warm-up + N iterations,
  median/min/max ms, x realtime for `decode_core` and `songcore_output`
- `summary.json` — ladder table, cross-profile PCM consistency matrix,
  swr bypass verdict
- `components/` — copies of configure-args/build-meta/enabled-components per profile

The harness regenerates everything from `corpus/` + `build/`; nothing in a
run directory is hand-edited.
