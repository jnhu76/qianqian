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
- `common_gate.py`：每阶段行为+体积门（oracle 等价、期望校验、SongCore PCM、seek/EOF probe、xRT、size 表）。
- `common_compare.py`：跨阶段回归比较（共享 corpus 行为必须字节一致；时间/ASLR 噪声剔除）。
- `common_calibrate.py`：用全能力构建**观测**各格式 seek 语义并钉入 corpus manifest（`--check` 模式供 clean-room 防漂移）。
- `common_so.py`：某阶段的 size-minimal `libqianqian_songcore.so`（PIC+-Os+sections+gc+version script），导出表恰 5 API 硬 gate + consumer smoke。
- `common_summary.py`：全部数字从 gate.json/so.json 机器派生成 `bench/results/common-formats/{summary.json,ladder.md}`，零手填。
- `common_cleanroom.sh`：一键复现整条 Linux 阶梯（`rm -rf build` 起 + 校准 drift check）。
- `common_windows.py` / `common_windows_summary.py` / `common_cross_tolerance.py`：Windows x86_64 阶段（cross oracle、mingw replay、COFF audit、DLL+PE gate、原生正确性、跨平台 PCM 容差）。

Windows host adapter：`songcore_windows_host.c`（`CreateFileW`/宽路径/`--unicode`/`--largefile` 虚拟 IO gate）；DLL 导出契约：`songcore_q.def`。
