# PlayerEngine v1 — Frozen Semantics

Semantic authority for the native PlayerEngine (Phase 1.5+). Frozen by the
Phase-1 Python reference model; the executable oracle is
`tools/player_model/scenarios.py` (36 gates, deterministic + seeded
randomized stress). This document states final semantics only.

Position in the stack:

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
- SongCore positions are microseconds (`song_seek` / `song_info`); the
  engine converts on the way in (clock rebase; round-to-nearest in the
  reference model — an implementation prior) and clamps against the known
  duration.
- SongCore seek returns the **actual landing position**; the engine bases
  the clock on the RETURNED landing, never on the request (§6). Position
  unknown (`-1` with `SONG_OK`) is handled per §6 landing quality.
- Duration may be **unknown** (`duration_us == -1`). Unknown duration is
  represented explicitly (`duration_known == false`); it is never
  overloaded as `duration == 0`, and it never produces a fake clamp.

## 2. Playback timeline principle

PlayerEngine strictly separates **device/render time**, **media time**,
and **decode position**. They describe different facts; no single frame
counter may implicitly serve as more than one of them.

> **A clock answers when; a timestamp answers what.**
>
> The device clock answers *when in the real world output was rendered*;
> media time answers *what part of the song that output represents*.
> Never infer one from the other without an explicit mapping. That the
> device consumed N frames never implies media time advanced by N frames.

The output pipeline is an explicit chain of distinct domains:

```text
SongCore
   ↓
Decoded Media PCM            (decode domain)
   ↓
PCM Queue                    (queued media)
   ↓
AudioEngine                  (BYPASS in v1)
   ↓
Submitted Output             (copied into the OS/device buffer)
   ↓
AudioBackend/device buffer
   ↓
Rendered Output              (proven rendered by the device clock)
   ↓
device→media mapping
   ↓
Media Position               (player_get_position())
```

The design must explicitly distinguish at least:

```text
decoded_source_frames
queued_media_frames
submitted_output_frames
rendered_output_frames
```

Do not collapse them. In v1 BYPASS the values are numerically equal along
the chain — that equality is never semantic identity, and SRC
(44.1 kHz source → 48 kHz output) must not break any accounting assumption.

### 2.1 Decode position

Decode position is where the decoder has reached on the media timeline.
It may legitimately run far ahead of playback:

```text
decode position = 15.5 s
media position  = 13.2 s
```

That is normal pre-decode behavior. Decode position:

- drives internal scheduling, buffering, and diagnostics only;
- is never the authority behind `player_get_position()`;
- must not advance user-visible progress merely because the decoder
  produced PCM.

### 2.2 Device / render time

Device time is how much physical playback time the output device has
actually advanced. The future Windows authority is:

```text
WASAPI IAudioClock
```

or the equivalent backend hardware/render clock. Device time advances for
every output frame actually rendered by the backend, including:

```text
media PCM
underrun-generated silence
backend-generated padding
```

So during an underrun the real world keeps moving:

```text
device clock += silence duration
```

`IAudioClock` positions belong to the **rendered output/device domain**.
They are NOT directly `player_get_position()`; the conceptual future path
is:

```text
IAudioClock device position
        ↓ output timeline mapping (§2.5)
media timeline position
        ↓
player_get_position()
```

### 2.3 Media position

Media position is where the listener currently is on the media timeline.
This is the semantics `player_get_position()` must express. It is not:

```text
decoded_frames / sample_rate
```

and it is not:

```text
device_frames / device_rate
```

It is:

> **Media Position is PlayerEngine's best available estimate of the
> currently audible position on the media timeline.**
>
> When the current segment has `CONFIRMED` landing quality, the segment
> base is anchored by SongCore's reported landing. When landing quality
> is `ESTIMATED`, the media clock remains continuous and deterministic
> from the requested/clamped seek target, but an unknown constant segment
> offset may exist between the reported position and the true media
> timestamp of the decoded PCM.

Only real PCM belonging to the media timeline advances media position.

### 2.4 Submitted != rendered

Frozen rule:

> Submitting PCM to AudioBackend does not make that PCM audible.
>
> Media Position must never advance merely because output frames were
> copied or submitted into an operating-system/device buffer.
>
> Media Position may advance only when backend clock evidence establishes
> that the corresponding output span has actually been rendered.

Example:

```text
device buffer latency = 100 ms

PlayerEngine submitted media through 10.100 s
hardware currently audible at 10.000 s

required:  player_get_position() ≈ 10.000 s
forbidden: player_get_position() = 10.100 s
```

Consequently `rendered_media_frames` in the clock formula (§8) means
media frames whose corresponding output has been **proven rendered by
backend/render progression** — never "copied into the backend",
"submitted to the backend", or "removed from the PCM queue".

### 2.5 Timeline mapping (OutputSpan reference model)

The output domain and the media domain are two coordinate systems:

```text
Device output timeline
────────────────────────────────────────►

[ media 10.0 → 10.5 ]
[ underrun silence   ]
[ media 10.5 → 11.0 ]
```

Frozen requirement:

> Once a real backend has pending output and a hardware/render clock,
> PlayerEngine MUST maintain enough information to map a rendered
> device/output position back to the media timeline.

The **representation is not frozen**. The Phase-1 reference model uses
one `OutputSpan` deque because the oracle must prove the mapping
executable (gate `s3_media_gap_output_mapping`) and must split submitted
from rendered output; an equivalent structure is equally valid:

```text
OutputSpan {
    output_begin
    output_end
    kind = MEDIA | GAP
    media_begin        (MEDIA: real content; GAP: none)
    media_end
}
```

- **MEDIA**: output frames correspond to actual media content.
- **GAP**: output frames correspond to synthetic silence/padding and
  carry **zero media duration** — they occupy device time but no media
  interval.

```text
device output domain:  0────100────110────210
                       [ MEDIA ] [ GAP ] [ MEDIA ]
media domain:          0────100        100────200
```

Mapping examples (device position → media position): 5 → 50 ms-range
content, 12 (inside the GAP) → still 100 ms, 20 → 150 ms. A GAP never
advances the mapped media position.

### 2.6 Frame-domain naming

Every frame counter declares its domain in its name. Ambiguous names are
forbidden:

```text
frames
position_frames
current_frames
```

Preferred:

```text
decoded_source_frames
queued_media_frames
submitted_output_frames
rendered_output_frames
rendered_media_frames
underrun_silence_output_frames
discarded_stale_media_frames
```

Once SRC exists (§10), source frames ≠ output frames. Frame counters from
different domains are never compared directly.

## 3. States

Public states only (no SEEKING/STOPPING leakage across the future ABI):

```text
EMPTY · READY · PLAYING · PAUSED · ENDED · ERROR
```

Transitions (everything unspecified is an illegal call → typed error on
the control ABI; `pause` and `stop` on EMPTY are documented no-ops):

| Call (state → effect) | EMPTY | READY | PLAYING | PAUSED | ENDED | ERROR |
|---|---|---|---|---|---|---|
| `open(song)` | → READY | → READY | → READY | → READY | → READY | → READY |
| `play()` | error | → PLAYING | idempotent | → PLAYING | seek 0, → PLAYING | error |
| `pause()` | no-op | no-op | → PAUSED | idempotent | no-op | error |
| `stop()` | no-op | → READY @0 | → READY @0 | → READY @0 | → READY @0 | → READY @0 * |
| `seek(T)` | error | @landing | @landing | @landing | → READY @landing | error |

\* `stop` from ERROR reaches READY @0 only when recovery actually
succeeds (see below); if it fails, ERROR persists.

Frozen policies:

- `open` never autoplays; it stops everything, drops the previous SongCore
  handle, clears the queue AND all pending output (§6), resets the clock,
  opens/probes the new song, → READY @0. A fresh handle starts at a
  CONFIRMED position 0.
- `stop` is not `pause`: it rewinds the source to 0, flushes the queue and
  pending output, resets position to 0, → READY.
- `play` from ENDED replays from the beginning (seek 0 → PLAYING); if that
  restart seek fails, the engine lands in ERROR (fail-closed, like any
  seek failure).
- `seek` from ENDED lands and → READY (play is then required).
- `pause` freezes media progression: audible progression stops, media
  position freezes, resume continues from the same logical media position,
  and the device stops consuming (render advancement halts; pending
  output stays pending). How the backend stops/restarts and whether
  buffered PCM is retained across the pause is implementation policy,
  not public semantic contract.
- A fatal decode error (typed SongCore status, incl. the partial-success
  framing: frames before the fault are delivered, the fault surfaces on
  the next call) → ERROR. Already-submitted output may drain audibly; no
  new PCM is submitted.

### 3.1 Seek failure (fail-closed)

SongCore may fail a seek (`SONG_ERR_SEEK_UNSUPPORTED`,
`SONG_ERR_SEEK_ERROR`, `SONG_ERR_IO`, ...). One deterministic v1 policy:

```text
seek requested
    ↓
current generation invalidated, queue flushed, pending output discarded
    ↓
SongCore seek fails
    ↓
PlayerEngine → ERROR
```

From every legal seek state:

```text
READY + failed seek   → ERROR
PLAYING + failed seek → ERROR
PAUSED + failed seek  → ERROR
ENDED + failed seek   → ERROR
```

After a failed low-level seek the engine must not assume decoder/demux
state, buffer state, or clock state can safely resume the old timeline.
It never silently continues PLAYING at the old position, never reports a
false seek-success position (the snapshot keeps the last audible media
position as a frozen diagnostic), and a failed seek is never relabeled an
ESTIMATED landing (§6). Transactional-seek semantics would require a
future SongCore capability and proof.

### 3.2 ERROR recovery

`stop()` is a **deterministic rebuild**, implemented as: rewind in place
via seek(0) when the handle is trusted (stop from a healthy state); drop
and reopen the source when it is not (stop from ERROR, or when the rewind
seek fails) — a fresh handle at position 0. If even the reopen fails
(file gone, I/O dead), ERROR persists with a deterministic diagnostic,
and `open(song)` is the guaranteed recovery. The engine never fakes READY
over a source it could not actually reposition.

## 4. PCM queue contract

- Bounded, preallocated, frame-based, **SPSC**: producer = decode worker,
  consumer = render callback. Capacity is fixed at open; no growth, ever.
  (SPSC itself is the reference implementation's shape — an
  implementation prior, not a frozen semantic; see §12.)
- Ring-level conservation: `produced == consumed + buffered + flushed`.
- Engine-level decode conservation (lifetime, across song changes):

```text
decoded_source_frames == ring_produced + discarded_stale_media + in_flight
```

- Backpressure: the worker begins a chunk only when
  `writable >= min(chunk, remaining)`. It never overwrites unread frames,
  never drops decoded PCM arbitrarily, never grows memory; when full it
  waits (native: producer-side wait primitive). The render side NEVER
  blocks on the producer.
- Flush (seek / stop / open) empties the queue; `buffered == 0` after
  every flush, and no pre-flush PCM may be consumed afterwards.

## 5. Underrun / preroll / EOS silence

Render callback asks for N frames while PLAYING; the queue holds M:

- `M == N` real frames → deliver as a MEDIA span.
- `M < N`, source NOT at EOF — pad with GAP silence so the realtime thread
  never blocks:
  - no real frame submitted yet in this segment → **preroll**: N−M
    silence, `preroll_events/frames++`; media position does not advance
    (playback has not started);
  - otherwise → **underrun**: deliver M real frames + N−M silence,
    `underrun_count/frames++`. Device/render time advances by the full
    period; **media position advances only by the M real media frames**.
- Source EOF + queue drained + nothing in flight → **EOS silence** (not
  an underrun): the final partial period is completed with GAP padding,
    `eos_silence_frames++`, → ENDED once the media part has rendered (§7).

The canonical example (100 Hz media/output rate):

```text
queue has 4 frames, device requests 10 frames
output:      4 media + 6 silence
device time: +100 ms
media time:  +40 ms

next decoded media begins exactly after those 4 media frames
```

Underrun silence has **device duration and zero media duration**. Playing
silence must never implicitly skip an equal amount of song content, and
media position lagging device elapsed time after an underrun is CORRECT —
the engine must not drop decoder frames, seek forward, or advance the
media base to make the two clocks numerically equal. Any future
catch-up/drop policy must be an explicit discontinuity/drop policy,
designed separately; for local-file playback underruns are abnormal
diagnostics, not an excuse to mutate the song timeline. Recovery from an
underrun is simply the producer refilling the queue: the next real media
resumes from the exact next source frame — no jump, no duplication, no
loss.

After a 10-second song with 500 ms cumulative underrun silence, the media
timeline is still 10 s (device session elapsed 10.5 s). Injected silence
is never interpreted as remaining media and never prolongs media duration.

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

- No PCM decoded before the commit may be rendered after it. This is
  enforced by the epoch: an in-flight decode result carries the epoch it
  was decoded under; the worker cannot be reached into synchronously, so
  a stale result **publishes into a dead epoch and is discarded at
  publish time** (`discarded_stale_media_frames`). The invariant holds
  for seek, stop, and open alike.
- **Stale output generations**: already-submitted output is discarded at
  the commit, and render progression is generation-guarded — a late
  device/render event from a dead generation is dropped and can never
  advance the new segment's media timeline (`stale_render_events`
  diagnostic). The frozen semantic is "stale decoder output cannot enter
  the current queue; stale backend/output accounting cannot advance the
  current media timeline"; whether one generation token or separate
  decode/output generations implement it is not frozen. For a real
  backend, already physically submitted old audio may require a backend
  stop/reset/flush before the seek commit — a future AudioBackend
  requirement.
- Every commit (open / seek / stop / restart-from-ENDED) opens a new
  **segment**. Two deliberately different invariants hold within a
  segment:
  - **Clock continuity** (production-realizable): the public media
    position equals segment base + rendered media duration and is
    monotonic. This is what a real PlayerEngine implements — it sees
    durations, never frame identities.
  - **Content continuity** (test-only oracle): using the fake decoder's
    hidden frame identities, rendered frames are exactly contiguous from
    the segment's true first decoded frame — no duplication, loss,
    reordering, or stale frames. For CONFIRMED segments the true first
    frame must equal the reported landing (absolute timeline
    continuity); for ESTIMATED segments it may differ by the unknown
    segment offset — the oracle must not require it to match, and the
    engine must never rebase onto it.
- SongCore handle calls on the seek path must be serialized with the
  decode worker (the handle is not thread-safe; see `songcore.h`).

### 6.1 Seek landing quality

SongCore seek returns a status plus an actual landing; `-1` with `SONG_OK`
means the landing is genuinely unknown (never manufactured). Two landing
qualities:

```text
CONFIRMED  actual_position_us >= 0 → media base = actual landing
ESTIMATED  SONG_OK, actual == -1   → media base = requested/clamped target
```

CONFIRMED carries both clock continuity and absolute timeline continuity
(the content really starts at the reported landing). ESTIMATED is frozen
as:

```text
1. Seek itself succeeded.
2. Exact first decoded media timestamp is unavailable.
3. PlayerEngine uses the requested/clamped target as the estimated
   media base.
4. Public media position remains deterministic and monotonic from that
   estimated base: estimated_base + rendered_media_duration.
5. The actual decoded content may differ by an unknown constant
   segment offset.
6. The engine must never fabricate CONFIRMED precision.
7. Hidden oracle metadata may verify content continuity, but must never
   alter production clock semantics — the engine never reads, simulates,
   or expects "true tags".
8. Within one ESTIMATED segment, the hidden true-vs-reported offset
   must remain constant unless an explicit discontinuity occurs.
```

The offset must not accumulate: with correct rate, no dropped and no
duplicated media, the error stays a segment-level constant (reported
5.000 → 5.100 → 5.200 vs true 5.037 → 5.137 → 5.237 keeps Δ = 37 ms).
Drift within a segment indicates a real clock/rate/accounting bug; the
oracle enforces this as the named invariant
`estimated-segment-offset-invariance` (constant across playback,
underruns, and pause/resume).

`player_get_position()` returns the best available estimate either way,
and playback proceeds normally from it; the quality is carried in the
snapshot/diagnostics (whether the future public ABI exposes it is a
later ABI review decision). A **failed** seek is neither CONFIRMED nor
ESTIMATED — it is an ERROR (§3.1); `SONG_OK` with an unknown landing is
never escalated to ERROR. Eliminating ESTIMATED would require a deeper
timestamp authority (e.g. a future SongCore exposing first-frame media
timestamps); that is explicitly NOT an ABI v1 requirement — tens of
milliseconds of seek landing uncertainty is normal for music playback.

Seek must reset the output mapping as well as the media clock: after a
successful commit, media base = the actual landing (never a value derived
from device elapsed time — see gate `t19`), and no pre-seek output span
may later mutate the new segment's media position.

## 7. EOF lifecycle

SongCore EOF does NOT end playback:

```text
SongCore EOF → worker marks source_eof → queue keeps draining
→ queue empty + pipeline quiesced + submitted media output rendered
→ AudioEngine.drain() → AudioBackend pending MEDIA drained → ENDED
```

ENDED means exactly: all audible PCM belonging to the current media
timeline has completed processing, queue drain, and backend playout. The
presence of submitted-but-unrendered media blocks ENDED; trailing
backend-required non-media padding (EOS GAP) does NOT postpone it once
all actual media output has completed playout. ENDED persists while
paused (drain completes only through render progression); play after
ENDED restarts from 0; seek after ENDED → READY.

ENDED position:

```text
duration known:   ENDED position = known media duration
duration unknown: ENDED position = final rendered media endpoint
```

where the final rendered media endpoint is the media timeline point
reached after every real media frame of the final segment has actually
rendered. Synthetic underrun/padding silence must not extend it, and an
unknown duration never clamps the position to −1 or 0.

## 8. Clock model

- Two domains, two counters: device/render time and media position are
  distinct and never share one counter (§2).
- Device/render time advances with every output frame the backend proves
  rendered — media PCM, underrun/preroll/EOS silence, backend padding.
  Output-domain accounting:
  `rendered_output == rendered_media_output + rendered_silence_output`.
- Media position:

```text
position = base(landing) + rendered_media_frames / media_rate
```

  clamped to duration when known. `rendered_media_frames` counts only
  real media-timeline PCM **proven rendered** by backend progression
  (§2.4) — never decode progress, never submitted/copied frames, never
  device frame counts, never wall time. Underrun silence advances device
  time only (§5). Preroll silence and buffering ahead do not move it.
  Pause freezes it; resume continues; seek rebases it to the actual
  SongCore landing.
- `player_get_position()` reports the media position. Decode position and
  device time are scheduling/mapping inputs, never the reported position.

## 9. Thread ownership

```text
control thread   open / play / pause / stop / seek (serialized)
decode worker    SongCore read → PCM ring → publish (epoch-checked)
audio callback   fill_output: ring read → device buffer, GAP on underrun
                 (§5 rules) — lock-free, no control mutex, no allocation
device clock     advance_render: proven render progression → mapping →
                 media position
```

Diagnostics for UI are a polled snapshot (state, media position, decode
position, duration(+known flag), buffered, underrun/preroll/stale
counters, submitted/rendered/pending output counters, position quality,
last error) at UI cadence (5–10 Hz); no native→Kotlin callbacks, none
from the realtime path. The product snapshot reports the media timeline
in MICROSECONDS (`position_us` / `duration_us`) — the UI never needs a
sample rate.

## 10. Production boundary: two layers (corrective §62)

The engine ships two distinct surfaces; only one is public. The realtime
path never touches the control mutex and never allocates; the backend
seam is internalized so a managed caller (KMP) knows nothing about how
audio is driven (corrective P0-A/P0-B).

**Layer 1 — product C ABI** (`include/player_engine.h`, the ONLY public
surface). Control + polled observation:
`pe_create / pe_destroy / pe_open / pe_play / pe_pause / pe_stop /
pe_seek / pe_get_snapshot` plus `player_engine_abi_version`. No audio
backend, no manual render ticks, no queue internals, no worker mode, no
SongCore surface beyond the `song_io` open() travels through. Every call
returns a typed `pe_status`; invalid caller input never aborts and no C++
exception crosses the boundary (P0-D). Control calls are caller-
serialized (the engine also serializes internally); `pe_get_snapshot` may
be polled concurrently.

**Layer 2 — internal AudioBackend seam** (never public). The boundary
between the engine and whatever drives the device, deliberately narrow so
WASAPI can implement it without re-opening the engine:

- `fill_output(float *dst, uint64_t requested_frames)` — the audio
  callback: reads the PCM ring into `dst`, appends a GAP span (underrun /
  preroll / EOS silence, §5) when the ring underflows. Lock-free
  (`active_fill_` count + atomics), no control mutex, no allocation.
- `advance_render(int64_t frames, int64_t generation)` — the device
  clock evidence: proven-rendered output advanced through the timeline →
  media position. Same realtime properties.

`NullAudioBackend` implements the device side in-process (deterministic,
no wall clock); `submit()` / `backend_render()` are TEST-ONLY locked
wrappers over the seam (the C consumer drives them through
`player_test_driver.h`, standing in for WASAPI's callback). A control
commit (seek/stop/open) calls `quiesce_backend()` before touching
ring/timeline — a consumer mid-fill is deterministically waited out, and
no resurrected frames survive the commit (corrective §11/§50). SRC stays
frozen: BYPASS when source rate/layout == device requirement, else
`aresample` / libswresample — owned by the device side, never SongCore.

**Realtime-path proof**: a test binary with a global operator-new counter
proves `fill_output` / `advance_render` / timeline ops perform ZERO
heap allocations, and a control thread provably holding the state mutex
does not block a concurrent fill. The bounded timeline (corrective
§12–§18) is a fixed-capacity span ring (256 spans) that coalesces
contiguous same-kind spans and lazily trims fully-rendered history into
prefix anchors; a pathological window that still exhausts the store
FAILS CLOSED (sticky overflow → deterministic diagnostic Error) — never
growth, never corruption.

## 11. Native mapping notes (Phase 1.5)

- C or C++ under `src/player/`, ABI header `include/player_engine.h`
  (frozen after native gates); narrow control ABI, snapshot struct for
  polling.
- SPSC queue with atomics only where required; producer wait on a futex /
  condition primitive; render path lock-free w.r.t. the producer where
  possible, never blocking.
- `NullAudioBackend` (configurable rate / channels / period / manual
  submit/render ticks) precedes WASAPI shared-mode event-driven playback
  so lifecycle tests are deterministic without wall clock. The submit
  callback and the render-progression reading stay separate entry points
  (§2.4).
- The device→media mapping structure (span deque, timestamp anchors,
  piecewise mapping, or backend-specific accounting) is an implementation
  choice; a bounded/trimmed representation replaces the oracle's growable
  span list. Only the mapping semantics are frozen.
- Commit (seek/stop/open/restart) must stop/reset/flush the backend's
  pending output before rebasing — the stale-output-generation guard (§6).
- Preroll rule for WASAPI: start the device after the first decoded chunk
  is queued (or gate device frames out of the clock) so the clock
  semantics above hold on real hardware.

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
- timeline mapping representation (OutputSpan deque, anchors, trimming)
```

The design rule:

> **Freeze what the user hears and what the timeline means; do not freeze
> how the implementation happens to achieve it.**

## Oracle

```bash
python3 tools/player_model/scenarios.py                 # full suite (36 gates)
python3 tools/player_model/scenarios.py --quick         # reduced stress
python3 tools/player_model/scenarios.py --stress-seeds 1000 --stress-ops 500
```

Gate inventory: `ring_unit` / `ring_property`; T1–T14 lifecycle, seek,
EOF, error, clock gates; `state_illegal_probes`; `clock_model`;
S1–S10 pre-native closure gates (submitted-vs-rendered, MEDIA/GAP
mapping, seek failure, unknown duration, landing quality, stale output
generations, EOF playout); `estimated_segment_offset_invariance` (the
named ESTIMATED invariant: true-vs-reported offset constant across
playback, underrun, and pause/resume — test-only content oracle);
T16–T20 clock-corrective gates (decode-ahead separation, underrun
device-vs-media clocks, repeated-underrun continuity, seek-after-
underrun rebase, EOF-after-underrun duration); `t15_randomized_stress`
(async submit/render + seek failures + stale render events, every
invariant checked after EVERY operation); `capacity_sweep`.

Invariants are checked after EVERY operation — ring/decode conservation,
output/media pipeline conservation, the rendered-output split, per-tick
backend accounting, clock continuity (`position == base +
rendered_media`, engine-observable counters only), content continuity
(hidden frame identities, never a production input), and the ESTIMATED
offset invariance. Failures print seed, operation trace, and a full
domain-labeled snapshot (generation, ring state, decode/media/device
positions, underrun accounting). The native implementation replays these
traces as its model-equivalence gate.

## Implementation status (Phase 1.5 corrective closeout)

```text
Python oracle (tools/player_model)      PROVEN — 36 gates, deterministic
Native PlayerEngine + NullAudioBackend  PROVEN — 53 native gates, model
                                        equivalence vs the oracle
C product ABI (include/player_engine.h) PROVEN — pure-C external consumer,
                                        no backend/test symbols
Real SongCore integration               PROVEN — player_real_songcore_smoke:
                                        REAL SongCore + REAL corpus
                                        fixtures (FLAC/MP3) + host FILE I/O
WASAPI / real backend                   NOT STARTED (explicitly out of
                                        scope; the internal seam is ready —
                                        next phase)
```

Native layout: `src/player/` (PcmRing, PlaybackTimeline, NullAudioBackend,
PlayerEngine, C shim), `tests/player/` (fake SongCore link-time
substitution, gate suite, trace runner, C consumer, real-SongCore smoke),
targets `player_core`, `player_gates`, `player_trace_runner`,
`player_consumer_c`, `player_real_songcore_smoke`. The engine carries no
test abstraction: test binaries link `tests/player/fake_songcore.cpp`
INSTEAD of real SongCore at the frozen C ABI — except
`player_real_songcore_smoke`, the one gate that links the REAL SongCore
archive and drives real fixtures through host I/O (P1-B). Observability
hooks are mutex-guarded debug setters, never production inputs.

Corrective production boundary (P0-A/P0-B/P0-C/P0-D): the public C ABI is
control + observation only — `pe_submit/pe_render/PE_CURRENT_GENERATION/
worker_thread/max_submit_frames` were removed, and the AudioBackend seam
was internalized. The product snapshot carries `position_us` /
`duration_us` directly (time authority, §58). The bounded timeline
(corrective §12–§18) uses a fixed-capacity span ring with coalescing,
lazy trimming into prefix anchors, and fail-closed overflow; a
coalesce-vs-render-cursor race found during the corrective closeout
(extending the very span the render cursor is inside) was fixed by never
extending the cursor's own span, and the trim soft limit was tightened to
8 so the store stays ≤ 8 live spans under pathological MEDIA/GAP
alternation-with-render. The long-run gate paces underruns the way the
real device sees them (an underrun REPLACES the period's output, docs §5)
so submission == rendering and the store provably stays bounded for
arbitrarily long playback.

Gate inventory (native): ring unit/property/SPSC-thread gates; engine
gates T1–T20, S1–S10, state-illegal probes, clock model,
estimated-segment offset invariance, capacity sweep, mutation gates
(§74: skip-epoch-guard, advance-on-submit, gap-counts-as-media,
end-without-pending — each provably caught by the suite); thread stress
(destruction, seek/stop/open vs decode via before-publish barriers,
render vs control, EOF vs control, play-to-end); C consumer lifecycle
gate; realtime-bound gates (corrective §47–§50): zero-allocation +
no-control-lock realtime path, timeline no-alloc ops, 6-hour long-run
bounded memory, pathological alternation bounded / fail-closed overflow,
reset-race quiesce, realtime-seam ENDED signal. `xmake test` runs
`player_gates`, `player_trace_runner` (200 seeds × 300 ops equivalence),
`player_consumer_c`, `player_real_songcore_smoke` (real fixtures), and
the SongCore regression; sanitizer variants build with
`xmake f --player_san=asan|ubsan|tsan`.

Evidence (2026-09-02, Linux x86_64, gcc 15):

```text
native gates (plain / ASan / UBSan)   53/53 PASS each
native gates (TSan)                   3 runs, 53/53 PASS, 0 warnings
model equivalence                     200×300 PASS (xmake test) and
                                      1000 seeds × 500 ops PASS
C consumer (product ABI)              PASS under plain/ASan/UBSan/TSan
real-SongCore smoke (FLAC + MP3)      PASS under plain/ASan/UBSan/TSan:
                                      ENDED @ true duration, seek landing
                                      in SongCore tolerance, READY @0 after
                                      stop, missing-file error path
SongCore sources/ABI                  untouched (engine consumes the
                                      frozen ABI only)
audio PCM                             unchanged (engine is above SongCore;
                                      tests consume fake PCM; real smoke
                                      decodes the real fixtures)
```

Two harness-level normalizations keep op-boundary equivalence exact
(docs §45): the trace runner injects the fake's remaining-frames truth
via `debug_set_source_hint` (the frozen SongCore ABI cannot report
remaining; production derives the duration estimate), and folds the
frozen ABI's separate 0-frame SONG_EOF read into the oracle's
eof-flag-on-last-chunk. The real-SongCore smoke paces its test driver
(~1 ms per period) because the test-only locked driver stand-in hammers
the control mutex if driven in a zero-pause tight loop; production is
immune — WASAPI's callback uses the lock-free seam, never the control
mutex (see §10).

Known oracle edge (found during Phase 1.5, oracle left frozen): a
randomized trace that seeks to exactly the end boundary can make the
ORACLE's producer issue `read_pcm(0)` (ValueError) — the native engine
handles the same state via its EOF poll. The equivalence generator
rejects such landings; the engine's native coverage of this state is the
EOF-poll gates.
