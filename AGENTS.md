# AGENTS.md

Qianqian is a local-first, lightweight, cross-platform music player and an architecture experiment in composable runtime design. This file defines repository-wide rules for coding agents. It is governance, not a feature manual.

## Start here

Before changing code or long-lived documentation:

1. Read the current issue/task.
2. Read `CONTEXT.md` for stable vocabulary.
3. Use `docs/README.md` to load only the minimum relevant authority.
4. Read `docs/architecture/overview.md` before changing architecture boundaries.
5. For generic composition work, read `docs/architecture/composition-kernel.md` and K0 design/implementation authority.
6. For playback/audio work, read `docs/adr/ADR-PBK-001.md` first. It is currently **PROPOSED / REOPENED**, not an accepted production implementation contract.
7. Inspect current repository reality before assuming a path, type, crate, test, TLA variable, or prior design is still authoritative.

Do not recursively preload historical refs or external failure evidence.

---

# Architecture reset rule

Playback architecture has been reopened from first principles.

Current code still contains names such as:

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Generation
Active / Prepared
Physical Fence
```

These are currently:

```text
EXPERIMENTAL / EXECUTABLE EVIDENCE
NOT STABLE VOCABULARY
NOT ARCHITECTURE AUTHORITY
NOT COMPATIBILITY CONTRACT
```

Do not preserve or extend them merely because they already exist.

If a new experiment independently earns one of these concepts, it may be reintroduced with the same or a different name.

---

# Architecture constitution

Freeze these repository-wide principles:

> **Base Kernel is domain-agnostic.**
>
> **Plugin/Fiber identity belongs to composition/lifecycle, not per-payload routing.**
>
> **Commands/capability calls ask the system to do something; facts describe what has already been committed.**
>
> **Committed facts may fan out; hot realtime payloads do not use generic fact/event dispatch.**
>
> **Realtime PCM flows through pre-bound typed data edges.**
>
> **Projection/read models are derived visibility, not writers of authoritative truth.**

The shortest mental model is:

```text
Composition Plane
    who exists / depends / withdraws

Execution / Control Plane
    commands / capability calls / workflows

Fact Plane
    committed facts / projections / persistence / observation

Realtime Data Plane
    PCM / callbacks / pre-bound processing graph
```

Do not collapse these into one generic `emit()`, one mutable state bag, or one service locator.

---

# Base Kernel K0

The generic Composition Kernel is already implemented and remains current.

Its primitive budget stays centered on:

```text
Context
Capability
Fiber
Effect
Reconcile
```

Do not add a generic primitive without a concrete architecture issue showing that K0 cannot express a required invariant cleanly.

The generic kernel must not know:

```text
PCM
AudioGraph
FFmpeg
WASAPI
track/playlist semantics
seek
playback cursor
platform UI payloads
```

An Event/Fact system is **not automatically a new K0 primitive**. Start it as a normal capability/service/plugin unless evidence demonstrates that it belongs in the kernel.

---

# Context is not a data bus

`Context` is a capability namespace/dependency view.

It must not become:

```text
global product state
universal event bus
message broker
PCM/audio transport
UI payload store
get-anything service locator
```

Once a capability is resolved/bound, ordinary execution happens through that service contract or a pre-bound data edge.

> **Capability plane != payload transport.**

---

# Plugin / Fiber discipline

A Plugin is a long-lived capability/lifecycle participant whose boundary has been justified.

A Fiber is a live Plugin instance and owns its composition-lifecycle identity and Effects.

“Everything is a Plugin” means:

> **Justified long-lived runtime capabilities enter the common composition/lifecycle protocol.**

It does **not** mean:

```text
one feature name = one plugin
one source file = one plugin
one DSP function = one plugin
one PCM block = one plugin
every AudioNode = automatically one Fiber
every Plugin = one dynamic library
```

For every proposed Plugin boundary, answer:

```text
What lifetime/resource does it own?
What capability does it provide?
What capability does it require?
Does it need independent replacement/withdrawal?
What execution/data edges cross the boundary?
What state is intentionally public?
What is the configuration/cognitive cost of splitting it?
```

---

# Execution / Control Plane

A Command expresses intent.

Examples of future shapes may include:

```text
open media
change volume
insert processing stage
select output
start/stop execution
```

A command may travel through domain logic, workflow, middleware/interception, and capability/service calls.

Do not confuse this with committed fact delivery.

> **Command != Fact.**

If a control hook can modify/reject/wrap an operation, treat it as execution/control semantics, not as a post-commit event observer.

Do not invent a generic waterfall/middleware primitive until a real extension seam requires one.

---

# Fact Plane

A Fact describes something that has already been committed by the relevant authority/mechanism.

Minimum ordering:

```text
validate / decide
      ↓
commit authoritative truth
      ↓
publish fact
      ↓
observers / projections / persistence / UI / telemetry
```

Freeze:

> **commit first -> publish fact**

Observers of one committed fact may react, but they must not rewrite whether that fact happened.

If a reaction needs to change the system, it issues a new command or commits a new fact.

### Projection

Projection is a derived read model.

```text
previous view + committed fact -> next view
```

A projection is not a semantic authority and must not become a write-back shortcut into runtime truth.

### Event sourcing is not yet frozen

Do not assume Qianqian must use one durable append-only event store just because another system does.

Still open:

```text
state + events vs event sourcing
which facts are durable
replay authority
snapshot strategy
memory commit vs disk durability
```

---

# Realtime Data Plane

PCM is hot data, not a generic Event.

Per block/callback, do not perform:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic EventBus fan-out
plugin registry traversal
filesystem/network I/O
UI/JS/managed-runtime round trip
unbounded allocation/blocking
```

Target shape:

```text
control side
    build + validate next realtime graph/view
    publish

realtime side
    load current pre-bound graph/view
    process PCM directly
```

The realtime graph may use objects/resources provided by Plugins, but the callback is not a Plugin dispatch loop.

---

# Graph publication / lifetime safety

Any object referenced by a published realtime graph/view must remain alive until all readers that can still dereference it have quiesced.

Required semantic order:

```text
withdrawal/replacement requested
        ↓
exclude from future graph construction
        ↓
publish replacement graph/view
        ↓
stop new readers entering old view
        ↓
wait old readers/queued references quiesce
        ↓
release old view references
        ↓
provider/resource final release
```

Do not destroy a provider first and hope the realtime side notices later.

Concrete mechanism is open:

```text
RCU / epoch / snapshot / Arc / lease / double buffer / other
```

---

# Parameter update != composition mutation

Cheap runtime parameter changes such as volume or filter coefficients should not automatically unload/reload Plugins or run full Reconcile.

Likewise, topology/provider replacement may require graph rebuild/publication.

The exact split is earned by Audio Runtime experiments.

Do not make heavyweight composition machinery mandatory for every realtime-safe parameter update.

---

# Boundary-first design

Do not begin by inventing APIs.

Required order remains:

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Interaction / execution semantics
        ↓
Fact / state authority
        ↓
Lifetime / withdrawal ordering
        ↓
Realtime boundary where relevant
        ↓
Executable evidence
        ↓
API / representation
```

A different Rust type, file, crate, feature name, or test helper is not proof that a new Plugin or authority is needed.

---

# Current playback research order

The reset deliberately postpones player state-machine design.

Preferred order:

```text
A. K0 composition reality
B. minimal PCM contract
C. direct-flow Source -> processing -> Sink experiment
D. graph publication / replacement / reader overlap
E. real decoder experiment
F. real output experiment
G. only then re-earn seek/stop/track/session semantics
```

Do not start a new playback task by reusing the old `MusicKernel/TransportKernel` model as a mandatory scaffold.

---

# Existing playback code/specs

The following remain useful as experimental evidence:

```text
qianqian-core::music
qianqian-core::transport
playback_temporal_traces
specs/playback/*
```

Allowed use:

```text
reuse bug reproducers
reuse testing/formal techniques
compare whether new mechanisms rediscover an old failure
```

Forbidden use:

```text
claim current authority from old type names
preserve old API for compatibility without explicit requirement
force production state to mirror old TLA variables
```

Formal models are evidence under explicit assumptions, never a second architecture authority.

---

# Formalization policy

Formalization is risk-driven.

Ask:

> **Which independently legal states/events can interleave and collide into an illegal state?**

If there is no concrete collision, prefer types, ownership, unit/property tests, static checks, or executable stress tests.

The old playback formal core is no longer a blocking acceptance gate for the reset architecture.

The first likely new formal target is graph-publication lifetime overlap **only if** executable work demonstrates a real collision risk.

Do not build a full model of the entire player or DSP graph.

---

# Global state discipline

Do not introduce:

```text
GlobalPlayerState
Arc<Mutex<Everything>>
MutableAppState
```

as a common writer for Composition, Control, Facts and realtime PCM.

Every durable semantic fact needs an explicit writer/authority, even if many observers and projections can see it.

The names of playback authorities are currently unfrozen.

---

# UI boundary

UI is not playback authority and never participates in realtime correctness.

UiHost remains an ordinary capability/plugin candidate.

Platform intent remains replaceable and should not leak into Base Kernel semantics.

---

# External evidence isolation

External failure mining lives under `evidence/` and is opt-in.

Do not load it during ordinary architecture/ADR/implementation review unless the task explicitly asks for external failure evidence or adversarial inspiration.

When external systems inspire a design distinction, re-derive and state the Qianqian invariant locally. Do not turn the external project's implementation into our authority.

---

# Review discipline

Fresh-context reviewers for new playback work should prioritize:

```text
plane confusion
Context/event/PCM misuse
hidden global authority
command/fact confusion
projection becoming writer
provider release before realtime readers quiesce
plugin boundary over-fragmentation
accidental preservation of old playback assumptions
```

Do not reject a new design merely because it differs from old PlaybackTemporal or old core types.

---

# Verification

Report what was actually verified.

Never mark an unrun device/platform/audio check PASS.

For architecture reset work, green Cargo tests are regression evidence, not architecture acceptance.

For realtime work, distinguish at least:

```text
control-side correctness
realtime hot-path constraints
graph publication visibility
resource lifetime under reader overlap
actual device/mechanism evidence
```

---

# Documentation

`docs/README.md` is the documentation router.

Keep one current authority per durable fact. Git history stores the old architecture; do not grow amendment/supersession chains in the working tree when a clean rewrite is possible.

Playback authority is currently reopened. `docs/adr/ADR-PBK-001.md` is PROPOSED until its reset gates pass.

---

# Delivery discipline

- inspect before assuming;
- keep changes narrow to the current gate;
- distinguish evidence from authority;
- do not perform unrelated cleanup;
- do not silently preserve stale architecture for compatibility;
- stop after opening the requested PR unless explicitly authorized to merge.
