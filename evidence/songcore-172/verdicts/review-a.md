# Review A — Build/Toolchain 独立评审（#172 PR-B windows release lane）

- 评审角色: Review A（Build/Toolchain），全新视角、只读；本文件为本次评审唯一新增产出
- 日期: 2026-10-29
- 战场: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`，HEAD `b461623`（8 提交
  `40ae32a`/`bf23516`/`62acc29`/`56b9a04`/`6acb25c`/`423e37a`/`6c52687`/`b461623`；
  `git log`/`git status` 实测，tracked 干净、仅 untracked evidence）
- 材料基线: `verdicts/` 全部 8 文件逐字读毕；契约
  `docs/release/songcore-release-contract.md`（822 行）全文读毕；下述每项
  核查均为本会话实际执行的命令或亲自读过的文件。未重跑任何大型构建（按
  ask 约束）；所有"通过"仅指本次实际执行过的检查。

## 结论

**APPROVE**（Build/Toolchain 视角）。必查九项全部以硬证据核查通过，
未发现伪造证据、假 PASS、门禁失效或职责越界；findings 均为 P2/P3 级
流程与文档缺口，不动摇证据链本身。

## 必查项核查（逐项，含实测命令）

### 1. D1 五反事实的真实性 + 修复的平台无关性 — PASS（实测）

- 断言总数逐文件累加实测：windows red **35**、windows green **47**、
  linux red **35**、linux green **47**（`grep -c "ASSERTION PASS"
  d1-counterfactual/results/{windows,linux}-{red,green}-*.txt`）——与
  README/commit/PR 正文声称的 "red 35/35、green 47/47 × 双宿主" 逐位一致。
- 抽读 `windows-red-case-a-backslash.txt`（RED 签名
  `QN_SONGCORE_HEADER_SHA256=\cf509a5c…`，payload==coreutils==hashlib
  双 oracle）、`windows-green-case-a-backslash.txt`（干净 64-hex）、
  `linux-green-case-d-no-sha256sum.txt`（PATH 剥空后仍完成且摘要正确），
  内部结构（probe exit code、oracle 段、断言行）一致、无矛盾。
- **缺陷机制本会话一手复现**：Git Bash 实测
  `sha256sum "C:\Users\fred1\source\qianqian-172\native\include\songcore.h"`
  → 输出 `\cf509a5c… *C:\\Users\\…`——coreutils 对反斜杠路径的转义前缀
  行为与 RED 案例记录完全一致。
- 修复平台无关性：`crates/qianqian-songcore-sys/build.rs` 为 std-only
  进程内 FIPS 180-4 SHA-256（`sha256_self_test()` 在 `main()` 首行，
  :98/:156），无外部工具调用、无平台分支；双宿主 harness 摘要逐字节一致
  （header `cf509a5c…8536` 与本会话磁盘实测头哈希同值）。case d 的语义
  变化（无 sha256sum 由 panic 转 GREEN）在 README/PR 正文如实记录。
- 覆盖面：case a/b/c/d + e(s1–s5) 双宿主 results 文件齐备（40 个，
  与 PR 正文计数一致）；WSL 侧 scratch 在 `/home/hoo/d1-harness`（WSL
  本地 fs，规避 /mnt/c mtime 漂移，README caveat 如实）。

### 2. D2 artifact policy 裁定 + Xmake 语义结论 — PASS（实测）

- recipe 与裁定一致：`native/ffmpeg/targets/windows-x86_64.json`
  `artifacts {static:true, shared:false, wasm:false}`（static-only 有据，
  契约 §6）；`linux-x86_64.json` `{static:true, shared:true}`。
- `native/build/songcore.lua` 逐行读：双 form 的 `before_build` 门
  （:120–126/:151–157）+ phony 在 `on_config` 解析依赖（:180–200）+
  `songcore_artifact_gate` 失败封闭终站（:206–227），与 bf23516 提交
  说明一一对应。
- Linux 不回归实测证据：`evidence/songcore-172/build/{prefix,postfix}-artifacts.txt`
  三件产物 sha256 逐位相同（`2a4a9daf…`/`2b4dd91d…`/`1b9a7e2b…`）、
  `{prefix,postfix}-so-songcount.txt` 均 15——前后缀字节一致成立。
- 负控两份实测在档：`red-windows-songcore-build.log`（修复前 DLL 链接
  死于 random_seed.c.obj 的 BCrypt×3 未定义引用）+
  `green-shared-explicit.log`（修复后 `songcore_shared` 被拒，文案与
  songcore.lua:95–101 格式串逐字一致）。
- **Xmake 语义结论有真实实验在档**：`evidence/songcore-172/semantics/`
  四份输出中，`sem-matrix-output.txt` p2 实证 on_load 错误毒化一切后续
  命令（连 `xmake build no-such-target` 都 exit 255），p1 实证
  before_build raise 只打击冒犯目标；`sem8-onconfig-output.txt` 实证
  on_config 动态依赖被 build planner 接受且配置缓存路径可用；
  `sem9-norecipe-output.txt` 实证无 recipe 会话的 gate 文案与正控。

### 3. WSL/Windows 职责隔离（无 /mnt/c 编译） — PASS（实测）

- `ops/build-rc.ps1` stage 守卫（:275–278、:550–557）：工作根/第二根
  必须 `/home/*|/root/*|/tmp/*`，`C:\` 形参经 `Convert-ToWslPath` 变
  `/mnt/c/...` 后会被守卫 exit 11 拒绝——/mnt/c 编译在编排器内结构性
  不可达。实际构建根 `/home/hoo/q172rc`、第二根 `/home/hoo/q172rc-s2pkg`
  （`build/rc/20261029-062848/summary.json` 实读）。
- S3 侧：构建/配置/fetch 日志 `/mnt/c` 计数全为 0
  （`grep -c "/mnt/c" linux/build/20261028T200502Z/{xmake-build,xmake-config,fetch-ffmpeg}.log`
  ——该目录为本地复跑输出，2026-11-01 后 git-ignored
  → 0/0/0）；构建树 `/home/hoo/qianqian-172-linux`；s3-verify cargo 以
  `CARGO_HOME=/home/hoo/.cargo-s3` + cwd 全程 /home/hoo（s3-verify §2）。
- 唯一的 /mnt/c 触碰均为**读**：clone 源（`file:///mnt/c/...`，
  `run1-derive/clone-source.txt` 如实记录直克隆失败现场）与产物
  copy-back/对比读（`package/repro/identity.txt`）。编译零触碰。
  产物 copy-back 只读入 worktree 消费路径，不改变"编译在 ext4"的事实。

### 4. SHA 一致性链 — PASS（实测）

- 本会话重算：`sha256sum native/build/artifacts/libsongcore.a` →
  `1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402`；
  `sha256sum native/include/songcore.h` → `cf509a5c…78536`（== 契约 §3
  冻结值）。
- 五 verdict 同值核验：`grep -rl "1f5f8cf8" verdicts/*.md` →
  s2-build/s2-csuite/s2-binding（全值）+ s2-package/s2-external（缩写
  `1f5f8cf8…`），五者同值零冲突。s3 两文件指向 Linux 侧不同产物
  （409ef3da…/5e9c5697…），不在此链内，正确。
- 产物独立审计（WinLibs binutils 本会话实跑）：`objdump -f` →
  **280** 成员 pe-x86-64、**0** elf；`nm` → **15** 个 ` T song` 符号
  （集合与契约 §3 完全一致）；`U BCrypt*` 恰 3 项——与 s2-build §1、
  契约 §9 全部吻合。
- 包内绑定：`evidence/songcore-172/package/package-summary.json`
  `artifact_sha256` 同值；binding gate 正证 receipt 首行同值。

### 5. ops 脚本退出码传播与 fail-closed — PASS（五脚本全部逐读）

- `build-rc.ps1`（677 行）：stage 退出码 10–17 全部有判定点；
  `Invoke-WslStage`（:138–153）原样传播 `$LASTEXITCODE`，非零即
  Fail→exit 1；bash 侧 `set -euo pipefail`；grep -c 的 exit-1 陷阱用
  `set +e/-e` 显式挂起后断言值（:381–393）；`songcore_shared` 负控
  exit 0 即 exit 16（:397–400）——策略回退会真实变红；双算 SHA 交叉
  （WSL sha256sum vs Get-FileHash，:481–495）+ 头冻结三重核对。
- `package-rc.ps1`（780 行）：`Fail`→exit 1；0.1.0 拒收（:165）；
  staged 产物/头哈希绑定（:379–383）；V4 与契约 §3 集合相等（:388–393）；
  V5 禁词（:478）；O1 pinned-zip 现场提取核对（:590–594）；B4（:604）；
  写后语义 round-trip（:633–655，5.1/7 双宿主实测替代无据版本门——
  正确的修法）；R4 重复/反斜杠/遍历/symlink/尺寸（:739–750）；R5 容器
  逐条目重哈希（:758–763）；失败即 exit 1 不留容器宣称。
- `run-csuite.ps1`（329 行）：Abort 10/11/12/13 分类（语料/构造身份
  漂移硬拒，:213–246 含 truncated 冻结 sha 逐次核对）；子进程
  RESULT 行缺失即 FAIL（:262–266）；任何 FAIL → exit 1（:328）。
- `run-binding-gate.ps1`（161 行）：preflight exit 10/11（产物+头
  冻结身份）；cargo 真实 exit code 原样传播（:161，"a failing test is
  never masked" 属实）；900s 超时 kill 后 exit 非零。
- `verify-external.ps1`（284 行）：zip 身份门（负证可拒，ask 实测
  000…000 → FAIL exit 1）；checksums 逐条重哈希；媒体缺失/无 ffmpeg
  一律 FAIL 不跳过（:213）；导入表白名单（:257–263）外者即 FAIL；
  `throw` + `$ErrorActionPreference=Stop` 全程终止性失败。

### 6. 绑定门禁调用裁定（escalation 落地）与 membership 拒绝 — PASS（receipts 实测）

- 根因 1 receipt：`gate-round1-ROOT-REPRO.log` = `error: package ID
  specification … did not match any packages`（root 下 `-p` 无法解析）；
  排除设计为已提交事实：根 `Cargo.toml:12` `exclude` + 两 crate
  Cargo.toml 注释块（`crates/qianqian-songcore-sys/Cargo.toml:12–17`
  实读："This crate is excluded from the enclosing workspace…"）。
- 根因 2 receipt：`gate-MEMBERSHIP-msvc-NEGATIVE.log` 实测 **12** 处
  LNK2019（`__mingw_fprintf`/`isatty`/`__mingw_vsnprintf` 等，mingw
  CRT/POSIX 扩展）——msvc 宿主默认 target 链不动 mingw 归档。
- membership 方案拒绝理由成立：(a) 违反已提交 exclude 设计注释（静默
  改写既有 authority）；(b) `gate-MEMBERSHIP-msvc-NEGATIVE.log` 证明
  membership 单独不修链接；(c) repo 级 `[build] target` 会连带打断
  S3 Linux lane 的 host-native cargo（.cargo/config 全局语义，技术上
  成立）；(d) msvc 侧拉 mingw 运行库 = 契约 §9 未经评审的新非系统
  依赖。四条理由均有对应 authority/receipt。规范入口
  `ops/run-binding-gate.ps1` 零仓库改动、两轮 exit 0 35/35
  （`gate-20261029-061452/061511.log` 实读，artifact/header 身份钉扎
  在 receipt 首部）。membership 实验已还原（本会话 `git status`：
  tracked 零修改）。escalation 本身属过程史，无法从物证复验，按记录
  采信；裁定内容技术正当。

### 7. E14-real-ELF 差异记录 + CI 实测核查 — PASS（本会话实测，无过头表述）

- 差异记录完整性：`pr-a-close.md` §1/§2/§4/§5 双层证据区分清晰
  （attempt4 完整门 vs 直接复跑 run2）、`pr-b-release-controls-direct-run3.log`
  在档（`E14-real-ELF: expected 2 'POLICY_RED: R3 native archive target
  mismatch'; got 2 …drift`）、s2-build §4/s2-csuite §4.3/s2-package §5
  三处独立复现同值同文案，处置一致（fail-closed、不改门代码）。
- **CI 实测核查（ask 点名）**：`.github/workflows/` 全目录 grep →
  唯一运行 `tools/test_release.py` 的 CI job 在
  `release-qianqian.yml:75–78`（step "Release adversarial controls
  (E01–E18)"，`runs-on: ubuntu-24.04`，pull_request 路径过滤含
  `native/**`——PR-A delta 触发）。并经 `gh`（WSL 路由）实测到**真实
  运行**：PR #326（head `bf23516`，OPEN）的 check rollup 中
  `build` = `release-qianqian.yml` 的 build job（全仓库唯一叫 `build`
  的 job），run **40759125478**（event=pull_request）conclusion
  **success**，其内 step "Release adversarial controls (E01–E18)"
  conclusion **success**——E14 已在 Linux runner 上真实执行并通过。
- 判定：PR #326 正文 "E14 控件将由 CI 在 ubuntu-24.04 上给出真实结果"
  **不是过头表述**——该承诺已被一次可指认的成功运行兑现（run id +
  step 级 conclusion 均实测）。相反方向亦核过：CI success 不会被误当
  "架构证明"，verdict 文件均只称其为 E14 的平台真值来源，与
  lefthook.yml:3–7 分工头注一致。

### 8. 打包容器身份链 + 外部验收 blocker 处置 — PASS（链实测，附 P2/P3）

- 链实测（本会话）：盘上 zip
  `sha256sum` = `7b4d0615d25af464…801536415` == `SHA256SUMS` sidecar ==
  `package-summary.json container.zip_sha256`（三算一致）；
  `container.rebuilt = True`、`container.previous_zip_sha256 =
  339c798d…`；包内绑定 `artifact_sha256 = 1f5f8cf8…` 不随容器重建漂移。
  链条 `b2c222cd`（初版，s2-package §6:239 作废不保留）→ `339c798d`
  （5.1 门禁形态轮，s2-package §1 记录值）→ `7b4d0615`（现行，
  07:05 重建）三个哈希在记录中均可指认，逐环有
  previous_zip_sha256/sidecar/summary 自证。
- blocker 处置正当：s2-external §7 将 ask 重钉定为唯一 blocker（文档
  级、owner 行动）；`ops/verify-external.ps1:34–43` 头注身份说明 +
  默认钉 `7b4d0615…`，负证可拒。v-checks 11/11 True（含
  manifest_roundtrip）。
- 遗留：`verdicts/s2-package.md` §1 仍以 `339c798d` 为"包身份"（其
  会话终态），对盘上现行 `7b4d0615` 已 stale——需读 s2-external +
  package-summary 才能对齐（见 findings P2/P3）。

### 9. lefthook round-2 WASAPI 抖动定性 — PASS（定性有据，附 P3）

- `package/pr-b2-lefthook.log`（1,475 行）同档保存了 round-2 失败段
  （:141–144 `cone-evidence facility-leg` panic 观察）与 round-3 全绿
  段（:1075 `test result: ok. 28 passed`，含同一测试
  `o_cone_evidence_facility_leg_witnesses_records_and_shutdown … ok`；
  尾部 summary rust-test 189.66s + release-policy 6.37s）。
- 定性审查：三轮间无任何代码变化（通道 delta 为 .ps1）、同一测试同
  内容先红后绿、`injected=true` 为故障注入观察日志而非失败——
  "时序敏感测试在门内并行负载下的宿主瞬时抖动、非稳定回归"的定性
  与 AGENTS.md concurrency-test-design 词汇一致，且 fail-closed
  （round 2 未被掩盖，如实入档）。见 findings P3（隔离复跑 receipt）。

## Findings

- **P2 — 容器哈希易变 + 下游钉扎错位的操作性陷阱**
  （`ops/package-rc.ps1` 默认重建 + `verdicts/s2-package.md` §1、
  `verdicts/s2-external.md` §7）。打包器有意撤掉"拒绝重跑"陷阱改为
  默认重建（s2-package §6，5.1/7 双宿主形态差使每次重跑容器哈希必
  变），而 ask→verdict→acceptance 的身份引用链已经两次落后于盘上
  容器（b2c222cd 作废后 ask 仍钉它；s2-package §1 的 339c798d 对现盘
  7b4d0615 已 stale）。现有缓解（previous_zip_sha256 + sidecar +
  summary 自证 + verify-external 身份头注）使漂移**可发现**，但
  shipping/evidence-merge 阶段若不冻结容器重建或统一以
  SHA256SUMS/package-summary 为唯一当前值重钉，同类 blocker 会复现。
- **P3 — `verdicts/s2-package.md` §1 容器哈希（339c798d）对盘上现行
  容器（7b4d0615）stale**，且 07:05 那次重建的执行者/动机无 verdict
  记录（仅 package-summary `rebuilt:true` + `previous_zip_sha256`
  可追溯；s2-external §1 只陈述事实）。证据合并提交时应补一行链路
  说明，否则链需跨三份文件拼合。
- **P3 — `ops/run-binding-gate.ps1:49` 硬编码产物 SHA `1f5f8cf8…`
  且无覆盖参数**（对比 run-csuite.ps1:52 有 `-ExpectedArtifactSha256`）。
  作为已提交"canonical entry"，L3 = NOT_ESTABLISHED（同 commit 不同
  绝对根字节即不同）意味着任何合法重建（含 T1 第二根产物）都会使该
  门 exit 11，直到改脚本。fail-closed 方向正确，但属每战役一次的
  硬钉，建议参数化或注明"仅对当前冻结 RC 产物有效"。
- **P3 — s2-package §7 "隔离复跑 `cargo test -p
  qianqian-output-wasapi --lib` → 28 passed" 无独立 receipt**：档内
  `package/pr-b2-lefthook.log:1075` 的 28-passed 是 round-3 全门内
  段（并门负载在场），非独立命令输出。定性实质（同测试同内容先红后
  绿）成立且更硬，但措辞与留档不完全对应；后续引用按"round-3 全门
  复跑"口径。
- **P3 — docs-verify 在本分支从未本地到达**：全部四轮 `--all-files`
  门（pr-a/pr-b/pr-c/pr-b2 日志）都因 release-policy（E14 已知差分）
  piped 中止在 docs-verify 之前；真实 push 也会在同一处停（s2-package
  §4.3 如实记录）。PR-B delta 含 docs/release/*.md，docs 门对契约
  正文的有效性目前只有 CI/后续阶段可给。属已披露的 shipping 项，
  非隐藏缺口。
- **P3 — AGENTS.md 本地门禁枚举滞后**（无 release-policy 项）：
  `pr-a-close.md` §5.2 已记录待 owner review；本评审仅确认该差异
  仍存在（AGENTS.md 未在本分支修改）。

## NOT_RUN / 边界（如实）

- 未重跑完整 lefthook 门、`tools/test_release.py`、D1 harness、C
  suite、binding gate、打包器、build-rc（ask 明令不重跑大型构建；
  verdict+receipt 为材料，小命令抽查见上）。
- 未验证 escalation 过程本身（无过程物证可查；裁定内容按 receipts
  采信，见 §6）。
- E14 的平台真值归 CI 的复核依赖 `gh`（WSL 路由）在线查询
  （PR #326 state/run 40759125478/step conclusion），查询时刻为本次
  评审会话。
- 本评审为只读；除本文件外未写任何文件、未改任何门/脚本/证据。
