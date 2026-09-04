# Platform audio architecture

> Purpose: AudioBackend 当前实现——WASAPI renderer 如何与 PlayerEngine seam
> 组成真实输出路径，以及 application-facing runtime 的打包形态。
> Scope: 平台输出层与 runtime 组成。seam 的 normative 实时规则由
> [contracts/player-api.md](../contracts/player-api.md) 拥有；SRC 决策见
> [ADR-0002](../adr/0002-songcore-src-boundary.md)。

## Components

- **WasapiRenderer** (`native/src/player/wasapi_renderer.*`, Windows-only,
  compiled solely in the runtime flavor) — the AudioBackend seam's first
  real implementation: default render endpoint, shared mode, one playback
  stream, event-driven.
- **Runtime composition** — one application-facing dynamic library:

```text
qianqian_runtime (shared, basename "qianqian" → qianqian.dll / libqianqian.so)
  files    native/src/player/player_engine_c.cpp  (+ wasapi_renderer.cpp, Windows)
  defines  SONGCORE_BUILD_SHARED, PLAYER_ENGINE_BUILD_SHARED
  symbols  hidden (dllexport only) — same recipe as the audited songcore.dll
  deps     player_core + songcore_static (FFmpeg closure merged inside)
  exports  the 9 pe_* + 15 song_* frozen ABI symbols, nothing else
```

On Windows the runtime flavor composes the renderer into
`pe_create`/`pe_destroy` (guarded by `QN_QIANQIAN_RUNTIME`): the shim
handle is a tiny struct `{ PlayerEngine* engine; WasapiRenderer*
renderer; }`; the other `pe_*` entry points unwrap through one `self()`
accessor. Destruction order: renderer stop+join **then** engine
destruction — after `pe_destroy` returns, no thread can touch the engine.
Tests never define the runtime macro, so `player_consumer_c` and the
gates keep driving the NullAudioBackend manually (single-consumer seam
rule preserved).

`songcore_static` / `songcore_shared` remain internal to the qianqian
product runtime; the product application never links them directly
([contracts/ffi-boundary.md](../contracts/ffi-boundary.md)). Standalone
SongCore integrators are the separate, legitimate consumer of
`libsongcore.a` / `libsongcore.so` / `songcore.dll`
([contracts/songcore-api.md](../contracts/songcore-api.md)).

## Device scope

`IMMDeviceEnumerator/IMMDevice/IAudioClient/IAudioRenderClient/
WAVEFORMATEXTENSIBLE(float32)` + `CoInitializeEx(MULTITHREADED)` on the
render thread (init and `CoUninitialize` on the same thread), minimal
local RAII, IIDs defined locally. No avrt/MMCSS, no exclusive mode, no
raw, no notifications, no hotplug / endpoint callbacks.

## Format path

1. First try Float32 / source rate / source channels
   (`WAVEFORMATEXTENSIBLE`) — accepted → zero-conversion BYPASS.
2. On `AUDCLNT_E_UNSUPPORTED_FORMAT` ask `IsFormatSupported` for the
   closest match and accept it **iff it is float32** (any rate/channel
   count): the device then runs the closest format and the renderer
   converts with **libswresample** — the device-side SRC the frozen design
   names ([ADR-0002](../adr/0002-songcore-src-boundary.md)). The shared
   engine does NOT convert; swr does. swresample is already inside the
   frozen FFmpeg closure (codec-base profile), so no closure change.
3. A non-float32 closest (or a second failure) → renderer stays silent
   (bounded retry); the frozen position is the honest "no output" signal.
   A hand-written resampler remains out of scope.

Machine evidence (measured host): `IsFormatSupported(44.1k f32 stereo)` →
S_FALSE closest 48k, `Initialize(44.1k)` → `AUDCLNT_E_UNSUPPORTED_FORMAT`,
`Initialize(48k)` → S_OK.

## Period loop (running)

`GetCurrentPadding` → `GetBuffer(available)` →
`fill_output(pData, available)` → `ReleaseBuffer(available)` → `padding`
again → `rendered = written − padding` →
`advance_render(rendered − last_advanced)`. Zero copy: the device buffer
IS the fill destination. Submitted ≠ rendered is honored exactly: media
position advances only when the padding arithmetic proves playout.

In swr-converted mode the engine PCM lands in an input staging buffer, swr
converts into the device buffer, and `advance_render` receives the media
equivalent of proven playout (exact integer ratio
`out_rendered × src_rate / dst_rate`; ≤ 1 frame over-report, clamped by
the engine's pending clamp so it can never exceed appended media).

## Idle and format-change handling

- `fill_output` returns `idle` → `ReleaseBuffer(0)`, then an
  `advance_render(0)` probe distinguishes PAUSED (`"paused"` → `Stop()`
  only: pending device output stays pending, content continues on resume)
  from quiesced/not-playing (`"idle"/"rendered"` → `Stop() + Reset()`:
  submitted-but-unrendered frames die, matching the commit's timeline
  reset; render accounting is rebased so no phantom advance). Never submit
  untouched `dst` (no uninitialized/stale PCM).
- Format change: only possible across a commit, and every commit passes
  through `idle`; the renderer re-initializes its client when the first
  post-idle activation requests a different format. Playing stretches
  never span a format change, so the steady path never re-inits.

## Failure behavior

Any WASAPI call failure (e.g. `AUDCLNT_E_DEVICE_INVALIDATED`, no
endpoint) tears the stream down and leaves the renderer silent with a
bounded, format-change-or-2s-gated retry. The frozen position is the
honest "no output" signal; degradation never produces new conversion code
or a fake playing state.

## Thread model

Exactly one backend-owned thread, created at `pe_create`, joined in the
renderer destructor. No detached threads, no pools, no unbounded queues.
Idle (engine not playing / quiesced): the thread sleeps on a bounded timed
wait (stop is checked atomically, so destroy wakes/join is bounded);
playing: event-driven. This thread is the single owner that serializes
`fill_output` / `advance_render`
([contracts/player-api.md](../contracts/player-api.md), Concurrency).

## Invariants

- PCM correctness: SongCore emits Float32 interleaved source-rate PCM; the
  engine queue is frame-accounted; `bytes = frames × channels × 4` for
  every submitted span. `fill_output` guarantees media+silence ==
  requested when admitted and playing (underrun/preroll/EOS spans
  zero-filled), so `ReleaseBuffer` never sends uninitialized bytes; `idle`
  periods submit nothing. Stale PCM cannot replay: commits drop the device
  buffer via `Reset` and the epoch/generation guard drops late render
  events.
- Export surface: the runtime exports exactly the 24 frozen ABI symbols;
  FFmpeg stays statically merged inside with hidden visibility
  (machine-audited).

## Verification

- `integration/ffi/ffi_smoke.c` — pure C99 external consumer, loads the
  runtime through `LoadLibraryA` / `dlopen`, drives create → open → play →
  polled snapshots → seek → stop → destroy. On Windows, PLAYING with
  `position_us` advancing proves real render progression through WASAPI;
  without progression it reports SKIP with the reason instead of a fake
  PASS.
- Export/import audits (24 symbols; imports bcrypt / KERNEL32 / msvcrt /
  ole32 only) recorded with the boundary-probe evidence
  ([archive/experiments/kotlin-boundary-probe.md](../archive/experiments/kotlin-boundary-probe.md)).
