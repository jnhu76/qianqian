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
- **格式适配 = 发现,不是预付(review P0-1 修复)**:能力意图只声明
  **期望的 filter**;configure-required 依赖由 pinned configure 的
  `<filter>_filter_deps` 规则机器解析(权威,如 pan→swresample),
  除此之外不预付任何东西。每个 stage/探针先构建"直接闭包"
  (codec+F0+目标 filter+configure 依赖),用 canonical flt 输入真实
  实例化代表图;图配置失败 → 才加最小格式适配(aresample,其
  configure 依赖为 swresample)并重建。**直接闭包**与**有效可用闭包**
  分别记账,适配成本与 filter 本体成本分开报告。probe 报告每张图的
  协商格式与 auto-inserted 转换 filter(instance 名 `auto_*`),不把
  flt↔fltp/dbl 转换藏进"filter 成本"。
- **多输入 filter(afir/acrossfade/headphone,review P1-1)**:probe
  增加 2-input `graph2` 功能性 smoke(signal+IR / A+B / signal+HRIR),
  只证明 config+process+drain+finite+non-empty,不做音质竞赛。
- **重放保真 flag 隔离(review P0-3)**:oracle 等价所需的
  `-fvisibility=default -UNDEBUG` 中和移入 xmake 选项
  `--av_replay_exact`(C0-only);默认生产构建语义与父分支一致
  (机器验证:同 manifest 下默认 session 的 `libqianqian_av.a`
  与父分支逐字节/符号集相等)。
- **live-bytes(§18)**:shipping link map 逐 member 对齐 manifest,
  compiled closure vs live closure 分开报告。
- avf-c0(codec-only)无 libavfilter 可链,probe 以 `QN_PROBE_NO_AVFILTER`
  fail-closed stub 同形链接;stub→真后端的体积差如实计入 F0。

## 3. 能力阶梯(意图 = bench/dsp-capabilities.json)

```text
C0 = codec closure only(n3-min-noswr)
C1 = C0 + F0(abuffer/abuffersink/anull/aformat,无 aresample)
C2 = C1 + F1(volume/equalizer/biquad/bass/treble/lowshelf/highshelf/
         lowpass/highpass;aresample+swresample 若且仅当被图发现)
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
### 边际成本阶梯（machine authority：`summary.json.ladder`）

| stage | tier | filters | compiled TU | +TU | live TU | live bytes | stripped | +stripped | xz | +xz |
|---|---|---|---:|--:|--:|--:|--:|--:|--:|--:|
| avf-c0 | codec-only | 0 | 205 | - | 104 | 651,151 | 588,016 | - | 202,168 | - |
| avf-c1 | F0 | 4 | 224 | 19 | 116 | 707,971 | 669,936 | 81,920 | 233,204 | 31,036 |
| avf-c2 | F1 | 14 | 236 | 12 | 125 | 804,521 | 801,008 | 131,072 | 270,504 | 37,300 |
| avf-c3 | F2 | 17 | 239 | 3 | 128 | 815,960 | 821,488 | 20,480 | 275,972 | 5,468 |
| avf-c4 | F3 | 21 | 243 | 4 | 132 | 828,471 | 833,776 | 12,288 | 282,032 | 6,060 |
| avf-c5 | F4 | 27 | 250 | 7 | 139 | 846,198 | 858,352 | 24,576 | 289,000 | 6,968 |
| avf-c6 | F5 | 32 | 255 | 5 | 144 | 907,974 | 919,792 | 61,440 | 308,500 | 19,500 |
| avf-c7 | F6 | 36 | 259 | 4 | 152 | 2,181,274 | 1,190,128 | 270,336 | 385,656 | 77,156 |
| avf-c8 | F7 | 37 | 260 | 1 | 153 | 2,188,319 | 1,198,320 | 8,192 | 389,080 | 3,424 |

### 单项探针（base = codec + F0；machine authority：`summary.json.probes`）

| probe | +TU vs c1 | direct Δxz vs c1 | adaptation Δxz | effective Δxz vs c1 | 适配类别 | 图内自动插入转换 | gate |
|---|--:|--:|--:|--:|---|---|---|
| alimiter | 11 | 3,300 | 25,724 | 29,024 | graph-format-adaptation | auto_aresample_0 | PASS |
| loudnorm | 12 | 7,604 | 25,544 | 33,148 | graph-format-adaptation | auto_aresample_0 | PASS |
| afir | 11 | 57,476 | 25,352 | 82,828 | graph-format-adaptation | auto_aresample_0,auto_aresample_1 | PASS |
| firequalizer | 11 | 57,380 | 24,784 | 82,164 | graph-format-adaptation | auto_aresample_0 | PASS |
| surround | 11 | 62,432 | 25,016 | 87,448 | graph-format-adaptation | auto_aresample_0 | PASS |
| headphone | 1 | 54,324 | 0 | 54,324 | none | none | PASS |
| atempo | 1 | 54,336 | 0 | 54,336 | none | none | PASS |
| crossfeed | 11 | 2,000 | 25,640 | 27,640 | graph-format-adaptation | auto_aresample_0 | PASS |
| chorus | 12 | 2,356 | 25,292 | 27,648 | graph-format-adaptation | auto_aresample_0 | PASS |
| flanger | 12 | 2,044 | 25,572 | 27,616 | graph-format-adaptation | auto_aresample_0 | PASS |

读法：`direct xz` = 只含 filter 本体 + configure-required 依赖的闭包（适配失败时的可用性见 gate）；`adaptation xz` = 由图实例化失败**发现**的最小转换能力（aresample+swresample）增量。两者不得合并为一个模糊边际数。

多输入 filter（afir/acrossfade/headphone）由 2-input graph2 功能性 smoke 覆盖（config+process+drain+finite+non-empty）。

### 闭包记账（machine authority：`manifest-union.json`）

| codec-only | filter-only | shared | combined | 配置宏差异数 | 校验 |
|--:|--:|--:|--:|--:|---|
| 0 | 55 | 205 | 260 | 39 | codec⊆combined=True flag conflicts=True |

### gate 总表（machine authority：`summary.json.stages`）

| stage | gates |
|---|---|
| avf-c0 | PASS |
| avf-c1 | PASS |
| avf-c2 | PASS |
| avf-c3 | PASS |
| avf-c4 | PASS |
| avf-c5 | PASS |
| avf-c6 | PASS |
| avf-c7 | PASS |
| avf-c8 | PASS |

**顶层 verdict：PASS** （谓词：add_one_probe_gates=PASS; external_dependency_policy=PASS; ffmpeg_pin_equal=PASS; ladder_stage_gates=PASS; license_policy=PASS; manifest_union=PASS）
<!-- END GENERATED C0 TABLES -->

### 4.1 读数要点(解读以上生成表;所有数字出自生成表,不新增手抄口径)

- **框架入口 F0 是一次性门票**:+19 TU / +31 KB xz——图基础设施
  (缓冲端点、格式协商、图引擎、filter 注册)几乎全部落在 C1;
  `av_tx` 变换 TU(tx/tx_float/tx_double/tx_int32)是 libavutil 的
  **无条件成员,从 C1 起就在编译闭包里**(pinned libavutil/Makefile
  198-201 行,不受任何 filter config 控制)——"FFT 基础由 F6 引入"
  是编译层错觉;F6 的真实成本发生在**链接期**(见下)。
- **F1(增益/EQ/音调)自身只值 +2 TU**(volume + biquad 族共享 TU);
  生成表里的 +12 是**图发现**出来的格式适配(aresample+swresample
  共 10 TU,biquad 族是 fltp 原生,canonical flt 输入图配置失败后被
  发现)加上 filter 本体。review P0-1 之前这个 +12 被当成"F1 的
  编译成本"报告——现在 direct/effective 分开记账。
- **F2/F3/F4/F5/F7 每档 +1..7 TU / +3..20 KB xz**(effective,
  与前档的差值;适配基础已在前档闭包中,不重复计)。
- **F6(FIR/firequalizer/surround/headphone)= 主导项**:+4 TU compiled
  但 live bytes 从 908 KB 跳到 2,181 KB(+1.27 MB):四个 FFT filter
  的代码把已在闭包里的 av_tx codelet 表**变 live**。单项探针口径:
  afir direct Δxz +57 KB(含 tx live 化),firequalizer/surround 同级,
  之后无新增(atempo 在 C7 之上只 +3.4 KB)——**第一个 FFT 用户付
  live 化的钱,之后的 FFT 用户近乎免费**。
- **loudnorm 的隐藏重采样被机器捕获**:冒烟记录其图协商输出
  `dbl @ 192000 Hz`——格式适配事实,不回答 SRC 选型(A1 的独立问题)。
- **live vs compiled 差异巨大**:c0 是 104/205,c8 是 153/260;shipping
  权威是最终链接 probe 的 stripped/xz,报 compiled TU 是错的。
- **单项探针的 honest 读法**(生成表第 2 节):`direct Δxz vs c1` =
  只含 filter 本体 + configure-required 依赖的闭包增量(alimiter
  只有 +3.3 KB!);`adaptation Δxz` = 图配置失败后**被发现**的最小
  转换能力(≈ +25 KB,aresample+swresample);`effective Δxz` = 两者
  之和。review P0-1 之前这三者混在一个模糊数字里。headphone
  (multich HRIR)与 atempo 直接吃 canonical flt,**不需要适配**——
  旧证据为 headphone 预付的适配成本(+25 KB)是错的。
- **多输入 filter 有真实功能证据**:afir(signal+微型 IR,双输入
  各被自动插入一个 aresample,协商 fltp)、acrossfade(A+B,原生
  flt)、headphone(signal+quad multich HRIR,原生 flt)都由
  2-input graph2 功能性 smoke 覆盖并计入 gate(review P1-1 修复;
  不再是"只有注册证据")。

### 4.2 配置兼容性与记账(§12 的机器证明)

union 记账(生成表第 3 节):codec 闭包 205 TU 全部原样出现在组合闭包
中(0 个 codec-only 丢失、共享 TU 的编译 flag 零冲突),filter 侧净增
55 TU(46 avfilter + 9 swresample)。同时两套独立 configure 根的
config 宏差异为 44 条——**这正是"禁止把两个 configure 根的 TU 拼接成
产品"的原因**;shipping 权威是组合意图下的单次 configure oracle
manifest,union 工具只做记账与冲突检测。

### 4.3 对抗性自审(§33 逐项,全部有机器证据)

- ~~"Xmake 真的编译了目标 filter"~~:nm `ff_*` 符号集合 oracle 归档 ==
  重放归档(逐 stage gate);`filter_list.c` 名单 == config.mak ==
  能力意图;运行时 `avfilter_get_by_name` present/absent 双向抽查。
  中途真实抓到过一次保真度 bug:xmake 注入的 `-fvisibility=hidden`/
  `-DNDEBUG` 改变 GCC IPA 决策,重放对象出现 oracle 没有的
  `.part.0` 拆分——修复后符号集合精确相等。该中和最初被无条件
  注入 `qianqian_av`(review P0-3 指出这改变了生产构建语义),
  现已隔离进 C0-only 选项 `--av_replay_exact`;默认构建与父分支
  逐字节/符号集机器验证一致。
- ~~"未选中的 filter 真的不在"~~:每 stage 的 absent 抽查(未启用层
  的 filter + rubberband/sofalizer/ladspa + not-justified 名单)全 PASS。
- ~~"codec 闭包没变"~~:c0 复现 canonical `n3-min-noswr` 闭包
  (205 TU,plain archive 2,658,174 B == E07 基线逐字节同值);
  union 证明组合闭包含全部 codec TU 且 flag 零冲突。
- ~~"组合配置是连贯的"~~:组合 manifest 来自单次 configure 会话;
  44 条配置宏差异被记录为"不许拼接"的证据而非被掩盖。
- ~~"archive 大小 = shipping"~~:本报告从不这样声称;archive 字节仅
  为诊断列,shipping 权威是最终链接 probe 的 stripped/xz。
- ~~"alimiter 真的需要 swresample"~~:**B1 假设被证实,且归因方式
  已修正**——alimiter 的 configure 依赖为空(机器解析 pinned
  configure 为证);它的 swr 需求完全来自格式适配:图配置失败被发现,
  加 aresample 后协商 packed double(`dbl`),图自动插入
  `auto_aresample_0`。冒烟捕获其默认 auto-level 行为(归一化回满幅),
  smoke 断言因此显式 `level=0`。
- ~~"FIR 很贵"~~:证实,见 4.1(且贵在 av_tx 基础,不在 afir 本身)。
- ~~"205 compiled TU = 205 live TU"~~:证伪,见 4.1。
- ~~"没有引入外部依赖"~~:external-libs gate 逐 stage PASS(rubberband/
  libmysofa/LADSPA/LV2/OpenCL 全 absent);ldd 逐 stage 记录,全阶梯
  仅 libm/libc,与 c0 基线一致。
- ~~"filter 启用名单 == 产品能力名单"~~:registration gate 逐 stage
  PASS;能力清单中的 `considered_not_enabled`(amix/stereotools/
  extrastereo/haas/replaygain 等)有 absent 抽查背书。
- **B1 数字勘误(诚实更新)**:B1 的"libavfilter 205 TU"是 Make 直建、
  含 avcodec 47 + avformat 32 的未 GC 数字;与 canonical codec 闭包的
  205 TU 相同纯属巧合,两者成分完全不同。在 capability 阶梯 +
  section GC + -Os 下,本实验的口径是 260 compiled / 153 live /
  +185 KB xz(全能力包)或 +72 KB xz(核心 F0+F1+F2)。
- **LTO**:c0/c1/c4/c8 四个代表点已跑(`shipping.json.lto`);LTO 的
  `.a` 是 bytecode,不参与 archive 比较(E07 同规)。

### 4.4 局限

- 单主机(Linux x86_64, WSL2);Windows/WASM 闭包必须重新求
  (manifest 记录 toolchain 身份,`--configure-extra` 支持交叉 oracle),
  本任务未做 Windows 重放。
- 冒烟 = 闭包可用性证明,不是音质竞赛(§20)。acrossfade/afir/
  headphone 原先只有注册+闭包证据(review P1-1),现由 2-input
  `graph2` 功能性 smoke 覆盖(afir=signal+微型 IR、acrossfade=A+B、
  headphone=signal+quad multich HRIR;只断言 config+process+drain+
  finite+非空,状态记录于各证据 JSON 的 `multi_input_status`)。
- alimiter 的 auto-level 行为断言(`level=0`)保持。
- probe 程序自身的体积差(stub↔真后端)计入 F0 档,未单独剥离。
- 图 lifecycle 只在代表性 filter 上测(create/destroy/重建一致性,
  `smoke-*.json.timing`);运行时性能不是本实验目标。

### 4.5 LIBAVFILTER VIABILITY(§35 分类,不选 DSP backend)

**B — VIABLE:核心 DSP 便宜;高级组应保持可选。**
(分类按修复后的测量重推;不是沿用旧 verdict。)

- 主流核心能力(C3 = F0+F1+F2 effective)= codec 基线 +73.8 KB xz,
  其中 F1 自身 filter 只占 2 TU,swresample 基础是一次性格式适配
  (发现所得,非某 filter 的本体成本);F4/F5 各 +6..7 KB xz、F7
  +3.4 KB xz;能力→成本的映射在每次新增时都可以通过"manifest 编辑
  → oracle → 重放 → 图实例化 → 冒烟"机器化复读,这正是北极星目标。
- 单项读法更狠:limiter 本体 direct Δxz 只有 +3.3 KB;任何"某 filter
  贵"的判断必须先区分本体、configure 依赖与发现的格式适配三层。
- F6(FIR/surround/headphone 类)+77 KB xz 应保持独立可选层——它的
  成本来自 av_tx 基础设施,一旦产品需要任何 FFT 用户即可摊销。
- 无新增外部依赖;LGPL-2.1-or-later 许可证据逐 stage 记录。
- 本分类不回答"thin DSP vs libavfilter 谁是最终 DSP backend"(§35 禁止
  对照,B1 的 thin 结论保持其历史证据范围);它回答的是:libavfilter
  可以作为**可测量的能力仓库**进入架构选项,成本随能力可预测增长。

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

```text
production source changed:             NO
production build semantics changed:    NO  (replay-fidelity flags isolated
                                            behind C0-only --av_replay_exact;
                                            default build proven identical
                                            to parent branch)
```

- 生产代码(`src/`、`include/`)零改动;xmake.lua 新增 bench-only 目标
  `qn_avfilter_cap_probe` 与 C0-only 选项 `av_replay_exact`(默认
  false;默认 session 的编译/链接语义与父分支机器验证一致)。
- 不选择 DSP backend;不合并 PR;不动 B2/SIMD;不动 UI/Kotlin。
- SRC 决策不受本实验影响(aresample 的出现只证明格式适配需求,不回答
  高质量 SRC 选型——那是 E10-A1 的独立问题)。
