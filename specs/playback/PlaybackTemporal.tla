(*
 * PlaybackTemporal — historical experimental playback model
 *                     （播放时间轴语义的形式化模型）
 *
 * STATUS:
 *   EXPERIMENTAL / FORMAL EVIDENCE ONLY
 *
 *   本模型保存早期 Playback architecture 实验的 failure witnesses 与
 *   verification techniques。其 MusicKernel / TransportKernel / Window /
 *   Generation / Fence 词汇不是当前 Playback architecture authority，
 *   不约束新的 production 设计，除非该语义被独立重新挣得。
 *   当前 Playback Foundations 提案：docs/adr/ADR-PBK-001.md（PROPOSED / REOPENED）。
 *
 * 被审计对象（历史）：旧版 docs/adr/ADR-PBK-001.md（Playback Architecture v1）
 * 中的 temporal 语义。
 *
 * 本模型只描述 temporal truth，不模拟 PCM sample、ring buffer、WASAPI 细节：
 *   - Window（ActiveWindow / PreparedWindow）
 *   - Generation admission（window-scoped 时间身份，不是全局 current_generation）
 *   - seek / next / stop 的 prepare -> prime -> close admission -> fence -> promote -> retire 骨架
 *   - Physical Fence（close admission 之后的物理冲刷握手，带成功/失败 verdict）
 *   - submitted / queued / rendered 记账（submitted != rendered）
 *   - admission 关闭时 decoded-but-never-submitted backlog 的显式 discard
 *     （discardBacklog 记账：否则 fail/abandon 后 drain predicate 永久悬空）
 *   - Decoder EOF / transport drained / ENDED 的层次
 *   - 迟到 decode result 的拒绝
 *   - rapid command supersede（seek/seek/next/stop 与 decode/fence/render 的交错）
 *   - Decoder provider withdrawal 的轻量交互（完整 withdrawal 顺序在 PlaybackOwnership 模型）
 *
 * 角色（旧版 ADR v1 的模型内分工；本模型整体为 experimental evidence）：
 *   MusicKernel     = music-domain semantic authority（intent 解释、ENDED 产品语义）
 *   TransportKernel = playback temporal authority（cursor、window role、admission、
 *                     fence、raw playback evidence 解释）
 *
 * 负控制（negative controls）通过 CONSTANT Mutation 注入，正常模型取 MutationNone。
 * 每个 mutation 对应一个 mutations/*.cfg；安全性 mutation 必须产生 counterexample
 * 才算通过；SingleGlobalGenerationCheck 属功能性破坏，用症状属性证明 dual window
 * 失效（详见 specs/playback/README.md）。
 *)

---- MODULE PlaybackTemporal ----

EXTENDS Naturals, FiniteSets, TLC

(* Mutation 开关（稳定语义名，不使用 ADR/Issue/PR/阶段编号词汇）。
 * 取值以 model value 实例化：cfg 中把 Mutation 绑到下面某个常量名上，
 * 其余常量各自绑到自身。ASSUME 保证绑定的值落在合法集合内，
 * 防止 cfg 手误静默退化为"无任何 mutation 分支匹配"的假 PASS。 *)
CONSTANT Mutation,
         MutationNone,
         MutationPromoteWithoutFence,
         MutationAcceptUnadmittedDecode,
         MutationSingleGlobalGeneration,
         MutationRetiredStillAdmitted,
         MutationEndBeforeRenderDrain

MutationChoices == {MutationNone,
                   MutationPromoteWithoutFence,
                   MutationAcceptUnadmittedDecode,
                   MutationSingleGlobalGeneration,
                   MutationRetiredStillAdmitted,
                   MutationEndBeforeRenderDrain}

ASSUME Mutation \in MutationChoices

MaxGen   == 4    (* generation 上界（有限模型边界）：足以覆盖 seek/seek/next/stop rapid trace *)
GenId    == 1..MaxGen
MaxMedia == 1    (* 每个 generation 的 decode/submit/render 记账上界 *)

FenceIdle == [phase |-> "idle", cut |-> 0, target |-> 0] (* 无进行中的 fence（恒为 record，避免异构取字段） *)
NoTarget == 0      (* fence 无 promotion 目标（stop：切断后不 promote）；0 不是合法 GenId *)
Roles    == {"active", "prepared"}
Phases   == {"idle", "requested", "claimed", "succeeded", "failed"}

VARIABLES
  (* ---- TransportKernel 拥有的 temporal truth ---- *)
  windows,          (* SET of [role, gen, track, ready]：ActiveWindow / PreparedWindow *)
  admitted,         (* open admission 的 generation 集合（admission 关闭后不再打开） *)
  retired,          (* 已 retire 的 generation *)
  nextGen,          (* 下一个新鲜 generation id（单调递增，永不复用） *)
  fence,            (* 恒为 record [cut, target, phase]；phase="idle" 表示无进行中的 fence *)
  fenceSuccessFor,  (* 成功完成 fence 的 cut generation 历史 *)
  promotions,       (* [new, cut] 记录：gen new 经 promotion 成为 ActiveWindow *)
  queued,           (* [GenId -> 0..MaxMedia] 已提交未渲染（设备侧排队中） *)
  submittedEver,    (* [GenId -> 0..MaxMedia] 历史提交数 *)
  rendered,         (* [GenId -> 0..MaxMedia] 历史渲染数（物理证据） *)
  discardedBacklog, (* [GenId -> 0..MaxMedia] admission 关闭转移显式 discard 的
                       decoded-but-never-submitted 媒体计数（决策 8）：提交要求
                       admission 开放且关闭单向，故该 backlog 在关闭时刻已不可
                       能变得可听；显式移入本记账使 drain predicate 保持可达 *)
  decodeAcceptedInAdmission,   (* admission 开放期间接受的 decode result 计数 *)
  decodeAcceptedOutOfAdmission,(* admission 关闭后接受的计数（正常模型恒 0，>0 即违规） *)
  producerTerminal, (* decoder EOF（或 provider withdrawal 后终止）的 generation 集合 *)
  sessionClosed,    (* DecodeSession 已关闭的 generation 集合 *)
  transportDrained, (* TransportKernel 发布的 transport drained 事实 *)
  (* ---- MusicKernel 拥有的产品语义状态 ---- *)
  ended,            (* ENDED（只能由 transport drained 推出） *)
  (* ---- 环境机制 ---- *)
  decoderWithdrawn  (* Decoder provider 已 withdraw（一次性全局事件） *)

vars == <<windows, admitted, retired, nextGen, fence, fenceSuccessFor, promotions,
          queued, submittedEver, rendered, discardedBacklog,
          decodeAcceptedInAdmission, decodeAcceptedOutOfAdmission,
          producerTerminal, sessionClosed, transportDrained, ended,
          decoderWithdrawn>>

(* ---------- 窗口辅助算子 ---------- *)

ActiveWindowSet   == {w \in windows : w.role = "active"}
PreparedWindowSet == {w \in windows : w.role = "prepared"}
HasActive   == ActiveWindowSet   # {}
HasPrepared == PreparedWindowSet # {}
(* CHOOSE 对空集无定义：TLC 求值动作时不保证短路，因此给安全的占位回退。
 * 占位值只在 HasActive / HasPrepared 为假时出现，所有行为都被对应存在性条件把门。 *)
PlaceholderWindow == [role |-> "active", gen |-> 0, ready |-> FALSE]
ActiveW   == IF HasActive   THEN CHOOSE w \in ActiveWindowSet   : TRUE ELSE PlaceholderWindow
PreparedW == IF HasPrepared THEN CHOOSE w \in PreparedWindowSet : TRUE ELSE PlaceholderWindow
ActiveGen   == ActiveW.gen
PreparedGen == PreparedW.gen
WindowWithGen(g) == {w \in windows : w.gen = g}
HasWindow(g)     == WindowWithGen(g) # {}
IsActiveGen(g)  == \E w \in windows : w.gen = g /\ w.role = "active"
(* CHOOSE 在空集上求值无定义：入口动作先检查 HasActive 再取字段，这里给安全默认值 *)
PreparedReadyExists == HasPrepared /\ PreparedW.ready

FreshGenOK == nextGen \leq MaxGen
SessionAlive(g) == g < nextGen /\ g \notin sessionClosed
NewWindow(role_, gen_) ==
  [role |-> role_, gen |-> gen_, ready |-> FALSE]

(* promotion 的 fence 条件（PromoteWithoutFence 负控制只删除 fence-completed
 * 这一条件，保留 close old admission 的骨架顺序——BUG 语义是"提前 promote"，
 * 不是"跳过整个 episode 骨架"）。 *)
PromoteFenceCondition ==
  \/ /\ Mutation = MutationPromoteWithoutFence
     /\ ActiveGen \notin admitted
  \/ /\ fence.phase # "idle"
     /\ fence.phase = "succeeded"
     /\ fence.cut = ActiveGen
     /\ fence.target = PreparedGen

(* decode result 的接收条件。
 * 正常语义 = "窗口存在 且 generation 仍被 admission 接纳"（ADR：晚到结果只有
 * 同时满足 generation + window role + admission contract 才能被接收）。
 * SingleGlobalGeneration 变体模拟经典 BUG：result.generation != current_generation
 * => stale，其中 current_generation 只跟踪当前 ActiveWindow——它对 fence 窗口期内
 * admission 已关闭的 old active gen 错误放行，同时让 prepared gen 永远无法 prime。 *)
AcceptCondition(g) ==
  CASE Mutation = MutationSingleGlobalGeneration ->
        HasActive /\ HasWindow(g) /\ g = ActiveGen
    [] Mutation = MutationAcceptUnadmittedDecode ->
        TRUE
    [] OTHER ->
        HasWindow(g) /\ g \in admitted

(* transport drained 的媒体清空条件（ADR ENDED 等价 predicate 的 transport 部分：
 * producer terminal 且无 submitted-but-unrendered 媒体且 active pipeline 内
 * 无"未 discard 的已接受未提交" decode result——admission 关闭转移已把不可
 * 再提交的 backlog 显式移入 discardedBacklog（决策 8），故该等式对关闭了
 * admission 的 pending-cut active 同样可达） *)
DrainMediaClear ==
  /\ queued[ActiveGen] = 0
  /\ decodeAcceptedInAdmission[ActiveGen]
       = submittedEver[ActiveGen] + discardedBacklog[ActiveGen]
  /\ \A g \in GenId : queued[g] = 0

(* ---------- Init ---------- *)

Init ==
  /\ windows = {}
  /\ admitted = {}
  /\ retired = {}
  /\ nextGen = 1
  /\ fence = FenceIdle
  /\ fenceSuccessFor = {}
  /\ promotions = {}
  /\ queued = [g \in GenId |-> 0]
  /\ submittedEver = [g \in GenId |-> 0]
  /\ rendered = [g \in GenId |-> 0]
  /\ discardedBacklog = [g \in GenId |-> 0]
  /\ decodeAcceptedInAdmission = [g \in GenId |-> 0]
  /\ decodeAcceptedOutOfAdmission = [g \in GenId |-> 0]
  /\ producerTerminal = {}
  /\ sessionClosed = {}
  /\ transportDrained = FALSE
  /\ ended = FALSE
  /\ decoderWithdrawn = FALSE

(* =====================================================================
 * MusicKernel：intent 解释（命令到达拥有被变更事实的 authority）
 * ===================================================================== *)

(* Play：从无 ActiveWindow 起播（stop / ended 之后的新 episode） *)
Play ==
  /\ \neg HasActive
  /\ fence.phase = "idle"
  /\ FreshGenOK
  /\ \neg decoderWithdrawn
  /\ windows' = windows \cup {NewWindow("active", nextGen)}
  /\ admitted' = admitted \cup {nextGen}
  /\ nextGen' = nextGen + 1
  /\ transportDrained' = FALSE
  /\ ended' = FALSE
  /\ UNCHANGED <<retired, fence, fenceSuccessFor, promotions, queued,
                   submittedEver, rendered, discardedBacklog,
                   decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, decoderWithdrawn>>

(* =====================================================================
 * TransportKernel：discontinuity prepare 与 supersede authority
 *
 * seek 与 next 共用同一 execution skeleton（旧版 ADR 冻结）：
 * 若已有 PreparedWindow，新 intent 原子 supersede 它（关闭 admission、retire、
 * 由 CloseRetiredDecodeSession 回收 session），再创建新 PreparedWindow。
 * TransportKernel 是 pending discontinuity 的唯一 supersede/cancel/replace authority。
 *
 * promote-fence 进行中仍允许新的 seek/next（mid-fence supersede 竞态由
 * ConsumeFenceVerdict / PromotePrepared 的 target 匹配处理）；
 * stop-fence 进行中不允许新 intent（stop 是终局性 cut，等 verdict 落定后才能
 * 开新 episode——这是本模型的明确决策，见 specs/playback/README.md）。
 * ===================================================================== *)

PrepareDiscontinuity ==
  /\ HasActive
  /\ fence.phase = "idle" \/ fence.target # NoTarget
  /\ FreshGenOK
  /\ \neg decoderWithdrawn
  /\ windows' = (IF HasPrepared
                 THEN (windows \ {PreparedW}) \cup {NewWindow("prepared", nextGen)}
                 ELSE windows \cup {NewWindow("prepared", nextGen)})
  /\ admitted' = (IF HasPrepared
                  THEN (admitted \ {PreparedGen}) \cup {nextGen}
                  ELSE admitted \cup {nextGen})
  /\ retired' = (IF HasPrepared THEN retired \cup {PreparedGen} ELSE retired)
  /\ nextGen' = nextGen + 1
  /\ UNCHANGED <<fence, fenceSuccessFor, promotions, queued, submittedEver,
                   rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* intent 两种形态：同 TrackSession seek 与 Track Replacement next。
 * 在本抽象层两者共享同一 execution skeleton（旧版 ADR 冻结），TrackSession 维度的
 * 区分由 PlaybackOwnership 模型覆盖，这里保留两个入口动作以区分 intent 语义。 *)
RequestSeek == PrepareDiscontinuity
RequestNext == PrepareDiscontinuity

(* stop：supersede 任何 pending prepared，关闭 active admission，发起终局 fence。
 * 若 promote-fence 已在途（其 cut 恒为当前 ActiveGen），stop 把同一次物理冲刷
 * 重解释为终局 cut：保留握手阶段、只清空 promotion 目标——设备不关心
 * promotion 计划，物理冲刷是同一次。
 * stop 同时否定陈旧的 transportDrained 事实：fence 落定前物理状态未定。 *)
RequestStop ==
  /\ HasActive
  /\ windows' = (IF HasPrepared THEN windows \ {PreparedW} ELSE windows)
  /\ admitted' = (IF HasPrepared
                  THEN admitted \ {ActiveGen, PreparedGen}
                  ELSE admitted \ {ActiveGen})
  /\ retired' = (IF HasPrepared THEN retired \cup {PreparedGen} ELSE retired)
  /\ fence' = [cut |-> ActiveGen, target |-> NoTarget,
               phase |-> IF fence.phase = "idle" THEN "requested" ELSE fence.phase]
  /\ transportDrained' = FALSE
  (* admission 关闭转移同步 discard 未提交 backlog（决策 8，与 CloseOldAdmission
   * 同一规则；对已关闭的 active 重解释 stop 时为幂等重写） *)
  /\ discardedBacklog' =
       [discardedBacklog EXCEPT
          ![ActiveGen] = decodeAcceptedInAdmission[ActiveGen]
                           - submittedEver[ActiveGen]]
  /\ UNCHANGED <<nextGen, fenceSuccessFor, promotions, queued, submittedEver,
                   rendered, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed,
                   ended, decoderWithdrawn>>

(* =====================================================================
 * TransportKernel：decode evidence 解释与 admission 检查
 *
 * 记账规则（DecodeResultRequiresAdmission 的可检测编码）：
 * decode result 的接受按"真实 admission 状态"分流计数——
 *   admitted 期间接受 -> decodeAcceptedInAdmission
 *   admission 关闭后接受 -> decodeAcceptedOutOfAdmission（violation 检测器，
 *                          正常模型 guard 保证恒 0，mutation 打开后 >0）
 * 迟到但被拒绝的结果不进任何 accept 计数（LateDecodeResult 动作即拒绝证据）。
 * ===================================================================== *)

AcceptDecodeResult(g) ==
  /\ SessionAlive(g)
  /\ g \notin producerTerminal
  /\ decodeAcceptedInAdmission[g] + decodeAcceptedOutOfAdmission[g] < MaxMedia
  /\ AcceptCondition(g)
  /\ ( \/ /\ g \in admitted
         /\ decodeAcceptedInAdmission' =
              [decodeAcceptedInAdmission EXCEPT ![g] = decodeAcceptedInAdmission[g] + 1]
         /\ decodeAcceptedOutOfAdmission' = decodeAcceptedOutOfAdmission
     \/ /\ g \notin admitted
         /\ decodeAcceptedOutOfAdmission' =
              [decodeAcceptedOutOfAdmission EXCEPT ![g] = decodeAcceptedOutOfAdmission[g] + 1]
         /\ decodeAcceptedInAdmission' = decodeAcceptedInAdmission )
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, queued, submittedEver, rendered, discardedBacklog,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* 迟到结果：admission 已关闭（被 supersede / close-old / retire）的 generation
 * 产生 decode result => 必须被拒绝。拒绝不改变任何 temporal truth（结果被丢弃），
 * 该动作只用于覆盖性证据：TLC coverage 中 LateDecodeResult 的触发次数证明
 * 模型确实探索了迟到结果的交错。 *)
LateDecodeResult(g) ==
  /\ SessionAlive(g)
  /\ g \notin admitted
  /\ UNCHANGED vars

(* PreparedWindow 完成 prime（至少有一条 admitted 期间接受的 decode result） *)
MarkPreparedReady ==
  /\ HasPrepared
  /\ \neg PreparedW.ready
  /\ decodeAcceptedInAdmission[PreparedGen] \geq 1
  /\ windows' = (windows \ {PreparedW}) \cup {[PreparedW EXCEPT !.ready = TRUE]}
  /\ UNCHANGED <<admitted, retired, nextGen, fence, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

DecoderEof(g) ==
  /\ SessionAlive(g)
  /\ g \notin producerTerminal
  /\ producerTerminal' = producerTerminal \cup {g}
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, queued, submittedEver, rendered,
                   discardedBacklog, decodeAcceptedInAdmission, decodeAcceptedOutOfAdmission, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* prepared decode 在 prime 完成前 EOF（如 seek 到文件尾附近）：
 * discontinuity 无法完成，取消该 intent（模型决策；ADR 骨架未写此失败路径，
 * 属 ADR 观察项，见 README） *)
DropUnprimablePrepared ==
  /\ HasPrepared
  /\ \neg PreparedW.ready
  /\ PreparedGen \in producerTerminal
  /\ windows' = windows \ {PreparedW}
  /\ admitted' = admitted \ {PreparedGen}
  /\ retired' = retired \cup {PreparedGen}
  /\ UNCHANGED <<nextGen, fence, fenceSuccessFor, promotions, queued,
                   submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* =====================================================================
 * TransportKernel：admission 关闭与 Physical Fence
 *
 * 骨架（旧版 ADR 冻结）：prime -> close old admission -> physical fence -> promote -> retire。
 * close old admission 后，old generation 的新 decode result 被 LateDecodeResult 拒绝；
 * 已提交媒体留在设备队列中，直到 fence 成功 verdict 原子冲刷（CompleteFence）。
 * Generation 不能替代 Physical Fence；fence 失败不得伪装成功 promotion。
 * ===================================================================== *)

CloseOldAdmission ==
  /\ HasActive
  /\ ActiveGen \in admitted
  /\ HasPrepared
  /\ PreparedW.ready
  /\ admitted' = admitted \ {ActiveGen}
  (* 决策 8：提交要求 admission 开放且关闭单向，故关闭时刻的
   * decoded-but-never-submitted backlog 永不可能变得可听——在关闭转移
   * 显式移入 discard 记账，而不是留给一个永远无法满足的 drain predicate *)
  /\ discardedBacklog' =
       [discardedBacklog EXCEPT
          ![ActiveGen] = decodeAcceptedInAdmission[ActiveGen]
                           - submittedEver[ActiveGen]]
  /\ UNCHANGED <<windows, retired, nextGen, fence, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

RequestFence ==
  /\ HasActive
  /\ ActiveGen \notin admitted
  /\ HasPrepared
  /\ PreparedW.ready
  /\ fence.phase = "idle"
  /\ fence' = [cut |-> ActiveGen, target |-> PreparedGen, phase |-> "requested"]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

ClaimFence ==
  /\ fence.phase # "idle"
  /\ fence.phase = "requested"
  /\ fence' = [fence EXCEPT !.phase = "claimed"]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* fence 成功 verdict：设备完成物理冲刷——被截断 generation 排队中的未渲染媒体
 * 被丢弃（flush），此后该 generation 保持静默（不可再产生新的可听输出）。 *)
CompleteFence ==
  /\ fence.phase # "idle"
  /\ fence.phase = "claimed"
  /\ fence' = [fence EXCEPT !.phase = "succeeded"]
  /\ fenceSuccessFor' = fenceSuccessFor \cup {fence.cut}
  /\ queued' = [queued EXCEPT ![fence.cut] = 0]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, promotions,
                   submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

FailFence ==
  /\ fence.phase # "idle"
  /\ fence.phase = "claimed"
  /\ fence' = [fence EXCEPT !.phase = "failed"]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

RetryFence ==
  /\ fence.phase # "idle"
  /\ fence.phase = "failed"
  /\ fence' = [fence EXCEPT !.phase = "requested"]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* 放弃失败的 discontinuity（fail closed）：不 promote；active 保持
 * admission-closed 自然排干——该排干之所以可达，是因为 admission 关闭转移
 * 已经把 decoded-but-never-submitted backlog 显式 discard（决策 8）；prepared
 * 留待后续 intent supersede 或重新 fence。 *)
AbandonFence ==
  /\ fence.phase # "idle"
  /\ fence.phase = "failed"
  /\ fence' = FenceIdle
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* fence 成功但 promotion 目标已被 supersede：verdict 被消费（物理冲刷确实发生），
 * 不做 promotion；新 prepared 走自己的 episode。 *)
ConsumeFenceVerdict ==
  /\ fence.phase # "idle"
  /\ fence.phase = "succeeded"
  /\ fence.target # NoTarget
  /\ \neg HasPrepared \/ PreparedGen # fence.target
  /\ fence' = FenceIdle
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* promotion：正常语义下只发生在针对 (cut = 当前 active, target = 当前 prepared)
 * 的 fence 成功 verdict 之后。promotion 开启新的 temporal episode，
 * 旧的 transportDrained 事实随之失效。 *)
PromotePrepared ==
  /\ HasActive
  /\ HasPrepared
  /\ PreparedW.ready
  /\ PromoteFenceCondition
  /\ fence' = FenceIdle
  /\ windows' = (windows \ {ActiveW, PreparedW})
                \cup {[PreparedW EXCEPT !.role = "active", !.ready = TRUE]}
  /\ retired' = retired \cup {ActiveGen}
  /\ promotions' = promotions \cup {[new |-> PreparedGen, cut |-> ActiveGen]}
  /\ transportDrained' = FALSE
  /\ UNCHANGED <<admitted, nextGen, fenceSuccessFor, queued, submittedEver,
                   rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, ended,
                   decoderWithdrawn>>

(* stop 完成：fence 成功后移除 ActiveWindow；transport 层面无在途媒体。
 * stopped != ENDED：两者都是 MusicKernel 对同一 transport truth 的不同产品解释。 *)
StopComplete ==
  /\ fence.phase # "idle"
  /\ fence.phase = "succeeded"
  /\ fence.target = NoTarget
  /\ HasActive
  /\ fence.cut = ActiveGen
  /\ fence' = FenceIdle
  /\ windows' = windows \ {ActiveW}
  /\ retired' = retired \cup {ActiveGen}
  /\ transportDrained' = TRUE
  /\ UNCHANGED <<admitted, nextGen, fenceSuccessFor, promotions, queued,
                   submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, ended, decoderWithdrawn>>

(* =====================================================================
 * DecodeSession 回收
 *
 * retired 或被 supersede（无窗口且无 admission）的 generation 关闭 decode
 * session。同时折叠历史记账（submittedEver/queued/decodeAcceptedInAdmission/
 * discardedBacklog 收敛为 rendered 值）——这是刻意的状态空间抽象：retired
 * generation 的历史细节不参与任何活跃 invariant；violation 检测器
 * decodeAcceptedOutOfAdmission 不折叠，保证 mutation counterexample 可见。
 * ===================================================================== *)

CloseRetiredDecodeSession(g) ==
  /\ SessionAlive(g)
  /\ g \in retired \/ (\neg HasWindow(g) /\ g \notin admitted)
  /\ sessionClosed' = sessionClosed \cup {g}
  /\ submittedEver' = [submittedEver EXCEPT ![g] = rendered[g]]
  /\ queued' = [queued EXCEPT ![g] = 0]
  /\ decodeAcceptedInAdmission' =
       [decodeAcceptedInAdmission EXCEPT ![g] = rendered[g]]
  /\ discardedBacklog' = [discardedBacklog EXCEPT ![g] = 0]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, rendered, decodeAcceptedOutOfAdmission,
                   producerTerminal, transportDrained,
                   ended, decoderWithdrawn>>

(* =====================================================================
 * 数据面记账：submitted / queued / rendered
 *
 * 只有 ActiveWindow 且 admission 仍开放的 generation 可以向设备提交媒体；
 * 每次提交必须由一条 admitted 期间接受的 decode result 支撑。
 * 新媒体进入 transport 即否定陈旧的 drained 事实。
 * ===================================================================== *)

SubmitMedia(g) ==
  /\ IsActiveGen(g)
  /\ g \in admitted
  /\ decodeAcceptedInAdmission[g] > submittedEver[g]
  /\ submittedEver[g] < MaxMedia
  /\ queued[g] < MaxMedia
  /\ submittedEver' = [submittedEver EXCEPT ![g] = submittedEver[g] + 1]
  /\ queued' = [queued EXCEPT ![g] = queued[g] + 1]
  /\ transportDrained' = FALSE
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, ended,
                   decoderWithdrawn>>

(* 物理渲染证据：从设备队列消费已提交媒体 *)
RenderMedia(g) ==
  /\ queued[g] > 0
  /\ rendered' = [rendered EXCEPT ![g] = rendered[g] + 1]
  /\ queued' = [queued EXCEPT ![g] = queued[g] - 1]
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, submittedEver, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   ended, decoderWithdrawn>>

(* =====================================================================
 * Provider withdrawal（轻量交互；完整 dependent-before-provider 顺序在
 * PlaybackOwnership 模型）：decoder provider 撤出后，不再创建新 decode
 * session，所有存活 session 的 producer 进入 terminal，系统自然排干。
 * ===================================================================== *)

WithdrawDecoderProvider ==
  /\ \neg decoderWithdrawn
  /\ decoderWithdrawn' = TRUE
  /\ producerTerminal' =
       producerTerminal \cup {g \in GenId : SessionAlive(g)}
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, queued, submittedEver, rendered,
                   discardedBacklog, decodeAcceptedInAdmission, decodeAcceptedOutOfAdmission, sessionClosed, transportDrained,
                   ended>>

(* =====================================================================
 * TransportKernel -> MusicKernel：drained / ENDED 语义层次
 *
 * TransportKernel 从 raw evidence 得出 transport-drained truth；
 * MusicKernel 再解释产品语义（ENDED）。Decoder EOF != transport drained != ENDED。
 * ===================================================================== *)

PublishTransportDrained ==
  /\ HasActive
  /\ ActiveGen \in producerTerminal
  /\ fence.phase = "idle"
  /\ (Mutation = MutationEndBeforeRenderDrain \/ DrainMediaClear)
  /\ transportDrained' = TRUE
  /\ UNCHANGED <<windows, admitted, retired, nextGen, fence, fenceSuccessFor,
                   promotions, queued, submittedEver, rendered,
                   discardedBacklog, decodeAcceptedInAdmission, decodeAcceptedOutOfAdmission, producerTerminal, sessionClosed,
                   ended, decoderWithdrawn>>

PublishEnded ==
  /\ transportDrained
  /\ HasActive
  /\ \neg HasPrepared
  /\ ended' = TRUE
  /\ windows' = windows \ {ActiveW}
  /\ admitted' = admitted \ {ActiveGen}
  /\ retired' = retired \cup {ActiveGen}
  /\ UNCHANGED <<nextGen, fence, fenceSuccessFor, promotions, queued,
                   submittedEver, rendered, discardedBacklog, decodeAcceptedInAdmission,
                   decodeAcceptedOutOfAdmission,
                   producerTerminal, sessionClosed, transportDrained,
                   decoderWithdrawn>>

(* RetiredStillAdmitted 负控制：retired generation 的 admission 被错误重新打开 *)
ReopenRetiredAdmission ==
  /\ Mutation = MutationRetiredStillAdmitted
  /\ \E g \in retired :
       /\ g \notin admitted
       /\ admitted' = admitted \cup {g}
  /\ UNCHANGED <<windows, retired, nextGen, fence, fenceSuccessFor, promotions,
                   queued, submittedEver, rendered,
                   discardedBacklog, decodeAcceptedInAdmission, decodeAcceptedOutOfAdmission, producerTerminal, sessionClosed,
                   transportDrained, ended, decoderWithdrawn>>

(* 等待环境（用户命令 / decoder / 设备）：合法等待状态不是 deadlock *)
Stall == UNCHANGED vars

Next ==
  \/ Play
  \/ RequestSeek \/ RequestNext \/ RequestStop
  \/ \E g \in GenId : AcceptDecodeResult(g)
  \/ \E g \in GenId : LateDecodeResult(g)
  \/ MarkPreparedReady
  \/ \E g \in GenId : DecoderEof(g)
  \/ DropUnprimablePrepared
  \/ CloseOldAdmission
  \/ RequestFence \/ ClaimFence \/ CompleteFence \/ FailFence
  \/ RetryFence \/ AbandonFence \/ ConsumeFenceVerdict
  \/ PromotePrepared \/ StopComplete
  \/ \E g \in GenId : CloseRetiredDecodeSession(g)
  \/ \E g \in GenId : SubmitMedia(g)
  \/ \E g \in GenId : RenderMedia(g)
  \/ WithdrawDecoderProvider
  \/ PublishTransportDrained \/ PublishEnded
  \/ ReopenRetiredAdmission
  \/ Stall

Spec == Init /\ [][Next]_vars

(* =====================================================================
 * Safety properties（稳定语义命名）
 * ===================================================================== *)

TypeOK ==
  /\ windows \subseteq [role : Roles, gen : GenId, ready : BOOLEAN]
  /\ admitted \subseteq GenId
  /\ retired \subseteq GenId
  /\ nextGen \in 1..MaxGen + 1
  /\ fence \in [cut : 0..MaxGen, target : 0..MaxGen, phase : Phases]
  /\ fenceSuccessFor \subseteq GenId
  /\ promotions \subseteq [new : GenId, cut : GenId]
  /\ queued \in [GenId -> 0..MaxMedia]
  /\ submittedEver \in [GenId -> 0..MaxMedia]
  /\ rendered \in [GenId -> 0..MaxMedia]
  /\ discardedBacklog \in [GenId -> 0..MaxMedia]
  /\ decodeAcceptedInAdmission \in [GenId -> 0..MaxMedia]
  /\ decodeAcceptedOutOfAdmission \in [GenId -> 0..MaxMedia]
  /\ producerTerminal \subseteq GenId
  /\ sessionClosed \subseteq GenId
  /\ transportDrained \in BOOLEAN
  /\ ended \in BOOLEAN
  /\ decoderWithdrawn \in BOOLEAN

(* --- Dual Window 结构 --- *)
AtMostOneActiveWindow == Cardinality(ActiveWindowSet) \leq 1
AtMostOnePreparedWindow == Cardinality(PreparedWindowSet) \leq 1
ActivePreparedGenerationsDistinct ==
  \neg HasActive \/ \neg HasPrepared \/ ActiveGen # PreparedGen

(* --- admission 与窗口角色 --- *)
(* admission 已关闭的 generation 不得持有 PreparedWindow 角色：
 * supersede / stop 都必须原子移除 prepared；只有 pending episode 的 active
 * cut 允许在 admission 关闭期间继续作为 ActiveWindow 渲染存量媒体 *)
PreparedWindowsAreAdmitted ==
  \A w \in windows : w.role = "prepared" => w.gen \in admitted

(* retired generation 不得重新进入：无 admission、无窗口、无在途媒体 *)
RetiredGenerationCannotReenter ==
  /\ retired \cap admitted = {}
  /\ \A g \in retired : \neg HasWindow(g)
  /\ \A g \in retired : queued[g] = 0

(* --- decode result admission 契约 --- *)
(* admission 关闭后接受的 decode result 计数恒为 0。
 * AcceptUnadmittedDecode / SingleGlobalGeneration mutation 以此为捕手；
 * 正常模型 guard 保证第二计数分支不可达 *)
DecodeResultRequiresAdmission ==
  \A g \in GenId : decodeAcceptedOutOfAdmission[g] = 0

(* --- 数据面记账 --- *)
(* 每次提交都由 admitted 期间接受的 decode result 支撑 *)
SubmissionsBackedByAdmittedDecodes ==
  \A g \in GenId : submittedEver[g] \leq decodeAcceptedInAdmission[g]

(* 在途媒体只存在于 ActiveWindow 的 generation *)
PendingMediaOnlyInActiveWindow ==
  \A g \in GenId : queued[g] > 0 => IsActiveGen(g)

(* PreparedWindow 在 promotion 前不得冒充输出 authority：
 * 不得提交、不得渲染、不得有在途媒体 *)
PreparedWindowIsNotOutputAuthority ==
  \A w \in PreparedWindowSet :
    submittedEver[w.gen] = 0 /\ rendered[w.gen] = 0 /\ queued[w.gen] = 0

RenderedNeverExceedsSubmitted ==
  \A g \in GenId : rendered[g] \leq submittedEver[g]

(* --- Physical Fence --- *)
(* 每次 promotion 的 cut generation 都经历过成功的 fence verdict *)
PromotionRequiresSuccessfulFence ==
  \A p \in promotions : p.cut \in fenceSuccessFor

(* fence 成功冲刷后的 generation 保持静默（无在途媒体可再渲染） *)
FenceFlushedGenerationsAreSilent ==
  \A g \in fenceSuccessFor : queued[g] = 0

(* --- drained / ENDED 层次 --- *)
(* transport drained 蕴含：无任何 submitted-but-unrendered 媒体，
 * 且 active pipeline 内无未 discard 的已接受未提交 decode result *)
TransportDrainRequiresRenderedDrain ==
  transportDrained =>
    /\ \A g \in GenId : queued[g] = 0
    /\ \A w \in ActiveWindowSet :
         decodeAcceptedInAdmission[w.gen]
           = submittedEver[w.gen] + discardedBacklog[w.gen]

(* admission 已关闭的窗口不得有未申报的 stranded backlog：关闭转移必须
 * 把 decoded-but-never-submitted 媒体显式移入 discard 记账（决策 8）。
 * 否则 fail/abandon 或 verdict-consumed 之后 accepted > submitted 的
 * admission-closed active 永远无法满足自然 drain predicate（executable
 * core 对抗 review 发现的真实悬挂状态）。 *)
NoStrandedDecodeAfterAdmissionClose ==
  \A w \in windows :
    w.gen \notin admitted =>
      decodeAcceptedInAdmission[w.gen]
        = submittedEver[w.gen] + discardedBacklog[w.gen]

(* ENDED 只能建立在 TransportKernel 的 drained truth 之上 *)
EndedRequiresTransportDrain == ended => transportDrained

(* stop-fence 的 cut 生命周期：终局 fence 从发起到 StopComplete 消费期间
 * ActiveWindow 不得被移除——drained/ENDED 的发布必须等待 fence 落定
 *（stop 与自然 ENDED 竞态的模型决策，见 README 模型决策记录） *)
StopFenceRequiresActiveWindow ==
  (fence.phase # "idle" /\ fence.target = NoTarget) => HasActive

(* --- 症状属性（供 SingleGlobalGenerationCheck 负控制使用） --- *)
(* 若 decode 接收使用全局 current_generation 相等检查（mutation 3），
 * PreparedWindow 将永远无法完成 prime——"Dual Window 失效"以该症状属性
 * 成立的形式被证明：mutated 模型必须满足（无任何 ready prepared 状态），
 * 而正常模型必须不满足（存在 ready prepared，即 TLC 会对此属性报 violation）。 *)
DualWindowNeverPrimesUnderGlobalCheck == [](\neg PreparedReadyExists)

====
