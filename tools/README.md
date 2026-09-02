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
| `songcore_ffi_smoke.py` | **Canonical external consumer gate** (pure stdlib: ctypes/argparse/hashlib/struct): mirrors the 15 frozen ABI symbols + every `songcore.h` struct, drives the shipped shared library over host IO (open/probe/metadata/artwork/decode/seek/close, EOF, typed-error contracts) across FLAC/MP3/AAC-M4A/ADTS/ALAC/WAV/Vorbis/Opus. `--play` adds audible Float32 output at source rate through sounddevice (no Python-side resampling, no `qn_pcm_dump`). `--json` writes machine evidence. |
| `songcore_wasm_smoke.py` | Independent WASI host (`wasmtime`): instantiates `build/artifacts/wasm/SongCore.wasm`, provides the `qianqian_host` read/seek/size imports over a plain file, and drives the `song_wasm_*` bridge end to end. |
| `qn_pcm_dump.c` | Thinnest native SongCore host: feeds SongCore and writes a self-describing Float32 PCM stream. No FFmpeg types, no audio device. Built by xmake as `qn_pcm_dump` (test/reference transport, not a consumer gate). |
| `verify_xmake_core.py` | Proves the Xmake-replayed single archive reproduces the FFmpeg oracle: benchmark binary linked two ways (upstream archives vs `libqianqian_av.a`) plus byte-identical SongCore PCM on representative fixtures. |
| `measure_songcore_artifacts.py` | Measures the current ABI v1 reference artifacts (`libsongcore.a` / `libsongcore.so`): raw/stripped/xz sizes, sha256, shared exports, dynamic deps → `bench/results/songcore-v1/reference-artifacts.json`. |
| `check_api_doc.py` | Fails unless `docs/songcore-api.md` documents every `SONGCORE_API` symbol in the header and the shared export table matches the header exactly. |
| `play_smoke.py` | Thin wrapper forwarding to `songcore_ffi_smoke.py --play` — one canonical audible path. Test-only sounddevice sink by rule (AGENTS.md §8); never decodes compressed audio itself. |

Build workflow: `docs/ffmpeg-minimization.md`. Regression:
`tests/songcore/README.md`. Verified-consumer matrix:
`bench/results/songcore-v1/ffi-consumers.json`.

## PlayerEngine reference model (Phase 1)

| Tool | Role |
|---|---|
| `player_model/scenarios.py` | Executable semantic oracle for the PlayerEngine (`docs/player-engine.md`): 20 deterministic + seeded-randomized gates — ring invariants/wraparound, T1–T15 lifecycle (seek epoch / stale-frame, EOF drain, pause/stop, clock, error injection), randomized stress with invariants checked after every op, buffer-duration sweep. Pure stdlib. Not shipping code. |
| `player_model/model_equivalence.py` | Python↔native equivalence gate driver: replays generated traces against the oracle and the native trace runner (`tests/player/trace_runner.cpp`), comparing every observable after every op; prints seed/op/first-diff on failure. Run via `xmake test player_trace_runner` (200×300) or `--seeds 1000 --ops 500` for the heavy gate. |
