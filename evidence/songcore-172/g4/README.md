# evidence/songcore-172/g4 — SongCore #172 G4 预检证据根

> **真值层级**：非权威证据（`evidence/README.md:3`）。本目录不定义架构语义，不授予发布权限；唯一发布权威是 `docs/release/songcore-release-contract.md`。
> **source_sha**：`69ba1ce05a2772f5aa077c6ece3365c40f6e0500`（`CURRENT_MAIN_SHA` 相同；`baseline_drift=no`）
> **round verdict**：`ENVIRONMENT_BLOCKED`
> **Owner 决策包**：[`G4-OWNER-DECISION-PACKET.md`](G4-OWNER-DECISION-PACKET.md)
> **机器可读摘要**：[`preflight-summary.json`](preflight-summary.json)
> **未决**：7（Q1、Q2-正式 closeout、Q3、Q4、Q5、D1、D3）

本 README 说明：目录结构、每个 lane/文件的用途、以及**哪些是 NOT_RUN**。所有 PASS/FIX_REQUIRED/FAIL 都来自本轮各 lane 的真实执行回执；本 README 作者没有重跑任何门禁（本包的执行边界见决策包 §0.1）。

---

## 1. 怎么读

1. 先读 `G4-OWNER-DECISION-PACKET.md` 的 §1（决策摘要）与 §4（门禁现状）。
2. 机器可读状态看 `preflight-summary.json`（键名与回执字段一致）。
3. 需要核对某个数字时，进入对应 lane 目录读 `stage.json`（完整文件清单 + `argv`/`exit_codes`/`evidence_sha256`/`verdict`/`not_run`），再读该 lane 的日志/证据文件。
4. 历史归档（v0.1.0、#329）在 `evidence/songcore-172/` 的其他子目录；本目录只引用，不复制其权威性。

---

## 2. 根文件

| 文件 | 用途 |
|---|---|
| `README.md` | 本文件：证据根说明、目录/文件用途、NOT_RUN 清单、卫生规则 |
| `G4-OWNER-DECISION-PACKET.md` | Owner 决策包：Q1–Q5、D1、D3 的可选方案/实测事实/影响/风险/推荐/改动清单/不选后果；G4 门禁现状；距合法 RC 的工程与授权前置 |
| `preflight-summary.json` | 机器可读摘要；键名：`BASE_SHA`、`CURRENT_MAIN_SHA`、`CONTRACT_AUTHORITY`、`TARGETS_IN_SCOPE`、`WINDOWS_ARTIFACT_READINESS`、`LINUX_ARTIFACT_READINESS`、`PACKAGE_LAYOUT`、`MANIFEST_V1_V7`、`LICENSE_O1_O4`、`LICENSE_B1_B4`、`RUNTIME_DEPENDENCIES`、`GLIBC_MEASURED_FLOOR`、`CONSUMER_STATIC`、`CONSUMER_SHARED_LINUX`、`CONSUMER_WINDOWS_PE`、`RELEASE_POLICY`、`DOCS_VERIFY`、`E01_E18`、`D1_DISPOSITION`、`D3_DISPOSITION`、`PREFLIGHT_VERDICT`、`NEW_RC`、`TAG`、`GITHUB_RELEASE`、`G4` |

> **没有根 `RECEIPTS.sha256`**。按 `p0-evidence-spec` 的约定，根 RECEIPTS 由 reconcile lane 在证据树清理后**最后**生成；本决策包 lane 不是 reconcile lane，未生成它。各 lane 的 `evidence-sha256.txt` / `evidence-final-sha256.txt` / `stage.json.evidence_sha256` 是当前可用的哈希回执。
>
> **归档轮更新（见 §8）**：本行描述的是预检 lane 落盘时的树。归档 PR 另行生成根 `receipts.sha256`（文件名小写，按本任务要求；`p0-evidence-spec` §5 的规范名是 `RECEIPTS.sha256`）与 `excluded-files.sha256`，并**不提交**下文 §6 列出的产物类文件。两份清单互不覆盖：`receipts.sha256` 只覆盖**已提交**的文件，`excluded-files.sha256` 只覆盖**未提交**的文件。

---

## 3. 目录结构

```text
evidence/songcore-172/g4/
├── G4-OWNER-DECISION-PACKET.md      本轮的 Owner 决策包（新增）
├── preflight-summary.json           本轮的机器可读摘要（新增）
├── README.md                        本文件（新增）
├── p0-contract/                     合约审计（header/exports/pin/profile/target/version/tag-form）
├── p0-entries/                      构建/打包入口审计（build-linux、package-rc、csuite、verify-external）
├── p0-governance/                   治理审计（git forensics、tag/release inventory、D1/D3 记录）
├── p0-evidence-spec/                证据规范（本目录的布局/回执/卫生规则来源）
├── d1-out-handle/                   D1 探针与反汇编（song_open out_handle）
├── d3-readpcm-zero/                 D3 探针与反汇编（song_read_pcm produced）
├── build-linux/                     Linux G0 构建 lane（static + shared）
├── build-windows/                   Windows G0 构建 lane（static，mingw）
├── consume-linux/                   Linux 消费者 lane（static/shared，corpus/seek/EOF/malformed）
├── lane-c-windows-pe/               Windows PE 消费者 lane（真实 Windows 主机）
├── lane-e-runtime-deps/             运行依赖 lane（NEEDED/glibc floor/旧发行版容器）
├── lane-d/                          许可 lane（O1-O4/B1-B4/notices/README）
├── package/                         预检打包 lane（win/linux 容器 + extract + v-checks）
├── package-laneB/                   Linux 打包器 lane（PR 分支运行回执）
├── release-policy/                  发布策略 lane（E01-E18、包安全 T1-T5、docs-verify、publish probes）
└── tools-linux-substrate/           工具 PR lane（ops/build-linux.sh + ops/package-linux.sh 分支证据）
```

> `p0-contract`、`p0-entries`、`p0-governance`、`d1-out-handle`、`d3-readpcm-zero`、`lane-c-windows-pe`、`lane-d`、`lane-e-runtime-deps`、`package-laneB`、`tools-linux-substrate` 是 `p0-evidence-spec` 九目录提案之外的实际 lane 名；未重命名，按实际路径引用。

---

## 4. 每个 lane 的用途、verdict 与文件

> 通用约定：每个 lane 的 `stage.json` 是机器回执，包含 `stage_id` / `source_sha` / `input_sha` / `target_toolchain` / `argv[]` / `exit_codes[]` / `evidence_paths[]` / `evidence_sha256[]` / `verdict` / `not_run[]`。下表的“关键文件”不是全部文件；完整清单以 `stage.json.evidence_paths` 与 `evidence-sha256.txt` 为准。

### 4.1 `p0-contract/` — 合约审计

- **verdict**：`OWNER_BLOCKED`
- **用途**：把合约 §2–§16 的冻结值与仓库现实逐项比对：header sha、15 exports、pin/profile、target artifacts、version identity、v0.1.0 不可变性、RC schema-2 清单、tag form grammar、runtime dependency grounding、artifact-level runtime audit。

| 文件 | 用途 |
|---|---|
| `stage.json` / `stage.sha256` | 机器回执与回执自身哈希 |
| `sha256-inputs.txt` / `sha256-win-worktree.txt` | 本轮输入的 sha256（contract/header/pin/profile/targets/xmake） |
| `env-identity.txt` / `git-tracked-status.txt` | 执行环境与 tracked 树状态 |
| `abi-version-lines.txt` / `export-declarations.txt` / `export-count-grep.txt` / `export-parse.txt` | `SONGCORE_ABI_VERSION` 与 15 export 的独立证据 |
| `contract-3-list-sorted.txt` / `header-names-sorted.txt` / `name-diff.txt` | 合约 §3:140-146 与 header 的集合差（空） |
| `pin-fields.txt` / `profile-fields.txt` / `targets-fields.txt` / `xmake-version-lines.txt` / `version-strings.txt` | §5/§8/§2 身份字段 |
| `v010-tarball-sha.txt` / `v010-extract.txt` / `v010-manifest-fields.txt` / `v010-header-vs-source.txt` | v0.1.0 历史基线（tarball `018e0cbd...`） |
| `rc-receipt-identity.txt` / `rc-package-dir.txt` / `rc-manifest-identity.txt` / `rc-v1-rehash.txt` / `rc-v2-header.txt` / `rc-v3-pin.txt` | 既有 RC 归档的 V1/V2/V3 交叉核对 |
| `rc-zip-entries.txt` / `rc-checksums-lines.txt` / `rc-checksums-paths.txt` / `rc-checksums-set-diff.txt` / `rc-files-vs-checksums.txt` / `rc-staged-files*.txt` / `rc-extracted-*.txt` | RC 包的集合算术与解包树 |
| `run-*.sh` / `run-*.log` | 各阶段执行脚本与日志（`run.log`、`run-secondary`…`run-finalize3`、`run-patch`、`run-manifest`） |
| `evidence-sha256.txt` / `evidence-final-sha256.txt` | 证据文件哈希清单 |
| `stage.json.not_run` | **NOT_RUN**：artifact-level runtime audit（bcrypt import set、280-member objdump、G0/G1/G2 runtime claims）；本 lane 没有 build/link/run/package |

- **关键失败**：`version_identity_chain = OWNER_BLOCKED`（xmake 0.1.0 vs manifest 0.2.0-rc.1/0.0.0-preflight；无脚本读 `set_version`）；`tag_form_grammar_differential = FIX_REQUIRED`（合约无 suffix vs `ops/package-rc.ps1:167-168` 接受 suffix）。

### 4.2 `p0-entries/` — 构建/打包入口审计

- **verdict**：`FIX_REQUIRED`
- **用途**：实测 `ops/build-linux.sh`、`ops/package-rc.ps1`、`ops/build-rc.ps1`、`ops/run-csuite.ps1`、`ops/verify-external.ps1` 的行为，判断它们能否作为 release substrate。

| 文件/组 | 用途 |
|---|---|
| `entry-inventory.txt` | `ops/` 入口清单（6 个脚本） |
| `build-linux-real-run2-rc.txt` / `build-linux-run2-summary.json` / `build-linux-run-facts.env` | `build-linux.sh` 在 HEAD 的真实运行（cache no-op，head 仍为 `bf23516...`） |
| `build-linux-build-times.log` / `list-build-times.sh` / `measure-build-linux-*.sh` | 构建耗时（0.267s/0.373s，0 compile lines） |
| `build-linux-clone-fail*.log` / `build-linux-clone-fail-rc.txt` | 干净 clone 路径失败（`:49`/`:79`/`:80`） |
| `package-rc-default.log` / `package-rc-out/default/package-summary.json` | 默认 RC 打包（`e0c488a9...`，14 entries） |
| `package-rc-stale-artifact.log` / `package-rc-out/stale-artifact/` | 陈旧 artifact 被以当前 HEAD 名义打包（provenance 缺陷） |
| `package-rc-cold.log` / `package-rc-cold-noartifact.log` | 冷树 fail-closed 行为 |
| `package-rc-version-0.1.0.log` / `-version-0.1.0-rc.1.log` / `-version-banana.log` / `-version-1.2.log` | 版本拒绝/接受实测（`0.1.0-rc.1` 被接受） |
| `run-csuite.log` / `run-csuite-override.log` / `csuite-results*/` | 默认 pin `1f5f8cf8...` exit 11；override 后 12/12 PASS |
| `verify-external.log` / `verify-external-newpkg.log` | 旧容器 `7b4d0615...` 与新容器 `e0c488a9...` 均 PASS |
| `build-rc-workroot.log` / `clean-clone-native-build.txt` / `workspace-state-at-audit.txt` | 工作根与干净树状态 |
| `inspect-wsl.log` / `inspect-clone.log` / `prepare-replay-root*.log` / `probe-*.sh` | 结构与 clone 检查 |
| `external-accept/` | 外部接受脚本/输出（如存在） |
| `stage.json.not_run` | **NOT_RUN**：`build-rc.ps1` 本 lane 未执行（写入共享 work root，与并行 lane 冲突；引用 sibling 的 `evidence/songcore-172/build/rc/20261030-203045/summary.json`） |

- **关键失败**：`build_linux_usable_at_H=NO`；`linux_package_entry=NO`（main 无 Linux 打包入口）；artifact provenance 无绑定；`run-csuite.ps1:52`/`run-binding-gate.ps1:49` pin 过期；`build-rc.ps1:344` 指向不存在的 `verify-build.ps1`；`verify-external.ps1` 默认指向归档 worktree。

### 4.3 `p0-governance/` — 治理审计

- **verdict**：`PASS`（含两个 Owner-blocked 项与一个环境 NOT_RUN）
- **用途**：v0.1.0 tag/release/asset 的不可变性、GitHub 线上状态、#326/#327/#328/#330/#331/#332 的落地、D1/D3 的既定记录。

| 文件 | 用途 |
|---|---|
| `AUDIT.md` | 治理审计叙述 |
| `git-forensics.log` | tag object / peeled commit / tag 未移动 |
| `release-summary.json` | v0.1.0 release/asset 摘要（asset `018e0cbd...`） |
| `rest-release-v0.1.0.json` / `rest-auth-release-v0.1.0.json` | 匿名与认证的 release API 载荷（字节相同） |
| `rest-auth-releases-list.json` / `rest-auth-releases-page2.json` / `rest-releases-list.json` | releases 清单（2 个 release，无 draft，page2 空） |
| `rest-matching-refs-songcore-v.json` / `rest-auth-matching-refs.json` / `rest-tags-page1.json` | 远端 songcore tag 只有 1 个，object sha 与本地一致 |
| `rest-issue-330.json` / `rest-330-is-pr.json` | #330 是 issue 而非 PR |
| `rest-pr-326/327/328/331/332.json` | 各 PR 的 merge commit 与本地一致 |
| `rest-auth-repo.json` / `rest-auth-user.json` | 认证身份 `jnhu76`，repo admin |
| `contract-330-sections.txt` / `contract-extract-window.txt` | 合约 §16 D1/D2/D3 的原文窗口 |
| `pr-merge-check.log` | ancestry 检查 |
| `audit_raw.log` / `auth-read.log` / `auth-summary.log` | REST 读取与解析日志 |
| `*.sh` / `*.py` | 取证脚本（`git_forensics.sh`、`wsl_release_audit.sh`、`auth_read.sh`、`parse_rest.py`、`parse_auth.py`、`make_stage.py`） |
| `stage.json.not_run` | **NOT_RUN**：`gh` CLI route（WSL 无网络）；`lefthook pre-push`（本 lane 无 tracked 变更） |

### 4.4 `p0-evidence-spec/` — 证据规范

- **verdict**：`FIX_REQUIRED`（规范本身 READY；round 有阻塞）
- **用途**：定义 G4 证据目录、每 lane 最小文件集、回执约定、NOT_RUN 措辞、禁止提交的产物、目录漂移与卫生问题。

| 文件 | 用途 |
|---|---|
| `README.md` | 证据规范全文（§2 布局、§6 禁止类、§7 三种报告类） |
| `stage.json` | 本 lane 的机器回执；`not_run` 列出 7 项 |

- **记录到的阻塞**：g4 下 4 个 >100 KiB 未跟踪文件（实际更多，见 §6）；目录集漂移；`evidence/songcore-172/build/rc/20261030-203045/` 写在 g4 之外；`registry.yml` ARCH-008 证据缺失；`docs/release/pr-a-body.md` 未路由；`evidence/README.md:3` 的 “external evidence” 表述与 songcore-172 自身证据不符。

### 4.5 `d1-out-handle/` — D1 探针

- **用途**：验证 `song_open` 失败时 `*out_handle` 是否真的 “untouched”。
- **结论**：header 文本与实现不一致；linux 执行 12/12 MATCH；windows 仅反汇编。

| 文件 | 用途 |
|---|---|
| `d1_probe.c` | 探针源码（sentinel / prior-handle / 各 status 路径） |
| `build.sh` / `build-wsl.sh` / `win-build.sh` / `build-driver.log` / `build-wsl.log` / `win-build-driver.log` / `win-build-wsl.log` | linux/windows 构建脚本与日志 |
| `run.sh` / `run.log` | linux 运行与结果 |
| `disasm.sh` / `disasm-linux.txt` / `disasm-windows.txt` | 两个 target 的 `song_open` 反汇编（NULL store 位置） |
| `collect.sh` | 证据收集 |
| `stage.json.not_run` | **NOT_RUN**：windows 运行时（仅反汇编）；Rust workspace tests；macOS/其他 target |

### 4.6 `d3-readpcm-zero/` — D3 探针

- **用途**：验证 `song_read_pcm` 的错误路径是否满足 `*out_frames_produced == 0`。
- **结论**：linux 执行 `VIOLATIONS 3`；windows 未执行。

| 文件 | 用途 |
|---|---|
| `readpcm_zero_probe.c` | 探针源码（poison `UINT64_MAX` + 四条调用 + 对照） |
| `build_and_probe.sh` / `force_rebuild_and_probe.sh` / `clean_rebuild_and_probe.sh` | 构建/强制重建/探针脚本 |
| `capture_evidence.sh` | 证据收集 |
| `logs/run-force.log` | 四条调用的 status/produced 实测 |
| `logs/song_read_pcm-force.dis` | 反汇编（0x35a02/0x35a19/0x35a2b/0x35a32 等） |
| `stage.json.not_run` | **NOT_RUN**：windows 运行时；修复后行为（未授权改代码） |

### 4.7 `build-linux/` — Linux G0 构建

- **verdict**：`PASS`；**L1 NOT_RUN**
- **用途**：在独立根构建 linux static + shared，做 member census、ABI audit、NEEDED audit、flag witness、与 #329 对照。

| 文件/组 | 用途 |
|---|---|
| `stage.json` / `summary.json` | 机器回执与摘要（artifact sha/bytes） |
| `drivers/` | 构建/校验驱动脚本 |
| `logs/` | `xmake-config.log`、`xmake-build*.log`、`compile-commands/`、`evidence-selfcheck.txt`、`invocations.tsv`、`facts.env` 等 |
| `verify/`（若在 `logs/verify` 或同级） | `artifacts-sha256.txt`、`objdump-f-foreign.txt`、`abi-diff-a.txt`、`abi-diff-so.txt`、`needed-libs.txt`、`compile-commands-witness.txt` |
| `compare-329/` | `ar-cmp-new-vs-329.json`、`version-bytes.txt`、`sha256-four.txt` |
| `stage.json.not_run` | **NOT_RUN**：L1 fresh closure；L2/G2 runtime（本 lane 不做）；L3 cross-root；windows lane；packaging/release |

### 4.8 `build-windows/` — Windows G0 构建

- **verdict**：`PASS`；**L1 NOT_RUN**
- **用途**：`ops/build-rc.ps1` 构建 windows static；census、15/15 ABI、BCrypt imports、closure manifest、与 #329 对照。

| 文件 | 用途 |
|---|---|
| `build-rc-summary.json` / `build-rc.stdout.json` | 构建摘要（与 `evidence/songcore-172/build/rc/20261030-203045/summary.json` 字节相同） |
| `verify-artifact.log` / `verify-artifact.sh` | ABI 15/15、BCrypt imports、artifact sha |
| `census.log` / `census.sh` | 280/280 PE 成员普查 |
| `objdump-f.txt` / `nm.txt` | `objdump -f` / `nm` 原始输出（`nm.txt` 347,635 B，>100 KiB） |
| `compare-329.log` / `compare-329b.log` / `compare-329.sh` / `compare-329b.sh` | 与 #329 windows static 的 occurrence 对比 |
| `version-provenance.log` / `version-members.log` / `version-*.sh` | `FFMPEG_VERSION` 差异根因 |
| `probe-closure.sh` / `probe-prior.sh` / `normalize-summary.*` / `ar_member_cmp.py` | 辅助检查 |
| `libsongcore.a.wsl-copy` | artifact 副本（3,309,294 B；>100 KiB，不应提交） |
| `stage.json.not_run` | **NOT_RUN**：second-root byte identity（T1–T6）；Windows runtime execution（本 lane 只验 archive shape）；L1 re-earn |

### 4.9 `consume-linux/` — Linux 消费者

- **verdict**：`PASS`（有覆盖缺口）
- **用途**：只针对解包后的预检包，做 static/shared 消费、4 个 corpus fixture、seek 25/50/75%、EOF、malformed、close/reopen。

| 文件/组 | 用途 |
|---|---|
| `stage.json` / `evidence-sha256.txt` | 回执与 61 个证据文件的哈希 |
| `out/comparison.json` | 每 fixture 的 status/PCM/seek 对照 |
| `out/static/` / `out/shared/` | 消费者二进制与运行输出 |
| `logs/stage02-build.log` / `stage03-static-run.log` / `stage04-shared-run.log` / `stage07-full-rerun.log` / `stage08-inventory.log` / `stage09-selfcheck.log` | 构建/运行/自检 |
| `logs/stage02c-op-probe.log` / `stage02d-flac-seek-anomaly.log` / `stage02e-flac-boundary.log` | FLAC post-EOF 末帧 seek 观察（status 110，非 gate） |
| `src/consume_corpus.c` / `src/robustness_probe.c` | 消费者源码 |
| `fixtures-bad/` | 空文件、垃圾、截断 fixture |
| `scripts/` | 驱动脚本 |
| `stage.json.not_run` | **NOT_RUN**：artwork payload（corpus artwork=0）；step 13 close 后 view invalidation；truncated-MP3 typed-error 分支；`song_select_stream`/多流；Windows 包运行；Rust/JNI/Apple binding parity；bench/perf；install-prefix/ldconfig；与 v0.1.0 包对比 |

### 4.10 `lane-c-windows-pe/` — Windows PE 消费者

- **verdict**：`PASS`
- **用途**：用预检包 `include/` + `lib/libsongcore.a` + `-lbcrypt` 链接 PE，在真实 Windows 主机执行 4 个 fixture，PCM 对照 reference。

| 文件/组 | 用途 |
|---|---|
| `stage.json` | 回执（source、输入、构建、运行、provenance） |
| `src/pe-consumer.c` | 消费者源码 |
| `build/pe-consumer.exe` | 最终 PE（`3ec3c672...`，2,876,435 B；>100 KiB） |
| `build/logs/build.log` / `link-wo-bcrypt.log` / `stage.log` | 构建、负控（无 `-lbcrypt` 失败）、staging |
| `build/sha256-manifest.txt` | 构建产物哈希 |
| `run/host.txt` / `run-all-windows.log` / `*-stdout.txt` / `cmd-*.stdout.txt` | 真实 Windows 主机运行日志 |
| `raw/reference-comparison.json` | PCM 对照结果 |
| `raw/pre-fix/` / `run/pre-fix-*` / `run/host.first-attempt.txt` / `run/host.runner-capture.txt` | 探测缺陷修正记录（保留，不当作最终证据） |
| `stage.json.not_run` | **NOT_RUN**：shared/DLL consumer（包内无 shared form）；MSVC；FFmpeg 熵路径（0 calls）；multi-stream；lifecycle negatives；32-bit/ARM64；audible/device；Defender/soak |

### 4.11 `lane-e-runtime-deps/` — 运行依赖

- **verdict**：`PASS`（测量）
- **用途**：NEEDED/ldd、15 exports、无 SONAME/RPATH、glibc 版本集与 floor、静态链接矩阵、旧发行版容器实测。

| 文件/组 | 用途 |
|---|---|
| `stage.json` | 回执（47 组 argv/exit code） |
| `raw/readelf-dynamic-so.txt` / `needed-libs.txt` / `ldd-isolated-so.txt` / `nm-D-defined-so.txt` / `nm-D-undefined-so.txt` | 动态依赖与导出 |
| `raw/objdump-T-so.txt` / `readelf-version-info-so.txt` / `glibc-versions.txt` / `glibc-highest.txt` / `non-glibc-explicit-check.txt` | glibc 版本集与 floor |
| `raw/container-runs-exitcodes.txt` / `raw/container-{focal,bookworm,noble,alpine}-*.txt` | 旧发行版容器运行 |
| `raw/probe-elision-proof.txt` / `raw/post-measurement-state.txt` | 探针/状态说明 |
| `raw/evidence-manifest.sha256.txt` | 127 个证据文件的哈希清单 |
| `*.sh` / `gen-stage.py` | 驱动脚本 |
| `stage.json.not_run` | **NOT_RUN**：旧 glibc 上的**链接**行为（容器只跑了运行）；其余见 stage |

> 该 lane 的 `PASS` 只表示“所有请求的测量都执行了”，不表示可移植性或支持范围通过。

### 4.12 `lane-d/` — 许可

- **verdict**：`FIX_REQUIRED`
- **用途**：O1–O4 / B1–B4 的独立复核：COPYING 文本与 pinned zip、notices 模板重渲染、README/许可证文本、closure provenance、tag→commit 重查。

| 文件/组 | 用途 |
|---|---|
| `stage.json` | 回执（输入 sha、argv、exit codes） |
| `logs/01`–`logs/10` | 各检查的原始日志（文本提取、manifest、notices、README、tag recheck） |
| `extract-license-sources.sh` / `check-notices-template.sh` / `render-v-version.sh` / `diff-notices.sh` | 许可/notices 检查脚本 |
| `audit-archives.sh` / `verify-package-integrity.sh` / `tree-identity.sh` | 包完整性与树身份 |
| `closure-provenance.sh` / `find-closure.sh` | closure manifest 定位 |
| `tag-commit-recheck.sh` | `git ls-remote` tag→commit 重查（**NOT_RUN**：WSL 无网络，exit 128） |
| `run-all.sh` / `tmp/` | 驱动与临时目录 |
| `stage.json.not_run` / `logs/10` | **NOT_RUN**：FFmpeg tag→commit recheck（无网络） |

### 4.13 `package/` — 预检打包

- **verdict**：`FIX_REQUIRED`
- **用途**：用 `ops/package-rc.ps1`（windows）与 PR 分支的 `ops/package-linux.sh`（linux）生成 `0.0.0-preflight` 容器；独立重走 V1–V7、O/B、容器安全检查。

| 路径 | 用途 |
|---|---|
| `win/songcore-v0.0.0-preflight-windows-x86_64.zip` | Windows 预检容器（`5d92b21d...`，1,449,803 B） |
| `win/extract/…` | 解包树（14 files） |
| `win/stage/…` | 打包器 staged 树 |
| `win/package-summary.json` / `win/v-checks.json` / `win/SHA256SUMS` / `win/stage-audit.log` / `win/stage-audit.sh` | 摘要、V 检查、外部校验和、staged 审计 |
| `linux/songcore-v0.0.0-preflight-linux-x86_64.tar.gz` | Linux 预检容器（`bfe780c3...`，2,575,536 B） |
| `linux/extract/…` | 解包树（15 files） |
| `linux/package-summary.json` / `linux/v-checks.json` / `linux/SHA256SUMS` | 摘要、V 检查、外部校验和 |
| `stage.json.not_run` | **NOT_RUN**：包内 consumer runtime/device smoke；Windows 主机上的 mingw 链接 smoke；任何 rebuild；tag/Release/publication |

- **关键失败**：§2 version identity（两个打包器无等式检查）；Windows artifact provenance（`package-rc` 可包陈旧 artifact）；linux 打包器未合并 main；`artifact_shared` 类型漂移（linux 字符串 vs windows boolean）。

### 4.14 `package-laneB/` — Linux 打包器 lane

- **用途**：在 PR 分支上运行 `ops/package-linux.sh`，记录 V 检查、负向 guard、两次确定性运行。

| 文件 | 用途 |
|---|---|
| `stage.json` | 回执 |
| `laneB-verify.json` / `laneB-verify.log` | V-CHECKS 与容器校验 |
| `linux-package.log` / `win-package.stdout.log` / `win-package.stderr.log` | 打包输出 |
| `verify-run.log` / `final-crosschecks.log` / `receipts.log` / `extract-stages.log` | 交叉校验与 receipt/stage 提取 |
| `run-linux-package.sh` / `collect-receipts.sh` / `extract-stages.sh` / `final-crosschecks.sh` / `verify.py` / `write-stage.py` | 驱动/解析脚本 |
| `package-linux.sh.branch-sha` | 被使用脚本的分支身份（`8d36f443...`） |

### 4.15 `release-policy/` — 发布策略

- **verdict**：`FIX_REQUIRED`
- **用途**：E01–E18、包安全 T1–T5、publish-policy 探针、docs-verify 两条路线（literal route 与 gate body）。

| 文件 | 用途 |
|---|---|
| `run-e01-e18.sh` / `e01-e18.log` / `e01-e18.exit.txt` / `release-controls.json` | E01–E18 真实运行（exit 0；69 executions / 44 labels） |
| `check_package_security.py` / `run-package-audit.sh` / `package-audit-windows-zip.{json,log}` / `package-audit-linux-targz.{json,log}` | T1–T5 包安全检查 |
| `run-publish-policy-probes.sh` / `publish-policy-probes.log` | songcore tag / existing release / dispatch / 新 tag 的策略探针 |
| `run-docs-verify.sh` / `docs-verify-literal-route.log` | `tools\docs-verify.cmd` 缺失（exit 1） |
| `docs-verify-verify.mjs.log` | `verify.mjs` PASS（0/0） |
| `docs-verify-vitepress-build.log` / `-retry.log` / `-retry2-nosandbox.log` | vitepress build 三次 FAIL（esbuild remove Access denied） |
| `docs-verify.meta.txt` / `package-audit.meta.txt` | 元信息 |
| `stage.json.not_run` | **NOT_RUN**：docs-verify 未变绿；实时 GitHub release/tag 状态（WSL 无网络）；CI 路线 |

### 4.16 `tools-linux-substrate/` — 工具 PR lane

- **verdict**：`ENVIRONMENT_BLOCKED`（PR 未打开；分支已推送）
- **用途**：`ops/build-linux.sh` + `ops/package-linux.sh` 的开发者分支证据：gates、负向 guard、真实构建/打包、PR body。

| 路径 | 用途 |
|---|---|
| `PR-BODY.md` | 完整 PR body（10 KB；未推送/未开 PR） |
| `stage.json` | 回执（39 个证据文件哈希） |
| `build/` | 真实构建证据（linux static/shared、member census、ABI、NEEDED） |
| `package/` | 真实打包证据（`3dc50510...`，15 files/14/13，V1–V7 + O/B） |
| `gates/` | lefthook 各 gate 的运行记录（docs-verify NOT_RUN） |
| `guards/` | 12 个负向 guard 的退出码 |
| `stage.json.not_run` | **NOT_RUN**：GitHub PR creation；docs-verify；runtime/device smoke；two-root repro；fresh FFmpeg closure；CI |

---

## 5. 全局 NOT_RUN 清单（原样保留）

> 这些是“没有执行”，不是“通过”。任何汇总都不得把它们写成 PASS。

### 5.1 构建/复现

```text
L1 fresh-clone + fresh-closure re-earn            NOT_RUN (build-linux, build-windows)
L2 semantic-ABI two-root                          NOT_RUN this round
L3 byte identity across roots this round          NOT_RUN this round
windows second-root T1–T6                         NOT_RUN
fresh FFmpeg download / ffmpeg_import             NOT_RUN (no network; reused pinned closure)
two-root reproducibility rebuild                  NOT_RUN (package manifests NOT_ESTABLISHED)
```

### 5.2 运行时/消费者

```text
artwork payload / role / mime / dimensions        NOT_RUN (all fixtures artwork=0)
post-close view invalidation assertion            NOT_RUN
song_select_stream / multi-stream                 NOT_RUN (single-audio-stream fixtures)
truncated-MP3 typed-error branch                  NOT_RUN (drained to SONG_EOF)
Windows shared/DLL consumer                       NOT_RUN (no shared form shipped)
MSVC / native-Windows-toolchain                   NOT_RUN
FFmpeg av_get_random_seed entropy path            NOT_RUN (instrumented, 0 calls)
32-bit / ARM64 Windows                            NOT_RUN
lifecycle negatives / ABI-invalid matrix in PE    NOT_RUN
audible/device output                             NOT_RUN
Rust/JNI/Apple binding parity                     NOT_RUN
bench/startup/latency performance                 NOT_RUN
install-prefix/ldconfig run                       NOT_RUN
old-glibc LINK behavior                           NOT_RUN (containers ran, but only runtime)
```

### 5.3 许可/文档/发布

```text
FFmpeg tag->commit recheck                        NOT_RUN (WSL no network; git ls-remote exit 128)
docs-verify vitepress build (CI route)            NOT_RUN (host build FAIL)
live GitHub release/tag state read                NOT_RUN (WSL no network)
gh CLI route                                      NOT_RUN (WSL no network)
lefthook pre-push on this worktree                NOT_RUN (no tracked changes)
CI coverage of ops/**                             NOT_RUN (no job invokes ops/)
```

### 5.4 本决策包自身

```text
任何构建/链接/运行时/打包/发布检查                NOT_RUN by this packet
任何 G0–G4 / V1–V7 / O–B / E01–E18 重跑          NOT_RUN by this packet
```

---

## 6. 证据卫生与当前状态

- 当前 `g4/` 下约 **760 个文件**；本包复算的 `find -type f -size +100k` 命中 20+ 个，包括：
  - `build-linux` / `build-windows` 的 artifact 副本（`libsongcore.a.wsl-copy` 3,309,294 B）
  - `package/{win,linux}/extract/...` 中的 `.a` / `.so` / `.zip` / `.tar.gz`
  - `lane-c-windows-pe/build/pe-consumer.exe`（2,876,435 B）
  - `p0-entries/.../csuite_consumer.exe`（2,498,824 B）
  - `build-linux/logs/compile-commands/compile_commands.json`（409,618 B）
  - `lane-e-runtime-deps/raw/archive-undefined-all.txt`（388,015 B）
- 按 `p0-evidence-spec` §6，**不应提交**：shipped artifacts（`.a/.so/.dll/.zip/.tar.gz`）、完整 build tree、object/closure 缓存、raw `compile_commands.json`、>256 KiB 日志、token/node_modules/site output、跨 lane 重复日志。当前树明显违反该规则；提交前必须由 owning lane 删除或移出并记录 sha256。
- 根 `RECEIPTS.sha256` **尚未生成**；应由 reconcile lane 在清理后最后生成，覆盖 `./` 相对路径、排序、只覆盖清理后的树。（归档轮已按同一格式生成小写 `receipts.sha256`，见 §8.1；`p0-evidence-spec` §5 指定的 reconcile lane 仍未执行，本文件不冒充 reconcile lane。）
- 既有 tracked 大文件先例（`evidence/songcore-172/linux/verify/package/songcore-v0.0.0-s3-verify-linux-x86_64.tar.gz` blob `1642c584`；`evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip` blob `56d9d883`）与本规则冲突；本包只记录，不修改历史。

---

## 7. 本目录**不能**证明什么

- 不能证明 `songcore-v*` 已经可以发布：`NEW_RC`、`TAG`、`GITHUB_RELEASE` 全部为 NOT_CREATED。
- 不能证明 Windows DLL / shared 受支持：本轮 Windows 只有 static。
- 不能证明任何 Ubuntu/Debian/Alpine 版本受支持：只有构建主机跑通；旧发行版全部在 loader 失败。
- 不能证明 L1/L2/L3：本轮没有重新获得；#329 的 L1–L3 是 campaign evidence，不是 release authority。
- 不能证明 docs-verify 通过：`verify.mjs` PASS，但 `vitepress build` 在本机 FAIL，CI 路线 NOT_RUN。
- 不能证明 SongCore 有自动发布器：没有；`check_release.py` 按 R1 拒绝 `songcore-v*`。
- 不能替 Owner 做 Q1/Q3/Q4/Q5/D1/D3 的决定：决策包只列方案与后果。

---

## 8. 归档形态（2026-10-30 归档 PR）：提交了什么、只记哈希的是什么、第三方怎么复跑

> 本节只描述**归档动作**（哪些文件进仓库、用什么哈希固定、怎么复跑），不改变 §4/§5 的任何 verdict，也不新增门禁结论。`PREFLIGHT_VERDICT=ENVIRONMENT_BLOCKED` 保持原样。

### 8.1 归档回执

```text
source_sha      69ba1ce05a2772f5aa077c6ece3365c40f6e0500（BASE_SHA = CURRENT_MAIN_SHA）
archive branch  evidence/songcore-172-g4-preflight（从 main 69ba1ce 开）
submitted       665 个 lane 文件（全部 <= 200,000 B 的文本/脚本/小型 witness）
                + excluded-files.sha256（未提交文件的哈希清单）
                + receipts.sha256（已提交文件的哈希清单，不含自身）
receipts.sha256 666 行，`./` 相对路径，按路径排序，sha256sum 格式
excluded        104 个文件 / 45,394,019 B，全部登记在 excluded-files.sha256
```

两份清单互不覆盖、互补：

```text
receipts.sha256        覆盖本目录下每一个被提交的文件（receipts.sha256 自身除外）
excluded-files.sha256  覆盖每一个因 8.2 规则未提交的原始文件，可用于在产出机/重建后逐条核对
```

哈希格式沿用仓库先例 `evidence/native-tier2-296/raw/closeout3/receipts.sha256`（`sha256sum` 输出、`./` 相对路径、排序）。**文件名小写**是本归档任务的指定形式；`p0-evidence-spec` §5 的规范名是根 `RECEIPTS.sha256`，由 reconcile lane 在树清理后生成——本归档不冒充该 lane，也不修改 §5。

校验命令（在 g4 根目录执行）：

```bash
sha256sum -c receipts.sha256        # 期望：全部 OK，0 FAILED
```

### 8.2 未提交文件：规则、规模与重建位置

判定是**按路径确定性**的，不按“看起来像不像证据”。下列五类未提交（完整清单 + 每文件 sha256 见 `excluded-files.sha256`）：

| 类 | 规则 | 文件数 | 字节 | 重建/取得方式 |
|---|---|---|---|---|
| shipped artifact 后缀 | `.a` `.so` `.dll` `.exe` `.zip` `.tar.gz` `.o` `.obj` `.pyc` | 16 | 39,620,005 | 见下方逐项 |
| build-windows artifact 副本 | `build-windows/libsongcore.a.wsl-copy` | 1 | 3,309,294 | sha256 `89d0428d…65e97` 与 `build-windows/stage.json` 记录的构建产物同一身份；重建见 8.3 的 build-windows argv |
| > 200,000 B | 单文件超过 200 KB 一律不收 | 3 | 1,145,268 | 见下方逐项 |
| package staging/extract 树 | `package/{win,linux}/extract/**`、`package/win/stage/**`、`p0-entries/package-rc-out/*/stage/**`、`p0-entries/csuite-results-override/*/media/**`，仅保留下表文本 witness | 83 | 1,309,778 | 重新执行打包器后解包；容器身份见下 |
| Python 缓存 | `build-linux/drivers/__pycache__/*.pyc` | 1 | 9,674 | 由 `python3` 重新导入驱动脚本自动生成，非证据 |

**容器/产物身份（未提交，可用 sha256 交叉核对既有归档）**：

| 未提交产物 | 字节 | sha256 |
|---|---|---|
| `package/win/songcore-v0.0.0-preflight-windows-x86_64.zip` | 1,449,803 | `5d92b21d7092b8d84d1db955ddbd38d3dc8b66f9f0ad29de35593f496e6397d3` |
| `package/linux/songcore-v0.0.0-preflight-linux-x86_64.tar.gz` | 2,575,536 | `bfe780c3bf65b43f8007f3cd48fe3cef39de8d3a17af81f9767280aa7a43d790` |
| `p0-entries/package-rc-out/default/*.zip`（RC 命名） | 1,449,651 | `e0c488a931d6f7bb0003d918eeee92f2202523e18319e617f26d264ae547084b` |
| `p0-entries/package-rc-out/stale-artifact/*.zip` | 1,449,931 | `c3983bfb0215d771b6343fed576d649a975662b1adcf90b85c52ed50e9ee1604` |
| `p0-entries/package-rc-out/ver-0.1.0-rc.1/*.zip` | 1,449,647 | `7e843c9a68477bd575298993eca1b961b027eceba88a3793c111423d436cb264` |
| windows static archive（各 stage 树内同一身份） | 3,309,294 | `89d0428db2152da60f233290c082ecfb33b5590628ab9c40b97e8354afc65e97` |
| stale-artifact 树内 windows static archive | 3,309,982 | `408d7d6ea4c9d1707283104366725c3deda47d5a4cebee5102c170fdf86d404b` |
| linux static archive | 4,066,980 | `0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308` |
| linux shared archive | 2,288,536 | `085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98` |
| `lane-c-windows-pe/build/pe-consumer.exe` | 2,876,435 | `3ec3c672bd8d2f0d0d92604ca1a1d07a76b479c2c2e306f6b3ef09eca4cbc60d` |
| `reviewer-b/reviewer-b-all15.exe` | 2,866,390 | `07f363d9d081255f7df1f73910b6af4a52b46f03de8e38509fbc340cd72b80d8` |
| `p0-entries/.../csuite_consumer.exe` | 2,498,824 | `0e4ae43ec57942e859c615cfecbcfade5fc7befbb024747c6091459d1964b31e` |
| `p0-entries/.../layout_probe.exe` | 101,114 | `26b1ff821f7194ec18477b44046820ea9f8e1cf4d17c7e6c055d8b12f78c3b1c` |

重建路线（每条都对应 §8.3 中真实执行过的 argv，不是推测）：

```text
windows 预检包     ops/package-rc.ps1（argv 见 package-laneB/stage.json 第 1 条与 p0-entries/stage.json）
linux 预检包       ops/package-linux.sh @ 分支 blob 8d36f443…（argv 见 package-laneB/stage.json）
RC 命名包          ops/package-rc.ps1 -Version <caller-supplied>（argv 见 p0-entries/stage.json）
windows static     ops/build-rc.ps1（argv 见 build-windows/stage.json；closure manifest target=windows-x86_64）
linux static/shared xmake f -p linux -a x86_64 -m release -y + xmake build（argv 见 build-linux/stage.json）
consumer 可执行   各 lane 自己的 link argv（consume-linux/stage.json、lane-c-windows-pe/stage.json、reviewer-b/stage.json）
```

**保留的 package 文本 witness**（未随 staging 树整体丢弃，仍可从已提交文件重做 V1–V7 的文本核对）：
`package/win/{metadata/manifest.json,metadata/checksums.txt,metadata/symbols.txt,stage/audit-symbols.txt,stage/audit-undefined.txt}`、
`package/linux/.../metadata/{manifest.json,checksums.txt,symbols.txt}`，以及 `package/{win,linux}/{package-summary.json,v-checks.json,SHA256SUMS}`。

**3 个超 200 KB 文件的取得方式**：

| 路径 | 字节 | sha256 | 如何重建 |
|---|---|---|---|
| `build-linux/logs/compile-commands/compile_commands.json` | 409,618 | `5e271e8714135d20e45033c6cd8338eedef1d14e713431690a7eaee7aa914114` | `xmake project -k compile_commands`（`build-linux/drivers/g4-s3-verify.sh` 与 `logs/xmake-compile-commands*.log` 记录同一命令）；派生 witness `logs/verify/compile-commands-witness.txt` 已提交 |
| `lane-e-runtime-deps/raw/archive-undefined-all.txt` | 388,015 | `3a2e1109f1a7d1b3a9c8d352764f2de8128fff4509c4c61228493197b9cbb998` | 对 `libsongcore.a` 的逐成员 undefined 全量（`nm -u` / `objdump`）；派生计数 `raw/archive-undefined-unique-count.txt`、`raw/archive-undefined-names.txt` 已提交 |
| `build-windows/nm.txt` | 347,635 | `7420f5c9cc4c4b490f3aef5a8623003157a62bb18adeb9531e2043118565943e` | `x86_64-w64-mingw32-nm` 全符号转储；15 符号 ABI 集与 undefined 普查在 `build-windows/verify-artifact.log`、`build-windows/stage.json` 中已记录 |

### 8.3 每个 lane 的 argv 位置

每个 lane 的**可执行 argv 与退出码**就在该 lane 的 `stage.json` 里（字段 `argv[]` / `exit_codes[]`），配套驱动脚本是被执行命令的字节级副本。下表给出位置与规模；“NOT_RUN 条数”是 `stage.json.not_run[]` 的长度，内容见 §4 与 §5。

| lane 目录 | `stage.json` argv/exit_codes | NOT_RUN 条数 | verdict（原样） | 驱动脚本 |
|---|---|---|---|---|
| `build-linux/` | 12 / 12 | 6 | PASS | `drivers/g4-*.sh`、`drivers/g4-*.py`、`drivers/probe-*.sh` |
| `build-windows/` | 13 / 13 | 6 | PASS（pre-check scope only） | `census.sh`、`verify-artifact.sh`、`compare-329*.sh`、`version-*.sh`、`ar_member_cmp.py` |
| `consume-linux/` | 42 / 42 | 3 | PASS | `scripts/01`–`09`、`src/*.c`、`05-compare.py` |
| `d1-out-handle/` | 12 / 10 | 4 | D1 CONFIRMED（header 文本缺陷 + 实现侧防御） | `build.sh`、`win-build.sh`、`run.sh`、`disasm.sh`、`collect.sh` |
| `d3-readpcm-zero/` | 14 / 14 | 5 | D3 CONFIRMED | `build_and_probe.sh`、`force_rebuild_and_probe.sh`、`clean_rebuild_and_probe.sh` |
| `lane-c-windows-pe/` | 30 / 30 | 8 | PASS | `build/build.sh`、`build/run-all-windows.sh`、`build/consolidate.sh`、`build/compare_reference.py` |
| `lane-d/` | 19 / 19 | 3 | FIX_REQUIRED | `*.sh`（licenses / notices / integrity / provenance） |
| `lane-e-runtime-deps/` | 47 / 47 | 5 | PASS（measurement only） | `run-elf.sh`、`run-consumers.sh`、`run-static-matrix.sh`、`run-containers.sh`、`gen-stage.py` |
| `p0-contract/` | 9 / 9 | 8 | OWNER_BLOCKED | `run-*.sh` |
| `p0-entries/` | 16 / 16 | 4 | FIX_REQUIRED | `inspect-*.sh`、`measure-build-linux-*.sh`、`run-build-linux-real.sh`、`patch-stage*.py` |
| `p0-evidence-spec/` | 41 / 41 | 7 | READY（规范本身） | 无（只读审计） |
| `p0-governance/` | 28 / 35 | 4 | PASS | `git_forensics.sh`、`auth_read.sh`、`parse_rest.py`、`parse_auth.py`、`make_stage.py` |
| `package/` | **无 `stage.json`**（见下） | – | FIX_REQUIRED | `win/stage-audit.sh` |
| `package-laneB/` | 8 / 8 | 5 | FIX_REQUIRED | `run-linux-package.sh`、`collect-receipts.sh`、`extract-stages.sh`、`final-crosschecks.sh`、`verify.py` |
| `release-policy/` | 17 / 5 | 5 | FIX_REQUIRED | `run-e01-e18.sh`、`run-package-audit.sh`、`run-publish-policy-probes.sh`、`run-docs-verify.sh`、`check_package_security.py` |
| `reviewer-b/` | 19 / 20 | 6 | REVISE | `reviewer-b-link.sh`、`reviewer-b-probe.sh`、`reviewer-b-winlink.sh` |
| `tools-linux-substrate/` | 23 / 23 | 6 | PASS（两条入口）；PR 未打开 | 证据在 `build/`、`package/`、`gates/`、`guards/` |

**`package/` 没有 `stage.json`**：该 lane 落盘时只写了 `package-summary.json` / `v-checks.json` / `SHA256SUMS` / `stage-audit.log`，没有按 `p0-evidence-spec` §3 的最小文件集写机器回执。这是本轮已存在的缺口；归档不改写其它 lane 的证据、也不补造回执，它执行过的命令行因此只能从 `package-laneB/stage.json`、`package/win/stage-audit.sh` 与 `package/*/package-summary.json` 的 `script` 字段复原。

### 8.4 第三方复跑步骤

前提（与本轮实测环境一致，缺任何一项就按 §5 记 NOT_RUN，不得用“应该通过”代替）：

```text
WSL Ubuntu 24.04 + gcc + x86_64-w64-mingw32-gcc + objdump/ar/readelf + python3
xmake 在 ~/.local/bin（export PATH=$HOME/.local/bin:$PATH）
windows 侧真实主机（lane-c-windows-pe 的 PE 运行证据只能在那里重做）
pinned FFmpeg：ffmpeg-n9.0.1.zip sha256 d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746（禁止重新下载）
可复用的已导入 closure：windows-x86_64（recipe_sha256 3ba0a9e6…8492d）、linux-x86_64（recipe_sha256 79c307e7…db6d）
```

```text
1. git fetch / checkout 本归档 commit；git rev-parse HEAD 与 PR 记录的 head 一致
2. cd evidence/songcore-172/g4 && sha256sum -c receipts.sha256      # 期望全部 OK
3. 逐个 lane 读 stage.json：argv[i] / exit_codes[i] / evidence_paths[i] 是同一条命令的三元组
4. 复跑时先把 argv 里的产出机绝对路径（/home/hoo/g4-172/...、C:\Users\fred1\...）替换成自己的
   work root；这些路径是 provenance，不是当前文件系统上的可用路径（决策包 §0.4 已实测：
   /home/hoo/g4-172/linux-root/src 与 receipts 目录已不存在）
5. 构建 lane 从复用 closure 起步就只能覆盖 pre-check 结论；L1/L2/L3 仍是 NOT_RUN（§5.1）
6. release-policy lane：python3 tools/test_release.py（E01–E18，44 个 label / 69 次执行）
7. 本归档轮自己的本地门禁见 §8.5
```

### 8.5 归档轮执行的本地门禁（真实结果口径）

归档 commit 之后在 WSL 工作树执行仓库规定的完整本地门禁：

```bash
lefthook run pre-push --all-files
```

结果口径（与 AGENTS.md 的 NOT_RUN 纪律一致）：

```text
PASS     该 gate 在归档 commit 上真实执行并返回 0
FAIL     真实执行并返回非 0（写明是哪一步、缺什么前提）
NOT_RUN  未执行；必须写明缺的前提
piped 的 pre-push 在第一个红 gate 处停止，之后每个 gate 对该次运行都是 NOT_RUN
```

本次归档轮的真实逐 gate 结果写在 **PR body** 里（PR 是本归档的交付面），不在本文件里二次转述，避免两处数字漂移。已知宿主限制（不是本轮引入）：

```text
docs-verify   WSL VM 无 node/npm 且无网络做 npm ci；Windows 侧文档路线
              tools/docs-verify.cmd 在 69ba1ce 不存在（release-policy/stage.json not_run 已记录）
release-policy 需要 python3/git/ar/objdump/mingw/xmake/rustc；WSL 侧齐备
```

### 8.6 权威边界（本归档不放宽任何发布权威）

- 本目录是 `evidence/README.md:3` 定义的非权威证据树。本归档 PR **不放宽**任何发布权威：`docs/release/songcore-release-contract.md` 一字未改，`native/include/songcore.h`、`native/src/songcore_ffmpeg.c`、`native/xmake.lua` 一字未改。
- 未创建 tag，未创建 GitHub Release，未触碰 `songcore-v0.1.0` 的任何历史资产。
- `PREFLIGHT_VERDICT=ENVIRONMENT_BLOCKED`、7 个未决项（Q1、Q2 正式 closeout、Q3、Q4、Q5、D1、D3）保持原样。
- #329 的 L1/L2/L3 仍是 campaign evidence，本归档不把它们升格为 RELEASE。
- 归档只做三件事：复制小文件、生成两份 sha256 清单、补写本节与 §2/§6 的指向说明。
