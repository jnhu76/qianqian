# S2 G3 判定 — Windows 发布通道 BINDING_RUNTIME 绑定一致性（C consumer ↔ Rust binding parity）

- 判定角色: 绑定一致性工程师（#172 S2，独立 subagent 会话）
- 日期: 2026-10-29（本机时钟 2026-10-28 晚间执行，evidence stamp 以目录名为准）
- 位置: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`；本会话**零提交**——新增文件全部为
  untracked 的 evidence/runner 文件（`git status` 实测：clean 除 untracked）
- 上游输入:
  - S2 C 套件判定（`verdicts/s2-csuite.md`）：G2 CORE_RUNTIME 已在该通道产物
    上 PASS，receipts 落 `evidence/songcore-172/suite-results/20261029-052222/`
    （12/12 case、109/109 子 case；本会话读其 summary.json 实测 verdict=PASS）
  - 冻结产物 `native/build/artifacts/libsongcore.a` sha256
    `1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402`——
    本会话 `sha256sum` 独立复算**同值**（与 C 套件 summary、run-csuite.ps1
    冻结期望一致）；header `native/include/songcore.h` sha256
    `cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536`（§3
    ABI v1 冻结身份）同值复算
  - 冻结契约 `docs/release/songcore-release-contract.md` §11 G3；oracle
    `docs/architecture/songcore-binding-architecture.md` §7（场景 01–13、
    §7.2 冻结比较集、§7.3 无第二 corpus）
  - canonical corpus `native/experiments/songcore-equivalence/
    {fixtures,reference.json}` + `results/songcore-equivalence.json`
    （frozen Linux direct-C source facts: container/duration_us）
  - 冻结 #296/#324 构造（readfault byte 76000 ^= 0x01；seekfault
    73502/77042/80375 ^= 0x01；truncated first 100000 bytes，frozen sha
    `0aa66168…b39f`——本会话 runner 构造后独立复算同值）

## 结论

**G3 绑定一致性在本通道产物上 PASS（fail-closed 全程成立）**。

C consumer 与 Rust binding 在**同一 artifact、同一批媒体**上语义一致：
绑定制 runner（`evidence/songcore-172/binding-parity/runner/`）以
qianqian-songcore-sys（raw）1:1 复刻 csuite_consumer.c 的全部模式与判词，
另经 qianqian-decode-songcore（adapter，经 kernel 解析的 PcmDecode
capability 的生产 open 路径）复刻 parity 形状；最终 stamp
`20261028-215619`：**21/21 通道级 case PASS、214/214 子 case PASS、
0 FAIL**；对 C 套件 receipts 的逐字段机器比对（`comparison.log`）
**35/35 PASS**，其中 4 个 parity fixture 的 INFO/META 观测行与 C 套件
receipts **逐字节一致**（`diff` 实测 FLAC fixture：IDENTICAL）。本通道
windows-x86_64 由此取得 S-BIND §5 词表下 (windows-x86_64, qianqian-
songcore-sys + qianqian-decode-songcore) 的 **BINDING_RUNTIME** 级证据
（按契约 §11 G3：与 direct-C 参考在冻结比较集上一致）。

## 1. 运行的命令（真实执行，无替代）

```text
# 门禁命令（编排器指定的同一命令）——真实跑，exit 0：
cd C:\Users\fred1\source\qianqian-172\crates\qianqian-decode-songcore
export PATH="/c/Users/fred1/tools/mingw64/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-gcc-ar
cargo test -p qianqian-songcore-sys -p qianqian-decode-songcore --target x86_64-pc-windows-gnu
→ exit 0；qianqian-decode-songcore lib tests 35 passed / 0 failed；
  songcore-sys lib 0 tests；ffmpeg_log_gate 0 tests；doc-tests 0
→ receipt: evidence/songcore-172/binding-parity/cargo-test-gate-20261028.log

# 绑定制 parity runner（本会话新增 evidence 工具，非产品代码）：
cargo build --target x86_64-pc-windows-gnu   # runner crate
./target/x86_64-pc-windows-gnu/debug/binding-parity-runner.exe
→ exit 0；SUITE PASS cases=21 passed=21 failed=0；214/214 子 case
→ stamp: evidence/songcore-172/binding-parity/20261028-215619/
```

工具链（实测）：rustc 1.96.0 (ac68faa20) host x86_64-pc-windows-msvc、
cargo 1.96.0、target x86_64-pc-windows-gnu、WinLibs GCC 16.2.0
（MinGW-W64 x86_64-msvcrt-posix-seh, r1，经 .cargo/config.toml 的
linker=x86_64-w64-mingw32-gcc）——与 #324/本通道 C 套件同一环境族。

### 调用约束（编排器门禁必须知道，实测负证在案）

```text
1) `-p` 解析位置：在仓库根 `cargo test -p qianqian-songcore-sys
   -p qianqian-decode-songcore` **无法解析**（两 crate 均被主 workspace
   exclude，error: package ID specification … did not match any packages，
   实测）。该命令仅在 qianqian-decode-songcore 的 crate 目录（或
   --manifest-path 指向其 Cargo.toml）下解析——sys 是 decode 的 path dep，
   处于其依赖图内；反向（在 sys 目录下 -p decode）不解析，实测。
2) 目标三元组：host 默认 msvc **链接失败**（LNK2019 × 16：mkstemp/fdopen
   等 POSIX CRT 符号，mingw 合并归档 vs MSVC CRT），exit 101。负证 receipt:
   evidence/songcore-172/binding-parity/cargo-test-msvc-default-NEGATIVE.log。
   必须 `--target x86_64-pc-windows-gnu` + PATH/AR 环境（#324 先例，
   evidence/windows-native-tier2-324/ENVIRONMENT.md 同款）。
```

以上两点是环境/调用事实记录，不是绑定缺陷。

### 门禁入口裁定（escalation 落地，2026-10-29）

编排器脚本级门禁第 1 轮以 root 下裸命令失败（exit 101，resolution）。
两个结构性根因均以本会话命令证实：

1. **resolution**：两 crate 由已提交的、带注释的设计决定排除于主
   workspace（`crates/qianqian-decode-songcore/Cargo.toml` 与
   `crates/qianqian-songcore-sys/Cargo.toml` 注释块："The enclosing
   workspace excludes this crate (it links the separately-built SongCore
   native artifact), so it compiles as its own unit with this
   lockfile"）→ root 下 `-p` 无法匹配（实测复现 exit 101，receipt
   `evidence/songcore-172/binding-parity/gate-round1-ROOT-REPRO.log`）。
2. **link**：host 默认 target `x86_64-pc-windows-msvc` 无法链接 mingw-w64
   归档（12 个唯一 LNK2019：`__mingw_fprintf/__mingw_strtod/
   __mingw_vsnprintf`、`gettimeofday`、`usleep`、`mkstemp`、`fdopen`、
   `sincosf`、`isatty`、BCrypt×3——mingw CRT/POSIX 扩展，MSVC/UCRT 无；
   receipt `cargo-test-msvc-default-NEGATIVE.log`）。msvc 侧唯一修法
   （拉入 mingw 运行库）= 新增非系统运行时依赖，未经契约评审禁止
   （发布契约 §9）。

修复矩阵（全部本会话实测）：

| 变体 | 裸命令 @ root | 结果 |
|---|---|---|
| 现状（crate 排除） | exit 101 resolution | 失败 receipt `gate-round1-ROOT-REPRO.log` |
| 临时 membership（已完全还原，git 状态实测 clean） | exit 101，msvc LNK2019×12 | `gate-MEMBERSHIP-msvc-NEGATIVE.log`——**membership 单独不是修法**，且会使当前已绿的 shipper lefthook `cargo test --workspace`（host msvc、无 mingw PATH，`pr-b-lefthook.log` ✔️ rust-test 238s）转红 |
| membership + gnu env（CARGO_BUILD_TARGET+PATH/AR） | exit 0，35/35 | `gate-MEMBERSHIP-gnuenv-POSITIVE.log`（实验后清单已还原） |
| **规范入口（零仓库改动）**：`ops/run-binding-gate.ps1` | exit 0，35/35 | 两轮绿：`gate-20261029-061452.log`、`gate-20261029-061511.log` |

**方案 (b)（repo 侧 membership/lefthook/config 改动）已被考虑并拒绝**：
违反上述已提交的 exclude 设计注释（静默改写既有 authority）；迫使
lefthook 引入 mingw PATH 依赖（其现环境没有）；repo 级 `[build] target`
默认会连带破坏同配置文件下 S3 Linux lane 的原生 cargo 运行；msvc 侧则
需未经评审的新运行时依赖。按 AGENTS.md（authority resolution / OPEN
规则）该选择不属实现 agent——escalation 裁定采用方案 (a)：
**修编排器调用方式，不动仓库**。

**旧命令的两次失败（第 1 轮 resolution、以及 membership 实验所展示的
msvc link 形态）是编排层 bug 与环境结构性的 receipt，不是绑定缺陷**；
本判定的性质判定（同一 artifact 上 C vs Rust 语义一致）以规范门禁
`ops/run-binding-gate.ps1` 的实测为准（两轮 exit 0，35/35）。

## 2. 比较字段（契约 §11 G3 冻结比较集 → 实测证据）

| 冻结比较字段 | 绑定侧证据 | 与 C 参考一致方式 |
|---|---|---|
| status / error class | raw 模式逐字判词：NOT_OPEN(102)、INVALID_ARGUMENT(100)、IO/UNSUPPORTED_CONTAINER/OOM open 类、DECODE_ERROR(108)+native −1094995529、SEEK_ERROR(110)+landing −1 | `cvb.*` 11 组 CASE 序列逐项 diff == C receipts；`comparison.log` 全 PASS |
| units（µs、frames） | seek target/landing 以 µs 比较；adapter landing frames == C landing µs→frames（同舍入式 (us*rate+500000)/1e6） | `cvad.seek.<id>.p{25,50,75}` 12/12 PASS |
| sentinel（landing −1、duration −1、mask、has_*） | landing −1 ↔ adapter `None`（unknown 不制造）；probe-idempotent、scope∈{0,1}、has_*∈{0,1} | INFO 行逐字节 diff == C；`cvb.*` PASS |
| source facts（rate/channels/mask/duration/bps/codec/container/streams/selected） | `INFO probe`/`INFO streams` 行逐字节比对 | FLAC 实测 diff IDENTICAL；4 fixture 全 PASS |
| PCM 格式 + 参考策略 | Float32 interleaved；**精确 pcm_sha256 对全部 4 fixture（含 lossy mp3，语料自身冻结策略；无任何自造数值公差）**；lossless seek 后 drain 与 landing 帧起顺序切片逐位相等 | `s07.full-decode` ×4 + `.lands` ×12(raw) 全 PASS；`cvad.drain.*` frames+sha == C |
| seek landing contract | SONG_OK、landing∈{−1}∪[0,target]、next-read-lands、metadata/artwork/format unchanged（raw 全字段 + adapter format/duration） | `.lands`/`.unchanged` 子 case 全 PASS；与 C 逐字节同 |
| EOF | SONG_EOF==1 终态、produced==0、可重复、与 error 可区分（`s11.eof-stable`）；adapter 侧 DecodeOutcome::Eof 稳定 | 全 PASS |
| 错误分类（malformed/truncated） | readfault：干净前缀→108/−1094995529→复现稳定；seekfault：110+landing −1+诊断文 "could not reach a landing point"（逐字）→post-read 108；truncated：typed fail-closed、终态稳定、零伪造数据 | `cvb.readfault/seekfault/truncated` PASS；adapter 臂 `ar.message-parity`/`as.seek-typed-diagnostic` PASS |
| ownership / lifetime | song_close(NULL) no-op 存活、close-后-reopen 全量重排恒等（`s13`/closecheck）、adapter drop-mid-stream→reopen 恒等、open 失败无 handle | 全 PASS；与 C 逐字节同 |
| ABI/layout | s01 abi==1（链接真件运行时）；layout 门：fresh Rust emission == 冻结 layout-rust.txt **且** == 冻结 layout-c.txt（223 行，CRLF 归一） | `lf.rust-vs-frozen-rust` / `lf.rust-vs-frozen-c` PASS |

C 套件 12 通道 case 的子 case 计数（22/22/22/22/5/5/3/1/1/4/2 + layout）
与绑定制 raw 通道**逐一相同**（MODE 行实测比对）。

## 3. 会话内的 fail-closed 事件（如实记录，均为 runner 自身开发迭代，非绑定缺陷）

1. SHA-256 NIST 自检两次 panic（K[52] 转写错 0x…b5→b3；112 字节 NIST 向量
   少打一段）——预检 abort，**未产生任何 evidence 目录**；修法：整体换用
   `tests/common/mod.rs` 的已验证实现（逐字节 splice）+ 自检保留。实现随后
   对正确向量全过（错误值 da584d21… 经 python hashlib 证实是向量字符串本身
   的真实摘要，实现无误）。
2. stamp 20261028-215245：FAIL 211/212——runner 把 record-only INFO 行
   print 而未记入比对事件向量（比对门抓到，fail-closed 生效）。
3. stamp 20261028-215414：FAIL 211/212——C 的 `lossy=` 字段实为传入
   lossless 旗标（mp3 → lossy=0）；改为复刻 C 可观测值。
4. stamp 20261028-215436：PASS 212/212（layout 门加入前）。
5. **最终 stamp 20261028-215619：PASS 214/214（含 layout 门）。**

## 4. NOT_RUN（如实）

- **相对 C 套件范围：无 NOT_RUN 格式。** C 套件 12/12 全部执行（verdict
  PASS），4 个 canonical fixture（mp3/flac/alac-16-44-stereo/alac-long）
  绑定侧全部同媒体执行；malformed/argument/lifetime 臂全部执行。绑定侧
  NOT_RUN 集为空。
- 语料中 224 s 真实 MP3 有意不入库（reference.json corpus_note），C 套件
  无此 case，绑定侧亦无——两侧一致地不声称。
- `experiments/songcore-call-comparison` crate 的 `layout` 模式（活编译
  C probe + 在线比对）未按其原样运行：该 crate 的 build.rs 无条件以
  `-lm -lpthread` 编译基准 C caller，Windows 宿主链接失败（实测 panic），
  修改该已入库实验超出本 ask 范围。其**门的实质**（Rust emission vs 冻结
  layout-rust.txt / layout-c.txt）以逐字复用的 emitter 在 runner 内执行
  PASS（`lf.*` 两 case + receipt `abi-layout-frozen.log` +
  `layout-rust-now.txt`）。此项如实作为替代执行记录，非原样重放。

## 5. 边界与诚实声明

- 绑定侧 **零新增 binding API**：qianqian-songcore-sys / qianqian-
  decode-songcore 源码零改动（`git status` 实测两 crate 路径无修改）；
  runner 为独立 evidence crate（自带 `[workspace]`，与两绑定 crate 同款
  机制），经 path 依赖复用现有 binding/adapter 公共面。
- adapter 层读路径的可观测面为 SourceFacts/format/duration/DecodeOutcome/
  ProviderSeekOutcome——metadata/artwork 原始枚举、argcheck、NULL 诊断等
  ABI 级字段由 raw 通道（sys）承担并比对；两层各自 PASS。
- 绿色 cargo test 是回归证据；本判定基于同 artifact、同媒体、冻结期望值、
  与 C receipts 的机器比对——非"测试绿即架构接受"。
- G3 契约附加要求（binding 构造期检查 songcore_abi_version()==1）由
  adapter 的 plugin 构造路径承担（`SongcoreDecode::new`，
  crates/qianqian-decode-songcore/src/lib.rs:51–66），本 runner 的
  kernel 激活路径实际执行了该构造；raw 侧 s01 断言同事实。
