# Audio Processing Plugin architecture audit

> ## 审计最终结论
>
> - **判决：`GO_WITH_CORRECTIVES`。** Audio Processing 可以在现有 K0 架构内承载，且不需要候选任务书中的 "Audio Processing Plugin + AudioProcessingCapability" 形状。
> - **统一 Plugin substrate 已存在且唯一**（`ComponentSpec → K0 admission → Fiber`）；审计结论是**不引入任何新 Plugin 抽象**，且当前证据下 **Audio Processing 既不挣得 Plugin 身份，也不挣得 Capability**（D13 三条件全部不成立；required-single 基数在 plan 时即确定性拒绝 "每 effect 一个 provider"）。
> - **首选架构（Model D+，修正后）**：Playback Session 将 ProcessorChain 作为又一个 episode-scoped owned resource（与 PcmEdge/decode worker 同类）；App 拥有有序链配置（App-owned product configuration，同 D14.9 volume / U2 playlist 惯例）；稳定 DSP seam = `qianqian-audio-api`（contracts crate，D7）内的普通 processor contract trait；PCM 插入点 = decode worker staging loop（RT render thread 完全不动）；Abstract Factory 仅以普通 Rust 构造函数形态存在于 per-episode instance 创建层。
> - **权威动作（必需）**：选项 B —— 对 `ADR-PBK-002` 的一条窄修正案（建议 D14.11 "Audio Processing minimum"）。DSP/EQ 在 U2 修正案中明确属本阶段未进入的禁入清单，PBK-002 §14 将 "Processing Plugin" 列为 OPEN（= 未授权），D14.10 禁止 coding agent 自行选择新 Plugin/Capability。**实现前必须先过 authority freeze；Gain 不能在本审计后立即实现。**
> - **下一任务：`QIANQIAN-AUDIO-PROCESSING-AUTHORITY-FREEZE`**（起草 D14.11 + 以 Gain 可弃置探针作为 grounding evidence，遵循 F5-GATE/V-PROBE 先例；不合并生产实现）。
> - **审计期间对 worktree 零改动、零提交、零 PR**；对照用 DeepSeek Harness clone 位于仓库之外（`~/Source/deepseek-harness`）。
> - **本文件是审计证据记录（EVIDENCE），不是架构权威**。唯一规范性权威仍是 `docs/adr/ADR-PBK-001.md`、`docs/adr/ADR-PBK-002.md`、`docs/adr/ADR-PBK-003.md` 与 K0 设计/实现 ADR；本报告不得被引用为新的 authority，其任何结论若要 earns normative status，必须走显式 authority amendment under review。

| Field | Value |
|---|---|
| Audit task | QIANQIAN-AUDIO-PROCESSING-PLUGIN-AUDIT |
| Date | 2026-09-30 |
| **BASE_SHA（审计证据基线）** | **`ba545ee5927e1c19963dee85922569668a3fa023`**（main；下文全部 file:line 引用、代码事实与权威文本均以此 HEAD 为准） |
| Baseline 取得方式 | 审计开始时本地 main 位于 `63b4d51`（落后 origin/main 7 个提交）；按任务指示先 `git pull --ff-only` 快进到 `ba545ee`（该次拉取带入 AGENTS.md 更新与 `songcore-binding-architecture.md`），**全部证据采集在 `ba545ee` 上进行** |
| Worktree during audit | clean（起点与终点 `git status --porcelain` 均为空；审计期间零文件改动、零提交、零 PR；本记录文件是审计结束后按用户指示单独落盘的唯一变更） |
| External reference clone | DeepSeek Harness clone 至仓库之外 `/home/hoo/Source/deepseek-harness`（`deepseek-ai/deepseek-harness`，shallow；其 `vendor/README.md` 确认 vendored cordis 4.0.0-rc.7，upstream `cordiverse/cordis`） |
| Evidence base | 三份 ADR 全文 + K0 设计/实现 ADR + CONTEXT/AGENTS/overview/PCM 契约与 Phase B/C 证据记录 + `docs/audits/plugin-boundary-conformance-audit.md` + 生产代码探查（两个独立 file:line 级探查：composition/app 组装面、PCM 路径/episode 生命周期面） |
| Truth class | EVIDENCE — audit record, not authority |
| Review outcome | 单轮 agent 独立审计（adversarial posture，H1–H8 全部执行）；**尚未经过独立 human adversarial review** —— 本记录随 PR 提交，等待 review；不得在 review 前作为实现授权 |

---

## 1. Executive verdict

**GO_WITH_CORRECTIVES。**

```text
现有 K0 能否承载 Audio Processing?
    能 —— 且承载形态比候选图更简单（无新 Plugin、无新 Capability）。
是否需要新的 plugin 抽象?
    不需要。ComponentSpec → K0 admission → Fiber 是唯一统一 substrate（§2）。
Abstract Factory 是否合适、在哪一层?
    仅在 per-episode processor instance 创建层合适，且只是普通 owned Rust
    mechanism（session 激活时按格式构造），不是架构级 Abstract Factory。
    任务书 §4 的四分法概念正确，但四层中只有后两层（Factory、Processor
    instance）在当前证据下存在；前两层（Plugin、Capability）未被 D13 挣得。
首选架构?
    Model D+（见 §6）：session-owned chain + audio-api processor contract
    + App-owned 有序配置 + decode-worker 插入点。
需要什么权威变更?
    必需 —— 窄修正案 B（D14.11），见 §12。
Gain 能否在本审计后立即实现?
    不能；必须先完成 QIANQIAN-AUDIO-PROCESSING-AUTHORITY-FREEZE。
```

## 2. Current architecture truth（BASE = `ba545ee` 的代码实况）

```text
Qianqian App (crates/qianqian-app: QianqianApp，薄准入层，自身零产品组件)
    │ register_component ×3 + revise_desired(Vec<DesiredEntry>)
    ▼
CompositionKernel (crates/qianqian-composition，[dependencies] 为空，
│                  tests/dependency_firewall.rs 强制 std-only)
│   ComponentSpec { name, requires, provides, activate, teardown }
│       component.rs:82-88；builder: .requires/.provides/.on_activate/.on_teardown
│   DesiredEntry { id, component, revision, enabled } — 平面 BTreeMap
│       desired.rs:60-65
│   step() 固定优先级 reconcile（unload→divert→retire→remove→mount→activate）
│       kernel.rs:145-188；Fiber 5 态 Pending/Activating/Active/Unloading/Failed
│       fiber.rs:25-31（TEARDOWN_VIOLATED 是 latch 标志，非第八态）
│   cardinality: required-single 全局唯一模式；≥2 个 enabled provider 对同一 key
│       ⇒ plan 时 CompositionError::AmbiguousProvider，整个 desired map 被拒
│       （kernel.rs:373-394；oracle 测试 capability_oracles.rs:186-213）
│   ActivationCtx: resolve/provide/register_effect/register_relation/dispose
│       context.rs:69-74；TeardownCtx 仅 resolve_committed（B14 窗口）
│       context.rs:191-194
│   Effect: 单一形状（total inverse），LIFO unwind，Discharge 二值判决，
│       Violated ⇒ latch 且阻塞 provider final release   kernel.rs:702-742
│   撤退 = 重发 desired map；替换 = staged retire→remove→mount（§E.4）
▼
Fibers（当前全部三个 plugin 走同一 substrate）:
    songcore_decode_plugin  provides PcmDecodeCapability, requires ∅
                            (crates/qianqian-decode-songcore/src/lib.rs:376-383)
    output_plugin           provides AudioOutputCapability, requires ∅
                            backend 由 crate 私有 selected_backend() 的 cfg(windows)
                            工厂体选择（output-wasapi/src/lib.rs:62-89；PBK-003 §11）
    playback_session_spec(file, handle)
                            requires Decode+Output, provides ∅
                            (crates/qianqian-playback/src/session.rs:50-55)
    │
==== DATA PLANE（K0 每 quantum 零参与）==============================
    │ THREAD "qianqian-decode"（session.rs:159-169）
    │   decode_stream.read_frames(1024-frame staging)      session.rs:433
    │   write_observing_seek → edge.write_some             session.rs:517-558
    ▼
PcmEdge: Mutex+双 Condvar 预分配 f32 ring，8192 frames ≈185ms，
    稳态零分配；terminal 四态 first-wins                    edge.rs:44-56,189-200
    │ RenderPcmInput::read_frames（PULL；dst 直接是 WASAPI GetBuffer 指针）
    ▼
THREAD "qianqian-wasapi-render"（wasapi.rs:132-143）
    loop-top RenderGate（pause/seek park，无设备缓冲跨 park）
    → OutputLevel → IAudioStreamVolume::SetAllVolumes（OS 侧，非 PCM 乘法）
    → GetBuffer → read_frames → ReleaseBuffer → PositionEvidence 单调发布
    ▼
Windows shared-mode engine（Tier-2 AUTOCONVERTPCM SRC 仅在源格式被拒时）
```

关键事实：

- **今天全链路没有任何 Rust 侧 DSP**；唯一样本转换在 native `frame_to_f32`（`songcore_ffmpeg.c`）。
- 音量是 OS 侧 `IAudioStreamVolume`（D14.9 + VOLUME-TAPER-1）；D14.9 明确把"软件 PCM 乘法"排除在 volume 机制之外（记录为无 stream-local 控制的平台的 portable fallback）。
- K0 对 PCM/格式/位置/播放语义零知识（`J.3` 类防火墙在代码中成立）；UI 只经 `PlaybackSessionHandle` seam。
- ReplayGain 元数据字段已存在于 SongCore ABI（`track_gain_mb`/`track_peak`，`qianqian-songcore-sys/src/lib.rs:144-151`），Rust 侧无人读取。
- `docs/audits/plugin-boundary-conformance-audit.md`（EVIDENCE，BASE `c92fd64`）：生产路径 0 边界违规；7 个 latent bypass 面（L1–L7）仅靠 review 纪律维持；加固方案已批准、**尚未实施**。

## 3. DeepSeek Harness / Cordis comparison

依据：K0 设计 §C（本仓对 Cordis 的一手对照）+ 本地 clone（`vendor/README.md`）+ 官方 docs。

| Concern | DeepSeek Harness / Cordis | Current Qianqian K0 | Relevant lesson |
|---|---|---|---|
| plugin admission | loader 插件 + 声明式 entry tree + HMR | 内存 ComponentSpec 注册；无动态加载/HMR（K0 §C.2 明确拒绝） | 静态准入是刻意的；DSP 不引入 loader 机制 |
| lifecycle | Cordis Fiber（dispose/restart/update waterfall） | Fiber 概念直接源自 Cordis 映射，但收敛为同步串行控制面 + 单次有界激活 + §G.6 violation latch | K0 已吸收可用教训且更严格 |
| service 定义 | `ctx.<key>` 字符串/symbol 槽位 | typed Capability key + `type Service`（TypeId 身份） | typed key 更强；未来若有 DSP service 应保持此形 |
| provider/consumer | 每 scope 每 key 单 provider（重复 provide 抛错）；`ctx.isolate` 作用域覆盖 | required-single + staged replacement；无 realms | Cordis 同样禁止同 key 多 provider——多实现走 service 内部 registry，与 K0 结论同构 |
| reversible registration | `ctx.effect` + disposer，LIFO，随 fiber 自动拆除 | Effect + total inverse + LIFO accumulator + Discharge 判决 | 未来任何贡献者 registry 都应走 register_effect 形状 |
| multiple contributors | registry service 模式（`ctx.tools.register`） | §H.4 同 key 贡献契约：仅**交换**关系可 entry-per-registration；DSP 链是非交换的 | registry 模式合法但不能决定 DSP 顺序 |
| runtime data plane | typed events（emit/waterfall/parallel/serial/bail）+ 直接 service 调用；无任何实时约束文档 | PCM 永不进 composition/event 系统；冻结 firewall（K0 §N.2） | DSH 无 RT 纪律可抄；其 waterfall 恰是 per-block 禁止项。冲突时 Qianqian 赢 |

结论：Cordis 是 K0 的上游参照而非对手。它对 DSP 的唯一可借鉴物是 registry-inside-service 模式——而该模式在 Qianqian 语境下只能承载交换性贡献（观察者/监听者），不能承载有序 DSP 链。

## 4. 四分法（Plugin / Capability / Factory / Processor instance）

任务书 §4 的区分在概念上正确，审计结论是：

```text
Plugin identity            今日对 DSP 不挣得（D13 三条件全不成立，§5）
Capability/service contract  今日对 DSP 不挣得（无提供/消费压力；且 required
                             require 会使处理成为强制组合真值，与"无处理=行为
                             不变"语义相悖——K0 无 optional 模式）
Factory                    合适，但仅在 per-episode instance 创建层，且是普通
                           owned Rust mechanism，不是架构名词
Processor instance         episode-scoped、format-bound 的 owned resource
                           （session 拥有，与 PcmEdge 同类）
```

不要把 Factory 升格为 universal Plugin 抽象；也不要为了"稳定边界"的直觉提前把前两层铸成型。

## 5. K0 cardinality 与候选模型比较

**required-single 已由三重来源证实**：K0 设计 §E.2 + §R 预算（capability cardinality modes = 1）+ 实现代码（`capability.rs:12-13` 注释、`kernel.rs:373-394` plan 时 `AmbiguousProvider` 拒绝整个 desired map、oracle 测试）。因此候选图中的

```text
Gain Plugin ────────┐
EQ Plugin ──────────┼── provides AudioEffectCapability
Compressor Plugin ──┘
```

**不是"可能有问题"，而是 plan 时被确定性拒绝**——desired map 整体被拒，什么都不会 mount（非 last-wins、非事后错误）。

| Model | K0 fit | D13 fit | multi-effect composition | realtime safety | complexity | verdict |
|---|---|---|---|---|---|---|
| **A** 每 effect 一个 capability provider | **FAIL**：required-single plan 时拒绝；解法需新增 cardinality 模式（K0 §S 明确 defer，预算=1） | **FAIL**（effect 无独立 composition identity） | 模糊 + 无序（§H.4：DSP 非交换） | n/a | 代码低/权威代价极高 | **REJECTED**（权威+代码双重证明） |
| **B** 一个 Audio Processing Plugin 拥有全部 effects | 佳（一个 spec/service） | **今日未挣得**：链生命周期 ≡ episode 生命周期，无独立 K0 协调压力；D13 ⇒ owned resource | 佳（内部有序链） | 佳 | 中（新 Plugin+Capability+required-mandatory 问题） | **DEFERRED**：未来凭真实 D13 证据（如跨 crate 贡献者/部署级替换压力）再挣 |
| **C** provider + 可逆贡献者 registry | 机械上可表达（register_effect 反操作即注销） | 贡献者 Plugins 同样未挣得（同一 D13 缺口）；registry 模式对交换贡献合法（§H.4），DSP 顺序仍需外部显式模型 | 需要额外的显式顺序模型 | 佳（若 registry 在 RT 前冻结） | 最高（registry+撤退+顺序三者交互） | **DEFERRED**：其*机制*（内部 factory registry）可在未来 B/D 内无 Plugin 化采用 |
| **D**（修正后 D+）Session 拥有 chain；audio-api 定义 processor contract；App 拥有链配置 | 完美（K0 零改动） | **PASS**——恰是 D13 强制的形态 | 佳（App-owned 有序配置；显式顺序 owner） | **最佳**（RT render thread 完全不动） | 最低 | **SELECTED** |

## 6. Preferred architecture（Model D+）

```text
                    Composition Kernel K0（不变）
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
   Decode Plugin      Output Plugin    Playback Session Plugin
        │                  │                  │ requires Decode+Output
        │                  │                  │ + 激活参数：App-owned 处理配置
        │                  │                  ▼
        │                  │         ProcessorChain（episode-scoped owned
        │                  │           resource，与 PcmEdge 同类；
        │                  │           激活时按已知 PcmFormat 构造）
        │                  │                  │
        └──► decode worker staging loop ──►►│──► PcmEdge ──► RenderPcmInput ──► Output
             （read_frames 与 edge.write_some 之间的唯一处理缝）

    App（进程级 host，episode 之外）
        │ 拥有：ordered chain 配置（如 [Gain(0.8), EQ(preset)]）
        ▼ 经 episode 构造参数（= file/handle 同一惯例，D14.6）
          + 幂等 seam command（D14.9 request_output_level 同构惯例）

    qianqian-audio-api（contracts crate，D7）
        │ 定义：processor contract trait（创建-绑定格式 / process / reset）
        ▼ —— 这是稳定的 DSP seam，与 PcmDecode/RenderPcmInput 同层、同性质
```

候选图中的 `Audio Processing Plugin + AudioProcessingCapability` 方块被移除：它们当前不为 D13 所挣；被一个 contracts-crate trait + session-owned chain + App-owned config 替代。日后若证据挣得 family Plugin，提升路径是纯增量的（session 改为从 resolved service 构造 chain），本次接口应预留该形状。

## 7. Taxonomy table

| Candidate | Classification | 依据 |
|---|---|---|
| Audio Processing（特性域） | application feature（**当前未授权，需修正案**） | U2 禁入清单 + PBK-002 §14 OPEN |
| "Audio Processing Plugin" | **非 Plugin**（今日）；未来仅凭新 D13 证据 | D13 三条件全部不成立（§5 表） |
| "AudioProcessingCapability" | **非 Capability**（今日） | D14.10；无提供/消费压力 |
| Gain / EQ / Compressor / Limiter / ReplayGain | App-configuration 选择的 **processor 模块**（owned code）；非 Plugin/Capability | D13 negative oracle；U2 先例（SeekPlugin/VolumePlugin 类拒绝） |
| ReplayGain | Gain 的一个配置来源（ABI 已有 track_gain_mb/track_peak，未读取）+ 普通 processor；非新架构 | 代码事实 songcore-sys:144-151 |
| Biquad / FFT window / delay line | processor instance 内的普通 Rust 字段（owned state） | "explanatory vocabulary is not architecture vocabulary" |
| ProcessorChain | **session-owned episode resource**（与 PcmEdge/worker 同类） | D6 所有权二分 |
| Processor instance | episode-scoped、format-bound、session 激活时创建的 owned resource | D6；pcm-contract-a0 #4 format stability |
| parameter state | session-owned cell（OutputLevel cell 同构模式），App 拥有 desired 值 | D14.9 先例 |
| preset | App-owned 配置数据（下一 episode 生效）；非架构名词 | 同 playlist/volume 惯例 |
| chain 构造 factory | 普通 owned mechanism（Rust 构造函数），非架构级 Abstract Factory | razor rule 1–3 全部 YES |

## 8. PCM placement decision

**选定：decode worker staging loop**（`session.rs:433-459`，`read_frames` 与 `edge.write_some` 之间；即 Phase C 已证形状 `source → stage → sink` 的 stage 位）。

| 候选位置 | 评估 | 裁决 |
|---|---|---|
| **decode worker staging loop** | 离 RT 线程；worker 已拥有格式与 1024-frame 稳定块；seek reset 挂在既有 `Applied` 臂（与 `edge.invalidate()` 同一序列点）；失败直接走既有 decode-failure→D11 路径；backpressure 预算 8192-frame edge（≈185ms）；position 核算在 render leg 完全不受影响；**PcmEdge/RenderRequest/PBK-003 契约零改动** | **SELECTED** |
| RenderPcmInput decorator / PcmEdge 内 | 落在 WASAPI render 线程（RT 关键路径）；与 park/quiescence/position 证据逻辑纠缠；D14.9 已以"永久 per-sample RT 税"为由否决软件 PCM 乘法做 volume——同一论证 | REJECTED |
| Output Plugin / backend 内 | 违反 PBK-003（backend 是平台 mechanism；DSP 语义不是 backend 语义；可移植性丢失）；假设 H4 被权威直接反驳 | REJECTED |
| native songcore C 层 | DSP 离开 Rust oracle 体系、ABI churn、native 库耦合 episode DSP 状态 | REJECTED |
| 独立处理 worker/线程 | 第三线程 + 第二条 edge；decode worker 本身就是合适的处理线程；最小机制拒绝 | REJECTED |
| PcmEdge 内部 | 破坏 edge 单一职责（transport + terminal 状态机；seek/pause/EOF 协议承重墙） | REJECTED |

## 9. Realtime contract

- **RT render thread 的冻结 firewall 行不变**——本设计使其完全不被触碰。
- **worker 稳态处理路径**（每 staging block）禁止新增：K0 任何操作、Capability resolution、registry lookup、图变更、plugin discovery、通用 event fan-out、文件/网络/UI I/O、无界分配/阻塞、无界锁。允许：对预绑定 staging buffer 的原位数学、预分配状态、块边界处的参数 cell 读取（OutputLevel 模式：一次 load+compare）。
- 控制面/数据面分离：**配置 = control-plane**（App → seam command → session-owned cell → worker 块边界生效）；**处理 = data-plane**。参数变更永不触发 composition mutation（PBK-001 §7 冻结条款）；链成员/顺序变更仅下一 episode 生效。
- 不发明比现有证据更严的约束：worker 现状本就有 2ms 有界等待与 seek-slot try_lock；本契约只要求"不新增"。

## 10. Lifecycle contract

| 阶段 | 冻结的最小语义 |
|---|---|
| create | session 激活内、`format()` 已知后构造；**一个 instance 绑定一个 PcmFormat**（native 中流格式变更已 fail-closed，episode 失败） |
| process | 每 staging block 原位处理；稳态零分配 |
| parameter update | 幂等 seam command → cell → 块边界生效；参数变化永不重置状态（除非语义明示） |
| seek/reset | 在 `Applied` 臂、与 `edge.invalidate()` 同一序列点执行 **reset()**（滤波器延迟态、包络、limiter 历史、delay line 全部归零）；`RefusedUnchanged` 路径**禁止** reset（D14.5 零内容损失不变量）；不需要 `flush()`——**v1 禁止 look-ahead 类加延迟处理器**（其 flush/drain 语义是新权威） |
| pause/resume | 零交互（edge 满则 worker 自然阻塞）；pause **不得**重置链状态 |
| stop/EOF | 链随 worker closure 丢弃；LIFO teardown 顺序不变；EOF 尾块不得注入静音（frame conservation MUST，pcm-contract-a0） |
| failure | 处理器失败 ⇒ 走既有 decode-failure 路径 ⇒ D11 `Failed`。**bypass-降级模式未授权**（类比 D14.5 "不可证明即破坏性"：失败处理器的输出不可信；恢复语义 = authority gap） |
| teardown/replacement | episode-scoped；配置替换 = D14.6 whole-episode replacement |

## 11. Ordering / configuration ownership（冻结最小规则）

```text
顺序真值 = App-owned 有序链配置（episode 构造参数），session 激活时消费，
          单 episode 内稳定。
顺序永不来自：K0 topology / mount order / registration order / hash 迭代
（PBK-001 §5 冻结 + K0 §H.4 明文点名 DSP 非交换）。
v1 无 slot 争用、无 registry、无环问题（有序列表结构上不可环）。
enable/disable = 配置语义；mid-episode 重排 = 不授权。
```

## 12. Authority differential（实现前必须变更的权威）

**必需：选项 B —— 对 `ADR-PBK-002` 的一条窄修正案（建议 D14.11 "Audio Processing minimum"），遵循 D14.5/7/8/9 的 gate→amendment→implementation 既有载体。**

```text
文档:        docs/adr/ADR-PBK-002.md（§14 + §20 新增 D14.11）
变更命题:
  1. 授权 Audio Processing 进入产品范围（显式取代 U2 清单中对本阶段的
     DSP/EQ 禁入——不改写历史文本，同 U2 的 scope-freeze 手法）；
  2. 冻结最小边界：session-owned episode chain / audio-api processor
     contract / App-owned 有序配置 / worker 插入点 / §9–§11 契约；
  3. 记录 D13 负裁决：per-effect Plugins、per-effect Capabilities、
     family Processing Plugin 今日均不挣得（negative oracle 入档）；
     保留未来 family Plugin/Capability 的明确再挣条件与增量提升路径；
  4. 重申 volume（D14.9）与 processing gain 的永久区分。
不变:         D11 终局权威、F3/F4/F5/F6、D14.9 volume 机制、P1–P5、
              K0 五原语与预算、PCM firewall（全部行）、D6 所有权二分、
              PBK-003 Output 边界。
```

无需新 ADR（选项 C）：DSP 是 PBK-001 §10/§12 早已预留的 Issue #12 领域，D14 系列是该类冻结的既定载体；也远不到 broaden reopen（选项 D）。

## 13. Gain architectural probe plan（authority freeze 之后的第一个实现探针）

**必证门（全部通过才允许 EQ）：**

```text
G1 unity 透明:       gain=1.0 时输出与 bypass 位相等（IEEE f32 ×1.0 精确，
                     断言 bit-equality）
G2 期望增益:         固定因子输出 == 参考乘法（精确 f32 语义）
G3 seek reset 排序:  reset() 调用与 edge.invalidate() 的序列点被 pin（白盒）；
                     refusal 路径零 reset（零内容损失负控）
G4 零稳态分配/无新增锁: counting-allocator oracle（direct-pcm-flow 同款技术）
G5 零 K0 操作/块:    debug_op_count witness 增量为 0
G6 pause/EOF/stop 不变性: 既有 seek 矩阵 + transport 场景全绿不回归
G7 失败路由:         注入失败处理器 ⇒ D11 Failed（非 bypass、非静音）
G8 D14.9 区分:       OS volume 与 processing gain 独立可变、互不写入
G9 物理冒烟:         Windows 实机有声 + position 证据不受影响
                     （worker 侧处理不触 render 核算）

EQ 附加门（对应 H8）: 有状态 reset 等价（seek-reset 输出 == 新建链同输入输出）、
                     参数更新 zipper 语义、多通道状态数组、
                     坏系数 NaN/Inf 传播的 fail 行为
```

实现体量：audio-api 一个 trait + playback 内 Gain 处理器 + session 激活/worker 循环两处小改 + App 配置入口。不新增 crate 级架构名词；若需要代码组织可加普通 library crate（crate ≠ Plugin）。探针本身若以 evidence 形态先行（`experiments/`，可弃置），遵循 §16 任务纪律：不提交、不留脏 worktree。

## 14. Adversarial checks（H1–H8）

| # | 假设 | 裁决 | 依据 |
|---|---|---|---|
| H1 | "需要 universal Plugin trait / abstract factory" | **FALSE** | K0 ComponentSpec 是唯一 substrate；factory 仅在 instance 创建层、作为普通 owned mechanism。四分法中仅后两层存在 |
| H2 | "每个 effect 应是自己的 Capability provider" | **FALSE（已证）** | required-single：plan 时 `AmbiguousProvider` 拒绝（design §E.2 + 代码 kernel.rs:373-394 + oracle 测试） |
| H3 | "每个 effect 应是自己的 Plugin" | **FALSE** | D13 三条件全不成立 + U2 先例（feature-shaped Plugin 拒绝）+ K0 §H.4（非交换关系需要显式有序结构，独立组合恰好不提供） |
| H4 | "DSP 应放进 Output（样本在此被消费）" | **FALSE** | PBK-003（backend = 平台 mechanism，DSP 语义不是 backend 语义；可移植性）+ per-sample 数学落 RT render 线程（D14.9 已以 RT 税为由否决同类做法） |
| H5 | "DSP 顺序可沿 K0 组合顺序" | **FALSE（权威明文）** | PBK-001 §5（dependency topology != realtime processing topology；顺序禁止来自 mount/registration/迭代顺序）+ K0 §H.4/§H.6（DSP 被点名非交换；顺序属于显式有序拓扑，归显式 owner） |
| H6 | "动态参数变更需要 K0 操作" | **FALSE** | PBK-001 §7（cheap realtime-safe parameter update 不得强制走 composition mutation）+ OutputLevel cell 先例（relaxed load+compare） |
| H7 | "processor instance 可跨 seek 原样存活" | **FALSE（反例成立）** | biquad IIR 延迟态、compressor 包络、limiter 历史、reverb delay line 均为有状态记忆；提交性 seek 后必须 reset（stale-DSP-state 是 D14.5 stale-PCM 不变量的推广）；refusal 路径禁止 reset |
| H8 | "Gain 通过即证明 EQ 架构" | **FALSE** | Gain 是无记忆变换；EQ 首次暴露：format-bound 有状态滤波器、reset 正确性（错误时可听：滤波瞬态/咔哒声）、参数更新 zipper 语义、多通道状态、坏系数 NaN 传播。EQ 需要 §13 的独立有状态门 |

## 15. Non-UI Core Closure matrix

| 边界 | 状态 | 依据 |
|---|---|---|
| source/media（Open/probe/folder/playlist/navigation） | **CLOSED** | F6 + U1 + U2（D14.6） |
| decode | **CLOSED** | SongCore + PcmDecodeCapability |
| playback session | **CLOSED** | episode Plugin + D11/D14.2 |
| PCM transport | **CLOSED** | PcmEdge + pcm-contract-a0 + Phase C 证据 |
| output（Windows WASAPI） | **CLOSED** | PBK-003；第二 backend 属机制/可移植性工作 |
| pause/resume | **CLOSED** | D14.7（PR #150） |
| seek | **CLOSED** | D14.5 + 实现 corrective-4（M1–M11 全 11/11） |
| position/duration | **CLOSED**（实现落地，PR 级 human review 进行中——过程项，非架构缺口） | D14.8 |
| volume | **CLOSED** | D14.9 + V-PROBE + VOLUME-TAPER-1 |
| error routing / shutdown cleanup | **CLOSED** | D11 优先级 + Discharge/fail-stop |
| plugin lifecycle（语义） | **CLOSED** | K0 + conformance audit 0 违规 |
| plugin lifecycle（机器强制加固） | **PARTIAL** | 加固方案已批准、施工未做（L1–L7 bypass 面仍在） |
| **audio processing（含 ReplayGain 应用）** | **OPEN** | 本审计对象；**最后一个未关闭的非 UI 播放域主边界** |
| device switch / format switch / 多设备 | **OPEN**（v1 fail-closed，非 closure 阻塞项） | PBK-002 §14 |
| analysis/spectrum、lyrics | **OUT_OF_SCOPE** | 无需求证据；lyrics 在 U2 未进入持久化族 |

**结论：不得宣告 `QIANQIAN-NON-UI-CORE-CLOSED`。** 剩余阻塞 = audio processing（本审计链）+ 加固施工（`QIANQIAN-PLUGIN-BOUNDARY-HARDENING-1`）+（视定义）device-switch。DSP 落地且加固完成后，closure 判定才有依据。

## 16. Risks / unknowns

```text
architecture:  family Processing Plugin/Capability 是否终有一日被挣得（开放，
               已留增量提升路径）; mid-stream format switch（现为 fail-closed）
               与未来 look-ahead 处理器的 flush 语义（新权威）;
               device-switch 与 chain 的交互（后者不改变前者仍 OPEN 的事实）。
implementation: SIMD/FTZ/denormal 策略; 参数平滑（新状态 → reset 契约要覆盖）;
               低端机 CPU 预算; worker 侧处理耗时挤占 1024-frame 产出节奏的
               极端情形。
physical/audio: reset 不完全时的可听瞬态; ReplayGain 元数据准确度;
               未来 look-ahead 延迟的可听性。
（physical 未知项不得以软件断言冒充——AGENTS.md "Verification" 条款适用。）
```

## 17. Recommended next task

**`QIANQIAN-AUDIO-PROCESSING-AUTHORITY-FREEZE`**

（修正案为必需，故非 `QIANQIAN-AUDIO-PROCESSING-GAIN-PROBE`。该任务 = 依 §12 起草 D14.11 窄修正案 + 以 §13 的 G1–G8 可弃置探针作为 grounding evidence（F5-GATE/V-PROBE 先例），不合并生产实现。）

---

## Verdict block

```text
LIVE_MAIN       = ba545ee5927e1c19963dee85922569668a3fa023
BASE_SHA        = ba545ee5927e1c19963dee85922569668a3fa023
                  （审计起点 63b4d51，按任务指示 fast-forward 7 commits 后采集全部证据）
FINAL_WORKTREE  = clean（审计期间 git status --porcelain 为空；零文件改动、零提交、零 PR）
VERDICT         = GO_WITH_CORRECTIVES
PREFERRED_MODEL = Model D+（修正后的 D）：session-owned episode ProcessorChain
                  + audio-api processor contract + App-owned 有序配置
                  + decode-worker staging-loop 插入点；
                  无新 Plugin、无新 Capability、无新 plugin 抽象
AUTHORITY_ACTION= B —— ADR-PBK-002 窄修正案（D14.11）：授权 DSP 特性域、
                  冻结最小边界、记录 D13 负裁决；
                  不改 D11/F3/F4/F5/F6/D14.9/P1–P5/K0/PBK-003
NEXT_TASK       = QIANQIAN-AUDIO-PROCESSING-AUTHORITY-FREEZE
```
