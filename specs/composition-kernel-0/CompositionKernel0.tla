--------------------------- MODULE CompositionKernel0 ---------------------------
(***************************************************************************)
(* CompositionKernel0 — K0 control plane 语义交错模型（FV-TEMP-0, #122）    *)
(*                                                                         *)
(* STATUS:                                                                 *)
(*   FORMAL EVIDENCE（本轮挣得）。本模型是 executable evidence，            *)
(*   不是第二份 normative authority。K0 语义 authority 仍是                 *)
(*   docs/architecture/composition-kernel-0-design.md（§F/§E.3/§E.4/       *)
(*   §G/§G.6/§L.1/§L.5）；representation decisions 仍在其 implementation    *)
(*   ADR。verifier-only 变量（inversesRun / activations /                  *)
(*   ghostRemovedOwed / envStopped / StopEnv / Mutation）按治理规则         *)
(*   （AGENTS.md "Verification authority boundary"）保持 non-normative，    *)
(*   不得进入 Rust / ADR 词汇。                                            *)
(*                                                                         *)
(* TARGET（#122 gate contract）:                                           *)
(*   K0 step/settle（synchronous serialized control plane）× 独立合法的     *)
(*   外部语义事件：desired revision（provider withdrawal / replacement     *)
(*   churn / 重新启用）、dispose_root（= desired 收敛到空集）、以及          *)
(*   bounded activation 的两种 legal 结局（自然完成 / raise，B19）。        *)
(*                                                                         *)
(* TOOL CHOICE:                                                            *)
(*   被挑战的性质是「独立合法语义事件的时序交错」—— withdraw × dispose ×    *)
(*   replacement churn × raise × 违约注入 在 settle 各 step 边界上的碰撞。  *)
(*   这正是 ADR §13 formalization policy 点名的候选形态；Kani/Loom 不覆盖   *)
(*   语义级交错穷举。                                                      *)
(*                                                                         *)
(* PRODUCTION MAPPING（model concept ↔ ADR concept ↔ current Rust）:        *)
(*   present/state/retired/rev/committed/effects                           *)
(*       ↔ Fiber (§D.3) ↔ crates/qianqian-composition/src/fiber.rs         *)
(*         Fiber { id, name, component, revision, retired, state,          *)
(*                 teardown_violated, pending_error, committed, effects }  *)
(*   state 值 pending/active/unloading/failed ↔ FiberState 同名             *)
(*         （"activating" 不出现在模型：K0 activation 是一个 bounded        *)
(*          atomic step（B3/B20，kernel.rs activate_fiber 一步完成），      *)
(*          step 边界之间永远观察不到 Activating。）                        *)
(*   committed[f] = p   ↔ Fiber.committed 的 committed view（episode-fixed, *)
(*                        B14 teardown window）；NoOne = 无开放 episode      *)
(*   effects[f]         ↔ owned-effect accumulator 深度（每个 episode        *)
(*                        注册一个 reversible effect 的计数抽象）            *)
(*   inversesRun[f]     ↔ 已消费 inverse 计数（verifier-only；支撑           *)
(*                        "exactly once" 的计数论证）                       *)
(*   ReliedOn(p)        ↔ kernel.rs relied_on()（§G L-Unload guard ¬relied）*)
(*   UnloadClose        ↔ kernel.rs step() rule 1 eligible_unload() +        *)
(*                        unload_fiber()（run_unwind LIFO + verdict +       *)
(*                        episode close）                                   *)
(*   Divert             ↔ step() rule 2 divert_candidate()（L-Leave）        *)
(*   Retire             ↔ step() rule 3 retire_mismatch_candidate()（§L.5    *)
(*                        revision identity / orphan / disabled）           *)
(*   Remove             ↔ step() rule 4 removal_candidate()（O-Remove）      *)
(*   Mount              ↔ step() rule 5 mount_candidate()（O-Insert；       *)
(*                        §E.4 staged replacement 的 step 7）               *)
(*   Activate           ↔ step() rule 6 activation_candidate() +            *)
(*                        activate_fiber()（L-Begin freeze view；complete / *)
(*                        raise 两种 legal 结局；raise 的 partial unwind    *)
(*                        在同一 step 内原子完成）                          *)
(*   SetDesiredP1/P2(r) ↔ Host 提交新 desired composition（kernel.rs         *)
(*                        set_desired()；provider withdrawal / replacement  *)
(*                        churn 经 §L.5 revision identity 表达）            *)
(*   SetDesiredEmpty    ↔ dispose_root()（kernel.rs: set_desired({}) +      *)
(*                        settle()——模型把 drain 交给同一组 kernel step）   *)
(*   StopEnv            ↔ 环境事件最终停止（liveness 条件前提，显式声明）    *)
(*   violated[f]        ↔ TEARDOWN_VIOLATED latch（§G.6，diagnostic flag，   *)
(*                        不是第八个状态）                                  *)
(*   pendingError[f]    ↔ pending activation error（episode metadata，§F.5）*)
(*   step 优先级        ↔ kernel.rs step() 的 1..6 if-chain 逐条镜像；       *)
(*                        §E.4 单一 provider 点不变式由该优先级涌现，        *)
(*                        本模型把它作为不变式检验而非假设。                *)
(*                                                                         *)
(* 模型假设（BOUNDS / ASSUMPTIONS，全部显式）：                             *)
(*   A1 3 fibers：consumer C requires {K}；P1/P2 各 provides {K}           *)
(*      （#122 建议的 bounded small instance；无链式二级依赖）。            *)
(*   A2 desired 只由 3 个 legal 形状之一给出（P1-active / P2-active /      *)
(*      empty），revision ∈ 0..MaxRevision；环境提交 desired revision 的    *)
(*      总次数 ≤ MaxRevisions（bounded churn window）。非法 desired（双     *)
(*      provider、环）在 Rust set_desired() plan-time 被拒绝（§L.4），       *)
(*      不在模型内。mutation run 用更紧的界（见 mutations/*.cfg 头注）。    *)
(*   A3 每 fiber 每 episode 注册 1 个 effect（effects ≤ MaxEffects）；     *)
(*      每 fiber generation 的 activation 总数 ≤ MaxActivations            *)
(*      （bounded churn window；超出后 Activate 在界内不可用——              *)
(*      bounded-model 截断，计入 BOUNDS，不声称覆盖无限 churn）。           *)
(*   A4 违约注入：unwind 中 inverse 返回 Violated 是合法的模型分支          *)
(*      ——它建模「defective component」下 kernel 的 §G.6 处理语义，         *)
(*      不是说 legal K0 inverse 可以违约（§H.7 totality 是组件契约）。      *)
(*   A5 liveness 公平性：WF_vars(KernelActions)（控制面持续前进）+          *)
(*      环境事件有限性经 StopEnv 显式建模。没有比这更强的 fairness         *)
(*      被假设；结果随 RESULTS.md 的 FAIRNESS 条目一起报告。               *)
(*                                                                         *)
(* NEGATIVE CONTROLS（Mutation 常量，逐 cfg 独立注入，均必须产生            *)
(* counterexample，否则 VERDICT = TOOLING-FAIL）：                         *)
(*   MutationDropReliedGuard          M1 卸载忽略 ¬relied guard（§G）       *)
(*   MutationRemoveBeforeDischarge    M2 effect 未清空即可移除（§K.4/Cor69）*)
(*   MutationEarlyReplacement         M3 旧代未移除即挂载新 provider        *)
(*                                    （§E.4 staging 优先级被撤）           *)
(*   MutationDoubleInverse            M4 违约 tombstone 被再次执行（§G.6）  *)
(*   MutationMountOverViolation       M5 撤掉 §G.6 违约边 guard —— 这不是   *)
(*                                    假想缺陷，而是当前 kernel.rs          *)
(*                                    mount_candidate 的真实行为；其        *)
(*                                    counterexample 即生产缺陷的可执行     *)
(*                                    复现（见 RESULTS.md differential）。  *)
(***************************************************************************)

EXTENDS Integers, FiniteSets

CONSTANTS
    \* 三个 desired entry id（= fiber 名；名字移除后才可复用，§F.1）
    Entries, C, P1, P2,
    \* 唯一被建模的 capability（§E.2 required-single）
    K,
    \* 「无开放 episode」哨兵
    NoOne,
    \* 负控制开关（语义名，稳定，不含 issue/PR 词汇）
    Mutation,
    MutationNone,
    MutationDropReliedGuard,
    MutationRemoveBeforeDischarge,
    MutationEarlyReplacement,
    MutationDoubleInverse,
    MutationMountOverViolation,
    \* 界
    MaxActivations,
    MaxEffects,
    MaxRevision,
    MaxRevisions

MutationChoices ==
    {MutationNone, MutationDropReliedGuard, MutationRemoveBeforeDischarge,
     MutationEarlyReplacement, MutationDoubleInverse,
     MutationMountOverViolation}

ASSUME /\ Entries = {C, P1, P2}
       /\ Mutation \in MutationChoices
       /\ MaxActivations \in 1..3
       /\ MaxEffects = 1
       /\ MaxRevision \in {1, 2}
       /\ MaxRevisions \in 2..6

VARIABLES
    \* ---- Reconcile 输入（desired composition，§L.1）----
    desiredEnabled,   \* [Entries -> BOOLEAN]
    desiredRev,       \* [Entries -> 0..MaxRevision]（§L.5 desired revision identity）
    \* ---- running registry（fiber 真相，§D.3 / fiber.rs）----
    present,          \* [Entries -> BOOLEAN] 已安装（registry 里有 entry）
    state,            \* [Entries -> FiberState]
    retired,          \* [Entries -> BOOLEAN]
    rev,              \* [Entries -> 0..MaxRevision] 运行代的 revision identity
    committed,        \* [Entries -> Entries ∪ {NoOne}] episode-fixed committed view
    effects,          \* [Entries -> 0..MaxEffects] owned-effect accumulator 深度
    pendingError,     \* [Entries -> BOOLEAN] pending activation error（§F.5 episode 元数据）
    violated,         \* [Entries -> BOOLEAN] TEARDOWN_VIOLATED latch（§G.6 诊断 flag）
    \* ---- verifier-only（non-normative，见头注）----
    inversesRun,      \* [Entries -> 0..MaxActivations]
    activations,      \* [Entries -> 0..MaxActivations]
    ghostRemovedOwed, \* [Entries -> BOOLEAN] 移除时仍欠 discharge 的历史记录
    \* ---- 环境 ----
    \* envStopped：环境不再提交新的语义事件
    \* envRevisionsUsed：0..MaxRevisions，已提交的 desired revision 数
    \*   （bounded churn window，模型假设 A2'；超出后 SetDesired* 不可用）
    envStopped,
    envRevisionsUsed

FiberState == {"absent", "pending", "active", "unloading", "failed"}

vars == <<desiredEnabled, desiredRev, present, state, retired, rev,
          committed, effects, inversesRun, activations, pendingError,
          violated, ghostRemovedOwed, envStopped, envRevisionsUsed>>

NoChange == UNCHANGED <<desiredEnabled, desiredRev, present, state, retired,
          rev, committed, effects, inversesRun, activations, pendingError,
          violated, ghostRemovedOwed, envStopped, envRevisionsUsed>>

-----------------------------------------------------------------------------
(************************** 派生真相（registry truth）************************)

\* 提供方 fiber（唯一 capability K 的 provider 候选）
Providers == {P1, P2}

\* §E.3 new-resolution eligibility：ACTIVE 且当前提供 K
ActiveProvidersOfK == {f \in Providers : present[f] /\ state[f] = "active"}

Requires(f) == IF f = C THEN {K} ELSE {}
Provides(f)  == IF f \in Providers THEN {K} ELSE {}

\* §L.4 activation readiness：每个 required key 恰有一个 active provider
Ready(f) == \A k \in Requires(f) : Cardinality(ActiveProvidersOfK) = 1

\* §G/B14：committed view 仍点名 p（episode 开放）
ReliedOn(p) ==
    \E g \in Entries : g # p /\ present[g] /\ committed[g] = p

\* §G.6：违约 fiber 的 episode 不关闭 → 它点名的 provider guard 保持 latched
CommittedStale(f) ==
    committed[f] # NoOne /\ ActiveProvidersOfK # {committed[f]}

\* §L.2/§L.5：desired 差异 → 该运行代必须 retire
Mismatch(f) == \/ ~desiredEnabled[f]
               \/ desiredRev[f] # rev[f]

DesiredAllDisabled == \A f \in Entries : ~desiredEnabled[f]

AnyViolated == \E f \in Entries : violated[f]

-----------------------------------------------------------------------------
(******************************** 初始状态 **********************************)

Init ==
    /\ desiredEnabled = [f \in Entries |-> f # P2]
    /\ desiredRev = [f \in Entries |-> 0]
    /\ present = [f \in Entries |-> FALSE]
    /\ state = [f \in Entries |-> "absent"]
    /\ retired = [f \in Entries |-> FALSE]
    /\ rev = [f \in Entries |-> 0]
    /\ committed = [f \in Entries |-> NoOne]
    /\ effects = [f \in Entries |-> 0]
    /\ inversesRun = [f \in Entries |-> 0]
    /\ activations = [f \in Entries |-> 0]
    /\ pendingError = [f \in Entries |-> FALSE]
    /\ violated = [f \in Entries |-> FALSE]
    /\ ghostRemovedOwed = [f \in Entries |-> FALSE]
    /\ envStopped = FALSE
    /\ envRevisionsUsed = 0

-----------------------------------------------------------------------------
(***************************** 环境动作（Host）*******************************)

\* Host 提交新 desired composition（set_desired）。仅呈现 legal 形状：
\* plan-time 检查（§L.4）在 Rust 拒绝其余——模型假设 A2。revision 变化
\* 表达「fresh desired incarnation」= replacement churn / 重新启用 / 撤回。

SetDesiredP1(r) ==
    /\ ~envStopped
    /\ envRevisionsUsed < MaxRevisions      \* bounded churn window（A2'）
    /\ r \in 0..MaxRevision
    /\ desiredEnabled' = [desiredEnabled EXCEPT ![C] = TRUE, ![P1] = TRUE, ![P2] = FALSE]
    /\ desiredRev' = [desiredRev EXCEPT ![P1] = r]
    /\ envRevisionsUsed' = envRevisionsUsed + 1
    /\ UNCHANGED <<present, state, retired, rev, committed, effects,
                   inversesRun, activations, pendingError, violated,
                   ghostRemovedOwed, envStopped>>

SetDesiredP2(r) ==
    /\ ~envStopped
    /\ envRevisionsUsed < MaxRevisions
    /\ r \in 0..MaxRevision
    /\ desiredEnabled' = [desiredEnabled EXCEPT ![C] = TRUE, ![P1] = FALSE, ![P2] = TRUE]
    /\ desiredRev' = [desiredRev EXCEPT ![P2] = r]
    /\ envRevisionsUsed' = envRevisionsUsed + 1
    /\ UNCHANGED <<present, state, retired, rev, committed, effects,
                   inversesRun, activations, pendingError, violated,
                   ghostRemovedOwed, envStopped>>

\* dispose_root 的语义核（kernel.rs: set_desired(空) + settle——drain 由
\* 同一组 kernel step 承担）
SetDesiredEmpty ==
    /\ ~envStopped
    /\ envRevisionsUsed < MaxRevisions
    /\ desiredEnabled' = [f \in Entries |-> FALSE]
    /\ envRevisionsUsed' = envRevisionsUsed + 1
    /\ UNCHANGED <<desiredRev, present, state, retired, rev, committed,
                   effects, inversesRun, activations, pendingError,
                   violated, ghostRemovedOwed, envStopped>>

StopEnv ==
    /\ ~envStopped
    /\ envStopped' = TRUE
    /\ UNCHANGED <<desiredEnabled, desiredRev, present, state, retired,
                   rev, committed, effects, inversesRun, activations,
                   pendingError, violated, ghostRemovedOwed, envRevisionsUsed>>

-----------------------------------------------------------------------------
(************************** kernel step 候选（§F.3）**************************)
(* 以下 Enabled* 逐条镜像 kernel.rs step() 的候选检查（1..6 同序）。        *)
(* 系统级「还有没有 step 可做」= ¬SystemStable。                            *)

EnabledUnloadClose(f) ==   \* rule 1：eligible_unload（¬relied guard，§G）
    /\ present[f] /\ state[f] = "unloading"
    /\ ~violated[f]                     \* §G.6：违约 latch 无出口
    /\ ~ReliedOn(f)

EnabledDivert(f) ==        \* rule 2：divert_candidate（L-Leave）
    /\ present[f] /\ state[f] = "active"
    /\ retired[f] \/ CommittedStale(f)

EnabledRetire(f) ==        \* rule 3：retire_mismatch_candidate（§L.5）
    /\ present[f] /\ ~retired[f]
    /\ state[f] # "unloading"           \* （Rust 还排除 Activating；模型无此态）
    /\ Mismatch(f)

EnabledRemove(f) ==        \* rule 4：removal_candidate（O-Remove；Thm 64/Cor 69）
    /\ present[f] /\ retired[f]
    /\ ~violated[f]
    /\ committed[f] = NoOne
    /\ effects[f] = 0
    /\ state[f] \in {"pending", "failed"}

\* §E.4 挂载重叠守卫（authority 语义，默认生效）：B12/E.4 的 O-Insert
\* disjointness 量化于所有 installed fibers 的 declared provisions——
\* "An Unloading old fiber is still installed, so inserting an overlapping
\* new provider before the old is removed would violate the registry
\* invariant outright"。clean 路径由 step 优先级（rule 4 先于 rule 5）保证
\* 该前提；「§G.6 违约 latch 使旧代永远无法到达移除」的路径（consumer 或
\* provider 自身违约，两种 trace 都已找到）则必须由 mount 候选检查显式
\* 把关，否则点不变式被打破。
\* FV-TEMP-0 differential：当前 kernel.rs mount_candidate 只查同名 fiber，
\* 不查 capability 重叠 —— M5 = MutationMountOverViolation 仅撤该守卫
\* （保留 staging 优先级，= 当前 Rust 行为）复现缺陷（见 RESULTS.md）；
\* M3 = MutationEarlyReplacement 把 staging 优先级与重叠守卫一并撤掉
\* （经典「replacement 未 staged」回归）。
MountOverlapBlock(e) ==
    /\ Mutation \notin {MutationEarlyReplacement, MutationMountOverViolation}
    /\ \E f \in Entries :
          present[f] /\ Provides(f) \cap Provides(e) # {}

EnabledMount(e) ==         \* rule 5：mount_candidate（O-Insert）
    /\ desiredEnabled[e]
    /\ ~present[e]
    /\ ~MountOverlapBlock(e)

EnabledActivate(f) ==      \* rule 6：activation_candidate（L-Begin）
    /\ present[f] /\ state[f] = "pending"
    /\ ~retired[f]
    /\ Ready(f)
    /\ activations[f] < MaxActivations   \* bounded-model 截断（A3）

AnyUnload  == \E f \in Entries : EnabledUnloadClose(f)
AnyDivert  == \E f \in Entries : EnabledDivert(f)
AnyRetire  == \E f \in Entries : EnabledRetire(f)
AnyRemove  == \E f \in Entries : EnabledRemove(f)
AnyMount   == \E e \in Entries : EnabledMount(e)

\* SystemStable = 没有任何 enabled kernel transition（step() 的 Settled/Blocked
\* 边界）。bounded 截断计入 BOUNDS。
SystemStable ==
    /\ ~AnyUnload /\ ~AnyDivert /\ ~AnyRetire /\ ~AnyRemove /\ ~AnyMount
    /\ ~\E f \in Entries : EnabledActivate(f)

QuietNow == SystemStable /\ ~AnyViolated   \* §L.1（quiet ≠ healthy ≠ successful）

-----------------------------------------------------------------------------
(***************************** kernel 转移动作 *******************************)

\* ---- rule 1：卸载收尾（L-Unload）：LIFO unwind + verdict + episode close ----
UnloadClose(f) ==
    /\ \/ EnabledUnloadClose(f)
       \/ /\ Mutation = MutationDropReliedGuard        \* M1：撤掉 ¬relied guard
          /\ present[f] /\ state[f] = "unloading" /\ ~violated[f]
       \/ /\ Mutation = MutationDoubleInverse          \* M4：撤掉 §G.6 无出口
          /\ present[f] /\ state[f] = "unloading"
          /\ violated[f] /\ ~ReliedOn(f)
    /\ IF violated[f] /\ Mutation = MutationDoubleInverse
       THEN \* 违约 tombstone 的 inverse 被第二次执行（模型缺陷注入）
            /\ effects' = [effects EXCEPT ![f] = effects[f] - 1]
            /\ inversesRun' = [inversesRun EXCEPT ![f] = inversesRun[f] + 1]
            /\ state' = [state EXCEPT ![f] = "pending"]
            /\ committed' = [committed EXCEPT ![f] = NoOne]
            /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired,
                           rev, activations, pendingError, violated,
                           ghostRemovedOwed, envStopped, envRevisionsUsed>>
       ELSE \* 正常路径（violated[f] = FALSE）
            \/ \* 完全 discharge：episode 关闭（committed view 最后丢弃，§F.3）
               /\ effects' = [effects EXCEPT ![f] = 0]
               /\ inversesRun' = [inversesRun EXCEPT ![f] =
                                     inversesRun[f] + effects[f]]
               /\ committed' = [committed EXCEPT ![f] = NoOne]
               /\ state' = [state EXCEPT ![f] = "pending"]
               /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired,
                              rev, activations, pendingError, violated,
                              ghostRemovedOwed, envStopped, envRevisionsUsed>>
            \/ \* §G.6 违约：inverse 返回 Violated —— episode 不关闭；
               \* tombstone（含其下未 discharge 的 effect）留在 accumulator，
               \* TEARDOWN_VIOLATED latch，guard 保持 latched
               /\ effects[f] >= 1
               /\ violated' = [violated EXCEPT ![f] = TRUE]
               /\ inversesRun' = [inversesRun EXCEPT ![f] = inversesRun[f] + 1]
               /\ UNCHANGED <<desiredEnabled, desiredRev, present, state,
                              retired, rev, committed, effects, activations,
                              pendingError, ghostRemovedOwed, envStopped, envRevisionsUsed>>

\* ---- rule 2：divert / retire 生效（L-Leave）：provisions 离开 σ_γ ----
Divert(f) ==
    /\ EnabledDivert(f)
    /\ ~AnyUnload
    /\ state' = [state EXCEPT ![f] = "unloading"]
    /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired, rev,
                   committed, effects, inversesRun, activations,
                   pendingError, violated, ghostRemovedOwed, envStopped, envRevisionsUsed>>

\* ---- rule 3：retire 标记（revision / orphan / disabled，§L.5）----
Retire(f) ==
    /\ EnabledRetire(f)
    /\ ~AnyUnload /\ ~AnyDivert
    /\ retired' = [retired EXCEPT ![f] = TRUE]
    /\ UNCHANGED <<desiredEnabled, desiredRev, present, state, rev,
                   committed, effects, inversesRun, activations,
                   pendingError, violated, ghostRemovedOwed, envStopped, envRevisionsUsed>>

\* ---- rule 4：移除已 drain 的退休代（O-Remove）。正常守卫 = Thm 64/Cor 69
\* 的完整前提：retired ∧ ¬violated ∧ 无开放 view ∧ accumulator 清空 ∧
\* Inactive-家族状态。M2 把守卫撤到只剩 retired（「移除纪律整体被撤」，
\* 覆盖 removed-before-discharge 的全部可达形态）。----
Remove(f) ==
    /\ present[f]
    /\ retired[f]
    /\ (EnabledRemove(f) \/ Mutation = MutationRemoveBeforeDischarge)
    /\ ~AnyUnload /\ ~AnyDivert /\ ~AnyRetire
    /\ ghostRemovedOwed' = [ghostRemovedOwed EXCEPT ![f] =
          ghostRemovedOwed[f] \/ effects[f] > 0
                             \/ inversesRun[f] # activations[f]]
    /\ present' = [present EXCEPT ![f] = FALSE]
    /\ state' = [state EXCEPT ![f] = "absent"]
    /\ committed' = [committed EXCEPT ![f] = NoOne]
    /\ UNCHANGED <<desiredEnabled, desiredRev, retired, rev, effects,
                   inversesRun, activations, pendingError, violated,
                   envStopped, envRevisionsUsed>>

\* ---- rule 5：挂载（O-Insert）。§E.4：旧 provider 代移除之后才允许——
\* 该 staging 由 step 优先级（rule 1..5 次序）涌现，不是模型假设。
\* M3 撤掉该优先级守卫；M5（MutationMountOverViolation）撤掉 §G.6 违约边
\* 守卫（= 当前 Rust 行为，见头注 differential）。
Mount(e) ==
    /\ EnabledMount(e)
    /\ ~MountOverlapBlock(e)
    /\ \/ /\ ~AnyUnload /\ ~AnyDivert /\ ~AnyRetire /\ ~AnyRemove
       \/ Mutation = MutationEarlyReplacement
    /\ present' = [present EXCEPT ![e] = TRUE]
    /\ state' = [state EXCEPT ![e] = "pending"]
    /\ retired' = [retired EXCEPT ![e] = FALSE]
    /\ rev' = [rev EXCEPT ![e] = desiredRev[e]]
    /\ committed' = [committed EXCEPT ![e] = NoOne]
    /\ effects' = [effects EXCEPT ![e] = 0]
    /\ inversesRun' = [inversesRun EXCEPT ![e] = 0]
    /\ activations' = [activations EXCEPT ![e] = 0]
    /\ pendingError' = [pendingError EXCEPT ![e] = FALSE]
    /\ violated' = [violated EXCEPT ![e] = FALSE]
    /\ ghostRemovedOwed' = [ghostRemovedOwed EXCEPT ![e] = FALSE]
    /\ UNCHANGED <<desiredEnabled, desiredRev, envStopped, envRevisionsUsed>>

\* ---- rule 6：activation（L-Begin，一个 bounded atomic step，B3/B20）。
\* committed view 冻结；两种 legal 结局：自然完成 / raise（B19）。
\* 完成路径上可注入「dispose 违约」（A21：§G.6 ⇒ 永不 Active）；
\* raise 路径上 partial unwind 原子完成：clean ⇒ FAILED；违约 ⇒ latch。----
Activate(f) ==
    /\ EnabledActivate(f)
    /\ ~AnyUnload /\ ~AnyDivert /\ ~AnyRetire /\ ~AnyRemove /\ ~AnyMount
    /\ effects[f] < MaxEffects
    \* L-Begin：冻结 committed view（只 commit 到 ACTIVE provider，§E.3）
    /\ committed' = [committed EXCEPT ![f] =
          IF Requires(f) = {} THEN NoOne
          ELSE CHOOSE p \in ActiveProvidersOfK : TRUE]
    /\ activations' = [activations EXCEPT ![f] = activations[f] + 1]
    /\ \/ \* 自然完成：effects owned；provisions installed（§F.3）
         /\ state' = [state EXCEPT ![f] = "active"]
         /\ effects' = [effects EXCEPT ![f] = effects[f] + 1]
         /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired, rev,
                        inversesRun, pendingError, violated,
                        ghostRemovedOwed, envStopped, envRevisionsUsed>>
      \/ \* 完成但 dispose 违约已 latch（A21）：§G.6 ⇒ 永不 Active，
         \* episode 开放、无 unwind、无 close
         /\ state' = [state EXCEPT ![f] = "unloading"]
         /\ violated' = [violated EXCEPT ![f] = TRUE]
         /\ effects' = [effects EXCEPT ![f] = effects[f] + 1]
         /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired, rev,
                        inversesRun, pendingError, ghostRemovedOwed, envStopped, envRevisionsUsed>>
      \/ \* raise，partial unwind 清洁完成 ⇒ FAILED（§F.3/F.5；无 auto retry）。
         \* 完全 discharge 关闭 episode：committed view 丢弃（kernel.rs:
         \* activate_fiber raise 路径 f.committed = None）。
         /\ state' = [state EXCEPT ![f] = "failed"]
         /\ committed' = [committed EXCEPT ![f] = NoOne]
         /\ pendingError' = [pendingError EXCEPT ![f] = TRUE]
         /\ inversesRun' = [inversesRun EXCEPT ![f] = inversesRun[f] + 1]
         /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired, rev,
                        effects, violated, ghostRemovedOwed, envStopped, envRevisionsUsed>>
      \/ \* raise 且 partial unwind 违约 ⇒ latched Unloading + tombstone，
         \* FAILED 不可达（§G.6）；pending activation error 留作 episode 元数据
         /\ state' = [state EXCEPT ![f] = "unloading"]
         /\ pendingError' = [pendingError EXCEPT ![f] = TRUE]
         /\ violated' = [violated EXCEPT ![f] = TRUE]
         /\ effects' = [effects EXCEPT ![f] = effects[f] + 1]
         /\ inversesRun' = [inversesRun EXCEPT ![f] = inversesRun[f] + 1]
         /\ UNCHANGED <<desiredEnabled, desiredRev, present, retired, rev,
                        ghostRemovedOwed, envStopped, envRevisionsUsed>>

-----------------------------------------------------------------------------
(************************** 次态关系与公平性 *********************************)

\* 合法停机（settle 的 Settled/Blocked 终态）：环境已停且无 enabled
\* transition。除此之外的死锁仍是模型缺陷，保持被检测。
TerminalStutter == envStopped /\ SystemStable /\ UNCHANGED vars

Next ==
    \/ SetDesiredP1(0) \/ SetDesiredP1(1)
    \/ SetDesiredP2(0) \/ SetDesiredP2(1)
    \/ SetDesiredEmpty
    \/ StopEnv
    \/ \E f \in Entries :
           UnloadClose(f) \/ Divert(f) \/ Retire(f) \/ Remove(f) \/ Activate(f)
    \/ \E e \in Entries : Mount(e)
    \/ TerminalStutter

\* Liveness 假设（显式声明，不偷设，A5）：serialized 控制面只要还有
\* enabled transition 就最终前进（kernel.rs settle() 的驱动假设 + Thm 73
\* 的进展前提）。环境事件有限性由 StopEnv 显式建模。
KernelActions ==
    \/ \E f \in Entries : UnloadClose(f) \/ Divert(f) \/ Retire(f)
           \/ Remove(f) \/ Activate(f)
    \/ \E e \in Entries : Mount(e)

Fair == WF_vars(KernelActions)

Spec == Init /\ [][Next]_vars /\ Fair

-----------------------------------------------------------------------------
(******************************* 类型不变式 **********************************)

TypeOK ==
    /\ desiredEnabled \in [Entries -> BOOLEAN]
    /\ desiredRev \in [Entries -> 0..MaxRevision]
    /\ present \in [Entries -> BOOLEAN]
    /\ state \in [Entries -> FiberState]
    /\ retired \in [Entries -> BOOLEAN]
    /\ rev \in [Entries -> 0..MaxRevision]
    /\ committed \in [Entries -> Entries \cup {NoOne}]
    /\ effects \in [Entries -> 0..MaxEffects]
    /\ inversesRun \in [Entries -> 0..MaxActivations]
    /\ activations \in [Entries -> 0..MaxActivations]
    /\ pendingError \in [Entries -> BOOLEAN]
    /\ violated \in [Entries -> BOOLEAN]
    /\ ghostRemovedOwed \in [Entries -> BOOLEAN]
    /\ envStopped \in BOOLEAN
    /\ envRevisionsUsed \in 0..MaxRevisions
    /\ \A f \in Entries :
          present[f] => state[f] # "absent"

-----------------------------------------------------------------------------
(******************************* 安全不变式 **********************************)

\* P4a（§E.4 点不变式 / B12 O-Insert disjointness）：任一时刻（不只
\* quiescence），registry 中声明提供同一 capability 的 installed fiber 至多
\* 一个。本模型里 P1/P2 都提供唯一的 K。
SingleSource == Cardinality({f \in Providers : present[f]}) <= 1

\* P1（§G relied_on guard）：凡有 open committed view 点名 p，p 的 episode
\* 不得已关闭、也不得被移除（只能处于 active / unloading 等待窗口）。
ReliedGuard ==
    \A p \in Entries :
        ReliedOn(p) => present[p] /\ state[p] \in {"active", "unloading"}

\* P2a（§H.1/§D.5「inverse 恰好一次」的计数论证）：每个已注册 effect 的
\* inverse 至多被执行一次。
InverseOnce == \A f \in Entries : inversesRun[f] <= activations[f]

\* P2b（§G.6/§K.4 tombstone）：违约 latch 后，fiber 保持 installed、episode
\* 开放、provenance tombstone（≥1 个 effect 记录）留在 accumulator——
\* 绑定不从未被假装清洁。
TombstoneRetained ==
    \A f \in Entries :
        violated[f] => present[f] /\ state[f] = "unloading" /\ effects[f] >= 1

\* P3（Cor 69/Thm 64 移除纪律）：任何移除都不得发生在 accumulator 未清空、
\* 或存在未消费 inverse 的状态下（ghost 历史 flag，verifier-only）。
NoRemovalOwing == \A f \in Entries : ~ghostRemovedOwed[f]

\* §G.6：违约 fiber 的开放 episode 使其点名的 provider guard 保持 latched
\* （provider 不得 final-release——由 ReliedGuard 联合强制）。
ViolatedKeepsGuard ==
    \A g \in Entries :
        violated[g] /\ committed[g] # NoOne => ReliedOn(committed[g])

\* B19/§F.5：FAILED 只能由带 pending activation error 的 raise 路径到达。
FailedHasPendingError ==
    \A f \in Entries : state[f] = "failed" => pendingError[f]

-----------------------------------------------------------------------------
(***************************** Liveness 性质 ********************************)

\* L1（§1.3a：settle 终止 / 不死循环）：环境停止提交事件后，控制面最终
\* 到达「无 enabled transition」（Settled 或 Blocked）。
L_STABLE == envStopped ~> SystemStable

\* L2（§G 收敛 / Thm 73 进展——含 §G.6 的显式放弃）：任何进入 Unloading
\* 的 fiber，其 episode 最终关闭（回 Pending / FAILED）、或其自身违约
\* latch、或被某个违约 fiber 跨边合法阻塞（§G.6：违约 fiber 的开放
\* committed view 使 ¬relied guard 永久 latched——「the guard stays
\* latched; the provider stays alive」；Thm 73 的进展前提 presupposes
\* inverses complete，违约组件放弃该进展是设计行为不是 bug）。本模型
\* 拓扑下阻塞深度为 1（唯一 consumer 是 C）。
BlockedByViolation(f) ==
    \E g \in Entries : g # f /\ violated[g] /\ committed[g] = f

L_UNLOAD ==
    \A f \in Entries :
        (present[f] /\ state[f] = "unloading")
            ~> (state[f] \in {"pending", "failed"}
                \/ violated[f]
                \/ BlockedByViolation(f))

-----------------------------------------------------------------------------
(******************* 可达性探针（vacuity 反证，预期必须被违反）***************)

\* §E.4 staging 窗口真实可达：旧代 retired 且 Unloading、尚未被移除
ProbeStagingWindowUnreachable ==
    ~ \E f \in Entries : present[f] /\ retired[f] /\ state[f] = "unloading"

\* §G.6 违约 latch 真实可达
ProbeViolatedLatchUnreachable ==
    ~ \E f \in Entries : violated[f]

\* §G.6 Scenario A：违约 consumer 的开放 view 把 provider 的 final-release
\* guard latched 在 Unloading
ProbeGuardLatchedUnreachable ==
    ~ (violated[C] /\ committed[C] = P1 /\ present[P1]
       /\ state[P1] = "unloading" /\ ReliedOn(P1))

\* §L.1 条款 3 / D1 oracle：FAILED 是 settled、quiet-legal 状态
ProbeFailedQuietUnreachable ==
    ~ (state[P1] = "failed" /\ QuietNow /\ desiredEnabled[P1])

\* §E.4/M2：staged replacement 完整走完 + consumer 对新代 re-commit
ProbeReplacementCompleteUnreachable ==
    ~ (present[P2] /\ state[P2] = "active" /\ state[C] = "active"
       /\ committed[C] = P2 /\ ~present[P1])

\* dispose_root 收敛：空 desired + quiet + 空 registry（§L.2 root disposal）
ProbeDisposeConvergenceUnreachable ==
    ~ (DesiredAllDisabled /\ QuietNow
       /\ \A f \in Entries : ~present[f])

=============================================================================
\* #### EOF ####
