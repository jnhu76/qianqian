--------------------------- MODULE SeekDiscontinuity ---------------------------
(***************************************************************************)
(* SeekDiscontinuity — F5 seek cutover 协议的安全模型（safety-only）。        *)
(*                                                                         *)
(* STATUS: GATE-LOCAL FORMAL EVIDENCE（F5-GATE；非第二份 authority）。        *)
(*   语义真相只有：                                                          *)
(*     ADR-PBK-002 §20 D14.5（seek 语义 spine + 2026-09-17 F5-GATE           *)
(*     amendment：cutover commit 之后 pre-seek PCM 不得再成为 post-seek      *)
(*     output；reservoir 全覆盖；不许单一模糊成功位）+ D14.8 的 seek 规则     *)
(*     （publication 不得混合 pre/post-cutover handed-off 总量）。           *)
(*   本模型只回答一个问题：冻结的 same-resource discontinuity protocol      *)
(*   （render leg 持有设备缓冲即不许 park；worker 在串行化点上先 song_seek、 *)
(*   成功后才自行 purge 并持有产量；session 在 landing ∧ edge 干净 ∧         *)
(*   设备尾排空 ∧ leg parked ∧ unsettled 时 commit；terminal stop 恒赢）     *)
(*   在其显式抽象内能否保证冻结不变式，并且 mutation 证明每条护栏承重。      *)
(*                                                                         *)
(* 模型词 -> current production reality 映射（完整表见 README.md）：          *)
(*   WriteOld/FinishWriteOld   decode worker 的 loop-top -> staging ->        *)
(*                             edge.write（session.rs decode_worker）        *)
(*   PullOld/SubmitHeld        render leg 的 read_frames -> ReleaseBuffer     *)
(*                             （wasapi.rs steady_loop；heldOld = 已拉取      *)
(*                             的一个 chunk，对应 GetBuffer..ReleaseBuffer   *)
(*                             窗口内的设备缓冲内存）                        *)
(*   ParkLeg（¬heldOld）       D14.7 gate park（loop-top、无设备缓冲跨 park； *)
(*                             F5 中由 session 在命令后路由为 cut 归属）      *)
(*   WorkerSeek                worker 串行化点上的 song_seek（BEFORE 一切     *)
(*                             invalidation；refusal 是 pre-cut，无损）      *)
(*   SeekRefused               song_seek 拒绝（SEEK_UNSUPPORTED/ERROR/       *)
(*                             INVALID_ARGUMENT）：无任何 invalidation，     *)
(*                             生产从当前 cursor 继续；seek 命令消费掉       *)
(*   WorkerCut                 song_seek 成功后 worker 自行的 purge：staging  *)
(*                             丢弃 + edge 清空 + landing 证据 + 持有产量    *)
(*                             （此后、直到 release，production 静默——模型   *)
(*                             里由 SubmitNew 的 committed 护栏承载）        *)
(*   CommitCut                 session 的 cutover commit：landing ∧ edge 干净 *)
(*                             ∧ 设备尾排空（D14.7 padding==0 证据）∧ leg     *)
(*                             parked ∧ episode unsettled（D14.8 rebase 的   *)
(*                             触发点）                                     *)
(*   DrainOld/DrainNew         设备消费（audible truth 的抽象；不变式关注     *)
(*                             的正是 DrainOld 在 commit 之后发生）          *)
(*   Stop                      terminal stop（D11/D14.4；恒赢，协议夭折）     *)
(*   PublishPositionOld/New    D14.8 位置发布的 epoch 归属（rebase 前后）     *)
(*                                                                         *)
(* 不建模（刻意的最小化）：K0/Fiber/Capability、PCM ring 容量与游标、          *)
(* song_seek 的时长、WASAPI 流状态机、请求语法/目标语义、多 seek 排队、       *)
(* Duration、Generation/Window/TimelineSegment（本模型的存在正是为了证明      *)
(* 它们不需要）。无 fairness / liveness：本门只攻击 safety 碰撞。             *)
(* 模型结论只在其显式 abstraction 与 assumptions 下成立；不得静默升级为       *)
(* architecture authority（AGENTS.md "Verification authority boundary"）。    *)
(*                                                                         *)
(* 负控制（Mutation 常量，逐个独立注入；对应交付要求 §31 的靶子）：            *)
(*   MutationCommitBeforeTailPurge   commit 不等设备尾排空（M1）              *)
(*   MutationSeekMidWrite            song_seek 容忍 worker 在 in-flight 写    *)
(*                                   半路上执行且 staging 未丢弃（M2；写随后  *)
(*                                   落队，旧 PCM 在 cut 之后复活）           *)
(*   MutationParkWhileHeld           park 允许持有已拉取的旧块（M3；render-    *)
(*                                   held block 攻击）                       *)
(*   MutationStalePositionWriter     commit 后仍按旧 basis 发布位置（M4；     *)
(*                                   注意：base 模型中位置发布动作是惰性的，  *)
(*                                   该不变式的 base 保证来自「单一 writer    *)
(*                                   在自身路径上 rebase」这一构造性事实，    *)
(*                                   M4 只证明注入缺陷确实可达并可检出）      *)
(*   MutationCommitBeforeLanding     decoder 未 reposition 就 commit（M5）    *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS MAX_TAIL, Mutation, MutationNone,
    MutationCommitBeforeTailPurge,
    MutationSeekMidWrite,
    MutationParkWhileHeld,
    MutationStalePositionWriter,
    MutationCommitBeforeLanding

MutationChoices == {MutationNone,
                    MutationCommitBeforeTailPurge,
                    MutationSeekMidWrite,
                    MutationParkWhileHeld,
                    MutationStalePositionWriter,
                    MutationCommitBeforeLanding}

ASSUME Mutation \in MutationChoices

VARIABLES
    \* —— 两条腿的位置（协议串行化点的抽象）——
    worker,           \* "idle" | "writing_old" | "seeking" | "repositioned"
    parked,           \* render leg 停在 loop-top gate（不持有设备缓冲）
    \* —— stale-PCM 三库（D14.5 reservoir accounting 的抽象）——
    edgeOld,          \* edge 队列可能还有旧 PCM
    heldOld,          \* render leg 手里拿着一个已拉取的旧 chunk
    tailOld,          \* 设备尾里排队的旧 chunk 数（0..MAX_TAIL）
    tailNew,          \* 设备尾里排队的新 chunk 数（0..MAX_TAIL）
    \* —— 协议状态 ——
    cutRequested,     \* seek command 在飞（command，非 fact；单飞语义）
    landing,          \* worker 已发布 landing 证据（成功 seek 的 purge 之后）
    committed,        \* session 已 commit cutover
    seekFailed,       \* 最近一次 seek 被 song_seek 拒绝（pre-cut，惰性）
    stopped,          \* terminal stop（D11 族；恒赢）
    \* —— verifier-only 见证变量（ghost，非 production representation）——
    oldDrainEver,     \* 是否发生过旧 PCM 的 audible drain（合法，pre-commit）
    oldAfterCommit,   \* 不变式靶：commit 之后旧 PCM 被 drain
    posMixAfterCommit \* 不变式靶：commit 之后按旧 basis 发布位置

vars == <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
          cutRequested, landing, committed, seekFailed, stopped,
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
    /\ seekFailed = FALSE
    /\ stopped = FALSE
    /\ oldDrainEver = FALSE
    /\ oldAfterCommit = FALSE
    /\ posMixAfterCommit = FALSE

-----------------------------------------------------------------------------
(********************************* 动作 *************************************)

\* —— decode leg（生产者）——

\* loop-top 之后取一个旧 chunk 解码进 staging（staging 本身不入模型：
\* 它唯一可见的效果是 FinishWriteOld 把它写进 edge；M2 攻击的正是
\* 「staging 未丢弃就 seek/cut」这个缺口）。
WriteOld ==
    /\ worker = "idle"
    /\ stopped = FALSE
    /\ worker' = "writing_old"
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
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
                   cutRequested, landing, committed, seekFailed, stopped,
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
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

UnparkLeg ==
    /\ parked = TRUE
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* 从 edge 拉一个旧块进手（GetBuffer..read 之后的窗口）。
PullOld ==
    /\ parked = FALSE
    /\ edgeOld = TRUE
    /\ heldOld = FALSE
    /\ stopped = FALSE
    /\ heldOld' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
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
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* post-commit 生产/提交（协议：worker 在 landing 与 release 之间持有
\* 产量，所以 SubmitNew 只在 committed 之后存在——rebase basis 因此精确）。
SubmitNew ==
    /\ parked = FALSE
    /\ worker = "repositioned"
    /\ committed = TRUE
    /\ tailNew < MAX_TAIL
    /\ stopped = FALSE
    /\ tailNew' = tailNew + 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— 设备（audible truth 的抽象：drain 即成为 audible output）——

DrainOld ==
    /\ tailOld > 0
    /\ stopped = FALSE
    /\ tailOld' = tailOld - 1
    /\ oldDrainEver' = TRUE
    /\ oldAfterCommit' = (oldAfterCommit \/ committed)
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
                   posMixAfterCommit>>

DrainNew ==
    /\ tailNew > 0
    /\ stopped = FALSE
    /\ tailNew' = tailNew - 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— 协议（session / worker 的协调动作）——

\* seek command（Command，非 fact；单飞语义 = cutRequested 单调占用；
\* refusal 消费掉它，允许后续再次 seek）。
RequestSeek ==
    /\ cutRequested = FALSE
    /\ stopped = FALSE
    /\ cutRequested' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* worker 串行化点上的 song_seek：发生在 BEFORE 一切 invalidation
\* （frozen order）——refusal 因此是 pre-cut、无损。承重护栏 = worker
\* 必须在 idle（loop top）；M2 把它放开到 writing_old（in-flight 写
\* 还在半路上、staging 未丢弃）。
WorkerSeek ==
    /\ cutRequested = TRUE
    /\ (worker = "idle"
        \/ (Mutation = MutationSeekMidWrite /\ worker = "writing_old"))
    /\ stopped = FALSE
    /\ worker' = "seeking"
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* song_seek 拒绝（基模型里的合法 nondeterminism）：什么都没有被
\* invalidation——生产从当前 cursor 继续；命令被消费，可再次 seek。
SeekRefused ==
    /\ worker = "seeking"
    /\ stopped = FALSE
    /\ worker' = "idle"
    /\ cutRequested' = FALSE
    /\ seekFailed' = TRUE
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* song_seek 成功（基模型里的合法 nondeterminism）之后 worker 自行的
\* purge：staging 丢弃 + edge 清空 + landing 证据 + 持有产量。此后该
\* 线程按 program order 在 release 前不再写任何块。M2 注入点：seek
\* 发生在 writing_old 半路上时，in-flight 写仍在——worker 回到
\* writing_old（而不是 repositioned），写随后落队，旧 PCM 在 purge
\* 之后复活。
WorkerCut ==
    /\ worker = "seeking"
    /\ stopped = FALSE
    /\ worker' = IF Mutation = MutationSeekMidWrite
                 THEN "writing_old"
                 ELSE "repositioned"
    /\ edgeOld' = FALSE
    /\ landing' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* session 的 cutover commit（D14.8 rebase 的触发点）。承重护栏 =
\* landing ∧ edge 干净 ∧ 设备尾排空 ∧ leg parked ∧ unsettled。
\* M1 攻击尾排空，M5 攻击 landing。
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
                   cutRequested, landing, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* terminal stop：恒赢。协议夭折（committed 永不成立），两腿退出。
Stop ==
    /\ stopped = FALSE
    /\ stopped' = TRUE
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* —— 位置发布（D14.8 seek 规则：publication 不得混合 pre/post basis）——

\* 旧 basis 的发布：commit 之后必须不可能（rebase 在 commit 点发生，
\* 同一 writer 同一执行路径）。M4 攻击它。注意：这两个发布动作在
\* base 模型里是惰性的——位置不变式的 base 保证来自「单一 writer 在
\* 自身路径上 rebase」的构造性事实（D14.5 amendment position-rebase
\* 条），M4 只证明注入缺陷可达且可检出。
PublishPositionOld ==
    /\ committed = FALSE
    /\ stopped = FALSE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* 新 basis 的发布（rebase 之后；数值不属于本模型——D14.8 把
\* representation 留给实现门，这里只检查 epoch 归属）。
PublishPositionNew ==
    /\ committed = TRUE
    /\ stopped = FALSE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit>>

\* M4 注入：commit 之后仍按旧 basis 发布（stale writer 复用）。
StalePositionPublish ==
    /\ Mutation = MutationStalePositionWriter
    /\ committed = TRUE
    /\ stopped = FALSE
    /\ posMixAfterCommit' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopped,
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
    \/ WorkerSeek
    \/ SeekRefused
    \/ WorkerCut
    \/ CommitCut
    \/ Stop
    \/ PublishPositionOld
    \/ PublishPositionNew
    \/ StalePositionPublish

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(***************************** 安全不变式 ***********************************)

TypeOK ==
    /\ worker \in {"idle", "writing_old", "seeking", "repositioned"}
    /\ parked \in BOOLEAN
    /\ edgeOld \in BOOLEAN
    /\ heldOld \in BOOLEAN
    /\ tailOld \in 0..MAX_TAIL
    /\ tailNew \in 0..MAX_TAIL
    /\ cutRequested \in BOOLEAN
    /\ landing \in BOOLEAN
    /\ committed \in BOOLEAN
    /\ seekFailed \in BOOLEAN
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

\* 探针 4：seek 拒绝路径必须可达（refusal 是 frozen 协议的一半；若不可
\* 达，则 WorkerSeek/SeekRefused 的分解是摆设）。
ProbeNoRefusalEver == seekFailed = FALSE

=============================================================================
