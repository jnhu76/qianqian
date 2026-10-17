--------------------------- MODULE SeekDiscontinuity ---------------------------
(***************************************************************************)
(* SeekDiscontinuity — F5 seek cutover 协议的安全模型（safety-only）。        *)
(*                                                                         *)
(* STATUS: GATE-LOCAL FORMAL EVIDENCE（F5-GATE；非第二份 authority）。        *)
(*   语义真相只有：                                                          *)
(*     ADR-PBK-002 §20 D14.5（seek 语义 spine + F5-GATE amendment：cutover   *)
(*     commit 之后 pre-seek PCM 不得再成为 post-seek output；reservoir 全    *)
(*     覆盖；不许单一模糊成功位）+ D14.8 的 seek 规则（publication 不得混合  *)
(*     pre/post-cutover handed-off 总量）+ §17 D11（episode 终局；本模型只   *)
(*     携带其最小受约束投影，见 TERMINAL_DECISION_MAP / README）。           *)
(*                                                                         *)
(* D-C 扩展（#289；receipt issues/289#issuecomment-6229911387，2026-10-17）：*)
(*   在本模型内补齐 GAP-01 的 pending-fatal latch 交错：partial success      *)
(*   latch 一枚 C 侧 fatal（edge 仍 Open、session 不知）之后，seek           *)
(*   （Applied 擦除 / Refused 保留 / Mutated 失败闭合）与 stop（intent 与    *)
(*   edge.stop() 两步、其间可交错）以及下一次 read（无前置 terminal 检查，   *)
(*   必复现 latch）的全部有界交错。语义锚点：reconciliation §2.3/§2.3.1/     *)
(*   §2.4（BASE_SHA 实测行号见 PENDING_FATAL_DAG.md；它是本文件的构造依据）。*)
(*                                                                         *)
(*   扩展纪律（D-C MODEL_POLICY）：不升格为 D11 权威；stop-race 的一半仍以    *)
(*   Rust 验证为主（E1-WORKER 是真实行为校准，不是被本模型取代的覆盖）；      *)
(*   TLC 只做有界 safety 交错搜索；任何反例不授权生产修改。                  *)
(*                                                                         *)
(* 模型词 -> current production reality 映射（完整表见 README.md）：          *)
(*   WriteOld/FinishWriteOld   decode worker 的 loop-top -> staging ->        *)
(*                             edge.write（session.rs decode_worker）。       *)
(*                             WriteOld 的 guard ¬pendingFatal 正是「下一次  *)
(*                             read 必复现 latch」（C:1111-1113 先于出帧）：  *)
(*                             latch 存在时唯一可发的读结果是 surfacing。    *)
(*   ReadPartialSuccess        song_read_pcm 的 partial-success 出口：       *)
(*                             produced>0 → SONG_OK，诊断清、latch 存活      *)
(*                             （C:1136-1156）。生产的块照常走写路径。       *)
(*   FatalFailurePublish       失败路线（1/3）错误发布：下一次读在入口       *)
(*                             复现 latch → adapter Err → decode_failed      *)
(*                             （C:1111-1113；session.rs:896-900）。**不受   *)
(*                             stopIntent/edgeTerminal 门控**——loop-top 到   *)
(*                             read 之间无任何 terminal 检查（R1.1 的核心）。*)
(*                             publication+settlement 同一 publish_evidence  *)
(*                             锁程（completion.rs:1319-1331）→ 二者原子；   *)
(*                             **edge.fail 与退出漏斗在该锁程之外**，拆为    *)
(*                             WorkerEdgeFailAdvance / WorkerExitFunnel      *)
(*                             （2026-10-17 终审 P1；W-E 窗口）。            *)
(*   PendingFatal（变量）      SongCore handle 的 fatal_error 位（C:82-83）： *)
(*                             SET=失败迭代（C:1136-1140）、ERASE=仅 song_seek*)
(*                             成功路径的 reset_decode_state（C:1195-1196）、 *)
(*                             RETAIN=seek 失败路径只覆盖诊断（C:1166）。     *)
(*   PullOld/SubmitHeld        render leg 的 read_frames -> ReleaseBuffer     *)
(*                             （wasapi.rs steady_loop；heldOld = 已拉取      *)
(*                             的一个 chunk）。                              *)
(*   ParkLeg（¬heldOld）       D14.7 gate park（loop-top、无设备缓冲跨 park） *)
(*   SeekEligibilityPass /     worker 串行化点上的 song_seek（BEFORE 一切    *)
(*   SeekProviderBegin         invalidation）：eligibility 三查（¬aborted ∧  *)
(*                             parked 证据 ∧ edge Open）**不排除             *)
(*                             pending-fatal**（reconciliation §2.2）；通过  *)
(*                             与 provider begin 之间是显式模型窗口（终审    *)
(*                             P2 拆分表达：intent / edge stop / settlement  *)
(*                             均可落入），begin 与 provider 返回再分离      *)
(*                             （Q6-v：stop 可落在 FFI 调用中途）。          *)
(*   SeekRefusedUnchanged      song_seek 的「证毕 pre-mutation 拒绝」：零     *)
(*                             invalidation、余量保留补完、命令消费。        *)
(*                             **pendingFatal UNCHANGED 是承重点**            *)
(*                             （RetainLatchOnRefused / D-T2）。             *)
(*   SeekMutatedThenFailed     song_seek 的「非证毕 pre-mutation 失败」：     *)
(*                             fail-closed 走既有 decode-failure 路线，      *)
(*                             worker_failure + edge.fail + return（绝不     *)
(*                             伪装成 refusal 恢复旧播放——M7 攻击它）。     *)
(*   WorkerCut                 song_seek 成功（Applied）后 worker 自行的      *)
(*                             purge：staging/余量丢弃 + edge 清空 + landing *)
(*                             证据 + **latch 擦除**（C:1196）+ 持有产量。   *)
(*   CommitCut                 session 的 cutover commit（F5 域；≠ D11        *)
(*                             terminal commit）。前置：landing ∧ edge 干净  *)
(*                             ∧ 尾排空 ∧ parked ∧ episode unsettled。       *)
(*   RecordStopIntent          request_stop 的 completion-lock 步：intent +   *)
(*                             pause/seek-hold 释放同一锁程（Q6-i 融合）。   *)
(*   EdgeStopFromStop          同一 request 的数据面 stop——锁程释放后的       *)
(*                             另一步（completion.rs:577-587）；intent 可见  *)
(*                             而 edge 仍 Open 是真实状态（Q6-iv 禁止融合）。*)
(*   CancelSeekBookkeeping     串行化点上的 seek_aborted 分支：               *)
(*                             release_seek_without_commit + pending=None    *)
(*                             （session.rs:620-625）——**bookkeeping，不是   *)
(*                             worker 退出**；控制流落入 remainder/read      *)
(*                             （R1.2 更正）。D-T3 注入的正是把它改成退出。  *)
(*   WriteStoppedExit          write_observing_seek 零进展 + terminal≠Open   *)
(*                             → return（session.rs:993-994→:884）＝真实      *)
(*                             stop-exit (a)。                               *)
(*   RemainderFlushStopExit    保留余量 flush 的 terminal break（:798-802     *)
(*                             →:805-809）＝真实 stop-exit (b)。             *)
(*   PostCutStopExit           Applied 等待/恢复路径上数据面在 cut 之下终止   *)
(*                             （:725-735/:753-771）＝真实 stop-exit (c)。   *)
(*   EofClose                  read 的 Eof 臂：close_eof + return（:892-895） *)
(*                             ——只从已 commit 的新 continuation 可达。      *)
(*   DrainOld/DrainNew         设备消费（audible truth 的抽象）。            *)
(*   LegObserveAborted/Drained render leg 的终态观察与 drain verdict 证据     *)
(*                             （Aborted：terminal 先于数据，edge.rs:237-239；*)
(*                             Drained：EOF 排干）。                         *)
(*   TerminalCommit            D11 终局的**最小受约束投影**：workerGone +     *)
(*                             drain verdict + worker_failure 按冻结 resolver*)
(*                             次序（completion.rs:1337-1398）一次性计算。   *)
(*                             不复制 episode-terminal-settlement 的整套     *)
(*                             settlement 模型，不产生第二套裁决语义。       *)
(*   PublishPositionOld/New    D14.8 位置发布的 epoch 归属（惰性动作，同基线）*)
(*                                                                         *)
(* 刻意的最小化（同基线 + 本轮新增）：K0/Fiber/Capability、ring 容量与游标、  *)
(* song_seek 时长、WASAPI 流状态机、请求语法/目标语义、多 seek 排队、        *)
(* Duration、Generation/Window/TimelineSegment；设备中断（device abort——     *)
(* resolver 的 Failed{device} 格在模型内不可达，分支保留以忠实转写冻结表）；  *)
(* Frames(0) 退出（真实 provider 在 pending-fatal 窗口不可达，R1.2；mock      *)
(* provider 之外的状态另有 Rust 证据）；processing-failure 退出与 panic 路径  *)
(* （session.rs:827-831/:856-866/:914-916——都在冻结六路径退出清单之外；两者  *)
(* 都发 worker_failure → 同一 failedDecode 格，模型内不可产生——审查 A-5）； *)
(* **新 continuation 自身的解码故障**    *)
(* （模型内新 continuation 无故障——因此 landing 之后的任何 worker failure    *)
(* 即旧 fatal，NoOldFatalAfterApplied 只在该抽象内是全称命题，如实声明）。    *)
(* 无 fairness / liveness：本门只攻击 safety 碰撞；不以任何 fairness 假设    *)
(* 代替真实同步事实。                                                        *)
(* 模型结论只在其显式 abstraction 与 assumptions 下成立；不得静默升级为       *)
(* architecture authority（AGENTS.md "Verification authority boundary"）。    *)
(*                                                                         *)
(* 负控制（Mutation 常量，逐个独立注入；M1–M7 为基线负控制，D-T1..D-T4 为     *)
(* #289 冻结 defect ledger（PENDING_FATAL_DAG §8）的实现）：                  *)
(*   MutationCommitBeforeTailPurge   commit 不等设备尾排空（M1）              *)
(*   MutationSeekMidWrite            song_seek 容忍 worker 在 in-flight 写    *)
(*                                   半路上执行且 staging 未丢弃（M2）        *)
(*   MutationParkWhileHeld           park 允许持有已拉取的旧块（M3）          *)
(*   MutationStalePositionWriter     commit 后仍按旧 basis 发布位置（M4）     *)
(*   MutationCommitBeforeLanding     decoder 未 reposition 就 commit（M5）    *)
(*   MutationRefusalDropsRemainder   refusal 丢弃保留余量（M6）               *)
(*   MutationResumeAfterMutatedSeek  destructive failure 后恢复旧游标生产     *)
(*                                   （M7）                                   *)
(*   MutationOldCursorResumeAfterApplied  D-T1：Applied 后旧 cursor 生产      *)
(*   MutationRefusedClearsLatch           D-T2：Refused 被误当 Applied 清 latch*)
(*   MutationLoopTopStopExit              D-T3：canceled seek 位置伪造 stop 退出*)
(*   MutationAppliedKeepsLatch            D-T4：Applied 不擦除 latch（单缺陷， *)
(*                                        终审 P2）→ 新 continuation 下一次读  *)
(*                                        复现旧 fatal → 非法 failure 发布     *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS MAX_TAIL, Mutation, MutationNone,
    MutationCommitBeforeTailPurge,
    MutationSeekMidWrite,
    MutationParkWhileHeld,
    MutationStalePositionWriter,
    MutationCommitBeforeLanding,
    MutationRefusalDropsRemainder,
    MutationResumeAfterMutatedSeek,
    MutationOldCursorResumeAfterApplied,
    MutationRefusedClearsLatch,
    MutationLoopTopStopExit,
    MutationAppliedKeepsLatch

MutationChoices == {MutationNone,
                    MutationCommitBeforeTailPurge,
                    MutationSeekMidWrite,
                    MutationParkWhileHeld,
                    MutationStalePositionWriter,
                    MutationCommitBeforeLanding,
                    MutationRefusalDropsRemainder,
                    MutationResumeAfterMutatedSeek,
                    MutationOldCursorResumeAfterApplied,
                    MutationRefusedClearsLatch,
                    MutationLoopTopStopExit,
                    MutationAppliedKeepsLatch}

ASSUME Mutation \in MutationChoices

VARIABLES
    \* —— 两条腿的位置（协议串行化点的抽象）——
    worker,           \* "idle" | "writing_old" | "writing_partial"
                      \* | "seek_eligible" | "seeking" | "repositioned"
                      \* | "decode_failed"
    parked,           \* render leg 停在 loop-top gate（不持有设备缓冲）
    \* —— stale-PCM 三库（D14.5 reservoir accounting 的抽象）——
    edgeOld,          \* edge 队列可能还有旧 PCM
    heldOld,          \* render leg 手里拿着一个已拉取的旧 chunk
    tailOld,          \* 设备尾里排队的旧 chunk 数（0..MAX_TAIL）
    tailNew,          \* 设备尾里排队的新 chunk 数（0..MAX_TAIL）
    \* —— seek 协议状态 ——
    cutRequested,     \* seek command 在飞（command，非 fact；单飞语义）
    landing,          \* worker 已发布 landing 证据（成功 seek 的 purge 之后）
    committed,        \* session 已 commit cutover（F5 域；≠ D11 terminal）
    seekFailed,       \* 最近一次 seek 被「证毕 pre-mutation 拒绝」
                      \* （RefusedUnchanged；单调 TRUE = refusal 曾发生）
    \* —— D-C 扩展：pending-fatal latch 与 stop/terminal 真实边界 ——
    pendingFatal,     \* SongCore handle 的 fatal_error 位（已 latch、未 surface）
    stopIntent,       \* request_stop 已记录 intent（completion lock 步；
                      \*   与 edge stop 之间存在真实窗口）
    edgeTerminal,     \* PcmEdge terminal："open"|"eof"|"failed"|"stopped"
                      \*   （first-wins；edge.rs:190-201）
    workerFailurePublished, \* worker_failure record 已发布（completion.rs
                      \*   WorkerFailure；decode/processing 同一记录）
    workerGone,       \* worker 已退出（worker_exited 发布 worker_gone）
    exitPath,         \* ghost：worker 退出路径（冻结退出清单的成员资格是
                      \*   TerminalCellConformance 的承重半边）
                      \*   "none" | "surfacedFatal" | "mutated" | "writeStopped"
                      \*   | "remainderBreak" | "postCutPlaneEnded" | "eofClose"
                      \*   | "loopTopStopExit"（仅 D-T3 注入）
    drainVerdict,     \* render leg 的 drain 证据："none"|"drained"|"aborted"
    committedOutcome, \* D11 terminal 的最小受约束投影：
                      \*   "none"|"completed"|"stopped"|"failedDecode"|"failedDevice"
    \* —— verifier-only 见证变量（ghost，非 production representation）——
    oldDrainEver,     \* 是否发生过旧 PCM 的 audible drain（合法，pre-commit）
    oldAfterCommit,   \* 不变式靶：commit 之后旧 PCM 被 drain
    posMixAfterCommit, \* 不变式靶：commit 之后按旧 basis 发布位置
    seekFromPartial,  \* 在飞的 seek 从 writing_partial 进入（余量在外）
    remainderLost,    \* 不变式靶：refusal 丢掉了保留余量（M6 靶）
    producedAfterFailure, \* 不变式靶：destructive failure 后恢复生产（M7 靶）
    oldCursorResumed, \* 不变式靶：Applied 之后旧 cursor 生产复活（D-T1 靶）
    fatalLatchedEver, \* 见证：pending fatal 曾被 latch（W_APPLIED_ERASURE_
                      \*   COMPLETED 的前半条件：latch→擦除→Completed）
    refusedWhileLatched \* 见证：latch 已存在的拒绝之后 latch 幸存（仅当
                      \*   拒绝前后都 latch 才置位；W_REFUSED_FATAL_RETAINED
                      \*   的精确形状——防「先拒绝后 latch」等深路径与
                      \*   「擦除后重 latch」两种形状失明）

vars == <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
          cutRequested, landing, committed, seekFailed, stopIntent,
          edgeTerminal, pendingFatal, workerFailurePublished, workerGone,
          exitPath, drainVerdict, committedOutcome,
          seekFromPartial, remainderLost, producedAfterFailure,
          oldDrainEver, oldAfterCommit, posMixAfterCommit,
          oldCursorResumed, fatalLatchedEver, refusedWhileLatched>>

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
    /\ stopIntent = FALSE
    /\ edgeTerminal = "open"
    /\ pendingFatal = FALSE
    /\ workerFailurePublished = FALSE
    /\ workerGone = FALSE
    /\ exitPath = "none"
    /\ drainVerdict = "none"
    /\ committedOutcome = "none"
    /\ seekFromPartial = FALSE
    /\ remainderLost = FALSE
    /\ producedAfterFailure = FALSE
    /\ oldDrainEver = FALSE
    /\ oldAfterCommit = FALSE
    /\ posMixAfterCommit = FALSE
    /\ oldCursorResumed = FALSE
    /\ fatalLatchedEver = FALSE
    /\ refusedWhileLatched = FALSE

-----------------------------------------------------------------------------
(********************************* 动作 *************************************)

\* —— decode leg（生产者）——

\* loop-top 之后取一个旧 chunk 解码进 staging。guard ¬pendingFatal 正是
\* 「latch 存在时下一次读必复现」（C 入口先于一切出帧）——surface 分支在
\* FatalFailurePublish。读不检查 edge terminal（生产在 loop-top 与 read 之间
\* 无任何 terminal 检查），故本动作不受 stopIntent/edgeTerminal 门控。
WriteOld ==
    /\ worker = "idle"
    /\ pendingFatal = FALSE
    /\ ~workerFailurePublished
    /\ ~workerGone
    /\ worker' = "writing_old"
    /\ oldCursorResumed' = (oldCursorResumed \/ landing)
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   fatalLatchedEver,
                   refusedWhileLatched>>

\* song_read_pcm 的 partial-success 出口（C:1136-1156）：出块成功、
\* fatal 被 latch。块照常走写路径；latch 由下一次读复现。
ReadPartialSuccess ==
    /\ worker = "idle"
    /\ pendingFatal = FALSE
    /\ ~workerFailurePublished
    /\ ~workerGone
    /\ worker' = "writing_old"
    /\ pendingFatal' = TRUE
    /\ fatalLatchedEver' = TRUE
    /\ oldCursorResumed' = (oldCursorResumed \/ landing)
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   refusedWhileLatched>>

\* bounded-slice 写的前缀落队：余量保留在 staging——由 seek 结果决定补完
\* （refusal）或随 cut 丢弃（成功）。前缀能落队要求 edge 仍 Open。
WritePartial ==
    /\ worker = "writing_old"
    /\ edgeTerminal = "open"
    /\ ~workerFailurePublished
    /\ ~workerGone
    /\ worker' = "writing_partial"
    /\ edgeOld' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* staging 写 edge 完成（writing_old：整块一次落队；writing_partial：
\* 保留余量补完——refusal 的零内容损失义务）。要求 edge Open：terminal 下
\* 零进展写走向 WriteStoppedExit / RemainderFlushStopExit。
FinishWriteOld ==
    /\ worker \in {"writing_old", "writing_partial"}
    /\ edgeTerminal = "open"
    /\ ~workerFailurePublished
    /\ ~workerGone
    /\ worker' = "idle"
    /\ edgeOld' = TRUE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 真实 stop-exit (a)：容量等待中零进展 + terminal≠Open（session.rs
\* :993-994 → :884）。只从 in-flight 整块写（writing_old）可达。
WriteStoppedExit ==
    /\ worker = "writing_old"
    /\ edgeTerminal = "stopped"
    /\ ~workerGone
    /\ workerGone' = TRUE
    /\ exitPath' = "writeStopped"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 真实 stop-exit (b)：保留余量 flush 的 terminal break（session.rs
\* :798-802 → :805-809）。latch（若在）未被 surface 即随退出湮灭——与生产
\* 一致（Refused 从未清除它，退出也不 surface 它）。
RemainderFlushStopExit ==
    /\ worker = "writing_partial"
    /\ edgeTerminal = "stopped"
    /\ ~workerGone
    /\ workerGone' = TRUE
    /\ exitPath' = "remainderBreak"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* pending-fatal surfacing（1/3）——错误发布：下一次读在 C 入口复现 latch
\* （C:1111-1113）→ adapter Err → decode_failed（session.rs:896-900）。
\* **不受 stopIntent/edgeTerminal 门控**——R1.1 的核心事实：default position
\* 上 pending fatal 胜过 stop。publication + settlement 同一 publish_evidence
\* 锁程（completion.rs:1319-1331），故二者原子；**edge.fail 与退出漏斗不在
\* 该锁程**（同一 Err 臂的后续独立语句 + :928 漏斗，终审 P1 更正——不得以
\* 锁程理由融合）。exitPath 是路由 ghost：Err 臂必然 return，路由在此确定。
\* 新 continuation 的 surfacing（D-T4 诊断形状）从 "repositioned" 同样可达：
\* 入口复现先于一切且不区分位置；但生产 hold 期间无读（:719-774 等待），故
\* repositioned 臂要求已 commit（恢复生产在 commit 路由之后，:775-778）。
FatalFailurePublish ==
    /\ worker \in {"idle", "repositioned"}
    /\ (worker = "idle" \/ committed)
    /\ pendingFatal = TRUE
    /\ ~workerFailurePublished
    /\ ~workerGone
    /\ worker' = "decode_failed"
    /\ workerFailurePublished' = TRUE
    /\ exitPath' = "surfacedFatal"
    /\ committedOutcome' = "failedDecode"
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal,
                   workerGone, drainVerdict,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* pending-fatal surfacing（2/3）——edge 终态推进：Err 臂的 edge.fail()
\* （session.rs:898；MutatedThenFailed 臂的 :675 同形）。**publish_evidence
\* 锁程之外**（锁已释放）→ 与 request_stop 的 edge.stop()（W-A 窗口）及
\* intent 记录真实可交错（终审 P1 的合法序列：settlement 先提交 Failed，
\* stop 的 edge.stop 抢先置 Stopped，本动作 first-wins 不覆盖）。
\* first-wins：已提交的 EOF/Stopped/Failed 不被改写（edge.rs:190-201）。
WorkerEdgeFailAdvance ==
    /\ worker = "decode_failed"
    /\ ~workerGone
    /\ edgeTerminal' = IF edgeTerminal = "open" THEN "failed" ELSE edgeTerminal
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* pending-fatal surfacing（3/3）——退出漏斗：worker_exited(edge.terminal())
\* （session.rs:928 → completion.rs:1062-1067；publication + settlement 同一
\* 锁程）。**publish_evidence first-wins**（completion.rs:1322-1324：outcome
\* 已 Some 即返回 false）：失败路线的 settlement 已在发布步发生，漏斗的
\* worker_gone / worker_terminal 证据被丢弃——workerGone 保持 FALSE 是忠实
\* 值，不得把「物理即将退出」编码为「worker_gone 已发布」（终审 P1）。
\* 条件形式忠实转写 publish_evidence 的门槛；本模型内 TRUE 臂不可达
\* （失败路线的发布步总是先 settle），与 Failed{device} 格同类的转写保留。
\* worker' 停留 decode_failed；abort_stranded_seek 的 slot 释放是模型外
\* bookkeeping（失败路线 cutRequested 保持原值，与既有抽象一致）。
WorkerExitFunnel ==
    /\ worker = "decode_failed"
    /\ workerGone' = (committedOutcome = "none")
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* —— render leg（消费者）——

\* D14.7 gate：loop-top park。承重护栏 = 不持有已拉取的块跨 park。M3 攻击它。
ParkLeg ==
    /\ parked = FALSE
    /\ (heldOld = FALSE \/ Mutation = MutationParkWhileHeld)
    /\ edgeTerminal = "open"
    /\ ~workerFailurePublished
    /\ drainVerdict = "none"
    /\ parked' = TRUE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

UnparkLeg ==
    /\ parked = TRUE
    /\ drainVerdict = "none"
    /\ parked' = FALSE
    /\ UNCHANGED <<worker, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 从 edge 拉一个旧块进手（GetBuffer..read 之后的窗口）。
PullOld ==
    /\ parked = FALSE
    /\ edgeOld = TRUE
    /\ heldOld = FALSE
    /\ edgeTerminal = "open"
    /\ ~workerFailurePublished
    /\ drainVerdict = "none"
    /\ heldOld' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* ReleaseBuffer 提交手里的块。
SubmitHeld ==
    /\ parked = FALSE
    /\ heldOld = TRUE
    /\ tailOld < MAX_TAIL
    /\ edgeTerminal = "open"
    /\ ~workerFailurePublished
    /\ drainVerdict = "none"
    /\ heldOld' = FALSE
    /\ tailOld' = tailOld + 1
    /\ UNCHANGED <<worker, parked, edgeOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* post-commit 生产/提交（协议：worker 在 landing 与 release 之间持有
\* 产量，所以 SubmitNew 只在 committed 之后存在——rebase basis 因此精确）。
SubmitNew ==
    /\ parked = FALSE
    /\ worker = "repositioned"
    /\ committed = TRUE
    /\ tailNew < MAX_TAIL
    /\ edgeTerminal = "open"
    /\ ~workerFailurePublished
    /\ ~workerGone
    /\ drainVerdict = "none"
    /\ tailNew' = tailNew + 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* —— 设备（audible truth 的抽象：drain 即成为 audible output）——
\* （DrainOld 的 open 门槛对应 render 侧「terminal 先于数据」：failed 边
\* 上的排空读立即放弃（edge.rs:237-239），已入设备尾的旧 chunk 随数据面
\* 终止被放弃——2026-09-18 基线注释里的「失败后仍自然播出」在本模型中
\* 被本门槛取代；无不变式/探针依赖失败后的 drain（WitnessOldDrainPreCommit
\* 在 pre-commit 窗口内保持可达）。）

DrainOld ==
    /\ tailOld > 0
    /\ edgeTerminal = "open"
    /\ tailOld' = tailOld - 1
    /\ oldDrainEver' = TRUE
    /\ oldAfterCommit' = (oldAfterCommit \/ committed)
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   posMixAfterCommit, oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

DrainNew ==
    /\ tailNew > 0
    /\ edgeTerminal \in {"open", "eof"}
    /\ tailNew' = tailNew - 1
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* —— render leg 的终态观察（drain 证据）——

\* terminal 检查先于数据（edge.rs:237-239）：stopped/failed 边上的 pull
\* 立即放弃缓冲退出。heldOld 可能仍 TRUE（ReleaseBuffer 永不发生）。
LegObserveAborted ==
    /\ edgeTerminal \in {"stopped", "failed"}
    /\ drainVerdict = "none"
    /\ drainVerdict' = "aborted"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* EOF 排干后的观察：全部已消费、手中无块 → 下一次 read 看到 Eof。
LegObserveDrained ==
    /\ edgeTerminal = "eof"
    /\ tailOld = 0
    /\ tailNew = 0
    /\ heldOld = FALSE
    /\ drainVerdict = "none"
    /\ drainVerdict' = "drained"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* —— 协议（session / worker 的协调动作）——

\* seek command（Command，非 fact；单飞语义）。生产侧 request_seek 采样
\* edge Open 并在 completion 锁下复核；terminal 边不接受新 seek。
RequestSeek ==
    /\ cutRequested = FALSE
    /\ ~stopIntent
    /\ committedOutcome = "none"
    /\ edgeTerminal = "open"
    /\ cutRequested' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* worker 串行化点上的 song_seek eligibility（BEFORE 一切 invalidation，
\* frozen order）。三查（¬aborted ≈ ¬stopIntent ∧ unsettled；parked 证据的
\* 承重位置在 CommitCut/WorkerCut 的纪律里，与基线一致；edge Open 复核）
\* 作为一步原子 guard——通过即消费 seek_aborted=false 的裁决（生产 :620-649
\* 的检查时序在此之后不再有取消分支）。**不排除 pending-fatal**
\* （reconciliation §2.2）。通过 → provider begin 之间是真实窗口（终审 P2
\* 拆分表达，替代原 A-2 的并集论证：intent / edge stop / settlement 均可
\* 落入；M2 注入点：从 in-flight 写进入）。
SeekEligibilityPass ==
    /\ cutRequested = TRUE
    /\ (worker \in {"idle", "writing_partial"}
        \/ (Mutation = MutationSeekMidWrite /\ worker = "writing_old"))
    /\ ~stopIntent
    /\ committedOutcome = "none"
    /\ edgeTerminal = "open"
    /\ ~workerGone
    /\ worker' = "seek_eligible"
    /\ seekFromPartial' = (worker = "writing_partial")
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   stopIntent, edgeTerminal, pendingFatal,
                   workerFailurePublished, workerGone, exitPath,
                   drainVerdict, committedOutcome,
                   remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* provider begin：eligibility 已过后的 decode_stream.seek（session.rs:650）。
\* **无 stopIntent / edgeTerminal 复核**——生产在该检查点之后不再重查
\* （终审 P2 的真实序列：check 通过 → intent 落入 → edge 仍 Open → 调用
\* 照常发出）。调用已发出，返回是 program order（Q6-v）。
SeekProviderBegin ==
    /\ worker = "seek_eligible"
    /\ ~workerGone
    /\ worker' = "seeking"
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   stopIntent, edgeTerminal, pendingFatal,
                   workerFailurePublished, workerGone, exitPath,
                   drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* song_seek 的「证毕 pre-mutation 拒绝」（INVALID_ARGUMENT 类）：什么都没
\* 有被 invalidation，余量原样归还补完，命令被消费。**pendingFatal UNCHANGED
\* 是本扩展的承重点之一**（C:1166 只覆盖诊断；RetainLatchOnRefused / D-T2）。
\* 返回不受 stopIntent 门控：调用已发出，返回是 program order（Q6-v）。
SeekRefusedUnchanged ==
    /\ worker = "seeking"
    /\ ~workerGone
    /\ worker' = IF seekFromPartial /\ Mutation # MutationRefusalDropsRemainder
                 THEN "writing_partial"
                 ELSE "idle"
    /\ remainderLost' = (seekFromPartial /\ Mutation = MutationRefusalDropsRemainder)
    /\ seekFromPartial' = FALSE
    /\ cutRequested' = FALSE
    /\ seekFailed' = TRUE
    /\ pendingFatal' = IF Mutation = MutationRefusedClearsLatch
                       THEN FALSE ELSE pendingFatal
    /\ refusedWhileLatched' = (refusedWhileLatched
                               \/ (pendingFatal /\ pendingFatal'))
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, stopIntent,
                   edgeTerminal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver>>

\* song_seek 的「非证毕 pre-mutation 失败」（generic SEEK_ERROR 等）：旧
\* decoder 延续不再有保证 → fail-closed：release_seek_without_commit +
\* worker_failure("seek failed")（session.rs:663-675）。publication+settlement
\* 同一锁程 → 与提交原子（Q6-iii）。edge.fail 与退出漏斗在该锁程之外，
\* 由 WorkerEdgeFailAdvance / WorkerExitFunnel 承载（终审 P1 拆分）。
\* 返回不受 stopIntent 门控（Q6-v）。绝不伪装成 refusal 恢复旧播放——
\* M7 注入的正是那种恢复。
SeekMutatedThenFailed ==
    /\ worker = "seeking"
    /\ ~workerGone
    /\ worker' = "decode_failed"
    /\ workerFailurePublished' = TRUE
    /\ exitPath' = "mutated"
    /\ committedOutcome' = "failedDecode"
    /\ seekFromPartial' = FALSE
    /\ cutRequested' = FALSE
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal,
                   workerGone, drainVerdict,
                   remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* song_seek 成功（Applied）之后 worker 自行的 purge：staging/余量丢弃 +
\* edge 清空 + landing 证据 + **latch 擦除**（C:1195-1196 reset_decode_state
\* ——擦除只存在于成功路径）+ 持有产量。此后该线程按 program order 等待
\* commit 路由。返回不受 stopIntent 门控（Q6-v：stop 可落在 FFI 调用中途，
\* 调用照常返回 Applied）。M2 注入点：worker 回到 writing_old（in-flight 写
\* 复活）；D-T1 注入点：worker 回到 idle（旧 cursor 生产复活）；D-T4 注入
\* 点：**仅 latch 不被擦除**（单缺陷，2026-10-17 终审 P2 更正——去向保持
\* 正常 repositioned，旧 fatal 由新 continuation 的下一次读经
\* FatalFailurePublish 的 repositioned 臂复现，surfacing 全程链入反例）。
WorkerCut ==
    /\ worker = "seeking"
    /\ ~workerGone
    /\ worker' = CASE Mutation = MutationSeekMidWrite -> "writing_old"
                 [] Mutation = MutationOldCursorResumeAfterApplied -> "idle"
                 [] OTHER -> "repositioned"
    /\ edgeOld' = FALSE
    /\ landing' = TRUE
    /\ pendingFatal' = IF Mutation = MutationAppliedKeepsLatch
                       THEN pendingFatal ELSE FALSE
    /\ seekFromPartial' = FALSE
    /\ UNCHANGED <<parked, heldOld, tailOld, tailNew,
                   cutRequested, committed, seekFailed, stopIntent,
                   edgeTerminal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 真实 stop-exit (c)：Applied 等待/恢复路径上数据面在 cut 之下终止
\* （session.rs:725-735/:753-771 的 plane-terminal 退出；landing 已发布，
\* 无 worker_failure，无 rebase 义务）。stop 已发生（唯一边 stop 源）。
PostCutStopExit ==
    /\ worker = "repositioned"
    /\ landing = TRUE
    /\ edgeTerminal = "stopped"
    /\ ~workerGone
    /\ workerGone' = TRUE
    /\ exitPath' = "postCutPlaneEnded"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 新 continuation 的 EOF 臂：close_eof + return（session.rs:892-895）。
\* 模型内只从已 commit 的新 continuation 启用（生产的两个等待循环先于
\* 恢复生产解析；stop intent 下等待以 plane-terminal 退出取代）。审查
\* A-3 范围注记：生产上该 EOF 臂也在无 seek 播放到达（不在本模型窗口内）
\* ——TCC(2d) 的 landing 合取是模型作用域选择，不是生产必要性。first-
\* wins：EOF 已提交后 edge.stop 是 no-op（EdgeStopFromStop 的 open 门槛）。
EofClose ==
    /\ worker = "repositioned"
    /\ landing = TRUE
    /\ committed = TRUE
    /\ edgeTerminal = "open"
    /\ ~workerGone
    /\ edgeTerminal' = "eof"
    /\ workerGone' = TRUE
    /\ exitPath' = "eofClose"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   pendingFatal, workerFailurePublished,
                   drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* request_stop 的 completion-lock 步（Q6-i 融合：intent + pause/seek-hold
\* 释放同一锁程，completion.rs:552-576）。**不设 committedOutcome 门槛**：
\* 生产 request_stop「safe from any thread and any state」——settle 之后
\* intent 照常记录、gate 照常释放、锁程外的 edge.stop() 照常执行（「after
\* settlement: nothing changes」指终局不被改写，不是动作不发生；终审 P1
\* 的合法交错正依赖此）。迟到 stop 的稳定性归 episode-terminal-settlement
\* 域（D11 first-wins）。
RecordStopIntent ==
    /\ ~stopIntent
    /\ stopIntent' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 同一 request 的数据面 stop——锁程释放后的另一步（completion.rs
\* :577-587；Q6-iv 禁止与 RecordStopIntent 融合）。first-wins：EOF 已提交
\* 则 no-op（edge.rs:190-201）。
EdgeStopFromStop ==
    /\ stopIntent = TRUE
    /\ edgeTerminal = "open"
    /\ edgeTerminal' = "stopped"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* 串行化点上的 seek_aborted 分支（session.rs:620-625）：
\* release_seek_without_commit + pending=None——**bookkeeping，不是退出**；
\* 控制流落入 remainder flush / DSP pickup / read（R1.2 更正）。
\* D-T3 注入的正是「把这个位置改成 worker 退出」——pre-R1.1 的错误措辞。
CancelSeekBookkeeping ==
    /\ cutRequested = TRUE
    /\ stopIntent = TRUE
    /\ worker \in {"idle", "writing_partial"}
    /\ ~workerGone
    /\ cutRequested' = FALSE
    /\ workerGone' = (Mutation = MutationLoopTopStopExit)
    /\ exitPath' = IF Mutation = MutationLoopTopStopExit
                   THEN "loopTopStopExit" ELSE exitPath
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* session 的 cutover commit（D14.8 rebase 的触发点；**F5 域的 CommitCut，
\* 不是 D11 TerminalCommit**）。承重护栏 = landing ∧ edge 干净 ∧ 尾排空 ∧
\* parked ∧ unsettled。M1 攻击尾排空，M5 攻击 landing。
CommitCut ==
    /\ cutRequested = TRUE
    /\ (landing = TRUE \/ Mutation = MutationCommitBeforeLanding)
    /\ edgeOld = FALSE
    /\ (tailOld = 0 \/ Mutation = MutationCommitBeforeTailPurge)
    /\ parked = TRUE
    /\ committed = FALSE
    /\ ~stopIntent
    /\ committedOutcome = "none"
    /\ ~workerGone
    /\ committed' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* —— D11 terminal 的最小受约束投影（TERMINAL_DECISION_MAP）——
\* 冻结 resolver（completion.rs:1337-1398）的转写：worker_failure 先于
\* 一切 stop-intent 咨询；Drained+Eof→Completed（不咨询 stop_intent）；
\* Aborted+Stopped+stop_intent→Stopped；其余 Aborted 格→Failed{device}
\* （模型内不可达：唯一边 stop 源是 stop intent；分支保留以忠实转写）。
\* 证据未齐（无 worker_failure 且无 drain verdict）则不 commit（生产侧
\* Aborted+worker_terminal=None → 等待）。
TerminalCommit ==
    /\ workerGone = TRUE
    /\ committedOutcome = "none"
    /\ (workerFailurePublished \/ drainVerdict # "none")
    /\ committedOutcome' =
         IF (workerFailurePublished \/ edgeTerminal = "failed")
         THEN "failedDecode"
         ELSE IF drainVerdict = "drained" /\ edgeTerminal = "eof"
         THEN "completed"
         ELSE IF drainVerdict = "aborted" /\ edgeTerminal = "stopped" /\ stopIntent
         THEN "stopped"
         ELSE "failedDevice"
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* —— 位置发布（D14.8 seek 规则：publication 不得混合 pre/post basis）——
\* （与基线相同：这两个发布动作是惰性的——位置不变式的 base 保证来自
\* 「单一 writer 在自身路径上 rebase」这一构造性事实，M4 只证明注入缺陷
\* 确实可达并可检出。）

PublishPositionOld ==
    /\ committed = FALSE
    /\ ~stopIntent
    /\ ~workerFailurePublished
    /\ UNCHANGED vars

PublishPositionNew ==
    /\ committed = TRUE
    /\ ~stopIntent
    /\ UNCHANGED vars

\* M4 注入：commit 之后仍按旧 basis 发布（stale writer 复用）。
StalePositionPublish ==
    /\ Mutation = MutationStalePositionWriter
    /\ committed = TRUE
    /\ ~stopIntent
    /\ posMixAfterCommit' = TRUE
    /\ UNCHANGED <<worker, parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost, producedAfterFailure,
                   oldDrainEver, oldAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

\* M7 注入：destructive failure 之后恢复旧游标生产——旧库里的旧 PCM
\* 加上重定位后的新内容，无 cutover、无 rebase，直接混合。反例由
\* producedAfterFailure 见证位本身触发（与基线一致）。
ResumeAfterFailure ==
    /\ Mutation = MutationResumeAfterMutatedSeek
    /\ workerFailurePublished = TRUE
    /\ worker = "decode_failed"
    /\ ~stopIntent
    /\ worker' = "writing_old"
    /\ producedAfterFailure' = TRUE
    /\ UNCHANGED <<parked, edgeOld, heldOld, tailOld, tailNew,
                   cutRequested, landing, committed, seekFailed, stopIntent,
                   edgeTerminal, pendingFatal, workerFailurePublished,
                   workerGone, exitPath, drainVerdict, committedOutcome,
                   seekFromPartial, remainderLost,
                   oldDrainEver, oldAfterCommit, posMixAfterCommit,
                   oldCursorResumed, fatalLatchedEver,
                   refusedWhileLatched>>

Next ==
    \/ WriteOld
    \/ ReadPartialSuccess
    \/ WritePartial
    \/ FinishWriteOld
    \/ WriteStoppedExit
    \/ RemainderFlushStopExit
    \/ FatalFailurePublish
    \/ WorkerEdgeFailAdvance
    \/ WorkerExitFunnel
    \/ ParkLeg
    \/ UnparkLeg
    \/ PullOld
    \/ SubmitHeld
    \/ SubmitNew
    \/ DrainOld
    \/ DrainNew
    \/ LegObserveAborted
    \/ LegObserveDrained
    \/ RequestSeek
    \/ SeekEligibilityPass
    \/ SeekProviderBegin
    \/ SeekRefusedUnchanged
    \/ SeekMutatedThenFailed
    \/ WorkerCut
    \/ PostCutStopExit
    \/ EofClose
    \/ RecordStopIntent
    \/ EdgeStopFromStop
    \/ CancelSeekBookkeeping
    \/ CommitCut
    \/ TerminalCommit
    \/ PublishPositionOld
    \/ PublishPositionNew
    \/ StalePositionPublish
    \/ ResumeAfterFailure

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(***************************** 安全不变式 ***********************************)

TypeOK ==
    /\ worker \in {"idle", "writing_old", "writing_partial", "seek_eligible",
                   "seeking", "repositioned", "decode_failed"}
    /\ parked \in BOOLEAN
    /\ edgeOld \in BOOLEAN
    /\ heldOld \in BOOLEAN
    /\ tailOld \in 0..MAX_TAIL
    /\ tailNew \in 0..MAX_TAIL
    /\ cutRequested \in BOOLEAN
    /\ landing \in BOOLEAN
    /\ committed \in BOOLEAN
    /\ seekFailed \in BOOLEAN
    /\ stopIntent \in BOOLEAN
    /\ edgeTerminal \in {"open", "eof", "failed", "stopped"}
    /\ pendingFatal \in BOOLEAN
    /\ workerFailurePublished \in BOOLEAN
    /\ workerGone \in BOOLEAN
    /\ exitPath \in {"none", "surfacedFatal", "mutated", "writeStopped",
                     "remainderBreak", "postCutPlaneEnded", "eofClose",
                     "loopTopStopExit"}
    /\ drainVerdict \in {"none", "drained", "aborted"}
    /\ committedOutcome \in {"none", "completed", "stopped",
                             "failedDecode", "failedDevice"}
    /\ seekFromPartial \in BOOLEAN
    /\ remainderLost \in BOOLEAN
    /\ producedAfterFailure \in BOOLEAN
    /\ oldDrainEver \in BOOLEAN
    /\ oldAfterCommit \in BOOLEAN
    /\ posMixAfterCommit \in BOOLEAN
    /\ oldCursorResumed \in BOOLEAN
    /\ fatalLatchedEver \in BOOLEAN
    /\ refusedWhileLatched \in BOOLEAN

\* F5 根不变式（D14.5）：commit 之后旧 PCM 不得再成为 output。
InvStaleOutput == oldAfterCommit = FALSE

\* D14.8 seek 规则：publication 不得混合 pre/post-cutover basis。
InvPositionNoMixing == posMixAfterCommit = FALSE

\* commit 时刻状态（护栏的自检：commit 蕴含 edge 干净 + 尾排空 + landing；
\* 这是不变式化了的 commit 前置，不是新增机制）。
InvCommitPurged ==
    committed = FALSE \/ (landing = TRUE /\ edgeOld = FALSE /\ tailOld = 0)

\* 拒绝零内容损失（corrective 1）：RefusedUnchanged 不得丢掉保留余量。
\* M6 攻击它。
InvRefusalContentContinuous == remainderLost = FALSE

\* 失败路线封闭（corrective 1）：MutatedThenFailed 之后不得恢复生产。
\* M7 攻击它。
InvFailClosed == producedAfterFailure = FALSE

\* —— D-C 扩展的新安全性质（#289；语义锚点 reconciliation §2.3/§2.3.1）——

\* D-T1 靶：Applied（landing）之后不得从旧 cursor 恢复生产——新 continuation
\* 只能来自 provider 的新 cursor（session.rs:676-779 的恢复点纪律）。
NoOldCursorResumeAfterApplied == ~oldCursorResumed

\* D-T2 靶：RefusedUnchanged 不得清除已存在的 pending fatal——擦除只存在
\* 于 song_seek 的成功路径（C:1196）；失败路径只覆盖诊断（C:1166）。
\* fatalLatchedEver 承担「refusal 时点上 latch 已存在」的见证（refusal 本身
\* 不创造 latch：从未 latch 过的普通 refusal 合法，见 base 反例 attempt-1）。
\* seekFailed 单调，landing=FALSE 表示该拒绝之后没有发生过 Applied。
RetainLatchOnRefused ==
    (seekFailed /\ ~landing /\ fatalLatchedEver) => pendingFatal

\* D-T4 靶（模型内抽象下声明，见模块头「新 continuation 自身的解码故障」
\* 条）：landing 之后既不能残留 latch（Applied 擦除语义），也不能再发布
\* worker failure（模型内新 continuation 无故障，故 cut 后任何 worker
\* failure 即旧 fatal 落入新 continuation）。
NoOldFatalAfterApplied ==
    landing => (~pendingFatal /\ ~workerFailurePublished)

\* 诊断（diagnostic-only，非产品契约；AGENTS.md「Verification harnesses may
\* encode stronger diagnostic checks than production contracts only when
\* those checks are named as diagnostics」）：landing 之后不得发布 worker
\* failure——即 NoOldFatalAfterApplied 的 publication 半边。与全称版不同，
\* 本条不在 landing 时刻（stale latch 残留处）提前触发，从而 D-T4 的反例
\* 必须走完「Applied → latch 保留 → 新 continuation 下一次读 → 旧 fatal
\* surfaced → 非法 failure publication」全程链（终审 P2 验收形状）。
\* 基线 11 条不变式不含本条；仅 D-T4 诊断 cfg 引用；基模型中本条被
\* NoOldFatalAfterApplied 蕴含（擦除语义使 landing 后无 latch 可复现）。
DiagnosticNoFailurePublicationAfterLanding ==
    landing => ~workerFailurePublished

\* 作用域化安全（FORMAL-PLAN Q9 REV.2）：pre-Applied continuation 上存活的
\* latch 封死旧 continuation 的 EOF/Completed——C:1111 入口复现先于一切
\* EOF 分支。**不扩大**为「pendingFatalEver ⇒ 永不 Completed」：Applied 合法
\* 擦除后的新 continuation 可以正常 EOF/Completed，且该合法 witness 强制
\* 可达（W_APPLIED_ERASURE_COMPLETED）。
ScopedLatchedFatalNoComplete ==
    (pendingFatal /\ ~landing) => committedOutcome # "completed"

\* D-T3 靶：committed outcome 落在冻结矩阵的合法格内。两半：
\* (1) worker 退出路径必须属于冻结退出清单（reconciliation §2.3.1 R1.2
\*     重建的清单；"loopTopStopExit" 只存在于 D-T3 注入）；
\* (2) committed cell 与 (exitPath, workerFailure, stopIntent, edgeTerminal,
\*     drainVerdict) 的冻结关系一致（TERMINAL_DECISION_MAP 的转写）。
TerminalCellConformance ==
    \* (1) 冻结退出清单
    /\ (workerGone =>
          exitPath \in {"surfacedFatal", "mutated", "writeStopped",
                        "remainderBreak", "postCutPlaneEnded", "eofClose"})
    \* (2a) 失败退出格：publication+settlement 同一锁程 → 原子 failedDecode
    /\ (exitPath \in {"surfacedFatal", "mutated"} =>
          workerFailurePublished /\ committedOutcome = "failedDecode")
    \* (2b) 真实 stop-exit 格：无 worker_failure；只有 Stopped 可提交，
    \*       且需要 stop intent + aborted drain（rule 4）
    /\ (exitPath \in {"writeStopped", "remainderBreak", "postCutPlaneEnded"} =>
          ~workerFailurePublished
          /\ committedOutcome \in {"none", "stopped"})
    \* (2c) EOF 退出格：edge 已 eof；只有 Completed 可提交（rule 3）
    /\ (exitPath = "eofClose" =>
          edgeTerminal = "eof" /\ committedOutcome \in {"none", "completed"})
    \* (2d) Completed 格的必要条件（rule 3：Drained ∧ Eof；新 continuation）
    /\ (committedOutcome = "completed" =>
          exitPath = "eofClose" /\ landing /\ ~workerFailurePublished
          /\ edgeTerminal = "eof" /\ drainVerdict = "drained"
          /\ ~pendingFatal)
    \* (2e) Stopped 格的必要条件（rule 4：Aborted ∧ Stopped ∧ stop_intent；
    \*       worker_failure 先于 stop-intent 咨询，故失败时本格不可达）
    /\ (committedOutcome = "stopped" =>
          ~workerFailurePublished /\ stopIntent
          /\ edgeTerminal = "stopped" /\ drainVerdict = "aborted"
          /\ exitPath \in {"writeStopped", "remainderBreak",
                           "postCutPlaneEnded"})
    \* (2f) failedDecode 格需要 worker_failure（rule 1/2）
    /\ (committedOutcome = "failedDecode" => workerFailurePublished)
    \* (2g) failedDevice 格（rule 4-else / rule 6）需要无 worker_failure 且
    \*       数据面被非 intent 源停止——模型内不可达（无设备中断源），保留
    \*       以忠实转写冻结表
    /\ (committedOutcome = "failedDevice" =>
          ~workerFailurePublished /\ edgeTerminal = "stopped"
          /\ ~stopIntent)

-----------------------------------------------------------------------------
(************************** witness 探针（MUST-FAIL）************************)

\* 探针 1：cutover commit 必须可达（否则核心不变式空洞）。
ProbeNeverCommitted == committed = FALSE

\* 探针 2：commit 之前的旧 PCM audible drain 必须可达（旧输出在
\* pre-commit 窗口是合法的）。
ProbeNoOldDrainEver == oldDrainEver = FALSE

\* 探针 3：edge 队列的旧 PCM 在 pre-commit 窗口必须可达。
ProbeNoOldEdgeEver == edgeOld = FALSE

\* 探针 4：seek 拒绝路径必须可达。
ProbeNoRefusalEver == seekFailed = FALSE

\* 探针 5：destructive failure 路线必须可达（否则 InvFailClosed/M7 摆设）。
ProbeNoDestructiveEver == workerFailurePublished = FALSE

\* 探针 6：从 writing_partial 进入的 seek（余量在外）必须可达。
ProbeNoPartialSeekEver == seekFromPartial = FALSE

\* —— D-C 扩展的合法 witness（强制验收项；反约束探针）——

\* W_APPLIED_ERASURE_COMPLETED：partial success → latch → Applied 擦除 →
\* 新 continuation → EOF → Completed 必须可达。这是「Applied 合法吞没旧
\* fatal、新 continuation 正常完成」的反约束证明：若过强断言（例如把
\* 作用域化安全扩大成 pendingFatalEver ⇒ 永不 Completed）杀死了这条合法
\* 路径，本探针红。
ProbeNoAppliedErasureCompleted ==
    ~(fatalLatchedEver /\ committedOutcome = "completed")

\* W_REFUSED_FATAL_RETAINED：L2 精确形状——latch 已存在的拒绝之后
\* latch 幸存（未被 surface、未被清除）。refusedWhileLatched 是幸存语义
\* （仅当拒绝前 latch 在且拒绝后仍在才置位）：「先拒绝后 latch」的等深
\* 合法路径与「擦除后重新 latch」（2026-10-17 审查 C-F2a 反例：拒绝擦除
\* 缺陷下 ReadPartialSuccess 会重新置位 pendingFatal，单纯发生语义的
\* 见证位仍会触发）都不满足前件；若保留义务回归为擦除（D-T2 缺陷进入
\* 基模型），本 witness 不可达而变绿，套件即红。
ProbeNoRefusedRetention ==
    ~(refusedWhileLatched /\ pendingFatal)

\* W_STOPPED_FROM_REAL_EXIT：真实 stop-exit 上的 Stopped 终局必须可达
\* （否则 stop-race 半边的模型是空转的）。
ProbeNoStoppedEver == committedOutcome # "stopped"

\* W_FAILED_FROM_PENDING_FATAL：L5 精确形状——stop intent 已记录而
\* surfacing 仍发生（default position 上 fatal 胜 stop）。若回归把
\* surfacing 错误地门控在 ¬stopIntent 上，本 witness 将不可达而变绿
\* （MUST-FAIL 落空），套件即红。
ProbeNoFatalSurfacing ==
    ~(exitPath = "surfacedFatal" /\ stopIntent)

\* W_FAILED_DECODE_WITH_STOPPED_EDGE：终审 P1 的合法交错 witness——失败
\* settlement（publish_evidence 锁程内）之后、worker 的 edge.fail() 之前，
\* request_stop 完成数据面 stop（intent :553 → 锁程外 edge.stop :585-587；
\* settle 后 request_stop 仍照常执行，只是不改写终局）→
\* committedOutcome=failedDecode ∧ edgeTerminal=stopped 必须可达。
\* first-wins 两半各自承重：edge.fail 不覆盖已提交的 Stopped
\* （edge.rs:190-201），D11 不重分类已提交的 Failed（rule 1 先于一切
\* stop-intent 咨询）。拆分前模型无法表达该交错（edge.fail 与 settlement
\* 被错误融合），本探针在旧形状下不可达。
ProbeNoFailedDecodeStoppedEdge ==
    ~(committedOutcome = "failedDecode" /\ edgeTerminal = "stopped")

\* W_MUTATED_THEN_FAILED：MutatedThenFailed 的 fail-closed 退出必须可达。
ProbeNoMutatedExit == exitPath # "mutated"

=============================================================================
