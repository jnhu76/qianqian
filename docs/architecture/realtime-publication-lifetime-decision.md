# Realtime Graph Publication / Reader Quiescence 决策证据

> **定位：EVIDENCE + ENGINEERING DECISION RECORD（证据 + 工程决策记录）**
>
> 这不是第二份 Playback Foundations constitution。normative foundation authority
> 仍然是 `docs/adr/ADR-PBK-001.md`（ACCEPTED）；本文只记录：
> 1. 一个已被 TLA+ 穷举证明的真实并发风险（formal evidence）；
> 2. 语义协议 P1–P5 的形式化推导与模型对应（其 normative 定义已吸收进
>    ADR §6；本文不是第二份 normative 定义，措辞与 ADR 分歧时以 ADR 为准）；
> 3. 候选机制的工程比较与当前倾向（工程判断，representation 保持 OPEN）。
>
> 形式化证据本体：`specs/realtime-publication/`（模型、mutation、探针、运行数据）。

---

## 1. Status

```text
形式化证据        COMPLETE（TLC 穷举 + M1–M4 反证 + 4 个可达性探针，全部门通过）
语义协议          FROZEN AS NORMATIVE（P1–P5 已吸收进 ADR-PBK-001 §6；本记录 §13
                  保留其形式化推导与模型对应，ADR 是唯一 normative 定义处）
实现机制          DEFERRED（不冻结；当前工程倾向见 §14，Phase D 可执行实验裁决）
ADR 影响          SEMANTIC CLARIFICATION / CORRECTIVE——P1–P5 把 ADR §1/§6 已接受
                  的 lifetime contract 用新挣得的形式化证据明文化为 normative 协议；
                  无新 architecture plane、无新 runtime primitive、无 representation
                  决策、未选具体回收机制
```

## 2. 问题定义

ADR-PBK-001 §6 冻结 publication correctness contract、lifetime safety 与语义协议
P1–P5（由本记录的形式化证据挣得、经 semantic corrective 吸收），并把
representation（ArcSwap / RCU / epoch / double-buffer / atomic pointer /
lease / hazard）与 reader-quiescence 具体机制留为 OPEN。本记录回答两个问题：

1. **问题是否真实**：realtime view N → N+1 发布与旧 reader overlap 时，
   "发布即可释放"是否真的能撞出非法状态？（是，见 §5/§9 的 M1 反例。）
2. **语义上必须成立什么**：任何候选机制必须实现哪些性质？（P1–P5——
   normative 定义在 ADR §6，形式化推导见本文 §13。）

不回答：哪个 crate 最好（§14 只记录工程倾向，不做 normative 选择）。

## 3. 已有 authority

- `docs/adr/ADR-PBK-001.md` §1（宪法第 5/6 条：pre-bound view；RT 可解引用资源
  在读者 quiesce 前保持有效）、§2.4、§6（publication correctness contract +
  P1–P5 协议 + lifetime 最小顺序）、§7（parameter vs topology）、§12 Phase D
  （mechanism validation）、§13（formalization policy）。
- 旧 playback TLA 套件（已从 main 删除，Git 历史存档）只被复用了 verifier 纪律（TLC warning fail-closed、
  负控制必须出反例、`Finished in`/`Model checking completed` 判据），没有继承
  任何旧 playback 状态词汇。

## 4. 非目标

不做 FFmpeg/WASAPI/真实 AudioOutput/播放器状态机；不冻结 PCM canonical
contract；不重新引入 MusicKernel/TransportKernel/Generation/Dual Window/
Physical Fence 等旧名词；不建"大一统 Playback TLA"；不先选库再倒推语义；
不把 K0 卷入 realtime 机制。

## 5. 最小并发历史（已被 TLC 证明可达且危险）

```text
view N（含资源 A）发布
→ reader R acquisition 进入 N
→ 控制侧构造 N+1（不含 A）
→ Realtime-view publication of N+1
→ R 仍持有 N（仍可解引用 A）
→ 释放 A
→ 【非法】R 此后解引用 A = use-after-release
```

TLC 在 M1 mutation 下 4 个状态撞出该形状（完整轨迹见
`specs/realtime-publication/README.md`）。关键结论：**"什么时候 A 才获得
release/reclaim 的权利"的答案是"当且仅当没有任何读者（active 执行或 queued
reference）仍可能通过任何一代视图解引用 A"——发布本身不授予该权利。**

## 6. TLA+ 状态模型

模型 `specs/realtime-publication/RealtimePublication.tla`，6 个状态变量：

| 变量 | 含义 | 对应任务书建议词汇 |
| --- | --- | --- |
| `publishedEpoch` + `pubResources` | 已发布视图（epoch=拓扑身份标签 + 资源表；拆两分量是为了让 M2 能表达"半发布"） | `publishedView` |
| `readerView[r]` | 读者当前仍可解引用的完整视图记录（或 NoView） | `readerView` |
| `acqEpoch[r]` | acquisition 时刻的纪元（承载 I3 闭门语义） | — |
| `resourceState[x]` | `live / retired / reclaimable / released` 四态 | `resourceState` |
| `certified[x]` | 历史变量：是否发生过 quiescence 认证 | — |

派生量：`retiredViews`（由 epoch 派生）、active readers（由 readerView 派生）。
刻意保持的状态区分：`Retired ≠ Reclaimable ≠ Released`、
`PublishedNew ≠ OldReadersGone`、`NoNewEntry ≠ Quiescent ≠ PhysicallyFreed`。

抽象假设（显式声明）：

- **queued reference 折叠**：`readerView[r] ≠ NoView` ⇔ 读者仍可能解引用该视图
  资源——覆盖"正在执行"与"已加载待执行"两类引用（对应 ADR §1 第 6 条的
  "realtime execution or queued reference"统一谓词）。
- 视图 = 原子整体：`[epoch, topology, resources]`，拓扑 e 的边恰好解引用
  `ResourcesAt(e)`；混合分量的组合只有整体一致才合法（I2 的语义基础）。
- 不建模：PCM 内容、线程调度、内存序、reader 崩溃/停滞、ABA/槽位复用
  （epoch 单调不复用）。

## 7. Safety invariants（与模型同名算子一一对应）

| 编号 | 不变式 | 语义 |
| --- | --- | --- |
| I1 | `NoReaderDereferencesReleasedResource` | 仍可能解引用 x 的读者存在 ⇒ x ∉ {reclaimable, released}；x 仍在 published 表 ⇒ x ≠ released |
| I2 | `ViewIsCoherent` | 一次 acquisition 只观察完整 N 或完整 N+1；不得"拓扑来自 N + 资源表来自 N+1" |
| I3 | `NoReaderAcquiresRetiredView` | 新 acquisition 不得进入 retired 视图；旧读者可继续完成 |
| I4 | `ReleaseRequiresCertifiedQuiescence` | 物理释放必须发生在 quiescence 认证之后 |
| I5 | （合法性声明，非不变式） | retired + 旧读者仍在 = 合法状态；由探针证明可达且不被 I1–I4 禁止 |
| I6 | `ReplacementEventuallyReclaimable`（temporal） | 资源 retired 后最终能获得回收资格（不是强制立即物理释放） |

## 8. Liveness assumptions（显式、不偷设）

```text
WF(ReaderExit)      realtime reader 不会永远停留在同一视图内
                    （停滞/崩溃读者延迟回收 = 真实工程风险，交给机制比较维度 E，
                     不在模型内假装解决）
WF(MarkReclaimable) 控制侧在 quiescence 成立后最终执行认证
```

不写其它 fairness。I6 只在这两条假设下成立并已验证。

## 9. Mutation negative controls（全部 MUST-FAIL-OK）

| Mutation | 注入缺陷 | 反例形状 | 违反 |
| --- | --- | --- | --- |
| M1 `ReleaseBeforeQuiesce` | 发布后未认证直接释放 | §5 的 4 状态最小危险历史 | I1 + I4 |
| M2 `SplitPublication` | 发布拆两步、读者中途观察 | 资源表半步后读者获取 `[topology 0, resources {}]` | I2 |
| M3 `StaleEntry`（safety） | 发布后新读者仍可进 retired 视图 | acquisition 纪元错位 | I3 |
| M3 `StaleEntry`（liveness） | 同上，只带 temporal property | Acquire→Exit→Acquire lasso，旧视图永不 quiesce | I6 |
| M4 `ForgetsOlderRetirement` | 发布链 N→N+1→N+2 认证只查最新退休代 | view0 读者跨越两次发布后被提前认证释放 | I1 |

M3 双侧取证说明：**Safety 与 Liveness 不是一回事**——闭门缺陷即使不产生直接
UAF，也让旧视图永无法 quiesce（资源泄漏/永不可回收）。

## 10. TLC 结果

工具链：tla2tools v1.7.4（TLC2 2.19）、OpenJDK 25.0.4、4 workers。全部 run
warnings=0；正常模型 `Model checking completed`；反例 run `Finished in`
（TLC 自行收尾）。逐 run 状态空间数据与复现命令见
`specs/realtime-publication/README.md`。

准确表述：**对本模型与上述显式假设下的 publication/reclamation safety 进行了
穷举状态探索，并通过 mutation 负控制证明检查器能撞出目标缺陷。** 不声称
"形式化证明了整个 audio runtime"。

## 11. Candidate mechanisms（候选清单与快速淘汰理由）

| # | 候选 | 一句话评估 |
| --- | --- | --- |
| 1 | **原子不可变视图 + 引用计数**（ArcSwap 风格：`ArcSwapAny<Arc<View>>`，View 为不可变快照） | 读路径最轻（load ≈ 1 次原子读 + TLS debt 记录）；回收=最后一个引用释放，天然逐资源、支持多代 overlap。**注意**：最后引用的 drop 会触发整棵析构级联——RT 线程不得成为最后 drop 者（见维度 O 与 §18） |
| 2 | **epoch / RCU 类**（如 crossbeam-epoch：pin/unpin + 延迟回收） | 读路径同样轻，但回收延迟由 epoch 推进决定；默认共享 collector 下一个停滞 reader 延迟整批回收（可用专用 `Collector` 实例把批次范围限定到 publication 子系统，但停滞仍是批次级延迟）；guard 纪律（不得长期 pin、跨调度点 pin 反模式）对音频 callback 形态是持续负担；unsafe 面大；且其原生 pin 无法表达“已入队待执行”的引用（见 §12 queued-reference 注） |
| 3 | **显式 reader 计数 / lease** | 语义最直白、可 safe-Rust 实现；但每次 acquisition/release 都做共享计数器 RMW（x86 `lock xadd`），多读者时缓存行乒乓；控制侧需等 count==0（等待/轮询与 RT 耦合，见维度 P） |
| 4 | **double-buffer + reader quiescence** | 经典音频方案；读路径为指针读 + 每回调握手（in-use 标记/纪元写），并非字面零成本；槽数固定 ⇒ 多代 overlap（M4 场景）结构性受限：读者卡在槽 0 时连续第二次发布会等槽；worst-case retirement delay 取决于槽周转 |
| 5 | **hazard pointer 类** | 为“多读者 × 少对象”高并发设计；读路径为本线程槽 store + 校验 load（无 RMW、无跨核争用），单看读路径并不比 reader-count 贵；真正的扣分项是协议复杂度、unsafe 面与退休簿记（见维度 B/H/L）；且槽位在解引用时刻发布，同样无法原生表达 queued reference |

未列入：`RwLock<Arc<T>>` / `Mutex`（读者侧可能阻塞，违反 RT 路径无阻塞约束，
arc-swap 文档亦以此为对照基准）；裸 `AtomicPtr` 无生命周期追踪（等价于 M1 的
释放语义，已被反例否决）。

## 12. Decision matrix（统一评价维度 A–N）

评分：`++ 很好 / + 好 / ~ 中性或依赖条件 / − 差`。针对 Qianqian 真实形态：
**读者数 1–2、publication 低频、parameter update 中频、无分配 RT 路径**。

| 维度 | 1 atomic-view+refcount | 2 epoch/RCU | 3 reader-count/lease | 4 double-buffer | 5 hazard |
| --- | --- | --- | --- | --- | --- |
| A. RT 读路径成本 | `++` load=1 原子读+TLS debt；Guard drop 常态零操作，最坏 1 次 refcount dec（但见维度 O：dec 到零 ≠ O(1)） | `+` pin/unpin=TLS+epoch 读 | `~` 每次进出 RMW×2（与写侧 count==0 轮询共享缓存行） | `+` 指针读 + 每回调握手（in-use 标记/纪元写） | `+` 本线程槽 store+校验 load，无 RMW 无跨核争用；扣分在 B/H/L 不在 A |
| B. 控制侧复杂度 | `+` swap+drop old；debt 代付 | `~` 需驱动 epoch 推进与 collect | `+` 等 count==0 直白 | `~` 槽等待/周转逻辑 | `−` 退休列表+全槽扫描 |
| C. 回收正确性 | `++` 最后引用释放即安全；逐资源 | `+` 正确但延迟批量 | `++` count==0 即安全 | `+` 正确但槽约束 | `+` 正确但协议复杂 |
| D. Worst-case 退休延迟 | `++` 逐资源：仅等该资源读者 | `−` 批次级：默认共享 collector 下停滞 reader 拖住整批（专用 Collector 实例可把范围缩到子系统，仍非逐资源） | `++` 逐资源 | `−` 槽周转：读者卡槽阻塞后续发布 | `+` 逐对象扫描 |
| E. 读者停滞/崩溃后果 | 该资源延迟释放，无全局影响（停滞 Guard 与超时无关：只能等或放弃回收） | 批次级延迟（同 D） | 该资源阻塞；任何超时策略只能选择泄漏/放弃回收或拆除读者——**不能**在 count>0 时释放（那就是 M1） | 槽被占，阻塞后续发布链 | 该资源延迟释放 |
| F. 多个同时旧视图 | `++` 每代 Arc 独立追踪 | `+` 支持 | `+` 支持（per-view 计数） | `−` 槽数硬上限 | `+` 支持 |
| G. parameter 与 topology publication 兼容 | `+` 同一视图原子快照语义（ADR §7 的 cheap update 走视图内参数槽，另行实验） | `~` | `~` | `~` | `~` |
| H. Rust ergonomics / unsafe 面 | `++` 全 safe（crate 内部 unsafe） | `−` guard 纪律+unsafe | `++` 可全 safe | `~` 依赖实现 | `−` unsafe 协议 |
| I. 跨平台可移植性 | `++` 纯 Rust 原子 | `+` | `++` | `++` | `+` |
| J. 可测试性 / 与形式模型对应 | `++` "load=AcquireView、drop=ReaderExit、最后引用释放=certified release"一一对应（M4 对应每代独立 Arc） | `~` 需在模型外论证 epoch 推进 ⟶ 认证 | `+` 对应直白 | `~` 槽数需在模型外补 | `~` |
| K. 可调试观测性 | `+` refcount 可观测 | `−` 延迟回收难归因 | `++` count 可读 | `+` 槽状态可读 | `~` |
| L. 依赖重量 | `~` 一个小 crate（或自研 ~百行） | `~` crossbeam-epoch | `++` 零依赖 | `++` 零依赖 | `−` 自研/canonical 包重 |
| M. 满足"reader 只见完整 N 或 N+1" | `++` 发布整体不可变快照即天然满足（单次 load 一致性，arc-swap 文档以此为范式用例） | `++` | `+`（须视图=单句柄） | `+`（固定槽实现还依赖逐槽纪元标签——属模型未覆盖的槽复用/ABA 邻接机制，见 README 未覆盖清单） | `+`（须视图=单句柄） |
| N. RT callback 是否承担生命周期重活 | `+` load/drop 常数极小——**但最后 drop 会级联析构**，必须由 §18.5 的 deferred-disposal 约束挡在 RT 之外 | `~` 每次 callback pin/unpin 纪律 | `−` 每 quantum 两次 RMW（若按 quantum 计） | `+` | `~` |
| O. 最终析构/释放由谁执行 | `~` 默认形态下任意最后 drop 者触发整棵析构（含级联 Drop+堆释放）——需 deferred-disposal 保证控制侧执行 | `++` collector 线程批量释放 | `++` 控制侧在 count==0 后释放 | `++` 控制侧槽复用 | `++` 控制侧退休流程 |
| P. 控制侧等待/轮询与 RT 的耦合 | `++` writer 不等待 RT；swap 变量对 RT 只读 | `~` collect 需推进 epoch（可异步） | `−` 等待/轮询 count==0：轮询触碰 RT 写入的缓存行（抖动源），等待期间持锁/持资源是优先级反转面 | `−` 等槽周转：同左 | `~` 退休扫描不阻塞 RT，但需调度 |

事实性依据（arc-swap 官方文档，经 context7 核对）：`load()` 返回借用式 Guard
（有限借用槽 + 线程本地 debt 记录，避免 refcount 争用；writer swap 时代付全部
未清 debt）；`load_full()` 才做 refcount clone（RMW）；旧实例在最后一个引用/
Guard 释放时回收（该 drop 会执行 View 析构级联——见维度 O）；writer 比
uncontended Mutex 贵但在高争用下优于锁。其余候选评分为概念级工程判断（未做
基准测试——见 §17/§19：这正是 DEFERRED 的原因）。

**Queued-reference 排名翻转风险（显式声明）**：模型把 queued reference 折叠进
"仍可解引用"谓词对 *safety* 是健全的，但对机制排名不中立——

- epoch 的原生 `pin()` 无法表达"已入队待执行"的引用（跨队列驻留持有 pin 即
  长 pin 反模式）；hazard 槽位在解引用时刻发布、同样无法预绑定；二者若要
  支持预绑定 queued ref，都得退回持有型句柄（即重造候选 1 的 refcount）；
- Arc 引用计数原生支持预绑定，但预绑定用的是 `load_full`（RMW clone），
  不是行 A 所庆祝的快路径 `load`。

若 E1/E2（lease 粒度、执行模型）的结论是"预绑定 queued 引用占主导"（音频
API 常见的 pre-bound next-callback 形态），行 A 的排名可能整体塌缩：候选 1
的读路径优势缩到接近候选 3，候选 2/5 失去原生读路径。**这是 §14 倾向最可能
被 Phase B/C 证据推翻的路径。**

## 13. Semantic protocol P1–P5（形式化推导；normative 定义在 ADR-PBK-001 §6）

```text
P1  Coherent publication      Realtime-view publication 对 reader acquisition
                              是原子的整体提交：读者只观察完整 N 或完整 N+1
                              （I2；M2 反证）。实现形态上"单句柄不可变快照"
                              只是充分形态之一；带版本校验的多分量一致获取
                              同样满足——被禁止的是无协调的逐分量独立发布。

P2  Retired-view closure      新视图发布后，新 acquisition 不得进入 retired
                              视图；旧读者可继续完成（I3；M3 safety 反证）。

P3  Quiescence precedes       资源的释放资格由语义谓词决定：当且仅当没有任何
    reclamation               读者（active 执行或 queued reference）仍可能通过
                              【任何一代】视图解引用它（I1/I4；M1 反证）。
                              quiescence 决定资格；认证/识别可由机制按 P5 前提
                              稍后完成（ADR §6 P3：iff 是 eligibility 谓词，
                              不是 recognition 时限）。认证范围必须是资源可达
                              的全代视图，不得只查最新退休代（M4 反证）。

P4  Retirement != reclamation retired + 旧读者仍在 是合法状态；发布、闭门、
                              quiescence、物理释放是四个不同事件（I5 探针 +
                              四态资源机）。

P5  Progress under reader     在 §8 两条假设下，retired 资源最终获得回收资格
    progress                  （I6；M3 liveness 反证其必要性）。
```

定位声明：**P1–P5 的 normative 定义已由 semantic corrective 吸收进
ADR-PBK-001 §6（P5 以显式 progress 前提下的 conditional liveness 形式冻结）。
本节保留的是各条性质的形式化出处（invariant + mutation 反证），回答"为什么
知道这些条件必须成立"；本文不是第二份 normative 定义，措辞与 ADR 分歧时以
ADR 为准。**

## 14. Implementation mechanism — **DEFERRED**

```text
SEMANTIC PROTOCOL: ADR-PBK-001 §6 P1–P5（normative 冻结；非本记录的选择对象）
MECHANISM: DEFERRED（未选择任何机制）
当前工程倾向（非冻结，Phase D 可执行实验的默认起点）：
    原子不可变视图快照 + 引用计数式回收（ArcSwap 风格）
```

**为什么不现在冻结**：ADR §12 的研究顺序是 minimal PCM contract → direct
flow → publication 实验。执行模型（callback / blocking push / pull / hybrid，
§6 措辞刻意未定）、lease 粒度（per-quantum vs per-epoch）、参数更新路径（§7）
都会改变维度 A/N 的实际权重；在 Phase B/C 之前冻结容器选型违反 ladder，也
没有可执行证据支撑。

**为什么记录倾向**：矩阵在 Qianqian 真实形态（读者 1–2、发布低频、无分配 RT
路径、要求多代 overlap 与逐资源回收）下，候选 1 在 A/C/D/F/H/J/M/P 等维度
占优或并列最优；候选 2 的批次回收延迟与 guard 纪律、候选 4 的槽数上限
（恰是 M4 场景）、候选 3 的读路径 RMW 与控制侧轮询耦合、候选 5 的协议复杂度
都命中 Qianqian 的短板。**两条已声明的翻盘路径**：(a) §12 的 queued-reference
翻转风险——若预绑定引用占主导，行 A 排名塌缩；(b) 维度 O——候选 1 默认形态
允许最后 drop 者触发析构级联，必须叠加 §18.5 的 deferred-disposal 约束才
成立，该约束本身是候选 1 之上的额外机制而非其自带属性。倾向是"从哪开始
实验"，不是"已经赢了"。

## 15. Why（对应任务书的九个问题）

1. **哪个问题已经证明真实？** 发布后旧读者 overlap 期间释放资源 =
   use-after-release（M1）；半发布可见（M2）；retired 视图闭门缺失（M3
   safety+liveness）；多代记账捷径（M4）。
2. **哪个 invariant 是必要的？** P1–P5（全部有反例支撑其必要性）。
3. **哪些候选被淘汰？** 无一被本轮证据"淘汰"；RwLock/Mutex（读者可阻塞）与
   裸 AtomicPtr（无生命周期追踪）被 RT 约束与 M1 反例直接否决。
4. **为什么淘汰？** 见上；hazard/epoch/double-buffer 是"降优先级"而非淘汰——
   它们各自在别的负载形态下合理。
5. **倾向候选为什么适合 Qianqian？** 见 §14：读者少、发布稀疏、逐资源回收、
   多代 overlap、safe-Rust、与形式模型语义一一对应。
6. **倾向候选的代价是什么？** 借用槽/debt 机制带来最坏情形 Guard drop 的 1 次
   RMW；`load_full` 路径有 refcount 争用（长持有/预绑定场景需评估，见 §12
   翻转风险）；视图整体重建成本（每次 publication 都要构造新不可变快照——
   低频，可接受）；**最后引用 drop 触发整棵析构级联**——必须叠加 §18.5 的
   deferred-disposal 约束才能用于 RT 邻接路径（这是它相对 epoch/double-buffer
   的真实结构性劣势，维度 O）；依赖一个 crate 或自研等价物。
7. **哪些风险 TLA+ 没覆盖？** 内存序（模型是顺序语义）、reader 崩溃/停滞、
   ABA/槽位复用、真实线程调度延迟、queued reference 的实际载体（队列/句柄
   泄漏路径）、参数热更新与视图发布的关系（§7 边界）。
8. **哪些需要 Rust 并发测试 / sanitizer / Loom / stress？** P1–P5 的实现级
   验证：Loom 验证发布/获取/释放交错、TSan/ASan 压力测试、停滞读者注入、
   双发布链 stress（M4 场景）、Guard 泄漏检测。
9. **是否已有足够 evidence 冻结具体 mechanism？** 没有——语义协议够，机制
   裁决要等 Phase B/C 的执行模型事实。

## 16. Rejected alternatives（摘要）

- **RwLock/Mutex<Arc>**：读者可能阻塞/优先级反转，违反 RT 路径约束。
- **裸 AtomicPtr / 发布即可释放**：等价 M1，反例直接否决。
- **epoch/RCU（crossbeam-epoch）**：批次级回收延迟（专用 Collector 实例可把
  批次范围缩到 publication 子系统，但停滞延迟仍非逐资源）+ pin 纪律负担 +
  unsafe 面 + 原生 pin 无法表达 queued reference；Qianqian 读者规模吃不到
  它的规模红利。
- **double-buffer**：槽数上限与 M4 多代 overlap 冲突；周转等待把读者停滞放大
  成发布链阻塞；一致性依赖逐槽纪元标签（槽复用/ABA 邻接机制，模型未覆盖）。
- **hazard pointer**：读路径并不贵（无 RMW），真正的问题是退休簿记协议复杂度、
  unsafe 面、以及槽位无法预绑定 queued reference——为高并发读者设计，错配。
- **reader-count/lease**：可行但读路径 RMW、控制侧轮询与 RT 缓存行耦合是
  纯损耗；保留为"全 safe、零依赖"的 fallback（若自研 arc-swap 等价物不划算）。

## 17. Residual risks（机制 DEFERRED 期间的敞口）

- 倾向候选的实测读路径成本未测（无 benchmark——Phase D 补）。
- Guard 生命周期与实际 callback/worker 模型的贴合度未验证。
- `load_full`（长持有）路径的 refcount 争用未测。
- queued reference 的实际载体（队列、句柄、预绑定边）未进入任何实验。
- 内存序正确性（acquire/release 语义在真机上的保证）未验证。

## 18. Implementation consequences（对后续实验的约束）

1. Phase D 的可执行 publication 实验必须同时提供：P1 一致的原子发布（单句柄
   或版本校验式整体一致获取）、P2 闭门、P3 全代 quiescence 认证、M4 形态的
   多代 overlap 测试。
2. 任何"只查最新退休代"的记账捷径都是已知缺陷（M4 反例在案）。
3. 任何无协调的逐分量独立发布都是已知缺陷（M2 反例在案）。
4. 释放资格判定必须同时覆盖 active 执行与 queued reference。
5. **RT 路径不得成为最后引用的 drop 者**：最后 drop 会触发 View 及其全部
   资源的析构级联（递归 Drop + 堆释放），这是音频 callback 的铁律禁区。
   采用引用计数类机制时必须叠加 deferred-disposal（RT 侧把"可能成为最后
   引用"的句柄移交给控制侧回收队列后再退出），epoch/double-buffer 的
   collector/槽复用形态天然满足本条。
6. 实现级验证（Loom/TSan/stress）是 P1–P5 的实现侧验收，不重复 TLA 结论。

## 19. Follow-up executable experiments

```text
E1  Phase B minimal PCM contract（ADR §12）——决定 lease 粒度的前提
E2  Phase C direct flow——决定 callback/worker 执行模型
E3  Phase D publication 实验：以 §14 倾向为起点，按 §18 清单验收；
    若读路径实测不达标或执行模型不适配，按 §12 矩阵换轨并记录原因
E4  M4 场景 Rust stress（双发布链 + 慢读者）
E5  Loom 模型化 P1/P3（swap/acquire/drop 交错）
```

## 20. Reproduction commands

```bash
specs/realtime-publication/check.sh        # 本记录引用的全部形式化证据
specs/check.sh                             # 全仓形式化套件（含本套件）
```

逐 run 状态空间数据、反例轨迹、工具链指纹见
`specs/realtime-publication/README.md`。
