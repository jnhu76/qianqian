# bench/

Benchmark and evidence home for the Native Audio Core.

## Layout

```text
bench/
  results/          tracked durable evidence (see bench/results/README.md)
  native/qn_bench.c benchmark binary used by tools/verify_xmake_core.py
                    (Xmake replay vs upstream-oracle equivalence)
```

## qn_bench.c

The benchmark binary behind the replay-fidelity gate. It records
open/probe/decode/seek correctness, PCM (canonical Float32-interleaved
sha256), artifact sizes, and decode throughput (median/min/max, ×realtime,
decode-core vs songcore-output). `tools/verify_xmake_core.py` links it two
ways (upstream archives vs the Xmake-replayed `libqianqian_av.a`) and
asserts byte-identical PCM.

The permanent SongCore regression lives in `native/tests/songcore/`
(`regression.py` + `songcore_probe`), which is the authority tree that
matters; `qn_bench.c` here exists to prove the Xmake replay is faithful to
the oracle.

## Results

Tracked vs regenerated evidence boundaries: see `bench/results/README.md`.
Local throwaway runs go to `bench/results/runs/` (untracked).
