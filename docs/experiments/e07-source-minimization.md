# E07 — Source/link minimization of the Qianqian FFmpeg core (Linux Native)

状态：`COMPLETE`（clean-room 复现通过；全部 gate 与 S0 基线 byte-identical）

## 研究问题

对当前 SongCore capability（MP3/FLAC demux+decode、probe、metadata/artwork 内部路径、
streaming Float32 PCM、seek、EOF/error handling），进入 `libqianqian_av.a` 的
FFmpeg source/object closure 最小是多少？`.a` 最小与 shipped linked core 最小分别是什么？

## 方法

按仓库第一原则，禁止按文件名猜删。整条梯子全部机器推导：

1. **S0 冻结基线** — canonical `n3-min-noswr` import 重放，205 TU / 2,658,174 B。
2. **S1 link reachability audit**（`tools/link_audit.py`）—
   `tools/songcore_link_probe.c` 强制引用完整 public contract
   （`song_open/song_probe/song_read_pcm/song_seek/song_close`），
   逐 member 解析 `libqianqian_av.a`（位置对齐 manifest，消除重名歧义），
   模拟 GNU ld archive resolution，得到 pulled set；
   用 "reduced archive 重链接 vs 全量链接 ELF 等价" 证明模拟正确；
   `-Wl,-Map` 作为旁证（name 集合完全一致）。
   结果 110/205 members 被拉入。对候选 archive 再审计 → 110/110 fixpoint。
3. **S3 configure 维度实验**（`tools/config_experiment.py`，一次一个维度，
   每个变体从 upstream configure/Make **重新求 closure**，不复制旧列表）：
   iconv、pthreads、runtime-cpudetect、asm、pic、iamf。
4. **S4 section GC**（`tools/minimize_flags.py` + `--gc_sections`）。
5. **S5 codegen**（-O2/-Os/-flto 替换，manifest flag mutation）。

每个候选阶段都跑完整 gate（`tools/minimize_gate.py`）：

- 15/15 Stage-A corpus：oracle vs candidate 行为等价（`verify_xmake_core.py`，
  含 `qn_bench correct` 的全文件解码 + 25/50/75% seek + suffix 校验 + EOF）；
- SongCore PCM canonical hash（MP3 + FLAC，与基线 byte-identical）；
- SongCore 级 seek/EOF（`tools/songcore_seek_probe.c`，生产路径）：
  FLAC = strict（suffix exact + clean EOF），MP3 = record 对比基线；
- 三个真实歌曲：qn_pcm_dump 全量解码（rc/frames/sha）+ SongCore seek probe；
- xRT throughput（真实歌曲，`qn_bench bench`）；
- 体积表（archive members/symbols/xz、linked、linked xz）。

gate 语义沿用 corpus 既有契约：FLAC seek `strict`，MP3 seek `record`
（MP3 bit reservoir 使 seek 后内容与顺序流天然不同，只做确定性对比）。

## 发现的真实基线问题

**真实 FLAC（flac-16-44-artwork.flac）在 S0 全量解码时，最后帧报
`invalid frame header`，qn_pcm_dump rc=1**——224.08 s 音频全部解出后才在文件末尾
失败。此前 audible smoke 用 `--seconds` 采样播放，从未触达文件尾部。
本实验将其作为 S0 基线行为记录（`real_song_pcm.exit_code=1` + 固定 sha），
所有候选阶段必须保持完全一致，未做任何"修复"。

## Ladder（`tools/minimize_cleanroom.sh` 从 `rm -rf build` 复现的数据）

| Stage | 变化 | TU | `.a` bytes | linked bytes | MP3 xRT(min) | FLAC xRT | Corpus | PCM | Seek |
|---|---|---:|---:|---:|---:|---:|---|---|---|
| S0 | canonical 基线 | 205 | 2,658,174 | 973,048 | 941 | 903 | PASS | PASS | PASS |
| S1 | link-reachability 投影 | 110 | 1,574,932 | 973,048 | — | — | PASS | PASS | PASS |
| S3 | --disable-iconv + --disable-pthreads（重投影） | 106 | 1,535,360 | 952,568 | 965 | 924 | PASS | PASS | PASS |
| S4 | + -ffunction-sections -fdata-sections + --gc-sections | 106 | ~1.80 MB | 719,080 | — | — | PASS | PASS | PASS |
| S5 | -Os -flto + --gc-sections | 106 | 3,927,088 | **530,664** | 755 | 866 | PASS | PASS | PASS |

（S1/S4 行取自逐步实验轮；S0/S3/S5 行为 clean-room 复现值。逐阶段 xRT 波动 ~±5%，
全部 ≥ 50× 警戒线两个数量级。S5 的 `.a` 膨胀是 LTO bytecode，不是 archive 最小化工具。）

## 两个答案（任务要求分别给出）

**A. Minimal archive**（对象是 `.a` 本身）：
- 纯 closure 最小（保持 -O3）：**1,535,360 B / 106 TU**（clean-room）
- 叠加 `-Os` codegen：**1,197,464 B / 106 TU**（逐步实验轮；xRT 745–920）
- LTO archive（3.9 MB，bytecode）不是 archive 最小化的正确工具。

**B. Minimal shipped codec core**（SongCore + 实际拉入的 FFmpeg 代码）：
- `-Os -flto` 编译 + `--gc-sections` 链接：**530,664 B**（S0 973,048 → −45%；clean-room）
- 仅 `-Os + --gc-sections`（不用 LTO）：563,432 B
- 参照：**全量 205-TU closure + fdata + gc 链接 = 739,568 B**——
  即对 linked binary，section GC 一项就拿走 source 最小化绝大部分收益；
  source 级最小化的价值在 `.a` 本身（−42%）、编译时间（106 vs 205 TU）
  与维护面，而非最终二进制。

> 结论：**"`.a` 有 2.66 MB，所以播放器至少增大 2.66 MB" 不成立**；
> 反过来 "archive 减半所以 binary 减半" 也不成立。

## S3 维度实验结论（一次一维，全部独立重求 closure）

| 维度 | 结果 | 证据 |
|---|---|---|
| `--disable-iconv` | removable（−1,448 B） | gate identical |
| `--disable-pthreads` | removable（−36,604 B，TU 110→106） | gate identical（解码本就单线程） |
| `--disable-runtime_cpudetect` | no_effect（+0） | gate identical |
| `--disable-asm` | **rejected：PCM 改变** | 7/7 MP3 corpus case 与 SIMD kernel 不一致（浮点结合性），gate 在 corpus 等价步拒绝 |
| `--disable-pic` | no_effect（−80 B，噪声；保留 PIC 保障未来 shared 链接） | gate identical |
| `--disable-iamf` | no_effect（+0） | gate identical |

encode.c / mux / crypto / video-generic 的对象在 link 层已被排除；
`encode.o`（21 KB）因 decode.c 无条件交叉引用 `ff_encode_flush_buffers`
而被迫拉入——删除它需要修改 FFmpeg private implementation，触及 stop condition，不做。

## 对抗性审计结论

1. data-dependent path：全 corpus（含 malformed 3 例）+ 真实歌曲 + EOF gate 全过；
2. 静态注册表：allcodecs/allformats/parsers 均在 pulled set，且被 data relocation 正确计入；
3. metadata/artwork：id3v2/id3v1/flac_picture/oggparsevorbis/exif/replaygain 全部 pulled，
   artwork corpus case 行为等价；limitation：SongCore contract 无 artwork 读取 API，
   artwork 只能以 attached-picture 排队路径间接验证；
4. malformed error path：corrupt-tail/truncated-header/flac-truncated 行为等价；
5. seek 独立验证：SongCore 级 seek probe（FLAC strict exact-suffix）+ bench 级 25/50/75%；
6. 无 --whole-archive/--start-group；模拟结果以 reduced-archive 重链接 ELF 等价证明；
7. 205 个 oracle object 均无 .init_array/.ctors/.preinit_array section（ctor 漏算不可能发生）；
8. gc-sections：.init_array 等为 GC root，且全部行为 gate byte-identical；
9. LTO gate 就在 LTO artifacts 上跑（bench 与 production 同 archive）；
10. gate 内置断言：verify 比对的 archive 字节数 == 体积表测量的 archive 字节数；
11. `ldd qn_pcm_dump`：仅 linux-vdso/libm/libc（pthreads 禁用后连 libpthread 都不依赖）；
12. 每阶段 `rm -rf build/xmake build/artifacts`；最终以 `rm -rf build` clean-room 复现；
13. 21 个生成 manifest 无机器绝对路径（importer 的 portability 断言 + 全量扫描）；
14. TU 下降 = 精确 manifest 投影（position 1:1 断言），无 object 合并；
15. `tools/minimize_cleanroom.sh`：从 `rm -rf build` 一键复现 S0 → 最小 archive →
    最小 linked 三个 finals，每步完整 gate。

## 性能

全梯子最低 xRT：653×（mp3-cbr-320-artwork，Os+LTO）；最高 ~1,180×。
警告线 50×。任何阶段都未接近。

## Stop condition

连续两轮（runtime-cpudetect、iamf、pic）增量为 0/噪声级；
encode.o 的进一步缩减需要维护 FFmpeg private fork。按规则停止。

## 复现

```bash
bash tools/minimize_cleanroom.sh
```
