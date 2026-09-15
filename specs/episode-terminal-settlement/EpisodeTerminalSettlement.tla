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
(*                             stream）。FinishTeardown 的两个守卫是生产      *)
(*                             现实，不是额外假设（见动作注释）。             *)
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
(*   MutationEvidenceProducerCommits   evidence 发布者自己写 outcome（M9）    *)
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
    MutationEvidenceProducerCommits

MutationChoices == {MutationNone,
                    MutationObserveCommits,
                    MutationWaitCommits,
                    MutationTerminalRewritable,
                    MutationLateStopReadsCurrentIntent,
                    MutationTeardownBeforeSettlement,
                    MutationActivationFailureBecomesFailed,
                    MutationFalseCompleted,
                    MutationFalseStopped,
                    MutationEvidenceProducerCommits}

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
    authoritySettled,       \* AuthoritySettle 是否至少执行过一次（writer 集合）
    observeRan,             \* 外部纯读是否至少发生过一次
    waitRan                 \* 外部等待是否至少发生过一次

vars == <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
          activationFailed, terminalOutcome, episodeLifecycle,
          decisionLatched, stopAtDecision,
          firstCommitted, authoritySettled, observeRan, waitRan>>

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
    /\ authoritySettled = FALSE
    /\ observeRan = FALSE
    /\ waitRan = FALSE

-----------------------------------------------------------------------------
(************************ 语义判决函数（纯函数） *****************************)

\* D11 accepted proposition 的极小转录：本函数**只**包含
\* merged D11 明确冻结/要求的判决内容：
\*
\*   Completed（D11 命题）：decode EOF evidence + output drain-complete
\*       evidence（不加入 physical audibility）。
\*   Stopped（D11 命题）：aborted episode terminal evidence + 决策边界时
\*       已记录 stop intent + 无更高优先级 failure 胜出。
\*   Failed（D11 命题）：按 authority decision contract 判为失败；failure
\*       class 由 semantic precedence 决定——本函数转录其最小核心：
\*       failure evidence（decode failure / worker Failed）压过一切；
\*       无 stop intent 的 abort 是 device failure。
\*
\* production resolver 的其余精确 precedence（如 Aborted+Eof -> Failed、
\* stage 文本）是 **current realization**，不是 D11 冻结内容，本模型不转录
\* （见 README 的 production 映射表；f2-terminal-commit-boundary 套件承载
\* current decision table
\* 证据）。第一分支是 M7 的负控制注入口。
TerminalCandidateOf(sa, df, wt, dv) ==
    CASE Mutation = MutationFalseCompleted /\ wt = "Eof" -> "Completed"
      [] Mutation = MutationFalseStopped /\ wt = "Stopped" /\ dv = "Aborted" -> "Stopped"
      [] df \/ wt = "Failed" -> "Failed"
      [] wt = "Eof" /\ dv = "Drained" -> "Completed"
      [] wt = "Stopped" /\ dv = "Aborted" -> IF sa THEN "Stopped" ELSE "Failed"
      [] OTHER -> "None"

\* 用"决策边界时刻的 stop intent"（ghost 值）算出的当前判决。
\* evidence write-once => 一旦判决锁定，判决函数值不再变化。
SettleCandidate == TerminalCandidateOf(stopAtDecision, decodeFailure, workerTerminal, drainVerdict)

\* 当前证据是否已足以判决。
Decisive == SettleCandidate # "None"

\* 决定性边界是否在"这块证据落地"的瞬间首次到达。
\* latch 判定用 stopSeen（此刻已记录的 intent）求值：最后一块决定性证据
\* 与 stop intent 的先后关系，就是 D11 late-command rule 的判决输入。
BecameDecisive(dfN, wtN, dvN) ==
    /\ ~decisionLatched
    /\ TerminalCandidateOf(stopSeen, dfN, wtN, dvN) # "None"

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
                   stopAtDecision, firstCommitted, authoritySettled,
                   observeRan, waitRan>>

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
        /\ authoritySettled' = (authoritySettled \/ escalated)
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
                       firstCommitted, authoritySettled, observeRan, waitRan>>

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
                   authoritySettled, observeRan, waitRan>>

PublishWorkerEof ==
    LET sneak ==
            Mutation = MutationEvidenceProducerCommits
            /\ terminalOutcome = "None"
            /\ TerminalCandidateOf(StopAtBoundary(decodeFailure, "Eof", drainVerdict),
                                   decodeFailure, "Eof", drainVerdict) # "None"
        cand ==
            TerminalCandidateOf(StopAtBoundary(decodeFailure, "Eof", drainVerdict),
                                decodeFailure, "Eof", drainVerdict)
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ workerTerminal = "None"
        /\ ~decodeFailure
        /\ workerTerminal' = "Eof"
        /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, "Eof", drainVerdict))
        /\ stopAtDecision' = StopAtBoundary(decodeFailure, "Eof", drainVerdict)
        /\ terminalOutcome' = IF sneak THEN cand ELSE terminalOutcome
        /\ UNCHANGED <<stopSeen, decodeFailure, drainVerdict, activationFailed,
                       episodeLifecycle, firstCommitted, authoritySettled,
                       observeRan, waitRan>>

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
                   authoritySettled, observeRan, waitRan>>

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
                   authoritySettled, observeRan, waitRan>>

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
                   authoritySettled, observeRan, waitRan>>

PublishDrainAborted ==
    /\ episodeLifecycle \in {"Active", "TeardownStarted"}
    /\ ~activationFailed
    /\ drainVerdict = "None"
    /\ drainVerdict' = "Aborted"
    /\ decisionLatched' = (decisionLatched \/ BecameDecisive(decodeFailure, workerTerminal, "Aborted"))
    /\ stopAtDecision' = StopAtBoundary(decodeFailure, workerTerminal, "Aborted")
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, activationFailed,
                   terminalOutcome, episodeLifecycle, firstCommitted,
                   authoritySettled, observeRan, waitRan>>

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
                   stopAtDecision, firstCommitted, authoritySettled,
                   observeRan, waitRan>>

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
FinishTeardown ==
    /\ episodeLifecycle = "TeardownStarted"
    /\ (   activationFailed
         \/ (workerTerminal # "None" /\ drainVerdict # "None"))
    /\ (   Mutation = MutationTeardownBeforeSettlement
         \/ activationFailed
         \/ ~Decisive
         \/ terminalOutcome # "None")
    /\ episodeLifecycle' = "TeardownDone"
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome, decisionLatched,
                   stopAtDecision, firstCommitted, authoritySettled,
                   observeRan, waitRan>>

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
           THEN TerminalCandidateOf(stopSeen, decodeFailure, workerTerminal, drainVerdict)
           ELSE SettleCandidate
    /\ firstCommitted' = terminalOutcome'
    /\ authoritySettled' = TRUE
    /\ UNCHANGED <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle, decisionLatched,
                   stopAtDecision, observeRan, waitRan>>

\* 纯读（D14.2 observe 语义）：只改 observeRan，永不写任何语义状态。
\* M1 负控制让它偷偷提交（且不登记 authoritySettled，writer 集合不变式
\* 必须发现它）。
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
                   stopAtDecision, firstCommitted, authoritySettled, waitRan>>

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
                   stopAtDecision, firstCommitted, authoritySettled, observeRan>>

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
    /\ authoritySettled \in BOOLEAN
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

\* S3 / S4 / S5 的 writer 集合 —— terminalOutcome 的唯一 None -> terminal
\* writer 是 AuthoritySettle。Observe（纯读）、Wait（纯等待）、证据发布
\* 动作都不在此集合内。M1 / M2 / M9 分别证明这条约束不是空洞的。
OutcomeWrittenOnlyByAuthoritySettle ==
    terminalOutcome = "None" \/ authoritySettled

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

\* S9 —— **已决定性（Decisive）时**，提交值必须是判决函数在（冻结的）边界
\* intent 下对当前证据的判决。单向蕴含是有意的范围选择：它只约束"决定性
\* 证据上的提交值"，不把本模型 None-region（未决定组合）的边界升格为
\* normative——production 当前 resolver 对部分未决定组合（如
\* Aborted+Eof -> Failed）更"果断"，那是 current realization，本模型不做
\* normative 约束（见 README 范围声明）。evidence write-once => 提交后的
\* 证据不再变化。它同时排除：改写已提交值（M3）、settlement 读取当前
\* stopSeen 导致的重标签（M4）。这是 late-command stability 的机器表达，
\* 配套正向 witness Unreachable_LateStopDecisiveFailedStaysFailed 的反证
\* （W4）。
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
