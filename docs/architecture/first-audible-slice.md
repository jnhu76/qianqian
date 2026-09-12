# First audible slice — static playback vertical-slice design

> **Status: first static playback vertical-slice design.** This is not a new
> overall architecture authority and carries no second normative copy of any
> constitution: the Playback Foundations remain normative only in
> [`../adr/ADR-PBK-001.md`](../adr/ADR-PBK-001.md), and K0 semantics only in
> [`composition-kernel-0-design.md`](composition-kernel-0-design.md) /
> [`composition-kernel-0-implementation-adr.md`](composition-kernel-0-implementation-adr.md).
> This document fixes one slice: capability boundaries, session ownership, the
> PCM data edge, thread/resource ownership, shutdown order, the Processing
> decision, and explicit non-scope. Facts, decisions and open questions are
> labelled. Everything here is earned by exactly one goal: make one real local
> file travel real capabilities under real K0 lifecycle into a real audio
> device.

---

## 0. Slice definition

```text
real local music file
        ↓
Decode capability (SongCore mechanism)
        ↓
Playback Session (owns one playback)
        ↓
bounded / preallocated PCM edge
        ↓
Output capability (WASAPI mechanism)
        ↓
real Windows audio device
        ↓
EOF / stop → join → release → AppRuntime::dispose()
```

Deliberate non-scope (deferred, not missing): pause, resume, seek, next track,
playlist, multiple sessions, device switch, runtime format switch, hot plugin
replacement, EQ, volume semantics, dynamic DSP insertion, UI, library,
metadata UX. The first runtime graph mutation triggers the Realtime Runtime /
P1–P5 production phase (`ADR-PBK-001` §6, §12) and does not enter this slice.

---

## 1. FACT — reality audit

### 1.1 SongCore source PCM (native/src/songcore_ffmpeg.c, native/include/songcore.h, ABI v1)

```text
sample type      Float32, normalized
interleaving     interleaved (one frame = channels consecutive f32)
sample rate      source stream rate
channel count    source stream count; SONG_CH_* mask; mask 0 = unknown
block semantics  song_read_pcm(h, dst, frame_capacity, &out):
                 SONG_OK (out > 0, <= capacity) / SONG_EOF (out == 0) /
                 typed error (out == 0); partial success defers the error
                 to the NEXT call; frame_capacity == 0 is invalid
EOF semantics    SONG_EOF is terminal, normal, not an error
format guard     mid-stream rate/layout change -> SONG_ERR_STREAM_CHANGE
                 (fail-closed; PCM never contradicts song_info)
```

### 1.2 SongCore raw FFI ownership (crates/qianqian-songcore-sys)

```text
song_open(io, &h) -> song_close(h)      ownership: one handle, one closer
thread-safety    handle is NOT internally thread-safe; calls on one handle
                 must be externally serialized; distinct handles run
                 concurrently
move-safety      the handle is a private C struct pointer; safe to move
                 between threads as long as serialization is preserved
IO               song_io host callbacks (read/seek/size + userdata); no
                 path-based open — the Rust side owns the file and the
                 callback closure
```

### 1.3 WASAPI mechanism (historical evidence only — 8db401f `wasapi_renderer.cpp`)

Recovered mechanism facts, not architecture authority:

```text
mode             shared; event-driven (AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                 auto-reset event); default render endpoint (eRender,
                 eMultimedia)
format           WAVEFORMATEXTENSIBLE, IEEE float32 subformat, interleaved;
                 Tier 1: source rate/channels accepted directly (shared
                 engine mixes to the device mix format itself);
                 Tier 2 (historical): closest float32 match + device-side
                 swr SRC
buffer model     GetBufferSize frames; per event: GetCurrentPadding ->
                 GetBuffer(available) -> fill -> ReleaseBuffer(submitted);
                 event wait bounded (~100 ms) as the stop-latency bound
COM apartment    CoInitializeEx(MTA) on the render thread itself; every COM
                 interface owned and released by that same thread
stop             Stop(); release render client; release client;
                 CloseHandle(event); CoUninitialize() — same thread
not used         exclusive mode, MMCSS, hotplug notifications
```

### 1.4 K0 mechanics (current kernel implementation)

```text
capability services are stored as Rc (control-plane resolution);
thread endpoints must be produced at activation as owned Send objects —
an Rc service never crosses to a worker/render thread

effects unwind strictly LIFO, both on activation raise and on unload,
and always BEFORE the component's on_teardown closure; a provision is
released with its record; therefore "stop -> join -> release" is earned
by registration order (register releases first, stop/join last), never
by destructor accident

activation raise: every registered inverse runs (LIFO), the committed
view is discarded, the fiber lands FAILED with no ghost provision

desired entries carry no config payload; ComponentSpec activation
closures capture what they need
```

### 1.5 Compatibility decision input

```text
source PCM  = float32 interleaved @ source rate/channels (SongCore)
render PCM  = float32 interleaved @ negotiated rate/channels (WASAPI shared)

directly compatible: YES for shared mode — the shared engine accepts the
float32 source format (historical Tier 1 "BYPASS") and converts to the
device mix format itself.
```

---

## 2. DECISION — ownership model

The ownership audit compared two models per mechanism and applied one rule:
*semantic ownership scope must match physical storage/lifetime granularity.*

A `song_handle` and an active render stream both live exactly one playback
episode — open, run, close. Kernel providers are long-lived capability
holders; a provider instance bound to one file/device episode would be
per-playback state living in the wrong granularity and would fork against
K0's multi-instance composition semantics.

```text
DECISION  Decode Plugin  = capability provider (long-lived mechanism)
          Output Plugin  = capability provider (long-lived mechanism)
          Playback Session = the one playback episode owner

Decode Plugin owns:     the SongCore mechanism binding (ABI check, host-IO
                        callback machinery); provides open_source(path)
Output Plugin owns:     the WASAPI mechanism (COM call sequences,
                        negotiation, render loop code); provides
                        open_stream(format, frame source, drain signal)
Playback Session owns:  one PcmSource endpoint (one song_handle), the
                        bounded PCM edge, the decode worker thread, one
                        acquired render stream (including its render
                        thread), the completion signal, EOF/stop
                        orchestration
```

## 3. DECISION — capabilities and crates

Capability contracts are defined once, beside their service traits, in
`qianqian-core::ports` (the contract definition site — capability identity is
this definition site). Contracts speak PCM, never FFmpeg/SongCore vendor
vocabulary; no `AV*` type, SongCore struct, or FFmpeg enum appears in a
public contract. `AudioOutputCapability` moves from `qianqian-runtime` to
`ports` so both providers and consumers depend on the contract, not on the
composition root.

```text
qianqian-core::ports       PcmFormat, PcmDecode + PcmSource, AudioOutput +
                           RenderStream + PcmFrameSource + DrainSignal,
                           capability keys
qianqian-playback          bounded PcmEdge, Playback Session component,
                           session completion handle   (workspace member)
qianqian-decode-songcore   Decode provider over qianqian-songcore-sys
                           (outside the workspace, like songcore-sys:
                           fail-closed native-artifact dependency)
qianqian-output-wasapi     Output provider; cfg(windows) mechanism,
                           non-Windows activation raises UnsupportedPlatform
                           (workspace member; compiles everywhere, activates
                           for real only on Windows)
qianqian-headless          Host entry; `playback` feature gates the real
                           plugins so the default workspace build keeps
                           compiling without native artifacts
```

No generic Plugin trait, PluginManager, PluginRegistry, PluginContext, event
bus, or service locator is introduced. K0 is unchanged.

## 4. DECISION — PCM data edge

One producer (decode worker), one consumer (render thread), kernel-free.

```text
bounded          fixed frame capacity, chosen once at activation
preallocated     one contiguous f32 ring allocated at activation; the
                 steady-state path performs no allocation
blocking         mutex + two condvars (not_empty / not_full); stop is a
                 flag + notify_all so both sides always unblock
terminals        Eof (drain, then done) and Failed (stop consuming)
block shape      contiguous f32 samples; producer writes from a
                 worker-owned staging buffer refilled by song_read_pcm;
                 consumer reads straight into caller-provided dst
capacity         8192 frames (~185 ms @ 44.1 kHz, ~170 ms @ 48 kHz);
                 measured decode p99 is ~0.1–0.2 ms per 1024-frame block
                 (decode-cost-model.md §5) — three-plus orders below the
                 capacity horizon; capacity is scheduling margin, not a
                 throughput parameter
```

No universal audio graph, no lock-free framework. Simple and correct first.

## 5. DECISION — threads and realtime boundary

```text
decode worker (session-spawned std::thread)
    song_read_pcm into worker staging -> edge write; filesystem/demux/
    decode/allocation-at-startup allowed here

render thread (spawned by the acquired render stream, owned by the session)
    CoInitializeEx(MTA) -> default endpoint -> negotiate (Tier 1: float32
    EXTENSIBLE at the source format) -> event-driven loop:
    WaitForSingleObject(event) -> GetCurrentPadding -> GetBuffer ->
    edge read straight into the device buffer -> ReleaseBuffer.
    Allowed: device waits, PCM copy, underrun handling, stop observation.
    Forbidden: filesystem, SongCore/FFmpeg, K0 resolve/lookup, graph
    construction, per-quantum allocation beyond device interaction.

steady-state firewall: after activation, audio flows with zero K0 work
per quantum (debug_op_count delta checked); capability resolution happens
exactly once per binding, on the activation (control) thread.
```

## 6. DECISION — lifecycle and shutdown order

Activation (control plane, one bounded step):

```text
resolve Decode capability ONCE
resolve Output capability ONCE
open decode endpoint (RAII rides with the decode worker closure)
build bounded edge (preallocate)
open render stream (bounded open verdict; failure = raise)
                                  -> register inverse: stop_and_join stream
spawn decode worker               (spawn failure = raise; the moved
                                   endpoint drops with the failed closure)
                                  -> register inverse: stop edge + join worker
Active
```

Note on the decode endpoint (implementation differential, resolved): the
design first sketched a dedicated release-source effect. The implemented
and current shape is simpler and equivalent for every path: the endpoint
is owned by the decode worker closure, so it is released exactly when the
worker joins (which the unwind ordering places before any stream
release), and drops on spawn failure or earlier raises via plain RAII.
There is no second owner and no destructor accident — the ordering is
still registration-order discipline.

Effects unwind strictly LIFO (kernel §H.1), so disposal runs exactly

```text
stop edge signal -> decode worker joins (endpoint released with it)
                 -> render stream stops, render thread joins, device released
                 -> providers may teardown later via their own effects
```

which is the required stop → join → release order, earned by registration
order — not by destructor accident. `on_teardown` is not used for resource
release (kernel effects always unwind before `on_teardown`).

EOF / drain (session truth, not K0 truth):

```text
decoder EOF -> edge Eof -> render drains the edge -> final frames submitted
-> device padding reaches 0 (bounded wait) -> drain signal -> session
completion = Completed
```

Failure paths: decode error sets the edge terminal Failed (render stops,
worker exits) → completion = Failed; device failure stops the edge (worker
exits) → completion = Failed. Stop: the stop inverse covers producer-blocked
(full edge), consumer-blocked (empty edge), and already-finished legs.

The session completion handle is session-owned; the Host waits on it and
then drives `AppRuntime::dispose()` explicitly. The Host never pumps PCM,
decodes, or owns a render loop.

## 7. DECISION — Processing

Not required for this slice: SongCore's float32 interleaved source PCM is
directly consumable by WASAPI shared mode (audit §1.5). No Processing Plugin
exists in this slice; no EQ/gain/effects code exists. If a real device
refuses the float32 source format, activation fails honestly; the minimal
conversion (SRC/rematrix) is then earned as a separate decision. (OPEN-1.)

## 8. DECISION — platform behavior

WASAPI is Windows mechanism. The Output provider compiles on all platforms;
on non-Windows its activation raises UnsupportedPlatform — loudly, never a
silent fake success, null output, or dummy render. CI stays Linux-clean; the
real-sound gate runs on Windows with a real output device.

## 9. OPEN

```text
OPEN-1  Tier-2 format fallback (closest float32 + SRC) — deferred until a
        real device refuses Tier 1; would be earned as minimal conversion.
OPEN-3  Mix-format-native rendering (rendering at the device mix format
        instead of the source format) — not needed while Tier 1 holds.
```

RESOLVED at gate time: OPEN-2 (the Windows native artifact recipe) — the
`windows-x86_64` mingw-cross target recipe was derived through the
documented fail-closed import (`ffmpeg_import.py --recipe
windows-x86_64`) and the real-sound gate ran on it.

Everything else outside §0 is deliberate deferral, not open design debt.

## 10. Real-sound gate record (2026-09-12)

```text
entry        qianqian-headless <file>   (windows x86_64, mingw-w64 closure)
file         reference corpus fixtures (real encodings, SHA-verified)
codec        mp3 (mp3float), flac
source       44100 Hz, 2 ch, mask 0x3 (SongCore probe)
negotiated   44100 Hz, 2 ch, mask 0x3 — Tier 1 direct, shared mode,
             event-driven, 970-frame device buffer (Realtek endpoint)
program      EOF "played out completely" (padding drained to zero),
             exit code 0, quiet disposal          — both fixtures
audible      YES — human-confirmed on the fixtures and re-confirmed on a
             full-length real song (张韶涵《隐形的翅膀》mp3, complete file,
             natural EOF, exit 0)
```

Verdict: **FIRST_AUDIBLE_SLICE = PASS** (human-confirmed 2026-09-12).
