# F4-GATE mechanism evidence — Position / Duration (Issue #119)

Status: **GATE EVIDENCE** — proposition inventory + publisher-side
position algebra + physical measurements + duration provenance, and the
authority proposal for ADR-PBK-002 §20 D14.8 (carried by the ADR
amendment in this same branch, at F4-GATE-CORRECTIVE-1). Not architecture
authority by itself; production code is untouched.

```text
BASE_SHA:        c1864cd568b702af983cee25505bf477cce93380 (main; PR #150 merge)
BRANCH:          research/f4-timeline-gate-1
SUITES:          cargo test (experiments/f4-timeline-gate) — 10/10 oracles × 3 runs
PHYSICAL:        f4probe.exe on the Windows host via WSL interop
                 (shared-mode WASAPI, default endpoint, 48 kHz float32
                 mix format), 3 runs, failures=0 each, in the
                 F4-GATE-CORRECTIVE-1 shape; the superseded two-cell
                 reader shape's raw runs are kept as
                 evidence/f4probe-pairtear-run*.log
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
    "not earned", and the seam contract is "one coherent pure read".
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
| handed-off total (`read_frames` → `ReleaseBuffer(n)`) | Render leg, mechanism-local accounting | Mechanism Evidence | exact submitted count | frozen at engagement | rebase at cutover | YES — derivation base (not published) |
| device tail (`GetCurrentPadding`) | Render leg (same execution path) | Mechanism Evidence | engine-queued frames of this stream | drains to 0 while parked | rebase at cutover | YES — derivation subtrahend (not published) |
| published monotone sample (`handed_off − tail`, max-guarded) | Render leg writer → session-owned cell | Mechanism Evidence | exact for the writer's own instant; the reader sees the latest published sample, no freshness bound claimed | stops moving exactly at tail quiescence (= Paused evidence) | rebase + new base at cutover | **YES — the derivation the product reads** |
| Position (pure load of that sample) | Playback Session observation | **Projection** | as above; never a Fact | Paused and Position share the tail-quiescence evidence, neither derives the other | follows the cell | **YES — the product Position** |
| IAudioClock position | Output mechanism (device) | Mechanism Evidence | device timeline; unit is endpoint/stream-specific (measured byte-rate here) | kept counting through park | device-stream-relative, needs origin mapping | NO (rejected, §4) |
| container/stream duration_us | Decode provider (probe) | Mechanism Evidence | reported metadata; measured overclaim up to ~29% on damaged input | N/A (source-scoped) | source-scoped | YES — as optional evidence |
| exact decoded total at EOF | Decode leg | Mechanism Evidence (exact, terminal-only) | exact | N/A | source-scoped | YES — terminal consumption truth; NOT exposed as Duration |

```text
SELECTED_POSITION_PROPOSITION
    device-consumed presentation location for the current episode,
    source-relative, in source PCM frames
SELECTED_POSITION_MECHANISM
    render leg derives handed_off − min(tail, handed_off) from its own
    two mechanism-local values and publishes it as a monotone
    non-decreasing sample in one session-owned cell; the application
    reads that cell with one pure load (no reader state, no cross-cell
    composition, no clamp outside the publication)

SELECTED_DURATION_PROPOSITION
    the duration the decode mechanism reports for the source at
    open/probe time; unknown stays unknown
SELECTED_DURATION_MECHANISM
    SongCore probe/stream duration_us relayed once at activation as
    episode evidence (same shape as source_format)
```

## 3. Experiment A — handed-off / padding / published sample across phases

Physical: `f4probe.exe`, Windows host, shared-mode event-driven
WASAPI, default endpoint, 48 kHz mix format, buffer_frames=1056,
block=1024, 8 s finite tone (384000 frames). Enforced invariants set
the exit code; 3/3 runs failures=0 (raw logs in `evidence/`).

The probe is a **mechanism twin**: it runs probe-local code in the
production render-loop order (gate → GetCurrentPadding → publish →
GetBuffer → `read_frames` → `ReleaseBuffer` → hand-off accounting)
against a real endpoint, driving the same `PositionEvidence` the oracles
use (imported from the crate lib, not copied). No production crate is
linked on the Windows path, so every number below is a twin measurement
of the mechanism shape, not a measurement of production code.

| Run | pause-command advance (frames) | frozen window | reader backward steps | reader ≥ handed-off | writer estimate regressions | EOF final | publications/s (steady) |
|---|---|---|---|---|---|---|---|
| 1 | 480 | 453 samples, constant | 0 | never | 0 | 384000 == total | 96 |
| 2 | 480 | 454 samples, constant | 0 | never | 0 | 384000 == total | 98 |
| 3 | 480 | 438 samples, constant | 0 | never | 0 | 384000 == total | 98 |

Established physically:

```text
hand-off accounting freezes at render-gate engagement:
    after the pause command, the handed-off total advances by one
    in-flight block at most (measured 480 < 1024) — the F3 mechanism-A
    invariant (gate strictly before GetBuffer) is exactly what makes it
    freeze.

the published sample stops moving exactly at tail quiescence:
    the sample is bit-constant through the quiesced park (first == last,
    hundreds of samples) and equals the frozen handed-off total at park
    end (measured: position_at_park_end == handed_off_at_park_end in all
    three runs). The freeze point IS the D14.7 output-tail-quiescence
    evidence — the same reading that establishes Paused. No separate
    freeze mechanism exists or is needed, and nothing freezes at command
    time.

a pure load never regresses: reader backward steps = 0 across the whole
    run while the tail reading itself is not monotone; the published
    sample is never above the mechanism's own accounting; the reader
    holds no state.

the real engine never regressed the writer's own estimate on this
    endpoint (writer_estimate_regressions = 0 in all three runs). The
    max-guard in the publication is therefore a cheap safety net here
    rather than a measured necessity — it is required by the contract
    because the tail reading is not monotone by contract, and the
    oracles pin its behavior for arbitrary schedules.

no freshness bound is claimed or measured: the reader samples every
    ~2 ms (≈420/s) and sees ≈802 distinct values, i.e. it observes the
    mechanism's own publication cadence (measured 96–98 publications/s
    ≈ 11 ms ≈ 528 frames at 48 kHz). How old a sampled value is depends
    on the reader's poll interval and that cadence — an asynchrony
    property, not a concurrency bound.

EOF:
    drained_to_total=true — the published sample rises to the exact
    handed-off total as the device drains, and D11 Completed observes
    the same tail==0 reading. On the Completed path the handed-off
    total equals the exact decoded total (384000 == 384000) because
    every produced frame is handed out: the edge producer blocks rather
    than dropping, and buffered frames are abandoned only on a
    stopped/failed terminal (edge.rs) — verdicts that do not claim
    Drained.
```

Simulation cross-check (all-platform oracles, `src/timeline.rs`):
undefined≠zero, exactness per writer instant, tail capping, monotone
publication under a regressing queue, consumed ≤ handed-off, per-park
freeze, EOF rise, terminal withdrawal, the rejected two-cell reader pair
as a negative control, and 5×400-step scripted interleavings — 10/10,
deterministic.

## 4. Experiment B — IAudioClock considered and rejected

Measured on the same runs (clock sampled on the render leg only):

```text
48 kHz leg      GetFrequency = 384000  = 48000 × 8   (numerically the
                stream-format BYTE rate — block align 8, not frames)
44.1 kHz leg    stream opened with AUTOCONVERTPCM on the same 48 kHz-mix
                endpoint; GetFrequency = 352800 = 44100 × 8
                → source_relative=false

position comparison (48 kHz leg, byte/8 → frames):
    clock-derived consumed vs the published sample at end of phase:
    run1 −861, run2 −865, run3 −868 frames (≈18 ms ≈ one engine
    period, 1056 frames) — a stable offset across runs, not drift: the
    clock tracks the same quantity the algebra already derives, after a
    unit conversion the algebra never needs. It also keeps counting
    through the park, where the published sample correctly freezes.
```

Narrowed claim (F4-GATE-CORRECTIVE-1): on the exercised endpoint
GetFrequency numerically matched the initialized stream format's byte
rate. That is an observation about this endpoint, not a general rule —
the documented `IAudioClock::GetFrequency` contract only guarantees that
the frequency unit is compatible with the unit of `GetPosition`, and the
unit may vary by stream/device. Either way the clock requires
unit/origin interpretation.

Verdict: the device clock is the **larger mechanism delivering no
additional source-relative truth** — it needs a COM service
acquisition, unit/origin interpretation, and device-origin mapping to
say what `handed_off − tail` already says in native source frames.
REJECTED by the smallest-mechanism razor; re-earnable only by a new
narrow authority decision if a proposition appears that the algebra
cannot support (e.g. device-clock-exposed product features).

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
                       the device is still draining the handed-off tail
                       and the published sample truthfully keeps
                       advancing.
render engagement      hand-offs stop (gate before GetBuffer).
tail quiescence        the published sample reaches the frozen
                       handed-off total; it stops moving EXACTLY here —
                       the same evidence that establishes D14.7 Paused.
paused park            tail stays 0 (park slices keep publishing);
                       published sample constant (measured bit-constant).
resume                 hand-offs continue; the sample follows.
```

No "position froze when the command was issued" lie exists in this
shape; the freeze instant is mechanically the Paused-establishment
instant.

## 7. EOF / terminal semantics

```text
decode EOF             edge drains; NOT product completion (D11).
device drain           the published sample rises to the exact
                       handed-off total.
D11 Completed          DrainVerdict::Drained == the same tail==0
                       reading: at settlement, the published sample
                       equals the exact decoded total. Whether that
                       equals the reported Duration is NOT guaranteed
                       (§5) and is never asserted.
Stopped / Failed       terminal Fact commits → the observation
                       withdraws the position (None). No final-
                       position latch storage is earned; the cell
                       simply stops being read (same truth-class
                       discipline as pause_engagement after
                       settlement).
never-activated        no publication ever happened → position never
                       existed (unknown, NOT zero) — the open-abort
                       episode class cannot forge one.
```

## 8. F5 seek forward-compatibility (rule recorded, nothing built)

The cell is an episode-local accumulator whose source-relative meaning
holds only within one seek epoch. F5's three-layer cutover protocol owns
its rebase (decode-side serialization point), plus a new base offset
(song_seek's actual landing) added at the projection. The F4 shape —
mechanism-local accumulation + one monotone published sample + a pure
read — requires no new architecture noun for this (no Generation /
SeekId / TimelineSegment). The forbidden-regression guard: the monotone
publication MUST NOT mix pre- and post-cutover handed-off totals.

## 9. Realtime cost analysis (frozen shape)

```text
writer             the render leg keeps one plain local counter and
                   derives from its own two values: one relaxed
                   monotone RMW per loop iteration / park slice / drain
                   check (~96–98/s measured steady ≈ 11 ms), on a cell
                   that is cache-hot on the rendering thread. No new
                   device call — the padding reading already exists.
reader             ONE relaxed load per observation; no reader state,
                   no new lock, no allocation.
per-quantum effect nothing beyond O(1) work already on the rendering
                   thread; no dispatch, no K0 visibility.
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
                  device-consumed presentation position, not
                  acoustic truth at the speaker.
formalization     no new TLA+ obligation: the publication's collision
                  space is single-threaded by construction (one writer
                  owns both inputs), the reader holds no state, and the
                  remaining asynchrony (freshness) is explicitly NOT a
                  correctness claim (PBK-001 §13 policy).
```

## 11. Authority proposal status

The D14.8 amendment in this branch (`docs/adr/ADR-PBK-002.md`) freezes
the propositions, truth classes, derivation ownership, writer/reader
rules, unknown/publication/freshness/pause/EOF/terminal/seek rules,
realtime cost boundary and the rejected alternatives above, at
F4-GATE-CORRECTIVE-1. Representation (Rust spelling of the cell,
observation fields, decode duration seam) is deliberately NOT frozen —
F4-implementation decisions under D14.10. `Resumed`/transport enums
remain forbidden; D11 unchanged; P1–P5 not triggered.

## 12. Round-1 adversarial review (attacks A–J, against commit 36104ce8)

Run against the amendment + this record after the gates, before opening
the PR. Verdicts and dispositions:

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
E  unknown collapse          PARTIAL → SUPERSEDED by §13. Unknown-before-
                              first-publication and withdrawal-at-
                              terminal were right, but the accuracy
                              statement this attack produced (a
                              "± one in-flight block" bound) was itself
                              withdrawn as a correctness claim.
F  global-state creep        PARTIAL → FIXED. The cell is session-owned
                              and global stores are forbidden; each
                              writer exists only while its mechanism is
                              live, so a dying leg cannot publish into
                              an observable projection.
G  feature-shaped arch       PASS. No Plugin/Capability/Fact kind/
                              authority/lifetime noun/store is created;
                              Position is a Projection.
H  realtime regression       PASS for the round-1 shape; SUPERSEDED by
                              §13, where the two-cell writer/reader
                              pair collapsed into one publication and
                              one pure load (strictly less work).
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
                              counting a discarded read) is explicitly
                              ruled out by the amendment.
L  router truth              PASS. CONTEXT.md / docs/README.md /
                              overview.md all mark D14.8 PROPOSED
                              pending human review; no silent promotion.
M  D14.7 coherence           PASS. The old "tail-quiescence must never
                              be promoted to a position source" sentence
                              is rerouted to the D14.8 explicit
                              selection; the establishment latch keeps
                              its F3 meaning.
N  probe identity            PASS, tightened. RESULTS/README state the
                              probe is a mechanism twin (production
                              render-loop order, probe-local code, no
                              production crate linked on the Windows
                              path) — its numbers are twin measurements,
                              not production measurements.
```

## 13. Round-2 review (human, pre-merge) → F4-GATE-CORRECTIVE-1

The round-1 PR (#151, HEAD 36104ce8) was reviewed and returned
`CHANGES_REQUIRED` with 2 MAJOR + 1 MINOR. The product semantics
(position/duration propositions, pause anchoring, EOF discipline, F5
no-mixing boundary, fact discipline) were accepted; the findings were
about the publication mechanism:

```text
MAJOR-1  the reader-side clamp `max(last, raw)` had no legal owner.
         `observe()` is contractually "one coherent pure read"
         (crates/qianqian-playback/src/handle.rs) whose purity is tested
         (t8_observe_neither_settles_nor_mutates): clamping there would
         either make the read mutating, push Position into UI-local
         presentation state, or relocate the same mutation into the
         session read path. The `last` argument of the experiment's
         `clamped()` made the missing owner visible: only a caller
         could supply it.
FIX      monotonicity moved to the writer side. The render leg derives
         from its own two mechanism-local values and publishes
         `max(published, estimate)` into one session-owned cell; the
         reader performs ONE pure load and holds no state. `clamped()`
         and the two-cell reader surface are gone from the algebra.
         The rejected shape is kept as an executable negative-control
         oracle (`rejected_two_cell_reader_pair_tears_backward_by_one_
         block`), which also proves the selected shape is immune to the
         same interleaving.

MAJOR-2  the "± one in-flight block" statement was written as a
         correctness bound, but Relaxed atomics give atomicity/coherence
         per location, never a freshness bound: a reader could legally
         see a newer handed-off total with an older tail. What the
         experiment had actually shown was the tear of one *constructed*
         adjacent interleaving, plus this device's empirical behaviour.
FIX      the two-cell reader pair no longer exists, so there is no tear
         for a bound to describe. The bound is withdrawn from the
         contract; freshness is named as an asynchrony property of the
         reader's schedule (poll interval + publication cadence) and
         explicitly not a correctness invariant. The probe now reports
         the mechanism's publication cadence (96–98/s steady) and zero
         reader-visible backward steps, and keeps the old raw runs as
         `evidence/f4probe-pairtear-run*.log` — the record of the
         superseded shape (measured max backward step 96, a property of
         the reader's pair loads, not of the device).

MINOR    the IAudioClock conclusion over-generalized: "GetFrequency is
         the initialized stream format's byte rate, never source
         frames". The API's documented contract only ties the frequency
         unit to GetPosition's unit.
FIX      narrowed to the exercised endpoint (measured 384000 / 352800 =
         48000 × 8 / 44100 × 8) plus the contract statement; the
         rejection verdict is unchanged, since the clock still needs
         unit/origin interpretation and adds no source-relative truth
         (§4).

Re-run after the corrective (the mechanism shape changed, so the
physical evidence was re-collected): 3/3 probe runs failures=0 with
max_position_backward=0, position_le_handed_off=true,
writer_estimate_regressions=0, drained_to_total=true,
final_position == final_handed_off == 384000; oracles 10/10; production
delta still zero.
```

No finding remains open; no AUTHORITY_GAP was reached, and no
production code was touched.
