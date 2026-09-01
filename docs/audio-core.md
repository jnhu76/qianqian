# Qianqian Audio Core

The single human authority for the Native Audio Core. UI technology is
replaceable; the Native Audio Core is a small, reusable library behind a
stable C ABI.

```text
Product / UI (Kotlin / KMP / Swift / Qt / future)
        │
        │ stable FFI / C ABI
        ▼
Native Audio Core
        ├── SongCore     parse · metadata · artwork · stream selection ·
        │                decode · seek · typed errors
        └── AudioEngine  SRC = aresample / libswresample
                         DSP = capability-trimmed libavfilter
        ▼
platform AudioBackend (device negotiation, buffering, clock, output)
```

## What SongCore does

```text
local byte source
  → parse container
  → select audio stream
  → metadata / artwork
  → decode
  → source Float32 PCM
```

- Output: **Float32, interleaved, source sample rate, source channel
  layout** — always, for every codec.
- Decode backend: **trimmed FFmpeg n9.0.1** (pin: `ffmpeg/pin.json`,
  capability intent: `ffmpeg/capabilities/songcore.json`, see
  [ffmpeg-minimization.md](ffmpeg-minimization.md)).
- SongCore does **not** resample, rematrix, run DSP/EQ/ReplayGain,
  normalize, limit, crossfade, or touch a device.
- Public contract: `include/songcore.h` (ABI v1, frozen). No FFmpeg type
  crosses the boundary.
- Host I/O is fully caller-provided (`song_io` read/seek/size): no
  filesystem, network, or URI semantics inside SongCore.

## What AudioEngine does

Optional, post-decode PCM processing:

```text
source format == required output format  →  BYPASS (no work)
otherwise                                →  aresample / libswresample
```

DSP is a capability-trimmed libavfilter graph (intent:
`ffmpeg/capabilities/dsp.json`): volume/preamp, parametric/graphic EQ,
tone, filters, and graph plumbing. Processing is opt-in per capability;
nothing runs unless a pipeline asks for it.

## What AudioBackend does

Future platform output layer: device negotiation, device buffering, clock,
and actual audio output (WASAPI / CoreAudio / AAudio / ALSA / PipeWire).
Not part of this codebase yet; SongCore ends at source PCM + song
information.

## Frozen implementations

| Concern | Decision |
|---|---|
| Decode | trimmed FFmpeg n9.0.1 (SongCore) |
| DSP | capability-trimmed libavfilter (AudioEngine) |
| SRC | aresample / libswresample, conditional BYPASS |
| Native build | Xmake (`xmake.lua`) |
| Public ABI | `include/songcore.h`, v1 |

## UI boundary

Kotlin/KMP/Compose/Swift/Qt/etc. communicate with the core **only** through
the stable C ABI (`songcore.h`), via JNI / JNA / cinterop / equivalent FFI,
against the Xmake-built native library. No UI code lives in this repo.

## Repository map

```text
include/songcore.h        public ABI v1 (frozen)
src/songcore_ffmpeg.c     SongCore implementation over pinned FFmpeg
src/wasm/                 WASM bridge (songcore_wasm_bridge.c)
ffmpeg/                   pin + capabilities + target recipes + profiles
tools/ffmpeg_import.py    FFmpeg import/oracle (see minimization doc)
xmake.lua                 Native Audio Core build authority
tests/songcore/           permanent regression (regression.py + probe)
bench/results/songcore-v1 machine authority tree (fail-closed --check)
docs/songcore-api.md      how callers call the library (permanent)
docs/ffmpeg-minimization.md  how FFmpeg is trimmed
docs/wasm.md                 WASM viability result
docs/history.md              how the decisions were reached
```

Regression: see `tests/songcore/README.md`. Boundaries and excluded
capabilities: `docs/architecture/negative-capability-manifest.md`.
