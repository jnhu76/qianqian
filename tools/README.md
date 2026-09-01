# Tools

Durable build / import / audit tools for the Native Audio Core. Nothing here
enters the SongCore shipping runtime; everything serves the FFmpeg
minimization method or SongCore validation.

## FFmpeg import / closure (the minimization method)

| Tool | Role |
|---|---|
| `ffmpeg_import.py` | Canonical import: runs pinned FFmpeg configure/Make **once** as an oracle, freezes the compile closure into `build/ffmpeg-xmake/manifest.json` (host target recipe, schema 2). Invoked via `xmake ffmpeg-import`. |
| `ffmpeg_profile_import.py` | Target-aware import: `--target <recipe-id>` derives `build/manifests/<target>/<profile>/manifest.json` from the target recipe (`ffmpeg/targets/*.json`); `--stage` remains for test/DSP stages under `build/minimize/<stage>/`. |
| `ffmpeg_manifest_union.py` | Codec-closure + filter-closure union accounting (conflict-detecting). |
| `dsp_closure.py` | DSP capability closure driver: `ffmpeg/capabilities/dsp.json` (intent) → profile → oracle import → Xmake replay → `dsp_cap_probe` → `bench/results/avfilter-minimize/`. The retained aggregate evidence is a frozen reference; re-run the ladder to regenerate on an FFmpeg upgrade. |

## SongCore validation

| Tool | Role |
|---|---|
| `qn_pcm_dump.c` | Thinnest native SongCore host: feeds SongCore and writes a self-describing Float32 PCM stream. No FFmpeg types, no audio device. Built by xmake as `qn_pcm_dump`. |
| `verify_xmake_core.py` | Proves the Xmake-replayed single archive reproduces the FFmpeg oracle: benchmark binary linked two ways (upstream archives vs `libqianqian_av.a`) plus byte-identical SongCore PCM on representative fixtures. |
| `measure_songcore_artifacts.py` | Measures the current ABI v1 reference artifacts (`libsongcore.a` / `libsongcore.so`): raw/stripped/xz sizes, sha256, shared exports, dynamic deps → `bench/results/songcore-v1/reference-artifacts.json`. |
| `check_api_doc.py` | Fails unless `docs/songcore-api.md` documents every `SONGCORE_API` symbol in the header and the shared export table matches the header exactly. |
| `play_smoke.py` | Audible acceptance: decodes in native SongCore, plays Float32 PCM through a test-only `sounddevice`/PortAudio sink. Test-only by rule (AGENTS.md §8); never decodes compressed audio itself. |

Build workflow: `docs/ffmpeg-minimization.md`. Regression: `tests/songcore/README.md`.
