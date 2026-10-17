# PENDING_FATAL_DAG — Phase A/B deliverable for the D-C extension (#289)

```text
STATUS:   TRACE-FIRST FROZEN LEDGER（模型构造依据；非 authority）
          REV.2（2026-10-17 PR #292 外部终审后修正：W-E 失败路线三步窗口、
          §2 原子性裁定更正、§3 seek_eligible、§8 D-T4 单缺陷化）
BASE_SHA: 0e43ef0cbac6fe21d0cd83e14875ace98c73501a (origin/main, frozen 2026-10-17)
DECISION: D-C = EXTEND_EXISTING_MODEL
RECEIPT:  https://github.com/jnhu76/qianqian/issues/289#issuecomment-6229911387
AUTHORITY: ADR-PBK-002 §20 D14.5 + D14.8（seek spine）+ §17 D11（terminal）
           + error-system-reconciliation.md §2.3/§2.3.1/§2.4（矩阵与退出清单）
SCOPE:    行号为 BASE_SHA 处实测；本文件不复制规范语义，只给源码锚点。
```

所有行号在 `0e43ef0` 上逐一重新核实（不沿用 reconciliation/migration 的旧
BASE_SHA `e44aa80` 行号；两轮之间这些区域无行号漂移的文件也一律重查）。

---

## 0. 源码锚点总表（BASE_SHA 实测）

| 事实 | 位置 |
|---|---|
| loop-top seek pickup（`pending_seek.is_none() && remainder.is_none()`） | `session.rs:610-612` |
| seek_aborted 取消 = 纯 bookkeeping（无 return/continue，落入 read） | `session.rs:620-625` |
| park 证据 + edge Open 复核（eligibility，**不排除 pending-fatal**） | `session.rs:626-649` |
| provider seek 调用（`decode_stream.seek`） | `session.rs:650` |
| `RefusedUnchanged` 臂（seek_refused + release park，余量稍后补完） | `session.rs:651-662` |
| `MutatedThenFailed` 臂（decode_failed + edge.fail + return） | `session.rs:663-675` |
| `Applied` 臂（remainder=None :690、signal-history invalidate :691、edge.invalidate :692、observation.invalidate :700、landing :701、cutover wait :719-739、release wait :750-774、恢复生产 :775-778） | `session.rs:676-779` |
| 保留余量 flush（`write_some` 循环；terminal break :798-802；mid-remainder return :805-809） | `session.rs:793-811` |
| DSP live-update pickup | `session.rs:827-831` |
| `read_frames`（读调用；**loop-top 到 read 之间无任何 terminal 检查**） | `session.rs:834` |
| `Frames(0)` 臂 terminal 退出（真实 provider 在 pending-fatal 窗口不可达） | `session.rs:838-842` |
| stage + observation + `write_observing_seek` 分派（Whole/Stopped/CutPoint） | `session.rs:843-890` |
| `Eof` 臂（close_eof + return） | `session.rs:892-895` |
| `Err` 臂（decode_failed + edge.fail + return；**pending-fatal surfacing 点**） | `session.rs:896-900` |
| 唯一退出漏斗（worker_exited + abort_stranded_seek） | `session.rs:928-929` |
| `write_observing_seek`（observed :973、promote :984-986、preserve :987、CutPoint :988、write_some :990、zero-progress+terminal→Stopped :993-994、capacity wait :996） | `session.rs:958-999` |
| `decode_failed`（第一 publication；publication+settlement 同一锁程） | `completion.rs:501-503` |
| `request_stop`：intent :553 + gate 释放 :573-574 同一 completion 锁程；锁释放后 test-fence :584；**edge.stop() :585-587 是另一步** | `completion.rs:550-588` |
| seek slot / evidence 读取（take_seek_command/observed/parked/aborted/refused/landing/cutover/release/without_commit） | `completion.rs:758,769,782,800,808,818,882,912,921,926,935` |
| `worker_exited`（worker_terminal + worker_gone，同一锁程 settlement） | `completion.rs:1062-1067` |
| `abort_stranded_seek` | `completion.rs:1081-1097` |
| episode_ending_evidence（stop_requested ∨ outcome ∨ teardown） | `completion.rs:1257-1259` |
| `publish_evidence`（单一锁程：first-wins + evidence + resolve + commit + notify） | `completion.rs:1319-1331` |
| resolver 决策表（rule1 worker_failure :1344-1348；rule2 worker_terminal=Failed :1349-1353；rule3 Drained+Eof→Completed :1354-1359；Aborted+None→等待 :1369；rule4 Aborted+Stopped+stop_intent→Stopped / ¬stop_intent→Failed{device} :1370-1387；rule6 Aborted+其他→Failed{device} :1388-1392） | `completion.rs:1337-1398` |
| `write_some`（terminal→0；部分写） | `edge.rs:89-106` |
| `wait_for_space`（capped 容量等待） | `edge.rs:114-143` |
| `invalidate`（清缓冲不动 terminal） | `edge.rs:161-170` |
| close_eof / fail / stop（first-wins set_terminal :190-201） | `edge.rs:174-201` |
| render read（**terminal 检查先于数据**：stopped/failed→立即放弃缓冲） | `edge.rs:237-239` |
| adapter `classify_seek_status`（OK→Applied；INVALID_ARGUMENT→RefusedUnchanged；其余→MutatedThenFailed） | `lib.rs:493-498` |
| adapter `read_frames`（SONG_OK→Frames；**latch 复现的 status→Err**；EOF） | `lib.rs:510-545` |
| C 入口 latch 复现（`fatal_error != SONG_OK` → "pending decode error"，先于一切出帧/EOF） | `songcore_ffmpeg.c:1111-1113` |
| C 失败迭代 latch（`h->fatal_error = st` + 一次性诊断） | `songcore_ffmpeg.c:1136-1140` |
| partial success（`produced > 0 → ok(h)`：诊断清、**latch 存活**） | `songcore_ffmpeg.c:1154-1156` |
| `song_seek` 成功路径（flush + `reset_decode_state` → **latch 擦除**） | `songcore_ffmpeg.c:1195-1196` |
| `song_seek` 失败路径（`fail_arg`/`fail`：覆盖诊断、**latch 保留**） | `songcore_ffmpeg.c:1166,1186-1199` |

---

## 1. EXECUTION_DAG（pending-fatal 窗口，BASE_SHA 实测）

```text
[worker]  read_frames → song_read_pcm
            entry latch re-publish（C:1111-1113）        ← latch 已存时读必 Err
            decode 失败 → latch fatal_error（C:1136-1140）
            produced>0 → ok(h)（C:1154-1156）             ← partial success：latch 存活
          → stage DSP → write（write_observing_seek；容量等待间再观察 seek slot）
          → loop-top ──┬── pending_seek ∧ ¬aborted ∧ parked ∧ edge Open（:626-649）
          │              │   eligibility 三查无一排除 pending-fatal
          │              └→ provider seek（:650，长 FFI 调用）
          │                    ├─ Applied    → C reset_decode_state（C:1196）→ LATCH 擦除
          │                    │               → worker purge/landing（:690-701）
          │                    │               → wait×2（:719-739/:750-774；plane 终态→return）
          │                    │               → 恢复生产（新 continuation）
          │                    ├─ RefusedUnchanged → latch 保留；余量补完或 mid-remainder 终态退出
          │                    └─ MutatedThenFailed → decode_failed("seek failed") + edge.fail + return
          ├── aborted → release_seek_without_commit（:620-625）＝ bookkeeping，**不是退出**，
          │             控制流落入 remainder/DSP/read → latch 复现 → Failed
          └── 无 seek → read_frames（:834，无前置 terminal 检查）
                          ├─ latch 已存 → C:1111-1113 → Err → decode_failed（:896-900）
                          │                → edge.fail → return（surfacing）
                          └─ 无 latch → Frames(n)→write ∣ Eof→close_eof ∣ Frames(0)→terminal 退出

[session] request_stop（:550-588）
            同一 completion 锁程：stop_requested=true + pause/seek-hold 释放（:553,:573-574）
          [锁程结束——真实窗口]
            （任何线程此刻可观察 intent；edge 仍 Open；worker 可继续读/写/整个 seek）
            → edge.stop()（:585-587；first-wins，EOF 已提交则 no-op）

[render]  read_frames（edge.rs:226-267）：terminal 检查先于数据（:237-239）
            Open → 正常 pull/submit；stopped/failed → 放弃缓冲退出；eof → 排干后 Eof
          → drain verdict 证据（Drained / Aborted）经 publish_evidence 发布

[commit]  publish_evidence（completion.rs:1319-1331，单一锁程）
            first-wins → evidence → resolve（:1337-1398）→ commit → notify
          worker 退出：worker_exited(edge.terminal())（:928→completion.rs:1062）
            —— worker_terminal 与 settlement 同一锁程
```

真实可交错窗口（必须保留的 split）：

```text
W-A  request_stop 的 intent 记录（锁程内） → 同一请求的 edge.stop()（锁程外）
W-B  worker eligibility 检查（:626-649） → provider begin（:650） → provider
     调用期间 → provider 返回——stop intent / gate release / edge stop /
     settlement 均可落在其中任意一段。
     模型表达（2026-10-17 终审 P2 拆分更正，替代原 A-2「intent 落点非确定
     性」并集论证）：SeekEligibilityPass（检查原子通过，消费 seek_aborted
     裁决）与 SeekProviderBegin（无任何复核，调用照常发出）是两个动作，
     检查通过→调用发出之间的窗口直接可交错；调用中途段由「begin 与返回
     分离」承载。
W-C  worker 块内写（staging 未落队）期间 → 命令观察 → 余量保留 → 下一 loop-top 串行化点
W-D  render leg 排干 / 终态观察 → worker_exited → TerminalCommit
     （decisive 证据齐备前 commit 不发生：Aborted+worker_terminal=None → 等待）
W-E  失败路线三步（2026-10-17 终审 P1 新增；源码本就是三段，模型此前
     错误融合）：
       ① decode_failed 的 publish_evidence 锁程：publication + settlement
         （session.rs:896-897；MutatedThenFailed 臂 :673-674 同形）
       ② edge.fail()（:898/:675，锁程已释放；first-wins，edge.rs:190-201）
       ③ return → 退出漏斗 worker_exited（:928；其 publication 因 outcome
         已 settle 被 publish_evidence 丢弃，completion.rs:1322-1324）
     三步之间可与 request_stop 全程交错：settle 之后 intent 照常记录、
     edge.stop() 照常执行（completion.rs「after settlement: nothing
     changes」指终局不被改写，不是动作不发生）。故
     committedOutcome=failedDecode ∧ edgeTerminal=stopped 是合法终态格：
     edge.fail first-wins 不覆盖已提交的 Stopped，D11 rule 1 先于一切
     stop-intent 咨询、已提交的 Failed 不被重分类。
     模型动作：FatalFailurePublish / WorkerEdgeFailAdvance /
     WorkerExitFunnel；合法交错 witness = W_FAILED_DECODE_WITH_STOPPED_EDGE。
```

## 2. LOCK_BOUNDARY_MAP

| 操作 | 锁/同步 | 原子性裁定 |
|---|---|---|
| stop intent + pause/seek-hold gate 释放 | completion lock 单一锁程（completion.rs:552-576） | **融合为一动作**（Q6-i，锁程引用）；**不设 settle 门槛**——settle 后 request_stop 仍照常执行（终审 P1；RecordStopIntent 无 committedOutcome guard） |
| intent 记录 → edge.stop() | 锁程释放之后（:577-587） | **必须 split**（Q6-iv）；W-A 窗口 |
| read 调用（surface vs frames） | 单次 C 调用（C:1103-1157；adapter lib.rs:510-545） | **融合为一动作的三分支**（Q6-ii） |
| decode_failed publication + settlement | publish_evidence 单一锁程（completion.rs:1319-1331） | **publication+settlement 同一模型动作**（Q6-iii 锁程引用；FatalFailurePublish / SeekMutatedThenFailed） |
| Err 臂 edge.fail()（session.rs:898；Mutated 臂 :675） | edge lock（set_terminal first-wins，edge.rs:188-201）；**publish_evidence 锁程之外** | **必须 split**（终审 P1）→ WorkerEdgeFailAdvance；与 request_stop 的 edge.stop() 可交错（W-A/W-E），先后决定终态格 stopped/failed |
| 退出漏斗 worker_exited（session.rs:928） | publish_evidence 锁程；**outcome 已 settle 时 evidence 丢弃**（completion.rs:1322-1324 早退） | **必须 split**（终审 P1）→ WorkerExitFunnel；workerGone 仅在未 settle 时发布（失败路线恒为 FALSE——不得把「物理退出」编码为「证据已发布」） |
| worker eligibility 三查 | 三次独立的单锁获取（seek_aborted/leg_parked_evidence 各取 completion 锁 completion.rs:800-803/:782-785；edge.terminal() 取 edge 锁 edge.rs:204-213），均不跨持有（2026-10-17 审查 A-1 措辞修正：非「无锁读」——恰因三次独立获取才与 request_stop 可交错） | 与 request_stop **可交错**；不得与 provider 调用融合（Q6-v）。模型为独立动作 SeekEligibilityPass；通过 → begin 之间显式窗口（终审 P2；W-B） |
| provider seek begin → return | 长 FFI 调用 | **eligibility（SeekEligibilityPass）→ begin（SeekProviderBegin）/ return（三分支动作）分离**；W-B 窗口（终审 P2） |
| purge+landing（Applied 臂 :690-701） | worker 自身 program order（串行化点纪律） | 融合为一动作（D14.5 frozen order；F5 基线已如此） |
| 余量 flush 的 write_some 循环 | edge lock 每次单独获取，不跨等待（session.rs:950-957 锁序注） | 模型抽象为「补完成功 ∣ mid-remainder 终态退出」两结果 |
| render pull/submit | edge lock | 模型三库记账（基线不变） |
| drain verdict 发布 | publish_evidence 锁程 | 独立证据动作（Leg 观察先于 worker 退出是常态） |
| TerminalCommit | 同 publish_evidence 锁程 | 单一动作；first-wins（committedOutcome 一次性） |

## 3. WORKER_POSITION_MAP（模型 worker 值 ↔ 源码位置）

| 模型值 | 源码位置 | pending-fatal 下的意义 |
|---|---|---|
| `idle` | loop-top（:608-612）→ read（:834）之间 | **default position**：stop 不能在此退出 worker；下一次 read 必复现 latch（R1.1 核心更正） |
| `writing_old` | staging 持有整块、未落队（:843-867 之后） | in-flight 写；容量等待观察 terminal → `WriteStep::Stopped`（:993-994→:884）＝真实 stop-exit (a) |
| `writing_partial` | 前缀已落队、余量保留（:987-988）；refusal 后回此态补完（:793-811） | 余量 flush 终态 break（:798-802→:805-809）＝真实 stop-exit (b) |
| `seek_eligible` | eligibility 通过（:626-649）→ provider begin（:650）之间 | 检查裁决已消费（此后无取消分支）；stop intent / edge stop / settlement 可落入该窗口（终审 P2；W-B 前半） |
| `seeking` | provider 调用在飞（:650-779） | 调用已发出；stop 可落在调用中途（W-B 后半）；返回后按 outcome 分派 |
| `repositioned` | Applied purge/landing 后（:676-779 完成时点抽象） | 新 continuation；恢复生产的写/EOF 从此态出发（恢复在 commit 路由之后，:775-778——模型 repositioned 臂的 surfacing 要求 committed） |
| `decode_failed` | 失败退出后（:896-900/:663-675/:914-916） | 终态；**workerGone 未发布**——失败路线 settlement 先于漏斗发生，漏斗 evidence 被 publish_evidence first-wins 丢弃（completion.rs:1322-1324；终审 P1 更正，原「workerGone 已发布」是模型融合伪影） |
| （exited） | 退出漏斗（:928-929） | 由 `workerGone` + `exitPath` 承载，不复用 worker 值 |

**canceled pending seek 不是 worker 位置也不是退出**（:620-625 只做 release
bookkeeping，控制流继续向下）——模型中以独立 no-op 动作表达。

## 4. PROVIDER_OUTCOME_MAP

| outcome（lib.rs:493-498 分类） | C 侧行为 | latch | worker 后续 |
|---|---|---|---|
| `Applied`（SONG_OK） | av_seek_frame≥0 → flush + **reset_decode_state（C:1196）** | **擦除**（从未 surface） | purge+landing → 等待 commit 路由 → 新 continuation |
| `RefusedUnchanged`（INVALID_ARGUMENT） | `fail_arg`（C:1166）仅覆盖诊断 | **保留**（生产 adapter 不可达该类，mock/ABI 控制下 contract-live） | release park → 余量补完 → 旧 cursor 继续 → 下次 read surface |
| `MutatedThenFailed`（其余全部） | `fail()`（C:1186 区）覆盖诊断；latch 保留但失效 | 被取代 | release_without_commit + decode_failed("seek failed") + edge.fail + return |

模型表达：provider outcome 不设独立变量；由 SeekProviderBegin 之后的三个
互斥 return 动作的非确定性选择表达（与基线一致）。**outcome 动作不受
stopIntent/edgeTerminal 门控**——调用已发出，返回是 program order（W-B）。

## 5. LATCH_LIFETIME_MAP

```text
SET      C:1136-1140   失败迭代 latch fatal_error（此时 produced 可能 >0）
SURVIVE  C:1154-1156   partial success 返回 OK；诊断清、latch 存活
REPUBLISH C:1111-1113  下一次 song_read_pcm 入口复现（先于出帧/EOF 分支）
ERASE    C:1195-1196   仅 song_seek 成功路径的 reset_decode_state
RETAIN   C:1166 等     seek 失败路径仅覆盖诊断，latch 原样
CONSUME  surfacing（session.rs:896-900）后 episode 终止；handle 物理位保留（对模型不再可见）
UNREACH  EOF 分支（C:1157-1158 在入口复现之后）——latch 存在时 EOF 不可达（矩阵第 8 行）
```

模型对应：`pendingFatal`（BOOL，映射真实 handle 位）。擦除只发生在 Applied
臂（模型 WorkerCut）；Refused 臂 **UNCHANGED**（RetainLatchOnRefused 承重）。

## 6. TERMINAL_DECISION_MAP（D11 最小受约束投影）

生产 resolver（completion.rs:1337-1398）冻结次序 → 模型 TerminalCommit：

```text
rule 1  worker_failure 已发布              → Failed{decode|processing}   （先于一切 stop-intent 咨询）
rule 2  worker_terminal = Failed           → Failed{decode}
rule 3  drain = Drained ∧ worker_terminal=Eof → Completed                 （不咨询 stop_intent）
rule 4  drain = Aborted ∧ worker_terminal=Stopped
        ∧ stop_requested                   → Stopped
        ∧ ¬stop_requested                  → Failed{device}              （模型内不可达：唯一 edge-stop 源是 stop）
rule 6  drain = Aborted ∧ worker_terminal∈{Eof,Failed} → Failed{device}  （同上，模型内不可达）
其他    证据未齐（Aborted ∧ worker_terminal=None）→ 不 commit，继续等待
```

模型投影边界（诚实声明）：
- `committedOutcome ∈ {none, completed, stopped, failedDecode, failedDevice}` 是
  D11 终局的**最小受约束投影**，由冻结决策表逐条转写；不复制
  episode-terminal-settlement 的整套 settlement 模型，不产生第二套裁决语义。
- `Failed{device}` 两格在模型内不可达（无设备中断源），保留分支以忠实转写
  冻结表；不为其构造可达性。
- surfacing / MutatedThenFailed 的 `decode_failed` 在生产即「publication +
  settlement 同一锁程」→ 模型中二者同一动作（FatalFailurePublish /
  SeekMutatedThenFailed）；**同臂的 edge.fail 与退出漏斗在锁程之外，模型
  split 为 WorkerEdgeFailAdvance / WorkerExitFunnel**（终审 P1，W-E）；
  其余格子等 workerGone + drain verdict 齐备后由 TerminalCommit 一次性计算。
- 失败路线的 workerGone 恒为 FALSE（漏斗 evidence 被 first-wins 丢弃）——
  终态格不受影响：rule 1 的 worker_failure 证据在发布步已在案。
- 设备中断（device abort）不建模（AF-1 领域）；EOF×stop 竞争由 first-wins
  edge terminal 表达（EOF 已提交则 edge.stop no-op）。

---

## 7. Phase B — 冻结的 legal trace families（先于任何 mutation）

```text
L1  APPLIED-ERASURE（必须可达）
    ReadPartialSuccess（latch）→ RequestSeek → SeekEligibilityPass →
    SeekProviderBegin → Applied(WorkerCut：latch 擦除) → CommitCut →
    新 continuation（SubmitNew）→ EofClose →
    LegObserveDrained → TerminalCommit = Completed
    ——「旧 fatal 被合法吞没、新 continuation 正常完成」是 D14.5 设计行为；
       W_APPLIED_ERASURE_COMPLETED 强制验收。

L2  REFUSED-RETENTION（必须可达）
    ReadPartialSuccess（latch）→ RequestSeek → SeekEligibilityPass →
    SeekProviderBegin → RefusedUnchanged
    （latch 保留，pendingFatal UNCHANGED）→ 余量补完（或无余量）→ 旧 cursor
    继续 → read 复现 → surfacedFatal → TerminalCommit = failedDecode

L3  MUTATED-THEN-FAILED（必须可达）
    … → SeekEligibilityPass → SeekProviderBegin → MutatedThenFailed →
    worker_failure("seek failed") →
    failedDecode；旧 cursor 生产永不恢复（InvFailClosed / M7 域）

L4  STOP-WITH-GENUINE-EXIT（必须可达）
    stop 落在真实退出位置之一：
      (a) writing_old 容量等待观察 terminal → writeStopped
      (b) writing_partial 余量 flush 终态 break → remainderBreak
      (c) Applied 等待中 plane 终态 → postCutPlaneEnded
    → drain=Aborted → TerminalCommit = Stopped（worker_failure 未发布）

L5  STOP-WITH-PENDING-FATAL-SURFACING（必须可达）
    latch 已存 → stop intent 落在 default position 附近（intent、edge.stop、
    取消 seek bookkeeping 均已发生）→ 下一次 read 仍运行（无前置 terminal
    检查）→ 复现 → surfacedFatal → TerminalCommit = failedDecode
    ——worker_failure 先于 stop-intent 咨询（completion.rs:1344 先于 :1381）

L6  SEEK-REFUSAL-WITH-REMAINDER × STOP（必须可达）
    writing_partial（余量在外）→ RefusedUnchanged（余量保留）→ stop 落在
    flush 中 → remainderBreak → Stopped；余量内容损失为零（零内容损失义务
    只约束 refusal 自身：InvRefusalContentContinuous/M6 域；stop 中断 flush
    属终态退出，不是 refusal 的内容损失）
```

## 8. Phase B — 冻结的 defect traces（每个 mutation 恰好实现一条）

| ID | Defect trace | PRECONDITION | INJECTION_POINT | ILLEGAL_TRANSITION | OBSERVABLE_FAILURE | TARGET_INVARIANT | SOURCE_MAPPING |
|---|---|---|---|---|---|---|---|
| D-T1 | Applied 后仍从旧 cursor 恢复生产 | landing=TRUE（Applied 已发生） | WorkerCut 的 worker 去向（repositioned→idle） | 旧生产动作（WriteOld）在 cut 之后把旧 PCM 写回数据面 | `oldCursorResumed=TRUE`（可能连带 edgeOld 复活→commit 后旧输出） | `NoOldCursorResumeAfterApplied` | session.rs:676-779（Applied 后必须从 provider 新 cursor 恢复）；Q10-D-T1 |
| D-T2 | RefusedUnchanged 被误当 Applied，错误清除旧 latch | seekFailed=TRUE ∧ landing=FALSE | SeekRefusedUnchanged 的 pendingFatal 赋值（UNCHANGED→FALSE） | 保留义务被破坏：旧 fatal 被静默吞没，旧 continuation 看似干净 | `seekFailed ∧ ¬landing ∧ ¬pendingFatal` | `RetainLatchOnRefused` | C:1166（fail_arg 不触碰 latch）vs C:1196（仅成功路径擦除）；矩阵第 3 行 |
| D-T3 | stop/worker 位置使 committed outcome 落到合法矩阵外 | cutRequested ∧ stopIntent（canceled seek 场景；latch 可有可无——缺陷机制是「不存在真实退出的位置退出」，与 latch 正交。**前提修订（执行轮如实记录）**：冻结时写作 pendingFatal ∧ …，反例复核显示注入的伪造退出独立于 latch 承载缺陷；有 latch 时该位置合法格是 failedDecode，无 latch 且余量在外时合法格是经由 remainderBreak 的 Stopped——两种情况下 exitPath="loopTopStopExit" 都在矩阵外） | CancelSeekBookkeeping 的「bookkeeping-only」边界（额外退出 worker） | 在不存在真实退出的位置退出（:620-625 无 return）→ 无 worker_failure 的 Stopped 提交 | exitPath ∉ 冻结退出清单；committedOutcome=stopped 但 exitPath 不是任何真实 stop-exit | `TerminalCellConformance`（冻结退出清单 + 格子矩阵） | reconciliation §2.3.1（R1.1 更正：canceled seek 不是退出；default position fatal 胜 stop）；反例必须经真实交错，不许无条件写错终局。已归档两条反例：counterexample-DT3LoopTopStopExit.log（gate 配置，6 态，退出清单半边即抓）；counterexample-DT3-committed-cell-chain.log（诊断运行，仅保留 committed-cell 半边，9 态全链：RequestSeek→RecordStopIntent→EdgeStopFromStop→LegObserveAborted→伪造退出→TerminalCommit=stopped） |
| D-T4 | Applied 清除 latch 后旧 fatal 再次 surface | landing=TRUE（擦除应已发生） | WorkerCut 的 pendingFatal 赋值（FALSE→保留）∧ worker 去向（恢复旧 loop-top） | 新 continuation 的 read 复现**旧** fatal 并提交 failedDecode | `landing ∧ workerFailurePublished` | `NoOldFatalAfterApplied`（模型内抽象：新 continuation 无故障，故 cut 后任何 worker failure 即旧 fatal） | C:1196 擦除是 Applied 语义的一部分；矩阵第 2 行「erased without ever surfacing」 |

冻结纪律：

```text
- 每个 mutation 恰好对应一条 D-T；mutation 数量是本表的输出，不是输入。
- 反例必须走真实交错路径；禁止用「无条件写错终局」凑 D-T3。
- 若代码复核发现某 D-T 前提不成立，修订本表并记录，不为凑数注入不可达缺陷。
  （本表四条前提在 BASE_SHA 上均已复核成立。）
- M-A「pickup-order-flipped 必须 red」已按 FORMAL-PLAN REV.2 撤回，不进入本表。
```
