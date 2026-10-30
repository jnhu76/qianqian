# SongCore #172 — G4 Owner 决策包

> **文件性质**：Owner 决策输入，不是发布授权，不是架构/发布权威。本文件不替 Owner 做任何决定。
> **日期**：2026-10-30
> **BASE_SHA = CURRENT_MAIN_SHA**：`69ba1ce05a2772f5aa077c6ece3365c40f6e0500`
> **PREFLIGHT_VERDICT**：`ENVIRONMENT_BLOCKED`
> **发布合约**：`docs/release/songcore-release-contract.md`，sha256 `c26611f72bdbe510fb0baf1f8dd331f71fb12e52962df456ef81dda26b9559a4`，OWNER-RATIFIED（`docs/README.md:17`；合约头 `:4-16`）
> **未决计数**：`openDecisions = 7`（Q1、Q2-正式 closeout、Q3、Q4、Q5、D1、D3）
> **修订**：v2（2026-10-30）。已吸收 A/B/C 三方独立评审的全部 22 条意见（2 high / 10 medium / 10 low）；逐条处置见 §8。**Owner 决策字段（Q1–Q5、D1、D3 的状态）一字未改。**

---

## 0. 阅读约定与执行边界

### 0.1 事实来源分级

| 级别 | 含义 | 本包如何引用 |
|---|---|---|
| A | 本包作者在本会话亲自读取或复算 | 标注“本包复读/复算”并给出 `path:line` 或命令 |
| B | 本轮 lane 回执（任务下发时提供的权威数据） | 引用 `evidence/songcore-172/g4/<lane>/` 与 lane 的 `stage.json` / `verdict` |
| C | 既有归档（v0.1.0 发布、#329 campaign） | 引用归档路径 + sha256；不作为 release authority |

**v1 作者实际执行过的动作**（仅此；v2 定稿轮的只读复核另见 §0.4）：

```text
git rev-parse HEAD
    -> 69ba1ce05a2772f5aa077c6ece3365c40f6e0500
git status --porcelain --untracked-files=no | wc -l
    -> 0
git tag -l 'songcore-v*'
    -> songcore-v0.1.0
sha256sum docs/release/songcore-release-contract.md native/include/songcore.h native/xmake.lua
    -> c26611f7...559a4 / cf509a5c...78536 / b420fc6c...e63
```

并读取了 `docs/release/songcore-release-contract.md`、`native/include/songcore.h`、`native/src/songcore_ffmpeg.c`、`native/build/ffmpeg.lua`、`native/build/songcore.lua`、`native/xmake.lua`、`native/ffmpeg/targets/*.json`、`docs/releasing.md`、`docs/README.md`、`docs/architecture/registry.yml`、`lefthook.yml`、`evidence/songcore-172/q2-crossroot/{README.md,t4-verdict-and-L.md}`，以及本轮预检包的 `metadata/manifest.json` 关键字段。

**v1 本包没有执行**：任何构建、链接、运行时、打包、发布、docs-verify、lefthook、E01–E18、gh/GitHub 操作。**v2 定稿人同样没有执行**任何构建、链接、产品运行时、打包、发布或 GitHub 写操作；v2 只做了 §0.4 列出的只读复核（hash / `ls` / `strings` / JSON 统计 / 包内容只读审计）。所有此类结论仍一律引用 lane 回执；lane 的 `NOT_RUN` 原样保留（见 §4、§6）。

### 0.2 真值边界

- `evidence/` 是非权威证据树（`evidence/README.md:3`）；本包与 `preflight-summary.json` 同为证据文件。
- 唯一发布权威是 `docs/release/songcore-release-contract.md`；本包不新增、不修改任何架构语义或发布规则。
- PASS 只表示“该 lane 在本轮真实执行并返回 PASS”，不表示本包执行过；NOT_RUN 一律不写成 PASS。

### 0.3 本轮硬约束（已遵守）

```text
未创建 tag；未创建 GitHub Release；未改动 songcore-v0.1.0 任何历史资产
未选择任何**发布**版本号（未把 0.1.1 / 0.2.0 / 0.2.0-rc.1 / 任何其他数值宣布为下一 release identity；
        lane 里的 0.2.0-rc.1 是入口审计的 caller-supplied provisional 参数，不是 release 选择）
未修改 native/include/songcore.h、native/src/songcore_ffmpeg.c、native/xmake.lua、
        docs/release/songcore-release-contract.md、发布权限或 main 上的产品代码
未把 Windows DLL 纳入目标；未声称任何 Ubuntu/Debian/Alpine 版本已受支持
未绕开 D1/D3 为发布放行
```

### 0.4 定稿轮（v2）只读复核执行记录

以下命令在本会话由定稿人真实执行；结果按观测记录（均为只读，未改产品代码、未重建、未发布）：

| 检查 | 命令 | 观测结果 |
|---|---|---|
| RC 命名容器盘点 | `find evidence/songcore-172/g4 \( -name '*.zip' -o -name '*.tar.gz' \)` | 5 个容器：2 个 preflight + 3 个 RC 命名（`e0c488a9…` / `7e843c9a…` / `c3983bfb…`） |
| Linux lane 根存在性 | WSL `ls -ld /home/hoo/g4-172/linux-root/src /home/hoo/g4-172/linux-root/receipts/g4-linux-build` | 两者均 `No such file or directory` |
| Linux 产物字节 | WSL `sha256sum /home/hoo/g4-172/linux-root/native/build/artifacts/{libsongcore.a,shared/libsongcore.so}` | `0d38f4b5…` / `085425e5…` |
| 包内产物 | `sha256sum package/linux/extract/*/lib/libsongcore.a package/linux/extract/*/lib/shared/libsongcore.so` | 同上，字节一致 |
| 包 build receipt | WSL `sha256sum /home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json` + 读字段 | `1f574edb…`；`entry=ops/build-linux.sh`、`clone=reuse`、`closure=reuse`、`source_repo=/home/hoo/g4-172/wt-tools`、`work_root=/home/hoo/g4-172/linux-root` |
| `ops/build-linux.sh` blob | WSL `sha256sum /home/hoo/g4-172/wt-tools/ops/build-linux.sh` | `18dba884…`（未记入该 receipt） |
| lane 回执自包含性 | WSL python 对 `build-linux/stage.json` 的 64 对 path/hash 求值 | 42 条绝对 WSL 路径按原样不可解析；映射到 `build-linux/logs/` 后 64/64 hash 匹配、0 mismatch；`evidence_sha256[0]=5c3e459c…` 对应 `build-linux/logs/clone.log`，不是列出的 WSL 路径 |
| Linux 归档内嵌 FFmpeg 版本 | `grep -a -o 'FFmpeg version [^ ]*' package/linux/extract/*/lib/libsongcore.a` | `FFmpeg version qianqian-v0.1.0-315-gbf23516` |
| Windows 归档内嵌版本 | 同命令作用于 windows 包 | `FFmpeg version 9.0.1` |
| in-build recipe 校验 | `grep -n 'recipe_sha256' native/build/ffmpeg.lua` | 无命中（exit 1）；`:48` 的注释声称 outdated recipe 会 stale，但函数未比较 recipe hash |
| 合约锚点 | `grep -n 'all existing v0.1.0 field names keep their meaning\|tag form            songcore-v\|^G2  CORE_RUNTIME' docs/release/songcore-release-contract.md` | `:342` / `:96` / `:586` |
| D1 断言数 | `grep -c MATCH d1-out-handle/run.log`；`grep -n 'report(' d1-out-handle/d1_probe.c \| wc -l` | 10；11（1 个定义 + 10 个调用） |
| E01–E18 label 数 | WSL python 读 `release-policy/release-controls.json` | 69 次执行；43 个非空 `id`；23 次空 `id`；0 个 `exit_code != expected` |
| Linux corpus run 数 | `grep -o '"static"\|"shared"' consume-linux/out/comparison.json \| sort \| uniq -c`；`consume-linux/stage.json` argv 计数 | 4/4（受门禁 8 次）；argv 中 10 次 `consume-corpus` 调用 |
| GitHub 读取能力 | `cat p0-governance/auth-read.log`；`cat p0-governance/auth-summary.log` | 6 次 HTTP 200；`permissions {'admin': True, …}`；releases total=2 |
| Windows host 捕获 | `sed -n '1,10p' lane-c-windows-pe/run/run-all-windows.log`；`cat lane-c-windows-pe/run/host.txt` | 主日志 `:4` 为 `host_windows=Binary file (standard input) matches`；`host.txt` 是正确捕获 |
| Windows 重编译范围 | WSL `find /home/hoo/q172rc/win/native/build/.objs -name '*.obj' -newermt '2026-10-30 20:00' \| wc -l`；`stat …/songcore_ffmpeg.c.obj` | 279/280；被复用者 mtime `2026-10-29 04:20` |
| G2 Windows 场景覆盖 | `grep -n 'garbage\|trunc\|reopen' lane-c-windows-pe/src/pe-consumer.c` | 无命中 |
| T1–T5（本轮 preflight 容器） | WSL `python3 release-policy/check_package_security.py <win zip> --expected-root songcore-v0.0.0-preflight-windows-x86_64 --expected-sha256 5d92b21d…`；linux tar.gz 同（`bfe780c3…`） | 两者 exit 0、`findings=0`、T1–T5 与 `sha256_expected` 全 `true` |
| 旧归档 hash（C-1 对照） | `sha256sum evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip`；WSL `sha256sum /home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz` | `7b4d0615…` / `018e0cbd…`（与 release-policy receipt 记录一致） |
| V1 集合算术（定稿人重走） | WSL python 对两个 preflight 解包树重算 `files` / `checksums` / `files{}` 并逐条 re-hash；`sha256sum -c metadata/checksums.txt` | win 14/13/12、linux 15/14/13；集合相等；0 mismatch；`sha256sum -c` 分别 13 OK / 14 OK |
| 分支打包器语法 | WSL `git show ops/songcore-linux-release-substrate:ops/package-linux.sh \| nl -ba \| sed -n '138,148p'` | `:141-142` 接受 `[-+.0-9A-Za-z]+` 并称其为 “contract section 2 VERSION_RULE form” |
| 三份 manifest 的 `artifact_shared` | `grep -n artifact_shared` 两个 preflight 包；WSL `tar -xOzf /home/hoo/s2pkg-v010/...` | win `:19` boolean `false`；linux `:19` string path；v0.1.0 `:19` string path |

---

## 1. 决策摘要

| ID | 主题 | 状态 | 本包推荐（不替 Owner 决定） | 阻塞性质 |
|---|---|---|---|---|
| Q1 | 下一版本号 + tag 形式 + xmake 等式 | `UNDECIDED` | 流程建议：先定“无后缀还是允许后缀”，再走版本 bump PR + 机器等式检查；**不提名数值** | Owner 授权 + 工程 |
| Q2 | #329「保留调试信息 + `-fmacro-prefix-map`」正式登记 | `RECORDED_ACCEPTANCE_PENDING_FORMAL_CLOSEOUT` | A：合约 §12/§14 + registry 证据正式登记 | Owner 文本审阅 |
| Q3 | Windows 本轮 static-only 还是新增 DLL | `UNDECIDED` | A：static-only（target json `shared=false` 不变） | Owner 授权 |
| Q4 | Linux 最低 glibc / 支持发行版范围 | `UNDECIDED` | A：把实测 `GLIBC_2.43` 声明为共享产物的最低要求，**不作发行版支持声明**；若要旧发行版可用则走 B 重建基线 | Owner 政策 |
| Q5 | 自动发布工作流 vs 人工发布 | `UNDECIDED` | A：人工发布 + G4 书面门禁 + 运行手册（当前无 SongCore 发布器） | Owner 授权 |
| D1 | `song_open` `*out_handle` 契约差异 | `UNDECIDED` | A：改 normative header 文字（`song_open` 与 `song_probe` 两处错误列表都补 `SONG_ERR_INVALID_ARGUMENT`） | Owner 授权（normative ABI 文本） |
| D3 | `song_read_pcm` 双守卫未清零差异 | `UNDECIDED` | D：A（代码修复）+ 把承诺写全 + 探针进入发布门禁 | Owner 授权（编译语义） |

> `openDecisions = 7`：Q2 的**实质结论**已在 #329 被接受，但其正式 closeout（合约文本 + registry 证据登记）尚未落地，因此本包仍把它计入未决数；它不是重新讨论“要不要保留调试信息”。

---

## 2. 本轮实测基线（关键数字）

> 本节数字全部来自本轮 lane 回执（B 类），除标注“本包复算”者。完整回执见 `evidence/songcore-172/g4/<lane>/stage.json`（`package/` 无 stage.json，见 §4.2）。

### 2.1 源码、合约与 ABI 身份

```text
source_sha                69ba1ce05a2772f5aa077c6ece3365c40f6e0500
baseline_drift            no
two_repos_mismatch        none
contract sha256           c26611f7...559a4（本包复算）
header sha256             cf509a5c...78536（本包复算；== 合约 §3:135 == v0.1.0 manifest == rc.1 包头）
SONGCORE_ABI_VERSION      1u（native/include/songcore.h:62）
export surface            15/15；非 API 顶层函数声明 0；与合约 §3:140-146 集合差为空
xmake set_version         0.1.0（native/xmake.lua:1-2；本包复算 sha b420fc6c...e63）
songcore tags             仅 songcore-v0.1.0（本包复算）
v0.1.0 tag commit         522c16d283f024c01d1ef56fa96e587b286ca3b8（annotated tag object a1b6c99...）
v0.1.0 asset              songcore-v0.1.0-linux-x86_64.tar.gz sha256 018e0cbd...6815, 2,588,376 B
v0.1.0 published           2026-09-02T13:07:42Z；asset created 13:07:40Z / updated 13:07:41Z（无重传）
                           （时间戳来自 p0-governance 的 authenticated REST 读取：auth-read.log 6×HTTP 200）
```

### 2.2 本轮构建工件（G0 产物）

| target | form | sha256 | bytes | member census |
|---|---|---|---|---|
| linux-x86_64 | static `libsongcore.a` | `0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308` | 4,066,980 | 280/280 `elf64-x86-64`，0 foreign |
| linux-x86_64 | shared `libsongcore.so` | `085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98` | 2,288,536 | 15/15 dynamic `song*` exports；NEEDED `libm.so.6` `libc.so.6` |
| windows-x86_64 | static `libsongcore.a` | `89d0428db2152da60f233290c082ecfb33b5590628ab9c40b97e8354afc65e97` | 3,309,294 | 280/280 `pe-x86-64`，0 foreign |

- Linux 构建：lane 记录的 **fresh-clone 根 `/home/hoo/g4-172/linux-root/src` 与 `receipts/g4-linux-build` 目录在定稿时已不存在**（定稿人 WSL `ls -ld` → `No such file or directory`）；该 lane 的 `stage.json` 有 64 条 `evidence_paths`，其中 42 条是绝对 WSL 路径、按原样不可解析，映射到仓库镜像 `build-linux/logs/` 后 64/64 hash 匹配、0 mismatch；lane 的 publish driver（`drivers/g4-s6-publish.sh:3`）明确只镜像 summary + logs、**不镜像 artifacts**。`xmake build songcore` / `songcore_static` / `songcore_shared` 的 exit 0 与 closure 20/20 校验只保留在该 lane 的镜像日志中；**L1 未重新获得**（closure 未重新派生）。
- Linux 产物字节仍可独立复核：`package/linux/extract/.../lib/libsongcore.a` = `0d38f4b5...`、`lib/shared/libsongcore.so` = `085425e5...`；同一字节也存在于 `/home/hoo/g4-172/linux-root/native/build/artifacts/`（定稿人 `sha256sum` 同上）。该 root 的 receipt 是 `ops/build-linux.sh` 的 **clone=reuse / closure=reuse** 运行，**不是** fresh-clone 构建。
- 包级 provenance：`package/linux/package-summary.json:26` 的 `build_receipt_sha256=1f574edb...` 解析到 `/home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json`，其字段为 `entry=ops/build-linux.sh`、`clone=reuse`、`closure=reuse`、`source_repo=/home/hoo/g4-172/wt-tools`、`work_root=/home/hoo/g4-172/linux-root`；该 receipt 未记录 `ops/build-linux.sh` 的 blob sha（定稿人实测 `18dba884...`）。因此 `artifact_provenance_attested:true` 绑定的是 **clone=reuse** 构建，fresh-clone 构建的 receipt 未被保留。
- Windows 构建：`ops/build-rc.ps1` exit 0，快照 `20261030-203045`；复用持久 work root `/home/hoo/q172rc` 的 warm xmake 缓存与已导入 closure（`manifest 0d0f7af9...`）；**L1 未重新获得**。定稿人复核：`.objs` 中 **279/280** 个 `.obj` 的 mtime 晚于 2026-10-30 20:00，仅 `songcore_ffmpeg.c.obj` 被复用（2026-10-29 04:20）——lane 的 “cache-compiled all units” 描述**低估**了实际重编译范围，但没有高估。
- 与 #329 归档对照：Linux shared **字节相同**（`085425e5...`）；Linux static **不同**（本轮 `0d38f4b5...` vs #329 `7b506922...`），根因实测为 4 个 `version.c.o` 内 `FFMPEG_VERSION` 字符串不同（closure 派生时的 `git describe` 不同：`qianqian-v0.1.0-315-gbf23516` vs `-316-gb49b18c`），不是缓存/gcc/构建根路径。Windows static 与 #329 `d8c8e688...` 不同（5 个 occurrence：符号表 + 4 个 `version.c.obj`），根因同类（`FFmpeg version 9.0.1` vs `qianqian-v0.1.0-316-gb49b18c`）+ `69ba1ce` 的 `native/build/ffmpeg.lua:144` 变更；两侧 archive 均无绝对构建根字面量（`strings` 命中 0）。

### 2.3 本轮预检包与 RC 命名容器（均非发布候选）

| 包 | sha256 | bytes | 文件数 | checksums | files{} |
|---|---|---|---|---|---|
| `songcore-v0.0.0-preflight-windows-x86_64.zip` | `5d92b21d7092b8d84d1db955ddbd38d3dc8b66f9f0ad29de35593f496e6397d3` | 1,449,803 | 14 | 13 | 12 |
| `songcore-v0.0.0-preflight-linux-x86_64.tar.gz` | `bfe780c3bf65b43f8007f3cd48fe3cef39de8d3a17af81f9767280aa7a43d790` | 2,575,536 | 15 | 14 | 13 |

本轮还产生了 **RC 命名容器**（存在，但不是发布候选、也未发布；全部来自入口审计/工具验证 lane）：

| 容器 | sha256 | 说明 |
|---|---|---|
| `p0-entries/package-rc-out/default/songcore-v0.2.0-rc.1-windows-x86_64.zip` | `e0c488a931d6f7bb0003d918eeee92f2202523e18319e617f26d264ae547084b` | `started=20261030-203142`；`artifact_sha256=89d0428d...`（本轮当前产物）；`version_status=provisional (contract section 14 Q1 owner-reserved)` |
| `p0-entries/package-rc-out/ver-0.1.0-rc.1/songcore-v0.1.0-rc.1-windows-x86_64.zip` | `7e843c9a68477bd575298993eca1b961b027eceba88a3793c111423d436cb264` | 同一产物，版本参数 `0.1.0-rc.1`；入口审计的语法接受性证据 |
| `p0-entries/package-rc-out/stale-artifact/songcore-v0.2.0-rc.1-windows-x86_64.zip` | `c3983bfb0215d771b6343fed576d649a975662b1adcf90b85c52ed50e9ee1604` | 故意使用陈旧 artifact 的负向控制 |
| `tools-linux-substrate/package/`（包树，非容器） | — | `manifest.json:3` `songcore_version=0.2.0-rc.1` |

这些 RC 命名产物**没有 tag、没有 GitHub Release、没有发布路径**；`NEW_RC` 仍为 `NOT_CREATED` 且未授权。但“本轮只有 preflight 包”的表述是错误的，已在本节改正。

- 集合算术为**独立重走**（不是读回执）：`checksums.txt == 包内文件 − 自身`，`files{} == checksums − manifest.json`；逐条 re-hash 0 mismatch；两个包内 `sha256sum -c` 均 OK。v2 定稿人重走确认：win 14/13/12、linux 15/14/13，集合相等、0 mismatch。
- `V1–V7` 在两个预检包上均 PASS；`O1–O4` 见 §4；`B1–B4` 均未触发。
- **§2 version identity 为 FIX_REQUIRED**：两个打包器都**没有** §2 等式检查，接受了 `0.0.0-preflight`；manifest `songcore_version=0.0.0-preflight`（linux manifest `:3`；windows manifest `:3`），目录名随之，tag 不存在，xmake leg 为 `0.1.0` = **FALSE**。
- `artifact_shared` 表示不一致：Windows 包为 boolean `false`、`shared_sha256=null`（windows manifest `:19-20`），包内无 `lib/shared/`；linux 包为字符串 `"lib/shared/libsongcore.so"`（linux manifest `:19`）；v0.1.0 manifest 同为字符串（`:19`）。合约 §8:342 只要求 carried v0.1.0 字段名保持含义，§8:354-359 只对 `target`/`toolchain` 明说 **type**；因此这是 **§8 未规定的表示缺口 + 无 V-check**（见 §4.2 的 `artifact_shared` 行），不能直接判为“违反 §8”。
- Linux 打包器来自 **未合并分支** `ops/songcore-linux-release-substrate @ 6df6c429cc6fb7b16cf566d8bb2e7d72e235ad74`（blob `8d36f443...`），PR 未开；main 上仍无 Linux 打包入口。该分支的 `ops/package-linux.sh:141-142` 也接受 `MAJOR.MINOR.PATCH[-suffix]` 并称其为 “contract section 2 VERSION_RULE form”，`PR-BODY.md:21` 写 “the §2 form is enforced”，而冻结 §2:96-98 不含 suffix——Q1 的 suffix 歧义已传播到新的 Linux 入口。

### 2.4 消费者与运行时依赖

| 项 | 结果 |
|---|---|
| Linux static consumer | PASS；4/4 fixtures 精确匹配 `reference.json`（`8113ff6e...`）；static == shared == reference |
| Linux shared consumer | PASS；`libsongcore.so` **无 SONAME、无 RPATH/RUNPATH**，必须 `LD_LIBRARY_PATH`（否则 exit 127） |
| Windows PE consumer | PASS；PE `3ec3c672...`；4/4 PCM 精确匹配；无 `-lbcrypt` 链接失败，有则成功；真实 Windows 主机运行 exit 0；主机身份以 `run/host.txt` 为准（主日志 `:4` 的 `host_windows` 是 Git Bash binary-match 伪影） |
| Linux NEEDED | 仅 `libm.so.6` + `libc.so.6`；无 FFmpeg 动态依赖 |
| Windows imports | `BCryptOpenAlgorithmProvider` / `BCryptGenRandom` / `BCryptCloseAlgorithmProvider`（bcrypt.dll）；运行期 IAT 计数在 4 个 fixture 上 **0 次调用**（该熵路径未被实际触发，不作 runtime-verified 声明） |
| glibc 实测下限 | `GLIBC_2.43`（`objdump -T` 最高版本；强制符号 `atan2f@2.43`、`hypot@2.35`、`pthread_*@2.34`） |
| 旧发行版实测 | Ubuntu 20.04（2.31）exit 1；Debian 12（2.36）exit 1；Ubuntu 24.04（2.39）exit 1；Alpine musl exit 255；**只有构建主机（2.43-2ubuntu2.4）跑通**。**没有任何发行版被认证为支持** |

### 2.5 发布、治理与文档门禁

```text
E01–E18（tools/test_release.py, WSL Ubuntu-24.04 python3 3.14.4）
    -> exit 0, "PASS: E01–E18; real PE controls; original HEAD/status/tool bytes unchanged"
       69 次子进程执行 / 43 个非空 id；另 23 次执行的 id 为空串；0 个 exit 不匹配
SongCore 发布器
    -> 不存在；ops/ 六个脚本无 gh release/gh api；workflow 仅 tags: qianqian-v*
    -> product check_release.py 按 R1 明确拒绝 songcore-v*（:74-76）
no-overwrite 设计（product 侧）
    -> R7 拒绝任何已存在 Release（check_release.py:221-224）
    -> publish-qianqian.py 重查 + ls-remote peeled tag == --source（:31-35）
    -> 唯一创建路径 `gh release create --verify-tag --target`（:41-44），无 clobber/edit/upload/delete
publish-policy 探针
    -> songcore-v0.1.0 / songcore-v0.2.0-rc.1 -> exit 2 R1
    -> existing qianqian-v0.1.0 -> exit 2 R7
    -> workflow_dispatch -> exit 2 R7
    -> 新 qianqian-v9.9.9 push -> exit 0（仅策略，不创建）
live GitHub state（p0-governance）
    -> Windows curl + jnhu76 token 成功读取：/user、repo、releases、release-by-tag、
       page2、matching-refs 全部 HTTP 200；permissions {'admin': True ...}；releases total=2
    -> WSL gh 自身无网络（仅 lo）——两项事实并存；“GitHub 状态不可读”不成立
docs-verify
    -> website `node scripts/verify.mjs` exit 0, 0 errors / 0 warnings
    -> `npx vitepress build` exit 1（三次）：[vite:esbuild-transpile] remove <tmp>\esbuild-<hash>: Access is denied
    -> AGENTS.md:450 记载的 `tools\docs-verify.cmd` 在 69ba1ce 树中不存在
    -> CI 路线 NOT_RUN
lefthook（tools PR 分支 6df6c42）
    -> commit-convention / vocabulary / plugin-boundary / rust-fmt / rust-clippy /
       rust-test / release-policy 均 PASS；docs-verify NOT_RUN（WSL 无 node）；lefthook exit 1
    -> 本包所在 worktree 无 tracked 变更，pre-push NOT_RUN（governance lane 记录）
```

### 2.6 D1 / D3 实测

```text
D1（linux 执行，10/10 MATCH，exit 0；windows 仅反汇编）
    - 104 / 103 / 112 失败后 *out_handle = NULL，sentinel 被覆盖
    - 100（INVALID_ARGUMENT）保留 sentinel；但该 status 不在 header :217 的错误列表里
    - 失败后被覆盖的 prior handle 仍可 song_probe -> SONG_OK, 44100 Hz, 2 ch（泄漏面）
    - 两个编译产物都含 NULL store（linux 221c / mingw 2100）
    - probe 只有 10 个 report() 断言（run.log 10 行 MATCH）；lane stage.json 的 “12/12” 是计数错误
    - song_probe 的 header 错误列表（songcore.h:226-228）同样漏掉 SONG_ERR_INVALID_ARGUMENT，
      而实现 :875 经 fail_arg(h) 返回它
D3（linux 构建 + 执行；windows NOT_RUN）
    - NULL handle -> 100，produced 读回 UINT64_MAX
    - NULL dst -> 100，produced 读回 UINT64_MAX
    - capacity 0 -> 100，produced 读回 0（符合）
    - NULL dst + capacity 0 -> produced 读回 UINT64_MAX
    - 对照调用 SONG_OK，256 frames；VIOLATIONS 3
    - 反汇编确认两条违规路径都在 `*out_frames_produced = 0` 之前返回
```

---

## 3. Owner 决策

> 以下每个决策都给出：可选方案 / 实测事实 / 技术与兼容性影响 / 风险 / 推荐方案及理由 / 选择后要改哪些文件与门禁 / 不选择的后果。**所有决策字段保持 UNDECIDED**（Q2 为已记录接受、待正式 closeout）。

---

### Q1 — 下一版本号（`songcore-vMAJOR.MINOR.PATCH`）与 §2 identity 等式

**状态：`UNDECIDED`（Owner-reserved；本包不提名任何数值）**

#### 可选方案

- **A（无后缀 route）**：Owner 记录一个新的 `songcore-vX.Y.Z`（纯 `MAJOR.MINOR.PATCH`，无 suffix）；版本 bump PR 修改 `native/xmake.lua:2`；两个打包器增加 §2 四元等式机器检查。合约 §2 的 tag form 不需要改。
- **B（允许后缀 route）**：Owner 记录的 identity 带 `-suffix`（预发布/RC）。必须修改冻结的 §2:96-98 tag form（Owner review），并同步 `docs/releasing.md` R1 文案与 `ops/package-rc.ps1:167-168` 对 “contract section 2 VERSION_RULE form” 的引用；同时定义 suffix 的不可变/排序语义。
- **C（推迟）**：不决定版本；G4 保持阻塞；任何包都只能以 `0.0.0-preflight` 这类非发布 identity 存在。

#### 实测事实

- 合约 §2:96-98 定义 `songcore-vMAJOR.MINOR.PATCH`，decimal、无 leading zeros、**无 suffix**。
- 合约 §2:99-102 定义四元等式：`manifest.songcore_version == tag == package directory name == Xmake project version`；`native/xmake.lua:2` 当前 `set_version("0.1.0")`（本包复读；文件 sha `b420fc6c...`）。
- 合约 §2:104-111 明确四个版本分开跟踪；`:113-115` 说明具体下一个号**不由合约决定**；§14 Q1:758-759 为 Owner-reserved。
- 实测：xmake `0.1.0`；rc manifest `songcore_version=0.2.0-rc.1`；package root `songcore-v0.2.0-rc.1-windows-x86_64`；唯一 tag `songcore-v0.1.0`（本包复算 `git tag -l 'songcore-v*'`）。四个 link 中 **2 个 open**（tag 不存在；xmake 不等于 manifest）。
- 本轮两个打包器都**没有** §2 等式检查：windows `ops/package-rc.ps1` 只拒绝精确 `0.1.0`（`:164-166`）并执行正则（`:167-169`）；linux `ops/package-linux.sh:138-148` 同样没有等式检查。两者都接受了 `0.0.0-preflight`（package lane `version_identity_s2` FIX_REQUIRED）。
- 无脚本读取 `set_version`：`tools/check_release.py:133` 只记录 `xmake --version`；`xmake_version_equality_machine_check = none`。
- tag form 歧义：`ops/package-rc.ps1:167` 的正则**接受** `[-+.0-9A-Za-z]+`，`:168` 把 `[-suffix]` 称为 “contract section 2 VERSION_RULE form”；而合约 `:96-98` 与 `docs/releasing.md` R1（本包复读）都没有 suffix。该歧义被 contract audit 记为 `tag_form_grammar_differential = FIX_REQUIRED`。
- 同一 suffix 语法已进入新的 Linux 打包器：`ops/songcore-linux-release-substrate @ 6df6c42` 的 `ops/package-linux.sh:141-142` 接受 `^[0-9]+\.[0-9]+\.[0-9]+([-+.0-9A-Za-z]+)?$` 并把 `[-suffix]` 称为 “contract section 2 VERSION_RULE form”；`tools-linux-substrate/PR-BODY.md:21` 进一步写 “the §2 form is enforced”，而冻结 §2:96-98 无 suffix。**该歧义已从 Windows 入口传播到 Linux 入口**，Q1 的裁定必须同时覆盖两处。
- `0.1.0` 不可复用：合约 §13:747-753 冻结历史；`ops/package-rc.ps1:164-166` 拒绝；且 package-rc 实测拒绝 `0.1.0` / `banana` / `1.2`（各 exit 1），但**接受** `0.1.0-rc.1`（exit 0）。
- `set_version` 是 `qianqian-native` 的**项目级**版本；把组件版本写进去，等于同时声明 native 项目的版本。合约 §2:104-111 没有说明该耦合是否 intended（contract audit 明确记为 blocker）。

#### 技术与兼容性影响

- **A**：只改 metadata / tag / 包目录名 / manifest；header、ABI、导出集、V2/V4 均不变；v0.1.0 仍为历史。唯一语义影响是 `native/xmake.lua:2` 的 bump——Owner 必须确认“native 项目版本 = SongCore 组件版本”是否 intended。
- **B**：在 A 的基础上增加冻结条款修订；suffix identity 会进入 tag/release 命名，必须给出“suffix tag 是否允许移动/重用”“预发布是否可发布”的规则，否则 §2 的 immutability 仍有空洞。
- **两者共同**：必须新增 §2 等式机器检查（读取 `native/xmake.lua:2`），否则 V1–V7 无法证明第四个等式；`tools/check_release.py` 是产品侧（R1 `qianqian-v` 正则、`qianqian.exe` 载荷），不能直接复用。

#### 风险

- Owner 只给数值、不裁定 tag grammar → 选出的 tag 可能违反冻结的 §2（suffix）或与打包器正则冲突。
- bump `set_version` 而不确认耦合 → native 项目版本被静默改成组件版本，未来 native 构建的 provenance 失真。
- 不做机器检查 → 本轮已实测 `0.0.0-preflight` 能通过打包并写入 manifest，任何未来包都可能重犯。
- 选 `0.1.0` → 被拒绝且违反不可变历史；选 `0.2.0-rc.1` 这类带后缀字符串 → 在 §2 未修订前不是合法 tag form。
- 提前打 tag → tag 不可移动（v0.1.0 是 annotated tag，且发布策略拒绝 re-tag）。

#### 推荐方案及理由

**推荐 A（流程性推荐；不提名具体数值）**：

1. Owner 在决策记录中写明版本号（本包不提名）。
2. Owner 同时裁定 suffix 政策：若保持无后缀 → 收紧 `ops/package-rc.ps1:167` 正则并修正 `:168` 的合约引用；若允许后缀 → 走 B，显式修订 §2:96-98 与 `docs/releasing.md` R1。
3. Owner 确认 `set_version` 耦合是否 intended；若否，需要为 SongCore 组件版本引入独立输入，而不是复用 `qianqian-native` 的项目版本。
4. 版本 bump PR + 两个打包器的 §2 等式机器检查一起落地。
5. 之后才允许生成最终包并跑 V1–V7。

理由：A 不触碰冻结条款，改动面最小；但它必须伴随“suffix 裁定 + 机器检查”，否则只是把已知的 identity 空洞换一个字符串。B 只有在 Owner 确实需要预发布 identity 时才值得，因为它要改冻结条款并定义 suffix 的不可变语义。

#### 选择后要改哪些文件与门禁

```text
A:
  native/xmake.lua:2                        版本值（tracked；Owner 给值）
  ops/package-rc.ps1:164-169                增加 §2 等式检查；按裁定收紧 suffix 正则
  ops/package-linux.sh（未合并，blob 8d36f443...） 增加同一 §2 等式检查
  docs/release/songcore-release-contract.md §14 Q1   记录 disposition
门禁:
  新增 §2 identity 机器检查（读取 native/xmake.lua:2）
  两个包的 V1–V7、§2 等式、端到端打包
  lefthook release-policy（native/** 触发）与 pre-push

B: A 的全部，再加：
  docs/release/songcore-release-contract.md:96-98    修订 tag form
  docs/releasing.md R1                               同步文案
  ops/package-rc.ps1:168                             删除/更正 “contract section 2 VERSION_RULE form” 引用
  定义 suffix 的不可变与排序语义（合约 §13 或 §2）
```

#### 不选择的后果

- G4 无法关闭：没有合法版本号，任何包都不能成为 release candidate。
- `manifest.songcore_version == tag == package directory == Xmake project version` 的第四等式持续为 FALSE；V1–V7 永远无法证明 §2。
- `0.1.0` 不可复用，人工发布也没有合法 tag 可打；发布只能停在“预检包”阶段。
- 若强行用带后缀字符串发布，会留下一个违反冻结 §2 的公共 tag，且不可回收。

---

### Q2 — 正式登记 #329 的「保留调试信息 + `-fmacro-prefix-map`」结论

**状态：`RECORDED_ACCEPTANCE_PENDING_FORMAL_CLOSEOUT`**

#### 可选方案

- **A（合约正式登记）**：在合约 §12（或新增 §16 D4）写入：保留调试信息；对重放的 FFmpeg translation units 采用单一 flag `-fmacro-prefix-map=<ffmpeg srcroot>=.`；`-ffile-prefix-map` 实测同样可行但**不采用**；不采用 `strip` / `-g0`；L3 的范围是“同一 closure 派生下的跨根字节同一性”，并记录 `FFMPEG_VERSION` 随 closure 派生 commit 变化的 caveat。同步把 §14 Q2 标为 closed；把 #329 归档加入 registry ARCH-008 证据。
- **B（仅证据登记）**：以 `evidence/songcore-172/q2-crossroot/t4-verdict-and-L.md` 作为登记，合约不动。作为“已接受结论”的证据可以，但作为**发布权威**不足：Q2 仍 open，V6 的 reproducibility 定义仍指向未采用的 flag。
- **C（重开 strip/-g0）**：重新评估剥离调试信息。**不推荐**：#329 已在两个 target 上实测 flag route 足够（282/282 member occurrences identical），没有测到需要 strip 的残余；剥离会改变 shipped bytes 并失去可调试性。

#### 实测事实

- 合约 §14 Q2:760-763 给出的二选一是“保留调试信息并追求跨根字节稳定（T4，`-ffile-prefix-map`）” vs “ship stripped/-g0”；它没有提到 #329 最终采用的 `-fmacro-prefix-map`。
- #329 `t4-verdict-and-L.md`：E0（无 map）两 target 均 differs（9 member occurrences）；`-ffile-prefix-map` identical 但 **NOT adopted**；`-fmacro-prefix-map` identical → **adopted single candidate**；未使用 strip / `-g0`。Winner：linux static `7b506922...` / shared `085425e5...`，windows static `d8c8e688...`；L1/L2/L3 PASS，但 windows shared NOT RUN、release-level L3 ratification NOT RUN。
- `native/build/ffmpeg.lua:144` 在 69ba1ce 已包含 `target:add("cflags", "-fmacro-prefix-map=" .. srcroot .. "=.", {force = true})`（本包复读）。
- 本轮 Linux 构建的 flag witness：`compile_commands.json` 281 条，279 条含 `-fmacro-prefix-map=<srcroot>=.`，0 条含 `-ffile-prefix-map`。
- 本轮 #329 对照：#329 shared 与本轮 shared **字节相同**；static 的差异只在 4 个 `version.c.o` 的 `FFMPEG_VERSION` 字符串；windows 的 5 个 occurrence 差异同类；两侧都没有绝对构建根字面量。
- **L1/L2/L3 在本轮没有重新获得**：两个 build lane 均把 L1 记为 NOT_RUN（closure 复用），package lane 两个 manifest 的 reproducibility 三项均为 `NOT_ESTABLISHED`。
- registry ARCH-008 的 evidence 只列 2 项（`docs/architecture/registry.yml:247-248`），既没有 #329 归档，也没有 S3 linux 归档（p0-evidence-spec 记为 FIX_REQUIRED）。

#### 技术与兼容性影响

- A 只改文档/登记，不改代码、不改 ABI、不改 shipped bytes；但它是**冻结合约文本**的修订，需 Owner review。
- A 必须把 L3 的适用范围写死，否则本轮 static 与 #329 static 的不同会被误读为 flag 失效；正确的表述是“**同一 closure 派生**下的两个独立根字节同一”，而不是“与 #329 归档字节相同”。
- 登记后，V6 的 reproducibility evidence 指针应同时指向 #329 归档与本轮 build receipts（若未来要声称 L1–L3）。
- C 会改变 shipped bytes（strip 后 debug 信息丢失），且没有实测收益支撑。

#### 风险

- 把 #329 的 campaign 级 L3 直接写成 release 级保证 → 违反“campaign evidence 不是 release authority”的边界（`t4-verdict-and-L.md` 开篇即声明）。
- 不登记 → 合约仍写 `-ffile-prefix-map`（未采用的 arm），未来 builder 按合约执行会得到不同的 flag 选择，且 Q2 永远 open。
- 登记时漏掉 `FFMPEG_VERSION` caveat → 未来跨根比较会把 closure 派生 commit 差异误判为回归。

#### 推荐方案及理由

**推荐 A**。理由：#329 的实质结论已经被 Owner 授权并实测（flag 在两个 target 达到 282/282 occurrence identical，且已在 69ba1ce 的 `native/build/ffmpeg.lua:144` 生效），缺的只是“把 accepted conclusion 写进唯一权威 + 让 registry 证据可路由”。这是纯文本/证据登记，不改变任何编译语义；B 不足以关闭 Q2，C 与实测相反。

#### 选择后要改哪些文件与门禁

```text
docs/release/songcore-release-contract.md
  §12（新增 adopted-flag subsection，或在 §16 增 D4）
  §14 Q2:760-763（disposition: accepted / closed）
  §16 标题区（若新增 D4）
docs/architecture/registry.yml:247-248
  增加 evidence/songcore-172/q2-crossroot/README.md
  增加 evidence/songcore-172/q2-crossroot/t4-verdict-and-L.md
  （并修正 ARCH-008 缺失的 S3 linux 证据，见 §4）
门禁:
  本次不改代码，无需重跑 G0–G2
  未来任何 release 级 L3 声明必须按 §12 重新执行两独立根构建并保留 receipt（V6）
```

#### 不选择的后果

- Q2 继续列为 Owner-open，G4 的 Owner-blocked 清单无法缩短。
- 合约 §14 的二选一与实际树中的 flag 不一致：一个照合约施工的 builder 会选择 `-ffile-prefix-map`（未采用的 arm），产出与当前证据不同的构建。
- L3 只能停留在 campaign evidence，不能作为 release 的 reproducibility 声明；V6 的 evidence 指针无权威落点。

---

### Q3 — Windows 本轮 static-only 还是新增 DLL

**状态：`UNDECIDED`**

#### 可选方案

- **A（static-only）**：保持 `native/ffmpeg/targets/windows-x86_64.json` 的 `artifacts.shared=false`；包内只有 `lib/libsongcore.a`；不新增 DLL。
- **B（新增 DLL）**：把 windows shared 纳入本轮目标，构建 `songcore.dll`，按 §9 的 `-lbcrypt` 与 R4 式 import 审计（`objdump -x` 对 allowlist）验证，扩展 §6/§7/§8 的 artifact/package/manifest。
- **C（显式推迟但按 static-only 交付）**：与 A 的交付物相同，但在合约中把 shared 标记为“下一轮再评估”。与 A 的差别只是措辞；本包把 A 视为“本轮决定”，C 视为“未决定”。

#### 实测事实

- `native/ffmpeg/targets/windows-x86_64.json`：`"artifacts": {"static": true, "shared": false, "wasm": false}`，`"status": "provisional"`（本包复读）。
- 合约 §6:243-272：shared 是 **OPTIONAL**，只在“independently verified with the full gate set”时 ship，“never for symmetry”；并明确 “windows ships static-only unless the owner amends (§14)”。
- 合约 §9:459-486：windows static 的 link line 是 `gcc … libsongcore.a -lbcrypt`；任何 shared/DLL form 的审计方法是 `objdump -x <dll>` import walk against the explicit system allowlist；新的非系统 import 需要 reviewed contract update（R4 规则）。
- 本轮 Windows build：280/280 `pe-x86-64`，15/15 ABI symbols，三个 `BCrypt*` undefined imports。
- 本轮 Windows consumer：不带 `-lbcrypt` 链接 **exit 1**（三个 BCrypt undefined）；带 `-lbcrypt` exit 0；PE 在真实 Windows 主机上 4/4 PCM 匹配；bcrypt 熵路径运行期 **0 次调用**（未验证）。
- 本轮 Windows package：14 files，`artifact_shared=false`，无 `lib/shared/`、无 DLL；consumer lane 的 shared/DLL 项为 NOT_RUN（包内没有 shared form）。
- #329 campaign 的 windows L2/L3 也是 static-only scope；shared NOT RUN。

#### 技术与兼容性影响

- A 不改变任何文件；包内容集固定为 14 files（本轮实测），manifest `artifact_shared=false`，V1–V7 形状不变；Windows 消费者继续走 static（文档已声明 “Static (the supported consumption path)”）。
- B 会扩展：target recipe（`shared: true` 或新增 kind）、`native/build/songcore.lua` 的 shared target（需要 mingw 配置）、两个打包器、§7 package layout（`lib/songcore.dll`）、§8 manifest（`artifact_shared` / `shared_sha256` / `runtime_dependencies`）、§9 审计方法、§6 的 artifact kind 声明；并需要 G1 shared（导出集恰 15）+ G2 shared（真实 Windows DLL 消费）+ R4 import 审计。
- B 还会引入 DLL 的 ABI 导出面与运行时加载语义（SONAME/import lib 等），这是一个新的兼容性承诺，不是“多一个文件”。

#### 风险

- A：Windows 消费者只能静态链接；若下游明确需要 DLL，会在 G4 之后形成缺口。但 static 已是文档化的 supported path，且本轮没有任何 DLL 证据。
- B：在没有 DLL 构建/运行证据的情况下纳入目标，等于用“对称性”代替“independently verified”，直接违反 §6 的 “never for symmetry”；且新增 import 面可能引入未审计的系统 DLL 依赖。
- C：留下“未决定”的 artifact set，G4 无法冻结内容集。

#### 推荐方案及理由

**推荐 A（static-only）**。理由：target recipe 本身 `shared=false`；合约把 shared 定义为 optional 且要求 full gate set；本轮没有任何 DLL 构建、链接、运行或 import 审计证据；Windows static 已被真实 PE consumer 验证（4/4 PCM）。B 是一个独立的 bounded decision，应该等有 DLL 构建/运行证据后再走 §14 修订，而不是在 G4 预检里顺带纳入。

#### 选择后要改哪些文件与门禁

```text
A:
  docs/release/songcore-release-contract.md §14 Q3   记录 disposition（static-only this round）
  不改 target json / 打包器 / manifest / 代码
门禁:
  现有 windows static 证据（G0/G1/G2 + V1–V7）已覆盖本轮交付物

B（若 Owner 选择新增 DLL）:
  native/ffmpeg/targets/windows-x86_64.json          artifacts.shared / status
  native/build/songcore.lua                          windows shared target
  ops/package-rc.ps1（及 linux 打包器，若对称）       包内 lib/shared/songcore.dll
  docs/release/songcore-release-contract.md §6/§7/§8/§9/§14 Q3
  manifest: artifact_shared / shared_sha256 / runtime_dependencies
门禁:
  G1 shared（导出集恰 15）
  G2 shared（真实 Windows DLL 消费，含 -lbcrypt 链接）
  R4-style objdump -x import walk vs allowlist
  V4/V5、V1–V7、包安全检查
```

#### 不选择的后果

- 包的 artifact set 无法冻结，G4 的 “exact content set” 无法声明。
- 若未来有人为了“对称”临时加 DLL，会绕开 §9 的 import 审计与 §6 的 full gate set，形成一个没有运行时证据的发布面。
- Windows 的 shared 能力继续处于 provisional/未验证状态（与今天相同），但至少不会把未验证的形态写进 release。

---

### Q4 — Linux 最低 glibc / 支持发行版范围

**状态：`UNDECIDED`**

#### 可选方案

- **A（声明实测下限，不作发行版支持声明）**：把 `GLIBC_2.43` 写为共享产物 `libsongcore.so` 的最低系统 glibc 要求（manifest 已记录），并在包 README/发布说明中只陈述该数值与实测方法；不声明任何发行版 family “supported”。
- **B（重建基线以降低 floor）**：在更老的 glibc 构建环境（容器/旧 sysroot）中重建 linux 产物，重新跑 G0–G2、L1–L3、打包与 V1–V7，把 floor 降到目标发行版可用的水平。
- **C（本轮 linux 只发 static）**：把 shared 从本轮交付物中移除（`shared` 在 §6 是 optional），从而不承诺 `.so` 的 glibc floor；static archive 本身不携带 glibc NEEDED，floor 转移到消费者自己的链接环境。
- **D（同时声明 floor 与支持发行版）**：不推荐，因为本轮只实测了构建主机，没有任何发行版通过 open→decode。

#### 实测事实

- 共享产物 `libsongcore.so`（`085425e5...`）的 `DT_NEEDED` 只有 `libm.so.6` + `libc.so.6`；无 FFmpeg 动态依赖；无 SONAME、无 RPATH/RUNPATH。
- `objdump -T | grep -o 'GLIBC_[0-9.]*' | sort -uV` 的最高版本为 **`GLIBC_2.43`**；完整 required set 为 `2.2.5, 2.3.2, 2.3.4, 2.4, 2.6, 2.7, 2.14, 2.17, 2.27, 2.28, 2.29, 2.34, 2.35, 2.43`；强制符号 `atan2f@2.43`、`hypot@2.35`、`pthread_create/join/once@2.34`。
- 非 GLIBC 版本化依赖：**NONE**（`GLIBCXX_/CXXABI_/GCC_/GOMP_/LIBATOMIC_/GLIBC_ABI_` grep exit 1）。
- 真实容器运行：Ubuntu 20.04（glibc 2.31）exit 1（`libc.so.6: version GLIBC_2.34 not found` + `libm.so.6: version GLIBC_2.35 / GLIBC_2.43 not found`）；Debian 12（2.36）exit 1（`GLIBC_2.43 not found`）；Ubuntu 24.04（2.39）exit 1（同）；Alpine musl exit 255（`no such file or directory`）。**只有构建主机（glibc 2.43-2ubuntu2.4）跑通完整 consumer。**
- 构建主机的 glibc 比 stock Ubuntu 24.04（2.39）更新；floor 反映的是构建环境，不是产品设计。
- 在构建主机上链接的 static-form consumer 也要求 2.43；但 static archive 本身没有 NEEDED，消费者在自己主机链接时会使用自己的 glibc。**不要把“构建主机上链接出的 consumer 的 floor”误读为 static archive 的 floor。**
- 合约 §9:488-503 要求记录 floor，并明确“floor POLICY（which distro family is supported）is an owner decision — §14”；§14 Q4:768-770 为 Owner-reserved。
- **测量值 ≠ 政策**：以上是测量；本包不据此宣布任何发行版受支持。

#### 技术与兼容性影响

- A：不改代码/工件；只把测量转成“最低系统 glibc = 2.43”的声明。后果是 stock Ubuntu 24.04（2.39）及更老的 glibc 系统**无法加载** `.so`；static-first 消费者不受此 `.so` 限制（他们在自己的主机链接）。
- B：改变构建环境，可能改变 closure 派生/工具链身份与所有 linux 产物 hash；必须重新跑 G0–G2、L1–L3、打包、V1–V7，成本最高但能真正降低 floor。
- C：包内容从 15 files 变为 14（去掉 `lib/shared/libsongcore.so`），linux target json `shared=true` 与合约 §6 的“optional”允许这样做，但会丢失一个已经通过 shared consumer 的形态。
- D：会让“构建主机可运行”被误读为“某发行版受支持”，与实测相反。

#### 风险

- A 若不写清“不是发行版支持声明”，发布说明可能被读成“Linux 支持”；用户会在 stock Ubuntu 24.04 上遇到 loader error。
- B 若只改构建环境而不重跑全部 gate，等于用环境变化替换证据。
- C 会让 linux 与 windows 都变成 static-only，shared 的验证成果无法交付；但不影响 static-first。
- 任何“支持 Ubuntu X”的声明都没有本轮证据支撑。

#### 推荐方案及理由

**推荐 A（本轮预检/RC 范围）**：把实测的 `GLIBC_2.43` 声明为 `libsongcore.so` 的最低系统 glibc 要求，manifest `runtime_dependencies.glibc_symbol_version_floor` 已记录（linux manifest `:207`），并在包 README 明确“仅构建主机完成 open→decode；未认证任何发行版”。如果 Owner 的目标是让 stock 发行版可用，则**唯一诚实的路线是 B**（重建基线并重跑全部 gate），而不是改一句支持声明。

理由：A 与现有证据一致、零代码改动、不会制造虚假的支持承诺；它把风险显式化（2.43 是一个很高的 floor）。B 成本高但方向正确；C 是可行的降级，但会丢掉 shared 形态。**不推荐 D。**

#### 选择后要改哪些文件与门禁

```text
A:
  包 README（两个包）与发布说明：写明 GLIBC_2.43 最低要求 + “未认证任何发行版”
  manifest.runtime_dependencies（linux 已记录 :195-207；确认字段名与措辞）
  docs/release/songcore-release-contract.md §14 Q4  记录 disposition
门禁:
  无新构建；若发布说明声称任何发行版，必须先在对应发行版跑 G2（本轮未做）

B:
  构建环境 / CI 容器基线（新）
  重新构建 linux static + shared
  G0–G2、L1–L3、V1–V7、consumer 全量重跑
  更新所有 artifact/package hash

C:
  打包器移除 shared 成员；§6/target json 的 shared 交付形态改为 static-only
  重跑 V1–V7 与包安全检查
```

#### 不选择的后果

- Q4 继续 Owner-open；G4 的 Owner-blocked 清单不减。
- 发布说明无法回答“这个 `.so` 能在哪些系统加载”；用户只能自行试错。
- 若默认按“构建主机能跑”发布，会制造一个事实上的“最新 glibc only”产品，且没有书面警告。

---

### Q5 — 自动发布工作流 vs 人工发布

**状态：`UNDECIDED`**

#### 可选方案

- **A（人工发布 + G4 书面门禁 + 运行手册）**：保持 v0.1.0 的人工模式；由 Owner 在 G4 全部满足后手工创建 annotated tag 与 GitHub Release；用一份 runbook 固定 no-overwrite / 拒绝 re-tag / 拒绝错误 source SHA / 资产校验的检查清单。
- **B（SongCore 自动发布工作流）**：新增 `songcore-v*` 的 tag-push workflow，具备与产品侧 R7 等价的保护：拒绝 re-tag、拒绝已存在 Release、要求 tag push 事件、peeled tag == 断言 source SHA、发布前重校包与 `.sha256`/provenance。
- **C（把产品发布器扩展到 SongCore）**：**不推荐**。`tools/check_release.py` 是产品硬编码（`TARGET='x86_64-pc-windows-gnu'`、`FILES=('qianqian.exe', ...)`、R1 `qianqian-v` 正则并显式拒绝 `songcore-v*`，`:18-28`/`:74-76`），扩展它等于重写产品发布门禁。

#### 实测事实

- **SongCore 当前没有任何发布器**：`ops/` 六个脚本（`build-rc.ps1`、`package-rc.ps1`、`build-linux.sh`、`verify-external.ps1`、`run-binding-gate.ps1`、`run-csuite.ps1`）只做构建/打包/校验/外部消费验证；`grep -rn 'git push|Invoke-RestMethod|gh release|curl -' ops/` 无命中；`.github/workflows/` 八个 workflow 只有 `release-qianqian.yml` 能发布，触发为 `tags: ['qianqian-v*']`，publish job 条件为 `event_name == 'push' && startsWith(github.ref, 'refs/tags/qianqian-v')`。
- 产品侧的 no-overwrite 设计是真实存在的：`check_release.py:221-224` 拒绝任何已存在 Release（含历史 0.1.0）；`publish-qianqian.py:31-35` 重查 releases 并 `git ls-remote` peeled tag == `--source`；`:41-44` 唯一创建路径 `gh release create --verify-tag --target`；全仓 grep 无 `--clobber` / `release edit` / `release upload` / `release delete`。
- `publish-policy` 探针实测：`songcore-v0.1.0` → exit 2 `POLICY_RED: R1 invalid product tag`；`songcore-v0.2.0-rc.1` → exit 2 R1；已存在的 `qianqian-v0.1.0` → exit 2 R7；`workflow_dispatch` → exit 2 R7；未使用过的 `qianqian-v9.9.9` push → exit 0（仅策略，不创建）。
- `v0.1.0` 是人工发布的（2026-09-02T13:07:42Z；tag object `a1b6c99...` → commit `522c16d...`；asset sha `018e0cbd...` 与本地历史副本一致，无重传）。
- 发布**在本轮没有执行**：本轮明确禁止创建 tag/GitHub Release，且不存在 SongCore 发布器。环境能力必须分开陈述：Windows 主机没有 `gh` 二进制，但 p0-governance 用同一 `jnhu76` token 经 Windows `curl` 成功读取 live GitHub 状态（6 次 HTTP 200、`permissions {'admin': True …}`、releases total=2，`evidence/songcore-172/g4/p0-governance/auth-read.log` / `auth-summary.log`）；WSL 的 `gh` 无网络（仅 `lo`，`gh api`/`gh release list` 报 `error connecting to api.github.com`）。因此“GitHub 状态不可读”不成立；“不能在本轮发布”是**规则与授权事实**，不是不可达。
- 合约 §14 Q5:771-775 明确把该问题列为 Owner 待决，并指出 v0.1.0 是 manual、产品侧有 tag-push publisher with R7 protections。

#### 技术与兼容性影响

- A：零代码；发布过程依赖人工检查清单。风险是人为 re-tag/overwrite，但 `songcore-v0.1.0` 的 tag/release/asset 一旦存在就被冻结，任何覆盖都必须被明确禁止。
- B：新增一个发布面（workflow + 脚本），需要自己的负向控制（拒绝 re-tag、拒绝已存在 Release、拒绝 dispatch/PR、拒绝错误 source、资产 re-hash）；这是新的攻击面，必须按 E01–E18 的方式做机器可判定的拒绝路径。
- C：需要改写产品门禁的硬编码 target/文件名/正则；高风险，且会让 SongCore 与产品发布语义耦合。

#### 风险

- A：人工操作可能在 tag 创建后才发现 G4 未满足；或误用已有 tag/Release。缓解：runbook 在创建任何 tag 之前逐项勾选 G4，并要求 `git rev-parse <tag>^{commit} == <source>` 与资产 hash 对 receipt。
- B：在没有任何 SongCore 发布器测试的情况下引入自动发布，风险高于人工；且当前环境无法端到端验证。
- C：产品门禁的 `qianqian.exe`/Windows PE 假设与 SongCore 的 tar.gz/双 target 不兼容，强行扩展会破坏产品侧的既有保护。

#### 推荐方案及理由

**推荐 A（人工发布，G4 作为书面门禁）**。理由：今天不存在 SongCore 发布器（实测）；v0.1.0 的人工先例可复用；B 是一个新的政策 + 代码面，应该在 G4 之后作为独立议题，而不是为了本次发布临时搭建。A 必须配一份 runbook，把产品侧已验证的 R7 语义逐条抄成人工检查项（拒绝 re-tag、拒绝覆盖、tag push 源 == source、资产 hash 对包内 `checksums.txt`/外部 receipt）。

#### 选择后要改哪些文件与门禁

```text
A:
  docs/release/songcore-publishing.md（新建 runbook）
  docs/release/songcore-release-contract.md §14 Q5  记录 disposition
  无代码/无 workflow 变更
门禁:
  人工检查项：G4 全绿、tag 不存在、source SHA 匹配、包 hash 与 receipt 匹配、
             release notes/LICENSE 附件齐备、创建后不改写

B:
  .github/workflows/songcore-release.yml（新建）
  tools/songcore_release.py 或等价脚本（新建）
  docs/release/songcore-release-contract.md §13/§14 Q5
  docs/releasing.md 或新建发布文档
门禁:
  负向控制（songcore tag push、existing release、dispatch、错误 source、资产 mismatch）
  与产品侧 E01–E18 同类的机器可判拒绝路径
```

#### 不选择的后果

- SongCore 没有合法发布路径：G4 通过也无法把 `songcore-v*` 发布出去（或只能无记录地手工操作）。
- 若贸然复用产品发布器，会被 R1 直接拒绝（实测 exit 2），或者需要改写产品门禁，风险外溢。
- G4 的 Owner-blocked 清单继续包含 Q5。

---

### D1 — `song_open` 的 `*out_handle` 契约差异

**状态：`UNDECIDED`**

#### 可选方案

- **A（修订 normative header 文字，不改代码）**：把 `native/include/songcore.h:218` 的 “*out_handle is then untouched” 改为 “on failure *out_handle is set to NULL (the caller's previous value is discarded)”；同步 `docs/architecture/songcore-binding-architecture.md:516`；并把 `SONG_ERR_INVALID_ARGUMENT` 补进 **`song_open` 的错误列表（`:217`，实现 `:824` 返回它）与 `song_probe` 的错误列表（`:226-228`，实现 `:875` 经 `fail_arg(h)` 返回它）**。
- **B（改实现以匹配原文）**：删除 `native/src/songcore_ffmpeg.c:825` 的 `*out_handle = NULL;`，让失败保留调用方的值。
- **C（只记录，不改）**：保持现状，差异继续留在合约 §16 D1。

#### 实测事实

- `native/include/songcore.h:216-220`：`song_open` 的错误列表为 `SONG_ERR_UNSUPPORTED_CONTAINER / SONG_ERR_IO / SONG_ERR_OUT_OF_MEMORY`（`:217`），随后 “*out_handle is then untouched”（`:218`）。`SONG_ERR_INVALID_ARGUMENT` 不在列表里。
- `native/src/songcore_ffmpeg.c:822-825`：参数 guard 在 `:823-824` 返回，`:825` 立即 `*out_handle = NULL;`；所有 post-guard 失败（三个列出的 status）都在 `:825` 之后发生。
- Linux 执行（probe **10/10** MATCH，exit 0）：104 / 103 / 112 失败后 sentinel `0x5E27E1CAFE` 被覆盖为 `NULL`；100（INVALID_ARGUMENT）保留 sentinel——而 100 正是 header 未列出的那条路径。10 是 `d1_probe.c` 的 `report()` 调用数，也是 `run.log` 的 MATCH 行数；lane `stage.json` 的 “12/12” 是计数错误（本包按实测记录，lane 回执未改）。
- prior-handle 场景：先成功 open（SONG_OK），保留 `prior`，再用垃圾 io 对同一 slot 重新 open（104）→ `h == NULL`，但 `song_probe(prior)` 仍返回 SONG_OK（44100 Hz，2 ch）——handle 仍存活，唯一引用被一个“header 说不会 touch”的调用销毁。
- Windows：仅反汇编（未执行）；两个编译产物都含 NULL store（linux `221c` `movq $0x0,(%rsi)`；mingw `2100` `movq $0x0,(%rdx)`）；五个 guard 分支都跳过 store。
- in-repo 消费者对两种读法都不敏感：`qianqian-songcore-sys` 是裸 extern（`crates/qianqian-songcore-sys/src/lib.rs:182`）；`qianqian-decode-songcore` 先零初始化 slot（`src/lib.rs:359`）并只看 status；`e1_abi_tests.rs:545-555` 先置 NULL 再断言 `is_null`，两种读法都通过，pinning 不了任何一侧。**但 `crates/qianqian-decode-songcore/src/lib.rs:358-372` 的注释已经把“native 侧在每次 open 失败都写 NULL”当作 verified fact 陈述**——行为上不敏感，代码注释上已经选边。
- `song_probe` 的 header 错误列表（`native/include/songcore.h:226-228`）同样漏掉 `SONG_ERR_INVALID_ARGUMENT`，而 `native/src/songcore_ffmpeg.c:875` 的 `fail_arg(h)` 会返回它；D1 的 header 修订必须同时覆盖两个函数。
- 合约 §16 D1:855-882 明确 `needsOwnerDecision`；D1 的影响面是 `.h` 文本（A）或两个 target 的编译语义（B）。

#### 技术与兼容性影响

- **A**：不改 ABI layout、不改 `SONGCORE_ABI_VERSION`、不改已发布 rc.1 包内 header（该包是 immutable evidence）；但**会改变 header 文件的 sha256**。合约 §3:135 把 `cf509a5c...78536` 冻结为身份，且写明“the ABI header has not changed since v0.1.0”。因此 A 必须**同时修订 §3**：记录 v0.1.0 的 header 仍为 `cf509a5c...`（历史），新 release 的 header 为新 hash；V2 的期望值也随之更新。任何重新打包都必须用新 header 重跑 V2。
- **B**：两个 target 的编译语义改变，且**没有 ABI 版本信号**；消费者无法区分“修复前/修复后”，除非探测行为。它把 fail-closed 的 NULL 保证转移给调用方：若调用方未初始化 `*out_handle` 并在失败后读取，会得到 indeterminate memory（UB），而不是安全的 NULL。
- **C**：header 继续对第三方说谎；已发布 rc.1 的 header 仍是这句话（不可改），但未来包可以修。

#### 风险

- A：只改文字但忘记 §3 的 header hash → V2 与合约 §3 立刻互相矛盾；或忘记 `songcore-binding-architecture.md:516` 的第二处重复 → 两份权威文本再次冲突。
- B：静默改变两个 target 的编译行为；loss of fail-closed NULL；且 probe 必须在两个 target 上重跑（Windows runtime 目前 NOT_RUN）。
- C：一个被实测证伪的 normative 句子继续作为第三方实现依据；D1 永远 open。

#### 推荐方案及理由

**推荐 A（可选地同时补 `SONG_ERR_INVALID_ARGUMENT` 到 `:217` 的错误列表）**。理由：代码是更安全的一半，且已经 ship 在两个 release target 的产物里；错的只是文本。A 不改变编译语义、不需要 ABI bump；它需要一次 header 文本修订 + §3 身份修订 + 两处重复文本同步。B 会在两个 target 上静默改变行为且没有版本标记，并把 fail-closed 保证换成仓库里没人使用的 sentinel 保留模式。C 不可接受：它让一个被测量证伪的承诺继续生效。

#### 选择后要改哪些文件与门禁

```text
A:
  native/include/songcore.h:217-218（措辞 + INVALID_ARGUMENT 列表）
  native/include/songcore.h:226-228（song_probe 的 INVALID_ARGUMENT 列表）
  docs/architecture/songcore-binding-architecture.md:516（同步）
  docs/release/songcore-release-contract.md §3:131-170（header sha256 / “unchanged since v0.1.0” 表述）
  docs/release/songcore-release-contract.md §16 D1（disposition）
门禁:
  重新计算 header sha256；V2 用新期望值；重新打包；consumer 重新编译（ABI 不变）
  确认 SONGCORE_ABI_VERSION 仍为 1

B:
  native/src/songcore_ffmpeg.c:825（删除 NULL store）
  两个 target 重新构建（G0）、G1/G2、打包、V1–V7
  D1 probe 在 linux + windows 运行时重跑（windows runtime 目前 NOT_RUN）
  合约 §16 D1 disposition（不改 header）

C:
  仅 §16 D1 保持 needsOwnerDecision；不推荐
```

#### 不选择的后果

- header 的 normative 句子继续为假：第三方按 `:218` 实现 sentinel 检测会在 104/103/112 上失败。
- 一个“失败即 NULL”的安全行为没有被写成契约，反而被写成相反的话；未来可能有人按文本删掉 `:825`，把安全行为改掉。
- D1 继续是 Owner-blocked；按本包规则，不得为赶发布绕开。

---

### D3 — `song_read_pcm` 两个 pre-zeroing guard 未清零

**状态：`UNDECIDED`**

#### 可选方案

- **A（修代码）**：把 `if (out_frames_produced) *out_frames_produced = 0;` 提到 `song_read_pcm` 的第一条语句，位于 `!h` guard（`:1111`）与 `!dst || !out_frames_produced` guard（`:1112`）之前；header 不动。
- **B（改 normative header）**：在 `native/include/songcore.h:358` 声明“参数校验失败不写 out-parameter”，代码不动。
- **C（只修 `!dst` guard）**：在 `:1111` 与 `:1112` 之间插入清零。**不完整**：NULL-handle 路径仍在清零前返回；本包明确拒绝。
- **D（A + 把承诺写全 + 探针进入发布门禁）**：按 A 修代码；把 header 的承诺改成可测试的全称命题（“whenever out_frames_produced is non-NULL it is written on every call”）；把 `evidence/songcore-172/g4/d3-readpcm-zero/readpcm_zero_probe.c` 提升为两个 target 的 release gate 回归。

#### 实测事实

- `native/include/songcore.h:355-369`：`other -> typed error; *out_frames_produced == 0 in that call`（`:358`）；`:369` 说 `frame_capacity == 0 -> SONG_ERR_INVALID_ARGUMENT`，没有为 out-parameter 开例外。
- `native/src/songcore_ffmpeg.c:1111-1114`：`:1111` `if (!h) return SONG_ERR_INVALID_ARGUMENT;`；`:1112` `if (!dst || !out_frames_produced) return fail_arg(h);`；`:1113` `*out_frames_produced = 0;`；`:1114` capacity guard。两个 pre-zeroing guard 都在 store 之前返回。
- Linux 执行（`libsongcore.so` `085425e5...`，强制重建）：poison `UINT64_MAX` 后，(a) NULL handle → status 100，读回 `0xffffffffffffffff`；(b) NULL dst → 100，读回 `0xffffffffffffffff`；(c) capacity 0 → 100，读回 `0x0`（符合）；(d) NULL dst + capacity 0 → 读回 `0xffffffffffffffff`。对照调用 SONG_OK 256 frames，说明 handle 有效。`VIOLATIONS 3`。
- 反汇编：`0x35a02 test %rdi; je 0x35b67`（裸返回 100，无 store）、`0x35a19/0x35a22 test %rsi,%rcx; je 0x35b30`、`0x35a2b movq $0x0,(%rcx)`、`0x35a32 test %rdx; je 0x35b30`——两条违规路径都在 store 之前。
- `crates/qianqian-decode-songcore/src/lib.rs:521-527` 在每次调用前 `let mut produced: u64 = 0;`，所以 in-repo 适配器永远观察不到这个违规；`e1_abi_tests.rs:158,168` 同样预清零。
- 既有归档 `evidence/songcore-172/linux/verify/suite/suite_ext.c:366-381` 只 gate 了 capacity==0，NULL-dst 只作为 observation。
- 合约 §16 D3:917-951 明确两条 pre-zeroing guard，并要求任何代码修复覆盖两条；`needsOwnerDecision`。
- Windows 运行时：**NOT_RUN**（只做了 Linux 构建 + 执行）。

#### 技术与兼容性影响

- A/D：只改 `song_read_pcm` 内的语句顺序；无 ABI layout 改变；header 不变；`SONGCORE_ABI_VERSION` 不变（不是语义破坏，而是让实现符合已发布的 v1 语义）。但两个 target 的产物 hash 会变，必须重建、重新打包、重跑 V1–V7 与 G0–G2。
- B：改 header 文本 → header hash 改变 → §3 身份修订；而且它削弱一个 v1 承诺。header `:13-15` 明确 “a layout/semantic break requires ABI v2”，所以 B 实际上是 ABI-v2 级别的改动，而不是文字润色。
- C：NULL-handle + 有效 out 指针的调用仍读回 `UINT64_MAX`；等于没修。
- D 的“全称命题”还要求处理 `out_frames_produced == NULL` 的情况（承诺字面上不能声称会写一个空指针）；探针已有三条 poisoned 调用，可直接扩成回归。

#### 风险

- 直接发布：已知的契约违反会进入 release；消费者无法信任 “produced == 0 after any error”。
- 只修 dst guard：漏掉 NULL-handle 路径。
- B：把 v1 承诺改弱，却没有 ABI 版本信号；第三方已经按 rc.1 header 编译。
- 修代码后不重跑 Windows：Windows 产物仍是旧语义，形成两个 target 的语义漂移。

#### 推荐方案及理由

**推荐 D（其中 A 是代码半边）**。理由：header `:358` 是第三方编译时依据，且 `cf509a5c...` 的同一句话已经 ship 在 rc.1 包里；让实现符合它只需要一次语句重排，没有 ABI layout 变化、没有 `SONGCORE_ABI_VERSION` bump、没有“今天能跑的调用”被破坏；唯一改变的是两条原本不写 out-parameter 的路径。B 看起来更便宜，但它把 v1 承诺改弱且没有 ABI 版本信号，按 header `:13-15` 属于 v2 级别。C 明确不完整。A 之后必须把探针提升为 gate，否则同样的回归可以再次发生。

#### 选择后要改哪些文件与门禁

```text
D:
  native/src/songcore_ffmpeg.c:1111-1114（把清零提到两条 guard 之前）
  native/include/songcore.h:358（把承诺写全称，可选但推荐）
  evidence/songcore-172/g4/d3-readpcm-zero/readpcm_zero_probe.c（提升为 gate 回归）
  docs/release/songcore-release-contract.md §16 D3（disposition）
门禁:
  linux + windows 重新构建（G0），G1/G2，打包，V1–V7
  d3 probe 在两个 target 运行时执行（windows 目前 NOT_RUN）
  若 header 文本改动：重新计算 header sha256，更新 §3，V2 用新期望值

B:
  native/include/songcore.h:358（改弱承诺）
  §3 header 身份修订；两个 target 重新打包
  不推荐：按 :13-15 应视为 ABI v2 级语义变化

C:
  明确拒绝（NULL-handle 路径仍违规）
```

#### 不选择的后果

- 发布的库在两个 guard 路径上违反自己的 header；消费者若按 `:358` 断言 `produced == 0` 会失败（实测 3 个 violating 调用）。
- D3 继续 Owner-blocked；按本包规则，不得为赶发布绕开。
- 若只做 C 或只改文档，问题会以另一种形式留下：前者仍违规，后者是 ABI-v2 级别的静默弱化。

---

## 4. G4 门禁现状

> 状态口径：**PASS** = 本轮某个 lane 真实执行并返回 PASS（不是本包执行）；**PASS_WITH_GAPS** = lane 已执行的部分全部 PASS，但合约要求的场景没有跑满——本包会计状态，**不计入“已满足”，也不写成 PASS**；**FIX_REQUIRED/FAIL** = lane 实测不合格；**NOT_RUN** = 未执行（不是通过）；**OWNER_BLOCKED** = 需要 Owner 决定；**N/A** = 合约明确不要求。

### 4.1 已满足（本轮 lane 实测 PASS）

| 项 | 状态 | 实测依据（lane） | 关键数字 |
|---|---|---|---|
| G0 BUILD linux-x86_64 | PASS（L1 NOT_RUN） | `build-linux` | static `0d38f4b5...` 4,066,980 B / shared `085425e5...` 2,288,536 B；280/280 ELF；closure 20/20 校验 |
| G0 BUILD windows-x86_64 | PASS（L1 NOT_RUN） | `build-windows` | static `89d0428d...` 3,309,294 B；280/280 PE；15/15 symbols |
| G1 LINK linux | PASS | `consume-linux` | static + shared consumer 均 exit 0；NEEDED 仅 libm/libc |
| G1 LINK windows | PASS | `lane-c-windows-pe` | `-lbcrypt` 必需；PE `3ec3c672...` 在真实 Windows 运行 |
| V1 集合算术 | PASS | `package` / `lane-d` | win 14/13/12；linux 15/14/13；独立 re-hash 0 mismatch |
| V2 header sha | PASS | `package` | 两包 `include/songcore.h` = `cf509a5c...78536` |
| V3 pin/closure | PASS | `package` | pin `224ad3d5...`；closure win `0d0f7af9...` / linux `d037e8b4...` |
| V4 symbols | PASS | `package` | 15/15；`symbols.txt` 一致 |
| V5 license flags | PASS | `package` / `lane-d` | gpl/version3/nonfree=false；`external_libs=[]` |
| V6 reproducibility 字段 | PASS（NOT_ESTABLISHED 如实） | `package` | 三项均 `NOT_ESTABLISHED`；无虚假声明 |
| V7 tools/target identity | PASS | `package` | cc/ar/xmake 版本逐字匹配；recipe sha 匹配 |
| O1 FFmpeg COPYING | PASS | `lane-d` | 4/4 字节等于 pinned zip 提取 |
| O2 THIRD_PARTY_NOTICES | PASS | `lane-d` | 两包均存在且被 checksums 覆盖 |
| O4 configure args | PASS | `lane-d` | 37/39 args；0 个 `--enable-gpl/version3/nonfree/lib*` |
| B1–B4 | 未触发（measured non-trigger） | `lane-d` | 4/4 texts 一致；V5 inputs 干净；notices 在；MIT/Apache 字节一致 |
| Runtime deps linux | PASS | `lane-e-runtime-deps` | NEEDED 仅 libm/libc；无 FFmpeg；15 exports |
| Runtime deps windows | PASS | `build-windows` / `lane-c-windows-pe` | 三个 BCrypt imports；无 `-lbcrypt` 失败、有则成功 |
| Consumer static linux | PASS | `consume-linux` | 4/4 PCM 精确；seek/EOF/malformed |
| Consumer shared linux | PASS | `consume-linux` | 无 SONAME；`LD_LIBRARY_PATH` 下可解析；负控 exit 127 |
| Consumer windows PE | PASS | `lane-c-windows-pe` | 4/4 PCM；真实 Windows 主机 exit 0 |
| E01–E18 | PASS | `release-policy` | exit 0；69 次执行 / 43 个非空 id（23 次空 id）；0 个 exit 不匹配 |
| Package security T1–T5 | PASS | `release-policy` + 定稿人复跑 | receipt 原覆盖的是 v0.2.0-rc.1 zip `7b4d0615...` 与 v0.1.0 tar.gz `018e0cbd...`；定稿人对本轮两个 preflight 容器（win `5d92b21d...` / linux `bfe780c3...`）重跑同一 `check_package_security.py`：均 exit 0、`findings=0`、T1–T5 全 `true` |
| Product no-overwrite 设计 | PASS | `release-policy` | R1 拒绝 songcore tag；R7 拒绝 existing；无 clobber 路径 |
| v0.1.0 不可变 | PASS | `p0-governance` | tag/release/asset 未动；asset sha `018e0cbd...` 与本地一致 |

#### 4.1.1 部分满足（PASS_WITH_GAPS；不计入“已满足”）

> G2 的合约定义是 S-BIND §7.1 **步骤 01–13 全跑**（contract §11:586-598）。两个 target 都没有跑满，因此 G2 不能写成 PASS，也不进入“已满足”。

| 项 | 状态 | 实测依据 | 缺口（lane 自己的 `not_run` + 定稿人复核） |
|---|---|---|---|
| G2 CORE_RUNTIME linux | PASS_WITH_GAPS | `consume-linux` | 已跑部分 4/4 fixtures 精确匹配、static == shared == reference；受门禁 8 次 corpus run（4 static + 4 shared；argv 共 10 次 corpus 调用）；缺 step 06 artwork payload（所有 fixture `artwork_count=0`）、step 13 close 后 view invalidation（未直接断言）、step 12 的 truncated-MP3 typed-error 分支（实际 clean EOF 108335 frames） |
| G2 CORE_RUNTIME windows | PASS_WITH_GAPS | `lane-c-windows-pe` | 已跑部分 4/4 PCM 精确匹配、seek/EOF/metadata 全走；**step 12（malformed/truncated）与 step 13（drop mid-stream then reopen / close 后 view invalidation）在 Windows lane 完全未执行**，且 lane 的 `not_run` 未声明这两个缺口（定稿人 `grep -n 'garbage\|trunc\|reopen' pe-consumer.c` 无命中） |

### 4.2 未满足（FIX_REQUIRED / FAIL / NOT_RUN）

| 项 | 状态 | 实测依据 | 缺口 |
|---|---|---|---|
| §2 version identity（四元等式） | **FIX_REQUIRED** | `p0-contract` / `package` / `p0-entries` | manifest `0.0.0-preflight` vs xmake `0.1.0`；tag 不存在；两个打包器无等式检查 |
| Linux 打包入口在 main | **FIX_REQUIRED** | `p0-entries` / `tools-linux-substrate` | main 无 `ops/package-linux.sh`；仅未合并分支 `6df6c42`（blob `8d36f443...`），PR 未开 |
| `ops/build-linux.sh` pin / clone | **FIX_REQUIRED** | `p0-entries` | 硬编码 `BRANCH`/`PINNED_HEAD=bf23516...`（`:41-42`），无法指向 69ba1ce；clone 路径 `:49`/`:79`/`:80` 在干净主机必然失败；真实运行是 cache no-op（0.267s/0.373s，0 compile lines），`build_linux_usable_at_H=NO` |
| `ops/package-rc.ps1` artifact provenance | **FIX_REQUIRED** | `p0-entries` | 无 artifact↔HEAD 绑定；把 2026-10-06 的 `408d7d6e...` 包成 `git_sha=69ba1ce`，exit 0 |
| 默认 artifact pin 过期 | **FIX_REQUIRED** | `p0-entries` | `run-csuite.ps1:52` / `run-binding-gate.ps1:49` 的 `1f5f8cf8...` 不存在于任何产物；override 后 12/12 PASS |
| `verify-build.ps1` 引用 | **FIX_REQUIRED** | `p0-entries` | `build-rc.ps1:344` 指向不存在的 `ops/verify-build.ps1` |
| `verify-external.ps1` 默认值 | **FIX_REQUIRED** | `p0-entries` | 默认路径指向归档 worktree；header 仍称 `7b4d0615` 是唯一当前容器且 “package-rc 不得重跑”，与实测 `e0c488a9` 矛盾 |
| O3 relink 可执行说明 | **FIX_REQUIRED**（字面 PASS，严格标准 FAIL） | `lane-d` | README 无 repo URL、无 fetch/import/xmake/merge_archive 命令；closure manifest 被 `native/.gitignore:1` 忽略，却被 README 称为“在源码仓库中” |
| Windows notices/README 准确性 | **FIX_REQUIRED** | `lane-d` | notices 提到 `libsongcore.so`（Windows 包没有）；README 称 notices 是 “published songcore-v0.1.0 asset”（实为 P3 刷新版） |
| docs-verify | **FAIL / NOT_RUN** | `release-policy` | `verify.mjs` PASS；`npx vitepress build` 三次 exit 1（esbuild remove Access denied）；`tools\docs-verify.cmd` 不存在；CI 路线 NOT_RUN |
| L1 / L2 / L3（本轮） | **NOT_RUN** | `build-linux` / `build-windows` / `package` | 两个 build lane 都复用 closure/warm cache；package lane 三项 `NOT_ESTABLISHED`；#329 的 L1–L3 是 campaign evidence，不是本轮 release 证据 |
| G3 BINDING_RUNTIME | **N/A / NOT_RUN** | 合约 G3:599-610 | 本轮无 binding 随包发布；合约明确 “Not required when no binding ships for the target” |
| G4 RELEASE | **NOT_RUN / OWNER_BLOCKED** | 全部 lane | 版本 identity 未闭合、D1/D3 未决、发布器不存在、docs-verify 未绿 |
| 运行期覆盖缺口（G2） | **NOT_RUN** | `consume-linux` / `lane-c-windows-pe` | artwork payload（所有 fixture artwork=0，仅走 count==0）；close 后 view invalidation；multi-stream/`song_select_stream`；truncated-MP3 的 typed-error 分支；**Windows 完全未跑 step 12/13**；windows bcrypt 熵路径（0 次调用） |
| Linux fresh-clone receipt 未保留 / 包 provenance 指向 clone=reuse | **FIX_REQUIRED（证据）** | `build-linux` / `package` + 定稿人复核 | lane 的 `/home/hoo/g4-172/linux-root/src` 与 receipts 目录已不存在；64 条 `evidence_paths` 中 42 条为绝对 WSL 路径、按原样不可解析（映射到 `build-linux/logs/` 后 64/64 hash 对）；包的 build receipt `1f574edb...` 是 `ops/build-linux.sh` 的 `clone=reuse` / `closure=reuse` 运行，且未 pin `ops/build-linux.sh` blob `18dba884...` |
| in-build closure 陈旧检测不比较 `recipe_sha256` | **FIX_REQUIRED（代码）** | 定稿人复读 `native/build/ffmpeg.lua:49-104` | `manifest_target_problem` 只比较 id/platform/arch/target_os/cross_prefix；`grep -n 'recipe_sha256' native/build/ffmpeg.lua` 无命中（exit 1），而注释 `:48` 声称 outdated recipe 会 stale。lane 驱动（`ops/build-rc.ps1:415-416`、`g4-closure-validate.py:99-101`、打包器 `package-rc.ps1:741`）有更严格检查，本轮产物未受影响；仅 in-build gate 本身缺该检查 |
| 编译期 FFmpeg 版本字符串与 manifest 不一致 | **FIX_REQUIRED** | 定稿人 `grep -a` + 读 manifest | linux static 归档内嵌 `FFmpeg version qianqian-v0.1.0-315-gbf23516`；windows static 归档内嵌 `9.0.1`；两包 manifest `ffmpeg_version=9.0.1`；无 V-check 比较 compiled string。共享 `.so` 只导出 15 个 `song*`（`av_version_info` 不在动态导出集），缺陷面向 static 消费者；共享形态的编译期字符串未在本轮测得（NOT_RUN） |
| `artifact_shared` 表示不一致 | **FIX_REQUIRED（§8 缺口）** | 定稿人读两包 manifest + v0.1.0 manifest | windows `:19` boolean `false`；linux `:19` string `"lib/shared/libsongcore.so"`；v0.1.0 `:19` string。contract §8:342 只说 carried field names keep their meaning，:354-359 只对 `target`/`toolchain` 明说 type；无 V-check 覆盖该字段 |
| package lane 缺 `stage.json` | **FIX_REQUIRED（证据）** | 定稿人 `find package -maxdepth 1 -name stage.json` 无输出 | README:66 与 README:289 的“每 lane 有 stage.json / 读 `stage.json.not_run`”对 `package/` 不成立；该 lane 的机器证据是 `package-laneB/stage.json`（`stage_id=g4-package-laneB`）+ `package/*/package-summary.json` + `v-checks.json` |
| S-BIND §7.2 的有损 tolerance/hash 政策不存在 | **FIX_REQUIRED（文档）** | 定稿人 `grep -rn 'frozen deterministic tolerance' docs native tools` | 在 `docs`/`native`/`tools` 下只命中 `docs/architecture/songcore-binding-architecture.md:473` 与 `contract:593` 两句 prose，未找到政策文件。consumer lane 实际使用 exact SHA-256（比 tolerance 更严格），因此**没有放宽任何 pass**，但“per reference policy”的引用名不副实 |
| Windows shared/DLL | **NOT_RUN / 不在目标** | `package` / `lane-c-windows-pe` | 包内无 shared form；无 DLL consumer |
| Windows MSVC / 32-bit / ARM64 | **NOT_RUN** | `lane-c-windows-pe` | 只有 mingw-w64 交叉 |
| Rust/JNI/Apple binding parity | **NOT_RUN** | `consume-linux` | 本轮无 binding 消费者 |
| `ops/**` 自动覆盖 | **FIX_REQUIRED（治理）** | `p0-entries` | lefthook release-policy globs（`:154-162`）不含 `ops/**`；CI 无 job 调用 `ops/` |
| registry ARCH-008 证据 | **FIX_REQUIRED（文档）** | `p0-evidence-spec` | `registry.yml:247-248` 缺 S3 linux 与 #329 归档 |
| 证据卫生 | **FIX_REQUIRED** | `p0-evidence-spec` + 本包复算 | g4 下 760 个文件、多份 >100 KiB 的 `.a/.so/.zip/.tar.gz/.exe`（本包复算 `find -size +100k` 命中 20+）；根 `RECEIPTS.sha256` 尚未生成（reconcile lane 最后生成） |

### 4.3 Owner-blocked

| 项 | 依据 | 说明 |
|---|---|---|
| Q1 版本号 | 合约 §14 Q1:758-759 | 未选号 → 无法生成合法 RC |
| Q2 正式登记 | 合约 §14 Q2:760-763 + 本轮 `p0-evidence-spec` | 实质已接受；文本/证据未登记 |
| Q3 Windows 形态 | 合约 §14 Q3:764-767 | static-only vs DLL |
| Q4 glibc/发行版政策 | 合约 §9:488-503 + §14 Q4:768-770 | 测量值 2.43 ≠ 政策 |
| Q5 发布机制 | 合约 §14 Q5:771-775 | 无 SongCore 发布器 |
| D1 | 合约 §16 D1:855-882 | header 文本 vs 编译语义 |
| D3 | 合约 §16 D3:917-951 | 代码修复 vs header 弱化 |
| License block 终审 | 合约 §10:558-560 | 机器检查不能替代法律审阅；每 release 由 Owner 审阅 |
| Tag / Release 创建 | 本轮任务规则 + 合约 §13 | 本轮明确禁止；只有 Owner 在 G4 后可授权 |

---

## 5. 距离合法 RC 还差什么

> **结论先行**：本轮预检的 `ENVIRONMENT_BLOCKED` 不是单一原因造成的。即使 Owner 立即做出全部决策，仍有工程前置未完成；反之，即使工程全部修好，没有 Q1/Q3/Q4/Q5/D1/D3 的授权也不能发布。两类前置必须分开推进。

> **`ENVIRONMENT_BLOCKED` 的环境事实**（逐项按定稿人复核校正）：a) `docs-verify` 的 VitePress 半场在本机三次 FAIL（`[vite:esbuild-transpile] remove <tmp>\esbuild-<hash>: Access is denied`），CI 路线 NOT_RUN；b) WSL 无外网（p0-governance / release-policy lane 记录：`gh` 与 FFmpeg 重新下载/派生均不可用，`ip addr` 仅 `lo`），所有 ELF/交叉工具链工作只能在 WSL 完成；c) Windows 主机无 `gh`/`gcc`/`ar`/`objdump`/`readelf`/`x86_64-w64-mingw32-gcc`，`python3` 是 WindowsApps stub；d) **GitHub 状态并非不可读**：p0-governance 用 Windows `curl` + `jnhu76` token 成功读取（6 次 HTTP 200、`admin=True`、releases total=2），本包 §2.1 的 v0.1.0 时间戳即来自该读取；e) tag/Release 的写操作在本轮**规则上明确禁止**，PR 未开是本轮只读范围的选择，**不是网络不可达**。工程侧的 FIX_REQUIRED 项（§4.2）与上述环境缺口叠加，但两者不是同一类问题。

### 5.1 工程前置（不需要 Owner 决策，但需要代码/文档 PR 与重跑）

| # | 前置 | 现状 | 完成判据 |
|---|---|---|---|
| E1 | 把 Linux 打包入口落到 main | `ops/package-linux.sh` 只在 `ops/songcore-linux-release-substrate @ 6df6c42`（blob `8d36f443...`）；PR 未开（本轮只读，未执行 GitHub 写操作） | main 上存在 `ops/package-linux.sh`；PR 经 review 合并；`--help`/负向 guard 在 main 可复现 |
| E2 | 修 `ops/build-linux.sh` | 硬编码 pin（`:41-42`）；clone 路径必然失败（`:49`/`:79`/`:80`）；真实运行是 cache no-op | 传入 69ba1ce 可实际构建；干净主机可 clone；日志有真实 compile lines |
| E3 | 修 `ops/package-rc.ps1` 的 artifact provenance | 陈旧 artifact 可用当前 HEAD 名义打包（实测） | 打包器要求 build receipt（head + artifact sha）并与 `gitHead` 绑定；陈旧 artifact 被拒绝 |
| E4 | 修过期 pin/默认值 | `run-csuite.ps1:52` / `run-binding-gate.ps1:49` 的 `1f5f8cf8...` 无对应产物；`build-rc.ps1:344` 指向不存在的 `verify-build.ps1`；`verify-external.ps1` 默认指向归档 worktree | 默认值由 provenance 派生或显式更新；缺失文件引用消失 |
| E5 | 版本 bump PR | `native/xmake.lua:2` 仍是 `0.1.0` | Owner 给号后，`set_version` = manifest = tag = 包目录；两个打包器有 §2 等式检查 |
| E6 | D3 修复 PR（若选 D） | `songcore_ffmpeg.c:1111-1114`；探针 VIOLATIONS 3 | 清零覆盖两条 guard；linux + windows 探针 0 violation；重建/重打包/V1–V7 |
| E7 | D1 header 修订 PR（若选 A） | `songcore.h:217-218` + `:226-228`；`songcore-binding-architecture.md:516` | 文本一致；两个函数的错误列表都补 `SONG_ERR_INVALID_ARGUMENT`；新 header sha；§3 同步；V2 新期望值 |
| E8 | licensing 文档修复 | O3 relink 不可执行；closure manifest 声明错误；Windows notices 提到 `.so`；Windows README 称 v0.1.0 asset | README 有 repo URL + fetch/import/xmake/merge_archive 命令；closure manifest 被正确描述为生成物；notices/README 准确 |
| E9 | docs-verify 在干净主机/CI 变绿 | `verify.mjs` PASS；`vitepress build` 本机 FAIL；`tools\docs-verify.cmd` 不存在 | CI 或干净主机上 `verify.mjs` + `vitepress build` 均 exit 0；或修正 AGENTS.md 路线 |
| E10 | 重跑 G4 全矩阵 | 本轮 L1/L2/L3 NOT_RUN；G4 NOT_RUN；预检包 version identity 非法 | 用最终 artifact 重新执行 G0–G2、V1–V7、O/B、runtime audit、consumer、package security，并保留 receipt |
| E11 | 治理覆盖 | `ops/**` 不在 lefthook release-policy globs，CI 也不调用 | 把 `ops/**` 纳入 release-policy 或新增 CI job；负向控制可复现 |
| E12 | 证据卫生 | g4 下 760 文件、多份 >100 KiB 二进制；根 `RECEIPTS.sha256` 未生成 | 删除/移出二进制；reconcile lane 生成根 RECEIPTS；registry 证据补齐 |
| E13 | 打开 tools substrate PR | 分支已推送（`6df6c42`）；PR 未开：本轮只读、未执行任何 GitHub 写操作（live 读取可用，见 §5 环境事实） | PR 打开并 review；或明确以分支方式交付 |
| E14 | in-build closure 陈旧检测 | `manifest_target_problem` 不比较 `recipe_sha256`（`native/build/ffmpeg.lua:49-104`） | 比较当前 recipe 文件的 sha256 与 `manifest.target.recipe_sha256`；不匹配 fail-closed |
| E15 | FFmpeg 编译期版本字符串身份 | linux static 归档 `qianqian-v0.1.0-315-gbf23516` vs manifest `9.0.1`；无 V-check | 定义并以机器检查绑定 compiled `FFmpeg version` 与 `manifest.ffmpeg_version`（或把派生字符串记录为独立字段） |
| E16 | `artifact_shared` 表示 | windows boolean vs linux/v0.1.0 string；§8 未规定 static-only 表示 | 合约/打包器明确表示并加 V-check（carried-field 类型一致性） |
| E17 | package lane / build-linux 证据回执 | `package/` 无 `stage.json`；build-linux receipt 的绝对路径不自包含 | 补 lane 级 stage.json（argv/exit_codes/hash）；收据路径改为仓库相对或归档镜像；README 的 `stage.json.not_run` 指引同步修正 |

### 5.2 授权前置（只有 Owner 能做）

| # | 前置 | 影响 |
|---|---|---|
| A1 | Q1：版本号 + suffix 政策 + `set_version` 耦合 | 没有它，E5/E10 无法完成；包只能是非发布 identity |
| A2 | Q2：把 #329 结论正式登记 | 没有它，合约仍指向未采用的 flag；L3 无权威 |
| A3 | Q3：Windows static-only vs DLL | 没有它，artifact set 无法冻结 |
| A4 | Q4：glibc floor / 发行版政策 | 没有它，`.so` 的可运行范围无法声明 |
| A5 | Q5：人工 vs 自动发布 | 没有它，合法 RC 之后也没有合法发布路径 |
| A6 | D1：header 文本 vs 编译语义 | 没有它，header 的 normative 句子与实现继续矛盾 |
| A7 | D3：代码修复 vs header 弱化 | 没有它，已知契约违反会进入 release |
| A8 | License block 终审 | 合约 §10 明说机器检查不能替代法律审阅 |
| A9 | Tag / Release 创建授权 | 本轮明确禁止；只有 Owner 在 G4 后可授权 |

### 5.3 不可绕开的原则

```text
D1/D3 不是“可选清理项”。它们是 header/实现之间的已测量差异；任何 RC 之前必须
有 disposition（A/B/C 之一），不得以 “G4 的 V1–V7 没列 D1/D3” 为由跳过。

L1/L2/L3 本轮没有重新获得，不能把 #329 的 campaign 结果当作本轮 release 证据；
任何 release 级 reproducibility 声明必须按 §12 重新执行两独立根构建。

docs-verify 未绿（vitepress build FAIL + 文档路线缺失），不能写成 PASS；
E01–E18 的 PASS 只覆盖产品发布策略的负向控制，不覆盖 SongCore 发布（后者不存在）。

0.0.0-preflight 包不是 RC：两个 manifest 的 notes 都明确写了 not a release；
Windows notes 另写明 must never be published under the songcore-v* namespace
without owner ratification；§2 version identity 在这两个包上是 FALSE。
本轮另外存在的 RC 命名容器（§2.3）同样不是已授权 RC：没有 tag、没有
GitHub Release、没有发布路径，version_status 为 provisional / owner-reserved。
```

---

## 6. 证据索引（lane → 目录 → verdict → 关键文件）

| lane | 目录 | verdict | 关键证据 |
|---|---|---|---|
| 合约审计 | `evidence/songcore-172/g4/p0-contract/` | `OWNER_BLOCKED` | `stage.json`、`sha256-inputs.txt`、`export-count-grep.txt`、`name-diff.txt`、`v010-manifest-fields.txt`、`rc-v1-rehash.txt`、`rc-v3-pin.txt` |
| 入口审计 | `evidence/songcore-172/g4/p0-entries/` | `FIX_REQUIRED` | `entry-inventory.txt`、`build-linux-real-run2-rc.txt`、`build-linux-run2-summary.json`、`build-linux-clone-fail.log`、`package-rc-default.log`、`package-rc-stale-artifact.log`、`run-csuite.log`、`verify-external.log` |
| 治理审计 | `evidence/songcore-172/g4/p0-governance/` | `PASS` | `AUDIT.md`、`git-forensics.log`、`release-summary.json`、`rest-auth-releases-list.json`、`rest-matching-refs-songcore-v.json` |
| 证据规范 | `evidence/songcore-172/g4/p0-evidence-spec/` | `FIX_REQUIRED` | `README.md`、`stage.json` |
| D1 | `evidence/songcore-172/g4/d1-out-handle/` | 差异实测（Owner-blocked） | `run.log`、`disasm-linux.txt`、`disasm-windows.txt`、`d1_probe.c`、`stage.json` |
| D3 | `evidence/songcore-172/g4/d3-readpcm-zero/` | 差异实测（Owner-blocked） | `logs/run-force.log`、`logs/song_read_pcm-force.dis`、`readpcm_zero_probe.c`、`stage.json` |
| Linux 构建（G0） | `evidence/songcore-172/g4/build-linux/` | `PASS`（L1 NOT_RUN） | `summary.json`、`stage.json`、`verify/`、`compare-329/`、`logs/compile-commands/` |
| Windows 构建（G0） | `evidence/songcore-172/g4/build-windows/` | `PASS`（L1 NOT_RUN） | `build-rc-summary.json`、`verify-artifact.log`、`census.log`、`compare-329*.log`、`objdump-f.txt`、`stage.json` |
| Linux 消费者 | `evidence/songcore-172/g4/consume-linux/` | `PASS`（G2 覆盖缺口见 4.1.1） | `out/comparison.json`、`logs/stage03-static-run.log`、`logs/stage04-shared-run.log`、`evidence-sha256.txt` |
| Windows PE 消费者 | `evidence/songcore-172/g4/lane-c-windows-pe/` | `PASS`（G2 step 12/13 未跑，见 4.1.1） | `build/pe-consumer.exe`、`build/logs/link-wo-bcrypt.log`、`run/run-all-windows.log`、`run/host.txt`、`raw/reference-comparison.json`、`stage.json` |
| 运行时依赖 | `evidence/songcore-172/g4/lane-e-runtime-deps/` | `PASS`（测量） | `raw/readelf-dynamic-so.txt`、`raw/glibc-versions.txt`、`raw/container-runs-exitcodes.txt`、`stage.json` |
| 打包（预检） | `evidence/songcore-172/g4/package/` | `FIX_REQUIRED` | `win/package-summary.json`、`linux/package-summary.json`、`*/v-checks.json`、`*/extract/` |
| Linux 打包器 lane | `evidence/songcore-172/g4/package-laneB/` | 引用 PR 分支运行 | `laneB-verify.json`、`linux-package.log`、`verify-run.log`、`stage.json` |
| 发布策略 | `evidence/songcore-172/g4/release-policy/` | `FIX_REQUIRED` | `release-controls.json`、`e01-e18.log`、`publish-policy-probes.log`、`package-audit-*.json`、`docs-verify-vitepress-build.log` |
| 许可 | `evidence/songcore-172/g4/lane-d/` | `FIX_REQUIRED` | `stage.json`、`logs/01`–`logs/10`、`verify-package-integrity.sh`、`diff-notices.sh` |
| 工具 PR | `evidence/songcore-172/g4/tools-linux-substrate/` | `ENVIRONMENT_BLOCKED` | `PR-BODY.md`、`stage.json`、`gates/`、`guards/`、`package/` |

> 除 `package/` 外的每个 lane 的 `stage.json` 是完整文件清单 + `argv`/`exit_codes`/`evidence_sha256`/`not_run` 的机器可读回执；`package/` **没有** `stage.json`（见 §4.2），其机器证据是 `package-laneB/stage.json`（`stage_id=g4-package-laneB`）与 `package/*/package-summary.json` / `v-checks.json`。本表只列关键文件。

---

## 7. 附：本包没有做的事

```text
没有替 Owner 选版本号；没有创建 tag；没有创建 GitHub Release
没有修改 header / 源码 / xmake / 合约 / 发布权限 / 产品代码
没有执行任何构建、链接、运行时、打包、docs-verify、lefthook、E01–E18
没有把任何 NOT_RUN 写成 PASS
没有把 Windows DLL 写成本轮目标；没有声称任何发行版受支持
没有用 #329 的 campaign 结果替代本轮 L1/L2/L3
```

---

## 8. 独立评审与处置（v2）

> **输入**：三位独立评审的 JSON，结论均为 `REVISE`——A=8 条（2 high / 4 medium / 2 low）、B=7 条（2 medium / 5 low）、C=7 条（4 medium / 3 low）。本节逐条处置。**Owner 决策字段一字未改**：Q1/Q3/Q4/Q5/D1/D3 仍 `UNDECIDED`，Q2 仍 `RECORDED_ACCEPTANCE_PENDING_FORMAL_CLOSEOUT`，`openDecisions = 7`。
>
> **处置口径**：`接受并已修正` = 评审事实成立，且正文/摘要已按实测改写；`接受并已修正（登记）` = 事实成立并已登记为 FIX_REQUIRED/E 项，但修复需要改代码/产物/回执，超出本定稿 lane 的只读边界；`部分接受/范围限定` = 事实成立，但评审结论的外推被限定，理由见 §8.4。

### 8.1 高严重度（2 条，全部接受并在正文改正）

| ID | 评审位置 | 处置 | 正文改动与实测依据 |
|---|---|---|---|
| A-1 | `build-linux/stage.json:44`（另 `:72`、`summary.json:100`、`drivers/g4-s6-publish.sh:3`） | **接受并已修正** | §2.2 的 “独立新克隆” 叙述已改写：fresh-clone 根 `/home/hoo/g4-172/linux-root/src` 与 `receipts/g4-linux-build` 在定稿时均不存在（WSL `ls -ld` 两次 `No such file or directory`）；该 lane 的 64 条 `evidence_paths` 有 42 条绝对 WSL 路径按原样不可解析，映射到 `build-linux/logs/` 后 64/64 hash 匹配、0 mismatch；产物字节改由 `package/linux/extract`（`0d38f4b5…`/`085425e5…`）与 `/home/hoo/g4-172/linux-root/native/build/artifacts` 复核；`package/linux/package-summary.json:26` 的 `build_receipt_sha256=1f574edb…` 绑定的是 `clone=reuse` 构建。§4.2 新增证据行、§5.1 新增 E17、§6 的 “每 lane 有 stage.json” 改为 “除 package 外”。 |
| A-2 | `preflight-summary.json:166-168`（v1 行号） | **接受并已修正** | §2.3 新增 “本轮 RC 命名容器” 表：`p0-entries/package-rc-out/default/songcore-v0.2.0-rc.1-windows-x86_64.zip`（`e0c488a9…`，`started=20261030-203142`，`artifact_sha256=89d0428d…`）、`ver-0.1.0-rc.1/songcore-v0.1.0-rc.1-windows-x86_64.zip`（`7e843c9a…`）、`stale-artifact/…`（`c3983bfb…`），以及 `tools-linux-substrate/package/manifest.json:3` 的 `songcore_version=0.2.0-rc.1`。同时明确这些产物没有 tag/Release/发布路径；`preflight-summary.json` 的 `NEW_RC.detail` 同步删除 “只有 preflight 包” 的错误陈述。 |

### 8.2 中严重度（10 条）

| ID | 评审位置 | 处置 | 正文改动与实测依据 |
|---|---|---|---|
| A-3 | `package/linux/package-summary.json:26`；`preflight-summary.json:33-34`；`build-linux/stage.json:44` | **接受并已修正** | §2.2 增加 provenance 段落：receipt `1f574edb…` 的 `entry=ops/build-linux.sh`、`clone=reuse`、`closure=reuse`、`source_repo=/home/hoo/g4-172/wt-tools`、`work_root=/home/hoo/g4-172/linux-root`；`ops/build-linux.sh` blob `18dba884…` 未记录在该 receipt 中。`LINUX_ARTIFACT_READINESS.l1_detail` 与 §2.2 使用同一事实；§4.2 新增证据行。 |
| A-4 | `native/build/ffmpeg.lua:48`（函数 `:49-104`） | **接受并已修正（登记为 FIX_REQUIRED，代码未改）** | `manifest_target_problem` 只比较 id/platform/arch/target_os/cross_prefix；定稿人 `grep -n 'recipe_sha256' native/build/ffmpeg.lua` 无命中（exit 1），而注释 `:48` 声称 outdated recipe 会 stale。§4.2 新增代码行、§5.1 新增 E14；**未改 `native/build/ffmpeg.lua`（本 lane 只读）**。lane 驱动侧（`ops/build-rc.ps1:415-416`、`g4-closure-validate.py:99-101`、`package-rc.ps1:741`）已有 recipe hash 检查，故本轮产物未被该缺口影响。 |
| A-5 | linux static 归档内嵌 FFmpeg 版本 | **接受并已修正（登记为 FIX_REQUIRED，产物未重建）** | 定稿人 `grep -a -o 'FFmpeg version [^ ]*'`：linux static = `qianqian-v0.1.0-315-gbf23516`，windows static = `9.0.1`；两包 manifest `ffmpeg_version=9.0.1`；打包器与 `v-checks` 均无 compiled-string 比较。§4.2 新增行、§5.1 新增 E15；未重建产物。共享 `.so` 只导出 15 个 `song*`，`av_version_info` 不在动态导出集，因此缺陷面向 static 消费者；shared 形态的编译期字符串 **NOT_RUN**。 |
| A-6 | `package/` 无 `stage.json`；`README.md:66/289`；packet §6 | **接受并已修正（正文/摘要；未补造回执）** | 定稿人 `find package -maxdepth 1 -name stage.json` 无输出；机器证据实为 `package-laneB/stage.json`（`stage_id=g4-package-laneB`）+ `package/*/package-summary.json` + `v-checks.json`。§4.2 新增证据行、§6 注改写；**没有补写 `package/stage.json`**（会构成事后伪造 argv/exit_codes）。README 的 `stage.json.not_run` 指引仍待修，登记为 E17。 |
| B-1 | G2 windows 行（v1 §4.1:674 / §4.2:713） | **接受并已修正** | G2 两行移出 “已满足”，新增 §4.1.1 `PASS_WITH_GAPS`；Windows 明确缺 S-BIND §7.1 step 12（malformed/truncated）与 step 13（drop mid-stream then reopen / close 后 view invalidation）。依据：定稿人 `grep -n 'garbage\|trunc\|reopen' lane-c-windows-pe/src/pe-consumer.c` 无命中；lane `not_run` 未声明；contract §11:586-598；S-BIND:453-455。 |
| B-2 | v1 packet:741（§5 环境事实）、:477（Q5）、:759（E13） | **接受并已修正** | “实时 GitHub release/tag 状态无法读取” 删除；改为 p0-governance 经 Windows `curl` + token 成功读取（`auth-read.log` 6 次 HTTP 200；`auth-summary.log` `permissions {'admin': True …}`、releases total=2），WSL `gh` 无网络只是 lane 级事实。Q5 事实与环境段落改写：本轮不能发布是**规则/授权**事实，不是不可达；E13 改为 “本轮只读、未执行写操作”。 |
| C-1 | v1 packet:692；`preflight-summary.json:192`；`release-policy/stage.json:6-13,39-40` | **接受并已修正** | 原 receipt 审计的是 `evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip`（`7b4d0615…`）与 `/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz`（`018e0cbd…`），不是本轮 preflight 容器。§4.1 行改为明确两个归档，并记录定稿人对本轮两个 preflight 容器重跑同一 `check_package_security.py`：win `5d92b21d…`、linux `bfe780c3…`，两者 exit 0、`findings=0`、T1–T5 全 `true`。 |
| C-2 | 同 A-6（`package/` 无 `stage.json`） | **接受并已修正** | 见 A-6。正文/摘要同步；未补造 lane 回执。 |
| C-3 | `package/win/.../metadata/manifest.json:19-20`；contract §8:342/354-359 | **部分接受并已修正**（理由见 §8.4-1） | 事实接受：windows `artifact_shared=false`（boolean）、linux 与 v0.1.0 为 string path，且无 V-check。**“违反 §8” 降级为 “§8 未规定的权威缺口”**：§8:342 只说 carried field names keep their meaning，§8:354-359 只对 `target`/`toolchain` 明说 type。§2.3 记录该差异，§4.2 新增 FIX_REQUIRED 行，§5.1 新增 E16；**未宣称已判定合约违规**。 |
| C-4 | `ops/package-linux.sh:141-142`（分支 `6df6c42`）；`tools-linux-substrate/PR-BODY.md:21` | **接受并已修正** | Q1 事实新增：新 Linux 打包器同样接受 `MAJOR.MINOR.PATCH[-suffix]` 并称其为 “contract section 2 VERSION_RULE form”；`PR-BODY.md:21` 写 “the §2 form is enforced”，而冻结 §2:96-98 无 suffix。§2.3 记录分支来源时同步。**未改分支代码**。 |

### 8.3 低严重度（10 条）

| ID | 评审位置 | 处置 | 正文改动与实测依据 |
|---|---|---|---|
| A-7 | `build-windows/stage.json:34` | **接受并已修正（正文记录；lane 回执未改）** | 定稿人复核：`.objs` 中 279/280 个 `.obj` 的 mtime 晚于 2026-10-30 20:00，仅 `songcore_ffmpeg.c.obj` 被复用（2026-10-29 04:20）；“cache-compiled all units” **低估**了重编译范围。§2.2 Windows bullet 已写明 279/280。 |
| A-8 | `build-windows/stage.json:11,31-32` | **接受并已修正（正文记录；lane 回执未改）** | `input_sha['ops/build-rc.ps1']` 是 prose、`exit_codes` 是 prose 字符串，不能机器校验。§4.2 证据卫生行与 E17 已覆盖；lane `stage.json` 不由定稿人改写。 |
| B-3 | `d1-out-handle` 的 “12/12” | **接受并已修正** | `grep -c MATCH run.log` = 10；`grep -n 'report(' d1_probe.c \| wc -l` = 11（1 定义 + 10 调用）。§2.6 与 D1 事实改为 **10/10**。**回执文件按任务要求逐字落盘，其 `D1_DISPOSITION` 仍保留工作流提供的 “12/12”；回执未改**；本包正文按实测记录。 |
| B-4 | `consume-linux` 的 “9 corpus runs” | **接受并已修正** | `comparison.json` 受门禁的是 4 static + 4 shared = **8 次**；lane argv 有 **10 次** `consume-corpus` 调用（含 1 次 RUNPATH、1 次 LD_DEBUG）。§4.1.1 与 `CONSUMER_SHARED_LINUX.detail` 已按此改写。 |
| B-5 | D1 option A / `song_probe` 错误列表 / adapter 注释 | **接受并已修正** | `songcore.h:226-228` 同样漏 `SONG_ERR_INVALID_ARGUMENT`，实现 `:875` 经 `fail_arg(h)` 返回；D1 option A 与 “要改的文件” 已同时覆盖 `song_open`/`song_probe`。`qianqian-decode-songcore/src/lib.rs:358-372` 的注释已把实现读法写成 verified fact，D1 事实已补。 |
| B-6 | `run-all-windows.log:4` | **接受并已修正** | 主日志 `host_windows=Binary file (standard input) matches`，正确捕获在 `run/host.txt`；§2.4 Windows PE 行已注明主机身份以 `host.txt` 为准。 |
| B-7 | S-BIND §7.2 的 tolerance/hash 政策 | **接受并已修正（登记为文档 FIX_REQUIRED）** | 定稿人 `grep -rn 'frozen deterministic tolerance' docs native tools` 只命中 `songcore-binding-architecture.md:473` 与 `contract:593` 两句 prose，未找到政策文件。实际比较用 exact SHA-256（更严格），**没有放宽 pass**；§4.2 新增文档行。 |
| C-5 | G2 在 “已满足” 中过强 | **接受并已修正** | 两行移入 §4.1.1 `PASS_WITH_GAPS`，并列出未跑步骤；不再计入 “已满足”。它不是 `FAIL`：已执行部分全 PASS，未执行部分是 `NOT_RUN`（见 §8.4-5）。 |
| C-6 | E01–E18 的 “44 labels” | **接受并已修正** | 定稿人 WSL python：69 executions、**43** 个非空 `id`、23 次空 `id`、0 个 `exit_code != expected`。§2.5 与 §4.1 已改为 43。 |
| C-7 | `build-linux/stage.json` 的绝对 `evidence_paths` | **接受并已修正** | 64 对 path/hash 中 42 条为绝对 WSL 路径、按原样不可解析（naive repo-root join 只解析 22 条）；映射到 `build-linux/logs/` 后 64/64 hash 匹配、0 mismatch。§2.2 与 §4.2/E17 已记录 “receipt 不自包含，镜像日志保留内容”。 |

### 8.4 部分接受、范围限定与不接受的措辞（理由）

1. **C-3（部分接受；不接受 “违反 §8” 的定性）**：事实接受——windows `artifact_shared=false`（boolean）、linux 与 v0.1.0 为 string path，且无 V-check。但 contract §8:342 只写 “all existing v0.1.0 field names keep their meaning”；§8:354-359 只对 `target`/`toolchain` 明确要求 **type** 不变；`artifact_shared` 在 static-only 包中的合法表示（boolean `false` vs 空字符串 vs 省略）**没有被合约规定**。因此降级为 “§8 未规定的权威缺口”，登记为 E16；在 Owner/合约澄清前，不能把它写成已证实的合约违反。
2. **A-1（接受；不接受 “artifact 身份不可复核” 的外推）**：接受 “无法从该 lane 自身 receipt 复算”。但不接受把该结论外推为 “本轮 Linux 产物不可复核”——字节可从 `package/linux/extract`（`0d38f4b5…`/`085425e5…`）与 `/home/hoo/g4-172/linux-root/native/build/artifacts` 两处独立 re-hash；不可恢复的是 fresh-clone 构建的 **receipt 链**，不是 artifact 的身份。
3. **A-5（接受；不接受 “shared 形态同样缺陷” 的外推）**：评审的对象是 archive（static），我确认该归档内嵌 `qianqian-v0.1.0-315-gbf23516` 与 manifest `9.0.1` 不一致。但共享 `.so` 只导出 15 个 `song*`，`av_version_info` 不在动态导出集内；因此不把 shared 记为同一缺陷，shared 形态记 `NOT_RUN`。
4. **C-1（接受；不接受 “覆盖完全不存在” 的外推）**：接受 receipt 的审计对象不是本轮 preflight 容器。但不接受把 “receipt 未覆盖” 外推为 “容器未通过 T1–T5”——定稿人已用同一 `check_package_security.py` 对两个 preflight 容器补跑（exit 0、`findings=0`、T1–T5 全 `true`）。原问题是**证据范围写错**，不是容器有问题。
5. **B-1/C-5（接受；不接受把 G2 写成 `FAIL`）**：接受 G2 不应列为 “已满足”。但已执行部分全部 PASS，未执行步骤是 `NOT_RUN`；因此记 `PASS_WITH_GAPS`，不记 `FAIL`。本包明确该状态只是会计状态，不是合约 gate 状态，不能替代合约的 PASS。

### 8.5 没有改变的事项（边界声明）

```text
Owner 决策字段：Q1/Q3/Q4/Q5/D1/D3 = UNDECIDED；Q2 = RECORDED_ACCEPTANCE_PENDING_FORMAL_CLOSEOUT
                 openDecisions = 7；PREFLIGHT_VERDICT = ENVIRONMENT_BLOCKED
未修改：native/include/songcore.h、native/src/songcore_ffmpeg.c、native/xmake.lua、
        native/build/ffmpeg.lua、docs/release/songcore-release-contract.md、任何产品代码或发布权限
未重建/未重打包：所有 artifact/package hash 与产物保持原样；A-4/A-5/C-3/C-4 只登记，未修复
未改写 lane 回执：没有补写 package/stage.json，没有改任何 lane 的 stage.json / verdict / not_run
未改写 receipt.md：按任务要求逐字落盘；其回执内的 “12/12”、“44 labels” 等原始表述保留，本包正文按实测更正
未打开 PR、未创建 tag、未创建 Release、未发布任何资产
仍需后续处理：README.md 的 :151（12/12）、:289（package 的 stage.json.not_run）、:313（44 labels）
             仍是旧表述，登记在 E17；本定稿只改 packet + preflight-summary + receipt 三个指定文件
```

> **给 Owner 的一句话**：A 的两条 high issue 都是**证据链/叙述**问题，不是产物字节问题——本轮 Linux/Windows 的 artifact sha256（`0d38f4b5…` / `085425e5…` / `89d0428d…`）没有变化；已改正的是 “这些字节由哪次构建、哪份 receipt、哪个容器证明” 的叙述，以及 G2/T1–T5/D1 覆盖范围的计数。

