# FV-TEMP-0 RESULTS — K0 控制面 TLA+/TLC（#122）

STATUS: FORMAL EVIDENCE（#124 truth class: evidence，非 authority）。

> **RESULT: BOUNDED-CLEAN（safety + liveness）+ 1 个 RESOLVED DIFFERENTIAL
> （M5 反例 = pre-#126 生产缺陷；已由 #126 确认并修复，见 §2 RESOLUTION）**
> BOUNDED-CLEAN 指在下方显式 BOUNDS/FAIRNESS 内未找到反例；它**不是**
> "architecture proven correct"，不构成 acceptance。

运行方式：`specs/composition-kernel-0/check.sh`（fail-closed runner；基线
PASS + 全部 mutation 必须反例 + 全部 probe 必须 witness，任一 TLC warning
即 FAIL）。

```text
== baseline（必须全部 PASS：invariants + temporal properties）
K0 / baseline safety                                 PASS  263314 states generated, 104816 distinct states found, 0 states left on queue.
K0 / baseline liveness（WF(KernelActions)）        PASS  263314 states generated, 104816 distinct states found, 0 states left on queue.
== 负控制 mutations（必须产生 counterexample）
Mutation / M1 DropReliedGuard                        MUST-FAIL-OK（违反 ReliedGuard）
Mutation / M2 RemoveBeforeDischarge                  MUST-FAIL-OK（违反 NoRemovalOwing）
Mutation / M3 EarlyReplacement                       MUST-FAIL-OK（违反 SingleSource）
Mutation / M4 DoubleInverse                          MUST-FAIL-OK（违反 InverseOnce）
Mutation / M5 MountOverViolation（pre-#126）    MUST-FAIL-OK（违反 SingleSource）
== 可达性探针（正向控制：witness 必须找到）
Probe / §E.4 staging 窗口                         MUST-FAIL-OK
Probe / §G.6 违约 latch                           MUST-FAIL-OK
Probe / §G.6 Scenario A guard latch                 MUST-FAIL-OK
Probe / §L.1 条款3 FAILED quiet-legal             MUST-FAIL-OK
Probe / §E.4 replacement 完成+re-commit           MUST-FAIL-OK
Probe / dispose_root 收敛到空 registry           MUST-FAIL-OK
```

（以上为 2026-09-13 于 `verification/fv-temp-0` 分支、base `e57e7f3` 的
实测输出；`tla2tools v1.7.4`，sha256 校验。）

---

## 1. Gate contract（#122 要求的逐项声明）

```text
TARGET
    K0 step/settle（synchronous serialized control plane）× 独立合法外部
    语义事件：desired revision（provider withdrawal / replacement churn /
    重新启用）、dispose_root（= desired 收敛到空集后由同一组 kernel step
    drain）、bounded activation 的两种 legal 结局（自然完成 / raise，B19）、
    以及 defective component 的违约注入（§G.6 的 kernel 侧处理语义）。

CLAIM
    Safety：
      P1  relied_on guard —— 有 open committed view 点名 provider 时，
          provider 的 episode 不得已关闭/被移除（§G）。
      P2a inverse 恰好一次 —— 每 effect 的 inverse 至多执行一次（§H.1）。
      P2b tombstone 保留 —— 违约 latch 后 fiber 保持 installed、episode
          开放、provenance tombstone 留在 accumulator（§G.6/§K.4）。
          （精度限定：模型直接探索的是 effect-bearing witness；第二个
          生产失败位 teardown-closure latch 未单独建模，由 Rust refinement
          oracle 承载覆盖——见 §4。）
      P3  移除纪律 —— 不存在 accumulator 未清空 / 有未消费 inverse 的
          移除（Thm 64/Cor 69）。
      P4a 点单一来源 —— 任一时刻至多一个 installed fiber 声明提供 K
          （§E.4 frozen pointwise invariant / B12）。
      违约保持 guard —— 违约 fiber 的开放 view 使其 provider 的
          final-release guard 保持 latched（§G.6）。
      FAILED 只能由 raise + 完全 discharge 到达（B19/§F.5）。
    Liveness（fairness 见 FAIRNESS）：
      L1  环境停止后 settle 终止（到达 Settled/Blocked）。
      L2  unloading episode 最终关闭，除非自身违约 latch、或被违约 fiber
          跨边合法阻塞（§G.6 显式放弃的进展）。

AUTHORITY SOURCE
    docs/architecture/composition-kernel-0-design.md
      §F.2/F.3/F.4（状态机与非法转移）、§E.3（resolution eligibility）、
      §E.4（staged replacement + 点单一来源不变式）、§G（withdrawal
      protocol / relied guard）、§G.6（teardown contract violation latch、
      guard latched、进展放弃）、§L.1（quiescence predicate，FAILED
      quiet-legal、owed-mount 不安静）、§L.2（root disposal；违约边不再
      发请求）、§L.5（desired revision identity，R1–R8）。
    implementation-adr：D2–D9（representation；模型不依赖具体表示）。

PRODUCTION MAPPING
    见 CompositionKernel0.tla 头注「PRODUCTION MAPPING」表（model concept
    ↔ ADR concept ↔ crates/qianqian-composition/src/kernel.rs、fiber.rs
    逐函数对应；step() rule 1–6 优先级逐条镜像 kernel.rs step() 的
    if-chain）。关键对应：UnloadClose↔eligible_unload+unload_fiber、
    Divert↔divert_candidate、Retire↔retire_mismatch_candidate、
    Remove↔removal_candidate、Mount↔mount_candidate、
    Activate↔activation_candidate+activate_fiber、
    SetDesired*↔set_desired、SetDesiredEmpty↔dispose_root、
    ReliedOn↔relied_on()、"activating" 不建模（bounded atomic step，
    B3/B20）。

FAILURE SHAPE
    见各 mutation 的反例与 evidence/m5-counterexample-trace.md：
    provider 在 relied 未清时被关闭（M1）、欠 discharge 被移除（M2）、
    双 provider 同时 installed（M3/M5）、tombstone inverse 二次执行（M4）。

TOOL CHOICE
    被挑战的是「独立合法语义事件在 settle 各 step 边界的时序交错」——
    ADR §13 formalization policy 点名的形态；需要穷举交错，Rust 类型/
    单测/Kani/Loom 不覆盖语义级 temporal 碰撞。见 #124 §2 路由表。

BOUND / ASSUMPTIONS
    B1 3 fibers：C requires {K}；P1/P2 各 provides {K}（#122 建议实例；
        无链式二级依赖）。
    B2 desired 仅 3 个 legal 形状，revision ∈ {0,1}；desired revision
        提交总次数 ≤ MaxRevisions=4（baseline）/ 2（mutation run）。
        非法 desired（双 provider/环）由 Rust plan-time 拒绝，不在模型。
    B3 每 episode 注册 1 个 reversible effect（effects ≤ 1）；每代
        activation 总数 ≤ MaxActivations=2（baseline）/ 1（mutation run）
        ——bounded churn window，超出后 Activate 在界内不可用（截断，
        不声称覆盖无限 churn）。
    B4 违约注入是「defective component」下 kernel 的 §G.6 处理语义；
        legal K0 inverse 的 totality 是组件契约（§H.7），模型不质疑它。
    B5 阻塞深度：本拓扑下 BlockedByViolation 深度为 1（唯一 consumer）。
    B6 activation 建模为 atomic step（Rust 语义：activate_fiber 单个
        step() 内完成，含 raise 的 partial unwind）。

FAIRNESS
    仅 WF_vars(KernelActions)：serialized 控制面只要还有 enabled
    transition 就最终前进（settle() 的驱动假设；Thm 73 进展前提）。
    环境事件有限性由 StopEnv 动作显式建模（liveness 性质以 envStopped
    为前提，env 不停则性质条件不触发）。没有更强的 fairness 被假设；
    §G.6 的进展放弃（违约 latch 阻塞其依赖边）以 BlockedByViolation
    显式表达，未被 fairness 掩盖。

NEGATIVE CONTROL
    M1 DropReliedGuard        → ReliedGuard 反例
    M2 RemoveBeforeDischarge  → NoRemovalOwing 反例
    M3 EarlyReplacement       → SingleSource 反例（撤 staging 优先级 +
                                重叠守卫）
    M4 DoubleInverse          → InverseOnce 反例（违约 tombstone 二次执行）
    M5 MountOverViolation     → SingleSource 反例（只撤重叠守卫 =
                                #126 修复前的 kernel.rs 行为，现作该
                                guard 的 TLA 侧负控制；见 DIFFERENTIAL）
    全部 mutation 反例命中目标；BASELINE clean。mutation run 使用更紧
    界（MaxRevisions=2、MaxActivations=1、只查目标 invariant）——原因
    见「Runner lessons」。

RESULT CLASS
    baseline safety      BOUNDED-CLEAN
    baseline liveness    BOUNDED-CLEAN（在上述 FAIRNESS 下）
    negative controls    全部 COUNTEREXAMPLE（gate 灵敏度成立）
    reachability probes  全部 witness 可达（模型非 vacuous）

DIFFERENTIAL
    见下节。
```

---

## 2. DIFFERENTIAL — RESOLVED：`mount_candidate` 曾缺 capability-overlap guard（pre-#126）

**分类（按 #124 §9 / AGENTS.md authority resolution）：**

```text
ADR says X（§E.4 frozen pointwise invariant：任一时刻至多一个 installed
fiber 声明提供同一 capability —— "An Unloading old fiber is still
installed, so inserting an overlapping new provider before the old is
removed would violate the registry invariant outright"；§L.2 违约行：
reconcile "issues no further requests through the affected edge"）
Rust violates X（当时 mount_candidate 只查同名 fiber，不查 capability 重叠）
=> PRODUCTION DEFECT（pre-#126；RESOLVED，见下方 RESOLUTION）
```

**[2026-09-15 RESOLUTION]** 已确认并修复：#126（bb7662d，2026-09-13
merge）为 mount_candidate 增加 capability-overlap withhold；TA 反例的
Rust 回归在 `crates/qianqian-composition/tests/staged_mount_oracles.rs`，
Rust 侧负控制为 M-K3（specs/composition-kernel-0-rust）。当前 mapping：

```text
baseline 模型（MountOverlapBlock 默认生效）
    = 当前 Rust 的 overlap-guard 行为（#126 之后）

M5 mutation（MountOverViolation）
    = 刻意撤掉该现行 guard
    = 复现 pre-#126 Rust 的反例（归档该历史缺陷行为）
```

**反例（模型复现 pre-#126 Rust 行为；trace：`evidence/m5-counterexample-trace.md`）：**

1. P1 激活期 partial unwind 违约（defective component）→ P1 latched 在
   Unloading，provision tombstone 永不释放（§G.6 语义，设计行为）。
   变体：违约发生在 consumer C 上时，P1 因 relied guard latched 同样
   永不可移除（§G.6 Scenario A，同为设计行为）。
2. 环境提交 desired：P1 → P2 替换。step() rule 1–4 对 latched fiber
   全部无候选（unload/remove 被 `¬violated` 阻断，retire 排除
   Unloading）。
3. `mount_candidate()`（kernel.rs）只检查「无同名运行 fiber」→ P2 被
   挂载并激活。
4. registry 同时存在两个 installed fibers 声明提供 K —— §E.4 点不变式
   在违约路径被打破；`provisions` 诊断投影可同时显示两者。

**为何 baseline 无法覆盖此缺陷**：clean 路径下 step 优先级（rule 4
removal 先于 rule 5 mount）使 staging 涌现成立；洞只在「latched fiber
永不可到达移除」的 §G.6 路径上。M5 正是把该现行守卫撤掉后的模型行为，
与 pre-#126 Rust 行为一致；#126 修复后 baseline（守卫在位）即当前
Rust 行为。

**拟议修复（最小、authority-faithful）**：`mount_candidate` 跳过
「其 provision 集与任一 installed fiber 的 provision 集相交」的
entry（使 §E.4 可 inspection-decidable，正是 §E.4 点名的实践收益）。
挂载将在违约边解除前保持欠着（Blocked/owed，loud 而非 silent），与
§G.6「进展放弃」一致。——已按此形状落地于 #126（含 Rust 侧对抗性
回归测试复现本 counterexample）。

---

## 3. Runner lessons（fail-closed，继承 #79/#82 学费 + 本轮新增）

```text
cfg model-value mismatch   全部 mutation/解读以 model-value 常量注入，
                           runner 校验 sha256 与 "Finished in"。
-continue false-complete   TLC 1.7.4 在 -continue 且 violation report
                           达数千条时内部崩溃（java.lang.
                           ArrayIndexOutOfBoundsException，exit 75/255），
                           队列非空即截断 —— violated 集不确定。
                           缓解：mutation run 只查目标 invariant +
                           收紧 churn 界（MaxRevisions=2、
                           MaxActivations=1），violation 数降至千级
                           以下仍可能崩溃 ⇒ runner 对 fail:/lfail 模式
                           重试至多 3 次并强制要求 log 含 "Finished in"
                           （被杀/崩溃丢尾的 run 不作为证据）。
exit-code semantics        runner 以 log 内容（completed / violated /
                           Finished）为准，不信任 exit code 单一信号。
tmp/state directory        工作目录固定真实磁盘（/var/tmp），不落 tmpfs。
warnings                   任何 TLC "Warning:" 行 ⇒ FAIL（无白名单）。
deadlock                   模型显式提供 TerminalStutter（仅 envStopped ∧
                           SystemStable 允许停机）；真卡死的工作态仍是
                           缺陷，保持被 TLC 死锁检测捕获。
```

## 4. §G.6 第二失败位（teardown-closure latch）— FORMALIZATION_NOT_EARNED（2026-09-15）

模型直接探索的违约路径全部是 **effect-bearing latch**（违约 inverse 停住
unwind，violated 记录留作 tombstone，`TombstoneRetained` 因此要求
`effects >= 1`）。生产的第二个失败位——**component teardown closure 在空
accumulator 上返回 Violated**（kernel.rs `unload_fiber`：run_unwind 完全
discharge 之后）——未单独建模。本轮按 differential 分析判定该位点
**不是**独立的状态/交错家族：

| 维度 | Shape A：effect-bearing inverse | Shape B：teardown closure（空 accumulator） |
| --- | --- | --- |
| 生产位点 | `run_unwind` inverse 返回 Violated | `unload_fiber` teardown closure 返回 Violated（unwind 已完全 discharge） |
| fiber state | Unloading | Unloading |
| accumulator 非空 | 是（violated 记录 = provenance tombstone，§K.4） | 否 |
| tombstone 存在 | 是 | 否（无 effect 绑定可保留；开放 episode 本身承载「未假装清洁」） |
| violation bit | TEARDOWN_VIOLATED latch | 同 |
| committed view / relied_on | 开放（两路径都在 `committed = None` 之前 return） | 同 |
| provider guard | 被 latched fiber 的开放 view 钉住（ViolatedKeepsGuard） | 同 |
| removal eligibility | `¬teardown_violated` 阻断 | 同 |
| mount overlap blocking | `mount_candidate` 按 installed 声明 provides withhold（#126） | 同（与 accumulator 深度无关） |
| dispose_root | 不移除 latched fiber | 同 |
| settle 终止 | 终止于 Blocked（loud，quiet=false） | 同 |
| FAILED 可达 | 否 | 否 |

**判定**：两形状在所有对两形状共同声明的 K0 不变式（ReliedGuard /
InverseOnce / NoRemovalOwing / SingleSource / ViolatedKeepsGuard /
FailedHasPendingError / L1 / L2）上取值相同；唯一差异（tombstone /
accumulator 深度）是 representation——**但它恰是 P2b tombstone 条款的
对象，而该条款已按 §1 的精度限定 scope 在 effect-bearing witness 上**；
本节不得被引用为「P2b 覆盖 teardown-closure latch」。除 P2b 外，该差异
不是任何其他已声明不变式的语义载体。未找到「独立合法事件在 Shape B
交错出 Shape A 抽象无法表达的状态」的具体碰撞 ⇒
**FORMALIZATION_NOT_EARNED，不加新 TLA state/operator，不设 M6**。

**Rust refinement oracle**（production mapping 证据，scope 刻意收窄）：
`crates/qianqian-composition/tests/lifecycle_oracles.rs ::
dependent_consumer_violation_loci_preserve_k0_guard_semantics` ——
dependent-consumer 拓扑（consumer 持开放 episode 违约 + 同 capability
replacement 待命）下两个 locus 走同一场景脚本（withdraw provider → latch →
尝试同 capability replacement → dispose_root），断言同一语义后果集
（latched+installed、episode 开放且 committed binding 仍可见、provider
guard 保持、removal 阻断、replacement withheld、settle Blocked、
diagnostic loud），并含 locus 路径证据断言防 vacuity。representation
差异刻意不要求一致。**该 oracle 只声明其覆盖的 dependent-consumer
拓扑，不是对所有 component role/topology 的普遍 locus 等价证明**
（provider-self 违约不在其覆盖内）。

**当前 TLA claim 精确化**：§G.6 violation-latch semantic family =
TLA witness（effect-bearing inverse failure，直接探索）+ Rust refinement
witness（teardown-closure empty-accumulator failure）。TLA 并未直接探索
Shape B——任何向该方向的转述都是 scope inflation。

## 5. UNVERIFIED SURFACES（明确不在本 gate 覆盖内）

```text
无限 churn / 无界 generation 数        （bounded churn window 截断）
阻塞深度 >1 的依赖链                   （本实例无双级依赖）
多 capability / 多 consumer 拓扑       （单 capability 单 consumer）
plan-time 校验（§L.4 的环/歧义拒绝）   （Rust 侧，非法 desired 不建模）
步骤粒度内的 ActivationCtx 可重入行为  （activation 建模为 atomic step）
implementation 细节（slab 复用、generation 回绕、handle 编号）
OS / realtime / FFI 表面               （Loom/Miri/集成层领域）
§G.6 的 teardown-closure latch 位点   （未单独建模：
                                        FORMALIZATION_NOT_EARNED 判定 +
                                        Rust refinement oracle 覆盖，见 §4）
```

## 6. 建议后续

1. mount_candidate overlap guard —— 已完成：#126 merge（含 Rust 回归
   staged_mount_oracles + M-K3 负控制），differential 见 §2 RESOLUTION。
2. K0 语义无 differential：模型与 ADR 一致；无需 authority 修订。
3. 后续 FV-RUST-0（Kani）可在 implementation 层复核 K1–K6 候选性质
   （见 #124 §3.B），与本模型的语义层结论互补。
