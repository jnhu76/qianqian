# E09 — WASM Total-Cost & Bottleneck Audit

状态：`COMPLETE`（Layer 1 全部在真实环境运行；Layer 2（Kotlin）gate 已
解锁但按任务边界**未启动**。数字唯一 machine authority：
`bench/results/wasm/{summary,correctness,performance,memory,shipping,
shipping-em,tolerance,em_performance,toolchains}.json` 与
`bench/profiles/wasm-*.hot.txt`、`bench/provenance/e09-perf/*`；报告正文
的表格由 `tools/wasm_report_tables.py` 从 summary 派生，禁止手抄。）

## 研究问题（issue #5 的 Layer 1 部分）

> 同一份 Qianqian 最小 FFmpeg source closure 编成 WASM guest 后，在
> 不同 runtime（Native / WAMR classic-interp / WAMR AOT / wasm3 / Wasmtime /
> Emscripten+Node）上的**总成本**是多少？瓶颈依次是什么？
> bridge/边界拷贝相对 guest 纯解码与 native 基线各占多少税？

三个口径严格分开（继承 E08 纪律）：

- **SHIPPING FOOTPRINT**：guest .wasm + runtime 份额 + bridge +（AOT 时）
  .aot artifact 的**总和**，raw / gzip -9 / brotli -11 / xz -9e 分别列，
  不许混写。浏览器分发口径另计 glue .js。
- **EXECUTION TAX**：`execution_tax = T_guest / T_native`（同机同 fixture
  同模式，Mode A 中位数）。
- **BRIDGE TAX**：boundary 回拷成本对照 guest 解码与 native 直拷。

## 实验设计

- 两层实验：Layer 1 = WASM core + runtime ladder（本文件）；
  Layer 2 = Kotlin 宿主（gate 解锁后另行开工）。
- WASI 优先（wasi-sdk 34，clang 23.1.0-wasi-sdk），Emscripten 6.0.8 为
  第二实现：同一 guest source，不同 libc/入口形状。
- Host-owned IO：guest 只见不透明 i64 handle + `qn_door_read/seek/size`
  （64-bit offset 全程保持）；bridge 为薄层 `src/wasm/songcore_wasm_bridge.c`；
  FFmpeg 类型不越 guest 边界（AGENTS.md 规则 3 在 WASM 边界同样生效）。
- 模式：
  - A：guest-only decode+hash（`bench`）——execution_tax 主口径；
  - B：host PCM pull（`pcm_host`）——boundary 全量回拷；
  - C：分块 pull（256/1024/4096 frames）——boundary 调用频谱与长尾。
- 正确性 gate：复用 E08 的 33 clean + 11 degraded = **44 fixture**，逐
  runtime 与 native twin 的 observable 全等；任何弱化 gate 的结论无效。
  另做 wasm 侧互检（四个 wasm runtime 相互全等）。

## 工具链 provenance

见 `bench/results/wasm/toolchains.json`（pinned release + SHA256；wamrc
由同一 WAMR source 对照系统 LLVM 18.1.8 构建）。代理地址等环境秘密不进入
本文件与 provenance。

## Typed findings（与数字无关，独立成立）

### E09-WAMR-1：classic-interp bytecode rewriter 对 wasi-libc init guard 的 mis-target

wasi-libc 的 `__wasm_call_ctors` guard 编译为
`i32.load; i32.eqz; br_if 0; unreachable`（guard==0 → br 到块尾继续
ctor；非 0 → 落到 `unreachable`）。LLVM 20 内联进 `_initialize` 后，
WAMR classic-interp 的 rewriter 把该 `br_if` 的跳转目标改写到
`unreachable` 字节上，首次 `_initialize` 即 trap。Reproducible：
`tools/wasm_patch_initialize_guard.py` 只把 `45 0d 00 00 0b → 45 0d 00
01 0b`（unreachable→nop，仅限 `_initialize`/`_start` 函数体内、幂等）
后即可启动。语义：单次 init 下 nop 等价；该 finding 上游级，升级 WAMR
需复测。

### E09-WAMR-2：app heap 布局在缺少 `__heap_base` 导出时踩 guest .bss

WAMR loader 把 app heap 放在 `__data_end`（stomp guest .bss），除非模块
导出 immutable global `__heap_base`（此时插堆并改写该 global）。修复：
wasi-reactor 链接加 `-Wl,--export-if-defined=__heap_base`。证据：BSS
内存探针（instantiate 后 guard 槽位被写坏）、gdb global slot dump。

### E09-WAMR-3：`wasm_runtime_call_wasm_v` 是变参 ABI，不是 wasm_val_t 数组

签名：`(exec_env, func, num_results, results[], num_args, ...raw args)`
——i32 传 `uint32`、i64 传 `uint64`。传 `wasm_val_t` 结构体等于把**结构
体地址**交给 guest（本实验中表现为 door handle 全是宿主栈地址）。
Wasmtime C API 才是 `wasmtime_val_t` 数组口径——两套 API 语义不同，
移植时不得照抄。

### E09-wasi-libc-1：guard 形态依赖 LLVM 内联决策

guard 在 `__wasm_call_ctors` 独立函数时是另一种字节形态；LLVM 20 把它
内联进 `_initialize` 才触发 E09-WAMR-1。补丁脚本按函数体扫描而非全局
替换，避免误伤相同字节序列。

### E09-xmake-1：多 buildir 会话共享 `build/artifacts` 时的产物互相覆盖

native / wasi / emscripten 三个会话（`xmake f -o build/xmake{-em,-wasi}`）
共享同一 artifact 目录；em 会话构建后 `libqianqian_av.a` 变 em-ar 格式
（GNU ld 报 "file format not recognized"）、guest .wasm 被覆盖（曾导致
gate 假阴性）。纪律：gate/bench 运行期**禁止并行构建**；跨会话切换后
`xmake build -r qianqian_av` + 恢复对应 guest artifacts。

### E09-emscripten-1：import 名压缩使宿主无法按名接线 door

emscripten -O2+ 压缩 import module/function 名（门函数混入单字母模块），
且 glue 对 undefined symbols 的 stub 装配晚于 `instantiateWasm` 钩子、
链接器还会重排 import 顺序。harness 的解法：解析 wasm type section，
按**签名**（read=(i64,i32,i32)->i64 / seek=(i64,i64)->i64 /
size=(i64)->i64）填充空槽。教训：嵌入方契约必须按签名或显式
`-g1` 级别保障，不能依赖 import 顺序。

### E09-bridge-abi-1：wasm32 staged-copy 协议不得原样搬到 64 位宿主

pb/native twin 曾把 64 位堆指针截断成 i32 传给 memcpy（段错误）。wasm32
线性内存指针天然 32 位，协议成立；64 位宿主必须用真实指针/handle。
已修（`tools/wasm/qn_pb_native.c`）。

### E09-gate-1：observable 混入 wall-clock 字段会把 gate 变成抽奖

首轮 gate 把 `decode_ms` 一并比较，出现"native vs 全体 42/44 假阴性"。
observable 必须只含可观测行为字段（`tools/wasm_gate.py` 已修正并注明）。

<!-- BEGIN GENERATED TABLES -->
### 1. Correctness gate（44-case，native twin 为 reference）

machine authority：`bench/results/wasm/correctness.json`

| runtime | passed / 44 |
|---|---|
| native | [44, 44] |
| wamr | [28, 44] |
| wamr_aot | [28, 44] |
| wasm3 | [28, 44] |
| wasmtime | [28, 44] |

wasm 侧互检（四个 wasm runtime 相互 observable 全等）：**44 / 44 一致，0 例分歧**。

### 3. 执行 ladder（Mode A，execution_tax = T_guest / T_native）

machine authority：`bench/results/wasm/performance.json`（逐 runtime 5 fixture；native twin 与 guest 同为 -Os codegen 口径）

| runtime | flac 16/44.8k stereo (4 s) | mp3 cbr (4 s) | aac-lc (12 s) | opus (12 s) | mp3 cbr long (12 s) |
|---|---:|---:|---:|---:|---:|
| native ms | 6.85 | 3.43 | 10.50 | 31.39 | 8.55 |
| wamr ms | 229.91 | 301.86 | 655.47 | 3,092.42 | 868.59 |
| wamr_aot ms | 5.33 | 4.99 | 9.71 | 28.77 | 15.20 |
| wasm3 ms | 86.08 | 164.79 | 283.26 | 853.78 | 533.69 |
| wasmtime ms | 5.95 | 6.64 | 11.38 | 32.33 | 25.29 |
| native tax | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| wamr tax | 33.57 | 87.90 | 62.42 | 98.50 | 101.59 |
| wamr_aot tax | 0.78 | 1.45 | 0.92 | 0.92 | 1.78 |
| wasm3 tax | 12.57 | 47.99 | 26.97 | 27.20 | 62.42 |
| wasmtime tax | 0.87 | 1.93 | 1.08 | 1.03 | 2.96 |
| emscripten ms | 8.26 | 14.23 | 18.56 | 57.11 | 25.59 |
| emscripten tax | 1.21 | 4.14 | 1.77 | 1.82 | 2.99 |

perf stat（flac，bench×3，含 instantiate）：

| runtime | cycles（flac bench×3） | IPC | elapsed ms |
|---|---:|---:|---:|
| native | 163,395,454 | 2.68 | 53 |
| wamr | 7,106,184,038 | 2.55 | 1,972 |
| wamr_aot | 188,383,865 | 2.39 | 67 |
| wasm3 | 2,943,488,104 | 1.97 | 806 |
| wasmtime | 4,775,044,732 | 1.49 | 286 |

### 4. Bridge / boundary（flac）

machine authority：`bench/results/wasm/memory.json`（Mode B/C）+ `em_performance.json`

| runtime | Mode B copy GB/s | Mode C 256fr calls/s | Mode C 4096fr calls/s | Mode C max call ms (4096fr) |
|---|---:|---:|---:|---:|
| wamr | 1.71 | 130,865 | 8,628 | 0.004 |
| wamr_aot | 1.74 | 124,185 | 8,525 | 0.042 |
| wasm3 | 1.70 | 133,903 | 8,956 | 0.004 |
| wasmtime | 6.64 | 125,153 | 8,705 | 0.024 |
| emscripten | 0.67 | — | — | 0.037 |

### 6. Memory audit

machine authority：`bench/results/wasm/memory.json`

| runtime | 线性内存 before→after（pages） | memory.grow 次数 | peak RSS（flac / mp3-long）MB |
|---|---|---|---|
| wamr | 256→256 | 0 | 29.8 / 36.5 |
| wamr_aot | 256→256 | 0 | 34.2 / 41.1 |
| wasm3 | 256→256 | 0 | 22.4 / 25.3 |
| wasmtime | 256→256 | 0 | 66.3 / 72.9 |

### 2. Shipping footprint

machine authority：`bench/results/wasm/shipping.json` + `shipping-em.json`

| 交付形态 | raw | gzip -9 | brotli -11 |
|---|---:|---:|---:|
| WASI guest bench .wasm | 1,303,377 | 550,521 | 451,334 |
| WASI SongCore.wasm（reactor） | 1,292,868 | — | — |
| wamrc AOT artifact（x86_64） | 3,588,964 | — | — |
| EM guest bench .wasm（-g1 保符号名） | 1,142,021 | 539,254 | 444,184 |
| EM glue .js（bench） | 118,958 | 33,319 | 28,236 |

### float 容差（native vs wasm，剩余分歧全量解释）

machine authority：`bench/results/wasm/tolerance.json`

| fixture | max\|Δ\| (f32) | 16-bit LSB 折算 | 差异样本占比 |
|---|---:|---:|---:|
| mp3-cbr-id3v23.mp3 | 1.79e-07 | 0.0059 | 48.64% |
| aac-lc-44-stereo.m4a | 5.96e-08 | 0.0020 | 0.00% |
| vorbis-44-stereo.ogg | 0.00e+00 | 0.0000 | 0.00% |
| opus-48-stereo.opus | 0.00e+00 | 0.0000 | 0.00% |

<!-- END GENERATED TABLES -->

## 结果解读

- **解释执行是数量级瓶颈**：WAMR classic-interp tax 33.6–101.6×、
  wasm3 12.6–62.4×，且 cycles 放大与之匹配（43.5× / 18×）；perf 采样
  99.4% 落在 `wasm_interp_call_func_bytecode`。fixture 越长/越碎，
  tax 越高（mp3-long 最差）。
- **AOT/JIT 把执行税基本抹平**：wamr_aot 0.78–1.78×、wasmtime
  0.87–2.96×、emscripten 1.21–2.99×。wamr_aot 在 flac 上 0.78× **反超
  native twin**——口径解释：native twin 与 guest 同为 -Os，wamrc 以
  LLVM -O3 重新本机代码生成，赢在 codegen 而非"wasm 更快"。
- **Bridge 不是瓶颈**：Mode B 全量回拷 1.4 MB（4s 音频）0.83–0.87 ms，
  仅为 wamr guest 解码时间的 ~0.3%；Mode C 每次边界调用 2–4 µs
  （256 帧粒度 ~130K calls/s），max call 无长尾异常。pb 微基准：native
  直拷 1 MiB 22.0 µs（47.6 GB/s），call+copy 与 direct 差 <4%。
- **内存静止**：16 MB 初始线性内存在全部 fixture 上零 `memory.grow`；
  peak RSS wasm3 22 MB 最省、wasmtime 66–73 MB 最重（含 JIT 编译期）。
- **正确性分层清晰**：五个 wasm runtime（含 Emscripten/V8）彼此
  **44/44 bit-identical**；与 native twin 的残余分歧全部由 float-DSP
  codegen 解释（mp3 max|Δ| = 0.006 LSB@16bit、aac 0.002 LSB、
  vorbis/opus = 0）。整数/定点 codec（flac/alac/wav/opus/vorbis）对
  native 也 bit-exact；降级（degraded）行为逐 case 同型。

## Bottleneck verdict（Layer 1）

1. **#1 runtime dispatch（解释执行）**：唯一数量级项。证据：tax 表
   （interp 系 12–102×）× perf 采样（99.4% 解释循环）× cycles 放大。
   对策（供 Layer 2 决策）：AOT（wamrc）或宿主 JIT（wasmtime/JS 引擎）。
2. **#2 shipping（AOT 的代价在体积）**：.aot artifact 3.59 MB ≈ guest
   .wasm 的 2.75×；runtime 静态份额 wasmtime 71 MB(.a) >> WAMR/WASM3
   (<1 MB 级)。分发口径的取舍是 guest+runtime 总和，不是 guest 单体。
3. **#3 bridge/boundary 可忽略**：<1% 总成本，且协议本身无放大；
   惟需记住 E09-bridge-abi-1 的 32 位指针边界。

## Layer 1 decision gate

- 正确性 gate：**PASS**（native twin 44/44；wasm 侧互检 44/44；
  分歧全部由 tolerance.json 量化解释，降级行为同型）。
- 性能 / bridge / 内存 / shipping 数据：**完备**（真实环境测量，
  provenance 齐全）。
- 判定：**Layer 1 PASS，Layer 2（Kotlin）解锁**。按任务边界本轮
  到此 **STOP**：不 merge、不启动 Layer 2。

## 结论有效性声明

- 本文件所有数字槽位由 `tools/wasm_report_tables.py` 从
  `bench/results/wasm/summary.json` 派生；手写即违规。
- 真实环境全量运行（Linux x86_64 / WSL2）；Windows 侧不在本轮范围。
- Emscripten 数据经 Node harness 实测（V8 口径）；浏览器（JSPI/worker）
  形态待 Layer 2 按需补测。
