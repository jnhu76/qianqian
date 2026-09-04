# Native Runtime Closure v1 — WASAPI Minimal Backend + Stable KMP/FFI Boundary (closed)

> Historical / non-authoritative。这是 Native Runtime Closure v1 的 phase
> document，阶段已完成并验证。current truth 拆分至：
> WASAPI renderer 与 runtime 组成 →
> [architecture/platform-audio.md](../../architecture/platform-audio.md)；
> seam 行为规则 → [contracts/player-api.md](../../contracts/player-api.md)；
> FFI 边界 → [contracts/ffi-boundary.md](../../contracts/ffi-boundary.md)。
> 文中引用的文档路径已指向 current 位置；`spec §NN` 是当期 phase 文档的
> 内部编号，仅作历史线索。


Phase document (spec §46): the audit results, the frozen contracts this
phase builds on, and the minimal composition that closes the native
playback runtime. 当期 authority order（历史）：PRD → audio-core → player-engine →
this document；现已由 DOCS-IA-2 taxonomy 取代。

## 1. Current ABI (audit answer A/B/F)

| Surface | File | Status |
|---|---|---|
| SongCore ABI v1 | `include/songcore.h` (15 `SONGCORE_API` symbols) | frozen, untouched |
| PlayerEngine ABI v1 | `include/player_engine.h` (9 `PE_API` symbols) | frozen, untouched |

`pe_*` already exposes the complete existing feature set: create/open/play/
pause/stop/seek/snapshot/destroy + abi_version, with the `PE_API` shared-
export macro pre-wired (`PLAYER_ENGINE_BUILD_SHARED`). **No new C ABI is
added this phase** (spec §19/§20): the runtime closure is packaging +
composition, not API growth.

## 2. The seam (audit answer D/E)

player-engine（现 contracts/player-api.md）§9/§10 fixes the production topology: ONE
backend-owned event-driven render thread calls
`fill_output(float* dst, uint64_t frames)` then `advance_render(int64_t)`.
Both are mutex-free, allocation-free, admission-gated (close-then-drain on
every control commit). GAP silence is physically zeroed in `dst`; an `idle`
return means "no output this period" and `dst` is deliberately untouched.
PlayerEngine holds its `NullAudioBackend` (test driver) internally; the
production backend is the CALLER of the seam, not a plugin.

**Seam corrective (Case B, minimal):** `OutputFillResult` (internal C++
struct, not the frozen C ABI) gains `source_rate` / `channels`, read in the
same admitted window as the existing `ring_.channels()` read. A real device
must negotiate a stream format; no other engine change. PlayerEngine
semantics, PCM contract, epoch/EOS/reset model: unchanged.

## 3. Runtime composition (audit answer C/H)

Today `songcore_shared` carries only SongCore and `player_core` is static —
no shared artifact can host playback. One new target:

```text
qianqian_runtime (shared, basename "qianqian" → qianqian.dll / libqianqian.so)
  files    src/player/player_engine_c.cpp  (+ wasapi_renderer.cpp, Windows)
  defines  SONGCORE_BUILD_SHARED, PLAYER_ENGINE_BUILD_SHARED
  symbols  hidden (dllexport only) — same recipe as the audited songcore.dll
  deps     player_core + songcore_static (FFmpeg closure merged inside)
  exports  the 9 pe_* + 15 song_* frozen ABI symbols, nothing else
```

One application-facing dynamic library (spec §2): KMP gets
`player_engine.h + qianqian.dll`; it never links `libsongcore.a`, never sees
FFmpeg/C++/WASAPI headers. `songcore_static`/`songcore_shared` remain
internal build artifacts. The release classification stays
ENGINEERING_FREE (spec §30).

On Windows the runtime flavor additionally composes the renderer into
`pe_create`/`pe_destroy` (guarded by `QN_QIANQIAN_RUNTIME`): the shim handle
is a tiny struct `{ PlayerEngine* engine; WasapiRenderer* renderer; }`; the
other `pe_*` entry points unwrap through one `self()` accessor. Destruction
order: renderer stop+join **then** engine destruction — after `pe_destroy`
returns, no thread can touch the engine. Tests never define the runtime
macro, so `player_consumer_c` and the gates keep driving the
NullAudioBackend manually (single-consumer seam rule preserved).

## 4. WasapiRenderer (the only new production component)

Files: `src/player/wasapi_renderer.hpp` (platform-neutral text: engine
reference + ctor/dtor only), `src/player/wasapi_renderer.cpp`
(Windows-only, compiled solely in the runtime flavor). ~300 LOC target.

- **Device scope:** default render endpoint, shared mode, one playback
  stream, event-driven. `IMMDeviceEnumerator/IMMDevice/IAudioClient/
  IAudioRenderClient/WAVEFORMATEXTENSIBLE(float32)` + `CoInitializeEx(MULTITHREADED)`
  on the render thread (init and `CoUninitialize` on the same thread),
  minimal local RAII, IIDs defined locally. No avrt/MMCSS, no exclusive
  mode, no raw, no notifications.
- **Format path:** first try Float32 / source rate / source channels
  (`WAVEFORMATEXTENSIBLE`) — accepted → zero-conversion BYPASS. On
  `AUDCLNT_E_UNSUPPORTED_FORMAT` ask `IsFormatSupported` for the closest
  match and accept it iff it is float32 (any rate/channel count): the
  device then runs the closest format and the renderer converts with
  **libswresample** — the device-side SRC the frozen design names
  (audio-core（现 architecture/audio-core.md）: "SRC = aresample / libswresample"; player-engine（现 contracts/player-api.md）§10:
  "BYPASS when source rate/layout == device requirement, else
  aresample / libswresample — owned by the device side"). Machine evidence
  (this host): `IsFormatSupported(44.1k f32 stereo)` → S_FALSE closest
  48k, `Initialize(44.1k)` → `AUDCLNT_E_UNSUPPORTED_FORMAT`,
  `Initialize(48k)` → S_OK — the shared engine does NOT convert; swr does.
  swresample is already inside the frozen FFmpeg closure (codec-base
  profile: `swr_alloc/swr_init/swr_convert/swr_get_delay` defined in
  `libsongcore.a`), so no closure change. A non-float32 closest (or a
  second failure) → renderer stays silent (bounded retry); the frozen
  position is the honest "no output" signal. A hand-written resampler
  remains STOP-5 territory.
- **Thread model (§14):** exactly one backend-owned thread, created at
  `pe_create`, joined in the renderer destructor. No detached threads, no
  pools, no unbounded queues. Idle (engine not playing / quiesced): the
  thread sleeps on a bounded timed wait (stop is checked atomically, so
  destroy wakes/join is bounded); playing: event-driven.
- **Period loop (running):** `GetCurrentPadding` → `GetBuffer(available)` →
  `fill_output(pData, available)` → `ReleaseBuffer(available)` →
  `padding` again → `rendered = written − padding` →
  `advance_render(rendered − last_advanced)`. Zero copy: the device buffer
  IS the fill destination. Submitted ≠ rendered is honored exactly: media
  position advances only when the padding arithmetic proves playout.
  In swr-converted mode the engine PCM lands in an input staging buffer,
  swr converts into the device buffer, and `advance_render` receives the
  media equivalent of proven playout (exact integer ratio
  `out_rendered × src_rate / dst_rate`; ≤ 1 frame over-report, clamped by
  the engine's pending clamp so it can never exceed appended media).
- **Idle handling:** `fill_output` returns `idle` → `ReleaseBuffer(0)`, then
  an `advance_render(0)` probe distinguishes PAUSED (`"paused"` → `Stop()`
  only: pending device output stays pending, content continues on resume)
  from quiesced/not-playing (`"idle"/"rendered"` → `Stop() + Reset()`:
  submitted-but-unrendered frames die, matching the commit's timeline
  reset; render accounting is rebased so no phantom advance). Never submit
  untouched `dst` (§13: no uninitialized/stale PCM).
- **Format change:** only possible across a commit, and every commit
  passes through `idle`; the renderer re-initializes its client when the
  first post-idle activation requests a different format. Playing stretches
  never span a format change, so the steady path never re-inits.
- **Failure:** any WASAPI call failure (e.g. `AUDCLNT_E_DEVICE_INVALIDATED`,
  no endpoint) tears the stream down and leaves the renderer silent with a
  bounded, format-change-or-2s-gated retry. No hotplug, no endpoint
  callbacks (non-goals).

## 5. PCM correctness (spec §11/§12/§13)

SongCore emits Float32 interleaved source-rate PCM; the engine queue is
frame-accounted; `bytes = frames × channels × 4` for every submitted span.
`fill_output` guarantees media+silence == requested when admitted and
playing (underrun/preroll/EOS spans zero-filled), so `ReleaseBuffer` never
sends uninitialized bytes; `idle` periods submit nothing. Stale PCM cannot
replay: commits drop the device buffer via `Reset` and the epoch/generation
guard drops late render events.

## 6. External consumer (spec §32/§50)

`tests/ffi_smoke/ffi_smoke.c` — pure C99, includes ONLY
`include/player_engine.h`, loads the runtime through `LoadLibraryA` /
`dlopen` + `GetProcAddress` (no import lib, no internal headers, no
implementation archives). Drives: create → open (real corpus fixture via
host `FILE*` `song_io`) → play → polled snapshots → seek → stop → destroy.
On Windows, PLAYING with `position_us` advancing proves real render
progression through WASAPI (real-endpoint evidence); without progression it
reports SKIP with the reason instead of a fake PASS (§39).

## 7. Expected diff budget (spec §43/§44)

```text
src/player/wasapi_renderer.hpp/.cpp   ~320 LOC production (new, Windows-only)
src/player/player_engine_c.cpp        ~25 LOC delta (shim struct + guards)
src/player/player_engine.{hpp,cpp}    ~6 LOC delta (OutputFillResult fields)
xmake.lua                             ~30 LOC (runtime + ffi_smoke targets)
tests/ffi_smoke/ffi_smoke.c           test code
docs/wasapi-native-runtime-closure.md this document
```

## 8. Stop conditions honored

STOP-2/3 (ABI/semantics redesign): not triggered — pe_* and engine
semantics unchanged. STOP-5/6 (general resampler/mixer): delegated to the
OS audio engine; a failed format negotiation degrades to silence, never to
new conversion code. STOP-9 (lifecycle): renderer-owns-thread + join-before-
engine-destroy closes the lifetime proof. STOP-10/11: one DLL, one
backend, LOC within budget.
