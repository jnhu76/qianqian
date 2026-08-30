# E08 — Common Formats: codec-cost ladder + Linux/Windows minimal artifacts

状态：`COMPLETE`（Linux ladder / Windows phase 全部 gate PASS；结构数字
（TU/字节）已由 clean-room 重验逐位复现；xRT 为运行时测量，见性能小节的
波动说明）。

## 研究问题（issue #8）

> 让一个现代本地音乐播放器从 MP3+FLAC 扩展到
> MP3/FLAC/AAC/ALAC/WAV/Vorbis/Opus，究竟增加多少 source closure 和 shipping bytes？
> 同一 Common Formats SongCore 在 Linux x86_64 与 Windows x86_64 上分别最低多大？
> 哪种 codec/container 边际成本最高，值不值？

三个口径严格分开：**BUILD FOOTPRINT**（TU / .a）、**SHIPPING FOOTPRINT**
（.so / .dll，stripped、compressed）、**RUNTIME PERFORMANCE**（xRT）。

## 能力阶梯

每级从 capability intent 重新经 upstream configure/Make oracle 求全量
closure（禁止在旧 closure 上手工追加文件），再经 real-ld link reachability
投影为 reachable closure，clean rebuild 后过全量 behavior gate：

```text
C0  MP3+FLAC                    (E07 冻结基线：--disable-iconv/--disable-pthreads 组合)
C1  +AAC/M4A + raw ADTS AAC     (首付 MOV/MP4 container + AAC decoder)
C2  +ALAC/M4A                   (隔离 ALAC decoder 边际)
C3  +PCM WAV (u8/s16/s24/s32/f32/f64)
C4  +Ogg Vorbis                 (首付 Ogg container + Vorbis decoder)
C5  +Ogg Opus                   (隔离 Opus decoder 边际)
C6  Common Formats minimized    (-Os / -Os+LTO / size-minimal .so ladder)
```

## Linux x86_64 结果（machine-generated，`bench/results/common-formats/ladder.md`）

| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `.so` stripped | Δ `.so` |
|---|---|---:|---:|---:|---:|---:|
| C0 | MP3+FLAC | 202 | 106 | 1.46 MiB | 539 KiB | — |
| C1 | +AAC | 237 | 157 | 2.69 MiB | 1.06 MiB | +548 KiB |
| C2 | +ALAC | 240 | 160 | 2.71 MiB | 1.07 MiB | +4 KiB |
| C3 | +WAV | 243 | 163 | 2.74 MiB | 1.08 MiB | +16 KiB |
| C4 | +Vorbis | 257 | 180 | 2.92 MiB | 1.16 MiB | +84 KiB |
| C5 | +Opus | 276 | 198 | 3.25 MiB | 1.28 MiB | +124 KiB |
| C6 | minimized | — | 198 | 3.25 MiB | 1.25 MiB (xz 492 KiB) | −36 KiB |

- C6 size-minimal `.so`（-Os+LTO+PIC+function/data sections+`--gc-sections`
  +version script）：stripped 1.25 MiB，stripped+xz **492 KiB**；
  dynsym 定义项恰为 5 个 `song_*` + 版本定义；`ldd` 仅 libc/libm。
- xRT（songcore-output，clean-room 权威 run 全阶梯最低）：C0 468× →
  C6 259×（-Os+LTO；警告线 50×）。`-Os` 的诚实代价（相对比例跨 run 稳定）：
  MP3 1862× → 632×（−66%）、AAC 1699× → 695×、ALAC 487× → 200×、
  Opus 498× → 229×；LTO 收回约一半（MP3 1163×、AAC 1039×）。
  运行间绝对值波动可观（前一轮 unloaded run 最低 372×，本轮持续满载
  259×——共享开发机热节流），故绝对 xRT 给出区间、相对代价为结论：
  最差码点仍 ≥ 4× 于 50× 线。

## 边际成本结论（increment 视角）

| 能力 | Δ TU | Δ `-O3 .a` | Δ shipping `.so` | 评注 |
|---|---:|---:|---:|---|
| AAC/M4A+ADTS | +51 | +1256 KiB | **+548 KiB** | MOV demuxer（isom 系）是最大单项；AAC 解码器本体其次 |
| ALAC | +3 | +19 KiB | +4 KiB | 几乎免费；与 AAC 共享 MOV container |
| PCM WAV ×6 | +3 | +35 KiB | +16 KiB | 全部 PCM 解码器共享一个 `pcm.c` TU |
| Vorbis | +17 | +177 KiB | +84 KiB | 首付 Ogg demuxer + Vorbis |
| Opus | +18 | +347 KiB | +124 KiB | **含上游强制的 libswresample**（见下） |

**真实发现（非预期项，均已机器证据）：**

1. **Opus 硬依赖 libswresample**（FFmpeg n9.0.1 configure：`Disabled
   opus_decoder because not all dependencies are satisfied: swresample`）。
   SongCore 契约仍不使用 swr；swresample 仅作为 Opus 的上游 link 依赖进入
   closure，其实际 pull 由 link audit 计量。resample 能力仍 out of scope。
2. **raw ADTS 无 seek**：FFmpeg 的 aac demuxer 未实现 seek
   （`av_seek_frame` 失败，gate 记录为 typed finding）。
3. **seek 语义按格式家族校准**（`tools/common_calibrate.py`，bench 级
   25/50/75% suffix-match 观测写入 corpus manifest，clean-room `--check` 防
   漂移）：WAV/ALAC/Vorbis/FLAC strict；AAC/Opus record——AAC-LC MDCT 50%
   帧间 overlap 与 CELT 帧间 overlap-add 使 seek 后首帧含 lapping 差异，
   样本级 suffix 相等物理不可能（诊断探针实证：sequential 首帧
   pts=-312+skip_samples 裁剪 648 样本；seek 后无 skip side data、整帧输出）。
4. **Opus preskip/end-trim 正确**：12s 编码解码恰为 576000 样本（nominal
   精确相等），无漂移。
5. u8/f64 WAV 的 packed U8/DBL→Float32 转换与 swresample **字节一致**
   （Python IEEE 精确换算对照验证）。

## Windows x86_64 结果（`bench/results/common-formats/windows.json`）

工具链：**llvm-mingw 20260826**（clang 23.1.0 / lld，UCRT），target triple
`x86_64-w64-mingw32`，从 WSL 交叉构建，全部二进制经 WSL interop 在真
Windows 上原生执行（无 Wine、无只编译不运行）。

| Metric | Linux x86_64 | Windows x86_64 |
|---|---:|---:|
| Oracle TU | 276 | 279 |
| Reachable TU | 198 | 192 |
| Static archive (stock -O3) | 3.25 MiB | 3.87 MiB |
| Shared core stripped | `.so` 1.25 MiB | `.dll` 1.27 MiB（LTO 1.27→1.25 MiB） |
| Shared core stripped+xz | 492 KiB | 539 KiB（LTO 533 KiB） |
| Import library | — (ELF 无) | 2.2 KiB（dev-only，不计 shipping） |
| Exported APIs | 5 | 5 |
| External FFmpeg runtime dep | 0 | 0（仅 kernel32 + api-ms-win-crt-* + bcrypt） |
| Unicode path | n/a (POSIX) | PASS（`测试音乐\歌曲-你好世界.m4a` 宽字符全契约） |
| >2 GiB seek | PASS | PASS（虚拟 3 GiB WAV，max seek offset 2.95 GiB，无负回绕） |
| Corpus (44 applicable cases) | PASS | PASS |

- Windows closure 独立推导：cross configure
  `--target-os=mingw32 --arch=x86_64 --cross-prefix=x86_64-w64-mingw32-`；
  禁止复用 Linux manifest。
- Link audit：llvm-nm COFF 语义模拟 + **真实 lld `-Map` 成员集合硬相等**
  （192/266 members pulled）。
- Xmake mingw replay 需显式 pin x86_64 工具：llvm-mingw SDK 含多个 target
  wrapper，autodetect 会选中 `arm64ec-w64-mingw32uwp-gcc`（其拒绝 FFmpeg
  内联 asm，`invalid input constraint 'c'`）。
- `av_random_bytes` 在 Windows 引入 `bcrypt` 系统库依赖（已记录，非 FFmpeg DLL）。

## 跨平台 PCM 策略与实测

- Lossless（FLAC/ALAC/WAV）：Windows SongCore PCM 与 canonical sha
  **字节相等**（硬 gate）。
- Lossy：Windows（clang/COFF）对 Linux（gcc/ELF）逐案 sha 对比：**26/33
  完全一致**（含全部 MP3/Vorbis/Opus/WAV…）；7 个 AAC 用例不一致——同码本
  不同编译器对 AAC 浮点内核的 codegen 差异。按 issue #11 规则记录
  deterministic tolerance metric（`windows.json.aac_cross_compiler_tolerance`）：
  帧数逐案相等；受影响样本 ≤ 66/356352（0.019%）；**max|Δ| = 5.96e-08
  （恰好 1 ULP @0.5）**，mean|Δ| ≈ 1.0e-08（≈ −140 dB，听感不可辨）。
  不放宽任何 gate：同平台内（Windows oracle vs Windows replay、DLL consumer
  前后、跨 stage）仍要求逐字节确定。

## 复现

```bash
bash tools/common_cleanroom.sh            # Linux：rm -rf build 起全阶梯 + summary
python3 tools/common_windows.py --all     # Windows：需 LLVM_MINGW_SDK（或 ~/toolchains/llvm-mingw）
python3 tools/common_windows_summary.py   # Windows 结果汇总
```

Corpus：`python3 corpus/tools/gen_corpus_common.py`（29 fixtures，确定性
合成）；seek 语义由 `tools/common_calibrate.py` 机器钉定并在 clean-room 中
`--check`。

## Stop condition

连续维度（runtime-cpudetect / iamf / pic 等 E07 已收）增量 0；本轮新增
closure 全部为 capability intent 直译，无手工删减需求；encode.o 类的进一步
缩减仍需 FFmpeg private fork，不做。

## 对抗性审计（issue #8 §35，20 项）

| # | 检查 | 结论 | 机器证据 |
|---|---|---|---|
| 1 | 偷链系统 FFmpeg？ | 无 | Linux `ldd` 断言（common_so）+ Windows PE import 表 gate |
| 2 | Linux/Windows manifest 独立？ | 是 | Windows 独立 cross configure（args 含 `--target-os=mingw32`，CC=clang wrapper，279≠276 TU） |
| 3 | whole-archive 误用？ | 无 | 全部链接无 `--whole-archive`/`--start-group` |
| 4 | solver 与真实 linker 一致？ | 是 | Linux：GNU ld `-Map` pulled-member 多重集硬相等；Windows：lld `-Map` 成员集合硬相等（192/266） |
| 5 | data-driven 漏算？ | 无 | allcodecs/allformats/parsers/codec_list 均在 pulled 集合；map 等价已含 data relocation 路径 |
| 6 | LTO archive 冒充 shipping？ | 否 | LTO `.a`（bytecode，5.4 MB）仅记录；shipping 一律 stripped `.so`/`.dll` |
| 7 | strip 改变功能？ | 否 | 功能 smoke 全部跑在未 strip 产物；strip 仅用于测量（临时副本） |
| 8 | `.so`/`.dll` 泄漏 FFmpeg 符号？ | 无 | Linux version script：dynsym 定义项恰 5 API+版本定义；Windows `.def` export table 恰 5 项（objdump 机器解析） |
| 9 | Windows import 表含 FFmpeg DLL？ | 无 | objdump `-p` 全量记录 + FORBIDDEN_DLL 正则硬 gate（仅 kernel32/api-ms-win-crt-*/bcrypt） |
| 10 | Windows 二进制真跑过？ | 是 | WSL interop = Windows loader 原生执行；correctness PASS 由 DLL consumer 产生（qn_pcm_dump.exe 亦原生跑） |
| 11 | Unicode 走 UTF-16 Host path？ | 是 | `CreateFileW` + 源内宽字面量目标路径 `测试音乐\歌曲-你好世界.m4a`；窄路径仅在 host 侧 UTF-8→wide 转换，SongCore 不见窄路径 |
| 12 | >2 GiB 真实触达？ | 是 | host 计数器记录 max seek offset **2,952,790,060**（>2^31），negative_seek=false；issue 许可的 synthetic virtual IO |
| 13 | 旧 MP3/FLAC gate 降级？ | 无 | stage-a 15 案在每个 stage 的 applicable corpus 内；c0→cN 行为比较全部 IDENTICAL；E07 strict FLAC probe 保留 |
| 14 | malformed 只测"没 crash"？ | 部分 | degraded 案记录 typed 行为（open_failed/eof/error_or_eof/stderr tail）且跨 stage 字节级比较；错误**分类**强度仍弱于正常案（已知限制） |
| 15 | AAC/Opus delay/end-trim 正确？ | 是 | 钉定断言：opus 恰 576000 样本（nominal 相等）；AAC 529408 vs 529200（+208，encoder padding 语义）；preskip 诊断探针实证 |
| 16 | lossless 严格 reference？ | 是 | FLAC/ALAC/WAV strict canonical sha 双平台硬 gate（Windows 也字节相等） |
| 17 | `.a` 与 `.so`/`.dll` 混淆？ | 否 | BUILD/SHIPPING/RUNTIME 三口径分列（ladder.md / windows.json / 本文档） |
| 18 | full vs minimized shipping delta 记录？ | 是 | C5→C6：`.so` 1.28→1.25 MiB（−36 KiB）；linked exe 2.15→1.26 MiB |
| 19 | `-Os/LTO` throughput 诚实报告？ | 是 | 逐 codec codegen 表：MP3 −57%（-Os）、ALAC −36%；未以"噪声"掩盖 |
| 20 | 为几十 KiB patch FFmpeg？ | 无 | FFmpeg 源码零修改；所有裁剪为 configure 维度 + 链接投影 |

### 过程中发现并修复的缺陷（诚实记录）

1. **stale manifest 污染**：c0/c1/c2 曾复用 profile 修正前的 import（多拉 4 个
   pthread 成员，110 vs 106 TU）——由 `--force` 重导 + 全阶段重跑修复；教训已
   写入 common_import 的 skip 语义。
2. **host 契约 tail-sha bug**：Windows correctness 首版把"head 解码后残余流"
   的 sha 当全量 reference——三方 sha 对比（Windows QPCM vs manifest vs
   ffmpeg f32le）定位为 host bug 而非解码差异，重构为 fresh-handle 全量解码。
3. **mono 别名**：64 帧内容搜索在短周期 mono 正弦上出现假匹配——修复为
   "候选点全后缀验证"（Linux probe 同步加固，严格性更强）。
4. **arm64ec 误选**：xmake SDK autodetect 选 `arm64ec-w64-mingw32uwp-gcc`
   导致内联 asm 拒编译——显式 pin x86_64 wrapper 并记录。
