--------------------------- MODULE SeekDiscontinuity ---------------------------
(***************************************************************************)
(* SeekDiscontinuity — F5 seek cutover 协议的安全模型（safety-only）。        *)
(*                                                                         *)
(* STATUS: GATE-LOCAL FORMAL EVIDENCE（F5-GATE；非第二份 authority）。        *)
(*   语义真相只有：                                                          *)
(*     ADR-PBK-002 §20 D14.5（seek 语义 spine：cutover commit 之后           *)
(*     pre-seek PCM 不得再成为 post-seek output；reservoir 全覆盖；          *)
(*     不许单一模糊成功位）+ D14.8 的 seek 规则（publication 不得混合        *)
(*     pre/post-cutover handed-off 总量）。                                  *)
(*   本模型只回答一个问题：提议的 same-resource discontinuity protocol      *)
(*   （render leg 不持有设备缓冲才许 park；worker 串行化点丢弃 staging 并    *)
(*   做第 2 相 edge cut；session 在 landing ∧ edge 干净 ∧ 设备尾排空         *)
(*   ∧ leg parked 时 commit；terminal stop 恒赢）在其显式抽象内能否          *)
(*   保证冻结不变式，并且 mutation 证明每条护栏真的承重。                    *)
(*                                                                         *)
(* 模型词 -> current production reality 映射（完整表见 README.md）：          *)
(*   WriteOld/FinishWriteOld   decode worker 的 loop-top -> staging ->        *)
(*                             edge.write（session.rs decode_worker）        *)
(*   PullOld/SubmitHeld        render leg 的 read_frames -> ReleaseBuffer     *)
(*                             （wasapi.rs steady_loop；heldOld = 已拉取      *)
(*                             的一个 chunk，对应 GetBuffer..ReleaseBuffer   *)
(*                             窗口内的设备缓冲内存）                        *)
(*   ParkLeg（¬heldOld）       D14.7 gate park：loop-top、不持有设备缓冲       *)
(*   SessionPurge              edge 第 1 相 invalidate（session 发起；清空    *)
(*                             队列 + 唤醒可能被 full edge 卡住的 writer；   *)
(*                             in-flight 写随后由 FinishWriteOld 落进空      *)
(*                             ring，死于第 2 相）                          *)
(*   WorkerCut                 worker 串行化点的第 2 相 invalidate：staging   *)
(*                             丢弃 + edge 清空 + landing 证据（此后该线程   *)
(*                             只写 post-cut PCM——program order）           *)
(*   CommitCut                 session 的 cutover commit：landing ∧ edge 干净 *)
(*                             ∧ 设备尾排空（D14.7 padding==0 证据）∧ leg    *)
(*                             parked（D14.8 rebase 的触发点）               *)
(*   DrainOld/DrainNew         设备消费（audible truth 的抽象；不变式关注     *)
(*                             的正是 DrainOld 在 commit 之后发生）          *)
(*   Stop                      terminal stop（D11/D14.4；恒赢，协议夭折）     *)
(*   PublishPositionOld/New    D14.8 位置发布的 epoch 归属（rebase 前后）     *)
(*                                                                         *)
(* 不建模（刻意的最小化）：K0/Fiber/Capability、PCM ring 容量与游标、          *)
(* WASAPI 流状态机、请求语法/目标语义、多 seek 排队、Duration、               *)
(* Generation/Window/TimelineSegment（本模型的存在正是为了证明它们不需要）。   *)
(* 无 fairness / liveness：本门只攻击 safety 碰撞（commit 后旧 PCM 复活）。   *)
(* 模型结论只在其显式 abstraction 与 assumptions 下成立；不得静默升级为       *)
(* architecture authority（AGENTS.md "Verification authority boundary"）。    *)
(*                                                                         *)
(* 负控制（Mutation 常量，逐个独立注入；对应交付要求 §31 的靶子）：            *)
(*   MutationCommitBeforeTailPurge   commit 不等设备尾排空（M1）              *)
(*   MutationCutMidWrite             串行化点容忍 worker 在 in-flight 写半路  *)
(*                                   上 cut（staging 未丢弃；in-flight 写    *)
(*                                   在 cut 之后落队）（M2）                  *)
(*   MutationParkWhileHeld           park 允许持有已拉取的旧块（M3；render-   *)
(*                                   held block 攻击）                       *)
(*   MutationStalePositionWriter     commit 后仍按旧 basis 发布位置（M4；     *)
(*                                   D14.8 no-mixing 的对手）                *)
(*   MutationCommitBeforeLanding     decoder 未 reposition 就 commit（M5）    *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS MAX_TAIL, Mutation, MutationNone,
    MutationCommitBeforeTailPurge,
    MutationCutMidWrite,
    MutationParkWhileHeld,
    MutationStalePositionWriter,
    MutationCommitBeforeLanding

MutationChoices == {MutationNone,
                    MutationCommitBeforeTailPurge,
                    MutationCutMidWrite,
                    MutationParkWhileHeld,
                    MutationStalePositionWriter,
                    MutationCommitBeforeLanding}

ASSUME Mutation \in MutationChoices

VARIABLES
    \* —— 两条腿的位置（协议串行化点的抽象）——
    worker,           \* "idle" | "writing_old" | "repositioned"
    parked,           \* render leg 停在 loop-top gate（不持有设备缓冲）
    \* —— stale-PCM 三库（D14.5 reservoir accounting 的抽象）——
    edgeOld,          \* edge 队列可能还有旧 PCM
    heldOld,          \* render leg 手里拿着一个已拉取的旧 chunk
    tailOld,          \* 设备尾里排队的旧 chunk 数（0..MAX_TAIL）
    tailNew,          \* 设备尾里排队的新 chunk 数（0..MAX_TAIL）
    \* —— 协议状态 ——
    cutRequested,     \* seek command 已记录（command，非 fact）
    landing,          \* worker 已发布 landing 证据（第 2 相 cut 之后）
    committed,        \* session 已 commit cutover
    stopped,          \* terminal stop（D11 族；恒赢）
    \* —— verifier-only 见证变量（ghost，非 production representation）——
    oldDrainEver,     \* 是否发生过旧 PCM 的 audible drain（合法，pre-commit）
    oldAfterCommit,   \* 不变式靶：commit 之后旧 PCM 被 drain
    posMixAfterCommit \* 不变式靶：commit 之后按旧 basis 发布位置

vars == <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
          cutRequested, landing, committed, stopped,
          oldDrainEver, oldAfterCommit, posMixAfterCommit>>

-----------------------------------------------------------------------------
(******************************** 初始状态 **********************************)

Init ==
    /\ worker = "idle"
    /\ parked = FALSE
    /\ edgeOld = FALSE
    /\ heldOld = FALSE
    /\ tailOld = 0
    /\ tailNew = 0
    /\ cutRequested = FALSE
    /\ landing = FALSE
    /\ committed = FALSE
    /\ stopped = FALSE
    /\ oldDrainEver = FALSE
    /\ oldAfterCommit = FALSE
    /\ posMixAfterCommit = FALSE

-----------------------------------------------------------------------------
(********************************* 动作 *************************************)

\* —— decode leg（生产者）——

\* loop-top 之后取一个旧 chunk 解码进 staging（staging 本身不入模型：
\* 它唯一可见的效果是 FinishWriteOld 把它写进 edge；M2 攻击的正是
\* 「staging 未丢弃就 cut」这个缺口）。
WriteOld ==
    /\ worker = "idle"
    /\ stopped = FALSE
    /\ worker' = "writing_old"
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* staging 写 edge 完成（真实 edge.write 是全有全无：切片要么整个进队，
\* 要么被 terminal 打断——这里抽象为一次落队）。注意：它不检查
\* committed/landing——一个 in-flight 的写只受自己的线程 program order
\* 支配，这正是 M2 想暴露的缺口形状。
FinishWriteOld ==
    /\ worker = "writing_old"
    /\ stopped = FALSE
    /\ worker' = "idle"
    /\ edgeOld' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— render leg（消费者）——

\* D14.7 gate：loop-top park。承重护栏 = 不持有已拉取的块跨 park
\* （真实不变式：park 时不持有设备缓冲）。M3 攻击它。
ParkLeg ==
    /\ parked = FALSE
    /\ (heldOld = FALSE \/ Mutation = MutationParkWhileHeld)
    /\ stopped = FALSE
    /\ parked' = TRUE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

UnparkLeg ==
    /\ parked = TRUE
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* 从 edge 拉一个旧块进手（GetBuffer..read 之后的窗口）。
PullOld ==
    /\ parked = FALSE
    /\ edgeOld = TRUE
    /\ heldOld = FALSE
    /\ stopped = FALSE
    /\ heldOld' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* ReleaseBuffer 提交手里的块。
SubmitHeld ==
    /\ parked = FALSE
    /\ heldOld = TRUE
    /\ tailOld < MAX_TAIL
    /\ stopped = FALSE
    /\ heldOld' = FALSE
    /\ tailOld' = tailOld + 1
    /\ UNCHANGED <<worker, parked, edgeOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* post-cut 生产/提交。
SubmitNew ==
    /\ parked = FALSE
    /\ worker = "repositioned"
    /\ tailNew < MAX_TAIL
    /\ stopped = FALSE
    /\ tailNew' = tailNew + 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— 设备（audible truth 的抽象：drain 即成为 audible output）——

DrainOld ==
    /\ tailOld > 0
    /\ stopped = FALSE
    /\ tailOld' = tailOld - 1
    /\ oldDrainEver' = TRUE
    /\ oldAfterCommit' = (oldAfterCommit \/ committed)
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   posMixAfterCommit>>

DrainNew ==
    /\ tailNew > 0
    /\ stopped = FALSE
    /\ tailNew' = tailNew - 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— 协议（session / worker 的协调动作）——

\* seek command（Command，非 fact；单飞语义即 cutRequested 的单调性）。
RequestSeek ==
    /\ cutRequested = FALSE
    /\ committed = FALSE
    /\ stopped = FALSE
    /\ cutRequested' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* worker 串行化点：staging 丢弃 + 第 2 相 edge invalidate + landing
\* 证据。承重护栏 = worker 必须在 idle（loop top；不在 in-flight 写的
\* 半路上）。M2 把护栏放开到 writing_old，且不让 worker 前进（in-flight
\* 写还在半路上，稍后由 FinishWriteOld 落队）——stale staging 因此能
\* 在 cut 之后复活。此后该线程按 program order 只写 post-cut PCM。
WorkerCut ==
    /\ cutRequested = TRUE
    /\ (worker = "idle"
        \/ (Mutation = MutationCutMidWrite /\ worker = "writing_old"))
    /\ stopped = FALSE
    /\ worker' = IF Mutation = MutationCutMidWrite
                 THEN worker
                 ELSE "repositioned"
    /\ edgeOld' = FALSE
    /\ landing' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* session 第 1 相 purge（consumer parked 时清空 edge 队列；顺带唤醒
\* 可能被 full edge 卡住的 writer——in-flight 写随后落进空 ring，
\* 死于第 2 相 WorkerCut。对安全不变式非承重，是协议形状的忠实记录）。
SessionPurge ==
    /\ parked = TRUE
    /\ edgeOld = TRUE
    /\ stopped = FALSE
    /\ edgeOld' = FALSE
    /\ UNCHANGED <<worker, parked, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* session 的 cutover commit（D14.8 rebase 的触发点）。承重护栏 =
\* landing ∧ edge 干净 ∧ 设备尾排空 ∧ leg parked。M1 攻击尾排空，
\* M5 攻击 landing。
CommitCut ==
    /\ cutRequested = TRUE
    /\ (landing = TRUE \/ Mutation = MutationCommitBeforeLanding)
    /\ edgeOld = FALSE
    /\ (tailOld = 0 \/ Mutation = MutationCommitBeforeTailPurge)
    /\ parked = TRUE
    /\ committed = FALSE
    /\ stopped = FALSE
    /\ committed' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* terminal stop：恒赢。协议夭折（committed 永不成立），两腿退出。
Stop ==
    /\ stopped = FALSE
    /\ stopped' = TRUE
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— 位置发布（D14.8 seek 规则：publication 不得混合 pre/post basis）——

\* 旧 basis 的发布：commit 之后必须不可能（rebase 在 commit 点发生，
\* 同一 writer 同一执行路径）。M4 攻击它。
PublishPositionOld ==
    /\ committed = FALSE
    /\ stopped = FALSE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* 新 basis 的发布（rebase 之后；数值不属于本模型——D14.8 把
\* representation 留给实现门，这里只检查 epoch 归属）。
PublishPositionNew ==
    /\ committed = TRUE
    /\ stopped = FALSE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* M4 注入：commit 之后仍按旧 basis 发布（stale writer 复用）。
StalePositionPublish ==
    /\ Mutation = MutationStalePositionWriter
    /\ committed = TRUE
    /\ stopped = FALSE
    /\ posMixAfterCommit' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, stopped,
                   oldDrainEver, oldAfterCommit>>

Next ==
    \/ WriteOld
    \/ FinishWriteOld
    \/ ParkLeg
    \/ UnparkLeg
    \/ PullOld
    \/ SubmitHeld
    \/ SubmitNew
    \/ DrainOld
    \/ DrainNew
    \/ RequestSeek
    \/ WorkerCut
    \/ SessionPurge
    \/ CommitCut
    \/ Stop
    \/ PublishPositionOld
    \/ PublishPositionNew
    \/ StalePositionPublish

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(***************************** 安全不变式 ***********************************)

TypeOK ==
    /\ worker \in {"idle", "writing_old", "repositioned"}
    /\ parked \in BOOLEAN
    /\ edgeOld \in BOOLEAN
    /\ heldOld \in BOOLEAN
    /\ tailOld \in 0..MAX_TAIL
    /\ tailNew \in 0..MAX_TAIL
    /\ cutRequested \in BOOLEAN
    /\ landing \in BOOLEAN
    /\ committed \in BOOLEAN
    /\ stopped \in BOOLEAN
    /\ oldDrainEver \in BOOLEAN
    /\ oldAfterCommit \in BOOLEAN
    /\ posMixAfterCommit \in BOOLEAN

\* F5 根不变式（D14.5）：commit 之后旧 PCM 不得再成为 output。
InvStaleOutput == oldAfterCommit = FALSE

\* D14.8 seek 规则：publication 不得混合 pre/post-cutover basis。
InvPositionNoMixing == posMixAfterCommit = FALSE

\* commit 时刻状态（护栏的自检：commit 蕴含 edge 干净 + 尾排空 + landing；
\* 这是不变式化了的 commit 前置，不是新增机制）。
InvCommitPurged ==
    committed = FALSE \/ (landing = TRUE /\ edgeOld = FALSE /\ tailOld = 0)

-----------------------------------------------------------------------------
(************************** witness 探针（MUST-FAIL）************************)

\* 探针 1：committed 必须可达（否则核心不变式空洞）。
ProbeNeverCommitted == committed = FALSE

\* 探针 2：commit 之前的旧 PCM audible drain 必须可达（旧输出在
\* pre-commit 窗口是合法的；不变式家族因此非空洞——它禁的不是旧输出，
\* 而是 commit 之后的旧输出）。
ProbeNoOldDrainEver == oldDrainEver = FALSE

\* 探针 3：edge 队列的旧 PCM 在 pre-commit 窗口必须可达（否则 edge 库
\* 在模型里从未出现，三库记账是摆设）。
ProbeNoOldEdgeEver == edgeOld = FALSE

=============================================================================
