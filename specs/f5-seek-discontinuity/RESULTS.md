# RESULTS — f5-seek-discontinuity（含 #289 D-C pending-fatal 扩展）

## 运行记录

- 本轮日期：2026-10-17（D-C EXTEND_EXISTING_MODEL 执行轮 + 同日 PR #292
  外部终审修正轮；receipt
  [issues/289#issuecomment-6229911387](https://github.com/jnhu76/qianqian/issues/289#issuecomment-6229911387)）
- 历史基线：2026-09-18（F5-GATE-CORRECTIVE-1：三分类 provider outcome 模型；
  672 distinct states / 7 mutation / 6 witness——原始 R0/F5-GATE 证据与旧
  状态空间保留为历史记录，见文末）
- 工具：tla2tools v1.7.4（Xenophanes，sha256 `936a2620…` 校验）；
  TLC2 Version 2.19 (rev 5a47802)；java 17.0.20.1，-XX:+UseParallelGC，
  4 workers
- 模块：`SeekDiscontinuity.tla`；bounds：`MAX_TAIL = 2`；safety-only
  （无 fairness；不以任何 fairness 假设代替真实同步事实）
- 仓库基点：BASE_SHA `0e43ef0cbac6fe21d0cd83e14875ace98c73501a`（模型冻结
  时点 origin/main；生产锚点行号见 PENDING_FATAL_DAG.md §0）
- 判定：**BOUNDED-CLEAN — base PASS（11 条不变式）+ 12 witness 全部可达 +
  11 mutation 全部 COUNTEREXAMPLE-WITNESSED（M1–M7 无回归；D-T1..D-T4 新增；
  2026-10-17 外部终审修正轮 [P1 原子性拆分 / P2 seek 窗口显式化 /
  P2 D-T4 单缺陷化] 后复跑确认）**
- 结果类：CHECKED-IN-MODEL（覆盖矩阵用语；不得升级为"架构已证明正确"）

## 终审修正轮（2026-10-17，PR #292 外部独立终审 REVISE → 修复）

外部终审（1×P1、2×P2，全文见 PR #292 审查记录）判定 FULL_CAUSAL_FIDELITY
= REVISE；三项发现的核实结论与修复：

1. **P1（核实成立）——失败路线原子性融合错误**。原 `ReadSurfacesFatal`
   把 `decode_failed`（publication+settlement）、`edge.fail()`、
   `worker_exited()` 融合为一个动作；生产中只有 publication+settlement 在
   同一 publish_evidence 锁程（completion.rs:1319-1331），`edge.fail`
   （session.rs:898，edge lock）与退出漏斗（session.rs:928）都在锁程之外，
   且 `publish_evidence` 对已 settle 的 outcome 直接丢弃后续 evidence
   （completion.rs:1322-1324）——原模型无条件把「物理即将退出」编码为
   「workerGone 已发布」是伪影。由此丢失的合法交错：失败 settlement 提交
   `Failed{decode}` 之后、worker `edge.fail()` 之前，request_stop 照常记录
   intent 并执行数据面 stop（「after settlement: nothing changes」指终局
   不改写，不是动作不发生）→ `committedOutcome=failedDecode ∧
   edgeTerminal=stopped` 是合法终态格（edge.fail first-wins 不覆盖
   Stopped；D11 rule 1 不重分类 Failed）。
   **修复**：拆为 `FatalFailurePublish`（publication+settlement 原子；
   `SeekMutatedThenFailed` 同形拆分）→ `WorkerEdgeFailAdvance`（first-wins
   边推进）→ `WorkerExitFunnel`（`workerGone' = (committedOutcome =
   "none")`——失败路线恒 FALSE，忠实转写 first-wins 丢弃）；
   `RecordStopIntent` 移除 settle 门槛（生产 request_stop any state）。
   **验收**：新 witness `W_FAILED_DECODE_WITH_STOPPED_EDGE` 可达（反例
   6 态＝初始态+5 动作步，TLC 计数含初始态；归档
   counterexample-WitnessFailedDecodeStoppedEdge.log，轨迹即终审描述的
   交错序列）；first-wins 两半不变式全绿。
2. **P2（核实成立）——eligibility 与 provider begin 的窗口靠并集论证**。
   原模型以「intent 落点非确定性」论证 `WorkerSeek` 原子 guard 的等价性，
   终审判定该等价性未被证明（正确）。生产序列 session.rs:620-650：check
   通过 → intent 落入 → edge 仍 Open → 调用照常发出（调用点之后无复核）。
   **修复**：显式拆分 `SeekEligibilityPass`（检查原子通过，消费
   seek_aborted 裁决）→ `SeekProviderBegin`（无 stopIntent/edgeTerminal
   复核，调用照常发出）；新 worker 值 `seek_eligible`；原 A-2 并集论证
   从模型与 PENDING_FATAL_DAG §1 W-B 撤除。
   **验收**：base 状态空间 6330 → 7470 distinct（增量即窗口状态）；
   D-T4 全链反例经过该窗口（SeekEligibilityPass → SeekProviderBegin 两步
   在轨迹中可见）；原 A-2 表达注记所指「检查之间落 intent」现在直接可交错。
3. **P2（核实成立）——D-T4 双缺陷注入稀释反例**。原注入同时改 latch 擦除
   与 worker 去向（→idle 旧循环复跑），违反「one mutation = one defect」
   （与 D-T3 前提修订同原则：缺陷机制应单因子）；且全称不变式在 landing
   时刻提前触发，归档反例未走完 surfacing。
   **修复**：D-T4 收敛为单缺陷（仅跳过擦除；去向保持正常 repositioned）；
   `FatalFailurePublish` 增加 repositioned 臂；新增诊断性命名检查
   `DiagnosticNoFailurePublicationAfterLanding`
   （NoOldFatalAfterApplied 的 publication 半边，仅 D-T4 诊断 cfg 引用，
   基线 11 条不含——AGENTS.md「named as diagnostics」条款）。
   （复审更正：初版给 repositioned 臂加了 committed 门槛并称「生产恢复
   生产在 commit 路由之后」——复审指出 cutover wait 的 Aborted 分支
   （session.rs:721→:750→:775-778）允许未 commit 恢复读，该说法夸大生产；
   已去掉门槛。基模型内 latch×repositioned 组合仍不可达（Applied 必擦除），
   base 状态空间逐点不变（22349/7470，f5-base-rewiden-recheck-run5c.log）。）
   **验收**：D-T4 反例走完全程 8 态链（归档
   counterexample-DT4-surfacing-chain-v2.log；初版 10 态经 CommitCut 的
   链在 committed 门槛下取得，v1 日志保留为 superseded）：ReadPartialSuccess
   → … → SeekEligibilityPass → SeekProviderBegin → WorkerCut（latch 未擦）
   → FatalFailurePublish（旧 fatal 在新 continuation surfaced）；全称版
   `NoOldFatalAfterApplied` 的违规由披露运行 counterexample-DT4-full-
   invariants-v2.log 另行记录（终止于 landing 时刻，与修复前同形）。

修复后全套件复跑（f5-full-suite-run5-finalrev-fixes.log）：**首跑全绿**。
无生产代码改动；模型语义锚点与 BASE_SHA 行号全部未变（PENDING_FATAL_DAG
REV.2 只更正原子性裁定与表达注记）。

```
== 正常模型（必须 PASS：TypeOK + 基线五条 + pending-fatal 五条安全不变式，探索完成）
Base / safety（commit 后旧 PCM 不可能；pending-fatal 格合法） PASS  22349 states generated, 7470 distinct states found, 0 states left on queue.
== witness 探针（必须 MUST-FAIL：可达性证明，防空洞不变式）
Witness / commit 可达                                          MUST-FAIL-OK（违反 ProbeNeverCommitted）
Witness / pre-commit 旧输出可达（合法）                 MUST-FAIL-OK（违反 ProbeNoOldDrainEver）
Witness / pre-commit edge 旧库可达                           MUST-FAIL-OK（违反 ProbeNoOldEdgeEver）
Witness / seek 拒绝路径可达                                MUST-FAIL-OK（违反 ProbeNoRefusalEver）
Witness / destructive failure 路线可达                       MUST-FAIL-OK（违反 ProbeNoDestructiveEver）
Witness / 余量在外 seek 可达                               MUST-FAIL-OK（违反 ProbeNoPartialSeekEver）
Witness / Applied 擦除→新 continuation Completed 可达（L1） MUST-FAIL-OK（违反 ProbeNoAppliedErasureCompleted）
Witness / Refused 后 latch 保留可达（L2）                 MUST-FAIL-OK（违反 ProbeNoRefusedRetention）
Witness / 真实 stop-exit 的 Stopped 可达（L4）            MUST-FAIL-OK（违反 ProbeNoStoppedEver）
Witness / pending fatal surfacing 可达（L5）                 MUST-FAIL-OK（违反 ProbeNoFatalSurfacing）
Witness / MutatedThenFailed 可达（L3）                       MUST-FAIL-OK（违反 ProbeNoMutatedExit）
Witness / Failed{decode}×stopped-edge 合法交错可达（终审 P1） MUST-FAIL-OK（违反 ProbeNoFailedDecodeStoppedEdge）
== 负控制（每个 mutation 必须被抓住）
Mutation / M1 CommitBeforeTailPurge（不等尾排空）         MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M2 SeekMidWrite（staging 未丢弃）                MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M3 ParkWhileHeld（held block 跨 park）             MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M4 StalePositionWriter（旧 basis 复用）          MUST-FAIL-OK（违反 InvPositionNoMixing）
Mutation / M5 CommitBeforeLanding（未 reposition）            MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M6 RefusalDropsRemainder（拒绝丢余量）         MUST-FAIL-OK（违反 InvRefusalContentContinuous）
Mutation / M7 ResumeAfterMutatedSeek（失败后复活）        MUST-FAIL-OK（违反 InvFailClosed）
Mutation / D-T1 OldCursorResumeAfterApplied（cut 后旧 cursor 复活） MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput,NoOldCursorResumeAfterApplied,NoOldFatalAfterApplied）
Mutation / D-T2 RefusedClearsLatch（拒绝误擦 latch）       MUST-FAIL-OK（违反 RetainLatchOnRefused）
Mutation / D-T3 LoopTopStopExit（取消位伪造 stop 退出） MUST-FAIL-OK（违反 TerminalCellConformance）
Mutation / D-T4 AppliedKeepsLatch（单缺陷；surfacing 全程链） MUST-FAIL-OK（违反 DiagnosticNoFailurePublicationAfterLanding）
F5-SEEK-DISCONTINUITY: ALL PASS
```

（以上为终审修正轮 run5 输出；修复前审查修正轮 run3/run4 的 11-witness
版本输出见证据目录，base 计数 15667/6330。）

### 复审轮（同日，两个 fresh-context 复审 agent）

- **Causal-fidelity 复审：APPROVE**。三项终审修复全部对照源码核实为
  忠实（publish_evidence 锁程 / request_stop any-state / 三次独立单锁
  eligibility / D-T4 单缺陷路径逐一验证）；独立重跑全套件与归档 run5
  **逐行一致**（22349/7470；12/12 witness、11/11 mutation、D-T4 恰好
  击中诊断不变式）；对 TerminalCellConformance 全部格逐格重推导无反例
  （含 relaxed RecordStopIntent 与 (2g)/(2e) 的相容性论证）。附带发现：
  §8 D-T4 行未更新（P2，见下）、repositioned 臂 committed 门槛的说法
  夸大生产（P2，见下）、3×P3（计数口径 / :675 行号 / 既有惰性
  drain-verdict 记录注记）。
- **Evidence-integrity 复审：REVISE（全部 docs-only）**。套件独立复现
  逐数吻合；证据归档四文件全部核verified（DT4 链逐状态比对）；发现
  §8 D-T4 行与 REV.2 头声明矛盾（其 P1）、RESULTS 证据清单把
  run1/run2 的 base 计数误并入 15667/6330（P2）、2×P3。
- **本轮 delta 修复（docs + 1 行模型 guard）**：§8 D-T4 行改写为单缺陷
  + 诊断目标 + 8 态直达链；FatalFailurePublish 的 repositioned 臂去掉
  committed 门槛（Aborted 恢复读无需 commit，session.rs:721→:750→
  :775-778），base 逐点复核不变（run5c）；行号引用 :671-672/:673 更正；
  witness 计数统一为 TLC 口径（6 态＝初始+5 步）；workerGone 动作计数
  更正（4 个无条件真实退出 + 条件漏斗）；run1/2 计数与 superseded 文件
  标注；LegObserve 的 settle 后 drain 记录惰性注记（既有形状，P3）。
  D-T4 两份证据运行按「重跑=新文件」归档为 *-v2。

## 状态空间

- base：22349 states generated / **7470 distinct**（穷举完成，0 on queue；
  终审修正轮前为 15667/6330——增量来自 P1 失败路线三步拆分、P2
  seek_eligible 中间位置与 RecordStopIntent 去 settle 门槛后的
  post-settlement stop 组合，全部是显式模型窗口的合法交错状态）；
  相对 2026-09-18 基线（2205/672）的增长来自本轮新增的真实维度：
  pendingFatal × stopIntent/edgeTerminal（intent→edge stop 窗口）×
  workerGone/exitPath（真实退出位置）× drainVerdict/committedOutcome
  （terminal 投影）+ witness 精度见证——全部是有界交错/见证的真实增长，
  未融合任何可交错动作来缩小空间，也未降低 bound（MAX_TAIL 保持 2）
- 每个 mutation / witness run 独立探索完成（"Finished in" 收尾，fail-closed）
- TLC `-continue` 判读注记（run5 起如实声明）：`-continue` 下 violating
  state 被报告但不再展开，故 mutation 行报告的违规集合是「生成状态上的
  完备集」；部分 mutation 的违规集合大于其目标（如 D-T1 连带
  InvCommitPurged/InvStaleOutput/NoOldFatalAfterApplied——后者经「cut 后
  cutRequested 未消费即可再 eligibility → MutatedThenFailed」的
  mutant-only 路径，基模型不可达；M7 的失败后复活若再遇 stop，其退出格
  也落在冻结矩阵外——同为 mutant-only 后果，目标 InvFailClosed 在注入点
  最近处先击中）。这属于负控制的附带诊断，不影响「目标不变式被击中」的
  判定。
- 反例轨迹归档：D-T1..D-T4 与终审 P1 witness 的 TLC 反例逐状态保存在执行轮
  本地证据目录 `/home/hoo/Source/qianqian-289-evidence/`（未入库；同轮其他
  证据：f5-full-suite-run.log/run2.log = 执行轮前两跑（base 13117/5216）、
  run3/run4.log = 执行轮定稿与审查修正轮（base 15667/6330）、
  f5-full-suite-run5-finalrev-fixes.log = 终审修正轮全套件（base
  22349/7470，本文引用数字；其头部 MODEL_SHA 记录的是 fbb3c64+未提交修复
  ——提交在其后 14 分钟完成，证据身份由两个复审 agent 在 2a516bf 上的独立
  复跑闭合：逐行一致）、f5-base-comment-recheck-run5b.log /
  f5-base-rewiden-recheck-run5c.log = 注释修正与复审去门槛后的 base 逐点
  复核（均 22349/7470）、base-attempt1-RetainLatchOnRefused-violated.log
  = 首跑红色日志、counterexample-DT1/DT2/DT3*.log = 各反例逐状态轨迹、
  counterexample-DT3-committed-cell-chain.log = D-T3 的 commit 完整链诊断
  运行、counterexample-DT4AppliedKeepsLatch.log = **superseded**（双缺陷
  模型的修复前反例，仅历史）、counterexample-DT4-surfacing-chain.log =
  **superseded**（committed 门槛版 10 态链）、
  counterexample-DT4-surfacing-chain-v2.log = **现行 D-T4 正式证据**
  （8 态直达链）、counterexample-DT4-full-invariants.log = **superseded**、
  counterexample-DT4-full-invariants-v2.log = 现行全称不变式披露运行
  （终止于 landing 时刻）、
  counterexample-WitnessFailedDecodeStoppedEdge.log = 终审 P1 合法交错
  witness 反例（6 态＝初始+5 步）、lefthook-pre-push-all*.log = 本地 gate
  日志、dt-run-* = 首次 D-T cfg 配置错误的失败现场）。诊断运行的复现配方：
  拷贝本目录 .tla + mutations/DT3LoopTopStopExit.cfg 后，把
  `TerminalCellConformance` 替换为仅保留其 (2e) committed-cell 合取项的形式
  （PENDING_FATAL_DAG §8 注明「非 gate 配置」）——gate 配置本身不变。
  2026-10-17 审查 F3 修复注记。

## 判读

1. **Base PASS**：在冻结护栏（park 不持块、worker 串行化点先 song_seek
   且 eligibility/begin 分离、begin/返回分离、RefusedUnchanged 即 pre-cut
   惰性且保留余量补完且**不触碰 latch**、MutatedThenFailed 即 fail-closed
   且绝不恢复旧游标生产、Applied 即 purge+landing+**latch 擦除**、stop
   intent 与 edge stop 两步分离、失败路线 publication/settlement →
   edge.fail → 漏斗三步分离、worker 只在真实退出位置退出、canceled seek
   只是 bookkeeping、terminal commit 按冻结 resolver 次序）下，全部 11 条
   不变式在 7470 个可达状态上成立。pending-fatal 的核心事实被模型逐格复现：
   - default position 上 latch 胜 stop（read 无前置 terminal 检查，
     worker_failure 先于一切 stop-intent 咨询）；
   - Applied 合法吞没旧 fatal（L1 witness：latch→擦除→新 continuation→
     Completed 可达——scoped safety 未误杀新 continuation）；
   - Refused 保留 latch（L2）；Mutated fail-closed（L3）；真实退出位上的
     Stopped 可达（L4）；surfacing-Failed 可达（L5）；失败 settlement 后
     edge 被 stop 抢先终止的 failedDecode×stopped 合法格可达（终审 P1
     witness——first-wins 两半各自承重）。
2. **Witness 全部可达**：12 条探针证明不变式非空洞（注：WitnessSeekDestructive
   的探针变量由基线 episodeFailed 换为 workerFailurePublished——超集；
   mutated 路径的精确可达由 W_MUTATED_THEN_FAILED 另行钉住）——特别是
   W_APPLIED_ERASURE_COMPLETED（强制验收项）：作用域化安全
   （ScopedLatchedFatalNoComplete 只约束 ¬landing 的旧 continuation）没有
   把「曾 latch ⇒ 永不 Completed」的过强断言引入模型。
3. **Mutation 全部被抓住，M1–M7 无回归**：
   - 基线七条护栏各被负控制证明承重（语义同 2026-09-18 基线；M2/M3/M5
     的被击路径在新结构下复现）；
   - D-T1（cut 后旧 cursor 生产复活）被 NoOldCursorResumeAfterApplied
     抓住，且连带触发 InvCommitPurged/InvStaleOutput——注入的旧行为
     真实把旧 PCM 写回了数据面（行为级击中，非仅 ghost 位）；
   - D-T2（refused 误擦 latch）被 RetainLatchOnRefused 干净抓住；
   - D-T3（canceled-seek 的 bookkeeping 位置被改成 worker 退出——
     pre-R1.1 错误措辞的复活）被 TerminalCellConformance 干净抓住。归档了
     两条反例：gate 配置下最短反例在伪造退出处被「冻结退出清单」半边抓住
     （6 态）；诊断运行（仅保留 committed-cell 半边，非 gate 配置）强制
     反例走完全链（9 态）：RequestSeek → RecordStopIntent（intent 可见而
     edge 仍 open 的真实窗口）→ EdgeStopFromStop → LegObserveAborted →
     伪造退出 → TerminalCommit=stopped——committed outcome 经一个生产中
     不存在的退出位置落格，违反冻结矩阵。latch 与该缺陷机制正交（有 latch
     时合法格是 failedDecode；无 latch 且余量在外时合法格是经由
     remainderBreak 的 Stopped——两种情况下该 exitPath 都在矩阵外），
     前提修订已记录在 PENDING_FATAL_DAG §8；
   - D-T4（Applied 不擦 latch——终审 P2 后为单缺陷，去向保持正常
     repositioned）被诊断检查 DiagnosticNoFailurePublicationAfterLanding
     抓住，反例走完 8 态 surfacing 全程链：ReadPartialSuccess → … →
     SeekEligibilityPass → SeekProviderBegin → WorkerCut（landing ∧ latch
     未擦）→ FatalFailurePublish（旧 fatal 在新 continuation surfaced →
     非法 failure publication；无需 commit——Aborted 恢复读路径合法）。
     全称版 NoOldFatalAfterApplied 在同模型中亦被违反（披露运行
     counterexample-DT4-full-invariants-v2.log，反例终止于 landing 时刻
     ——其保留半边先行触发，走不完链，故正式证据采用诊断版全程链，两份
     日志都已归档）。

## 校准关系（如实声明）

E1-ABI（双腿绿）与 E1-WORKER（W1–W6 落入 §2.3.1 预测格）是 **Rust 真实
行为校准**（#287 证据，2026-10-16 记录；D-C receipt 引用）：它们证明模型
的 ground truth 与真实 SongCore latch 行为、真实 Session/worker 退出面
一致。本模型的 TLC 结果**不取代**它们，也不是 production 正确性证明；
stop-race 的一半仍以 Rust 验证为主（E1-WORKER 观察真实锁与退出面，
一次一个调度；TLC 穷举的是有界抽象交错矩阵）。

## 边界（结论的适用范围）

- 本结论只在模型的显式 abstraction 下成立：布尔化三库、无容量/游标、
  无 fairness、无 WASAPI 流状态机、无请求语法、**设备中断不建模**
  （resolver 的 Failed{device} 格模型内不可达，分支保留为冻结表转写）、
  **新 continuation 自身无解码故障**（因此 NoOldFatalAfterApplied 是模型内
  全称命题，不是生产全称命题——生产上新 continuation 的真实故障走正常
  Failed 路线，与本模型的 latch 归因抽象正交）、position no-mixing 的
  base 保证仍是构造性的（单写者），M4 只是注入缺陷写者的诊断。
  它证明的是**协议形状的 safety 护栏集在 pending-fatal 窗口内仍充分**，
  不是 production 实现的正确性。
- 本模型不是第二份 authority；语义真相在 ADR-PBK-002 D14.5/D14.8 + D11
  与 reconciliation §2.3/§2.3.1。committedOutcome 是 D11 的最小受约束
  投影（TERMINAL_DECISION_MAP），不复制 episode-terminal-settlement 的
  settlement 模型，不产生第二套裁决语义。
- D-C 执行轮的调试记录：base 首跑曾红一次——RetainLatchOnRefused 初版
  写成 `(seekFailed ∧ ¬landing) ⇒ pendingFatal`，被「从未 latch 过的普通
  refusal」合法反例击中（模型缺陷注入不可能、不变式过强）；修正为以
  fatalLatchedEver 见证「refusal 时点上 latch 已存在」。首跑日志按证据
  契约归档（base-attempt1-RetainLatchOnRefused-violated.log），未以重跑
  洗绿。
- 历史记录：早期记录（旧两相序模型 997/300；2026-09-17 refusal-first 模型
  1349/426；2026-09-18 三分类模型 2205/672、7 mutation、6 witness）已被
  本轮扩展模型取代，保留为历史基线，不再作为当前记录。
