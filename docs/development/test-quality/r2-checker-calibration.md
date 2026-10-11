# R2 — Checker 试点标定记录(opt-in;非强制规范)

> **证据等级:PROTOTYPE_EXECUTED**(原型级执行证据,承袭 #248 REV.3 口径;不是
> CI_ACCEPTED,不是仓库规范)。本标定**不安装**任何 Skill / lefthook 钩子 /
> CI Job;`tools/test-quality/` 只是可复现副本 + 调用说明。机器层边界
> (#249 REV.5):H1–H5、P 类检查**只能输出 Review Signal**,永不自动宣布
> 测试语义错误;每条信号的最终处置(确认/驳回/挂账)属于 review。

```text
DATE           = 2026-10-12
CHECKER        = tools/test-quality/check_test_quality_prototype.py
CHECKER_VERSION= sha256 ab704dd309818fd7bab3c759223445ce46c12bad8e6a9f30b0fc8d594d3105d8
                 (16028 bytes;byte 级等于 #248 issuecomment-6108683301 内联源与
                 qianqian-r1-reports/flash-b/checker-prototype/ 实物;本仓副本
                 self-test 6/6 复验通过)
SUBJECT_SHA    = HEAD 056a01dab311dcffc2ca5a51fbf645d9603423c3 (R2 修复后,d0002e47+f5f50a88+056a01da)
                 BASE 3f139694f175650e09e2843d2280456984052d4b (修复前,负向样本导出源)
ENV            = Linux/WSL2, python3 stdlib;仓库零生产改动(本次仅新增本文件与 tools/test-quality/)
SCOPE          = R2 pilot 三类已修测试(render_gate_seek / pause_seam 目标测试 + 新增负控制)
                 + Seek S6/PREMISE_DEBT 行 6 位点 + #242 open_abort/ports 正控制
复验声明(诚实口径):
  - 首轮标定执行于 C3 落地前的 HEAD(= f5f50a88 的树形状;该轮所基于的
    C1/C2 与本 PR 的 d0002e47/f5f50a88 经 git patch-id --stable 核验为同一
    patch,树内容零差异)。
  - C3(056a01da,R0B-05 修复)落地后,本会话于新 HEAD **全量复跑** §0–§2.5
    全部案例(self-test、4 个 BASE 负向导出扫描、全部 HEAD 正向扫描、H5 两对
    oracle-diff、record 冒烟):输出与首轮逐字一致,无一条漂移。
  - C3 对 live_tests 信号的增量影响单独记录于 §2.6(H4 目标信号**未消失**,
    词法层盲区,review 裁决驳回——与 POS-1a/POS-2a 的『修复即消失』不同类)。
CLASSIFICATION约定(本表):
  TP = 检查器输出与标定期望一致(缺陷位点触发、或合法位点正确沉默、或结构检查按设计工作)
  FP = 信号触发但语义 review 驳回(逐信号驳回,不存在文件级豁免清单)
  FN = 已知缺陷形状未触发(本次未出现)
```

## 0. 复跑命令(全部在本仓根目录)

```bash
P=tools/test-quality/check_test_quality_prototype.py
# 负向样本(BASE 导出;导出命令见 §1)
git show 3f139694f175650e09e2843d2280456984052d4b:crates/qianqian-audio-api/tests/render_gate_seek.rs > /tmp/r2-neg/render_gate_seek.rs
git show 3f139694f175650e09e2843d2280456984052d4b:crates/qianqian-playback/tests/pause_seam.rs      > /tmp/r2-neg/pause_seam.rs
git show 3f139694f175650e09e2843d2280456984052d4b:crates/qianqian-playback/src/live_tests.rs         > /tmp/r2-neg/live_tests.rs
git show 3f139694f175650e09e2843d2280456984052d4b:crates/qianqian-playback/tests/seek_seam.rs        > /tmp/r2-neg/seek_seam.rs
python3 $P self-test
python3 $P scan <file.rs>                     # exit 恒 0
python3 $P oracle-diff <before.rs> <after.rs> # exit 恒 0
python3 $P record tools/test-quality/fixtures/ter_valid_m3.md crates/qianqian-audio-api/src/ports.rs
```

## 1. 检查器自身负控制

`SELF-TEST OK (6/6)`,exit 0(实物副本与仓内副本各跑一次,均 6/6)。

## 2. 标定案例表(全部 EXECUTED,真实运行输出)

判例列 = 本执行者作为 review 的三值裁决;**它不是检查器的输出**,也不改变任何
默认门禁。`BASE` 行的信号行号以 `/tmp/r2-neg/` 导出文件为准,`HEAD` 行以仓内
文件为准。

### 2.1 负向样本(修复前;目标缺陷位点必须触发)

| Case | 样本 (SUBJECT_SHA) | EXPECTED_SIGNAL | ACTUAL_SIGNAL(逐字运行结果) | 分类 | 语义 review 裁决 |
|---|---|---|---|---|---|
| NEG-1 | BASE `crates/qianqian-audio-api/tests/render_gate_seek.rs` | H1 @ :173 `a_seek_park_publishes_only_seek_events_in_order`(R0B-01 CONFIRMED_STATIC 裸 sleep 前提) | H1 @ **:173** 命中 + 伴随 H3 @ :451(channel-witness unbound leg) | **TP** | 目标命中确认(裸 60ms sleep 对抗 10ms PARK_SLICE,无 happens-before;修复于 d0002e47)。伴随 H3:leg 经 `rx.recv_timeout` 见证 park 结算(send 先于退出),线程退出本身未 join——登记为 detached-exit 候选债,DEFERRED(R2 未改此测试) |
| NEG-2 | BASE `crates/qianqian-playback/tests/pause_seam.rs` | H4 @ :373 `stop_from_parked_after_eof_still_completes`(R0B-02:claim 'eof' 无见证) | H4 @ **:373** 命中 + H4 @ :317/:406('parked' 词法弱信号)+ H1 @ :605(不在 R2 范围) | **TP** | 目标命中确认(post-EOF 前提全无见证;修复于 f5f50a88)。:317/:406 为词法弱 FP(wait_established helper 实际见证 parked,token 不在 inline 行);:605 sleep(100ms) 挂账 DEFERRED(R0B-03 族,R2 未修) |
| NEG-3 | `crates/qianqian-playback/src/live_tests.rs`(BASE 导出;**首轮标定时 HEAD 与 BASE 同形**——R0B-05 当时未修。C3 修复 056a01da 已于本 PR 落地,其对信号的影响见 §2.6 复验) | H4 @ :1150 `a_failure_during_a_transition_settles_failed_through_d11`(R0B-05 oracle 盲) | H4 @ **:1150** 命中 + H1 @ :861/:871/:1034/:1493 | **TP** | 目标命中确认(transition 期间失败前提无 witness,合法路径与 broken pickup 不可区分)——该负向位点标定基于 BASE 导出,**结论不变**。H1×4 伴随信号 DEFERRED(R2 范围外,入台账待 S1–S6 分类;C3 落地后行号漂移——前三处 −1(删 import)、末处 +122(目标测试重写净增行),见 §2.6) |
| NEG-4 | `crates/qianqian-playback/tests/seek_seam.rs`(BASE=HEAD;**Seek S6/PREMISE_DEBT 台账行 6 指定位点**) | H1 @ :457 `an_unknown_landing_…`(owner 裁决:带债保留、非确定性充分、无免检先例) | H1 @ **:457** 命中(共 11 条:H1×10 + H4 @ :1023) | **TP** | 指定位点命中;REMARK:checker 对该位点只能复报信号,S6/PREMISE_DEBT 的裁定空间(#249 REV.5 adequacy_verdict = LIMITED_EVIDENCE_WITH_PREMISE_DEBT)永远在 review/台账侧,机器层不可给出 |

### 2.2 正向样本一:R2 修复后版本(修复敏感度——目标信号必须消失)

| Case | 样本 (SUBJECT_SHA) | EXPECTED_SIGNAL | ACTUAL_SIGNAL | 分类 | 语义 review 裁决 |
|---|---|---|---|---|---|
| POS-1a | HEAD `render_gate_seek.rs` | 目标 H1(原 :173)消失(60ms 前提 sleep 被 wait_until(quiescence) witness 替代) | **消失**(该测试仅剩 :199 与 :477 信号,见 POS-1b) | **TP** | checker 对修复敏感:前提 sleep 移除即触发消失,无需豁免清单 |
| POS-1b | 同上 | :199 的 25ms 诊断窗口 sleep(词法上仍是裸 sleep)按设计仍会触发 H1 | H1 @ **:199** 触发 | **FP**(记录在案,非豁免) | 合法诊断窗口:代码注释明示『NOT a correctness premise』——quiescence witness 后正确实现的 probe 计数已冻结,窗口只放大错误实现的暴露,不可能把正确实现变红。词法层无法区分『文档化检测窗口』与『伪装 S1 前提』——这正是信号必须走 review 的原因。S 分类(检测≠建立)归 review |
| POS-2a | HEAD `pause_seam.rs` | 目标 H4(原 :373)消失(EOF 前提现由 `eof.returned()` 经真实 read_frames 路径见证 + Linux `named_thread_gone_within`) | **消失**(该测试不再出现在信号表) | **TP** | 修复敏感度确认;witness token `eof` 现位于 wait 行,H4 词法条件被真实满足 |
| POS-2b | 同上 | 新增负控制测试 `stop_from_parked_without_eof_settles_stopped`(:553 起)doc 含 'post-EOF' 字样而 body 无 eof witness 行 → H4 会触发 | H4 @ **:561** 触发 | **FP**(记录在案,非豁免) | **缺席即性质**:该测试的属性恰是『decode 永不 EOF ⇒ 必结算 Stopped』,doc 的 'post-EOF' 指向被否证的 oracle 而非本测试前提。H4 的 claim→witness token 配对对否定式属性结构性失明——记为 H4 第二类已知 FP 形状(第一类见 POS-3 之外的 :429/:601) |

### 2.3 正向样本二:合法惯用法(不得误伤;已知 FP 必须重现并逐信号驳回)

| Case | 样本 (SUBJECT_SHA=HEAD) | EXPECTED_SIGNAL | ACTUAL_SIGNAL | 分类 | 语义 review 裁决 |
|---|---|---|---|---|---|
| POS-3 | `crates/qianqian-output-wasapi/src/open_abort.rs`(#242 640c6f5/ef66461:gate-event witness、poll 节流 sleep(2ms)、post-join exit witness) | 已知 FP 类:#248 标定为 H3×2(exit-witness / join-watcher 惯用法);**必须重新纳入,不得以文件豁免换取干净结果** | H3 ×2 @ :254 `a_hostile_repause_after_the_close_cannot_repark_the_aborting_leg`:①`spawned handle 'leg' never joined` ②`unbound spawn whose closure never joins` | **FP**(重现确认) | 逐信号驳回:①`let leg = {…spawn}` 块绑定的 handle 被 **move 进 watcher spawn**(`abort_render_thread(leg,…)` 内部执行 join;测试 2 无 `join_within` 字样故触发);②watcher 自身 unbound 且 join 深于闭包一层。结算边界由 `done_rx`/`exit_rx.recv_timeout(5s)` 断言见证(watcher 完成即 join 已沉降;exit witness 区分 panic 被吞)。**本仓与检查器均未建立任何豁免清单**;另记录不对称性:测试 1 仅因 body 恰含 `join_within` 字样而通过(正则交替的偶然满足),生产化需 handle-move 追踪 |
| POS-4 | `crates/qianqian-audio-api/src/ports.rs`(gate_pause_timing_tests:Mutex 事件日志轮询、块绑定 + join-watcher、`Latch::wait_for`) | 0 信号(合法 Mutex 轮询 poll 形状豁免;节流 sleep 在 loop 内;无裸 sleep) | **0 信号** | **TP**(正确沉默) | 真阴性确认:轮询节流(open_abort :127-142/:310-325 的 2ms sleep 同理被 poll 形状豁免)、块绑定 + watcher、Condvar wait_timeout 均不误伤 |

### 2.4 H5 首次真实数据运行(此前 #248 标定仅用合成 fixture)

| Case | 输入对 | EXPECTED_SIGNAL | ACTUAL_SIGNAL | 分类 | 语义 review 裁决 |
|---|---|---|---|---|---|
| H5-J | BASE→HEAD `render_gate_seek.rs` | 无真实 oracle 弱化(修复实为**加强**:`assert!((2..=4).contains)` → `assert_eq!(observations, 2)`) | H5b ×8(assert 行数 39→42) | **FP(全部)** | 位置 zip 错位:修复**新增** assert 行(premise asserts),逐位配对把 before 的 assert_eq 行对到 after 偏移后的 assert! 行。无任何真实 assert_eq→assert 降级 |
| H5-K | BASE→HEAD `pause_seam.rs` | 无真实弱化(terminal 断言保持 assert_eq!) | H5b ×8(68→75,含 `assert_no_leg_threads();` 被子串 'assert' 计入配对污染)+ H5c ×2(『50→500』) | **FP(全部)** | 同上 zip 错位;H5c 机制:after 的两条 `wait_until(from_millis(500))`(HEAD :465/:947,**对应 BASE :353/:752,原已存在**,无放大)各被拿去与 before 全文**第一个** from_millis——:54 `SourceBehavior::Paced` 的**非等待结构字段** delay(50)——比较,500 ≥ 10×50 误报。两条 H5c 均非真实预算放大 |
| H5-TN | 同上两对 | H5a(assert 行数下降)不得误触 | **未触发**(行数只增不减) | **TP**(真阴性) | 修复方向为加强时 H5a 正确沉默 |

### 2.5 record 模式对 R2 worktree 的结构校验冒烟

| Case | 输入 | EXPECTED | ACTUAL | 分类 |
|---|---|---|---|---|
| REC-1 | `record fixtures/ter_valid_m3.md crates/qianqian-audio-api/src/ports.rs`(自本仓根目录运行,P3 路径按 CWD 解析) | exit 0 + 两条路径 NOTE | `NOTE … PRODUCTION_BOUNDARY path resolves: crates/qianqian-audio-api/src/ports.rs:319`;`NOTE … AUTHORITY path resolves: docs/adr/ADR-PBK-002.md`;exit 0 | **TP** |

P 类为纯结构校验(fail-closed;PASS 不携带任何充分性含义)。malformed/tier-lie
负控制在 self-test 6/6 内覆盖,未在 R2 样本上重跑(非本次范围)。

### 2.6 C3 落地后的 live_tests 复验(新 HEAD 056a01da;本轮实际执行)

R0B-05 修复(C3 = 056a01dab311dcffc2ca5a51fbf645d9603423c3,重写
`a_failure_during_a_transition_settles_failed_through_d11` 为 transition-only
门控注入器 + 构造性 hold + 事件并集断言)落地后,本会话对 HEAD 的
`crates/qianqian-playback/src/live_tests.rs` 重新执行 `scan`(2026-10-12):

| Case | 样本 | EXPECTED(若修复与 POS-1a/POS-2a 同类) | ACTUAL_SIGNAL(逐字) | 分类 | 语义 review 裁决 |
|---|---|---|---|---|---|
| POS-5 | HEAD(post-C3)`live_tests.rs` | ——(实测与『修复即消失』**不同**) | H4 @ **:1195** `a_failure_during_a_transition_settles_failed_through_d11`(`doc claims 'transition' but no wait/assert line mentions it`)+ H1 @ :860/:870/:1033/:1615(伴随,行号较 BASE :861/:871/:1034 漂移 −1——删除 `std::sync::atomic` import 一行所致;末处 :1493→:1615 为目标测试重写净增 122 行所致,仍在 `n4_the_pause_oracle_catches_a_wallclock_ramp` 内,位点未变) | **FP**(目标 H4)/ 伴随 H1 维持 DEFERRED | 目标 H4 **未随修复消失**:C3 的 transition 见证是结构性的——事件并集断言(`TransitionStarted` 恰 1、无 `TransitionCompleted`、无 `UpdateRefused`、终局后 `pending()` 归空)与构造性 hold(`paused() ∧ buffered==EDGE_CAPACITY ∧ processed==9×STAGING`)——但 `TransitionStarted` token 位于 rustfmt 续行(`.filter(|e| matches!(…))`),该行本身不匹配 WITNESS_LINE 正则;构造性 hold 的几何谓词对词法层完全不可见。归入 FP-H4-1 已知族(token 不在 inline wait 行),逐信号驳回,依据=C3 的事件并集断言与 hold 三见证(见 `r2-pilot-report.md` §C.3)。**这不是修复敏感度反例**:C3 改变的是 oracle 的构造性(前提从轮询假设变为同步缝几何),不是移除裸 sleep/词法前提;词法信号如实保留并交 review 正是设计 |

复验结论:R0B-05 的负向位点标定(§2.1 NEG-3,基于 BASE 导出)结论不变;
C3 落地对 checker 信号的影响 = 目标 H4 位移(:1150→:1195)且仍触发(词法层
盲区,review 驳回)、伴随 H1×4 行号 −1、无新增信号类别。

### 2.7 复审修正轮复扫(HEAD 6d5c036;本轮实际执行)

PR #250 复审修正轮(报告 §H)落地三个修正 commit后,对三个被触文件重扫
(2026-10-12,`scan` @ `6d5c0368cc0f…`):**信号族与既往裁定一致、仅行号漂移**——

- `render_gate_seek.rs`(+35/−13 @ 4cdb52ba):H1 :199→**:221**(POS1B 25ms 诊断窗口,FP-H1-1 族)、
  H3 :477→**:499**(L6)。修正新增的失败收集路径(`wait_until(… is_finished)` + `join`)不触发
  任何信号(块绑定 handle 的结构化 join,与 #242 join-witness 惯用法同类)。
- `pause_seam.rs`(+34/−22 @ bad486b3):H1 :800→**:812**(L7,R0B-03 族)、H4 :429/:561/:601→
  **:434/:573/:613**(eof_while_parked / POS2B 否定式负控 FP-H4-2 / failure_while_parked,处置同 §2.2/§2.3)。
- `live_tests.rs`(+1/−1 @ 6d5c036):H4@:1195 与伴随 H1×4(:860/:870/:1033/:1615)**行号不变**
  (等长变更:diagnostic 断言收紧为含 `processing: ` 前缀)。

无新信号族、无信号消失;逐信号处置沿用 §2–§3 既有裁定,无需新增豁免。

## 3. FP 类登记簿(截至本次标定;无文件级豁免)

```text
FP-H3-1  exit-witness / join-watcher 惯用法(open_abort :254 ×2;render_gate_seek :477 同族)
         驳回路径:逐信号 review,依据结算边界断言(done_rx/exit_rx.recv_timeout;
         render_gate_seek 为 rx.recv_timeout)。检查器无抑制机制,每次扫描必重报——
         这是设计(信号去向是 review,不是静默)。生产化方向:handle-move 追踪。
FP-H4-1  helper 见证的 claim(wait_established 见证 parked,token 不在 inline wait 行;
         pause_seam :429/:601,BASE :317/:406);同族新增:结构性事件断言的 token
         在 rustfmt 续行(post-C3 live_tests :1195,见 §2.6)
FP-H4-2  否定式属性(absence is the property;pause_seam 负控制 :561)—— 新登记
FP-H1-1  文档化诊断检测窗口(fixed render_gate_seek :199,25ms;注释明示非前提)——
         S 分类(检测≠建立)归 review,checker 永远只报『possible』
FP-H5-1  真实数据 zip 错位 + 子串 'assert' 污染(assert_no_leg_threads 计入配对)
FP-H5-2  H5c first-match 基线取到非等待结构字段(from_millis(50) @ Paced delay)
结论:H5 在生产化前**不可**单独用于 before/after 判定,只能作为 review 线索;
     H1/H3/H4 维持 Review Signal 定位(#249 REV.5 机器层边界)。
```

## 4. 与 R1 标定(#248 checker-calibration.md)的关系

- BASE 信号集**逐条重现**(行号一致:H1@173/H3@451;H4@373 等 4 条;live_tests
  5 条;seek_seam 11 条;ports 0;open_abort H3×2)——R1 结果可复现性得到独立确认。
- 本标定**新增**两个 R1 不存在的维度:
  1. **修复敏感度**(POS-1a/POS-2a):修复移除缺陷形状 ⇒ 目标信号消失,无需豁免;
     修复引入的新合法形状 ⇒ 信号如实保留并交 review(POS-1b/POS-2b)。
  2. **H5 真实数据首跑**:全部触发均为机制性 FP(§2.4),合成 fixture 标定
     (H5b/H5c 各一条『有效』)在真实修复对上不成立——标定必须包含真实对,
     这本身是本次试点的一条结论。
- 未复用任何 #248 旧标定结果填表;每格均来自本次运行输出(§0 命令)。

## 5. 本标定证明什么 / 不证明什么

```text
证明(EXECUTED):
  - 负向 4 位点(R0B-01 :173 / R0B-02 :373 / R0B-05 :1150 / S6 :457)信号精确触发
  - R2 两处修复使目标信号消失(d0002e47、f5f50a88),checker 无需豁免即跟踪修复;
    C3(056a01da)落地后的 live_tests 复验同轮实际执行(§2.6:目标 H4 仍触发,
    词法层盲区,review 驳回——修复敏感度对『移除裸 sleep/词法前提』类修复成立,
    对『改 oracle 构造性』类修复如实保留信号)
  - open_abort H3×2 FP 类重现并逐信号驳回;未建立任何豁免清单
  - ports.rs / 轮询节流 / 块绑定 + watcher / Condvar 等待零误伤
  - H5 在两对真实修复对上 18/18 触发全为机制性 FP;H5a 真阴性
  - record/P3 结构校验对本 worktree 成立(路径按 CWD 解析)
  - checker 副本 self-test 6/6(仓外实物与仓内副本各一次)
不证明:
  - 任何信号 = 测试语义错误(全部三值裁决在 review;机器层边界不放松)
  - 任何测试的 adequacy_verdict(REV.5 裁定属台账/review,本文件不含裁定)
  - H2/H6/H7(本轮样本不含 try_recv 断言形状;H6/H7 未在原型实现)
  - 生产 checker 形态、Skill 触发率(Skill 未安装,见 §6)
  - R0B-05(live_tests)的修复效果:首轮标定时该修复不在树(REV.4 当时裁定
    R2_PRECONDITION_UNPROVEN);C3 = 056a01da **已于本 PR 落地**,本会话已本地复验
    其对信号的影响(§2.6)——目标 H4 未消失(词法层无法看到构造性 hold 与
    续行 token),语义裁决仍在 review,机器层边界不放松
  - seek_seam 其余 H1 族(:666–:1090)与 pause_seam :800 的 S 分类——DEFERRED 入台账
```

## 6. 边界与授权状态(逐条对应硬边界)

- lefthook 配置与默认执行链:**未改动**(本目录未被任何钩子引用)。
- GitHub CI Job:**未新增**。
- Skill(#248 SKILL.md 候选):**未安装为规范**;其触发/发现在本执行环境无
  独立 agent 会话可验证——HARNESS_UNVERIFIED / NOT_RUN,如实登记,不写成通过。
- 新 Runtime / 依赖服务:**无**(stdlib Python 脚本,只读扫描)。
- `crates/` 生产代码:**零改动**——R2 修复提交 d0002e47/f5f50a88/056a01da 均为
  纯测试改动(d0002e47/f5f50a88 只触 tests/ 下测试文件;056a01da 只触
  `crates/qianqian-playback/src/live_tests.rs`,该文件为 cfg(test) 单元测试,
  不进生产构建)。
- 台账/规范冻结:#249 REV.5 仍是 CANDIDATE;本文件是 pilot 标定记录,不构成
  规范、不构成免检先例(S6(ii) 无先例条款不受影响)。
