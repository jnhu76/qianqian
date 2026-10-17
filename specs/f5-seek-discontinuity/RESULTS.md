# RESULTS — f5-seek-discontinuity（含 #289 D-C pending-fatal 扩展）

## 运行记录

- 本轮日期：2026-10-17（D-C EXTEND_EXISTING_MODEL 执行轮；receipt
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
- 判定：**BOUNDED-CLEAN — base PASS（11 条不变式）+ 11 witness 全部可达 +
  11 mutation 全部 COUNTEREXAMPLE-WITNESSED（M1–M7 无回归；D-T1..D-T4 新增；
  2026-10-17 审查修正轮后复跑确认）**
- 结果类：CHECKED-IN-MODEL（覆盖矩阵用语；不得升级为"架构已证明正确"）

```
== 正常模型（必须 PASS：TypeOK + 基线五条 + pending-fatal 五条安全不变式，探索完成）
Base / safety（commit 后旧 PCM 不可能；pending-fatal 格合法） PASS  15667 states generated, 6330 distinct states found, 0 states left on queue.
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
Mutation / D-T4 AppliedKeepsLatch（旧 fatal 落入新 continuation） MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput,NoOldCursorResumeAfterApplied,NoOldFatalAfterApplied）
F5-SEEK-DISCONTINUITY: ALL PASS
SUITE_RC=0
```

## 状态空间

- base：15667 states generated / **6330 distinct**（穷举完成，0 on queue；
  审查修正轮加入 refusedWhileLatched 见证前的首轮为 13117/5216）；
  相对 2026-09-18 基线（2205/672）的增长来自本轮新增的真实维度：
  pendingFatal × stopIntent/edgeTerminal（intent→edge stop 窗口）×
  workerGone/exitPath（六个真实退出位置）× drainVerdict/committedOutcome
  （terminal 投影）+ witness 精度见证——全部是有界交错/见证的真实增长，
  未融合任何可交错动作来缩小空间，也未降低 bound（MAX_TAIL 保持 2）
- 每个 mutation / witness run 独立探索完成（"Finished in" 收尾，fail-closed）
- 反例轨迹归档：D-T1..D-T4 的 TLC 反例逐状态保存在执行轮本地证据目录
  `/home/hoo/Source/qianqian-289-evidence/`（未入库；同轮其他证据：
  f5-full-suite-run.log / f5-full-suite-run2.log / f5-full-suite-run3.log /
  f5-full-suite-run4.log = 修复 cfg 前后、审查修正轮与幸存语义见证修正
  共四次全套件完整输出（RESULTS 引用的数字来自 run3/run4，二者 base
  计数一致：15667/6330）、base-attempt1-RetainLatchOnRefused-violated.log = 首跑红色日志、
  counterexample-DT*.log = 四条反例逐状态轨迹、counterexample-DT3-committed-
  cell-chain.log = D-T3 的 commit 完整链诊断运行、lefthook-pre-push-all*.log
  = 本地 gate 日志、dt-run-* = 首次 D-T cfg 配置错误的失败现场）。诊断运行的
  复现配方：拷贝本目录 .tla + mutations/DT3LoopTopStopExit.cfg 后，把
  `TerminalCellConformance` 替换为仅保留其 (2e) committed-cell 合取项的形式
  （PENDING_FATAL_DAG §8 注明「非 gate 配置」）——gate 配置本身不变。
  2026-10-17 审查 F3 修复注记。

## 判读

1. **Base PASS**：在冻结护栏（park 不持块、worker 串行化点先 song_seek
   且 begin/返回分离、RefusedUnchanged 即 pre-cut 惰性且保留余量补完且
   **不触碰 latch**、MutatedThenFailed 即 fail-closed 且绝不恢复旧游标
   生产、Applied 即 purge+landing+**latch 擦除**、stop intent 与 edge stop
   两步分离、worker 只在六个真实位置退出、canceled seek 只是 bookkeeping、
   terminal commit 按冻结 resolver 次序）下，全部 11 条不变式在 6330 个
   可达状态上成立。pending-fatal 的核心事实被模型逐格复现：
   - default position 上 latch 胜 stop（read 无前置 terminal 检查，
     worker_failure 先于一切 stop-intent 咨询）；
   - Applied 合法吞没旧 fatal（L1 witness：latch→擦除→新 continuation→
     Completed 可达——scoped safety 未误杀新 continuation）；
   - Refused 保留 latch（L2）；Mutated fail-closed（L3）；真实退出位上的
     Stopped 可达（L4）；surfacing-Failed 可达（L5）。
2. **Witness 全部可达**：11 条探针证明不变式非空洞（注：WitnessSeekDestructive
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
   - D-T4（Applied 不擦 latch 且旧循环复跑）被 NoOldFatalAfterApplied
     抓住——反例终止于「landing ∧ pendingFatal 残留」状态（不变式的保留
     半边，2026-10-17 审查 F1 修正）；surfacing 随后必然发生（ReadSurfacesFatal
     的 guard 在该状态直接放行），但归档轨迹未走完那一步，如实声明。

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
