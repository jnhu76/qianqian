# S2 打包通道判定 — Windows RC 包（package-rc.ps1 + 双根复现性实测）

- 判定角色: RC 打包工程师（#172 S2，独立 subagent 会话）
- 日期: 2026-10-29
- 位置: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`（基线 `423e37a`；本会话新增
  1 提交：`6c52687` 打包器（含门禁第 1 轮修复的 amend，原 `aa19c9c`）；
  `git log` 实测。打包运行时 `native/`
  tracked-clean，`git status --porcelain -- native/` 无非 `??` 行）
- 冻结契约: `docs/release/songcore-release-contract.md`（§7 PACKAGE_LAYOUT、
  §8 PROVENANCE_SCHEMA、§9 RUNTIME_DEPENDENCIES、§10 LICENSE_OBLIGATIONS、
  §12 复现性——全文已读；以下条目编号均指该文件）
- 上游输入: S2 构建通道产物 `native/build/artifacts/libsongcore.a`
  sha256 `1f5f8cf8…fd402`（= s2-build 最终轮产物，同值；本会话
  build-rc 复跑再确认同值）；D2 已导入 closure（WSL
  `/home/hoo/q172d2-verify` 的 pinned zip + `/home/hoo/q172rc` 工作根，
  本会话逐次 pin 校验）；S3 通道既有打包先例
  `evidence/songcore-172/linux/verify/package/package_linux.py`（复用其
  V1–V7 控件形状与 files{} 非循环绑定差分记录，不另起炉灶）。

## 结论

**S2 打包通道 PASS（本通道交付范围内），fail-closed 全程成立。**
RC 包已按契约 §7/§8/§10 产出并通过全部机器检查（V1–V7、O1/O2、B4、
R3/R4/R5 方法镜像）；双根复现性按 §12 T1–T3 实测并如实记录
（L3 字节不可复现，根因已定位；L1/L2 按证据缺席如实判 NOT_ESTABLISHED）。
版本 `0.2.0-rc.1` 为 ask 指定的 provisional 回退（契约 §14 Q1 仍
owner 保留），manifest notes 已标注。

## 1. 包身份（实测，三算一致）

```text
CONTAINER  C:\Users\fred1\source\qianqian-172\evidence\songcore-172\package\
           songcore-v0.2.0-rc.1-windows-x86_64.zip
SHA256     7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415
           ★ 唯一现行容器（评审 C F1 冻结；重建链与作废历史见 §10 addendum）
BYTES      1,448,965
构建宿主   Windows PowerShell 5.1.26100（打包门禁调用形态；20261029-070516 轮）
SHA 双算   Git Bash sha256sum == SHA256SUMS sidecar（sidecar 由脚本内
           Get-FileHash 写出——两个独立工具链；本会话 sha256sum 实测同值）
ROOT       单一版本化根目录 songcore-v0.2.0-rc.1-windows-x86_64/（14 文件）
ARTIFACT   lib/libsongcore.a = 1f5f8cf8…fd402（3,309,710 B，280 成员
           280/280 pe-x86-64、0 elf；== 构建通道产物与 s2-csuite G2
           PASS 所绑定的同一字节，verdicts/s2-csuite.md §1）
HEADER     include/songcore.h = cf509a5c…78536（冻结 ABI v1 身份；
           与仓库源文件 cmp 逐字节一致）
SIDEcar    SHA256SUMS（zip 哈希单行，产品 R5 sidecar 惯例）
```

> 评审 C F1：容器此后**冻结**——不再重跑 ops/package-rc.ps1；一切
> shipping 材料（PR body / evidence-merge / verify-external）只引用
> `7b4d0615…`。本节历史上的 `b2c222cd…`/`339c798d…` 身份一律作废
> （详见 §10）。

包内容（契约 §7 v0.1.0 形状；windows 静态-only，无 lib/shared/）：
include/songcore.h、lib/libsongcore.a、LICENSES/{COPYING.GPLv2,
COPYING.GPLv3, COPYING.LGPLv2.1, COPYING.LGPLv3, THIRD_PARTY_NOTICES.txt,
LICENSE-MIT, LICENSE-APACHE}、README.md、metadata/{manifest.json,
symbols.txt, checksums.txt, external-consumer-example.c}。
ask 中的别名按契约 §7 映射落位：BUILD-MANIFEST.json →
metadata/manifest.json；SHA256SUMS → metadata/checksums.txt（zip 旁另有
SHA256SUMS sidecar）；LICENSE/THIRD-PARTY-NOTICES → LICENSES/ 七件。

独立验证（不信打包器自述）：Windows bsdtar 列表/解包 → 14 条目精确集合、
正斜杠路径；包内 `sha256sum -c metadata/checksums.txt` 13/13 OK；
解包树逐文件再哈希 == manifest files{} 12/12 OK。

## 2. Manifest（schema_version 2，契约 §8 全清单）

机器检查 v-checks.json 全绿（10/10：V1_files_rehash、V1_sets、
V2_header_sha、V3_pin_carry_dups、V4_symbols、V5_license_args、
V6_repro_receipt、V7_tools、O1_license_texts、B4_component_licenses）。

```text
carried    schema_version=2, songcore_version=0.2.0-rc.1(PROVISIONAL),
           songcore_abi=1, abi_header_sha256, target/platform/arch
           windows-x86_64/windows/x86_64, toolchain="mingw-w64",
           ffmpeg n9.0.1 | commit bf1b838f… | source d16837dd…（三者与
           pin.json 逐字段相等，V3 携带重复字节等价）,
           ffmpeg_profile=codec-base | sha256 57fb70ed…54fa,
           artifact lib/libsongcore.a | sha256 1f5f8cf8… | 3,309,710 B,
           archive_members=280, artifact_shared=false, shared_sha256=null,
           public_symbols 15 个（== symbols.txt == nm ' T song' 审计 ==
           契约 §3 集合）, git_sha 423e37a…66961
additions  target_identity{… triple x86_64-pc-windows-gnu, target_os mingw32,
           cross_prefix x86_64-w64-mingw32-, toolchain==recipe 块,
           recipe_sha256 3ba0a9e6…8492d（== 源文件三处一致）},
           tools{cc,ar,xmake 精确 --version 首行}, sdk_ndk 显式
           "not applicable …"（#172 §5 清单项可见处置）,
           source{commit,status:"clean"}（R2 纪律）,
           pin{…pin_sha256 224ad3d5…（== 契约 §15 复核值）},
           closure{manifest_sha256 0d0f7af9…aed14（== #324 实录值，
           closure 未漂移）, toolchain 块, configure_args 39 条,
           units{279/279/0}, reference_archives 4 件},
           files{} 12 条 = checksums.txt 13 条 − manifest.json 自身
           （非循环绑定；对 §8 "same set" 措辞的差分已在 notes 记录，
            与 S3 linux 通道同一读法）,
           runtime_dependencies{bcrypt REQUIRED（nm 实证 BCrypt* 三符号）,
           mingw-w64 runtime, link_line "gcc … -lsongcore -lbcrypt",
           ffmpeg_runtime none}（契约 §9）,
           license{LGPL-2.1-or-later, gpl/version3/nonfree=false,
           external_libs=[], license_files 4 件 sha256},
           reproducibility{recipe/semantic_abi/byte 全 NOT_ESTABLISHED,
           evidence 指向 §3 实测回执}, built_utc, packer, notes
           （provisional 版本声明 + Q1 保留 + 差分 + 许可机器检查边界）
```

## 3. 复现性实测（ask 指定 repro 节；§12 T1–T3 真实执行）

T1 双根：本会话真实运行 `ops/build-rc.ps1 -BuildDir /home/hoo/q172rc-s2pkg`
（run `20261029-062848`，exit 0，全门绿；closure 复用已导入缓存，无下载
无重导入）。R1 = 主根消费路径产物，R2 = 第二独立绝对根
`/home/hoo/q172rc-s2pkg`（产物仅落 evidence，消费路径未动）。

```text
identity     R1 1f5f8cf8…fd402 / 3,309,710 B
             R2 059eb84e…dab6 / 3,309,822 B（+112 B）
             → L3 字节级：NOT byte-identical（同 commit 423e37a、
               同 closure、同 toolchain，不同绝对根）
成员顺序     x86_64-w64-mingw32-ar t 逐项 diff：IDENTICAL（280 名单全等）
ar 头字段    ar tv：uid/gid 0/0、时间戳 epoch-0（GNU ar 确定性模式生效
             → 无时间戳/归属差异类，与 S3 linux 实测同类）
逐成员 diff  ar x 双侧展开（契约 §12 T2 冻结的二进制安全方法）：
             280 成员 = 259 逐字节相同 + 8 不同
             （differing-members.txt：flac_picture / hdr_dynamic_metadata /
               iamf_parse / iamf_reader / lcevc / oggparseflac /
               oggparseogm / oggparsevorbis，全是 FFmpeg 目标文件）
根因分类     8/8 不同成员各自内嵌本根绝对路径字符串
             （r1root_hits≥1 且 r2root_hits≥2 一一对应；
               例 flac_picture.c.obj：295 字节差异，两侧节表全等
               （.text/.rdata 尺寸同），strings diff 即
               /home/hoo/q172rc/...bytestream.h vs
               /home/hoo/q172rc-s2pkg/...bytestream.h）。
             归档级 grep -aoc：R1 含 9 行 /home/hoo/q172rc/，R2 含 9 行
             /home/hoo/q172rc-s2pkg/，互为 0，旧 /home/hoo/Source/ 为 0。
             ★ 修正 S-325 的措辞：本 release 构建（无 -g）目标文件无
             .debug_* 节——路径数据在普通节（rdata/文本）的字符串常量
             （__FILE__ 类），不是 DWARF debug info。根因不变（FFmpeg
             重放编译未做路径重映射，契约 §12 T4 预案），定位更准。
SongCore 侧  songcore_ffmpeg.c.obj 双根逐字节相同（S-323 结论在
             windows 复现）；closure manifest 双根 sha 相等
             （0d0f7af9…，CLOSURE_IDENTITY=BYTE_IDENTICAL）
运行面       双根 undefined-symbol 集合全等（1185 符号，含且仅含
             BCryptOpenAlgorithmProvider/BCryptGenRandom/
             BCryptCloseAlgorithmProvider 三项 BCrypt*）→ 契约 §9
             运行依赖面双根一致（L2-d 实证）
L 判词       L1 RECIPE_RECONSTRUCTIBLE：NOT_ESTABLISHED（本通道复用
             worktree + 已导入 closure，按 ask 约束；新鲜环境端到端
             重建未执行——s2-build NOT_RUN #7 原样继承）
             L2 SEMANTIC_ABI_VERIFIED：NOT_ESTABLISHED（a 符号审计
               15/15 双根相等、b closure 身份字节等、d 运行依赖面全等
               均已实测；c 项 G2 套件未在 R2 产物上执行——ask 对比项
               不含 G2-on-R2，不越界代跑）
             L3 BYTE_REPRODUCIBLE：NOT_ESTABLISHED（上为实测不等 +
               根因分类回执）
T4 处置      未添加任何编译旗标。实测差异类唯一归因 = 嵌入绝对路径；
             对症候选是 -ffile-prefix-map=<root>=.（契约 §12 T4 首项），
             但 T4 要求"一次一旗、只保留实测消除差异类者"且 Q2（debug
             策略）owner 保留——本通道不越权堆参（零装饰性旗标）。
回执         evidence/songcore-172/package/repro/（identity.txt、
             member-order.{verdict,diff}、member-diff.verdict、
             r{1,2}-member-shas.txt、differing-members.txt、
             rootcause-strings.txt、archive-path-scan.txt、
             debug-info-sample.txt、songcore-c-object.txt、
             undefined-imports.txt、closure-identity.txt、
             ar-header-*、run-compare.sh）
```

## 4. 许可义务（契约 §10）

```text
O1 PASS   四件 FFmpeg COPYING 文本从 pinned n9.0.1 源 zip 现场提取
          （打包器每次运行先以 sha256 复核 zip == pin.json
          source_sha256 d16837dd…，本会话 WSL 实测同值），staged sha
          == 提取 sha 逐件核对；manifest.license.license_files 落 4 件
          哈希。绝不用手工维护副本。
O2/B3 PASS  THIRD_PARTY_NOTICES.txt 从已发布 songcore-v0.1.0 资产现场
          提取（本会话 gh release download 实取，sha256
          018e0cbd…6815 == 契约 S-V010 冻结值；文件入
          /home/hoo/s2pkg-v010/ 供重放），入包并入 checksums.txt/files{}。
O3 PASS   README "Relink obligations" 节明示重链接路径（pin.json +
          profile + target recipe + closure manifest + xmake 配置）。
O4 PASS   configure_args 39 条含 0 个 --enable-gpl/--enable-version3/
          --enable-nonfree/--enable-lib*；gpl/version3/nonfree 全 false。
B4 PASS   LICENSE-MIT/LICENSE-APACHE 与仓库根文本逐字节一致。
RELEASE_BLOCKED 候选：无（B1–B4 全部机器可查且全绿）。
边界如实记录：机器检查只证在位性/哈希同一性/configure 溯源，不能证明
法律充分性（§10 limit 已写入 manifest notes 与 README）。
```

## 5. 既有 release policy 控件对照（复用，不改控件）

```text
R3 方法镜像 PASS   WSL x86_64-w64-mingw32-objdump -f：280/280
                   pe-x86-64、0 elf（= check_release.py:117–121 同法）；
                   打包 header == 源 header（= :120–121 同法）。
R2 纪律镜像 PASS   git rev-parse HEAD + native/ tracked-clean 门
                   （= :83 同法；打包器内实装）。
R4/R5 方法镜像 PASS  zip 精确内容集/无重复/无遍历/无反斜杠/无 symlink/
                   大小上限 + 容器逐条目重哈希绑定 files{}/checksums
                   （= :192–218 同法，适配 SongCore 布局）。
R1/R7 不适用        产品 tag 形态（qianqian-v*）/CHANGELOG/发布政策是
                   产品通道控件（check_release.py:74–76 明示
                   "SongCore tags are separate"）；SongCore 包按 §7
                   v0.1.0 形状自带 checksums/manifest，不冒用产品
                   BUILD-MANIFEST.txt 名（布局名经 §7 别名映射落位，
                   无第二布局、无冲突）。
控件健康实测        PATH 含 tools/bin + xmake + mingw64/bin 直跑
                   python3 tools/test_release.py（日志
                   package/test-release-direct-run.log）：确定性推进到
                   E14-real-ELF 并复现 PR-A/s2-build 同一已知平台
                   differential（exit 3 TOOLING_FAIL，负例期望文案在本
                   宿主结构性不可产生）——E01–E13 + native-synthetic-
                   baseline + E14-hash 全部执行通过，无新红；未改任何
                   门代码。
```

## 6. 打包器（提交，可重放；门禁第 1 轮修复记录）

`ops/package-rc.ps1`：preflight（R2 clean 门 + 冻结 header 身份 +
0.1.0 拒收）→ WSL 审计/许可单级（zip==pin 复核、COPYING 从 zip 提取、
v0.1.0 tarball==S-V010 复核、NOTICES 提取、R3 目标身份、15/15 符号、
成员数、BCrypt* 审计；任何失配 exit 非零）→ §7 布局 staging →
schema-2 manifest 绑定真实字节 → **写后语义 round-trip 校验**（重读
落盘字节断言空数组/null/条目数——把序列化正确性交给实测而不是版本号
假设）→ V1–V7/O1/O2/B4 全过才写容器 → zip（.NET ZipArchive，正斜杠
条目，14 条目精确序）→ 重开容器 R4/R5 复核 → SHA256SUMS sidecar +
package-summary.json。失败即 exit 1 且不留容器宣称。

**门禁第 1 轮失败（如实记录）**：门禁以 `powershell`（5.1.26100）调用，
被初版脚本的无据版本门拒绝（`requires PowerShell 7+ ... got 5.1`，
exit 1，已如实复现）。根因是**我的未验证假设**而非 5.1 缺陷：双宿主
序列化探针实测 5.1.26100 与 7.4.22 对本 manifest 全部敏感形态（空数组、
null、嵌套 ordered dict）语义逐一相同（空数组 `[]` round-trip 为
Object[]/count 0）。修复：撤掉版本门，代之以写后语义 round-trip 校验
（v-checks.json 新增 manifest_roundtrip=true，5.1 宿主实测）；同时移除
"无 -Force 拒绝重跑"的重放陷阱（改为默认重建 + previous_zip_sha256
留痕，本会话实测 rebuilt=true 记录了 pwsh 7 轮的 7c7f2477… 被 5.1 轮的
339c798d… 重建）。修复后双宿主真实全流程重跑各一次：
pwsh 7.4 → 7c7f2477…（1,458,011 B）PASS；powershell 5.1（门禁形态）→
339c798d…（1,448,964 B）PASS——两者差异仅为 deflate 流与 JSON 排版
（语义经 round-trip 断言等价），产物字节 1f5f8cf8… 与全部门检 identical。

初版容器（b2c222cd…，1,458,012 B，pwsh 7 构建）由此作废，不另留副本：
SHA256SUMS/summary/receipt 均为当前容器的一致快照。

## 7. 本地 pre-push 门（诚实记录：三轮 + 一次瞬时抖动 + 已知红继承）

`lefthook run pre-push --all-files`（PATH 含 tools/bin + xmake +
mingw64/bin；同一提交 6c52687，内容无变化，日志
`package/pr-b2-lefthook.log`——仅保留末轮，前两轮结果见下文实测记录）：

```text
round 1  227.8s   commit-convention ✔ vocabulary ✔ plugin-boundary ✔
                  rust-fmt ✔ rust-clippy ✔ rust-test ✔（183.3s）
                  🥊 release-policy（5.85s，exit 3，E14 已知差分）
round 2   81.3s   rust-test 🥊（29.95s）：-p qianqian-output-wasapi --lib
                  cone-evidence facility-leg 报
                  "render loop panicked"（timing 敏感 WASAPI 测试）
round 3  249.3s   与 round 1 同：6/7 ✔（rust-test 189.66s）+
                  🥊 release-policy（6.37s，E14 同一差分）
```

round 2 红的定性（不许掩盖，逐级实测）：隔离复跑
`cargo test -p qianqian-output-wasapi --lib` → **28 passed; 0 failed**
（该测试自身的 `[cone-evidence facility-leg]` 行是故障注入观察日志
`injected=true`，非失败）；round 3 全门绿。本通道 delta 为非 Rust 的
.ps1，三轮间无任何代码变化——round 2 定性为该 WASAPI 时序敏感测试在
门内并行负载下的宿主瞬时抖动（AGENTS.md concurrency-test-design 所述
类别），非本通道引入，非稳定回归。release-policy 红三轮均为
E14-real-ELF 同一已知宿主平台差分（§5，非本通道 delta 引入，处置不变：
E14 平台真值归 ubuntu-24.04 CI）。docs-verify 未达（被 piped 中止，
未运行——与 s2-build 记录一致）。

## 8. NOT_RUN（如实列出，不充当 PASS）

1. G2 CORE_RUNTIME 套件未在第二根产物（059eb84e…）上执行——L2 判词 c 项
   缺失，semantic_abi 因此 NOT_ESTABLISHED（已实测的 a/b/d 项记入回执）。
2. L1 新鲜环境端到端重建（fresh clone + 全新 closure 导入）——未执行。
3. T4 旗标实验（-ffile-prefix-map 等）——未执行；属 S3/owner（Q2）范围。
4. zip 容器自身的字节确定性——未设计未声称（L3 只约束 shipped
   library；容器哈希已如实记录，重放会得新容器哈希）。
5. metadata/external-consumer-example.c 在 windows mingw 下的编译冒烟
   ——未执行（包内文件为 S3 通道作者版逐字节复用，其头注链接行为
   linux 形态；windows 链接契约由 README 与 manifest.runtime_dependencies
   明示 -lbcrypt）。G1 消费冒烟证据属 D2/#325 记录，本通道未重跑。
6. PR-B 分支未推送、未开 PR（ask 未授权；stop-after-branch）。

## 9. 边界

- 未修改门代码/AGENTS.md/既有证据/契约；打包器为唯一新提交（6c52687，
  含门禁第 1 轮修复的 amend）。
- evidence/songcore-172/package/（zip、SHA256SUMS、stage 树、审计与
  V 检查回执、repro 回执、门日志）与判定文件留在盘上未提交（按战役
  惯例证据合并属后续 shipping 提交）。
- 版本 0.2.0-rc.1 是 provisional 回退：发布号仍 owner 保留（Q1）；
  本包是战役验证证据，不是 release（§11），不得以 songcore-v* 名义
  发布，除非 owner 批准该号。

## 10. Addendum — 评审 C（第 1 轮）修复记录（2026-10-29，本会话）

### F1 容器身份链与冻结声明

实测重建链（每跳均有落盘证据，非记忆重构）：

```text
b2c222cd…3aa7  初版容器（pwsh 7，1,458,012 B）——§6 门禁第 1 轮修复后作废
7c7f2477…ddb   pwsh 7.4.22 全流程重跑（1,458,011 B）——§6 双宿主验证轮，
               随后被 5.1 门禁形态轮重建（本判定 §6 原文记录）
339c798d…8400  powershell 5.1 门禁形态轮（1,448,964 B）——§6 原记为
               "最终产物"，被门禁第 2 轮重建取代
7b4d0615…6415  ★ 唯一现行容器（1,448,965 B，powershell 5.1，run
               20261029-070516）
```

最后一跳的机器证据 = `evidence/songcore-172/package/package-summary.json`
（实测读取）：
`zip_sha256=7b4d0615…6415`、`zip_bytes=1,448,965`、
`rebuilt=True`、`previous_zip_sha256=339c798d…8400`、
`source.commit=6c5268777cd2…`。更早各跳的 previous 记录未留存独立快照
（每次重建覆盖同一 summary 文件——已知的审计折衷），其存在性由本判定
§1/§6 原文与 §10 首版文字交叉印证；此处如实声明该证据边界。

**冻结声明**：自本 addendum 起，`7b4d0615d25af464739373a0a3565f77f3208
ce9c263f59e578d443801536415`（1,448,965 B）是唯一现行容器；**不再重跑
ops/package-rc.ps1**（重跑会产生新容器、打破已发布的身份引用）。一切
shipping 材料（PR body / evidence-merge / verify-external.ps1 / 下游
引用）**只引用 7b4d0615**，不得引用 339c798d / b2c222cd / 7c7f2477。
本会话实测：`sha256sum` 该 zip = `7b4d0615…6415`，与 SHA256SUMS sidecar
逐字节一致（两个独立工具链）。

配套修复（评审 C F1/F3）：`ops/verify-external.ps1` 的默认身份 pin 改为
**读取 zip 旁的 SHA256SUMS sidecar**（缺 sidecar / 无匹配行 / 行式畸形
均 FAIL，保持 fail-closed；`-ExpectedZipSha256` 仅作负向 receipt 的显式
覆写）。两份 receipt 实测落盘 `evidence/songcore-172/external-accept/`：

```text
负向  powershell -File ops/verify-external.ps1 -ExpectedZipSha256 000…0
      → exit 1（真实退出码，非管道尾码），transcript
        20261029-074123-external-log.txt 记录
        FAIL [zip-identity] sha256 mismatch
正向  子进程 PATH 仅 C:\Windows\System32 + -MediaFile
      native/experiments/songcore-equivalence/fixtures/flac-16-44-stereo.flac
      → exit 0，transcript 20261029-074149-external-log.txt：
        identity pin: from SHA256SUMS sidecar -> 7b4d0615…（证明默认
        pin 走 sidecar 而非任何硬编码），in-package checksums 13/13 OK，
        open→probe→decode→seek(50%)→EOF→close PASS，
        import table bcrypt.dll, KERNEL32.dll, msvcrt.dll 全在 README
        声明的 allowlist 内
```

### F2 release-blocking 记录（防 RC 证据被误读为可发布）

**RELEASE-BLOCKING**：本 RC 产物与 linux 侧 RC 产物在以下条件全部满足
之前，**不得产出任何 songcore-v\* 真发布**：

1. 契约 §14 Q2（debug-info / 路径策略：T4 的 -ffile-prefix-map 路线 vs
   strip/-g0 路线）由 owner 裁决；且
2. 按裁决方案实测消除"内嵌构建根绝对路径"差异类（T1–T5 重跑：两根
   产物字节级消除该类，或按 §12 判据确立相应层级），并保持 G0–G2 绿。

实测依据（本判定 §3，均已执行）：windows `1f5f8cf8…` 与 linux
`409ef3da…`（S3 判定 §4）两个 RC 归档均含 **8 个**内嵌构建根绝对路径的
FFmpeg 成员；路径数据在普通节（.rdata/.text 字符串常量，__FILE__ 类），
**非 DWARF debug info**（release 对象无 .debug\_\* 节）。这就是
manifest.reproducibility.byte=NOT_ESTABLISHED 的实测内容。

相互引用（防误读）：容器内冻结的
`metadata/manifest.json → reproducibility.byte = "NOT_ESTABLISHED"` 与
`reproducibility.evidence → evidence/songcore-172/package/repro/` 是
机器可读的一半；本节是人对可读的一半，并把它升格为**发布阻断规则**。
容器已冻结（F1），manifest 不可改——若未来按 Q2 裁决消除该差异类，须
在**新容器**的 manifest 中把 byte 判词改为实测结果，并在新 verdict 中
关闭本阻断；不得以任何方式改写本容器的冻结字节。RC 证据 ≠ 可发布证据；
"campaign verification evidence" 与 "release" 的界线由契约 §11 与本节
共同守卫。

### F5 源漂移说明（git_sha vs 分支 HEAD）

manifest/summary 的 `source.commit = 6c5268777cd2…`（打包门禁轮的
HEAD）比分支当前 HEAD `b461623` 早一个提交。本会话实测该提交的完整
内容：`git diff --stat 6c52687..b461623` = 仅 `ops/verify-external.ps1`
（+284 行，评审 C 修复对象）；`git diff --stat 6c52687..b461623 --
native/` 为**空**（构建输入零变化）。结论：容器绑定的 git_sha 与分支
HEAD 的漂移**仅**是打包后新增的验证 runner 提交，native/ 源树完全一致
——不构成源不洁，不要求重产容器（且 F1 冻结禁止重跑）。后续任何
evidence-merge 引用本容器时，请连同本节一起引用，防止漂移被误读。

### shipping 材料现状（如实）

`docs/release/` 现只有 `pr-a-body.md`（PR-A 的 body）；**PR-B body 尚
不存在**，evidence-merge 材料亦未创建——故本 addendum 是当前唯一
shipping 面记录，上列身份引用规则（只引 7b4d0615）与 release-blocking
规则对 PR-B body / evidence-merge 的作者构成约束。本会话 grep `docs/`
无任何 `339c798d`/`b2c222cd`/`7c7f2477` 引用（实测，无 stale 引用需清）。

### 本会话门（评审修复提交 a9159ef）

`lefthook run pre-push --all-files`（233.87s）：commit-convention ✔（含
a9159ef 头）vocabulary ✔ plugin-boundary ✔ rust-fmt ✔ rust-clippy ✔
rust-test ✔（183.44s）；🥊 release-policy（6.15s）= E14-real-ELF 同一
已知宿主平台差分（§5/§7，非本通道 delta 引入）。docs-verify 未达（被
piped 中止，未运行）。
