# Qianqian Audio Observation Plane

> **Truth class: EVIDENCE / DESIGN INPUT (authority candidate)**
>
> **Status: DRAFT.** This document is merged so #187 has a durable reviewable
> candidate. It does **not** authorize production implementation, does not
> close #187, and does not make any OPEN observation-point/representation
> choice current authority.
>
> Owner work: [#187](https://github.com/jnhu76/qianqian/issues/187)  
> Consumers: [#188](https://github.com/jnhu76/qianqian/issues/188), future GUI,
> and any separately admitted read-only analysis surface.

---

## 1. Authority relationship

| Domain | Authority / source | This document's relationship |
|---|---|---|
| Command / Fact / Projection, PCM firewall, P1-P5 | [`ADR-PBK-001`](../adr/ADR-PBK-001.md) | consumes; does not copy foundational rules |
| Terminal outcome, Plugin admission, seek/pause/Position, processing minimum | [`ADR-PBK-002`](../adr/ADR-PBK-002.md) D11/D13/D14.5/D14.7/D14.8/D14.11 | consumes accepted playback truth; creates no lifecycle authority |
| Output / host backend | [`ADR-PBK-003`](../adr/ADR-PBK-003.md) | does not change PcmEdge/RenderRequest/Output ownership |
| DSP product output, order and contract | [`dsp-product-model.md`](dsp-product-model.md) | consumes accepted DSP product semantics |
| Observation location/material/analysis/snapshot | this candidate + #187 decision/evidence | intended ownership if/when accepted |
| TUI/GUI layout, controls and visual cadence | #188 / future presentation work | consumes observation semantics; does not rewrite audio truth |

“Observation Plane” is a domain-separation term. It does not automatically
earn a Rust type, crate, Plugin, K0 Capability or generic PCM-consumer
framework.

---

## 2. Candidate stable boundary: main playback vs read-only observation

Playback PCM continues through the existing Playback Session / PcmEdge /
Output main path. An observer may only read material at an accepted
observation point. It cannot modify main-path PCM or decide playback order,
seek success, D14.8 Position or D11 terminal outcome.

Candidate observation material/snapshots are:

```text
non-authoritative
ephemeral
lossy
bounded
safe to drop
```

Slow consumers may lose/overwrite old observation material or lower analysis
frequency. Playback must never wait for a visualizer to consume complete data.
This lossy contract is local to observation and cannot be generalized to
Output, recording or another future PCM consumer with different semantics.

FFT, RMS, band aggregation, waveform downsampling and display smoothing do
not run as mandatory work on the playback per-block critical path. The
producer side may perform only bounded transfer/publication work. Analysis
runs in a separate execution context; exact worker/task/thread representation
remains OPEN. PCM does not transit K0, Context or generic EventBus per block.

Queue/window/history/snapshot sizes and one analysis step all require explicit
bounds. Observer disappearance or shutdown must not make playback teardown
wait without bound. Observation failure invalidates its own data/availability;
it does not create a playback `Failed` Fact unless the actual failure also
hits a playback mechanism governed by existing authority.

---

## 3. Observation point remains OPEN

The default #187 candidate is:

```text
post-DSP / pre-PcmEdge
```

This document does **not** accept that location yet. #187 must decide the tap
using timing, cost, lifecycle, format and freshness evidence.

If accepted, the observed signal must be named honestly:

> **Qianqian application post-processing PCM view** — not
> device-consumed PCM, physical-device truth or acoustic-output truth.

It reflects the processing configuration actually applied to those samples,
not current UI draft/desired values that have not yet been applied. It is
upstream of D14.9 Output Volume and any host-backend adaptation; therefore
Output Volume changes need not change this view's sample peaks, and this view
cannot prove final device loudness or true peak.

If #187 selects another point, domain/format/time/Volume/protection wording
must change with it. “Observation support” is not blanket permission for an
implementation to place taps arbitrarily.

---

## 4. PCM material meaning and lifetime

Analysis input must remain interpretable as belonging to:

```text
one observation point/domain
one actual PCM format/channel meaning
one episode/cut world
one observation interval
one bounded sample/material payload or derived datum
```

These concepts do not require every item to become a public field or a new
playback Generation/epoch type.

Required constraints:

- an observer cannot retain a mutable staging-buffer borrow that decode/DSP
  will later reuse or modify; owned copy, preallocated transfer or another
  bounded representation must be justified by implementation evidence;
- publishing material to observation does not prove that it entered PcmEdge
  or was consumed by the device; cut/failure/cancellation can still invalidate
  later main-path contribution;
- a processed remainder retried after `RefusedUnchanged` remains the same
  signal time; DSP is not rerun and observation must not manufacture duplicate
  “new time” from a retry point;
- dropped material breaks continuity. An FFT/waveform implementation must
  reset, mark a discontinuity or use another explicit gap policy; it cannot
  splice missing samples into one apparently continuous signal;
- multiple presentation/analysis consumers do not automatically justify
  per-consumer PCM queues, a consumer registry or independent provider
  lifecycle.

---

## 5. Time semantics: produced is not consumed

These are distinct:

```text
PCM decode/DSP production time
observation-material publication time
analysis completion time
presentation refresh time
D14.8 device-consumed Position
```

Decode/DSP staging may run ahead of device consumption. A post-DSP/pre-PcmEdge
sample can therefore represent audio the user has not heard yet.

Pause makes this distinction especially important. Current pause authority
gates the **render leg**, not the decode/DSP producer. While bounded PcmEdge
capacity remains, decode/DSP may continue producing real future PCM. Therefore
Paused does not imply “observer receives no more material.”

Snapshot timing must state its domain and the meaning of age/loss/staleness.
Possible coordinates include a source-relative observation interval,
processing/output interval or analysis timestamp; exact choice remains OPEN.
Wall-clock time and produced-frame count must not be presented as
`device-consumed Position`.

A surface that claims “this spectrum is synchronized with what you hear now”
needs a separately defined synchronization strategy and error/buffer budget.
This draft makes no such guarantee. “Latest snapshot” is a freshness policy,
not a playback clock.

---

## 6. Cut, pause, replacement and late results

Observation consumes existing episode/seek outcomes; it never decides them.

| Upstream event | Candidate observation obligation |
|---|---|
| `RefusedUnchanged` | no fake cut; normal continuation is not reset solely because the seek was refused; local gap/drop policy still applies |
| `Applied` | invalidate old pending material, FFT overlap/window, waveform history, signal-derived smoothing and old results; new snapshots contain no pre-cut signal contribution |
| `MutatedThenFailed` | retire/fail with the old episode; do not reconstruct normal old continuation |
| Pause / Resume | preserve upstream semantics; do not assume decode/DSP immediately stops; freeze/decay/latest display policy is separate |
| Open / replacement | retire old episode observation state; new episode gets a fresh observation context |
| Stop / terminal / teardown | bounded cancel/retire; stale old snapshots cannot reappear as fresh live truth |

Clearing a queue is not enough. An old FFT/analysis task can finish after an
Applied cut. The publish path must reject a result that no longer belongs to
the valid episode/cut world. Exact token/cancellation/sequence representation
remains OPEN and does not require a new playback Generation/RCU mechanism.

If presentation uses wall-clock decay while paused, that decay is display
animation. It is not evidence that DSP envelopes advanced or Position moved.

---

## 7. Snapshot semantics

A snapshot is read-only, presentation-independent observation data. Its audio
meaning must be defined independently of Ratatui cells, GUI pixels, colors or
layout.

| Data | Semantic questions that must be stable before acceptance | Representation left OPEN |
|---|---|---|
| Spectrum | frequency-band meaning, unit/reference, aggregation and smoothing semantics, validity | FFT size/library, internal window storage, bar geometry |
| Peak / level | channel policy, analysis window, normalization/unit, sample-peak vs other measures | LUFS/true-peak claims unless separately implemented/validated |
| Waveform | bounded interval, channel policy, decimation, gap meaning | terminal glyphs, point count, unbounded history |
| Identity/time/validity | episode/cut relation, observation domain, age/loss/stale semantics | whether every concept becomes a public field |

`dBFS`, sample peak, RMS, DSP preamp, Output Volume and acoustic loudness are
different concepts and must be named separately. Float PCM may exceed nominal
full scale; rendering clamp must not hide that fact in the semantic data.
Nothing is called LUFS or true peak without the corresponding accepted and
validated measurement semantics.

Analysis cadence, presentation cadence and audio sample rate remain separate.
Whether hidden visualizer views stop analysis, retain the last snapshot or
change cadence is later #187/#188 policy.

---

## 8. Consumption of future DSP regimes

Observation cannot bake the current Gain/EQ one-source-frame-to-one-output-
frame assumption into a permanent contract.

| Future obligation | Observation must adapt to |
|---|---|
| `L` latency/lookahead | tap signal time, priming, freshness/synchronization labels |
| `T` mandatory residual/drain | valid tail observation and cancellation; no fabricated source Position/Duration |
| `M` frame/time mapping | separate source and processing/output coordinates; consume accepted mapping |
| `F` rate/channel/layout transform | actual tap output rate/layout/channel meaning |
| live transition | actual configuration/transition that produced material; latest desired config is not automatically applied config |

Those changes first require their upstream DSP/Position/PCM/Output authority.
Observation adds consumer obligations; it cannot authorize a new processing
regime merely because a visualizer wants to inspect it.

---

## 9. OPEN decisions and non-authorized scope

Before #187 can accept/implement the first observation slice, it must decide
at least:

```text
tap location
transfer representation and capacity
drop/gap policy
analysis windowing/cadence
snapshot time/validity model
pause display policy
snapshot fields/visibility
FFT/window/band/level parameters
steady-state cost/allocation/teardown bounds
```

Values such as 1024/2048-point FFT or 30–60 Hz analysis/render cadence remain
candidates, not authority.

This draft does not authorize:

```text
recording / lossless capture
beat/pitch/music recognition
LUFS compliance
microphone/room analysis
visualizer-driven playback control
cross-episode retained signal state
generic EventBus PCM
a new Plugin/Capability identity
```

If correct observation implementation would need to alter PcmEdge semantics,
Position, main-path backpressure, provider lifecycle or cross-episode signal
state, #187 must identify the exact authority gap and reopen it narrowly.
Multiple presentation consumers alone do not earn D13 Plugin admission.

---

## 10. Acceptance evidence required by #187

The first accepted observation slice must at minimum demonstrate:

```text
slow observer never makes main playback wait for completion
capacity and per-analysis work are bounded
Applied cut rejects late old-world results
RefusedUnchanged continuation is not falsely reset
pause bounded-prefetch behavior matches snapshot/display policy
replacement/terminal retires old observation state
shutdown/teardown is bounded
domain/format/time labels are truthful
steady-state cost and allocation behavior have evidence
```

The result proves only the selected observation slice. Snapshot/UI tests do
not substitute for Windows native E2E, device/acoustic evidence, DSP algorithm
correctness or unrelated realtime guarantees.

```text
DOCUMENT_STATUS = DRAFT
OBSERVATION_POINT = OPEN (post-DSP/pre-PcmEdge is default candidate)
REPRESENTATION = OPEN
AUTHORITY_ACCEPTED = NO
ISSUE_187_CLOSED = NO
PRODUCTION_CHANGE = NONE
```
