# fix(songcore): make Windows static target builds deterministic and fail-closed (#172)

> **PR-A 判定（2026-10-29）**：完整本地门禁在本机 fail-closed，**未推送、未开
> PR**。阻塞点是 `release-policy` 检查的 E14-real-ELF 控件（详见
> "脚本级门禁结果"§）：该控件依赖 Linux 宿主工具链产生真实 ELF 对象，
> Windows 宿主上结构性不可通过（非本 PR 两个修复的缺陷；两个修复本身
> D1/D2 的 RED→GREEN 证据完整）。本文保留为战役后续开 PR 时的正文草稿；
> E14 平台假设须先经 authority review 处置（AGENTS.md authority resolution）。

## 本地门禁与 CI 的分工

本地完整门禁 7 项中 6 项全绿（commit-convention、vocabulary、
plugin-boundary、rust-fmt、rust-clippy、rust-test）；唯一红项是
`release-policy` 的 E14-real-ELF 控件——Linux 宿主假设，Windows 宿主上
结构性不可通过（详见"脚本级门禁结果"§），已按 AGENTS.md authority
resolution 记录（"遗留 finding" 2）待 owner review。

按仓库门禁分工（lefthook.yml 头注同文），GitHub Actions 是平台/平台矩阵
证据的 authority：E14 控件将由 CI 在 Linux（ubuntu-24.04）runner 上给出
真实结果（`.github/workflows/release-qianqian.yml` 运行
`python3 tools/test_release.py --report dist/release-controls.json`）。

分支 `songcore-172/pr-a-build-fixes`，base `main`（`05f5fef`）。修复 issue #172
（SONGCORE-MULTI-TARGET-RELEASE-1）战役确认的两个 Windows 静态目标构建缺陷
（D1 / D2，均由 #324 战役实录确认、本 PR 修复）。不含 ABI/语义变更；
`native/include/songcore.h` 未动；不主张任何 RELEASE 成熟度。

## D1 — build.rs 溯源摘要缺陷（commit `40ae32a`）

**根因**：`crates/qianqian-songcore-sys/build.rs` 用外部 GNU coreutils
`sha256sum` 计算溯源摘要并取输出第一个空白分隔 token。任何含反斜杠的文件名
（即所有 Windows 原生绝对路径）都会触发 coreutils 转义格式
（`<digest>  <escaped-name>` 前置反斜杠），因此 Windows 上
`QN_SONGCORE_HEADER_SHA256` / `QN_SONGCORE_ARTIFACT_SHA256`
（`src/lib.rs` 的 `env!()` 常量）带前置 `\`——65 字符而非 64 hex。
另外 PATH 上没有 `sha256sum` 时整个构建 panic（exit 101）。
即 #324 `ENVIRONMENT.md` out-of-scope observation #1。

**修复**：摘要改为进程内计算——纯 std 的 FIPS 180-4 SHA-256，每次
build-script 调用先跑 4 组 NIST 测试向量自检（不匹配即 panic，fail-closed）。
不引入任何 build-dependency：crate 级 `Cargo.lock` 字节不变、workspace 解析
不受影响、无新许可面；全部 cargo 指令（含 rerun-if-changed）与修复前逐字节
一致。`sha2` 依赖方案被评估并否决（约 8 个传递依赖 + 脆弱排除 workspace
区域的网络拉取，无正确性增益）。

**证据**（`evidence/songcore-172/d1-counterfactual/`，可重放
`run.sh red|green`）：修复前快照（base `05f5fef` 的逐字节 build.rs 快照）与
修复版在受控 native 树上对照：

- Windows：RED 35/35 → GREEN 47/47；WSL：RED 35/35 → GREEN 47/47。
- 覆盖反斜杠路径 / 空格路径 / 非 ASCII 路径 / 无 sha256sum / cargo
  rerun-if-changed 全序列（cold、fresh、untracked、header 变更、还原），
  results/ 下 40 个逐例结果文件。
- RED 机制实证：raw `sha256sum` 输出行以 `\` 开头、文件名转义
  （`windows-red-case-a-mechanism.txt`）；case d 为 panic 实录。
- 摘要双重 oracle（coreutils + hashlib）一致；跨 Windows probe / WSL probe /
  双侧真实 cargo 构建逐字节一致（header `cf509a5c…8536`、artifact
  `322dbf2e…`）。

**记录在案的语义变化**：case d（无 sha256sum）从 RED（panic）变为 GREEN
（构建完成）——这是修复的预期后果，摘要契约（64 位小写 hex）不变且经双
oracle 复验。

## D2 — artifact policy 不跟随目标 recipe（commit `bf23516`）

**根因**：`native/build/songcore.lua` 的 `songcore` phony 无条件依赖
`songcore_static` + `songcore_shared`，而 windows-x86_64 recipe
（`native/ffmpeg/targets/windows-x86_64.json`）声明
`artifacts.static=true / artifacts.shared=false`。static-only 会话被拖进
`songcore_shared` DLL 链接，死于 FFmpeg 闭包的
`BCryptOpenAlgorithmProvider` / `BCryptGenRandom` /
`BCryptCloseAlgorithmProvider`（唯一引用成员
`libavutil/random_seed.c.obj`），此时 static 侧已产出部分产物
（#324 路线实测 `xmake build songcore` exit 255，缺陷 D2）。

**修复**：artifact policy 完全由目标 recipe 驱动：

- `songcore_static` / `songcore_shared` 各加 `before_build` 门：被构建的
  形态必须被会话 recipe 启用（`artifacts.<form> = true`），否则以 recipe
  判词 fail-closed，不再链接半配置产物。
- phony 在 `on_config` 从**同一** recipe 解析依赖集，未启用形态根本不进
  构建。`on_config` 是必须的语义：在 `on_load` 加 deps 会强制全项目重载，
  其沙箱缺少 action-scope 环境（实测）。
- recipe 未启用任何形态（或无匹配 recipe）的会话落到
  `songcore_artifact_gate` fail-closed 终点，`before_build` 给出精确原因
  （raise 不能放 dep-解析钩子——它也会在普通 `xmake f` 期间运行；实测
  见 `evidence/songcore-172/semantics/` 的 sem 矩阵与
  `evidence/songcore-172/semantics/` 输出）。

**证据**（WSL xmake v2.9.7，Ubuntu 26.04 宿主，mingw cross
`x86_64-w64-mingw32-gcc 13-win32` / 宿主 gcc 15.2.0）：

- windows-x86_64：`xmake build -r songcore` exit 0，产物恰为
  `libqianqian_av.a` + `libsongcore.a`（280 pe-x86-64 成员、0 elf、15 个
  `song*` ABI 符号）；显式 `xmake build songcore_shared` exit 255 并给出
  recipe 判词（fail-closed 反向控件）。
- linux-x86_64：修复前后产物集逐字节相同（`libqianqian_av.a`、
  `libsongcore.a`、`shared/libsongcore.so` sha256 相等），shared 导出面
  15 个 `song*` 符号——shared 形态零回归。
- 无 recipe 会话 fail-closed（sem9：no-matching-recipe → exit 255 精确
  判词；正控件确认 FFmpeg closure 门独立生效）。
- 完整 Windows 路线回放：`ops/verify-build.ps1`（preflight → WSL closure
  import/复用 → mingw 构建 → 产物校验 → fail-closed 反向控件，输出机器
  可读 JSON summary）。回放记录
  6 次 `build/verify` 运行的 summary（原始 JSON 已按 2026-11-01 证据精简移除，
  其中 1 次为修枝期间的真实 FAIL 记录，随后复跑全 ok）。

## 脚本级门禁结果（fail-closed，未过）

- 战役编排器此前已真实执行 `ops/verify-build.ps1` 与
  `cargo test --workspace`（exit 0）。
- 本 PR 开出前在本 worktree 真实执行
  `lefthook run pre-push --all-files`（完整门禁，非收窄）：**未通过
  （exit 1），fail-closed**。判定依据如下；每次原始输出全部留档于
  `evidence/songcore-172/`：
  - attempt 1（`pr-a-lefthook.attempt1-stub-python.log`）：默认 PATH 的
    `python3` 是 Windows Store 存根（exit 49），`commit-convention` 在
    第一个检查即停——WORKBRIEF §1 记录的既知环境项。
  - attempt 2（`pr-a-lefthook.attempt2-no-mingw-path.log`）：`mingw64/bin`
    不在 PATH，`release-policy` 的 E13 控件（`tools/test_release.py`
    调用 `x86_64-w64-mingw32-gcc` / `objdump`）FileNotFoundError
    （直接复跑 traceback 留档
    `pr-a-release-controls-direct-run1-missing-ar.log`）——既知环境前置。
  - attempt 3（`pr-a-lefthook.attempt3-no-xmake.log`）：合成 native 控件
    经 `tools/check_release.py` 的 tools census 调用 `xmake --version`，
    Windows 侧无 xmake（WORKBRIEF §1/§4 记录"未装"）。已按官方 release
    （v2.9.7 win64 zip；sha256 与官方 `.sha256` 文件机械比对一致）用户级
    安装至 `C:\Users\fred1\tools\xmake` 并入 PATH。
  - attempt 4（`pr-a-lefthook.attempt4-xmake-but-fail.log` + 现行
    `pr-a-lefthook.log`）：补齐 `x86_64-w64-mingw32-ar` 标准别名（WinLibs
    只带 `ar.exe` 与 `gcc-ar` 系，完整 mingw-w64 工具链本就有此前缀别名；
    在 `tools/bin` 以同一 binutils ar 二进制补齐）后，FileNotFoundError
    全部消除，harness 推进到 **E14-real-ELF 控件并确定性失败**：
    `expected 2 'POLICY_RED: R3 native archive target mismatch'; got 2
    POLICY_RED: R3 native archive/receipt drift`（直接复跑
    `pr-a-release-controls-direct-run2-e14-real-elf.log`）。
- **结构性根因（环境修复不可达）**：E14-real-ELF
  （`tools/test_release.py` E14 段）用**宿主** `gcc -c` 产出"异构目标
  对象"塞入归档，期望 `check_release.py` 的探针（`objdump -f` 后
  `all(f == 'pe-x86-64')`）以 target-mismatch 拒绝。实测：Windows 宿主
  gcc 产出 `pe-x86-64`（`pr-a-host-gcc-object-format-proof-win-pe.o`），
  WSL 宿主 gcc 产出 `elf64-x86-64`（对照实测）。即该控件要求宿主工具链
  本身产生 ELF——Linux 宿主假设，Windows 宿主上无论装什么工具都无法
  产生该控件期望的拒绝文案。这是检查实现与本地门禁宿主平台之间的
  authority differential（本地门禁定位 vs 检查的平台假设），须按
  AGENTS.md authority resolution 处置；本 PR 不越权修改门禁代码或以
  shim 改换 `gcc` 语义绕过。
- attempt 4 实际通过项：commit-convention ✔、vocabulary ✔、
  plugin-boundary ✔、rust-fmt ✔、rust-clippy ✔、rust-test ✔（170s）；
  release-policy ✘（E14-real-ELF）；docs-verify NOT_RUN（lefthook 因
  release-policy 失败中止，四轮 attempt 均未到达）。
- 本门不证明的（AGENTS.md 列举）：Windows cfg(windows) 之外的
  MSVC/arm64 编译、TLA+/TLC、Miri/Loom、WASAPI 设备证据。

## 独立评审

本分支两个修复已过战役独立评审：评审已执行且**无必须修复项**（终态非
NO_GO；编排脚本控制流保证——否则不会进入开 PR 阶段）。评审员被要求逐项
核查的清单包括：D1 五种反事实是否真实 RED、修复是否平台无关且
rerun-if-changed 语义正确、是否零新增依赖、D2 是否严格遵循
windows-x86_64.json 的 artifact policy、Linux shared 不回归、有无
`|| true`/吞错/伪成功、consumer link contract 是否明确可重放、是否越权
改动主工作树/API/ABI、提交头部是否符合仓库约定。

**如实声明**：评审结论原文未随战役材料持久化（仅存在于编排会话
journal）。这是战役编排流程缺陷，已列为改进项：后续阶段所有 verdict
（评审/门禁/契约）必须落盘到 `evidence/`。

## 遗留 finding（来自战役工件与本 PR 门禁实测，未在本 PR 处理）

1. **`.cargo/config.toml` 的 `-lbcrypt` 耦合**（link-contract §2 风险注）：
   `qianqian-songcore-sys` build.rs 对 windows 目标只发
   `cargo:rustc-link-lib=static=songcore`，`-lbcrypt` 来自仓库级
   `.cargo/config.toml` 的 rustflags（其注释把该 flag 归因于 rustc 自身
   windows-gnu 运行时）。两个消费方都是真实的、相互独立的。若 #172 发行
   工程认为该 flag 仅为 rustc 运行时服务而移除，windows-gnu 构建将死于
   恰好三个 BCrypt 符号。处置二选一：保留 flag 并写明双重理由，或把
   `cargo:rustc-link-lib=bcrypt` 移入 build.rs（D2 刻意不动 build.rs——
   D1 拥有它）。
2. **本地门禁的 Linux 宿主假设**（本 PR fail-closed 的直接原因）：E14-
   real-ELF 控件使 `release-policy` 在 Windows 宿主结构性不可通过
   （见"脚本级门禁结果"）。native/** 变更因此无法在本机走完完整
   pre-push 门——需要 authority review 决定：检查改为宿主无关（例如以
   WSL 交叉产出异构对象，或把 release-policy 声明为 CI-only），或本机
   门禁范围显式收窄。本文档不作决定。
3. **D1 harness 的覆盖边界**（d1-counterfactual README "Not covered"）：
   digest 级与 cargo 集成验证使用受控输入树；真实 `libsongcore.a` 的
   链接集成由 D2 路线覆盖，两者证据等级不同，不互相替代。
4. **artifact sha 的路径依赖**（#324 已知，本 PR 未改变）：合并归档 sha
   嵌入绝对源路径，换目录重建即变；稳定身份是
   `native/src/songcore_ffmpeg.c` 的 sha + closure manifest。
5. **release contract 草稿的 OPEN 项**（`docs/release/songcore-contract-draft.md`
   §14，DRAFT 非 authority）：v0.1.0 后续版本机制、SongCore 发布流水线
   保护等 5 项留待 S1 freeze。

## Consumer link contract（实测，`evidence/songcore-172/link-contract.md`）

| 目标 / 形态 | 契约 | 依据 |
|---|---|---|
| windows-x86_64 static | `-lsongcore -lbcrypt`，此外无任何 `-l` | ABI-complete consumer 加 `-lbcrypt` exit 0；不加则恰好 3 个 BCrypt 符号 undefined；**最小** consumer（仅引用 `songcore_abi_version`）不加 `-lbcrypt` 同样失败——成员级 pull-in 必然拉入 `random_seed.c.obj`（唯一 BCrypt 引用成员），不因"不触碰 RNG"而豁免 |
| linux-x86_64 static | `-lsongcore -lm -lpthread` | 去 `-lm` → cos/exp undefined；全去 → cos/exp/atan undefined；`-lpthread` 在新 glibc 是无害 no-op、旧 glibc 必须 |
| linux-x86_64 shared | 启用；导出面恰 15 个 `song*` 符号 | `-Wl,--exclude-libs,ALL` 强制闭包内化 |
| windows-x86_64 shared | recipe 声明不支持；显式构建 fail-closed（recipe 判词） | 未来若启用，DLL 链接必须加 `-lbcrypt`，否则死于与 pre-fix RED 完全相同的三个符号 |

当前 Rust consumer 不违约但满足方式是间接的（见遗留 finding 1）。

## Known issues

- **本机完整门禁在本机不可达 GREEN**（E14-real-ELF 平台假设，见上）——
  PR-A fail-closed 的精确阻塞点。
- 本机环境前置（均非仓库缺陷，已留档）：默认 PATH 的 `python3` 是
  Store 存根（需 tools/bin shim）；`release-policy` 需要 mingw64/bin、
  `x86_64-w64-mingw32-ar`（本机 WinLibs 打包缺该标准前缀别名，已以同一
  binutils ar 二进制在 `tools/bin` 补齐）与 xmake（本机原无，已按官方
  v2.9.7 win64 release 用户级安装，sha256 与官方摘要机械比对一致）；
  `docs-verify` 需要 `cd website && npm ci`（lefthook.yml 注释记录的
  门外一次性步骤）。
- WSL 在 /mnt/c 上 mtime 漂移会使 cargo 指纹检查偶发误判
  rebuild（d1 README "Environment caveats"）；native 构建路线本就按 #324
  惯例把构建根放 WSL 本地文件系统。
- Windows shared 形态不被 recipe 支持是设计决定，显式构建的失败与判词
  是 fail-closed 行为，不是缺陷。
- 本 PR 不主张：MSVC/arm64/shared-DLL Windows 产物、打包/release 矩阵
  （归 S2/S3）、CORE_RUNTIME 或任何 LINK 以上成熟度、Playback Session/D11
  挂载。产物为 verification-grade。

## 证据索引（本 worktree，未提交部分随 PR 前置归档）

- D1：`evidence/songcore-172/d1-counterfactual/`（README + run.sh + results/
  下 40 个逐例结果文件，含双 host RED/GREEN 全矩阵）
- D2 构建/回放：`evidence/songcore-172/build/`（red/green 路线脚本与日志、
  verify/ 六次 summary 的结论）、`evidence/songcore-172/semantics/`
  （sem 矩阵）、`evidence/songcore-172/semantics/`
- link contract：`evidence/songcore-172/link-contract.md` +
  `evidence/songcore-172/audit/`
- 门禁证据（日志已按 2026-11-01 证据精简移除，结论见
  `evidence/songcore-172/RESULTS.md`）：`pr-a-lefthook.log`（attempt 4 终态）+
  attempt 1–4 原始日志 + `pr-a-release-controls-direct-run{1,2}-*.log`
  （traceback）+ `pr-a-host-gcc-object-format-proof-win-pe.o`（格式实证）
- 一次性回放工具：`ops/verify-build.ps1`
