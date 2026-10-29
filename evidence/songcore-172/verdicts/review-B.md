# Review B — ABI / Correctness 独立评审（#172 PR-B windows release lane）

- 评审角色: Review B（ABI/Correctness），全新视角、只读；本文件为本次评审唯一新增产出
- 日期: 2026-10-29
- 对象: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`，8 提交
  （40ae32a/bf23516 构建修复 + 62acc29 契约/56b9a04 编排器/6acb25b C 套件/
  423e37a 绑定门/6c52687 打包器/b461623 外部消费者 runner）；
  `git log`/`git status` 实测，tracked 树干净
- 材料: 8 份既有 verdict（evidence/songcore-172/verdicts/，全文已读）+
  契约 docs/release/songcore-release-contract.md（822 行全文已读）；
  按战役约束未重跑大型构建，以下每项均注明"本人实测"或"材料来源"

## VERDICT: APPROVE

ABI/Correctness 视角未发现任何会坏事的缺口。必查 8 项全部由本人以命令
实测复核（非转述 verdict），证据如下。

## 1. ABI 冻结头（本人实测 PASS）

```text
sha256sum native/include/songcore.h
  → cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536
  == 契约 §3 冻结 ABI v1 身份（cf509a5c…78536 前缀吻合）
git show origin/main:native/include/songcore.h | sha256sum → 同值；
cmp 两文件 exit 0 → 与 main 逐字节一致
grep SONGCORE_API native/include/songcore.h → 15 个函数声明
  （abi_version/last_error/open/probe/stream_count/stream_info/
    select_stream/metadata×3/artwork×2/read_pcm/seek/close），
  恰为契约 §3 集合；SONGCORE_ABI_VERSION 1u（songcore.h:62）
```

## 2. 套件计数与比对真实性（本人实测 PASS）

- C 套件 summary（suite-results/20261029-052222/summary.json）：verdict
  PASS，12/12 case；子 case 算术 22×4+5+5+3+1+1+4+2=109（+布局 case）
  与 s2-csuite 一致；artifact/header sha 与 §1 同值。
- 绑定 summary（binding-parity/20261028-215619/summary.json）：21/21 case；
  comparison `[35,0]`；算术 109(raw)+68(adapter)+35(comparison)+2(layout
  lf.*)=214/214，layout receipt 实含恰 2 条 `lf.*` PASS。
- **comparison.log 真伪重验（本人重跑 diff，非引用）**：11 组 raw receipt
  逐一以 C receipt 正文（剥 exit/stdout/stderr 包裹）`cmp`：9/11 逐字节
  IDENTICAL；garbage 两例仅 INFO 行文件路径显示形式不同（绝对路径 vs
  basename），观测值 status=104 相同——归一化属合理语义比对。FLAC parity
  receipt 全载荷与 C 仅差 runner 包裹行。verdict 对 parity fixture 的
  "INFO/META 行逐字节一致"主张经本人实测成立，无伪造迹象。
- 绑定门禁两轮绿日志（gate-20261029-061452/061511.log）实测含
  `test result: ok. 35 passed; 0 failed`（qianqian-decode-songcore）；
  `ops/run-binding-gate.ps1:49-82` 与 `ops/run-csuite.ps1:52-53,155-161`
  均对 artifact/header sha fail-closed 钉扎（mismatch → exit 11）。

## 3. Oracle 唯一性（本人实测 PASS）

- 语料唯一：`git diff origin/main...HEAD --stat -- native/experiments/
  native/include/ native/src/` 为空；fixtures/ 恰 4 件；reference.json
  （schema songcore-equivalence-reference/1）为唯一期望源。
- **lossy mp3 政策核对**：reference.json 对 mp3-cbr-id3v23 冻结
  `pcm_sha256 7f220370…60e926`（精确确定性哈希，无公差字段）——与 S3
  Linux 判定所述"lossy 同为精确哈希"同政策。Windows 侧同策略实测：
  C 套件 mp3 receipt `CASE s07.full-decode PASS` 且
  `pcm_sha256=7f220370…60e926`（断言非仅记录）；summary 的
  reference_policy 明示 "exact … ALL fixtures, lossy included; no numeric
  lossy tolerance exists and none was invented"。
- 套件/runner 源码无自造数字：`grep -E '[0-9a-f]{32,64}|tolerance'`
  csuite_consumer.c → 零冻结哈希/零公差（仅政策注释，reference.json 引用
  1 处）；runner/src/*.rs 内嵌哈希仅为 artifact/header/truncated-sha 钉扎
  与 SHA NIST 自检向量，无 PCM 期望值。lossy post-seek 两侧同为
  record-only（C: `INFO post-seek-record-only`；adapter:
  `INFO adapter-post-seek-record-only lossy=1`，drain 帧数 133729/89953/
  46177 两侧行同值），无损 post-seek 断言 landing 帧起逐字节相等
  （csuite_consumer.c:689-697）。

## 4. seek/EOF/错误分类/malformed/truncated 四侧一致（本人实测 PASS）

- **冻结归因引用为真**：#296 MATRIX.md:28/35（truncated sha 0aa66168…、
  decode-read 108/-1094995529）、decode-seek 行（73502/77042/80375 ^=1、
  "could not reach a landing point"）；`grep` 实证该诊断串在
  native/src/songcore_ffmpeg.c:1228。
- **Windows C 侧**（receipts）：readfault frames_before=82944 →
  108/-1094995529/"pending decode error"；seekfault 110 + landing −1 +
  post-read 108；truncated 115200 帧 → typed 108 终态稳定；garbage →
  104；argcheck 100/102/诊断契约。
- **Rust raw 侧**：11 组 receipt 与 C 逐字节一致（§2 实测）。
- **Rust adapter 侧**（adapter.log）：full-decode sha 与 reference.json
  逐字相同（4 fixture）；landing frames 与 C µs 经同一舍入式吻合
  （例 flac p50: C 1985306 µs ↔ adapter 87552 帧）。
- **S3 Linux 侧**（linux/verify/canonical/c-suite-receipt.json + 判定）：
  bps 24/24/16/0 按 fixture 与 Windows receipts 完全一致（alac=24、
  flac=16、mp3=0）；alac-16 seek landing 928798/1950476/2972154 与
  Windows 逐字节相同；flac p50 1985306 两侧相同；EOF 终态
  （SONG_EOF, produced==0, 可重复）两侧同断言。
- 契约 G2 语义（EOF 形状、next-read-lands、metadata/artwork/format
  unchanged、typed fail-closed、无伪造数据）在 C
  （csuite_consumer.c）、Rust raw/adapter、外部消费者三处源码/回执中
  均按同一语义编码，未见任一侧发明独立语义。

## 5. 绑定侧零新增 API（本人实测 PASS）

`git show --stat` 逐提交核对：六个 PR-B 提交（62acc29…b461623）在
`crates/` 下零改动；分支对 crates/ 的全部差异 = 仅
`crates/qianqian-songcore-sys/build.rs`（+133/−11，来自 ask 已知的 PR-A
构建修复 40ae32a，属 provenance 哈希实现，非 API 面）。两绑定 crate 的
lib.rs/公共面/测试零改动；s2-binding "源码零改动"主张与 git 事实一致。

## 6. songcore-v0.1.0 未被改动（本人实测 PASS）

`git tag songcore-v0.1.0^{commit}` = 522c16d2…，
`git ls-remote origin refs/tags/songcore-v0.1.0^{}` = 522c16d2…（一致）；
分支 diff 文件清单不含任何 release 命名空间路径；打包/取证对 v0.1.0
资产均为只读提取（s2-package O2、契约 §15 记录链）。

## 7. #324/#323 引用而非重释（本人实测 PASS）

s2-build/s2-package 引用的 recipe sha `3ba0a9e6…` 与 closure manifest
`0d0f7af9…` 经 grep 在 evidence/windows-native-tier2-324/ARTIFACT.md:45,63
实测同值（closure 未漂移）；#296 归因（§4）逐项与 MATRIX 相符；
包内 THIRD_PARTY_NOTICES 提取链（018e0cbd…）与契约 S-V010 冻结值同源。
未发现把旧证据"重释"为新语义的情形。

## 8. 外部消费者语义形状 vs 契约（本人实测 PASS）

包（zip 实测解包）：14 文件恰为契约 §7 形状；`sha256sum -c
metadata/checksums.txt` OK；包内 songcore.h = cf509a5c…、libsongcore.a =
1f5f8cf8…（与构建/套件/绑定同一字节）。zip 现值 7b4d0615… == SHA256SUMS
sidecar == package-summary.container.zip_sha256；339c798d 为
rebuilt:true 记录的前代（见 §10-3）。README L37/L39 明示 windows 链接
契约恰为 `-lsongcore -lbcrypt`，manifest.runtime_dependencies 同。
`metadata/external-consumer-example.c` 与 `ops/verify-external.ps1` 内嵌
消费者断言：abi==1、EOF 形状（SONG_EOF 时 got==0）、SONG_OK 时
0<got≤4096、seek SONG_OK、post-seek 首读 got>0、`song_close(NULL)`
文档化 no-op——与契约 §11 G1/G2 及 songcore.h 语义一致，无越权断言
（未把"sample-exact seek"等头文件不承诺的性质编码进 oracle）。

## 9. NOT RUN（如实，不充当 PASS）

1. **PR-A #326 CI 状态未独立复核**：本 shell 无 `gh` 可执行
   （`gh pr view 326` → command not found）。"OPEN、CI 全绿"采信为
   ask 给定材料，非本人实测。
2. 未重跑 C 套件/绑定 runner/构建/打包全流程（按战役约束：verdict 与
   receipt 为主要材料，本人以 §2/§3 的抽查命令复核真伪；抽查覆盖
   comparison 重 diff、summary 算术、门禁日志、包解包校验、四侧 receipt
   交叉比对，未覆盖每轮 build/rc stamp 的全量重放）。
3. GitHub v0.1.0 release 资产本体未重新下载重哈希（tag 指针已实测一致；
   资产哈希链采信契约 §15/O2 记录）。

## 10. 发现（全部 P3，不阻塞本通道）

1. **P3 — `song_open` 头文件措辞 vs 实现 differential（本人复核属实）**：
   `native/include/songcore.h:215` 写失败时 "*out_handle is then
   untouched"，而 `native/src/songcore_ffmpeg.c` 的 song_open 在参数检查
   后即 `*out_handle = NULL` 且全部失败路径保持 NULL。两读法下"失败后
   调用方不持有可用句柄"不变量均成立，套件仅断言该双真不变量。s2-csuite
   §4.1 已如实记录并归 authority review——按 AGENTS.md 不得由验证侧静默
   改写；本评审确认该处置正确，留待 owner 裁决（改措辞或改实现二选一，
   不影响 ABI v1 冻结身份以外的任何已测行为）。
2. **P3 — Windows 包内 `external-consumer-example.c` 头注为 Linux 形态
   链接行**（`-lm -lpthread`）；windows 契约由 README L56 与
   manifest.runtime_dependencies 正确承载（`-lbcrypt`）。s2-package
   NOT_RUN #5 已记录。建议后续修订时在该文件加一行 windows 链接指引
   （文档级，非语义缺陷）。
3. **P3 — 容器哈希引用链换代未在 s2-package 文内加注**：s2-package §1
   记录 339c798d… 为容器哈希，而磁盘/SHA256SUMS/summary/外部验收的现行
   容器为 7b4d0615…（rebuilt:true、previous_zip_sha256:339c798d 均有
   机器留痕，两 verdict 各自诚实）；仅读 s2-package §1 会拿到过期身份。
   建议 shipping 提交在 s2-package 加一行"最终容器=7b4d0615…"的补注，
   并保持 s2-external §7 已记录的 ask 侧旧哈希（b2c222cd）重钉事项。
4. **P3 — PR-A #326 CI 绿为 ask 给定、未经本评审独立复核**（§9-1）。
   合并 PR-B 前由 shipping 阶段以 CI 面板复核一次即可。

## 11. 结论

ABI 冻结头与 main 逐字节一致；oracle 唯一且对 lossy 同为精确确定性哈希
（Windows 与 S3 同策略，无自造公差）；12/12 + 109、21/21 + 214/214 +
35/35 的机器比对经本人重 diff 证实真实；seek/EOF/错误分类/malformed/
truncated 语义在 C、Rust raw、Rust adapter、S3 Linux 四侧一致且可溯源至
#296/#324 冻结归因；绑定 API 零新增；v0.1.0 历史未被触碰；外部消费者
断言与契约一致。既有记录诚实（失败轮、NOT_RUN、差分均留痕）。
**APPROVE**。上述 P3 项均不要求在本分支修复，但须随 shipping 材料一并
呈现给 owner。
