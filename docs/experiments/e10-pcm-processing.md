# E10 — PCM Processing Pipeline（P0：契约 + 测量仪器）

状态：`P0 COMPLETE`（全部 gate 在真实环境运行并通过；SRC/DSP 选择性
实验 A0/A1/B0 未启动。数字唯一 machine authority：
`bench/results/pcm-processing/p0-summary.json`（由 `tools/pcm_p0.py`
从 `p0-correctness.json` / `p0-buffer-accounting.json` /
`p0-placement.json` / `p0-performance.json` / `p0-negative-tests.json`
汇编，并携带 `p0-mutations.json` 与 sanitizer 结果）；本文件表格由
`tools/pcm_report_tables.py` 从 summary 派生，禁止手抄。）

```text
P0 selects no SRC.
P0 selects no DSP implementation.
P0 selects no libavfilter architecture.
P0 selects no SIMD strategy.
P0 makes no production RT-vs-worker placement decision.
```

## 1. 问题陈述（issue #12）

SongCore 已冻结为 decode-only（`source-rate / source-layout Float32
interleaved PCM` 边界，PRD §9 / songcore-boundary.md）。缺失层是：

```text
SongCore PCM
   ↓
PCM Processing（optional SRC，optional DSP chain）
   ↓
AudioSink
```

E09 的教训：一旦执行税消失，bridge/copy 会成为第一线成本。因此 E10
的第一个 PR 不选 SRC/DSP 实现，而是先把 **PCM 处理契约、buffer
所有权、transparent-bypass 正确性 gate、可机读测量仪器** 立起来——
先有可信的量尺，再用它选实现。

## 2. SongCore 边界

- SongCore 保持 decode-only；本实验**零改动**生产代码
  （`src/`、`include/`、`xmake.lua`、`package-manifest.json`、
  `corpus/` 均未触碰，由 driver 以 `git diff` 机器验证）。
- 实验代码全部位于 `bench/pcm/`（契约 + BYPASS + 仪器）与
  `tools/pcm_p0.py`（driver）。
- FFmpeg 类型不出现、也不允许出现在 PCM-processing 契约中
  （AGENTS.md 规则 3 同样约束未来该层的一切实现）。
- 上层（产品/UI）只应看到一个 `PcmPipeline` policy；rate-changing 与
  rate-preserving 语义在实现内部保持分离。

## 3. 两种 stage 语义（分开建模，不强行统一）

### Rate-changing stage（SRC 语义）

```text
prepare(in_rate, out_rate, channels, max_frames)
process(in_span, out_cap) -> consumed_frames, produced_frames
drain(out_cap) -> produced_frames
reset()
latency_frames()                       # 输出帧口径
required_input_for_output(out_frames)
```

要点：输入/输出帧数可以不同；存在内部输入缓冲（有界，prepare 期
分配）；`drain()` 在流尾清空内部状态；`required_input_for_output`
供上游调度/backpressure 决策。P0 的实现是 **BYPASS**（1:1、零缓冲、
零延迟），它是未来一切 SRC 实现必须 diff 的透明性参考。

### Rate-preserving stage（DSP 语义）

```text
prepare(sample_rate, channels, max_frames)
process_in_place(interleaved_pcm, frames)   # 帧数与采样率不变
reset()
latency_frames()
```

要点：in-place、帧数守恒。P0 提供 **OFF** stage（true no-op，不是
“乘 1 的 biquad”），证明管线组合 ≥1 个 DSP stage 时仍 bit 透明；
P0 的透明 gate 本体使用 0 个 DSP stage。

### 管线形态

```text
Input PCM → RateStage（BYPASS | 唯一 SRC） → FixedRateDSP 0..N → Output PCM
```

不允许把 SRC 塞进 in-place/frame-preserving 抽象（kickoff §0 的架构
纠正）。P0 不冻结任何 public production ABI——`bench/pcm/pcm_pipeline.h`
是实验契约，生产化时允许改名/改形。

## 4. Buffer ownership 模型

- **所有** stage 内部缓冲必须在 `prepare()` 分配、容量有界并进入
  计数（`internal_buffer_capacity_frames` / `peak_buffered_frames`）。
- 模拟 RT 路径（`prepare()` 之后的一切 process/drain/reset/queue
  操作）不允许任何分配；由计数分配器
  （`pcm_malloc/pcm_calloc` + armed 违规计数）确定性验证，**不从 RSS
  推断**。gate 具备 FAIL 能力：armed 区间故意 canary 分配必须被
  记为 violation（allocator selftest）。
- Shape B 的 processed-PCM 队列为固定容量 slab ring：存储 prepare 期
  一次性分配；worker `acquire(capacity_frames) → 填充 → commit(token,
  actual_frames)`，sink `pop → 拷出 → retire`；flush（seek/reset）把
  全部 storage 归还 free list。
- **队列所有权（P0-2）**：每个 acquire 返回 `{slot, generation,
  capacity_frames}` token；flush 递增 queue generation 并作废任何
  未完成 token——stale commit 返回类型化
  `PCM_ERR_STALE_TOKEN`，绝不入队。任意时刻
  `free_top + owned_queued_slots + valid_pending_slots ==
  capacity_slabs` 必须成立（机器断言）。`commit(actual_frames)` 支持
  部分块 / SRC 可变输出帧 / drain tail / 1-frame 病理块；实际帧数
  超 capacity 时拒绝（P1-3，不冻结为“每项必须整 slab”的 API）。
- zero-copy 是**被验证的事实而不是假设**：copy 模式统计
  `explicit_copy_calls/bytes`（精确 memcpy 点）；零拷贝转发以
  指针同一性断言（`out_ptr == in_span`），并计数
  `zero_copy_frames_forwarded`。BYPASS 管线在 copy 模式 = 恰好
  1.0 full memory pass；零拷贝转发模式 = 0.0。B-forward 的借指针
  生命周期必须覆盖队列驻留期（borrow lifetime > queue residency），
  仅作 BYPASS-only 合成实验，不构成 production 结论（P1-4）。

## 5. RT vs worker 执行形状（假设，不是结论）

```text
A（RT processing）:  predecoded PCM → pipeline → simulated sink callback
B（worker）:         predecoded PCM → pipeline worker
                     → 有界 processed-PCM slab 队列 → tiny callback
```

确定性调度常量：callback/slab = 256 frames，queue cap = 8 slabs，
worker high-water = 4 slabs，update 事件 @callback 500，reset 事件
@callback 1000，7 reps。度量：额外缓冲、额外拷贝/memory passes、
callback 工作量、pipeline 工作量、control-update 可见性延迟代理
（stale frames）、reset/flush 丢弃、underrun。

先验假设（待 A0/A1 与真实 AudioSink 证据检验，**P0 不下结论**）：

- A：零额外缓冲、最少 memory pass（1.0）、参数下一 callback 生效
  （stale=0）、reset 零丢弃；代价是全部 pipeline 工作量计入
  callback deadline（SRC/FIR 成本进入 RT 预算）。
- B：callback 近零（纯 sink 拷贝）；代价是 +1 队列缓冲、copy 模式
  memory pass 2.0、update 可见性 ≈ 队列缓冲 frames（worker 块量化
  ≤ slab_frames-1）、reset 丢弃队列内容。B 的 forward 变体
  （BYPASS 时 worker 以指针入队）实测 memory pass 回到 1.0，但该
  变体依赖“stage 无必须物化”这一 BYPASS 特权，未来 SRC/DSP 不一
  定享有。

## 6. Bypass 语义（透明 gate 的定义）

`RateStage = BYPASS, DSP = OFF` 时，管线边界输出必须：

- 与输入 **bit-identical**（copy 模式：逐 call `memcmp`；零拷贝模式：
  指针同一性）；
- 帧数守恒（consumed == produced == input）；
- 不改采样率、不改 channel 数、不加 gain、不 clip、不 rematrix；
- `[-1,1]` 之外的值（±3.5、±1e30、denormal、±0.0、NaN payload、
  ±Inf）**原样通过**——bit 级等价天然包含这一点，并单独断言
  out-of-range 样本计数；
- 接受 0-frame 调用（API 允许处）：no-op、计数器只动调用计数；
- **BYPASS 拒绝采样率不匹配**（P0-1）：`in_rate != out_rate`（以及
  `in_rate <= 0` / `out_rate <= 0` / `channels <= 0`）必须返回类型化
  错误——BYPASS 语义即 `in_rate == out_rate`，绝不静默 1:1 拷贝。
  correctness JSON 单独携带 `requested_in_rate` /
  `requested_out_rate`，gate 直接比较它们（不再是比较自身）。

权威是 **PCM pipeline boundary**，不是 device 输出；本实验不做任何
device 输出声明。

corpus：deterministic splitmix64 合成 PCM；mono/stereo ×
44.1/48/96 kHz × block pattern {tiny: 1/2/3/7…, ordinary:
256…2048, mixed（含每 3 次 1 次 0-frame 调用 + 1-frame 块）,
even-tail（ uneven 末尾 643 帧）} × {uniform random, out-of-range}
× {copy, zero-copy} = 96 cases。

## 7. 测量方法学

- **确定性**：splitmix64 PRNG、固定调度、无线程；跨 rep 的输出
  fnv1a64 必须全等（不等即 FAIL）。
- **分配**：计数分配器（管道/harness 自有内存），armed 区间违规
  计数；selftest 证明 gate 可 FAIL。不使用 RSS 推断。
- **拷贝**：只统计精确 memcpy 位置的显式拷贝；
  `full_memory_passes = explicit_bytes_copied / stream_bytes`。
- **计时**：`CLOCK_MONOTONIC`，每 pass 一次读钟（ns/call 在整条流
  上摊销）；1 warmup + 5 timed passes，median/min/max 全记录。
  volatile sink + 跨 TU memcpy 阻止编译器消除。单主机数据，不做
  跨主机比较；不追求微基准噪声以下的精度。throughput 单位
  MiB/s = bytes × 1e9 / ns / 1024²，并做派生指标 sanity
  （finite、>0、与 bytes/elapsed 自洽），杜绝“行存在即 PASS”。
- **sanitizer**：correctness + negative 在
  `-fsanitize=address,undefined -fno-omit-frame-pointer` 下重跑，
  结果成为机器证据（`p0-correctness-sanitized.json` /
  `p0-negative-tests-sanitized.json`）；performance/placement 保持
  非 sanitized。hostile-float 防消除 sink 消费 raw Float32 bits，
  不做 float→int 转换（NaN/Inf/1e30 场景是 UB）。
- **JSON 全部机器生成**；Markdown 表格由 generator 派生，
  `--check` 漂移即 FAIL。

## 8. P0 结果

<!-- BEGIN GENERATED TABLES -->
### P0 机器 gate

machine authority：`bench/results/pcm-processing/p0-summary.json`（由 `tools/pcm_p0.py` 从四个 section JSON 汇编；禁止手抄数字）

| gate | verdict |
|---|---|
| `bypass_pcm_bit_identical` | PASS |
| `no_implicit_clipping_gain_rematrix_rate_change` | PASS |
| `bypass_rate_mismatch_rejected` | PASS |
| `post_prepare_allocation_gate` | PASS |
| `buffer_bound` | PASS |
| `queue_ownership_conservation` | PASS |
| `queue_bound_enforced` | PASS |
| `stale_token_rejected` | PASS |
| `reset_lifecycle_same_instance` | PASS |
| `reprepare_rate_transition_semantics` | PASS |
| `zero_copy_guard_with_dsp_stage` | PASS |
| `copy_memory_pass_accounting_present` | PASS |
| `rt_vs_worker_placement_evidence_present` | PASS |
| `bypass_overhead_measured` | PASS |
| `sanitizer_clean` | PASS |
| `mutation_tests_caught` | PASS |

总体 verdict：**PASS**。`report --check` 由 `tools/pcm_report_tables.py --check` 单独执行（doc 与 authority JSON 漂移即 FAIL）。

### P0 正确性汇总

machine authority：`p0-correctness.json`（经 summary 转录）

| 项 | 值 |
|---|---|
| corpus cases（4 patterns × 2ch × 3 rates × 2 corpus × 2 modes） | 96 |
| bit/alias identical | 96 / 96 |
| 逐 call memcmp 总字节 | 31,277,304 |
| 同一 pipeline 实例 reset→process 5 cycles 输出 bit-identical | True （same instance = True） |
| state_epoch 逐 reset 严格递增 | True （epoch 序列 [1, 2, 3, 4, 5]） |
| lifecycle armed 区间分配 = 0 | 0 （True） |
| 跨 cycles 无 stale buffered frames | 0 |
| reprepare 44.1k→48k 后输出 bit-identical | True |
| reconfigure 丢弃 buffered frames | 0 |
| zero-copy guard（+1 DSP stage 必须走 copy 路径） | pass |
| partial consume（out_cap<in_frames 可分段重组） | pass |

### P0 negative / mutation / sanitizer

machine authority：`p0-negative-tests.json` / `p0-mutations.json` （经 summary 转录；sanitizer 结果来自 `p0-correctness-sanitized.json` / `p0-negative-tests-sanitized.json`）

| check | 断言 | verdict |
|---|---|---|
| `bypass_rejects_rate_mismatch_44100_48000` | BYPASS 拒绝 44100→48000 | pass |
| `bypass_rejects_rate_mismatch_48000_44100` | BYPASS 拒绝 48000→44100 | pass |
| `bypass_rejects_zero_in_rate` | BYPASS 拒绝 0 采样率 | pass |
| `bypass_rejects_negative_in_rate` | BYPASS 拒绝负采样率 | pass |
| `stale_commit_after_flush_rejected` | flush 后 stale commit 被拒（PCM_ERR_STALE_TOKEN） | pass |
| `queue_ownership_conservation` | free+owned+pending == capacity 全程成立 | pass |
| `queue_bound_enforced` | count ≤ capacity；满时 acquire 背压 | pass |
| `variable_frames_commit` | commit(actual) 保留精确帧数（1/partial tail/超容量拒绝） | pass |
| `zero_copy_guard_with_dsp_stage` | 带 DSP stage 时必须走 copy 路径 | pass |
| `state_epoch_bump_on_prepare_and_reset` | prepare/reset 严格递增 state_epoch | pass |
| `bypass_memcpy_present` | copy 模式每次 process 恰好 1 次 memcpy | pass |

negative 合计：11 / 11 pass（0 fail）。

mutation tests：6 个确定性故障注入，全部被对应 check 捕获 = **True**：`allow_bypass_rate_mismatch` → QN_MUT_RATE_MISMATCH_ACCEPT → `bypass_rejects_rate_mismatch_44100_48000`/`bypass_rejects_rate_mismatch_48000_44100`；`remove_bypass_memcpy` → QN_MUT_REMOVE_BYPASS_MEMCPY → `bypass_memcpy_present`；`weaken_zero_copy_guard` → QN_MUT_WEAK_ZEROCOPY_GUARD → `zero_copy_guard_with_dsp_stage`；`remove_state_epoch_bump` → QN_MUT_NO_EPOCH_BUMP → `state_epoch_bump_on_prepare_and_reset`；`allow_stale_queue_commit` → QN_MUT_STALE_COMMIT_ALLOWED → `stale_commit_after_flush_rejected`/`queue_ownership_conservation`；`break_queue_bound` → QN_MUT_QUEUE_BOUND_BREAK → `queue_bound_enforced`。

sanitizer（ASan+UBSan，correctness + negative）：PASS（correctness=PASS，negative=PASS）——hostile-float sink 已改 raw-bits 消费，sanitizer 成为机器证据。

### Buffer / copy accounting（BYPASS，1M frames stereo @48k，256fr blocks）

machine authority：`p0-buffer-accounting.json`（经 summary 转录）

| 模式 | input frames | output frames | process calls | explicit copies | copied bytes | forwarded frames | logical copy bytes | est. mem traffic bytes | full memory passes | peak buffered | internal capacity |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| copy | 1,048,576 | 1,048,576 | 4,096 | 4,096 | 8,388,608 | 0 | 8,388,608 | 16,777,216 | 1.000 | 0 | 0 |
| zero_copy | 1,048,576 | 1,048,576 | 4,096 | 0 | 0 | 1,048,576 | 0 | 0 | 0.000 | 0 | 0 |

allocation gate：counting allocator (deterministic; not RSS-derived)；armed RT 区间分配 = **0**，violation = **0**（selftest 证明该 gate 具备 FAIL 能力）。queue slab 全部在 prepare 期分配（11 次）。

buffer bound：`logical_copy_bytes` 是精确 memcpy 字节；`estimated_memory_traffic_bytes` 按一次拷贝含读+写估算（= logical × 2），**不是硬件计数器 truth**。queue 快照：count=0，free_top=8，owned=0，pending=0，capacity=8；ownership conservation = **pass**（free+owned+pending==capacity）。

### Placement model：Shape A（RT）vs Shape B（worker）

machine authority：`p0-placement.json`（经 summary 转录；确定性单线程调度模型，非真实 RT 线程）

| shape | mode | full memory passes | 峰值缓冲 (slabs×frames) | update 时 stale frames | reset 丢弃 frames | underrun |
|---|---|---:|---:|---:|---:|---:|
| A_rt | copy | 1.000 | 0×256 (cap 8×256) | 0 | 0 | 0 |
| B_worker | copy | 2.002 | 4×256 (cap 8×256) | 1,024 | 1,024 | 0 |
| B_worker | forward | 1.000 | 4×256 (cap 8×256) | 1,024 | 1,024 | 0 |

reps = 7，全部 shape 的 device-stream fnv1a64 逐 rep 相等且跨 shape 相等（bypass 恒等）。Control-update 可见性：A = 下一 callback 边界（stale 0）；B = 队列中已缓冲 frames + worker 块量化（≤ slab_frames-1）。**不做 production placement 决策。**

callback / pipeline / sink-copy 工作量的 ns 分布（7 reps，median/min/max）属逐 run 易变数据，只记录于 `p0-placement.json` authority，不进 doc；本表仅含跨 run 确定的结构量。

### BYPASS 开销 block matrix

machine authority：`p0-performance.json`（经 summary 转录）

| block frames | ch | copy ns/call (median) | memcpy ref ns/call | overhead ns/call | overhead ratio | zero-copy ns/call |
|---:|---:|---:|---:|---:|---:|---:|
| 64 | 1 | 13.2 | 3.3 | 9.9 | 3.00 | 6.3 |
| 128 | 1 | 17.9 | 7.2 | 10.8 | 1.50 | 6.5 |
| 256 | 1 | 24.9 | 14.3 | 10.6 | 0.74 | 6.3 |
| 512 | 1 | 47.7 | 32.0 | 15.7 | 0.49 | 6.5 |
| 1024 | 1 | 76.8 | 57.3 | 19.5 | 0.34 | 6.6 |
| 2048 | 1 | 145.1 | 126.7 | 18.4 | 0.14 | 6.0 |
| 64 | 2 | 18.7 | 9.7 | 9.0 | 0.94 | 7.8 |
| 128 | 2 | 35.2 | 18.6 | 16.6 | 0.89 | 7.0 |
| 256 | 2 | 109.0 | 39.7 | 69.2 | 1.74 | 7.5 |
| 512 | 2 | 165.3 | 95.8 | 69.6 | 0.73 | 8.3 |
| 1024 | 2 | 221.4 | 187.9 | 33.5 | 0.18 | 7.3 |
| 2048 | 2 | 375.1 | 416.2 | -41.0 | -0.10 | 5.7 |

timed passes = 5 + 1 warmup，每个 timed sample 内重复整条流 32 次（摊销钟/调度噪声；分布见 p0-performance.json）；零拷贝转发路径即管线抽象地板（~8 ns/call @ 256fr stereo）。小 block 行的 overhead 受残余噪声支配，解读以量级为准。

### E10-A0 Windows AudioSink（原生 WASAPI，本主机）

machine authority：`a0-summary.json`（经 `tools/pcm_a0_windows.py` 汇编；表格禁止手抄）

device evidence status：**COLLECTED**（单主机；端点数 1）

| endpoint | app BYPASS（shared 原生率） | Windows SRC | exclusive/source-rate device |
|---|---|---|---|
| `e5f79a1d…` | YES (48000 kHz) | YES | NO |

| 端点 | mix format | engine period（default/min, 100ns） |
|---|---|---|
| `e5f79a1d…` | 48000 Hz / 2ch / f32 (float) | 100000 / 30000 |

reopen/reconfigure（44.1k→48k→44.1k，每 rate 30 cycles，QPC）：

| rate | total cycle median ms | initialize median ms | min ms | max ms |
|---|---:|---:|---:|---:|
| 44100 | 42.45 | 39.56 | 41.20 | 46.99 |
| 48000 | 42.37 | 39.53 | 40.87 | 45.49 |
| 44100 | 43.31 | 40.35 | 41.14 | 51.48 |

limitations：single Windows host; one active render endpoint (Realtek)；exclusive-mode IsFormatSupported/Initialize returned AUDCLNT_E_UNSUPPORTED_FORMAT for every rate on this device；no audible signals emitted; silence only；format acceptance is not absence of driver/device DSP。

machine authority：`a0-windows-endpoints.json` / `a0-format-support.json` / `a0-reopen.json` / `a0-summary.json`（`tools/pcm_a0_windows.py` 汇编；`--check` 漂移即 FAIL）。

### E10-A1 SRC shootout（BYPASS/swr/soxr/r8b/lsr）

machine authority：`a1-summary.json`（经 `tools/pcm_a1.py` 汇编；quality/perf/shipping 数字禁止手抄）

| candidate | THD+N 1k (min..max, dB) | alias rej (downsample, dB) | imaging (upsample, dB) | DC gain (min) | near-nyq passband (dB) |
|---|---|---:|---:|---:|---:|
| swr | -141.9..-106.6 | -32.7 | -106.8 | 0.999992 | -2.16..-0.02 |
| soxr | -136.7..-133.8 | -161.4 | -120.0 | 1.000000 | -0.02..-0.0 |
| r8b | -154.9..-150.5 | -198.2 | -120.0 | 1.000000 | 0.0..0.0 |
| lsr | -154.9..-145.8 | -172.5 | -120.0 | 1.000000 | -0.0..-0.0 |

| candidate | ns/input frame (real 44.1→48) | ns/input frame (real 96→44.1) | xRT (min across streams) | post-prepare alloc calls |
|---|---:|---:|---:|---:|
| swr | 21.13 | 22.09 | 484.3 | 0 |
| soxr | 17.18 | 10.91 | 925.0 | 43 |
| r8b | 47.34 | 28.66 | 363.4 | 0 |
| lsr | 744.19 | 761.88 | 14.0 | 0 |

| candidate | runner raw bytes | stripped | xz -9 |
|---|---:|---:|---:|
| bypass (baseline) | 21808 | - | - |
| swr | 398040 | 370928 | 126892 |
| soxr | 354504 | 339128 | 121504 |
| r8b | 150248 | 125224 | 50220 |
| lsr | 1516920 | 1510024 | 938140 |

### E10-B0 thin DSP 参考（Gain/Biquad/EQ10/Limiter）

machine authority：`b0-summary.json`（经 `tools/pcm_b0.py` 汇编）

| chain | nodes | logical PCM passes / block | explicit copies | buffered frames | post-prepare allocs |
|---|---:|---:|---:|---:|---:|
| gain_only | 1 | 1 | 0 | 0 | 0 |
| one_biquad | 1 | 1 | 0 | 0 | 0 |
| eq10 | 1 | 10 | 0 | 0 | 0 |
| gain_eq10 | 2 | 11 | 0 | 0 | 0 |
| gain_eq10_limiter | 3 | 12 | 0 | 0 | 0 |

| response check | measured | analytical | max error (20..20k) |
|---|---|---:|---:|
| biquad 1k +6dB | 5.998 dB | 5.997 dB | 0.122 dB |
| EQ10 全带 +6dB | - | - | 1.39 dB |

correctness verdict：**PASS**（gain 0dB bit-identical / -6dB analytical / fusion equivalent；biquad 稳定 + reset 清状态；limiter 无过冲 clamp + latency 0；NaN 策略 active-sanitize，TRUE OFF 位透明）

machine authority：`b0-correctness.json` / `b0-memory.json` / `b0-dsp-response.json` / `b0-summary.json`

<!-- END GENERATED TABLES -->

读数要点（数字一律以上方生成表为准，不在此手抄）：

- 全部 96 个 corpus cases bit/alias identical；同一 pipeline 实例
  reset→process 5 cycles 输出 bit-identical（epoch 严格递增
  [1,2,3,4,5]，armed 区间 0 分配）；reprepare 44.1k→48k 输出
  bit-identical；state_epoch 证明 stale 状态不跨 reset/reprepare
  存活。
- BYPASS copy 管线 = 恰好 1.0 full memory pass；零拷贝转发 = 0.0；
  post-prepare RT 区间 0 分配（armed 区间 canary selftest 证明该
  gate 具备 FAIL 能力）。`logical_copy_bytes` 精确计 memcpy 字节，
  `estimated_memory_traffic_bytes` = ×2 估算（读+写），非硬件
  计数器 truth。
- **P0-1 关闭**：BYPASS 类型化拒绝 44100→48000 / 48000→44100 / 0 /
  负采样率（4 个 negative checks pass）；tautological gate 已删除，
  gate 直接比较 `requested_in_rate == requested_out_rate > 0`。
- **P0-2 关闭**：flush 后 stale commit 返回 `PCM_ERR_STALE_TOKEN`
  且不入队；`free+owned+pending == capacity` 守恒全程成立
  （11 个 negative checks 全 pass）。
- **P0-4 关闭**：hostile-float sink 改 raw-bits 消费；ASan+UBSan
  下 correctness + negative 全 PASS，成为机器证据。
- Placement：A = 单次 memory pass、零额外缓冲、参数下一 callback
  边界生效、reset 零丢弃，代价是全部 pipeline 工作量位于 callback
  deadline 内；B(copy) = 多出一整次 memory pass（含 reset 后按
  seek 语义重读的帧）+ 高水位缓冲 + update stale ≈ 高水位帧数 +
  reset 丢弃队列内容；B(forward) 把 pass 数降回 A 水平，但这是
  “BYPASS 无必须物化”的特权，未来 SRC/DSP 不一定适用。underrun
  在两种 shape 的确定性调度下均为 0。
- **反直觉读数（对 “worker 更便宜” 的证伪尝试结果）**：B(forward)
  的单次 callback 工作量反而高于 B(copy)——零拷贝 handoff 使
  callback 直接从冷的大 decode buffer 流式读取，而 copy 模式的
  callback 读的是 cache-hot 的复用 slab。memory pass 更少 ≠
  callback 更便宜；placement 决策必须同时看两者（这也是 P0 不做
  placement 决策的理由之一）。
- BYPASS 抽象本身接近免费：copy 模式相对裸 memcpy 的每次调用开销
  与零拷贝转发路径（调用+计数开销）在同一量级；block 64→2048 无
  异常尺度（小 block 行的 overhead 受残余计时噪声支配，解读以
  量级为准）。throughput 单位修正后（MiB/s = bytes×1e9/ns/2^20）
  不再是 0.0，并全部通过派生指标 sanity。

## E10-A0 读数要点（Windows-first AudioSink）

机器证据见上方生成表（`a0-summary.json`）。三条路径在本主机
（Realtek 默认渲染端点）的实测分类：

- **A（app BYPASS）**：shared 模式下只有 mix rate（48000 Hz）是
  `IsFormatSupported = S_OK`；44.1k/96k/88.2k/176.4k/192k 全部
  `S_FALSE`（closest = 48000），即引擎需要转换。48k Float32 歌曲
  可以无 app SRC 直接供给；44.1k 歌曲在 shared 模式下必须有人做
  SRC（app 或 Windows）。
- **B（Windows-owned SRC）**：`AUTOCONVERTPCM | SRC_DEFAULT_QUALITY`
  在 44.1k 上 `Initialize = S_OK`——Windows Audio Engine 会做转换。
  这是 sink-owned 的免费竞争者，质量/成本需 A1 对照。
- **C（source-rate device format）**：本设备全部采样率的 exclusive
  `IsFormatSupported = AUDCLNT_E_UNSUPPORTED_FORMAT`、exclusive
  `Initialize` 同样失败——**该设备不存在 true source-rate 路径**。
  若未来有支持 exclusive 的设备，format 接受 ≠ bit-perfect（驱动
  DSP 仍可能介入），A0 不声称 bit transparency。
- **reopen cost**：44.1k→48k→44.1k 每次 ~42-43 ms（median），
  其中 `Initialize` 占 ~40 ms，`Activate/GetMixFormat/Start/Stop`
  合计 ~2 ms。一次采样率切换的固定成本非零但有限——“常驻 app SRC”
  与“按需 reopen”之争需要 A1 给出 SRC 的持续 CPU 成本后才有数字
  结论（本机不决策）。

局限：单 Windows 主机、单活动渲染端点；silence-only，无 audible
signal；不推广到其他设备/驱动。多设备矩阵留给 reviewer 或后续
`a0` 扩展。

## E10-A1 读数要点（SRC shootout）

机器证据见上方生成表（`a1-summary.json`）。方法：确定性信号、
impulse 测 delay、sine-fit（精确减基波）测 THD+N 与各单音增益、
Hann 周期图测 imaging；**频率以 Hz 保存**（重采样只改每周期样本
数），所有单音指标按保存频率测量；downsample 的 alias 音放在目标
Nyquist 之上 8% 处，量折返点。

- **swr（FFmpeg n9.0.1，默认配置）质量最弱**：THD+N 约 -107 dB
  （44.1 系转换，其余候选 -134 dB 以下）；downsample 近 Nyquist
  alias 抑制仅 **-33 dB**（25920 Hz→96k 转 48k，其默认
  filter_size=32 的过渡带），远处频率才到 -106 dB；DC 增益
  0.99999（-0.0001）；48→44.1 近 Nyquist 通带有 -2.2 dB 衰减。
  这是默认配置的实测——swr 可调（filter_size/cutoff），调优后
  是否追上 soxr/r8b/lsr 属后续项。**swr 已在 Qianqian FFmpeg
  closure 内**，增量 shipping 只算可达符号，远小于独立 runner。
- **soxr（HQ）**：质量 -134 dB 级、alias -161 dB；**最快**
  （10.9-17.2 ns/frame，≥925×RT）；唯一 post-prepare 有分配者
  （43 次，违反 RT 零分配），若走 RT 路径需预留或换配置。
- **r8b（线性相位）**：质量最好（THD+N -151 dB、alias -198 dB）、
  **shipping 最小**（stripped 125 KB / xz 50 KB）、0 分配；但内部
  double 精度要求 f32↔double 双转换胶水（28-47 ns/frame，含胶水），
  且**没有干净的 EOF/drain 语义**（需按理想输出帧数喂零收尾，
  getLatency() 恒 0）。
- **lsr（BEST）**：质量好（-146..-155 dB、alias -173 dB）、0 分配、
  drain 语义干净；但 **744 ns/frame（14×RT）慢 30-70 倍**、**shipping
  巨大**（xz 938 KB，best-quality sinc 系数表）。
- **duration/latency**：全部候选输出长度误差 ≤1 帧；soxr 首个输出
  在 768 输入帧后、r8b 在 1536 帧后（lookahead），swr/lsr 立即输出；
  drain tail = 各自滤波器延迟（swr 17 / soxr 504 / r8b 1795 / lsr 0）。
- **BYPASS 同率参考**：block 1..4096 全部 bit-identical，延迟 0。
- **Pareto 初读（不冻结决策）**：质量/速度 sweet spot 是 soxr
  （但违反 RT 零分配）；质量/shipping 最优是 r8b（但需处理
  drain 语义与 double 胶水）；“零增量成本”是 swr（但默认质量最弱，
  近 Nyquist alias 是 96k→48k 的真实风险）；lsr 除质量外无优势。
  单一候选对四个平台都不显然——生产决策留给 reviewer，
  本实验只交付证据。


## E10-B0 读数要点（thin DSP 参考）

机器证据见上方生成表（`b0-summary.json`）。要点：

- **内存 pass 研究问题（§39 的答案）**：节点抽象确实造成“每个逻辑
  filter 一遍完整 PCM pass”——Gain=1、单 biquad=1、**10-band EQ=10**、
  Gain+EQ10=11、Gain+EQ10+Limiter=12。这是标量参考的事实，不做
  fusion 优化（留给 B2）；成本先可见。
- **Biquad 权威**：RBJ peaking、DF2T；浮点实现 vs 解析式在 20..20k
  最大误差 0.12 dB（1k +6dB 实测 5.998 vs 解析 5.997）；EQ10 级联
  全带 +6dB 误差 1.39 dB（带边缘累积）。
- **Limiter**：瞬时起音 peak-hold + 慢释音，无过冲 clamp、latency 0、
  reset 清包络；**显式启用**，不在默认路径（默认 = P0 TRUE OFF）。
- **NaN/Inf 策略**：DSP ACTIVE 时 sanitize（非有限样本→0，事件计数，
  IIR 状态不被毒化）；**DSP OFF = P0 位透明 bypass**（NaN 原样通过，
  P0 已证）。
- **分配**：所有链 post-prepare 0 分配（--wrap 计数）。

## 9. 遗留问题（交给 E10-A0 / A1 / B0）

1. **A0（Windows-first AudioSink）**：真实设备在多大比例上能直接
   吃 source rate？`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM`（平台
   SRC）作为 sink-owned 竞争者的实测成本？reopen/reconfigure vs
   常驻 SRC 的切换成本？——决定 BYPASS 之外的 SRC 是否真的必要。
2. **A1（SRC shootout）**：swr / SoXR / r8brain / libsamplerate 在
   本契约下的 filter-delay 补偿后质量、`drain()` 语义、内部缓冲、
   first-output latency；`required_input_for_output` 的真实调度
   价值；SRC 是否破坏 B-forward 变体的零拷贝（预期：破坏）。
3. **B0（thin DSP）**：`process_in_place` 契约下 biquad cascade 的
   状态布局与 reset 语义；参数平滑是否需要 latency_frames 非 0；
   NaN/Inf 策略（P0 证明 BYPASS 原样传递 NaN，真实 DSP 必须显式
   定义 NaN policy——这是一个未决的契约决定）。
4. **契约未冻结项**：错误处理通道（当前返回码）、`latency_frames`
   的组合规则、多 stage 间 buffer 所有权转移、以及 SRC 与 DSP 的
   排序分解（kickoff §5）全部留待证据；production ABI 不提前
   固化。
5. **placement 的真实线程复验**：P0 是确定性单线程模型；真实
   worker 线程 + 无锁队列 + 真 RT callback 的复制实验在有了真实
   AudioSink（A0）之后再做才有意义。

## 10. 复现命令与 provenance

```bash
# 一键：编译 + 运行 + gates + 写 authority JSON + sanitizer +
# 6 个 mutation 故障注入验证（含 provenance）
python3 tools/pcm_p0.py

# doc 表格再生 / 漂移检查（机器权威 = p0-summary.json）
python3 tools/pcm_report_tables.py            # regenerate
python3 tools/pcm_report_tables.py --check    # drift = exit 1

# summary 与 section JSON 一致性检查（不重跑 bench）
python3 tools/pcm_p0.py --check
```

- 编译：`cc -std=c11 -O2 -Wall -Wextra`；sanitizer 构建
  `-fsanitize=address,undefined -fno-omit-frame-pointer -O1`
  （compiler/CPU/OS/git SHA/dirty 状态记录在
  `p0-summary.json.provenance`）。
- mutation：`build/pcm-p0/mutations/<name>/` 下按 `QN_MUT_*` 宏构建，
  每个 mutation 必须被其对应 negative check 捕获；未捕获即顶层 gate
  FAIL（`p0-mutations.json`）。
- 运行环境：单主机（AMD Ryzen 7 5800H, WSL2）；计时数据为该主机
  分布，不跨主机比较。
- 追踪的 authority 文件：`bench/results/pcm-processing/*.json`
  （p0-{correctness,buffer-accounting,placement,performance,
  negative-tests,mutations,summary}.json +
  p0-{correctness,negative-tests}-sanitized.json，全部小体量机器
  JSON；无大型 artifact 入库）。

## 11. Adversarial review 记录（自我证伪）

- ~~“zero copy”~~：只对 pipeline 边界 + BYPASS 成立（指针同一性断言
  + 计数为 0 双证据）；copy 模式明确记 1.0 pass。B-forward 的 1.0
  pass 依赖“stage 无必须物化”，已被标注为 BYPASS 特权而非通用结论；
  且借指针生命周期必须覆盖队列驻留期（borrow > queue residency），
  当前仅作合成实验（P1-4）。
- ~~“zero allocation”~~：gate 只覆盖计数分配器可见的 pipeline/harness
  自有分配；不覆盖未来引入的第三方库内部分配（进入 A1 时需把候选
  SRC 的 allocator 挂进同一计数器）。
- ~~“bit-identical bypass”~~：copy 模式逐 call memcmp；零拷贝模式
  指针同一性。开发期 harness 自身曾出现两处会造假的 bug（armed 区间
  计数含 prepare 分配；Shape A 输出指针断言写反）——被 gate/corpus
  抓出并修复，说明仪器有效；两个断言现在均为双向（必须相等且必须
  指向正确存储）。
- ~~“bounded buffering”~~：BYPASS 内部缓冲恒 0；B 队列峰值 slabs
  由调度决定且 ≤ capacity，两者都进 JSON；ownership conservation
  与 queue bound 现在被机器强制（queue_peak ≤ cap、free_top ≤ cap、
  free+owned+pending == capacity），并有 `break_queue_bound` mutation
  证明该 gate 会 FAIL。真实 SRC 的缓冲界必须在 A1 以同一字段复验。
- ~~“deterministic reset”~~：同一 pipeline 实例 5 cycles 输出 fnv
  全等 + epoch 严格递增 + armed 区间 0 分配；但 P0 的 BYPASS 没有
  真实滤波状态，“reset 清状态”的实质考验在 B0/A1。
- **mutation tests（可复现）**：6 个确定性故障注入以 `QN_MUT_*`
  宏编译进 pipeline.c，driver 自动断言其对应 negative check 转
  FAIL——(1) 允许 BYPASS rate mismatch；(2) 去掉 BYPASS memcpy；
  (3) 弱化 zero-copy guard；(4) 去掉 state_epoch bump；(5) 允许
  stale queue commit；(6) 打破 queue bound。全部被捕获
  （`p0-mutations.json`）。开发期另有一个被 ASan 当场抓出的 harness
  自身 bug（negative 测试里 copy-mode buffer 尺寸写错导致 memcpy
  OOB），修复后 sanitizer 全 PASS——sanitizer 不是摆设。
- ~~“worker 更便宜/更安全” 与 “RT 更便宜/更低延迟”~~：P0 只记录
  结构差异（passes、缓冲、stale、reset 丢弃、callback 工作量分布）。
  callback 工作量 B < A、memory pass A ≤ B 是本模型下的事实，但
  deadline 安全、功耗、真实调度抖动未测——**不做 production
  placement 决策**。
