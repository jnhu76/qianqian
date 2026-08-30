# E07 — Source/link minimization of the Qianqian FFmpeg core (Linux Native)

状态：`COMPLETE`（revision 2：PR #6 评审 9 项全部落实后，从 `rm -rf build`
clean-room 重推整条梯子；全部 gate 与 S0 基线行为等价，全部数字机器派生）。

评审修订内容：

1. 符号解析只认 **global/weak/common/unique 定义**（nm `ABCDGRSTVWu`）；local 符号
   （小写类型）不可能触发 archive member pull——修复前版本会把 local 定义也算进
   definition set，只能证明"充分闭包上界"。
2. probe 固定改为**机器断言**：`songcore_link_probe.o` 无任何 libc 引用，
   `nm -u` 的未定义集必须**恰好等于** 5 个契约符号（此前只有 `song_open`/`song_close`
   被 main 实际消费，其余 3 个仅靠 SongCore 内部级联带入）。
3. real GNU ld 证据升级为**硬 gate**：解析 `-Map` 的
   "Archive member included" 段（仅第 0 列成员行；缩进行是引用不是 pull），
   pulled-member **多重集**必须与模拟完全相等。
4. ELF 等价升级为**内容证明**：full-archive 链接 vs reduced-archive 链接
   **整文件 SHA256 相等**（本机 build-id 为内容派生，相等即整文件相等）；
   不等时回退到逐 section SHA256 比对并 fail。
5. 新增 **S6：libqianqian_songcore.so 实验**（见下）——把 shipped core 的
   符号边界用 version script 机制性强制，并给出其 **ABI/visibility baseline** 体积。
6. linked 体积统一三口径：**raw / stripped / stripped+xz**，shipping 结论一律用
   stripped（本工具链 raw==stripped：xmake release 链接产物本身不含 .symtab）。
7. `minimize_summary.py` **零手填数字**：所有字节/TU/verdict/xRT 从
   `gate.json`/`so.json` 派生，贡献分解由脚本计算并断言自洽。
8. clean-room 覆盖叙事依赖的全部 stage（含 s4-full-gc 对照组与 s5-Os），
   provenance 文件按 round 1（迭代）/ round 2（clean-room 权威）分组。
9. 修复对照组定义：s4-full-gc 必须带与最小 stage 相同的
   `-ffunction-sections -fdata-sections`，否则 `--gc-sections` 无段可收
   （首版对照 205 TU+GC 只省 8 B，即为该 bug）。

## 研究问题

对当前 SongCore capability（MP3/FLAC demux+decode、probe、metadata/artwork 内部路径、
streaming Float32 PCM、seek、EOF/error handling）：

- **A.** 进入 `libqianqian_av.a` 的最小 FFmpeg source/object closure？
- **B.** shipped codec core（SongCore + 实际链接进入的 FFmpeg 代码）最小多大？

## 方法

按仓库第一原则，禁止按文件名猜删。整条梯子全部机器推导：

1. **S0 冻结基线** — canonical `n3-min-noswr` import 重放，205 TU / 2,658,174 B。
2. **S1 link reachability audit**（`tools/link_audit.py`）—
   probe 固定完整 contract（`nm -u` 精确集断言）→ 逐 member 解析
   `libqianqian_av.a`（位置对齐 manifest，消除重名歧义）→ 只用 global/weak/common
   定义模拟 GNU ld fixpoint → **real ld `-Map` member 多重集硬相等** →
   **reduced-archive 重链接整 ELF SHA256 相等**。
   结果：202-member variant 闭包 → **106 pulled**，三重机器验证全绿
   （`build/minimize/s3-pthreads/report.json`，schema 2）。
3. **S3 configure 维度实验**（`tools/config_experiment.py`，一次一个维度，
   每个变体从 upstream configure/Make **重新求 closure**，不复制旧列表）：
   iconv、pthreads、runtime-cpudetect、asm、pic、iamf。
4. **S4 section GC**（`tools/minimize_flags.py` + `--gc_sections`；
   full-closure 对照组 s4-full-gc 携带相同 section flags）。
5. **S5 codegen**（-O2/-Os/-flto 替换，manifest flag mutation）。
6. **S6 shipped .so**（`tools/minimize_so.py`）：106-TU closure 加 `-fPIC`
   （唯一 codegen 维度；corpus oracle 等价性重跑），SongCore 以 PIC 编译，
   `tools/songcore_version.map`（`QIANQIAN_1.0 { global: 5 个 song_*; local: *; }`）
   + `--gc-sections` 链接出 `libqianqian_songcore.so`；导出符号断言
   （恰好 5 个带版本入口）+ consumer（`songcore_so_consumer.c`）对 MP3/FLAC
   走 open/probe/read/seek/close 功能 smoke。

每个候选阶段都跑完整 gate（`tools/minimize_gate.py`）：

- 15/15 Stage-A corpus：oracle vs candidate 行为等价（`verify_xmake_core.py`，
  含 `qn_bench correct` 的全文件解码 + 25/50/75% seek + suffix 校验 + EOF）；
- SongCore PCM canonical hash（MP3 + FLAC，与基线 byte-identical）；
- SongCore 级 seek/EOF（`tools/songcore_seek_probe.c`，生产路径）：
  FLAC = strict（suffix exact + clean EOF），MP3 = record 对比基线；
- 三个真实歌曲：qn_pcm_dump 全量解码（rc/frames/sha）+ SongCore seek probe；
- xRT throughput（真实歌曲，`qn_bench bench`）；
- 体积表（archive members/symbols/xz、linked raw/stripped/stripped+xz）。

gate 语义沿用 corpus 既有契约：FLAC seek `strict`，MP3 seek `record`
（MP3 bit reservoir 使 seek 后内容与顺序流天然不同，只做确定性对比）。

## 发现的真实基线问题

**真实 FLAC（flac-16-44-artwork.flac）在 S0 全量解码时，最后帧报
`invalid frame header`，qn_pcm_dump rc=1**——224.08 s 音频全部解出后才在文件末尾
失败（9,881,952 帧 == STREAMINFO 声明的全部样本）。此前 audible smoke 用
`--seconds` 采样播放，从未触达文件尾部。本实验将其作为 S0 基线行为记录
（`real_song_pcm.exit_code=1` + 固定 sha），所有候选阶段保持完全一致。
分类调查见 issue #7，本 PR 不修。

## Ladder（round-2 clean-room，`bash tools/minimize_cleanroom.sh` 一键复现）

| Stage | 变化 | TU | `.a` bytes | linked raw | linked stripped | stripped+xz | Corpus |
|---|---|---:|---:|---:|---:|---:|---|
| s0 | canonical 基线 | 205 | 2,658,174 | 973,048 | 973,048 | 350,604 | PASS |
| s3-pthreads | −iconv −pthreads，S1 投影 | 106 | 1,535,360 | 952,568 | 952,568 | 342,644 | PASS |
| s4-full-gc | **对照组**：205 TU + section flags + gc | 205 | 3,113,446 | 739,568 | 739,568 | 263,940 | PASS |
| s4-gc | 106 TU + section flags + gc | 106 | 1,801,016 | 719,080 | 719,080 | 255,868 | PASS |
| s5-Os | 106 TU，-Os（无 GC/LTO） | 106 | 1,196,120 | 690,504 | 690,504 | 241,468 | PASS |
| s5-Os-LTO | 106 TU，-Os -flto + gc 链接 | 106 | 3,927,032 | 530,664 | 530,664 | 180,408 | PASS |
| s6-shipped-so | 106 TU PIC + version script → `.so` | 106 | 1,535,360 | 1,019,536 | **949,016** | 339,984 | PIC PASS + smoke |

全部 5 个阶段 `minimize_compare` verdict = equivalent to s0；xRT 最低 762×
（警告线 50×）。s5-Os-LTO 的 `.a` 是 LTO bytecode，不参与 archive 比较。

### Linked-size 分解（stripped 口径，由 `minimize_summary.py` 从 gate.json 计算）

| 手段 | 节省 | 占总收益 |
|---|---:|---:|
| section GC（205 TU，973,048→739,568） | 233,480 B | 52.8% |
| 源码闭包 205→106（同样 GC 条件，739,568→719,080） | **20,488 B** | 4.6% |
| size codegen（-Os/LTO，719,080→530,664） | 188,416 B | 42.6% |
| **合计** | **442,384 B** | 100% |

## 两个答案（修订口径）

**A. Archive 最小化**——三个概念分开：

- **Proven sufficient closure（stock -O3）**：**106 TU / 1,535,360 B**。
  严格 global-only resolver 下 real ld 精确 fixpoint = 106（map 多重集相等 +
  整 ELF SHA256 相等）；这是**精确可达闭包**，但"精确数学最小值"仍以
  行为 gate 为准（106 通过全部行为 gate，是已验证上界而非下界证明）。
- **Smallest tested conventional archive**：同 closure 加 `-Os` =
  **1,196,120 B**。
- LTO archive（3.9 MB bytecode）不是 archive 最小化的正确工具。

**B. Shipped codec core 最小化**：

- 可执行形态（qn_pcm_dump = SongCore+FFmpeg+host 代码）：**530,664 B stripped**
  （xz 180,408；S0 973,048 → −45%）。
- **动态库形态（ABI/visibility baseline，新增）**：
  `libqianqian_songcore.so` = 106-TU PIC closure + SongCore PIC，
  version script 只导出 5 个 `song_*`（`QIANQIAN_1.0`），FFmpeg/内部符号全部
  local——stripped **949,016 B**（xz 339,984）；导出 dynsym 恰好 6 项定义项
  （5 API + `QIANQIAN_1.0` 版本定义）。它证明的是**符号边界可以被机制性强制**：
  整个 codec core 只暴露 5 个 SongCore API（AGENTS.md 第 3 条的机器证明）。
  口径注意：此 stage 是 s3 closure 的默认 -O2 + PIC，未开 -Os/LTO，因此
  949,016 与 530,664 不可解读为"PIC/动态库多了 400 KB"——两者 codegen 维度不同，
  S6 的价值在 API 面与符号可见性，不在与可执行形态比大小。

> 核心结论（round-2 clean-room 重新确认）：**源码裁剪主要优化的是构建闭包、
> 升级面和维护成本（.a −42%，TU −48%）；最终交付体积主要由 linker GC（52.8%）
> 与 size codegen（42.6%）决定，源码裁剪在 GC 条件下只额外贡献 4.6%（≈20 KB）。**
> "`.a` 有 2.66 MB 所以播放器至少大 2.66 MB" 不成立；"archive 减半所以
> binary 减半" 也不成立。

## S3 维度实验结论（round 1，一次一维，全部独立重求 closure；结论被 round-2 采用）

| 维度 | 结果 | 证据 |
|---|---|---|
| `--disable-iconv` | removable（−1,448 B） | gate identical |
| `--disable-pthreads` | removable（−36,604 B，TU 110→106） | gate identical（解码本就单线程） |
| `--disable-runtime_cpudetect` | no_effect（+0） | gate identical |
| `--disable-asm` | **rejected：PCM 改变** | 7/7 MP3 corpus case 与 SIMD kernel 不一致（浮点结合性），gate 在 corpus 等价步拒绝 |
| `--disable-pic` | no_effect（−80 B，噪声；保留 PIC 保障 shared 链接，S6 实测成立） | gate identical；S6 PIC archive 与非 PIC 字节一致 |
| `--disable-iamf` | no_effect（+0） | gate identical |

encode.c / mux / crypto / video-generic 的对象在 link 层已被排除；
`encode.o`（21 KB）因 decode.c 无条件交叉引用 `ff_encode_flush_buffers`
而被迫拉入——删除它需要修改 FFmpeg private implementation，触及 stop condition，不做。

## 对抗性审计结论（修订后）

1. data-dependent path：全 corpus（含 malformed 3 例）+ 真实歌曲 + EOF gate 全过；
2. 静态注册表：allcodecs/allformats/parsers 均在 pulled set，且被 data relocation 正确计入；
3. metadata/artwork：id3v2/id3v1/flac_picture/oggparsevorbis/exif/replaygain 全部 pulled，
   artwork corpus case 行为等价；limitation：SongCore contract 无 artwork 读取 API，
   artwork 只能以 attached-picture 排队路径间接验证；
4. malformed error path：corrupt-tail/truncated-header/flac-truncated 行为等价；
5. seek 独立验证：SongCore 级 seek probe（FLAC strict exact-suffix）+ bench 级 25/50/75%；
6. 无 --whole-archive/--start-group；模拟 = real ld（-Map 多重集硬相等）+
   内容级 ELF SHA256 相等；
7. probe pin 机器断言：`nm -u probe.o` == 恰好 5 个契约符号（probe 无 libc 引用）；
8. 符号解析语义与 GNU ld 一致：仅 global/weak/common/unique 定义可触发 pull；
9. 205 个 oracle object 均无 .init_array/.ctors/.preinit_array section（ctor 漏算不可能发生）；
10. gc-sections：.init_array 等为 GC root，且全部行为 gate 行为等价；
11. LTO gate 就在 LTO artifacts 上跑（bench 与 production 同 archive）；
12. gate 内置断言：verify 比对的 archive 字节数 == 体积表测量的 archive 字节数；
13. `ldd qn_pcm_dump`：仅 linux-vdso/libm/libc（pthreads 禁用后连 libpthread 都不依赖）；
14. 每阶段 `rm -rf build/xmake build/artifacts`；最终以 `rm -rf build` clean-room 复现；
15. 21 个生成 manifest 无机器绝对路径（importer portability 断言 + 全量扫描）；
16. TU 下降 = 精确 manifest 投影（position 1:1 断言），无 object 合并；
17. S6 导出符号断言（5 个带版本 `song_*` API；dynsym 定义项共 6 = 5 API +
    `QIANQIAN_1.0` 版本定义）+ consumer smoke：`.so` 形态不泄漏 FFmpeg 符号且功能可用。

## 性能

round-2 全梯子最低 xRT：762×（s5-Os-LTO mp3）；最高 ~1,180×。警告线 50×。
size-oriented codegen **不是免费的**：S0 MP3 最低 956× → s5-Os-LTO 762×，
本次运行最多牺牲约 20% decode throughput（FLAC 964×→876×，约 9%）；
但最差仍约 762× realtime，远高于 50× gate，本地播放无可感知影响。

## Provenance

`bench/provenance/source-minimization.json`（schema 2）：
round 1 = 迭代推导的实验记录（含被拒维度）；round 2 = clean-room 权威数字，
全部由 `gate.json`/`so.json` 机器派生。`bench/results/source-minimization/
{summary.json,ladder.md}` 由 `tools/minimize_summary.py` 生成，零手填。

## Stop condition

连续两轮（runtime-cpudetect、iamf、pic）增量为 0/噪声级；
encode.o 的进一步缩减需要维护 FFmpeg private fork。按规则停止。

## 复现

```bash
bash tools/minimize_cleanroom.sh   # rm -rf build 起，全阶梯 + summary
```
