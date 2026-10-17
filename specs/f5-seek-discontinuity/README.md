# f5-seek-discontinuity — F5 seek cutover 安全模型（含 pending-fatal 扩展）

```text
STATUS: CHECKED-IN-MODEL（current formal gate；F5-GATE 证据）
权威:   ADR-PBK-002 §20 D14.5（seek 语义 spine；F5-GATE amendment）+ D14.8
        seek 规则 + §17 D11（terminal 仅以最小受约束投影出现）
D-C:    #289 EXTEND_EXISTING_MODEL（receipt issues/289#issuecomment-6229911387，
        2026-10-17）；扩展构造依据 = PENDING_FATAL_DAG.md（BASE_SHA 实测行号）
攻击:   commit 之后旧 PCM 复活；位置发布混用 pre/post basis；
        pending-fatal × seek × stop 交错产生合法矩阵外的终局/续接
```

## 入口问题（ADR-PBK-001 §13）

> **seek cutover 的哪些独立合法事件（旧解码、旧 edge 队列、旧设备尾、park、
> landing、commit、stop、partial-success latch、provider 返回）能交错出
> 「commit 之后旧 PCM 仍成为 output」「位置混用 pre/post 证据」或
> 「pending-fatal 窗口内终局/续接落到 reconciliation §2.3 合法矩阵之外」？**

模型只建模 **discontinuity protocol**（D14.5 三库 + commit boundary + GAP-01
pending-fatal 窗口），不建模音频细节。11 条安全不变式：

```text
基线五条 + TypeOK：
InvStaleOutput       commit 之后旧 PCM 不得再被 audible drain（D14.5 根不变式）
InvPositionNoMixing  commit 之后不得按旧 basis 发布位置（D14.8 seek 规则）
InvCommitPurged      commit 蕴含 landing ∧ edge 干净 ∧ 尾排空（护栏自检）
InvRefusalContentContinuous  refusal 不得丢掉保留余量（零内容损失）
InvFailClosed        MutatedThenFailed 之后不得恢复旧游标生产

pending-fatal 五条（#289 D-C 扩展；锚点 reconciliation §2.3/§2.3.1）：
NoOldCursorResumeAfterApplied   Applied 后不得从旧 cursor 恢复生产（D-T1 靶）
RetainLatchOnRefused            Refused 不得清除已存在的 latch（D-T2 靶）
NoOldFatalAfterApplied          Applied 擦除后旧 fatal 不得残留/再 surface
                                （模型内抽象下声明，见下）（D-T4 靶）
ScopedLatchedFatalNoComplete    pre-Applied continuation 上存活的 latch 封死
                                旧 continuation 的 Completed；**不扩大**为
                                「曾 latch ⇒ 永不 Completed」——新 continuation
                                的合法 Completed 由 W_APPLIED_ERASURE_COMPLETED
                                强制可达（Q9 REV.2 的 scoped 化）
TerminalCellConformance         worker 退出路径 ∈ 冻结退出清单 ∧ committed
                                outcome 落在 reconciliation §2.3.1 的合法格
                                （D-T3 靶；TERMINAL_DECISION_MAP 转写）
```

## 两个 commit 的区分（禁止混淆）

```text
CommitCut        = F5 seek discontinuity commit（D14.8 rebase 触发点；
                   变量 committed）——不产生 D11 terminal authority
TerminalCommit   = D11 episode terminal decision 的最小受约束投影
                   （变量 committedOutcome）——由冻结 resolver
                   （completion.rs:1337-1398）逐格转写，不复制
                   episode-terminal-settlement 的整套 settlement 模型，
                   不产生第二套裁决语义
协议 Stop         = RecordStopIntent + EdgeStopFromStop 两步（其间是真实
                   窗口）——**不是 D11 Stopped 的判定**；窗口内 episode 的
                   终局由 TerminalCommit 按冻结表计算（worker_failure 优先）
```

`Failed{device}` 两格（resolver rule 4-else/6）在模型内不可达（无设备中断
源），分支保留以忠实转写冻结表，不为其构造可达性。

## 协议形状（被检对象 = F5-GATE 冻结协议 + GAP-01 pending-fatal 窗口）

```text
session      记录命令并把 render leg 停到 loop-top gate（D14.7 park）
worker       保持生产直到 leg parked；随后在串行化点上先 song_seek
             （BEFORE 一切 invalidation；begin 与 provider 返回分离——stop
             可落在 FFI 调用中途），成功才自行 purge（staging/余量丢弃 +
             edge 清空 + landing + **latch 擦除**）并持有产量直到 release
provider     Applied（擦除 latch、purge、新 continuation）/ RefusedUnchanged
             （零 invalidation、余量补完、**latch 保留**、命令消费）/
             MutatedThenFailed（fail-closed：worker_failure + edge.fail，
             绝不恢复旧播放）
stop         RecordStopIntent（completion-lock 步）→【真实窗口】→
             EdgeStopFromStop（数据面）；worker 只在真实退出位置退出
             （writeStopped / remainderBreak / postCutPlaneEnded / surfacing /
             mutated / eofClose）；**canceled seek 是 bookkeeping 不是退出**
read         loop-top 与 read 之间无 terminal 检查：latch 存在时下一次读
             必复现（surfacing）——default position 上 fatal 胜 stop
terminal     workerGone + drain verdict + worker_failure 按冻结 resolver
             次序一次性提交（TerminalCommit；first-wins）
```

## 模型词 → production 映射

| 模型词 | production reality |
|---|---|
| `WriteOld`/`FinishWriteOld` | decode worker loop-top → staging → `edge.write`（`session.rs` `decode_worker`）；`WriteOld` 的 ¬pendingFatal guard 正是「latch 存在时下一次读必复现」 |
| `ReadPartialSuccess` | `song_read_pcm` partial-success 出口：出块成功、fatal latch（C:1136-1156） |
| `ReadSurfacesFatal` | 下一次读入口复现 latch → adapter Err → `decode_failed`+`edge.fail`+return（C:1111-1113；session.rs:896-900）；**不受 stop 门控** |
| `PullOld`/`SubmitHeld` | render leg `read_frames` → `ReleaseBuffer`（wasapi.rs steady_loop） |
| `ParkLeg`（¬heldOld） | D14.7 render gate park |
| `WorkerSeek` | 串行化点上的 `song_seek` begin；eligibility 三查不排除 pending-fatal（reconciliation §2.2） |
| `SeekRefusedUnchanged` | 拒绝：零 invalidation，**pendingFatal UNCHANGED（承重）**，余量补完 |
| `SeekMutatedThenFailed` | 非证毕失败：fail-closed 既有 decode-failure 路线 |
| `WorkerCut` | Applied 后 worker purge + landing + **latch 擦除**（C:1195-1196） |
| `CommitCut` | session cutover commit（F5 域；≠ D11） |
| `RecordStopIntent` / `EdgeStopFromStop` | `request_stop` 的 completion-lock 步 / 其数据面 stop（两步间为真实窗口，Q6-iv） |
| `CancelSeekBookkeeping` | 串行化点 seek_aborted 分支：bookkeeping，**不是退出**（R1.2） |
| `WriteStoppedExit` / `RemainderFlushStopExit` / `PostCutStopExit` | 三个真实 worker stop-exit（session.rs:993-994 / :798-802 / :725-735,:753-771） |
| `EofClose` | read 的 Eof 臂（:892-895）；只从已 commit 的新 continuation 可达 |
| `LegObserveAborted` / `LegObserveDrained` | render leg 终态观察与 drain verdict 证据（edge.rs:237-239 terminal 先于数据） |
| `TerminalCommit` | D11 最小受约束投影（冻结 resolver 转写；见上文区分） |
| `DrainOld`/`DrainNew` | 设备消费（audible truth 抽象） |
| `PublishPositionOld/New` | D14.8 位置发布 epoch 归属（惰性动作——见 RESULTS 诚实注） |

Ghost 变量（`oldDrainEver`/`oldAfterCommit`/`posMixAfterCommit`/`seekFromPartial`/
`remainderLost`/`producedAfterFailure`/`oldCursorResumed`/`fatalLatchedEver`/
`exitPath`）是 verifier-only 见证，不要求 production 表示（AGENTS.md 验证权威
边界）。

## 最小状态预算（FORMAL-PLAN Q5 六问逐项）

| 变量 | 代表的真实事实 | 拥有者 | 改变它的动作 | 需要它的性质 | 可否推导 | 类别 |
|---|---|---|---|---|---|---|
| `pendingFatal` | SongCore handle fatal_error 位（已 latch 未 surface） | SongCore handle | ReadPartialSuccess(T) / WorkerCut(F, C:1196)；surfacing 不清除（物理位保留，对模型不可见） | Retain/NoOldFatal/Scoped/TCC + 5 witness | 否（本轮新增的事实本身） | 真实状态映射 |
| `stopIntent` | request_stop 已记录 intent（completion lock 步） | SessionCompletion | RecordStopIntent | TCC（rule 4 读 stop_requested）；W-A 窗口 | 否——与 edgeTerminal 的窗口正是 GAP-01 | 真实状态映射 |
| `edgeTerminal` | PcmEdge terminal（first-wins，edge.rs:190-201） | PcmEdge | EdgeStopFromStop/ReadSurfacesFatal(边 fail)/SeekMutated/ EofClose | 全部真实退出位置的门控；TCC；first-wins EOF×stop | 否——基线 `stopped` 一位无法表达 open/eof/failed/stopped | 真实状态映射 |
| `workerFailurePublished` | worker_failure record 已发布 | SessionCompletion | ReadSurfacesFatal / SeekMutated（first-wins） | resolver rule 1 先于 stop 咨询；TCC | 否——基线 episodeFailed 只覆盖 Mutated 一支 | 真实状态映射 |
| `workerGone` | worker 已退出（worker_exited） | SessionCompletion | 六个退出动作 | TerminalCommit 前置（证据未齐不 commit） | 否 | 真实状态映射 |
| `drainVerdict` | render leg 的 drain 证据 | SessionCompletion（drain observer） | LegObserveAborted/Drained | resolver rule 3/4 的输入 | 否 | 真实状态映射 |
| `committedOutcome` | D11 terminal 的最小受约束投影 | Playback Session（authority） | TerminalCommit（一次性） | Scoped/TCC；L4/L5 | 否——本轮新增 | 投影（受约束） |
| `exitPath` | worker 从哪个真实位置退出 | —（verifier-only） | 六个退出动作 | TCC 的冻结退出清单半边（D-T3 靶） | 否 | ghost |
| `oldCursorResumed` | Applied 后旧 cursor 生产复活 | —（verifier-only） | WriteOld（landing 时置位） | NoOldCursorResumeAfterApplied | 否 | ghost |
| `fatalLatchedEver` | pending fatal 曾被 latch | —（verifier-only） | ReadPartialSuccess | W_APPLIED_ERASURE_COMPLETED；RetainLatchOnRefused 的「refusal 时已存在」见证 | 否（见其不变式注释：普通 refusal 合法，反例 attempt-1） | ghost |

拒绝的候选变量（照抄 Q5 候选表被否决的项）：`latchRetained`（保留 =
Refused 动作对 pendingFatal 的 UNCHANGED，由 RetainLatchOnRefused +
witness 承载，独立位冗余）；`stopSeen`（被 stopIntent + edgeTerminal 两
真实位取代——单独「seen」位会重新融合 Q6-iv 要求分离的两步）；
`providerOutcome`（由 WorkerSeek 后三个互斥返回动作的非确定性表达，与基线
一致，独立变量是冗余）；`workerPos` 独立变量（复用基线 `worker` 值域，
M2 的 writing_old 攻击形状依赖它）。FORMAL-PLAN Q5 的
`exitedStop/exitedFail` 位置值由 workerGone+exitPath 承载。

## 刻意不建模

K0/Fiber/Capability、ring 容量与游标、WASAPI 流状态机、请求语法/目标
语义、多 seek 排队、Duration、Generation/Window/TimelineSegment、设备中断
（device abort——Failed{device} 格模型内不可达）、Frames(0) 退出（真实
provider 在 pending-fatal 窗口不可达，R1.2；mock provider 场景归 Rust 证据）、
**新 continuation 自身的解码故障**（模型内新 continuation 无故障——因此
NoOldFatalAfterApplied 是模型内全称命题，不是生产全称命题，如实声明）。
无 fairness/liveness：本门只攻击 safety 碰撞；不以任何 fairness 假设代替
真实同步事实。

## 负控制（每个必须 COUNTEREXAMPLE-WITNESSED）

基线七条（M1–M7，与 `f5-seek-implementation/` 的 M1–M7 生产补丁一一对应）：

| Mutation | 攻击 | 被抓不变式 |
|---|---|---|
| M1 `CommitBeforeTailPurge` | commit 不等设备尾排空 | InvStaleOutput |
| M2 `SeekMidWrite` | worker 在 in-flight 写半路上被 seek（staging 未丢弃） | InvStaleOutput |
| M3 `ParkWhileHeld` | render leg 持有已拉取旧块跨 park | InvStaleOutput |
| M4 `StalePositionWriter` | commit 后 stale writer 仍按旧 basis 发布位置 | InvPositionNoMixing |
| M5 `CommitBeforeLanding` | decoder 未 reposition 就 commit | InvStaleOutput |
| M6 `RefusalDropsRemainder` | refusal 丢弃保留余量 | InvRefusalContentContinuous |
| M7 `ResumeAfterMutatedSeek` | destructive failure 后恢复旧游标生产 | InvFailClosed |

M1/M2/M3/M5 同时违反 InvCommitPurged（commit 前置自检先行报警），属预期。

D-C 扩展四条（#289 冻结 defect ledger，PENDING_FATAL_DAG §8；
每条 mutation 恰好实现一条 D-T，反例必须走真实交错）：

| Mutation | Defect trace | 被抓不变式 |
|---|---|---|
| D-T1 `OldCursorResumeAfterApplied` | Applied 后旧 cursor 生产复活（WorkerCut→idle） | NoOldCursorResumeAfterApplied |
| D-T2 `RefusedClearsLatch` | RefusedUnchanged 被误当 Applied 清除 latch | RetainLatchOnRefused |
| D-T3 `LoopTopStopExit` | canceled-seek 的 bookkeeping 位置被改成 worker 退出（pre-R1.1 错误措辞）——反例链：pendingFatal→seek→stop intent→edge stop→取消→aborted drain→commit=Stopped，而该位置合法格是 failedDecode | TerminalCellConformance |
| D-T4 `AppliedKeepsLatch` | Applied 不擦除 latch 且旧循环复跑 → 旧 fatal 落入新 continuation | NoOldFatalAfterApplied |

FORMAL-PLAN REV.2 撤回的 M-A（pickup-order-flipped）不进入负控制集
（「fatal 先于 seek 结果被读到」是 §2.3 的合法格，不保证 red）。

## Witness 探针（必须 MUST-FAIL；防空洞不变式 + 反约束）

```text
基线六条：
ReachCommit                  cutover commit 可达
WitnessOldDrainPreCommit     pre-commit 旧 PCM audible drain 可达（合法）
WitnessOldEdgePreCommit      pre-commit edge 旧库可达
WitnessSeekRefused           seek 拒绝路径可达
WitnessSeekDestructive       destructive failure 路线可达
WitnessPartialSeek           余量在外 seek 可达

D-C 扩展五条（L1–L5 legal families 的可达证明；PENDING_FATAL_DAG §7）：
W_APPLIED_ERASURE_COMPLETED  latch→Applied 擦除→新 continuation→EOF→
                             Completed 可达（L1；强制验收项——若过强断言
                             杀死合法新 continuation，此探针红）
W_REFUSED_FATAL_RETAINED     Refused 后 latch 仍在可达（L2）
W_MUTATED_THEN_FAILED        MutatedThenFailed 可达（L3）
W_STOPPED_FROM_REAL_EXIT     真实 stop-exit 上的 Stopped 可达（L4）
W_FAILED_FROM_PENDING_FATAL  pending fatal surfacing 可达（L5；
                             default position 上 fatal 胜 stop）
```

## 运行

```text
specs/check.sh f5      # 本套件
specs/check.sh current # 含本套件
```

结论只能表述为：**在该模型的显式 bounds/假设内未找到反例**；不得表述为
"架构已被证明正确"。模型是 D-C 决策下的有界 safety 交错搜索证据，不是
D11 权威；stop-race 的一半仍以 Rust 真实边界验证为主（E1-ABI/E1-WORKER
是真实行为校准，RESULTS 引用其记录）。
