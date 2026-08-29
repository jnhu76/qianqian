# Benchmark Harness

统一记录：

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

核心指标：

- cold open latency
- probe latency
- decode throughput (x realtime)
- seek p50/p95
- RSS
- peak memory
- linked binary size
- compressed artifact size

结果写入 `bench/results/`。

M0 阶段不允许只贴单次漂亮数字。
至少运行多次并报告分布/中位数。
