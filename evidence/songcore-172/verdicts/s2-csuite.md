# S2 G2 判定 — Windows 发布通道 CORE_RUNTIME Common Formats parity C 套件

- 判定角色: C 消费者套件工程师（#172 S2，独立 subagent 会话）
- 日期: 2026-10-29
- 位置: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`；本会话新增提交 `6acb25c`
  （`git log` 实测；ask 要求提交的 `ops/run-csuite.ps1` 与套件源码
  `evidence/songcore-172/suite/csuite_consumer.c` 已入库）
- 上游输入: S2 构建通道（`verdicts/s2-build.md`，产物
  `native/build/artifacts/libsongcore.a` sha256
  `1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402`，
  本会话 `sha256sum` 独立复算同值）；D2 链接契约
  （`evidence/songcore-172/link-contract.md` §2：windows-x86_64 static =
  `-lsongcore -lbcrypt`，无其他）；#324/#296 冻结构造与归因；
  S-BIND `docs/architecture/songcore-binding-architecture.md` §5/§7；
  发布契约 `docs/release/songcore-release-contract.md` §11 G2。
  S2 verdict §5 NOT_RUN #2/#3（G1 冒烟、G2 parity）由此会话填补。

## 结论

**G2 CORE_RUNTIME 套件在本通道产物上 PASS（fail-closed 全程成立）**：
直接原生 C 消费者在 Windows 真机（10.0.26300 x64，WinLibs GCC 16.2.0，
mingw 静态链接）对冻结产物执行 S-BIND §7.1 全 13 步 parity 场景 ×
4 个 canonical 冻结 fixture + 6 个 malformed/argument/lifetime 臂 +
ABI/layout 冻结门，**12/12 通道级 case PASS，109/109 子 case PASS，
0 FAIL**；两轮独立重放同结果。本通道 windows-x86_64 由此取得
S-BIND §5 词表下的 **CORE_RUNTIME** 证据（仅 direct-C；binding 级
G3 另计，见 NOT_RUN）。

## 1. 产物与链接身份（实测）

```text
ARTIFACT  native/build/artifacts/libsongcore.a
          sha256 1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402
          （== S2 verdict §1 冻结值；Get-FileHash/runner 内独立复算一致）
HEADER    native/include/songcore.h sha256
          cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536
          （== 契约 §3 冻结 ABI v1 身份）
LINK      gcc -O2 -std=c11 -Wall -Wextra -I native/include
          evidence/songcore-172/suite/csuite_consumer.c
          native/build/artifacts/libsongcore.a -lbcrypt -o csuite_consumer.exe
          （= link-contract.md §2 显式契约：归档 + -lbcrypt，无其他；
            实测编译零警告，-Wall -Wextra 干净）
GCC       gcc.exe (MinGW-W64 x86_64-msvcrt-posix-seh, r1) 16.2.0 (WinLibs)
```

链接契约负证由 D2 已冻结证据承担（`audit/win-link-nobcrypt.log`：
缺 `-lbcrypt` 时任意 SongCore 成员拉入即死于 3 个 BCrypt 符号），
本会话未重做负例。

## 2. 用例统计（两轮一致；stamp 20261029-051408 / 20261029-051431）

| # | case | 子 case | 结果 | 冻结对照物 |
|---|---|---|---|---|
| 1 | abi-layout-frozen | 223 行逐行 diff | PASS | `experiments/songcore-call-comparison/results/layout-c.txt`（C probe vs 冻结输出，内容逐位一致；换行差 CRLF/LF 已归一并在 receipt 记录）|
| 2 | parity.mp3-cbr-id3v23 | 22 | PASS | reference.json + equivalence.json |
| 3 | parity.flac-16-44-stereo | 22 | PASS | 同上 |
| 4 | parity.alac-16-44-stereo | 22 | PASS | 同上 |
| 5 | parity.alac-long | 22 | PASS | 同上 |
| 6 | readfault-flac | 5 | PASS | #296/#324 decode-read 构造 |
| 7 | seekfault-flac | 5 | PASS | #296/#324 decode-seek 构造 |
| 8 | truncated-flac | 3 | PASS | #296/#324 100000-byte 截断构造 |
| 9 | garbage-nomedia | 1 | PASS | songcore.h song_open 错误集 |
| 10 | garbage-empty | 1 | PASS | 同上 |
| 11 | argcheck-stable-classes | 4 | PASS | songcore.h 状态表 + native guard |
| 12 | closecheck-reopen-stable | 2 | PASS | S-BIND §7.1 step 13 |
| — | 合计 | 109 子 case（+布局 223 行）| 12/12 PASS | |

parity 22 子 case = S-BIND §7.1 步 01 abi、02 open、02 诊断契约、
03 NOT_OPEN 类、03 probe 事实、03 probe 幂等、04 流枚举+默认流身份+
非法索引、05 metadata canonical+raw 枚举、06 artwork 结构、
07 full-drain（frames+sha+terminal）、11 EOF 稳定、08/09/10 seek×
（status+landing / next-read-lands / unchanged）×3、13 reopen 稳定、
13 close(NULL) no-op。

### 冻结 PCM 实测值（Windows 真机直采，全部 == 冻结值）

```text
mp3-cbr-id3v23     176400 frames  pcm_sha256 7f220370…60e926  terminal=SONG_EOF
flac-16-44-stereo  176400 frames  pcm_sha256 3dc4728a…2dd62dc  terminal=SONG_EOF
alac-16-44-stereo  176400 frames  pcm_sha256 431db36e…f1b2791a46 terminal=SONG_EOF
alac-long          264600 frames  pcm_sha256 e9479a0a…0e45f612  terminal=SONG_EOF
```

### seek 25/50/75% 实测（status 全 SONG_OK，landing 均非哨兵且 ≤ target）

```text
flac  landing 940408 / 1985306 / 2925714 µs；post-seek drain 与
      顺序参考切片按 landing 帧逐字节 memcmp 相等（无损帧精确，
      songcore.h:394 冻结语义），帧数 == pcm_frames − landing 帧
alac  landing 928798 / 1950476 / 2972154 µs（alac-long 另含 50% =
      2972154）；同上逐字节相等
mp3   landing 992653 / 1985306 / 2977959 µs；有损：post-seek PCM
      不做身份断言（语料无冻结数值公差，Windows 不得自造解释），
      drain 帧数 133729 / 89953 / 46177 与 terminal=EOF 记录在案
75% 臂额外执行 song_seek(h, target, NULL)（songcore.h:390 允许
NULL out）再二次 seek 观测 landing，均 SONG_OK
```

### malformed / typed fail-closed 实测

```text
readfault   byte 76000 ^= 0x01：干净前缀 82944 帧 → SONG_ERR_DECODE_ERROR(108)
            native=-1094995529 (AVERROR_INVALIDDATA)（== #296 MATRIX
            line 35 冻结归因），诊断非空，下次 read 复现同错误且 0 帧
seekfault   73502/77042/80375 ^= 0x01：seek 2.0s → SONG_ERR_SEEK_ERROR(110)
            landing=-1 哨兵，诊断 "could not reach a landing point"
            （== #296/#324 MATRIX + native/src/songcore_ffmpeg.c:1228）；
            post-seek read → 108 / -1094995529 / 0 帧
truncated   前 100000 字节（构造 sha256 0aa66168…38b39f == #296/#324
            MATRIX 冻结身份，runner 逐次核验）：probe OK → 115200 帧 →
            SONG_ERR_DECODE_ERROR(108) typed 终态；终态稳定；无
            SONG_OK-零帧伪造（精确终态类按契约只记录不强断言）
garbage     非媒体/空文件：song_open → 104 UNSUPPORTED_CONTAINER，
            调用方不持有可用 handle
argcheck    NULL handle 四入口 → 100；未 probe 操作（count/read/seek/
            metadata/select_stream）→ 102；capacity 0 → 100 且
            *out_frames==0；非法流索引/负 seek → 100；诊断契约
            （成功后 NULL message，失败后非空）
```

## 3. Reference/公差政策的实测来源（record-only 声明）

- 期望值唯一来源 = canonical 冻结语料（S-BIND §7.3 / 契约 §11 G2
  "no second corpus, no second reference numbers"）：
  `native/experiments/songcore-equivalence/reference.json`（frames/
  rate/channels/pcm_sha256/fixture_sha256）+
  `results/songcore-equivalence.json`（冻结 Linux direct-C 源事实：
  container、duration_us）。runner 运行时读取，C 子进程只收 argv，
  套件源码零冻结数字副本。
- **有损公差**：ask 预期"有损=既有冻结 tolerance"。实测语料中
  不存在任何数值公差——mp3 fixture 的既有冻结政策就是确定性
  `pcm_sha256` 精确哈希（Rust 绑定测试 `tests/common/mod.rs` 与
  Linux direct-C reference 同一政策）。本套件照此执行（全 fixture
  精确哈希），未发明 Windows 特有解释；有损 post-seek PCM 身份
  因无冻结公差而只记录不断言（见 NOT_RUN）。
- 构造媒体 = #296/#324 MATRIX.md 冻结构造的逐字节重建
  （身份已验证的 FLAC fixture 上 XOR/截断），构造结果与冻结
  sha256 逐次核对（truncated == `0aa66168…`）。

## 4. 判定外发现（如实记录，不在本通道裁决）

1. **`song_open` 头文件措辞 vs 实现的 differential**：
   `songcore.h:219` 写 "*out_handle is then untouched"，而
   `native/src/songcore_ffmpeg.c` song_open 在分配后的全部失败路径
   先 `*out_handle = NULL` 再返回。语义不变量（失败后调用方不持有
   可用 handle）两种读法下都成立；套件按双读法皆真的不变量断言
   （预置 NULL 后失败仍 NULL），未把字面"untouched"编码进 oracle。
   差异已记录，归 authority review 处置（verification 不改产）。
2. **套件自缺陷（非产物缺陷）**：初版 drain 累积器在 truncated 臂
   越界写堆（4096 帧 buffer 收 115200 帧），触发 SIGSEGV
   （gdb 回溯 #0–#2 在 FFmpeg 闭包、#4/#5 在套件代码——根因是套件
   自身堆破坏，非 FFmpeg 崩溃）。修复为 block 大小 scratch +
   max_frames 截断拷贝后，同构造干净通过。该事故未进入任何
   最终证据轮；两轮最终 stamp 均为修复后构建。
3. **本地 pre-push 门 6/7 绿（与 S2 §4 相同的既有红）**：
   `lefthook run pre-push --all-files`（215.71s，日志
   `evidence/songcore-172/suite-results/pr-c-lefthook.log`）：
   commit-convention ✔（含本通道 6acb25c，首版 header 74>72 被
   fail-closed 拦截后 amend）、vocabulary ✔、plugin-boundary ✔、
   rust-fmt ✔、rust-clippy ✔、rust-test ✔ (172.26s)；
   release-policy 🥊 exit 3 = `E14-real-ELF: expected 2 'POLICY_RED:
   R3 native archive target mismatch'; got 2 POLICY_RED: R3 native
   archive/receipt drift` —— 与 `verdicts/s2-build.md` §4 逐字相同
   的宿主平台结构性 differential（Windows 宿主 gcc 产 pe-x86-64，
   E14 负例期望 ELF 文案；处置不变：fail-closed，平台真值归
   ubuntu-24.04 CI）。本通道 delta（C 文件 + PS 文件）不触碰该门
   输入，红为 PR-A 既有状态原样继承。docs-verify 被 piped 中止
   未达（同 S2 记录）。

## 5. NOT_RUN（如实列出，不充当 PASS）

1. **冻结语料之外的格式**（WAV/OGG/Vorbis/Opus/AAC-ADTS/多声道/
   高位深/24-bit 等）：canonical 语料只有 mp3/flac/alac×2 四个
   fixture 且 #8 common formats closure 的冻结媒体止于此；无冻结
   reference 即 NOT_RUN，不以未冻结媒体冒充 parity。
2. **artwork 内容级 parity**（role/mime/字节同一性/宽高哨兵）：
   四个冻结 fixture 均无内嵌 artwork（实测 count==0），套件只做
   结构断言；无提供 artwork 的冻结媒体。
3. **多流 song_select_stream 真实切换**：四 fixture 均 1 条可解码
   音频流（实测 streams=1）；非法索引类（100）已由 argcheck 覆盖，
   合法切换路径无冻结多流媒体，NOT_RUN。
4. **有损（mp3）post-seek PCM 身份**：语料无冻结数值公差；
   套件只记录 drain 帧数与 terminal，不做身份断言，亦不发明
   Windows 公差。
5. **G3 BINDING_RUNTIME**（Rust sys/adapter 走同一场景与 direct-C
   对表）：不在本 ask 范围，未执行。
6. **windows shared/DLL、MSVC、arm64 形态**：recipe
   artifacts.shared=false + 契约 §4 out-of-scope，未构建未测试。
7. **lefthook 门在真实 push 下的收窄行为**：本会话只有手工
   --all-files 全量门记录；未推送。

## 6. 边界

- 未重做 S2 构建通道/D1/D2；产物与 closure 未动（sha 双核一致）。
- 套件结果目录（suite-results/<stamp>/ 含 12 receipt + summary.json +
  编译日志 + 构造媒体）留盘未提交；verdict 文件留盘未提交（按战役
  惯例，evidence/verdict 合并属后续 shipping 提交；本通道仅提交
  ask 指定的 runner + 套件源码 `6acb25c`）。
- 分支未推送、未开 PR（ask 未授权）。

## 7. 重放

```text
powershell -ExecutionPolicy Bypass -File ops\run-csuite.ps1
# 前置：mingw gcc 可解析（-Gcc 或 PATH 或宿主 WinLibs 路径）；
# 无需 cargo/WSL。每次运行落新 stamp：
#   evidence/songcore-172/suite-results/<stamp>/summary.json + 12 receipt
# stdout 摘要 ~0.8KB（< 64KB）；两轮 stamp 20261029-051408/20261029-051431
# 均 12/12 PASS、exit 0。
```
