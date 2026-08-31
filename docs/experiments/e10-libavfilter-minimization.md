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
### 边际成本阶梯（machine authority：`summary.json.ladder`）

| stage | tier | filters | compiled TU | +TU | live TU | live bytes | stripped | +stripped | xz | +xz |
|---|---|---|---:|--:|--:|--:|--:|--:|--:|--:|
| avf-c0 | codec-only | 0 | 205 | - | 104 | 651,151 | 588,016 | - | 201,456 | - |
| avf-c1 | F0 | 4 | 224 | 19 | 116 | 707,971 | 665,840 | 77,824 | 230,824 | 29,368 |
| avf-c2 | F1 | 14 | 236 | 12 | 125 | 804,521 | 796,912 | 131,072 | 268,500 | 37,676 |
| avf-c3 | F2 | 17 | 239 | 3 | 128 | 815,960 | 813,296 | 16,384 | 273,488 | 4,988 |
| avf-c4 | F3 | 21 | 243 | 4 | 132 | 828,471 | 829,680 | 16,384 | 279,628 | 6,140 |
| avf-c5 | F4 | 27 | 250 | 7 | 139 | 846,198 | 850,160 | 20,480 | 286,508 | 6,880 |
| avf-c6 | F5 | 32 | 255 | 5 | 144 | 907,974 | 915,696 | 65,536 | 306,168 | 19,660 |
| avf-c7 | F6 | 36 | 259 | 4 | 152 | 2,181,274 | 1,186,032 | 270,336 | 383,228 | 77,060 |
| avf-c8 | F7 | 37 | 260 | 1 | 153 | 2,188,319 | 1,190,128 | 4,096 | 386,620 | 3,392 |

### 单项探针（base = codec + F0；machine authority：`summary.json.probes`）

| probe | +TU vs c1 | +stripped vs c1 | +xz vs c1 | 图内自动插入转换 | smoke |
|---|--:|--:|--:|---|---|
| alimiter | 11 | 77,824 | 28,664 | auto_aresample_0 | PASS |
| loudnorm | 12 | 90,112 | 33,036 | auto_aresample_0 | PASS |
| afir | 11 | 282,624 | 82,792 | none | PASS |
| firequalizer | 11 | 278,528 | 82,100 | auto_aresample_0 | PASS |
| surround | 11 | 299,008 | 87,396 | auto_aresample_0 | PASS |
| headphone | 11 | 270,336 | 79,676 | none | PASS |
| atempo | 1 | 200,704 | 54,792 | none | PASS |
| crossfeed | 11 | 77,824 | 27,412 | auto_aresample_0 | PASS |
| chorus | 12 | 77,824 | 27,932 | auto_aresample_0 | PASS |
| flanger | 12 | 77,824 | 27,444 | auto_aresample_0 | PASS |

### 闭包记账（machine authority：`manifest-union.json`）

| codec-only | filter-only | shared | combined | 配置宏差异数 | 校验 |
|--:|--:|--:|--:|--:|---|
| 0 | 55 | 205 | 260 | 44 | codec⊆combined=True flag conflicts=True |

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
<!-- END GENERATED C0 TABLES -->

### 4.1 读数要点(解读以上生成表,不在 prose 重复数字口径之外的新数字)

- **框架入口 F0 是一次性门票**:图基础设施(缓冲端点 + 协商 + 图引擎)
  的成本几乎全部落在 C1,之后每个能力组的编译 TU 增量都只有个位数。
  注意 c1 的 +stripped 里含 probe 程序自身从"无 avfilter 后端 stub"变为
  "真后端"的胶水代码差——这是 F0 档测量口径的一部分,如实计入。
- **F1(增益/EQ/音调)= 最大的核心档**,因为产品能力同时引入
  aresample+swresample(格式适配基础,biquad 族是 fltp 原生)。
  biquad 族 8 个 filter 共享一个源 TU(`af_biquads.c`),能力语义不塌缩,
  成本上自然去重。
- **F2/F3/F4/F7 每档只有 +3..7 TU / +16..20 KB stripped(+3..7 KB xz)**
  ——经典播放器效果族的边际成本是小且可预测的。
- **F6(FIR/卷积/hdr EQ/surround/headphone)= 绝对主导项**:+270 KB
  stripped(+77 KB xz),因为它引入 av_tx 变换基础库(n9 的 FFT 框架)。
  单项探针显示 afir/firequalizer/surround/headphone 每个对 c1 基座都是
  +270..300 KB——第一个 FFT 用户付基础设施钱,之后的所有 FFT 用户
  近乎免费(c8 的 atempo 在 F6 之上只 +4 KB)。
- **loudnorm 的隐藏重采样被机器捕获**:c6 冒烟记录其图协商输出为
  `dbl @ 192000 Hz`——这是 §22 要求的"转换可见"证据;它只回答格式
  适配事实,不回答 SRC 选型(那是 E10-A1 的独立问题)。
- **live vs compiled 差异巨大**:c0 是 104/205,c8 是 153/260。报
  "compiled TU shipping"是错的;link-map 账本(`live-sections.json`)
  是权威。tx_*.o 在被拉入后 ~1.2 MB live(浮点/双精度/int32 三套
  codelet 表经内部函数指针表互相钉住,section GC 无法裁剪)——这是
  "F6 贵"的机械原因。
- **单项探针 vs 阶梯读法**:探针的 delta 以 c1 为基座,代表"产品里
  还没有该 filter 的任何基础依赖时的真实启用成本"(例如 alimiter 的
  +77.8 KB 含 swresample+aresample 基础 ~60 KB;atempo 的 +200 KB 含
  av_tx);阶梯 delta 则是这些基础在累计闭包里的摊销值。两者都进
  `marginal-cost.json`,不可混读。

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
  `.part.0` 拆分——修复(注入 flag 末尾中和)后符号集合精确相等。
- ~~"未选中的 filter 真的不在"~~:每 stage 的 absent 抽查(未启用层
  的 filter + rubberband/sofalizer/ladspa + not-justified 名单)全 PASS。
- ~~"codec 闭包没变"~~:c0 复现 canonical `n3-min-noswr` 闭包
  (205 TU,plain archive 2,658,174 B == E07 基线逐字节同值);
  union 证明组合闭包含全部 codec TU 且 flag 零冲突。
- ~~"组合配置是连贯的"~~:组合 manifest 来自单次 configure 会话;
  44 条配置宏差异被记录为"不许拼接"的证据而非被掩盖。
- ~~"archive 大小 = shipping"~~:本报告从不这样声称;archive 字节仅
  为诊断列,shipping 权威是最终链接 probe 的 stripped/xz。
- ~~"alimiter 真的需要 swresample"~~:**B1 假设被证实且更精确**——
  alimiter 协商 packed double(`dbl`),图自动插入 `auto_aresample_0`,
  而 aresample 的 configure 依赖是 swresample;冒烟还捕获了它的默认
  auto-level 行为(归一化回满幅),smoke 断言因此显式 `level=0`。
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
- 冒烟 = 闭包可用性证明,不是音质竞赛(§20);acrossfade/afir/
  headphone 是双输入 filter,在线性 probe 范围外,只有注册与闭包
  成本证据(记录于 `correctness-smoke.json` 与能力清单)。
- probe 程序自身的体积差(stub↔真后端)计入 F0 档,未单独剥离。
- 图 lifecycle 只在代表性 filter 上测(create/destroy/重建一致性,
  `smoke-*.json.timing`);运行时性能不是本实验目标。

### 4.5 LIBAVFILTER VIABILITY(§35 分类,不选 DSP backend)

**B — VIABLE:核心 DSP 便宜;高级组应保持可选。**

- 主流核心能力(F0+F1+F2)= +72 KB xz(+224 KB stripped)于 codec
  基线之上,含格式适配基础;F3/F4/F7 各 +3..7 KB xz;能力→成本的
  映射在每次新增时都可以通过"manifest 编辑 → oracle → 重放 → 冒烟"
  机器化复读,这正是北极星目标。
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

- 生产代码(`src/`、`include/`)零改动;xmake.lua 仅新增 bench-only 目标。
- 不选择 DSP backend;不合并 PR;不动 B2/SIMD;不动 UI/Kotlin。
- SRC 决策不受本实验影响(aresample 的出现只证明格式适配需求,不回答
  高质量 SRC 选型——那是 E10-A1 的独立问题)。
