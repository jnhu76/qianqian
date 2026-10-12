# R3 Wave A 报告 — Headless Oracle 修复（H1 诊断合法性 / H3 Seal-Ack 因果性）

> **证据等级：EXECUTED_TEST_EVIDENCE（目标测试 + 全套 + 突变负控均已执行）。**
> 本文件是 R3 Wave A 的实验记录（experiment record），不是 Test Quality Authority：
> 它不定义新的测试语义、架构名词、生命周期或门禁。`SCREENED → ADEQUATE` 的
> 台账采纳权属 review；本报告只登记已执行的证据与限制，不做静默晋升。
> 权威输入：#253（工单）、#251（含独立复审
> issuecomment-6116778156）、#252（只读清单）、#244（Umbrella）、
> `docs/development/concurrency-test-design.md`（测试创作法）。

```text
DATE           = 2026-10-12
BASE           = 3664337f0b54b9badaeff8fc25aab8da59a90472 (origin/main, 与审计冻结 BASE 一致,
                 执行时重新解析确认无新提交)
HEAD(A3 前)    = cc6d9fe  test(headless): strengthen seal acknowledgement oracle
                 79b8d90  test(headless): align preseal diagnostic oracle with legal outcomes
BRANCH/WORKTREE= test/r3-wave-a-headless-oracles @ ../qianqian-r3-wave-a (独立新建,
                 未触碰只读审计 worktree ../qianqian-r3-a)
ENV            = Linux/WSL2 6.18.33.2-microsoft-standard-WSL2 x64; rustc/cargo 1.97.1
MUTATION_ENV   = 独立 scratch worktree ../qianqian-wavea-mut(同一对象库), 逐突变应用→运行→还原
PRODUCT_CHANGE = NO (两 commit 均只改 apps/headless/src/machine_input/tests.rs)
SIZE/SCOPE     = H1/H3 维持 Medium / Component(源码复核未推翻 #252 CSV 行)
```

## 0. 结论摘要

```text
H1_VERDICT        = CONTRACT_CORRECT(修正后 oracle 接受全部合法 terminal/diagnostic 组合,
                    并仍拒绝三类非法变体)
H1_LEGAL_FAILED_NONE_ACCEPTED = YES(MUT-A 合法变体: 修正后 GREEN; 修正前 RED, 过度 pin 实证)
H3_VERDICT        = CAUSAL_KILL_PROVEN(值判别设计; 注册 no-wait mutant 200/200 确定性 RED,
                    修正前同 mutant 199/200 ESCAPED —— 审计发现由实测确认)
H3_NO_WAIT_MUTANT_RESULT      = 新测试 200/200 RED(0 逃逸); 旧测试 1/200 检出
FINAL_HEAD_CI                  = 见 §6(GitHub Actions 于 Draft PR 上执行; 本地 lefthook 全绿)
```

## 1. H1 — `preseal_failure_preserves_natural_completed_and_runtime_failed`

### 1.1 性质与权威

测试的合法输入空间是四个执行组合（2 终局 × 2 失败次序），oracle 断言的权威
边界是 `PlaybackSessionObservation::failure_diagnostic` 的真类文档
(`crates/qianqian-playback/src/handle.rs:130-135`)：

```text
Completed → 永不携带 diagnostic(必须 None)
Stopped   → 永不携带 diagnostic(必须 None; 本测试不产生 Stopped 分支)
Failed    → 可有可无(presence 是 presentation, 不是语义)
```

同一边界在 `completion.rs` `SessionOutcome::split` 的文档中重申
（"its presence, absence or spelling is not part of the semantic contract"）。

### 1.2 修正前/后 oracle 对比

```text
修正前: assert_eq!(diagnostic.is_some(), terminal == Failed)
        → 隐含"Failed 必有 diagnostic" = 把 presence 钉成语义, 拒绝合法执行
修正后: 仅 Completed 分支 assert!(diagnostic.is_none());
        Failed 分支对 diagnostic 不做断言(权威允许两者)
```

保留的验证（未删任何既有断言）：自然终局事实两次 `wait_terminal == terminal`、
HostFailure 域经退出码 1（`machine_exit_code(host_failed=true)`）、
failure-first 分支的 record/response 剖切与 "host lock across terminal wait"、
非 failure-first 分支的 "no response needed after terminal"、`disposed` 清理。

**报告输出验证归属性核查（工单 A1-4）**：兄弟测试
`final_owner_reports_preserve_terminal_and_distinct_failure_domains`
已完整拥有 diagnostic→`outcome_report` 渲染 oracle（reports[1..] 与
`outcome_report(terminal, diagnostic)` 全等比较，覆盖 Failed+Some 与
Failed+None 两种渲染）。本测试不新增跨模块报告依赖，不重复该验证。

### 1.3 合法/非法结果表

| 执行 | terminal | diagnostic | 修正前 oracle | 修正后 oracle |
|---|---|---|---|---|
| EofAfter(16), 任一次序 | Completed | None(现实) | 接受 | 接受 |
| FailAfter(0), 任一次序 | Failed | Some(现实) | 接受 | 接受 |
| **FailAfter(0) + 无 diagnostic 发布** | Failed | None | **拒绝(非法拒绝)** | **接受(合法)** |
| Completed + diagnostic 发布 | Completed | Some | 拒绝 | 拒绝(仍非法) |

### 1.4 突变矩阵（scratch worktree, 逐个应用→运行→还原）

预测均先于执行登记; 签名为实际 panic 输出。

| ID | 突变位点 | 合法性 | 预测(修正后 H1) | 实际 | 语义签名 |
|---|---|---|---|---|---|
| MUT-A | `split()` Failed 臂 → `(Failed, None)` | **合法**(契约明示) | GREEN | **GREEN** | — |
| MUT-A' | 同上, 对**修正前** H1(BASE) | 合法 | RED(过度 pin 实证) | **RED** | tests.rs:245 `left: false, right: true` |
| MUT-B | `split()` Completed 臂 → `Some("tampered")` | 非法 | RED | **RED** | tests.rs:253 `failure_diagnostic.is_none()` |
| MUT-C | `resolve()` 自然 EOF → `Stopped` | 非法(篡改自然终局) | RED | **RED** | tests.rs:233 `left: Stopped, right: Completed` |
| MUT-D | `Operation::fail` 移除 terminal 守卫 | 非法(终局后发 Stop response) | RED | **RED** | tests.rs:235 `no response needed after terminal` |
| MUT-E | `seal()` 返回 `None`(HostFailure 域损坏) | 非法 | RED | **RED** | tests.rs:242 `ExitCode(0) != ExitCode(1)` |

MUT-A 连带观察（诚实记录）：修正后 H1 GREEN 的同时，qianqian-playback lib
有 7 个测试 RED（eq_tests×2、gain_tests、live_tests、settlement_contract
t6/t15、stateful_probe）——这些测试在**各自更窄场景**内 pin 了失败诊断的
存在/内容，其 oracle 属于各自场景的呈现预期，与 H1 的通用真类边界分属
不同验证层。本波不改动它们；此观察登记为台账注释，不作为 H1 证据使用。

无一个突变的 RED 依赖编译错误（全部语义签名; MUT-A/MUT-A' 的
`stage: _` 写法保持可编译）。Scratch worktree 还原后 `git status` 清洁。

## 2. H3 — `seal_waits_for_admitted_report_ack_and_rejects_fresh_work`

### 2.1 审计缺陷与实测确认

修正前 oracle 的"等待证据"是单样本负向采样：

```rust
wait_closed(&input);
assert!(matches!(result.try_recv(), Err(TryRecvError::Empty)),
        "seal overtook output acknowledgement");
```

登记 no-wait mutant（删除 `seal()` 的 `while operation_active { wait }`）
后实测（正确 unittest 二进制、逐次独立进程、N=200）：

```text
旧 H3 vs MUT-H3:  ESCAPED(GREEN) = 199/200, DETECTED = 1/200 (~0.5%)
检出签名(唯一一次): tests.rs:323 "seal overtook output acknowledgement"
```

即：该断言对本机调度几乎是空转的——`wait_closed` 与 mutant 的 send 竞争
同一把 bookkeeping mutex 的重取权，sealer 解锁后紧接的 `send` 几乎总是
先于 main 的重锁+采样落地。审计"单样本不能建立等待前提"的判断由实测确认。
（诊断性单核 pin `taskset -c 0` ×50 亦 50/50 ESCAPED；上述 199/200 为主测量。）

### 2.2 最小执行 DAG 与四态区分

```text
T: 测试主线程   W: 持有已接纳 Operation 的 worker(阻塞在输出回调内)   S: sealer

W: admit → operation_active = true            (Operation Active)
W: 进入回调, entered.send → T.started.recv
S: lock; admission_closed = true              (Admission Closed)
S: [正确] 观测 operation_active=true → Condvar 等待(释放锁)
S: [mutant] 同一临界区内读 first_failure → 解锁 → return None → sealed.send
T: wait_closed 观测 admission_closed=true     (与 S 的置位同锁互斥, 是序点)
T: line("stop") 被拒(admission closed, 新工作不得超前排空)
T: release.send → W 回调返回/panic
W: [record 分支] first_failure.get_or_insert(Panic)   (Record, 持锁)
W: Operation::drop → operation_active=false; notify   (ACK)
S: [正确] 醒来, 循环退出, 读 first_failure → return → sealed.send   (Seal Returned)
T: result.recv(阻塞) 断言返回值
```

四个机制态（Admission Closed / Operation Active / ACK / Seal Returned）
在现有接口下的可观测性：前三态经 `shared.state`（子模块测试可见）与
`is_open()` 可同步观测；"Seal Returned" 只有 channel send 一个异步见证，
**没有任何 happens-before 边能强迫"mutant 已 send"先于任何主线程采样**。
因此无论把 `try_recv` 放主线程还是 worker 回调内（复审反例：S 在置位后、
send 前被抢占，两种放置都得到相同的 Empty→空洞 GREEN），单样本都不构成
因果见证。此结论封死"移动一次断言 = 确定性修复"的路线。

### 2.3 值判别设计（正确实现 vs no-wait mutant 的关键相对排序）

修复：worker 的输出回调在 `release` 后、ACK 前注入一次失败记录（经既有
panic 缝：`line()` 的 `catch_unwind` → `Operation::fail(Panic)` → 记录 →
Drop ACK），测试改为**阻塞 recv 并断言 seal 的返回值**：

```text
关键排序(全部成立时判别完成):
  正确实现: Record(w2) < ACK(w3) < Seal读(读在 ACK 后) → 返回 Some(Panic)
  no-wait mutant: Seal读 与 admission_closed=true 同一临界区 < wait_closed
                  < release < Record → 读到 None → 返回 None
  ⇒ 断言 recv == Some(Panic): mutant 确定性 RED
```

前提的因果强度：`wait_closed` 取同一把 mutex，其返回必然晚于 mutant 的
整个前置临界区（置位与读取都在其中）；`release.send` 只在其后发出，
Record 只在其后被 worker 执行。S 被抢占只会延迟 send、不改变已固定的
返回值——复审的"抢占反例"不再产生漏检。阻塞 `recv` 无采样竞争。
（诚实边界：本设计杀死"跳过/绕过 ACK 等待"与"提前读 first_failure"两类
mutant；"等一次 Condvar 但不带谓词重检"的 spurious-wakeup 类缺陷需要
强制虚假唤醒，超出本波登记范围，见 §6 债务。）

保留断言：fresh-work 拒绝（seal 在飞时 `line("stop")` 返回 false，两分支
各执行一次）、"host lock across blocking output"、worker/sealer join、
5s watchdog。成功报告分支（`record_failure=false`，seal 返回 None）保留
原场景覆盖；失败记录分支提供确定性判别。旧 try_recv 断言删除——其声称
的"等待"语义由更强的值判别取代（非删证，是换证）。

### 2.4 新测试 vs MUT-H3（N=200, 同法同机）

```text
新 H3 vs MUT-H3:  ESCAPED = 0/200, DETECTED = 200/200 (确定性)
签名: tests.rs:352 "seal returned before the admitted acknowledgement"
      left: None, right: Some(Panic)   (record_failure=true 分支)
```

与 §2.1 的 199/200 ESCAPED 构成同机同 mutant 的前后对照。
200 次重复只为测量"概率性 vs 确定性"这一登记声明，不构成正确性证明。

### 2.5 Loom/模型评估

现有测试接口（子模块可见的 `shared.state` + 既有 panic 记录缝 + 阻塞
recv）已足以建立确定性判别，**不引入 Loom/Shuttle/新依赖**（符合工单
"先证明现有接口不足"的次序）。

## 3. 验证记录（目标→工具→边界→结果类）

| 目标 | 命令(完整测试名经 `--list` 核实) | 结果 |
|---|---|---|
| 发现核对 | `cargo test -p qianqian-headless --lib -- --list` | 356 tests; machine_input 18 |
| BASE 基线 | `cargo test ... machine_input::` @3664337 | 18/18 GREEN |
| HEAD 基线 | 同上 @cc6d9fe | 18/18 GREEN (~0.01s) |
| H1 精确目标 | `... machine_input::tests::preseal_failure_preserves_natural_completed_and_runtime_failed -- --exact` | GREEN(4 组合×2 分支) |
| H3 精确目标 | `... machine_input::tests::seal_waits_for_admitted_report_ack_and_rejects_fresh_work -- --exact` ×10 | 10/10 GREEN |
| 全套(回归) | `cargo test -p qianqian-headless --lib` @cc6d9fe | 356/356 GREEN |
| 全仓(回归) | `cargo test --workspace` @e5f7cb1(final HEAD) | 857/857 GREEN(1:45.60) |
| 本地门禁 | `lefthook run pre-push --all-files` @e5f7cb1 | 7/7 GREEN(commit-convention/vocabulary/plugin-boundary/fmt/clippy/rust-test/docs-verify+VitePress build, 104.04s; website 依赖按 AGENTS.md 一次性 `npm ci`) |
| 突变负控 | scratch worktree, §1.4/§2.4 | 6 突变, 预测=实际 |

注：早期一次直接二进制循环误选了 CLI 主程序（usage 退出误计 RED），
已废弃该测量并以 cargo `Executable` 行定位 unittest 二进制重测；
上述数字均为修正后测量。

## 4. 清理与 join 验证

- H3 两条线程均 `join().unwrap()`；seal 经 5s `recv_timeout` watchdog，
  楔死时以超时签名失败而非悬挂。
- 失败路径拓扑与修正前相同（suite 既有惯用法）：主线程若在 release 前
  panic，worker 的 `recv_timeout(5s)` 与 sealer 的 condvar(由 worker 的
  ack 释放)均可在 ≤5s 内自行终止，无永久 parked 线程新增。
- `episode()` 的 disposal-effect 断言（`try_lock` / `disposed` marker）
  在 H1/H3 全分支保持通过 = 无跨终局持锁。

## 5. 工作树与最终 HEAD

```text
PR worktree  (qianqian-r3-wave-a): A1/A2 后 git status 清洁, A3 仅新增本文件
scratch      (qianqian-wavea-mut): 每个突变 git checkout -- 还原, 最终清洁
最终 HEAD 复跑(R2 教训, A3 落地后于 e5f7cb1 实测):
  MUT-A  → 新 H1 GREEN(合法变体仍被接受)
  MUT-H3 → 新 H3 RED(tests.rs:352, left: None, right: Some(Panic), 与 §2.4 同签名)
  scratch 复跑后 git status 清洁
```

## 6. 未证明的交错 / 验证债务（不晋升、不掩饰）

1. **Spurious-wakeup 类 seal 变体**（等一次 Condvar 无谓词重检）未登记
   falsifier——需要强制虚假唤醒的模型(Loom 子模型)或 owner 决策。
2. H3 成功报告分支（record_failure=false）对 no-wait mutant 无判别力
  （两实现同返 None）；其"等待"语义由失败分支+兄弟测试
   (`admitted_failure_response_ack_cannot_be_overtaken_by_seal`、
   `admitted_output_panic_records_before_ack_even_during_seal`)的值判别共同承载。
3. MUT-A 连带的 7 个 playback 侧 RED 提示: 失败诊断 presence/content 的
   场景级 pin 与通用真类契约的分层关系值得 review 议一遍（本波不改）。
4. #252 清单中 H1/H3 行的状态修正（SCREENED 口径 → 本报告证据）属台账
   review 采纳事项; 本报告不改写 #252 历史注释, 仅作增量勘误线索。
5. 测量边界: 所有调度敏感计数(200/200, 199/200, 1/200)是**本机 WSL2
   单宿主**测量; 值判别的确定性主张来自 §2.3 的 happens-before 论证,
   重复次数只是对该论证的经验一致性检查, 不是全调度枚举。
