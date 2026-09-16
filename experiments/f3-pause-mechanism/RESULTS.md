# F3-GATE mechanism evidence — Pause/Resume (Issue #119)

Status: **GATE EVIDENCE** — mechanism comparison + establishment semantics
proposal for ADR-PBK-002 D14.7. Not architecture authority; the ADR
amendment in this same branch carries the proposal. Production code is
untouched.

```text
BASE_SHA:        99b20befac8f1b64d4cef8fd4697df8d0ed80e86 (main)
BRANCH:          research/f3-pause-gate-1
SUITES:          cargo test (experiments/f3-pause-mechanism) — 11/11 × 6 runs
PHYSICAL:        f3probe.exe on the Windows host (shared-mode WASAPI,
                 default endpoint, 48 kHz float32 mix format),
                 round 1: 4 runs; CORRECTIVE-1 round: 4 runs (incl. 5 s park)
AUDIBILITY:      NOT CLAIMED — whether pause SOUNDS right is the human
                 reviewer's acceptance on a real device
```

§8 records CORRECTIVE-1 (human review round 2): render engagement is
NOT an audible pause; the corrected establishment adds output-tail
quiescence evidence and demarcates Paused/Resumed as Projections.

## 1. Mechanism comparison

```text
A  explicit render-loop pause gate located BEFORE WASAPI GetBuffer
   (loop-top park; device stream stays open; submitted tail plays out
   then the device renders silence; edge contents preserved)

B  the same gate wrapped in IAudioClient::Stop / Start around the park
   (device consumption frozen; submitted tail stays queued and
   continues on resume)

REJECTED (campaign rule): "let read_frames block while GetBuffer is
   held" — i.e. parking inside the edge read. The production render
   loop already blocks inside read_frames while holding a GetBuffer
   when the edge is empty (empty-edge backpressure shape). That shape
   must NOT become the pause mechanism; the negative-control scenario
   (broken loop holding a buffer across the park) proves the harness
   catches it.

C  no genuinely smaller credible third mechanism exists in the current
   code/API reality. Considered and rejected as NOT smaller and not
   honest: submitting muted/zero PCM while "paused" (keeps the fill
   path running, muddies PCM/position semantics). The event-wait park
   variant (parking at the WaitForSingleObject instead of a separate
   gate) is the same mechanism class as A — a loop-top park — spelled
   differently.
```

Both A and B share the same semantic spine (this is what the gate
freezes; A vs B differs only in device-buffer physics):

```text
same-episode, non-terminal        no new Fiber, no D11 settlement
unpark-and-continue stop wake     the gate NEVER aborts the render leg;
                                  the data-plane terminal decides
bounded park slice                command latency capped without
                                  depending on notify delivery
engagement acknowledgment         "Paused truthfully established" =
                                  observed command + parked ack
no device buffer across the park  GetBuffer never held while parked
device stream stays open          no resource replacement in either
P1–P5 NOT triggered               no old/new RT world overlap: same
                                  edge, same render stream, same
                                  device session across pause/resume
```

## 2. Decision — propose A as the minimum accepted mechanism

A is the minimum: it adds one gate + a bounded park to the existing
loop; it touches no COM stream state. B adds IAudioClient::Stop/Start
failure modes and freezes mid-buffer audio, for no Phase-F product need
(immediate-freeze pause semantics are not a current requirement and may
be re-earned later by a new narrow decision, with B as the natural
candidate). The measured physics:

```text
already-submitted audio   A: plays out (padding drains to 0), then silence
                          B: frozen (padding constant), continues on resume
pause engage latency      A: ack ≤ in-flight iteration + park slice
                          B: same + Stop() call
resume latency            A: wake → refill within one period
                          B: wake → Start() → continues frozen buffer
state-changing COM calls  A: none        B: Stop/Start per cycle
```

## 3. Synchronization-shape evidence (all platforms)

`cargo test` — 11 scenarios, 6 consecutive all-green runs. Oracles read
an instrumented total event order after all joins; the gate publishes
its Engaged/Disengaged acknowledgments at the state-change point
itself, so the oracles observe the real ack order (the negative control
proved this oracle setup catches the broken shape — an earlier oracle
draft that never saw ack events WAS vacuous and the negative control
caught it). The two CORRECTIVE-1 scenarios pin the corrected
establishment: Paused waits for output-tail quiescence, and the
engagement-only mutant goes RED in the reviewed state (§8).

| Campaign question | Established by |
|---|---|
| Render thread holds GetBuffer while paused? | Never in A/B (park strictly before GetBuffer; oracle tracks GetBuffer→Release pairing across the park; negative control proves non-vacuity) |
| Already-submitted audio? | Physical (§4): A drains to 0; B frozen |
| PcmEdge contents? | Preserved across the park (frozen at capacity while parked; consumed only after resume) |
| Decoder stops via bounded backpressure? | Producer reaches full-edge blocked state (write-in-flight && capacity && no progress) and unblocks on resume |
| Stop wakes every blocked participant? | session_stop (edge stop + gate release) → parked render exits in ms, producer exits via WriteOutcome::Stopped; all joins bounded |
| pause × stop | Mid-play: unpark → PcmPull::Stopped → abort → the worker-Stopped × drain-Aborted × intent history → existing table's Stopped (no resolver change) |
| pause × EOF | EOF while parked: no drain → unsettled until resume (drains → Drained) or stop (plays tail → Drained → the frozen table's Completed history — same as today's stop-after-EOF) |
| pause × failure | Failure evidence settles immediately by existing precedence; parked leg still joined through the gate stop |
| Resume after prolonged pause | Liveness across 32 repeated pause/resume cycles (monotone consumption); physical 5 s park (§4) |
| Pause before steady playback | Command before the first loop iteration engages without ANY GetBuffer ever running |
| Resource replacement | None required in either mechanism (one device session per phase) |

### resolver-consistency (why the frozen D11 table needs NO change)

The gate shape produces only evidence histories that the already-merged
48-tuple decision-table conformance oracle (PR #144) classifies without
modification, because the gate's stop wake is **unpark-and-continue**:

```text
mid-play stop from pause   worker Stopped × drain Aborted × intent
                           → existing table: Stopped
stop from pause after EOF  worker Eof × drain Drained → existing
                           table: Completed (tail plays out — the same
                           outcome as today's stop-after-EOF; the gate
                           does NOT force the drain-Aborted ×
                           worker-Eof device-failure history)
failure while parked       decode-failure-first precedence, unchanged
```

The one trap (documented for F3 implementation): a gate that force-
aborts on stop would fabricate the drain-Aborted × worker-Eof →
Failed{device} history. The scenarios pin the non-aborting behavior
(`gate_a_stop_after_eof_while_paused_continues_to_drained_not_aborted`).

## 4. Physical WASAPI evidence (Windows host, real device)

`f3probe.exe` (cross-compiled x86_64-pc-windows-gnu, run on the Windows
host via interop; plays ~12 s of quiet 440 Hz tone):

```text
endpoint:      default render endpoint ({0.0.0.0}.{359cb7bc-…})
mix format:    48 kHz / 2 ch / float32 EXTENSIBLE (tag 0xFFFE)
device buffer: 1056 frames (~22 ms); default period 10 ms
mechanism:     shared-mode, event-driven (the production loop shape)
```

Default park 1.2 s; runs 2–4 stable; run 5 = prolonged 5 s park:

```text
                                run2      run3      run5 (park 5s)
A engage ack                    10 ms     10 ms     10 ms
A padding pre-park → post-res.  576 → 0   576 → 0   576 → 0   (drained)
A resume first refill           <1 ms     <1 ms     <1 ms
A GetBuffer-in-park             0         0         0
A stop-from-paused exit         3 ms      4 ms      4 ms
A device sessions               1         1         1
B engage ack                    10 ms     10 ms     10 ms
B padding pre-park → post-res.  576 → 576 576 → 576 576 → 576 (frozen)
B resume first refill           <1 ms     <1 ms     <1 ms
B GetBuffer-in-park             0         0         0
B device sessions               1         1         1
B stop-from-running exit        clean     clean     clean
```

Device-session continuity is enforced by the probe's pass criterion
(delta == 0 within a phase), not merely printed. Resume "first refill"
is command → first post-resume GetBuffer, millisecond-truncated
(observed 0 in every run).

### §4.1 CORRECTIVE-1 round — tail-quiescence timing (4 runs: 3 default + 1 × 5 s park)

The probe now carries a dedicated tail observer: a second MTA thread
that samples the episode session's `GetCurrentPadding` at ~2 ms cadence
during the park (the parked render leg makes no WASAPI calls, so the
observer is the session's only caller). Enforcement added to the pass
criterion: A MUST observe the tail quiesce (padding → 0 after
engagement, never rising again, within 500 ms); B MUST stay
unquiesced.

```text
                                    3 default runs      5 s park run
A engage ack                        10 ms / 10 / 10     10 ms
A padding at engage (observer)      1056 / 1056 / 1056  1056  (≈ one full
                                                              device buffer,
                                                              ~22 ms @ 48 kHz)
A tail_drain_ms (ack → first 0)     29 / 29 / 28        30
A held zero to resume               yes (486–518 smp)   yes (516 samples)
A GetBuffer-in-park                 0                   0
A stop-from-paused exit             3 ms                2 ms
A device sessions per phase         1                   1
B tail quiesced while parked        no (frozen 1056)    no
B park fate                         frozen (576 → 576)  frozen
B device sessions per phase         1                   1
```

Interpretation (corrected establishment, D14.7):

```text
T_engaged        the engage ack (observed; ≤1 ms poll skew)
T_tail_quiesced  first post-engagement padding==0 observation
tail_drain_latency = T_tail_quiesced − T_engaged ≈ 28–30 ms
                 one full device buffer (1056 frames ≈ 22 ms) plus
                 period quantization and ack-poll skew
```

During `T_engaged … T_tail_quiesced` the parked episode's submitted
tail is still pending for rendering — the reviewed state
(intent ∧ engaged ∧ unsettled ∧ padding > 0). Only after
`T_tail_quiesced` is the corrected Paused projection true. Mechanism B
can never observe quiescence while parked (padding frozen), which is
the establishment-side discriminator that keeps B unselected. These
timings are mechanism evidence for F4 planning; they are NOT product
Position.

Interpretation:

```text
A  the ~12 ms submitted tail audibly plays out, then the device renders
   silence for the rest of the park; on resume the loop refills from
   the preserved edge. The ~170 ms of decoded PCM held in the edge is
   untouched while parked (scenario 1 pins this).
B  the submitted ~12 ms is frozen mid-buffer and CONTINUES on resume.
```

Both are honest pause semantics. A's tail-play-out is bounded by the
device buffer (tens of ms at this endpoint), not by the edge capacity.

The probe does NOT claim: audibility correctness (human acceptance),
behavior under device loss while parked (out of F3 minimum; resume-time
device errors surface through the existing abort path), or any
position/duration proposition (F4 OPEN).

## 5. Environment observation (recorded, no action in this gate)

The same init matrix (F3PROBE_INIT_MATRIX=1) shows this endpoint's mix
format is 48 kHz and **refuses a 44.1 kHz float32 EXTENSIBLE shared-mode
Initialize with 0x88890008 (AUDCLNT_E_INVALID_STREAM_FLAG) today —
including with zero stream flags — while the AUTOCONVERTPCM combo and
the 48 kHz mix-format open both succeed**. The product's Tier-1-only
format negotiation (PBK-002 D8 posture; SRC fallback OPEN) may
therefore fail differently on THIS machine today than at the September
F1 baseline. This is a format-negotiation reality differential, NOT a
pause finding; recorded here only so the physical pause evidence's
48 kHz carrier is understood. It changes nothing in the F3 freeze.
(Format/SRC authority remains OPEN-1; any product response is a
separate narrow decision.)

## 6. Proposed authority freeze (carried by the ADR amendment in-branch)

```text
mechanism (minimum accepted)  A — explicit render-loop pause gate before
                              GetBuffer, unpark-and-continue stop wake,
                              bounded park slice, engagement ack
truth classes                 pause intent = Command state (episode seam)
                              engagement/disengagement = Mechanism Evidence
                              output-tail quiescence = Mechanism Evidence
                              Paused/Resumed = application-facing derived
                              Projections (CORRECTIVE-1): NO new Fact kind,
                              NO new fact authority, NO Paused terminal
                              variant, NO transport state enum, NO
                              lifecycle noun; never a correctness basis
truthful establishment        paused  ⇔ unsettled ∧ pause intent ∧
                              engagement ∧ output-tail quiescence
                              (CORRECTIVE-1; engagement alone is NOT an
                              audible pause)
                              resumed ⇔ resume release ∧ disengagement
                              evidence (= pause control no longer
                              established and submission re-enabled; NOT
                              "already audible")
                              (all visible on the episode observation;
                              product never infers pause from edge
                              occupancy / FiberState / UI / ad-hoc WASAPI
                              reads outside the establishment chain)
terminal interaction          D11 table unchanged; unpark-and-continue;
                              Completed/Stopped/Failed propositions
                              unchanged; pause never settles terminal
representation                episode-observation field spelling and
                              the port seam that carries the gate to
                              the mechanism stay implementation freedom
                              (the episode-handle public-surface
                              allowlist update is the explicit F3
                              implementation architecture event)
```

Rejected-for-freeze (unless separately re-earned): PausePlugin, any
second playback lifecycle noun, Generation/Window, a transport-state
enum (Playing/Starting/Paused/…), mechanism-B as the frozen minimum.

## 7. Fresh-context adversarial review round

```text
verdict:  ACCEPT_WITH_MINORS (fresh-context reviewer, read-only)
lenses:   12/12 PASS on authority forgery, GetBuffer-across-park,
          backpressure deadlock, stop-wake liveness, EOF classification
          (verified path-by-path against the real resolver),
          pause-truth inference, D13 admission, lifecycle nouns,
          scope discipline, ADR coherence, vocabulary
correctives (applied in-branch):
  MINOR-1  D14.7 paused/resumed conjunction now guarded by
           "episode has no committed terminal outcome"; session
           settlement/teardown must release the gate
  MINOR-2a mechanism-B padding>0 assert replaced by a bounded wait
  MINOR-2b probe enforces device-session delta == 0 in its pass
           criterion (continuity now enforced, not printed)
  MINOR-2c resume-latency row transcribed into §4
no MAJOR finding; suite re-run 9/9 and physical probe re-run green
after the correctives
```

## 8. CORRECTIVE-1 — render engagement is not an audible pause (review round 2)

Human review finding (accepted): the pre-corrective establishment
formula — `Paused ⇔ unsettled ∧ pause intent ∧ engagement` — is too
strong. Mechanism A's own round-1 physical evidence proves the defect:
after the engage ack, already-submitted device audio keeps playing
until the WASAPI shared-mode padding drains. A valid interval exists
(intent ∧ engaged ∧ unsettled ∧ padding > 0) in which render submission
is parked but audible playback is still advancing. Engagement is a
render-leg fact, not an audible-pause fact.

### 8.1 Corrected establishment (now in D14.7)

```text
pause intent            Command state
render engagement       Mechanism Evidence (leg parked at the
                        pre-GetBuffer gate; submits nothing)
output-tail quiescence  Mechanism Evidence: no frame submitted BEFORE
                        engagement remains queued for rendering by this
                        output session — observed as
                        GetCurrentPadding() == 0 from the episode's own
                        render session after engagement, with no
                        submission in between
Paused / Resumed        application-facing derived Projections; NOT
                        Facts, NO new fact kind/authority; never a
                        correctness basis for resume/stop legality,
                        teardown, terminal settlement, resource
                        lifetime, mechanism wakeup, or K0 lifecycle

Paused  ⇔ unsettled ∧ intent ∧ engagement ∧ tail quiescence
Resumed ⇔ "pause control no longer established and render submission
           re-enabled" (explicitly NOT "already audible")
```

Why the padding reading is sound for this mechanism (recorded so the
rule is auditable): the platform defines shared-mode render padding as
exactly the frames of this stream queued up to play in the endpoint
buffer (`IAudioClient::GetCurrentPadding`, Microsoft Learn); the frozen
mechanism shape makes post-engagement submission impossible (gate
strictly before GetBuffer, no device buffer held across the park), so
padding is non-increasing while parked and one zero observation proves
every pre-engagement frame left the queued-to-play set. The claim is
deliberately narrow — no speaker/DAC motionlessness, no downstream
device-latency claim, no other-session silence, no human-hears-silence
claim.

### 8.2 Executable oracles (this crate)

```text
src/establishment.rs   the corrected projection shape + the
                       pre-corrective engagement-only conjunction kept
                       ONLY as the negative-control mutation subject
scenario: paused_projection_establishes_only_after_output_tail_quiescence
           deterministically constructs intent ∧ engaged ∧ unsettled ∧
           padding > 0 (drain-hold on the sim device, mirroring the
           probe's measured 28–30 ms window) and proves Paused is NOT
           claimed there; flips true exactly at the post-engagement
           padding==0 observation; Resumed = disengagement semantics
negative control: negative_control_engagement_without_tail_quiescence_never_establishes_paused
           runs the engagement-only mutant through the same oracle —
           it fails RED in the reviewed state — and pins the corrected
           truth table over all 16 input combinations, disagreeing with
           the mutant exactly where tail quiescence matters
scenario: mechanism_b_freezes_device_consumption_across_the_park (extended)
           B's frozen tail can never be observed quiesced, so Paused
           must stay false for the whole park under B — the
           establishment-side discriminator
```

### 8.3 Physical evidence for the corrected rule

See §4.1: `padding_at_engage = 1056` (one full device buffer),
`tail_drain_ms = 28–30`, held zero to resume; B never quiesces. The
measured `tail_drain_latency` is recorded for future F4 semantics and
is NOT product Position.
