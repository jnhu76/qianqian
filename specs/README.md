# specs/ — 当前验证证据

> **STATUS: CURRENT EVIDENCE ONLY。**
>
> 本目录只保存**今天仍然用于验证当前 architecture / production reality 的
> verification artifacts**。每个幸存套件都能回答：验证哪条当前不变式、
> 权威在哪、攻击什么碰撞、对应哪段 production 代码、负控制是什么、
> PASS 意味着什么（见下表与各套件 README/RESULTS）。
>
> 2026-09（PR #139 之后的 formal-spec reset）删除了 pre-reset playback
> TLA 模型（PlaybackTemporal/PlaybackOwnership 及其 mutations）、pre-139
> F2 design audit 报告、native-boundary audit 轮次报告、以及旧
> executable temporal-core 测试 harness。它们的 bug witness 已由当前
> 证据承载（见 deletion ledger，PR 描述）；Git 历史 / `playback-reference-v1`
> ref / PR 记录是唯一存档。**本目录不是博物馆。**

## 验证哲学（不变）

> **形式化验证优先用于发现高风险状态组合产生的反直觉错误，不用于为整个架构建立第二份完整实现。**

> **TLA+ 用来找撞车，不用来证明整个架构。**

入口问题（ADR-PBK-001 §13）：

> **这里有哪些独立合法的状态或事件，可能因为交错而撞出一个非法状态？**

回答不出这个问题的问题用类型系统 / ownership / 普通测试 / Loom / Miri
解决。模型结论只在其显式 abstraction 与 assumptions 下成立；模型不得
静默升级为 architecture authority（AGENTS.md "Verification authority
boundary"）。

---

## 当前验证覆盖矩阵

### A. Formal semantic verification（TLA+ / TLC）

| Invariant family | Authority | 攻击的碰撞 | Artifact | 负控制 | Bounds / 假设 | 结果 |
| --- | --- | --- | --- | --- | --- | --- |
| K0 控制面：relied_on guard、inverse 恰好一次 + 违约 tombstone、移除纪律、§E.4 点单一来源与 staged replacement、§G.6 违约 latch 与 guard 保持、FAILED settlement（raise+完全 discharge）、settle 终止 | `docs/architecture/composition-kernel-0-design.md` §E.3/E.4/F/G/G.6/L.1/L.5 | withdraw × dispose × replacement churn × activation raise × 违约注入在 settle 各 step 边界的交错 | `composition-kernel-0/`（PRODUCTION MAPPING 表映射到 `crates/qianqian-composition/src/kernel.rs`） | M1–M5 mutation（M5 = 曾检出的 mount overlap 缺陷，已由 #126 修复）+ 6 可达性探针 | 3 fibers / 单 capability 单 consumer / bounded churn window；见 RESULTS.md B1–B6 | BOUNDED-CLEAN（safety+liveness），2026-09-13 实测 |
| Realtime view publication / reader quiescence / resource reclamation：coherent acquisition（P1）、retired 闭门（P2）、跨代 quiescence 先于回收资格（P3）、Retired≠Reclaimable≠Released（P4）、条件回收进展（P5） | `docs/adr/ADR-PBK-001.md` §6（ACCEPTED；normative 协议本体在 ADR，语义定义不在本目录） | publication N→N+1 与旧 reader overlap；多代退休记账下提前回收；split publication；stale entry liveness | `realtime-publication/` | M1 ReleaseBeforeQuiesce / M2 SplitPublication / M3 StaleEntry（safety+liveness）/ M4 ForgetsOlderRetirement + 4 可达性探针 | ≤2 readers、bounded publication 链、WF(ReaderExit)+WF(MarkReclaimable)；不建模 PCM/线程/内存序 | BOUNDED-CLEAN；M1–M4 全部 COUNTEREXAMPLE-WITNESSED |

### B. Implementation / concurrency verification（Rust）

| Invariant family | Authority | 攻击的碰撞 | Artifact | 负控制 | 结果 |
| --- | --- | --- | --- | --- | --- |
| K0 Rust refinement：stale FiberId、relied provision 可解析、inverse-once + LIFO + tombstone 保留、移除纪律（clean+violation 格）、quiet truth、single-source 含 #126 withheld-mount | K0 design + `composition-kernel-0-implementation-adr.md` | 真实 `kernel.rs` 全部合法 step 序列（62 distinct-behavior 场景矩阵，K2/K6 每 step 后断言） | `composition-kernel-0-rust/`（harness 在 `crates/qianqian-composition/src/kernel_verify.rs`，直测 production） | M-K1/M-K2/M-K3 production mutation patches | BOUNDED-CLEAN（native）+ MIRI-CLEAN；Kani symbolic = TOOLING-INSUFFICIENT（如实记录） |
| 当前 PCM 边并发：FIFO ring 完整性、terminal 单调（EOF×stop、failure×stop 且 Failed 身份不降级）、阻塞 producer/consumer 唤醒 | PBK-002 D8 PCM data plane + `crates/qianqian-playback` 契约 | 真实 `PcmEdge`（仅 `cfg(loom)` 换 Mutex/Condvar）的全部调度 | `playback-concurrency/`（harness 在 `crates/qianqian-playback/tests/loom_edge.rs`） | M-L1 drop data_ready notify | SCHEDULE-CLEAN（≤3 threads / ≤2 samples / ≤3 ops）+ native/stress |
| 每层结果与工具选择的 single-writer | — | — | `playback-concurrency/CAMPAIGN-1.md`（VERIFICATION-CAMPAIGN-1 轮次记录；Miri per-crate 与 integration stress 结果的唯一 canonical owner） | — | PASS_WITH_LIMITATIONS（轮次判据） |

### C. Ordinary regression evidence（不在 specs/，`cargo test --workspace` 承载）

- K0 语义 oracle（`crates/qianqian-composition/tests/*_oracles.rs`，含
  staged-mount #126 反例回归）；
- episode 终局语义（`crates/qianqian-playback/tests/`：
  session_activation / edge_lifecycle / stop_seam —— SessionCompletion
  单写者 + precedence、stop×EOF×failure、join/leak oracle）；
- P1–P5 真实机制证据（`crates/qianqian-audio-api/tests/realtime_view_publication/`，
  含 twin-kill 负控制）；
- PCM 契约 / direct-flow / RT firewall（pcm_edge_contract /
  direct_pcm_flow / k0_firewall，含 trybuild compile-fail 类型系统证据）。

层间纪律：**TLA+ 持语义级交错证据，Rust matrices/Miri/loom 持
实现/refinement 证据，普通测试持回归证据；任何一层不得声称证明了
另一层检查的性质。**

---

## 运行入口

```bash
# 全部当前形式化验证（缺省 = current = K0 + realtime publication）
specs/check.sh

# 显式当前集
specs/check.sh current

# 专项
specs/check.sh k0         # K0 控制面 TLA+ 套件
specs/check.sh realtime   # realtime publication TLA+ 套件

# Rust 侧当前验证（cargo 矩阵 + Miri + production mutation 负控制 +
# loom；需要 nightly/miri 与 loom feature）
specs/check.sh rust
```

工具链固定 `tla2tools v1.7.4 (Xenophanes)`，各 runner 内嵌 sha256 校验、
fail closed；jar 缓存在 `specs/tools/`（不入库），缺失时自动下载
（需要代理时先 `export http_proxy/https_proxy`）。

Runner 纪律（所有套件继承）：baseline 必须 TLC 探索完成且全部
invariant/temporal property PASS；每个 mutation 必须被抓住
（counterexample 才算通过）；任何 TLC `Warning:` 行即 FAIL；
反例 run 必须由 TLC 自行收尾（log 含 `Finished in`）。

## 结果词汇与声明边界

允许的结果类（issue #124 vocabulary）：`BOUNDED-CLEAN` /
`COUNTEREXAMPLE-WITNESSED` / `SCHEDULE-CLEAN` / `MIRI-CLEAN` /
`TEST-PASS` / `STATIC-CHECK-PASS` / `TOOLING-INSUFFICIENT` / `NOT-RUN`。

正确表述：

> 当前选定的不变式已在显式记录的抽象、bounds 与假设下检查，未发现反例。

禁止表述："architecture proven correct" / "race-free" / "bug-free" /
"系统已形式化证明"。一个 clean run 只意味着**在所述模型、界、假设与
fairness 条件内未找到反例**。

## 命名规则

- spec 文件、TLA+ module、operator、invariant、mutation 与长期注释使用
  **稳定领域 vocabulary**（描述语义，不描述里程碑）。
- ADR/Issue/PR/Corrective/Phase 编号只作 README/RESULTS 的 traceability
  信息，不进入模型 vocabulary 与文件名。

## 目录

| 目录 | 为什么现在存在 |
| --- | --- |
| `composition-kernel-0/` | K0 控制面语义交错的唯一 TLA+ 证据（权威：K0 design） |
| `composition-kernel-0-rust/` | K0 语义在真实 Rust kernel 上的 refinement 证据（矩阵 + Miri + mutation） |
| `realtime-publication/` | PBK-001 §6 P1–P5 的唯一 TLA+ 语义证据 |
| `playback-concurrency/` | 当前 PcmEdge 并发证据（loom + native + mutation）与 CAMPAIGN-1 轮次记录 |
| `tools/` | 固定版本 TLC jar 本地缓存（gitignored） |
