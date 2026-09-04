# native/

Native Runtime / Native Audio Core: implement, build, verify, package.

- `xmake.lua` — native build router: native-wide session configuration (wasm
  session toolchain gate) + `includes()` of the ownership modules below.
- `build/` — ownership-scoped build modules:
  `ffmpeg.lua` (closure import/replay + fail-closed identity gate),
  `songcore.lua` (SongCore artifacts + its instruments),
  `player.lua` (PlayerEngine + `qianqian_runtime`),
  `wasm.lua` (WASM session support).
- `include/` — source-controlled public ABI: `songcore.h` v1,
  `player_engine.h` v1.
- `src/` — `songcore_ffmpeg.c`, `player/`, `wasm/`.
- `ffmpeg/` — pinned dependency intent: `pin.json`, `capabilities/`,
  `profiles/`, `targets/`. No FFmpeg source here; the pinned upstream source
  is fetched into `build/ffmpeg-src/`.
- `tests/` — native contract proofs: `songcore/`, `player/`, `consumer/`.

Build from the repository root; all frozen commands are unchanged
(`xmake ffmpeg-import`, `xmake build songcore`, `xmake test`, …).
Authority routing: root `AGENTS.md` → `docs/README.md`.
