# Audio Processing Plugin architecture audit

> ## 审计最终结论
>
> - **判决：`GO_WITH_CORRECTIVES`。** Audio Processing 可以在现有 K0 架构内承载，且不需要候选任务书中的 "Audio Processing Plugin + AudioProcessingCapability" 形状。
> - **统一 Plugin substrate 已存在且唯一**（`ComponentSpec → K0 admission → Fiber`）；本审计评估的是 "Audio Processing 是否挣得 Plugin/Capability 身份" 这一假设，结论是**驳回当前准入**——是 "今日不挣得"，不是 "永远不可能"（可证伪再挣条件见 §5）。当前证据下 **Audio Processing 既不挣得 Plugin 身份，也不挣得 Capability**；驳回理由是 **D13 三条件今日全部不成立，而不是 K0 无法表达**：required-single 基数在 plan 时确定性拒绝的只是 "同一 capability 有 ≥2 个 enabled provider"（即 "每 effect 一个 provider"）的形状；一个 family 级单例 provider（disabled 时合法产出透明/旁路处理）在 K0 中完全可表达（§4/§5 窄化）。
> - **首选架构（Model D+，修正后）**：Playback Session 将 ProcessorChain 作为又一个 episode-scoped owned resource（与 PcmEdge/decode worker 同类）；App 拥有有序链配置（App-owned product configuration，同 D14.9 volume / U2 playlist 惯例）；**冻结的是处理语义形状**（session 拥有一条 episode 绑定的处理链；链在解码后、进入 PcmEdge 前接收 source-format PCM——**当前最小执行位置**；processor 实例 episode 绑定、格式绑定；processor 必须满足冻结的处理/生命周期语义），**而非任何具体 Rust trait / trait 可见性 / crate 位置 / factory 表示 / 动静态派发——representation 保持 OPEN**（权威应冻结处理语义契约，不是某个 Rust trait 或 crate 放置；首个 Gain 实现 MAY 保持 crate-private 于 `qianqian-playback`；只有当出现多个 owner/provider/实现 crate 或其他真实共享契约消费者时，promote 进 `qianqian-audio-api` 才被挣得）；PCM 插入点 = decode worker staging loop（RT render thread 完全不动；**当前最小执行位置**，executor ≠ owner，§8 四分）；当前最小切片有界（source-rate/layout/frame-count preserving、bounded causal、no mandatory pending output at EOF，§8）；instance 创建是普通 owned mechanism（Abstract Factory 不挣得架构地位；确切构造形态属 representation）。
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
| Review outcome | 独立对抗复审历史（全部为独立 agent 审计；**Human adversarial review：本记录不作主张**）：<br>① **Independent adversarial review #1 — CHANGE_REQUIRED**（单轮 agent 独立审计，adversarial posture，H1–H8 全部执行；findings：representation 过度冻结、closure 过度主张等）→ Corrective pass #1 已应用（P1-1/P1-2/P2-1/P2-2 + 有状态探针门 + freeze scope 收窄 + closure scoped claim，commit `42cc993`）。<br>② **Independent adversarial review #2 — CHANGE_REQUIRED**（architecture model 本身 **PASS**；residuals：stale PR body、通用参数生效时机过度冻结、残留 in-place/reset 表示措辞、CI 状态未解决）→ Corrective pass #2（PR #176，commit `f023639`）逐项关闭上述 residuals。<br>③ **Independent review #3 — PASS_WITH_CORRECTIVES**（P0=0 P1=0 **P2=4** P3=1；架构裁决维持——CURRENT_ARCHITECTURE_RULING=B、DSP_PLUGIN=NOT_EARNED_NOW、AUDIO_PROCESSING_CAPABILITY=NOT_EARNED、PUBLIC_PROCESSOR_CONTRACT=REPRESENTATION_OPEN、PR_176_ARCHITECTURE=PASS_WITH_CORRECTIVES；correctives：F1 required-single 论证对 family 单例 provider 过宽、F2 "顺序永不来自组合拓扑"过广、F3 owner/executor/backend/contract 混同与"零输出契约改动"未按最小切片限定、F4 参数/配置变更被读作自动 whole-episode 替换、F5 PR 文件记账缺两个 test-only fmt 文件）→ **Corrective pass #3（2026-10-02，本 pass）已应用**：F1–F4 修正论证而不改结论（§4/§5 窄化 + 再挣条件、§11 三分、§8 四分 + 最小切片 + SongCore/FFmpeg 水位线、§9/§10/§12 参数四分）；F5 走保留 + PR body 如实记账选项。<br>④ **merge 前仍需第四轮独立复审**——复审前本记录不得作为实现授权 |

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
    Model D+（见 §6）：session-owned chain + 语义冻结的处理契约
    （representation OPEN）+ App-owned 有序配置 + decode-worker
    插入点（当前最小执行位置，executor ≠ owner，§8 四分）。
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
| multiple contributors | registry service 模式（`ctx.tools.register`） | §H.4 同 key 贡献契约：仅**交换**关系可 entry-per-registration；DSP 链是非交换的 | registry 模式合法但 registry/目录顺序不能决定 DSP 执行链顺序（目录 ≠ 执行链，见 §11） |
| runtime data plane | typed events（emit/waterfall/parallel/serial/bail）+ 直接 service 调用；无任何实时约束文档 | PCM 永不进 composition/event 系统；冻结 firewall（K0 §N.2） | DSH 无 RT 纪律可抄；其 waterfall 恰是 per-block 禁止项。冲突时 Qianqian 赢 |

结论：Cordis 是 K0 的上游参照而非对手。它对 DSP 的唯一可借鉴物是 registry-inside-service 模式——而该模式在 Qianqian 语境下只能承载交换性贡献（观察者/监听者），不能承载有序 DSP 链。

## 4. 四分法（Plugin / Capability / Factory / Processor instance）

任务书 §4 的区分在概念上正确，审计结论是：

```text
Plugin identity            今日对 DSP 不挣得（D13 三条件全不成立，§5）
Capability/service contract  今日对 DSP 不挣得——理由是 D13 层面的
                             （无提供/消费压力），**不是 K0 表达力理由**。
                             两点窄化（F1 corrective）：
                             (a) required-single 拒绝的是 "同一 key ≥2 个
                             enabled provider"（Model A），不是单例
                             provider（Model B）——后者 K0 完全可表达；
                             (b) "DSP disabled" 不蕴含 provider 必须消失：
                             常驻单例 provider 合法产出透明/空/旁路处理。
                             required 语义与 "无处理=行为不变" 的冲突仅存在于
                             "以 provider 缺席表达 off" 的表示（0 provider ⇒
                             消费者永远 Pending，E.2）；常驻透明 provider
                             不冲突
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

**不是"可能有问题"，而是 plan 时被确定性拒绝**——desired map 整体被拒，什么都不会 mount（非 last-wins、非事后错误）。该拒绝只覆盖 Model A 的 "同一 key 多 enabled provider" 形状，**不构成对 family 单例 provider（Model B）的驳回**——后者 K0 完全可表达，其驳回依据只是 D13（见表 B 行与表后窄化块）。

| Model | K0 fit | D13 fit | multi-effect composition | realtime safety | complexity | verdict |
|---|---|---|---|---|---|---|
| **A** 每 effect 一个 capability provider | **FAIL**：required-single plan 时拒绝；解法需新增 cardinality 模式（K0 §S 明确 defer，预算=1） | **FAIL**（effect 无独立 composition identity） | 模糊 + 无序（§H.4：DSP 非交换） | n/a | 代码低/权威代价极高 | **REJECTED**（权威+代码双重证明） |
| **B** 一个 Audio Processing Plugin 拥有全部 effects | **完全可表达**（单例 spec/service；disabled 由常驻透明/旁路 provider 表达，provider 无须消失） | **今日未挣得**：链生命周期 ≡ episode 生命周期，无独立 K0 协调压力；D13 ⇒ owned resource | 佳（内部有序链） | 佳 | 中（真实代价 = 多一个组合身份/可达性/撤退边界；required-mandatory 有常驻透明 provider 解法，非阻断） | **DEFERRED**：K0 可表达但 D13 今日不挣；未来凭真实 D13 证据（再挣条件见下）再挣 |
| **C** provider + 可逆贡献者 registry | 机械上可表达（register_effect 反操作即注销） | 贡献者 Plugins 同样未挣得（同一 D13 缺口）；registry 模式对交换贡献合法（§H.4），DSP 顺序仍需外部显式模型 | 需要额外的显式顺序模型 | 佳（若 registry 在 RT 前冻结） | 最高（registry+撤退+顺序三者交互） | **DEFERRED**：其*机制*（内部 factory registry）可在未来 B/D 内无 Plugin 化采用 |
| **D**（修正后 D+）Session 拥有 chain；处理契约语义冻结（representation OPEN）；App 拥有链配置 | 完美（K0 零改动） | **PASS**——恰是 D13 强制的形态 | 佳（App-owned 有序配置；显式顺序 owner） | **最佳**（RT render thread 完全不动） | 最低 | **SELECTED** |

**required-single 论证的适用范围（F1 corrective 窄化）：**

```text
A. 同一 required-single capability 有 ≥2 个 enabled provider
   （"每 effect 一个 provider"，Model A）
   ⇒ K0 plan 时 AmbiguousProvider，整个 desired map 被拒
   ⇒ 该论证对 Model A 成立（权威 + 代码双重证明）

B. 一个 family 级单例 processing provider
   （一个 Plugin 提供一个 AudioProcessingCapability，Model B）
   ⇒ K0 完全可表达；"DSP disabled" 也不要求 provider 消失
     （常驻 provider 合法产出透明/空/旁路处理）
   ⇒ 今日驳回的唯一理由是 D13：当前需求不要求 K0 额外持有
     任何依赖/可达性/撤退顺序——不是 "K0 无法表达"

正确的驳回论证是：

    not:   K0 cannot express it
    but:   当前需求不需要 K0 为它持有任何额外的
           组合身份 / 依赖 / 可达性 / 撤退排序
```

**再挣条件（可证伪示例——非永久排除）：**

```text
一个可独立撤退的 DSP provider D：
    - 被多个独立组合的 K0 消费者使用；
    - D 的存在/替换本身是 desired-composition 真值；
    - D 的撤退必须在 provider 资源释放前协调这些消费者；
    - 把 D 藏进任一单个消费者会丢失真实依赖，或迫使
      App 手工复现 K0 依赖/生命周期排序。

只有此时，AudioProcessingPlugin（以及可能的
AudioProcessingCapability）才成为认真的准入候选。

仅以下各项 alone 不构成提升证据：
    多算法 / 多 crate / 多 processor 实现 / 大状态 /
    convolution / FFmpeg backend / 硬件实现 / presets / 第三方代码
```


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
             （read_frames 与 edge.write_some 之间的处理缝——
               **当前最小执行位置**；executor ≠ owner，见 §8 四分）

    App（进程级 host，episode 之外）
        │ 拥有：ordered chain 配置（如 [Gain(0.8), EQ(preset)]）
        ▼ 经 episode 构造参数（= file/handle 同一惯例，D14.6）
          （若某 effect 需要运行期更新：该 effect 自己的更新入口；
            通用 publication 机制 OPEN，见 §9 冻结/OPEN 划分）

    处理语义契约（冻结的是语义形状，不是 Rust 接口）
        │ session 拥有一条 episode 绑定的处理链（**语义 owner**）；链在
        │ 解码后、进入 PcmEdge 前接收 source-format PCM（**当前最小
        │ 执行位置**——可证伪的执行选择，非永久法律；未来执行位置
        │ 被挣得时不必然改变所有权或 Plugin 准入，见 §8 四分）；
        │ processor 实例 episode 绑定、格式绑定；必须满足 §9–§11
        │ 处理/生命周期语义
        ▼
        REPRESENTATION OPEN —— exact Rust trait / trait 可见性 /
        crate 位置（含是否放入 `qianqian-audio-api`）/ factory 表示 /
        动静态派发 / 是否存在 trait / enum vs trait object 表示 /
        处理缓冲表示（in-place / out-of-place / scratch / SIMD 布局 /
        缓冲所有权）/ 超出所选最小值的线程拓扑 / 是否集成 libavfilter
        （Qianqian 侧实现机制选择，见 §8 水位线），全部开放。
        冻结的只有：解码后 source-format PCM 在进入 PcmEdge 前被变换。
        首个 Gain 实现 MAY 保持 crate-private 于 `qianqian-playback`。
        promote 进 `qianqian-audio-api`（contracts crate，D7）仅在
        出现多个 owner/provider/实现 crate 或其他真实共享契约
        消费者时被挣得。
```

候选图中的 `Audio Processing Plugin + AudioProcessingCapability` 方块被移除：它们当前不为 D13 所挣；被 "语义冻结的处理契约（representation OPEN）" + session-owned chain + App-owned config 替代。日后若证据挣得 family Plugin，提升路径是纯增量的（session 改为从 resolved service 构造 chain）；本审计不为该未来形状预铸任何接口。

## 7. Taxonomy table

| Candidate | Classification | 依据 |
|---|---|---|
| Audio Processing（特性域） | application feature（**当前未授权，需修正案**） | U2 禁入清单 + PBK-002 §14 OPEN |
| "Audio Processing Plugin" | **非 Plugin**（今日）；未来仅凭新 D13 证据 | D13 三条件全部不成立（§5 表） |
| "AudioProcessingCapability" | **非 Capability**（今日，非永久排除） | D14.10/D13；无提供/消费压力（**非 K0 表达力理由**——单例 family provider 可表达，见 §4/§5 窄化） |
| Gain / EQ / Compressor / Limiter / ReplayGain | App-configuration 选择的 **processor 模块**（owned code）；非 Plugin/Capability | D13 negative oracle；U2 先例（SeekPlugin/VolumePlugin 类拒绝） |
| ReplayGain | Gain 的一个配置来源（ABI 已有 track_gain_mb/track_peak，未读取）+ 普通 processor；非新架构 | 代码事实 songcore-sys:144-151 |
| Biquad / FFT window / delay line | processor instance 内的普通 Rust 字段（owned state） | "explanatory vocabulary is not architecture vocabulary" |
| ProcessorChain | **session-owned episode resource**（与 PcmEdge/worker 同类） | D6 所有权二分 |
| Processor instance | episode-scoped、format-bound、session 激活时创建的 owned resource | D6；pcm-contract-a0 #4 format stability |
| parameter state | App 拥有 desired 值；runtime 表示 **OPEN**（见 §9 冻结/OPEN 划分；Gain 可为 episode-fixed 配置或 Gain-specific 标量 publication，均不挣得通用机制） | D14.9 先例（所有权层面） |
| preset | App-owned 配置数据（当前最小模型：下一 episode 生效）；非架构名词 | 同 playlist/volume 惯例 |
| chain 构造 factory | 普通 owned mechanism，非架构级 Abstract Factory；确切构造表示 **OPEN** | razor rule 1–3 全部 YES |

## 8. PCM placement decision

**owner / executor / backend / contract 四分（F3 corrective）——本节裁决的是执行位置，不是语义所有权：**

```text
语义 owner        = Playback Session（episode 处理运行时/状态；
                    架构所有权裁决，见 §6；不随执行位置改变）
当前 executor     = decode worker staging loop（当前最小执行位置——
                    可证伪的执行选择，不是永久法律）
后端实现机制      = backend ≠ Plugin（PBK-003）；processor 的实现机制
                    （含未来 libavfilter 等）是 owned mechanism 选择
PCM/输出契约      = 当前最小切片的有界范围（见本节末"当前最小切片"）

executor ≠ owner；thread ≠ Plugin；backend ≠ Plugin。
未来执行位置（render 侧有界 DSP / 专用处理 worker / platform-backend
执行 / 硬件 DSP）= NOT EARNED / NOT SELECTED（今日），非永久非法——
可被未来证据独立挣得，且不必然改变 DSP 所有权或 Plugin 准入。
```

**选定（当前最小执行位置）：decode worker staging loop**（`session.rs:433-459`，`read_frames` 与 `edge.write_some` 之间；即 Phase C 已证形状 `source → stage → sink` 的 stage 位）。

| 候选位置 | 评估 | 裁决 |
|---|---|---|
| **decode worker staging loop** | 离 RT 线程；worker 已拥有格式与 1024-frame 稳定块；`Applied` 臂即 cut 发生点（staging 丢弃 + `edge.invalidate()`），处理历史失效语义可在该控制流位置满足——确切调用形态属 representation（§10 seek/状态失效行）；失败走既有 D11 `Failed` 终局类（诊断归属不得伪装成 decode failure，见 §10 failure 行）；backpressure 预算 8192-frame edge（≈185ms）；position 核算在 render leg 完全不受影响；**对当前最小切片**：PcmEdge/RenderRequest/PBK-003 契约零改动（切片范围见本节末） | **SELECTED（当前最小）** |
| RenderPcmInput decorator / PcmEdge 内 | 落在 WASAPI render 线程（RT 关键路径）；与 park/quiescence/position 证据逻辑纠缠；D14.9 已以"永久 per-sample RT 税"为由否决软件 PCM 乘法做 volume——同一论证作为**当前证据**反对该位置 | **NOT_SELECTED**（今日未挣得/未选择；render 侧**有界** DSP 可在未来被独立挣得——非永久非法） |
| Output Plugin / backend 执行 | 把 DSP **语义**放进 backend 违反 PBK-003（backend 是平台 mechanism；DSP 语义不是 backend 语义；可移植性丢失；假设 H4 被权威直接反驳）。注意区分：platform/backend/hardware **执行**作为未来机制不被本审计永久排除——今日只是 NOT SELECTED | **语义归属 REJECTED / 执行位置 NOT_SELECTED** |
| native songcore C 层 | DSP 语义/生命周期迁入 SongCore 违反水位线（见下）——这是**边界裁决**（EQ/presets/处理生命周期不迁入 SongCore），不是执行位置偏好问题；同时 DSP 离开 Rust oracle 体系、ABI churn、native 库耦合 episode DSP 状态 | **REJECTED**（水位线裁决） |
| 独立处理 worker/线程 | 第三线程 + 第二条 edge；decode worker 本身就是合适的处理线程；当前最小机制拒绝 | **NOT_SELECTED**（今日最小机制拒绝；未来真实需求可重开——非永久非法） |
| PcmEdge 内部 | 破坏 edge 单一职责（transport + terminal 状态机；seek/pause/EOF 协议承重墙） | **REJECTED**（当前设计裁决） |

**SongCore / FFmpeg 水位线（保持）：**

```text
SongCore     media → source-format PCM（解码边界）
Qianqian     PCM → player-product 处理语义
EQ / presets / 处理生命周期不迁入 SongCore。

libavfilter MAY someday 成为 Qianqian 拥有的实现机制——但：
    libavfilter ≠ 语义权威
    libavfilter ≠ AudioProcessingPlugin 的证明
    使用 FFmpeg ≠ 把 DSP 迁入 SongCore
（是否/如何集成属 representation，OPEN，见 §6）
```

**当前最小切片（有界范围，F3 corrective）：**

```text
CURRENT MINIMUM —— 处理切片限定为：
    source-rate preserving
    source-layout preserving
    frame-count preserving
    bounded causal processing
    no mandatory pending output at EOF
（兼容示例：Gain、因果 biquad EQ）
⇒ 仅对该切片，"PcmEdge / RenderRequest / PBK-003 输出契约零改动"
  这一主张成立；不得把它概括为 "所有 DSP 都不改输出契约"。

FUTURE REOPEN —— 不在当前最小切片内、各自需要新权威：
    lookahead / 改变延迟与 EOF drain 的 limiter 设计 /
    convolution tail / SRC / channel conversion /
    其它改变 latency、frame mapping、EOF drain、pending output、
    format、channel interpretation 或 Position 语义的变换
（它们可能同时改变执行位置选择；见 §16）
```

## 9. Realtime contract

- **RT render thread 的冻结 firewall 行不变**——本设计使其完全不被触碰。
- **worker 稳态处理路径**（每 staging block）禁止新增：K0 任何操作、Capability resolution、registry lookup、图变更、plugin discovery、通用 event fan-out、文件/网络/UI I/O、无界分配/阻塞、无界锁。允许：对预绑定 staging buffer 的有界样本数学（**缓冲表示 OPEN**——in-place/out-of-place/scratch buffer/所有权布局均不在此冻结，见 §10 process 行）、预分配状态、预绑定配置。参数读取的时机与方式不是本行的冻结项（见下方 冻结 vs OPEN 划分；标量 Gain 可采用 OutputLevel 式 load+compare，这是 Gain 场景的可用先例，不是通用机制的冻结）。
- 控制面/数据面分离：**配置 = control-plane**（App → episode 构造参数；运行期更新入口若某 effect 需要，属下方 OPEN 机制，不在此冻结形状内）；**处理 = data-plane**。参数变更永不触发 composition mutation（PBK-001 §7 冻结条款——参数变更 ≠ 自动 Open/重建 episode，四分见 §10 teardown 行）；v1 episode-fixed 模型下链成员/顺序变更于下一 episode 生效（当前最小模型的选择，非永久法律）。
- 不发明比现有证据更严的约束：worker 现状本就有 2ms 有界等待与 seek-slot try_lock；本契约只要求"不新增"。

**参数 publication：冻结 vs OPEN（P1-2 corrective）**

```text
本审计可主张冻结的语义（架构层）:
    - episode 内处理拓扑/顺序稳定（v1）
    - processor 实例 episode-scoped、format-bound
    - 处理（当前最小切片，§8）发生在 PcmEdge 之前的 decode worker
      staging 路径（**当前最小执行位置**——可证伪执行选择，
      scoping 同 §6/§12）
    - 控制面活动不得造成每 PCM 块的 K0 工作
    - 提交性 seek 不连续必须先失效全部 pre-cut 处理历史，
      再处理任何 post-cut PCM（§10 seek/状态失效行）

保持 OPEN 的机制（本审计不冻结）:
    - 通用的 mid-episode 多参数 update/publication 机制——含其
      **可见性/生效时机**（block 边界、sample 边界或其它）、ramp/
      平滑过渡、snapshot 表示与 coherence 机制，全部 OPEN
      （corrective R2：不得把"块边界生效"冻结为通用契约；未来
      有状态 DSP 可能需要 coherent snapshot / 参数平滑 /
      sample-ramped transition / epoch/snapshot 边界或其它被
      挣得的机制，任何一种都不被本审计预先排除或预定）。
      标量 Gain 可容纳于一个 cell/原子量；有状态 EQ/compressor
      需要一致的多字段配置（frequency/Q/gain/enabled；
      attack/release/threshold/ratio）——独立读取各字段可产生
      撕裂的语义配置（frequency 新 / Q 旧 / gain 新）。本审计已
      在 §13/§14 记录 zipper 一致性问题，故通用机制不被 Gain
      证据挣得，也不由 D14.11 冻结。
    - Gain MAY 使用：A. episode-fixed 配置；或 B. Gain-specific
      标量 publication。两者都不挣得、也不冻结通用 DSP 参数
      publication 模型。**首个 Gain 架构探针完全不需要 live 参数
      更新（最小机制优先）**；若日后实现 live 标量 Gain 控制，
      该 Gain-specific 契约必须单独挣得，且 MUST NOT 被推广为
      通用 DSP 参数模型。
    - 不冻结：generic AudioProcessorConfig cell / ArcSwap /
      原子 snapshot / mutex / RCU / lock-free parameter block /
      event-update bus / 任何特定生效时机或 coherence 机制。
    - 不冻结：参数/配置变更与 episode 替换之间的映射——四分
      （desired 配置 / episode-fixed 快照（当前最小）/ future live
      更新 / whole-episode 替换仅限语义确需重建者）见 §10 teardown 行；
      标量/参数变更 ≠ 自动 whole-episode Open 替换（PBK-001 §7）。
    - live 更新若使 old/new realtime 执行视图重叠、且读者仍可解引用
      旧世界，则路由到既有 PBK-001 P1–P5 义务；单 owner worker
      无重叠视图地更新私有状态，不主张触发 P1–P5。
```

## 10. Lifecycle contract

| 阶段 | 冻结的最小语义 |
|---|---|
| create | session 激活内、`format()` 已知后构造；**一个 instance 绑定一个 PcmFormat**（native 中流格式变更已 fail-closed，episode 失败） |
| process | 每 staging block 执行处理；稳态零分配。**缓冲表示 OPEN**：in-place/out-of-place/scratch buffer/SIMD 布局/缓冲所有权均不冻结——冻结的只是"解码后 source-format PCM 在进入 PcmEdge 前被变换"（§8 PCM placement） |
| parameter update | **唯一冻结的通用契约**：控制面活动不得产生每 PCM 块的 K0 / Capability resolution / composition mutation 工作；live-update 可见性/生效时机（block/sample 边界或其它）、ramp/平滑、snapshot/coherence 机制全部 **OPEN**（§9 冻结/OPEN 划分）；参数变化本身不构成处理状态失效事件（除非该 effect 语义明示） |
| seek/状态失效 | **语义义务（冻结）**：对 `Applied` seek 不连续，全部由 pre-cut PCM 派生的处理状态必须先失效，再处理任何 post-cut PCM；`RefusedUnchanged` 路径不得因 seek 被尝试而使处理状态发生可观察变化（D14.5 零内容损失不变量）——**含已处理 staging remainder 的保留/丢弃义务，细化见 §10.1**。**表示（OPEN）**：`reset()` 调用、实例重建、状态替换、epoch/状态交换等确切 Rust API 由实现决定（"fresh-instance observational equivalence" 是验收语义，不是某个调用名）。不需要 `flush()`——**v1 禁止 look-ahead 类加延迟处理器**（其 flush/drain 语义是新权威） |
| pause/resume | 零交互（edge 满则 worker 自然阻塞）；pause **不得**使链处理状态失效 |
| stop/EOF | 链随 worker closure 丢弃；LIFO teardown 顺序不变；EOF 尾块不得注入静音（frame conservation MUST，pcm-contract-a0） |
| failure | **语义（冻结）**：使继续处理不可信的处理器失败 ⇒ 既有 D11 `Failed` 终局类；**不发明新 public 终局变体**。DSP/处理失败**不得**仅仅因为其执行在 decode worker 线程就被诊断归类为 decode failure——终局语义类可以相同，机制诊断阶段必须如实区分（现有内部诊断槽名为 `decode_failure`/`decode_failed`，`completion.rs:408`；D14.5 已把 seek `MutatedThenFailed` 权威性地冻结为走 ordinary decode-failure route——那是 seek 的权威决定，不构成 DSP 失败默认复用同一诊断来源的理由；内部表示 OPEN：worker_failure / processing_failure / 内部携带 stage/category 等均可，本审计不选型）。**bypass-降级模式未授权**（类比 D14.5 "不可证明即破坏性"：失败处理器的输出不可信；恢复语义 = authority gap，除非未来权威显式挣得 bypass/recovery 语义） |
| teardown/replacement | episode-scoped。**参数/配置变更 ≠ 自动 whole-episode Open 替换（F4 corrective 四分）**：① desired 产品配置（App 拥有：enabled / preset / preamp / EQ band gains / chain order）；② episode-fixed 已应用配置（**当前最小**：episode 启动时快照 desired 配置）；③ future live 参数更新（band gain / preset / threshold 变更——机制 **OPEN**，须独立挣得 coherence / apply boundary / smoothing / state continuation / publication 语义，§9）；④ whole-episode/拓扑替换 = D14.6，**仅保留给语义确实需要重建 episode/拓扑的变更**（episode 替换、文件替换等），标量参数变化不属于此类（PBK-001 §7） |

### 10.1 已处理 staging remainder 与 seek/不连续（语义义务细化）

**现实形状**：处理可能对整个 staging block 先完成，edge 才部分接收它——

```text
decode block → 整块处理 → partial write to PcmEdge → 已处理尾部保留
```

现状代码即此形状：`write_observing_seek` 的 bounded-slice 写可在已写前缀处停止，未写尾部保留（`session.rs` D14.5 零内容损失语义）。插入处理后，该保留尾部是**已处理** PCM。由此细化（冻结的是语义义务，不是任何具体 API）：

```text
RefusedUnchanged seek:
    必须保留：processor 信号历史 / 已处理 staging remainder /
              当前生效配置
    不得：重置处理历史 / 重处理已处理尾部 / 二次推进滤波状态
    remainder 的继续方式 == 无 seek 时的继续方式（精确一致）
    （D14.5 refusal 零内容损失不变量的 DSP 推广）

Applied seek —— 在处理任何 post-cut PCM 之前：
    丢弃旧已处理 remainder
    失效全部旧信号派生处理历史
    按既有 seek 协议清空 stale edge PCM

MutatedThenFailed:
    不得重建旧 continuation；走既有 episode failure/terminal 契约
    （D14.5 已冻结其走 ordinary decode-failure route）

seek 期间的 pause:
    pause 意图按既有要求存活；pause 本身不蕴含处理历史重置
    （同 §10 pause/resume 行）

Open/replacement:
    旧 episode 的 processor 生命周期随旧 episode 结束；
    新 episode 按其选定配置获得全新 episode 处理状态
```

**表示保持 OPEN**：不冻结 `reset()` 调用 / 实例重建 / 状态替换 / epoch-状态交换等任何确切机制；remainder 的运行时表示（如 worker 级暂存字段）同理不冻结。"fresh-instance observational equivalence" 是验收语义，不是某个调用名。

## 11. Ordering / configuration ownership（冻结最小规则）

```text
顺序真值 = App-owned 有序链配置（episode 构造参数），session 激活时消费，
          单 episode 内稳定。
顺序永不来自隐式组合顺序：mount order / registration order /
hash-map 迭代 / capability discovery / 任何偶然枚举顺序
（PBK-001 §5 冻结 + K0 §H.4 明文点名 DSP 非交换）。

F2 corrective 窄化——这不是 "DSP 顺序永远不能来自组合"：
    K0 设计 §H.4 本身已把 DSP order 的合法落点列为
    declared desired topology / future graph-owner / domain kernel，
    即显式声明的拓扑/顺序/集成关系原则上可被表达。
    三分如下：
        隐式顺序（mount/registration/迭代/枚举）  禁止作为语义权威
        显式有序产品配置（App 列表 → session 链）  当前最小选择（SELECTED）
        未来显式组合拓扑                          今日未授权、未被需求挣得，
                                                  但不被宣布永久不可能

目录（catalog / implementation registration）≠ 执行链（execution chain）：
    多个 DSP 实现可以存在并注册；其注册/目录顺序不定义 PCM 处理顺序。

v1 无 slot 争用、无 registry、无环问题（有序列表结构上不可环）。
enable/disable = 配置语义；mid-episode 重排 = 今日不授权。
```

## 12. Authority differential（实现前必须变更的权威）

**必需：选项 B —— 对 `ADR-PBK-002` 的一条窄修正案（建议 D14.11 "Audio Processing minimum"），遵循 D14.5/7/8/9 的 gate→amendment→implementation 既有载体。**

```text
文档:        docs/adr/ADR-PBK-002.md（§14 + §20 新增 D14.11）
变更命题:
  1. 授权 Audio Processing 进入产品范围（显式取代 U2 清单中对本阶段的
     DSP/EQ 禁入——不改写历史文本，同 U2 的 scope-freeze 手法）；
  2. 冻结最小边界——只冻结语义/所有权，不冻结 Rust 接口：
       ownership:            Playback Session 拥有 episode 处理链
                             （语义 owner；executor ≠ owner）
       configuration:        App 拥有有序 desired 链配置；四分语义——
                             desired 配置 / episode-fixed 快照（当前
                             最小）/ live 更新（OPEN）/ whole-episode
                             替换仅限语义确需重建者；参数变更 ≠
                             自动 Open（PBK-001 §7）
       episode topology:     链/顺序在单 episode 内稳定（v1）
       PCM placement:        解码后 PCM 在 decode-worker staging 路径、
                             进入 PcmEdge 之前处理（当前最小执行位置，
                             可证伪执行选择；未来执行位置被挣得时
                             不必然改变所有权/Plugin 准入）
       minimum slice:        当前最小切片 = source-rate / source-layout /
                             frame-count preserving、bounded causal、
                             no mandatory pending output at EOF；
                             lookahead / convolution tail / SRC /
                             channel conversion = FUTURE REOPEN
                             （各自需要新权威，§8）
       lifetime:             processor 状态属于 episode 与 source PCM 格式
       seek/discontinuity:   Applied seek 在处理任何 post-cut PCM 前
                             失效全部 pre-cut 处理历史（含丢弃旧已处理
                             remainder）；RefusedUnchanged 不因 seek
                             被尝试而使处理状态发生可观察变化（含已处理
                             remainder 精确继续；机制表示不冻结，§10.1）
       realtime firewall:    每 PCM 块零 K0 / Capability resolution /
                             composition mutation / 通用 dispatch
       failure semantic:     不可恢复的处理器失败映射到既有 D11 Failed，
                             不发明新 public 终局类
       negative D13 ruling:  今日不挣得 per-effect Plugin / per-effect
                             Capability / family Processing
                             Plugin/Capability（可证伪再挣条件在档，
                             §5；非永久排除）
  3. 记录 D13 负裁决：per-effect Plugins、per-effect Capabilities、
     family Processing Plugin 今日均不挣得（negative oracle 入档）；
     保留未来 family Plugin/Capability 的明确再挣条件与增量提升路径；
  4. 重申 volume（D14.9）与 processing gain 的永久区分。
D14.11 明确不冻结:
    public AudioProcessor trait / qianqian-audio-api 放置 /
    factory trait / 动静态派发 / enum vs trait object 表示 /
    crate topology /
    处理缓冲表示（in-place/out-of-place/scratch/SIMD 布局/所有权）/
    通用参数 update 表示与生效时机（ArcSwap / 原子 snapshot /
    mutex / RCU 选择；block/sample 边界可见性 / ramp / coherence
    机制）/ live 更新与 episode 替换的映射（除 v1 episode-fixed
    快照外的全部情形）/
    超出所选最小值的线程拓扑 /
    libavfilter 集成与否（Qianqian 侧实现机制选择，§8 水位线）/
    确切状态失效机制 API（reset() / 实例重建 / 状态替换 /
    epoch/状态交换）/ 确切 failure 诊断 struct /
    未来 registry / 未来 Processing Plugin / 未来 AudioProcessingCapability
不变:         D11 终局权威、F3/F4/F5/F6、D14.9 volume 机制、P1–P5、
              K0 五原语与预算、PCM firewall（全部行）、D6 所有权二分、
              PBK-003 Output 边界。
```

区分要点（P1-1/P1-2 corrective）：

```text
freeze semantics, not Rust interfaces
freeze ownership, not premature shared APIs
freeze discontinuity obligation, not exact function calls
```

无需新 ADR（选项 C）：DSP 是 PBK-001 §10/§12 早已预留的 Issue #12 领域，D14 系列是该类冻结的既定载体；也远不到 broaden reopen（选项 D）。

## 13. Gain architectural probe plan（authority freeze 之后的第一个实现探针）

Gain 是无记忆变换：它只证明无状态处理可以安全插入。**Gain 证据不得被引用为有状态 DSP 语义或通用参数机制已落地**（见下方 STATEFUL-PROBE / PARAMETER-COHERENCE 门）。

**Gain 探针必证门（G1–G9；全部通过只允许结论"Gain 形状的插入是安全的"）：**

```text
G1 unity 透明:       对 finite-normal / ±0 测试语料，gain=1.0 ⇒ 输出
                     位相同（bit-identical）。
                     对 NaN / Inf / subnormal 输入：行为必须由实现/测试
                     策略显式规定，并与所选参考语义比较——不主张普遍
                     位保持（NaN payload/sNaN/subnormal/FTZ-DAZ/平台
                     FP 行为差异；仓库当前无更强 FP policy 可引用，
                     §16 仍列 SIMD/FTZ/denormal 为 open）。
G2 确定性增益:       输出 == 同一 FP policy 下的参考 f32 乘法
G3 PCM 插入:         变换后样本进入 PcmEdge；RenderPcmInput /
                     PBK-003 输出契约零改动（Gain 属当前最小切片，
                     §8——该主张不概括到最小切片之外）
G4 realtime 防火墙:  每 processing block 零 K0 / Capability / reconcile
                     活动（debug_op_count witness 增量为 0）；无新增
                     稳态分配/锁（counting-allocator oracle，
                     direct-pcm-flow 同款技术）
G5 transport 回归:   pause / stop / EOF / 既有 seek 矩阵全绿不回归
G6 失败路由:         注入处理失败 ⇒ D11 Failed（非 bypass、非静音）；
                     且诊断不得被误分类为 decode failure（§10 failure 行）
G7 volume 区分:      processing gain 与 D14.9 OS stream volume 独立
                     可变、互不写入
G8 物理冒烟:         Windows 实机有声 + position 证据不受影响
                     （worker 侧处理不触 render 核算）
G9 representation 纪律: 除非实现演示出真实共享契约需求，不引入任何
                     public/shared processor 抽象（trait / crate 放置 /
                     factory——§6 REPRESENTATION OPEN 维持）
```

**STATEFUL-PROBE 门（在 EQ/任何有状态 DSP 的 authority promotion 之前必须存在）：**

Gain 无法证明"processor 拥有历史"类的语义。需要一个可弃置的最小有状态探针，例如单状态递推：

```text
StatefulProbe:  y[n] = x[n] + k * y[n-1]
```

它只为证明（探针本身可弃置，不入产品）：

```text
1. processor 拥有跨 block 历史
2. 提交性 seek 使 pre-cut 派生旧历史失效（含丢弃旧已处理
   staging remainder、按既有协议清空 stale edge PCM）
3. 同一 post-seek PCM 下，seek 后输出 == 全新 processor 实例的输出
   （fresh-instance observational equivalence）
4. RefusedUnchanged seek 不使处理状态发生可观察变化——含已处理
   staging remainder 的保留与精确继续：不重处理已处理样本、
   不二次推进滤波状态，remainder 继续方式 == 无 seek 的继续方式
   （§10.1）
5. pause/resume 不使处理状态失效
6. stop/teardown 正常销毁实例
```

其中 2/3 对应 §10 seek/状态失效行已冻结的**语义义务**（pre-cut 派生状态先失效、再处理 post-cut PCM）；探针验证的是该义务的实现，而不是把某个具体调用形态（如 `reset()`、实例重建、状态替换或 epoch/状态交换中的某一种）升格为契约。EQ 的其余附加关注（参数更新 zipper 一致性、多通道状态数组、坏系数 NaN/Inf 传播的 fail 行为）仍归 EQ 自己的 gate（对应 §14 H8）。

**PARAMETER-COHERENCE 门（在通用多参数 live update 机制被授权之前必须存在）：** 必须先证明一个多字段配置的一致更新/读取方案（无撕裂语义配置，§9 OPEN 块），才允许把任何具体参数机制推广为通用 DSP 参数 publication 模型。Gain 的标量 case 不能替代该门。

实现体量（示意，非冻结）：playback 内 Gain 处理器（MAY crate-private）+ session 激活/worker 循环两处小改 + App 配置入口；是否引入 trait、放哪个 crate 属 §6 representation 决策。不新增 crate 级架构名词；若需要代码组织可加普通 library crate（crate ≠ Plugin）。探针本身若以 evidence 形态先行（`experiments/`，可弃置），遵循 §16 任务纪律：不提交、不留脏 worktree。

## 14. Adversarial checks（H1–H8）

| # | 假设 | 裁决 | 依据 |
|---|---|---|---|
| H1 | "需要 universal Plugin trait / abstract factory" | **FALSE** | K0 ComponentSpec 是唯一 substrate；factory 仅在 instance 创建层、作为普通 owned mechanism。四分法中仅后两层存在 |
| H2 | "每个 effect 应是自己的 Capability provider" | **FALSE（已证，仅覆盖 Model A）** | required-single：plan 时 `AmbiguousProvider` 拒绝（design §E.2 + 代码 kernel.rs:373-394 + oracle 测试）。该证明只覆盖 per-effect 同 key 多 provider 形状；family 单例 provider 的驳回依据是 D13，不是 required-single（§5 窄化） |
| H3 | "每个 effect 应是自己的 Plugin" | **FALSE** | D13 三条件全不成立 + U2 先例（feature-shaped Plugin 拒绝）+ K0 §H.4（非交换关系需要显式有序结构，独立组合恰好不提供） |
| H4 | "DSP 应放进 Output（样本在此被消费）" | **FALSE** | PBK-003（backend = 平台 mechanism，DSP 语义不是 backend 语义；可移植性）+ per-sample 数学落 RT render 线程（D14.9 已以 RT 税为由否决同类做法） |
| H5 | "DSP 顺序可沿（隐式）K0 组合顺序" | **FALSE（权威明文）** | PBK-001 §5（dependency topology != realtime processing topology；顺序禁止来自 mount/registration/迭代顺序）+ K0 §H.4/§H.6（DSP 被点名非交换；顺序属于显式有序拓扑，归显式 owner）。被禁止的是**隐式**组合顺序作为语义权威；显式声明的拓扑不被宣布永久不可能——K0 §H.4 已列 declared desired topology / future graph-owner 为合法 order 落点，只是今日未授权（F2 corrective，见 §11 三分） |
| H6 | "动态参数变更需要 K0 操作" | **FALSE** | PBK-001 §7（cheap realtime-safe parameter update 不得强制走 composition mutation）+ OutputLevel cell 先例（relaxed load+compare） |
| H7 | "processor instance 可跨 seek 原样存活" | **FALSE（反例成立）** | biquad IIR 延迟态、compressor 包络、limiter 历史、reverb delay line 均为有状态记忆；提交性 seek 后必须先失效全部 pre-cut 派生处理状态（含丢弃旧已处理 remainder），再处理任何 post-cut PCM（stale-DSP-state 是 D14.5 stale-PCM 不变量的推广）；refusal 路径不得使处理状态发生可观察变化（含已处理 remainder 的精确继续，§10.1）。确切机制（`reset()` 调用 / 实例重建 / 状态替换 / epoch/状态交换）属 representation（§10 seek/状态失效行） |
| H8 | "Gain 通过即证明 EQ 架构" | **FALSE** | Gain 是无记忆变换；EQ 首次暴露：format-bound 有状态滤波器、seek 后状态失效的正确性（fresh-instance observational equivalence；违反时可听：滤波瞬态/咔哒声）、参数更新 zipper 语义、多通道状态、坏系数 NaN 传播。EQ 需要 §13 的独立有状态门 |

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
| **audio processing（含 ReplayGain 应用）** | **OPEN** | 本审计对象；**本审计所进入并考察的 transport/playback 执行范围内最后一个 OPEN 边界**（scoped claim，非全产品/全核心主张，见下） |
| device switch / format switch / 多设备 | **OPEN**（v1 fail-closed；closure 归属由未来 closure audit 判定） | PBK-002 §14 |
| analysis/spectrum、lyrics | **UNSCOPED_FOR_THIS_AUDIT**（本 DSP 审计不进入；这不构成它们位于最终产品/核心范围之外的证据） | 无需求证据；lyrics 在 U2 未进入持久化族 |

**结论：不得宣告 `QIANQIAN-NON-UI-CORE-CLOSED`。** Audio Processing 是本审计所进入并考察的 transport/playback 执行范围内最后一个 OPEN 边界；但把 analysis/spectrum、lyrics 标记为 UNSCOPED_FOR_THIS_AUDIT 只说明本 DSP 审计不进入这些领域，**不证明**它们位于最终 closure 之外。在项目可宣告 `QIANQIAN-NON-UI-CORE-CLOSED` 之前，必须先有一份独立的 **`QIANQIAN-NON-UI-CORE-CLOSURE` audit** 枚举预期的产品/核心范围，并至少对以下各项逐一判定 blocker / deferred / excluded：

```text
analysis / spectrum
lyrics timing / lyric data
device switching
portable output backend coverage
plugin-boundary hardening
```

本 corrective 不做这些判定。

## 16. Risks / unknowns

```text
architecture:  family Processing Plugin/Capability 是否终有一日被挣得
               （开放，可证伪再挣条件在档，§5；
               多算法/多 crate/大状态/convolution/FFmpeg backend/
               硬件/presets alone 均不构成提升证据）;
               未来执行位置（render 侧有界 DSP / 专用处理 worker /
               platform-backend 执行 / 硬件 DSP）可被独立挣得，
               且不必然改变 DSP 所有权或 Plugin 准入（§8 四分）;
               mid-stream format switch（现为 fail-closed）
               与未来 look-ahead 处理器的 flush 语义（新权威）;
               device-switch 与 chain 的交互（后者不改变前者仍 OPEN 的事实）。
implementation: SIMD/FTZ/denormal 策略; 参数平滑（若引入，构成新的
               状态失效/过渡语义，须由那时权威覆盖）;
               低端机 CPU 预算; worker 侧处理耗时挤占 1024-frame 产出节奏的
               极端情形。
physical/audio: 状态失效不完全时的可听瞬态; ReplayGain 元数据准确度;
               未来 look-ahead 延迟的可听性。
（physical 未知项不得以软件断言冒充——AGENTS.md "Verification" 条款适用。）
```

## 17. Recommended next task

**`QIANQIAN-AUDIO-PROCESSING-AUTHORITY-FREEZE`**

（修正案为必需，故非 `QIANQIAN-AUDIO-PROCESSING-GAIN-PROBE`。该任务 = 依 §12 起草 D14.11 窄修正案（只冻结语义/所有权，representation 保持 OPEN）+ 以 §13 的 G1–G9 Gain 门作为 grounding evidence，并连同 STATEFUL-PROBE 门与 PARAMETER-COHERENCE 门一并入档为后续 promotion 的前置（F5-GATE/V-PROBE 先例），不合并生产实现。）

---

## Verdict block

```text
LIVE_MAIN       = ba545ee5927e1c19963dee85922569668a3fa023
BASE_SHA        = ba545ee5927e1c19963dee85922569668a3fa023
                  （审计起点 63b4d51，按任务指示 fast-forward 7 commits 后采集全部证据；
                  corrective pass 不改动证据基线）
FINAL_WORKTREE  = clean（审计期间 git status --porcelain 为空；零文件改动、零提交、零 PR）
VERDICT         = GO_WITH_CORRECTIVES
PREFERRED_MODEL = Model D+（修正后的 D）：session-owned episode ProcessorChain
                  + App-owned 有序配置 + decode-worker staging-loop 插入点
                  （**当前最小执行位置**——executor ≠ owner、可证伪执行
                  选择而非永久法律；当前最小切片 = source-rate/layout/
                  frame-count preserving、bounded causal、no mandatory
                  pending output at EOF）
                  + 处理语义契约冻结（representation OPEN——无 public/shared
                  processor trait、无 qianqian-audio-api 放置、无 factory
                  trait / 派发选择被冻结）；
                  无新 Plugin、无新 Capability、无新 plugin 抽象
                  （AudioProcessingPlugin/Capability = K0 可表达但
                  D13 今日不挣；非永久排除，再挣条件 §5）
AUTHORITY_ACTION= B —— ADR-PBK-002 窄修正案（D14.11）：授权 DSP 特性域、
                  只冻结语义/所有权边界（ownership / topology / PCM placement /
                  lifetime / seek 失效义务 / realtime firewall / failure 语义 /
                  D13 负裁决）、明确不冻结 Rust 接口与参数机制表示；
                  不改 D11/F3/F4/F5/F6/D14.9/P1–P5/K0/PBK-003
NEXT_TASK       = QIANQIAN-AUDIO-PROCESSING-AUTHORITY-FREEZE
```
