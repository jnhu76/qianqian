# specs/episode-terminal-settlement — episode terminal settlement 的当前规范一致性模型

> **STATUS: CURRENT FORMAL GATE（current-spec conformance）。**
> 已接入 `specs/check.sh current` 与 CI `formal-semantic-gate`。
>
> 语义真相只有一份：`docs/adr/ADR-PBK-001.md` §2（semantic commit /
> fact authority identity / projection 非权威）与
> `docs/adr/ADR-PBK-002.md` §17 D11（episode terminal outcome authority +
> settlement ownership + late-command stability + 三条外部命题）。
> 本套件**不是第二份 authority**：它只机器检查"accepted D11 contract 能否
> 被极小模型一致表达、错误版本能否被机器抓住"。绿 run 只说明在所述
> bounds/假设内未找到反例，**不是** architecture acceptance。
>
> 命名说明：suite/module/不变式全部使用稳定领域词汇。ADR 决策编号
> （D11）按 `specs/README.md` 命名规则只作 traceability 信息，不进入
> 模型文件名/词表。

## 与 `f2-terminal-commit-boundary/` 的 authority 关系

```text
specs/f2-terminal-commit-boundary/
    HISTORICAL / EXPLORATORY evidence（campaign artifact，非 CI gate）。
    回答过"terminal Fact 的 commit ownership 归谁"（A/B/B′ 三变体），
    verdict = CURRENT_CONTRACT_UNDERSPECIFIED。exploration 已完成。

specs/episode-terminal-settlement/
    CURRENT accepted semantic conformance（本套件，CI gate）。
    A/B/B′ exploration 已被 D11 settlement corrective（ADR-PBK-002 §17）
    裁决收尾；本套件不重开比较，不把任何变体（尤其 B′ atomic-commit）
    当作 production requirement。
```

本模型固化的分工来自 merged D11 文本：

```text
decision（分类边界）与 semantic commit 执行**可以分离**：
    evidence 落地的那一步只 latch 决策边界（记下"此刻 stop intent 是否
    已被记录"），不提交 outcome；
    AuthoritySettle 才是 None -> terminal 的唯一 writer。
    （production 允许在同一 call stack 内由 authority 连续完成两步——
    模型只要求 writer 归属 authority 角色，不要求两步之间有物理间隔。）
```

## D11 contract 的普通话翻译（模型逐一对应）

| RULE | 普通话 | 类型 | 模型表达 |
| --- | --- | --- | --- |
| C1 单个 terminal Fact | 一个 episode 至多一个终局（None→Completed/Stopped/Failed 一次），提交后不可改写 | safety | `TerminalOutcomeImmutable`（S2）；mutation M3 |
| C2 authority-owned settlement | 歌播完/失败后，**不需要有人来问**，authority 自己负责把终局落下来。Observe/Wait/Projection/App query 都不是 Fact 出现的必要条件 | safety + witness | writer 集合（S3）+ `WitnessNoConsumerCommit`（W6）；mutation M9 |
| C3 Observe 纯读 | observe() 只能读；不能 resolve/commit/改证据/改 stop intent/改生命周期 | safety | `Observe` 只写 `observeRan`；mutation M1 |
| C4 Wait 纯等待 | wait() 只能等 authority 已经落下的 Fact；不跑 resolver、不是 terminal writer、不是 Fact 存在的前提。TLA 不模拟 OS blocking，只需 wait 动作无 semantic writer 效果 | safety | `Wait` 只写 `waitRan`；mutation M2（今日生产 `wait()->resolve()` differential 的形状，必须被抓住） |
| C5 teardown settlement 边界 | 如果 teardown 已经完成，那"早就足够判决"的 terminal Fact 不能还没落下来。这是 safety 边界，**不是**"每个 episode 最终都会 teardown"的进度承诺 | safety | `TeardownRequiresSettlement`（S10，条件形式）；mutation M5 |
| C6 late-command stability | 终局证据首次决定性**之后**才到的 stop 命令，不能改变这次 episode 应得的分类：决定性已判 Failed 时晚到 stop ≠ Stopped；边界**之前**记录的 stop intent 仍满足 Stopped | safety | `stopAtDecision` ghost + `CommittedOutcomeMatchesContract`（S9）+ `NoFalseStopped`（S7）；mutation M4/M8；正向 witness W4/W5 |
| C7 activation failure firewall | activation failure 是 diagnostic，不是 D11 episode terminal Failed（除非 live episode 的 terminal evidence 独立满足 contract） | safety | `ActivationFailureIsNotTerminalFailed`（S8）；mutation M6 |
| C8 无虚构 Completed | Completed 至少要求 decode/worker EOF evidence + output drain-complete evidence（不加物理可听性） | safety | `NoFalseCompleted`（S6）；mutation M7 |
| C9 无虚构 Stopped | Stopped 至少要求 aborted episode terminal evidence + 决策边界时已记录 stop intent + 无更高优先级 failure 胜出。不许重写成 stopRequestedNow | safety | `NoFalseStopped`（S7，**只读 stopAtDecision**）；mutation M8 |

## 模型词 → current production reality 映射

| 模型动作/变量 | production referent | 现实依据 |
| --- | --- | --- |
| `Activate` | session 激活成功（edge 已绑定、render stream 已打开、worker 已 spawn） | `session.rs` `activate_inner` |
| `PublishActivationFailure` | `SessionCompletion::activation_failed`（diagnostic） | `completion.rs`（first-wins） |
| `RequestStop` / `stopSeen` | `SessionCompletion::request_stop` / `stop_requested` | command 状态，不是 fact；单调 FALSE→TRUE；"stop before full open" 在 bind 时生效（`bind_stop_target`） |
| `PublishDecodeFailure` | `SessionCompletion::decode_failed` | worker / panic guard 发布，first-wins |
| `PublishWorkerEof/Stopped/Failed` / `workerTerminal` | `SessionCompletion::worker_exited(edge.terminal())` | decode worker 退出前最后一步；edge terminal first-wins 单调 |
| `PublishDrainDrained/Aborted` / `drainVerdict` | `DrainSignal::complete(verdict)` | render 线程退出前必发（`qianqian-output-wasapi/src/wasapi.rs`）；`stop_and_join()` 返回 ⇒ verdict 已发布 |
| `BeginTeardown` / `FinishTeardown` | session fiber 的 effect 逆序释放（LIFO：stop edge + join worker → `stop_and_join` stream） | `session.rs` `register_relation` 逆序；join 返回 ⇒ 对应 leg 的发布必然已执行（`FinishTeardown` 守卫 1 的现实依据） |
| `AuthoritySettle` | Playback Session semantic authority 的 settlement 动作。**当前 Rust 由 consumer 调用路径代为触发**（`wait()`/`try_resolve_now()` → `resolve()`）——这正是 D11 记录的 known differential，F2 修正目标。本模型按 accepted contract 建 authority-owned settlement | `completion.rs::resolve`（current realization） |
| `Observe` / `Wait` | D14.2 seam 语义的纯读 / 纯等待 | 当前生产 `wait()` 会顺手 resolve（known differential；M2 形式化证明当前 contract 拒绝它） |
| `terminalOutcome` / `firstCommitted` | `CompletionState.outcome`（memoized） | `completion.rs` |
| `decisionLatched` / `stopAtDecision` | **无 production 对应**（verifier-only ghost） | 见下节 |
| `observeRan` / `waitRan` / `authoritySettled` | **无 production 对应**（verifier-only 见证变量） | — |

`TerminalCandidateOf(...)` 只转录 accepted D11 命题的极小判决核心
（failure evidence 压过一切 → `Eof∧Drained`=Completed → aborted+边界
stop intent=Stopped、否则 Failed）。production resolver 的其余精确
precedence（如 `Aborted+Eof → Failed`、stage 文本）是 **current
realization**，不进本套件的 normative 不变式；current decision table 的
形式证据保留在 `f2-terminal-commit-boundary/`（historical）与 production
oracle tests。改变 precedence 若不改变外部命题无需回 authority review。

### verifier-only ghost/辅助变量（非 normative）

`decisionLatched` / `stopAtDecision` 是 formal ghost/history 变量，**不是
production architecture state**：它们只在"最后一块决定性证据落地"的那一步
记录"此刻 stop intent 是否已被记录"，使 late-command stability 可以被机器
检查（"未提交但已决定"的窗口里，晚到命令不得重释历史）。production 允许
在边界瞬间立即 settlement（从而根本不需要保留这个历史），或保存足以防止
late-command 重解释的最小 session-owned state——两条路 D11 都明示合法，
ghost 变量不要求任何 Rust 表示。`firstCommitted` / `authoritySettled` /
`observeRan` / `waitRan` 同类（auxiliary verifier-only）。

### 抽象里刻意保留的现实约束（不是额外假设）

1. **证据组合合法性**：`Drained ⇒ workerTerminal ∈ {None, Eof}`（render
   只在 edge terminal=Eof 后才可能报 Drained）；`decodeFailure ⇒
   workerTerminal ∈ {None, Failed}`——后半条是理想化顺序而非严格现实约束
   （decode_failed 与已放开的 stop edge 竞争可产生 decodeFailure ∧
   worker=Stopped 的组合；无害：模型与 production 都把 decode failure 判在
   第一位）。本不变式（`EvidenceConsistent`）是守卫一致性自查，**无独立
   mutation 负控制**（如实声明，不作为"有约束力"的证据引用）。
2. **activation 失败后不存在任何 leg**：证据发布动作一律要求
   `~activationFailed`（S8 的结构性来源）。已知 production corner：WASAPI
   open 失败/超时会 abort render 线程，使其在 activationFailed 的同时发布
   drain `Aborted`——语义等价（两侧都不产生 commit：production 的
   `resolve()` 在无 worker 证据时返回 None），本模型把这类情形折叠为
   "activation 失败下无证据"，不用于推理激活期机制时序。
3. **teardown 完成的两个守卫性质不同**：join 纪律（两条腿都已发布）是
   **生产现实**（join 返回 ⇒ 发布已执行），不是额外假设；settlement 边界
   （decisive 未提交时不得完成）是 **accepted D11-C5 要求**——今日生产
   可以在决定性证据未提交时完成 teardown（consumer-triggered commit
   differential，F2 修正目标），模型表达的是 accepted contract。
4. **teardown 两个阶段折叠**：两条 leg 的 join 中间态对 terminal
   settlement 问题不可观测。
5. **`CHECK_DEADLOCK FALSE`**：终局之后"环境停摆"是 `[][Next]_vars`
   允许的合法行为——没有任何参与者是 required 的，这正是 C2/W6 要表达的。
6. **已知粗粒度处（如实声明）**：`Activate` 是原子步骤；激活期间发布
   drain verdict 的极端时序折叠进 Activate 之后。对该问题不可观测。
7. **chronology 只有一维**：模型里唯一的时序轴是"stop intent 相对决定性
   边界的先后"（由 ghost 捕获）。production 的 `resolve()` 读调用瞬间的
   `stop_requested`（无边界概念）——该 differential 已由 D11 记录、F2 修正；
   模型表达的是 accepted contract，不是 differential。

### 范围声明（None-region 与证据形状是 model-scoped，非 normative 边界）

`TerminalCandidateOf` 只转录 accepted D11 命题的极小判决核心，因此：

- **未决定组合的 None-region 是本模型的抽象**，不是 normative 边界：
  production 当前 resolver 对部分组合（如 `Aborted+Eof → Failed`）比模型
  更"果断"。那属于 current realization（ADR-PBK-002 §17 明示 precedence
  非冻结），本套件不做 normative 约束，也因此不把"production 会判而模型
  未判"的组合当作缺陷。
- `CommittedOutcomeMatchesContract`（S9）相应取**单向蕴含**
  `Decisive ⇒ (outcome = None ∨ outcome = SettleCandidate)`：只约束
  "决定性证据上的提交值"，不把 None-region 升格为 normative。M3/M4 在该
  形式下仍然被击穿（runner 验证）。
- `NoFalseStopped`（S7）的 antecedent（`worker=Stopped ∧ drain=Aborted`）
  与 `TeardownRequiresSettlement`（S10）的 decisive 判定同样使用本模型的
  极小证据形状；同上，是 model-scoped 转录而非 production precedence 的
  冻结。

## writer 集合（谁可以写 terminalOutcome）

```text
terminal Fact 的唯一 None -> terminal writer:   AuthoritySettle
mechanism evidence writers（只写证据，从不写 outcome）:
    PublishDecodeFailure / PublishWorkerEof / PublishWorkerStopped /
    PublishWorkerFailed / PublishDrainDrained / PublishDrainAborted
forbidden writers（被不变式排除，mutation 证明有牙齿）:
    Observe（M1）、Wait（M2）、evidence 发布者顺手提交（M9）、
    late stop 改写已提交值（M3）
```

M9 的边界（重要）：它**不代表** production 永远不能在同一 Rust call
stack 内由 authority 完成 decision+commit。它检查的是 semantic writer
必须归属 authority 角色——evidence producer（decode worker / render leg
这样的 mechanism provider）不得因为自己的发布让判决变得决定性，就自己
成为另一个 authority（PBK-001 §2.3：mechanism observation 不得发布另一
authority 的 semantic fact）。

## 检查清单（`check.sh`，23 条 TLC run）

正常模型（2）：

| run | 内容 | 期望 |
| --- | --- | --- |
| `EpisodeTerminalSettlement.cfg` | 全部安全不变式 + `SettlementProgress`，`SPECIFICATION SpecSettlementFairness`（WF_vars(AuthoritySettle)） | PASS |
| `EpisodeTerminalSettlementSafetyOnly.cfg` | 同一不变式集，`SPECIFICATION Spec`（无 fairness） | PASS（safety 与进度假设解耦） |

负控制 mutation（9，全部必须产生 counterexample）：

| 文件 | 注入 | 期望击穿 |
| --- | --- | --- |
| `mutations/ObserveCommits.cfg` | 纯读偷偷提交（不登记 authoritySettled） | `OutcomeWrittenOnlyByAuthoritySettle`（S3/S4 非空洞） |
| `mutations/WaitCommits.cfg` | 纯等待调用 settlement（= 今日生产 differential 的形状） | 同上（S5 非空洞；与 M1 分开成 run） |
| `mutations/TerminalRewritable.cfg` | late stop 改写已提交 Completed/Failed | `TerminalOutcomeImmutable`（S2 非空洞） |
| `mutations/LateStopReadsCurrentIntent.cfg` | settlement 读当前 stopSeen（决定性已判 Failed → 晚到 stop → 提交 Stopped） | `CommittedOutcomeMatchesContract`（S9；#143 新规则约束力的核心 mutation，附带击穿 `NoFalseStopped`） |
| `mutations/TeardownBeforeSettlement.cfg` | 去掉 FinishTeardown 的 settlement 守卫 | `TeardownRequiresSettlement`（S10 非空洞） |
| `mutations/ActivationFailureBecomesFailed.cfg` | activation failure 升格 terminal Failed | `ActivationFailureIsNotTerminalFailed`（S8 非空洞） |
| `mutations/FalseCompleted.cfg` | 无 Eof+Drained 也判 Completed | `NoFalseCompleted`（S6 非空洞） |
| `mutations/FalseStopped.cfg` | 无边界 stop intent 也判 Stopped（stopRequestedNow 反模式） | `NoFalseStopped`（S7 非空洞） |
| `mutations/EvidenceProducerCommits.cfg` | evidence 发布者顺手写 outcome | `OutcomeWrittenOnlyByAuthoritySettle`（writer authority 非空洞） |

反向控制（5 + 1，必须被违反；overclaim 全部在带 WF 的最强让步下检查）：

| 文件 | 断言（模型不得宣称） | 期望 |
| --- | --- | --- |
| `mutations/OverclaimTermination.cfg` | `EveryEpisodeEventuallyTerminates` | liveness 反例 |
| `mutations/OverclaimActiveCompletes.cfg` | `EveryActiveEpisodeEventuallyCompletes` | liveness 反例 |
| `mutations/OverclaimStopStops.cfg` | `EveryStopEventuallyStops` | liveness 反例 |
| `mutations/OverclaimDecoderExits.cfg` | `EveryDecoderEventuallyExits` | liveness 反例 |
| `mutations/OverclaimDeviceDrains.cfg` | `EveryDeviceEventuallyDrains` | liveness 反例 |
| `probes/NoFairnessProgressFails.cfg` | 去掉 WF 后 `SettlementProgress` 必须失效 | liveness 反例（fairness 是承重的） |

可达性 witness（6，断言"不可达"必须被违反 = witness 找到）：

| 文件 | 合法路径 |
| --- | --- |
| `probes/WitnessNaturalCompletion.cfg` | W1 自然 EOF → Completed |
| `probes/WitnessUserStop.cfg` | W2 边界前 stop → Stopped |
| `probes/WitnessDeviceAbortFailed.cfg` | W3 无 stop 的设备 abort → Failed |
| `probes/WitnessLateStopCannotRelabel.cfg` | W4 决定性 Failed 后晚到 stop，settlement **仍是 Failed**（与 M4 成对：同一调度形状，正确版/错误版） |
| `probes/WitnessLateStopAfterCompleted.cfg` | W5 Completed 已提交 + 晚到 stop 共存（同 run `TerminalOutcomeImmutable` PASS） || `probes/WitnessNoConsumerCommit.cfg` | W6 Observe/Wait 从未运行，terminal Fact 仍建立 |

**读 TLC 结果的纪律**：`-continue` 的 violated 列表不是穷尽清单；mutation
cfg 的 INVARIANT 列表读作"本次检查了这些"，runner 只要求目标出现在
violated 集合里。

## 运行

```bash
specs/check.sh current                       # 含本套件（current formal gate）
specs/check.sh terminal                      # 仅本套件
specs/episode-terminal-settlement/check.sh   # 直接运行（23 条 TLC run）
```

工具链与其它套件共用 `specs/tools/tla2tools.jar`（v1.7.4，sha256 校验，
缺失自动下载）。任何 TLC Warning 即 FAIL（fail closed，无 whitelist）。

## Bounds / 假设（结论只在这个范围内成立）

- 状态空间：616 个 distinct state（穷举；无界参数，除 `Mutation` 常量外
  无其它常量）。
- 进度性质依赖其 `SPECIFICATION` 显式写出的 `WF_vars(AuthoritySettle)`；
  无 fairness 的 run 是反向控制/承重控制，不是结论。
- 不建模：K0 graph / Fiber 依赖 / Capability / ComponentSpec / PCM /
  WASAPI / decoder staging / seek / pause / open / Generation / Window /
  TimelineSegment / DataPlaneAuthority / PlaybackSessionHandle / UI /
  PlayerEngine / 线程调度 / 内存序 / 物理可听性。
- 单 episode、单次 teardown、无 multi-session / preload / gapless。
- 模型选择的是 **accepted contract 的极小判决核心**；production 当前
  precedence 的其余分支不在 normative 不变式内（见上文映射表）。
