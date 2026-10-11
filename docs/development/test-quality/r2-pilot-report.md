# R2 试点实验报告 — 并发测试 Adequacy 修复(C1/C2/C3)与准入检查器标定(C4)

> **文档类别声明(必读)**:本文件是 R2 试点的**实验报告**(experiment record),**不是正式
> Test Quality Authority**。它不定义新的测试语义、架构名词、生命周期或门禁;检查器信号的
> 最终处置(确认/驳回/挂账)与 #249 REV.5 `adequacy_verdict` 的裁决权属 review / 台账流程。
> §F 的 Ledger 表是按 REV.5 词汇整理的**提案行**,供 review 采纳或驳回,不是已生效裁决。
> 历史证据引用允许保留历史用语,不构成现行架构表述。

## 0. 范围、冻结基线与证据口径

- 冻结 BASE:`3f139694f175650e09e2843d2280456984052d4b`(#242 merge)。【git·档案】
  `git -C /home/hoo/Source/qianqian-r2 rev-parse origin/main` → 同值,`mainDelta` 冻结成立。
- 分支 / HEAD:交付分支 `test/r2-adequacy-pilots`(由暂存分支 `r2-finish` 移交,见 §G);
  报告基于链上最新前驱 `e5a13e4ed2739f016c04e8dbb0581010f1b81180`(C4,amended)写入。
  本文件自身的 commit SHA(C5)由脚本补记(PR 正文 HEAD 记为 `PENDING_HEAD`)。
  **复审修正轮(2026-10-12,见 §H)**:PR #250 独立复审(REQUEST_CHANGES)后追加三个修正
  commit(C1 失败路径 join / C2 Linux 限定 / C3 origin 断言收紧)与本档案修订 commit;
  §0/A/D/F 各处仅以"复审修正"行与 §H 交叉引用,不改写首轮记录原文。
- Commit 链【git·档案,`git log --format='%H|%P|%s'`】:
  `3f139694` ← C1 `d0002e47` ← C2 `f5f50a88` ← C3 `056a01da` ← C4 `e5a13e4e`(写入时 HEAD)
  ← C5 `32beab21`(本文件)← 修正 C1′ `4cdb52ba` ← 修正 C2′ `bad486b3` ← 修正 C3′ `6d5c036`
  ← 复审修正档案 commit(本节,SHA 由 §H 命令记录)。
  **C3 已落地**(首轮受阻回滚,重新落地经过见 §D.5 执行事故记录)。
- Commit 头改写偏差(向读者披露):campaign 推荐 C1–C3 header 为
  `fix(test/audio-api)` / `fix(test/playback)`(scope 含斜杠,不合本仓 commit-convention
  门禁),落地时按仓库门禁改写为 `test(audio-api)` / `test(playback)` ×2,**正文零变化**
  ——C1/C2 与首轮对应提交经 `git patch-id --stable` 核验为同一 patch(§G);C3 首轮无
  commit(受阻未落地),不存在前序版本可偏差。
- 工作树状态【git·档案】:`git status --porcelain --untracked-files=all` 为空、
  `git diff --numstat` 与 `git diff --cached --numstat` 为空 → 工作树完全干净;
  `git diff --stat 3f139694..HEAD -- crates/qianqian-playback/src/live_tests.rs` =
  **+146/−24(单文件)** → C3 修复在树中(`056a01da`)。
- 证据口径标注:
  - 【git·档案】= 档案会话以 git 实测(完整命令清单见 §G);
  - 【证据包·实施者】/【证据包·mutation】/【证据包·评审】= 引自 R2 工作流证据包原文
    (实施者 / mutation-agent / 只读评审者三席各自的声明与运行记录);
  - 【本档案会话复跑】= 本轮档案(stitch)会话在本仓 HEAD 上实际重新执行并记录于 §G 的
    命令(checker 全套标定复跑、C3 目标测试 / playback lib 套件 / ×20 串行);
  - 除【本档案会话复跑】外,执行结果均引自证据包并标注原执行主体,不由本档案背书为
    "我方复跑"。
- 诚实总则:本地 `cargo test` 绿是回归证据,**不构成并发交错完备性证明**
  (AGENTS.md Verification 口径)。本文全部 ADEQUATE 判定仅在各自声明的性质、界限与假设内成立。

---

## A. Commit Evidence

字段说明:COMMIT_SHA / PARENT_SHA / CHANGED_FILES / 代码增量 为【git·档案】实测;
PROPERTY / AUTHORITY / PRODUCTION_DAG / TEST_EXECUTED / 负控 / 清理 / 限制 引自证据包
或【本档案会话复跑】并逐条标注执行主体。

### A.C1 — `test(audio-api): witness seek quiescence before release`

| 字段 | 内容 |
|---|---|
| COMMIT_SHA | `d0002e47e20431659ab65bb1d9fd3f3efb59bbb8`【git·档案】(与首轮对应提交经 patch-id 核验同一 patch——commit 头按门禁改写、正文零变化;核验命令与 patch-id 见 §G) |
| PARENT_SHA | `3f139694f175650e09e2843d2280456984052d4b`(=BASE)【git·档案】 |
| CHANGED_FILES | `crates/qianqian-audio-api/tests/render_gate_seek.rs`(+34/−8,单文件单测试函数)【git·档案 `git show --numstat`】 |
| PROPERTY | seek park 期间 gate 仅按序发布 `SeekEngaged → SeekTailQuiesced → SeekDisengaged`(恰一次 quiescence、无 pause 事件归属伪造,D14.5 attribution separation);release payload 恰被消费一次(`captured == Aborted`);quiescence 之后停止 probe(`observations == 2`,机制真值)。 |
| AUTHORITY | 方法权威:`docs/development/concurrency-test-design.md`(前提须真实同步边 :14/:36;watchdog/失败释放 :46;sleep 三分类 :75 与 :83;负控非空性 :78;卫生 :79)——该文档自声明 NON-NORMATIVE for product behavior。事件归属语义引用 ADR-PBK-002 D14.5(评审者声明已核对原文)。 |
| PRODUCTION_DAG | test main → `RenderGate` + leg 线程:park_loop_top → intent 锁 → seek_hold → `SeekEngaged` → park slice probe ×2(第 2 次返回 Quiesced)→ `SeekTailQuiesced` 发布 → main `release_seek_hold(Aborted)` → leg wait 谓词假返回 → break → `SeekDisengaged` → 出口漏斗 `slice(SeekRelease)` → join。完整 DAG 见 §C.1。 |
| TEST_EXECUTED | 【证据包·评审】`cargo test -p qianqian-audio-api --test render_gate_seek a_seek_park_publishes_only_seek_events_in_order -- --exact --test-threads=1` → 1 passed 0.05s;全文件 `--test-threads=1` → 9 passed 0.12s;加载复测:测试二进制直接调用 220 次(taskset 核 16-19,8 个 busy-loop 负载 taskset 核 0-15)→ pass=220 fail=0;`cargo fmt` / `cargo clippy -p qianqian-audio-api --all-targets -- -D warnings` 干净。【证据包·gates 字段】fmt / clippy / 目标测试 / `cargo test -p qianqian-audio-api` 全部 exit 0。本档案会话未复跑 C1 cargo 项(见 §G 未执行声明)。 |
| TARGETED_TEST_COUNT | 1(目标测试;所在文件共 9 测,修复为原位修改、数量不变;修复前计数未单独记录) |
| RELATED_SUITE_RESULT | render_gate_seek 全文件 9 passed 0.12s;`cargo test -p qianqian-audio-api` exit 0【证据包】 |
| NEGATIVE_CONTROL | F1–F4 由【证据包·mutation】实际执行,全部 RED=true、签名匹配、已恢复:F1 必要 quiescence 不发布(删 `ports.rs:715` emit)/ F2 归属伪造(pause 事件泄入 seek park)/ F3 停探守卫恒真化(重复 probe+重复 quiescence)/ F4 出口漏斗断供(`captured=None`)。评审者**静态核验**签名与失败清理路径后未复跑(只读边界,如实声明)。 |
| CLEANUP_VERIFIED | 两个 wait 点均 release-before-assert(`render_gate_seek.rs:167-188`,【git·档案】Read 核实在位);F1 mutation 的实际 RED 运行即走过"witness 超时 → 先 release → leg 有界退出"的清理路径(不悬挂)【证据包·mutation + 评审静态核验 P1 走 `ports.rs:677-690` pre-arrival 消费、P2 走 `:708-710` break】。 |
| LIMITATIONS | (i) 5s watchdog 是诊断上界,leg 饿 >5s 仍可 false-RED(与本文件全部既有 `wait_until` 同类,doc 明示 timeout 非公平性证明);(ii) `assert_eq!(observations, 2)` 与当前 probe 形状耦合,机制合法改变 probe 次数时需同步更新;(iii) 同文件其余测试 6 处裸 `assert!(wait_until)` 无失败 release(:239/:295/:381/:421/:454/:503-506,既有卫生债 → §F L10);(iv) 同文件 H3 channel-witness unbound leg(HEAD :477)挂账(→ §F L6);(v) 本地绿 ≠ 交错完备证明。 |
| 复审修正 | **P1/C1(2026-10-12 复审)**:两失败分支原为 release 后直接 `assert!`,`leg` 句柄随 unwind 静默 detach(RELEASE ≠ JOIN)——修正 @ `4cdb52ba`:release → 有界 join(`wait_until(5s, is_finished)` → `join()`)→ panic 携带 `exited_within_5s/joined` 收集证据;两条错误路径突变 M1(set_seek_hold 失效)/ M2(SeekTailQuiesced 不发布)实测 RED 且 `joined=true`、有界完成。证据与签名见 §H.1。 |

### A.C2 — `test(playback): establish real post-EOF stop premise`

| 字段 | 内容 |
|---|---|
| COMMIT_SHA | `f5f50a88edec55df0ab152eca110d2eb51401376`【git·档案】(与首轮对应提交经 patch-id 核验同一 patch——commit 头按门禁改写、正文零变化;核验命令与 patch-id 见 §G) |
| PARENT_SHA | `d0002e47e20431659ab65bb1d9fd3f3efb59bbb8`(C1)【git·档案】 |
| CHANGED_FILES | `crates/qianqian-playback/tests/pause_seam.rs`(+200/−5,单文件)【git·档案】;`tests/common/mod.rs` **零触碰**(与 fixPlan 的唯一偏差:原计划给 `TestDecode` 加 `pub eof` 字段会破坏 `seek_seam.rs:201/:598` 的全字段结构体字面量编译,故改为本文件内委托式 double `EofWitnessedDecode`——爆炸半径更小,11 个共享 binary 零影响可证【证据包】) |
| PROPERTY | D14.7:已停靠且 **post-EOF** 的 episode,`request_stop` 不得 force-abort——尾部播完并排空,经 D11 committed Fact(`wait_terminal`)结算 `Completed`(不得伪造 `Failed{device}`);否定式伴随属性:无 EOF 的停靠 stop 必结算 `Stopped`。 |
| AUTHORITY | ADR-PBK-002 D11(terminal-outcome authority 设计)、D14.7(stop-from-parked-after-EOF)。**原文核对状态(向读者披露)**:证据包 diagnosis/评审对 D14.7 仅作"引用"而未声明核对原文——证据强度弱于 A.C1(其 D14.5 有"评审者声明已核对原文");**本档案会话补核**:`docs/adr/ADR-PBK-002.md:2294-2297`(D14.7 节,节首 :2094)逐字为 "Stop-from-paused after decode EOF plays the tail out and drains → worker-Eof × drain-Drained → Completed … the gate must NOT force-abort, which would fabricate the Failed{device} history",负控的 Stopped 臂同节 :2293——C2 性质与 F2 所引条款与原文一致。方法权威 concurrency-test-design.md(:14/:36/:46/:78);测试注释显式区分 EOF 边界证据(decode seam 机制证据)与断言对象(D11 committed Fact)。 |
| PRODUCTION_DAG | T(测试线程)→ activate → spawn W(`qianqian-decode`)+ L(`qianqian-test-render`);L 首个 park slice → Engaged+TailQuiesced → Paused 投影;W `read_frames → Eof(latch) → close_eof → worker_exited(Eof) → 线程退出`;T `request_stop`(completion 锁内 stop_requested + gate 释放;锁外 `edge.stop()` first-wins,对已 Eof 的 edge 为 no-op);L 离开 park 排空 → `drain.complete(Drained)` → resolve(Drained×Eof)→ `Completed`;T `wait_terminal` → dispose → quiet / no leaked leg threads。完整 DAG 见 §C.2。 |
| TEST_EXECUTED | 【证据包·评审】`cargo test -p qianqian-playback --test pause_seam` → 10 passed(含新增负控);两目标测试串行 200/200 GREEN(证据包原文如此,**未写明是两目标各 200 次还是合计 200 次**,本档案按原文引用、不改写);16 个外部 yes-spinner(20 核)负载下 50/50 GREEN;clippy / fmt 干净。【证据包·gates 字段】fmt / clippy / `--test pause_seam` / `cargo test -p qianqian-playback` 全部 exit 0。本档案会话未单独复跑 pause_seam 集成测试(见 §G;playback lib 套件复跑覆盖 C3 面,不含 tests/ 下集成测试——如实区分)。 |
| TARGETED_TEST_COUNT | 2(修复目标 1 + 新增负控 1);文件 9 → 10 测(评审者更正:实施者"改动前后各一次 10 passed"的 before 计数不准,HEAD 原为 9 测)。 |
| RELATED_SUITE_RESULT | pause_seam 10 passed;`cargo test -p qianqian-playback` exit 0【证据包】 |
| NEGATIVE_CONTROL | (a) 新增确定性负控 `stop_from_parked_without_eof_settles_stopped`(Paced 源永不 EOF ⇒ `Stopped` 结构唯一可达,`pause_seam.rs:553-596`【git·档案】Read 核实在位);(b) F1–F4 由【证据包·mutation】实际执行,全部 RED=true、签名匹配、已恢复:F1 Drained×Eof 臂改标 Stopped / F2 first-wins 破坏(committed Eof 被 stop 覆写 → `Failed`)/ F3 drain verdict 伪造 Aborted(→ `Failed`)/ F4 无-EOF stop 被误标 Completed(负控咬合)。评审者实核 resolve 三臂(`completion.rs:1298-1301/:1313-1326/:1331-1335`)。 |
| CLEANUP_VERIFIED | 负向 mutation 均为干净 `assert_eq` RED、无挂起【证据包·mutation】;`dispose().quiet` + `assert_no_leg_threads()` 在位(:547-549、:592-594【git·档案】)。**台账更正(评审者)**:先前分析"QianqianApp Drop 触发 teardown discharge"与代码不符——`qianqian-app`/`qianqian-composition` 均无 `impl Drop`(grep 证实),失败路径实际依赖 `within()` watchdog 大声失败 + 分离线程随进程退出;该姿态为本文件全部 10 测共有且先于本修复存在,非本修复引入、不 wedge harness。 |
| LIMITATIONS | (i) 非 Linux 残余缝:Eof 返回 → `close_eof` 之间无同步,`request_stop` 的 `edge.stop()` 理论上可先落(first-wins 为设计语义)→ 合法 false-RED 窗口,已在测试注释登记(`pause_seam.rs:496-500`【git·档案】Read 核实),关闭需生产侧 test seam、超出本次范围;(ii) 前提依赖当前 funnel 形状(Eof 返回与 close_eof 之间无阻塞工作);(iii) `stop_from_an_established_pause…`(:374)的"尚未 EOF"前提仍无 witness(既有债 → §F L9);(iv) 同文件 H1@:605→HEAD :800(R0B-03 族)未修(→ §F L7);(v) 本地绿 ≠ 交错完备证明。 |
| 复审修正 | **P1/C2(2026-10-12 复审)**:上述限制 (i) 的形态从"可运行但不可靠的断言 + 登记债务"改为**诚实 NOT_RUN 限定** @ `bad486b3`——`EofWitness::returned()` 在 `read_frames` 返回处 latch,先于生产 worker 的 `edge.close_eof()`,Linux 线程退见证据之外的平台上无条件 `Completed` 断言构成对合法调度的 false-RED oracle;修正将整个测试 `cfg(target_os = "linux")` 限定(内层 cfg 块随之展开为无条件),非 Linux 不再运行该断言;可移植提交点见证需要生产侧 API(`PcmEdge`/`EdgeTerminal` crate-private、`RenderPcmInput` 无 terminal 查询),复审裁决试点不得新增。no-EOF 负控保持跨平台(其 oracle 无 EOF 前提、交错完备成立)。Windows `x86_64-pc-windows-gnu` 交叉 `cargo check --tests` 编译级冒烟通过(非运行证据,§H.2);L2 verdict 提案更新(§H.2)。 |

### A.C3 — `test(playback): causally pin live transition failure` — **已落地(landed)**

| 字段 | 内容 |
|---|---|
| COMMIT_SHA | `056a01dab311dcffc2ca5a51fbf645d9603423c3`【git·档案】 |
| PARENT_SHA | `f5f50a88edec55df0ab152eca110d2eb51401376`(C2)【git·档案】 |
| CHANGED_FILES | `crates/qianqian-playback/src/live_tests.rs`(**+146/−24**,单文件)【git·档案 `git show --numstat`】;cfg(test) 单元测试文件 + 文档注释 + 删除旧注入器专用的 `std::sync::atomic` import;**不改生产码、不改 tests/common/mod.rs、不新增仪器**。**与首轮评审候选的差异(如实披露)**:评审记录的候选 diff 为 +125/−26(候选未提交、已随首轮回滚消失,无法实测比对);落地版净行差(+21/−2)为事件并集断言块扩充与文档注释强化(落地 Read 核实 :1377-1398),注入器门控 / 构造性 hold / release-under-witness 的因果结构与 §C.3 DAG 一致;该差异使首轮 mutation 结果的适用性留有一条限制(见 NEGATIVE_CONTROL 与 LIMITATIONS)。 |
| PROPERTY | live transition 接纳后 from-side `stage` 失败必须经 D11 结算 `Failed`,诊断保持 processing 来源真实(不得伪 decode 标签);pre-pickup steady 失败不得被误读为该性质(vacuity 破除,五路径见 §B.C3);update 递交构造性先于 EOF(排除 EOF-before-update 竞态);pickup 必须取走 pending(丢失 pickup 不得伪装通过)。 |
| AUTHORITY | ADR-PBK-002 D11(terminal-outcome);concurrency-test-design.md 五问契约/前提/负控条款;R1 记录 R0B-05(CONFIRMED_STATIC vacuity)。 |
| PRODUCTION_DAG | 落地形状【git·档案】Read 核实在位(pause 先路由 :1300 → revise :1302 → catch_unwind 守护 :1316 → hold 三见证 :1329-1330 → held 中 request_update :1339 → pending 保留断言 :1341 → request_resume :1349 → `wait_terminal==Failed` :1357 → 事件并集断言 :1377-1398)。完整 DAG 见 §C.3。 |
| TEST_EXECUTED | 【证据包·评审,首轮候选 worktree(+125/−26)】目标测试 honest ×1 pass(0.02s);×100 串行 100/100;lib 套件 183/183(64.93s);60 自旋线程超订阅 ×10(单轮 0.04s);clippy / fmt 干净;vocabulary gate PASS。【本档案会话复跑,落地 HEAD(+146/−24),2026-10-12】`cargo test -p qianqian-playback --lib live_tests::a_failure_during_a_transition_settles_failed_through_d11 -- --exact` → **1 passed 0.01s**;`cargo test -p qianqian-playback --lib` → **183 passed / 0 failed,64.72s**;同目标测试串行 ×20 → **20/20 ok**。本会话未复跑:×100、超订阅、clippy / fmt、vocabulary gate(见 §G 未执行声明)。 |
| TARGETED_TEST_COUNT | 1 |
| RELATED_SUITE_RESULT | lib 183/183【证据包·评审(候选)+ 本档案会话复跑(落地)】。 |
| NEGATIVE_CONTROL | 【证据包·mutation + 评审,首轮候选的 /tmp 沙箱,仓库未动,逐条还原】**baseline 正控:RED=false(诚实实现必须通过——这是正向对照的达标态,不是缺陷;首轮工作流曾误要求其 RED,是为本轮事故一,见 §D.5)** + F1 missing-pickup(`live.rs:533-534` 取走即丢)/ F2 D11 路由吞错(`session.rs:849`)/ F3 伪 decode 标签(`session.rs:857`)/ F4 注入器门恒真(armed-即-失败形状回退)/ F6 pickup 拒收(`live.rs:379`)全部 RED=true、签名匹配;评审者独立复现五条(F4 实测整测 5.00s 完成而非 90s 看门狗)。**限制(如实)**:上述 mutation 执行于首轮候选;落地版与候选的 +21/−2 差异(事件并集断言)不触任何被突变位点(F1/F2/F3/F6 位于生产码 `live.rs`/`session.rs`;F4 位于测试侧注入器门,非生产位点,其落地版支撑另由 §C.3 DAG 的落地 Read 核实),但本档案会话**未在落地版上复跑 mutation**——review 如需可下令重跑。 |
| CLEANUP_VERIFIED | 【证据包·评审】F4 沙箱复现整测 5.00s 完成(非 90s 看门狗)证明 `catch_unwind → runtime.dispose() → resume_unwind` 守护确实释放 parked leg 与阻塞 worker(dispose 经 LIFO:edge.stop+worker.join → release_pause_gate+stop_and_join);`QianqianApp` 无 Drop teardown(评审 grep 证实),故守护是必要的。落地版守护在位(:1316-1351)【git·档案】。 |
| LIMITATIONS | (i) mutation(F1–F4/F6+baseline)执行于首轮候选,落地版未复跑(见 NEGATIVE_CONTROL 限制);(ii) 90s `within()` watchdog 是诊断上界,非公平性证明;(iii) `live_tests.rs` 伴随 H1 信号 ×4(HEAD :860/:870/:1033/:1615,其他测试)未修(→ §F L8);(iv) 每 stage 一次 `tap.events()` 克隆仅 cfg(test) 且决策点事件数 ≤2(评审备注,可接受);(v) 任务指令口径:"DSP Live"(=本行对象,`live_tests.rs` 中 live 处理链 transition 失败路径的目标测试;R2 任务指令内部称谓,仓库文档无此缩写,定义仅此一处)未构造性证明 update-before-EOF 不得 ADEQUATE——**构造性证明已由 hold 建立**(§B.C3/§C.3),故本条限制不再阻塞 §F L4 的 ADEQUATE 提案;(vi) 本地绿(含 ×20)≠ 交错完备证明。 |
| 复审修正 | **证据闭环(2026-10-12 复审)**:限制 (i) 撤销——五条 mutant + baseline 已在最终代码 HEAD `6d5c036` 的隔离 worktree 实际复跑:baseline GREEN;F1/F2/F6 RED 于 `live_tests.rs:1356`(`left: Completed / right: Failed`);F4 RED 于 `:1317`(:5.00s,守护窗口内,非 90s 看门狗;**预测偏差如实记录**:预注册签名 predicted hold 见证断言,实测红在更早的 parked 前提——mutant 使 episode 在 leg park 前即死,判别性质成立);**F3 在收紧前实测存活**(落地断言 `contains("live transition-side failure")` 不含 origin 前缀,origin 伪造不红——探针执行记录见 §H.3)→ 断言收紧为 `contains("processing: live transition-side failure")`(`WorkerFailure::stage()` 的 crate 内 presentation form,completion.rs:141)@ `6d5c036` 后 F3 RED 于 `:1366`。该发现本身即"落地 HEAD 复跑"要求的直接收益:首轮候选的 F3 咬合在落地重写(+21/−2)中丢失,源级核验未捕获,突变复跑捕获。逐条签名表与还原证明见 §H.3;L4 界限 (i) 撤销。 |

### A.C4 — `test(quality): calibrate opt-in adequacy admission checks`

| 字段 | 内容 |
|---|---|
| COMMIT_SHA | `e5a13e4ed2739f016c04e8dbb0581010f1b81180`【git·档案】(cherry-pick 自首轮 C4 提交并 amend:标定文档按本轮复验结果修订;amend 前后变更集 = 同 11 文件,标定文档 +153 → +187) |
| PARENT_SHA | `056a01dab311dcffc2ca5a51fbf645d9603423c3`(C3)【git·档案】 |
| CHANGED_FILES | 11 个新文件共 **+717** 行【git·档案 `git show --numstat`】:`tools/test-quality/check_test_quality_prototype.py`(+371)、`tools/test-quality/README.md`(+81)、8 个 fixtures(+78:h1_bare 9 / h1_poll 12 / h2 10 / oracle_after 5 / oracle_before 5 / ter_malformed 9 / ter_tier_lie 9 / ter_valid_m3 19)、`docs/development/test-quality/r2-checker-calibration.md`(+187;首轮为 +153,amend 增补 = C3 落地后的 §2.6 复验小节 + R0B-05 表述修正 + 新 SHA 引用)。`crates/` 零改动;lefthook / 默认执行链 / CI 零改动。 |
| PROPERTY | 标定性质:负样本(已知缺陷位点)必须触发;正样本(合法惯用法)不得误报(或逐信号 review 驳回);修复敏感度(R2 修复使目标信号消失且无需豁免清单);record/P3 结构校验按设计工作;oracle-diff(H5)仅可作 review 线索。 |
| AUTHORITY | **无正式权威**——R1 候选规范(#248 REV.3 原型)在真实修复样本上的 opt-in 标定;证据等级自记为 PROTOTYPE_EXECUTED(非 CI_ACCEPTED、非仓库规范,`r2-checker-calibration.md:3-7`);机器层边界按 #249 REV.5:启发式只输出 Review Signal,语义裁决与 adequacy_verdict 归 review/台账。 |
| PRODUCTION_DAG | 不适用(标定流程,非并发测试)。流程:BASE(3f139694)导出负样本 → `scan` / HEAD `scan` → `oracle-diff` 成对 → `record` 结构校验(复跑命令见 `r2-checker-calibration.md` §0)。 |
| TEST_EXECUTED | 首轮 15 个标定案例**全部实际执行**(10 TP / 5 FP / 0 FN)且 self-test 6/6 ×2(仓外实物与仓内副本各一次);逐案例运行输出记录于 `r2-checker-calibration.md` §2。【本档案会话复跑,C3 落地后的 HEAD,2026-10-12】checker sha256 `ab704dd3…`(16028 bytes)与首轮记录一致;self-test 6/6;4 个 BASE 负向导出扫描、HEAD 正向扫描(render_gate_seek / pause_seam / live_tests / open_abort / ports)、H5 两对 oracle-diff、record 冒烟——**输出与首轮逐字一致**;live_tests 的 C3 增量单独记录为其 §2.6(H4 @ :1195 仍触发,词法层盲区,review 驳回;伴随 H1×4 行号漂移)。 |
| TARGETED_TEST_COUNT | 不适用(15 标定案例)。 |
| RELATED_SUITE_RESULT | 【证据包·gates】fmt / clippy(-p qianqian-audio-api、-p qianqian-playback,均 --all-targets -D warnings)exit 0;仓库词汇门禁对新文件 PASS。本档案会话未复跑 fmt/clippy/词汇门禁(§G)。 |
| NEGATIVE_CONTROL | 负样本案例本身(NEG1–NEG4):R0B-01 H1@:173、R0B-02 H4@:373、R0B-05 H4@:1150、Seek S6 行 6 H1@:457 四个缺陷位点在 BASE 导出上精确触发,与 R1 信号集逐条一致(独立复现;本档案会话复跑再次一致)。 |
| CLEANUP_VERIFIED | 不适用(无并发 fixture)。边界核验:首轮标定时 `git status --porcelain` 实测仅两个未跟踪目录,commit 后全部入库;本轮 amend 后 `git status --porcelain --untracked-files=all` 为空【git·档案】。 |
| LIMITATIONS | H2/H6/H7 无样本或未实现;H5 首次真实数据运行即 18/18 全为机制性 FP → 生产化前仅 review 线索;H1/H4/H5 词法层无法区分文档化诊断窗口/否定式属性/续行 token 等语义 → 全部信号必须走 review(C3 落地后 H4 不消失即为实例);Skill 发现/触发验证 HARNESS_UNVERIFIED / NOT_RUN;checker 单次运行耗时未精确计时(UNMEASURED;本轮复跑各命令体感 <1s,不作数据引用)。 |

---

## B. Before / After 对比(C1 / C2 / C3)

> 口径:本节静态机制论断(合法值集合、false-RED 链、行号引用)依据证据包(分析/评审席位记录),行号为当时源码快照,可能与当前 HEAD 漂移;执行性证据的字段级标注见 §A。

### B.C1 — render_gate_seek `a_seek_park_publishes_only_seek_events_in_order`

- **原测试验证了什么**:seek park 的 GateEvent 向量恰为 `[SeekEngaged, SeekTailQuiesced, SeekDisengaged]`、`captured == Aborted`、probe 次数落在 `(2..=4)`。
- **原 Oracle 缺口**:(1) **前提缺陷**——`sleep(60ms)` 是"第 2 个 probe slice 已跑、SeekTailQuiesced 已发布"的唯一证据,无任何锁/latch/join/atomic 建立 happens-before(违反 concurrency-test-design.md:14);机制合同明示 wait cap 不 bound scheduling(`ports.rs:300-302`)。(2) **上界无机制依据**——`(2..=4)` 的 3..=4 不可达:probe 被 `!quiesced_published` 守卫(`ports.rs:711`)、flag 于首个 Quiesced 置位(`:713-714`)、闭包恰在第 2 次调用 Quiesce,合法值仅 {1,2};上界区间假装容忍不存在的调度、稀释 oracle,并在"回归出第 3 次 probe"时掩盖。(3) 注释宣称验证"quiescence 后不得重发布",但 sleep 与 release 的先后未给该性质任何确定性窗口。(4) 两个 `wait_until` 失败时直接 assert,不停靠的 parked leg 线程滞留(:79 卫生条款)。
- **修复真实增加了哪条因果边**:`wait_until(5s, ‖ events.snapshot().contains(SeekTailQuiesced))` —— probe#2 返回 Quiesced → `emit`(`ports.rs:715`,在所有 gate 锁之外)→ Events mutex `record` → main 观察到 → 才 release。mutex 交接即真实 happens-before,替代 60ms 定时假设(`render_gate_seek.rs:176-192`【git·档案】在位)。
- **排除了哪个反例**:静态构造的合法 false-RED 链——main 见 parked 后 leg 饿调度 60ms,期间 release 先落(同锁持有内置 payload+清 hold+notify,`ports.rs:467-471`),leg 被调度时 wait 谓词假立即返回、break 先于 probe#2 → `observations==1`、向量缺 `SeekTailQuiesced` → 正确实现双 RED(与 #238 同形态)。该反例被 P2 因果边**从构造上排除**。
- **哪个非法行为能产生预期 RED**(F1–F4 已由 mutation-agent 实际执行,全 RED+签名匹配):F1 必要 quiescence 不发布;F2 pause 事件归属伪造;F3 停探守卫移除(重复 probe/重复 quiescence,25ms 诊断窗口使其确定化);F4 出口漏斗断供(`captured=None`)。
- **哪些行为仍未覆盖**:leg 饿 >5s(watchdog 诊断上界内的 false-RED 残面);probe 形状耦合(见 A.C1 限制);同文件其余测试的既有卫生债与 H3@:477。
- **是否修改生产语义**:**否**——仅测试文件单函数(+34/−8)【git·档案】;生产代码、wait_until/Events/gate_probe 仪器、同文件其余 8 测零改动(评审 diff 核实)。

### B.C2 — pause_seam `stop_from_parked_after_eof_still_completes`

- **原测试验证了什么**:已停靠 pause 后 `request_stop`,终局经 `wait_terminal`(D11 committed Fact)必须为 `Completed`(D14.7)。
- **原 Oracle 缺口**:**post-EOF 前提完全无见证**。`wait_established` 只证 Paused 投影(其建立只需 render leg 首个 park slice + 空设备队列首探即 quiesced,与 decode worker 进度**零因果**——worker 在 activation 独立 spawn);`EofAfter(64)` 短源只是统计大概率,不构成同步边。完整合法 false-RED 链(静态逐行核实,证据包):worker 未被调度 → `request_stop` 的 `edge.stop()` first-wins(Open→Stopped)→ leg 读得 `PcmPull::Stopped` → Aborted → resolve → `Stopped` → `assert_eq(…, Completed)` 在正确实现上变红。二阶缝隙:即便有"Eof 返回"见证,Eof 返回 → `close_eof` 间无同步,`edge.stop()` 理论上仍可先落。
- **修复真实增加了哪条因果边**(两条):(1) `EofWitnessedStream::read_frames` 在 `Ok(DecodeOutcome::Eof)` 处 SeqCst latch → main 有界轮询 `eof.returned()`——证明**生产 worker 的真实 read_frames 返回了 Eof**,不阻塞 worker;(2) Linux 下 `named_thread_gone_within("qianqian-decode")`——线程 OS 退出 happens-after `close_eof` + `worker_exited`(测试侧唯一 happens-after 通道),**关死 stop-vs-close_eof 的 first-wins 缝**(`pause_seam.rs:523-539`【git·档案】在位)。
- **排除了哪个反例**:worker 未被调度时 stop 先落的整条 false-RED 链(Linux 上全链闭合;非 Linux 上残缝见下)。
- **哪个非法行为能产生预期 RED**:F1 Drained×Eof 臂改标 Stopped → 目标测试红;F2 first-wins 破坏(committed Eof 被 stop 覆写)→ `Failed`(对应 ADR"committed EOF 不得改名 / force-abort 不得伪造 Failed{device}"条款;**条款原文 = `docs/adr/ADR-PBK-002.md:2294-2297`,本档案会话已核对**——证据包评审对该条未声明核对,此处为补核);F3 drain verdict 伪造 Aborted → `Failed`;F4 无-EOF stop 被误标 Completed → **新增负控**红(证明目标测试的 `Completed` 断言确系于 EOF 前提,而非接受任何 stopped-from-paused 形状)。
- **哪些行为仍未覆盖**:非 Linux 的 Eof→close_eof 残缝(登记于测试注释 :496-500);`stop_from_an_established_pause…` 的"尚未 EOF"前提;H1@:605→HEAD :800(R0B-03 族)。
- **是否修改生产语义**:**否**——仅测试文件(+200/−5)【git·档案】;`tests/common/mod.rs` 与其余 10 个共享 binary 零触碰;F1–F4 mutation 临时验证后已全部还原(git status 全集 == changedFiles)。

### B.C3 — live_tests `a_failure_during_a_transition_settles_failed_through_d11`(**已落地:056a01da**)

- **原测试验证了什么(名义,BASE 形状)**:live transition 期间的处理失败经 D11 结算 `Failed` 且诊断真实。
- **原 Oracle 缺口(R0B-05,CONFIRMED_STATIC vacuity + false-RED 窗口;修复前五路径逐一核验)**:① 注入器是引擎 INITIAL 侧,armed 后对每个 block 无条件 Err → pre-pickup steady 失败(非 transition-owned)产出与目标路径**同一 Failed+同一诊断串**,测试全绿;② 唯一真正行使性质的路径(接纳后 from-side stage 失败)从不被证明走过(tap 零读取);③ pickup 丢失类 mutant 下无任何 transition,armed steady 仍失败 → 全绿(vacuity);④ update 拒收同理全绿;⑤ EOF-before-update:`EofAfter(352800)` + Consume 是无节流快路径,`position≥5512` 轮询见证不约束 worker 进度——观察到位移时 worker 可已 EOF 结算 `Completed`,arm/submit 落终局后 → `assert_eq Failed` 对诚实实现变红(合法子形态 5a submit 晚于最后一次 poll_update / 5b EOF 读先于后续 stage)。**八秒内容 ≠ 八秒墙钟,无任何 happens-before。**
- **落地修复真实增加的因果边(三条)**:(1) **transition-only 失败门**——注入器改读引擎自身单调事件 tap(同 worker 线程序),仅在"有 `TransitionStarted` ∧ 无 `TransitionCompleted`"时 Err;接纳前门不可能发火,pre-pickup steady 失败**结构性不可表达**(破路径①)。(2) **构造性 hold 替换 position 轮询**——pause 先于 composition revise 路由(:1300),leg 在首个 loop-top park、消费为零;worker 填满边(8×1024=8192)并 stage 第 9 块后阻塞在满边 write;hold 三见证 `paused() ∧ buffered_frames()==Some(EDGE_CAPACITY_FRAMES) ∧ processed_frames()==9*STAGING_FRAMES`(:1329-1330)皆为真同步缝证据,钉死"worker 已越过本轮 pickup、阻塞在满边 write、leg 已 park 不再读"的稳定状态 → worker 距 EOF 尚差 352800−9216 帧(pull decode 无 read-ahead),**update-before-EOF 与 pending 存活成为构造性结论**(破路径⑤的 5a/5b,无时序余量参与)。(3) **release under witness + 事件并集断言**——held 中递交 update(:1339),阻塞中的 worker 构造性不可 take,slot 必须保留(:1341 断言);resume(:1349)后 worker 首个可达 `poll_update` 接纳并记 `TransitionStarted`,紧随的 from-side stage 失败;终局后事件并集断言(:1377-1398):`TransitionStarted` 恰 1、无 `TransitionCompleted`、无 `UpdateRefused`、`pending()` 归空——丢失 pickup(③)与拒收(④)由构造+断言共同钉死。
- **排除了哪个反例**:路径①由门控结构性不可表达;路径⑤(5a/5b)由 hold 构造性排除;③④由 pending 归空/无 Refused 断言 + F1/F6 负控钉死。
- **哪个非法行为能产生预期 RED**(首轮候选沙箱已执行,全 RED+签名匹配;落地版未复跑,见 A.C3):F1 missing-pickup / F2 D11 路由吞错 / F3 伪 decode 标签 / F4 门恒真(armed-即-失败形状回退,hold 建立失败即红)/ F6 pickup 拒收;**baseline 正控 = 诚实实现必须 GREEN(RED=false 即达标)**。
- **哪些行为仍未覆盖**:90s watchdog 诊断上界内的极端饿调度残面;伴随 H1×4(其他测试)未修;每 stage `tap.events()` 克隆的 cfg(test) 成本;交错完备性(见 LIMITATIONS)。
- **是否修改生产语义**:**否**——仅 `src/live_tests.rs`(cfg(test))测试体+注释+删旧 import(+146/−24)【git·档案】;未改生产码、未改共享 fixture、未新增仪器。

---

## C. 最小执行 DAG 与概念澄清

> DAG 口径:who-waits-for-whom / who-unblocks-whom(线程所有权 + 锁/condvar/atomic/日志 mutex 边),
> 引自证据包(实施者绘制、评审者逐跳核验);C.3 已对照落地代码行号核实【git·档案】。

### C.1 C1 修复后(render_gate_seek 目标测试)

```text
main: set_seek_hold(true)                      [先于 spawn]
  ↘ leg: park_loop_top → intent 锁 → seek_hold → emit SeekEngaged
        → wait#1 ≤10ms slice → probe#1: observations=1, Pending, parked:=true
main: wait_until(parked==true)   [watchdog 5s;同步边 = probe#1 内 SeqCst store]
  leg: wait#2 ≤10ms → probe#2: observations=2, Quiesced → emit SeekTailQuiesced → Events.record
main: wait_until(events ∋ SeekTailQuiesced)   [watchdog 5s;同步边 = mutex 发布的事件日志 —— 修复核心]
main: 诊断性 grace 25ms(显式标注 NOT a correctness premise;期间 leg 循环 wait slice,无 probe、无事件)
main: release_seek_hold(Aborted) → notify_all
  leg: wait 谓词假 → 立即返回 → break → emit SeekDisengaged → 出口漏斗消费 Aborted → slice(SeekRelease) → 返回
main: leg.join() → 断言 captured==Some(Aborted) ∧ observations==2 ∧ events==[Engaged, TailQuiesced, Disengaged]
```

无环论证:main 的 release 永可推进(`wait_timeout_while` 释放 intent 锁);leg 的 emit 永可推进(观察者在所有 gate 锁之外调用);main 轮询只在 µs 级持有 seen 锁。失败出口:任一 wait 超时 → **先 release 再 assert**(leg 有界退出,不悬挂)。

### C.2 C2 修复后(pause_seam 目标测试;T=测试线程,W=decode worker,L=render leg)

```text
T: request_pause(激活前) → activate → spawn W + open_stream → spawn L
L: park_loop_top → Engaged → TailProbe(tail==0) → TailQuiesced          [park 前提]
T: wait_established ← paused()==true                                      [投影,前提1]
W: read_frames→Frames(64) → stage → offer → edge.write(64)
W: read_frames→Eof ──latch EofWitness──→ T: wait_until(eof.returned())   [前提2:生产调用返回 Eof]
W: edge.close_eof()(terminal Open→Eof, first-wins)→ worker_exited(Eof) → 线程退出
       └─(/proc)→ T(Linux): named_thread_gone_within("qianqian-decode")  [前提3:关死 stop-vs-close_eof 缝]
T: request_stop [锁内 stop_requested + gate 释放] → edge.stop() = no-op(Eof 已 first-wins)
       └(gate 释放)→ L: 离开 park(Disengaged)→ 读 64 帧 → 再读 PcmPull::Eof
                     → drain.complete(Drained) → resolve(Drained × worker_terminal=Eof) → commit Completed
T: wait_terminal → Completed(D11 committed Fact)
T: dispose() → [edge.stop + join W] → [release gate + stop_and_join L] → snapshot.quiet → no leaked leg threads
```

等待关系:T 等 L(establishment)、T 等 W(EOF/线程退出)、L 在 park 等 T 的 stop 释放;W 不等任何人(EOF 后自行退出)。无可达不可满足等待环:所有被等事件都不依赖断言之后的工作。**非 Linux:前提3 缺席,"Eof 返回 → close_eof"窗口保持开放(登记的限制)。**

### C.3 C3 落地修复的 DAG(056a01da;T/W/L 同上;行号为落地 HEAD 实测)

```text
T: request_pause(激活前,:1300) → revise_desired(:1302) → 真实激活:W、L spawn
W: loop { poll_update(空) → read 1024 → stage(gate 闭→Ok) → write } ×8 → 边满 8192
W: 第 9 块 stage(Ok) → write 在满边阻塞(edge condvar)
L: loop-top park(tail=0→Quiesced)
T: wait paused() ∧ buffered==Some(8192) ∧ processed==9216(:1329-1330)   [hold 三见证 = 真同步缝]
T: request_update(Rock)(:1339) → control 锁:desired=Rock, pending=Some(Rock)
T: assert pending()==Some(Rock)(:1341)   (W 卡在 write,构造性不可 take)
T: request_resume()(:1349) → gate 锁+notify → L 醒 → read 256 → edge 空位 → W write 完成
W: 下轮 loop-top poll_update → take_pending(Rock)[control 锁,真同步] → accept → TransitionStarted 移入 from
W: read 1024 → transition.from.stage(注入器读 tap:Started∧¬Completed → Err)
W: processing_failed(首胜)→ edge.fail → 出口漏斗 worker_exited;L read→Stopped→drain Aborted,leg 退出
T: 事件并集断言(:1377-1398):恰 1×Started;0×Completed;0×Refused;pending 归空
T: wait_terminal==Failed(D11)(:1357)+ 诊断串 processing 来源断言
T: runtime.dispose()
失败清理 DAG(:1316-1351): held 窗口 panic → catch_unwind → runtime.dispose() → release_pause_gate
              → stop_and_join → edge.stop + worker.join → resume_unwind
```

### C.4 五条概念澄清(本三例中各自的实例)

1. **State Graph ≠ Execution DAG**。状态图枚举可达状态,不回答"谁等谁、谁解锁谁"。C2 的核心缺陷正是状态图错觉:"Paused 投影成立"与"worker 已 EOF"是两个独立维度,投影状态可达**不蕴含** worker 进度;只有执行 DAG 的等待边(C.2 前提 2/3)才能表达前提。
2. **release ≠ join**。gate release(`set_paused(false)`+notify)只是解锁;线程真正退出要经 wait 谓词假返回 → break → 出口漏斗 → OS 线程结束,join/线程消失才建立 happens-after。C1 的 `captured` 断言在 `leg.join()` 之后;C2 前提 3 恰恰要用"线程已消失"而非"gate 已释放"来闭合次序;C3 dispose 依赖 join 释放两腿。
3. **watchdog ≠ synchronization**。`within(20s/90s)`、`wait_until(5s)` 只是失败检测的诊断上界,不建立任何 happens-before,也不是公平性证明(concurrency-test-design.md:46/:74)。C1 残余限制 (i) 与 C2 台账更正(Drop teardown 表述)均属此条。
4. **postcondition detection ≠ precondition establishment**。`wait_terminal` 观测到 `Completed/Failed` 只证明终局发生,不证明"`request_stop` 时已 post-EOF"这类**前置**条件;C2 的 `EofAfter(64)` 短源与 C3 的 `position≥5512` 轮询都是把"终局/进度大概率"误当前提,修复分别以 latch+线程退见证据、构造性 hold 建立**前提**。
5. **event presence ≠ 事件仍处活动窗口**。事件日志含 `SeekTailQuiesced` 只证明它**曾**发布;此刻 leg 是否仍 parked 取决于 release 是否已落。C1 修复中 witness 只承担"已发布"前提;"release 时 leg 仍活动"由向量断言中 `SeekDisengaged`(在 release 之后由 leg 亲自发布)与 `captured` 消费断言共同证明。

---

## D. 正负对照

### D.1 #242 正向控制未误伤(open_abort exit witness / ports.rs)

- **POS3-OAB-H3X2**(BASE 上重新纳入):`H3×2 @ :254` 精确重现后**逐信号驳回**——① 块绑定 handle 被 move 进 watcher spawn(`abort_render_thread(leg,…)`)内部 join,测试 2 无 `join_within` 字样故触发;② watcher unbound 且 join 深于闭包一层;结算边界由 `done_rx/exit_rx.recv_timeout(5s)` 断言见证。**未建立任何文件级豁免清单**;另如实记录 test1/test2 不对称(test1 仅因 body 恰含 `join_within` 字样而偶然通过)。
- **POS4-PORTS-SILENT**:ports.rs **0 信号**(真阴性)——合法 Mutex 事件日志轮询、块绑定+join-watcher、`Latch::wait_for`、轮询节流 sleep(open_abort :127-142/:310-325 的 2ms)均不误伤。
- 结论:#242(基线 3f139694 所合入的 PR,commit `640c6f5`/`ef66461`【git·档案】可溯)引入的 gate-event / post-join exit-witness 惯用法**未被 R2 修复与检查器误伤**;本档案会话复跑(2026-10-12)输出一致。

### D.2 Seek S6 带债案例(维持)

- **NEG4-SKS-S6**:BASE 导出上 `H1 @ :457`(`an_unknown_landing_…`)精确触发(该文件共 11 条信号),独立复现 R1 位点;本档案会话复跑再次一致。
- 处置:**owner 裁决带债保留**,R2 未修该位点;Ledger 维持 `LIMITED_EVIDENCE_WITH_PREMISE_DEBT`(§F L5),**不得因 R2 其他测试通过而升级**。
  - **出处与可核验性(如实披露)**:该"owner 裁决"记录于证据包 NEG4-SKS-S6 案例行(expected 字段原文"owner 裁决带债保留")与 `r2-checker-calibration.md` §2.1 NEG-4 行("owner 裁决:带债保留、非确定性充分、无免检先例"),两处一致;**但两处均未提供该裁决的 issue/评论/台账指针,本档案无法进一步溯源**——本档案不将其升格为"已独立核验的事实",留待 review 补指针后核验。

### D.3 三个 R2 修复目标的正负结果汇总(含 baseline 正控语义列)

| 目标 | 正向(执行主体) | 负向 mutation(执行主体) | **baseline 正控语义(baseline-honest)** |
|---|---|---|---|
| C1 | 目标 1 passed 0.05s;文件 9 passed 0.12s;加载复测 220/220;`-p qianqian-audio-api` exit 0【证据包·评审/gates】 | F1–F4 全 RED=true 签名匹配,已恢复【证据包·mutation】;评审静态核验未复跑(如实声明) | 无单独命名的 baseline 运行记录;等效正控证据 = mutation 恢复后套件全绿(修复版测试在诚实实现上 GREEN)。**语义**:baseline(未注入 mutant)必须 GREEN——它验证 oracle 不咬诚实实现;必须 RED 的只有注入 mutant |
| C2 | pause_seam 10 passed;串行 200/200;16-spinner 50/50;`-p qianqian-playback` exit 0【证据包·评审/gates】 | F1–F4 全 RED=true 签名匹配,已恢复【证据包·mutation】;F4 的 oracle 即新增负控本身 | 同上:无单独命名 baseline;恢复后 10 passed 为等效正控 |
| C3(**landed**) | 候选:0.02s ×1、×100 串行、lib 183/183(64.93s)、超订阅 ×10【证据包·评审】;**落地【本档案会话复跑】:1 passed 0.01s、lib 183/183(64.72s)、×20 串行 20/20** | F1/F2/F3/F4/F6 全 RED=true 签名匹配 + baseline **RED=false**,沙箱(/tmp)逐条还原【证据包·mutation+评审】;**落地版未复跑(如实)** | **显式执行并记录:baseline RED=false = 正控达标**(诚实实现必须通过;把 baseline 当作"必须 RED 的负控"是语义颠倒——首轮工作流正是犯此错误,见 §D.5 事故一) |

编号说明:C3 mutant 编号自证据包起即从 F4 直接跳至 F6(**F5 未定义,证据包未记录跳号原因**),本档案按原文保留编号、不虚构 F5 内容。

### D.4 Checker TP/FP 与 Mutation 执行状态

- **Checker(首轮 15 案例全部实际执行,10 TP / 5 FP / 0 FN;本档案会话复跑输出逐字一致)**:四负向位点在 BASE 精确触发(NEG1–NEG4);修复敏感度——两处修复(d0002e47/f5f50a88)使目标信号消失,无需豁免清单;修复引入的合法形状如实触发并交 review(POS1B 25ms 诊断窗口 → FP-H1-1;POS2B 否定式负控 → FP-H4-2);open_abort H3×2 逐信号驳回(POS3);ports.rs 零误伤(POS4);H5 首次真实数据运行两对修复 18/18 触发全为机制性 FP(BASE/HEAD 行 zip 错位、`assert` 子串污染、H5c 基线误取非等待结构字段)→ 生产化前仅 review 线索;H5a 真阴性;record/P3 结构校验 NOTE 正常。H2/H6/H7:无样本或未实现(NOT_RUN 类)。**C3 落地增量(复验)**:live_tests 目标 H4 @ :1195 仍触发——C3 的 transition 见证是结构性事件断言,token 位于 rustfmt 续行且构造性 hold 对词法层不可见,归 FP-H4-1 已知族逐信号驳回(`r2-checker-calibration.md` §2.6);这不是修复敏感度反例,而是"改 oracle 构造性"类修复在词法信号层的如实保留。
- **15 案例构成(档案会话 Read 对照 `r2-checker-calibration.md` §1–§2.5 案例表复核,与证据包 cases 逐条一致)**:CHK-SELFTEST(TP);NEG1-RGS-BASE / NEG2-PSM-BASE / NEG3-LIVE-BASE / NEG4-SKS-S6(TP×4);POS1A 修复敏感度(TP)、POS1B 诊断窗口(FP);POS2A 修复敏感度(TP)、POS2B 否定式负控(FP);POS3 open_abort H3×2(FP);POS4 ports.rs 真阴性(TP);H5J 修复对 oracle-diff(FP)、H5K 修复对 oracle-diff(FP)、H5TN 真阴性(TP);REC1 record 冒烟(TP)——**TP 10 / FP 5 / FN 0**,两份记录对账一致(证据包、首轮标定表、本档案会话复跑输出三方一致)。
- **"18/18 全 FP"与"5 FP"的记账对账(两套口径,不冲突)**:"18/18"是**信号级**计数——H5-J 案例含 8 条 H5b 信号、H5-K 案例含 8 条 H5b + 2 条 H5c = 合计 18 条信号全部为机制性 FP;"5 FP"是**案例级**计数(POS1B/POS2B/POS3/H5J/H5K 五行)。H5-TN 为 TP(真阴性,不含 FP 信号)。对账依据:`r2-checker-calibration.md` §2.4(H5-J ×8;H5-K ×8+×2)与 §3 FP 登记簿(FP-H5-1/FP-H5-2)。
- **Mutation 执行状态**:C1 F1–F4 与 C2 F1–F4 均为【证据包·mutation】实际执行后恢复("已恢复");C3 五条+baseline 为首轮候选的沙箱实际执行后逐条还原(仓库未动),**落地版(+146/−24)上未复跑——落地与候选的 +21/−2 差异(事件并集断言)不触任何被突变位点(F4 在测试侧注入器门,非生产位点),但如实登记,review 可下令重跑**;C4 不适用(标定 commit)。**本档案会话未复跑任何 mutation**;恢复态由【git·档案】佐证:C1/C2/C4 的 commit 变更集 == changedFiles、工作树 porcelain 为空、live_tests.rs 变更集 == C3 commit 的 +146/−24。**复审修正轮(§H)已在最终代码 HEAD `6d5c036` 实际复跑**:C3 五条 mutant(baseline GREEN;F1/F2/F6 RED@:1356、F4 RED@:1317、F3 收紧前探针实测**存活**→断言收紧 @ `6d5c036` 后 RED@:1366)与 C1 两条错误路径 mutant M1/M2(RED 经失败收集路径、`joined=true`),全部隔离 worktree 执行、逐条删除还原(§H 命令记录)。
- **C3 blocker 矛盾(已由事故记录解释,原文留档)**:首轮席位 blocker 字段原文"**负向控制未达成**",但其后粘贴的 mutation 记录为 baseline RED=false(诚实实现通过,正确)+ F1–F6 全 RED=true 签名匹配 + 已恢复。两种读法(结果未达标 vs. 落地分支缺负控)的矛盾,经 §D.5 事故一定位:**blocker 文本源于首轮工作流把 baseline 正控误要求为 RED 的判定 bug,mutation 记录本身才是正确结果**。本档案按原文保留两份记录,以事故记录消解其矛盾,最终裁决权仍在 review。

### D.5 执行事故与处置(首轮 R2 工作流判定事故;诚实记录,不美化)

首轮 R2 工作流在 C3 终检阶段发生两个**判定侧**(judgment)bug,导致 C3 候选被误阻塞并回滚。两者均非被测代码缺陷,而是工作流自身门禁逻辑错误。记录如下,供 review 与后续 campaign 防再发。

**事故一:baseline 正向对照被误要求 RED,导致 C3 被误判"负向控制未达成"并回滚。**

- 现象:首轮终检把"baseline(未注入 mutant 的诚实实现)必须 RED"当作负向控制达标条件;baseline 实测 GREEN(RED=false——这恰是正控的正确结果),被误判为"负向控制未达成",写入席位 blocker 字段,候选修复(+125/−26)被回滚出树。
- 正确语义:baseline 是**正向对照**(positive control)——它验证 oracle **不咬诚实实现**(必须 GREEN);必须 RED 的是 F1–F6 每条注入 mutant(每条破坏一条被测因果边)。要求 baseline RED 等价于要求"正确实现必须失败",是把正控误当负控的语义颠倒。
- 发现:档案会话通读证据包时发现席位 blocker 字段("负向控制未达成")与其后粘贴的 mutation 记录(baseline RED=false + F1–F6 全 RED)相互矛盾(原报告如实保留的矛盾,见 D.4);结合本轮任务指令的复盘结论定位为上述正/负控语义混淆。
- 处置:C3 以 commit `056a01da` 重新落地(因果结构与评审时候选一致;+146/−24 对候选 +125/−26 的差异为事件并集断言与注释扩充,A.C3 已披露);§D.3 负控表增设 **baseline 正控语义列**,把"baseline RED=false = 正控达标"固化为表内语义;§F 判据声明同步写明;本小节留档。
- 防再发:正控/负控语义进入本报告判据(§F 头部)与 PR 正文 Independent Review Checklist("baseline 必须 GREEN,mutant 必须 RED"分列核验)。

**事故二:白名单终检误用 porcelain 解析。**

- 现象:首轮终检的"变更文件必须 ⊆ 白名单"检查以不当方式解析 `git status --porcelain` 输出。porcelain 行格式为 `XY <path>`(XY 为两字母状态列;rename 行为 `R  old -> new`;未跟踪条目为 `?? path`;含空格路径带引号),按空白切分取首 token 当路径、或不处理 rename/untracked 形状的解析,会把状态列/旧路径/引号误当文件名,产生错误的通过/不通过判定。
- 影响:该检查的判定结果不可信(可能误放行越白名单文件,也可能误阻塞合法变更集)。本轮任务指令将其与事故一同列为首轮两个工作流判定 bug(**出处 = 本轮任务指令/campaign 交接记录;首轮工作流检查器源码不在本仓库,该缺陷无法在本仓复现执行——如实声明,非本仓可验证事实**)。
- 发现:首轮收尾复盘(交接本轮时点名);本档案会话无法检视首轮检查器源码,只能记录缺陷类别与影响面。
- 处置:本轮(stitch/档案会话)等价检查改用显式 git 查询并留 §G 命令痕迹:`git status --porcelain --untracked-files=all`(整体必须为空)、`git diff --numstat` / `git diff --cached --numstat`(必须为空)、`git show --numstat`(逐 commit 变更集对账)、`git diff --stat BASE..HEAD -- <file>`(单文件对账)——不依赖对 porcelain 行的二次解析;本小节留档。
- 防再发:凡工作流门禁需要"变更文件清单",一律用 `git diff --name-only <base>`(含 rename 规范化)或 `git status --porcelain -z`(NUL 分隔)解析,禁止按行空白切分。

---

## E. 成本

| 项 | C1 | C2 | C3(landed) | C4 |
|---|---|---|---|---|
| 目标测试实测耗时 | 0.05s(--exact,评审实测);文件 9 测 0.12s | 未记录单测耗时(UNMEASURED);10 passed | 候选:0.02s、×100 串行、lib 183 测 64.93s、超订阅单轮 0.04s(评审实测);**落地复跑:0.01s、lib 64.72s、×20 串行(本档案会话)** | — |
| 加载/复测成本 | 220 次(taskset 16-19 + 8 busy-loop) | 串行 200/200(证据包原文,各 200 或合计 200 未写明)+ 16-spinner 50/50 | 候选:60 自旋 ×10;**落地复跑:×20 串行** | — |
| Checker 运行耗时 | — | — | — | 首轮 15 案例 + self-test ×2 实际执行;复跑全量一次(本档案会话);单次耗时均未精确计时(UNMEASURED) |
| 相关套件增量 | render_gate_seek 9 测(原位修改,数量不变) | pause_seam 9 → 10(+1 负控) | lib 183(候选与落地两次全绿) | crates/ 零改动;fmt+clippy 两包 exit 0(首轮 gates) |
| 新增代码 | +34/−8(1 测试文件)【git·档案】 | +200/−5(1 测试文件)【git·档案】 | +146/−24(1 个 cfg(test) 文件)【git·档案】 | +717 / 11 新文件(tools 8 + docs 1 + README 1 + 原型 1)【git·档案】 |
| 共享 fixture 影响 | 零(单文件单函数;其余 8 测原样) | `tests/common/mod.rs` 零触碰(计划偏差:委托式 double 放 pause_seam.rs 内,11 个共享 binary 零影响可证) | 零(同样不触 common/mod.rs) | 无 fixture 语义影响 |
| 新增依赖 | 无 | 无 | 无 | 无(原型为 stdlib Python 3 正则+括号配平;crates/ 依赖零变化) |
| 审查耗时 | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED(证据包无任何时间记录) |
| 新误报/不可复现风险 | watchdog 残面;probe 形状耦合 | 非 Linux 残缝;funnel 形状依赖 | 落地版 mutation 未复跑(+21/−2 差异不触被突变位点,A.C3);90s watchdog 残面 | FP-H1-1/FP-H4-2/H5 机制性 FP;H1/H4 词法层无语义分辨力 |

CI 成本:无新 CI Job;lefthook 与默认执行链未动 → CI 增量仅为常规 Rust 测试时间(未另行测量,UNMEASURED)。

---

## F. Ledger(按 #249 REV.5 词汇整理 — **提案行,待 review 采纳/驳回**)

> 判据声明:只有**明确满足完整性质与证据要求**的行才可归入 ADEQUATE——此处操作化为:
> 性质陈述完整(含否定式伴随)、前提经真实同步缝构造性建立、oracle 精确且有机制依据、
> 负向控制实际执行且 RED 签名匹配、**baseline 正向对照 GREEN(不咬诚实实现)**、失败清理
> 路径验证、门禁绿、独立评审通过。任何 ADEQUATE 也仅在所述界限内成立,**不是并发交错
> 完备性证明**。

| 行 | 对象 | verdict(提案) | 关键依据 | 债务 / 条件 |
|---|---|---|---|---|
| L1 | C1 目标测试 `render_gate_seek::a_seek_park_publishes_only_seek_events_in_order` @ d0002e47 + 修正 @ 4cdb52ba | **ADEQUATE** | 前提经 mutex 事件日志同步缝见证;oracle 精确(事件向量 + `observations==2` 机制真值);F1–F4 负控执行(RED+签名+恢复);失败路径 release → **有界 join → 带收集证据 panic**(复审修正,M1/M2 实测 RED 且 `joined=true`,§H.1);220 次加载复测;评审 APPROVED | 界限注记(非证据债):5s watchdog 诊断上界;`observations==2` 与 probe 形状耦合 |
| L2 | C2 目标测试 `pause_seam::stop_from_parked_after_eof_still_completes` @ f5f50a88 + 修正 @ bad486b3 | **ADEQUATE(提案更新;执行域 = Linux)** | Linux 前提链闭合(eof latch + 线程退见证据,闭合 close_eof-vs-stop first-wins 缝);F1–F4 执行;评审 APPROVED;复审修正后非 Linux 不再运行不可靠断言(测试 `cfg(target_os="linux")` 限定,doc 注释诚实登记) | **执行域限定**:非 Linux 该性质 **NOT_RUN(非 PASS)**;可移植提交点见证需生产侧 seam(`PcmEdge` crate-private、`RenderPcmInput` 无 terminal 查询),复审裁决试点不新增产品 API;Windows 交叉编译冒烟 = 编译级证据,非运行 |
| L3 | C2 新增负控 `pause_seam::stop_from_parked_without_eof_settles_stopped` @ f5f50a88 | **ADEQUATE** | 否定式属性确定性(Paced 永不 EOF ⇒ `Stopped` 结构唯一可达);F4 mutation 咬合(RED) | 维护注记:与 resolver 臂形状(completion.rs:1324-1326)耦合 |
| L4 | C3 目标测试 `live_tests::a_failure_during_a_transition_settles_failed_through_d11` @ 056a01da(**已落地**)+ 断言收紧 @ 6d5c036 | **ADEQUATE** | **构造性 update-before-EOF 成立**:hold 三见证(`paused() ∧ buffered==EDGE_CAPACITY(8192) ∧ processed==9×STAGING(9216)`,`live_tests.rs:1329-1330`)钉死 worker 阻塞在满边 write、距 EOF 差 352800−9216 帧,held 中递交 update 且 pending 保留(:1339/:1341),resume 后首个可达 `poll_update` take——递交先于 EOF 是构造结论,非时序余量;五路径 vacuity 破除(§B.C3);失败清理 panic-guard 实证(F4 5.00s 非 90s 看门狗);**落地 HEAD(`6d5c036`)突变复跑(§H.3)**:baseline GREEN + F1/F2/F3/F4/F6 全 RED(F1/F2/F6@:1356 Completed/Failed、F3@:1366 decode: 前缀、F4@:1317)——复跑并实际修复了落地版 F3 咬合丢失(断言收紧);落地复跑 1 passed / lib 183/183 / ×20 串行【本档案会话】 | 界限:(i) ~~mutation 执行于首轮候选,落地未复跑~~ **已撤销**(§H.3 复跑;且发现并修复 F3 落地咬合丢失——复跑要求的直接收益);(ii) 90s watchdog 诊断上界;(iii) 伴随 H1×4(L8)不随本行升级;(iv) 本地绿 ≠ 交错完备证明 |
| L5 | Seek S6 行 6 位点(`seek_seam.rs` H1@457 @ BASE) | **LIMITED_EVIDENCE_WITH_PREMISE_DEBT(维持)** | owner 带债裁决(出处=证据包 NEG4 + `r2-checker-calibration.md` §2.1 NEG-4,均无 issue 指针,溯源状态见 §D.2);NEG4 于 BASE 独立复现位点仍触发(含本档案会话复跑) | **不得因 R2 其他测试通过而升级**;需专项修复重评 |
| L6 | `render_gate_seek` H3 channel-witness unbound leg(HEAD :477) | DEFERRED | NEG1 伴随信号;既有形状 | 随卫生债批次处理 |
| L7 | `pause_seam` H1@:605→HEAD :800(R0B-03 族) | DEFERRED | NEG2 伴随;R2 未修 | — |
| L8 | `live_tests` 伴随 H1 ×4(HEAD :860/:870/:1033/:1615) | DEFERRED | NEG3 伴随;R2 未修 | 属其他测试的既有债,与 L4 的 ADEQUATE 相互独立 |
| L9 | `pause_seam::stop_from_an_established_pause…` 的"尚未 EOF"前提无 witness | DEFERRED | 评审备注 (d) | 既有债,超出 C2 范围 |
| L10 | `render_gate_seek` 其余测试 6 处裸 `assert!(wait_until)` 无失败 release | DEFERRED | 评审备注 (iii) | 既有卫生债 |
| L11 | 检查器 H5(oracle-diff) | DEFERRED | 首次真实数据:两对修复 18/18 全机制性 FP(复跑一致) | 生产化前仅 review 线索 |
| L12 | 检查器 H2 / H6 / H7 | DEFERRED | 无样本 / 未实现 | — |
| L13 | Skill 发现 / 触发验证 | DEFERRED | HARNESS_UNVERIFIED / NOT_RUN | 无头环境无可验证的安装/触发面 |

---

## G. 本档案会话实测命令与未执行声明

**已执行(全部在 /home/hoo/Source/qianqian-r2;首轮档案命令与复跑/stitch 会话命令分列)**:

首轮档案会话(2026-10-12):

```bash
git -C /home/hoo/Source/qianqian-r2 log --format='%H|%P|%s' -8
git -C /home/hoo/Source/qianqian-r2 branch --show-current
git -C /home/hoo/Source/qianqian-r2 rev-parse origin/main HEAD
git -C /home/hoo/Source/qianqian-r2 status --porcelain --untracked-files=all   # 空
git -C /home/hoo/Source/qianqian-r2 diff --numstat && git -C /home/hoo/Source/qianqian-r2 diff --cached --numstat  # 空
git -C /home/hoo/Source/qianqian-r2 show --numstat --format='%H parent=%P%n%s' d0002e47 / f5f50a88 / 056a01da / e5a13e4e
git -C /home/hoo/Source/qianqian-r2 diff --stat 3f139694..HEAD -- crates/qianqian-playback/src/live_tests.rs  # +146/−24(C3 在树)
git -C /home/hoo/Source/qianqian-r2 ls-tree -r --name-only HEAD -- docs/development/test-quality tools/test-quality
grep -n "D14" docs/adr/ADR-PBK-002.md | head -40                          # D14.7 节首 :2094
grep -n "Stop-from-paused after decode EOF" docs/adr/ADR-PBK-002.md       # :2294
grep -n "force-abort\|Failed{device}" docs/adr/ADR-PBK-002.md             # :2296-2297
```

复跑 / stitch 会话(2026-10-12 同日;C4 cherry-pick+amend 与 C5 cherry-pick 之后):

```bash
# patch-id 等同核验(C1/C2 与首轮提交正文零差异)。注:命令中保留首轮链的原始 SHA——
# 这是本会话实际执行的命令原文(改写即伪造审计记录);核验时旧链经移交前的
# test/r2-adequacy-pilots 分支可达,分支移交后旧链仅存于 reflog/对象库,新克隆不可解析。
git show f4597982 | git patch-id --stable && git show d0002e47 | git patch-id --stable   # 同 id db05a95b…
git show 735328ec | git patch-id --stable && git show f5f50a88 | git patch-id --stable   # 同 id 50f5bfb7…
# checker 全套标定复跑(C3 落地后 HEAD)
git show 3f139694…:crates/{…}/render_gate_seek.rs|pause_seam.rs|live_tests.rs|seek_seam.rs > /tmp/r2-neg/*.rs
sha256sum tools/test-quality/check_test_quality_prototype.py   # ab704dd3…(16028 bytes,与首轮一致)
python3 tools/test-quality/check_test_quality_prototype.py self-test                       # SELF-TEST OK (6/6)
python3 tools/test-quality/check_test_quality_prototype.py scan /tmp/r2-neg/{四文件}        # NEG1–NEG4 逐条一致
python3 tools/test-quality/check_test_quality_prototype.py scan crates/qianqian-playback/src/live_tests.rs
                                                            # H4@:1195 + H1×4(:860/:870/:1033/:1615)= 标定 §2.6
python3 tools/test-quality/check_test_quality_prototype.py scan crates/qianqian-audio-api/tests/render_gate_seek.rs   # H1@:199+H3@:477
python3 tools/test-quality/check_test_quality_prototype.py scan crates/qianqian-playback/tests/pause_seam.rs           # H4@:429/:561/:601+H1@:800
                                                            # 信号映射:H4@:561=POS2B/FP-H4-2(否定式负控);H4@:429/:601=两个对照测试
                                                            # (eof_while_parked/failure_while_parked)既有 H4 形状,R2 未触碰(既有债);
                                                            # H1@:800→L7(R0B-03 族);render_gate_seek H1@:199=POS1B/FP-H1-1、H3@:477→L6
python3 tools/test-quality/check_test_quality_prototype.py scan crates/qianqian-output-wasapi/src/open_abort.rs         # H3×2@:254
python3 tools/test-quality/check_test_quality_prototype.py scan crates/qianqian-audio-api/src/ports.rs                  # 0 信号
python3 tools/test-quality/check_test_quality_prototype.py oracle-diff /tmp/r2-neg/render_gate_seek.rs crates/qianqian-audio-api/tests/render_gate_seek.rs  # H5b×8
python3 tools/test-quality/check_test_quality_prototype.py oracle-diff /tmp/r2-neg/pause_seam.rs crates/qianqian-playback/tests/pause_seam.rs                # H5b×8+H5c×2
python3 tools/test-quality/check_test_quality_prototype.py record tools/test-quality/fixtures/ter_valid_m3.md crates/qianqian-audio-api/src/ports.rs         # 2×NOTE,exit 0
# C3 落地复跑
cargo test -p qianqian-playback --lib live_tests::a_failure_during_a_transition_settles_failed_through_d11 -- --exact
                                                            # 1 passed 0.01s
cargo test -p qianqian-playback --lib                       # 183 passed / 0 failed,64.72s
(同 --exact 目标)×20 串行                                  # 20/20 ok
```

另以 Read/grep 抽段核实:`render_gate_seek.rs:148-220`(C1 修复体)、`pause_seam.rs:480-609`(C2 修复体+负控)、
`live_tests.rs:1190-1400`(C3 落地体,含 :1300/:1302/:1316/:1329-1330/:1339/:1341/:1349/:1357/:1377-1398)、
`concurrency-test-design.md:1-85`、`r2-checker-calibration.md` 全文(含修订后 §2.6)。

**未执行(如实声明)**:本档案(含复跑)会话**未运行**:任何 mutation(含 C3 落地版)、
`cargo fmt` / `cargo clippy`、Lefthook pre-push、C1/C2 的目标/集成测试复跑(pause_seam 等 tests/ 下
集成测试)、×100/超订阅加载复跑、词汇门禁、非 Linux 平台验证。文中所有此类结果均引自证据包并
标注原执行主体。首轮事故二所指工作流检查器源码不在本仓库,无法复现执行(§D.5)。
**复审修正轮的执行/未执行边界见 §H.5**(该轮已实际运行 mutation 与 fmt/clippy/目标与集成测试;
仍未运行:×100/超订阅加载复跑、Windows/macOS 运行时、CI 复查在推送后进行)。

---

## H. 复审修正轮记录(2026-10-12;PR #250 独立复审 REQUEST_CHANGES 的响应)

> 复审三项要求:P1/C1 失败路径 JoinHandle 静默 detach;P1/C2 非 Linux 无条件 `Completed` 断言的
> false-RED oracle;C3 落地 HEAD 无已执行突变证据(候选上的 F1–F6 不能替代)。响应 = 三个修正
> commit + 本节证据。**未合并、未关闭 #244/#247;台账仍为提案行。**

### H.1 P1/C1 修复 — `test(audio-api): collect the parked leg before a failed premise`(`4cdb52ba`)

- 修复:两失败分支(parked 前提 / quiescence 发布前提)改为调用 `fail_premise_releasing_leg`:
  `release_seek_hold(Aborted)` → `wait_until(5s, leg.is_finished())` → `join()` → panic 携带
  `exited_within_5s`/`joined` 收集证据(有界清理策略:超界未退出的 leg 在 panic 信息中**具名**,
  不再静默 detach)。RELEASE ≠ JOIN 由结构化 join 收口;正常路径断言零变化。
- 错误路径验证(隔离 worktree @ `6d5c036`,逐条删除还原):
  - **M1** `set_seek_hold` 失效(leg 永不 park)→ RED @ `render_gate_seek.rs:164`:
    `premise failed: the leg never parked under the routed hold within 5s; leg collection: exited_within_5s=true, joined=true`,5.00s 有界完成。
  - **M2** 删 `ports.rs` 的 `SeekTailQuiesced` emit(flag 保留)→ RED @ `:164`:
    `premise failed: the seek park never published its tail quiescence within 5s; leg collection: exited_within_5s=true, joined=true`,5.02s。
  - 两者均为预注册签名(执行前书面锁定),实测逐字匹配;睡眠未放大(join 尝试的 5s 上界与全文件
    `wait_until` 同口径,非 sleep 前提)。
- 诚实基线:`render_gate_seek` 全文件 9/9、目标 ×20 串行 20/20 @ `4cdb52ba`(该文件其后未再变更)。

### H.2 P1/C2 修复 — `test(playback): scope the post-EOF stop oracle to linux`(`bad486b3`)

- 裁决依据(复审原文):可移植因果提交/退出见证,**或** 将该测试的声明/实际执行限定 Linux、其他
  目标诚实 NOT_RUN;不得为试点新增产品 API。源级核实:`PcmEdge`/`EdgeTerminal` crate-private
  (playback lib.rs:41-44),ports 缝 `RenderPcmInput` 仅 `read_frames`+`stop`、无 terminal 查询
  → 可移植提交点见证不存在现成缝,新增即产品 API。**取限定路线**。
- 修复:测试整体 `#[cfg(target_os = "linux")]`;原内层 Linux cfg 块展开为无条件;doc 注释改写为
  "非 Linux = NOT_RUN(非 PASS)" 的诚实登记。**(复审后更正,见后续小 commit)**:初版曾对
  `registered_runtime_with_decode`/`EofWitness::returned` 各加
  `#[cfg_attr(not(target_os="linux"), allow(dead_code))]`;复审指出前者经 `registered_runtime`
  **全平台可达**、豁免从不触发(且注释理由错误)——真正需要豁免的只有 `EofWitness::returned`
  (其唯一读者即 Linux 限定的目标测试);已在复审后小 commit 中移除前者并更正注释,
  该更正不触任何执行证据引用的 SHA。
  no-EOF 负控保持跨平台(其 oracle 无 EOF 前提,`Stopped` 结构唯一可达,交错完备)。
- 证据:pause_seam 全文件 10/10、目标 ×20 串行 20/20 @ `bad486b3`;
  `cargo check -p qianqian-playback --tests --target x86_64-pc-windows-gnu`(及 audio-api)exit 0
  ——**编译级冒烟,不构成 Windows 运行证据**;Windows 运行时验证仍属 CI/平台证据边界。

### H.3 C3 证据闭环 — 落地 HEAD 突变复跑 + F3 咬合丢失的发现与修复(`6d5c036`)

**预注册签名表(执行前书面锁定)与实测对照**(全部隔离 worktree @ `6d5c036`,目标测试
`live_tests::a_failure_during_a_transition_settles_failed_through_d11`,逐条删除还原,
`git worktree list` 终态无本轮沙箱、主树 porcelain 空):

| mutant(位点) | 预注册签名 | 实测 | 判定 |
|---|---|---|---|
| baseline(诚实) | GREEN(正控) | 1 passed(×20 串行 20/20,lib 183/183) | ✓ |
| F1 take-and-drop(live.rs `poll_update`) | RED@wait_terminal,Completed/Failed | `:1356 left: Completed / right: Failed`,0.03s | ✓ 匹配 |
| F2 stage 错误臂吞掉(session.rs:849 区) | 同上 | `:1356 left: Completed / right: Failed`,0.02s | ✓ 匹配 |
| F3 origin 伪造(session.rs stage 路由 processing_failed→decode_failed,单点位) | (收紧后)RED@diagnostic 断言,decode: 前缀 | **收紧前探针实测:GREEN(mutant 存活)**;断言收紧 @ `6d5c036` 后 RED@`:1366 …decode: live transition-side failure` | ✓ 匹配(收紧后) |
| F4 注入器门恒真(live_tests.rs gate) | RED@hold 见证,≈5s 非看门狗 | RED@`:1317 the pre-routed pause never parked the leg`,5.00s(非 90s 看门狗) | RED 达成;**预测偏差如实记录**(见下) |
| F6 pickup 拒收(live.rs `accept`) | RED@wait_terminal,Completed/Failed | `:1356 left: Completed / right: Failed`,0.02s | ✓ 匹配 |

- **F4 预测偏差**:预注册预测红在 hold 见证断言;实测红在更早的 parked 前提(:1317)——门恒真使
  首个 steady block 立即失败、episode 在 leg park 之前死亡。两者同为 catch_unwind 守护窗口内的
  前提断言,判别性质(≈5s 完成、非 90s 看门狗、守护释放两腿)成立;偏差属预测错误,非修复缺陷。
- **F3 咬合丢失(本轮实质发现)**:落地版断言 `contains("live transition-side failure")` 不含
  origin 前缀,而 `failure_diagnostic` = `WorkerFailure::stage()` 的 `processing: `/`decode: ` 前缀
  形式(completion.rs:141/:1287-1289)→ origin 伪造在落地版**不红**(首轮候选的 F3 RED 记录
  对应候选版更强断言;落地重写 +21/−2 丢失该咬合,源级核验未发现,探针突变捕获)。分类:测试
  oracle 缺口,非生产缺陷;处置 = 断言收紧为 `contains("processing: live transition-side failure")`
  (crate 内白箱断言 presentation form,D14.11 origin 真实性),收紧后 baseline 仍 GREEN、F3 RED。
- 操作性备注(如实):F2 首次执行的锚文本笔误致脚本断言失败、沙箱遗留;重试在遗留沙箱(同 HEAD)
  完成,并以全新沙箱复跑取得完整签名行(F2b)——两次均为 `6d5c036` + 同一 mutant 编辑。

### H.4 Checker 复扫(修正后 HEAD)

`scan` 三文件 @ `6d5c036`:信号族与既往裁定一致、仅行号漂移——render_gate_seek H1 :199→**:221**
(POS1B 诊断窗口,FP-H1-1 族)、H3 :477→**:499**(L6);pause_seam H1 :800→**:812**(L7)、
H4 :429/:561/:601→**:434/:573/:613**(两个既有形状 + POS2B 否定式负控 FP-H4-2);live_tests
H4@:1195 与 H1×4(:860/:870/:1033/:1615)行号不变(+1/−1 等长变更)。无新信号族、无信号消失
(C3 目标 H4@:1195 仍在,词法层盲区,FP-H4-1 族维持)。逐信号处置沿用 §D.4/标定文档既有裁定。

### H.5 修正轮执行/未执行边界(如实)

- **已执行(修正会话,2026-10-12)**:上列全部 mutation(baseline/F1/F2/F3×2/F4/F6/M1/M2,隔离
  worktree,逐条还原);render_gate_seek 9/9、pause_seam 10/10、C3 目标 ×1+×20、playback lib
  183/183(65.02s);`cargo fmt --check`、`cargo clippy -p qianqian-audio-api -p qianqian-playback
  --all-targets -- -D warnings`;checker scan 三文件;Windows gnu 目标 `cargo check --tests`(编译级);
  commit 头规范检查 `--range origin/main HEAD` 全 PASS;全量 Lefthook pre-push(见 PR)。
- **未执行**:×100 / 超订阅加载复跑(本轮以 ×20 串行为稳定性证据,弱于首轮候选的 ×100/超订阅
  ——如实降格登记);Windows/macOS **运行时**验证(交叉编译仅为编译级);Loom/Miri/TLA+(不在
  本 PR 范围);exact-HEAD CI 结果于推送后复查(PR 正文记录)。
