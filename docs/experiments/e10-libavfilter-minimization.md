# E10-C0 — libavfilter 能力化最小化(capability-driven minimization)

状态:`C0 COMPLETE`(全部 gate 在真实环境运行;机器权威 =
`bench/results/avfilter-minimize/summary.json`,由 `tools/pcm_c0.py`
从各 stage 证据 JSON 汇编;本文表格全部生成,`--check` 漂移即 FAIL。)

```text
C0 selects no DSP backend.
C0 does not productionize libavfilter.
C0 answers: can libavfilter be treated as a capability repository whose
cost grows predictably under the existing Xmake replay pipeline?
```

## 1. 问题(任务 #12 / E10-C0)

B1 曾给出初步证据:libavfilter 205 TU / xz 205 KB,thin DSP 2 TU / 6 KB。
但 B1 是 Make 直建、单一 filter 组合、没有 link-time 最小化、没有
capability 分解——"205 TU" 里有多少是 DSP 能力真正要付的?每加一个
DSP 功能边际成本是多少?本实验把 E06/E07 已验证的最小化方法(capability
intent → upstream configure/Make oracle → compile-closure manifest →
Xmake replay → section GC/-Os/LTO → 最终链接 artifact)扩展到 libavfilter。

北极星:不是"把 libavfilter 压到理论最小",而是
**把 libavfilter 变成可测量的、capability 驱动的依赖——未来每加一个
DSP 特性,成本可以可预测地读出来**:

```text
enable capability → regenerate oracle → regenerate manifest
→ Xmake build → observe marginal cost → smoke-test
```

## 2. 方法

```text
bench/dsp-capabilities.json        人类维护的能力意图(机器权威,非 Markdown)
        ↓ tools/pcm_c0.py 逐 stage 派生 profile
tools/common_import.py             upstream configure/Make oracle(仅 import 期)
        ↓ V=1 compile-closure manifest(build/minimize/avf-*/manifest.json)
xmake f --av_manifest=<manifest>   qianqian_av 重放(正常构建不经 FFmpeg Make)
        ↓ qn_avfilter_cap_probe(bench-only product-shaped link)
registration / smoke / negotiated-format / ldd / sizes / link-map live bytes
        ↓
bench/results/avfilter-minimize/*.json
```

- **单一配置根原则(§12)**:每个阶梯点都是一次独立 configure 会话下的
  codec+filter 组合意图;shipping manifest 权威来自组合 oracle,不拼接
  两个配置根的 TU。`tools/ffmpeg_manifest_union.py` 做记账
  (codec-only / filter-only / shared / 冲突检测)并机器记录配置宏分歧
  ——即"为什么不许拼接"。
- **shipping 权威(§17)**:最终链接的 product-shaped probe,stripped +
  xz;`.a`/TU 数只是诊断。三档:plain release(manifest 原样 flags)、
  shipping(`-ffunction-sections -fdata-sections -Os` + `--gc-sections`,
  E07 S4/S5 同款 flag mutation)、LTO(代表性阶梯点)。
- **registration 机器门(§19)**:oracle 生成的 `filter_list.c` 名单 ==
  config.mak 启用名单 == 能力意图;nm 的 `ff_*` 符号集合 oracle 归档 ==
  Xmake 重放归档;运行时 `avfilter_get_by_name` present/absent 双向抽查。
- **格式策略(§22)**:probe 报告每张图的协商格式与 auto-inserted 转换
  filter(instance 名 `auto_*`,来自 pinned `avfiltergraph.c`),不把
  flt↔fltp/dbl 转换藏进"filter 成本"。
- **live-bytes(§18)**:shipping link map 逐 member 对齐 manifest,
  compiled closure vs live closure 分开报告。
- avf-c0(codec-only)无 libavfilter 可链,probe 以 `QN_PROBE_NO_AVFILTER`
  fail-closed stub 同形链接;stub→真后端的体积差如实计入 F0。

## 3. 能力阶梯(意图 = bench/dsp-capabilities.json)

```text
C0 = codec closure only(n3-min-noswr)
C1 = C0 + F0(abuffer/abuffersink/anull/aformat,无 aresample)
C2 = C1 + F1(volume/equalizer/biquad/bass/treble/lowshelf/highshelf/
         lowpass/highpass;+aresample+swresample 假设,图证据验证)
C3 = C2 + F2(acompressor/alimiter/agate)
C4 = C3 + F3(pan/channelmap/crossfeed/stereowiden)
C5 = C4 + F4(aecho/chorus/flanger/aphaser/tremolo/vibrato)
C6 = C5 + F5(afade/acrossfade/adelay/loudnorm/dynaudnorm)
C7 = C6 + F6(afir/firequalizer/surround/headphone)
C8 = C7 + F7(atempo)
```

排除项与理由记录在能力清单里(`considered_not_enabled` /
`external_optional`):amix、stereotools、extrastereo、haas、replaygain
filter、rubberband/libmysofa/LADSPA 一律不进默认闭包。

## 4. 结果

<!-- BEGIN GENERATED C0 TABLES -->
<!-- END GENERATED C0 TABLES -->

## 5. 复现命令

```bash
# 全阶梯(oracle + xmake replay + smoke + 证据;可续跑)
python3 tools/pcm_c0.py

# 单 stage / 单探针
python3 tools/pcm_c0.py --stage avf-c2
python3 tools/pcm_c0.py --stage avf-p-alimiter

# 记账 + 汇编 + 表格 + 漂移检查
python3 tools/pcm_c0.py --union
python3 tools/pcm_c0.py --aggregate
python3 tools/pcm_c0.py --report
python3 tools/pcm_c0.py --check
```

oracle 环境与 pinned FFmpeg(n9.0.1 bf1b838f)由 `bench/ffmpeg-pin.json`
冻结;每 stage 的 configure args / 警告 / 生成 `filter_list.c` sha256
记录在该 stage 证据 JSON。host:AMD Ryzen 7 5800H,WSL2,Linux x86_64。

## 6. 范围声明

- 生产代码(`src/`、`include/`)零改动;xmake.lua 仅新增 bench-only 目标。
- 不选择 DSP backend;不合并 PR;不动 B2/SIMD;不动 UI/Kotlin。
- SRC 决策不受本实验影响(aresample 的出现只证明格式适配需求,不回答
  高质量 SRC 选型——那是 E10-A1 的独立问题)。
