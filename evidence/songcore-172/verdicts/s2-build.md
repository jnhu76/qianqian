# S2 构建通道判定 — Windows 发布通道（build-rc.ps1 + 契约定稿）

- 判定角色: Windows 发布通道工程师（#172 S2，独立 subagent 会话）
- 日期: 2026-10-29
- 位置: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`（基于 PR-A HEAD `bf23516`；
  本会话新增 2 提交：`62acc29` 契约定稿、`56b9a04` 编排器；
  `git log` 实测；tracked 文件干净，`git status --short` 无已跟踪变更）
- 上游输入: PR-A #326（OPEN）已修复的构建路线（`40ae32a` build.rs 哈希、
  `bf23516` recipe artifact-policy）；D2 已导入的 FFmpeg closure
  （WSL `/home/hoo/q172d2-verify/closure`，本会话实测：
  `ffmpeg-src/.qianqian-verified` =
  `bf1b838f…|d16837dd…` 与 pin.json 一致，manifest + oracle 齐备）。
  D1/D2 未重做。

## 结论

**S2 构建通道 PASS（本通道交付范围内）**，fail-closed 全程成立：

1. 契约定稿提交 `62acc29`：`docs/release/songcore-release-contract.md`，
   内容 = 草案 REVISION 2 状态（契约评审 APPROVE 轮的最终内容），
   仅改头部块（TRUTH CLASS/FINALIZED）、§14 标题（Owner 待决，Q1–Q5
   保留 owner 裁决）与 §15 定稿记录；`diff` 实测三处之外零差异。
   Owner 批准仍是权威行为，文中明示。
2. 编排器提交 `56b9a04`：`ops/build-rc.ps1`（667 行）。单入口 =
   环境预检（含 native/ tracked-clean 门 + 冻结 ABI header 常量核对）→
   固定 source SHA → WSL Ubuntu-24.04 xmake v2.9.7 mingw 路线 →
   静态归档 → PE/COFF 成员身份门（280/280 pe-x86-64、0 elf）+
   15/15 ABI 符号 + `songcore_shared` 负控 → artifact/header SHA256
   独立双算（WSL sha256sum vs Windows Get-FileHash）核对 → stdout
   机器可读 JSON（实测 3.3KB < 64KB）；详细日志落
   `evidence/songcore-172/build/rc/<stamp>/`。closure 只复用不导入
   （pin 标记逐次校验；缺 closure 即 FAIL 并给指引，绝不静默重导入）。
   重入安全（持久 WSL 工作根 + tracked 源 overlay 刷新 + 增量 build）；
   `-BuildDir` 第二独立根；子进程退出码逐级真实传播。
3. 真实运行共 5 轮（见 §2），产物、FFmpeg provenance、toolchain 身份
   全部落盘 evidence。
4. 产物不入库：`native/.gitignore:1` `/build/*`（仅豁免
   ffmpeg.lua/songcore.lua）——仓库惯例是构建产物不追踪、evidence 记
   hash+provenance（与 #324/#325 一致）。已在编排器提交说明中写明。
5. 本地 pre-push 门：6/7 绿；release-policy 红为**已处置的宿主平台
   differential + 门内 PATH 缺件**（见 §4，非本通道 delta 引入，
   本会话已把缺件根因定位到可复现命令）。

## 1. 产物身份（最终轮 20261029-042824）

```text
SOURCE        56b9a044aa4bfd7575b11981f797b70dce4fac8c（native/ tracked-clean）
ARTIFACT      C:\Users\fred1\source\qianqian-172\native\build\artifacts\libsongcore.a
SHA256        1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402
BYTES         3,309,710
FORMAT        objdump -f: 280/280 members pe-x86-64, 0 elf（build_and_validate.log）
ABI           x86_64-w64-mingw32-nm: 15/15 ` T song`（15 符号 ABI v1 面）
HEADER        sha256(native/include/songcore.h) = cf509a5c…78536
              == 冻结 ABI v1 身份（契约 §3），WSL/Windows 双算一致
SHA256 独立核对  WSL sha256sum == Windows Get-FileHash == Git Bash sha256sum
              （第三算：本判定会话 `sha256sum native/build/artifacts/libsongcore.a`
              实测同值）
MAGIC         `!<arch>\n`（PowerShell 逐字节核验）
```

同 SHA 复现：三轮回同根重建（042035 冷建 10.8s、042223、042824 增量
0.281s）产物 sha 全部为 `1f5f8cf8…` —— 同根增量重入不改变字节。

## 2. 运行记录（evidence/songcore-172/build/rc/<stamp>/）

| stamp | 结果 | 内容 |
|---|---|---|
| 20261029-041837 | FAIL（诚实记录） | xmake `--version` 管道输出带 ANSI 色码 → 版本钉扎匹配失败；fail-closed 拦截（summary.json steps.wsl_tools=FAIL）。修复：stage 内 strip_ansi |
| 20261029-042035 | PASS | 冷建全量：closure 经 `-ClosureSource` 复制（无导入），build ok spent 10.805s，280/0/15，负控 nonzero，SHA 双算一致 |
| 20261029-042223 | PASS | 重入（主根增量）+ `-BuildDir /home/hoo/q172rc-r2` 第二根冷建（spent 9.743s）：R2 sha `b4522631…c1ac`、3,309,742 B、280/0/15；第二根产物只落 evidence（消费路径未动）。同 commit `2c84d0f` 下 T1 双根 hash 对 = `1f5f8cf8…` vs `b4522631…`（bytes 不同——契约 §12 已记录的绝对路径字节不稳性；分类属 S3，本通道不判） |
| 20261029-042537 | FAIL（诚实记录） | 本会话自身回归：ASCII 化时误以旧 HEAD 为底，丢失 strip_ansi 修复 → 同 041837 症状；fail-closed 拦截。修复并 amend |
| 20261029-042824 | PASS（最终） | amend 后 HEAD `56b9a04`；增量 0.281s；全部门绿；summary.json 3,334 B；TOOL_XMAKE 行无 ANSI |

每轮目录含：stage-tools/prepare/build(+r2).sh 及 .log、config.log、
build.log、negative-shared.log、wsl_tools.log、summary.json（内嵌
source/ffmpeg_provenance/toolchain/artifacts 全块）。

## 3. Provenance（最终轮 summary.json 实录）

```text
FFMPEG   tag n9.0.1 | commit bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa
         | source digest d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746
         （三者 = pin.json，编排器内逐字段核对 CLOSURE_CROSSCHECK=OK）
TARGET   recipe windows-x86_64 | recipe_sha256 3ba0a9e6…8492d
         （三处一致：manifest.target.recipe_sha256 == WSL 本地文件 ==
          Windows Get-FileHash；xmake fail-closed 身份门逐次再验）
PROFILE  codec-base | sha256 57fb70ed…54fa（== 契约 §5 值）
CLOSURE  manifest sha256 0d0f7af9a6bf102be92cece254319f8b12261dd822feefa2f3125ca9ed0aed14
         | 279 TUs | closure toolchain {linux, x86_64, x86_64-w64-mingw32-gcc,
         "gcc 13-win32 (GCC)"}
         （该 manifest digest 与 #324 于 issue #172 实录的 closure manifest
          digest 逐位相同——closure 未漂移）
TOOLCHAIN（构建侧，WSL 实测）
         xmake v2.9.7+20250101（/home/hoo/.local/bin/xmake）
         cc x86_64-w64-mingw32-gcc (GCC) 13-win32
         ar GNU ar (GNU Binutils) 2.45.90.20260125
         ld GNU ld (GNU Binutils) 2.45.90.20260125
         python 3.14.4
```

## 4. 本地 pre-push 门（诚实记录：6/7 绿 + 已知红）

`lefthook run pre-push --all-files`（PATH：tools/bin + mingw64/bin；
280.83s；日志 `evidence/songcore-172/pr-b-lefthook.log`）：

```text
✔ commit-convention（校验 origin/main..HEAD 全部 4 提交，含本通道 2 个头）
✔ vocabulary      ✔ plugin-boundary      ✔ rust-fmt
✔ rust-clippy     ✔ rust-test（238.08s）
🥊 release-policy（3.50s，exit 3）→ docs-verify 被 piped 中止未达（skip）
```

release-policy 红的精确定性（本会话实测）：

1. 门内红灯（attempt 类）= `TOOLING_FAIL: … native-synthetic-baseline:
   expected 0 ''; got 3` + `FileNotFoundError: [WinError 2]`。本会话以
   进程内驱动（patch `check_release.run` 打印缺失 argv）定位根因：
   `tools/check_release.py:131` 的 `tools{xmake}` 身份探测
   `run(['xmake','--version'])` 在 Windows 宿主找不到 xmake——
   attempt3/4 安装的 Windows xmake 在
   `C:\Users\fred1\tools\xmake\xmake\xmake.exe`（实测存在，
   `xmake v2.9.7+HEAD.2bb99e0c3`），不在默认/门内 PATH。
2. 把该路径加入 PATH 后直跑 `python3 tools/test_release.py`
   （`pr-b-release-controls-direct-run3.log`）：通过
   native-synthetic-baseline 与 E14-hash，确定性推进到 **E14-real-ELF**
   并复现 PR-A run2 的同一 differential：
   `expected 2 'POLICY_RED: R3 native archive target mismatch'; got 2
   POLICY_RED: R3 native archive/receipt drift`——Windows 宿主 gcc 产出
   pe-x86-64，E14 负例期望文案在本宿主结构性不可产生（判定链：
   `verdicts/pr-a-close.md` §1/§2/§4）。处置不变：fail-closed，不改门
   代码；E14 的平台真值归 ubuntu-24.04 CI runner
   （lefthook.yml:3-7 头注 + `.github/workflows/release-qianqian.yml`）。
3. 本通道 delta（docs/release/*.md、ops/*.ps1）不含 release-policy
   输入；该红为 PR-A 既有状态的原样继承。注：piped 门下真实 push 仍会
   在 release-policy 停下（docs-verify 届时同样未达）——推送处置属
   shipping 阶段，不在本通道范围。

## 5. NOT_RUN（如实列出，不充当 PASS）

1. 契约 §12 T2–T5（双根逐成员 diff/根因分类）与 L1/L2/L3 判词（T6）
   ——本通道只产出 T1 双根 hash 对；分类与判词是 S3 执行。
2. 对本产物 `1f5f8cf8…` 的 G1 消费者链接冒烟与运行时
   `songcore_abi_version()==1` 断言——本会话未编译/运行任何 C 消费者；
   既有 link-contract（`-lsongcore -lbcrypt`）证据属 D2 产物。
3. G2 CORE_RUNTIME 13 步 parity 场景（S-BIND §7.1）——未执行；windows
   仍止于 BUILD/LINK 级证据（契约 §4 现状表述不变）。
4. G4 打包（zip 容器、manifest/checksums/symbols、§7/§8 校验）——未开始。
5. docs-verify（website verify + vitepress build）——被门 piped 中止
   跳过，本地未运行。
6. Windows 宿主（非 WSL）mingw 会话变体路线——未运行（本通道按 ask 走
   WSL 路线；recipe 声称同 recipe 覆盖 Windows 宿主，此主张未在本会话
   验证）。
7. L1 新鲜环境重建（fresh clone + 全新 closure 导入）——未执行；本通道
   复用 worktree 与 D2 已导入 closure（按 ask 约束）。
8. 共享/DLL 形态、MSVC、arm64 ——超范围（recipe artifacts.shared=false；
   契约 §4 明列 out-of-scope）。

## 6. 边界

- 未重做 D1/D2；未修改门代码、AGENTS.md、既有证据文件。
- 本判定文件与 rc/ 证据、pr-b 门日志留在盘上未提交（按战役惯例，
  证据合并属后续 shipping/evidence-merge 提交；本通道仅提交契约文档
  与编排器两个提交）。
- PR-B 分支未推送、未开 PR（ask 未授权；stop-after-branch）。
