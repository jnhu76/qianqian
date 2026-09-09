# specs/playback — 播放架构形式化验证

被审计对象：`docs/adr/ADR-PBK-001.md`（PROPOSED / FORMAL CORE PASS）。

定位：**Playback architecture formal exploration with a small blocking temporal core and additional supporting lifecycle evidence.** 本目录不声称整个 Playback Architecture 已被形式化证明——一个小的 blocking temporal core（`PlaybackTemporal` + 4 个 core negative controls）负责攻击高风险状态交错；其余模型与 mutation 是 supporting evidence，不阻塞 ADR ACCEPTED。

本目录用 TLA+/TLC 做显式状态空间探索：正常模型穷举命令与证据的交错，负控制注入故意错误证明 checker 不是 vacuous。

专业名词保留英文，含义以模型为准：

| 术语 | 模型中的含义 |
| --- | --- |
| `TransportKernel` | playback temporal authority：window 角色、generation admission、fence、raw evidence 解释 |
| `MusicKernel` | music-domain semantic authority：intent 解释、ENDED 产品语义（模型中只保留 ENDED 发布动作） |
| `TrackSession` | media identity / source lifetime root（Ownership 模型的树节点；Temporal 模型中该维度被抽象掉） |
| `DecodeSession` | one independently advancing decoder cursor；Temporal 模型中与 generation 一一对应 |
| Generation | window-scoped 时间身份；"stale" = 不再被 owning temporal role 的 admission 接纳 |
| admission | 一个 generation 的 decode result / submission 接收资格；创建时打开，关闭后不再打开 |
| Physical Fence | close admission 之后的设备物理冲刷握手（requested → claimed → succeeded/failed），成功 verdict 原子丢弃被截断 generation 的排队媒体 |
| ActiveWindow / PreparedWindow | 同时最多 1 + 0..1；可以属于不同 generation |

---

## 一、Core acceptance / Extended exploration

### Core acceptance checks（ADR ACCEPTED blocking）

| Semantic risk | Check | Blocking |
| --- | --- | --- |
| Dual Window | PlaybackTemporal | Yes |
| Generation admission | PlaybackTemporal | Yes |
| Physical Fence | PlaybackTemporal | Yes |
| submitted / rendered | PlaybackTemporal | Yes |
| EOF / drained / ENDED | PlaybackTemporal | Yes |
| PromoteWithoutFence | mutation | Yes |
| AcceptUnadmittedDecode | mutation | Yes |
| SingleGlobalGenerationCheck | mutation | Yes |
| EndBeforeRenderDrain | mutation | Yes |

core mutation 与 semantic risk 的对应：`PromoteWithoutFence` → Physical Fence；`AcceptUnadmittedDecode` → Generation admission；`SingleGlobalGenerationCheck` → Dual Window / no-global-current；`EndBeforeRenderDrain` → EOF / physical drain。retired-generation re-enter 的保护处于 core 集——它由 `PlaybackTemporal` 正常模型的不变量 `RetiredGenerationCannotReenter` 承担；对应 mutation 是 extended 证据。

### Extended exploration（supporting / non-blocking）

| Check | Purpose | Blocking |
| --- | --- | --- |
| PlaybackOwnership | resource-lifecycle exploration | No |
| RetiredGenerationStillAdmitted | admission 语义的扩展 mutation 证据 | No |
| ReleaseProviderEarly | provider ordering | No |
| MultipleImmediateOwners | ownership sanity | No |
| OwnershipCycle | ownership sanity | No |
| KernelAdoptsLifetimeOwnership | historical exploratory mutation | No |

Extended 项继续保留并继续运行，其历史结果不删除。若某个 extended mutation 被判断为过度约束 ADR，在本 README 记录 **supporting model assumption, not production architecture authority** 即可，不急着重构模型。

---

## 二、Semantic Claims（从 ADR 提取的验证目标）

### Temporal claims

1. 一个时刻最多一个 ActiveWindow、最多一个 PreparedWindow。
2. Active 与 Prepared 可以同时存在且属于不同 generation。
3. Generation 有效性由 admission / window role 决定，**不是**全局 current_generation 相等。
4. Prepared generation 可以接收 decode result（prime），但 promotion 前不得冒充输出 authority（不提交、不渲染、无在途媒体）。
5. admission 关闭后（被 supersede / close-old / retire），迟到的 decode result 必须被拒绝。
6. 要求 hard cut 的 promotion 必须等待成功的 Physical Fence verdict；fence 失败不得伪装成功 promotion。
7. fence 成功冲刷后，被截断 generation 不得再有在途可渲染媒体。
8. rendered 媒体必须曾被 submitted；每次提交必须由 admitted 期间接受的 decode result 支撑。
9. Decoder EOF ≠ transport drained ≠ ENDED：drained 要求无任何 submitted-but-unrendered 媒体且 active pipeline 内无"未 discard 的已接受未提交结果"（admission 关闭转移把不可再提交的 backlog 显式移入 discard 记账，见模型决策 8）。
10. retired generation 不得重新获得 admission / 窗口 / 在途媒体。
11. pending discontinuity 被新 intent supersede 时，由 TransportKernel 原子取消/替换 prepared 窗口。
12. rapid 命令序列 `seek(100) → seek(200) → next → stop` 与 decode / fence / render 证据的全部交错。
13. stop-fence 在途时，drained/ENDED 的发布必须等待 fence 落定（stop 与自然 ENDED 的竞态裁决，见模型决策 2）。

### Ownership claims

1. MusicComponent 是 composition lifecycle root：episode 完全退出 ⇒ 所有 subordinate 已退出。
2. 每个 nested runtime resource 恰有一个 immediate lifetime owner；owner 边不构成环；所有 ownership 路径到达 lifecycle root。
3. TrackSession 是其 DecodeSession 的 immediate lifetime owner。
4. lifetime ownership ≠ semantic authority：任一 semantic authority 事实（playback cursor / window roles / generation admission / discontinuity execution / playlist policy / selection / ENDED 语义）的 holder 都不是任何 nested resource 的 immediate lifetime owner。
5. provider withdrawal 顺序：withdraw 后不再接受新 commitment；已 commit 的 teardown 访问合法；全部存活 dependent 退出后才能 final release（从未创建的 dependent 不阻塞）。
6. TrackSession 完全退出 ⇒ 名下 DecodeSession 已全部 discharge。

---

## 三、PlaybackTemporal 模型

### 变量（TransportKernel 的 temporal truth + MusicKernel 产品状态 + 环境机制）

| 变量 | 含义 |
| --- | --- |
| `windows` | Window record 集合 `[role, gen, ready]`；用集合表示使 AtMostOne* 成为可检查的不变量而非类型必然 |
| `admitted` / `retired` | open admission 的 generation 集合 / 已 retire 的集合 |
| `nextGen` | 新 generation 分配器（单调递增，永不复用） |
| `fence` | 恒为 record `[cut, target, phase]`；`phase="idle"` 表示无进行中 fence |
| `fenceSuccessFor` | 成功完成 fence 的 cut generation 历史 |
| `promotions` | `[new, cut]` 记录集合（谁经 promotion 成为 Active、切的是谁） |
| `queued` / `submittedEver` / `rendered` / `discardedBacklog` | 每 generation 的设备队列 / 历史提交 / 历史渲染 / admission 关闭时显式 discard 的 decoded-but-never-submitted 计数（drain predicate 读 `accepted = submitted + discarded`，见模型决策 8） |
| `decodeAcceptedInAdmission` / `decodeAcceptedOutOfAdmission` | decode result 按接收时**真实 admission 状态**分流计数；后者是 violation 检测器，正常模型恒 0 |
| `producerTerminal` | decoder EOF（或随 provider withdrawal 终止）的 generation 集合 |
| `sessionClosed` | DecodeSession 已关闭集合 |
| `transportDrained` / `ended` | TransportKernel 的 drained 事实 / MusicKernel 的 ENDED |
| `decoderWithdrawn` | Decoder provider 已 withdraw（一次性全局事件，轻量交互） |

### 关键抽象决策（故意省略的现实细节）

- **不模拟 PCM / buffer / 设备**：媒体流动抽象为提交/渲染计数（上限 `MaxMedia=1`，同一 generation 多块在途与 flush 的次级交错被折叠为 0/1 计数）。
- **不模拟 track 身份与位置**：seek 与 next 共用同一 `PrepareDiscontinuity` 骨架（ADR 冻结两者同 skeleton）；next 特有的 TrackSession 子树释放竞态属 Ownership 模型（两模型不合并——temporal 与 ownership 的拆分来自 ADR 本身，组合盲区已知）。
- **不模拟 pause/resume**：transport 级暂停不承载任何被验证的 temporal 安全属性（产品语义），省去后状态空间减半。
- **supersede 原子化**："取消旧 prepared"与"创建新 prepared"折叠为一步（TransportKernel 是唯一 supersede authority）。
- **decode session 关闭后不再产生结果**：`LateDecodeResult` guard 要求 session 存活；"decoder worker 的 in-flight result 晚于 session close 到达"的现实竞态被排除（由于所有关闭都发生在 admission 已关之后，该竞态即使探索也只会落入纯拒绝动作）。
- **单 fence 槽**：同一时刻最多一次在途物理冲刷（现实设备通常一次一个 flush）；fence verdict 不会晚于下一次 fence 乱序到达。
- **retired generation 的记账折叠**：DecodeSession 关闭时把该 generation 的 submitted/queued/accepted/discard 计数收敛为 rendered 值（discardedBacklog 归零）——retired 历史不参与任何活跃不变量，这是状态空间抽象；violation 检测器 `decodeAcceptedOutOfAdmission` 不折叠，保证 counterexample 可见。
- **admission 关闭即 discard 未提交 backlog**：提交要求 admission 开放且关闭单向，因此 admission 关闭转移（`CloseOldAdmission` / `RequestStop`）当場把 decoded-but-never-submitted backlog 写入 `discardedBacklog`（模型决策 8）；不做这一步，fail/abandon 或 verdict-consumed 之后的 admission-closed active 会让 drain predicate 永久悬空。
- **fence 成功 = 设备冲刷完成**：`CompleteFence` 原子丢弃 cut generation 的排队媒体（flush 语义），此后该 generation 静默。
- **有限边界**：`MaxGen=4`（覆盖 `seek/seek/next/stop` 完整 rapid trace），`MaxMedia=1`。

### 动作

`Play`；`RequestSeek` / `RequestNext`（共用 `PrepareDiscontinuity`：原子 supersede 旧 prepared + 创建新 prepared generation）；`RequestStop`（supersede prepared + 关 active admission + discard 未提交 backlog + 发起/重解释终局 fence）；`AcceptDecodeResult(g)` / `LateDecodeResult(g)`；`MarkPreparedReady`；`DecoderEof(g)`；`DropUnprimablePrepared`；`CloseOldAdmission`（关 active admission + discard 未提交 backlog）；`RequestFence` / `ClaimFence` / `CompleteFence` / `FailFence` / `RetryFence` / `AbandonFence` / `ConsumeFenceVerdict`；`PromotePrepared`；`StopComplete`；`CloseRetiredDecodeSession(g)`；`SubmitMedia(g)` / `RenderMedia(g)`；`WithdrawDecoderProvider`；`PublishTransportDrained` / `PublishEnded`；`Stall`（合法等待，非 deadlock）。

### Safety properties 与防御的 bug

标注说明：**detector** = 由检测器变量编码、正常模型 guard 保证恒真、由负控制证明可捕获；**constructive** = 由动作 guard 构造性成立（负控制未覆盖其可捕获性）；**state** = 状态级直接可违反。

| Invariant | 类型 | 防什么 |
| --- | --- | --- |
| `AtMostOneActiveWindow` / `AtMostOnePreparedWindow` | constructive | 双窗口结构被 promotion/supersede 动作破坏 |
| `ActivePreparedGenerationsDistinct` | constructive | 两窗口共一代导致 admission 语义坍缩 |
| `PreparedWindowsAreAdmitted` | constructive | admission 已关的 generation 仍持有 prepared 角色 |
| `RetiredGenerationCannotReenter` | state | retired generation 重新获得 admission / 窗口 / 在途媒体 |
| `DecodeResultRequiresAdmission` | detector | admission 关闭后的 decode result 被接收（迟到结果污染新时间轴） |
| `SubmissionsBackedByAdmittedDecodes` | constructive | 无 decode 支撑的"凭空"提交 |
| `PendingMediaOnlyInActiveWindow` | constructive | prepared/retired generation 出现在途媒体 |
| `PreparedWindowIsNotOutputAuthority` | constructive | prepared 在 promotion 前冒充输出 authority |
| `RenderedNeverExceedsSubmitted` | constructive | 设备渲染未提交媒体 |
| `PromotionRequiresSuccessfulFence` | state | 未经成功 fence 的 hard-cut promotion（旧尾未死即切换） |
| `FenceFlushedGenerationsAreSilent` | constructive | fence 成功后被截断 generation 仍可发声 |
| `TransportDrainRequiresRenderedDrain` | state | 有 submitted-but-unrendered 媒体时宣布 drained |
| `NoStrandedDecodeAfterAdmissionClose` | constructive | admission 已关闭的窗口仍有未申报的 decoded-but-unsubmitted backlog（fail/abandon 或 verdict-consumed 后 drain predicate 永久悬空——决策 8 关闭的洞） |
| `EndedRequiresTransportDrain` | constructive | ENDED 绕过 transport drained 事实 |
| `StopFenceRequiresActiveWindow` | state | stop-fence 在途时 ActiveWindow 被移除（drained/ENDED 抢先导致命令永久锁死——见模型决策 2） |

ADR 冻结语义中「old generation cannot submit after successful promotion」与「fence failure ⇒ no fake promotion」由动作 guard（`SubmitMedia` 的 admission 条件、`PromoteFenceCondition` 只认 succeeded）构造性成立，分别由 `FenceFlushedGenerationsAreSilent` / `PromotionRequiresSuccessfulFence` 从旁佐证。

---

## 四、PlaybackOwnership 模型（supporting / non-blocking）

> **定位：resource-lifecycle 假设的 supporting formal exploration。** 本模型不作为 ADR-PBK-001 ACCEPTED 的 blocking 前置条件：`PlaybackOwnership` FAIL 不自动推出 ADR 不能 ACCEPTED，除非它发现 ADR 本身存在明确语义矛盾。不为让它完美映射未来 production ownership 而扩大模型。

### 有限实例宇宙

1 MusicComponent（lifecycle root）+ 1 MusicKernel + 1 TransportKernel + 2 TrackSession + 3 DecodeSession + 2 Provider（DecoderProvider / AudioOutputProvider）。

`ownerOf` 把每个 resource 的 immediate lifetime owner 表示为**集合**——"恰有一个 owner"因此是可检查的不变量而不是函数类型的结构性必然。资源状态 `Absent → Alive → Draining → Gone`；provider 状态 `Bound → Withdrawing → Released`。

**Semantic authority 事实表**（正结构，非注释）：`TemporalAuthorityFacts`（playbackCursor / windowRoles / generationAdmission / discontinuityExecution / physicalEvidenceInterpretation → TransportKernel）与 `MusicDomainAuthorityFacts`（playlistPolicy / selectionSemantics / endedSemantics → MusicKernel）。authority 关系定义在**事实维度**，ownership 关系定义在**资源维度**，`SemanticAuthoritiesHoldNoLifetimeOwnership` 检查二者不相交，负控制 `KernelAdoptsLifetimeOwnership` 证明其可失败。

### 动作与顺序

`CreateTrackSession` / `CreateDecodeSession`（新 session 需要 decoder provider 仍 Bound——withdrawal 后不再接受新 commitment）；`StartTeardown`；`FinishDecodeSession`（provider Withdrawing 期间保持合法——已 commit 的 teardown 访问）；`FinishTrackSession`（guard：名下 DecodeSession 全部 Gone）；`EndMusicEpisode` / `FinishKernel` / `FinishMusicComponent`（episode 完全退出）；`WithdrawProvider` / `FinalReleaseProvider`（guard：全部 dependent 处于 Absent/Gone——从未创建的 dependent 不阻塞，存活 dependent 必须先退出）。

### Safety properties

| Invariant | 类型 | 说明 |
| --- | --- | --- |
| `UniqueImmediateLifetimeOwner` | state | 存在中的 nested resource 恰有一个 immediate lifetime owner |
| `OwnershipReachesLifecycleRoot` | state | 所有存在中的资源沿 owner 边 ≤3 跳到达 MusicComponent |
| `OwnershipIsAcyclic` | state | owner 边不构成 ≤3 长度环（实例宇宙内最深链 2 跳，见 tla 内有界深度说明） |
| `DecodeSessionsOwnedByTrackSessions` | constructive+mutation | DecodeSession 的 owner ⊆ TrackSession（KernelAdopts 负控制顺带可破坏） |
| `TrackSessionsOwnedByMusicComponent` | constructive+mutation | TrackSession 的 owner = {MusicComponent}（OwnershipCycle 负控制顺带可破坏） |
| `SemanticAuthoritiesHoldNoLifetimeOwnership` | state | 任一 authority 事实的 holder 不是任何资源的 immediate owner |
| `ProviderFinalReleaseRequiresDependentExit` | state | Released 后无任何存活 dependent |
| `TrackSessionGoneImpliesDecodeSessionsDischarged` | constructive+mutation | TrackSession Gone 后名下无 DecodeSession 挂靠 |
| `MusicComponentGoneImpliesSubordinatesExited` | constructive | episode 完全退出后 subordinate 全部 Gone（FinishMusicComponent guard 镜像） |

### 抽象决策与已知边界

- **Window / Generation 不进入 ownership 树**：ADR §2 已纠正——Active / Prepared 是 TransportKernel 内的 temporal role / slot，不是 Nested Runtime Resource，因此不产生 immediate lifetime owner 问题（原「window immediate owner 未冻结」观察项由此关闭）。"TransportKernel 拥有 Window semantic authority 但非 lifetime owner"的张力随之消解；两模型不合并的组合盲区对 window 项不再存在。
- **ProviderDependents(AudioOutputProvider) = {TransportKernel}** 是模型决策（ADR 未定义此依赖关系，ADR §4 绑定 AudioOutput 的是 MusicComponent；模型假设 TransportKernel 的物理输出路径使其成为 dependent）。
- **withdrawal 五段顺序的覆盖**：阶段 3-4（全部 dependent 退出 → final release）有 guard + 不变量 + 专属负控制三层验证；阶段 1（不再接受新 commitment）仅由 `CreateDecodeSession` guard 表达（transition 级）；阶段 2（teardown 访问保持合法）由 `FinishDecodeSession` 无 provider guard 的"许可"表达，无专门交错覆盖证据。
- **Up1..Up3 有界深度**：对当前动作集（最长链 2 跳、可能最长环 2）充分；扩展模型时必须同步加深，否则环/根检测静默漏检。
- 不建模 liveness（"最终一定退出"需要 fairness 假设）；全部结论限于 safety。

---

## 五、如何验证

```bash
# 仅 core acceptance 集（PlaybackTemporal 正常模型 + 4 个 core mutation；ADR ACCEPTED blocking 集）
specs/check.sh core

# 全量（正常模型 + 全部负控制；缺省模式）
specs/check.sh

# 需要代理下载工具链时
export https_proxy=http://127.0.0.1:7897
specs/check.sh

# 单独运行（示例：正常 Temporal 模型，带 coverage）
cd "$(mktemp -d)"
cp specs/playback/PlaybackTemporal.tla specs/playback/PlaybackTemporal.cfg .
java -jar specs/tools/tla2tools.jar -workers 4 -coverage 60 PlaybackTemporal.tla

# 单独运行某个负控制（示例）
cd "$(mktemp -d)"
cp specs/playback/PlaybackTemporal.tla specs/playback/mutations/PromoteWithoutFence.cfg .
java -jar specs/tools/tla2tools.jar -workers 4 -config PromoteWithoutFence.cfg PlaybackTemporal.tla
```

工具链：`tla2tools v1.7.4 (Xenophanes)`，sha256 `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`，`check.sh` 自动下载并校验（fail closed）。

注：TLC 每次 run 只报告 cfg 顺序中**第一个**被违反的不变量——负控制 cfg 已把目标不变量排在首位；counterexample 规模数字随 worker 数/终止时点浮动，仅作量级参考。`SingleGlobalGenerationCheck` 由 runner 以 `-continue` 运行：安全性违反与症状属性（`PROPERTY DualWindowNeverPrimesUnderGlobalCheck` 必须成立）在同一次运行中机器检查。

---

## 六、运行结果（真实运行数据，2026-09-09，4 workers，含 executable-core corrective 后模型）

core acceptance 集 = `PlaybackTemporal` 正常模型 + 4 个 core mutation（`PromoteWithoutFence` / `AcceptUnadmittedDecode` / `SingleGlobalGenerationCheck` / `EndBeforeRenderDrain`）；下表其余行为 extended exploration 证据，全部保留。

本列数据对应 decision 8（admission 关闭 discard 记账 + `NoStrandedDecodeAfterAdmissionClose` 不变量）合入后的模型；与 2026-09-08 数据不可直接对比（新增变量与新不变量改变了可达状态空间与 drain 发布条件）。

### 正常模型

| 模型 | 结果 | states generated | distinct states | depth | runtime |
| --- | --- | --- | --- | --- | --- |
| PlaybackTemporal | **PASS**（17 个 invariant 全部成立，0 TLC warning） | 36,016,926 | 3,213,723 | 47 | ~105s |
| PlaybackOwnership | **PASS**（10 个 invariant 全部成立） | 99,061 | 14,528 | 19 | <1s |

正常模型非空洞（coverage 证据，PlaybackTemporal 关键动作触发数 / 启用状态数）：`PrepareDiscontinuity` 9,270 / 60,190、`CloseOldAdmission` 10,977 / 23,602、`RequestFence` 22,231 / 30,497、`CompleteFence` 252,909 / 377,957、`FailFence` 220,298 / 377,957、`AbandonFence` 173,628 / 377,957、`PromotePrepared` 15,091 / 26,922、`StopComplete` 205,432 / 314,448、`RequestStop` 137,471 / 2,462,900、`PublishTransportDrained` 231,874 / 500,530、`PublishEnded` 115,457 / 218,531、`WithdrawDecoderProvider` 160,328 / 2,638,555、`ConsumeFenceVerdict` 6,553 / 33,417、`DropUnprimablePrepared` 11,792 / 81,506；`LateDecodeResult` 在 8,179,286 个状态上启用（纯拒绝动作不改变状态，触发数按启用计）；`RetryFence` 触发数为 0 属 TLC coverage 归因（其 failed→requested 后继总是先由 `RequestFence`/`RequestStop` 等动作生成；与 2026-09-08 模型行为一致），探索完整性不受影响。Ownership 模型 `FinalReleaseProvider` 与 `FinishMusicComponent` 均有非平凡触发。

### 负控制（mutation 必须 FAIL 才算通过）

| Mutation | 目标 property（必须被违反） | 结果 | CE 规模（近似） |
| --- | --- | --- | --- |
| PromoteWithoutFence | `PromotionRequiresSuccessfulFence` | **MUST FAIL / 已失败** ✓ | 782 distinct |
| AcceptUnadmittedDecode | `DecodeResultRequiresAdmission` | **MUST FAIL / 已失败** ✓ | 28 distinct |
| SingleGlobalGenerationCheck | `DecodeResultRequiresAdmission`（stop 窗口期）+ 症状属性成立 | **MUST FAIL / 已失败** ✓ | 完整探索 ~51.6k（`-continue`） |
| RetiredGenerationStillAdmitted | `RetiredGenerationCannotReenter` | **MUST FAIL / 已失败** ✓ | 59 distinct |
| EndBeforeRenderDrain | `TransportDrainRequiresRenderedDrain` | **MUST FAIL / 已失败** ✓ | 186 distinct |
| ReleaseProviderEarly | `ProviderFinalReleaseRequiresDependentExit` | **MUST FAIL / 已失败** ✓ | 169 distinct |
| MultipleImmediateOwners | `UniqueImmediateLifetimeOwner` | **MUST FAIL / 已失败** ✓ | 18 distinct |
| OwnershipCycle | `OwnershipReachesLifecycleRoot` | **MUST FAIL / 已失败** ✓ | 103 distinct |
| KernelAdoptsLifetimeOwnership | `SemanticAuthoritiesHoldNoLifetimeOwnership` | **MUST FAIL / 已失败** ✓ | 109 distinct |

Ownership 模型未随本次 corrective 变更（运行数据与 2026-09-08 一致）。

`SingleGlobalGenerationCheck` 的双证据（均由 runner 机器检查）：

1. **安全性失败**：恢复 `result.generation != current_generation => stale` 后，stop 窗口期（active admission 已关、fence verdict 未落、无 promotion 改变全局代）old active gen 的 decode result 被错误接收，TLC 给出 counterexample。
2. **功能性破坏**：全局相等检查使 prepared gen（≠ current）永远无法被喂送——症状属性 `DualWindowNeverPrimesUnderGlobalCheck` 在完整探索中成立（`MarkPreparedReady` / `PromotePrepared` coverage 为 `0:0`，从未启用；正常模型中两者分别有 2,181 / 15,091 次触发）。Dual Window 在该检查下不成立。

---

## 七、模型决策记录（ADR 未明确定义处的建模选择）

以下决策是模型为封闭探索所做的选择，**不是**对 ADR 的改写；若实现期发现不同选择，应先更新本记录并重跑验证。决策 2 / 4 的最小 invariant 已反哺 ADR（§18）；决策 3 对应的 ADR 冻结只有 claimed-transaction 不可逆一条（§8）：

1. **stop 可以 supersede 在途 promote-fence**：用户在 seek fence 未落定时按 stop 是真实竞态。模型把同一次物理冲刷重解释为终局 cut（保留握手阶段、清空 promotion 目标——设备不关心 promotion 计划）。ADR §8/§18 未定义此交错。
2. **stop-fence 在途时不发布 drained/ENDED**：stop 与自然 ENDED 的竞态（EOF 排干 vs stop 物理切断）ADR 未定义。模型裁决：fence 握手在途 ⇒ 物理状态未定 ⇒ TransportKernel 不解释 drained（`PublishTransportDrained` 要求 fence idle；`RequestStop` 重置陈旧 drained 事实；`StopFenceRequiresActiveWindow` 不变量守护）。对抗 review 曾证明无此裁决时存在"ENDED 抢先移除 ActiveWindow → fence 永久卡死 → 命令锁死"的可达坏状态。**该裁决的最小 invariant 已反哺 ADR §18**（stop × 自然 ENDED 竞态冻结：Physical Fence 在途时，自然 EOF/drain 不得提前终态化并销毁 fence 所需 active temporal state）。
3. **stop-fence 期间不接受新 seek/next**：stop 是终局性 cut，等 fence verdict 落定后才能开新 episode（`stop → seek 重开` 交错未探索）。**这是模型为闭合探索所做的决策，不是产品语义冻结**；ADR 只冻结「fence 进入 claimed（不可逆）阶段后，后续 intent 不得取消或改写已 claim 的 physical transaction」（§8）。reject / defer / coalesce / latest-wins 留给 executable implementation/oracle。
4. **prepared 在 prime 完成前 decoder EOF**（如 seek 到文件尾）：`DropUnprimablePrepared` 取消该 discontinuity。**ADR 已收编最小冻结**（§18 Prepared EOF）：EOF evidence does not imply PreparedWindow readiness；prepared contribution 在 readiness 前 terminal 必须得到显式 outcome。具体分类（prepare failed / empty media / seek-to-EOF 等）留给实现与后续 oracle。
5. **fence 失败的出路**：`RetryFence`（重试）或 `AbandonFence`（fail closed：不 promote，active 保持 admission-closed 自然排干）。自然排干之所以在 abandon 后仍然可达，是因为 admission 关闭转移已经把未提交 backlog 显式 discard（决策 8）。
6. **fence 成功但 promotion 目标已被 supersede**：verdict 被消费（物理冲刷确实发生），不 promotion；新 prepared 走自己的 episode（同一 cut gen 可再次 fence——设备已静默，幂等）。
7. **EOF 后、drained 前的已接受未提交结果**：drained predicate 要求 `decodeAcceptedInAdmission = submittedEver + discardedBacklog`（ADR "software media pipeline drained" 的模型化；discard 项见决策 8）。
8. **admission 关闭即 discard 未提交 backlog**：提交要求 admission 开放且关闭单向，因此 decoded-but-never-submitted 媒体在 admission 关闭转移（`CloseOldAdmission` / `RequestStop`）当場显式移入 `discardedBacklog` 记账。没有这一步，fail/abandon（以及 verdict-consumed 后 admission 保持关闭的 active）在 `accepted > submitted` 时会落入 drain predicate 永远无法满足的悬挂状态——executable core 的对抗 review 首先发现了这个洞（Rust `TransportKernel::discard_stranded_backlog` 与本模型同步修复）。这是 fail-closed 语义的记账补全，不改写 ADR 的 fence failure ⇒ no fake promotion / retry or fail closed 冻结。

## 八、TLC 没有证明什么

- **整个 Playback Architecture**：本目录只攻击五组高风险 temporal 语义与 resource-lifecycle 假设，**不构成对整个架构的形式化证明**（formal exploration，非完整架构证明）。
- 真实 WASAPI/CoreAudio/AAudio 的 flush 正确性（fence 是抽象握手，不模拟设备）。
- 真实 Rust 实现：memory ordering、锁、ring buffer、lock-free 结构。
- 真实 decoder 行为（EOF 语义、坏帧、seek landing 精度）。
- 真实 RT scheduling 与性能、音质。
- 任意数量 generation / TrackSession / DecodeSession 的参数化证明（有限模型边界 `MaxGen=4`、2 track、3 session；TLAPS 未使用）。
- liveness（"最终会 drain / 退出"）：模型只验证 safety；`Stall` 表示合法等待。
- crossfade / gapless（ADR 明确 Dual Window 不自动授权）。
- 两模型的组合性质（temporal 竞态 × ownership 释放竞态无单一模型同时可见）。

## 九、Traceability

- **Core acceptance（blocking）**：`PlaybackTemporal` 覆盖 ADR-PBK-001 §21 **Formal Acceptance** 的五组高风险 temporal 语义，与上文 temporal claims 及 properties 表一一对应。core mutation 对应：`PromoteWithoutFence` → Physical Fence；`AcceptUnadmittedDecode` → Generation admission；`SingleGlobalGenerationCheck` → Dual Window / no-global-current（安全性半：stop 窗口期误接收；功能性半：症状属性）；`EndBeforeRenderDrain` → EOF / physical drain（注入点在 TransportKernel 的 drained 发布层——EOF 证据的误解释发生在该层，ENDED 经 `EndedRequiresTransportDrain` 间接被保护）。
- **Extended exploration（non-blocking）**：`PlaybackOwnership` 覆盖上文 ownership claims；`RetiredGenerationStillAdmitted`（状态面：retired 重新 admitted）与 `AcceptUnadmittedDecode`（症状面：迟到结果被接收）配对覆盖 retired/late-decode admission 语义；`ReleaseProviderEarly` / `MultipleImmediateOwners` / `OwnershipCycle` / `KernelAdoptsLifetimeOwnership` 为 ownership / provider-ordering 探索证据。
- 历史标签对照（早期 ADR 修订曾用 BUG-A..E 命名同类注入，仅作研究历史保留）：BUG-A = `PromoteWithoutFence`；BUG-B = `SingleGlobalGenerationCheck`；BUG-C = `RetiredGenerationStillAdmitted` + `AcceptUnadmittedDecode`；BUG-D = `ReleaseProviderEarly`；BUG-E = `EndBeforeRenderDrain`。
- ADR 观察项闭环：模型决策 2（stop × 自然 ENDED）与决策 4（Prepared EOF）的最小 invariant 已反哺 ADR §18；window immediate-owner 观察项由 ADR §2 纠正关闭（Active / Prepared 是 temporal role / slot，非 Nested Runtime Resource）。

## 十、对抗 review 记录

本目录的模型经过三轮 fresh-context 对抗 review（temporal 抽象 / ownership 三分 / 负控制有效性），修复了：stop×ENDED 竞态导致的命令永久锁死（补 fence-idle guard + `StopFenceRequiresActiveWindow`）、stop×promote-fence 交错被排除（`RequestStop` 放宽）、semantic authority 缺正结构（authority 事实表 + `KernelAdoptsLifetimeOwnership` 负控制）、provider final release 把从未创建的 Absent dependent 当阻塞条件、`SingleGlobalGenerationCheck` 症状属性未接入 gate（`-continue` + PROPERTY 机器检查）、mutation cfg 目标不变量排序（TLC 只报第一个违反）。

第四轮（2026-09-09，executable core 对抗 review）首次由 Rust executable oracle 反向发现模型欠定义：admission 关闭后 decoded-but-never-submitted backlog 使 drain predicate 在 fail/abandon 与 verdict-consumed 路径下永久悬空。同步修复 = 决策 8 + `discardedBacklog` 变量 + `NoStrandedDecodeAfterAdmissionClose` 不变量 + Rust `TransportKernel::discard_stranded_backlog` 与回归测试（Rust/模型/spec 三方同步）。本轮还暴露一个取证陷阱并已修正：`RequestStop` 的 UNCHANGED 列表误含 `discardedBacklog` 时 TLC 仅发 Warning（"variable changed while UNCHANGED"）并静默丢弃相关转移，`check.sh` 的 grep 不检查 warning——首轮"PASS"数据（21.5M states）由此作废，修复后重跑（36.0M states，0 warning）。
