# native/

SongCore: the Media→PCM decode mechanism.

```text
media (host bytes) → SongCore C ABI → trimmed FFmpeg decode closure
                       → canonical Float32 interleaved PCM (source rate/layout)
```

This tree is a self-contained Xmake project (run xmake from `native/`).
It contains only the decode mechanism and its pinned FFmpeg substrate.
There is no player, no WASM, no DSP/processing surface, and no Rust
integration here.

- `xmake.lua` — build router.
- `build/` — ownership modules: `ffmpeg.lua` (closure import/replay +
  fail-closed target identity gate), `songcore.lua` (static/shared
  artifacts).
- `include/songcore.h` — public ABI v1 (15 exports; no FFmpeg type crosses
  it).
- `src/songcore_ffmpeg.c` — mechanism implementation.
- `ffmpeg/` — pinned dependency intent: `pin.json`,
  `profiles/codec-base.json` (the shipping closure),
  `targets/linux-x86_64.json` (proven target recipe). No FFmpeg source
  here; the pinned upstream tree is fetched into `build/ffmpeg-src/`.
- `scripts/fetch-ffmpeg` — fetch + verify the pinned source (sha256 + tag
  recheck).
- `tools/ffmpeg_import.py` — the import-time oracle: runs upstream
  configure/Make once, freezes the compile closure into
  `build/ffmpeg-xmake/manifest.json`; normal builds replay it.
- `experiments/songcore-equivalence/` — native-only port-equivalence
  experiment (correctness + performance vs the frozen historical
  baseline).

## Build (Linux x86_64)

```bash
cd native
xmake ffmpeg-import        # fetch pinned FFmpeg + derive closure (once)
xmake f -m release
xmake build songcore       # libsongcore.a + libsongcore.so under build/artifacts/
```

`libswresample` is part of the decode closure only as a transitive
dependency of the native Opus decoder; SongCore never resamples (PCM
contract: source rate/layout).

The FFmpeg compile closure is generated state: `build/` may be deleted and
reproduced from scratch by rerunning the commands above.
