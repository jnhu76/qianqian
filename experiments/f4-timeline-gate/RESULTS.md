# F4-GATE mechanism evidence — Position / Duration (Issue #119)

Status: **GATE EVIDENCE** — proposition inventory + projection algebra +
physical measurements + duration provenance, and the authority proposal
for ADR-PBK-002 §20 D14.8 (carried by the ADR amendment in this same
branch). Not architecture authority by itself; production code is
untouched.

```text
BASE_SHA:        c1864cd568b702af983cee25505bf477cce93380 (main; PR #150 merge)
BRANCH:          research/f4-timeline-gate-1
SUITES:          cargo test (experiments/f4-timeline-gate) — 8/8 oracles × 3 runs
PHYSICAL:        f4probe.exe on the Windows host via WSL interop
                 (shared-mode WASAPI, default endpoint, 48 kHz float32
                 mix format), 3 runs, failures=0 each
DURATION:        f4duration (Linux, static libsongcore.a, ABI v1),
                 3 runs, failures=0 each
AUDIBILITY:      NOT CLAIMED — the frozen proposition is device-consumed
                 presentation position in source frames; acoustic truth
                 is never claimed from software counters
```

## 1. Production inventory (current reality at BASE)

No position/duration mechanism exists in production today. Verified by
code inspection, not by name inference:

```text
DecodedPcmStream (qianqian-audio-api ports.rs)
    format() + read_frames() only; no position, no duration surface.
PcmEdge (qianqian-playback edge.rs)
    ring read_pos/write_pos are occupancy cursors (mod capacity), NOT
    monotone totals; buffered_frames() is cfg(test)-only and forbidden
    as product read (D14.2/D14.3). No counters exist.
WASAPI render loop (qianqian-output-wasapi wasapi.rs)
    reads GetCurrentPadding every iteration (gate tail check +
    available-space calc); no submitted counter; no IAudioClock use;
    no device clock anywhere.
PlaybackSessionObservation (qianqian-playback handle.rs)
    no position/duration fields; the doc comment already names them as
    "not earned".
SongCore ABI v1 (songcore.h / songcore-sys)
    song_info.duration_us + song_stream_info.duration_us (FFmpeg
    fmt->duration / st->duration, AV_NOPTS_VALUE → -1); song_seek
    (F5); NO current-position query exists on the ABI.
Decode endpoint (qianqian-decode-songcore)
    probes song_info but discards duration_us today.
Headless status/TUI
    renders the observation only; no timeline lines; FORBIDDEN_STATUS
    vocabulary guards unearned claims.
```

Roadmap cross-check (Issue #119 F4 section): decoded position /
submitted position / device-consumed / estimated-audible / known
duration — all five rows land in the propositions below with the same
truth-class direction the roadmap already fixed ("raw counters =
mechanism evidence" 已定; Fact 升格问题由本门以证据回答).

## 2. Propositions and truth classes

| Candidate | Owner | Truth class | Accuracy | Pause | Seek (F5) | Keep? |
|---|---|---|---|---|---|---|
| decoded frames (worker cumulative) | Decode leg (session worker) | Mechanism Evidence | exact decoded count | runs ahead (edge backpressure only) | jumps | NO (not exposed; diagnostic only) |
| edge read total ("submitted") | Playback Session (session-owned edge) | Mechanism Evidence | exact handed-off count | frozen at engagement | rebase at cutover | YES — derivation base |
| device tail ("tail", GetCurrentPadding) | Output mechanism | Mechanism Evidence | engine-queued frames of this stream | drains to 0 while parked | rebase at cutover | YES — derivation subtrahend |
| submitted − tail, clamped | Session observation | **Projection** | device-consumed presentation location in source frames; ≤1 block tear bound | freezes exactly at tail quiescence (= Paused evidence) | rebase + new base at cutover | **YES — the product Position** |
| IAudioClock position | Output mechanism (device) | Mechanism Evidence | device timeline in stream-format BYTE units | kept counting through park only in byte-equivalent terms | device-stream-relative, needs origin mapping | NO (rejected, §4) |
| container/stream duration_us | Decode provider (probe) | Mechanism Evidence | reported metadata; measured overclaim up to ~29% on damaged input | N/A (source-scoped) | source-scoped | YES — as optional evidence |
| exact decoded total at EOF | Decode leg | Mechanism Evidence (exact, terminal-only) | exact | N/A | source-scoped | YES — terminal consumption truth; NOT exposed as Duration |

```text
SELECTED_POSITION_PROPOSITION
    device-consumed presentation location for the current episode,
    source-relative, in source PCM frames
SELECTED_POSITION_MECHANISM
    submitted cell (session-owned edge read path)
    − tail cell (output mechanism's GetCurrentPadding reading),
    clamped monotone at the observation boundary

SELECTED_DURATION_PROPOSITION
    the duration the decode mechanism reports for the source at
    open/probe time; unknown stays unknown
SELECTED_DURATION_MECHANISM
    SongCore probe/stream duration_us relayed once at activation as
    episode evidence (same shape as source_format)
```

## 3. Experiment A — submitted / padding / consumed across phases

Physical: `f4probe.exe`, Windows host, shared-mode event-driven
WASAPI, default endpoint, 48 kHz mix format, buffer_frames=1056,
block=1024, 8 s finite tone (384000 frames). Enforced invariants set
the exit code; 3/3 runs failures=0 (raw logs in `evidence/`).

The probe is a **mechanism twin**: it runs probe-local code in the
production render-loop order (gate → GetCurrentPadding → GetBuffer →
`read_frames` → submit → ReleaseBuffer) against a real endpoint. No
production crate is linked on the Windows path, so every number below
is a twin measurement of the mechanism shape, not a measurement of
production code.

| Run | pause-command advance (frames) | frozen window | max raw backward (bound 1024) | EOF final | monotone (submitted / clamped) |
|---|---|---|---|---|---|
| 1 | 480 | 451 samples, constant | 96 | 384000 == total | true / true |
| 2 | 480 | 444 samples, constant | 96 | 384000 == total | true / true |
| 3 | 480 | 444 samples, constant | 96 | 384000 == total | true / true |

Established physically:

```text
submission freezes at render-gate engagement:
    after the pause command, submitted advances by one in-flight
    block at most (measured 480 < 1024) — the F3 mechanism-A
    invariant (gate strictly before GetBuffer) is exactly what makes
    the submitted cell freeze.

position freezes exactly at tail quiescence:
    the clamped projection is bit-constant through the quiesced park
    (first == last, hundreds of samples). The freeze point IS the
    D14.7 output-tail-quiescence evidence — the same reading that
    establishes Paused. No separate freeze mechanism exists or is
    needed.

consumed ≤ submitted always; tail ≤ submitted always; raw may tear
    backward by at most one in-flight block (measured 96); the same
    one-block staleness can also lead the truth forward (measured at
    stream start, where the first tail observation precedes the first
    submission); clamped projection monotone for the whole run.

derivation error bound:
    ± one in-flight block (1024 source frames ≈ 21 ms at 48 kHz) in
    either direction; no accumulating error (both cells are exact).
    The display clamp removes the backward half of that bound only —
    the forward lead does not persist because raw catches up within
    one block of real consumption.

EOF:
    drained_to_total=true — consumed rises to the exact submitted
    total as the device drains, and D11 Completed observes the same
    tail==0 reading. On the Completed path the submitted total equals
    the exact decoded total (384000 == 384000) because every produced
    frame is handed out: the edge producer blocks rather than
    dropping, and buffered frames are abandoned only on a
    stopped/failed terminal (edge.rs) — verdicts that do not claim
    Drained.
```

Simulation cross-check (all-platform oracles, `src/timeline.rs`):
capping, unknown≠zero, torn bound, clamp monotonicity, per-park freeze,
EOF rise, terminal withdrawal, 5×400-step scripted interleavings —
8/8, deterministic.

## 4. Experiment B — IAudioClock considered and rejected

Measured on the same runs (clock sampled on the render leg only):

```text
48 kHz leg      GetFrequency = 384000  = 48000 × 8   (stream-format
                BYTE rate — block align 8, not frames)
44.1 kHz leg    stream opened with AUTOCONVERTPCM on the same 48 kHz-mix
                endpoint; GetFrequency = 352800 = 44100 × 8
                → the clock's unit is the initialized stream format's
                byte rate, never source frames; source_relative=false

position comparison (48 kHz leg, byte/8 → frames):
    clock-derived consumed vs submitted−tail at end of phase:
    run1 −386, run2 −411, run3 −398 frames (≈8 ms) — the clock tracks
    the same quantity the algebra already derives, within engine
    period rounding, after a unit conversion the algebra never needs.
```

Verdict: the device clock is the **larger mechanism delivering no
additional source-relative truth** — it needs a COM service
acquisition, byte-unit conversion, and device-origin mapping to say
what `submitted − tail` already says in native source frames. REJECTED
by the smallest-mechanism razor; re-earnable only by a new narrow
authority decision if a proposition appears that the algebra cannot
support (e.g. device-clock-exposed product features).

## 5. Experiment C — duration provenance

Physical: `f4duration`, static `libsongcore.a` (ABI v1, archive
verified fresh against source history), committed corpus +
adversarial 30%-byte-truncated copies. 3/3 runs failures=0
(`evidence/f4duration-run*.log`).

| Input | container_us | stream_us | exact decoded frames | exact_us | container delta |
|---|---|---|---|---|---|
| mp3-cbr-id3v23 | 4000000 | 4000000 | 176400 (= ref) | 4000000 | 0 |
| flac-16-44-stereo | 4000000 | 4000000 | 176400 (= ref) | 4000000 | 0 |
| alac-16-44-stereo | 4000000 | 4000000 | 176400 (= ref) | 4000000 | 0 |
| alac-long | 6000000 | 6000000 | 264600 (= ref) | 6000000 | 0 |
| mp3 truncated 70% | **4000000** | 4000000 | 124463 (< ref) | 2822290 | **+1177710 (~29%)** |
| flac truncated 70% | 4000000 | 4000000 | 119808, decode error (clean fail-closed) | 2716734 | +1283266 |

Verdict:

```text
intact curated corpus      metadata happens to equal the exact decoded
                           total — but that is a property of the
                           corpus, not a guarantee.
damaged input              container duration is a LIAR: the truncated
                           CBR MP3 still reports the full 4.0 s while
                           only 2.82 s is decodable. FFmpeg itself
                           warned "filesize and duration do not match
                           (growing file?)".
therefore                  reported duration is optional, source-
                           scoped Mechanism Evidence with no exactness
                           guarantee; unknown (AV_NOPTS_VALUE → -1)
                           must stay unknown. Only the actual decoded
                           total at decode EOF is exact, and it
                           exists only at the end.
```

## 6. Pause semantics (F4/F3 interaction, measured)

```text
request_pause          Command only. Position does NOT freeze here;
                       the device is still draining the submitted tail
                       and the projection truthfully keeps advancing.
render engagement      submitted cell freezes (gate before GetBuffer).
tail quiescence        consumed reaches the frozen submitted value;
                       the projection freezes EXACTLY here — the same
                       evidence that establishes D14.7 Paused.
paused park            tail stays 0 (park closure keeps publishing);
                       projection constant (measured bit-constant).
resume                 submissions continue; projection follows.
```

No "position froze when the command was issued" lie exists in this
shape; the freeze instant is mechanically the Paused-establishment
instant.

## 7. EOF / terminal semantics

```text
decode EOF             edge drains; NOT product completion (D11).
device drain           consumed rises to the exact submitted total.
D11 Completed          DrainVerdict::Drained == the same tail==0
                       reading: at settlement, consumed == the exact
                       submitted total, which equals the exact decoded
                       total on this path because every produced frame
                       is handed out (producer blocks; abandonment is
                       a stopped/failed-terminal event). Whether that
                       equals the reported Duration is NOT guaranteed
                       (§5) and is never asserted.
Stopped / Failed       terminal Fact commits → the observation
                       withdraws the position (None). No final-
                       position latch storage is earned; the cells
                       simply stop being derived from (same
                       truth-class discipline as pause_engagement
                       after settlement).
never-activated        no tail observation ever published → position
                       never existed (unknown, NOT zero) — the
                       open-abort episode class cannot forge one.
```

## 8. F5 seek forward-compatibility (rule recorded, nothing built)

The cells are episode-local accumulators whose source-relative meaning
holds only within one seek epoch. F5's cutover protocol (three layers,
still OPEN) owns their rebase and any new base offset; F4 freezes only
the forbidden-regression guard — the projection MUST NOT mix pre- and
post-cutover submitted totals — and the shape itself needs no new
architecture noun (no Generation / SeekId / TimelineSegment / base
term earned here).

## 9. Realtime cost analysis (frozen shape)

```text
counter writers    edge read path: one relaxed fetch_add per block
                   (~43/s at 44.1 kHz/1024). Output mechanism: one
                   relaxed store per loop iteration / park slice /
                   drain check (~50–100/s). No per-frame accounting.
reader path        two relaxed loads + clamp at observe(), under the
                   existing completion-lock snapshot; no new lock.
per-quantum effect two relaxed RMW on two episode-local cache lines;
                   no allocation, no dispatch, no K0 visibility.
```

## 10. Environment and limitations

```text
endpoint          Windows default render endpoint, 48 kHz float32 mix
                  (F3 record): 44.1 kHz shared-mode Initialize refused
                  (0x88890008), AUTOCONVERTPCM accepted — the product
                  Tier-1 format-negotiation differential is F3-era
                  recorded reality, unchanged here; SRC authority
                  remains OPEN. GetBuffer/padding units are frames of
                  the INITIALIZED format, so consumed stays
                  source-relative even under a future SRC fallback.
physical channel  WSL2 → Windows interop (binfmt) executed the
                  mingw-built probe as a real Windows process with
                  audio device access.
audibility        NOT CLAIMED anywhere; the proposition is
                  device-consumed presentation position (frames the
                  engine has taken from this stream's buffer), not
                  acoustic truth at the speaker. All latency downstream
                  of engine consumption (engine queue, hardware, DAC)
                  is excluded and unmeasured here; product status must
                  not present the projection as the acoustic instant.
formalization     no new TLA+ obligation: the two-cell derivation's
                  collision space (staleness/tear by one block in
                  either direction) is bounded by the oracles and
                  physically measured; no independently legal states
                  interleave into an illegal state beyond that bound
                  (PBK-001 §13 policy).
```

## 11. Authority proposal status

The D14.8 amendment in this branch (`docs/adr/ADR-PBK-002.md`) freezes
the propositions, truth classes, evidence cells, writer/reader rules,
unknown/pause/EOF/terminal/seek rules, realtime cost boundary and the
rejected alternatives above. Representation (Rust spelling of cells,
observation fields, decode duration seam) is deliberately NOT frozen —
F4-implementation decisions under D14.10. `Resumed`/transport enums
remain forbidden; D11 unchanged; P1–P5 not triggered.

## 12. Fresh adversarial review (attacks A–J)

Run against the amendment + this record after the gates, before
opening the PR. Verdicts and dispositions:

```text
A  fake audible position      PARTIAL → FIXED. "Device-consumed" could
                              be read as acoustic presentation; the
                              amendment now names the latency exclusion
                              (engine queue/hardware/DAC unmeasured) and
                              states the estimate-not-measurement rule.
B  metadata truth inflation   PASS. Duration is optional source-scoped
                              Mechanism Evidence, unknown stays unknown,
                              the ~29% overclaim is recorded, no
                              estimate-to-fill-the-UI is allowed.
C  pause lie                 PASS. Freeze instant == D14.7
                              tail-quiescence; command-time freeze is
                              explicitly forbidden; measured advance
                              480 < 1024 frames after the command.
D  EOF lie                   MAJOR → FIXED. The unqualified
                              "consumed == exact decoded total" claim
                              became conditional on the Completed path
                              (producer blocks, abandonment only on
                              stopped/failed terminals — verified in
                              edge.rs), keeping the measured
                              384000 == 384000.
E  unknown collapse          PARTIAL → FIXED. Unknown-before-first-tail
                              and withdrawal-at-terminal were right; the
                              accuracy statement only described backward
                              tearing. The bound is now stated in both
                              directions (± one in-flight block, the
                              forward lead measured at stream start),
                              with the clamp's one-sided effect named.
F  global-state creep        PARTIAL → FIXED. Cells were session-owned
                              and global stores forbidden; a writer
                              lifetime rule was missing. Each writer now
                              exists only while its mechanism is live,
                              so a dying leg cannot publish into an
                              observable projection.
G  feature-shaped arch       PASS. No Plugin/Capability/Fact kind/
                              authority/lifetime noun/store is created;
                              Position is a Projection.
H  realtime regression       PASS. One relaxed publication per
                              hand-off, one per loop iteration/park
                              slice; reader derives under the existing
                              snapshot lock; no new lock, allocation,
                              dispatch, or K0 visibility.
I  seek trap                 MAJOR → FIXED. The amendment had
                              pre-decided where F5's landing offset
                              enters ("at the observation") while F5's
                              cutover stays OPEN — the PR #143 MAJOR-1
                              class. F4 now freezes only the no-mixing
                              constraint; rebase and base term are
                              F5-owned.
J  fact inflation            PASS. Position/Duration never Facts, no
                              fact authority, D11 contract unchanged,
                              P1–P5 untriggered.
```

Additional attacks beyond A–J:

```text
K  writer identity           VERIFIED, then hardened in text. Production
                              read→release is adjacent (read_frames →
                              ReleaseBuffer(n)); a handed-out block that
                              is never submitted can only be a terminal
                              abort, where the projection is withdrawn.
                              Mutation risk (counting decoded frames or
                              counting a discarded read) is now
                              explicitly ruled out by the amendment.
L  router truth              PASS. CONTEXT.md / docs/README.md /
                              overview.md all mark D14.8 PROPOSED
                              pending human review; no silent promotion.
M  D14.7 coherence           PASS. The old "tail-quiescence must never
                              be promoted to a position source" sentence
                              is rerouted to the D14.8 explicit
                              selection; the establishment latch keeps
                              its F3 meaning.
N  probe identity            PASS, tightened. RESULTS/README now state
                              the probe is a mechanism twin (production
                              render-loop order, probe-local code, no
                              production crate linked on the Windows
                              path) — its numbers are twin measurements,
                              not production measurements.
```

No finding remains open; no AUTHORITY_GAP was reached, and no
production code was touched.

