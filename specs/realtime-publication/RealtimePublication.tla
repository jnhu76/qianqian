--------------------------- MODULE RealtimePublication ---------------------------
(***************************************************************************)
(* RealtimePublication — realtime view publication / reader quiescence /  *)
(*                        resource reclamation 的最小并发安全模型            *)
(*                                                                         *)
(* STATUS:                                                                 *)
(*   FORMAL EVIDENCE（当前挣得）。语义范围来自 ACCEPTED                     *)
(*   docs/adr/ADR-PBK-001.md §6（Graph Publication 最小 correctness         *)
(*   contract 与 lifetime safety）与 §12 Phase D / §13 Formalization        *)
(*   policy 的候选风险。本模型不是第二份 normative authority；              *)
(*   ADR-PBK-001 仍是唯一 Playback Foundations constitution。              *)
(*                                                                         *)
(*   本模型只攻击一个具体交错：realtime view N -> N+1 发布与旧 reader         *)
(*   overlap 时，resource 何时才获得 release/reclaim 权利。它不建模           *)
(*   PCM、播放器语义、线程调度或内存序（见 README 的未覆盖清单）。            *)
(*                                                                         *)
(* 模型词汇 -> ADR-PBK-001 §6 协议动词映射：                                *)
(*   AcquireView        reader 获取当前 published view（一次 acquisition     *)
(*                      只观察一个完整视图）                                 *)
(*   PublishNewView     Realtime-view publication：retire 旧视图 + 原子切换   *)
(*                      published 拓扑与资源表（RT-safe boundary 的一次提交）*)
(*   ReaderExit         读者退出视图（此后不再能解引用该视图资源）            *)
(*   MarkReclaimable    控制侧认证 quiescence（没有任何 reader / queued       *)
(*                      reference 仍能解引用该资源）                          *)
(*   ReleaseResource    物理释放（只有已认证资源可获得释放资格）              *)
(*                                                                         *)
(* 关键状态区分（不合并成 bool 的原因）：                                    *)
(*   Retired != Reclaimable != Released                                    *)
(*   PublishedNew != OldReadersGone                                        *)
(*   NoNewEntry != Quiescent != PhysicallyFreed                            *)
(*                                                                         *)
(* 负控制（Mutation 常量，逐个独立注入）：                                   *)
(*   MutationReleaseBeforeQuiesce     M1 发布后未等 quiescence 直接释放      *)
(*   MutationSplitPublication         M2 拓扑/资源表分两步发布（half 视图）  *)
(*   MutationStaleEntry               M3 发布后仍允许新 reader 进入旧视图    *)
(*   MutationForgetsOlderRetirement   M4 多代发布链中只检查最新退休代读者    *)
(***************************************************************************)

EXTENDS Integers

CONSTANTS
    \* realtime reader 身份（抽象：覆盖正在执行与已加载待执行的 queued reference）
    Readers,
    \* 可被 realtime 解引用的资源全集
    Resources,
    \* OldResource：view 0 中存在、被第 1 次 publication 排除的资源（最小危险历史的 A）
    OldResource,
    \* ChainResource：可选第二资源；存在于 view 0/1，被第 2 次 publication 排除
    ChainResource,
    \* 1 = 最小历史（N -> N+1 单次发布）；2 = 发布链（增加 N+2，用于 M4）
    MaxPublications,
    \* 负控制开关（稳定语义名，不使用 ADR/Issue/PR/阶段编号词汇；
    \* 取值以 model value 实例化：cfg 中把 Mutation 绑到下面某个常量名上）
    Mutation,
    MutationNone,
    MutationReleaseBeforeQuiesce,
    MutationSplitPublication,
    MutationStaleEntry,
    MutationForgetsOlderRetirement

MutationChoices == {MutationNone,
                    MutationReleaseBeforeQuiesce,
                    MutationSplitPublication,
                    MutationStaleEntry,
                    MutationForgetsOlderRetirement}

ASSUME /\ Mutation \in MutationChoices
       /\ Readers # {}
       /\ Resources # {}

VARIABLES
    \* 已完成发布步数；epoch 同时是 published topology 的身份标签。
    \* view e retired  iff  e < publishedEpoch。
    publishedEpoch,
    \* 已发布 resource-table 分量（published view = epoch 标签 + 该表；
    \* 拆成两个状态分量是为了让 M2 能表达“半发布”窗口）
    pubResources,
    \* 每读者当前仍可解引用的完整视图记录（或 NoView 哨兵）
    readerView,
    \* 每读者 acquisition 时刻的 publishedEpoch（NoView 时为陈旧值，无意义）
    acqEpoch,
    \* 每资源生命周期：live | retired | reclaimable | released
    resourceState,
    \* 历史变量：是否发生过 quiescence 认证（MarkReclaimable）
    certified

vars == <<publishedEpoch, pubResources, readerView, acqEpoch, resourceState, certified>>

-----------------------------------------------------------------------------
(**************************** 配置良构性与视图族 ******************************)

ConfigSensible ==
    /\ OldResource \in Resources
    /\ MaxPublications \in {1, 2}
    /\ (MaxPublications >= 2 => /\ ChainResource \in Resources
                                /\ ChainResource # OldResource)

\* view e 的资源表：每次 publication 排除 OldResource，链式第二次再排除 ChainResource
ResourcesAt(e) ==
    IF e = 0 THEN Resources
    ELSE IF e = 1 THEN Resources \ {OldResource}
    ELSE Resources \ {OldResource, ChainResource}

\* 第 e 次 publication（发布 view e）所排除的资源集
ExcludedBy(e) == ResourcesAt(e - 1) \ ResourcesAt(e)

\* 完整视图 = [纪元 | 拓扑标签 | 资源表]。拓扑以 epoch 为标签：同一视图的
\* 拓扑与资源表只有作为整体才是一致的（拓扑 e 的边恰好解引用 ResourcesAt(e)）。
ViewAt(e) == [epoch |-> e, topology |-> e, resources |-> ResourcesAt(e)]

WellFormedViews == {ViewAt(e) : e \in 0 .. MaxPublications}

\* 退出态哨兵（字段结构与视图记录同形，便于不变式无短路求值）
NoView == [epoch |-> -1, topology |-> -1, resources |-> {}]

\* 当前 published view = published epoch/topology 分量 + published 资源表分量的组合。
\* 正常协议下两分量只随一次原子发布同时变化，组合恒为 WellFormedViews 成员；
\* M2 把发布拆成两步后，中途组合出“拓扑来自 N + 资源表来自 N+1”的混合视图。
ComposeCurrent ==
    [epoch |-> publishedEpoch, topology |-> publishedEpoch, resources |-> pubResources]

RetiredViews == {ViewAt(e) : e \in 0 .. publishedEpoch - 1}

\* 仍可能通过持有视图解引用资源 x 的读者（active 执行 + queued reference 的统一抽象）
DerefHolders(x) == {r \in Readers : x \in readerView[r].resources}

-----------------------------------------------------------------------------
(******************************** 初始状态 **********************************)

Init ==
    /\ publishedEpoch = 0
    /\ pubResources = Resources
    /\ readerView = [r \in Readers |-> NoView]
    /\ acqEpoch = [r \in Readers |-> 0]
    /\ resourceState = [x \in Resources |-> "live"]
    /\ certified = [x \in Resources |-> FALSE]

-----------------------------------------------------------------------------
(******************************* 协议动作 ************************************)

\* acquisition 候选：正常协议只允许获取当前 published view（等价于
\* “retired 视图不再接受新进入”的闭门条件——旧读者可继续完成）。
\* M3 撤掉闭门：额外允许进入已 retired 的视图。
AcquireCandidates ==
    IF Mutation = MutationStaleEntry
    THEN {ComposeCurrent} \cup RetiredViews
    ELSE {ComposeCurrent}

AcquireView(r) ==
    /\ readerView[r] = NoView
    /\ \E v \in AcquireCandidates :
        /\ readerView' = [readerView EXCEPT ![r] = v]
        /\ acqEpoch' = [acqEpoch EXCEPT ![r] = publishedEpoch]
    /\ UNCHANGED <<publishedEpoch, pubResources, resourceState, certified>>

ReaderExit(r) ==
    /\ readerView[r] # NoView
    /\ readerView' = [readerView EXCEPT ![r] = NoView]
    /\ UNCHANGED <<publishedEpoch, pubResources, resourceState, certified, acqEpoch>>

\* Realtime-view publication（原子）：retire 旧视图（epoch 前移）+
\* 切换 published 资源表；被新视图排除的资源进入 retired。
PublishNewView ==
    /\ publishedEpoch < MaxPublications
    /\ Mutation # MutationSplitPublication
    /\ publishedEpoch' = publishedEpoch + 1
    /\ pubResources' = ResourcesAt(publishedEpoch + 1)
    /\ resourceState' =
        [x \in Resources |->
            IF x \in ExcludedBy(publishedEpoch + 1) /\ resourceState[x] = "live"
            THEN "retired"
            ELSE resourceState[x]]
    /\ UNCHANGED <<readerView, acqEpoch, certified>>

\* M2：把一次发布拆成两步——先单独切换资源表分量（不前移 epoch），
\* 产生“拓扑来自 N + 资源表来自 N+1”的可见窗口；再提交拓扑/epoch 半步。
SplitPublishResources ==
    /\ publishedEpoch < MaxPublications
    /\ Mutation = MutationSplitPublication
    /\ pubResources # ResourcesAt(publishedEpoch + 1)
    /\ pubResources' = ResourcesAt(publishedEpoch + 1)
    /\ resourceState' =
        [x \in Resources |->
            IF x \in ExcludedBy(publishedEpoch + 1) /\ resourceState[x] = "live"
            THEN "retired"
            ELSE resourceState[x]]
    /\ UNCHANGED <<publishedEpoch, readerView, acqEpoch, certified>>

SplitPublishTopology ==
    /\ publishedEpoch < MaxPublications
    /\ Mutation = MutationSplitPublication
    /\ pubResources = ResourcesAt(publishedEpoch + 1)   \* 资源表半步已提交
    /\ publishedEpoch' = publishedEpoch + 1
    /\ UNCHANGED <<pubResources, readerView, acqEpoch, resourceState, certified>>

\* quiescence 认证守卫：正常语义要求“没有任何读者仍可能解引用 x”（无论
\* 通过哪一代视图）。M4 模拟多代发布链上的记账捷径：只检查最新 retired
\* 代的读者是否退出，遗忘仍停留在更老 retired 视图里的读者。
QuiescenceGuard(x) ==
    IF Mutation = MutationForgetsOlderRetirement
    THEN /\ publishedEpoch >= 1
         /\ \A r \in Readers : readerView[r] # ViewAt(publishedEpoch - 1)
    ELSE DerefHolders(x) = {}

MarkReclaimable(x) ==
    /\ resourceState[x] = "retired"
    /\ QuiescenceGuard(x)
    /\ resourceState' = [resourceState EXCEPT ![x] = "reclaimable"]
    /\ certified' = [certified EXCEPT ![x] = TRUE]
    /\ UNCHANGED <<publishedEpoch, pubResources, readerView, acqEpoch>>

\* 物理释放：正常协议要求先经 MarkReclaimable 认证。
\* M1 跳过认证：retired 即可直接释放（发布后立即 reclaim）。
ReleaseGuard(x) ==
    IF Mutation = MutationReleaseBeforeQuiesce
    THEN resourceState[x] \in {"retired", "reclaimable"}
    ELSE resourceState[x] = "reclaimable"

ReleaseResource(x) ==
    /\ ReleaseGuard(x)
    /\ resourceState' = [resourceState EXCEPT ![x] = "released"]
    /\ UNCHANGED <<publishedEpoch, pubResources, readerView, acqEpoch, certified>>

-----------------------------------------------------------------------------
(************************** 次态关系与公平性 *********************************)

Next ==
    \/ \E r \in Readers : AcquireView(r) \/ ReaderExit(r)
    \/ PublishNewView
    \/ SplitPublishResources
    \/ SplitPublishTopology
    \/ \E x \in Resources : MarkReclaimable(x) \/ ReleaseResource(x)

\* Liveness 假设（显式声明，不偷设）：
\*   WF(ReaderExit)     —— realtime reader 不会永远停留在同一视图内
\*                        （停滞/崩溃读者属于候选机制比较维度，不在本模型内）；
\*   WF(MarkReclaimable) —— 控制侧在 quiescence 成立后最终会执行认证。
Fair ==
    /\ \A r \in Readers : WF_vars(ReaderExit(r))
    /\ \A x \in Resources : WF_vars(MarkReclaimable(x))

Spec == Init /\ [][Next]_vars /\ Fair

-----------------------------------------------------------------------------
(******************************* 类型不变式 **********************************)

TypeOK ==
    /\ ConfigSensible
    /\ publishedEpoch \in 0 .. MaxPublications
    /\ pubResources \in {ResourcesAt(e) : e \in 0 .. MaxPublications}
    /\ \A r \in Readers :
        /\ readerView[r].epoch \in -1 .. MaxPublications
        /\ readerView[r].topology \in -1 .. MaxPublications
        /\ readerView[r].resources \subseteq Resources
    /\ acqEpoch \in [Readers -> 0 .. MaxPublications]
    /\ resourceState \in [Resources -> {"live", "retired", "reclaimable", "released"}]
    /\ certified \in [Resources -> BOOLEAN]

-----------------------------------------------------------------------------
(******************************* 安全不变式 **********************************)

\* I2 — Publication Is Coherent：
\* 一次 acquisition 只能观察完整 N 或完整 N+1，不得是混合视图。
ViewIsCoherent ==
    \A r \in Readers : readerView[r] \in WellFormedViews \cup {NoView}

\* I3 — No New Reader Enters Retired View：
\* 第一合取子是本不变式被 M3 击杀的核心内容：任何持有视图的读者，其
\* acquisition 必须发生在该视图仍是 published 视图的纪元。
\* 第二合取子（epoch <= publishedEpoch）是防御条款：当前全部 mutation 下
\* 结构性不可违反（AcquireCandidates 只提供当前或已 retired 视图，且
\* publishedEpoch 单调），保留用于约束未来新增 acquisition 路径。
NoReaderAcquiresRetiredView ==
    \A r \in Readers :
        readerView[r] # NoView
            => /\ acqEpoch[r] = readerView[r].epoch
               /\ readerView[r].epoch <= publishedEpoch

\* I1 — No Reader Uses Released Resource（含 published 侧防御条款）：
\* 第一合取子是本不变式被 M1/M4 击杀的核心内容：仍可能解引用 x 的读者
\* 存在 => x 不得进入 reclaimable/released。
\* 第二合取子是防御条款：当前全部 mutation 下结构性不可违反（一切释放
\* 路径都要求先 retired，而 retired 意味着已离开 published 资源表，含 M2
\* 混合窗口——其 pubResources 已是新表）；保留用于防止未来出现“仍在
\* published 表内却被释放”的路径（新读者 acquisition 侧 UAF）。
NoReaderDereferencesReleasedResource ==
    \A x \in Resources :
        /\ (DerefHolders(x) # {} => resourceState[x] \notin {"reclaimable", "released"})
        /\ (x \in ComposeCurrent.resources => resourceState[x] # "released")

\* I4 — Release Requires Certified Quiescence：
\* 物理释放必须发生在 quiescence 认证之后（发布即可释放 = 跳过认证）。
ReleaseRequiresCertifiedQuiescence ==
    \A x \in Resources : resourceState[x] = "released" => certified[x]

-----------------------------------------------------------------------------
(***************************** Liveness 性质 ********************************)

\* I6 — Reclamation Eventually Becomes Possible（在 Fair 的两条假设下）：
\* 资源一旦 retired（被某次完成的发布排除），最终应能被认证为
\* reclaimable（或已被释放）。注意这只是“回收资格最终可获得”，
\* 不是强制立即物理释放。
ReplacementEventuallyReclaimable ==
    \A x \in Resources :
        resourceState[x] = "retired"
            ~> resourceState[x] \in {"reclaimable", "released"}

-----------------------------------------------------------------------------
(******************* 可达性探针（vacuity 反证，预期必须被违反）***************)

\* 证明合法 overlap 态可达（I5：Retirement != Reclamation，
\* retired + 旧读者仍在 = 合法状态，不是 bug；被违反 = witness 找到）
RetirementOverlapUnreachable ==
    ~ \E x \in Resources :
        resourceState[x] = "retired" /\ DerefHolders(x) # {}

\* 证明“已 quiescent 但未认证未释放”的中间态可达（NoNewEntry !=
\* Quiescent != PhysicallyFreed 三个区分在模型里都活着）
QuiescentUncertifiedUnreachable ==
    ~ \E x \in Resources :
        /\ resourceState[x] = "retired"
        /\ DerefHolders(x) = {}
        /\ publishedEpoch >= 1

\* 证明认证路径可达（MarkReclaimable 非死代码）
ReclaimableUnreachable ==
    ~ \E x \in Resources : resourceState[x] = "reclaimable"

\* 证明释放路径可达（ReleaseResource 非死代码）
ReleasedUnreachable ==
    ~ \E x \in Resources : resourceState[x] = "released"

=============================================================================
\* #### EOF ####
