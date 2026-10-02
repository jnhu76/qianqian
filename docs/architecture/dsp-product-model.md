# Qianqian DSP product model

> **Truth class: NORMATIVE AUTHORITY** for Qianqian DSP product semantics.
>
> This document defines durable product-level DSP configuration semantics,
> algorithm contract obligations, ordering rules, compatibility boundaries,
> and the downstream PCM-consumer handoff. It does **not** replace playback
> foundations in [`ADR-PBK-001`](../adr/ADR-PBK-001.md), episode lifecycle,
> seek/pause/Position/processing-minimum authority in
> [`ADR-PBK-002`](../adr/ADR-PBK-002.md), or Output/backend authority in
> [`ADR-PBK-003`](../adr/ADR-PBK-003.md).
>
> D14.11 remains the authority for the processing minimum, episode
> ownership and current decode-worker placement; live parameter update was
> narrowly admitted 2026-10-02 under §7.3 (four operation classes, one
> frozen transition contract).

---

## 1. Scope and authority relationship

This model answers a narrower question than a DSP framework design:

> What product semantics must remain stable while Qianqian grows from the
> current Gain + 10-band EQ slice into additional first-party algorithms and
> richer clients?

In scope:

```text
typed desired DSP configuration
algorithm-specific configuration and contracts
explicit processing order and ordering constraints
draft / desired / applied / saved-state distinctions
processing time/format obligations
headroom/protection boundaries
persistence/interchange compatibility when those boundaries exist
downstream PCM consumer boundary
runtime representation that remains deliberately unearned
```

Out of scope unless separately earned:

```text
new processing algorithms merely because they appear in the long-term map
live parameter publication mechanism
lookahead / mandatory drain / convolution-tail protocol
SRC / frame mapping / channel-layout transforms
third-party native code or dynamic loading
public AudioProcessor API
ProcessorRegistry / DSP graph / generic parameter bus
Plugin-per-effect identity
Observation implementation details (#187)
TUI/GUI interaction design (#188)
```

The ownership hierarchy is unchanged:

```text
SongCore
    owns media -> source-format PCM semantics

Application / product-control
    owns desired DSP product configuration

Playback Session
    owns one episode's applied configuration,
    signal-derived state, runtime resources and teardown

K0
    composes the owners; it does not transport per-block PCM
```

`AudioProcessingPlugin` and `AudioProcessingCapability` remain
`NOT_EARNED_NOW` under D13. Nothing in this document creates a new Plugin,
Capability, playback lifecycle, Fact authority or realtime transport.

---

## 2. Current production mapping

The current production realization is a valid, intentionally small instance
of this model:

```text
Source-format Float32 PCM
        ↓
PCM Gain / Preamp
        ↓
10-band Graphic EQ
        ↓
PcmEdge
        ↓
Output
```

Current product data can express:

```text
Bypass
Gain
Gain + 10-band EQ
8 factory EQ presets
custom 10-band EqConfig values
```

Current ordering is exactly:

```text
Gain -> Graphic EQ
```

Current EQ remains source-rate/source-layout/frame-count preserving and
causal under D14.11. Current implementation has no implicit clipper, limiter,
auto-headroom, mandatory EOF drain, SRC or channel remap.

### 2.1 Rate availability of the fixed band table (LOW_RATE_EQ_POLICY, accepted 2026-10-02, Issue #190 D1)

The historical product limitation — every enabled 10-band EQ configuration
required every fixed center to remain strictly below Nyquist, so the 16 kHz
band refused all sources at **≤ 32 kHz** (Flat failing where Processing Off
succeeded) — is superseded by the **rate-aware active-band profile**:

```text
A fixed product band participates in an episode's compiled cascade
iff its center frequency is strictly below that episode's source
Nyquist frequency.
```

Frozen semantics:

- an **available** band compiles normally; its configured trim shapes the
  episode;
- an **unavailable** band (center ≥ Nyquist) is **inert for that episode**:
  it is not compiled (at or above Nyquist the band's normalized frequency
  degenerates and no source content exists there to shape); its configured
  trim STAYS in the desired configuration and becomes active again on an
  episode whose source domain includes the band;
- availability **never refuses activation** and never changes the processing
  class of §6.1 (still source-rate/layout/frame preserving, bounded causal,
  no mandatory pending output at EOF — the profile varies, the R0 contract
  does not);
- invalid band **data** (non-finite or out-of-bound trims, non-positive Q)
  remains an activation failure at every rate, on available and unavailable
  bands alike — intrinsic validity is rate-independent and is not rendered
  harmless by a band being unavailable;
- `Flat` (all bands neutral) remains observationally identical to bypass at
  **every** source rate;
- strictness at the Nyquist boundary is load-bearing: at exactly Nyquist
  the band's recursion degenerates, so the boundary band belongs to the
  unavailable side;
- honest exposure: product read models MUST be able to observe which bands
  are active for the current episode (the desired configuration still
  carries ten trims; the applied profile is the available subset). A
  presentation surface that shows a trim of an unavailable band as
  shaping sound would be lying; the observation plane / TUI work (#187,
  #188) consumes this distinction, it does not redefine it.

Alternatives evaluated and rejected: retaining the hard refusal (Policy A —
dishonest: a neutral configuration refused where bypass succeeds); a
rate-specific band table (Policy C — changes product identity: preset and
custom-config meaning would drift per rate). This decision is product
semantics under this document; it does not amend ADR-PBK-002 D14.11, whose
processing-minimum class is unchanged.

---

## 3. Typed product configuration and algorithm identity

DSP desired configuration is **typed product data** owned by the
application/product-control layer. Runtime representation is not part of the
product compatibility promise.

Each heterogeneous algorithm uses algorithm-specific typed configuration.
Qianqian MUST NOT collapse all future algorithms into a universal map of
string keys and floats merely to make them look generic.

A future algorithm may conceptually have:

```text
algorithm identity
algorithm-specific parameters and validation
neutral/default configuration
format constraints
processing contract
latency / tail / frame-mapping obligations
update-transition requirements
optional external resource references
```

Stable external identity/schema/revision is required **when** an algorithm or
configuration crosses a persistence, interchange or cross-version boundary.
The current private/in-memory runtime representation is not required to
materialize fields such as `algorithm_id`, schema version or behavior
revision merely because this model anticipates future persistence.

Therefore:

```text
product semantic identity != Rust type name
product semantic identity != registry ordinal
product semantic identity != K0 Plugin identity
product semantic identity != dynamic loading ABI
```

---

## 4. Product families vs processing contract

A product family answers **what the user is trying to do**. A processing
contract answers **what the algorithm does to time, frames, format, state and
lifecycle**. These are separate dimensions.

Non-authoritative examples of product families include:

```text
level / gain staging
tonal shaping
loudness / normalization
dynamics / nonlinear processing
spatial / headphone processing
correction / convolution
format adaptation
creative / specialized first-party algorithms
```

These examples are a taxonomy aid, not authorization to implement all listed
features. A specialized first-party algorithm does not have to pretend to be
EQ or compressor merely to fit the product model.

Graphic EQ and future Parametric EQ may share filter mathematics while
remaining different product configuration/editing contracts. Converting an
arbitrary PEQ into the fixed 10-band GEQ can be approximate; a UI MUST NOT
silently change sound merely by switching editor views.

---

## 5. Processing order and anchor rules

Processing order is explicit product semantics.

### 5.1 Current frozen order

The currently implemented template remains:

```text
Gain / Preamp
    ↓
10-band Graphic EQ
```

The runtime MUST NOT derive semantic order from:

```text
registration order
mount order
hash-map iteration
provider discovery order
enumeration order
```

### 5.2 Future ordering constraints

This document does not pre-create a seven-slot DSP pipeline. Future
capabilities add ordering constraints only when real product semantics require
them.

Examples of durable constraints:

- source normalization, if introduced, must explicitly define its relation to
  manual preamp and threshold-sensitive downstream processing;
- manual preamp/headroom remains PCM-domain product intent, distinct from
  D14.9 Output Volume;
- each tonal/creative/dynamics/spatial/correction algorithm declares the
  domains and neighbors with which its order is semantically valid;
- rate/layout/frame-changing adaptation is not an anonymous ordinary effect;
- final peak protection, if it claims to protect an application PCM domain,
  must follow every peak-producing transform covered by that guarantee and
  cannot be bypassed by arbitrary user reordering;
- Output stream level remains an Output concern and cannot prove that an
  upstream PCM overload did not occur.

### 5.3 User reorderability

Current product semantics do not expose user reordering.

Future constrained serial reordering MAY be earned for concrete combinations
that have clear user value and a validated legal-order model. Insert/delete/
reorder is a structural update, not a scalar parameter update.

The following remain unearned:

```text
fully free graph
feedback graph
branch/merge graph
arbitrary plugin patchbay
```

Parallel wet/dry or feedback processing requires separate evidence for
latency alignment, mix gain, state ownership, cut behavior and lifetime.

---

## 6. Processing regimes and authority pressure

Processing contracts are described with independent obligations. `R0` is
**not** a base bit that can be combined with contradictory obligations.

### 6.1 R0 — current-minimum-compatible result

An algorithm/configuration/whole chain is `R0` only after its **complete
contract** satisfies the D14.11 minimum:

```text
source-rate preserving
source-layout preserving
frame-count preserving
bounded causal processing
no mandatory pending output at EOF
```

`R0` is therefore a classification result after review, not a composable base
label.

### 6.2 Additional obligation axes

Future processing may introduce one or more independent obligations:

| Axis | Meaning | Examples of new obligations |
|---|---|---|
| `L` | explicit buffering / lookahead latency | priming, latency accounting, delayed presentation, seek discard |
| `T` | mandatory residual / drain after source EOF | bounded drain, tail cancellation, completion refinement |
| `M` | frame/time mapping changes | source-time vs render-time mapping, Position/accounting changes |
| `F` | rate/channel/layout format transform | input/output domain, channel identity, negotiation |

A lookahead/full-tail convolution has `L` and `T` obligations and therefore
is **not R0**. SRC has `M` and/or `F` obligations and may additionally carry
`L` or `T`; its exact contract decides.

Absence of an `L/T/M/F` label does not itself prove boundedness, causality,
cut correctness or R0 admission. The full algorithm and full serial-chain
contract must be checked.

Tail and latency composition are semantic operations, not simple label or
frame-count arithmetic. Upstream residual becomes downstream input, and
cross-rate latency must be compared in a common time domain.

---

## 7. Configuration states and update classes

These states are distinct:

```text
Editing draft
Desired configuration
Saved configuration        (only if persistence exists)
Applied episode configuration
```

A UI draft does not become desired until validated/committed. Desired does
not become applied merely because a control surface changed. Saved data, if
introduced, is not Playback Session state.

### 7.1 Current binding model

CURRENT authority remains D14.11, as narrowly extended by the 2026-10-02
live-admission amendment (§7.3):

```text
U0 = episode-fixed applied snapshot            (the established baseline)
U2/U3 = the four live-authorized operation     (narrowly earned, §7.3)
        classes of the current slice
```

A desired configuration still binds at episode establishment; live
application exists ONLY for the operation classes §7.3 authorizes, under
exactly the contract it freezes. Everything not named there remains
episode-fixed (U0) until separately earned.

### 7.2 Update-class vocabulary

| Class | Meaning |
|---|---|
| `U0` | episode-fixed; no live application |
| `U1` | live application where a discontinuity is explicitly acceptable |
| `U2` | live update with sample-driven ramp/smoothing |
| `U3` | live update requiring processor-state transition, reset, transform or crossfade |
| `U4` | semantics require whole-episode rebuild/replacement |

`U0` is mutually exclusive with a live application of the same operation.
`U2` and `U3` may both be obligations of one live transition. `U4` is not a
shortcut for every parameter change.

The live-authorized operations of §7.3 carry these classes: scalar
Preamp/Gain change = `U2` (the crossfade degenerates to the interpolated
gain); 10-band GEQ band-gain change and factory-preset switch = `U3`
(dual-processor crossfade, old state live, new state from rest); processing
enabled/bypass toggle = `U3` (processed/dry crossfade — bypass is NOT
`gain = 0`).

### 7.3 Live-update admission contract (LIVE_DSP_MINIMUM, accepted 2026-10-02, Issue #190 D3)

Differential, explicit: this subsection is the narrow live-admission
authority the D14.11 amendment records; it does not reopen any other OPEN
item. The transition evidence is the disposable probe through the REAL
Playback Session staging seam (real worker loop, partial PcmEdge writes,
seek/pause/terminal protocol).

**Live-authorized operations — exactly these four:**

```text
scalar Preamp/Gain change
10-band GEQ band-gain change
factory-preset switch (one desired configuration to another)
processing enabled/bypass toggle
```

Everything else stays OUT of live scope: PEQ topology changes, Q changes
(beyond what a preset/custom switch compiles as a whole), ReplayGain,
compressor, limiter, crossfeed, convolution/IR replacement, SRC, channel
remap, algorithm replacement, chain reordering, and any per-parameter
addressing beyond the whole-configuration update.

**Semantic states (semantic vocabulary first; public enum variants only if
an implementation needs them):**

```text
Desired         a pending update held by the product-control side
Accepted        validated and compiled against the episode format; the
                engine holds live-old and fresh-new processors and the
                applied-target identity
Applied         the update has reached the apply boundary below
Transitioning   old/new contributions coexist ONLY through the
                authorized bounded crossfade
Settled         only the accepted configuration contributes; the settled
                stream equals a fresh instance of the accepted
                configuration started at the transition start
```

**Frozen propositions:**

- *Coherent acceptance.* An update is accepted only as ONE whole valid
  configuration — validate + compile against the episode format BEFORE
  acceptance. A refusal (invalid data, compile failure) reports a
  diagnostic and leaves the old configuration running bit-exactly:
  never a processing failure, never a partial config, never a silent
  fallback to a different sound.
- *Apply boundary.* An accepted update takes effect at the next WHOLE
  staging block that has not yet been DSP-processed (the worker pickups
  run after any preserved remainder is flushed and with no seek past the
  serialization point — "in flight" means an actionable/accepted cut or
  a resolved refusal; a merely observed, not-yet-actionable seek command
  does not block the pickup, per the same D14.5
  production-continues principle). The pre-boundary stream stays
  bit-exactly the old configuration's continuation.
- *Processed-remainder rule (non-negotiable).* Already-processed
  remainder PCM is written exactly as processed; an update MUST NOT
  reprocess it, mutate it, or retroactively re-sound it. The transition
  starts only at the accepted new-block boundary.
- *Transition time domain.* Sample-driven, never wall-clock: the
  crossfade advances on processed frames only. The realized model is the
  dual-processor crossfade (Model C): the old side carries its live
  signal state, the new side starts from rest at the transition start,
  so the settled stream is EXACTLY a fresh instance of the accepted
  configuration started at the transition start. For a gain-only change
  between stateless configurations the transition stretch is exactly the
  interpolated gain on the same input; with a live EQ on either side the
  stretch is the same Model C blend of the two real processors (the
  new side's EQ starts from rest inside the crossfade) — the
  correctness-bearing exactness claim is the settled stream, and the
  transition-continuity oracles pin the blend law in both forms.
  Evaluated and
  rejected: instant switching (Model A — clicks at preset-scale jumps,
  pinned by a negative control), parameter smoothing (Model B — its
  intermediate parameter states correspond to no compiled
  configuration, so the settled stream can never equal a fresh instance
  of the accepted configuration and no exactness oracle exists),
  state transformation (Model D — no robust standard recipe for biquad
  state mapping between arbitrary configs). Transition durations
  are product tuning recorded with evidence, not authority.
- *Rapid updates.* Deterministic policy: complete-in-flight,
  latest-wins pending slot of depth one. An accepted transition always
  completes; the newest desired update replaces any pending one and is
  accepted at the first fresh-block pickup after settle. Intermediate
  desired states may legitimately never be applied.
- *Seek collisions (extends D14.5).* `RefusedUnchanged` preserves the
  in-flight transition and all signal state; the continuation equals
  the no-seek path bit-exactly. `Applied` discards the transition and
  invalidates ALL pre-cut signal-derived history (old side, new side,
  ramp) before post-cut PCM; the post-cut stream equals a fresh instance
  of the ACCEPTED configuration fed the exact post-landing tags.
  `MutatedThenFailed` follows the ordinary D11 failure path.
- *Pause (extends D14.7).* Pause gates rendering only; bounded prefetch
  may keep processing real future PCM (the ramp advances on those
  processed samples — honestly, since they are real processed frames).
  When the edge is full and the leg parked, no PCM is processed and the
  ramp does not advance. D14.8 Position never advances because DSP
  processed future PCM. Wall-clock time alone MUST NOT advance a
  transition (negative control pinned).
- *Failure.* Before acceptance: refusal (above). After acceptance, an
  unrecoverable processing failure during the transition settles through
  the existing D11 `Failed` class with a truthful processing-origin
  diagnostic; no partial-world resurrection.
- *Open/replacement.* A new episode compiles fresh under its own
  snapshot; no live state leaks across episodes.
- *Visibility.* Desired (pending), accepted (applied target) and applied
  (realized) remain distinct; a UI must never be forced to claim that a
  control change is already audible. The read model is D5's decision.
- *Realtime publication.* The old/new processor overlap here is
  same-thread, same-owner, episode-bounded state inside one worker —
  there is no cross-thread execution view to retire, so PBK-001 P1–P5
  are NOT triggered. This record does not authorize RCU, epochs, ArcSwap,
  generic snapshots or a parameter bus.
- *Landed representation (2026-10-02, Issue #190 D4 — a record, not new
  authority).* The production mechanism is exactly what this subsection
  freezes, no more: one mutex-guarded product-control cell per episode
  (desired whole configuration + depth-1 latest-wins pending slot + last
  refusal diagnostic as mechanism evidence) reached through the handle's
  four typed `set_*` commands (no generic parameter addressing), and one
  episode-owned dual-processor runtime behind the existing
  static-dispatch staging seam — the pickup reads the cell once per
  whole staging block, compiles the accepted configuration against the
  episode format there, and runs the Model C crossfade. Steady state
  (no pending update) adds one uncontended lock per block and nothing
  else; measured evidence lives in the D4 phase report. The transition
  length is product tuning recorded with evidence, not authority.
  Truthful stage vocabulary in this realization: a command's `Ok` is a
  coherent DESIRED update recorded after intrinsic validation; the
  semantic ACCEPTANCE is the pickup-time compile against the episode
  format; APPLY is the fresh staging block. The typed commands compose
  and commit under one lock hold (a stale-snapshot read-modify-write
  that could lose an unrelated concurrent field change is a named
  mutant, N9, with a pinned negative control), and activation reads the
  desired state through a bind that consumes the pending slot in the
  same lock hold, so a command issued between establishment and
  activation folds into the initial applied configuration instead of
  starting a phantom initial→same transition. The cell's lock is
  bounded on both sides — the worker takes one `Option` per fresh
  block; a command's critical section is one fixed-size
  compose+validate+commit over `Copy` data (only a refusal allocates
  its diagnostic, on the command path) — the bounded-blocking reading
  of the per-block firewall, which bans UNBOUNDED blocking; no
  condvar, no waiter, no I/O inside the lock.

---

## 8. Cut, pause, replacement and EOF interaction

This document consumes, rather than replaces, D14.5/D14.7/D14.11.

### 8.1 Cut rule

The durable DSP product rule is:

> Pre-cut signal-derived contribution must not contaminate committed
> post-cut presentation.

Therefore:

- `RefusedUnchanged` preserves applied config, already-processed remainder,
  signal history AND any in-flight live transition (§7.3); continuation
  remains equivalent to the no-seek control;
- `Applied` discards old processed remainder and invalidates pre-cut
  signal-derived state before post-cut PCM is processed;
- `MutatedThenFailed` does not reconstruct an old continuation and follows the
  existing terminal failure path;
- `Open/replacement` retires old episode signal state and binds fresh state for
  the new episode.

Signal-derived state includes filter recursion, envelopes, delay/lookahead
buffers, convolution residual and transition audio. Immutable coefficients,
user parameters or validated immutable assets are not signal history merely
because they are reused.

### 8.2 Pause rule — no false processing-clock freeze

Pause preserves signal continuity and MUST NOT reset processing history.

The current pause gate is on the render leg. Decode/DSP may continue bounded
prefetch of real future PCM while PcmEdge has capacity; therefore `Paused`
does **not** imply that PCM production or DSP processing immediately stops.
D14.8 Position still does not advance merely because future PCM was decoded or
processed.

A future live ramp/envelope MUST state whether its progression is based on
processed-sample time, presented-sample time or another accepted domain. When
no PCM is processed, wall-clock time alone MUST NOT be silently treated as
proof that a signal-domain DSP transition advanced.

### 8.3 Future EOF/drain hierarchy

Current R0 processing requires no processor drain protocol. If a `T`
obligation is later admitted, the model must distinguish:

```text
source EOF
processor input EOF
processor drain / residual production
processing output EOF
Output drain
D11 episode terminal settlement
```

A tail does not create a longer source Duration or authorize fabricated source
Position. Stop/replacement may cancel residual according to the separately
accepted contract.

---

## 9. Headroom, clipping and protection

CURRENT truth:

```text
internal Float32 may exceed nominal ±1
Gain/EQ do not implicitly clip
Gain/EQ do not implicitly limit
Output Volume is not PCM preamp
```

Lowering D14.9 Output Volume cannot recover information already lost to an
upstream nonlinear overload or prove that upstream PCM was safe.

Normative boundary:

- PCM Preamp/headroom and Output Volume remain separate product concepts;
- no hidden limiter, hidden soft clipper or implicit clipping is authorized;
- existing factory-preset unity preamp semantics cannot be silently changed;
- any automatic headroom or final-protection claim must name the PCM domain,
  rate/layout assumptions and guarantee it actually covers;
- source metadata peak values and frequency-response estimates are evidence
  for explicitly defined policies, not automatic proof of final true-peak
  safety after arbitrary processing/backend conversion.

Manual ranges, recommended preset attenuation values, automatic headroom,
optional/mandatory final protection and listening defaults remain product
policy to be decided with quantitative/native/listening evidence. They are not
frozen here — except for the single advisory slice §9.1 now accepts.

### 9.1 Estimated steady-state EQ headroom guidance (accepted 2026-10-02, Issue #190 D2)

Differential, explicit: this subsection supersedes §9's "not frozen here"
clause for exactly ONE slice — the non-binding estimated steady-state EQ
headroom advisory defined below. Automatic headroom, final protection,
preset default attenuation values and listening defaults remain unfrozen.
This subsection does NOT amend ADR-PBK-002 D14.11: the advisory is pure
analysis over desired configuration data, not a processor, not a
live-update right, and not a session-runtime change.

```text
HEADROOM_POLICY              manual preamp + truthful advisory metadata
HEADROOM_GUARANTEE_DOMAIN    the EQ cascade's steady-state
                             frequency-response gain, grid-sampled
                             (estimate), rate-aware (§2.1 active bands)
FINAL_LIMITER                NOT_EARNED (no limiter, no soft clipper,
                             no auto-headroom)
```

The advisory (`estimated_eq_headroom_guidance`) is a deterministic pure
function of the desired EQ data and the source rate (deterministic within
a process/platform; no cross-platform bit-determinism is claimed). Its
frozen semantics:

- it reports the attenuation (dB ≤ 0) that would place the available-band
  cascade's largest grid-sampled steady-state gain at unity, plus the peak
  location as presentation diagnostic;
- it is an ESTIMATE on a dense log-frequency grid — never a closed-form
  supremum and never a true-peak, arbitrary-signal or acoustic-loudness
  guarantee (inter-sample peaks, transients and signal level itself are
  outside its domain; the in-crate probe witnesses content that exceeds
  unity under the advice);
- the manual Preamp is NOT an input: the advice sits beside the user's own
  headroom control and never silently mutates the desired configuration;
- the D14.9 Output Volume structurally cannot enter the calculation;
- `None` refuses to advise rather than fabricating: a zero rate, or data
  outside the product's intrinsic validity — the same domain establishment
  validation refuses (invalid trims or Q), finite or not. Garbage is never
  answered with a plausible "no attenuation advised";
- desired configuration, applied configuration and advice remain three
  distinct product concepts; applying the advice is always an explicit
  product/user act on the preamp.

---

## 10. Persistence and compatibility — conditional authority

There is currently no durable DSP config interchange format. When one is
introduced, it MUST distinguish compatibility concerns instead of binding them
to the player binary version:

```text
container/schema version
algorithm parameter schema
algorithm behavior revision
factory preset identity/revision when externally referenced
asset content identity/format when external assets exist
```

Rules for any future persisted/interchanged active DSP data:

- old configurations are not silently reinterpreted;
- unknown **active execution semantics** are not silently skipped while the UI
  claims the configuration was applied;
- unknown disabled/non-executing extensions may only be preserved/ignored when
  their semantics are understood well enough to do so safely;
- invalid/missing assets or unsupported format constraints preserve user data
  but fail activation honestly rather than silently falling back to a
  different sound;
- runtime signal history, queues, episode handles and filter state are never
  serialized as product desired configuration;
- migration must be deterministic and must not pretend a changed algorithm
  behavior is the old revision.

Factory preset identity/revision becomes an external compatibility obligation
only when preset identity crosses save/import/interchange/cross-version
boundaries. The current in-memory `EqPreset` does not need an artificial
revision field merely because persistence may exist later.

Whether vNext ships user preset save/load, cross-startup restore or any other
persistence feature is OPEN and belongs to the accepted release slice.

---

## 11. Presentation capability boundary

A presentation client may expose only the subset it can edit truthfully.

Capability must not collapse into one boolean. At minimum consumers may need
to distinguish:

```text
known to product model
supported by this engine build
authorized by current architecture
available for this source/render format
editable by this presentation surface
live-capable under the accepted update contract
```

A simpler client MUST NOT erase hidden advanced configuration merely because
it does not understand those fields. It must either preserve them losslessly,
present the configuration read-only, or perform an explicit user-visible
conversion into a supported subset.

This rule does not require TUI/GUI feature parity and does not imply a K0
Capability type.

---

## 12. Downstream PCM consumer boundary

The selected processing order produces **Qianqian application processing
output PCM** in the processing domain actually established for the episode.
This document defines that output's DSP/time/format contract; Playback Session,
PcmEdge and Output ownership remain governed by the existing ADRs.

Downstream consumers split into two fundamentally different categories:

```text
playback main path
    PcmEdge -> Output

readonly observation path
    #187 Audio Observation Plane
```

The Observation Plane contract lives in
[`audio-observation-plane.md`](audio-observation-plane.md). It owns any
accepted readonly sampling/tap contract, bounded-loss behavior,
analysis/snapshot semantics, cut/reset invalidation and observation time/format
meaning.

Consuming post-processing PCM does **not** grant an observer authority over:

```text
playback order
seek success
D14.8 Position
D11 terminal outcome
Output-device consumed truth
physical/acoustic output truth
```

The current `post-DSP / pre-PcmEdge` observation point remains a #187
**candidate until that document/issue accepts it**. This DSP authority does
not silently freeze a tap location.

Do not generalize #187's lossy observer contract to all possible future PCM
consumers. Output is the main path; lossless recording, feedback control or a
new PCM-transforming consumer would require its own contract and cannot be
smuggled in as “another observer.”

---

## 13. Release/versioning boundary

DSP currently ships as part of the Qianqian player release. There is no
independent DSP release train.

A standalone `qianqian-dsp` component/release is reconsidered only when real
evidence creates an independently consumable API/ABI/artifact with external
consumers and a separately maintained compatibility/support contract.

The following alone do not earn an independent release train:

```text
a separate crate
many algorithms
large DSP code size
shared use by TUI and GUI
future configuration persistence
```

A DSP config schema version, if persistence exists, is a data-compatibility
version and is not the Qianqian player semver.

---

## 14. Authority-reopen guide

This table is routing guidance, not blanket implementation authorization.
“D14.11 minimum sufficient” means the current transport/time/format class can
host the feature after its own product/algorithm contract is reviewed.

| Change | Current D14.11 minimum sufficient? | Required action |
|---|---:|---|
| Current fixed 10-band custom EQ | Yes | product/control exposure only; live still separate |
| Current scalar preamp | Yes | product/headroom semantics only; keep Volume separate |
| Live preamp / live EQ / live preset | No | narrow live-update authority |
| ReplayGain fixed-gain realization | Potentially R0 | define metadata seam and product policy first |
| Causal compressor without new latency/tail | Potentially R0 | define family/state/headroom contract |
| Lookahead limiter | No | `L` + Position/EOF/accounting review |
| Layout-preserving bounded crossfeed | Potentially R0 | channel/state contract; actual latency/tail decides |
| Full-tail convolution | No | `T`, often `L`; drain/completion review |
| AutoEQ-style IIR/PEQ | Potentially R0 | new typed tonal/profile contract |
| FIR correction | Depends | actual `L/T` behavior decides |
| SRC / time remapping | No | `M` and usually format/output authority |
| Downmix/upmix/remap | No | `F` + PCM/output format authority |
| First-party specialized algorithm | Depends | review its complete contract; “custom” grants nothing |
| User-reorderable serial body | Not by default | legal-order/product contract; live reorder separately |
| Third-party executable DSP | No | ABI/isolation/resource/failure/loading review; D13 separately |
| Hardware/platform DSP | Depends | placement/refinement review; no automatic ownership/Plugin change |

---

## 15. Runtime representation deliberately left OPEN

This product model does not earn:

```text
public AudioProcessor trait/API
ProcessorRegistry
generic DSP graph
Plugin-per-effect
AudioProcessingCapability
dynamic native module loading
generic parameter bus
schema-driven execution runtime
RCU / epoch / ArcSwap live publication
dedicated DSP worker
render-thread DSP
```

Private structs/enums/traits may evolve when an actual implementation needs
them. A durable public/shared mechanism must be justified by concrete
consumers, duplication, lifetime/resource pressure or a correctness problem
that the current local representation cannot solve.

“OPEN” is not coding-agent permission to invent new semantics. New timing,
format, ownership, lifecycle or generic runtime behavior returns to authority
review.

---

## 16. Non-authoritative future map

The detailed family survey, UI capability matrix and long-term roadmap that
motivated this model are design evidence, not implementation authorization.

In particular this authority does not decide:

```text
whether vNext includes live Gain/EQ
whether vNext includes save/load persistence
exact headroom recommendation numbers
ReplayGain fallback/default policy
future PEQ UI details
future limiter default policy
future rich-UI algorithm coverage
roadmap phase numbering
```

Those are selected by later product/release/algorithm decisions while obeying
the stable model above.
