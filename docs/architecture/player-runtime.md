# Player runtime architecture

> Purpose: PlayerEngine 运行时怎样组成——组件、数据结构、线程拓扑、验证接线。
> Scope: 当前 native PlayerEngine 的内部结构。对外承诺的行为（状态机、
> seek/EOF 生命周期、silence、clock、线程规则）由
> [contracts/player-api.md](../contracts/player-api.md) 拥有；本文不重复。

```text
SongCore (frozen ABI v1: source-rate Float32 PCM, seek, EOF)
     ↓
PlayerEngine
     ├ decode worker
     ├ bounded PCM queue (SPSC)
     ├ state machine / epochs / timeline clocks
     ├ AudioEngine hook (BYPASS in v1)
     └ AudioBackend hook (NullBackend → WASAPI)
```

Kotlin/UI participates only through control calls and a polled snapshot;
it never touches the realtime PCM loop.

## Components

### Decode worker

Single producer thread: SongCore `read_pcm` → PCM ring → publish.
Publishing is epoch-checked — a result decoded under a stale epoch is
discarded at publish time. The worker publishes only `source_eof` / decode
state and never reads the timeline. Its backpressure/idle sleep is bounded
and re-checked on a short timeout (the lock-free seam cannot wake it);
ring-ahead buffering makes the poll latency irrelevant.

### PCM ring

- Bounded, preallocated, frame-based, **SPSC**: producer = decode worker,
  consumer = render callback. Capacity fixed at open; no growth, ever.
- Ring conservation: `produced == consumed + buffered + flushed`.
- Engine decode conservation (lifetime): `decoded_source_frames ==
  ring_produced + discarded_stale_media + in_flight`.
- Backpressure: the worker begins a chunk only when
  `writable >= min(chunk, remaining)`. Never overwrites unread frames,
  never drops decoded PCM arbitrarily, never grows memory; when full it
  waits. The render side NEVER blocks on the producer.
- Flush (seek / stop / open) empties the queue; `buffered == 0` after
  every flush, and no pre-flush PCM may be consumed afterwards.

### Playback timeline

Fixed-capacity span list mapping the output domain back onto the media
domain (the representation is not frozen; the native implementation is a
256-span ring):

```text
OutputSpan { output_begin, output_end, kind = MEDIA | GAP }
```

**MEDIA** = real media content; **GAP** = synthetic silence/padding with
**zero media duration** (occupies device time, no media interval).

```text
device output domain:  0────100────110────210
                       [ MEDIA ] [ GAP ] [ MEDIA ]
media domain:          0────100        100────200
```

The span ring coalesces contiguous same-kind spans and lazily trims
fully-rendered history into prefix anchors; a pathological window that
still exhausts the store FAILS CLOSED (sticky overflow → deterministic
Error) — never growth, never corruption. The device thread is the
timeline's one runtime owner (ownership rule:
[contracts/player-api.md](../contracts/player-api.md)); its scalar
accounting counters are atomics for the control plane's lock-free polled
snapshot.

### Backend seam drivers

- `NullAudioBackend` implements the device side in-process
  (deterministic, no wall clock) for gates and tests; `submit()` /
  `backend_render()` are TEST-ONLY locked wrappers over the seam (the C
  consumer drives them through `player_test_driver.h`, standing in for a
  real backend's callback).
- The production backend is the CALLER of the seam, not a plugin: the
  Windows WASAPI renderer
  ([platform-audio.md](platform-audio.md)) composes into the runtime
  flavor and drives `fill_output` / `advance_render` from its render
  thread.

## Frame-domain naming

Every frame counter declares its domain in its name
(`decoded_source_frames`, `queued_media_frames`, `submitted_output_frames`,
`rendered_output_frames`, `rendered_media_frames`,
`underrun_silence_output_frames`, `discarded_stale_media_frames`);
ambiguous names (`frames`, `position_frames`) are forbidden. Once SRC
exists, source frames ≠ output frames; counters from different domains are
never compared directly.

## Thread topology

```text
control thread   open / play / pause / stop / seek (serialized)
decode worker    SongCore read → PCM ring → publish (epoch-checked)
audio callback   fill_output: ring read → device buffer, GAP on underrun
device clock     advance_render: proven render progression → mapping →
                 media position
```

The normative ownership and quiesce rules for these four roles are the
Concurrency section of [contracts/player-api.md](../contracts/player-api.md).

## Realtime-path proof

A test binary with a global operator-new counter proves `fill_output` /
`advance_render` / timeline ops perform ZERO heap allocations and take NO
MUTEX, and a control thread provably holding the state mutex does not
block a concurrent fill. Additional realtime gates: GAP zero-fill poison
buffer, timeline no-alloc ops, 6-hour long-run bounded memory,
pathological alternation bounded, production-seam overflow → ERROR,
admission quiescence close-first + waits-for-inflight, snapshot rate
coherence (44.1k/48k × polls), realtime-seam ENDED commit, EOF-publish vs
timeline-writer ownership regression + real-topology ENDED soak.

## Validation

```text
player_gates (native suite, binary exit-code gates):
  ring unit/property/SPSC-thread; lifecycle T1–T14; state-illegal probes;
  clock model; submitted-vs-rendered + MEDIA/GAP mapping S1–S10; ESTIMATED
  offset invariance; clock-corrective T16–T20; capacity sweep;
  thread stress (destruction, seek/stop/open vs decode via before-publish
  barriers, render vs control, EOF vs control, play-to-end);
  realtime contract (see above)
player_consumer_c    pure-C TU over the product ABI (lifecycle, snapshot,
                     no-crash probes)
player_real_songcore_smoke  REAL SongCore + REAL corpus fixtures (FLAC/MP3)
                     + host FILE* I/O: ENDED @ true duration, seek landing
                     in SongCore tolerance, stop → READY @0, error path
```

Run `xmake test` (all gates + the SongCore regression) or the binaries
directly. Sanitizer variants build with
`xmake f --player_san=asan|ubsan|tsan`. SongCore sources/ABI are untouched
(engine consumes the frozen ABI only); audio PCM is unchanged (engine is
above SongCore).

## Dependencies

- SongCore via the frozen ABI only (`song_io`, `song_read_pcm`, `song_seek`,
  typed statuses) — [contracts/songcore-api.md](../contracts/songcore-api.md).
- AudioEngine hook is BYPASS in v1; SRC/DSP decisions:
  [ADR-0002](../adr/0002-songcore-src-boundary.md),
  [ADR-0003](../adr/0003-dsp-libavfilter-audioengine.md).
