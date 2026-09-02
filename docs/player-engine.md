# PlayerEngine v1 — Frozen Semantics

Semantic authority for the native PlayerEngine. Semantics were originally
validated against an executable reference model during Phase 1; the
permanent native regression suite in `tests/player/` now owns these
contracts (inventory in §11).

```text
SongCore (frozen ABI v1: source-rate Float32 PCM, seek, EOF)
     ↓
PlayerEngine   ← this document
     ├ decode worker
     ├ bounded PCM queue (SPSC)
     ├ state machine / epochs / timeline clocks
     ├ AudioEngine hook (BYPASS in v1)
     └ AudioBackend hook (NullBackend → WASAPI)
```

Kotlin/UI participates only through control calls and a polled snapshot;
it never touches the realtime PCM loop.

## 1. Units

- The accounting unit is the **frame**: one frame = one sample for every
  channel. Frames, never bytes or samples, cross every internal boundary.
- SongCore positions are microseconds; the engine converts on the way in
  (round-to-nearest clock rebase) and clamps against the known duration.
- SongCore seek returns the **actual landing position**; the engine bases
  the clock on the RETURNED landing, never on the request (§6). `-1` with
  `SONG_OK` means the landing is genuinely unknown (§6.1).
- Duration may be **unknown** (`duration_us == -1`), represented explicitly
  (`duration_known == false`) — never overloaded as `duration == 0`, never
  a fake clamp.

## 2. Playback timeline principle

PlayerEngine strictly separates **device/render time**, **media time**, and
**decode position**. They describe different facts; no single frame counter
may implicitly serve as more than one of them.

> The device clock answers *when in the real world output was rendered*;
> media time answers *what part of the song that output represents*.
> Never infer one from the other without an explicit mapping.

Pipeline chain (distinct domains): `SongCore → Decoded Media PCM → PCM
Queue → Submitted Output → Rendered Output → device→media mapping → Media
Position`. The design must explicitly distinguish
`decoded_source_frames`, `queued_media_frames`, `submitted_output_frames`,
`rendered_output_frames`; do not collapse them. In v1 BYPASS the values are
numerically equal along the chain — that equality is never semantic
identity, and SRC (44.1 kHz source → 48 kHz output) must not break any
accounting assumption.

- **Decode position** may legitimately run far ahead of playback
  (pre-decode is normal); it drives scheduling/buffering/diagnostics only,
  never user-visible progress.
- **Device/render time** advances for every output frame the backend proves
  rendered — media PCM, underrun silence, backend padding (future
  authority: WASAPI `IAudioClock`).
- **Media position** is the best estimate of the currently audible
  position: segment base (landing) + proven-rendered media. Only real
  media-timeline PCM advances it.

### 2.4 Submitted != rendered

> Submitting PCM to the AudioBackend does not make that PCM audible.
> Media Position may advance only when backend clock evidence establishes
> that the corresponding output span has actually been rendered.

`rendered_media_frames` in the clock formula (§8) means media frames whose
output has been **proven rendered by backend/render progression** — never
"copied into the backend" or "removed from the PCM queue".

### 2.5 Timeline mapping

The output domain and the media domain are two coordinate systems. Once a
real backend has pending output and a hardware/render clock, the engine
maintains enough information to map a rendered device/output position back
to the media timeline. The **representation is not frozen**; the native
implementation is a fixed-capacity span list:

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

### 2.6 Frame-domain naming

Every frame counter declares its domain in its name
(`decoded_source_frames`, `queued_media_frames`, `submitted_output_frames`,
`rendered_output_frames`, `rendered_media_frames`,
`underrun_silence_output_frames`, `discarded_stale_media_frames`);
ambiguous names (`frames`, `position_frames`) are forbidden. Once SRC
exists, source frames ≠ output frames; counters from different domains are
never compared directly.

## 3. States

Public states only (no SEEKING/STOPPING leakage across the ABI):
`EMPTY · READY · PLAYING · PAUSED · ENDED · ERROR`. Everything unspecified
is an illegal call → typed error on the control ABI; `pause` and `stop` on
EMPTY are documented no-ops.

| Call (state → effect) | EMPTY | READY | PLAYING | PAUSED | ENDED | ERROR |
|---|---|---|---|---|---|---|
| `open(song)` | → READY | → READY | → READY | → READY | → READY | → READY |
| `play()` | error | → PLAYING | idempotent | → PLAYING | seek 0, → PLAYING | error |
| `pause()` | no-op | no-op | → PAUSED | idempotent | no-op | error |
| `stop()` | no-op | → READY @0 | → READY @0 | → READY @0 | → READY @0 | → READY @0 * |
| `seek(T)` | error | @landing | @landing | @landing | → READY @landing | error |

\* `stop` from ERROR reaches READY @0 only when recovery succeeds (§3.2).

Frozen policies:

- `open` never autoplays: stops everything, drops the previous SongCore
  handle, clears queue AND all pending output (§6), resets the clock,
  opens/probes the new song → READY @0 (fresh handle: CONFIRMED 0).
- `stop` is not `pause`: rewinds the source to 0, flushes queue and pending
  output, resets position → READY.
- `play` from ENDED replays from the beginning (seek 0 → PLAYING); a failed
  restart seek lands in ERROR (fail-closed).
- `seek` from ENDED lands and → READY (play is then required).
- `pause` freezes media progression (position freezes, resume continues
  from the same logical position, device stops consuming — render
  advancement halts, pending output stays pending). Buffered PCM retained.
- A fatal decode error (typed SongCore status, incl. partial-success
  framing: frames before the fault are delivered, the fault surfaces on the
  next call) → ERROR. Already-submitted output may drain audibly; no new
  PCM is submitted.

### 3.1 Seek failure (fail-closed)

```text
seek requested → current generation invalidated, queue flushed, pending
output discarded → SongCore seek fails → PlayerEngine → ERROR
```

From every legal seek state (READY / PLAYING / PAUSED / ENDED) a failed
seek → ERROR. The engine never assumes decoder/demux/buffer/clock state can
safely resume the old timeline, never silently continues PLAYING at the old
position, never reports a false seek-success position (the snapshot keeps
the last audible media position as a frozen diagnostic), and a failed seek
is never relabeled an ESTIMATED landing (§6.1).

### 3.2 ERROR recovery

`stop()` is a **deterministic rebuild**: rewind in place via seek(0) when
the handle is trusted (healthy state); drop and reopen the source when it
is not (ERROR, or failed rewind) — a fresh handle at position 0. If even
the reopen fails (file gone, I/O dead), ERROR persists with a deterministic
diagnostic, and `open(song)` is the guaranteed recovery. The engine never
fakes READY over a source it could not actually reposition.

## 4. PCM queue contract

- Bounded, preallocated, frame-based, **SPSC**: producer = decode worker,
  consumer = render callback. Capacity fixed at open; no growth, ever.
- Ring conservation: `produced == consumed + buffered + flushed`.
- Engine decode conservation (lifetime): `decoded_source_frames ==
  ring_produced + discarded_stale_media + in_flight`.
- Backpressure: the worker begins a chunk only when
  `writable >= min(chunk, remaining)`. Never overwrites unread frames,
  never drops decoded PCM arbitrarily, never grows memory; when full it
  waits. The render side NEVER blocks on the producer.
- Flush (seek / stop / open) empties the queue; `buffered == 0` after every
  flush, and no pre-flush PCM may be consumed afterwards.

## 5. Underrun / preroll / EOS silence

Render callback asks for N frames while PLAYING; the queue holds M:

- `M == N` real frames → deliver as a MEDIA span.
- `M < N`, source NOT at EOF — pad with GAP silence so the realtime thread
  never blocks:
  - no real frame submitted yet in this segment → **preroll**: N−M
    silence, `preroll_events/frames++`; media position does not advance;
  - otherwise → **underrun**: M real frames + N−M silence,
    `underrun_count/frames++`. Device time advances by the full period;
    **media position advances only by the M real media frames**.
- Source EOF + queue drained + nothing in flight → **EOS silence** (not an
  underrun): final partial period completed with GAP padding,
  `eos_silence_frames++`, → ENDED once the media part has rendered (§7).

Canonical example (100 Hz): queue has 4 frames, device requests 10 →
output 4 media + 6 silence, device +100 ms, media +40 ms; the next decoded
media begins exactly after those 4 media frames.

Underrun silence has **device duration and zero media duration**. Playing
silence must never implicitly skip an equal amount of song content, and
media position lagging device elapsed time after an underrun is CORRECT.
Recovery is the producer refilling the queue: the next real media resumes
from the exact next source frame — no jump, no duplication, no loss. After
a 10-second song with 500 ms cumulative underrun silence the media timeline
is still 10 s (device session 10.5 s); injected silence never prolongs
media duration.

The callback itself only reads the preallocated queue, copies, counts, and
returns: no allocation, no SongCore calls, no file I/O, no logging, no
blocking mutex, no Kotlin entry.

## 6. Seek lifecycle (epoch model)

Control thread commits a seek in this exact order:

```text
epoch += 1                      ← invalidates producer output FIRST
flush PCM queue
discard pending output          ← submitted-but-unrendered spans die here
SongCore seek(T) → status + actual landing
AudioEngine.reset()
[on failure → ERROR (§3.1); on success:]
clock rebase: base = landing, rendered_media = 0, new segment
resume according to previous play/pause state
```

- No PCM decoded before the commit may be rendered after it. An in-flight
  decode result carries the epoch it was decoded under; a stale result
  **publishes into a dead epoch and is discarded at publish time**
  (`discarded_stale_media_frames`). Holds for seek, stop, and open alike.
- **Stale output generations**: submitted output is discarded at the
  commit, and render progression is generation-guarded — a late device/
  render event from a dead generation is dropped and can never advance the
  new segment's media timeline (`stale_render_events`).
- Every commit (open / seek / stop / restart-from-ENDED) opens a new
  **segment**, with two deliberately different invariants:
  - **Clock continuity** (production-realizable): public media position ==
    segment base + rendered media duration, monotonic.
  - **Content continuity** (test-only, hidden fake frame identities):
    rendered frames are exactly contiguous from the segment's true first
    frame — no duplication, loss, reordering, or stale frames. CONFIRMED
    segments: true first frame == reported landing. ESTIMATED: may differ
    by the unknown segment offset — the engine never rebases onto hidden
    truth.
- SongCore handle calls on the seek path must be serialized with the decode
  worker (the handle is not thread-safe; see `songcore.h`).

### 6.1 Seek landing quality

```text
CONFIRMED  actual_position_us >= 0 → media base = actual landing
ESTIMATED  SONG_OK, actual == -1   → media base = requested/clamped target
```

ESTIMATED is frozen as: the seek succeeded; the exact first decoded media
timestamp is unavailable; the engine uses the requested/clamped target as
the estimated base; public position is deterministic and monotonic from it;
the decoded content may differ by an unknown constant segment offset; the
engine never fabricates CONFIRMED precision. Within one ESTIMATED segment
the hidden true-vs-reported offset must stay constant unless an explicit
discontinuity occurs (suite: named invariant
`estimated_segment_offset_invariance`). A **failed** seek is neither
CONFIRMED nor ESTIMATED — it is an ERROR (§3.1). Eliminating ESTIMATED
would require a deeper timestamp authority — explicitly NOT an ABI v1
requirement.

Seek resets the output mapping as well as the media clock: after a
successful commit, media base = the actual landing (never a value derived
from device elapsed time), and no pre-seek output span may later mutate the
new segment's media position.

## 7. EOF lifecycle

SongCore EOF does NOT end playback:

```text
SongCore EOF → worker marks source_eof → queue keeps draining
→ queue empty + pipeline quiesced + submitted media output rendered → ENDED
```

ENDED means exactly: all audible PCM belonging to the current media
timeline has completed processing, queue drain, and backend playout.
Submitted-but-unrendered media blocks ENDED; trailing backend-required
non-media padding (EOS GAP) does NOT postpone it once all actual media
output has completed playout. ENDED persists while paused (drain completes
only through render progression); play after ENDED restarts from 0; seek
after ENDED → READY.

ENDED position: known duration → the known duration; unknown duration →
the final rendered media endpoint (the point reached after every real media
frame of the final segment has actually rendered). Synthetic underrun/
padding silence must not extend it; unknown duration never clamps to −1 or
0.

## 8. Clock model

- Two domains, two counters: device/render time and media position are
  distinct and never share one counter (§2).
- Device/render time advances with every output frame the backend proves
  rendered. Output accounting:
  `rendered_output == rendered_media_output + rendered_silence_output`.
- Media position: `position = base(landing) + rendered_media_frames /
  media_rate`, clamped to duration when known. `rendered_media_frames`
  counts only real media-timeline PCM **proven rendered** by backend
  progression (§2.4). Underrun silence advances device time only (§5).
  Pause freezes it; resume continues; seek rebases it to the actual
  SongCore landing.

## 9. Thread ownership

```text
control thread   open / play / pause / stop / seek (serialized)
decode worker    SongCore read → PCM ring → publish (epoch-checked)
audio callback   fill_output: ring read → device buffer, GAP on underrun
                 (§5 rules) — lock-free, no mutex, no allocation
device clock     advance_render: proven render progression → mapping →
                 media position
```

Backend ownership rule: `fill_output()` and `advance_render()` are
serialized by the AudioBackend's SINGLE realtime/device thread — one
event-driven render thread for WASAPI that calls `fill_output` then
queries/advances the device render clock. `PlaybackTimeline` is therefore
NOT a concurrent multi-writer structure and takes no locks. The control
thread may run concurrently ONLY through the admission/quiesce protocol
(§10): a commit closes backend admission, drains counted in-flight ops,
resets ring/timeline/backend, then re-opens admission. No other
concurrency pattern is supported.

Diagnostics for UI are a polled snapshot (state, media position, decode
position, duration(+known flag), buffered, underrun/preroll/stale counters,
submitted/rendered/pending output counters, position quality, last error)
at UI cadence (5–10 Hz); no native→Kotlin callbacks, none from the realtime
path. The product snapshot reports the media timeline in MICROSECONDS
(`position_us` / `duration_us`) — the UI never needs a sample rate.

## 10. Production boundary: two layers

**Layer 1 — product C ABI** (`include/player_engine.h`, the ONLY public
surface). Control + polled observation: `pe_create / pe_destroy /
pe_open / pe_play / pe_pause / pe_stop / pe_seek / pe_get_snapshot` plus
`player_engine_abi_version`. No audio backend, no manual render ticks, no
queue internals, no worker mode, no SongCore surface beyond the `song_io`
open() travels through. Every call returns a typed `pe_status`; invalid
caller input never aborts and no C++ exception crosses the boundary.
Control calls are caller-serialized; `pe_get_snapshot` may be polled
concurrently.

**Layer 2 — internal AudioBackend seam** (never public), narrow so WASAPI
can implement it without re-opening the engine:

- `fill_output(float *dst, uint64_t requested_frames)` — the audio
  callback: reads the PCM ring into `dst`, appends a GAP span (underrun /
  preroll / EOS silence, §5) on underflow. GAP silence is PHYSICALLY
  zeroed in `dst` — a real backend consumes the buffer as returned, so
  stale PCM after the media frames would be audible garbage. Lock-free and
  MUTEX-FREE, zero heap allocation.
- `advance_render(int64_t frames, int64_t generation)` — the device clock
  evidence: proven-rendered output advanced through the timeline → media
  position. Same realtime properties.

`NullAudioBackend` implements the device side in-process (deterministic,
no wall clock); `submit()` / `backend_render()` are TEST-ONLY locked
wrappers over the seam (the C consumer drives them through
`player_test_driver.h`, standing in for WASAPI's callback). A control
commit calls `quiesce_backend()` before touching ring/timeline —
CLOSE-THEN-DRAIN admission: admission is closed FIRST (a callback that
attempts entry after the close returns idle and is never counted), then
counted in-flight ops are waited out — a consumer that entered just before
the close is deterministically drained, and no resurrected frames survive
the commit. Only after the reset completes does the commit re-open
admission. Timeline overflow is a deterministic ERROR on the PRODUCTION
seam: `fill_output` sets the atomic overflow flag AND the atomic Error
state; the snapshot translates it into the fixed `"timeline capacity
exhausted"` diagnostic. SRC stays frozen: BYPASS when source rate/layout ==
device requirement, else `aresample` / libswresample — owned by the device
side, never SongCore.

**Realtime-path proof**: a test binary with a global operator-new counter
proves `fill_output` / `advance_render` / timeline ops perform ZERO heap
allocations and take NO MUTEX, and a control thread provably holding the
state mutex does not block a concurrent fill. The bounded timeline is a
fixed-capacity span ring (256 spans) that coalesces contiguous same-kind
spans and lazily trims fully-rendered history into prefix anchors; a
pathological window that still exhausts the store FAILS CLOSED (sticky
overflow → deterministic Error) — never growth, never corruption. The
product snapshot is ONE coherent instant: `position_us` / `duration_us` /
`sample_rate` all derive from the source rate captured inside the
snapshot's lock hold — never a post-lock read — so polling concurrently
with open() can never mix one song's frame counts with another song's rate.

## 11. Validation

```text
player_gates (native suite, binary exit-code gates):
  ring unit/property/SPSC-thread; lifecycle T1–T14; state-illegal probes;
  clock model; submitted-vs-rendered + MEDIA/GAP mapping S1–S10; ESTIMATED
  offset invariance; clock-corrective T16–T20; capacity sweep;
  thread stress (destruction, seek/stop/open vs decode via before-publish
  barriers, render vs control, EOF vs control, play-to-end);
  realtime contract (zero-allocation + no-mutex seam, GAP zero-fill poison
  buffer, timeline no-alloc ops, 6-hour long-run bounded memory,
  pathological alternation bounded, production-seam overflow → ERROR,
  admission quiescence close-first + waits-for-inflight, snapshot rate
  coherence 44.1k/48k × polls, realtime-seam ENDED signal)
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

## 12. Frozen semantics vs implementation priors

Frozen PlayerEngine semantics:

```text
- decode progress is not playback position
- media position represents the audible media timeline
- device time and media time are distinct domains
- submitted output is not rendered output; submission never advances
  media position, only proven render progression does
- the hardware/render clock belongs to the output/device domain
- a correct device→media mapping must exist for native backends
- the mapping representation is not frozen
- underrun silence advances device time but not media time
- underruns never drop or skip media content; catch-up/drop would be an
  explicit separately-designed policy
- seek success opens a new media segment
- seek failure is a deterministic fail-closed ERROR, never silently
  resumed as old playback
- seek uses the CONFIRMED actual landing when SongCore reports one
- successful seek with unknown landing uses an ESTIMATED position,
  never labeled an actual landing; its error is an unknown constant
  segment offset that must not accumulate within the segment
- known-duration ENDED reports the known duration endpoint;
  unknown-duration ENDED reports the final rendered media endpoint
- stale decoder output cannot enter the current queue; stale
  backend/output accounting cannot advance the current media timeline
- pause freezes media progression and stops device consumption
- EOF becomes ENDED only after actual media is drained and rendered
- GAP silence is physically zero in the device buffer
- control reset and the backend callback quiesce via close-then-drain
- the realtime path performs no allocation and takes no mutex
```

Implementation priors, NOT frozen ABI semantics:

```text
- SPSC ring buffer
- generation/epoch counter representation (one token or two)
- retaining PCM while paused
- ring capacity
- chunk size
- worker scheduling
- atomics/mutex implementation
- backend period
- round-to-nearest conversion implementation
- timeline mapping representation (span ring, anchors, trimming)
```

The design rule:

> **Freeze what the user hears and what the timeline means; do not freeze
> how the implementation happens to achieve it.**
