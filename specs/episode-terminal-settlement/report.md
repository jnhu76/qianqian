# QIANQIAN-D11-TERMINAL-SETTLEMENT-CONFORMANCE-1 — report

## 先说人话

1. **现在"谁负责把播放结束变成事实"？** Playback Session 的语义角色（对一个
   播放 episode 而言）是唯一的 terminal 权威。机制证据（worker 退出、drain
   结果）只是原料；证据足够判决之后，**系统自己**负责把 Completed/Stopped/
   Failed 落成 Fact——不需要任何人来问。今天生产里这一步还要靠 consumer 调用
   `wait()` 顺带触发，那是 D11 已记录的 known differential，F2 修正目标。

2. **observe() 做什么、不做什么？** 只读。它不能 resolve、不能 commit、不能改
   证据、不能改 stop intent、不能改生命周期。模型里 `Observe` 动作只写一个
   verifier-only 见证位；mutation M1 证明"observe 顺手提交"会被 writer 集合
   不变式当场抓住。

3. **wait() 做什么、不做什么？** 只等 authority 已经落下的 Fact。它不跑
   resolver、不是 terminal writer、也不是 Fact 出现的前提。模型里 `Wait` 只写
   一个见证位；mutation M2（形状正是今天生产 `wait()->resolve()` 的样子）被
   当场抓住。W6 进一步证明：Observe 和 Wait 从未发生，terminal Fact 照样建立。

4. **用户在设备已经失败后才按 stop，为什么不能再算成 Stopped？** 因为
   分类在"证据首次足够判决"的那一刻就由**当时已记录的 stop intent** 决定了。
   设备失败决定性时没人按过 stop，判决就是 Failed；之后晚到的 stop 只是命令
   历史，不能追溯改写。模型用 ghost 变量 `stopAtDecision` 在边界瞬间记下
   "当时有没有 stop 意图"，mutation M4（settlement 偷读当前 stopSeen）被击穿，
   正向 witness W4 证明同一调度下正确行为（保持 Failed）可达。

5. **teardown 为什么不能跑在 terminal Fact 前面？** 如果证据已经足够判决、
   而 Fact 还没落下来，teardown 完成就意味着"系统假装这件事没有结论地散场了"。
   D11 冻结的是这条 safety 边界：decisive 未提交 ⇒ teardown 不得完成。
   mutation M5 证明去掉这条边界立刻产生反例。这不是"每首歌必须 teardown"的
   进度承诺。

6. **这次有没有证明"每首歌一定会结束"？** **没有。** 恰恰相反：5 条 overclaim
   反向控制（在 settlement 公平调度的最强让步下）全部被反例击穿——证据可以
   永远不来、stop 可以被 failure 压过、decoder/device 可以永不退出。模型只承诺
   "证据足够之后、authority 获得执行机会时，Fact 最终被提交"这一条带前提的
   条件进度（且 fairness 承重控制证明该前提是真的承重的）。

7. **这次有没有决定 PlaybackSessionHandle 长什么样？** **没有。** 模型零生产
   代码、零 ADR 改动、零 F2 representation 选择。`stopAtDecision` 是
   verifier-only ghost，明确不要求任何 Rust 表示（production 可以在边界瞬间
   立即 settlement，从而根本不需要保留它）。F2 的 Rust representation 仍由
   F2-READ-SIDE-SEAM-REALITY-GATE 决定。

---

```text
QIANQIAN-D11-TERMINAL-SETTLEMENT-CONFORMANCE-1

BASE_SHA:  129f31161b83a00a06b80cc6ddf992c845661ade
HEAD_SHA:  <见 PR（branch formal/d11-terminal-settlement-conformance-1）>
BRANCH:    formal/d11-terminal-settlement-conformance-1
PR:        <本 PR>
WORKTREE:  /home/hoo/Source/qianqian

AUTHORITY:
    D11 version: ADR-PBK-002 §17（2026-09-15 settlement corrective 后的
                 current 文本；decision + commit ownership 归 Playback
                 Session semantic authority，late-command stability 冻结）
    D14 relation: D14.2 定义 observe/wait/record-stop 的 seam 语义合同与
                  truth class；D14.3 冻结 authority-owned structured
                  settlement 实现目标；representation 明示不在本轮冻结
                  （F2-READ-SIDE-SEAM-REALITY-GATE 仍 OPEN）

MODEL_SCOPE:
    one episode terminal settlement（单 episode、单次 teardown；
    不建模 K0/Capability/PCM/WASAPI/seek/pause/open/Generation/Window/
    PlaybackSessionHandle/UI/线程/内存序/物理可听性）

PLAIN LANGUAGE:
    见上文「先说人话」1–7。

NORMATIVE PROPERTIES:
    C1: 单个 terminal Fact：None → 恰一个终局、不可改写
        → TerminalOutcomeImmutable（S2）
    C2: authority-owned settlement：Fact 的出现不依赖任何外部消费者
        → writer 集合 S3 + Witness W6（无 consumer 提交可达）
    C3: Observe 纯读 → Observe 动作只写见证位；M1 击穿
    C4: Wait 纯等待 → Wait 动作只写见证位；M2 击穿（= 今日生产
        wait()->resolve() differential 的形状）
    C5: teardown settlement 边界（条件 safety）
        → TeardownRequiresSettlement（S10）；M5 击穿
    C6: late-command stability → stopAtDecision ghost +
        CommittedOutcomeMatchesContract（S9）+ NoFalseStopped（S7）；
        M4/M8 击穿；W4/W5 正向 witness
    C7: activation failure firewall → ActivationFailureIsNotTerminalFailed
        （S8）；M6 击穿
    C8: 无虚构 Completed → NoFalseCompleted（S6）；M7 击穿
    C9: 无虚构 Stopped（边界 intent，非 stopRequestedNow）
        → NoFalseStopped（S7，只读 stopAtDecision）；M8 击穿

SAFETY:
    S1:  TypeOK —— PASS
    S2:  TerminalOutcomeImmutable —— PASS（M3 击穿验证非空洞）
    S3:  OutcomeWrittenOnlyByAuthoritySettle —— PASS（M1/M2/M9 击穿验证）
    S4:  Observe 纯读 —— Observe 结构上只写 observeRan；M1 证明偏离即被抓
    S5:  Wait 纯等待 —— Wait 结构上只写 waitRan；M2 独立 run 证明偏离即被抓
    S6:  NoFalseCompleted —— PASS（M7 击穿验证）
    S7:  NoFalseStopped（只用 stopAtDecision）—— PASS（M4/M8 击穿验证）
    S8:  ActivationFailureIsNotTerminalFailed —— PASS（M6 击穿验证）
    S9:  CommittedOutcomeMatchesContract（单向：Decisive ⇒ 提交值=边界判决）
         —— PASS（M3/M4 击穿验证）
    S10: TeardownRequiresSettlement（条件形式）—— PASS（M5 击穿验证）
    辅助：EvidenceConsistent（守卫自查，无 mutation 负控制，如实声明）、
         BoundaryLatchedWhenDecisive（模型内部一致性）、
         DiagnosticActivationFailureLeavesOutcomeUncommitted（命名诊断）

PROGRESS:
    assumptions: WF_vars(AuthoritySettle)（且仅在 SpecSettlementFairness
                 中显式出现；cfg 可见）
    properties:  SettlementProgress == Decisive ~> terminalOutcome # "None"
                 —— PASS
    general liveness intentionally NOT claimed（全部 COUNTEREXAMPLE FOUND，
                 且在带 WF 的最强让步下）:
                 EveryEpisodeEventuallyTerminates / 
                 EveryActiveEpisodeEventuallyCompletes /
                 EveryStopEventuallyStops / EveryDecoderEventuallyExits /
                 EveryDeviceEventuallyDrains
    fairness 承重控制：NoFairnessProgressFails（去 WF 后 SettlementProgress
                 被违反）—— PASS（即 fairness 确实承重）

LATE COMMAND:
    decisive boundary representation: 最后一块决定性 evidence 落地的同一步
                 latch（decisionLatched FALSE→TRUE 一次，stopAtDecision :=
                 当时的 stopSeen 并冻结）；evidence 动作只写 ghost，从不写
                 terminalOutcome
    ghost-state status: verifier-only formal ghost/history，README/模型头
                 注明非 production architecture state、不要求任何 Rust 表示
    late-stop witness: W4（决定性 Failed 后晚到 stop，settlement 仍 Failed）
                 与 W5（Completed 已提交 + 晚到 stop 共存，同 run
                 TerminalOutcomeImmutable PASS）—— 均可达
    late-stop mutation: M4（settlement 读当前 stopSeen → 提交 Stopped）
                 击穿 S9（附带 S7）；反例轨迹：Activate → worker Stopped →
                 drain Aborted（latch, ¬stopAtDecision）→ 晚到 RequestStop →
                 误读 settlement = Stopped ✗；M8（判决函数无条件判 Stopped）
                 击穿 S7

TEARDOWN:
    production mapping: session.rs 逆序 LIFO——stop edge + join worker
                 （join 返回 ⇒ worker_exited 必已执行）→ stop_and_join
                 stream（返回 ⇒ drain verdict 必已发布）；守卫 1（join
                 纪律）是生产现实；守卫 2（settlement 边界）是 accepted
                 D11-C5 要求（今日生产因 consumer-triggered differential
                 尚不满足，F2 修正）
    formal invariant: S10 条件形式 (TeardownDone ∧ ¬activationFailed ∧
                 Decisive) ⇒ terminalOutcome # "None"
    mutation: M5（去掉 settlement 守卫）击穿 S10

READ:
    observation: Observe 只写 observeRan（纯读）
    mutation: M1 ObserveCommits —— 击穿 S3

WAIT:
    wait: Wait 只写 waitRan（纯等待；TLA 不模拟 OS blocking，语义要求
                 只有"无 semantic writer 效果"）
    mutation: M2 WaitCommits —— 击穿 S3（独立于 M1 的单独 run）

WRITER SET:
    terminal Fact writer: 仅 AuthoritySettle（S3 + authoritySettled 见证位）
    mechanism evidence writers: 6 个 Publish* 动作（只写证据与 ghost）
    forbidden writers: Observe（M1）、Wait（M2）、evidence 发布者顺手提交
                 （M9）、late stop 改写已提交值（M3）
    边界说明：M9 不禁止 production 在同一 Rust call stack 内由 authority
                 完成 decision+commit；它禁止的是 mechanism provider 因
                 发布证据而自己成为另一个 authority

WITNESSES:
    W1: 自然 EOF → Completed —— 可达
    W2: 边界前用户 stop → Stopped —— 可达
    W3: 无 stop 的设备 abort → Failed —— 可达
    W4: 决定性 Failed 后晚到 stop，settlement 仍 Failed —— 可达
    W5: Completed 已提交 + 晚到 stop 共存 —— 可达
    W6: Observe/Wait 从未运行，terminal Fact 仍建立 —— 可达

MUTATIONS:
    M1 ObserveCommits:                击穿 OutcomeWrittenOnlyByAuthoritySettle ✅
    M2 WaitCommits:                   击穿 OutcomeWrittenOnlyByAuthoritySettle ✅
    M3 TerminalRewritable:            击穿 TerminalOutcomeImmutable ✅
    M4 LateStopReadsCurrentIntent:    击穿 CommittedOutcomeMatchesContract ✅
    M5 TeardownBeforeSettlement:      击穿 TeardownRequiresSettlement ✅
    M6 ActivationFailureBecomesFailed: 击穿 ActivationFailureIsNotTerminalFailed ✅
    M7 FalseCompleted:                击穿 NoFalseCompleted ✅
    M8 FalseStopped:                  击穿 NoFalseStopped ✅
    M9 EvidenceProducerCommits:       击穿 OutcomeWrittenOnlyByAuthoritySettle ✅

OVERCLAIM CONTROLS:
    EveryEpisodeEventuallyTerminates / EveryActiveEpisodeEventuallyCompletes /
    EveryStopEventuallyStops / EveryDecoderEventuallyExits /
    EveryDeviceEventuallyDrains —— 全部 COUNTEREXAMPLE FOUND（SpecSettlement-
    Fairness 下，最强让步）；NoFairnessProgressFails —— SettlementProgress
    被违反（fairness 承重确认）

FRESH REVIEW:
    verdict: PASS_WITH_MINOR（fresh adversarial reviewer，独立以 pinned jar
             重跑全部 23 条 run 并用 scratch 模块探查 None-region）
    MAJOR:   0
    MINOR:   5 —— 全部已修并重跑全套确认：
             1. S9 由双向形式收窄为单向蕴含（Decisive ⇒ …），None-region 不
                再被升格为 normative；M3/M4 在新形式下仍被击穿（重跑验证）。
                None-region/证据形状的 model-scoping 已在 README 范围声明。
             2. "README 缺失"——事实失效（README.md 在 reviewer 启动前已存在；
                reviewer 核查的是其 /var/tmp 运行副本）。已核实存在。
             3. FinishTeardown 注释把守卫 2（D11-C5 要求）误标为"生产现实"
                ——已改为明确区分：守卫 1=生产现实，守卫 2=accepted 要求
                （今日生产 differential，F2 修正）。
             4. W5 cfg 注释声称同 run 检查 TerminalOutcomeImmutable 但
                INVARIANT 列表没有它——已把 TerminalOutcomeImmutable 加进
                该 run 的 INVARIANT 列表（注释成真）。
             5. 模型注释含 PR 号与悬空 campaign 引用（违反 specs/README 命名
                规则）——已全部清除/改为 ADR 章节引用，grep 验证清零。
    corrective rerun: 全套 23 条 TLC run 重跑 —— 23/23 通过（与修前同结果）

CURRENT FORMAL GATE:
    promoted? YES（本 PR 内完成注册）
    reason: 全部 accepted D11 properties PASS（safety 与 fairness 解耦双跑）；
            9 mutation 全 killed；5 overclaim + fairness 承重控制全部按预期
            反例；fresh reviewer 0 MAJOR，5 MINOR 已修并重跑。已接入
            specs/check.sh current 与 CI formal-semantic-gate 触发路径；
            f2-terminal-commit-boundary 标注为 HISTORICAL/EXPLORATORY。
            最终接受仍等待本 PR 的人工 review（READY_FOR_HUMAN_FORMAL_
            CONFORMANCE_REVIEW）。

PRODUCTION CODE:
    CHANGED? NO

ADR:
    CHANGED? NO

F2 REPRESENTATION:
    CHOSEN? NO

FORMAL VERDICT:
    D11_CONFORMANCE_PASS
    （当前已接受的 D11 terminal-settlement contract 可被极小、非空洞的
     TLA+ 模型一致表达；9/9 错误变体被机器抓住；无一般 liveness 偷渡；
     未发现 merged D11 自相矛盾或无法一致建模之处 → 无 AUTHORITY DEFECT）

NEXT SINGLE STEP:
    QIANQIAN-F2-READ-SIDE-SEAM-REALITY-GATE-2

STOP.
DO NOT IMPLEMENT F2.
DO NOT CHOOSE F2 REPRESENTATION.
```

## 结果词汇

- 正常模型：`BOUNDED-CLEAN`（616 distinct states 穷举；safety 无 fairness
  双跑 PASS；条件进度在显式 WF 下 PASS）
- 9 mutation + 5 overclaim + 1 fairness 承重：`COUNTEREXAMPLE-WITNESSED`
- 6 witness：可达性确认（asserted-unreachable 不变式被违反）
- 按 specs/README 结果声明边界：以上只在所述模型、界、假设与 fairness
  条件内成立，不构成 "architecture proven correct"。
