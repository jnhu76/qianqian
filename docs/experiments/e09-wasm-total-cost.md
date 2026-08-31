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

- **SHIPPING FOOTPRINT**：deployment-shaped accounting —— 每后端交付集合
  为 host（stripped release）+ 对应 guest artifact + bridge；WAMR AOT 部署
  时以 `.aot` **替代** .wasm（不双份相加），浏览器口径另计 glue .js。
  raw / gzip -9 / brotli -11 / xz -9e 分别列，不许混写。
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
  runtime 与 native twin 比较：结构行为 exact-required；mp3/aac 的 PCM
  hash 字段走 identity-bound 的数值 tolerance（见 float 容差节）；
  exact / accepted-with-tolerance / rejected 分别计数且不被隐藏。另做
  wasm 侧互检（五个 WASM 实现/路径相互全等，canonical anchor = WAMR），
  分歧数 ≠ 0 即 machine closure **FAIL**（硬 gate，不只是报告指标）。

## 工具链 provenance

见 `bench/results/wasm/toolchains.json`（pinned release + SHA256；wamrc
由同一 WAMR source 对照系统 LLVM 18.1.8 构建）。代理地址等环境秘密不进入
本文件与 provenance。

## Typed findings（与数字无关，独立成立）

### E09-WAMR-1：WAMR interp 与 AOT 在 wasi-libc init guard 上同样 misexecute（OBSERVED BEHAVIOR）

**OBSERVED（已复现，非推断）**：同一份 pristine guest 字节——

- Wasmtime / wasm3 / Node（V8）：`_initialize` 正常；
- WAMR classic-interp：`_initialize` trap `unreachable`；
- WAMR AOT（wamrc 编译 pristine 模块）：同样 trap `unreachable`。

workaround（`tools/wasm_patch_initialize_guard.py` 把 guard 内的
`unreachable` 改成 `nop`，只作用于 `_initialize`/`_start` 函数体、幂等）后，
WAMR interp 与 AOT 均正常初始化。字节形态：wasi-libc 的 `__wasm_call_ctors`
guard 编译为 `i32.load; i32.eqz; br_if 0; unreachable`，且 LLVM 20 内联进
`_initialize`；`45 0d 00 00 0b → 45 0d 00 01 0b` 即可启动。语义：单次 init 下
nop 等价；代价是失去 double-init abort（E09 runner 只 init 一次）。

**ROOT-CAUSE HYPOTHESIS（待 upstream 最小 repro 确认，勿升级为结论）**：既然
interp 与 AOT 两条路径同样复现，根因可能落在 WAMR 共享的 loader/control-flow
机制、interp 的 branch/block target 重写、wamrc 的 codegen、或 validation——
不能仅凭 interp 表现就钉死为"classic-interp bytecode rewriter"。正式 upstream
report 需要最小 repro 后单独提交，再升级结论。

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

### E09-native-pcm-1：native twin 的 Mode C 把 64 位堆指针截断成 wasm32 i32

native twin 的 `bench_pcm_host` 复用 guest 的 `bench_pcm_pull(int32 dst)`
（wasm32 线性内存指针口径），把 `malloc` 出的 64 位堆指针强转 i32 后符号
扩展成非法地址 → SIGSEGV。与 E09-bridge-abi-1 同族；修复为 native 专用
`pull_native(uint8_t*)` 直传真实指针（`bench/wasm/qn_guest_bench.c`）。

### E09-sha-1：runner 与 guest 的 SHA-256 K 常量表缺一项并截断（本轮发现）

`tools/wasm/qn_runner_common.c` 与 `bench/wasm/qn_guest_bench.c` 里
copy-paste 的 SHA-256 实现，其 K 常量表**缺失 `0x391c0cb3`（K[52]）且只有
63 项**（K[63] 被零初始化）——第 52–63 轮压缩用了错误/缺失的常量，所有
"sha256" 值都是错的（如 `sha256("abc")` 应为 `ba7816bf…` 却得
`ab0bd158…`）。影响：correctness.json 的 canonical/suffix/seek hash、
`pcm_host` 的 sha256 全部不是真 SHA-256。由于 guest/runner 共享同一错误
实现，跨 runtime 比较仍自洽（gate 的判等语义未受影响），但"sha256"标签
是假的。修复：两个文件各补上缺失常量；本轮已全量重生成 correctness /
correctness-em / tolerance 证据（native twin 用回完整 codec 的
`build/minimize/c5/manifest.json`，与历史 PCM 逐字节一致——旧 hash 可用
错误实现复算验证）。

## Typed findings 责任归属（review 后重分类）

- **upstream candidates**：E09-WAMR-1（已具最小 repro 形态；正式 upstream
  report 待本 PR machine-closed 后单独提交。根因**未提前钉死**——interp 与
  AOT 均复现，最小 repro 后再判断是共享 loader/control-flow 机制、interp
  的 branch/block target rewrite、wamrc codegen 还是 validation；回归测试
  至少要求 `br_if 0` 必须跳到 block end，不能落在 `end` 前的指令）。
  E09-WAMR-2 暂称 **upstream candidate，待
  contract audit**——需先证明"无 `__heap_base` 导出 + 合法模块 + 正常
  instantiate = WAMR 覆盖 guest live .bss"；若官方 contract 本要求该
  embedding 模式导出 `__heap_base`，则应归为 documentation/API safety
  issue 而非语义 bug。
- **Qianqian bugs**：E09-bridge-abi-1、E09-native-pcm-1、E09-gate-1、
  E09-sha-1（SHA-256 K 表缺项，本轮发现并修复）、
  E09-xmake-1（artifact 会话互相覆盖——已有纪律，见工具链 provenance）。
- **API/toolchain hazards**：E09-WAMR-3（变参 ABI）、E09-emscripten-1
  （import 名压缩/重排）、E09-wasi-libc-1（guard 形态依赖 LLVM 内联决策）。

## Artifact 纪律（pristine vs WAMR-workaround）

WASI session 现在产出 **pristine** toolchain artifact（xmake 不再在
after_build 里打补丁）。WAMR classic-interp 所需的 init-guard workaround
（E09-WAMR-1）只施加在派生副本上，由 `tools/wasm_prepare_artifacts.py`
生成并记录 pre/post SHA256（`bench/results/wasm/artifacts.json`）：

- `SongCore.wasm` / `qn_guest_bench.wasm` = pristine，Wasmtime / wasm3 /
  Node 消费；
- `SongCore.wamr-workaround.wasm` / `qn_guest_bench.wamr-workaround.wasm` =
  WAMR classic-interp 消费；
- `SongCore.aot`（product，由 `SongCore.wamr-workaround.wasm` 经 wamrc
  编译——WAMR AOT 同样 trap pristine 的 init guard，E09-WAMR-1 覆盖 AOT
  路径）与 `qn_guest_bench.aot` 供 AOT 路径。

回归证明：pristine 在 Wasmtime / wasm3 / Node 正常初始化并在 WAMR
classic-interp 于 `_initialize` trap（unreachable）；workaround 副本在 WAMR
classic-interp 与 WAMR AOT 均正常初始化。

<!-- BEGIN GENERATED TABLES -->
### 1. Correctness gate（44-case，native twin 为 reference）

machine authority：`bench/results/wasm/correctness.json` + `correctness-em.json`（V8 行，Node harness 独立 authority）

| runtime | exact | accepted\_with\_tolerance | rejected |
|---|---:|---:|---:|
| native | 44 | 0 | 0 |
| wamr | 28 | 16 | 0 |
| wamr_aot | 28 | 16 | 0 |
| wasm3 | 28 | 16 | 0 |
| wasmtime | 28 | 16 | 0 |
| emscripten | 28 | 16 | 0 |

wasm 侧互检（emscripten, wamr, wamr_aot, wasm3, wasmtime，canonical anchor = wamr）：**44 / 44 一致，0 例分歧**；互检硬 gate = **PASS**（≠0 即 machine closure FAIL）。

### 3. 执行 ladder（Mode A，execution_tax = T_guest / T_native）

machine authority：`bench/results/wasm/performance.json`（逐 runtime 5 fixture；native twin 与 guest 同为 -Os codegen 口径）

| runtime | flac 16/44.1k stereo (4 s) | mp3 cbr (4 s) | aac-lc (12 s) | opus (12 s) | mp3 cbr long (12 s) |
|---|---:|---:|---:|---:|---:|
| native ms | 8.68 | 7.19 | 15.04 | 46.25 | 14.31 |
| wamr ms | 352.74 | 499.17 | 1,020.31 | 3,415.26 | 1,091.32 |
| wamr_aot ms | 7.50 | 6.43 | 10.83 | 35.69 | 18.19 |
| wasm3 ms | 108.16 | 197.31 | 358.22 | 998.07 | 597.97 |
| wasmtime ms | 9.74 | 10.15 | 15.77 | 46.60 | 26.49 |
| native tax | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| wamr tax | 40.63 | 69.42 | 67.86 | 73.84 | 76.26 |
| wamr_aot tax | 0.86 | 0.89 | 0.72 | 0.77 | 1.27 |
| wasm3 tax | 12.46 | 27.44 | 23.82 | 21.58 | 41.78 |
| wasmtime tax | 1.12 | 1.41 | 1.05 | 1.01 | 1.85 |
| emscripten ms | 10.89 | 17.86 | 20.64 | 54.90 | 34.88 |
| emscripten tax | 1.25 | 2.48 | 1.37 | 1.19 | 2.44 |

perf stat（flac，bench×3，整进程 elapsed——startup 分阶段见 `lifecycle`）：

| runtime | cycles（flac bench×3） | IPC | elapsed ms（整进程） |
|---|---:|---:|---:|
| native | 181,274,539 | 2.75 | 72 |
| wamr | 8,554,267,096 | 2.12 | 3,118 |
| wamr_aot | 191,592,563 | 2.27 | 84 |
| wasm3 | 3,057,142,236 | 1.90 | 988 |
| wasmtime | 5,660,557,418 | 1.27 | 719 |

### 4. Bridge / boundary（flac，bridge_tax = 回拷成本 / 该后端 guest 解码成本）

machine authority：`bench/results/wasm/memory.json`（Mode B/C）+ `em_performance.json`（em 行，view/copy/hash 分列）

| runtime | Mode B copy GB/s | copy ms | guest decode ms | bridge\_tax | Mode C 256fr calls/s | Mode C 4096fr calls/s |
|---|---:|---:|---:|---:|---:|---:|
| native | 1.45 | 0.976 | 8.68 | 0.112 | 94,032 | 5,891 |
| wamr | 1.10 | 1.280 | 352.74 | 0.004 | 92,033 | 5,536 |
| wamr_aot | 1.08 | 1.300 | 7.50 | 0.173 | 75,401 | 4,980 |
| wasm3 | 1.10 | 1.285 | 108.16 | 0.012 | 85,719 | 6,328 |
| wasmtime | 3.65 | 0.387 | 9.74 | 0.040 | 99,785 | 6,640 |
| emscripten（view+copy+hash 分列） | 1.30 (copy) | 1.085 | 10.89 | 0.100 | — | — |

### 6. Memory audit

machine authority：`bench/results/wasm/memory.json`（native 行已修正 CLI，RSS 为真实测量）

| runtime | 线性内存 before→after（pages） | memory.grow 次数 | peak RSS（flac / mp3-long）MB |
|---|---|---|---|
| native | n/a（native 无线性内存） | — | 6.8 / 12.6 |
| wamr | 256→256 | 0 | 28.5 / 36.5 |
| wamr_aot | 256→256 | 0 | 33.1 / 41.1 |
| wasm3 | 256→256 | 0 | 22.5 / 25.4 |
| wasmtime | 256→256 | 0 | 68.7 / 75.2 |

### 7. Lifecycle / startup（load → compile → instantiate → first open → first PCM → steady）

machine authority：`bench/results/wasm/lifecycle.json`（flac）

| runtime | load ms | compile/JIT ms | instantiate ms | first open ms | first PCM ms | steady decode ms |
|---|---:|---:|---:|---:|---:|---:|
| native | — | — | — | 0.61 | 0.13 | 8.29 |
| wamr | 14.92 | — | 2.31 | 22.20 | 7.63 | 334.00 |
| wamr_aot | 7.51 | — | 1.14 | 2.58 | 0.20 | 8.79 |
| wasm3 | 1.54 | 12.48 | — | 11.41 | 2.68 | 124.79 |
| wasmtime | 1.83 | 414.60 | 0.28 | 0.49 | 0.15 | 8.77 |
| emscripten | 1.56 | — | 16.59 | 13.06 | 0.65 | 14.96 |

### 2. Shipping footprint（product-shaped deployment sets）

machine authority：`bench/results/wasm/shipping.json`（host = stripped release runner；AOT 替代 .wasm）

| 交付形态 | host（stripped release） | guest artifact | 合计 raw | 合计 xz -9e | 浏览器口径 gzip/brotli |
|---|---:|---:|---:|---:|---:|
| native | 1,505,608 | —（native 内置） | 1,505,608 | 580,792 | — |
| wamr_interp | 465,056 | SongCore.wamr-workaround.wasm 1,292,868 | 1,757,924 | 591,664 | — |
| wamr_aot | 465,056 | SongCore.aot 3,555,004 | 4,020,060 | 1,306,528 | — |
| wasm3 | 207,432 | SongCore.wasm 1,292,868 | 1,500,300 | 499,524 | — |
| wasmtime | 25,413,560 | SongCore.wasm 1,292,868 | 26,706,428 | 6,753,232 | — |
| browser | 118,367 | SongCore.wasm (emscripten) 1,132,413 | 1,250,780 | — | 564,651 / 468,094 |

参考行（单 artifact）：
- SongCore.wasm（pristine）：raw 1,292,868，gzip -9 546,255，brotli -11 447,763
- SongCore.aot（product，wamrc from SongCore.wamr-workaround.wasm）：raw 3,555,004，xz -9e 1,146,836（AOT 部署时替代 .wasm，不与 .wasm 相加）

### float 容差（native vs wasm，剩余分歧全量解释）

machine authority：`bench/results/wasm/tolerance.json`（identity-bound 逐 fixture 证据：native + canonical WASM anchor PCM hash 逐 stream 绑定 + 数值 delta，覆盖全部 tolerated fixture）

| fixture | max\|Δ\| (f32) | 16-bit LSB 折算 | 差异样本占比 | samples / frames |
|---|---:|---:|---:|---:|
| aac-adts-44-stereo.aac | 5.96e-08 | 0.0020 | 0.04% | 178176 / 174 |
| aac-artwork.m4a | 5.96e-08 | 0.0020 | 0.02% | 176400 / 173 |
| aac-lc-44-mono.m4a | 5.96e-08 | 0.0020 | 0.01% | 176400 / 173 |
| aac-lc-44-stereo.m4a | 5.96e-08 | 0.0020 | 0.01% | 529200 / 517 |
| aac-lc-48-stereo.m4a | 2.98e-08 | 0.0010 | 0.02% | 192000 / 188 |
| aac-malformed-header.aac | 5.96e-08 | 0.0020 | 1.89% | 3072 / 3 |
| aac-short.m4a | 5.96e-08 | 0.0020 | 0.23% | 13230 / 13 |
| aac-truncated.m4a | 5.96e-08 | 0.0020 | 0.01% | 314368 / 307 |
| aac-vbr.m4a | 5.96e-08 | 0.0020 | 0.02% | 176400 / 173 |
| mp3-cbr-id3v23-artwork.mp3 | 1.79e-07 | 0.0059 | 97.29% | 176400 / 155 |
| mp3-cbr-id3v23.mp3 | 1.79e-07 | 0.0059 | 97.29% | 176400 / 155 |
| mp3-corrupt-tail.mp3 | 1.79e-07 | 0.0059 | 97.09% | 132527 / 116 |
| mp3-long.mp3 | 1.79e-07 | 0.0059 | 97.15% | 529200 / 461 |
| mp3-minimal.mp3 | 1.79e-07 | 0.0059 | 97.52% | 88200 / 78 |
| mp3-short.mp3 | 1.19e-07 | 0.0039 | 97.66% | 13230 / 13 |
| mp3-vbr-id3v24.mp3 | 1.79e-07 | 0.0059 | 97.11% | 176400 / 155 |
| opus-48-stereo.opus | 0.00e+00 | 0.0000 | 0.00% | 576000 / 601 |
| opus-truncated.opus | 0.00e+00 | 0.0000 | 0.00% | 431688 / 450 |
| vorbis-44-stereo.ogg | 0.00e+00 | 0.0000 | 0.00% | 529200 / 518 |
| vorbis-truncated.ogg | 0.00e+00 | 0.0000 | 0.00% | 360000 / 352 |

evidence：native↔canonical anchor（wamr）identity-bound （逐 tolerated stream 记录 native/anchor PCM hash，stale 即 REJECT）；bound = 1e-06。

<!-- END GENERATED TABLES -->

## 结果解读

- **解释执行是数量级瓶颈**：WAMR classic-interp 与 wasm3 的 execution_tax
  都在一个数量级以上（见 ladder 表），且 cycles 放大与之匹配；perf 采样
  99.4% 落在 `wasm_interp_call_func_bytecode`。fixture 越长/越碎，tax 越高
  （mp3-long 最差）。
- **AOT/JIT 把执行税基本抹平**：wamr_aot、wasmtime 稳态解码回到
  native 同量级；wamr_aot 在部分 fixture 上 **反超 native twin**——口径
  解释：native twin 与 guest 同为 -Os，wamrc 以 LLVM -O3 重新本机代码
  生成，赢在 codegen 而非"wasm 更快"。
- **startup 与稳态解码必须分开看**：wasmtime 整进程 elapsed 的大头是
  **JIT compile**（lifecycle 表里 compile_ms 单独列出，flac 上数百 ms
  量级），instantiate 本身是亚毫秒级；WAMR AOT 的 load 也远大于
  instantiate。lifecycle 表按
  load → compile → instantiate → first open → first PCM → steady decode
  分阶段列，禁止再用整进程 elapsed 当 startup。
- **Bridge 是相对成本，不是绝对结论**：`bridge_tax = 回拷成本 / 该后端
  guest 解码成本` 逐后端计算（见 bridge 表）。慢解释器下仅 ~0.4%
  （WAMR interp flac），但 **execution tax 一旦被 AOT/JIT 消掉，
  bridge/copy 会从三级成本升级成一线成本**：WAMR AOT flac ~17%、
  Wasmtime ~4%、emscripten ~10%，native 直拷基线与之同量级（同一 host
  memcpy）。EM 的 Mode B 按 view / copy / hash 分列（`Buffer.from` 是
  共享 view 不是复制，且原测量把 SHA256 混进了 copyMs——hash 实测比 copy
  还贵）。
- **内存**：native RSS 基线已修正 CLI 并实测（远低于全部 wasm runtime，
  无运行时开销）；`memory.grow` 为 **0 的结论限定在已审计的代表性
  fixture（flac + mp3-long）**，不是声称 44 案全量 instrumented。wasm3
  最省、wasmtime 最重（含 JIT 编译期）。
- **正确性分层由机器 authority 支撑**：gate 现在把
  `exact-required + tolerance-allowed = accepted` 写成可执行策略——结构
  字段（frame/sample count、typed degraded、metadata、seek 语义）exact，
  只有 mp3/aac float-DSP 族的 PCM hash 字段走 tolerance，且每个 tolerated
  fixture 都有 **identity-bound 逐 fixture 证据**（`tolerance.json` 记录被
  数值比较的具体 native 与 canonical WASM anchor（WAMR）PCM hash，覆盖
  full decode / suffix / seek suffix 每个 tolerated stream，逐 stream
  max|Δ| ≤ 1e-6 实测）。gate 只有当前 native/anchor hash 等于证据、且该
  runtime 与 anchor bit-identical 时才放行（stale evidence / PCM 变更即
  REJECT）；非 anchor runtime 与 anchor 的互检 ≠ 0 使 machine closure
  **FAIL**（硬 gate）。`exact_matches` 与 `accepted_with_tolerance` 分别
  计数，28/44 exact + 16/44 tolerance 的事实不被藏掉。Emscripten/V8 由
  独立的 `correctness-em.json`（Node harness）加入同一 authority，"五个
  wasm 实现 44/44 bit-identical"因此有统一机器依据。

## Bottleneck verdict（Layer 1）

1. **#1 runtime dispatch（解释执行）**：唯一数量级项。证据：tax 表
   （interp 系全部在一个数量级以上）× perf 采样（99.4% 解释循环）×
   cycles 放大。对策（供 Layer 2 决策）：AOT（wamrc）或宿主 JIT
   （wasmtime/JS 引擎）。
2. **#2 shipping / startup（AOT 与 JIT 的代价在体积与启动）**：按
   product-shaped deployment set 计（stripped release host + 对应 guest
   artifact；AOT 以 .aot **替代** .wasm，不双份相加）。wasmtime host
   静态份额最大（stripped 仍 ~25 MB 级，含 JIT）；WAMR/wasm3 host
   <1 MB 级；AOT artifact（product `SongCore.aot`）是 guest .wasm 的
   ~2.75×。startup 的 JIT compile 是 wasmtime 的启动大头。
3. **#3 bridge/boundary 在 execution tax 消除后进入一线**：慢解释器下
   bridge 可忽略（~0.4%），AOT/JIT 下 4–17%（见 bridge 表）；协议本身
   无放大，惟需记住 E09-bridge-abi-1 / E09-native-pcm-1 的 64 位宿主
   指针边界。

## Layer 1 decision gate

- 正确性 gate：**PASS**（可执行策略：exact + tolerance = accepted，
  rejected = 0；`exact_matches` 与 `accepted_with_tolerance` 分开计数；
  每个 tolerated fixture 都有 identity-bound 逐 fixture 证据（native +
  canonical WASM anchor PCM hash 绑定 + 逐 stream 数值 delta ≤ bound）；
  wasm 互检 = 0 是硬 gate（分歧 ≠ 0 即 closure FAIL）；Emscripten/V8 由
  独立 `correctness-em.json` 加入同一 authority；降级行为同型）。
- 性能 / bridge / 内存 / shipping / lifecycle 数据：**完备**（真实环境
  测量，provenance 齐全；native 内存基线 CLI 已修正；startup 分阶段
  测量）。
- 判定：**Layer 1 PASS，Layer 2（Kotlin）解锁**。按任务边界本轮
  到此 **STOP**：不 merge、不启动 Layer 2。E09-WAMR-1 upstream report
  在本次 machine-closure 之后单独提交。

## 结论有效性声明

- 本文件所有数字槽位由 `tools/wasm_report_tables.py --check` 保证与
  `bench/results/wasm/summary.json` 同步；summary 是唯一 machine
  authority，禁止任何脚本绕过它直读原始 JSON。
- 真实环境全量运行（Linux x86_64 / WSL2）；Windows 侧不在本轮范围。
- Emscripten 数据经 Node harness 实测（V8 口径）；浏览器（JSPI/worker）
  形态待 Layer 2 按需补测。
- `memory.grow = 0` 仅覆盖已审计的代表性 fixture（flac + mp3-long），
  不扩展到 44 案全量。
