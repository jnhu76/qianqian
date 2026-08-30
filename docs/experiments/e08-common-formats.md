# E08 — Common Formats: codec-cost ladder + Linux/Windows minimal artifacts

状态：`COMPLETE`（Linux ladder / Windows phase 全部 gate PASS；结构数字
（TU/字节）由 clean-room 逐位复现；xRT 为运行时测量，见性能小节的波动
说明。数字唯一 machine authority：
`bench/results/common-formats/{summary.json,windows.json,attribution.json,
e07-rerun-observations.json}`，ladder.md / PR_BODY.md 由其派生并带
provenance sha，`common_summary.py --check` 拒绝漂移。）

## 研究问题（issue #8）

> 让一个现代本地音乐播放器从 MP3+FLAC 扩展到
> MP3/FLAC/AAC/ALAC/WAV/Vorbis/Opus，究竟增加多少 source closure 和 shipping bytes？
> 同一 Common Formats SongCore 在 Linux x86_64 与 Windows x86_64 上分别最低多大？
> 哪种 codec/container 边际成本最高，值不值？

三个口径严格分开：**BUILD FOOTPRINT**（TU / .a）、**SHIPPING FOOTPRINT**
（.so / .dll，stripped、compressed）、**RUNTIME PERFORMANCE**（xRT）。
TU 口径再分三层：**oracle TU**（configure/Make 推导的全量闭包）、
**reachable TU**（link-reachability 投影闭包）、**actual compiled TU**
（Xmake 实际编译并被逐 TU 核对的集合）——三者不相等时不允许混用。

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

| Stage | Capability | Oracle TU | Reachable=Compiled TU | `-O3 .a` | `.so` stripped | Δ `.so` |
|---|---|---:|---:|---:|---:|---:|
| C0 | MP3+FLAC | 202 | 106 | 1.46 MiB | 539 KiB | — |
| C1 | +AAC | 237 | 157 | 2.69 MiB | 1.06 MiB | +548 KiB |
| C2 | +ALAC | 240 | 160 | 2.71 MiB | 1.07 MiB | +4 KiB |
| C3 | +WAV | 243 | 163 | 2.74 MiB | 1.08 MiB | +16 KiB |
| C4 | +Vorbis | 257 | 180 | 2.92 MiB | 1.16 MiB | +84 KiB |
| C5 | +Opus | 276 | 198 | 3.25 MiB | 1.28 MiB | +124 KiB |
| C6 | minimized | (198) | 198 | 3.25 MiB | 1.25 MiB (xz 492 KiB) | −36 KiB |

- C0→C6 合计：ΔTU +92、Δ`.a` +1833 KiB、Δshipping `.so`
  **+757,840 B（≈ 740.1 KiB）**（脚本从 summary.json 计算，禁止手抄）。
- C6 size-minimal `.so`（-Os+LTO+PIC+function/data sections+`--gc-sections`
  +version script）：stripped 1,309,520 B，stripped+xz **503,684 B**；
  dynsym 定义项恰为 5 个 `song_*` + 版本定义；`ldd` 仅 libc/libm。
- xRT（songcore-output）：**易漂移量，本文件不复制具体数字**——逐 run /
  逐 codegen（-O3/-Os/-Os+LTO）的权威表是机器生成的
  `bench/results/common-formats/ladder.md`（provenance sha 绑定
  summary.json）。结构化结论只有三条：全部观测高于 50× 实时警告线；
  `-Os` 相对 `-O3` 有可观 decode 代价（逐 codec 见 ladder.md 的 codegen
  对照表）；LTO 收回其中约一半。

## 能力增量归因（C0→C1，machine-derived，`attribution.json`）

方法：增量成员（stage pulled ∩ base 未 pulled）× 该成员 file-backed
section 字节（offset 解包 + `size -A`，DWARF/bss 剔除），族按 **manifest
源路径**归组——n9.0.1 的 raw ADTS demuxer 在 `libavformat/aacdec.c`，与
`libavcodec/aacdec.c`（AAC decoder）同名碰撞，因此禁止按 basename 归组。

| 族 | 增量字节 | 占比 |
|---|---:|---:|
| AAC decoder（libavcodec/aac*） | 296,788 | 33.7% |
| MOV/ISOM demux（libavformat/mov*, isom*） | 165,579 | 18.8% |
| raw ADTS demux（libavformat/aacdec.c + rawdec.c） | 1,603 | 0.2% |
| shared（container/dsp/util 支撑） | 417,283 | 47.4% |

**结论（修正先前措辞）**：数据不支持“MOV demuxer 是最大单项”。
可归因的最大单项是 **AAC decoder 族（33.7%）**；MOV/ISOM 仅 18.8%；
接近一半（47.4%）是与既有能力共享的支撑代码（increment 分摊，非 AAC 专属）。
严格的说法是：**AAC/M4A/ADTS 是最大边际 capability bundle（+548 KiB
shipping）**，其内部大头是 decoder 本体而非 container。

## Seek 契约（gate 拥有独立最小语义；calibrate 只负责观测）

| 契约 | 家族 | 最低要求 |
|---|---|---|
| STRICT | FLAC / ALAC / WAV / Vorbis | seek 成功 + bounded resume + PCM + clean EOF + **suffix 逐字节相等** |
| LAPPED | MP3 / AAC-M4A / Opus | seek 成功 + bounded resume + PCM + clean EOF；suffix 字节相等**不要求**（MP3 bit reservoir、AAC MDCT 50% overlap、CELT overlap-add 使 seek 后首帧含 lapping 差异，样本级相等物理不可能——诊断探针实证：Opus sequential 首帧 pts=-312 + skip_samples 裁 648 样本，seek 后无 skip side data） |
| UNSUPPORTED | raw ADTS | typed seek 失败可接受（上游 demuxer 无 seek 实现）；decode/EOF gate 仍须成立 |
| REGRESSION | E07 real songs | 记录 + 跨阶段比较（已知受损输入，E07 文档化） |

机器校准（`common_calibrate.py`）只观测家族 suffix-match 行为并钉入 corpus
manifest（clean-room `--check` 防漂移）；**gate 的三档最低语义独立于校准**，
"record" 不等于"任何结果都算过"。

**bounded resume 是硬 gate**（Linux `common_gate.seek_resume_check`，Windows
correctness 同一函数）：由 `sequential_frames − post_seek_frames` 推出
implied resume 点（不依赖 PCM 相等，LAPPED 下本就不可比），要求落在
target ± 2 个 codec 帧内（MP3 2304 / AAC 2048 / Opus 1920 样本）。冻结
corpus 的确定性观测包络：MP3 −2077..−1429、AAC +412..+820、Opus −234..−78
（demuxer 落 packet 边界 + priming/lapping 裁剪，帧几何决定量级）；"要 50%
却从 20% 播"类错误偏差在 10⁵ 样本量级，被该窗口拦下。STRICT 的偏差只记录
不 gate（suffix 逐字节相等已钉死 resume 点；Vorbis 页粒度可早数十 ms 落点）。

## 其他真实发现

1. **Opus 硬依赖 libswresample**（FFmpeg n9.0.1 configure：`Disabled
   opus_decoder because not all dependencies are satisfied: swresample`）。
   SongCore 契约仍不使用 swr；swresample 仅作为 Opus 的上游 link 依赖进入
   closure，其实际 pull 由 link audit 计量。resample 能力 out of scope
   （PRD §4/§9 已同步为"SongCore 不提供 SRC/rematrix"）。
2. **Opus preskip/end-trim 正确**：12s 编码解码恰为 576000 样本（nominal
   精确相等），无漂移。AAC 529408 vs 529200（+208，encoder padding 语义）。
3. u8/f64 WAV 的 packed U8/DBL→Float32 转换与 swresample **字节一致**
   （Python IEEE 精确换算对照验证）。

## Windows x86_64 结果（`bench/results/common-formats/windows.json`）

工具链：llvm-mingw（clang 23.1.0 / lld，UCRT），target triple
`x86_64-w64-mingw32`，从 WSL 交叉构建，全部二进制经 WSL interop 在真
Windows 上原生执行（无 Wine、无只编译不运行）。

```text
Windows capability intent
    ↓ cross configure/Make oracle（禁复用 Linux manifest）
279-TU oracle manifest
    ↓ Xmake full replay → 279-member archive
    ↓ COFF link-reachability（offset 级成员；重名 basename 不折叠）
    ↓ archive member ↔ manifest TU 机器映射（1:1 断言）
202-TU projected manifest
    ↓ Xmake clean rebuild（逐 TU 核对：恰 202 个编译）
    ↓ re-audit：202/202 pulled（fixpoint 一轮收敛）
    ↓ DLL(-Os) / DLL(-Os+LTO)，各自全套 PE gates + 原生正确性
```

| Metric | Linux x86_64 | Windows x86_64 |
|---|---:|---:|
| Oracle TU | 202 (C0) → 276 (C5) | 279 |
| Reachable (projected) TU | 198 | **202**（offset 级审计；旧的 name 折叠口径 192 少计了 9 组重名成员） |
| Actual compiled TU | 198 | 202（xmake 对象树逐 TU 核对） |
| Static archive (projected) | 3,412,572 B | 3,413,190 B |
| Shared artifact raw | `.so` 1,435,336 B | `.dll` 1,829,888 B |
| Shared artifact stripped | 1,309,520 B | 1,327,616 B（-Os）/ 1,307,136 B（-Os+LTO） |
| Stripped+xz（canonical minimum） | **503,684 B** | **546,260 B**（dll-lto；-Os 变体 552,244 B 亦全 gates PASS） |
| Exports | 恰 5 `song_*` | 恰 5 `song_*` |
| Runtime FFmpeg deps | 0（ldd 仅 libc/libm） | 0（kernel32 + api-ms-win-crt-* + bcrypt） |
| Unicode path | n/a (POSIX) | PASS（`CreateFileW` + `测试音乐\歌曲-你好世界.m4a` 全契约） |
| >2 GiB seek | PASS | PASS（虚拟 3 GiB WAV；max seek offset 2,952,790,060，无负回绕） |
| Corpus | 每 stage 全 applicable gate | 33 clean + 11 degraded = **44/44 executed，skipped 0** |

- **Reachability 证据链**：offset 级 COFF 成员解析（llvm-nm 语义模拟，
  `utils.c.obj`×3、`pcm.c.obj`×2 等 9 组重名不再折叠）→ 真实 lld 链接的
  **reduced-archive 内容相等证明**（full vs 只含 pulled 成员的 archive，
  strip-debug 后 .text/.rdata/.data/.pdata 的 VMA/size/SHA 逐段相等）
  → lld `-Map` 成员集合佐证（限定在 FFmpeg manifest 全集内比较）。
- **DLL 只从 projected closure 构建**：flags 投影以 projected manifest 为
  基（无 full-manifest 回退路径），两个变体（-Os、-Os+LTO）各自 clean
  rebuild + 逐 TU 核对 + export/import gate + 原生正确性；
  canonical shipping = 两变体中 stripped+xz 最小者（本轮 dll-lto）。
- LTO 变体的 reachability 证明沿用 -Os 闭包（同一 projected manifest，
  符号图相同）+ 原生正确性/PE gates；map 级拉取对 bitcode 无意义，不冒充。
- Xmake mingw replay 需显式 pin x86_64 工具（SDK autodetect 会选中
  `arm64ec-w64-mingw32uwp-gcc`，其拒绝 FFmpeg 内联 asm）；`--av_manifest`
  必须传项目相对路径（绝对值在 build 时静默回落默认值）；切换平台前须清
  `.xmake`（sticky config session）。
- `av_random_bytes` 在 Windows 引入 `bcrypt` 系统库依赖（已记录，非 FFmpeg DLL）。

## Windows degraded corpus（11/11 原生执行，typed 分类，两次运行一致）

| 分类 | 案例 | 行为 |
|---|---|---|
| OPEN_FAILED | mp3-truncated-header, wav-malformed-header, opus-malformed-header | song_open 拒绝 |
| DECODE_ERROR | aac-malformed-header(3072f), aac-truncated(314368f), alac-truncated, flac-truncated, wav-truncated | bounded 解码后 typed 失败，帧数 ≥ corpus floor |
| DEGRADED_EOF | mp3-corrupt-tail(132527f), vorbis-truncated, opus-truncated | 有界输出至 EOF |

gate：无 crash / 无 hang（timeout）/ 界内帧数 / 两次分类与帧数完全一致。
malformed 案在 Linux 生产路径同样执行 typed 门（OPEN_OR_PROBE_FAILED 或
DECODE_ERROR_AFTER_OUTPUT，CLEAN_DECODE 或不确定 → FAIL）。

## 跨平台 PCM 策略与实测

- Lossless（FLAC/ALAC/WAV）：Windows SongCore PCM 与 canonical sha
  **字节相等**（硬 gate）。
- Lossy：Windows（clang/COFF）对 Linux（gcc/ELF）逐案 sha 对比：**26/33
  完全一致**（含全部 MP3/Vorbis/Opus/WAV）；7 个 AAC 用例不一致——同码本
  不同编译器对 AAC 浮点内核的 codegen 差异。按 issue #11 规则记录
  deterministic tolerance metric（`windows.json.aac_cross_compiler_tolerance`）：
  帧数逐案相等；受影响样本 ≤ 66/356352（0.019%）；**max|Δ| = 5.96e-08
  （恰好 1 ULP @0.5）**，mean|Δ| ≈ 1.0e-08（≈ −140 dB，听感不可辨）。
  不放宽任何 gate：同平台内（Windows oracle vs Windows replay、DLL consumer
  前后、跨 stage）仍要求逐字节确定。

## E07 provenance

E07 的 `bench/results/source-minimization/` 为 frozen historical experiment，
本轮**未改动**（已恢复 main 权威版本）。E08 机器状态下的重跑观测（xRT
偏移、LTO archive 字节非逐位一致等）单独记录于
`bench/results/common-formats/e07-rerun-observations.json`，不覆盖历史。
E08 自身 ladder 的结构数字（TU/字节）在 clean-room 重跑间逐位复现。

## 复现

```bash
bash tools/common_cleanroom.sh            # Linux：rm -rf build 起全阶梯 + 归因 + summary + --check
python3 tools/common_windows.py --all     # Windows：需 LLVM_MINGW_SDK（或 ~/toolchains/llvm-mingw）
python3 tools/common_windows_summary.py
python3 tools/common_cross_tolerance.py [--linux-exe <preserved Linux qn_pcm_dump>]
python3 tools/common_summary.py && python3 tools/common_summary.py --check
```

Corpus：`python3 corpus/tools/gen_corpus_common.py`（29 fixtures，确定性
合成）；seek 语义由 `tools/common_calibrate.py` 机器观测钉定并在 clean-room
`--check`；ladder.md / PR_BODY.md 与 summary.json 的派生一致性由
`common_summary.py --check` 把关。

## Stop condition

连续维度（runtime-cpudetect / iamf / pic 等 E07 已收）增量 0；本轮新增
closure 全部为 capability intent 直译，无手工删减需求；encode.o 类的进一步
缩减仍需 FFmpeg private fork，不做。

## 对抗性审计（issue #8 §35，review 后重审）

| # | 检查 | 结论 | 机器证据 |
|---|---|---|---|
| 1 | 偷链系统 FFmpeg？ | 无 | Linux `ldd` 断言（common_so）+ Windows PE import 表 gate |
| 2 | Linux/Windows manifest 独立？ | 是 | Windows 独立 cross configure（args 含 `--target-os=mingw32`，CC=clang wrapper，279≠276 TU） |
| 3 | whole-archive 误用？ | 无 | 全部链接无 `--whole-archive`/`--start-group` |
| 4 | solver 与真实 linker 一致？ | 是 | Linux：GNU ld `-Map` pulled-member 多重集硬相等；Windows：reduced-archive PE 内容相等（load-bearing section SHA）+ lld `-Map` 佐证 |
| 5 | data-driven 漏算？ | 无 | allcodecs/allformats/parsers/codec_list 均在 pulled 集合；reduced-archive 链接成功且内容相等含 data relocation 路径 |
| 6 | LTO archive 冒充 shipping？ | 否 | LTO `.a` 仅记录；shipping 一律 stripped `.so`/`.dll`；canonical 由 summary 按 stripped+xz 选择 |
| 7 | strip 改变功能？ | 否 | 功能 smoke 全部跑在未 strip 产物；strip 仅用于测量（临时副本） |
| 8 | `.so`/`.dll` 泄漏 FFmpeg 符号？ | 无 | Linux version script：dynsym 定义项恰 5 API+版本定义；Windows `.def` export table 恰 5 项（objdump 机器解析） |
| 9 | Windows import 表含 FFmpeg DLL？ | 无 | objdump `-p` 全量记录 + FORBIDDEN_DLL 正则硬 gate（仅 kernel32/api-ms-win-crt-*/bcrypt） |
| 10 | Windows 二进制真跑过？ | 是 | WSL interop = Windows loader 原生执行；44/44 corpus + unicode + largefile 均为 DLL consumer 原生运行 |
| 11 | Unicode 走 UTF-16 Host path？ | 是 | `CreateFileW` + 源内宽字面量目标路径；窄路径仅在 host 侧 UTF-8→wide 转换，SongCore 不见窄路径 |
| 12 | >2 GiB 真实触达？ | 是 | host 计数器记录 max seek offset 2,952,790,060（>2^31），negative_seek=false；synthetic virtual IO |
| 13 | 旧 MP3/FLAC gate 降级？ | 无 | stage-a 15 案在每个 stage 的 applicable corpus 内；c0→cN 行为比较全部 IDENTICAL；E07 strict FLAC probe 保留（STRICT 契约） |
| 14 | malformed 只测"没 crash"？ | **否（已修复）** | degraded 案 11/11 原生执行：typed 分类（OPEN_FAILED/DECODE_ERROR/DEGRADED_EOF）+ 帧数 floor + 两次运行一致 + 无 crash/hang；review 前的"33 clean PASS"口径已纠正为 44/44 |
| 15 | AAC/Opus delay/end-trim 正确？ | 是 | 钉定断言：opus 恰 576000 样本；AAC +208 encoder padding；preskip 诊断探针实证 |
| 16 | lossless 严格 reference？ | 是 | FLAC/ALAC/WAV strict canonical sha 双平台硬 gate（Windows 也字节相等） |
| 17 | `.a` 与 `.so`/`.dll` 混淆？ | 否 | BUILD/SHIPPING/RUNTIME 三口径分列；TU 再分 oracle/reachable/compiled 三层 |
| 18 | full vs minimized shipping delta 记录？ | 是 | C5→C6：`.so` 1.28→1.25 MiB（−36 KiB）；Windows 279→202 TU projection 单列 |
| 19 | `-Os/LTO` throughput 诚实报告？ | 是 | 逐 codec codegen 表；绝对 xRT 给区间、相对代价为结论；机器 authority 以 summary.json 当前值为准 |
| 20 | 为几十 KiB patch FFmpeg？ | 无 | FFmpeg 源码零修改；所有裁剪为 configure 维度 + 链接投影 |

### 过程中发现并修复的缺陷（诚实记录）

1. **stale manifest 污染**：c0/c1/c2 曾复用 profile 修正前的 import（110 vs
   106 TU）——`--force` 重导 + 全阶段重跑修复。
2. **host 契约 tail-sha bug**：Windows correctness 首版把"head 解码后残余流"
   的 sha 当全量 reference——三方 sha 对比定位为 host bug，重构为
   fresh-handle 全量解码。
3. **mono 别名**：64 帧内容搜索在短周期 mono 正弦上假匹配——修复为
   "候选点全后缀验证"。
4. **arm64ec 误选**：xmake SDK autodetect 选 `arm64ec-w64-mingw32uwp-gcc`
   拒编译内联 asm——显式 pin x86_64 wrapper。
5. **Windows "192 reachable TU" 是 name 折叠口径（review 指出）**：llvm-nm
   按 basename 折叠了 9 组重名成员。offset 级 COFF 解析 + member↔manifest
   1:1 映射后的真实数字为 **202**；并补完投影管线（projected manifest →
   clean rebuild 逐 TU 核对 → fixpoint re-audit → DLL 仅从投影闭包构建）。
6. **c5 profile 矛盾**：swresample 同时出现在 enable/disable（靠 configure
   参数顺序侥幸成立）——已消除并加 profile 校验 fail-fast。
7. **E07 历史结果曾被本轮重跑覆盖（review 指出）**——已恢复 main 权威版本，
   重跑观测单独存档于 e07-rerun-observations.json。
8. **数字三方漂移（review 指出）**：PR body 手抄 372×/711 KiB 与机器 JSON
   不一致——summary.json 成为唯一 authority，ladder.md/PR_BODY.md 为其派生
   函数（含 provenance sha），`--check` 拒绝漂移。
9. **"MOV demuxer 为最大单项"是未归因的措辞（review 指出）**——补
   machine attribution 后修正：AAC decoder 33.7% > MOV/ISOM 18.8%。
10. **xmake 平台切换坑**：`--av_manifest` 绝对路径在 build 时静默回落默认
    （须传项目相对路径）；`.xmake` config session 粘滞（切平台前须清除）。
    二者均以 persist-verify + 清理修复。
