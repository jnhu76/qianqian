# specs/episode-terminal-settlement — episode terminal settlement 的当前规范一致性模型

> **STATUS: CURRENT FORMAL GATE（current-spec conformance）。**
> 已接入 `specs/check.sh current` 与 CI `formal-semantic-gate`。
>
> **CORRECTIVE-1（review 响应）已合入**：fresh review 以 2 MAJOR + 1 CI
> BLOCKER 拒绝了初版 verdict。修正：
> ① decisive 触发域从 deliberately-minimal 判决域改为 current decision
>    contract 的全部可判决形状（`CurrentDecisionDecisive`；初版把
>    production 果断的 `Eof+Aborted → Failed(device)` 留在触发域外，使
>    C5/progress/latch 成为局部证明）；
> ② writer identity 从 ghost 布尔状态不变式改为 transition 级性质
>    （`AuthorityIsSoleWriter`；ghost 布尔可被"顺手维护 ghost"的冒写动作
>    骗过——M9 即该最大对手）；
> ③ CI BLOCKER：本 suite runner 在 git 中缺可执行位（100644），CI
>    `Permission denied`，已修。新增 W7 witness 与 M10 负控制。
>
> **CORRECTIVE-2（review 响应）已合入**：fresh review 确认 CORRECTIVE-1 的
> 2 MAJOR + CI BLOCKER 均已修复，但提出新 MAJOR——`completion.rs::resolve()`
> 与 `CurrentDecisionDecisive` 是两个 truth source，只有人工逐支核对的
> 快照相等，缺 durable binding（"TLA models do not re-run on implementation
> diffs" 使漂移可以静默通过 CI）。修正：**production ↔ formal 判决合同
> refinement oracle**（见下节）——单一共享真值表 artifact
> `CurrentDecisionTable.tla`（48 元组穷举、由 Rust oracle 从公开 seam 生成/
> 比对），TLC run 逐行校验表 ↔ formal 判决函数；completion.rs 与该
> artifact 进入两个 gate 的 trigger。W7/M10 证明的是"给定 formal ground
> truth，C5 对完整域承重"；ground truth ↔ production 的相等性由本 oracle
> 机器维护。
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

## production ↔ formal 判决合同 refinement oracle（CORRECTIVE-2）

问题（review MAJOR）：`completion.rs::resolve()` 与
`CurrentDecisionDecisive`/`CurrentDecisionVerdict` 是两个 truth source，
CORRECTIVE-1 的相等性只由 fresh reviewer 人工逐支核对——它不能承担永久
CI gate 的同步职责。production 判决分支增删后，Rust gate 仍绿、TLA 根本
不运行，旧 formal ground truth 继续被当作 CURRENT 证据。

修正：**一个共享真值表 artifact，两侧各由一个 gate 机器绑定**：

```text
production resolve()（经 SessionCompletion crate 内 seam 逐元组驱动；F2 起 oracle 为 crate-internal 白盒）
        │  Rust exhaustive oracle（48 元组穷举，byte-compare）
        │  crates/qianqian-playback/src/decision_table_oracle.rs
        ▼  Verification Rust Gate
specs/episode-terminal-settlement/CurrentDecisionTable.tla
（生成文件：<<stop_intent, decode_failure, worker_terminal,
  drain_verdict, class>> × 48 行；class ∈ undecided / completed /
  stopped / failed-decode / failed-device）
        │  TLC 穷举 run（48 初始状态逐行比对）
        │  EpisodeTerminalSettlementTable.tla + .cfg
        ▼  Formal Semantic Gate
CurrentDecisionDecisive / CurrentDecisionVerdict（主模型，语义不变）
```

class 保留 failed-decode / failed-device 细分（production `Failed.stage`
的 realization 命名）；TLC 侧比较时经 `OutcomeClassOf` 投影到模型 verdict
的 "Failed"——投影只用于比较，不改变模型语义。

漂移矩阵（谁改了什么、哪个 gate 拒绝）：

| 漂移 | 拒绝它的 gate / 机制 |
| --- | --- |
| `completion.rs` 判决分支增删/改值 | Rust gate：oracle byte-compare 失败，直到表重生成 |
| 手改 `CurrentDecisionTable.tla`（任何字节） | Rust gate（byte-compare）；投影可见的改值另被 Formal gate TLC run 击穿 |
| 改 TLA `CurrentDecisionDecisive`/`CurrentDecisionVerdict` | Formal gate：`TableDecisiveMatchesContract` / `TableVerdictMatchesContract` 违反 |
| 表有缺行/重复/畸形行 | Formal gate：`TableRowsWellFormed` **显式 bijection**（行数=域大小 ∧ 每行形状合法 ∧ 每个域 key 恰好一行）直接击穿——"48 行穷举、key 无缺无重"是模块自身的 theorem；`RowFor` 的 CHOOSE 只作查找（选择集非空且唯一已由不变式保证） |

trigger（两侧闭环）：`crates/qianqian-playback/src/completion.rs` 加入
Formal Semantic Gate；`specs/episode-terminal-settlement/CurrentDecisionTable.tla`
加入 Verification Rust Gate。判决合同变更的合法路径只有一条：authority
记录在案（ADR-PBK-002 §17 D11）→ `QIANQIAN_UPDATE_DECISION_TABLE=1`
重生成表 → `specs/check.sh terminal` TLC run 必须仍 PASS（formal 判决
函数同步重推导）。

负控制（本轮实际执行过，见 report.md）：删 `CurrentDecisionDecisive`
的 Aborted 支 → `TableDecisiveMatchesContract` 立即违反；表行改值
（failed-device→completed）→ `TableVerdictMatchesContract` 违反；表行
stage 级改值（failed-device→failed-decode，TLC 投影不可见）→ Rust
byte-compare 失败。三条漂移路径全部有牙齿。

边界：oracle 冻结的是**静态**判决合同（证据形状 → 判决类，intent 在
resolve 时刻固定）。production `resolve()` 读当前 stop intent 的 known
differential（late intent 重判）不在表域内——那是动态时序问题，由主模型
M4/W4 机器检查、D11 记录、F2 修正；本 oracle 不是它的替代品。

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
| C2 authority-owned settlement | 歌播完/失败后，**不需要有人来问**，authority 自己负责把终局落下来。Observe/Wait/Projection/App query 都不是 Fact 出现的必要条件 | safety + witness | `AuthorityIsSoleWriter`（S3，transition 级）+ `WitnessNoConsumerCommit`（W6）；mutation M1/M2/M9 |
| C3 Observe 纯读 | observe() 只能读；不能 resolve/commit/改证据/改 stop intent/改生命周期 | safety | `Observe` 只写 `observeRan`；mutation M1 |
| C4 Wait 纯等待 | wait() 只能等 authority 已经落下的 Fact；不跑 resolver、不是 terminal writer、不是 Fact 存在的前提。TLA 不模拟 OS blocking，只需 wait 动作无 semantic writer 效果 | safety | `Wait` 只写 `waitRan`；mutation M2（今日生产 `wait()->resolve()` differential 的形状，必须被抓住） |
| C5 teardown settlement 边界 | 如果 teardown 已经完成，那"早就足够判决"的 terminal Fact 不能还没落下来。这是 safety 边界，**不是**"每个 episode 最终都会 teardown"的进度承诺 | safety | `TeardownRequiresSettlement`（S10，条件形式；判定用全形状触发域）；mutation M5 / M10 |
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
| `observeRan` / `waitRan` / `firstCommitted` | **无 production 对应**（verifier-only 见证变量；`firstCommitted` 只服务改写检出） | — |

判决概念分两层（review corrective 轮确立，**不得混用**）：

```text
CurrentDecisionDecisive(df, wt, dv)
    current decision contract 在哪些证据形状上已能作出终局判决。
    settlement obligation 的触发域（decision 边界 latch / C5 门 /
    progress）是 normative 层：必须覆盖 current contract 能判决的
    全部形状。逐支对应 production resolve() 的分支；不可判决形状
    （如 Drained+worker 未退出/Stopped、Aborted+worker 未退出）在域外。

CurrentDecisionVerdict(sa, df, wt, dv)
    current contract 对可判决形状的判决值，在**边界冻结 stop intent**
    下求值。current realization conformance oracle：精确 precedence
    （Aborted+Eof→Failed(device)、stage 文本等）是 realization，
    决策 contract 演进时随 authority 变更重推导；与 production 的唯一
    刻意偏差是模型读边界 intent（D11 late-command rule），production
    resolve() 读当前 stop_requested（known differential，F2 修正目标）。

D11 的三条外部命题（Completed/Stopped 的成立条件、无虚构）由独立
不变式（S6/S7）承载，不从判决表导出——normative 层与 realization 层
分离。S9 用判决表做 conformance 检查，但触发域、C5、progress 的
义务结构不依赖表中具体取值。

### verifier-only ghost/辅助变量（非 normative）

`decisionLatched` / `stopAtDecision` 是 formal ghost/history 变量，**不是
production architecture state**：它们只在"最后一块决定性证据落地"的那一步
记录"此刻 stop intent 是否已被记录"，使 late-command stability 可以被机器
检查（"未提交但已决定"的窗口里，晚到命令不得重释历史）。production 允许
在边界瞬间立即 settlement（从而根本不需要保留这个历史），或保存足以防止
late-command 重解释的最小 session-owned state——两条路 D11 都明示合法，
ghost 变量不要求任何 Rust 表示。`firstCommitted` / `observeRan` /
`waitRan` 同类（auxiliary verifier-only）。

`authoritySettled` ghost 布尔已**退役**（CORRECTIVE-1）：它承载的
writer 集合检查可被"顺手维护 ghost"的冒写动作骗过，writer identity
改由 transition 级性质 `AuthorityIsSoleWriter` 承载（见下节）。

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
8. **一处保守超近似**：模型允许 `(decodeFailure, worker=None, drain=Drained)`
   这一 production 不可达组合（decode failure 会 fail 掉 edge，render leg
   只会 abort 不会 Drained）。保守方向无害：该形状在两侧都判 Failed
   （模型经 `df` 分支，production 经 `decode_failure` 分支），不产生任何
   可观测分类分歧。

### 范围声明（触发域两层边界，CORRECTIVE-1 后）

- **触发域（normative）**：`CurrentDecisionDecisive` 覆盖 current decision
  contract 能判决的**全部**形状，包括 production 果断的
  `Eof+Aborted → Failed(device)`（初版曾把它留在触发域外，使 C5/progress
  成为局部证明——CORRECTIVE-1 修正）。域外的形状是 current contract
  **真的不可判决**（如 `Drained` 而 worker 未退出），不是刻意收窄。
  M10 负控制证明该覆盖是真实约束：把机制门缩回 minimal 域，C5 立即被违反。
  措辞边界：W7/M10 证明的是"**给定 `CurrentDecisionDecisive` 这份 formal
  ground truth**，C5 等性质对完整域承重"；这份 ground truth 与 production
  判决合同的相等性由 refinement oracle 机器维护（上一节），不靠人工逐支
  核对。
- **判决值（realization oracle）**：`CurrentDecisionVerdict` 的精确
  precedence 是 current realization conformance oracle，不是 D11 冻结
  内容；决策 contract 演进时本表随 authority 变更重推导，触发域全覆盖、
  C5、progress 的义务结构不变。
- D11 外部命题由独立不变式（S6/S7）承载；`CommittedOutcomeMatchesContract`
  （S9）取单向蕴含 `Decisive ⇒ (outcome = None ∨ outcome = 判决值)`：
  settlement 执行前 outcome 为 None 合法。M3/M4 在该形式下仍被击穿。

## writer identity（谁可以写 terminalOutcome）

```text
terminal Fact 的唯一 None -> terminal writer:   AuthoritySettle
    —— 由 transition 级性质 S3 表达：
       OutcomeChangedOnlyByAuthority ==
           (terminalOutcome' # terminalOutcome) => AuthoritySettle
       （[][A]_vars 形式，TLC 以 Action property 检查、按名报告。）
mechanism evidence writers（只写证据，从不写 outcome）:
    PublishDecodeFailure / PublishWorkerEof / PublishWorkerStopped /
    PublishWorkerFailed / PublishDrainDrained / PublishDrainAborted
forbidden writers（mutation 证明有牙齿）:
    Observe（M1）、Wait（M2）、evidence 发布者冒充 authority 提交（M9）、
    late stop 改写已提交值（M3）
```

**为什么 S3 必须是 transition 级**（CORRECTIVE-1）：初版用 ghost 布尔
（`authoritySettled`）做状态不变式，被 review 证伪——一个"顺手把 ghost
也维护掉"的冒写动作可以骗过全部状态检查。状态谓词原则上表达不了"是哪个
动作写的"。M9 现在就是这个最大对手：值、边界 intent、`firstCommitted`
全部如实维护，**全部状态不变式保持绿色**，唯一能抓住它的是 S3
（runner 的 `tfail` 模式验证"temporal 按名违反 + 零状态违反"）。

M9 的边界（重要）：它**不代表** production 永远不能在同一 Rust call
stack 内由 authority 完成 decision+commit。它检查的是 semantic writer
必须归属 authority 角色——evidence producer（decode worker / render leg
这样的 mechanism provider）不得因为自己的发布让判决变得决定性，就自己
成为另一个 authority（PBK-001 §2.3：mechanism observation 不得发布另一
authority 的 semantic fact）。

## 检查清单（`check.sh`，26 条 TLC run + 1 条 Rust oracle run）

正常模型（3）：

| run | 内容 | 期望 |
| --- | --- | --- |
| `EpisodeTerminalSettlement.cfg` | 全部安全不变式 + `SettlementProgress` + `AuthorityIsSoleWriter`，`SPECIFICATION SpecSettlementFairness`（WF_vars(AuthoritySettle)） | PASS |
| `EpisodeTerminalSettlementSafetyOnly.cfg` | 全部安全不变式 + `AuthorityIsSoleWriter`（`SettlementProgress` 不在此 run——它是进度性质，需要 WF），`SPECIFICATION Spec`（无 fairness） | PASS（safety——含 S3——与进度假设解耦；S3 在无 fairness 的行为超集上成立，带 WF 时 a fortiori） |
| `EpisodeTerminalSettlementTable.cfg` | refinement oracle：`DecisionDomain` 全部 48 元组为初始状态，逐行比对 `CurrentDecisionTable`（production 冻结表）↔ `CurrentDecisionDecisive`/`CurrentDecisionVerdict` + 行完整性（含显式 key bijection） | PASS（CORRECTIVE-2；production 侧绑定由 Rust oracle run 承担） |

Rust oracle run（Verification Rust Gate 内执行，不在本 check.sh）：
`cargo test -p qianqian-playback --lib decision_table` ——
48 元组经 `SessionCompletion` crate 内 seam 驱动真实 `resolve()`，byte-compare
`CurrentDecisionTable.tla`（F2 迁移：evidence mutators 已收缩为 crate 私有，oracle 随之白盒化；判决合同与 48 行不变）。

负控制 mutation（10，全部必须产生 counterexample）：

| 文件 | 注入 | 期望击穿 |
| --- | --- | --- |
| `mutations/ObserveCommits.cfg` | 纯读偷偷提交（值正确，但 writer 不是 AuthoritySettle） | `AuthorityIsSoleWriter`（S3 非空洞；状态不变式全绿——writer identity 只能由 transition 级性质承载） |
| `mutations/WaitCommits.cfg` | 纯等待调用 settlement（= 今日生产 differential 的形状） | 同上（与 M1 分开成 run） |
| `mutations/TerminalRewritable.cfg` | late stop 改写已提交 Completed/Failed | `TerminalOutcomeImmutable`（S2 非空洞） |
| `mutations/LateStopReadsCurrentIntent.cfg` | settlement 读当前 stopSeen（决定性已判 Failed → 晚到 stop → 提交 Stopped） | `CommittedOutcomeMatchesContract`（S9；late-command rule 约束力的核心 mutation，附带击穿 `NoFalseStopped`） |
| `mutations/TeardownBeforeSettlement.cfg` | 去掉 FinishTeardown 的 settlement 守卫 | `TeardownRequiresSettlement`（S10 非空洞） |
| `mutations/ActivationFailureBecomesFailed.cfg` | activation failure 升格 terminal Failed | `ActivationFailureIsNotTerminalFailed`（S8 非空洞） |
| `mutations/FalseCompleted.cfg` | 无 Eof+Drained 也判 Completed | `NoFalseCompleted`（S6 非空洞） |
| `mutations/FalseStopped.cfg` | 无边界 stop intent 也判 Stopped（stopRequestedNow 反模式） | `NoFalseStopped`（S7 非空洞） |
| `mutations/EvidenceProducerSpoofsAuthority.cfg` | evidence 发布者冒充 authority：值/边界 intent/`firstCommitted` 全部如实维护的最大对手 | `AuthorityIsSoleWriter`（S3 按名违反；**零**状态不变式违反——ghost 状态无法承载 writer identity 的机器证明） |
| `mutations/NarrowDecisiveDomain.cfg` | teardown settlement 门缩回 deliberately-minimal 触发域（`Eof+Aborted` 落在门外），性质不动 | `TeardownRequiresSettlement`（S10 触发域覆盖是真实约束，非跟着定义空洞成立） |

反向控制（5 + 1，必须被违反；overclaim 全部在带 WF 的最强让步下检查）：

| 文件 | 断言（模型不得宣称） | 期望 |
| --- | --- | --- |
| `mutations/OverclaimTermination.cfg` | `EveryEpisodeEventuallyTerminates` | liveness 反例 |
| `mutations/OverclaimActiveCompletes.cfg` | `EveryActiveEpisodeEventuallyCompletes` | liveness 反例 |
| `mutations/OverclaimStopStops.cfg` | `EveryStopEventuallyStops` | liveness 反例 |
| `mutations/OverclaimDecoderExits.cfg` | `EveryDecoderEventuallyExits` | liveness 反例 |
| `mutations/OverclaimDeviceDrains.cfg` | `EveryDeviceEventuallyDrains` | liveness 反例 |
| `probes/NoFairnessProgressFails.cfg` | 去掉 WF 后 `SettlementProgress` 必须失效 | liveness 反例（fairness 是承重的） |

可达性 witness（7，断言"不可达"必须被违反 = witness 找到）：

| 文件 | 合法路径 |
| --- | --- |
| `probes/WitnessNaturalCompletion.cfg` | W1 自然 EOF → Completed |
| `probes/WitnessUserStop.cfg` | W2 边界前 stop → Stopped |
| `probes/WitnessDeviceAbortFailed.cfg` | W3 无 stop 的设备 abort → Failed |
| `probes/WitnessLateStopCannotRelabel.cfg` | W4 决定性 Failed 后晚到 stop，settlement **仍是 Failed**（与 M4 成对：同一调度形状，正确版/错误版） |
| `probes/WitnessLateStopAfterCompleted.cfg` | W5 Completed 已提交 + 晚到 stop 共存（同 run `TerminalOutcomeImmutable` PASS） |
| `probes/WitnessNoConsumerCommit.cfg` | W6 Observe/Wait 从未运行，terminal Fact 仍建立 |
| `probes/WitnessEofAbortedFailed.cfg` | W7 Eof+Aborted 全程无 stop → Failed(device)（CORRECTIVE-1 收回触发域的 production-decisive 分支） |

**读 TLC 结果的纪律**：`-continue` 的 violated 列表不是穷尽清单；mutation
cfg 的 INVARIANT 列表读作"本次检查了这些"，runner 只要求目标出现在
violated 集合里。

## 运行

```bash
specs/check.sh current                       # 含本套件（current formal gate）
specs/check.sh terminal                      # 仅本套件
specs/episode-terminal-settlement/check.sh   # 直接运行（26 条 TLC run）
cargo test -p qianqian-playback --lib decision_table
                                             # production ↔ 表 refinement
                                             # oracle（Rust gate 侧；F2 起 crate-internal）
```

工具链与其它套件共用 `specs/tools/tla2tools.jar`（v1.7.4，sha256 校验，
缺失自动下载）。任何 TLC Warning 即 FAIL（fail closed，无 whitelist）。

## Bounds / 假设（结论只在这个范围内成立）

- 状态空间：652 个 distinct state（穷举；无界参数，除 `Mutation` 常量外
  无其它常量）。refinement-oracle run 另有 48 个初始状态（决策元组域
  穷举，纯枚举器、无行为深度）。
- 进度性质依赖其 `SPECIFICATION` 显式写出的 `WF_vars(AuthoritySettle)`；
  无 fairness 的 run 是反向控制/承重控制，不是结论。
- 不建模：K0 graph / Fiber 依赖 / Capability / ComponentSpec / PCM /
  WASAPI / decoder staging / seek / pause / open / Generation / Window /
  TimelineSegment / DataPlaneAuthority / PlaybackSessionHandle / UI /
  PlayerEngine / 线程调度 / 内存序 / 物理可听性。
- 单 episode、单次 teardown、无 multi-session / preload / gapless。
- 触发域/判决值分两层：settlement obligation 的触发域是 normative
  （全形状覆盖）；判决值表是 current realization conformance oracle
  （见上文"判决概念分两层"）。
