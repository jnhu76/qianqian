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
> 证据承载（见下方 deletion witness ledger）；Git 历史 / originating
> PR 记录是唯一存档。**本目录不是博物馆。**

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

覆盖类词汇（每行必须声明自己属于哪一类，禁止 scope inflation）：

```text
CHECKED-IN-MODEL             该不变式在 TLA+/TLC 的显式 bounds/假设内被检查
REFINEMENT-CHECKED-IN-RUST   该不变式在真实 Rust 实现/机制上被检查
REGRESSION-ONLY              仅普通回归测试承载（cargo test --workspace）
NOT-MODELED                  无当前 verifier；如实声明
```

### A. Formal semantic verification（TLA+/TLC）

CI 列的 **formal-semantic-gate** = `.github/workflows/formal-semantic-gate.yml`
（path-scoped；run `specs/check.sh current`；fail-closed）。

| 不变式 | 权威来源 | Production referent | 攻击的碰撞 | 覆盖类 | Artifact | 负控制 | Bounds / 假设 | 当前结果 | 未覆盖面 | CI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| K0 relied_on guard（P1） | `composition-kernel-0-design.md` §G | `crates/qianqian-composition/src/kernel.rs` `relied_on`/`eligible_unload` | withdraw × dispose × churn × raise 交错下 provider 被关闭/移除 | CHECKED-IN-MODEL | `composition-kernel-0/` | M1 DropReliedGuard | B1–B6（3 fibers/单 K 单 consumer/bounded churn） | BOUNDED-CLEAN（safety+liveness，2026-09-13） | 依赖深度 >1；多 capability/consumer | formal-semantic-gate |
| K0 inverse 恰好一次（P2a） | §H.1 | `run_unwind`/`dispose_effect` | 违约注入 × settle step 边界交错中 inverse 二次执行 | CHECKED-IN-MODEL | 同上 | M4 DoubleInverse | 同上 | 同上 | 实现细节（slab/generation 回绕）→ Rust 层 | formal-semantic-gate |
| K0 违约 tombstone 保留（P2b，**effect-bearing witness**） | §G.6/§K.4 | `run_unwind` violated 记录留 accumulator | 违约 latch 后假装清洁/移除 | CHECKED-IN-MODEL | 同上 | M4（tombstone 二次执行） | 同上；模型违约路径全部 effects ≥ 1 | 同上 | **teardown-closure latch 位点 NOT-MODELED**（见 §G.6 行） | formal-semantic-gate |
| K0 移除纪律（P3） | design Thm 64/Cor 69 | `removal_candidate` | accumulator 未清/欠 inverse 即移除 | CHECKED-IN-MODEL | 同上 | M2 RemoveBeforeDischarge | 同上 | 同上 | 同上 | formal-semantic-gate |
| K0 §E.4 点单一来源 + staged replacement（P4a） | §E.4/B12 | `mount_candidate`（含 #126 overlap withhold）+ step 优先级 | 违约边 mount 重叠；replacement 未 staged | CHECKED-IN-MODEL | 同上 | M3 EarlyReplacement；M5 MountOverViolation（= pre-#126 缺陷的 TLA 侧负控制） | 同上 | 同上 | plan-time 拒绝（Rust 层） | formal-semantic-gate |
| K0 §G.6 violation-latch semantic family（dependent-consumer 拓扑） | §G.6 | `unload_fiber`/`run_unwind`/teardown closure latch | 违约 latch × guard 保持 × 移除/替换阻塞 | **TLA witness：effect-bearing inverse failure = CHECKED-IN-MODEL；teardown-closure empty-accumulator failure = NOT-MODELED（TLA）+ REFINEMENT-CHECKED-IN-RUST**（differential 判定 FORMALIZATION_NOT_EARNED，见 `composition-kernel-0/RESULTS.md` §4） | TLA：`composition-kernel-0/`；Rust oracle（dependent-consumer 拓扑）：`crates/qianqian-composition/tests/lifecycle_oracles.rs :: dependent_consumer_violation_loci_preserve_k0_guard_semantics` | M4 + probe GuardLatched/ViolatedLatch（TLA） | 同上 | TLA 部分 BOUNDED-CLEAN；Rust oracle TEST-PASS | TLA 未直接探索 teardown-closure 位点（如实声明）；oracle 只声明 dependent-consumer 拓扑，非普遍 locus 等价证明 | formal-semantic-gate + verification-rust-gate |
| K0 FAILED settlement / settle 终止（L1/L2） | §F.5/§L.1/L.2 | `activate_fiber` raise 路径/`settle` | raise × 违约 × churn 交错下假收敛/死循环 | CHECKED-IN-MODEL | 同上 | probes（FailedQuiet、DisposeConvergence 等 6 项正向控制） | 同上 + WF(KernelActions) | 同上 | 强 fairness 未假设 | formal-semantic-gate |
| Realtime publication P1–P5（coherent acquisition / retired 闭门 / quiescence 先于回收 / Retired≠Reclaimable≠Released / 条件回收进展） | `docs/adr/ADR-PBK-001.md` §6（normative 协议本体在 ADR） | **production realization 尚不存在**（多代 publication 机制未进任何 production crate；ADR §6/§12：production representation OPEN）。当前 production surface = episode-scoped 单代 teardown（`crates/qianqian-playback` edge/session/completion 的 join-quiescence），协议上不得违反 P1–P5；可执行机制证据是 **test-local candidate harness**（`realtime_view_publication/`，非 lib API） | publication N→N+1 与旧 reader overlap；多代退休记账提前回收；split publication；stale entry liveness | CHECKED-IN-MODEL（TLA）+ candidate-mechanism harness（Rust，test-local） | `realtime-publication/` | M1 ReleaseBeforeQuiesce / M2 SplitPublication / M3 StaleEntry（safety+liveness）/ M4 ForgetsOlderRetirement + 4 可达性探针 | ≤2 readers、bounded publication 链、WF(ReaderExit)+WF(MarkReclaimable)；不建模 PCM/线程/内存序 | BOUNDED-CLEAN；M1–M4 全部 COUNTEREXAMPLE-WITNESSED | production 机制 realization（OPEN，ADR Phase D）；PCM/线程/内存序 | formal-semantic-gate |
| episode terminal settlement（D11 current-spec conformance：单次提交不可改写、writer identity=authority（transition 级性质）、Observe/Wait 纯性、late-command stability（决策边界 ghost）、teardown settlement 边界（全形状触发域）、activation failure firewall、Completed/Stopped 命题必要条件、条件性 settlement 进度） | `docs/adr/ADR-PBK-002.md` §17 D11（normative authority）+ ADR-PBK-001 §2.2–§2.3 | `crates/qianqian-playback/src/completion.rs` resolver / `session.rs`（current realization；F2 已把 settlement 迁回 evidence-publication 触发的 authority-owned 路径，pre-F2 consumer-triggered differential 已闭合——模型表达 accepted contract） | observe/wait/证据发布者/晚到 stop 偷偷写 terminal Fact；teardown 跑在决定性证据未提交之前；无边界 stop intent 判 Stopped；触发域收窄逃逸 C5 | CHECKED-IN-MODEL | `episode-terminal-settlement/` | M1 ObserveCommits / M2 WaitCommits / M3 TerminalRewritable / M4 LateStopReadsCurrentIntent / M5 TeardownBeforeSettlement / M6 ActivationFailureBecomesFailed / M7 FalseCompleted / M8 FalseStopped / M9 EvidenceProducerSpoofsAuthority / M10 NarrowDecisiveDomain + 5 overclaim 反向控制（带 WF 最强让步）+ fairness 承重控制 + 7 可达性 witness | 652 distinct states（穷举）；WF(AuthoritySettle) 仅承载条件性进度 | BOUNDED-CLEAN（safety 无 fairness 双跑 PASS）；10 mutation 全部 COUNTEREXAMPLE-WITNESSED（writer 类为纯 temporal 反例：状态不变式全绿）；7 witness 可达 | 判决值表 precedence（current realization，非 D11 冻结；触发域全覆盖是 normative）；Rust refinement 层（verification-rust-gate 承载） | formal-semantic-gate |

| F5 seek cutover 协议（commit 之后旧 PCM 不可能成为 output；位置发布不混 pre/post basis；commit 前置：landing ∧ edge 干净 ∧ 尾排空 ∧ parked；stop 恒赢） | `docs/adr/ADR-PBK-002.md` §20 D14.5 + D14.8 seek 规则（normative authority；F5-GATE amendment 为拟冻结文本） | **production realization 尚不存在**（F5 停在 gate；实现门属 F5-IMPLEMENTATION）。模型对象 = F5-GATE 选定的 same-resource discontinuity protocol 抽象 | commit 后旧 PCM 复活（三库：edge/held/设备尾）；commit 后 stale position writer | CHECKED-IN-MODEL | `f5-seek-discontinuity/` | M1 CommitBeforeTailPurge / M2 SeekMidWrite / M3 ParkWhileHeld / M4 StalePositionWriter / M5 CommitBeforeLanding + 4 可达性 witness | MAX_TAIL=2；布尔化三库；safety-only 无 fairness；无 WASAPI 流状态机/请求语法/多 seek | BOUNDED-CLEAN（426 distinct states 穷举）；5 mutation 全部 COUNTEREXAMPLE-WITNESSED；4 witness 可达 | 实现门需 loom（真 edge 并发）+ 物理 gate（audible cutover）；本模型不预授权任何 production 表示 | formal-semantic-gate |

### B. Implementation / concurrency verification（Rust）

CI 列的 **verification-rust-gate** = `.github/workflows/verification-rust-gate.yml`
（path-scoped；run `specs/check.sh rust`；fail-closed）。

| 不变式 | 权威 / 契约来源 | Production referent | 攻击的碰撞 | 覆盖类 | Artifact | 负控制 | Bounds / 假设 | 当前结果 | 未覆盖面 | CI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| K0 Rust refinement：stale FiberId、relied provision 可解析、inverse-once + LIFO + tombstone 保留、移除纪律（clean+violation 格）、quiet truth、single-source 含 #126 withheld-mount | K0 design + `composition-kernel-0-implementation-adr.md`（representation） | 真实 `kernel.rs` 全部合法 step 序列（62 distinct-behavior 场景矩阵，K2/K6 每 step 后断言） | 真实实现上的全部矩阵场景 | REFINEMENT-CHECKED-IN-RUST | `composition-kernel-0-rust/`（harness：`crates/qianqian-composition/src/kernel_verify.rs`） | M-K1/M-K2/M-K3 production mutation patches | 场景矩阵 bounds（RESULTS.md）+ ≤14 step drain | BOUNDED-CLEAN（native）+ MIRI-CLEAN；Kani symbolic = TOOLING-INSUFFICIENT（如实记录） | Kani 符号层未挣得 | verification-rust-gate |
| 当前 PCM 边并发：FIFO ring 完整性、**edge 机制 terminal 单调**（EOF×stop、failure×stop 且 Failed 身份不降级）、阻塞 producer/consumer 唤醒 | **架构边界权威：PBK-002 D8**（PCM data plane firewall）；edge 行为契约属 production（`crates/qianqian-playback` 代码与契约为 production referent，非 architecture authority） | 真实 `PcmEdge`（仅 `cfg(loom)` 换 Mutex/Condvar） | 真实 edge 的全部调度（stop×write/read/EOF/fail、blocked wake） | REFINEMENT-CHECKED-IN-RUST | `playback-concurrency/`（harness：`crates/qianqian-playback/tests/loom_edge.rs`） | M-L1 drop data_ready notify | ≤3 threads / ≤2 samples / ≤3 ops；loom Arc refcount 不在 modeled slice | SCHEDULE-CLEAN + native/stress | settlement core（completion.rs）未 loom（native + crate-internal settlement contract tests 承载；F2 起 wait_terminal 为纯 condvar wait） | verification-rust-gate |
| **episode 语义终局**（Completed/Stopped/Failed 单写、precedence、不可改写） | **语义权威：PBK-002 D11**（Playback Session 为 designated authority）——edge 机制 terminal state ≠ episode semantic terminal outcome，两者不得互相冒认 | `crates/qianqian-playback/src/completion.rs` resolver（production realization，非 authority） | stop×EOF×failure 交错下的终局归属 | REFINEMENT-CHECKED-IN-RUST（机制层）+ REGRESSION-ONLY（resolver 决策表） | `crates/qianqian-playback/src/settlement_contract_tests.rs`（crate-internal resolver/settlement oracle，F2 起）+ `tests/`（session_activation、stop_seam、read_seam） | —（resolver oracle 为方向性断言；无 mutation 负控制，如实声明） | 原生时序 + CPU 压力（CAMPAIGN-1 B4） | TEST-PASS + stress PASS | loom 未覆盖（settlement core） | verification-rust-gate（native 部分）|
| 每层结果与工具选择的 single-writer | — | — | — | — | `playback-concurrency/CAMPAIGN-1.md`（VERIFICATION-CAMPAIGN-1 轮次记录；Miri per-crate 与 integration stress 结果的唯一 canonical owner） | — | — | PASS_WITH_LIMITATIONS（轮次判据） | — | —（历史轮次记录） |

### C. Ordinary regression evidence（不在 specs/，`cargo test --workspace` 承载）

- K0 语义 oracle（`crates/qianqian-composition/tests/*_oracles.rs`，含
  staged-mount #126 反例回归、violation-latch cross-locus refinement oracle
  （dependent-consumer 拓扑，scope 见 §B §G.6 行））；
- episode 终局语义（`crates/qianqian-playback/tests/`：
  session_activation / edge_lifecycle / stop_seam / read_seam ——
  PlaybackSessionHandle 公共 seam + crate-internal settlement core
  单写者 + precedence、stop×EOF×failure、join/leak oracle）；
- P1–P5 真实机制证据（`crates/qianqian-audio-api/tests/realtime_view_publication/`，
  23 项测试含 twin-kill mutation 负控制；**test-local candidate mechanism
  harness** —— 非任何 lib API，production representation 仍 OPEN，
  ADR-PBK-001 §6/§12）；
- PCM 契约 / direct-flow / RT firewall（pcm_edge_contract /
  direct_pcm_flow / k0_firewall，含 trybuild compile-fail 类型系统证据）。

层间纪律：**TLA+ 持语义级交错证据，Rust matrices/Miri/loom 持
实现/refinement 证据，普通测试持回归证据；任何一层不得声称证明了
另一层检查的性质。**

---

### D. Campaign artifacts（**非 CI gate**；挑战 authority 中的语义边界）

这些套件产出**尚未裁决**的语义发现，按 AGENTS.md「Verification authority
boundary」它们必须先回到 ADR / design-authority review，因此**不接入**
`specs/check.sh` 的 current 集，也不构成 acceptance gate。它们仍是可复跑、
fail-closed 的当前证据，只是结论用途不同。

| 套件 | 问题 | 覆盖类 | 结果 | 状态 |
| --- | --- | --- | --- | --- |
| `f2-terminal-commit-boundary/` | 一个 episode 的 terminal Fact semantic commit 归谁所有：外部 `wait()`/`try_resolve_now()` 触发（当时实现）还是 Playback Session authority 自己推进？外部 read/wait 是在消费 truth 还是在创造 truth？ | CHECKED-IN-MODEL（TLA/TLC，A/B/B′ 三变体 + 9 mutation（含 3 条 precedence/join 纪律负控制）+ 9 witness + 3 over-claim 反向控制 + 2 fairness 承重控制 + 1 已证不可达，共 27 条 TLC run） | 3 正常模型 PASS；全部负控制按预期（含 fresh adversarial review 的两轮修正）；核心 witness：A 中"teardown 完成而 Fact 缺席"可达、无 consumer 时提交不可达（穷举证明）；B 中无 consumer 也能提交 | **HISTORICAL / EXPLORATORY EVIDENCE（已关闭）**。verdict 为 CURRENT_CONTRACT_UNDERSPECIFIED；该 underspecification 已由 ADR-PBK-002 §17 D11 settlement corrective 裁决（decision+commit ownership 归 Playback Session semantic authority；late-command stability 冻结）。**current-spec conformance 证据是 `episode-terminal-settlement/`（§A，CI gate）**；本套件不再演进，A/B/B′ 变体不构成 current requirement（报告 `f2-terminal-commit-boundary/report.md`） |

---

## Deletion witness ledger（post-139 reset：每个退役 witness 族的现居所）

删除不消灭 witness；每个 retired major witness family 必须有精确现居所：

| 退役 witness family | 当前居所 |
| --- | --- |
| stop × EOF | loom `loom_l3a`（first-terminal-wins）；native：`edge_lifecycle::eof_drains_before_terminating_and_stays_terminal`、`stop_seam::late_stop_after_completed_changes_nothing` |
| failure × stop | loom `loom_l3b`（Failed 身份不降级）；native：`src/settlement_contract_tests.rs`（crate-internal，F2 起）completion_* resolver oracle、`stop_seam::stop_before_a_failing_activation_leaves_the_diagnostic_in_charge` |
| blocked producer | loom `loom_l4a`；native：`edge_lifecycle::stop_unblocks_a_producer_blocked_on_a_full_edge`、`src/settlement_contract_tests.rs::stop_wakes_a_producer_blocked_on_a_full_edge`（F2 起 crate-internal，witness 依赖 buffered_frames 诊断）；stress（CAMPAIGN-1 B4） |
| blocked consumer | loom `loom_l4b`；native：`edge_lifecycle::stop_unblocks_a_reader_blocked_on_an_empty_edge`、`src/settlement_contract_tests.rs::stop_wakes_a_consumer_blocked_on_an_empty_edge`（同上）；stress（B4） |
| provider release order（release 先于 realtime readers quiesce） | realtime-publication TLA M1 ReleaseBeforeQuiesce（COUNTEREXAMPLE-WITNESSED）+ P2/P3 语义；机制 harness（test-local candidate）：`realtime_view_publication/`（twin-kill 负控制） |
| stale publication | realtime-publication M3 StaleEntry（safety+liveness）+ StaleEntryLiveness；机制 harness（test-local candidate）：`realtime_view_publication/` |
| replacement overlap（双 provider 同 capability） | K0 TLA M3/M5（pre-#126 缺陷负控制）+ Rust M-K3 + `staged_mount_oracles.rs` 回归 |

## 运行入口

```bash
# 全部当前形式化验证（缺省 = current = K0 + realtime publication +
# episode terminal settlement）
specs/check.sh

# 显式当前集
specs/check.sh current

# 专项
specs/check.sh k0         # K0 控制面 TLA+ 套件
specs/check.sh realtime   # realtime publication TLA+ 套件
specs/check.sh terminal   # episode terminal settlement 套件（D11 conformance）
specs/f2-terminal-commit-boundary/check.sh   # campaign artifact（见 §D，非 CI gate，historical）

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

### CI 门（durable，path-scoped，fail-closed）

```text
formal-semantic-gate     specs/check.sh current
    触发：specs/{composition-kernel-0,realtime-publication,
          episode-terminal-settlement}/**、specs/check.sh、
          K0 design/implementation ADR、ADR-PBK-001、ADR-PBK-002、
          crates/qianqian-composition/**
verification-rust-gate   specs/check.sh rust
    触发：crates/qianqian-{composition,playback}/**、specs/{composition-kernel-0-rust,
          playback-concurrency}/**、specs/check.sh
    （含 §G.6 cross-locus refinement oracle —— 见 composition rust runner）
```

Trigger paths 按仓库现实划定：多代 publication 机制**尚无 production
realization**（唯一可执行机制证据是 audio-api 的 test-local candidate
harness），audio-api src 仅 ports；`crates/qianqian-playback` 承载的是
当前 episode-scoped 单代 teardown surface（PcmEdge/session/completion），
其变更由 verification-rust-gate 承载。TLA 模型检查语义协议，不随实现
diff 失效，故 playback 实现变更不触发 TLA 重跑。Miri 在 CI 上仅跑
composition 7 矩阵（约分钟级），不是全仓 Miri campaign；更大范围
Miri/真机/FFI 证据仍为 local/manual（见 CAMPAIGN-1）。

## 结果词汇与声明边界

允许的结果类（issue #124 vocabulary）：`BOUNDED-CLEAN` /
`COUNTEREXAMPLE-WITNESSED` / `SCHEDULE-CLEAN` / `MIRI-CLEAN` /
`TEST-PASS` / `STATIC-CHECK-PASS` / `TOOLING-INSUFFICIENT` / `NOT-RUN`。

正确表述：

> 当前选定的不变式已在显式记录的抽象、bounds 与假设下检查，未发现反例。

禁止表述："architecture proven correct" / "race-free" / "bug-free" /
"系统已形式化证明"。一个 clean run 只意味着**在所述模型、界、假设与
fairness 条件内未找到反例**。

覆盖声明的精度规则：**不得对只有部分实现位点被直接建模的不变式写
「已验证」**。写法示例（§G.6）：

```text
§G.6 violation-latch semantic family:
    TLA witness: effect-bearing inverse failure（CHECKED-IN-MODEL）
    Rust refinement witness: teardown-closure empty-accumulator failure
        （REFINEMENT-CHECKED-IN-RUST；TLA NOT-MODELED）
```

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
| `f5-seek-discontinuity/` | F5 seek cutover 协议的 gate-local formal evidence（权威：PBK-002 §20 D14.5 + D14.8 seek 规则；F5-GATE） |
| `episode-terminal-settlement/` | D11 episode terminal settlement contract 的 current-spec conformance 证据（权威：PBK-002 §17 D11） |
| `playback-concurrency/` | 当前 PcmEdge 并发证据（loom + native + mutation）与 CAMPAIGN-1 轮次记录 |
| `tools/` | 固定版本 TLC jar 本地缓存（gitignored） |
