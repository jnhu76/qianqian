# Scripts

Small operational entry points. The build authority is `native/xmake.lua` (+ `native/build/`); these
scripts only fetch the pinned upstream source.

## fetch-ffmpeg

```text
scripts/fetch-ffmpeg [--force]
```

Downloads and verifies the pinned FFmpeg source tree
(`native/ffmpeg/pin.json` → `build/ffmpeg-src/` + a local zip cache). Integrity
comes from the zip sha256 and the tag→commit sha recorded in the pin.
The source tree is never committed to the repo.

After fetching, `xmake ffmpeg-import` resolves the compile closure and
`xmake build songcore ...` replays it (see `docs/architecture/ffmpeg-minimization.md`).
