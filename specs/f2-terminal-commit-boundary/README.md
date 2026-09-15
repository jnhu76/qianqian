# specs/f2-terminal-commit-boundary — terminal Fact commit ownership 边界

> **CAMPAIGN ARTIFACT。** 这是 `QIANQIAN-F2-TERMINAL-COMMIT-BOUNDARY-FORMAL-1`
> 的形式化证据，**不是第二份 authority**，也**不是** architecture acceptance。
> 语义真相仍是 `docs/adr/ADR-PBK-001.md` §2（Fact Plane / semantic commit）
> 与 `docs/adr/ADR-PBK-002.md` §17 D11（episode terminal outcome authority）。
> 本套件**未**接入 CI formal gate（`specs/check.sh` 的 current 集不含它）：
> 它挑战的是一个尚未裁决的语义边界，不是一条已冻结的不变式。

## 本模型回答的唯一问题

> terminal evidence 已经足够时，**谁**拥有把 evidence 变成 terminal Fact 的
> 权利与责任？外部 read / wait 是在**消费** truth，还是在**创造** truth？

它不回答：怎么实现（线程 / callback / Condvar 等机制全部 OPEN）、
F2 的 Rust representation 该长什么样、ADR 该不该改。裁决留给下一轮
ADR-vs-code gap audit。完整结论见 `report.md`。

## 三个变体（`Ownership` 常量）

| 变体 | 语义 | 提交点 | current Rust 对应 |
| --- | --- | --- | --- |
| A `OwnershipConsumerTriggered` | 只有外部 consumer 调用 `wait()`/`try_resolve_now()` 的那一步能提交 | 外部调用者的动作 | **就是今天的实现** |
| B `OwnershipAuthorityOwned` | Playback Session semantic authority 自己拥有提交动作，证据决定性后推进提交 | 系统内部动作 | 今天不存在对应实现 |
| B′ `OwnershipAtomicWithEvidence` | 提交与"最后一块决定性证据"同一步完成（不做延迟解析） | 证据发布动作本身 | 今天不存在对应实现 |

三者都**不改变** D11 的 Completed / Stopped / Failed 命题文本；差别只在
"谁触发 / 何时触发 / 谁在 writer 集合里"。B′ 是唯一的把
`证据决定性 ⇒ Fact 已存在` 变成 **safety**（而非进度承诺）的形态。

## 模型词 → current production reality 映射

| 模型动作 | production referent | 现实依据 |
| --- | --- | --- |
| `Activate` | session 激活成功（edge 已绑定、render stream 已打开、worker 已 spawn） | `crates/qianqian-playback/src/session.rs` `activate_inner` |
| `PublishActivationFailure` | `SessionCompletion::activation_failed` | `completion.rs`（first-wins） |
| `RequestStop` | `SessionCompletion::request_stop` | command 状态，不是 fact；`stop_requested` 单调 FALSE→TRUE |
| `PublishDecodeFailure` | `SessionCompletion::decode_failed` | worker / panic guard 发布，first-wins |
| `PublishWorkerEof` / `PublishWorkerStopped` / `PublishWorkerFailed` | `SessionCompletion::worker_exited(edge.terminal())` | `decode_worker` 退出前最后一步；`EdgeTerminal` first-wins 单调 |
| `PublishDrainDrained` / `PublishDrainAborted` | `DrainSignal::complete(verdict)` | render 线程退出前的最后一步（`qianqian-output-wasapi/src/wasapi.rs` `run_render_thread`） |
| `BeginTeardown` / `FinishTeardown` | session fiber 的 effect 逆序释放（stop edge + join worker → `stop_and_join` stream） | `session.rs` 的 `register_relation` 逆序 LIFO |
| `ConsumerWait` | `wait()` 的等待语义（未到达提交那一轮 resolve） | `completion.rs::wait`（20 ms 轮询 + `resolve`） |
| `ConsumerObserve` | 纯读（F2 要求的 read seam 形状；今天只有 `stop_requested`/`source_format`/`activation_error`/`buffered_frames` 是纯读） | `completion.rs` |
| `ConsumerTriggeredCommit` | `wait()` / `try_resolve_now()` → `resolve()` → `state.outcome = Some(...)` | `completion.rs::resolve`；生产唯一调用者 `apps/headless/src/main.rs:116` |
| `AuthorityResolve` | 候选 B：authority 自己的提交动作（无现成实现） | — |

模型变量（`stopRequested` / `decodeFailure` / `workerTerminal` / `drainVerdict` /
`activationFailed` / `terminalOutcome` / `episodeLifecycle`）分别对应
`CompletionState` 的 `stop_requested` / `decode_failure` / `worker_terminal` /
`DrainSignal.verdict` / `activation_failure` / `outcome` 与 episode 生命周期。

`CandidateOf(...)` 逐分支复刻 `completion.rs::resolve` 的 current precedence
（decode failure > worker Failed > Drained+Eof > Aborted+Stopped（stop 意图
只做区分器）> Aborted+Eof）。**这是 current realization，不是 D11 命题本身**：
改 precedence 若不改变外部命题无需回 authority review，改了外部命题才需要。

### verifier-only 辅助变量（非 normative）

`firstCommitted`（检出改写）、`committerStepped`（writer 集合）、`consumerAsked`
（是否有外部询问）只在模型内存在。它们**不要求** production 具有任何对应表示
（AGENTS.md：auxiliary verifier-only variables are allowed, non-normative）。

## 抽象里刻意保留的现实约束（不是额外假设）

1. **证据组合的合法性**：`Drained ⇒ worker ∈ {None, Eof}`（render 只在 edge
   terminal = Eof 后才可能报 Drained，而 edge terminal 就是 worker 退出时读到的
   那个 first-wins 值）；`decodeFailure ⇒ worker ∈ {None, Failed}` —— 前半条是
   保留的现实约束；**后半条是理想化顺序而非严格现实约束**：production 里
   `decode_failed` 与已放开的 stop edge 竞争可产生 decode_failure ∧ worker=Stopped
   的组合（first-wins 使 edge.fail() 成为 no-op）。无害：模型与 production 都把
   decode failure 排在判决第一位，被排除的组合在两侧都判 Failed。
2. **activation 失败后不存在任何 leg**：证据发布动作一律要求 `~activationFailed`。
3. **teardown 完成 ⇔ 两个 leg 的 join 都已返回**（`FinishTeardown` 的守卫）：
   `worker.join()` 返回时 `worker_exited` 必然已执行；`stop_and_join()` 返回时
   `drain.complete` 必然已执行。因此 "证据齐备" 是 teardown 完成的前置条件，
   而不是我们额外假设的——这条由模型推出 `TeardownImpliesDecisive`。
4. **teardown 的两个阶段在本模型折叠**：中间态（只 join 了一条 leg）对
   terminal commit 问题不可观测，故不建两条 join 的次序。
5. `CHECK_DEADLOCK FALSE`：终局之后"环境停摆"是 `[][Next]_vars` 允许的合法
   行为，也是本模型要表达的东西（"没人做任何事"恰恰是 A 变体的失败模式）。
6. **已知的粗粒度处（如实声明）**：`Activate` 是一个原子步骤，但现实中 render
   侧的 drain verdict 在一个极端时序下可以在激活**期间**就发布（read_frames 因
   设备错误直接 abort）。本模型把这类情形折叠进 Activate 之后，不区分
   "激活中发布"与"激活后发布"——它对 terminal commit 所有权问题不可观测，
   但不适合用来推理激活期的机制时序。
7. **决策输入的 cause 维度不存在**（忠实转录，不是简化）：`CandidateOf` 的输入
   只有 (stopRequested, decodeFailure, workerTerminal, drainVerdict)。production
   里 edge 的 `Stopped` 终态不携带"用户停的还是设备死的"，`resolve()` 读的是
   调用瞬间的 `stop_requested`。模型里因此不存在 chronology 变量，
   "晚到 stop 改变分类"这种带时序的断言**不在**本模型范围内（见 report 的
   "判决瞬间窗口"一节）。

## 检查清单（`check.sh`）

正常模型（3）：A / B / B′，全部 invariant + 两条条件性进度性质必须 PASS。

| 类别 | 文件 | 期望 |
| --- | --- | --- |
| M1 `ObserveCommits` | `mutations/ObserveCommits.cfg` | 违反 `OutcomeWrittenOnlyByContractCommitter`（纯读变 writer ⇒ S2 非空洞） |
| M2 `TerminalRewritable` | `mutations/TerminalRewritable.cfg` | 违反 `TerminalOutcomeImmutable`（⇒ S1b/S3 非空洞） |
| M3 `WaitIsSoleResolver` | `mutations/WaitIsSoleResolver.cfg` | liveness 反例 `TerminalEvidenceCommitsEventually` |
| M4 `AuthorityResolverRemoved` | `mutations/AuthorityResolverRemoved.cfg` | 同上（进度确实由 authority 侧动作承载） |
| M5 `ActivationFailureIsFailed` | `mutations/ActivationFailureIsFailed.cfg` | 违反 `ActivationFailureIsNotTerminalFailed`（⇒ S4 非空洞） |
| M6 `ResolverIgnoresEvidence` | `mutations/ResolverIgnoresEvidence.cfg` | 违反 `NoFalseCompleted`（⇒ S5 非空洞；附带击穿 2 条 Scenario，见 cfg 注释） |
| M7 `StopDiscriminatorRemoved` | `mutations/StopDiscriminatorRemoved.cfg` | 违反 `ScenarioUserStop`（precedence 负控制；证明场景矩阵有约束力） |
| M9 `FailureDowngraded` | `mutations/FailureDowngraded.cfg` | 违反 `ScenarioDecodeFailureNeverStopped`（附带击穿 `NoFalseStopped`：解析蕴含、经独立单检查项 run 验证；TLC 每状态只报第一条，套件输出不展示附带杀伤，见 report「运行结果的读法」） |
| M8 `TeardownBeforeLegsJoined` | `mutations/TeardownBeforeLegsJoined.cfg` | 违反 `TeardownImpliesDecisive`（join 纪律是承重的，不是假设） |
| 反向控制 ×3 | `mutations/Overclaim_*.cfg` | `EpisodesEventuallyTerminate` 必须被违反（模型不得宣称无条件终结） |
| fairness 承重 ×2 | `probes/NoFairnessProgressFails_*.cfg` | 去掉 fairness 后条件性进度必须被违反 |
| witness ×9 | `probes/*.cfg` | 断言必须被违反 = witness 找到 |
| 不可达取证 ×1 | `probes/NoConsumerCommitImpossible.cfg` | 必须 PASS = 已证"无 consumer 时提交不可达" |

**没有负控制的断言（如实声明，不作为"有约束力"的证据引用）**：
`EvidenceConsistent`（守卫一致性自查）与 `ScenarioNaturalEof`（Completed 规则转录）
没有任何 cfg 会违反它们。`Scenario*` 块整体是 **current realization 的
decision table 转录**，不是 D11 命题本身——D11 只写 Stopped ⇒ had recorded stop
intent，模型断言的是更强的反向；其负控制是 M7/M9/M6。

**读 TLC 结果的纪律**：`-continue` 的 violated 列表**不是**穷尽清单（TLC 在同一
违规状态通常只报一条）。mutation cfg 的 INVARIANT 列表读作"本次检查了这些"，
runner 只要求目标出现在 violated 集合里。

## 运行

```bash
specs/f2-terminal-commit-boundary/check.sh   # 27 条 TLC run（3 正常 + 9 mutation
                                            # + 3 over-claim + 2 fairness + 9 witness
                                            # + 1 不可达取证）
```

工具链与其它套件共用 `specs/tools/tla2tools.jar`（v1.7.4，sha256 校验，
缺失自动下载）。任何 TLC Warning 即 FAIL（fail closed，无 whitelist）。

## Bounds / 假设（结论只在这个范围内成立）

- 状态空间：A 209 / B 262 / B′ 162 个 distinct state（穷举，无界参数除
  `Ownership` / `ConsumerEnvironment` / `Mutation` 三个常量外没有其它常量）。
- 每条 run 的进度性质依赖其 `SPECIFICATION` 里显式写出的 fairness；
  没有 fairness 的 run 是**反向控制**，不是结论。
- 不建模：K0 / Capability / ComponentSpec / Fiber 依赖图 / PCM / WASAPI /
  decoder staging / seek / pause / open / Generation / Window /
  Realtime Audio Runtime / 线程调度 / 内存序 / 物理可听性。
- 单 episode、单次 teardown、无 multi-session / preload / gapless。
