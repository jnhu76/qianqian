# specs/realtime-publication — Realtime View Publication / Reader Quiescence / Resource Reclamation 形式化证据

> **STATUS: FORMAL EVIDENCE（当前挣得）**
>
> 语义来源：`docs/adr/ADR-PBK-001.md` §6（ACCEPTED 的 publication correctness
> contract、P1–P5 语义协议与 lifetime safety）、§12 Phase D、§13 Formalization
> policy。本目录不是第二份 Playback Foundations authority；ADR-PBK-001 仍是唯一
> normative constitution。模型结论只在其显式抽象与假设下成立。
>
> 决策记录（候选机制比较）不在本文件，见
> `docs/architecture/realtime-publication-lifetime-decision.md`。

本目录用 TLA+/TLC 对一个具体交错做穷举状态探索：

> realtime view N → N+1 发布与旧 reader overlap 时，resource 何时才获得
> release/reclaim 的权利？

不建模：PCM 内容、播放器语义、线程调度、内存序、reader 崩溃/停滞、ABA/视图
槽位复用（epoch 单调不复用）。这些属于后续 Rust 并发测试 / Loom / sanitizer
的职责（见决策记录的 residual risks）。

读者实例化约定：`Readers` 按**持有**实例化，不按物理线程——一个真实线程若
合法地同时解引用两代视图（如 crossfade 重叠渲染新旧图），应建模为两个 reader
身份；当前 run 配置（2 readers）覆盖的是“至多两个并发持有”。

---

## 模型词汇（不继承旧 playback nouns）

| 模型概念 | 含义 |
| --- | --- |
| view | 完整 realtime 视图记录 `[epoch, topology, resources]`；拓扑与资源表只有作为整体才一致 |
| published view | 控制侧当前发布的视图（`publishedEpoch` + `pubResources` 两个分量；拆分是为了让 M2 能表达“半发布”窗口） |
| reader | realtime 读者；持有某视图 = 仍可能解引用该视图资源（**统一抽象** active 执行与已加载待执行的 queued reference） |
| retired | 视图被后续发布取代（`e < publishedEpoch`），不再接受新 acquisition；旧读者可继续完成 |
| live / retired / reclaimable / released | 资源生命周期四态；`retired`（被新视图排除）与 `released`（物理释放）之间隔着 quiescence 认证 |
| certified | 历史变量：该资源发生过 MarkReclaimable 认证 |
| quiescence | 没有任何 reader 仍可能通过任何代视图解引用该资源 |

协议动词（与 ADR §6 最小顺序一致）：`AcquireView` / `PublishNewView`（retire
旧视图 + 原子切换，即一次 RT-safe boundary 提交）/ `ReaderExit` /
`MarkReclaimable`（quiescence 认证）/ `ReleaseResource`（物理释放）。

关键状态区分（刻意不揉成 bool）：

```text
Retired != Reclaimable != Released
PublishedNew != OldReadersGone
NoNewEntry != Quiescent != PhysicallyFreed
```

四个可达性探针证明这些区分在模型里都是活状态（见下）。

## 不变式与性质

| 编号 | 不变式 | 语义 |
| --- | --- | --- |
| I1 | `NoReaderDereferencesReleasedResource` | 仍可能解引用 x 的读者存在 ⇒ x 不得进入 reclaimable/released（M1/M4 击杀的核心条款）；x 仍在 published 资源表 ⇒ x 不得已 released（**防御条款**：当前全部 mutation 下结构性不可违反——一切释放路径都要求先 retired；保留以防未来出现“仍在 published 表内却被释放”的路径） |
| I2 | `ViewIsCoherent` | 一次 acquisition 只能观察完整 N 或完整 N+1，不得是“拓扑来自 N + 资源表来自 N+1”的混合视图 |
| I3 | `NoReaderAcquiresRetiredView` | 新 acquisition 不得进入 retired 视图（M3 击杀的核心条款：acquisition 纪元必须等于视图纪元）；另附 `epoch ≤ publishedEpoch` **防御条款**（全部 mutation 下结构性不可违反） |
| I4 | `ReleaseRequiresCertifiedQuiescence` | 物理释放必须发生在 quiescence 认证之后（M1 在无读者交错下独立击杀） |
| I5 | （合法性声明） | retired + 旧读者仍在 = 合法状态，不是 bug；由探针 `RetirementOverlapWitness` 证明可达且不被任何不变式禁止 |
| I6 | `ReplacementEventuallyReclaimable`（temporal） | 资源 retired 后最终能被认证回收（回收资格，不是强制立即物理释放） |

**Liveness 假设（显式声明，不偷设）**：

- `WF(ReaderExit)`：realtime reader 不会永远停留在同一视图内。停滞/崩溃读者
  延迟回收是真实工程风险，属于机制比较维度（决策记录 E 项），不在本模型内。
- `WF(MarkReclaimable)`：控制侧在 quiescence 成立后最终执行认证。

## 负控制（mutation 注入，TLC 必须给出 counterexample）

| Mutation | 注入缺陷 | 必须违反 |
| --- | --- | --- |
| M1 `ReleaseBeforeQuiesce` | 发布后未经 quiescence 认证直接释放 | I1（+I4 同步失败：跳过认证） |
| M2 `SplitPublication` | 一次发布拆成“资源表半步 + 拓扑半步”，读者可中途观察 | I2（half N / half N+1） |
| M3 `StaleEntry`（safety） | 发布后撤掉闭门，新 reader 仍可进入 retired 视图 | I3（附带：认证后 stale entry 产生 reclaimable-with-holder，I1 同步被违反——标准 run 中 I3 先失败会掩盖状态级报告，单独以 I1 为不变式可复现） |
| M3 `StaleEntry`（liveness） | 同上，仅带 temporal property | I6（lasso：旧视图被反复进入，永无法 quiesce——即使无直接 use-after-release） |
| M4 `ForgetsOlderRetirement` | 发布链 N→N+1→N+2 下认证只查最新退休代读者，遗忘更老视图读者 | I1（多代 overlap 提前回收） |

M3 的 safety 与 liveness 分开取证：**Safety 与 Liveness 不是一回事**——
闭门缺陷即使不产生直接 UAF，也会让旧视图永远无法 quiesce。

## 可达性探针（正向控制：断言“不可达”的不变式必须被违反 = witness 找到）

| Probe | 证明 |
| --- | --- |
| `RetirementOverlapWitness` | retired + 仍有读者持有旧视图 的合法 overlap 态可达（I5 witness；正常模型不禁止它） |
| `QuiescentUncertifiedWitness` | “已 quiescent 但未认证未释放”中间态可达（Quiescent != PhysicallyFreed 活着） |
| `ReclaimableWitness` | 认证路径可达（MarkReclaimable 非死代码） |
| `ReleasedWitness` | 释放路径可达（ReleaseResource 非死代码，正常协议能走完全链） |

## 运行结果（工具链：tla2tools v1.7.4 / TLC2 2.19，OpenJDK 25.0.4，4 workers）

| Run | states | distinct | depth | 结果 |
| --- | --- | --- | --- | --- |
| normal（{A}，MaxPub=1） | 99 | 38 | 8 | PASS（全部不变式 + temporal property；warnings=0；completed） |
| normal-chain（{A,B}，MaxPub=2） | 707 | 220 | 11 | PASS（同上；多代退休记账下同一套不变式成立） |
| M1 ReleaseBeforeQuiesce | 147 | 54 | 8 | MUST-FAIL-OK：违反 I1 + I4 |
| M2 SplitPublication | 218 | 78 | 13 | MUST-FAIL-OK：违反 I2 |
| M3 StaleEntry safety | 204 | 61 | 8 | MUST-FAIL-OK：违反 I3 |
| M3 StaleEntry liveness | 204 | 61 | — | MUST-FAIL-OK：temporal property 违反（lasso） |
| M4 ForgetsOlderRetirement | 950 | 292 | 11 | MUST-FAIL-OK：违反 I1 |
| 探针 ×4 | — | — | — | 全部 witness 找到 |

所有 run：TLC warnings = 0；正常模型带 `Model checking completed`；反例 run 带
`Finished in`（TLC 自行收尾）。fail-closed 判据由 `check.sh` 强制。

### M1 反例（最小危险历史，4 状态）

```text
State 1  init                     view0 published（含 A），A live
State 2  AcquireView(r1)          r1 持有 [epoch 0, topology 0, {A}]
State 3  PublishNewView           epoch 1，新表不含 A，A retired；r1 仍持有 view0
State 4  ReleaseResource(A)       （M1：从 retired 直接释放）
→ I1 违反：r1 仍可能解引用 A，而 A 已 released
→ I4 同步违反：释放未经任何 quiescence 认证
```

### M2 反例（混合视图）

```text
State 2  SplitPublishResources    资源表先切成 {}，epoch/拓扑仍是 0
State 3  AcquireView              读者观察到 [topology 0, resources {}]
→ I2 违反：拓扑来自 N + 资源表来自 N+1
```

### M3 liveness 反例（lasso）

```text
发布 N+1 后：AcquireView(进入 retired view0) → ReaderExit → AcquireView …
（Back to state 4 循环）
→ I6 违反：旧视图永不 quiesce，A 永远到不了 reclaimable——无 UAF 也致命
```

### M4 反例（多代 overlap 提前回收）

```text
AcquireView(view0 含 A/B) → Publish(N+1，排除 A) → Publish(N+2，排除 B)
→ MarkReclaimable 被放行（守卫只查最新退休代 V1 的读者，遗忘 view0 读者）
→ I1 违反：view0 读者仍持有 A/B，认证/释放却已发生
```

## 运行入口

```bash
specs/realtime-publication/check.sh

# 需要代理下载工具链时：
export https_proxy=http://127.0.0.1:7897
specs/realtime-publication/check.sh
```

工具链固定 `tla2tools v1.7.4`，与 `specs/playback` 共用 `specs/tools/tla2tools.jar`
（内嵌 sha256 校验，fail closed；缺失时自动下载）。

规则：正常模型必须探索完成且全部不变式 + temporal property PASS；每个 mutation
必须违反其目标不变式；liveness 反例必须由 TLC 自行收尾输出；任何 Warning 即
FAIL。证据判定逻辑全部在 `check.sh`，无人工 grep。

## Traceability

- 语义范围对应 `docs/adr/ADR-PBK-001.md` §6（publication correctness
  contract / P1–P5 语义协议 / lifetime safety 最小顺序）与 §13 点名的头号候选交错。
- 与 §13 时序的关系（显式声明）：§13 要求“先有具体 collision，再建最小模型”。
  本模型针对的正是 §13 自己点名的候选交错（old view 引用 provider → 新视图
  排除 → 旧 reader 仍在 → final release）；TLC 穷举 + M1 反例证明该交错在
  模型空间内**确实**撞出非法状态——即 collision 在语义层已被证实，模型不是
  为名词而建。实现层的碰撞确认（§12 Phase D 可执行实验 / ADR §14 G5）仍 OPEN，
  不因本模型的存在而视为已完成。
- 本模型证明的是：**在上述抽象与假设下**，对 publication/reclamation safety 的
  穷举状态探索 + mutation 反证。不声称“形式化证明整个 audio runtime”。
- 机制（ArcSwap / epoch / RCU / hazard / refcount / lease 等 representation）
  保持 OPEN；任何正确机制必须满足的语义协议 P1–P5 已 normative 冻结于
  `docs/adr/ADR-PBK-001.md` §6（其形式化推导与机制比较见决策记录
  `docs/architecture/realtime-publication-lifetime-decision.md`）。
