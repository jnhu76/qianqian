# Tools

这些工具服务 Phase 0 Step 1.5，不进入 SongCore shipping runtime。

- `ffmpeg_import.py`：import/upgrade-time oracle capture。允许调用 pinned FFmpeg 的 configure/Make，一次性观察真实 compiler closure，生成 `build/ffmpeg-xmake/manifest.json`。
- `verify_xmake_core.py`：比较 upstream oracle archives 与 Xmake 单一 `libqianqian_av.a` 的 Stage A corpus 行为，并验证 production SongCore PCM 与 benchmark canonical PCM byte-identical。
- `qn_pcm_dump.c`：极薄 native host；本地文件 → SongCore → QPCM/Float32 stdout。无 FFmpeg type、无 audio device。
- `play_smoke.py`：test-only audible sink；只把 `qn_pcm_dump` 的 Float32 PCM 送到 Python `sounddevice`/PortAudio，不做任何压缩音频解码。

完整实验定义见 `docs/experiments/e06-xmake-selective-build-playback-smoke.md`。
