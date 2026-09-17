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

## 协议形状（被检对象 = F5-GATE 选定的 same-resource discontinuity protocol）

```text
render leg   loop-top park（承重护栏：不持有已拉取块跨 park = D14.7
             no-buffer-across-park）
session      第 1 相 edge purge（parked 时清队列；顺带唤醒被 full edge
             卡住的 writer；in-flight 写随后落进空 ring）
worker       串行化点（idle）做第 2 相 cut：staging 丢弃 + edge 清空 +
             landing 证据；此后该线程按 program order 只写 post-cut PCM
session      CommitCut：landing ∧ edge 干净 ∧ 设备尾排空 ∧ leg parked
             （= D14.7 padding==0 证据 + D14.8 rebase 触发点）
stop         terminal 恒赢：committed 永不成立，协议夭折
```

## 模型词 → production 映射

| 模型词 | production reality |
|---|---|
| `WriteOld`/`FinishWriteOld` | decode worker loop-top → staging → `edge.write`（`session.rs` `decode_worker`） |
| `PullOld`/`SubmitHeld` | render leg `read_frames` → `ReleaseBuffer`（`wasapi.rs` `steady_loop`）；`heldOld` = GetBuffer..ReleaseBuffer 窗口内的设备缓冲内存 |
| `ParkLeg`（¬heldOld） | D14.7 render gate park（loop-top、无设备缓冲跨 park） |
| `SessionPurge` | edge 第 1 相 invalidate（session 发起，F5-GATE 拟冻结的原语） |
| `WorkerCut` | worker 串行化点第 2 相 invalidate + staging 丢弃 + landing 证据 |
| `CommitCut` | session cutover commit（D14.8 rebase 触发点；D14.7 padding==0 证据） |
| `DrainOld`/`DrainNew` | 设备消费（audible truth 抽象） |
| `Stop` | D14.4/D11 terminal stop |
| `PublishPositionOld/New` | D14.8 位置发布的 epoch 归属（rebase 前后） |

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
| M2 `CutMidWrite` | worker 在 in-flight 写半路上被 cut（staging 未丢弃），写随后落队 | InvStaleOutput |
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
```

## 运行

```text
specs/check.sh f5      # 本套件
specs/check.sh current # 含本套件
```

结论只能表述为：**在该模型的显式 bounds/假设内未找到反例**；不得表述为
"架构已被证明正确"。
