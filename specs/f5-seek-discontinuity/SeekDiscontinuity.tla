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
(*                             invalidation）。基座允许从 idle（块间）或      *)
(*                             writing_partial（块内前缀已落队、余量保留）    *)
(*                             进入——这正是 E2 bounded-slice 写的中途形状。   *)
(*   SeekRefusedUnchanged      song_seek 的「证毕 pre-mutation 拒绝」         *)
(*                             （INVALID_ARGUMENT 类）：无任何 invalidation， *)
(*                             且余量保留（writing_partial 原样返回，由       *)
(*                             FinishWriteOld 补完）——refused 输出 = 无 seek  *)
(*                             对照（零内容损失）。命令消费掉，可再次 seek。   *)
(*   SeekMutatedThenFailed     song_seek 的「非证毕 pre-mutation 失败」       *)
(*                             （generic SEEK_ERROR——ABI 里它也可能在 demuxer *)
(*                             成功重定位 + decoder flush 之后返回——以及     *)
(*                             SEEK_UNSUPPORTED/STREAM_CHANGE/DECODE_ERROR）：*)
(*                             旧 decoder 延续不再有保证，episode 走既有      *)
(*                             decode-failure 路线（episodeFailed；绝不伪装   *)
(*                             成 refusal 恢复旧播放）。保守规则：不能证明    *)
(*                             无扰动 = destructive。                        *)
(*   WorkerCut                 song_seek 成功后 worker 自行的 purge：staging  *)
(*                             丢弃（含保留余量）+ edge 清空 + landing 证据 + *)
(*                             持有产量                                       *)
(*   CommitCut                 session 的 cutover commit：landing ∧ edge 干净 *)
(*                             ∧ 设备尾排空（D14.7 padding==0 证据）∧ leg     *)
(*                             parked ∧ episode unsettled（D14.8 rebase 的   *)
(*                             触发点）                                     *)
(*   DrainOld/DrainNew         设备消费（audible truth 的抽象；不变式关注     *)
(*                             的正是 DrainOld 在 commit 之后发生）          *)
(*   Stop                      terminal stop（D11/D14.4；恒赢，协议夭折）     *)
(*   WritePartial              bounded-slice 写的前缀落队（余量仍在 staging； *)
(*                             E2 abandon+resume 形状的模型对应物）          *)
(*   episodeFailed             SeekMutatedThenFailed 后的既有 decode-failure *)
(*                             路线：生产终止、设备尾自然排空、episode 终止   *)
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
(*   MutationRefusalDropsRemainder   refusal 丢弃保留余量的一帧/余段（M6；   *)
(*                                   refused 输出 ≠ 无 seek 对照——内容损失）  *)
(*   MutationResumeAfterMutatedSeek  destructive failure 后恢复旧游标生产    *)
(*                                   （M7；正是 F5 要消灭的 pre/post 混合）   *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS MAX_TAIL, Mutation, MutationNone,
    MutationCommitBeforeTailPurge,
    MutationSeekMidWrite,
    MutationParkWhileHeld,
    MutationStalePositionWriter,
    MutationCommitBeforeLanding,
    MutationRefusalDropsRemainder,
    MutationResumeAfterMutatedSeek

MutationChoices == {MutationNone,
                    MutationCommitBeforeTailPurge,
                    MutationSeekMidWrite,
                    MutationParkWhileHeld,
                    MutationStalePositionWriter,
                    MutationCommitBeforeLanding,
                    MutationRefusalDropsRemainder,
                    MutationResumeAfterMutatedSeek}

ASSUME Mutation \in MutationChoices

VARIABLES
    \* —— 两条腿的位置（协议串行化点的抽象）——
    worker,           \* "idle" | "writing_old" | "writing_partial"
                      \* | "seeking" | "repositioned" | "decode_failed"
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
    seekFailed,       \* 最近一次 seek 被「证毕 pre-mutation 拒绝」
                      \* （RefusedUnchanged；pre-cut，惰性）
    episodeFailed,    \* SeekMutatedThenFailed 后走既有 decode-failure 路线
    stopped,          \* terminal stop（D11 族；恒赢）
    \* —— verifier-only 见证变量（ghost，非 production representation）——
    oldDrainEver,     \* 是否发生过旧 PCM 的 audible drain（合法，pre-commit）
    oldAfterCommit,   \* 不变式靶：commit 之后旧 PCM 被 drain
    posMixAfterCommit, \* 不变式靶：commit 之后按旧 basis 发布位置
    seekFromPartial,  \* 在飞的 seek 从 writing_partial 进入（余量在外）
    remainderLost,    \* 不变式靶：refusal 丢掉了保留余量（M6 靶）
    producedAfterFailure \* 不变式靶：destructive failure 后恢复生产（M7 靶）

vars == <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
          cutRequested, landing, committed, seekFailed, episodeFailed,
          stopped, oldDrainEver, oldAfterCommit, posMixAfterCommit,
          seekFromPartial, remainderLost, producedAfterFailure>>

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
    /\ episodeFailed = FALSE
    /\ stopped = FALSE
    /\ oldDrainEver = FALSE
    /\ oldAfterCommit = FALSE
    /\ posMixAfterCommit = FALSE
    /\ seekFromPartial = FALSE
    /\ remainderLost = FALSE
    /\ producedAfterFailure = FALSE

-----------------------------------------------------------------------------
(********************************* 动作 *************************************)

\* —— decode leg（生产者）——

\* loop-top 之后取一个旧 chunk 解码进 staging（staging 本身不入模型：
\* 它唯一可见的效果是 FinishWriteOld 把它写进 edge；M2 攻击的正是
\* 「staging 未丢弃就 seek/cut」这个缺口）。
WriteOld ==
    /\ worker = "idle"
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ worker' = "writing_old"
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* bounded-slice 写的前缀落队：真实协议允许命令观察把 in-flight 块
\* 停在已写前缀上（E2 的 abandon 形状），余量保留在 staging——由
\* seek 结果决定补完（refusal）或随 cut 丢弃（成功）。
WritePartial ==
    /\ worker = "writing_old"
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ worker' = "writing_partial"
    /\ edgeOld' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* staging 写 edge 完成（writing_old：整块一次落队；writing_partial：
\* 保留余量补完——refusal 的零内容损失义务）。注意：它不检查
\* committed/landing——一个 in-flight 的写只受自己的线程 program order
\* 支配，这正是 M2 想暴露的缺口形状。
FinishWriteOld ==
    /\ worker \in {"writing_old", "writing_partial"}
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ worker' = "idle"
    /\ edgeOld' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* —— render leg（消费者）——

\* D14.7 gate：loop-top park。承重护栏 = 不持有已拉取的块跨 park
\* （真实不变式：park 时不持有设备缓冲）。M3 攻击它。
ParkLeg ==
    /\ parked = FALSE
    /\ (heldOld = FALSE \/ Mutation = MutationParkWhileHeld)
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ parked' = TRUE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

UnparkLeg ==
    /\ parked = TRUE
    /\ episodeFailed = FALSE
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* 从 edge 拉一个旧块进手（GetBuffer..read 之后的窗口）。
PullOld ==
    /\ parked = FALSE
    /\ edgeOld = TRUE
    /\ heldOld = FALSE
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ heldOld' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* ReleaseBuffer 提交手里的块。
SubmitHeld ==
    /\ parked = FALSE
    /\ heldOld = TRUE
    /\ tailOld < MAX_TAIL
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ heldOld' = FALSE
    /\ tailOld' = tailOld + 1
    /\ UNCHANGED <<worker, parked, edgeOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* post-commit 生产/提交（协议：worker 在 landing 与 release 之间持有
\* 产量，所以 SubmitNew 只在 committed 之后存在——rebase basis 因此精确）。
SubmitNew ==
    /\ parked = FALSE
    /\ worker = "repositioned"
    /\ committed = TRUE
    /\ tailNew < MAX_TAIL
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ tailNew' = tailNew + 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* —— 设备（audible truth 的抽象：drain 即成为 audible output）——
\* （drain 不 gate episodeFailed：destructive failure 之后，已入设备尾
\* 的旧 PCM 仍会自然播出，然后流终止——这是既有失败路线的合法形态。）

DrainOld ==
    /\ tailOld > 0
    /\ stopped = FALSE
    /\ tailOld' = tailOld - 1
    /\ oldDrainEver' = TRUE
    /\ oldAfterCommit' = (oldAfterCommit \/ committed)
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, posMixAfterCommit,
                   seekFromPartial, remainderLost, producedAfterFailure>>

DrainNew ==
    /\ tailNew > 0
    /\ stopped = FALSE
    /\ tailNew' = tailNew - 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* —— 协议（session / worker 的协调动作）——

\* seek command（Command，非 fact；单飞语义 = cutRequested 单调占用；
\* refusal 消费掉它，允许后续再次 seek）。
RequestSeek ==
    /\ cutRequested = FALSE
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ cutRequested' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, seekFailed, episodeFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   seekFromPartial, remainderLost, producedAfterFailure>>

\* worker 串行化点上的 song_seek：发生在 BEFORE 一切 invalidation
\* （frozen order）。基座允许从 idle（块间）或 writing_partial（块内
\* 前缀已落队、余量保留——E2 abandon 形状）进入；M2 把它放开到
\* writing_old（staging 还没落过任何片、也没有保留语义可言的形状）。
WorkerSeek ==
    /\ cutRequested = TRUE
    /\ (worker \in {"idle", "writing_partial"}
        \/ (Mutation = MutationSeekMidWrite /\ worker = "writing_old"))
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ worker' = "seeking"
    /\ seekFromPartial' = (worker = "writing_partial")
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, remainderLost, producedAfterFailure>>

\* song_seek 的「证毕 pre-mutation 拒绝」（INVALID_ARGUMENT 类；基模型
\* 里的合法 nondeterminism）：什么都没有被 invalidation，且保留余量
\* 原样归还（writing_partial → writing_partial，由 FinishWriteOld 补完
\* ——refused 输出因此 = 无 seek 对照，零内容损失）。M6 注入：余量
\* 被丢弃（remainderLost，内容不连续 witness）。命令被消费，可再次 seek。
SeekRefusedUnchanged ==
    /\ worker = "seeking"
    /\ stopped = FALSE
    /\ worker' = IF seekFromPartial /\ Mutation # MutationRefusalDropsRemainder
                 THEN "writing_partial"
                 ELSE "idle"
    /\ remainderLost' = (seekFromPartial /\ Mutation = MutationRefusalDropsRemainder)
    /\ seekFromPartial' = FALSE
    /\ cutRequested' = FALSE
    /\ seekFailed' = TRUE
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, episodeFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   producedAfterFailure>>

\* song_seek 的「非证毕 pre-mutation 失败」（generic SEEK_ERROR——ABI
\* 里它也可能在 demuxer 重定位成功 + decoder flush/reset 之后返回——
\* 以及 SEEK_UNSUPPORTED/STREAM_CHANGE/DECODE_ERROR；基模型里的合法
\* nondeterminism）：旧 decoder 延续不再有保证。episode 走既有
\* decode-failure 路线：生产终止、设备尾自然排空、流终止。绝不伪装成
\* refusal 恢复旧播放——M7 注入的正是那种恢复。
SeekMutatedThenFailed ==
    /\ worker = "seeking"
    /\ stopped = FALSE
    /\ worker' = "decode_failed"
    /\ episodeFailed' = TRUE
    /\ seekFromPartial' = FALSE
    /\ cutRequested' = FALSE
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, seekFailed, stopped,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   remainderLost, producedAfterFailure>>

\* M7 注入：destructive failure 之后恢复旧游标生产——旧库里的旧 PCM
\* 加上重定位后的新内容，无 cutover、无 rebase，直接混合（F5 要消灭
\* 的 pre/post mixing 的 failure 路线形态）。
ResumeAfterFailure ==
    /\ Mutation = MutationResumeAfterMutatedSeek
    /\ episodeFailed = TRUE
    /\ worker = "decode_failed"
    /\ stopped = FALSE
    /\ worker' = "writing_old"
    /\ producedAfterFailure' = TRUE
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost>>

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
    /\ seekFromPartial' = FALSE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, committed, seekFailed, episodeFailed,
                   stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, remainderLost, producedAfterFailure>>

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
    /\ episodeFailed = FALSE
    /\ committed' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, seekFailed, episodeFailed,
                   stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* terminal stop：恒赢。协议夭折（committed 永不成立），两腿退出。
Stop ==
    /\ stopped = FALSE
    /\ stopped' = TRUE
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* —— 位置发布（D14.8 seek 规则：publication 不得混合 pre/post basis）——

\* 旧 basis 的发布：commit 之后必须不可能（rebase 在 commit 点发生，
\* 同一 writer 同一执行路径）。M4 攻击它。注意：这两个发布动作在
\* base 模型里是惰性的——位置不变式的 base 保证来自「单一 writer 在
\* 自身路径上 rebase」的构造性事实（D14.5 amendment position-rebase
\* 条），M4 只证明注入缺陷可达且可检出。
PublishPositionOld ==
    /\ committed = FALSE
    /\ stopped = FALSE
    /\ episodeFailed = FALSE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* 新 basis 的发布（rebase 之后；数值不属于本模型——D14.8 把
\* representation 留给实现门，这里只检查 epoch 归属）。
PublishPositionNew ==
    /\ committed = TRUE
    /\ stopped = FALSE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   posMixAfterCommit, seekFromPartial, remainderLost,
                   producedAfterFailure>>

\* M4 注入：commit 之后仍按旧 basis 发布（stale writer 复用）。
StalePositionPublish ==
    /\ Mutation = MutationStalePositionWriter
    /\ committed = TRUE
    /\ stopped = FALSE
    /\ posMixAfterCommit' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   episodeFailed, stopped, oldDrainEver, oldAfterCommit,
                   seekFromPartial, remainderLost, producedAfterFailure>>

Next ==
    \/ WriteOld
    \/ WritePartial
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
    \/ SeekRefusedUnchanged
    \/ SeekMutatedThenFailed
    \/ WorkerCut
    \/ CommitCut
    \/ Stop
    \/ PublishPositionOld
    \/ PublishPositionNew
    \/ StalePositionPublish
    \/ ResumeAfterFailure

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(***************************** 安全不变式 ***********************************)

TypeOK ==
    /\ worker \in {"idle", "writing_old", "writing_partial", "seeking",
                   "repositioned", "decode_failed"}
    /\ parked \in BOOLEAN
    /\ edgeOld \in BOOLEAN
    /\ heldOld \in BOOLEAN
    /\ tailOld \in 0..MAX_TAIL
    /\ tailNew \in 0..MAX_TAIL
    /\ cutRequested \in BOOLEAN
    /\ landing \in BOOLEAN
    /\ committed \in BOOLEAN
    /\ seekFailed \in BOOLEAN
    /\ episodeFailed \in BOOLEAN
    /\ stopped \in BOOLEAN
    /\ oldDrainEver \in BOOLEAN
    /\ oldAfterCommit \in BOOLEAN
    /\ posMixAfterCommit \in BOOLEAN
    /\ seekFromPartial \in BOOLEAN
    /\ remainderLost \in BOOLEAN
    /\ producedAfterFailure \in BOOLEAN

\* F5 根不变式（D14.5）：commit 之后旧 PCM 不得再成为 output。
InvStaleOutput == oldAfterCommit = FALSE

\* D14.8 seek 规则：publication 不得混合 pre/post-cutover basis。
InvPositionNoMixing == posMixAfterCommit = FALSE

\* commit 时刻状态（护栏的自检：commit 蕴含 edge 干净 + 尾排空 + landing；
\* 这是不变式化了的 commit 前置，不是新增机制）。
InvCommitPurged ==
    committed = FALSE \/ (landing = TRUE /\ edgeOld = FALSE /\ tailOld = 0)

\* 拒绝零内容损失（corrective 1）：RefusedUnchanged 不得丢掉保留余量
\* ——refused 输出必须与无 seek 对照连续一致（E2 REFUSAL-EQUIV 的
\* 模型对应物）。M6 攻击它。
InvRefusalContentContinuous == remainderLost = FALSE

\* 失败路线封闭（corrective 1）：MutatedThenFailed 之后不得恢复生产
\* ——重定位后的 decoder 内容不得无 cutover/rebase 混入数据面（E2
\* FAIL-CLOSED 的模型对应物）。M7 攻击它。
InvFailClosed == producedAfterFailure = FALSE

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

\* 探针 4：seek 拒绝路径必须可达（RefusedUnchanged 是 frozen 协议的
\* 一支；若不可达，则 WorkerSeek/SeekRefusedUnchanged 的分解是摆设）。
ProbeNoRefusalEver == seekFailed = FALSE

\* 探针 5：destructive failure 路线必须可达（corrective 1：不能证明
\* 无扰动的 seek 失败是真实存在的 provider 行为；若不可达，则
\* InvFailClosed/M7 是摆设）。
ProbeNoDestructiveEver == episodeFailed = FALSE

\* 探针 6：从 writing_partial 进入的 seek（余量在外）必须可达——
\* 余量保留/补完义务与 M6 承重的前提形状。
ProbeNoPartialSeekEver == seekFromPartial = FALSE

=============================================================================
