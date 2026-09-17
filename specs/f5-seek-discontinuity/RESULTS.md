# RESULTS — f5-seek-discontinuity

## 运行记录

- 日期：2026-09-17（F5-GATE）
- 工具：tla2tools v1.7.4（Xenophanes，sha256 校验）；java -XX:+UseParallelGC，4 workers
- 模块：`SeekDiscontinuity.tla`；bounds：`MAX_TAIL = 2`；safety-only（无 fairness）
- 判定：**BOUNDED-CLEAN — base PASS + 3 witness 可达 + 5 mutation 全部 COUNTEREXAMPLE-WITNESSED**
- 结果类：CHECKED-IN-MODEL（覆盖矩阵用语；不得升级为"架构已证明正确"）

```
== 正常模型（必须 PASS：TypeOK + 三条安全不变式，探索完成）
Base / safety（commit 后旧 PCM 不可能）                   PASS  997 states generated, 300 distinct states found, 0 states left on queue.
== witness 探针（必须 MUST-FAIL：可达性证明，防空洞不变式）
Witness / commit 可达                                          MUST-FAIL-OK（违反 ProbeNeverCommitted）
Witness / pre-commit 旧输出可达（合法）                 MUST-FAIL-OK（违反 ProbeNoOldDrainEver）
Witness / pre-commit edge 旧库可达                           MUST-FAIL-OK（违反 ProbeNoOldEdgeEver）
== 负控制（每个 mutation 必须被抓住）
Mutation / M1 CommitBeforeTailPurge（不等尾排空）         MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M2 CutMidWrite（staging 未丢弃）                 MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M3 ParkWhileHeld（held block 跨 park）             MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
Mutation / M4 StalePositionWriter（旧 basis 复用）          MUST-FAIL-OK（违反 InvPositionNoMixing）
Mutation / M5 CommitBeforeLanding（未 reposition）            MUST-FAIL-OK（违反 InvCommitPurged,InvStaleOutput）
F5-SEEK-DISCONTINUITY: ALL PASS
```

## 状态空间

- base：997 states generated / **300 distinct**（穷举完成，0 on queue）
- 每个 mutation / witness run 独立探索完成（"Finished in" 收尾，fail-closed）

## 判读

1. **Base PASS**：在协议护栏（park 不持块、worker 串行化点第 2 相 cut、
   commit = landing ∧ edge 干净 ∧ 尾排空 ∧ parked、stop 恒赢）下，
   `InvStaleOutput` / `InvPositionNoMixing` / `InvCommitPurged` 在全部
   300 个可达状态上成立。
2. **Witness 全部可达**：commit、pre-commit 旧输出（合法）、pre-commit
   edge 旧库都真实出现——不变式不是空洞的，它禁的是 commit 之后的旧输出。
3. **Mutation 全部被抓住**：五条护栏各被一个负控制证明承重——
   - M1（commit 不等尾排空）、M2（staging 未丢弃就 cut）、M3（held block
     跨 park）、M5（未 reposition 就 commit）都让旧 PCM 在 commit 后复活；
   - M4（stale writer 复用旧 basis）单独击穿 D14.8 no-mixing。
   - 其中 M1/M2/M3/M5 同时先触发 InvCommitPurged（commit 前置自检报警），预期。

## 边界（结论的适用范围）

- 本结论只在模型的显式 abstraction 下成立：布尔化的三库、无容量/游标、
  无 fairness、无 WASAPI 流状态机、无请求语法。它证明的是**协议形状的
  safety 护栏集充分**，不是 production 实现的正确性；实现门还需 loom
  （真 edge 并发）+ 物理 gate（audible cutover）。
- 本模型不是第二份 authority；语义真相在 ADR-PBK-002 D14.5/D14.8。
