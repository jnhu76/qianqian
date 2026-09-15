# QIANQIAN-F2-TERMINAL-COMMIT-BOUNDARY-FORMAL-1

```text
BASE_SHA:  6d7cc200e460177348be2e53e3eae8b65b53344f  (current main: post-#139/#140/#141)
BRANCH:    formal/f2-terminal-commit-boundary-1
HEAD_SHA:  2afdc2c3ca2732abe790485b6a3c5555efff5e15  （模型 + runner + README
           + adversarial review 修正；本报告在其后单独提交，
           它描述的就是 2afdc2c 这棵树）
WORKTREE:  /home/hoo/Source/qianqian，干净
ARTIFACT:  specs/f2-terminal-commit-boundary/
RUNNER:    specs/f2-terminal-commit-boundary/check.sh（27 条 TLC run，全绿）
```

```text
MODEL_SCOPE:
    one playback episode terminal semantic machine
    （evidence 发布 → 判决 → terminal Fact 提交；单 episode / 单次 teardown）

AUTHORITY_READ:
    docs/adr/ADR-PBK-001.md §2.1–2.3（semantic commit 定义 / fact authority
    identity / Projection 非权威 / commit-first-then-publish）、§13 formalization
    policy（formal evidence 约束语义、不选实现机制）
    docs/adr/ADR-PBK-002.md §17 D11（episode terminal outcome authority：
    Decision / Terminal outcome propositions / Mechanism evidence firewall /
    App·K0·projection firewall / Still OPEN）
    AGENTS.md "Verification authority boundary"、"Command / Fact / Projection
    discipline"、"Realtime / PCM firewall"

CODE_REALITY_READ:
    crates/qianqian-playback/src/completion.rs   （SessionCompletion / resolve）
    crates/qianqian-playback/src/session.rs      （activate / decode_worker / effect 逆序）
    crates/qianqian-playback/src/edge.rs         （EdgeTerminal first-wins）
    crates/qianqian-audio-api/src/ports.rs       （DrainSignal write-once）
    crates/qianqian-output-wasapi/src/wasapi.rs  （render 线程 drain.complete）
    apps/headless/src/main.rs                    （生产唯一 wait() 调用点）
```

---

## 先说人话

我们比较了两种做法，问的是同一个问题：**当"这首歌完了"的证据已经齐了，谁负责把证据变成事实？**

**方案 A（= 今天的代码）**：证据本身不会自动变成事实。decode worker 报了 EOF、
设备报了 drain 完成、两个 leg 都 join 干净了 —— 但 `Completed` 这个事实仍然不存在，
直到外面**有人主动调用** `wait()` / `try_resolve_now()`，那一刻 `resolve()` 才把
`state.outcome` 写成 `Some(...)`。

TLC 找到了什么？**找到了反例，而且比预想的更短**：

```text
初始 → Activate → worker 报 Eof → render 报 Drained → BeginTeardown → FinishTeardown
终点状态：workerTerminal=Eof, drainVerdict=Drained, episodeLifecycle=TeardownDone,
        terminalOutcome="None", consumerAsked=FALSE
```

说人话：**音乐其实已经正常播完了，两个 leg 都收干净了，但 "Completed" 这个事实
没人负责落锤。** 只要外部消费者**不再问**（模型里：要么不假设"消费者会一直来问"
这条 fairness，要么干脆没有消费者），事实就永远不存在。还有一条更硬的：在 A 里
把外部消费者整个去掉，**提交根本不可达**——`probes/NoConsumerCommitImpossible.cfg`
断言"永远不会有 outcome"，TLC 穷举全状态空间后 **PASS**（= 已证不可达）。
也就是说，A 的事实存在性完全挂在外部调用者身上。

**方案 B**：Playback Session 自己负责在证据足够时提交 terminal Fact；外面的
observation 只看、wait 只等（两者都不再是 writer）。TLC 验证：所有与 A 相同的不变式
照样 PASS（不扩大 D11 命题），而且**没有一个外部参与者时事实照样成立**
（`Init → Activate → PublishDecodeFailure → AuthorityResolve → Failed`，
全程 `consumerAsked=FALSE`）。

**最重要的差别不是代码放在哪，而是两件事：**

1. **谁承担"进度"这个假设。** 没有任何一个变体能免费获得进度——把 fairness 拿掉，
   A 和 B 的"证据齐了最终会提交"**都会被违反**（我们专门做了这个反向控制）。
   差别是：A 需要的假设是"**外面的人会一直来问**"，B 需要的假设是
   "**我们自己的提交动作会被调度到**"。（"环境 vs 系统"这个说法是**代码审计
   结论被编码成 guard 的结果**，不是 TLA+ 挣来的区分——TLA+ 里所有动作都由同一个
   外部调度器选。完整的降级表述见 "LIVENESS / FAIRNESS" 一节。）
2. **判决发生在哪个瞬间。** 这是本轮另一种形式的"事实依赖消费者"：
   在 A 里，`Failed` 还是 `Stopped` 的判决瞬间是**外部调用者选的**；
   在 B 里是 authority 自己选的；在 B′ 里被钉死在"最后一块决定性证据落地"
   的那一步。三者都符合 D11 现有文字（D11 只说 "at terminal semantic
   resolution"，没说这个 resolution 何时发生）——这正是下一轮要裁的缝。

一句话总结：**今天的事实确实是被外部消费者"问"出来的，而不是 authority
自己"确立"的。这是不是 bug，本轮不下结论；但"D11 指定了谁决定、没有指定谁触发"
这个形式化事实是确定的，下一轮应该去查 ADR 还是查代码，报告最后一节给了清单。**

---

## CURRENT RUST REALITY（先记录现实，不评价对错）

```text
who publishes evidence:
    decode worker        SessionCompletion::decode_failed()          [first-wins]
                         SessionCompletion::worker_exited(edge.terminal())
                         （decode_worker 退出前最后一步；edge terminal first-wins 单调）
    render thread        DrainSignal::complete(Drained|Aborted)
                         （run_render_thread 退出前最后一步；write-once）
    activation path      SessionCompletion::activation_failed()      [first-wins]
                         SessionCompletion::set_source_format()
    App                  SessionCompletion::request_stop()           [command，非 fact]

who currently commits:
    crates/qianqian-playback/src/completion.rs :: resolve()
    —— 它是唯一写 state.outcome 的地方。
    production 唯一调用者：apps/headless/src/main.rs:116  completion.wait()
    try_resolve_now() 是 pub，但全仓调用者只有 tests/（stop_seam、edge_lifecycle、
    session_activation）。
    session.rs 自己从不调用 resolve()。

what Observe would do（今天的可用面）:
    今天**没有**"只读 outcome 而不提交"的生产读缝。
    try_resolve_now() 是"会提交的读"（它调用 resolve()，因此会写 state.outcome）。
    今天真正的纯读只有 stop_requested() / source_format() / activation_error() /
    buffered_frames()。

what Wait currently does:
    completion.rs::wait() 在 loop 里反复调用 resolve()，
    两次之间用 signal.wait_timeout(20ms) 等（drain verdict 在自己的 condvar 上，
    这个有界轮询是桥）。第一次看到决定性证据就提交并返回。
```

---

## MODEL A — CONSUMER-DRIVEN COMMIT

```text
definition:
    terminalOutcome 初始 "None"。
    唯一能把它变成 terminal Fact 的动作是 ConsumerTriggeredCommit：
    需要 ConsumerEnvironment = ConsumerPresent ∧ 证据已决定性 ∧ outcome 仍为 None。
    机制证据发布动作**从不**提交。
    ConsumerWait / ConsumerObserve 在 A 下都不写 terminalOutcome。

invariants（A cfg 全部 PASS）:
    TypeOK / EvidenceConsistent / TerminalOutcomeImmutable /
    OutcomeWrittenOnlyByContractCommitter / NoFalseCompleted / NoFalseStopped /
    ActivationFailureIsNotTerminalFailed /
    DiagnosticActivationFailureLeavesOutcomeUncommitted / TeardownImpliesDecisive /
    BoundaryCommitRequiresConsumer / ScenarioNaturalEof / ScenarioUserStop /
    ScenarioDeviceAbortWithoutStop / ScenarioDecodeFailureNeverStopped
    properties: TerminalEvidenceCommitsEventually（需要 WF(consumer 提交动作)）
                TeardownCommitsEventually（同上）

counterexamples（两条，都可达）:
    1) probes/TeardownWithoutFactWitness_OwnershipConsumerTriggered.cfg
       断言"teardown 完成而 Fact 缺席"不可达 → 被违反（witness found）
       注意：在**假设了 WF(consumer 提交动作)** 的 A 里，这个状态是"暂时"的
       （条件性进度性质在同一 cfg 里 PASS）；它变成"永久"当且仅当外部不再问
       （SpecNoFairness）或没有外部（ConsumerAbsent）——两种情形都有单独取证。
    2) probes/DecisiveEvidencePendingWitness.cfg
       断言"证据决定性 ⇒ Fact 已存在" → 被违反，最短 trace 只有 3 步

shortest decisive trace（CE #2）:
    Init → Activate → PublishDecodeFailure
    终点：decodeFailure=TRUE, terminalOutcome="None", consumerAsked=FALSE
    （此刻 Candidate = Failed 已决定性，事实不存在）

shortest teardown trace（CE #1，已按 review 修正为含 Activate 的忠实路径）:
    Init → Activate → PublishWorkerEof → PublishDrainDrained
         → BeginTeardown → FinishTeardown
    终点：workerTerminal=Eof, drainVerdict=Drained,
          episodeLifecycle=TeardownDone, terminalOutcome="None", consumerAsked=FALSE
    同一 run 里 TeardownImpliesDecisive 必须 PASS —— 也就是说
    "证据已齐备且决定性"不是我们的口径，是模型自己证明的。
    （review 修正前 BeginTeardown 允许从"既没激活也没失败"的 BeforeActivation 出发，
      那是一条 production 里不存在的路径；现在的守卫是
      "Active ∨ (BeforeActivation ∧ activationFailed)"。）

plain-language meaning:
    "歌播完了、leg 都 join 了、事实不存在。" Fact 的存在性挂在外部调用者身上。
    极端情形：整个 episode 期间都不存在外部消费者 → 提交不可达（已证）。
```

### 对 A 的最强反方读法（以及它为什么仍然不解决问题）

反方会说：`wait()` 本身就是一个阻塞循环（`completion.rs:238-256`，每 20 ms
重新 resolve 一次），所以"消费者会一直问"根本不是额外假设，而是 `wait()` 的
定义。因此 A 在实践中就是"事实一定会被确立"；模型里那个"永久 pending"只是
"线程被无限期冻结"的极端情形，没有工程意义。

这个反驳有一半成立，而且正好切中本轮的边界：

1. 它成立的**前提**是"有一个参与者正阻塞在 wait() 里"。在模型里，这不是
   fairness，而是 `ConsumerEnvironment = ConsumerPresent` 这个**结构性**前置
   条件；把它置成 `ConsumerAbsent`，提交不是"慢"，而是**不可达**（穷举证明）。
   换成一句人话：**事实的存在需要一个外部参与者存在。** 这正是本轮要问的问题，
   而不是它的解法。
2. F2 恰好把这条前提削掉一半：`status` 必须走"纯读、无副作用"的 read seam
   （Issue #119 F2 完成标准："read 必须无副作用（read 不 resolve / 不 commit
   authority）"）。**纯读者不是提交者。** 于是"谁负责问"必须由另一个参与者
   承担；如果驱动层的形状是"轮询 status"（UI 最自然的形状），就没人承担它。
3. 更重要的是：fairness 争的是"事实**何时**存在"，而 A 还有一个 fairness
   解决不了的差别——"事实**是什么**"。判决瞬间由调用者选择，于是完全相同的
   证据，在"有没有一个晚到的 stop 落在判决之前"这两种世界里会给出不同的分类
   （见"判决瞬间窗口"一节）。这一条与 fairness 无关：B′ 把它钉死在证据落地那一步，
   B 只是把它从"调用者选择"换成"authority 自己选择"。

（顺带说明模型为什么把 `ConsumerWait` 与 `ConsumerTriggeredCommit` 分成两个动作：
`wait()` 的一次调用在抽象上同时包含"等"和"到达提交的那一轮 resolve"。若合并成
一个动作，`WF` 会被那个"等了但还没提交"的分支满足，liveness 结论会变成空洞的。
分开命名并**不是**为了让 A 输——A 在 `SpecConsumerTriggered` 下两条条件性进度
性质都是 PASS 的，这一点在报告里如实写出。）

因此本轮的结论不是"A 会卡住"，而是：**A 把事实的存在与内容都挂在外部参与者的
行为上。** 这条是否可接受，取决于 D11 想表达的是"谁决定"还是"谁触发/何时"——
那是下一轮的权威裁决，不是本模型能裁的。

---

## MODEL B — AUTHORITY-OWNED COMMIT

```text
definition:
    Playback Session semantic authority 自己拥有一个提交动作 AuthorityResolve：
    证据决定性时推进并提交。外部 ConsumerWait / ConsumerObserve 是**纯动作**，
    在任何变体下都不写 terminalOutcome。
    （D11 的 Completed/Stopped/Failed 命题文本不变；变的只是触发者与 writer 集合。）

invariants（B cfg 全部 PASS）:
    与 A 相同的集合，去掉 BoundaryCommitRequiresConsumer（它在 B 下被击穿）。
    另外 **不**断言 DecisiveImpliesCommitted：B 的提交仍然可以延迟到下一次
    authority 执行机会，所以那条强度属于 B′。

required fairness:
    WF_vars(AuthorityResolve)（写在 SPECIFICATION SpecAuthorityOwned 里，
    由 cfg 显式选择；没有 fairness 的 SpecNoFairness 是反向控制）。

counterexamples:
    1) probes/CommitWithoutConsumerWitness.cfg —— 断言"Fact 存在 ⇒ 有外部询问"
       （A 的特征命题）在 B 下被违反：
       Init → Activate → PublishDecodeFailure → AuthorityResolve
       终点：terminalOutcome="Failed" ∧ consumerAsked=FALSE
    2) probes/NoConsumerCommitWitness.cfg —— 完全没有外部消费者时
       "永远不会有 outcome" 被违反（提交可达）。
    3) probes/TeardownWithoutFactWitness_OwnershipAuthorityOwned.cfg —— 同 A 的
       witness 依然可达：B 只把"事实缺席"从**永久**降级为**暂时**
       （liveness），没有把它变成 safety。

plain-language meaning:
    authority 自己负责落锤；外部只看。进度假设从"环境"搬回"系统自己"。
```

## MODEL B′ — COMMIT ATOMIC WITH THE DECISIVE EVIDENCE

```text
definition:
    哪一步发布了"最后一块决定性证据"，就在那一步同时提交。
    authoritiy 仍然拥有提交权，只是提交与其依据不再分离。零 fairness 假设。

invariants（B′ cfg 全部 PASS，含两条 A/B 都没有的）:
    DecisiveImpliesCommitted            ← 存在性从"进度承诺"降级为 **safety**
    DiagnosticTeardownWithoutFactUnreachable  ← teardown 完成时事实必然已存在

plain-language meaning:
    B′ 是三种变体里唯一把"证据决定性 ⇒ Fact 已存在"变成不变式（不变量）的形态：
    不需要任何 fairness 假设就成立。代价是它把"判决瞬间"钉死在证据落地那一刻，
    而 D11 现有文字对这个瞬间是沉默的（下一轮裁）。
```

---

## SAFETY RESULTS

| # | 命题 | 结果 | 证据 |
| --- | --- | --- | --- |
| S1 | 至多一个 terminal outcome；一旦 commit 不可改写 | **PASS**（三变体） | `TerminalOutcomeImmutable`（`firstCommitted` 写入一次，任何改写立刻可见）；非空洞性由 M2 证明 |
| S2 | Observe 是纯读（reading truth never creates truth） | **PASS**（三变体）；termimalOutcome 一侧机器检查 | `OutcomeWrittenOnlyByContractCommitter`：`terminalOutcome ≠ None ⇒ committerStepped`；M1 让 Observe 偷偷提交 → 立刻被抓住。其余变量（stopRequested / evidence / lifecycle）的纯读性在模型里是 `UNCHANGED` 结构，见 "WHAT THE MODEL DOES NOT PROVE" 的诚实声明 |
| S3 | 晚到的 stop 不能改写已提交终局 | **PASS**（三变体） | 同 S1 的不变式；另有 3 条 witness（`probes/LateStopAfterCommitWitness_*.cfg`）证明"已提交 Fact + 晚到 stop 意图共存"可达且 Fact 未变；M2 证明检查有约束力 |
| S4 | activation failure 不是 D11 Failed | **PASS**（三变体，含更锐的命名诊断 `activationFailed ⇒ outcome = None`） | `ActivationFailureIsNotTerminalFailed` + `DiagnosticActivationFailureLeavesOutcomeUncommitted`；M5 把它升格成 Failed → 立刻被抓住 |
| S5 | 不虚构 Completed | **PASS**（三变体） | `NoFalseCompleted`；M6（resolver 忽略证据）→ 立刻被抓住 |
| S6 | 不虚构 Stopped | **PASS**（三变体） | `NoFalseStopped`；M2 的附带杀伤（改写路径伪造 Stopped）证明它有约束力 |

补充：模型还证明了 **`TeardownImpliesDecisive`**（teardown 完成 ⇒ 证据已齐备且决定性），
不是靠假设，而是由"两个 leg 的 join 语义 + 证据组合的合法性"推出。这条是本轮
B2 结论的地基：**teardown 之后缺的不是证据，是判决。**

---

## BOUNDARY RESULTS（本轮重点）

```text
B1 — Consumer independence（terminal Fact 是否依赖外部 Wait/Observe）

    **证据链（review MAJOR-2 修正后，按贡献顺序）**

    前提（代码审计，TLC 不检查）：`resolve()` 是 state.outcome 的唯一 writer
        （completion.rs:232–241），而它的 production 唯一调用者是
        apps/headless/src/main.rs:116 的 completion.wait()；session.rs 从不 resolve。
        ⇒ "提交动作由外部调用者承载"这条**前提**来自代码，不是 TLC 发现的。

    后果（TLC 穷举，这才是机器的贡献）：
        probes/NoConsumerCommitImpossible.cfg —— A 变体 + 无 consumer，
        断言 NeverCommitted（outcome 永远 "None"）。TLC 穷举全状态空间后 **PASS**
        ⇒ 在该模型里"无外部参与者时提交不可达"是穷举事实（174 states generated,
        78 distinct）。对称控制：同一个断言在 B + 无 consumer 下被违反
        （witness：Init → Activate → PublishDecodeFailure → AuthorityResolve，
        全程 consumerAsked=FALSE）。

    辅助（bookkeeping 性质，**不作为强证据引用**）：
        BoundaryCommitRequiresConsumer == (outcome ≠ None ⇒ consumerAsked)。
        它在 A 下构造性成立（每个 writer 都设 consumerAsked），
        并且会被本套件自己的 M5 顺带违反——这说明它是"writer 集合的记账性质"，
        而不是一条独立发现的深层真理。报告不再把它当"最硬的证据"。

    ⇒ B1 的正确表述：**代码前提（谁调用 resolve）+ TLC 后果（无 consumer 时
      提交不可达）**；两者缺一不可。只有后者不成立（那是模型自证），
      只有前者也不够（那没有穷举覆盖）。

B2 — Teardown boundary（teardown 完成而 Fact 仍缺席）

    该状态在 A 可达（witness + 上面 5 步 trace），在 B 可达但只是**暂时**
    （liveness 覆盖：TeardownCommitsEventually 在 B 的 fairness 下 PASS），
    在 B′ **不可达**（DiagnosticTeardownWithoutFactUnreachable 是 B′ 的不变式）。
    ⇒ 三个变体给出三种不同的强度阶梯：
        A  : 永久缺席（除外部询问外无解除条件）
        B  : 暂时缺席，靠系统自己的进度假设解除
        B′ : 不可能缺席

    关于"是否应该冻结 teardown ⇒ Fact 必须已存在"：本模型只报告可达性，不裁决。
    可用的论据是：teardown 完成前两个 leg 的 join 都已返回，也就是说决定性证据
    在 authority **退休之前**就已经完整存在；因此"退休后观察者永久拿不到真相"
    的形态在 B′ 被排除。是否要把这条写成 ADR 承诺，是下一轮的事。

B3 — Retained observation（事实一旦提交，之后无限次观察都读到同一个值）

    PASS（三变体）：consumerAsked/观察动作不改变 terminalOutcome；
    `probes/LateStopAfterCommitWitness_*.cfg` 显示晚到的 command 也不会改变它。
    注意这条**不等于**"defer 掉的真相不会丢"：在今天的 Rust 里，
    CompletionState / DrainSignal 都被 App 持有，所以 teardown 之后**再**调用
    wait() 依然能正确解析——A 的问题是"事实没被确立"，不是"真相丢了"。
    模型里这一点由"证据 write-once + 永不回退"承载。

B4 — No observer-owned authority（writer 集合）

    terminalOutcome 的 writer 集合（构造 + 机器检查双证）：

        Model A : { ConsumerTriggeredCommit }        ← 由**外部调用者**执行
        Model B : { AuthorityResolve }               ← 由**系统内部**执行
        Model B′: { PublishDecodeFailure, PublishWorkerEof,
                    PublishWorkerStopped, PublishWorkerFailed,
                    PublishDrainDrained, PublishDrainAborted,
                    PublishActivationFailure(仅 M5 负控制) }
                   ← 证据发布动作本身

    ConsumerWait / ConsumerObserve 在**任何**变体里都不在 writer 集合内。
    机器检查：`OutcomeWrittenOnlyByContractCommitter`；
    非空洞性：M1（Observe 提交）、M2（RequestStop 改写）都被抓住。

    两条如实声明（回应 review Q8）：
    (a) 这条不变式检查的是模型的记账变量 `committerStepped`，"谁写 outcome"
        与"谁登记 writer"是否一致；它检验的是**模型自身的一致性**，
        而不是"现实里的 read seam 会不会偷偷提交"。
    (b) production **还不存在**纯读 outcome 的 seam（F2 尚未落地），
        所以 S2 检验的是一个尚未实现的设计假设，不是既有实现的性质。
```

---

## LIVENESS / FAIRNESS

```text
exact assumption:
    A  : SpecConsumerTriggered  == Init /\ [][Next]_vars /\ WF_vars(ConsumerTriggeredCommit)
    B  : SpecAuthorityOwned     == Init /\ [][Next]_vars /\ WF_vars(AuthorityResolve)
    B′ : SpecAtomicWithEvidence == Init /\ [][Next]_vars        （无 fairness）
    反向控制：SpecNoFairness     == Init /\ [][Next]_vars        （两条都加）

why needed:
    没有 fairness 时，"证据决定性后最终提交"在**所有**变体里都可被违反。
    最干净的 counterexample 取自在 B 上做无 fairness 的实验：
        ... → PublishDecodeFailure（已决定性）→ PublishWorkerFailed
            → PublishDrainAborted → FinishTeardown → **Stuttering 永远**
        终点：evidence 齐备、teardown 完成、consumerAsked=TRUE，outcome 仍是 None。
    这正是 §13 CE-B1 要的行为，也说明 fairness 是**承重**的：
    A 的进度挂在外部动作上（"外面的人会一直来问"），
    B 的进度挂在自己的动作上（"我们自己的提交会被调度"）。
    B′ 不需要 fairness，因为它把存在性做成了 safety。

为什么"把 WF 放在不同动作上"是有意义的判断（回应 review Q4/Q7）:
    TLA+ 本身**没有** environment / system 之分——Next 里每个动作都由模型外部
    的调度器选择，三变体共用同一套动作。因此"环境 vs 系统"不是 TLA 发现的结构，
    而是**代码审计结论被编码成 guard 的结果**：A 里唯一能写 outcome 的动作
    需要 ConsumerEnvironment = ConsumerPresent（因为 production 里只有
    wait()/try_resolve_now() 会调用 resolve()），B/B′ 里不需要任何外部动作。
    换句话说：这条区分不是命名把戏，但它也不是 TLC 挣来的——它的证据在代码侧，
    TLC 只负责把它的后果穷举出来（"无 consumer ⇒ 提交不可达"）。
    本条按 fresh review 的要求如实降级表述。

what it does NOT imply（§15 纪律）:
    模型**不**证明"每个 episode 最终都会结束"，也不证明"每首歌最终都能播完"。
    我们专门为这条纪律加了反向控制：mutations/Overclaim_*.cfg 在三个变体上
    断言 EpisodesEventuallyTerminate，三条**全部必须被违反**——结果是全部被违反。
    也就是说：媒体不一定 EOF、设备不一定 drain、stop 不一定及时、decoder 可能挂死，
    模型对这些一概不作承诺。它只承诺带完整前提的那一条条件性进度。
```

---

## SCENARIO MATRIX（§12）

判决一侧用 `Candidate`（当前证据上的 D11 命题）；提交值一侧用 S5/S6。
把一个场景直接写成"对提交值的要求"会被合法执行击穿（见下方 note），这是
本轮被 TLC 逼出来的建模纪律。

| 场景 | 判决（Candidate） | A 的提交 | B 的提交 | B′ 的提交 |
| --- | --- | --- | --- | --- |
| 1 自然 EOF（worker=Eof, drain=Drained） | Completed | 需外部询问，否则永久 None | authority 调度到即提交 | 证据落地即提交 |
| 2 用户 stop（worker=Stopped, drain=Aborted, stopRequested） | Stopped | 同上 | 同上 | 同上 |
| 3 设备 abort 无 stop（worker=Stopped, drain=Aborted, ¬stopRequested） | **Failed** | 同上 | 同上 | 同上 |
| 4a decode failure → 之后 stop | Failed（decode failure 优先，不被降格） | 同上 | 同上 | 同上，**stop 先到也一样** |
| 4b stop → 之后 decode failure | Failed（同上） | 同上 | 同上 | 见证：`Activate → RequestStop → PublishDecodeFailure → Failed` |

```text
note（建模纪律，写进 README 与本报告）:
    "worker=Stopped ∧ drain=Aborted ∧ stopRequested ⇒ outcome ∈ {None, Stopped}"
    这种对**当前证据**的要求会误伤合法执行：设备 abort 先按 ¬stopRequested 判 Failed
    并提交，之后才到的 stop 意图把状态改写成 (Stopped, Aborted, stopRequested=TRUE)，
    而提交值仍是 Failed（不可改写 = 正确行为）。TLC 找到过这条反例，我们因此把
    场景断言改成 Candidate 形式，并把"提交值正当性"交给 S5/S6。
```

---

## MUTATION RESULTS

```text
M1 ObserveCommits              → 违反 OutcomeWrittenOnlyByContractCommitter
    trace: Activate → PublishDecodeFailure → ConsumerObserve
           终点 terminalOutcome="Failed" ∧ committerStepped=FALSE
    意义：纯读变成 writer 会立刻暴露 ⇒ S2/B4 不是空洞约束。

M2 TerminalRewritable          → 违反 TerminalOutcomeImmutable（附带 NoFalseStopped）
    trace: Activate → PublishDecodeFailure → ConsumerTriggeredCommit(Failed)
           → RequestStop ⇒ terminalOutcome 被改写成 "Stopped"
    意义：S1b/S3 有约束力；也说明现实里 resolve() 的 `outcome.is_some()` 与
          edge 的 first-wins terminal 两道闸门不是装饰。

M3 WaitIsSoleResolver          → liveness 反例 TerminalEvidenceCommitsEventually
    （在 B 的骨架上取消 authority 提交权，只留外部 resolve 路径；
       fairness 仍按契约落在已被移除的 authority 动作上 ⇒ 买不到进度）
    意义：A 的形状本身就是进度依赖环境的形状。

M4 AuthorityResolverRemoved    → liveness 反例 TerminalEvidenceCommitsEventually
    意义：该进度性质确实由 authority 侧动作承载，不是空洞。

M5 ActivationFailureIsFailed   → 违反 ActivationFailureIsNotTerminalFailed
    （mutation 如实登记提交者，因此被击穿的是 S4 本身，而不是 writer 集合）
    意义：D11 firewall（activation_failure 是 diagnostic，不是 terminal authority）。

M6 ResolverIgnoresEvidence     → 违反 NoFalseCompleted
                                  （附带击穿 ScenarioUserStop 与
                                    ScenarioDeviceAbortWithoutStop —— 非全部四条；
                                    ScenarioNaturalEof 恒返回 Completed 而不受影响，
                                    ScenarioDecodeFailureNeverStopped 亦然）
    意义：S5 有约束力；另外这条 mutation 只污染"判决内容"、不改"提交所有权"，
         而 writer 集合不变式在同一 run 里仍然成立 —— 这是"判决内容"与
          "提交所有权"两件事被分开检查的证据（回应 Q5 的依赖声明）。

M7 StopDiscriminatorRemoved    → 违反 ScenarioUserStop（precedence 负控制，
    由 fresh review MAJOR-1 引入）
    trace: Init → Activate → RequestStop → PublishWorkerStopped → PublishDrainAborted
    意义：场景矩阵断言的是 **current realization 的 decision table**，
         不是 D11 命题本身（D11 只写 Stopped ⇒ had recorded stop intent，
         模型断言的是更强的反向）。M7 证明这一块有约束力；
         同一 mutation 下全部 ownership / boundary 结论不受影响。

M9 FailureDowngraded           → 违反 ScenarioDecodeFailureNeverStopped
                                  （附带击穿 NoFalseStopped）
    意义：失败被 stop 意图覆盖这一方向有明确的负控制。

M8 TeardownBeforeLegsJoined    → 违反 TeardownImpliesDecisive
    trace: Init → Activate → BeginTeardown → FinishTeardown
    意义：本报告 B2 那句"teardown 之后缺的不是证据，是判决"**挂在一条真实的
         实现纪律上**（join 返回 ⇒ worker_exited / drain.complete 已发生）。
         一旦这条纪律被去掉，TeardownImpliesDecisive 立刻不成立——
         也就是说 B2 不是凭空假设出来的。

未设负控制的两条（如实声明，见 review MINOR）:
    EvidenceConsistent       —— 守卫一致性自查（发布动作的 guard 是否维持证据合法性），
                                当前没有任何 cfg 会违反它。
    ScenarioNaturalEof       —— Completed 规则的转录；要击穿它需要造一个
                                "EOF+drain 却判成别的"的荒谬 resolver，
                                本套件不提供这种 mutation。
    这两条**不**作为"有约束力"的证据被引用；它们的角色是转录核对。
```

---

## COUNTEREXAMPLE-FIRST（§13 逐条）

每条都先问"能不能找到反例"，并按 FOUND / NOT FOUND 如实记录。
NOT FOUND 的一条必须同时给出"这条检查为什么不是空洞的"（否则 NOT FOUND 可能只是
模型没能力表达该失败）——这正是 mutation 的用途。

```text
CE-A1  decisive evidence exists, no consumer asks, terminalOutcome remains None
       → FOUND（A）
       最短 trace：Init → Activate → PublishDecodeFailure
                 终点 decodeFailure=TRUE, terminalOutcome="None", consumerAsked=FALSE
       取证：probes/DecisiveEvidencePendingWitness.cfg
       永久性取证：probes/NoConsumerCommitImpossible.cfg（无 consumer 时提交不可达）

CE-A2  teardown completes with decisive evidence but terminalOutcome never commits
       → FOUND（A）
       最短 trace：Init → BeginTeardown → PublishWorkerEof → PublishDrainDrained
                 → FinishTeardown
                 终点 workerTerminal=Eof, drainVerdict=Drained,
                      episodeLifecycle=TeardownDone, terminalOutcome="None"
       取证：probes/TeardownWithoutFactWitness_OwnershipConsumerTriggered.cfg
       同 run 内 TeardownImpliesDecisive 必须 PASS ⇒ "证据已齐备"不是我们的口径。
       这条 trace **不主张 A 违反任何东西**；它主张的是一个结构事实：
       A 里事实的存在性由外部参与者的行为决定。prose 里的"歌播完了"只是这个
       结构事实的中文说法，不是评价。

CE-A3  Observe accidentally commits
       → 正常模型中 NOT FOUND（OutcomeWrittenOnlyByContractCommitter 全程 PASS）
       → 注入 M1 后 FOUND：
         Activate → PublishDecodeFailure → ConsumerObserve
         终点 terminalOutcome="Failed" ∧ committerStepped=FALSE
       结论：该约束有约束力；"read 不创造 truth" 在 terminalOutcome 这一面
       是机器检查过的，不是口号。

CE-A4  late Stop rewrites terminal
       → 作为"改写已提交事实"：NOT FOUND（TerminalOutcomeImmutable 全程 PASS）
         正向取证：probes/LateStopAfterCommitWitness_*.cfg（三变体）
         —— "已提交 Fact + 晚到 stop 意图"可达，而 Fact 未变。
       → 作为"改写尚未提交的判决分类"：FOUND（见本报告"判决瞬间窗口"一节）
         取证：probes/PendingDeviceAbortWindowWitness.cfg（B）
       注入 M2 后"改写已提交事实"变成 FOUND ⇒ 该不变式非空洞。

CE-A5  activation failure alone becomes Failed
       → 正常模型中 NOT FOUND（ActivationFailureIsNotTerminalFailed 全程 PASS）
       → 注入 M5 后 FOUND（违反 ActivationFailureIsNotTerminalFailed）
       附：模型中 activation 失败后证据结构上不可能产生，故还有更锐的
       DiagnosticActivationFailureLeavesOutcomeUncommitted（同样被 M5 击穿）。

CE-B1  B without fairness still allows permanent pending
       → FOUND（这正是 §13 要求确认的自由度）
       trace（SpecNoFairness，B，逐 action）：
             Init → RequestStop → ConsumerWait → BeginTeardown
             → PublishWorkerFailed → PublishDecodeFailure → PublishDrainAborted
             → FinishTeardown → Stuttering 永远
             终点 decodeFailure=TRUE, workerTerminal=Failed, drainVerdict=Aborted,
                  episodeLifecycle=TeardownDone, terminalOutcome="None",
                  consumerAsked=TRUE
       取证：probes/NoFairnessProgressFails_AuthorityOwned.cfg（必须被违反，已违反）
       意义：**没有任何变体免费获得进度**；差别只在假设落在谁身上，
             以及 B′ 把存在性做成 safety（见下）。

CE-B2  B with chosen fairness accidentally claims too much liveness
       → NOT FOUND。我们为此专门加了三条反向控制：在 A / B / B′ 上断言
         无条件终结 EpisodesEventuallyTerminate，三条**必须**被违反，
         实际三条全部被违反（mutations/Overclaim_*.cfg）。
       意义：模型只宣称"证据决定性之后、提交者持续具备条件且获得正常执行机会时
             最终提交"这一条带前提的进度，不宣称"每首歌最终都会结束"。
       非空洞性：同一批 run 里条件性进度（TerminalEvidenceCommitsEventually）
             是 PASS 的 ⇒ 检查器确实在工作，只是没有被喂给错误的命题。
```

## WHAT THE MODEL PROVES

1. 在 **A**（= 当前实现的语义抽象：唯一 writer 是 resolve()，唯一生产调用者是
   wait()）里，terminal Fact 的存在性挂在外部消费者身上；没有外部消费者时
   提交**不可达**（穷举证明，不是抽样）。注意证据链：
   "谁调用 resolve" 来自代码审计，"无 consumer ⇒ 不可达" 来自 TLC。
2. 在 A 里存在可达状态：证据齐备且决定性、两个 leg 都已 join、teardown 完成，
   而 Fact 仍然不存在。**teardown 完成 ⇒ 证据齐备** 是由模型推出的定理。
3. 在 **B** 里，同一批安全不变式照样成立（不扩大 D11 命题），而事实可以在
   **没有任何外部参与者**的情况下成立；进度假设从环境搬到系统自己。
4. 在 **B′** 里，"证据决定性 ⇒ Fact 已存在"成为 safety（不需要 fairness），
   "teardown 完成而 Fact 缺席"不可达。
5. 三种变体都满足：至多一个 terminal、不可改写、不虚构 Completed/Stopped、
   activation failure 不等于 Failed、Observe/Wait 不在 writer 集合内。
6. fairness 是承重的：去掉它，A 与 B 的条件性进度都可被违反（并给出 trace）。
7. 模型**没有**多承诺任何无条件终结命题（三条反向控制全部按预期被违反）。

## 关于运行结果的读法（review MINOR）

```text
1. `-continue` 的 violated 列表不是"全部被击穿的性质"的清单：TLC 在同一个
   违规状态通常只报一条被违反的不变式。因此 mutation cfg 的 INVARIANT 列表
   应当读成"本次检查了这些"，不是"只有目标那条会挂"。runner 的判据
   （目标必须出现在 violated 集合里）不受影响。
2. 判据强度：`pass` = 穷举完成且无违规；`fail:<Inv>` = 目标被违反且 TLC
   自行收尾；`lfail:<P>` = temporal 性质被违反。三者都不等于"性质成立"，
   只等于"在该 bounds 下未被证伪 / 已被证伪"。
```

## WHAT THE MODEL DOES NOT PROVE

```text
不证明媒体一定 EOF                    （PublishWorkerEof 不需要被取用）
不证明 device 一定 drain               （PublishDrainDrained 同理）
不证明 stop 一定及时执行                （RequestStop 同理）
不证明 decoder 不挂死
不证明 Fiber 一定 teardown             （BeginTeardown/FinishTeardown 同理）
不证明 physical audibility             （Drained 只是设备侧 drain 契约完成）
不证明 future seek / pause / open      （不在模型 scope）
不证明线程调度 / 内存序 / loom 级并发
不证明"每首歌最终都会结束"              （§15：这是未授权命题，已用反向控制钉住）
不证明 Rust representation 该长什么样    （机制裁决留给下一轮）

诚实声明（模型内部性质）:
  S2 的机器检查只覆盖 terminalOutcome 这一个 writer 面（OutcomeWrittenOnlyByContractCommitter）。
  ConsumerWait / ConsumerObserve 对 stopRequested / evidence / episodeLifecycle
  的纯读性是模型里的 UNCHANGED 结构（读源码可见），不是独立的不变式。
  我们选择不为它造 shadow-state 检查（会让状态空间平方级膨胀），
  这一条如实声明为"结构性成立、未单独机器检查"。
```

---

## ADR IMPLICATIONS — DO NOT MODIFY YET

```text
current ADR statements potentially relevant（逐字引用，不做裁决）:

  ADR-PBK-002 §17 D11 Decision:
      "The designated semantic authority for episode terminal outcome is the
       Playback Session semantic role for one playback episode."
      "Designation attaches to the **semantic role** — not to `resolve()` or any
       Rust type, and not to Fiber identity"

  ADR-PBK-002 §17 D11 Terminal outcome propositions:
      Completed: "... the episode reached decode EOF, the output mechanism
      reported its drain contract completed, and **the Playback Session authority
      committed terminal completion**."
      Stopped: "**at terminal semantic resolution**, the aborted episode had
      recorded stop intent and no higher-precedence failure classification won."
      "一个 playback episode 至多有一个 terminal outcome；一旦 commit 即不可改写。
       这是 cardinality/immutability contract，**不是 liveness 承诺**。"

  ADR-PBK-002 §17 Mechanism evidence firewall:
      "mechanism evidence ↓ Playback Session semantic decision ↓ terminal outcome
       semantic commit"

  ADR-PBK-001 §2.3:
      "Semantic commit = the producing semantic authority considers the fact
       established, according to that fact's contract."
      "Projection is derived visibility, not authority."

questions to inspect next（下一轮 ADR-vs-code gap audit 的输入，不是结论）:
  Q-a  D11 指定了 authority 身份，但**没有**写"谁/何时触发 commit"。
       "at terminal semantic resolution" 对 resolution 的**时刻**保持沉默：
       由外部调用者决定（A）/ 由 authority 自己决定（B）/ 由证据落地钉死（B′）
       三种读法在现有文字下都无法排除。
  Q-b  "the Playback Session authority committed terminal completion" 里，
       commit 这个**动作**由谁执行？如果把 resolve() 看作 authority 的决策过程、
       外部调用者只是"触发者"，则 A 不违反 D11；如果"外部触发"本身就被视为
       把 commit 权部分让渡给了非 authority，则 A 违反。这是**语义裁决**，
       本轮不做。
  Q-c  D11 明确写"不是 liveness 承诺"，因此**不能**从这里推出
       "teardown 完成必须有 outcome"。本轮只报告该状态可达性，不主张它违法。
  Q-d  "App / K0 / projection firewall" 只说 App 不是 playback semantic authority；
       它没有说 App 不能当触发器。所以"F2 的 status 只读"与"谁触发 commit"
       是两个问题，需要分开裁。
  Q-e  如果最终采纳 B/B′，那是对**触发/时刻**的显式化，不是对
       Completed/Stopped/Failed 命题的修改——但它仍然是 architecture decision，
       必须走 authority review，不能由本 formal evidence 静默升级。

no adjudication yet.
```

## CODE IMPLICATIONS — DO NOT MODIFY YET

```text
current wait/try_resolve behavior（事实陈述）:
  - resolve() 是唯一 writer；它被 wait()/try_resolve_now() 调用。
  - wait() 是一个 20ms 轮询循环；try_resolve_now() 是"会提交的读"。
  - 今天没有任何"只读 outcome、不提交"的生产读缝。
    F2 要求 read 零副作用（Issue #119 F2 完成标准："read 必须无副作用
    （read 不 resolve / 不 commit authority）"）——这条要求与 A 的形状叠加后
    产生一个具体后果：**如果驱动层不调用 wait()，纯读的 status 会永远读到
    pending，哪怕 episode 早就结束了。** 今天的 headless 掩盖了这一点，
    因为 apps/headless/src/main.rs:116 在 dispose 之前阻塞在 wait() 上；
    F2 阶段"status 从正式 read seam 读取"之后，这个掩盖就消失了。

potential mismatch questions（不 patch，先问）:
  M-a  A 的形状是否满足 D11？见 Q-a/Q-b，属权威裁决。
  M-b  如果裁决要求 B/B′，需要的是"authority 拥有提交动作"这一形状，
       而不是某个具体机制（线程 / callback / Condvar 全部仍然 OPEN）。
  M-c  在 A 下，"判决瞬间由外部调用者选择"会带来一个可观测后果：见下一节
       的 late-stop 窗口。是否需要收紧，同样先裁语义再谈机制。
  M-d  try_resolve_now() 作为"会提交的读"与 F2 的"read 零副作用"要求不相容；
       它今天是 test-only seam，是否退役属 F2 representation 门。

no patch yet.
```

---

## 额外发现：判决瞬间窗口（late stop 的第二种形态）

这一条不是本轮主问题，但它是"谁决定判决瞬间"的直接后果，且**在今天的代码里
可达**，因此按 AGENTS.md 的"疑似缺陷立即报告"纪律记录在这里（不下结论）：

```text
设备 abort 路径（无任何用户 stop）:
    render 侧失败 → DrainVerdict = Aborted
                 → render_input.stop() → edge terminal = Stopped
                 → worker 以 Stopped 退出 → worker_exited(Stopped)
    此时 state = { drain: Aborted, worker: Stopped, stop_requested: false }
    resolve() 在这个时刻会给出 Failed{stage:"device"}

但 resolve() 只在有人调用时才跑。如果在这之后、任何一次 resolve() 之前，
用户按了 stop（request_stop 先记意图、再放开 edge）：
    state 变成 { Aborted, Stopped, stop_requested: true }
    resolve() 此刻给出 **Stopped**（D11 的 non-causal 口径允许它：
    "at terminal semantic resolution, the aborted episode had recorded stop intent"）

⇒ 同一个物理事件（设备死了、用户没介入）会因为在"判决瞬间"之前有没有
   晚到一次 stop，而被写成 Failed 或 Stopped。

在今天的 headless 里这个窗口很窄：wait() 的轮询上界是 20 ms，而实际上是
worker_exited 的 notify_all 把它唤醒（completion.rs:224–229；注意 drain verdict
自己的 condvar **不会**叫醒 completion 的等待者），所以常见量级是微秒；
但它不是已发生的 bug；
但在"驱动层不调用 wait()"的形态下（例如 F2 的纯 status 读 + 长时间不结束的
episode），这个窗口就是无界的。
模型侧到底证明了什么（review MAJOR-3 修正后，避免过度归因）：

  (i) **窗口存在**：probes/PendingDeviceAbortWindowWitness.cfg 证明
      "证据已决定性（Aborted + Stopped + ¬stopRequested）而 Fact 尚未提交"
      的状态可达（B 下同样可达）；B′ 下同一断言作为不变式 **PASS**
      （窗口不存在）。
  (ii) **判决只依赖判决瞬间的 stop 意图**：模型里 `resolve` 的输入只有
      (workerTerminal, drainVerdict, stopRequested)，**没有 cause 维度**。
      这不是建模偷懒——它忠实转录了 production：edge 的 Stopped 终态
      不携带"是用户停的还是设备死的"，resolve() 读的是**调用那一刻**的
      stop_requested。ScenarioUserStop（转录）因此把"真实用户 stop"与
      "设备 abort + 晚到 stop"两种世界**合并成同一个状态**。
  (iii) 模型**没有**证明"翻转发生了"：翻转是一个带时序的断言
      （stop 在 abort 证据之后到达），本模型没有 chronology 变量，因此
      这一条**不在机器检查范围内**。它是代码层结论，证据是
      completion.rs:296–306（判别器读 flag）+ request_stop 无 guard
      + apps/headless 的 stdin 线程可在任意时刻调用它。
  (iv) 独立复核（fresh reviewer，本仓库外的一次性探针，直接对真实 crate 跑）：
       `resolve` before any stop → `Failed { stage: "device" }`；
       stop recorded before first resolve → `Stopped`；
       `wait()` after late stop → `Stopped`。同一份证据、不同取值。
       并注意 apps/headless/src/main.rs:130–139 把 Stopped 映射成
       ExitCode::SUCCESS ⇒ 一次设备 abort 可能被报成"成功停止"。
       （该探针不是本套件的一部分，仅作为独立复核记录。）

  因此本节的结论强度如实限定为：**模型证明"存在一个未提交窗口"，代码与独立
  复核共同说明"窗口里的晚到 stop 会改变最终分类"**；模型不承担后半句。

是否算缺陷取决于"terminal semantic resolution 发生在何时"这条尚未裁定的语义，
所以本轮只记录，不 patch、不改 ADR。
```

---

## FRESH ADVERSARIAL REVIEW

（见本文件末尾 "ADVERSARIAL REVIEW RECORD" 一节；该节记录独立 reviewer 的攻击
结果、MAJOR/MINOR 分类与处理。）

---

## FORMAL VERDICT

```text
CURRENT_CONTRACT_UNDERSPECIFIED
```

（fresh reviewer 对"中心断言"的独立判定是 **PARTIALLY SUPPORTED**：
模型内穷举成立、代码层面窄义成立，但**不构成"架构缺陷"的证明**——
它依赖一条代码审计前提与一处模型 guard 的位置。本报告的措辞已按此降级，
verdict 本身不变（未定契约，而非"A 违规"）。）

理由与排除项：

```text
A_VALID_BOUNDARY？        不选。A 满足 D11 的全部**安全**命题，但它的"事实存在性
                          挂在外部调用者身上"是结构性的（无 consumer ⇒ 提交不可达）。
                          把它判为"有效边界"等于默认 D11 允许外部触发；D11 没有
                          这么说，也没有说相反的话。
B_STRONGER_BOUNDARY？     不选为唯一结论。B 确实更强（同样的安全命题 + 不依赖
                          外部参与者），但它强在"进度假设归属"，不在"安全命题"；
                          按题目要求单选出结论，应表述为"契约未定"，而 B 是
                          一旦契约要求触发所有权时的对应形状。
BOTH_VALID_UNDER_DIFFERENT_CONTRACTS？
                          这是最接近的替代读法，与 CURRENT_CONTRACT_UNDERSPECIFIED
                          在观察上等价；差别只在于"ADR 是否有意留白"。
                          本轮的正式结论取 UNDESPECIFIED，因为它不预设 ADR 的意图。
NEED_MORE_MODELING？      不选。本轮问题（谁拥有 commit、外部 read/wait 是否创造
                          truth）已经被穷举模型回答；剩下的自由度属于语义裁决，
                          不是建模缺口。
```

## PLAIN-LANGUAGE VERDICT

我们用最小模型把两种做法摆在一起跑完了全部状态空间，结论可以压成四句话。

第一句：**两种做法在"安全"上没有区别。** 至多一个终局、一旦写下就不能改、
不会凭空出现 Completed 或 Stopped、激活失败不等于播放失败、观察和等待都不写事实
——这些 A、B、B′ 都通过。所以这不是"今天的代码有 bug"的问题。

第二句：**区别在"事实什么时候存在"，以及这个存在性挂在谁身上。**
今天的做法里，事实是被"问"出来的：没人调用 `wait()`，`Completed` 就不存在——
模型给出了五步反例，最后一步的状态是两个 leg 都 join 完、teardown 完成、而事实缺席。
更硬的一条是：把外部消费者整个拿掉，提交在今天的形状下**根本不可达**（穷举证明）。

第三句：**B 把这件事收回到系统自己身上，代价是一条关于自己的进度假设。**
没有任何做法能免费获得进度：把 fairness 去掉，A 和 B 的"证据齐了最终会提交"
都会失败（我们专门跑了这个反向控制）。区别只在假设落在谁身上——A 假设
"外面的人会一直来问"（关于环境），B 假设"我们自己的提交动作会被调度到"
（关于系统自己）。ADR 不可能承诺环境的行为，所以这两种假设不是同一类承诺。
另外 B′ 更进一步：把提交和"最后一块决定性证据"绑在同一步，于是
"证据齐了 ⇒ 事实存在"从一句进度承诺变成一条不变式，连 fairness 都不需要。

第四句：**今天还无法判 A 违规，因为 D11 只写了"谁决定"，没写"谁触发"。**
D11 说 terminal outcome 的 authority 是 Playback Session 语义角色，也说
Stopped 的口径是"at terminal semantic resolution"——但它对"resolution 发生在
哪个瞬间"保持沉默。于是三种行为都读得通：外部调用者触发（A）、authority 自己
触发（B）、证据落地即触发（B′）。本轮的形式化结论因此是 **契约未定**
（CURRENT_CONTRACT_UNDERSPECIFIED），而不是"谁违反了 ADR"。

这不是文字游戏，它有一个具体后果：在 A 里，**判决瞬间由调用者选择**。
如果设备真的失败了，但用户在任何人 resolve 之前按了 stop，同一件事会被写成
`Stopped` 而不是 `Failed`——D11 的 non-causal 口径允许这个结果。今天 headless
里这个窗口只有几十毫秒（因为主线程每 20ms resolve 一次），但 F2 要的
"纯读 status"形态下，如果驱动层不调用 `wait()`，这个窗口就是无界的。
模型已经把"窗口存在"和"窗口不存在（B′）"两边都取证了。是否要收紧、按哪个口径收紧，
是下一轮 ADR-vs-code 审计要回答的问题，本轮只交事实，不交补丁。

## NEXT SINGLE STEP

```text
QIANQIAN-F2-TERMINAL-COMMIT-ADR-CODE-GAP-AUDIT-1
```

输入：本报告的 Q-a…Q-e（ADR 侧问题）、M-a…M-d（代码侧问题）、
"判决瞬间窗口"一节，以及 `specs/f2-terminal-commit-boundary/` 的全部证据。
范围：只做 ADR 文本与 production reality 的差分审计与裁决，不实现 F2、
不改 production code、不改 ADR（除非裁决明确要求，并走 authority review）。

---

## ADVERSARIAL REVIEW RECORD

独立 fresh-context reviewer（无本对话上下文）自行复跑了全部套件
（24/24 当初全绿，exit 0），并在 /var/tmp 的副本上做了额外实验（仓库文件未被其修改）。
它对 10 个指定问题 + 1 个自加问题都给了 SOUND / WEAK / BROKEN 判定。

### 总体判定（reviewer 原文口径）

> **PARTIALLY SUPPORTED.** 在模型内，"terminal Fact 的存在性在 A 中需要 consumer
> 动作、在 B/B′ 中不需要"是穷举成立的（P1 在 A 中不可达；在 B/B′ 中无需任何
> consumer 动作即可达）。作为对 current Rust 的陈述，它在"state.outcome 只被
> wait()/try_resolve_now() 写过"这个窄意义上也成立。但它**没有被确立为架构缺陷**，
> 而且最强形式的断言依赖两件 TLC 没发现的东西：一处作者写下的 guard 位置，
> 以及把 App（ADR 定义、代码文档化为 episode driver 的角色）当作"外部环境"。

### MAJOR（全部已处理；处理后才允许本报告作为有效版本）

```text
MAJOR-1  Scenario* 不变式被当作 D11 命题呈现，实际是 current resolver precedence 的转录。
         证据（reviewer 在副本上实测）：去掉 stop 区分器 ⇒ ScenarioUserStop 被违反，
         而全部 ownership / boundary 结论不受影响；M6 击穿其中 2 条；四条都没有负控制。
         处理：
           (a) 模块与 README/report 明确标注该块是 **current-realization decision table
               转录**，并指出模型断言的是 D11 的反向（更强）；
           (b) 新增 M7 StopDiscriminatorRemoved（击穿 ScenarioUserStop）
               与 M9 FailureDowngraded（击穿 ScenarioDecodeFailureNeverStopped
               + NoFalseStopped）；
           (c) M6 的 cfg 现在同时列出它会附带击穿的两条 Scenario；
           (d) 明确声明 EvidenceConsistent 与 ScenarioNaturalEof 没有负控制，
               不把它们当作"有约束力"的证据引用。

MAJOR-2  A 的旗舰不变式 BoundaryCommitRequiresConsumer 是构造性的、且被本套件自己的 M5 顺带违反；
         报告把它当"最硬证据"是过度归因；真正的证据是 resolve() 调用者的代码审计。
         处理：B1 一节改为"代码前提（谁调用 resolve）+ TLC 后果（无 consumer 时提交不可达）"
         的证据链；不可达性结果（NoConsumerCommitImpossible）升为主证据；
         BoundaryCommitRequiresConsumer 降级为 writer 集合的记账性质并写明这一降级。

MAJOR-3  判决瞬间窗口一节的"模型侧证据"引用不支撑"翻转"断言：
         PendingDeviceAbortWindowWitness 只证明**未提交窗口**存在；
         翻转需要 stop/abort 时序，模型没有 chronology 变量；
         而 ScenarioUserStop 恰好把翻转后的取值声明为正确。
         处理：该节改写为四段式——(i) 模型证明窗口存在（B′ 中不存在）；
         (ii) 模型证明判决输入里**没有 cause 维度**（这是对 production 的忠实转录，
         edge 的 Stopped 不携带死因、resolve 读调用瞬间的 flag）；
         (iii) 明确声明"翻转本身不在机器检查范围内"，它是代码层结论；
         (iv) 记录 reviewer 在真实 crate 上做的一次性独立探针结果
         （Failed{device} vs Stopped，且 Stopped 被映射成 ExitCode::SUCCESS），
         并标注它不是本套件的一部分。
```

### MINOR（全部已处理）

```text
- witness 计数错误（README/specs-README 写 ×11，实际 ×9）→ 已改为 ×9，
  并写明总数 27 条 run 的构成。
- report 关于 M6 的"Scenario* 一并被击穿"过宽 → 已改为明确的两条 + 说明另两条不受影响。
- CE-A2 头版 trace 跳过 Activate（模型允许从"既未激活也未失败"的 BeforeActivation
  进入 teardown，这在 production 里不存在）→ 已收紧 BeginTeardown 守卫为
  "Active ∨ (BeforeActivation ∧ activationFailed)"，并更新为含 Activate 的忠实 trace。
- "窗口只有几十毫秒"不准确 → 20 ms 是轮询上界，通常由 worker_exited 的 notify_all
  唤醒（微秒级），且 drain verdict 自己的 condvar 不会叫醒 completion 等待者 → 已改。
- `-continue` 的 violated 列表不是穷尽清单 → 已在报告新增"关于运行结果的读法"一节。
- environment / system 的 fairness 二分是作者的标注选择，TLA+ 无此区分
  → 已在 LIVENESS/FAIRNESS 一节明确降级为"代码审计结论被编码成 guard 的结果"。
- S2 检验的是尚未实现的设计假设（production 还没有纯读 outcome 的 seam）
  → 已在 B4 一节如实声明。
```

### reviewer 明确判为 SOUND 的部分（保留供读者判断覆盖面）

```text
- 不夸大：没有任何文件把 A 判成"错的"；frozen verdict 是 CURRENT_CONTRACT_UNDERSPECIFIED。
- Q9 vacuity：三变体重跑 PASS（209/262/162 distinct）、M3/M4 liveness 反例、
  NoConsumerCommitImpossible PASS（176/78）全部复现；reviewer 另外自建探针验证
  各 Scenario 前提可达、Decisive/TeardownDone 可达 ⇒ liveness 非空洞；
  在声明该不变式的变体里无法证伪任一条。
- Q6：activationFailed 属于 D11 firewall 的范围，且 S4 在 production 里由结构保证
  （worker 最后 spawn，session.rs:104–141），M5 证明它可被证伪——没有虚假安全网。
  （附带 caveat：模型把"激活期间就可能发布的 render 侧证据"折叠进了粗粒度的 Activate。）
```

### 处理后的复核

三处 MAJOR 的处理都落在模型/配置/文档层，改动后可复跑（27 条 run 全绿）。
其中 (a) BeginTeardown 守卫收紧与 (b) 新增 M7/M8/M9 都改变了模型文本，
因此本报告描述的树是**处理之后的树**（见 HEAD_SHA）。
