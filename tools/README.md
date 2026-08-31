# Tools

这些工具服务 Phase 0 Step 1.5，不进入 SongCore shipping runtime。

- `ffmpeg_import.py`：import/upgrade-time oracle capture。允许调用 pinned FFmpeg 的 configure/Make，一次性观察真实 compiler closure，生成 `build/ffmpeg-xmake/manifest.json`。
- `verify_xmake_core.py`：比较 upstream oracle archives 与 Xmake 单一 `libqianqian_av.a` 的 Stage A corpus 行为，并验证 production SongCore PCM 与 benchmark canonical PCM byte-identical。
- `qn_pcm_dump.c`：极薄 native host；本地文件 → SongCore → QPCM/Float32 stdout。无 FFmpeg type、无 audio device。
- `play_smoke.py`：test-only audible sink；只把 `qn_pcm_dump` 的 Float32 PCM 送到 Python `sounddevice`/PortAudio，不做任何压缩音频解码。

完整实验定义见 `docs/experiments/e06-xmake-selective-build-playback-smoke.md`。

## Common Formats ladder（issue #8，E08）

- `common_corpus.py`：阶梯共享库。stage→capability 集合、corpus 过滤（stage-a + common-formats）、qn_bench 构建/运行、fixture 完整性校验。
- `common_import.py`：按 capability profile 跑一次 FFmpeg configure/Make oracle（支持 target-specific 附加参数，如 Windows cross），冻结 compile manifest；记录 configure 降级警告。
- `common_stage.py`：单阶段全流程 = import → 全量 replay → link audit → 投影 → clean rebuild → gate。
- `common_gate.py`：每阶段行为+体积门（oracle 等价、期望校验、SongCore PCM、seek/EOF probe、xRT、size 表）。seek 契约分 STRICT（suffix 逐字节）/ LAPPED（seek 成功 + bounded resume + PCM + clean EOF，suffix 不要求相等）/ UNSUPPORTED（typed seek 失败，如 raw ADTS）三档，gate 拥有独立最小语义；malformed 案在生产路径上以 typed、确定性失败门执行。
- `common_compare.py`：跨阶段回归比较（共享 corpus 行为必须字节一致；时间/ASLR 噪声剔除）。
- `common_calibrate.py`：用全能力构建**观测**各格式 seek 语义并钉入 corpus manifest（`--check` 模式供 clean-room 防漂移）；只负责观测与分类，不做 gate。
- `common_so.py`：某阶段的 size-minimal `libqianqian_songcore.so`（PIC+-Os+sections+gc+version script），导出表恰 5 API 硬 gate + consumer smoke；链接产出 `so.map` 供归因。
- `common_attribution.py`：能力增量归因——从 `.so` 链接 map 解析 gc 后的 live section 字节，按成员族（mov/isom、adts-demux、aac-decoder、shared）分组，输出 `bench/results/common-formats/attribution.json`。
- `common_summary.py`：全部数字从 gate.json/so.json 机器派生成 `bench/results/common-formats/{summary.json,ladder.md,PR_BODY.md}`，零手填；ladder.md 与 PR body 是 summary.json(+windows.json) 的纯函数并内嵌 provenance sha，`--check` 拒绝任何漂移。
- `common_cleanroom.sh`：一键复现整条 Linux 阶梯（`rm -rf build` 起 + 校准 drift check + 归因 + markdown 权威 check）。
- `common_windows.py` / `common_windows_summary.py` / `common_cross_tolerance.py`：Windows x86_64 阶段（cross oracle、mingw replay、COFF audit、TU 投影 + 逐 TU 编译核对 + fixpoint 复审、从投影闭包构建 -Os 与 -Os+LTO 双 DLL 变体（各过全套 PE gates + 原生正确性）、degraded typed 分类、跨平台 PCM 容差）。
- `link_audit_windows.py`：COFF/PE 版链接可达性审计——offset 级成员解析（重名 basename 不再折叠）、llvm-nm 语义模拟、reduced-archive PE 内容相等证明（load-bearing section sha）+ lld -Map 佐证；reachable-objects.json 与 Linux 版同 schema。

Windows host adapter：`songcore_windows_host.c`（`CreateFileW`/宽路径/`--unicode`/`--largefile` 虚拟 IO gate/`--robust` degraded typed 分类）；DLL 导出契约：`songcore_q.def`。

## PCM Processing P0（issue #12，E10-P0）

- `pcm_p0.py`：E10-P0 driver——编译 `bench/pcm/` 契约 + harness（`cc -std=c11 -O2`）、运行 96-case 透明 bypass corpus + lifecycle + allocation gate + placement model + block matrix，机器汇编 `bench/results/pcm-processing/p0-summary.json`（含 provenance + gates；生产代码零改动由 `git diff` 验证）。`--check` 校验 summary 与 section JSON 无漂移。
- `pcm_report_tables.py`：`docs/experiments/e10-pcm-processing.md` 数字表格生成器，唯一 authority 为 `p0-summary.json`；`--check` 拒绝 doc 与 JSON 漂移。

实验代码：`bench/pcm/pcm_pipeline.{h,c}`（rate-changing/rate-preserving 双契约 + BYPASS/OFF + 计数分配器 + 有界 slab 队列）与 `bench/pcm/qn_pcm_p0_harness.c`（测量仪器）。全部为实验代码，不进入 shipping runtime。
