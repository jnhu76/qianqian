# H0 公开证据工件

这些是本仓库自己的执行证据，不是外部 failure mining，也不是产品 authority。

- `h0-independent-audit.zip`：原始独立审计归档，内容不重写。执行器历史分类 65 valid / 2 invalid；共享 target 的 provenance 限制已在当前报告说明，本轮关键旧绿灯另外复现。
- `h0-adversarial-remediation.zip`：修复与重新复现的 101 次执行，含逐次原日志、metadata、patch、函数体 snapshots、各失败分类、restore proof 与脚本。包括 1 次构建身份无效、1 次编译失败、1 次 clippy failure、1 次 F5 tooling failure；这些不计语义 RED。
- `SHA256SUMS`：两个 ZIP 的 SHA256；每个 ZIP 内另有文件级 manifest。

当前索引：[执行](../h0-adversarial-execution-index.csv)、[mutation](../h0-adversarial-mutation-matrix.csv)、[18 项处置](../h0-disposition-ledger.csv)、[中文说明](../h0-adversarial-remediation.zh.md)。复现方法在修复 ZIP 的 README.zh.md 中，聚合旧 runner 子进程 trace 未全部保留的限制也明确记载。

索引列语义：`SUBJECT_SHA` 是**运行发起时的仓库 HEAD 指针**（test-then-commit 工作流下实际执行树可能新于该标签）；每次 run 的真实源码身份以包内 `metadata.json` 的全仓 `source_hashes` 为准。详见[中文说明的 G4 补记第 1 节](../h0-adversarial-remediation.zh.md)。既有 CSV 行与日志不改写。

最后的文档发布 HEAD gate 是归档提交后执行，完整原日志与实际 HEAD 关系另保存在方法 issue #262 的回读记录；不能将其混入较早归档的 RUN_ID。该最终 HEAD 本地全量 gate 的 RUN_ID 为 `20261012T180342-publication-head-full-pre-push`（未入本目录索引，引用见中文说明 G4 补记第 4 节）。
