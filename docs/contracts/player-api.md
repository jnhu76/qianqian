# PlayerEngine API

> Authority: Normative
> Scope: PlayerEngine 公开 C ABI（`native/include/player_engine.h`，v1）对外承诺的
> 行为，以及内部 AudioBackend seam 的实时性规则。运行时如何组成见
> [architecture/player-runtime.md](../architecture/player-runtime.md)。

## Purpose

Freeze what the user hears and what the timeline means; do not freeze how
the implementation happens to achieve it. This contract is the authority
for playback state, position meaning, seek/EOF lifecycle, silence, and
threading rules. The permanent native regression suite in `native/tests/player/`
owns the executable verification of these rules.

## Public surface

**Layer 1 — product C ABI** (`native/include/player_engine.h`, the ONLY public
surface). Control + polled observation: `pe_create / pe_destroy /
pe_open / pe_play / pe_pause / pe_stop / pe_seek / pe_get_snapshot` plus
`player_engine_abi_version`. No audio backend, no manual render ticks, no
queue internals, no worker mode, no SongCore surface beyond the `song_io`
open() travels through. Every call returns a typed `pe_status`; invalid
caller input never aborts and no C++ exception crosses the boundary.
Control calls are caller-serialized; `pe_get_snapshot` may be polled
concurrently.

**Layer 2 — internal AudioBackend seam** (never public), narrow so a real
backend can implement it without re-opening the engine:

- `fill_output(float *dst, uint64_t requested_frames)` — the audio
  callback: reads the PCM ring into `dst`, appends a GAP span (underrun /
  preroll / EOS silence) on underflow. GAP silence MUST be PHYSICALLY
  zeroed in `dst` — a real backend consumes the buffer as returned, so
  stale PCM after the media frames would be audible garbage. Lock-free
  and MUTEX-FREE, zero heap allocation.
- `advance_render(int64_t frames, int64_t generation)` — the device clock
  evidence: proven-rendered output advanced through the timeline → media
  position. Same realtime properties.

`NullAudioBackend` implements the device side in-process for tests. A
control commit MUST call `quiesce_backend()` before touching
ring/timeline — CLOSE-THEN-DRAIN admission: admission is closed FIRST (a
callback that attempts entry after the close returns idle and is never
counted), then counted in-flight ops are waited out — a consumer that
entered just before the close is deterministically drained, and no
resurrected frames survive the commit. Only after the reset completes
does the commit re-open admission. Timeline overflow is a deterministic
ERROR on the PRODUCTION seam: `fill_output` sets the atomic overflow flag
AND the atomic Error state; the snapshot translates it into the fixed
`"timeline capacity exhausted"` diagnostic. SRC stays frozen: BYPASS when
source rate/layout == device requirement, else `aresample` /
libswresample — owned by the device side, never SongCore
([ADR-0002](../adr/0002-songcore-src-boundary.md)).

## Units and timeline domains

- The accounting unit is the **frame**: one frame = one sample for every
  channel. Frames, never bytes or samples, cross every internal boundary.
- SongCore positions are microseconds; the engine converts on the way in
  (round-to-nearest clock rebase) and clamps against the known duration.
- SongCore seek returns the **actual landing position**; the engine bases
  the clock on the RETURNED landing, never on the request.
  `-1` with `SONG_OK` means the landing is genuinely unknown.
- Duration may be **unknown** (`duration_us == -1`), represented
  explicitly (`duration_known == false`) — never overloaded as
  `duration == 0`, never a fake clamp.
- PlayerEngine strictly separates **device/render time**, **media time**,
  and **decode position**. They describe different facts; no single frame
  counter may implicitly serve as more than one of them.

> The device clock answers *when in the real world output was rendered*;
> media time answers *what part of the song that output represents*.
> Never infer one from the other without an explicit mapping.

Pipeline chain (distinct domains): `SongCore → Decoded Media PCM → PCM
Queue → Submitted Output → Rendered Output → device→media mapping → Media
Position`. The design must explicitly distinguish
`decoded_source_frames`, `queued_media_frames`, `submitted_output_frames`,
`rendered_output_frames`; do not collapse them. In v1 BYPASS the values
are numerically equal along the chain — that equality is never semantic
identity, and SRC (44.1 kHz source → 48 kHz output) must not break any
accounting assumption.

- **Decode position** may legitimately run far ahead of playback
  (pre-decode is normal); it drives scheduling/buffering/diagnostics only,
  never user-visible progress.
- **Device/render time** advances for every output frame the backend
  proves rendered — media PCM, underrun silence, backend padding
  (WASAPI authority: `IAudioClock`).
- **Media position** is the best estimate of the currently audible
  position: segment base (landing) + proven-rendered media. Only real
  media-timeline PCM advances it.

### Submitted != rendered

> Submitting PCM to the AudioBackend does not make that PCM audible.
> Media Position may advance only when backend clock evidence establishes
> that the corresponding output span has actually been rendered.

`rendered_media_frames` in the clock formula means media frames whose
output has been **proven rendered by backend/render progression** — never
"copied into the backend" or "removed from the PCM queue".

## State semantics

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

\* `stop` from ERROR reaches READY @0 only when recovery succeeds.

Frozen policies:

- `open` never autoplays: stops everything, drops the previous SongCore
  handle, clears queue AND all pending output, resets the clock,
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

## Seek semantics

Control thread commits a seek in this exact order:

```text
close backend admission, drain the realtime seam
commit-flush hook: device buffer PROVEN empty   ← cancelled (before the
                                                  renderer claimed): commit
                                                  abandoned, INTERNAL error,
                                                  generation intact
epoch += 1                      ← invalidates producer output
flush PCM queue
discard pending output          ← submitted-but-unrendered spans die here
SongCore seek(T) → status + actual landing
AudioEngine.reset()
[on SongCore failure → ERROR; on commit-flush cancel → internal error with
 the generation intact; on commit-flush failure after the claim → INTERNAL
 error AND the generation invalidated (epoch bump, pending timeline/ring
 discard) with state ERROR — the physical session is gone, so playback must
 not resume; on success:]
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
- Seek resets the output mapping as well as the media clock: after a
  successful commit, media base = the actual landing (never a value derived
  from device elapsed time), and no pre-seek output span may later mutate
  the new segment's media position.

### Seek landing quality

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
discontinuity occurs (suite invariant: `estimated_segment_offset_invariance`).
A **failed** seek is neither CONFIRMED nor ESTIMATED — it is an ERROR.
Eliminating ESTIMATED would require a deeper timestamp authority —
explicitly NOT an ABI v1 requirement.

## EOF and ENDED semantics

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

## Silence semantics

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
  `eos_silence_frames++`, → ENDED once the media part has rendered.

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

## Clock and snapshot semantics

- Two domains, two counters: device/render time and media position are
  distinct and never share one counter.
- Device/render time advances with every output frame the backend proves
  rendered. Output accounting:
  `rendered_output == rendered_media_output + rendered_silence_output`.
- Media position: `position = base(landing) + rendered_media_frames /
  media_rate`, clamped to duration when known. `rendered_media_frames`
  counts only real media-timeline PCM **proven rendered** by backend
  progression. Underrun silence advances device time only. Pause freezes
  it; resume continues; seek rebases it to the actual SongCore landing.

Diagnostics for UI are a polled snapshot (state, media position, decode
position, duration(+known flag), buffered, underrun/preroll/stale counters,
submitted/rendered/pending output counters, position quality, last error)
at UI cadence (5–10 Hz); no native→Kotlin callbacks, none from the
realtime path. The product snapshot reports the media timeline in
MICROSECONDS (`position_us` / `duration_us`) — the UI never needs a sample
rate. The product snapshot is ONE coherent instant: `position_us` /
`duration_us` / `sample_rate` all derive from the source rate captured
inside the snapshot's lock hold — never a post-lock read — so polling
concurrently with open() can never mix one song's frame counts with
another song's rate.

## Error semantics

- Seek failure is fail-closed: from every legal seek state (READY /
  PLAYING / PAUSED / ENDED) a failed seek → ERROR. The engine never
  assumes decoder/demux/buffer/clock state can safely resume the old
  timeline, never silently continues PLAYING at the old position, never
  reports a false seek-success position (the snapshot keeps the last
  audible media position as a frozen diagnostic), and a failed seek is
  never relabeled an ESTIMATED landing.
- `stop()` from ERROR is a **deterministic rebuild**: rewind in place via
  seek(0) when the handle is trusted (healthy state); drop and reopen the
  source when it is not (ERROR, or failed rewind) — a fresh handle at
  position 0. If even the reopen fails (file gone, I/O dead), ERROR
  persists with a deterministic diagnostic, and `open(song)` is the
  guaranteed recovery. The engine never fakes READY over a source it could
  not actually reposition.
- Timeline overflow is a deterministic sticky ERROR (see Public surface),
  never growth, never corruption.

## Ownership and lifetime

- `pe_create` / `pe_destroy` bracket the engine; destruction joins the
  backend render thread before engine destruction in the runtime flavor
  ([architecture/platform-audio.md](../architecture/platform-audio.md)) —
  after `pe_destroy` returns, no thread can touch the engine.
- The SongCore handle is owned by the engine between open and
  stop/open/destroy transitions; `NULL` destroy is a documented no-op.
- `NULL` destroy documented no-op; illegal calls never abort the process.

## Concurrency

```text
control thread   open / play / pause / stop / seek (serialized)
decode worker    SongCore read → PCM ring → publish (epoch-checked)
audio callback   fill_output: ring read → device buffer, GAP on underrun
                 — lock-free, no mutex, no allocation
device clock     advance_render: proven render progression → mapping →
                 media position
```

Backend ownership rule: `fill_output()` and `advance_render()` are
serialized by the AudioBackend's SINGLE realtime/device thread — one
event-driven render thread per backend that calls `fill_output` then
queries/advances the device render clock. `PlaybackTimeline` is therefore
NOT a concurrent multi-writer structure and takes no locks: the device
thread is its one runtime owner. The control thread may run concurrently
ONLY through the admission/quiesce protocol: a commit closes backend
admission, drains counted in-flight ops, resets ring/timeline/backend, then
re-opens admission. No other concurrency pattern is supported.

Consequences of the ownership rule, all enforced by the seam:

- The timeline's scalar accounting counters are atomics so the control
  plane's POLLED SNAPSHOT may read them lock-free while the device thread
  mutates (individually coherent values; cross-field conservation holds at
  quiescence). The span store itself stays single-owner.
- ENDED is committed BY the device thread: when the ENDED condition holds
  (playing, source_eof, queue empty, nothing in flight, AND
  `pending_media() == 0` — the owner reads its own timeline), it commits
  ENDED through the atomic state surface, the realtime path's only legal
  transition (same as overflow → ERROR). The decode worker publishes ONLY
  `source_eof` / decode state and NEVER reads the timeline. Control
  transitions cannot silently clobber an async ENDED/ERROR: pause and play
  commit by CAS, and a successful seek re-affirms its entry play state
  after the commit (an in-flight op that drained during the quiesce may
  lawfully have committed ENDED for the dying generation).
- The decode worker never receives a wakeup from the lock-free seam (a
  mutex-free notify would race the predicate check), so its backpressure/
  idle sleep is bounded and re-checked on a short timeout; ring-ahead
  buffering makes the poll latency irrelevant.

## Compatibility

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

## Required verification

`native/tests/player/` owns the executable verification: `player_gates` (native
suite — lifecycle, clock model, submitted-vs-rendered mapping, ESTIMATED
offset invariance, thread stress, realtime contract), `player_consumer_c`
(pure-C TU over the product ABI), `player_real_songcore_smoke` (REAL
SongCore + REAL corpus fixtures + host FILE* I/O). Run via `xmake test` or
the binaries directly; sanitizer variants build with
`xmake f --player_san=asan|ubsan|tsan`. Inventory:
[architecture/player-runtime.md](../architecture/player-runtime.md).
