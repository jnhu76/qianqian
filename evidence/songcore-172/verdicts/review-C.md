# Review C — Release/Distribution 独立评审（#172 PR-B windows release lane）

- 评审角色: Review C（Release/Distribution 视角，全新上下文，只读复核；
  本文件为本次评审唯一新增产出）
- 日期: 2026-10-29
- 位置: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`。`git log origin/main..HEAD` 实测
  恰为 ask 所述 8 提交（40ae32a/bf23516 两个构建修复 + 62acc29 契约定稿/
  56b9a04 编排器/6acb25b C 套件/423e37a 绑定门禁入口/6c52687 打包器/
  b461623 外部消费者 runner）；`git status --porcelain` 非 `??` 行为空
  （tracked clean）。
- 评审材料: 契约 `docs/release/songcore-release-contract.md`（全文 822 行
  亲读）；8 份 verdict（pr-a-close/s2-build/s2-csuite/s2-binding/
  s2-package/s2-external/s3-build/s3-verify 全文亲读）；verdict 所引
  receipt 逐项抽查（下文凡"本评审实测"均为我本人执行的命令）。
- 评审纪律: 未重跑大型构建/套件（按 ask 以 verdict + receipt 为主材，
  小命令抽查真伪）；未修改任何门代码/契约/既有证据；以下 NOT_RUN 如实
  列出。

## 结论

> **★ 现行判定见文末"# 第 2 轮复审"——第 2 轮结论 = APPROVE**（2026-10-29，
> 修复提交 a9159ef 之后，逐项复测第 1 轮 mustFix）。以下第 1 轮正文
> （含 REVISE 判定）保留为历史记录。

**REVISE（窄口径）**。证据基底本身经本评审独立抽查成立：zip 布局忠实于
契约 §7、manifest schema-2 对 §8 全清单覆盖且字节绑定真实、O1–O4/B4
全部实测通过、外部消费者证据链完整、复现性判词诚实（无一处把
NOT_ESTABLISHED 当 PASS 宣传）、版本 provisional 处置正确、无任何超出
证据的 RELEASE/SUPPORTED 级声明。但存在 **2 个 P2 级发布链缺口**
（容器身份链三代漂移且 s2-package verdict 头部身份已过期；shipped
artifact 内嵌构建宿主绝对路径对真发布构成阻塞且未在 shipping 层面
显式记录为 release-blocking），修复具体且便宜，修后可转 APPROVE。

## 1. 本评审独立实测（区分于"读到的材料"）

### 1.1 容器与布局（契约 §7）

- `sha256sum` 实测现行容器 =
  `7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415`
  （1,448,965 B）== 包旁 `SHA256SUMS` sidecar == package-summary.json
  `container.zip_sha256`，三算一致。
- 解包实测（Git Bash `unzip -l` + 解包树 `find`）：**14 文件精确集合**，
  单一版本化根 `songcore-v0.2.0-rc.1-windows-x86_64/`，无散落文件、无
  symlink、正斜杠路径——与 §7 v0.1.0 形状（windows 静态-only，无
  `lib/shared/`，与 recipe `artifacts.shared=false`、契约 §6 一致）吻合。
- **别名映射忠实于契约而非扭曲**：zip 内不存在 `BUILD-MANIFEST.json`/
  `SHA256SUMS`，对应落位 `metadata/manifest.json`、`metadata/checksums.txt`
  （§7 明文映射）；`SHA256SUMS` 仅作为容器**旁** sidecar 存在，不进包、
  不构成第二布局；`LICENSE/THIRD-PARTY-NOTICES` → `LICENSES/` 七件
  （4 FFmpeg COPYING + NOTICES + LICENSE-MIT/APACHE，即 §7 基集 +
  schema-2 B4 两件，无删改基集）。
- 包内 `sha256sum -c metadata/checksums.txt` 本评审实测 **13/13 OK**；
  checksums.txt 13 条 = 全部打包文件 − 自身（v0.1.0 形状）；manifest
  `files{}` 12 条 = checksums 集 − manifest.json（非循环；该 D1 authority
  differential 已在 manifest.notes 与 s3-verify §6 双侧如实记录，同一
  读法，未静默解决）。

### 1.2 manifest schema-2（契约 §8 全清单 + V1–V7）

- manifest.json 全文亲读（解包树）。**§8 携带字段全数在位**：
  schema_version/songcore_version/songcore_abi/abi_header_sha256/target/
  platform/arch/toolchain/ffmpeg_version/ffmpeg_commit/ffmpeg_source_sha256/
  ffmpeg_profile/ffmpeg_profile_sha256/artifact/sha256/artifact_bytes/
  archive_members/artifact_shared/shared_sha256/public_symbols/license{}/
  reproducibility{}/git_sha；**schema-2 additions 全数在位**：
  target_identity{id,…,triple,target_os,cross_prefix,toolchain{family,
  identity},recipe_sha256}/tools{cc,ar,xmake}/sdk_ndk 显式处置/source{commit,
  status:"clean"}/pin{…pin_sha256}/closure{manifest_sha256,toolchain,
  configure_args 39 条,units,reference_archives}/files{}/
  runtime_dependencies/license{…license_files}/reproducibility/built_utc/
  packer/notes。
- **真实字节绑定（本评审亲自重哈希源文件比对）**：
  `sha256sum native/ffmpeg/pin.json` = `224ad3d5…ff85` == manifest.pin_sha256
  （== 契约 §15 复核值）；`native/include/songcore.h` = `cf509a5c…78536`
  == manifest.abi_header_sha256 == checksums.txt 行；`…/codec-base.json` =
  `57fb70ed…54fa`；`…/windows-x86_64.json` = `3ba0a9e6…8492d` ==
  manifest.target_identity.recipe_sha256；manifest.ffmpeg_commit ==
  pin.ffmpeg_commit_sha == pin.json.ffmpeg_commit_sha，manifest.ffmpeg_source_sha256
  == pin.ffmpeg_source_sha256 == pin.json.source_sha256（V3 携带重复字节
  等价，本评审逐字段比对）；windows recipe 的 toolchain 块与
  manifest.target_identity.toolchain 逐字相等（V7）。
- `metadata/symbols.txt` = 15 个 `song*`/`songcore_abi_version`，排序，
  与 manifest.public_symbols 及契约 §3 集合逐名一致（V4）。
- V5 本评审实测：`grep -c "enable-gpl\|enable-version3\|enable-nonfree\|enable-lib"`
  对 manifest.json = **0 命中**（39 条 configure_args 逐条目视复核同）；
  gpl/version3/nonfree 全 false。
- V1–V7/O1/B4 不是纸面 JSON：`ops/package-rc.ps1` 代码亲读（grep 定位 +
  关键段核读），`Fail "V5_license_args"`（:475–478）、O1 staged sha ==
  pinned-zip 提取 sha 硬门（:590–598）、B4 逐字节比对（:604）、V1 集合
  门（:618/:662–671）、写后语义 round-trip（:652）均为真实 fail-closed
  实现；`v-checks.json` 11 项全 true。

### 1.3 许可义务（契约 §10 O1–O4/B4）

- **O1 独立复验**：WSL 实测 pin 标记
  `/home/hoo/q172d2-verify/closure/ffmpeg-src/.qianqian-verified` =
  `bf1b838f…|d16837dd…`（== pin.json tag→commit + zip sha256），对该
  pin-verified 源树 `sha256sum COPYING.*` 四件 =
  `8177f975/8ceb4b9e/246041b6/da7eabb7`——与包内 checksums.txt、
  manifest.license.license_files 逐一相等（边界如实注记：我复验的是
  stamp 断言 zip 身份的提取树，盘上未在 maxdepth 2 内找到原始 zip 本体；
  打包器每次运行先复核 zip==pin 的记录见 stage-audit.log 首行；该日志已按
  2026-11-01 证据精简移除，结论保留）。
- O2/B3：THIRD_PARTY_NOTICES.txt 在包、入 checksums/files（本评审 13/13
  rehash 已覆盖）；stage-audit.log（未保留）记录其取自 sha-verified v0.1.0 资产
  （018e0cbd… == 契约 S-V010 冻结值）；NOTICES 全文亲读。
- O3：README "Relink obligations" 节亲读——明示重链接输入
  （pin.json + profile + target recipe + closure manifest + xmake 配置）
  且声明 licensee 可重组，符合 O3。
- O4：见 §1.2 V5（0 命中）。
- B4：本评审 `cmp` 实测包内 LICENSE-MIT/LICENSE-APACHE 与仓库根文本
  **逐字节一致**（MIT IDENTICAL / APACHE IDENTICAL）。
- stage-audit.log 与 package-summary.json audits 块（R3 280/0、15 符号、
  BCrypt 三符号、1185 undefined）与 verdicts 记录一致。

### 1.4 独立消费者证据强度（s2-external）

- 两份 transcript（`external-accept/20261029-071806`、`…072050`）亲读：
  解包 → 包内 13/13 checksums → README 契约链接行原文一次通过
  （`gcc -O2 -Iinclude … -Llib -lsongcore -lbcrypt`）→
  open→probe→decode(441000 帧→SONG_EOF)→seek50%→post-seek→close 全序列
  exit 0 → 导入表 = `bcrypt.dll, KERNEL32.dll, msvcrt.dll`，白名单审计
  在**已提交**的 `ops/verify-external.ps1:253–263` 真实实现（allowlist
  之外任何 DLL 即 Fail）。
- 负证链真实：`audit/win-link-nobcrypt.log` 亲读——缺 `-lbcrypt` 时
  3 个 BCrypt 符号 undefined reference 硬链接失败。
- 但两项声明**无 receipt**（见 F3）。

### 1.5 shipped artifact 内嵌宿主绝对路径（P2 定级）

- 本评审以 `/usr/bin/grep`（注：本机 `grep` 函数被 ugrep 代理，二进制
  语义不同——首次 `-aoc` 得 0 为**假阴性**，已换 GNU grep 复核）实测
  **shipped** `lib/libsongcore.a`（zip 内，sha `1f5f8cf8…`）：
  `grep -aoc "home/hoo/q172rc"` = **9 行**；内容分布
  `q172rc/win/native/build/ffmpeg-src/libavcodec/bytestream.h` ×5、
  `libavutil/iamf.h` ×2、`libavcodec/put_bits.h` ×2。
- 双根 receipt（`package/repro/`）逐份亲读：identity.txt（R1
  1f5f8cf8…/3,309,710 B vs R2 059eb84e…/3,309,822 B）、member-order
  IDENTICAL(280)、259 同 + **8 异**（differing-members.txt 全 FFmpeg
  .obj）、rootcause-strings.txt 8/8 各含本根路径、archive-path-scan
  （R1 9 行 /home/hoo/q172rc/，R2 9 行 /home/hoo/q172rc-s2pkg/，交叉
  0，旧 /home/hoo/Source/ 0）、debug-info-sample.txt（295 字节差、双侧
  节表全等、**无 .debug_* 节** → 非 DWARF，是普通节内 `__FILE__` 类
  字符串常量）、ar 头 uid/gid 0/0 + epoch-0（GNU ar 确定性模式）、
  songcore_ffmpeg.c.obj 双根逐字节相同、closure manifest 双根 sha 相等、
  undefined 集合 1185 全等（L2-d 实证）。receipt 与 s2-package §3 记录
  完全一致。
- 定级 **P2**，理由：①对真发布是构建宿主拓扑泄漏（/home/hoo/q172rc
  路径进入分发物）；②结构性阻断 L3 字节复现（manifest 已如实
  NOT_ESTABLISHED）；③无功能/安全影响（csuite 12/12、外部消费者
  全序列 PASS 均在该 artifact 上取得）。处置正确性：lane 未自作主张加
  `-ffile-prefix-map`（契约 §12 T4 一次一旗 + Q2 owner 保留；S3 linux
  通道同样零旗标）——**这与 owner Q2 的关系是：修复路径已冻结在契约
  T4 首选旗标，但"改 vs strip/-g0"是 Q2 owner 裁决，任何 songcore-v*
  真发布前必须先裁决并消除该差异类**。S3 linux 产物同类（同 8 成员、
  同根因，s3-verify §5），两目标一致。

### 1.6 容器哈希非确定性 — 如实声明核对

- s2-package §8 NOT_RUN #4 明文："zip 容器自身的字节确定性——未设计
  未声称（L3 只约束 shipped library；容器哈希已如实记录，重放会得新
  容器哈希）"；§6 记录 pwsh7 轮 7c7f2477…(1,458,011 B) vs 5.1 轮
  339c798d…(1,448,964 B) 差异仅为 deflate 流 + JSON 排版、语义经
  round-trip 断言等价；本评审另实测第三代 7b4d0615…(1,448,965 B，
  与 339c798d 差 1 B——zip 条目 mtime/deflate 所致)。manifest 与
  package-summary 均未对容器做任何字节确定性声明。**声明诚实**。
- 但三代容器的存在本身造成 verdict 链身份漂移（见 F1）。

### 1.7 复现性 L1/L2/L3 诚实性

- Windows manifest（本评审解包亲读）：`recipe/semantic_abi/byte` 三项
  **全 NOT_ESTABLISHED**，evidence 指向真实存在的 repro 回执目录
  （V6 门在 package-rc.ps1:682 实测检查文件存在性）。s2-package 判词
  明说 L2-c 项（G2-on-R2）缺失故 NOT_ESTABLISHED，a/b/d 已实测项如实
  入回执。**没有被当 PASS 宣传**。
- Linux manifest（s3-verify §3）：`RECIPE_RECONSTRUCTIBLE` +
  `SEMANTIC_ABI_VERIFIED` + `byte NOT_ESTABLISHED`——按契约 §12 判据
  属已挣得：R1 全新 clone + fetch(proxy 如实记录) + import oracle +
  build 全文档化命令 + G2 套件在该产物上 PASS；R2 独立根过全量 G2
  （frozen_reference_match 4/4）、ABI 15/15 双形态、closure 字节等、
  NEEDED 面一致的 L2 四要素（a–d）齐备；R2 的 closure 为按 ask 逐字
  复制（非独立导入）已明示。诚实。
- 全部 verdict 亲读范围内无一处把 NOT_ESTABLISHED/NOT_RUN 写成 PASS。

### 1.8 版本 0.2.0-rc.1 provisional 处置

- README 开头亲读："This is a release-candidate verification archive
  (#172), NOT a release … owner-reserved (contract section 14 Q1)"；
  manifest.notes 与 package-summary `version_status` 同句三处；包名
  `songcore-v0.2.0-rc.1-*` 是 §7 布局要求，落点在 evidence/ 本地、
  未发布未打 tag——处置正确。

### 1.9 RELEASE/SUPPORTED 级声明越界检查

- 全部 verdict 与包内 README/manifest 均将 G4/发布/下载可达性/历史
  release 不可触碰明确列为 NOT performed（s2-package §8 #6、s3-verify
  §7、s3-build §7）；windows-x86_64 的 CORE_RUNTIME/BINDING_RUNTIME 均
  以"取得证据"措辞（S-BIND §5 词表内），无 RELEASE 状态宣告。**未发现
  越界声明**。

### 1.10 PR-A #326 与 release-policy E14 differential 的收口真实性

- 本评审经 WSL `gh` 实测：PR #326 state=OPEN，head
  `bf23516b5851174dc63fb5058997fa5478751713`；对 head SHA 的
  check-runs API 返回 6 项——4 SUCCESS（build、fmt+check+test+clippy
  Linux、Cargo topology、cfg(windows) compile）+ publish/CodeRabbit
  skipped。经 workflow job 名比对（release-qianqian.yml 仅有 `build`/
  `publish` 两个 job id，其余 workflow 的 job 名为
  rust-regression/windows-compile/plugin-boundary/tla-semantic 等），
  **"build"/"publish" 即 release-qianqian.yml 的两个 job**：其
  pull_request 触发含 `native/**`，build job（ubuntu-24.04）内含
  "Release adversarial controls (E01–E18)" = `python3 tools/test_release.py`
  （release-qianqian.yml:75–78）。即 **E01–E18 已在 Linux runner 上真实
  执行且绿**——六个 verdict 反复引用的"E14 平台真值归 ubuntu-24.04
  CI"不是空头支票，是已兑现的收口。本地 6/7 + E14 红的平台 differential
  定性成立。
- PR-B 分支未推送、无 PR（各 verdict 如实声明）；PR-B 尚无 CI 记录，
  不得引用 PR-A 的绿替代（各 verdict 亦未如此引用）。

## 2. Findings

### F1 (P2) 容器身份链三代漂移，s2-package verdict 头部身份已过期

- 位置: `evidence/songcore-172/verdicts/s2-package.md` §1（container
  SHA256 = `339c798d…`，BYTES 1,448,964）；现行盘上容器 =
  `7b4d0615…`（1,448,965 B，本评审实测 == sidecar == summary）；
  `package/package-summary.json` `rebuilt: true`、
  `previous_zip_sha256: 339c798d`；三代链
  `b2c222cd（作废，无副本）→ 339c798d → 7b4d0615`；s2-external §1/§7
  已把 ask 钉住 b2c222cd 记为唯一 blocker（文档级）。
- 危害: 任何人按 s2-package §1 引用容器哈希，`ops/verify-external.ps1`
  的 zip-identity 门即拒（fail-closed 正确但链断了）；shipping 阶段写
  PR body/evidence-merge 时若不钉 7b4d0615 将复制同一错误。根因是
  容器字节非确定（zip 条目 mtime + deflate），每次重跑 package-rc.ps1
  都会使全部已记录容器哈希过期，而 verdict 冻结后无 addendum 机制。
- 注: artifact 级链路无损——本评审实测消费路径
  `native/build/artifacts/libsongcore.a` 现哈希仍 = `1f5f8cf8…` ==
  zip 内 checksums/manifest == csuite/binding receipts。

### F2 (P2) shipped artifact 内嵌构建宿主绝对路径 — 真发布阻塞项，须在 shipping 层显式挂到 owner Q2

- 位置/实测: 见 §1.5。shipped zip 内 lib/libsongcore.a 含 9 行
  `/home/hoo/q172rc/win/...`（普通节字符串，非 DWARF）；双根 receipt
  完整。
- 危害: 发布物泄漏构建机布局；L3 字节复现被阻断（已如实
  NOT_ESTABLISHED）。当前作为 RC 验证档案可接受，但 **F2 未解决前不得
  产出任何 songcore-v* 真发布**；修复路径（-ffile-prefix-map T4 一次
  一旗，或 strip/-g0）由契约 §12/§14 Q2 owner 裁决——本评审要求的是
  把该阻塞状态写进 shipping 材料显式可见，而非现在改旗标。

### F3 (P3) 两项 s2-external 声明无 receipt

- 负向 `-ExpectedZipSha256 000…0 → FAIL [zip-identity] exit 1`：声明于
  s2-external §5，但 `verify-external.ps1:60–67` 的 identity 门在
  `Start-Transcript`（:71）**之前** Fail，结构性不可能留下 transcript；
  代码逻辑可推出该行为，但证据链上无档。干净 PATH（仅 System32）复测
  同为会话内手工运行，无档。正向路径 receipt 齐备（两 transcript +
  已提交 runner 的白名单审计 + nobcrypt 负证）。

### F4 (P3) THIRD_PARTY_NOTICES.txt 与 B4 组件许可文本存在措辞级矛盾（v0.1.0 遗留）

- 位置: 包内 `LICENSES/THIRD_PARTY_NOTICES.txt`（Qianqian 节："carry
  no open-source license yet; distribution terms are owned by the
  project and are NOT granted by this package"）与同目录
  LICENSE-MIT/LICENSE-APACHE（本评审 cmp 实测 == 仓库根）。契约 §10/B4
  背景段已如实记录 v0.1.0 该声明；机器检查只证在位性/哈希同一性，管
  不了措辞。owner 在真发布前应刷新 NOTICES 措辞（或加组件许可节），
  避免受许可人读到两个相反的授予声明。RC 档案不阻塞。

### F5 (P3) manifest 源 commit（6c52687）≠ 分支 HEAD（b461623）

- package-summary/manifest `source.commit` = 6c52687（打包时 HEAD）；
  此后 b461623 仅新增 ops/verify-external.ps1（native/ 未动，artifact
  链无损，本评审已验证）。证据链上可解释，但 shipping 时应注记或在
  最终 HEAD 重产容器，避免"RC 容器的 git_sha 落后于携带它的分支"。

### F6 (P3) 杂项观察（不阻塞）

- manifest `closure.toolchain.arch = "x86"`（recipe 为 x86_64）——本
  评审在 WSL 实测 imported closure manifest（sha `0d0f7af9…`）原值即
  如此，packager 忠实搬运（契约 §8 该块定义即 S-IMP 导入值），非
  错误；仅提示消费方勿误读。
- s2-build §4 预测"piped 门下真实 push 仍会在 release-policy 停下"，
  而 §4 第 3 条又说本通道 delta（docs/ops）不命中 release-policy glob
  ——两处措辞并存；真实 push 时 lefthook 收窄行为由 shipping 实测，
  不得引用任一预测充当实测。
- `verify-external.ps1` 把会漂移的容器哈希 `7b4d0615` 作为已提交默认值
  （头部有 identity note）；未来任何打包器重跑都会使其过期——建议改
  读 sidecar（保持 fail-closed）或接受并在 shipping 注记。

## 3. NOT_RUN（本评审未执行，不充当 PASS）

- 未重跑 build-rc/package-rc/run-csuite/run-binding-gate/verify-external
  任何全流程（以 verdict + receipt 抽查替代，抽查项见 §1）。
- 未独立复算 csuite 109/109、binding 214/214、suite 12 通道 receipt 的
  每一项断言（抽读 summary.json verdict=PASS + 关键字段一致）。
- 未在 WSL 找到 pinned FFmpeg zip 本体（O1 复验走 pin-stamped 提取树，
  见 §1.3 边界注记）。
- 未检查 S3 linux 包（tar.gz）内容细节（s3-verify 材料已亲读；本评审
  主 ask 为 windows lane，linux lane 未做解包级抽查）。
- TLA+/Kani/Loom/Miri/性能门：不在本评审范围，未运行。

## 4. 判定（第 1 轮）

REVISE —— F1/F2 两项 P2 修复后（均为文档/链路级，不动产物）可转
APPROVE。证据基底的真实性与诚实性经本评审独立抽查未发现任何伪造、
吞错或把 NOT_ESTABLISHED 写成 PASS 的情形；fail-closed 纪律在全部
实测路径（打包器 V 门、外部消费者 identity/白名单门、构建门负控）上
成立。

---

# 第 2 轮复审（2026-10-29，修复提交 a9159ef 之后）

## 5. 修复内容与逐项核验（全部为本评审本轮亲测）

修复提交: `a9159ef` "fix(songcore): pin the external verify to the sha
sidecar (#172)"（`git show --stat` 实测：仅 ops/verify-external.ps1，
+67/−16）；证据面修复为未跟踪的 verdict addendum + 新 receipt（按战役
惯例 evidence 不入库，随 shipping 合并）。分支现为 9 提交
（`git log origin/main..HEAD` 实测），tracked clean。

### F1 容器身份链 — 已解决 ✅

- `verdicts/s2-package.md` §1 已改钉 `7b4d0615…536415`（1,448,965 B）
  并加冻结 blockquote；新增 §10 Addendum 记录完整四代重建链
  （b2c222cd→7c7f2477→339c798d→7b4d0615，每跳标注出处），并如实声明
  证据边界（更早 previous_* 快照被覆盖未留存，靠 §1/§6 原文交叉印证）。
- **冻结声明**（不重跑 package-rc.ps1）与"shipping 只引 7b4d0615"规则
  成文。本评审实测：容器 mtime 仍 07:05（未重打包），`sha256sum` =
  `7b4d0615…` == `SHA256SUMS` sidecar（两个独立工具链）。
- `ops/verify-external.ps1` 默认 pin 改为**读取 zip 旁 SHA256SUMS
  sidecar**（diff 亲读：缺 sidecar / 无匹配行 / 行式畸形三分支均
  Fail，fail-closed 保持；`-ExpectedZipSha256` 仅作负向显式覆写）。
  修后门日志 `package/pr-b2-lefthook.log`（233.87s）尾部实测：
  6/7 ✔ + release-policy 🥊 = E14 同一已知宿主差分（与第 1 轮 §1.10
  同一定性，CI 真值已由 PR-A release-qianqian build job 绿收口）。
- 本评审实测 `git grep`：tracked 面仅 ops/verify-external.ps1（注释性
  identity note）引用 7b4d0615；`/usr/bin/grep -rn
  "339c798d|b2c222cd|7c7f2477" docs/` = **0 命中**（shipping 面无
  stale 引用）。

### F2 release-blocking 记录 — 已解决 ✅

§10 Addendum F2 节成文 RELEASE-BLOCKING 规则：windows `1f5f8cf8…` 与
linux `409ef3da…` 两个 RC 归档各含 8 个内嵌构建根绝对路径成员（普通节
非 DWARF），在 ①契约 §14 Q2 owner 裁决 且 ②按裁决方案 T1–T5 实测消除
该差异类并保持 G0–G2 绿 之前，**不得产出任何 songcore-v\* 真发布**；
与容器内 `manifest.reproducibility.byte="NOT_ESTABLISHED"` 相互引用，
并明令不得改写冻结容器（消除差异类须走新容器 + 新 verdict）。该节
同时如实声明：PR-B body 尚不存在，本 addendum 是当前唯一 shipping 面
记录、对 PR-B body / evidence-merge 作者构成约束（本评审实测
`docs/release/` 现仅 pr-a-body.md，属实）。满足第 1 轮 mustFix 的
"显式记录 + 交叉引用"要求。

### F3 缺失 receipt — 已解决 ✅

- 代码：`Start-Transcript` 移至 identity 门**之前**；catch 记录
  FAIL-CLOSED 并以 `exit 1` 收尾（diff 亲读，`$verdict` 非 PASS 即
  exit 1 + JSON 摘要）。
- 负向 receipt 落盘 ×2（`external-accept/20261029-074115/074123`，
  transcript 内可见 `FAIL [zip-identity] sha256 mismatch`）。**本评审
  亲测复跑** `powershell -File ops/verify-external.ps1 -ExpectedZipSha256
  000…0` → **exit 1**，transcript 20261029-074937 落盘（真实退出码，
  非推断）。
- 正向 sidecar-pin 跑 receipt `20261029-074149`（transcript 可见
  "identity pin: from SHA256SUMS sidecar -> 7b4d0615…"，13/13 OK，
  全序列 PASS，导入表 bcrypt/KERNEL32/msvcrt）。
- **干净 PATH 本评审独立复测**：冻结容器新鲜解包（checksums 复验
  13/13、artifact `1f5f8cf8…` 复验同值）→ 按 README smoke 行编译包内
  `metadata/external-consumer-example.c`（`gcc -O2 -Iinclude … -Llib
  -lsongcore -lbcrypt`，导入表实测恰为 bcrypt/KERNEL32/msvcrt）→
  `cmd /c "set PATH=C:\Windows\System32&& consumer.exe <canonical flac
  fixture>"` → **CLEANPATH_EXIT=0**，probe/176400 帧 decode/seek
  landing 1985306≤2000000/EOF/close 全 PASS。属性（无非系统 DLL 运行
  时依赖）由本评审直接建立。

### F5 源漂移注记 — 已解决 ✅（含一处时效小瑕，见 P3-1）

§10 Addendum F5 节以 `git diff --stat 6c52687..b461623`（仅
ops/verify-external.ps1；`-- native/` 为空）成文漂移说明并要求
evidence-merge 连节引用。本评审实测该论证**机械延伸至现行 HEAD**：
`git diff --stat 6c52687..HEAD` = 仅 ops/verify-external.ps1（335 行），
`-- native/` 为空，`native/build/artifacts/libsongcore.a` 现哈希仍 =
`1f5f8cf8…`（与冻结容器绑定值一致）。

## 6. 第 2 轮残留 findings（全部 P3，不阻塞）

- **P3-1** Addendum §F5 的"分支当前 HEAD `b461623`"一句已时效性过期
  （现行 HEAD = `a9159ef`）。实质无碍——本评审实测 a9159ef 亦仅动
  ops/verify-external.ps1、native/ 零变化，漂移论证按同一方法覆盖；
  evidence-merge 引用时请按"6c52687 之后全部为 ops-only 提交"读，而非
  引用句中过期的 commit 号。
- **P3-2** 074149 正向 receipt 的 transcript 未记录子进程 PATH 环境，
  addendum "子进程 PATH 仅 C:\Windows\System32" 属修复会话的自述、
  transcript 无法自证；该**属性**已由本评审独立复测直接建立（见 §5
  F3）。后续如需自包含 receipt，可在 transcript 内 echo 受限 PATH。
- **P3-3（自第 1 轮 F4 原样继承）** THIRD_PARTY_NOTICES.txt 的
  "NOT granted" 措辞与并置的 LICENSE-MIT/LICENSE-APACHE 矛盾仍在
  （本轮修复未涉及，机器检查管不了措辞）——真发布前 owner 须刷新
  NOTICES 或加组件许可节；与本轮 F2 的 release-blocking 规则相互
  独立，均须在真发布前关闭。
- **P3-4（自第 1 轮 F6 原样继承）** s2-build §4 对真实 push 时
  release-policy/docs-verify 行为的两句并存预测，仍须由 shipping 在
  真实 push 时实测，不得引预测充当实测。

## 7. 判定（第 2 轮）

**APPROVE** —— 第 1 轮全部 4 项 mustFix（F1 身份链冻结 + sidecar 化、
F2 release-blocking 成文、F3 负向/干净 PATH receipt、F5 漂移注记）经
本评审逐项独立复测确认真实解决；残留 4 项 P3 均为文档时效/自述边界/
owner 保留项，不阻塞本战役 RC 验证证据的成立。

**边界声明**：本 APPROVE 只覆盖 #172 S2/S3 战役的 RC 验证证据与
windows-x86_64 打包通道；它**不是**任何 songcore-v\* 发布的批准——
Addendum F2 的 RELEASE-BLOCKING 规则（内嵌路径差异类 + Q2 owner 裁决）、
契约 §14 Q1–Q5（版本号、发布机制等 owner 保留）、以及 NOTICES 措辞
刷新（P3-3）在任何真发布前仍然全部有效。
