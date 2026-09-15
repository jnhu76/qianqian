--------------------------- MODULE F2TerminalCommitBoundary ---------------------------
(***************************************************************************)
(* F2TerminalCommitBoundary — 一个 playback episode 的 terminal outcome       *)
(*                            semantic commit ownership 边界                  *)
(*                                                                         *)
(* STATUS: FORMAL EVIDENCE（campaign artifact）。**不是第二份 authority。**   *)
(*   ADR-PBK-001 §2（Fact Plane / semantic commit / projection 非权威）与     *)
(*   ADR-PBK-002 §17 D11（episode terminal outcome authority）是唯一语义真相； *)
(*   本模型只在显式 abstraction 与 assumptions 下回答一个问题：              *)
(*                                                                         *)
(*     terminal evidence 已经足够时，谁拥有把 evidence 变成 terminal Fact     *)
(*     的权利与责任？外部 read / wait 是在消费 truth，还是在创造 truth？      *)
(*                                                                         *)
(*   本模型不建模（刻意的最小化）：K0 / Capability / ComponentSpec / Fiber    *)
(*   依赖图 / PCM / WASAPI / decoder staging / seek / pause / open /         *)
(*   Generation / Window / Realtime Audio Runtime / 线程调度 / 内存序。      *)
(*   本模型不选择 Rust representation（线程 / callback / Condvar 等机制全     *)
(*   部 OPEN）；机制裁决属于 ADR + code reality gap audit。                  *)
(*   模型结论只在其显式 abstraction 与 assumptions 下成立，不得静默升级为     *)
(*   架构权威（AGENTS.md "Verification authority boundary"）。               *)
(*                                                                         *)
(* 模型词汇 -> current production reality 映射（完整表见 README.md）：        *)
(*   Activate                   session 激活成功（edge 绑定、render stream    *)
(*                              打开、decode worker 已 spawn）                *)
(*   PublishActivationFailure   SessionCompletion::activation_failed          *)
(*   RequestStop                SessionCompletion::request_stop（command 状态，*)
(*                              不是 fact；first-wins，不反向改写已提交终局）  *)
(*   PublishDecodeFailure       SessionCompletion::decode_failed              *)
(*   PublishWorkerEof/Stopped/  SessionCompletion::worker_exited(edge.terminal)*)
(*     Failed                   —— worker 退出前最后一步                     *)
(*   PublishDrainDrained/       DrainSignal::complete（render 线程退出前必发） *)
(*     Aborted                                                              *)
(*   BeginTeardown /            session fiber 的 effect 逆序释放              *)
(*     FinishTeardown           （stop edge + join worker / stop_and_join     *)
(*                              render stream；两者 join 完成后 teardown 才算 *)
(*                              完成——见 FinishTeardown 的守卫）              *)
(*   ConsumerTriggeredCommit    SessionCompletion::wait() / try_resolve_now() *)
(*                              -> resolve() -> state.outcome = Some(...)     *)
(*   ConsumerWait/Observe       wait() 的等待语义 / 纯读语义                  *)
(*   AuthorityResolve           Authority-owned commit：authority 自己推进    *)
(*                              提交（current Rust 中无对应实现；候选 B）     *)
(*                                                                         *)
(* 关键状态区分（不合并成 bool 的原因）：                                     *)
(*   evidence 已发布 != candidate 已决定性 != terminal outcome 已 commit      *)
(*   stop intent（command）!= Stopped（fact）                                *)
(*   activation failure（diagnostic）!= terminal Failed（fact）               *)
(*   teardown 完成 != fact 已存在                                            *)
(*                                                                         *)
(* 变体（Ownership 常量，稳定语义名）：                                       *)
(*   OwnershipConsumerTriggered  A：只有外部 consumer 的那一步能提交          *)
(*   OwnershipAuthorityOwned     B：authority 拥有一个自己的提交动作          *)
(*   OwnershipAtomicWithEvidence B′：提交与"最后一块决定性证据"同一步完成     *)
(*   ——三者都不改变 D11 的命题本身；差别在"谁/何时"，见 report.md。           *)
(*                                                                         *)
(* 负控制（Mutation 常量，逐个独立注入；稳定语义名，不使用编号词汇）：        *)
(*   MutationObserveCommits            纯读动作偷偷提交（S2 非空洞性）        *)
(*   MutationTerminalRewritable        已提交终局可被 late stop 改写（S1b/S3）*)
(*   MutationWaitIsSoleResolver        取消 authority 提交权，外部 wait 成为  *)
(*                                     唯一 resolver（B1 非空洞性）           *)
(*   MutationAuthorityResolverRemoved  authority 侧提交动作整体移除（progress）*)
(*   MutationActivationFailureIsFailed activation failure 被升格为 D11 Failed *)
(*   MutationResolverIgnoresEvidence   resolver 忽略证据直接判决（S5/S6）     *)
(*                                                                         *)
(* 本模型不检查 TLC deadlock：terminal 状态之后"环境停摆"是该模型要表达的     *)
(* 合法行为（[][Next]_vars 允许 stuttering），不是建模缺陷。                  *)
(***************************************************************************)

EXTENDS Integers, TLC

CONSTANTS
    \* 提交所有权变体：谁可以推进 terminal Fact 的提交。
    Ownership,
    \* 外部消费者环境：是否存在一个会调用 wait()/观察的外部 consumer。
    ConsumerEnvironment,
    \* 负控制开关。
    Mutation,
    OwnershipConsumerTriggered,
    OwnershipAuthorityOwned,
    OwnershipAtomicWithEvidence,
    ConsumerPresent,
    ConsumerAbsent,
    MutationNone,
    MutationObserveCommits,
    MutationTerminalRewritable,
    MutationWaitIsSoleResolver,
    MutationAuthorityResolverRemoved,
    MutationActivationFailureIsFailed,
    MutationResolverIgnoresEvidence

OwnershipChoices == {OwnershipConsumerTriggered,
                     OwnershipAuthorityOwned,
                     OwnershipAtomicWithEvidence}

ConsumerChoices == {ConsumerPresent, ConsumerAbsent}

MutationChoices == {MutationNone,
                    MutationObserveCommits,
                    MutationTerminalRewritable,
                    MutationWaitIsSoleResolver,
                    MutationAuthorityResolverRemoved,
                    MutationActivationFailureIsFailed,
                    MutationResolverIgnoresEvidence}

ASSUME /\ Ownership \in OwnershipChoices
       /\ ConsumerEnvironment \in ConsumerChoices
       /\ Mutation \in MutationChoices

VARIABLES
    \* —— 机制证据（发布即成立，write-once / 单调，与 current realization 相符）——
    stopRequested,          \* command 状态，FALSE -> TRUE 单调；不是 fact
    decodeFailure,          \* write-once
    workerTerminal,         \* write-once: "None" | "Eof" | "Stopped" | "Failed"
    drainVerdict,           \* write-once: "None" | "Drained" | "Aborted"
    activationFailed,       \* write-once diagnostic
    \* —— 被观察的边界两端 ——
    terminalOutcome,        \* "None" | "Completed" | "Stopped" | "Failed"
    episodeLifecycle,       \* BeforeActivation | Active | TeardownStarted | TeardownDone
    \* —— 以下是 verifier-only 历史/见证变量（auxiliary，非 normative，        *)
    \*    不要求 production 具有任何对应表示 ——                            *)
    firstCommitted,         \* 第一次提交写入的值（write-once），用于检出改写
    committerStepped,       \* 契约指定的提交者是否至少执行过一次
    consumerAsked           \* 外部 consumer 是否至少等待/观察过一次

vars == <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
          activationFailed, terminalOutcome, episodeLifecycle,
          firstCommitted, committerStepped, consumerAsked>>

WorkerTerminals == {"None", "Eof", "Stopped", "Failed"}
DrainVerdicts == {"None", "Drained", "Aborted"}
Outcomes == {"None", "Completed", "Stopped", "Failed"}
Lifecycles == {"BeforeActivation", "Active", "TeardownStarted", "TeardownDone"}

-----------------------------------------------------------------------------
(******************************** 初始状态 **********************************)

Init ==
    /\ stopRequested = FALSE
    /\ decodeFailure = FALSE
    /\ workerTerminal = "None"
    /\ drainVerdict = "None"
    /\ activationFailed = FALSE
    /\ terminalOutcome = "None"
    /\ episodeLifecycle = "BeforeActivation"
    /\ firstCommitted = "None"
    /\ committerStepped = FALSE
    /\ consumerAsked = FALSE

-----------------------------------------------------------------------------
(************************ 语义判决函数（纯函数） *****************************)

\* CURRENT REALIZATION 的 resolver precedence（对应 completion.rs::resolve）。
\* 这是"当前实现如何判决"，不是 D11 冻结的命题本身：D11 只冻结
\* Completed / Stopped / Failed 三段外部命题，precedence 属于 current
\* realization，改 precedence 若改变外部命题才需要回 authority review。
\*
\* 与 resolve() 逐分支对应：
\*   decode_failure                       -> Failed（先于一切，不被 stop 改写）
\*   worker_terminal = Failed             -> Failed
\*   drain = Drained  /\ worker = Eof     -> Completed
\*   drain = Aborted  /\ worker = Stopped -> stopRequested ? Stopped : Failed
\*   drain = Aborted  /\ worker = Eof     -> Failed（render 在 EOF 后 abort）
\*   其余（worker 尚未退出等）             -> undecided
CandidateOf(stopReq, decFail, worker, drain) ==
    IF Mutation = MutationResolverIgnoresEvidence
    THEN "Completed"                       \* 负控制：忽略证据直接判决
    ELSE IF decFail THEN "Failed"
    ELSE IF worker = "Failed" THEN "Failed"
    ELSE IF drain = "Drained" /\ worker = "Eof" THEN "Completed"
    ELSE IF drain = "Aborted" /\ worker = "Stopped"
         THEN (IF stopReq THEN "Stopped" ELSE "Failed")
    ELSE IF drain = "Aborted" /\ worker = "Eof" THEN "Failed"
    ELSE "None"

\* 当前状态下 evidence 是否已足以判决。
Candidate == CandidateOf(stopRequested, decodeFailure, workerTerminal, drainVerdict)
Decisive == Candidate # "None"

-----------------------------------------------------------------------------
(************************** 提交所有权（常量派生） ***************************)

\* 哪条路径在契约下拥有 terminal Fact 的提交权。B′ 把提交并进证据发布本身。
AuthorityCommitAllowed ==
    /\ Mutation # MutationAuthorityResolverRemoved
    /\ Mutation # MutationWaitIsSoleResolver
    /\ Ownership = OwnershipAuthorityOwned

AtomicCommitAllowed ==
    /\ Mutation # MutationAuthorityResolverRemoved
    /\ Mutation # MutationWaitIsSoleResolver
    /\ Ownership = OwnershipAtomicWithEvidence

ConsumerCommitAllowed ==
    \/ Mutation = MutationWaitIsSoleResolver
    \/ Ownership = OwnershipConsumerTriggered

\* 纯读动作是否被负控制污染（M1）。
ObserveCommitAllowed == Mutation = MutationObserveCommits

-----------------------------------------------------------------------------
(******************************* 环境/机制动作 *******************************)

\* session 激活成功：两个 leg 从此存在（reality: 先 render stream 后 worker）。
Activate ==
    /\ episodeLifecycle = "BeforeActivation"
    /\ ~activationFailed
    /\ episodeLifecycle' = "Active"
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome,
                   firstCommitted, committerStepped, consumerAsked>>

\* 激活失败：diagnostic，不是 terminal outcome 权威（D11 firewall）。
\* 负控制 MutationActivationFailureIsFailed 让它把 activation failure 直接
\* 升格成 D11 terminal Failed（并如实登记提交者，使 S4 成为唯一被击穿的性质，
\* 避免与 writer-set 不变式混在一起）。
PublishActivationFailure ==
    LET escalated == Mutation = MutationActivationFailureIsFailed
    IN  /\ episodeLifecycle = "BeforeActivation"
        /\ ~activationFailed
        /\ activationFailed' = TRUE
        /\ terminalOutcome' = IF escalated THEN "Failed" ELSE terminalOutcome
        /\ firstCommitted' = IF escalated /\ firstCommitted = "None"
                             THEN "Failed" ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ escalated)
        /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                       episodeLifecycle, consumerAsked>>

\* stop intent：command，不是 fact。first-wins；不反向改写已提交终局。
RequestStop ==
    LET rewritten ==
            Mutation = MutationTerminalRewritable
            /\ terminalOutcome \in {"Completed", "Failed"}
    IN  /\ episodeLifecycle \in {"BeforeActivation", "Active", "TeardownStarted"}
        /\ stopRequested = FALSE
        /\ stopRequested' = TRUE
        /\ terminalOutcome' = IF rewritten THEN "Stopped" ELSE terminalOutcome
        /\ UNCHANGED <<decodeFailure, workerTerminal, drainVerdict, activationFailed,
                       episodeLifecycle, firstCommitted, committerStepped, consumerAsked>>

\* 证据发布。AtomicCommitAllowed 下，提交与该证据发布同一步完成（B′）。
\* 全部发布动作都要求 ~activationFailed：激活失败时不存在任何 leg，
\* 因而 runtime evidence 结构上不可能产生（这也是 S4 成立的原因）。
\* 每个动作内的 cand 用"次态证据值"计算，即"这块证据落地后的 candidate"。
PublishDecodeFailure ==
    LET cand == CandidateOf(stopRequested, TRUE, workerTerminal, drainVerdict)
        commits == AtomicCommitAllowed /\ terminalOutcome = "None" /\ cand # "None"
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ decodeFailure = FALSE
        /\ workerTerminal \in {"None", "Failed"}
        /\ decodeFailure' = TRUE
        /\ terminalOutcome' = IF commits THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF commits /\ firstCommitted = "None" THEN cand ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ commits)
        /\ UNCHANGED <<stopRequested, workerTerminal, drainVerdict, activationFailed,
                       episodeLifecycle, consumerAsked>>

PublishWorkerEof ==
    LET cand == CandidateOf(stopRequested, decodeFailure, "Eof", drainVerdict)
        commits == AtomicCommitAllowed /\ terminalOutcome = "None" /\ cand # "None"
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ workerTerminal = "None"
        /\ ~decodeFailure
        /\ workerTerminal' = "Eof"
        /\ terminalOutcome' = IF commits THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF commits /\ firstCommitted = "None" THEN cand ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ commits)
        /\ UNCHANGED <<stopRequested, decodeFailure, drainVerdict, activationFailed,
                       episodeLifecycle, consumerAsked>>

PublishWorkerStopped ==
    LET cand == CandidateOf(stopRequested, decodeFailure, "Stopped", drainVerdict)
        commits == AtomicCommitAllowed /\ terminalOutcome = "None" /\ cand # "None"
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ workerTerminal = "None"
        /\ ~decodeFailure
        /\ drainVerdict # "Drained"          \* Drained 蕴含 edge terminal = Eof
        /\ workerTerminal' = "Stopped"
        /\ terminalOutcome' = IF commits THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF commits /\ firstCommitted = "None" THEN cand ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ commits)
        /\ UNCHANGED <<stopRequested, decodeFailure, drainVerdict, activationFailed,
                       episodeLifecycle, consumerAsked>>

PublishWorkerFailed ==
    LET cand == CandidateOf(stopRequested, decodeFailure, "Failed", drainVerdict)
        commits == AtomicCommitAllowed /\ terminalOutcome = "None" /\ cand # "None"
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ workerTerminal = "None"
        /\ drainVerdict # "Drained"
        /\ workerTerminal' = "Failed"
        /\ terminalOutcome' = IF commits THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF commits /\ firstCommitted = "None" THEN cand ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ commits)
        /\ UNCHANGED <<stopRequested, decodeFailure, drainVerdict, activationFailed,
                       episodeLifecycle, consumerAsked>>

PublishDrainDrained ==
    LET cand == CandidateOf(stopRequested, decodeFailure, workerTerminal, "Drained")
        commits == AtomicCommitAllowed /\ terminalOutcome = "None" /\ cand # "None"
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ drainVerdict = "None"
        /\ workerTerminal \in {"None", "Eof"}   \* Drained 只在 edge 到达 EOF 后可能
        /\ drainVerdict' = "Drained"
        /\ terminalOutcome' = IF commits THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF commits /\ firstCommitted = "None" THEN cand ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ commits)
        /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, activationFailed,
                       episodeLifecycle, consumerAsked>>

PublishDrainAborted ==
    LET cand == CandidateOf(stopRequested, decodeFailure, workerTerminal, "Aborted")
        commits == AtomicCommitAllowed /\ terminalOutcome = "None" /\ cand # "None"
    IN  /\ episodeLifecycle \in {"Active", "TeardownStarted"}
        /\ ~activationFailed
        /\ drainVerdict = "None"
        /\ drainVerdict' = "Aborted"
        /\ terminalOutcome' = IF commits THEN cand ELSE terminalOutcome
        /\ firstCommitted' = IF commits /\ firstCommitted = "None" THEN cand ELSE firstCommitted
        /\ committerStepped' = (committerStepped \/ commits)
        /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, activationFailed,
                       episodeLifecycle, consumerAsked>>

-----------------------------------------------------------------------------
(******************************** teardown 边界 ******************************)

\* 两个 leg 的 inverse 顺序（stop edge + join worker → stop_and_join stream）
\* 在本模型中折叠成 teardown 的两个阶段；中间态对 terminal commit 问题不可观测。
BeginTeardown ==
    /\ episodeLifecycle \in {"BeforeActivation", "Active"}
    /\ episodeLifecycle' = "TeardownStarted"
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome,
                   firstCommitted, committerStepped, consumerAsked>>

\* teardown 完成 = 两个 leg 的 join 都已返回。
\* reality：join 返回时 worker 必然已执行 worker_exited；
\*          stop_and_join 返回时 render 线程必然已执行 drain.complete。
\* 所以"证据齐备"是 teardown 完成的前置条件，而不是额外假设。
FinishTeardown ==
    /\ episodeLifecycle = "TeardownStarted"
    /\ (activationFailed \/ (workerTerminal # "None" /\ drainVerdict # "None"))
    /\ episodeLifecycle' = "TeardownDone"
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome,
                   firstCommitted, committerStepped, consumerAsked>>

-----------------------------------------------------------------------------
(****************************** 消费者侧动作 ********************************)

\* 等待（A 变体下这是唯一提交点的语义载体；B/B′ 下它只等，不写）。
ConsumerWait ==
    /\ ConsumerEnvironment = ConsumerPresent
    /\ consumerAsked' = TRUE
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, terminalOutcome, episodeLifecycle,
                   firstCommitted, committerStepped>>

\* 纯读：永不成为 terminalOutcome 的 writer。
\* M1 负控制让它在"证据已决定性"时偷偷提交（且不登记 committerStepped），
\* 用于证明 "Observe 是纯读" 这条约束不是空的。
ConsumerObserve ==
    /\ ConsumerEnvironment = ConsumerPresent
    /\ consumerAsked' = TRUE
    /\ terminalOutcome' =
           IF ObserveCommitAllowed /\ terminalOutcome = "None" /\ Decisive
           THEN Candidate
           ELSE terminalOutcome
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle,
                   firstCommitted, committerStepped>>

\* A（consumer-driven commit）：提交发生在外部消费者调用 wait()/resolve 的那一步。
\* 与 ConsumerWait 分开命名是刻意的：若合并成一个动作，WF 会被 stutter 分支
\* 满足，liveness 结论会变成空洞的——见 report.md 的 fairness 一节。
ConsumerTriggeredCommit ==
    /\ ConsumerEnvironment = ConsumerPresent
    /\ ConsumerCommitAllowed
    /\ terminalOutcome = "None"
    /\ Decisive
    /\ consumerAsked' = TRUE
    /\ terminalOutcome' = Candidate
    /\ firstCommitted' = Candidate
    /\ committerStepped' = TRUE
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle>>

-----------------------------------------------------------------------------
(******************************** 权威侧动作 ********************************)

\* B（authority-owned commit）：Playback Session semantic authority 自己在
\* 证据决定性时推进并提交；外部 read/wait 只消费已提交的 truth。
AuthorityResolve ==
    /\ AuthorityCommitAllowed
    /\ terminalOutcome = "None"
    /\ Decisive
    /\ terminalOutcome' = Candidate
    /\ firstCommitted' = Candidate
    /\ committerStepped' = TRUE
    /\ UNCHANGED <<stopRequested, decodeFailure, workerTerminal, drainVerdict,
                   activationFailed, episodeLifecycle, consumerAsked>>

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
    \/ ConsumerWait
    \/ ConsumerObserve
    \/ ConsumerTriggeredCommit
    \/ AuthorityResolve

\* 进度假设（显式声明，不偷设）。spec 由 cfg 通过 SPECIFICATION 选择，
\* 使"这次运行假设了谁的 fairness"在配置里直接可见：
\*
\*   SpecConsumerTriggered  WF_vars(ConsumerTriggeredCommit)
\*       —— "外部消费者在事实可确立时会持续询问"。这是关于**环境**的假设，
\*          不是关于本系统组件的假设；ADR 从未承诺任何关于环境的性质。
\*   SpecAuthorityOwned     WF_vars(AuthorityResolve)
\*       —— "authority 获得正常执行机会"。这是关于**系统自己**的假设，
\*          不涉及任何外部参与者。
\*   SpecAtomicWithEvidence 无 fairness
\*       —— B′ 的"证据决定性 ⇒ Fact 已存在"已是 safety，不需要进度假设。
\*   SpecNoFairness         无 fairness
\*       —— 反向控制：用来证明上面两条 fairness 是承重的，不是装饰。
\*
\* TLC 要求 fairness 必须出现在 SPECIFICATION 顶层，因此这里写成多个 spec
\* 而不是用一个 IF/CASE 间接引用。
SpecConsumerTriggered ==
    Init /\ [][Next]_vars /\ WF_vars(ConsumerTriggeredCommit)

SpecAuthorityOwned ==
    Init /\ [][Next]_vars /\ WF_vars(AuthorityResolve)

SpecAtomicWithEvidence ==
    Init /\ [][Next]_vars

SpecNoFairness ==
    Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(******************************* 类型不变式 **********************************)

TypeOK ==
    /\ stopRequested \in BOOLEAN
    /\ decodeFailure \in BOOLEAN
    /\ workerTerminal \in WorkerTerminals
    /\ drainVerdict \in DrainVerdicts
    /\ terminalOutcome \in Outcomes
    /\ episodeLifecycle \in Lifecycles
    /\ activationFailed \in BOOLEAN
    /\ firstCommitted \in Outcomes
    /\ committerStepped \in BOOLEAN
    /\ consumerAsked \in BOOLEAN

\* 证据组合的现实约束（render 只在 edge terminal = Eof 时才可能报 Drained，
\* 而 edge terminal 就是 worker 退出时读到的那个 first-wins 值）。
EvidenceConsistent ==
    /\ (drainVerdict = "Drained" => workerTerminal \in {"None", "Eof"})
    /\ (decodeFailure => workerTerminal \in {"None", "Failed"})

-----------------------------------------------------------------------------
(******************************* 安全不变式 **********************************)

\* S1b / S3 —— 至多一个 terminal，一旦 commit 即不可改写（含 late stop 不能改写）。
TerminalOutcomeImmutable ==
    firstCommitted = "None" \/ terminalOutcome = firstCommitted

\* B4 / S2 —— terminalOutcome 的 writer 集合 == 契约指定的提交者集合。
\* 纯读观察（ConsumerObserve）与等待（ConsumerWait）不在此集合内。
OutcomeWrittenOnlyByContractCommitter ==
    terminalOutcome = "None" \/ committerStepped

\* S5 —— 不虚构 Completed。
NoFalseCompleted ==
    terminalOutcome = "Completed"
        => (workerTerminal = "Eof" /\ drainVerdict = "Drained")

\* S6 —— 不虚构 Stopped。
NoFalseStopped ==
    terminalOutcome = "Stopped"
        => (workerTerminal = "Stopped" /\ drainVerdict = "Aborted" /\ stopRequested)

\* S4 —— activation failure 不是 D11 terminal Failed。
ActivationFailureIsNotTerminalFailed ==
    activationFailed => terminalOutcome # "Failed"

\* 命名诊断（非 normative，比 S4 更锐）：activation 失败时本模型不允许任何
\* runtime evidence 产生，故 outcome 只能保持未提交。
DiagnosticActivationFailureLeavesOutcomeUncommitted ==
    activationFailed => terminalOutcome = "None"

\* teardown 完成 ⇒ evidence 已足以判决（由 join 纪律推出，不是额外假设）。
TeardownImpliesDecisive ==
    (episodeLifecycle = "TeardownDone" /\ ~activationFailed) => Decisive

-----------------------------------------------------------------------------
(************************** 边界性质（本轮重点） *****************************)

\* B1（A 侧表述）—— terminal Fact 的存在是否依赖外部 consumer？
\* 在 A 中这是构造性成立的；在 B/B′ 中同一谓词被反例击穿（见 probes/）。
BoundaryCommitRequiresConsumer ==
    terminalOutcome # "None" => consumerAsked

\* B2/B′ 强度——把"证据决定性 ⇒ Fact 已存在"降级为 safety（B′ 成立，A/B 不成立）。
DecisiveImpliesCommitted ==
    Decisive => terminalOutcome # "None"

\* 命名诊断（非 normative）："device abort 已决定性但尚未提交"的窗口是否存在。
\* B/B′ 对比用：B 中可达（window 存在），B′ 中不可达（提交与证据同步）。
DiagnosticPendingDeviceAbortUnreachable ==
    ~(workerTerminal = "Stopped" /\ drainVerdict = "Aborted"
      /\ ~stopRequested /\ terminalOutcome = "None")

\* 命名诊断（非 normative）：teardown 完成而 terminal Fact 仍缺席的状态。
\* TeardownImpliesDecisive 已证明 teardown 完成时证据必然齐备且决定性，
\* 因此该状态一旦可达，就意味着"证据齐了、leg 都 join 了，事实却不存在"。
\* A/B 中可达（witness）；B′ 中不可达（提交与决定性证据同步）。
DiagnosticTeardownWithoutFactUnreachable ==
    ~(episodeLifecycle = "TeardownDone" /\ ~activationFailed
      /\ terminalOutcome = "None")

\* 命名诊断（非 normative）：已提交终局与"晚到的 stop 意图"共存的状态。
\* 该状态**应当可达**——stop intent 是 command、不是 fact，已提交的 Fact 不被
\* 追溯改名（S3）。探针断言其不可达，预期被违反 = witness 找到；
\* 同一 run 内 TerminalOutcomeImmutable 必须同时 PASS（事实没被改写）。
DiagnosticLateStopAfterCommitUnreachable ==
    ~(terminalOutcome # "None" /\ stopRequested)

\* 未提交哨兵（用于"无 consumer 时 commit 是否可达"的正/负控制）。
NeverCommitted ==
    terminalOutcome = "None"

-----------------------------------------------------------------------------
(************************** 场景矩阵（§12 命题） *****************************)

\* 四个场景断言的是 D11 命题在**当前证据**上的判决（Candidate），不是"最终提交值"。
\*
\* 这个区分是被 TLC 逼出来的，不是修辞：把"当前证据"直接写成对提交值的要求
\* （例如 "worker=Stopped ∧ drain=Aborted ∧ stopRequested ⇒ outcome ∈ {None,Stopped}"）
\* 会被一条合法执行击穿——device abort 先按 ¬stopRequested 判为 Failed 并提交，
\* 之后才到达的 stop 意图把状态改写成 (Stopped, Aborted, stopRequested=TRUE)，
\* 而提交值仍是 Failed（不可改写，正确行为）。那正是本轮要找的
\* "判决发生在哪个瞬间"的自由度，不是缺陷。
\*
\* 因此：判决一侧由下列 Candidate 不变式承担；提交值的正当性由
\* NoFalseCompleted / NoFalseStopped（S5/S6）承担——它们只要求提交值能被
\* 当前（单调增长的）证据支持，不会错误地要求实现预知未来的 stop 意图。
ScenarioNaturalEof ==
    (workerTerminal = "Eof" /\ drainVerdict = "Drained")
        => Candidate = "Completed"

ScenarioUserStop ==
    (workerTerminal = "Stopped" /\ drainVerdict = "Aborted" /\ stopRequested)
        => Candidate = "Stopped"

ScenarioDeviceAbortWithoutStop ==
    (workerTerminal = "Stopped" /\ drainVerdict = "Aborted" /\ ~stopRequested)
        => Candidate = "Failed"

ScenarioDecodeFailureNeverStopped ==
    decodeFailure => Candidate # "Stopped"

-----------------------------------------------------------------------------
(***************************** Liveness 性质 ********************************)

\* 条件性进度 —— evidence 决定性后，Fact 最终被提交。
\* **不是**"每首歌最终都会结束"：它只断言"当决定性证据已经存在、提交者在
\* 该状态下持续具备执行条件、并且获得正常执行机会时，提交不依赖外部 observer"。
TerminalEvidenceCommitsEventually ==
    Decisive ~> terminalOutcome # "None"

TeardownCommitsEventually ==
    (episodeLifecycle = "TeardownDone" /\ ~activationFailed)
        ~> terminalOutcome # "None"

\* 未经授权的命题（§15 纪律）：本模型**不得**宣称它。
\* 反向控制：此性质必须在所有变体下被违反，否则说明模型偷偷承诺了
\* "证据一定会到来 / episode 一定会终结"。
EpisodesEventuallyTerminate ==
    <> (terminalOutcome # "None")

=============================================================================
\* #### EOF ####
