# f5-seek-discontinuity — F5 seek cutover 安全模型

```text
STATUS: CHECKED-IN-MODEL（current formal gate；F5-GATE 证据）
权威:   ADR-PBK-002 §20 D14.5（seek 语义 spine；F5-GATE 拟冻结的
        physical-output cutover 机制决定见其 amendment）+ D14.8 seek 规则
攻击:   commit 之后旧 PCM 复活（D14.5 根不变式的 safety 碰撞）；
        commit 之后位置发布混用 pre/post basis（D14.8 no-mixing 碰撞）
```

## 入口问题（ADR-PBK-001 §13）

> **seek cutover 的哪些独立合法事件（旧解码、旧 edge 队列、旧设备尾、park、
> landing、commit、stop）能交错出「commit 之后旧 PCM 仍成为 output」或
> 「位置混用 pre/post 证据」？**

模型只建模 **discontinuity protocol**（D14.5 的三库 + commit boundary），
不建模音频细节。两条不变式：

```text
InvStaleOutput       commit 之后旧 PCM 不得再被 audible drain（D14.5 根不变式）
InvPositionNoMixing  commit 之后不得按旧 basis 发布位置（D14.8 seek 规则）
InvCommitPurged      commit 蕴含 landing ∧ edge 干净 ∧ 尾排空（护栏自检）
```

## 协议形状（被检对象 = F5-GATE 冻结的 same-resource discontinuity protocol）

```text
session      记录命令并把 render leg 停到 loop-top gate（D14.7 park
             不变量，归属为 cut——不是 pause engagement）
worker       保持生产直到 leg 的 parked 证据到达（提前停产会把 leg
             困在空 edge 的阻塞 read 里、协议停摆）；随后在串行化点上
             先 song_seek（BEFORE 一切 invalidation；refusal 是 pre-cut、
             无损——最多损失一个 in-flight staging block），成功才自行
             purge（staging 丢弃 + edge 清空 + landing 证据）并持有
             产量直到 release——program order 是承重排除，不是原语
session      CommitCut：landing ∧ edge 干净 ∧ 设备尾排空（D14.7
             padding==0 证据）∧ leg parked ∧ unsettled（D14.8 rebase
             触发点）
stop         terminal 恒赢：committed 永不成立，协议夭折
```

## 模型词 → production 映射

| 模型词 | production reality |
|---|---|
| `WriteOld`/`FinishWriteOld` | decode worker loop-top → staging → `edge.write`（`session.rs` `decode_worker`） |
| `PullOld`/`SubmitHeld` | render leg `read_frames` → `ReleaseBuffer`（`wasapi.rs` `steady_loop`）；`heldOld` = 从拉取（read）到提交（ReleaseBuffer）之间 leg 手里的一块——承重性质（park 不持块）覆盖整个窗口 |
| `ParkLeg`（¬heldOld） | D14.7 render gate park（loop-top、无设备缓冲跨 park） |
| `WorkerSeek` | worker 串行化点上的 `song_seek`——BEFORE 一切 invalidation；refusal 是 pre-cut、无损 |
| `SeekRefused` | `song_seek` 拒绝（SEEK_UNSUPPORTED/ERROR/INVALID_ARGUMENT）：零 invalidation，命令消费，可再 seek |
| `WorkerCut` | song_seek 成功后 worker 自行的 purge：staging 丢弃 + edge 清空 + landing 证据 + 持有产量直到 release（模型中 = `SubmitNew` 的 committed 护栏） |
| `CommitCut` | session cutover commit（D14.8 rebase 触发点；D14.7 padding==0 证据） |
| `DrainOld`/`DrainNew` | 设备消费（audible truth 抽象） |
| `Stop` | D14.4/D11 terminal stop |
| `PublishPositionOld/New` | D14.8 位置发布的 epoch 归属（rebase 前后；base 模型中为惰性动作——见 RESULTS §17 诚实注） |

Ghost 变量（`oldDrainEver`/`oldAfterCommit`/`posMixAfterCommit`）是
verifier-only 见证，不要求 production 表示（AGENTS.md 验证权威边界）。

## 刻意不建模

K0/Fiber/Capability、ring 容量与游标、WASAPI 流状态机、请求语法/目标
语义、多 seek 排队、Duration、Generation/Window/TimelineSegment（本模型
的存在正是为了证明它们不被需要）。无 fairness/liveness：本门只攻击
safety 碰撞。

## 负控制（每个必须 COUNTEREXAMPLE-WITNESSED）

| Mutation | 攻击 | 被抓不变式 |
|---|---|---|
| M1 `CommitBeforeTailPurge` | commit 不等设备尾排空（绕过 D14.7 padding==0 证据） | InvStaleOutput |
| M2 `SeekMidWrite` | worker 在 in-flight 写半路上被 seek（staging 未丢弃），写随后落队 | InvStaleOutput |
| M3 `ParkWhileHeld` | render leg 持有已拉取旧块跨 park（D14.7 no-buffer-across-park 被破坏） | InvStaleOutput |
| M4 `StalePositionWriter` | commit 后 stale writer 仍按旧 basis 发布位置 | InvPositionNoMixing |
| M5 `CommitBeforeLanding` | decoder 未 reposition 就 commit | InvStaleOutput |

M1/M2/M3/M5 同时违反 InvCommitPurged（commit 前置自检先行报警），属预期。

## Witness 探针（必须 MUST-FAIL；防空洞不变式）

```text
ReachCommit              committed 可达（否则核心不变式空洞）
WitnessOldDrainPreCommit pre-commit 旧 PCM audible drain 可达
                         （旧输出在 commit 前合法——不变式禁的是 commit 后）
WitnessOldEdgePreCommit  pre-commit edge 旧库可达（三库记账非摆设）
WitnessSeekRefused       song_seek 拒绝路径可达（frozen 协议的失败半边
                         被真实建模，不是装饰）
```

## 运行

```text
specs/check.sh f5      # 本套件
specs/check.sh current # 含本套件
```

结论只能表述为：**在该模型的显式 bounds/假设内未找到反例**；不得表述为
"架构已被证明正确"。
