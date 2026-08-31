# E09 — WASM Total-Cost & Bottleneck Audit

状态：`CODE_COMPLETE_PENDING_VALIDATION`（Layer 1 结构完成；下文所有
`[machine]` 槽位以 `bench/results/wasm/*.json` 与 `bench/profiles/wasm-*.json`
为唯一权威，报告禁止手抄数字。Layer 2（Kotlin）在 Layer 1 gate 判定前
**禁止启动**。）

## 研究问题（issue #5 的 Layer 1 部分）

> 同一份 Qianqian 最小 FFmpeg source closure 编成 WASM guest 后，在
> 不同 runtime（Native / WAMR classic-interp / WAMR AOT / wasm3 / Wasmtime /
> Emscripten+JSPI-host）上的**总成本**是多少？瓶颈依次是什么？
> bridge/边界拷贝相对 guest 纯解码与 native 基线各占多少税？

三个口径严格分开（继承 E08 纪律）：

- **SHIPPING FOOTPRINT**：guest .wasm + runtime 份额 + bridge + AOT
  artifact 的**总和**，raw / stripped / xz；浏览器分发口径另列
  gzip/brotli（不许与 xz 混写）。
- **EXECUTION TAX**：`execution_tax = T_guest / T_native`（同机同
  fixture 同模式）。
- **BRIDGE TAX**：`bridge_tax = T_bridge / T_guest`；
  `total_tax = T_bridge / T_native`。

## 实验设计

- 两层实验：Layer 1 = WASM core + runtime ladder（本文件）；
  Layer 2 = Kotlin 宿主（**锁定**，gate 通过才解禁）。
- WASI 优先，Emscripten 为第二实现（同一 guest source，不同 libc/入口形状）。
- Host-owned IO：guest 只见不透明 handle + `qn_door_read/seek/size`
  （64-bit offset）；bridge 为薄层 `songcore_wasm_bridge`；FFmpeg 类型
  不得越过 guest 边界（AGENTS.md 规则 3 在 WASM 边界同样生效）。
- 模式：
  - A：guest-only decode+hash（`bench`）——execution_tax 主口径；
  - B：host PCM pull（`pcm`）——bridge 全量回拷；
  - C：分块 pull（256/1024/4096 frames）——bridge 调用频谱。
- 正确性 gate：复用 E08 的 33 clean + 11 degraded = **44 fixture**，
  逐 runtime 与 native twin 的 observable 全等（status/container/codec/
  rate/channels/duration/metadata/artwork_sha/decode/eof/suffix/seeks）。
  任何弱化 gate 的结论无效。

## 工具链 provenance（pinned，见 bench/provenance/e09-perf/toolchains.json）

- wasi-sdk 25.0（clang 20.1.x）
- WAMR v2.4.5（classic interp：FAST_INTERP=0；AOT 走 wamrc + 系统 LLVM 18）
- wasm3（pinned commit）
- Wasmtime v48.0.x（C API）
- Emscripten 6.0.8（emsdk pinned）
- 代理地址等环境秘密不得进入本文件与 provenance（仅记录镜像源事实时用
  "http_proxy set" 布尔）。

## Typed findings（与数字无关，独立成立）

### E09-WAMR-1：classic-interp bytecode rewriter 对 wasi-libc init guard 的 mis-target

wasi-libc 的 `__wasm_call_ctors` guard 编译为
`global.get __memory_base; i32.const N; add; i32.load; i32.eqz; br_if 0; unreachable`
（guard==0 → br 到块尾继续 ctor；非 0 → 落到 `unreachable`）。
LLVM 20 内联进 `_initialize` 后，WAMR classic-interp 的 rewriter 会把该
`br_if` 的跳转目标改写到 `unreachable` 字节上，首次 `_initialize` 即
trap。Reproducible：`tools/wasm_patch_initialize_guard.py` 只把
`45 0d 00 00 0b → 45 0d 00 01 0b`（unreachable→nop，仅限
`_initialize`/`_start` 函数体内、幂等）后即可启动。语义：我们单次
init，nop 等价；但该 finding 上游级（wasm_loader.c rewriter 对
`br_if` depth-0 目标重算），升级 WAMR 需复测。

### E09-WAMR-2：app heap 布局在缺少 `__heap_base` 导出时踩 guest .bss

WAMR loader 把 app heap 放在 `__data_end`（stomp guest .bss），除非模块
导出 immutable global `__heap_base`（此时插堆并改写该 global）。
修复：wasi-reactor 链接加 `-Wl,--export-if-defined=__heap_base`。
证据：BSS 内存探针（instantiate 后 guard 槽位被写坏 01/ffffffff）、
gdb global slot dump。

### E09-WAMR-3：`wasm_runtime_call_wasm_v` 是变参 ABI，不是 wasm_val_t 数组

签名：`(exec_env, func, num_results, results[], num_args, ...raw args)`
——i32 传 `uint32`、i64 传 `uint64`。传 `wasm_val_t` 结构体等于把
**结构体地址**交给 guest（本实验中表现为 door handle 全是宿主栈地址）。
Wasmtime C API 才是 `wasmtime_val_t` 数组口径——两套 API 语义不同，
移植时不得照抄。

### E09-wasi-libc-1：guard 形态依赖 LLVM 内联决策

guard 在 `__wasm_call_ctors` 独立函数时是另一种字节形态；LLVM 20 把它
内联进 `_initialize` 才触发 E09-WAMR-1。补丁脚本按函数体扫描而非全局
替换，避免误伤相同字节序列。

### E09-xmake-1：多 buildir 会话共享 `build/artifacts` 时的产物互相覆盖

native / wasi / emscripten 三个会话（`xmake f -o build/xmake{-em,-wasi}`）
共享同一 `artifact_dir`；em 会话构建后 `libqianqian_av.a` 变 em-ar 格式
（GNU ld 报 "file format not recognized"）、`build/artifacts/wasm/*.wasm`
被 em 版覆盖（曾导致 gate 假阴性 wamr 2/44）。纪律：gate/bench 运行期
**禁止并行构建**；跨会话切换后先 `xmake build -r qianqian_av` 重建再
恢复对应 guest artifacts（`/tmp/wasi-artifacts` 快照）。

### （候选）E09-Wasmtime-C-API-1：`wasm_valtype_vec_new_uninitialized`+`wasm_functype_new` 双释放

（上会话现象，待复现最小 repro 后定级；当前 runner 绕开该组合。）

## 结果

### 1. Correctness gate（44-case，native twin 为 reference）

[machine：bench/results/wasm/correctness.json]

| runtime | passed/44 |
|---|---|
| native twin | [machine] |
| WAMR interp | [machine] |
| WAMR AOT | [machine] |
| wasm3 | [machine] |
| Wasmtime | [machine] |

### 2. Shipping footprint（总和口径）

[machine：bench/results/wasm/shipping.json]

guest .wasm 单体不算数；shipping = guest + runtime 静态份额 + bridge +
（AOT 时）.aot artifact。浏览器分发另列 gzip/brotli。

### 3. 执行 ladder（Mode A，execution_tax）

[machine：bench/results/wasm/performance.json]

| runtime | T_guest (ms) | xRT | execution_tax |
|---|---:|---:|---:|
| native twin | [machine] | [machine] | 1.00 |
| WAMR interp | [machine] | [machine] | [machine] |
| WAMR AOT | [machine] | [machine] | [machine] |
| wasm3 | [machine] | [machine] | [machine] |
| Wasmtime | [machine] | [machine] | [machine] |

### 4. Bridge tax（Mode B/C）

[machine：bench/results/wasm/bridge.json]

### 5. PCM-copy 微基准

[machine：bench/results/wasm/bridge.json#pcm_copy]

### 6. Memory audit（RSS / 线性内存 / memory.grow / 长尾暂停）

[machine：bench/results/wasm/memory.json]

### 7. perf 归因（大类：decoder / runtime dispatch / bridge / memcpy /
allocation / hashing / other）

[machine：bench/profiles/wasm-*.json + bench/provenance/e09-perf/]

### 8. Emscripten 变体

[machine：bench/results/wasm/toolchains.json#emscripten]

## Bottleneck verdict（Layer 1 出口）

[machine 汇总后填写：#1/#2/#3 成本项 + 证据链；不得凭感觉排序]

## Layer 1 decision gate

[machine：gate 判定 PASS/FAIL + Layer 2 解禁/维持锁定]

## 结论有效性声明

- 本文件所有数字槽位由脚本从 `bench/results/wasm/*.json` 派生；
  手写即违规。
- 未经真实环境复现的 selective-build / audible smoke 标
  `CODE_COMPLETE_PENDING_VALIDATION`，不得写 PASS（AGENTS.md 规则 9）。
