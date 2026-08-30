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

| Stage | Capability | Oracle TU | Reachable=Compiled TU | `-O3 .a` | `-Os .a` | `.so` stripped | Δ `.so` | min xRT | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| c0 | MP3+FLAC | 202 | 106 | 1.46 MiB | — | 539 KiB | — | 474× | PASS |
| c1 | +AAC/M4A +ADTS | 237 | 157 | 2.69 MiB | — | 1.06 MiB | +548 KiB | 526× | PASS |
| c2 | +ALAC/M4A | 240 | 160 | 2.71 MiB | — | 1.07 MiB | +552 KiB | 506× | PASS |
| c3 | +PCM WAV (6 fmts) | 243 | 163 | 2.74 MiB | — | 1.08 MiB | +568 KiB | 519× | PASS |
| c4 | +Ogg Vorbis | 257 | 180 | 2.92 MiB | — | 1.16 MiB | +652 KiB | 533× | PASS |
| c5 | +Ogg Opus | 276 | 198 | 3.25 MiB | — | 1.28 MiB | +776 KiB | 541× | PASS |
| c6 | Common Formats minimized | 198 | 198 | 3.25 MiB | 2.97 MiB | 1.25 MiB | +740 KiB | 393× | PASS |

三个口径分开：BUILD（TU/.a）、SHIPPING（.so/.dll stripped+xz）、RUNTIME（xRT）。
`.a` 不是 app 体积；shipping 结论一律 stripped `.so`/`.dll`。

## Linux final

```text
oracle TU                 202 (C0) → 276 (C5)；C6 复用 C5 oracle
reachable = compiled TU   198（link-reachability 投影 + clean rebuild，逐 TU 核对）
-O3 .a / -Os .a           3.25 MiB / 2.97 MiB
size-oriented exe         1.26 MiB stripped (-Os+LTO+gc)
size-minimal .so          1.25 MiB stripped / 492 KiB stripped+xz
exports                   恰 5 个 song_*（version script），ldd 仅 libc/libm
xRT (C6 -Os+LTO)          最低 393×（alac-long；50× 警告线）
C0→C6 total               ΔTU +92、Δ.a +1833 KiB、Δ.so +740 KiB (757,840 B)
```

## Windows final（x86_64，clang version 23.1.0，UCRT，cross + 原生执行）

```text
oracle TU                 279（独立 cross configure，禁复用 Linux manifest）
reachable / compiled TU   202 / 202（archive member ↔ manifest TU 机器映射；投影 clean rebuild 后逐 TU 核对；fixpoint 1 轮）
reachability proof        reduced-archive 链接按 section SHA 相等 + lld -Map 成员集合佐证
static archive            3.26 MiB（projected closure replay）
qianqian_songcore.dll     1.25 MiB stripped / 533 KiB xz（canonical = dll-lto；LTO 变体已构建并通过同套 gates）
import library            2,262 B（dev-only，不计 shipping）
export table              恰 5 个 song_*（.def + objdump 机器 gate）
import table              kernel32 + api-ms-win-crt-* + bcrypt；零 FFmpeg DLL
Unicode path              PASS（CreateFileW + 测试音乐\歌曲-你好世界.m4a 宽路径全契约）
>2 GiB seek               PASS（虚拟 3 GiB WAV；max seek offset 2,952,790,060，无负回绕）
corpus                    33 clean + 11 degraded = 44 applicable，全部原生执行（skipped 0）
```

## Codec marginal cost（增量表，machine-derived）

```text
+AAC/M4A +ADTS               ΔTU +51   Δ.a +1256 KiB   Δ.so +548 KiB
+ALAC/M4A                    ΔTU  +3   Δ.a   +19 KiB   Δ.so +4 KiB
+PCM WAV (6 fmts)            ΔTU  +3   Δ.a   +35 KiB   Δ.so +16 KiB
+Ogg Vorbis                  ΔTU +17   Δ.a  +177 KiB   Δ.so +84 KiB
+Ogg Opus                    ΔTU +18   Δ.a  +347 KiB   Δ.so +124 KiB
Common Formats minimized     ΔTU  +0   Δ.a    +0 KiB   Δ.so -36 KiB
```

## Correctness（每格式）

```text
MP3 / FLAC        PASS（原有 corpus 无回归；c0→cN 行为逐字节一致）
AAC/M4A/ADTS      PASS（ADTS seek = UNSUPPORTED typed finding；其余 LAPPED 契约）
ALAC              PASS（lossless strict canonical sha 双平台字节相等）
WAV ×6            PASS（strict；odd-chunk/malformed/truncated 覆盖）
Vorbis            PASS（strict seek）
Opus              PASS（preskip/end-trim：12s == 576000 样本精确）
seek 契约         STRICT: FLAC/ALAC/WAV/Vorbis（suffix 逐字节）
                  LAPPED: MP3/AAC/Opus（seek 成功+bounded resume+PCM+clean EOF，
                  suffix 不要求相等——bit reservoir / MDCT overlap / CELT lapping）
                  UNSUPPORTED: raw ADTS（typed seek 失败，decode/EOF 仍 gate）
degraded 案       Windows 原生执行 11 案：typed 分类（OPEN_FAILED/PROBE_FAILED/DECODE_ERROR/DEGRADED_EOF）+ 无 crash/hang + 界内输出 + 两次分类一致
```

## Regression

```text
existing MP3/FLAC corpus + real songs + E07 strict probes: PASS（每阶段）
```

## Limitations

- raw ADTS：上游 demuxer 无 seek 实现（typed finding，非本 PR 引入）。
- AAC 跨编译器 PCM 非逐字节一致（gcc-Linux vs clang-Windows）：issue #11 规则下已记录
  deterministic tolerance metric（max|Δ| = 5.960e-08 = 1 ULP；帧数逐案相等）；平台内确定性不受影响。
- Opus 强制引入 libswresample（上游 build 依赖）；SongCore 契约仍不使用 swr，无 resample 能力。
- xRT 为运行时测量：绝对值跨 run 波动可观（热/调度），相对 -Os 代价为结论；机器 authority 一律以 summary.json 当前值为准。
- MSVC 变体、逐 codec Windows ladder：非本轮（canonical 为 llvm-mingw）。

## Reproduce

```bash
# Linux（rm -rf build 起全阶梯 + summary，一键）
bash tools/common_cleanroom.sh

# Windows（llvm-mingw SDK；产物经 WSL interop 在真 Windows 原生执行）
python3 tools/common_windows.py --all
python3 tools/common_windows_summary.py
python3 tools/common_summary.py --check
```

Corpus：确定性合成（`corpus/tools/gen_corpus_common.py`），seek 语义由
`tools/common_calibrate.py` 机器观测钉定并在 clean-room `--check` 防漂移；
本 PR body 由 `common_summary.py` 从 summary.json + windows.json 派生，含 provenance sha，
任何手工改动都会被 `--check` 拒绝。

---

Closes #8 的 Linux + Windows 测量目标（不 merge 前保持 DRAFT；#9 不受影响）。

<!-- provenance (machine authority): summary.json sha256=1044d7c3f9c24312ca61702e1c0636d9c27792e8bf7e630b4f19576f674eb9fb; windows.json sha256=5cd391f5f4215ca423286297629e8cf4cb12558c2d93c2477a929be66ad07442 -->
