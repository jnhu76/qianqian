# SongCore G4 False-Green Elimination 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 让 SongCore 的构建与打包入口在 source、recipe、toolchain、artifact 或 receipt 不一致时 fail closed，并由 CI 中真实执行的负向控制防止回归。

**架构：** Linux 与 Windows 构建入口各自产生 schema 固定的 build receipt；打包入口只接受与请求源码及实际字节完全匹配的 receipt。`tools/test_songcore_release_substrate.py` 用临时 fixture 执行 M1–M10，工作流与 Lefthook 只负责发现并执行该测试，不引入新的发布 authority。

**技术栈：** Bash、PowerShell 7/Windows PowerShell、Python 3 标准库、GitHub Actions、Xmake/FFmpeg 现有 closure manifest。

---

### 任务 1：冻结缺陷与建立 RED 负向控制

**文件：**
- 创建：`tools/test_songcore_release_substrate.py`
- 修改：无

- [ ] 写入对 M1–M10、版本文法、manifest 类型和 CI trigger 的 fixture 测试。
- [ ] 运行 `python3 tools/test_songcore_release_substrate.py`，确认 main/PR #335 当前实现因缺失强制 receipt、旧包冒充和 trigger 缺口而失败。
- [ ] 保存未过滤输出、SHA、argv、退出码作为本次新 receipt。

### 任务 2：接入并收紧 Linux substrate

**文件：**
- 修改：`ops/build-linux.sh`
- 创建/修改：`ops/package-linux.sh`

- [ ] 接入 PR #335 的显式 full-SHA/branch/clean-root 基线。
- [ ] receipt 记录 source、tracked-input tree/blob identities、recipe/profile/pin/closure/toolchain、artifact hash/bytes、compile/cache-reuse 状态。
- [ ] release-qualified 模式强制 receipt；无 receipt 只能生成 `UNATTESTED_TEST_ONLY`，且验证器永久拒绝。
- [ ] 逐个运行 M1–M10，观察正确 RED；恢复 fixture 后观察 GREEN。

### 任务 3：修复 Windows artifact attribution

**文件：**
- 修改：`ops/build-rc.ps1`
- 修改：`ops/package-rc.ps1`
- 创建：`ops/verify-build.ps1`

- [ ] 先用 fixture 复现旧 archive 可被写成当前 commit 的 RED。
- [ ] build receipt 增加 source/header/recipe/profile/pin/closure/toolchain/target/artifact hash 与 bytes。
- [ ] packager 默认要求 receipt，并逐字段对照源码与实际 artifact；删除正式路径的 provisional 默认版本。
- [ ] 正式模式只接受 `MAJOR.MINOR.PATCH`；suffix 仅 `UNATTESTED_TEST_ONLY`。
- [ ] 运行 Windows 负向控制并恢复 GREEN。

### 任务 4：修复 closure recipe staleness

**文件：**
- 修改：`native/build/ffmpeg.lua`
- 测试：`tools/test_songcore_release_substrate.py`

- [ ] 先篡改 fixture recipe，确认当前 `manifest_target_problem` 错误接受。
- [ ] 比较 `target.recipe_sha256` 与当前 target recipe 的 SHA256，缺失或不等即拒绝。
- [ ] 验证 recipe mutation RED、恢复后 GREEN，并保持 #329 prefix-map 行为不变。

### 任务 5：建立 ops CI 与触发反事实

**文件：**
- 创建：`.github/workflows/songcore-release-substrate.yml`
- 修改：`lefthook.yml`
- 测试：`tools/test_songcore_release_substrate.py`

- [ ] workflow paths 覆盖 ops 入口、测试、recipe/profile/pin/schema/header和 workflow 自身。
- [ ] Linux job 运行 bash/static/fixture mutations；Windows job 运行 PowerShell parser 与 Windows fixture mutations。
- [ ] 测试模拟每个输入改变，断言至少一个对应 job/本地 gate 被选择。
- [ ] 运行 workflow 静态非空检查与全部负向控制。

### 任务 6：非规范分发与验证修正

**文件：**
- 修改：`ops/run-csuite.ps1`
- 修改：`ops/run-binding-gate.ps1`
- 修改：`ops/verify-external.ps1`
- 创建：`tools/docs-verify.cmd`
- 修改：包 README/notices 生成模板（位于双平台 packager）

- [ ] 删除陈旧默认 artifact hash/path，改为显式参数或 receipt 派生。
- [ ] G2 13-step 输出逐项 PASS/NOT_RUN，禁止 4/4 fixture 扩大为全部 G2。
- [ ] 修正 Linux relink、Windows `.so` 误引和许可证来源陈述。
- [ ] Windows docs wrapper 执行 `npm ci`（仅显式 bootstrap）、`verify.mjs` 和 VitePress build；保留真实退出码。

### 任务 7：验证、独立审查与 PR

**文件：**
- 创建：`evidence/songcore-172/g4-engineering/<run>/...`（仅最小文本 receipt）
- 修改：`evidence/songcore-172/g4/G4-OWNER-DECISION-PACKET.md` 或新增 erratum（不改历史原文）

- [ ] 运行 `python3 tools/test_songcore_release_substrate.py`、双宿主脚本语法检查、相关 build/package fixture、docs gate 和 `lefthook run pre-push --all-files`。
- [ ] 明确记录 PASS/FAIL/NOT_RUN/ENVIRONMENT_BLOCKED；禁止把宿主未执行的 gate 写成 PASS。
- [ ] Build/Provenance、ABI/Runtime、Release/Distribution 三位 fresh-context reviewer 审查真实 diff 与 receipt；修复全部 P1/P2。
- [ ] 按工程/规范边界拆 PR；只有已授权工程 PR 且实际 CI 对应 HEAD 全绿时才合并。
- [ ] 写回 #333；保持 `G4=NOT_APPROVED`、`RC_BUILD=NOT_AUTHORIZED`、`TAG=NOT_CREATED`、`RELEASE=NOT_PUBLISHED`。
