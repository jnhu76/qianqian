--------------------------- MODULE EpisodeTerminalSettlement ---------------------------
(***************************************************************************)
(* EpisodeTerminalSettlement — 一个 playback episode 的 terminal settlement   *)
(*                             contract 的当前规范一致性模型                  *)
(*                                                                         *)
(* STATUS: CURRENT-SPEC FORMAL CONFORMANCE（current formal gate 候选）。      *)
(*   本模型**不是第二份 authority**。语义真相只有：                          *)
(*     ADR-PBK-001 §2（semantic commit / fact authority identity /          *)
(*                   projection 非权威）                                    *)
(*     ADR-PBK-002 §17 D11（episode terminal outcome authority + settlement  *)
(*                   ownership + late-command stability + 三条外部命题）     *)
(*   它只回答一个问题：当前已接受的 D11 contract 能否被一个极小、非空洞的     *)
(*   TLA+ 模型一致表达，并且 mutation 能证明这些约束真的有约束力。           *)
(*                                                                         *)
(* 与 f2-terminal-commit-boundary（historical / exploratory evidence）的     *)
(* 区别：本模型不比较 A/B/B′ 提交所有权变体。exploration 已完成，            *)
(* D11 settlement corrective 已完成 authority decision：                     *)
(*   decision ownership + commit-progress ownership 都归 Playback Session    *)
(*   semantic authority；decision（分类边界）与 semantic commit 执行可以      *)
(*   分开，不要求 atomic-with-evidence（B′ 不是 production requirement）。   *)
(*                                                                         *)
(* 本模型不建模（刻意的最小化）：K0 graph / Fiber 依赖 /                    *)
(* Capability / PCM ring / WASAPI / Generation / Window / TimelineSegment /  *)
(* DataPlaneAuthority / PlaybackSessionHandle / UI / PlayerEngine。          *)
(* 本轮只有 one episode terminal semantics。不选择任何 Rust representation。 *)
(* 模型结论只在其显式 abstraction 与 assumptions 下成立，不得静默升级为      *)
(* 架构权威（AGENTS.md "Verification authority boundary"）。                 *)
(*                                                                         *)
(* 模型词 -> current production reality 映射（完整表见 README.md）：          *)
(*   Activate                  session 激活成功（edge 绑定、render stream    *)
(*                             打开、decode worker 已 spawn）                *)
(*   PublishActivationFailure  SessionCompletion::activation_failed          *)
(*                             （diagnostic，不是 D11 Failed）               *)
(*   RequestStop               SessionCompletion::request_stop（command，     *)
(*                             不是 fact；单调 FALSE -> TRUE）               *)
(*   PublishDecodeFailure      SessionCompletion::decode_failed（first-wins） *)
(*   PublishWorkerEof/         SessionCompletion::worker_exited(edge.terminal)*)
(*   Stopped/Failed            —— worker 退出前最后一步                      *)
(*   PublishDrainDrained/      DrainSignal::complete（render 线程退出前必发；*)
(*   Aborted                   stop_and_join 返回 => verdict 已发布）        *)
(*   BeginTeardown /           session fiber 的 effect 逆序释放（LIFO：       *)
(*   FinishTeardown            stop edge + join worker -> stop_and_join      *)
(*                             stream）。守卫 1（join 纪律）是生产现实；      *)
(*                             守卫 2（settlement 边界）是 accepted D11-C5    *)
(*                             要求（见动作注释）。                           *)
(*   AuthoritySettle           Playback Session semantic authority 的         *)
(*                             settlement 动作（当前 Rust 中由               *)
(*                             consumer 调用路径代为触发——那是 D11 已记录的   *)
(*                             known differential，F2 修正目标；本模型按      *)
(*                             accepted contract 建模 authority-owned）。     *)
(*   Observe / Wait            纯读 / 纯等待（D14.2 seam 语义；当前生产       *)
(*                             wait() 会顺手 resolve——known differential，   *)
(*                             M2 mutation 形式化地证明它不满足当前 contract）。*)
(*                                                                         *)
(* 关键状态区分（绝不合并）：                                                *)
(*   evidence 已发布 != decision 已锁定 != terminal outcome 已 commit        *)
(*   stop intent（command，stopSeen）!= decision 边界时的 stop intent        *)
(*                                   （ghost：stopAtDecision）               *)
(*   activation failure（diagnostic）!= terminal Failed（fact）              *)
(*   teardown 完成 != fact 已存在                                            *)
(*                                                                         *)
(* stopAtDecision 是 verifier-only ghost/history 变量：它在最后一块决定性     *)
(* evidence 落地的同一步记录"此刻 stop intent 是否已被记录"，用于机器检查     *)
(* late-command stability（D11：晚到命令不得重释已决定性的 mechanism 历史）。 *)
(* 它**不是** production architecture state，不要求 production 建立对应      *)
(* 表示；production 允许在边界瞬间立即 settlement，从而不需要保留它          *)
(* （ADR：最小实现可以在最后一块决定性 evidence 到达时立即 settlement）。    *)
(*                                                                         *)
(* 负控制（Mutation 常量，逐个独立注入；稳定语义名）：                        *)
(*   MutationObserveCommits            纯读动作偷偷提交（M1）                 *)
(*   MutationWaitCommits               纯等待动作偷偷提交（M2；即今日生产      *)
(*                                     wait()->resolve() differential 的形状）*)
(*   MutationTerminalRewritable        已提交终局被 late stop 改写（M3）      *)
(*   MutationLateStopReadsCurrentIntent settlement 读取当前 stopSeen 而非     *)
(*                                     决策边界记录值（M4 —— D11 late-command *)
(*                                     stability 约束力的核心 mutation）     *)
(*   MutationTeardownBeforeSettlement  teardown 在决定性证据未提交时完成（M5） *)
(*   MutationActivationFailureBecomesFailed  activation failure 升格为        *)
(*                                     D11 Failed（M6）                      *)
(*   MutationFalseCompleted            无 Eof+Drained 也判 Completed（M7）    *)
(*   MutationFalseStopped              无边界 stop intent 也判 Stopped（M8）  *)
(*   MutationEvidenceProducerSpoofsAuthority                                           *)
(*                                     evidence 发布者冒充 authority 提交：   *)
(*                                     值/边界 intent/firstCommitted 全部如实 *)
(*                                     维护，状态不变式全绿，只有 S3 的       *)
(*                                     transition 级性质能抓住（M9）          *)
(*   MutationNarrowDecisiveDomain      把 teardown settlement 门缩回          *)
(*                                     deliberately-minimal 触发域：C5 必须    *)
(*                                     抓住（证明 C5 触发域覆盖是真实的）（M10）*)

(* review corrective round 记录（不改变 authority，只修正模型两处不足）：     *)
(*   1. decisive 触发域从 deliberately-minimal 判决域改为 current decision    *)
(*      contract 的**全部**可判决形状（CurrentDecisionDecisive）；上一轮把    *)
(*      production 果断的 Eof+Aborted -> Failed(device) 留在触发域外，使      *)
(*      C5 / progress / latch 成为局部证明。判决**值**表仍是 current          *)
(*      realization conformance oracle（非 normative precedence）。           *)
(*   2. writer identity 从 ghost 布尔状态不变式改为 transition 级性质         *)
(*      （OutcomeChangedOnlyByAuthority）：ghost 布尔可被"顺手维护 ghost"的   *)
(*      冒写动作骗过（M9 即该最大对手）。                                     *)
(*   新增 W7（Eof+Aborted -> Failed witness）与 M10（触发域收窄负控制）。     *)
(*                                                                         *)
(* 本模型不检查 TLC deadlock：终局之后"环境停摆"是 [][Next]_vars 允许的      *)
(* 合法行为，也是本模型要表达的东西（没有任何人是 required 的）。            *)
(***************************************************************************)

EXTENDS Integers, TLC

CONSTANTS
    \* 负控制开关（每条 cfg 恰好注入一个）。
    Mutation,
    MutationNone,
    MutationObserveCommits,
    MutationWaitCommits,
    MutationTerminalRewritable,
    MutationLateStopReadsCurrentIntent,
    MutationTeardownBeforeSettlement,
    MutationActivationFailureBecomesFailed,
    MutationFalseCompleted,
    MutationFalseStopped,
    MutationEvidenceProducerSpoofsAuthority,
    MutationNarrowDecisiveDomain

MutationChoices == {MutationNone,
                    MutationObserveCommits,
                    MutationWaitCommits,
                    MutationTerminalRewritable,
                    MutationLateStopReadsCurrentIntent,
                    MutationTeardownBeforeSettlement,
                    MutationActivationFailureBecomesFailed,
                    MutationFalseCompleted,
                    MutationFalseStopped,
                    MutationEvidenceProducerSpoofsAuthority,
                    MutationNarrowDecisiveDomain}

ASSUME Mutation \in MutationChoices

VARIABLES
    \* —— 机制证据（发布即成立，write-once / 单调，与 current realization 相符）——
    stopSeen,               \* stop intent：command 状态，FALSE -> TRUE 单调；不是 fact
    decodeFailure,          \* write-once
    workerTerminal,         \* write-once: "None" | "Eof" | "Stopped" | "Failed"
    drainVerdict,           \* write-once: "None" | "Drained" | "Aborted"
    activationFailed,       \* write-once diagnostic（不是 D11 Failed）
    \* —— terminal Fact 与 episode 生命周期 ——
    terminalOutcome,        \* "None" | "Completed" | "Stopped" | "Failed"
    episodeLifecycle,       \* BeforeActivation | Active | TeardownStarted | TeardownDone
    \* —— decision boundary ghost（verifier-only，非 production state）——
    decisionLatched,        \* 决定性边界是否已经到达（write-once FALSE -> TRUE）
    stopAtDecision,         \* 边界时刻已记录的 stop intent（latch 时冻结）
    \* —— 以下是 verifier-only 历史/见证变量（auxiliary，非 normative，        *)
    \*    不要求 production 具有任何对应表示 ——                            *)
    firstCommitted,         \* 第一次提交写入的值（write-once），用于检出改写
    observeRan,             \* 外部纯读是否至少发生过一次
    waitRan                 \* 外部等待是否至少发生过一次
    \* 注：上一轮的 authoritySettled ghost 布尔已退役——它是被 review 证伪的    *)
    \*    writer 证明形状（冒写动作顺手维护 ghost 即可骗过；M9 即该对手）。      *)
    \*    writer identity 现由 transition 级性质 S3 承载（见安全不变式节）。     *)

vars == <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
          activationFailed, terminalOutcome, episodeLifecycle,
          decisionLatched, stopAtDecision,
          firstCommitted, observeRan, waitRan>>

WorkerTerminals == {"None", "Eof", "Stopped", "Failed"}
DrainVerdicts == {"None", "Drained", "Aborted"}
Outcomes == {"None", "Completed", "Stopped", "Failed"}
Lifecycles == {"BeforeActivation", "Active", "TeardownStarted", "TeardownDone"}

-----------------------------------------------------------------------------
(******************************** 初始状态 **********************************)

Init ==
    /\ stopSeen = FALSE
    /\ decodeFailure = FALSE
    /\ workerTerminal = "None"
    /\ drainVerdict = "None"
    /\ activationFailed = FALSE
    /\ terminalOutcome = "None"
    /\ episodeLifecycle = "BeforeActivation"
    /\ decisionLatched = FALSE
    /\ stopAtDecision = FALSE
    /\ firstCommitted = "None"
    /\ observeRan = FALSE
    /\ waitRan = FALSE

-----------------------------------------------------------------------------
(************************ 语义判决函数（纯函数） *****************************)

\* —— 概念拆分（review corrective 轮确立，两层不得混用）—————
\*
\* (1) CurrentDecisionDecisive —— current decision contract 在哪些证据形状上
\*     已经能作出终局判决。settlement obligation 的触发域（decision 边界
\*     latch / C5 teardown 门 / progress）是 normative 的：D11 冻结
\*     "decisive evidence 出现 => authority 必须 settlement"，而 decisive
\*     必须覆盖 current contract 能判决的**全部**形状——把 production 已
\*     果断的分支留在触发域外会把 C5/progress 变成局部证明。
\*     每个析取支映射 production resolver（SessionCompletion::resolve）的
\*     一个分支，四条全覆盖：
\*       decode failure 最先判（drain-independent）
\*       worker terminal = Failed（drain-independent）
\*       drain = Drained + worker = Eof  -> Completed
\*       drain = Aborted + worker 已退出（Stopped / Eof / Failed 均可判决；
\*       其中 Eof -> Failed(device) 是上一轮被留在触发域外的分支）
\*     反例（不可判决，不在域内）：Drained + worker 未退出或 Stopped；
\*     Aborted + worker 未退出。
CurrentDecisionDecisive(df, wt, dv) ==
    df
    \/ wt = "Failed"
    \/ (dv = "Drained" /\ wt = "Eof")
    \/ (dv = "Aborted" /\ wt # "None")

\* (2) CurrentDecisionVerdict —— current contract 对可判决形状的判决值，
\*     在**决策边界冻结的 stop intent**（sa）下求值。这是 current
\*     realization 的 conformance oracle：精确 precedence 是 realization，
\*     决策 contract 演进时本表随 authority 变更重推导；normative 层是
\*     (1) 的触发域全覆盖 + S6/S7 承载的 D11 外部命题，不是这张表的
\*     具体取值。与 production 的唯一刻意偏差：production resolve() 读
\*     **当前** stop_requested（known differential，F2 修正目标）；本模型
\*     按 D11 late-command rule 读边界 intent。
CurrentDecisionVerdict(sa, df, wt, dv) ==
    IF ~CurrentDecisionDecisive(df, wt, dv) THEN "None"
    ELSE IF df THEN "Failed"
    ELSE IF wt = "Failed" THEN "Failed"
    ELSE IF dv = "Drained" THEN "Completed"      \* 域内必有 wt = "Eof"
    ELSE IF wt = "Stopped" /\ sa THEN "Stopped"  \* 此处 dv = "Aborted"
    ELSE "Failed"                                \* Aborted + Eof；Aborted + Stopped 无边界 stop

\* 上一轮 deliberately-minimal 触发域。现在只作为 M10 负控制的变异对象：
\* 把 teardown settlement 门缩回这个域时 C5 必须被违反（证明 C5 的触发域
\* 覆盖是真实约束，不是跟着模型定义空洞成立）。
MinimalCandidateDomain(df, wt, dv) ==
    df \/ wt = "Failed"
    \/ (wt = "Eof" /\ dv = "Drained")
    \/ (wt = "Stopped" /\ dv = "Aborted")

\* M7 / M8 的注入口：劫持判决值（不动触发域）。正常模型下与
\* CurrentDecisionVerdict 恒等。
SettleVerdict(sa, df, wt, dv) ==
    CASE Mutation = MutationFalseCompleted /\ wt = "Eof" -> "Completed"
      [] Mutation = MutationFalseStopped /\ wt = "Stopped" /\ dv = "Aborted" -> "Stopped"
      [] OTHER -> CurrentDecisionVerdict(sa, df, wt, dv)

\* 用决策边界时刻的 stop intent（ghost 值）算出的当前判决。
\* evidence write-once => 一旦判决锁定，判决函数值不再变化。
SettleCandidate == SettleVerdict(stopAtDecision, decodeFailure, workerTerminal, drainVerdict)

\* 当前证据是否已处于 settlement obligation 的触发域（normative 层）。
Decisive == CurrentDecisionDecisive(decodeFailure, workerTerminal, drainVerdict)

\* teardown settlement 门使用的 decisive 判定。正常模型下与 Decisive 恒等；
\* M10 让它缩回 MinimalCandidateDomain（变异的是机制门，不是性质——性质
\* 仍以全形状 Decisive 为准，这正是该负控制的检验点）。
SettlementGateDecisive ==
    IF Mutation = MutationNarrowDecisiveDomain
    THEN MinimalCandidateDomain(decodeFailure, workerTerminal, drainVerdict)
    ELSE Decisive

\* 决定性边界是否在"这块证据落地"的瞬间首次到达。
\* 触发域与 stop intent 无关（见 CurrentDecisionDecisive），因此 latch 判定
\* 不读 intent；intent 的先后关系由 StopAtBoundary 在边界瞬间冻结。
BecameDecisive(dfN, wtN, dvN) ==
    /\ ~decisionLatched
    /\ CurrentDecisionDecisive(dfN, wtN, dvN)

\* 边界时刻冻结下来的 stop intent：边界刚到就用"此刻已记录的 intent"，
\* 之前已 latch 则保持不变。
StopAtBoundary(dfN, wtN, dvN) ==
    IF BecameDecisive(dfN, wtN, dvN) THEN stopSeen ELSE stopAtDecision

-----------------------------------------------------------------------------
(***************************** 环境/机制动作 ********************************)

\* session 激活成功：两个 leg 从此存在（reality: 先 render stream 后 worker）。
Activate ==
    /\ episodeLifecycle = "BeforeActivation"
    /\ ~activationFailed
    /\ episodeLifecycle' = "Active"
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome, decisionLatched,
                   stopAtDecision, firstCommitted, observeRan, waitRan>>

\* 激活失败：diagnostic，不是 terminal outcome 权威（D11 firewall）。
\* 负控制 MutationActivationFailureBecomesFailed 让它把 activation failure
\* 直接升格成 D11 terminal Failed（并如实登记提交者，使 S8 成为被击穿的
\* 性质，不与 writer-set 不变式混在一起）。
PublishActivationFailure ==
    LET escalated == Mutation = MutationActivationFailureBecomesFailed
    IN  /\ episodeLifecycle = "BeforeActivation"
        /\ ~activationFailed
        /\ activationFailed' = TRUE
        /\ terminalOutcome' = IF escalated THEN "Failed" ELSE terminalOutcome
        /\ firstCommitted' = IF escalated /\ firstCommitted = "None"
                             THEN "Failed" ELSE firstCommitted
        /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                       episodeLifecycle, decisionLatched, stopAtDecision,
                       observeRan, waitRan>>

\* stop intent：command，不是 fact。first-wins；不反向改写已提交终局。
\* 晚到的 stop（决定性边界之后）只进入 command history，不得改变分类
\* （D11 late-command rule）——这条由 stopAtDecision ghost + M4/M8 机器检查。
\* 负控制 MutationTerminalRewritable 让它改写已提交的 Completed/Failed。
RequestStop ==
    LET rewritten ==
            Mutation = MutationTerminalRewritable
            /\ terminalOutcome \in {"Completed", "Failed"}
    IN  /\ episodeLifecycle \in {"BeforeActivation", "Active", "TeardownStarted"}
        /\ stopSeen = FALSE
        /\ stopSeen' = TRUE
        /\ terminalOutcome' = IF rewritten THEN "Stopped" ELSE terminalOutcome
        /\ UNCHANGED <<decodeFailure, workerTerminal, drainVerdict, activationFailed,
                       episodeLifecycle, decisionLatched, stopAtDecision,
                       firstCommitted, observeRan, waitRan>>

\* —— 证据发布。每个动作在"这块证据落地"的同一步执行决定性边界 latch
\*    （decisionLatched / stopAtDecision），但**从不**写 terminalOutcome：
\*    classification boundary 与 semantic commit 执行是分离的（ADR-PBK-002
\*    §17 D11 冻结），
\*    只有 AuthoritySettle 是 None -> terminal 的 writer（S3）。
\*    全部发布动作都要求 ~activationFailed：激活失败时不存在任何 leg，
\*    因而 runtime evidence 结构上不可能产生（这也是 S8 成立的原因）。

\* decode failure：worker / panic guard 发布，first-wins。
PublishDecodeFailure ==
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ ~activationFailed
    /\ decodeFailure = FALSE
    /\ workerTerminal \in {"None", "Failed"}
    /\ decodeFailure' = TRUE
    /\ decisionLatched' = (decisionLatched \/ BecameDecisive(TRUE, workerTerminal, drainVerdict))
    /\ stopAtDecision' = StopAtBoundary(TRUE, workerTerminal, drainVerdict)
    /\ UNCHANGED <<stopSeen, workerTerminal, drainVerdict, activationFailed,
                   terminalOutcome, episodeLifecycle, firstCommitted,
                   observeRan, waitRan>>

\* evidence 发布者冒充 authority 的负控制注入口（M9）：sneak 触发时，
\* 发布动作执行 AuthoritySettle 的**全部**状态效果——判决值按边界 intent
\* 正确计算、firstCommitted 如实维护。结果：全部状态不变式（S2/S6/S7/S9/
\* C5……）保持绿色，唯一能抓住它的是 S3 的 transition 级性质。这正是
\* writer identity 不能用 ghost 状态表达、必须落在"改变 outcome 的
\* transition 必须就是 AuthoritySettle"上的机器证明。
PublishWorkerEof ==
    LET spoof ==
            /\ Mutation = MutationEvidenceProducerSpoofsAuthority
            /\ terminalOutcome = "None"
            /\ CurrentDecisionDecisive(decodeFailure, "Eof", drainVerdict)
        cand ==
            SettleVerdict(StopAtBoundary(decodeFailure, "Eof", drainVerdict),
                          decodeFailure, "Eof", drainVerdict)
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ workerTerminal = "None"
        /\ ~decodeFailure
        /\ workerTerminal' = "Eof"
        /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, "Eof", drainVerdict))
        /\ stopAtDecision' = StopAtBoundary(decodeFailure, "Eof", drainVerdict)
        /\ terminalOutcome' = IF spoof THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF spoof /\ firstCommitted = "None"
                             THEN cand ELSE firstCommitted
        /\ UNCHANGED <<stopSeen, decodeFailure, drainVerdict, activationFailed,
                       episodeLifecycle, observeRan, waitRan>>

PublishWorkerStopped ==
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ ~activationFailed
    /\ workerTerminal = "None"
    /\ ~decodeFailure
    /\ drainVerdict # "Drained"          \* Drained 蕴含 edge terminal = Eof
    /\ workerTerminal' = "Stopped"
    /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, "Stopped", drainVerdict))
    /\ stopAtDecision' = StopAtBoundary(decodeFailure, "Stopped", drainVerdict)
    /\ UNCHANGED <<stopSeen, decodeFailure, drainVerdict, activationFailed,
                   terminalOutcome, episodeLifecycle, firstCommitted,
                   observeRan, waitRan>>

PublishWorkerFailed ==
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ ~activationFailed
    /\ workerTerminal = "None"
    /\ ~decodeFailure
    /\ drainVerdict # "Drained"
    /\ workerTerminal' = "Failed"
    /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, "Failed", drainVerdict))
    /\ stopAtDecision' = StopAtBoundary(decodeFailure, "Failed", drainVerdict)
    /\ UNCHANGED <<stopSeen, decodeFailure, drainVerdict, activationFailed,
                   terminalOutcome, episodeLifecycle, firstCommitted,
                   observeRan, waitRan>>

PublishDrainDrained ==
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ ~activationFailed
    /\ drainVerdict = "None"
    /\ workerTerminal \in {"None", "Eof"}   \* Drained 只在 edge 到达 EOF 后可能
    /\ drainVerdict' = "Drained"
    /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, workerTerminal, "Drained"))
    /\ stopAtDecision' = StopAtBoundary(decodeFailure, workerTerminal, "Drained")
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, activationFailed,
                   terminalOutcome, episodeLifecycle, firstCommitted,
                   observeRan, waitRan>>

PublishDrainAborted ==
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ ~activationFailed
    /\ drainVerdict = "None"
    /\ drainVerdict' = "Aborted"
    /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, workerTerminal, "Aborted"))
    /\ stopAtDecision' = StopAtBoundary(decodeFailure, workerTerminal, "Aborted")
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, activationFailed,
                   terminalOutcome, episodeLifecycle, firstCommitted,
                   observeRan, waitRan>>

-----------------------------------------------------------------------------
(******************************** teardown 边界 ******************************)

\* teardown 的两个阶段（两条 leg 的 join）在模型中折叠；中间态对 terminal
\* settlement 问题不可观测。
BeginTeardown ==
    /\ \/ episodeLifecycle = "Active"
       \/ (episodeLifecycle = "BeforeActivation" /\ activationFailed)
    /\ episodeLifecycle' = "TeardownStarted"
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome, decisionLatched,
                   stopAtDecision, firstCommitted, observeRan, waitRan>>

\* teardown 完成。两个守卫的性质不同：
\*
\* 1. join 纪律（**生产现实**，不是额外假设）：reality 中 join 返回时
\*    worker 必然已执行 worker_exited、stop_and_join 返回时 render 线程
\*    必然已执行 drain.complete（session.rs 逆序 LIFO + wasapi.rs
\*    run_render_thread 退出前必发 verdict），所以"两条腿都已发布"是
\*    teardown 完成的前置条件。
\* 2. settlement 边界（**accepted D11-C5 要求**，不是今日生产现实）：
\*    decisive evidence 存在而 outcome 未提交时，teardown 不允许完成。
\*    今日生产可以在决定性证据未提交时完成 teardown（consumer-triggered
\*    commit differential，见本文件头部），F2 将把 commit progress 收回
\*    authority 自己的执行/teardown path。settlement 本身是 authority 的
\*    独立动作（AuthoritySettle），可以在 Active 或 TeardownStarted 期间
\*    发生——"settlement 挂在 teardown path 上"的最小实现即这两步的先后。
\*    负控制 MutationTeardownBeforeSettlement 去掉守卫 2。
\*    守卫 2 的 decisive 判定走 SettlementGateDecisive：M10 负控制把它缩回
\*    deliberately-minimal 触发域（Eof+Aborted 落在门外），C5 不变式仍按
\*    全形状 Decisive 检查——证明 C5 的触发域覆盖是真实约束。
FinishTeardown ==
    /\ episodeLifecycle = "TeardownStarted"
    /\ (   activationFailed
         \/ (workerTerminal # "None" /\ drainVerdict # "None"))
    /\ (   Mutation = MutationTeardownBeforeSettlement
         \/ activationFailed
         \/ ~SettlementGateDecisive
         \/ terminalOutcome # "None")
    /\ episodeLifecycle' = "TeardownDone"
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome, decisionLatched,
                   stopAtDecision, firstCommitted, observeRan, waitRan>>

-----------------------------------------------------------------------------
(****************************** authority / 消费侧动作 ***********************)

\* Playback Session semantic authority 的 settlement 动作：D11 冻结的
\* None -> terminal 唯一 writer。它用**决策边界冻结的** stopAtDecision
\* 计算分类，不用当前 stopSeen（late-command stability）。
\* 负控制 MutationLateStopReadsCurrentIntent 让它读取当前 stopSeen——
\* 即"晚到 stop 把已决定性 Failed 改判 Stopped"的缺陷形状。
AuthoritySettle ==
    /\ ~activationFailed
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ decisionLatched
    /\ Decisive
    /\ terminalOutcome = "None"
    /\ terminalOutcome' =
           IF Mutation = MutationLateStopReadsCurrentIntent
           THEN SettleVerdict(stopSeen, decodeFailure, workerTerminal, drainVerdict)
           ELSE SettleCandidate
    /\ firstCommitted' = terminalOutcome'
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle, decisionLatched,
                   stopAtDecision, observeRan, waitRan>>

\* 纯读（D14.2 observe 语义）：只改 observeRan，永不写任何语义状态。
\* M1 负控制让它偷偷提交——值完全正确，但写入 transition 不是
\* AuthoritySettle，S3 的 transition 级性质必须发现它。
Observe ==
    /\ observeRan' = TRUE
    /\ terminalOutcome' =
           IF /\ Mutation = MutationObserveCommits
              /\ ~activationFailed
              /\ terminalOutcome = "None"
              /\ Decisive
           THEN SettleCandidate
           ELSE terminalOutcome
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle, decisionLatched,
                   stopAtDecision, firstCommitted, waitRan>>

\* 纯等待（D14.2 wait-for-terminal 语义）：只改 waitRan。TLA 不模拟 OS
\* blocking；语义要求只有一条——wait 没有任何 semantic writer 效果，
\* 也不是 settlement 发生的必要条件（W6）。
\* M2 负控制让它调用 settlement（这正是今日生产 wait()->resolve()
\* differential 的形状；当前 contract 下它必须被抓住）。
Wait ==
    /\ waitRan' = TRUE
    /\ terminalOutcome' =
           IF /\ Mutation = MutationWaitCommits
              /\ ~activationFailed
              /\ terminalOutcome = "None"
              /\ Decisive
           THEN SettleCandidate
           ELSE terminalOutcome
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle, decisionLatched,
                   stopAtDecision, firstCommitted, observeRan>>

-----------------------------------------------------------------------------
(************************** 次态关系与公平性 *********************************)

Next ==
    \/ Activate
    \/ PublishActivationFailure
    \/ RequestStop
    \/ PublishDecodeFailure
    \/ PublishWorkerEof
    \/ PublishWorkerStopped
    \/ PublishWorkerFailed
    \/ PublishDrainDrained
    \/ PublishDrainAborted
    \/ BeginTeardown
    \/ FinishTeardown
    \/ AuthoritySettle
    \/ Observe
    \/ Wait

\* 进度假设（显式声明，不偷设）。spec 由 cfg 通过 SPECIFICATION 选择，
\* 使"这次运行假设了谁的 fairness"在配置里直接可见：
\*
\*   Spec                    无 fairness —— safety run 与反向控制用。
\*   SpecSettlementFairness  WF_vars(AuthoritySettle) —— 条件性进度：
\*       "decisive evidence 已存在 + episode authority 仍在
\*        Active/TeardownStarted + settlement 动作持续使能
\*        => 最终提交"。这是关于**系统自己**的假设，不涉及外部参与者。
\*
\* 刻意**不**宣称（Overclaim 系列必须被违反）：
\*   NOT EveryEpisodeEventuallyTerminates（证据可以永远不来）
\*   NOT EveryActiveEpisodeEventuallyCompletes（stop/fail 都是合法终局）
\*   NOT EveryStopEventuallyStops（failure 可压过 stop）
\*   NOT EveryDecoderEventuallyExits / EveryDeviceEventuallyDrains
\* TLC 要求 fairness 必须出现在 SPECIFICATION 顶层，因此写成独立 spec。
Spec ==
    Init /\ [][Next]_vars

SpecSettlementFairness ==
    Init /\ [][Next]_vars /\ WF_vars(AuthoritySettle)

-----------------------------------------------------------------------------
(******************************* 类型不变式 **********************************)

TypeOK ==
    /\ stopSeen \in BOOLEAN
    /\ decodeFailure \in BOOLEAN
    /\ workerTerminal \in WorkerTerminals
    /\ drainVerdict \in DrainVerdicts
    /\ activationFailed \in BOOLEAN
    /\ terminalOutcome \in Outcomes
    /\ episodeLifecycle \in Lifecycles
    /\ decisionLatched \in BOOLEAN
    /\ stopAtDecision \in BOOLEAN
    /\ firstCommitted \in Outcomes
    /\ observeRan \in BOOLEAN
    /\ waitRan \in BOOLEAN

\* 证据组合的现实约束（render 只在 edge terminal = Eof 时才可能报 Drained；
\* decode failure 先于 edge.fail() 发布，first-wins 使 fail 成为 no-op）。
\* 注意：第二条是理想化顺序而非严格现实约束（decode_failed 与已放开的
\* stop edge 竞争可产生 decodeFailure ∧ worker=Stopped；无害——模型与
\* production 都把 decode failure 判在第一位，被排除的组合在两侧都是
\* Failed）。本不变式无独立 mutation 负控制（如实声明：它是守卫一致性
\* 自查，不作为"有约束力"的证据引用）。
EvidenceConsistent ==
    /\ (drainVerdict = "Drained" => workerTerminal \in {"None", "Eof"})
    /\ (decodeFailure => workerTerminal \in {"None", "Failed"})

\* 模型内部一致性：决定性边界一到就 latch（结构上 publish 与 latch 同步，
\* 因此不存在"已决定性却未 latch"的可达状态）。
BoundaryLatchedWhenDecisive == Decisive => decisionLatched

-----------------------------------------------------------------------------
(******************************* 安全不变式 **********************************)

\* S2 —— 至多一次提交；一旦提交不可改写（含 late stop 不能改写）。
TerminalOutcomeImmutable ==
    firstCommitted = "None" \/ terminalOutcome = firstCommitted

\* S3 —— writer identity，**transition 级**性质（review corrective 轮重做）：
\* terminalOutcome 的任何改变必须**就是** AuthoritySettle 这个动作。
\* 上一轮用 ghost 布尔状态不变式表达 writer 集合，被 review 证伪：一个
\* "顺手把 ghost 也维护掉"的冒写动作可以骗过全部状态检查。状态谓词在
\* 原则上无法表达"是哪个动作写的"，所以这条性质必须落在 transition 上，
\* 由 TLC 以 temporal property 检查（cfg 的 PROPERTY 节引用
\* AuthorityIsSoleWriter）。M1 / M2 / M9 证明它不空洞；其中 M9 是最大
\* 对手（值、边界 intent、firstCommitted 全部如实维护，状态不变式全绿）。
OutcomeChangedOnlyByAuthority ==
    (terminalOutcome' # terminalOutcome) => AuthoritySettle

\* [][A]_vars 形式：每一步要么满足 A，要么整体 stutter（stutter 时
\* outcome 不变，蕴含前件为假，平凡满足）。
AuthorityIsSoleWriter == [][OutcomeChangedOnlyByAuthority]_vars

\* S6 —— 不虚构 Completed（D11 命题：decode EOF + drain contract 完成；
\* 不加入 physical audibility）。
NoFalseCompleted ==
    terminalOutcome = "Completed"
        => (workerTerminal = "Eof" /\ drainVerdict = "Drained")

\* S7 —— 不虚构 Stopped（D11 命题：aborted evidence + 决策边界时已记录
\* stop intent + 无更高优先级 failure 胜出）。**只能读 stopAtDecision**，
\* 不得读当前 stopSeen（D11 Stopped 命题：不许重写成 stopRequestedNow）。
NoFalseStopped ==
    terminalOutcome = "Stopped"
        => (/\ workerTerminal = "Stopped"
            /\ drainVerdict = "Aborted"
            /\ stopAtDecision
            /\ ~decodeFailure)

\* S9 —— 已处于 settlement obligation 触发域（全形状 Decisive）时，提交值
\* 必须是判决函数在（冻结的）边界 intent 下对当前证据的判决。单向蕴含是
\* 有意的：只约束"可判决证据上的提交值"，未提交（None）在 settlement
\* 执行前合法。判决**值表**（CurrentDecisionVerdict）是 current
\* realization conformance oracle——conformance 的对象正是当前已接受的
\* decision contract；其 precedence 演进时本不变式随 authority 变更重推导，
\* 而触发域全覆盖、C5、progress 的义务结构不变。它同时排除：改写已提交
\* 值（M3）、settlement 读取当前 stopSeen 导致的重标签（M4）。这是
\* late-command stability 的机器表达，配套正向 witness
\* Unreachable_LateStopDecisiveFailedStaysFailed 的反证（W4）。
CommittedOutcomeMatchesContract ==
    Decisive =>
        (terminalOutcome = "None" \/ terminalOutcome = SettleCandidate)

\* S8 —— activation failure 不是 D11 terminal Failed。
ActivationFailureIsNotTerminalFailed ==
    activationFailed => terminalOutcome # "Failed"

\* 命名诊断（非 normative，比 S8 更锐）：activation 失败时本模型不允许任何
\* runtime evidence 产生，故 outcome 只能保持未提交。
DiagnosticActivationFailureLeavesOutcomeUncommitted ==
    activationFailed => terminalOutcome = "None"

\* S10 —— teardown settlement 边界（D11-C5 的条件形式）：decisive evidence
\* 存在时，teardown 不允许完成成 TeardownDone ∧ outcome 未提交。
\* 判定用**全形状** Decisive（normative 触发域）。M10 负控制证明这个覆盖
\* 是真实的：只把机制门（SettlementGateDecisive）缩回 minimal 域、性质
\* 不动，C5 立即被违反。
\* 这是 safety boundary，**不是**"每个 episode 最终都会 teardown/终结"的
\* liveness 承诺。
TeardownRequiresSettlement ==
    (episodeLifecycle = "TeardownDone" /\ ~activationFailed /\ Decisive)
        => terminalOutcome # "None"

-----------------------------------------------------------------------------
(********************** witness 不可达断言（探针用） *************************)

\* 以下断言在正常模型中都必须被**违反**（witness found）——每条对应一条
\* 必须可达的合法路径。单独的 probe cfg 逐一检查（见 check.sh / README）。

\* W1 自然 EOF：Activate -> Eof -> Drained -> latch(Completed) -> settle。
Unreachable_NaturalCompletion ==
    ~(terminalOutcome = "Completed" /\ workerTerminal = "Eof" /\ drainVerdict = "Drained")

\* W2 用户 stop：stop intent 在决定性边界之前记录，最终 Stopped。
Unreachable_UserStop ==
    ~(terminalOutcome = "Stopped" /\ stopAtDecision /\ stopSeen)

\* W3 设备 abort（无 stop）：无边界 stop intent 的 abort 判 Failed。
Unreachable_DeviceAbortFailed ==
    ~(terminalOutcome = "Failed" /\ workerTerminal = "Stopped"
      /\ drainVerdict = "Aborted" /\ ~stopAtDecision)

\* W4（late-command stability 的正向 witness）：设备失败已决定性为 Failed
\* （边界记录 stopAtDecision=FALSE），晚到的 RequestStop 已进入 command
\* history（stopSeen=TRUE），settlement 结果**仍是 Failed**。
Unreachable_LateStopDecisiveFailedStaysFailed ==
    ~(decisionLatched /\ ~stopAtDecision /\ stopSeen
      /\ workerTerminal = "Stopped" /\ drainVerdict = "Aborted"
      /\ terminalOutcome = "Failed")

\* W5 Completed 判定后的晚到 stop：已提交 Fact 不被追溯改名，
\* 但晚到 stop 作为 command history 与其共存。
Unreachable_CompletedWithLateStop ==
    ~(terminalOutcome = "Completed" /\ stopSeen)

\* W6（current authority 最直观的 conformance witness）：
\* 整个执行中 Observe 从未运行、Wait 从未运行，terminal Fact 仍被建立。
Unreachable_CommitWithoutConsumer ==
    ~(terminalOutcome # "None" /\ ~observeRan /\ ~waitRan)

\* W7（review corrective 轮新增）：Eof + Aborted（全程无 stop）——上一轮
\* 被留在触发域外、导致 C5/progress 局部化的 production-decisive 分支——
\* 现在由 authority 判为 Failed(device) 并提交。
Unreachable_EofAbortedFailedDevice ==
    ~(terminalOutcome = "Failed" /\ workerTerminal = "Eof"
      /\ drainVerdict = "Aborted" /\ ~stopAtDecision /\ ~stopSeen)

-----------------------------------------------------------------------------
(***************************** Liveness 性质 ********************************)

\* 条件性进度（极克制）：decisive evidence 已存在 => Fact 最终
\* 被提交。成立前提：SpecSettlementFairness 中的 WF_vars(AuthoritySettle)
\* （settlement 动作获得正常执行机会）。decisive 之后 FinishTeardown 被
\* settlement 边界阻塞，因此 AuthoritySettle 在提交前持续使能，WF 足以
\* 承载进度；不假设任何关于证据到来 / episode 终结 / teardown 完成的事。
\* probes/NoFairnessProgressFails.cfg 证明这条 fairness 是承重的。
SettlementProgress == Decisive ~> terminalOutcome # "None"

\* —— 未经授权的命题：本模型**不得**宣称以下任何一条；
\* 每条都必须被违反（Overclaim 系列反向控制；全部在带 WF 的最强让步下
\* 检查，证明 fairness 没有偷渡 general liveness）。——
EveryEpisodeEventuallyTerminates ==
    <>(terminalOutcome # "None")

EveryActiveEpisodeEventuallyCompletes ==
    [](episodeLifecycle = "Active" => <>(terminalOutcome = "Completed"))

EveryStopEventuallyStops ==
    [](stopSeen => <>(terminalOutcome = "Stopped"))

EveryDecoderEventuallyExits ==
    <>(workerTerminal # "None")

EveryDeviceEventuallyDrains ==
    <>(drainVerdict # "None")

=============================================================================
\* #### EOF ####
