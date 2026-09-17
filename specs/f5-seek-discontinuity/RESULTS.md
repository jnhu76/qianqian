# RESULTS — f5-seek-discontinuity

## 运行记录

- 日期：2026-09-18（F5-GATE-CORRECTIVE-1：三分类 provider outcome 模型）
- 工具：tla2tools v1.7.4（Xenophanes，sha256 校验）；java -XX:+UseParallelGC，4 workers
- 模块：`SeekDiscontinuity.tla`；bounds：`MAX_TAIL = 2`；safety-only（无 fairness）
- 判定：**BOUNDED-CLEAN — base PASS + 6 witness 可达 + 7 mutation 全部 COUNTEREXAMPLE-WITNESSED**
- 结果类：CHECKED-IN-MODEL（覆盖矩阵用语；不得升级为"架构已证明正确"）

```
== 正常模型（必须 PASS：TypeOK + 五条安全不变式，探索完成）
Base / safety（commit 后旧 PCM 不可能）                   PASS  2205 states generated, 672 distinct states found, 0 states left on queue.
== witness 探针（必须 MUST-FAIL：可达性证明，防空洞不变式）
Witness / commit 可达                                          MUST-FAIL-OK（违反 ProbeNeverCommitted）
Witness / pre-commit 旧输出可达（合法）                 MUST-FAIL-OK（违反 ProbeNoOldDrainEver）
Witness / pre-commit edge 旧库可达                           MUST-FAIL-OK（违反 ProbeNoOldEdgeEver）
Witness / seek 拒绝路径可达                                  MUST-FAIL-OK（违反 ProbeNoRefusalEver）
Witness / destructive failure 路线可达                       MUST-FAIL-OK（违反 ProbeNoDestructiveEver）
Witness / 余量在外 seek 可达                               MUST-FAIL-OK（违反 ProbeNoPartialSeekEver）
== 负控制（每个 mutation 必须被抓住）
Mutation / M1 CommitBeforeTailPurge（不等尾排空）         MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M2 SeekMidWrite（staging 未丢弃）                 MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M3 ParkWhileHeld（held block 跨 park）             MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M4 StalePositionWriter（旧 basis 复用）          MUST-FAIL-OK（违反 InvPositionNoMixing）
Mutation / M5 CommitBeforeLanding（未 reposition）            MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M6 RefusalDropsRemainder（拒绝丢余量）         MUST-FAIL-OK（违反 InvRefusalContentContinuous）
Mutation / M7 ResumeAfterMutatedSeek（失败后复活）        MUST-FAIL-OK（违反 InvFailClosed）
F5-SEEK-DISCONTINUITY: ALL PASS
```

## 状态空间

- base：2205 states generated / **672 distinct**（穷举完成，0 on queue）
- 每个 mutation / witness run 独立探索完成（"Finished in" 收尾，fail-closed）

## 判读

1. **Base PASS**：在三分类冻结护栏（park 不持块、worker 串行化点先
   `song_seek` 后才做唯一的 worker 侧 edge purge、RefusedUnchanged 即
   pre-cut 惰性且保留余量原样补完、MutatedThenFailed 即既有
   decode-failure 路线且绝不恢复旧游标生产、commit = landing ∧ edge 干净
   ∧ 尾排空 ∧ parked、stop 恒赢）下，`InvStaleOutput` /
   `InvPositionNoMixing` / `InvCommitPurged` / `InvRefusalContentContinuous`
   / `InvFailClosed` 在全部 672 个可达状态上成立。
2. **Witness 全部可达**：commit、pre-commit 旧输出（合法）、pre-commit
   edge 旧库、RefusedUnchanged 拒绝路径、destructive failure 路线、
   writing_partial（余量在外）进入的 seek 都真实出现——不变式不是空洞的：
   它禁的是 commit 之后的旧输出、拒绝路径的内容损失、失败路线的伪装复活。
3. **Mutation 全部被抓住**：七条护栏各被一个负控制证明承重——
   - M1（commit 不等尾排空）、M2（staging 未丢弃就 cut）、M3（held block
     跨 park）、M5（未 reposition 就 commit）都让旧 PCM 在 commit 后复活；
   - M4（stale writer 复用旧 basis）单独击穿 D14.8 no-mixing；
   - M6（refusal 丢弃保留余量）击穿 InvRefusalContentContinuous——
     refused 输出与无 seek 对照不连续（E2 drop-remainder must-fire
     控制的模型对应物）；
   - M7（destructive failure 后恢复旧游标生产）击穿 InvFailClosed——
     正是 round-3 review MAJOR-2 指出的 pre/post 无 cutover 混合形状。
   - 其中 M1/M2/M3/M5 同时先触发 InvCommitPurged（commit 前置自检报警），预期。

## 边界（结论的适用范围）

- 本结论只在模型的显式 abstraction 下成立：布尔化的三库、无容量/游标、
  无 fairness、无 WASAPI 流状态机、无请求语法；position no-mixing 的
  base 保证是构造性的（单写者），M4 只是注入缺陷写者的诊断
  （见模块头诚实注记）。它证明的是**协议形状的 safety 护栏集充分**，
  不是 production 实现的正确性；实现门还需 loom（真 edge 并发）+
  物理 gate（audible cutover）。
- 本模型不是第二份 authority；语义真相在 ADR-PBK-002 D14.5/D14.8。
- 早期记录（旧两相序模型：997/300、3 witness、M2 CutMidWrite；以及
  2026-09-17 的 refusal-first 模型：1349/426、4 witness、5 mutation）已被
  本三分类模型取代，不再保留为当前记录。
