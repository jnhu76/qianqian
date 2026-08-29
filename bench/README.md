# Benchmark Harness

v0 已实现（Phase 0 Step 1，Native）。统一记录：

```text
backend
git SHA
FFmpeg version
build profile hash
platform
CPU
OS
corpus id
```

## 运行

```text
scripts/bench-native            # fetch + build 全部 profile + 跑 corpus
python3 bench/harness/run_bench.py --out bench/results/runs/<id>
```

## v0 保证的四类结果

1. **correctness**：`open → probe → metadata → artwork → sequential decode
   → seek(25/50/75%) → decode again → EOF`。失败必须带类型
   （open/probe/decode/seek/timeout/crash），不允许裸 PASS/FAIL。
2. **PCM correctness**：lossless FLAC 对照 manifest 中的 canonical
   Float32-interleaved sha256（spec-forced，严格相等）；全部 profile 之间
   同一 fixture 的 PCM 必须一致（MP3 用一致性作为 gate）；`n3-min` 与
   `n3-min-noswr` 必须逐字节一致（swresample bypass 验证）。
3. **artifact size**：static libs（逐库 bytes + defined symbols）、
   linked bench、stripped、xz compressed。不用 build directory size 冒充。
4. **decode throughput**：warm-up 1 次 + N 轮，报告 median/min/max 与
   x realtime；分 `decode-core`（demux+decode）与 `songcore-output`
   （+ Float32 interleave）两条路径。另记录 cold open 与 peak RSS。

结果格式与 tracked/untracked 边界见 `bench/results/README.md`。

M0 阶段不允许只贴单次漂亮数字：throughput 至少 warm-up + 多轮，
报告分布与中位数。
