(*
 * PlaybackOwnership — 播放运行时资源 ownership 的形式化模型
 *
 * 被审计对象：docs/adr/ADR-PBK-001.md 的 ownership 语义（Composition Lifecycle
 * Root / Immediate Lifetime Owner / Semantic Authority 三分，以及 provider
 * withdrawal 的 dependent-before-final-release 顺序）。
 *
 * 本模型与 PlaybackTemporal 互补，不重复其 temporal 语义：
 *   - MusicComponent 是 composition lifecycle root（episode 结束时 subordinate
 *     playback runtime 必须全部退出）
 *   - Nested Runtime Resource 形成严格 ownership 树：
 *         MusicComponent
 *         ├── MusicKernel
 *         ├── TransportKernel
 *         └── TrackSession A / B
 *             └── DecodeSession（1..3 个，属于恰好一个 TrackSession）
 *   - lifetime ownership != semantic authority：MusicKernel / TransportKernel
 *     拥有各自 semantic authority 事实，但不是任何 nested resource 的
 *     immediate lifetime owner
 *   - Decoder provider / AudioOutput provider 是独立组合的 provider：
 *     withdrawal 时 dependents 先失去新 commitment 能力，已 commit 的
 *     teardown 访问保持合法，全部 dependent 退出后 provider 才能 final release
 *
 * 抽象边界（有限实例）：
 *   1 MusicComponent + 1 MusicKernel + 1 TransportKernel
 *   2 TrackSession、3 DecodeSession、2 Provider
 *
 * 注意：ActiveWindow / PreparedWindow 的 immediate lifetime owner 未被 ADR
 * 冻结（ADR 留给 implementation design），故不进入本 ownership 树；window 的
 * temporal 语义由 PlaybackTemporal 模型覆盖。
 *
 * 负控制（negative controls）通过 CONSTANT Mutation 注入（model value 实例化），
 * 每个 mutation 对应一个 mutations/*.cfg，必须产生 counterexample。
 *)

---- MODULE PlaybackOwnership ----

EXTENDS Naturals, FiniteSets, TLC

(* Mutation 开关（稳定语义名，不使用 ADR/Issue/PR/阶段编号词汇） *)
CONSTANT Mutation,
         MutationNone,
         MutationReleaseProviderEarly,
         MutationMultipleImmediateOwners,
         MutationOwnershipCycle,
         MutationKernelAdoptsOwnership

MutationChoices == {MutationNone,
                   MutationReleaseProviderEarly,
                   MutationMultipleImmediateOwners,
                   MutationOwnershipCycle,
                   MutationKernelAdoptsOwnership}

ASSUME Mutation \in MutationChoices

(* ---------- 有限实例宇宙 ---------- *)

MusicComponent    == "MusicComponent"
MusicKernelRsc    == "MusicKernel"
TransportKernel   == "TransportKernel"
TrackSessions     == {"TrackSessionA", "TrackSessionB"}
DecodeSessions    == {"DecodeSession1", "DecodeSession2", "DecodeSession3"}

TrackSessionOf(d) == IF d = "DecodeSession1" THEN "TrackSessionA"
                     ELSE IF d = "DecodeSession2" THEN "TrackSessionB"
                     ELSE "TrackSessionA"   (* 第 3 个 session 归 A，用于同 Track 双 DecodeSession *)

KernelResources   == {MusicKernelRsc, TransportKernel}
NestedResources   == KernelResources \cup TrackSessions \cup DecodeSessions
AllResources      == {MusicComponent} \cup NestedResources

DecoderProvider   == "DecoderProvider"
AudioOutputProvider == "AudioOutputProvider"
Providers         == {DecoderProvider, AudioOutputProvider}

(* DecodeSession 的机制依赖：decoder provider。
 * TransportKernel 的机制依赖：audio output provider（物理输出路径）。 *)
DependsOnDecoder(d) == d \in DecodeSessions
(* 模型决策（ADR 未定义此关系）：TransportKernel 的物理输出路径依赖
 * AudioOutput provider，故它是 output provider 的 dependent。 *)
ProviderDependents(p) ==
  IF p = DecoderProvider THEN DecodeSessions
  ELSE {TransportKernel}

(* Semantic authority 事实表（ADR §1.3/§3.3 冻结的正结构）：
 * 每个 playback 事实有唯一 semantic authority holder。
 * lifetime ownership 与 semantic authority 是两种正交关系——
 * authority 关系在事实维度上，ownership 关系在资源维度上，二者不相交。 *)
TemporalAuthorityFacts == {"playbackCursor", "windowRoles", "generationAdmission",
                          "discontinuityExecution", "physicalEvidenceInterpretation"}
MusicDomainAuthorityFacts == {"playlistPolicy", "selectionSemantics", "endedSemantics"}
AuthorityFacts == TemporalAuthorityFacts \cup MusicDomainAuthorityFacts
SemanticAuthorityHolder(f) ==
  IF f \in MusicDomainAuthorityFacts THEN MusicKernelRsc ELSE TransportKernel

(* ---------- 生命周期状态 ---------- *)

Absent   == "Absent"    (* 尚未创建 *)
Alive    == "Alive"     (* 存在且可服务 *)
Draining == "Draining"  (* teardown 已开始，资源仍存在、teardown 访问仍合法 *)
Gone     == "Gone"      (* 完全退出，ownership 边已清除 *)
ResourceStates == {Absent, Alive, Draining, Gone}

Bound     == "Bound"      (* provider 绑定中，可接受新 commitment *)
Withdrawing == "Withdrawing" (* withdrawal 已开始：不再接受新 commitment *)
Released  == "Released"   (* final release 完成 *)
ProviderStates == {Bound, Withdrawing, Released}

VARIABLES
  ownerOf,        (* [AllResources -> SUBSET AllResources]：immediate lifetime owner 集合。
                   * 用集合表示是刻意的：唯一 owner 因此成为可检查的不变量，
                   * 而不是函数类型的结构性必然。 *)
  state,          (* [AllResources -> ResourceStates] *)
  providerState   (* [Providers -> ProviderStates] *)

vars == <<ownerOf, state, providerState>>

(* ---------- ownership 路径辅助算子（有界深度，树深最多 3） ---------- *)

Up1(r) == ownerOf[r]
Up2(r) == UNION {ownerOf[o] : o \in Up1(r)}
Up3(r) == UNION {ownerOf[o] : o \in Up2(r)}

(* 有界深度说明：实例宇宙内 ownership 链最深为
 * DecodeSession -> TrackSession -> MusicComponent（2 跳），Up3 可发现
 * 长度 <= 3 的环。若扩展动作集或层次深度，必须同步加深此处，
 * 否则环/根检测会静默漏检。 *)
ReachesLifecycleRoot(r) == MusicComponent \in (Up1(r) \cup Up2(r) \cup Up3(r))
InOwnershipCycle(r) == r \in (Up1(r) \cup Up2(r) \cup Up3(r))

Exists(r) == state[r] \in {Alive, Draining}

(* ---------- Init：composition 已就绪 ---------- *)

Init ==
  /\ ownerOf = [r \in AllResources |->
                  IF r = MusicKernelRsc THEN {MusicComponent}
                  ELSE IF r = TransportKernel THEN {MusicComponent}
                  ELSE {}]
  /\ state = [r \in AllResources |->
                IF r = MusicComponent THEN Alive
                ELSE IF r = MusicKernelRsc THEN Alive
                ELSE IF r = TransportKernel THEN Alive
                ELSE Absent]
  /\ providerState = [p \in Providers |-> Bound]

(* =====================================================================
 * 组合生命周期：TrackSession / DecodeSession 的创建
 * ===================================================================== *)

CreateTrackSession(ts) ==
  /\ state[MusicComponent] = Alive
  /\ state[ts] = Absent
  /\ state' = [state EXCEPT ![ts] = Alive]
  /\ ownerOf' = [ownerOf EXCEPT ![ts] = {MusicComponent}]
  /\ UNCHANGED providerState

(* 新 DecodeSession 需要 decoder provider 仍在接受新 commitment *)
CreateDecodeSession(d) ==
  /\ state[TrackSessionOf(d)] = Alive
  /\ state[d] = Absent
  /\ providerState[DecoderProvider] = Bound
  /\ state' = [state EXCEPT ![d] = Alive]
  /\ ownerOf' = [ownerOf EXCEPT ![d] =
                   IF Mutation = MutationMultipleImmediateOwners
                   THEN {TrackSessionOf(d)} \cup (TrackSessions \ {TrackSessionOf(d)})
                   ELSE {TrackSessionOf(d)}]
  /\ UNCHANGED providerState

(* =====================================================================
 * teardown：Draining -> Gone
 * ===================================================================== *)

StartTeardown(r) ==
  /\ state[r] = Alive
  /\ state' = [state EXCEPT ![r] = Draining]
  /\ UNCHANGED <<ownerOf, providerState>>

(* DecodeSession 退出（teardown 访问在 provider Withdrawing 期间保持合法） *)
FinishDecodeSession(d) ==
  /\ state[d] \in {Alive, Draining}
  /\ state' = [state EXCEPT ![d] = Gone]
  /\ ownerOf' = [ownerOf EXCEPT ![d] = {}]
  /\ UNCHANGED providerState

(* TrackSession 退出：其名下所有 DecodeSession 必须先完全退出 *)
FinishTrackSession(ts) ==
  /\ state[ts] \in {Alive, Draining}
  /\ \A d \in DecodeSessions :
       TrackSessionOf(d) = ts /\ ownerOf[d] = {ts} => state[d] = Gone
  /\ state' = [state EXCEPT ![ts] = Gone]
  /\ ownerOf' = [ownerOf EXCEPT ![ts] = {}]
  /\ UNCHANGED providerState

(* MusicComponent episode 结束：subordinate playback runtime 必须全部退出 *)
EndMusicEpisode ==
  /\ state[MusicComponent] = Alive
  /\ state' = [state EXCEPT ![MusicComponent] = Draining]
  /\ UNCHANGED <<ownerOf, providerState>>

FinishMusicComponent ==
  /\ state[MusicComponent] = Draining
  /\ \A r \in NestedResources : state[r] = Gone
  /\ state' = [state EXCEPT ![MusicComponent] = Gone]
  /\ UNCHANGED <<ownerOf, providerState>>

(* Kernel 退出只发生在 MusicComponent teardown 末段 *)
FinishKernel(r) ==
  /\ r \in KernelResources
  /\ state[MusicComponent] = Draining
  /\ \A ts \in TrackSessions \cup DecodeSessions : state[ts] = Gone
  /\ state[r] \in {Alive, Draining}
  /\ state' = [state EXCEPT ![r] = Gone]
  /\ ownerOf' = [ownerOf EXCEPT ![r] = {}]
  /\ UNCHANGED providerState

(* =====================================================================
 * Provider withdrawal：dependent-before-final-release 顺序
 *
 *   provider begins withdrawal（不再接受新 commitment）
 *       -> dependents 失去新 commitment 能力
 *       -> 已 commit 的 dependent teardown 访问保持合法
 *       -> 全部 dependent 退出
 *       -> provider final release
 * ===================================================================== *)

WithdrawProvider(p) ==
  /\ providerState[p] = Bound
  /\ providerState' = [providerState EXCEPT ![p] = Withdrawing]
  /\ UNCHANGED <<ownerOf, state>>

FinalReleaseProvider(p) ==
  /\ providerState[p] = Withdrawing
  /\ (Mutation = MutationReleaseProviderEarly
      \/ \A r \in ProviderDependents(p) : state[r] \in {Absent, Gone})
  /\ providerState' = [providerState EXCEPT ![p] = Released]
  /\ UNCHANGED <<ownerOf, state>>

(* =====================================================================
 * 负控制专用动作
 * ===================================================================== *)

(* KernelAdoptsLifetimeOwnership 负控制：TransportKernel（playback temporal
 * semantic authority holder）错误地成为 DecodeSession 的 immediate
 * lifetime owner——semantic authority 越权为 lifetime ownership *)
KernelAdoptsLifetimeOwnershipAction ==
  /\ Mutation = MutationKernelAdoptsOwnership
  /\ state["DecodeSession1"] \in {Alive, Draining}
  /\ ownerOf' = [ownerOf EXCEPT !["DecodeSession1"] = {TransportKernel}]
  /\ UNCHANGED <<state, providerState>>

(* OwnershipCycle：把 TrackSessionA 的 owner 改成它自己的 DecodeSession，
 * 形成 A -> d -> A 环 *)
RewireOwnershipCycle ==
  /\ Mutation = MutationOwnershipCycle
  /\ state["TrackSessionA"] \in {Alive, Draining}
  /\ state["DecodeSession1"] \in {Alive, Draining}
  /\ ownerOf["DecodeSession1"] = {"TrackSessionA"}
  /\ ownerOf' = [ownerOf EXCEPT !["TrackSessionA"] = {"DecodeSession1"}]
  /\ UNCHANGED <<state, providerState>>

Stall == UNCHANGED vars

Next ==
  \/ \E ts \in TrackSessions : CreateTrackSession(ts)
  \/ \E d \in DecodeSessions : CreateDecodeSession(d)
  \/ \E r \in NestedResources : StartTeardown(r)
  \/ \E d \in DecodeSessions : FinishDecodeSession(d)
  \/ \E ts \in TrackSessions : FinishTrackSession(ts)
  \/ EndMusicEpisode
  \/ FinishMusicComponent
  \/ \E r \in KernelResources : FinishKernel(r)
  \/ \E p \in Providers : WithdrawProvider(p)
  \/ \E p \in Providers : FinalReleaseProvider(p)
  \/ KernelAdoptsLifetimeOwnershipAction
  \/ RewireOwnershipCycle
  \/ Stall

Spec == Init /\ [][Next]_vars

(* =====================================================================
 * Safety properties（稳定语义命名）
 * ===================================================================== *)

TypeOK ==
  /\ ownerOf \in [AllResources -> SUBSET AllResources]
  /\ state \in [AllResources -> ResourceStates]
  /\ providerState \in [Providers -> ProviderStates]

(* 存在中的 nested resource 恰有一个 immediate lifetime owner；
 * 不存在的 resource 没有 owner 边。MusicComponent 是 composition
 * lifecycle root，自身无 owner，不参与此约束 *)
UniqueImmediateLifetimeOwner ==
  \A r \in NestedResources :
    Exists(r) <=> Cardinality(ownerOf[r]) = 1

(* owner 自身必须存在（且最终到达 composition lifecycle root） *)
OwnershipReachesLifecycleRoot ==
  \A r \in NestedResources : Exists(r) => ReachesLifecycleRoot(r)

OwnershipIsAcyclic ==
  \A r \in AllResources : \neg InOwnershipCycle(r)

(* 层次形状：DecodeSession 的 immediate owner 是 TrackSession；
 * TrackSession 的 immediate owner 是 MusicComponent *)
DecodeSessionsOwnedByTrackSessions ==
  \A d \in DecodeSessions : Exists(d) => ownerOf[d] \subseteq TrackSessions

TrackSessionsOwnedByMusicComponent ==
  \A ts \in TrackSessions : Exists(ts) => ownerOf[ts] = {MusicComponent}

(* lifetime ownership != semantic authority（可检查形式）：
 * 任一 semantic authority 事实的 holder 都不是任何 resource 的 immediate
 * lifetime owner。负控制 KernelAdoptsLifetimeOwnership 证明此不变量可失败。 *)
SemanticAuthoritiesHoldNoLifetimeOwnership ==
  \A f \in AuthorityFacts :
    SemanticAuthorityHolder(f) \notin UNION {ownerOf[r] : r \in AllResources}

(* provider final release 后不得残留任何存活 dependent（从未创建的
 * Absent dependent 不阻塞 release，也不构成违反） *)
ProviderFinalReleaseRequiresDependentExit ==
  \A p \in Providers :
    providerState[p] = Released =>
      \A r \in ProviderDependents(p) : state[r] \in {Absent, Gone}

(* TrackSession 完全退出后，名下不得再挂任何 DecodeSession *)
TrackSessionGoneImpliesDecodeSessionsDischarged ==
  \A ts \in TrackSessions :
    state[ts] = Gone => \A d \in DecodeSessions : ts \notin ownerOf[d]

(* MusicComponent episode 完全退出后，所有 subordinate 必须已退出 *)
MusicComponentGoneImpliesSubordinatesExited ==
  state[MusicComponent] = Gone =>
    \A r \in NestedResources : state[r] = Gone

(* 正在 withdrawal 的 provider 不再接受新 commitment（结构性检查：
 * 若 DecodeSession 在 provider 非 Bound 时创建过，此不变量无法直接观测——
 * 由 CreateDecodeSession guard 保证，README 记录） *)

====
