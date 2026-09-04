# Local AGENTS — native/

## Local scope

- Native Audio Core implementation (`src/`), source-controlled public ABI
  (`include/`), pinned FFmpeg dependency intent (`ffmpeg/`), native build
  modules (`build/`), native contract proofs (`tests/`).
- `xmake.lua` here is the native build router; ownership lives in
  `build/{ffmpeg,songcore,player,wasm}.lua`. The root `xmake.lua` stays a
  thin workspace entry — never add native definitions back to it.

## Forbidden dependencies

- Application/product-layer code (playlist, library, navigation, UI state)
  must not enter this tree.
- `native/build/*.lua` must not reference `integration/` targets; the
  dependency direction is integration → native, never reverse.

## Required authorities

- `docs/architecture/audio-core.md` before changing the SongCore build or
  `native/include/songcore.h`.
- `docs/architecture/ffmpeg-minimization.md` before changing
  `build/ffmpeg.lua` or `ffmpeg/` (closure derivation, identity gate,
  upgrade rule).

## Local verification

- From the repository root: `xmake build songcore && xmake test` must pass
  for any build-affecting change; report corpus, build profile, and binary
  size delta per root AGENTS §7.
- WASM-session changes additionally need their session gates, or are
  recorded `CODE_COMPLETE_PENDING_VALIDATION`.
