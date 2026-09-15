# QIANQIAN-D11-TERMINAL-SETTLEMENT-CONFORMANCE-1 — report

## 先说人话

1. **现在"谁负责把播放结束变成事实"？** Playback Session 的语义角色（对一个
   播放 episode 而言）是唯一的 terminal 权威。机制证据（worker 退出、drain
   结果）只是原料；证据足够判决之后，**系统自己**负责把 Completed/Stopped/
   Failed 落成 Fact——不需要任何人来问。今天生产里这一步还要靠 consumer 调用
   `wait()` 顺带触发，那是 D11 已记录的 known differential，F2 修正目标。

2. **observe() 做什么、不做什么？** 只读。它不能 resolve、不能 commit、不能改
   证据、不能改 stop intent、不能改生命周期。模型里 `Observe` 动作只写一个
   verifier-only 见证位；mutation M1 证明"observe 顺手提交"会被 transition
   级 writer 性质（S3）当场抓住——即使提交的值完全正确。

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

8. **模型里的判决函数凭什么跟着生产代码走？** CORRECTIVE-2 起，不再凭
   人工逐支核对：production `resolve()` 的判决合同被 Rust exhaustive
   oracle（48 元组、真实公开 seam）冻结成单一真值表 artifact
   `CurrentDecisionTable.tla`，TLC run 逐行校验这张表与
   `CurrentDecisionDecisive`/`CurrentDecisionVerdict` 一致。改
   `completion.rs` 判决分支、手改表、改 TLA 判决函数，三者任何漂移都会
   击穿对应 CI gate——W7/M10 证明"formal ground truth 内部承重"，这张表
   机器维护"ground truth ↔ production 相等"。

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
    S3:  AuthorityIsSoleWriter（transition 级：terminalOutcome 的任何改变
         必须就是 AuthoritySettle；[][A]_vars 形式，TLC 以 Action property
         检查、按名报告）—— PASS（M1/M2/M9 击穿验证；M9 为全保真冒写，
         状态不变式全绿仍被按名抓住）
    S4:  Observe 纯读 —— Observe 结构上只写 observeRan；M1 证明偏离即被抓
    S5:  Wait 纯等待 —— Wait 结构上只写 waitRan；M2 独立 run 证明偏离即被抓
    S6:  NoFalseCompleted —— PASS（M7 击穿验证）
    S7:  NoFalseStopped（只用 stopAtDecision）—— PASS（M4/M8 击穿验证）
    S8:  ActivationFailureIsNotTerminalFailed —— PASS（M6 击穿验证）
    S9:  CommittedOutcomeMatchesContract（单向；触发域=全形状
         CurrentDecisionDecisive，提交值=边界 intent 下的 current 判决）
         —— PASS（M3/M4 击穿验证）
    S10: TeardownRequiresSettlement（条件形式；antecedent 用全形状触发域）
         —— PASS（M5/M10 击穿验证）
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
    terminal Fact writer: 仅 AuthoritySettle —— 由 transition 级性质 S3
                 （AuthorityIsSoleWriter）承载；初版的 authoritySettled
                 ghost 布尔已退役（可被顺手维护 ghost 的冒写动作骗过）
    mechanism evidence writers: 6 个 Publish* 动作（只写证据与 ghost）
    forbidden writers: Observe（M1）、Wait（M2）、evidence 发布者全保真
                 冒写（M9：值/边界 intent/firstCommitted 全部如实维护，
                 状态不变式全绿，仅 S3 抓住）、late stop 改写已提交值（M3）
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
    W7: Eof+Aborted 全程无 stop → Failed(device) 提交（触发域收回的
                 production-decisive 分支）—— 可达

MUTATIONS:
    M1 ObserveCommits:                击穿 AuthorityIsSoleWriter（纯 temporal
                                      反例；状态不变式全绿）✅
    M2 WaitCommits:                   击穿 AuthorityIsSoleWriter（同上）✅
    M3 TerminalRewritable:            击穿 TerminalOutcomeImmutable ✅
    M4 LateStopReadsCurrentIntent:    击穿 CommittedOutcomeMatchesContract ✅
    M5 TeardownBeforeSettlement:      击穿 TeardownRequiresSettlement ✅
    M6 ActivationFailureBecomesFailed: 击穿 ActivationFailureIsNotTerminalFailed ✅
    M7 FalseCompleted:                击穿 NoFalseCompleted ✅
    M8 FalseStopped:                  击穿 NoFalseStopped ✅
    M9 EvidenceProducerSpoofsAuthority: 击穿 AuthorityIsSoleWriter（全保真
                                      冒写；零状态不变式违反）✅
    M10 NarrowDecisiveDomain:         击穿 TeardownRequiresSettlement
                                      （机制门缩回 minimal 域、性质不动）✅

OVERCLAIM CONTROLS:
    EveryEpisodeEventuallyTerminates / EveryActiveEpisodeEventuallyCompletes /
    EveryStopEventuallyStops / EveryDecoderEventuallyExits /
    EveryDeviceEventuallyDrains —— 全部 COUNTEREXAMPLE FOUND（SpecSettlement-
    Fairness 下，最强让步）；NoFairnessProgressFails —— SettlementProgress
    被违反（fairness 承重确认）

FRESH REVIEW（round 1，针对初版模型——历史记录）:
    verdict: PASS_WITH_MINOR（fresh adversarial reviewer，独立以 pinned jar
             重跑全部 23 条 run 并用 scratch 模块探查 None-region）
    说明: 本轮 review 未发现下述两个 MAJOR（触发域收窄、ghost writer
          证明），它们由随后的人工 review 发现——见 CORRECTIVE-1。
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

CORRECTIVE-1（人工 review 对上一轮的 2 MAJOR + 1 CI BLOCKER，全部修正）：

    review 判定（对初版 HEAD）：REQUEST_CHANGES，
    D11_CONFORMANCE_PASS 不成立 → D11_CONFORMANCE_NEEDS_CORRECTIVE。

    MAJOR-1（None-region 收窄了 settlement obligation 的触发域）：
        初版模型的 TerminalCandidateOf 把 production 果断的
        Eof+Aborted → Failed(device) 留在触发域（Decisive）之外，
        于是存在这样的可达执行：current decision contract 已能判决
        （decisive=TRUE, verdict=Failed）、teardown 已完成、outcome
        未提交——而 C5 不变式因 antecedent 用同一个被收窄的 Decisive
        而不响。S9 可以收窄（已收窄），但 C5/progress/latch 不可以
        跟着收窄：settlement obligation 的 decisive domain 不得被
        under-approximate。
        修正：判决概念拆成两层——
          CurrentDecisionDecisive（触发域，normative 层）：覆盖 current
            decision contract 能判决的全部形状（逐支对应 production
            resolve() 分支），latch/C5/progress 全部换用它；
          CurrentDecisionVerdict（判决值，realization conformance
            oracle）：精确 precedence 是 current realization，演进时
            随 authority 变更重推导；D11 三条外部命题由独立不变式
            （S6/S7）承载，不从判决表导出。
        新增正向 witness W7（Eof+Aborted 全程无 stop → Failed(device)
        提交可达）与负控制 M10（把机制门缩回 minimal 域、性质不动，
        C5 必须被违反——证明触发域覆盖是真实约束而非空洞成立）。

    MAJOR-2（writer-set theorem 证明的是 ghost 被置位，不是 writer
    identity）：
        初版 S3 = "outcome 非空 ⇒ authoritySettled ghost 为 TRUE"，
        M9 之所以被杀只因 mutation 忘记登记 ghost——mutation 预先配合
        了 oracle。"foreign writer 本身会被抓"并没有被证明。
        修正：退役 authoritySettled 布尔；S3 改为 transition 级性质
          OutcomeChangedOnlyByAuthority ==
              (terminalOutcome' # terminalOutcome) => AuthoritySettle
        （[][A]_vars 形式，TLC 以 Action property 检查、按名报告）。
        M9 重构为最大对手 EvidenceProducerSpoofsAuthority：值按边界
        intent 正确计算、firstCommitted 如实维护——全部状态不变式保持
        绿色，唯一能抓住它的是 S3（runner 新增 tfail 模式验证
        "temporal 按名违反 + 零状态不变式违反"）。M1/M2 同步升级为
        同类验证。

    CI BLOCKER（Formal Semantic Gate 红）：
        根因：本 suite 的 check.sh 在 git 中无可执行位（100644），
        CI 报 Permission denied（本地 shell 有 +x 掩盖了它）。已以
        git update-index --chmod=+x 修正。

    corrective rerun: 全套 25 条 TLC run（2 base + 10 mutation +
        5 overclaim + 1 fairness 承重 + 7 witness）—— 25/25 通过；
        聚合 specs/check.sh current（K0 + realtime-publication +
        本 suite）exit 0。状态空间 616 → 652 distinct states
        （触发域扩大所致）。

    corrective-1 fresh review（1582a93 后，实际结果，非预写）：
        verdict: CHANGES_REQUIRED —— 1 MAJOR（文档完整性）+ 2 MINOR；
                 全部由紧随的 commit 修正。模型/cfg/runner 本体被 reviewer
                 独立复核确认 sound：
                 - decisive 域对照 completion.rs resolve() 双向逐支核对
                   精确一致（含"worker 退出只可能报告
                   {Eof, Stopped, Failed}"的 reality 检查，session.rs
                   逐返回路径核过）；唯一偏差 = 已文档化的边界 intent
                   读取（D11 late-command rule）。
                 - M9 全保真冒写在全部 652 states 上零状态不变式违反、
                   Action property 按名违反（非单轨迹，穷举证明）。
                 - M10 非循环：C5 的 antecedent 不使用被变异的门。
                 - fairness 论证无死角：decisive ∧ 未提交 ⇒ teardown 被
                   阻塞、AuthoritySettle 持续使能，WF 承载进度成立。
                 - 25/25 run 与聚合 specs/check.sh current exit 0 由
                   reviewer 独立复现；scope 检查（diff 只触 specs/）
                   通过。
                 MAJOR（已修）：report.md 本文件多处 ledger 仍描述修正前
                 模型（S3 旧名 / authoritySettled / M9 旧名 / 缺 M10 与
                 W7 行），并且预写了尚未发生的 review 结论——违反
                 "Report what was actually verified"。已全部改为当前
                 现实；本节即该 review 的真实产物。
                 MINOR-1（已修）：base fairness run 的 PROPERTY 实际只含
                 SettlementProgress（当初一次编辑失败未重试），README
                 表格却写成双性质。已把 AuthorityIsSoleWriter 补进 base
                 cfg（修复后全量重跑），README 如实描述两次 base run
                 各自的 PROPERTY 集。
                 MINOR-2（已修）：README"已知粗粒度处"补第 8 条——模型
                 允许 (decodeFailure, worker=None, drain=Drained) 这一
                 production 不可达组合（保守超近似；两侧同判 Failed）。

CORRECTIVE-2（人工 review 对 CORRECTIVE-1 的复核——原判修复确认 + 1 新
MAJOR，已修正）：

    review 判定（对 CORRECTIVE-1 HEAD 136f616）：CHANGES_REQUIRED ——
        原 MAJOR-1（decisive 触发域 under-approximation）确认 FIXED；
        原 MAJOR-2（可伪造 ghost writer 证明）确认 FIXED；
        CI BLOCKER（可执行位）确认 FIXED；
        corrective-1 fresh findings（stale ledger / base PROPERTY 漏接 /
        保守超近似未声明）确认 FIXED。
        NEW MAJOR：current decision contract（completion.rs::resolve）与
        TLA CurrentDecisionDecisive 是两个 truth source，只有人工逐支
        核对的快照相等，缺 durable binding——Rust gate 不比对判决域、
        Formal gate 不触发于 completion.rs，判决分支增删后所有 formal
        tests 可保持绿色而 ground truth 已过期（W7+M10 证明的是"给定
        CurrentDecision* 这份 formal ground truth，C5 对完整域承重"，
        不能机器证明这份 ground truth 永远等于 production contract）。

    修正（production 源码零改动，纯 verification 侧）：
        1. 共享真值表 artifact specs/episode-terminal-settlement/
           CurrentDecisionTable.tla（生成文件，48 元组穷举：
           stop_intent × decode_failure × worker_terminal × drain_verdict
           → undecided/completed/stopped/failed-decode/failed-device；
           failed-* 细分保留 production stage 命名，TLC 比较时投影为
           "Failed"）。
        2. Rust exhaustive oracle
           crates/qianqian-playback/tests/completion_decision_table.rs：
           每个 tuple 一个 fresh SessionCompletion，经公开 seam
           （decode_failed/worker_exited/drain_signal().complete/
           request_stop）发布证据后 try_resolve_now 一次；渲染完整表
           文件并 byte-compare（QIANQIAN_UPDATE_DECISION_TABLE=1 再生成）。
           Verification Rust Gate 经 playback native regression 执行。
        3. TLC refinement run（EpisodeTerminalSettlementTable.tla/.cfg）：
           DecisionDomain 全部 48 元组为初始状态（穷举枚举器），
           TableDecisiveMatchesContract（decisive 域逐行一致）+
           TableVerdictMatchesContract（判决值逐行一致）+
           TableRowsWellFormed（行数=域大小 ∧ 每行形状合法 ∧ **每个域
           key 恰好一行**——显式 bijection，"48 行穷举、key 无缺无重"
           是模块自身的 theorem，不依赖无解 CHOOSE 的运行时行为兜底；
           review follow-up MINOR 采纳）。Formal Semantic Gate 执行。
        4. trigger 闭环：crates/qianqian-playback/src/completion.rs 加入
           formal-semantic-gate；CurrentDecisionTable.tla 加入
           verification-rust-gate（手改表也会重跑 Rust oracle）。
        5. 主模型 EpisodeTerminalSettlement.tla 语义零改动（表模块仅
           EXTENDS 复用 CurrentDecisionDecisive/CurrentDecisionVerdict）。

    oracle 负控制（漂移路径逐条实际注入验证，非声称）：
        - TLA 侧漂移：删 CurrentDecisionDecisive 的
          (Aborted ∧ wt≠None) 支 → TableDecisiveMatchesContract
          立即违反（Eof+Aborted 行）。
        - 表侧漂移（投影可见）：行值 failed-device→completed →
          TableVerdictMatchesContract 违反。
        - 表侧漂移（TLC 投影不可见）：failed-device→failed-decode
          （两者都投影为 "Failed"）→ TLC 不响，Rust byte-compare
          FAILED（这正是 artifact 单侧绑定不够、双侧绑定必要的原因）。
        - bijection 破坏（review follow-up 场景）：key A 两行不同
          class + key B 缺行、总行数仍 48 → TableRowsWellFormed
          直接 FALSE（显式 invariant 击穿，非 CHOOSE 运行时错误）。

    corrective-2 follow-up（第三轮 review，对 93e9e2a）：
        verdict: 不挡 merge，1 MINOR（oracle hygiene）——
        "每个域 key 恰好一行"的 bijection 未写成显式 invariant，
        完整性依赖 RowFor 无解 CHOOSE 的执行行为。已采纳收紧：
        TableRowsWellFormed 增加 Cardinality-per-key = 1 子句
        （负控制场景实跑验证，见上）。全套 26 条 TLC run 重跑通过。

    corrective rerun: 全套 26 条 TLC run（2 base + 1 table refinement +
        10 mutation + 5 overclaim + 1 fairness 承重 + 7 witness）——
        26/26 通过（table run：48 初始状态穷举，96 states generated）；
        cargo test -p qianqian-playback 全绿（含 oracle test）。

    scope 检查：diff 不触 crates/qianqian-playback/src/**（生产源码
        零改动）、不触 docs/adr/**（authority 零改动）；只新增
        verification artifact/test/workflow trigger 与文档措辞。

CURRENT FORMAL GATE:
    promoted? YES（本 PR 内完成注册；CORRECTIVE-2 后维持）
    reason: 全部 accepted D11 properties PASS（safety 与 fairness 解耦双跑，
            含 S3 transition 级 writer 性质）；10 mutation 全 killed
            （writer 类为纯 temporal 反例：状态不变式全绿）；5 overclaim +
            fairness 承重控制全部按预期反例；触发域全覆盖由 W7（正）与
            M10（负）证明——**在其给定 formal ground truth 的意义上**：
            W7/M10 证明 C5 等性质对 CurrentDecisionDecisive 的完整域真实
            承重，而该 ground truth 与 production 判决合同的相等性由
            refinement oracle 机器维护（CORRECTIVE-2，48 元组穷举双侧
            绑定，三条漂移路径负控制全部有牙齿），不依赖人工逐支核对；
            corrective-1/2 review 的实际结果与处置见上两节。已接入
            specs/check.sh current 与 CI formal-semantic-gate 触发路径
            （含 completion.rs refinement seam）；f2-terminal-commit-boundary
            标注为 HISTORICAL/EXPLORATORY。最终接受仍等待本 PR 的人工
            review（READY_FOR_HUMAN_FORMAL_CONFORMANCE_REVIEW）。

PRODUCTION CODE:
    CHANGED? NO（CORRECTIVE-2 亦然：oracle 测试与 artifact 属
          verification 侧；crates/qianqian-playback/src/** 零改动）

ADR:
    CHANGED? NO

F2 REPRESENTATION:
    CHOSEN? NO

FORMAL VERDICT:
    D11_CONFORMANCE_PASS（CORRECTIVE-2 后）
    （当前已接受的 D11 terminal-settlement contract 可被极小、非空洞的
     TLA+ 模型一致表达；10/10 错误变体被机器抓住——含触发域收窄与
     全保真冒写两个初版漏掉的最大对手；无一般 liveness 偷渡；production
     ↔ formal 判决合同由 refinement oracle 双侧机器绑定（48 元组穷举，
     漂移即 gate 红）；未发现 merged D11 自相矛盾或无法一致建模之处 →
     无 AUTHORITY DEFECT）

NEXT SINGLE STEP:
    QIANQIAN-F2-READ-SIDE-SEAM-REALITY-GATE-2

STOP.
DO NOT IMPLEMENT F2.
DO NOT CHOOSE F2 REPRESENTATION.
```

## 结果词汇

- 正常模型：`BOUNDED-CLEAN`（652 distinct states 穷举；safety 无 fairness
  双跑 PASS；条件进度在显式 WF 下 PASS）
- refinement oracle：`EXHAUSTIVE-CLEAN`（48 元组穷举双绑定：Rust oracle
  byte-compare PASS + TLC 逐行比对 PASS；三条漂移路径负控制
  `COUNTEREXAMPLE-WITNESSED` / Rust `FAILED`）
- 10 mutation + 5 overclaim + 1 fairness 承重：`COUNTEREXAMPLE-WITNESSED`
- 7 witness：可达性确认（asserted-unreachable 不变式被违反）
- 按 specs/README 结果声明边界：以上只在所述模型、界、假设与 fairness
  条件内成立，不构成 "architecture proven correct"。
