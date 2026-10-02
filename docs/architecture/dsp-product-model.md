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
> D14.11 remains the authority for the currently authorized processing
> minimum, episode ownership, current decode-worker placement, and the fact
> that live parameter update is still OPEN.

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

CURRENT authority remains D14.11:

```text
U0 = episode-fixed applied snapshot
```

Desired changes bind on a later episode establishment under the current
minimum. This is a statement about **current** semantics, not a promise that
vNext must remain non-live.

### 7.2 Future update requirements

If live processing is entered later, the operation must earn the required
transition semantics. Useful vocabulary:

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

Live update remains OPEN. Neither this document nor #188 grants live rights.
If a future product slice enters live Gain/EQ, it must independently define:

```text
coherent prepare/validation
atomic acceptance of one executable config
apply boundary
sample/presentation-time basis for smoothing
state preserve/transform/reset/crossfade
seek/stop/replacement collision
accepted vs applied vs audible visibility
failure behavior
resource retirement where real overlap exists
```

PBK-001 P1–P5 apply only when an actual old/new execution-view lifetime
overlap exists; this document does not pre-authorize RCU, epochs, ArcSwap,
generic snapshots or a parameter bus.

---

## 8. Cut, pause, replacement and EOF interaction

This document consumes, rather than replaces, D14.5/D14.7/D14.11.

### 8.1 Cut rule

The durable DSP product rule is:

> Pre-cut signal-derived contribution must not contaminate committed
> post-cut presentation.

Therefore:

- `RefusedUnchanged` preserves applied config, already-processed remainder and
  signal history; continuation remains equivalent to the no-seek control;
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
frozen here.

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
