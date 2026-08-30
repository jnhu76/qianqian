## Capability

```text
MP3 / FLAC          (E07 frozen baseline)
AAC-LC / M4A        new
raw ADTS AAC        new
ALAC / M4A          new
PCM WAV             u8 / s16le / s24le / s32le / f32le / f64le    new
Ogg Vorbis          new
Ogg Opus / .opus    new
```

能力按 container+codec 定义（非扩展名）；每级 closure 由 capability intent 经
pinned FFmpeg n9.0.1 的 configure/Make oracle 机器推导，禁止手工追加文件。

## Codec-cost ladder（Linux x86_64，machine-generated）

| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `.so` stripped | Δ `.so` | min xRT | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---|
| C0 | MP3+FLAC | 202 | 106 | 1.46 MiB | 539 KiB | — | 548× | PASS |
| C1 | +AAC/M4A+ADTS | 237 | 157 | 2.69 MiB | 1.06 MiB | +548 KiB | 531× | PASS |
| C2 | +ALAC | 240 | 160 | 2.71 MiB | 1.07 MiB | +4 KiB | 525× | PASS |
| C3 | +WAV×6 | 243 | 163 | 2.74 MiB | 1.08 MiB | +16 KiB | 468× | PASS |
| C4 | +Vorbis | 257 | 180 | 2.92 MiB | 1.16 MiB | +84 KiB | 486× | PASS |
| C5 | +Opus | 276 | 198 | 3.25 MiB | 1.28 MiB | +124 KiB | 489× | PASS |
| C6 | minimized | — | 198 | 3.25 MiB | 1.25 MiB (xz **492 KiB**) | −36 KiB | 372× | PASS |

三个口径分开：BUILD（TU/.a）、SHIPPING（.so/.dll stripped+xz）、RUNTIME（xRT）。
`.a` 不是 app 体积；shipping 结论一律 stripped `.so`/`.dll`。

## Linux final

```text
reachable TU        198
-O3 .a              3.25 MiB     -Os .a  2.97 MiB
size-oriented exe   1.26 MiB stripped (-Os+LTO+gc)
size-minimal .so    1.25 MiB stripped / 492 KiB stripped+xz
exports             恰 5 个 song_*（version script），ldd 仅 libc/libm
xRT                 最低 372×（50× 警告线）；-Os 使 MP3 1968×→848×（诚实记录），LTO 收回 1528×
```

## Windows final（x86_64，llvm-mingw 20260826 / clang 23.1.0 / UCRT，cross + 原生执行）

```text
Oracle TU / Reachable TU   279 / 192（独立 cross configure，禁复用 Linux manifest；lld -Map 成员集合硬相等）
static archive             3.87 MiB (-O3 replay)
qianqian_songcore.dll      1.27 MiB stripped / 539 KiB xz（-Os+LTO 变体 1.27 MiB / 533 KiB）
import library             2.2 KiB（dev-only，不计 shipping）
consumer .exe              正常驱动全契约（经 DLL import library 链接）
export table               恰 5 个 song_*（.def + objdump 机器 gate）
import table               kernel32 + api-ms-win-crt-* + bcrypt；零 FFmpeg DLL
Unicode path               PASS（CreateFileW + 测试音乐\歌曲-你好世界.m4a 宽路径全契约）
>2 GiB seek                PASS（虚拟 3 GiB WAV；max seek offset 2.95 GiB，无负回绕）
```

## Codec marginal cost（增量表）

```text
+AAC/MOV   ΔTU +51   Δ.a +1256 KiB   Δ.so +548 KiB   ← 最大单项（MOV/isom demuxer 为主）
+ALAC      ΔTU +3    Δ.a +19 KiB     Δ.so +4 KiB
+WAV ×6    ΔTU +3    Δ.a +35 KiB     Δ.so +16 KiB    ← 6 种 PCM 共享一个 pcm.c TU
+Vorbis    ΔTU +17   Δ.a +177 KiB    Δ.so +84 KiB
+Opus      ΔTU +18   Δ.a +347 KiB    Δ.so +124 KiB   ← 含上游强制 libswresample
```

## Correctness（每格式）

```text
MP3 / FLAC        PASS（原有 corpus 无回归；c0→cN 行为逐字节一致）
AAC/M4A/ADTS      PASS（degraded 案 typed 记录；ADTS 无 seek 为上游行为，typed finding）
ALAC              PASS（lossless strict canonical sha 双平台字节相等）
WAV ×6            PASS（strict；odd-chunk/malformed/truncated 覆盖）
Vorbis            PASS
Opus              PASS（preskip/end-trim：12s == 576000 样本精确）
seek              strict: FLAC/ALAC/WAV/Vorbis；record: MP3/AAC/Opus（lapping 语义，机器校准钉定）
```

## Regression

```text
existing MP3/FLAC corpus + real songs + E07 strict probes: PASS（每阶段）
```

## Limitations

- raw ADTS：上游 demuxer 无 seek 实现（typed finding，非本 PR 引入）。
- AAC 跨编译器 PCM 非逐字节一致（gcc-Linux vs clang-Windows，7/7 AAC 案；
  MP3/Vorbis/Opus 反而一致）——issue #11 规则下已记录 deterministic
  tolerance metric（见 windows.json），平台内确定性不受影响。
- Opus 强制引入 libswresample（上游 build 依赖）；SongCore 契约仍不使用 swr，
  无 resample 能力。
- 错误分类（malformed 案）记录强度弱于正常案（typed behavior + 跨阶段一致性，
  非逐错误码断言）。
- MSVC 变体、逐 codec Windows ladder：非本轮（canonical 为 llvm-mingw）。

## Reproduce

```bash
# Linux（rm -rf build 起全阶梯 + summary，一键）
bash tools/common_cleanroom.sh

# Windows（llvm-mingw SDK；产物经 WSL interop 在真 Windows 原生执行）
python3 tools/common_windows.py --all
python3 tools/common_windows_summary.py
```

Corpus：确定性合成（`corpus/tools/gen_corpus_common.py`），seek 语义由
`tools/common_calibrate.py` 机器观测钉定并在 clean-room `--check` 防漂移。

---

Closes #8 的 Linux + Windows 测量目标（不 merge 前保持 DRAFT；#9 不受影响）。
