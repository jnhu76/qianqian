# PR-A 收尾独立复核 — release-policy E14-real-ELF 分类判定

- 复核角色: PR-A 收尾工程师（独立 subagent，只读复核；本文件为本次复核唯一新增产出）
- 日期: 2026-10-29
- 位置: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-a-build-fixes`（HEAD `bf23516b5851…`，base `origin/main`
  = `05f5feffaedc…`，领先 2 提交：`40ae32a1aaca` / `bf23516b5851`；以上
  `git rev-parse` / `git log` 实测）
- 复核对象: 前序船运员 PR-A fail-closed 判定（`docs/release/pr-a-body.md`
  开头判定块 + "脚本级门禁结果"§）

## 结论

**认同原分类**：`release-policy` 的 E14-real-ELF 控件是检查实现的
**Linux 宿主假设**——负例要求宿主 `gcc` 产出 ELF 对象，Windows 宿主 gcc
产出 `pe-x86-64`，期望的拒绝文案在本机结构性不可产生；属 authority
differential（检查的平台假设 vs 本地门禁宿主），**非本 PR 两个修复
（D1/D2）的缺陷**。fail-closed 处置（不改门禁代码、不以 shim 改换 gcc
语义）正确。

另有两点精确化（不改变结论，见 §5）。

## 1. 控件代码事实（逐行读取）

- `tools/test_release.py:119-122`：E14-real-ELF 用**宿主**
  `gcc -c main.c -o elf.o`（第 119 行，无交叉前缀）+ `ar rcs` 把产物塞进
  合成归档，然后要求 `check_release.py native` 以 exit 2 + 文案
  `POLICY_RED: R3 native archive target mismatch` 拒绝。
- `tools/test_release.py:23-30`：`call()` 对每个子进程判定**同时**断言
  exit code 与文案；文件头 docstring 第 4 行同义（"Every subprocess verdict
  must match both exit code and error class/property"）。
- `tools/check_release.py:117-119`：`objdump -f` 后
  `require(formats and all(f == 'pe-x86-64' for f in formats),
  'R3 native archive target mismatch')`。
- `tools/check_release.py:251-256`：native op 的执行顺序是
  `native_identity()`（含第 119 行格式检查）**先**于
  `R3 native archive/receipt drift` 的 receipt 对比。
- 结构性推论：若宿主 gcc 产出 pe-x86-64，负例归档成员全部满足第 119 行
  要求，格式检查通过，先触发的是后续判词（实测为 drift）——期望文案在
  Windows 宿主不可产生；只有宿主 gcc 产出非 pe 格式（Linux/ELF）时该负例
  才按设计触发。

## 2. 宿主格式对照（本人 2026-10-29 本机实测）

| 宿主 | 命令 | 结果 |
|---|---|---|
| Windows（WinLibs gcc 16.2.0，`/c/Users/fred1/tools/mingw64/bin` 加入 PATH） | `gcc -c t.c -o t.o`；`od -A x -t x1 -N 4 t.o`；`objdump -f t.o` | magic `64 86`；`file format pe-x86-64` |
| WSL Ubuntu-24.04（gcc 15.2.0，同一探针源码） | 同上（经 `wsl.exe -d Ubuntu-24.04 -- bash -lc`） | magic `7f 45 4c 46`；`file format elf64-x86-64`（`file`：ELF 64-bit LSB relocatable） |

与既有物证一致：`evidence/songcore-172/pr-a-host-gcc-object-format-proof-win-pe.o`
（本人以 `od` + `file` 复验：x86-64 COFF object file，非 ELF）。

## 3. 门禁来源与分工（git log + 配置读取）

- `lefthook.yml:152-163`：pre-push `release-policy` job（priority 7；glob 含
  `tools/*release*`、`native/**`、`lefthook.yml` 等），运行
  `python3 tools/test_release.py`。
- 来源：`git log -S release-policy -- lefthook.yml` → 唯一命中 `e44183e`
  "ci(release): share clean native pipeline and isolate publication"
  （2026-10-13 22:06:21 +0800）。
- `lefthook.yml:3-7` 头注自证分工：本 hook 是快速本地拒绝层；
  "GitHub Actions remains the authority for platform, formal (TLA+),
  Miri/Loom, mutation, Windows and **release evidence**."
- CI 侧：`.github/workflows/release-qianqian.yml:32`（`runs-on: ubuntu-24.04`）
  与 `:78`（`python3 tools/test_release.py --report
  dist/release-controls.json`）——E14 将在 Linux runner 上按设计执行并给出
  真实结果。

## 4. 运行记录核验（前序留档逐条读取）

- 完整门禁 `lefthook run pre-push --all-files`：`pr-a-lefthook.log` 与
  `pr-a-lefthook.attempt4-xmake-but-fail.log` 经 `cmp` 逐字节相同。summary
  （log 尾部 summary 段）：commit-convention ✔ 0.53s、vocabulary ✔ 1.67s、
  plugin-boundary ✔ 34.08s、rust-fmt ✔ 1.24s、rust-clippy ✔ 4.45s、
  rust-test ✔ 170.15s、release-policy ✘ 3.95s、docs-verify skip（piped
  中止未达）、EXIT_CODE=1。船运员"6/7 绿 + release-policy 红"计数与机器
  事实一致。
- 直接复跑链：`pr-a-release-controls-direct-run1-missing-ar.log`
  （`test_release.py:100` 处 `x86_64-w64-mingw32-ar` FileNotFoundError）→
  `...run2-e14-real-elf.log`：确定性推进到 E14-real-ELF，
  `AssertionError: E14-real-ELF: expected 2 'POLICY_RED: R3 native archive
  target mismatch'; got 2 POLICY_RED: R3 native archive/receipt drift`——
  exit code 相同、文案不同，被 §1 的双断言如实判红。

## 5. 精确化（不改变结论）

1. **两层证据应区分**：完整门禁 attempt4 里 release-policy 的红记录为
   `TOOLING_FAIL: control harness: [WinError 2] 系统找不到指定的文件。`
   （exit 3；lefthook hook 环境缺某个可执行文件，仅凭该日志无法定位具体
   exe）。E14-real-ELF 的确定性失败证据来自**直接复跑**（run2 traceback）
   + §1 代码顺序 + §2 宿主格式对照，三者独立成立。`pr-a-body.md` 已以
   "直接复跑"注记 E14 traceback 出处，无矛盾；后续引用时应区分两层来源。
2. **门禁清单对照**：AGENTS.md 的本地门禁枚举（always / Rust paths /
   docs-website）**不含 release-policy**；WORKBRIEF §6 只记录"未验证
   pre-push 门"。release-policy 由 e44183e（2026-10-13）加入
   lefthook.yml，机器事实晚于 AGENTS.md 的枚举文本。本 PR 推送 delta
   （`crates/qianqian-songcore-sys/build.rs`、`evidence/songcore-172/
   d1-counterfactual/**`、`native/build/songcore.lua`）命中 release-policy
   的 `native/**` glob、不命中 docs-verify 的 `docs/**`/`website/**`
   glob——真实 push 时 lefthook 收窄后的可运行检查恰为 7 项，与船运员
   "7 项中 6 绿"计数一致。AGENTS.md 枚举滞后于此记录在案，待 owner
   review（本复核不修改 AGENTS.md）。

## 6. 复核边界

- 本人未重跑完整 lefthook 门禁（attempt4 ≡ final 已 cmp 核验后引用既有
  日志）；未重跑 `tools/test_release.py` 全套（run1/run2 留档 + 代码/格式
  对照已足以定位结构根因）。
- 未修改任何门禁代码、检查脚本、AGENTS.md、既有证据文件。
- E14 的最终平台证据归 CI（ubuntu-24.04 runner），与 lefthook.yml:3-7 头注
  及 AGENTS.md 的本地门禁/CI 分工一致。
