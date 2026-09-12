# CONTEXT.md

This file carries stable vocabulary and the current repository mental model. It is not a substitute for current code, contracts, ADRs, or task-specific evidence.

Playback Foundations are **ACCEPTED** (`ADR-PBK-001`). Stable vocabulary still describes the new foundations, not legacy playback nouns that happen to remain in code.

---

# Current vocabulary

> **Status: Playback Foundations are ACCEPTED (`ADR-PBK-001`); legacy playback nouns remain experimental evidence.** Normative definitions live in `docs/adr/ADR-PBK-001.md` (constitution §1–§2; role/vocabulary definitions §16); this table is a short-status index, not a second authority.

| Term | Meaning |
|---|---|
| Qianqian / 千千·现代 | A local-first, lightweight, cross-platform music player and a testbed for composable runtime architecture. |
| Architecture v2 | Boundary-first architecture built on a generic Rust Composition Kernel plus domain-specific control/fact/data-plane runtimes. |
| Boundary-first design | Decide granularity, capability dependency, execution semantics, fact authority, lifecycle ordering and realtime boundary before APIs/representation. |
| Composition Kernel | Generic domain-agnostic control/lifecycle kernel. It knows Context, Capability, Fiber, Effect and Reconcile, not PCM/music/UI/platform semantics. |
| Context | Capability namespace/dependency view visible to a Fiber. It controls reachability/availability; it is not a payload bus, event store or global state bag. |
| Capability | A named/typed service contract that can be required/provided. Capability identity is distinct from concrete provider implementation. |
| Service Definition | Domain-facing execution contract represented by a Capability. Consumers depend on the definition across Plugin seams. |
| Provider | A Plugin/Fiber that makes a Capability/Service available while it satisfies the lifecycle contract. |
| Consumer | A Plugin/Fiber that requires a Capability/Service. |
| Plugin | Long-lived component definition participating in the common composition/lifecycle protocol after its boundary has been justified. It is not a data packet or pipeline hop by definition. |
| Fiber | Live Plugin instance with identity, scope, requirements, provided Capabilities, Effects and lifecycle state. Runtime composition unit, not crate/package. |
| Effect | Kernel-visible mutation/resource provenance owned by a Fiber, with teardown/inverse where reversal is actually valid. |
| Reconcile | Moves the running Fiber graph toward desired composition while respecting dependency/lifecycle invariants. |
| Profile | Desired product/platform composition. |
| Composition Plane | Who exists, who requires/provides what, who owns Effects, and how providers/dependents enter/withdraw. |
| Execution / Control Plane | Commands, workflows and Capability/Service calls that ask the system to perform work or mutate authoritative state. |
| Command | Intent/request to do something. A Command is not proof that the requested outcome happened. |
| Fact | A typed observation/truth established by its designated semantic authority (normative contract: ADR-PBK-001 §2.3). |
| Fact Event | Publication of a committed Fact to observers. Fan-out semantics do not make the Fact a mutable middleware object. |
| Fact Plane | Committed facts plus their observation, projection, persistence and presentation paths. |
| Projection | Derived read model/materialized view built from committed facts and/or authoritative snapshots. Projection is not an authority/writer. |
| Commit-first publication | Semantic commit occurs before Fact publication (ADR-PBK-001 §2.3). |
| Mechanism Evidence | Observation produced by a mechanism/provider that may feed a later semantic decision. Evidence != Fact unless the producer is that fact identity's designated authority (ADR-PBK-001 §16.4). |
| Runtime | Abstract category: an active mechanism/system owning continuously-running state, execution rules or lifecycle authority — not "any crate named runtime"; always use with a qualifier (ADR-PBK-001 §16.2). |
| Host | Architecture role that selects/installs desired components, creates the Base Kernel, drives composition lifecycle and owns application bootstrap/shutdown initiation/orchestration. Initiating/orchestrating shutdown does not freeze a cross-domain shutdown protocol (ADR-PBK-001 §17). Does not automatically own playback/realtime authority. Architecture role != crate name. |
| Realtime Runtime | Specialised runtime responsibility for realtime execution-view legality and realtime-visible lifetime safety (earned per ADR §6 P1–P5 evidence). Not a crate name; does not freeze a mechanism representation. |
| Realtime mechanism | Concrete mechanism realizing a Realtime Runtime invariant (refcount ledger / epoch / hazard pointer / …); representation, not responsibility. |
| Realtime Execution View | Coherent pre-bound execution state published by the control side for direct realtime consumption. Execution View != Projection. |
| Realtime Data Plane | High-frequency bounded data flow such as PCM, executed through pre-bound realtime-safe graph edges rather than generic Context/Event dispatch. |
| PCM | Canonical family of decoded audio payloads for realtime processing. Exact Qianqian `PcmBlock` representation is currently unfrozen. |
| Realtime Graph/View | Pre-built/pre-bound processing view consumed directly by the realtime path. Exact representation is unfrozen. (Same artifact family as the Realtime Execution View, ADR-PBK-001 §16.2.) |
| Graph Publication | Control-side act of validating/building a new realtime graph/view and making it visible at an RT-safe boundary. |
| Reader Quiescence | State where no active or queued realtime reader can still dereference the relevant resource through any still-reachable generation (ADR-PBK-001 §6 P3 summary). |
| Reclamation chain | Retirement (closed to new acquisition) != Reclaimable (quiescence certified) != Released (physically disposed); publication is not a reclamation certificate (ADR-PBK-001 §6 P2–P4, §16.5). |
| Capability plane != Data plane | Context/Capability establishes reachability and execution contracts; hot payloads normally flow through already-bound service/data edges. |
| Command != Fact | Intent and committed truth use separate semantics; one generic `emit()` must not blur them. |
| Fact != Hot Data | Meaningful committed observations may fan out; PCM blocks stay on the realtime data plane. |
| Projection != Authority | Read models can be globally visible but cannot write the truth they summarize. |
| Dependency topology | Composition relationship: requirements/providers/lifecycle ordering. |
| Realtime processing topology | Execution/data relationship: order/branch/merge of realtime processing nodes. It is not inferred from Fiber mount or Capability resolution order. |
| Component Granularity | Decision of what deserves to be one Plugin/component; feature names and source-file boundaries do not decide it. |
| Operation Locality | An operation on one capability/key should not secretly mutate unrelated hidden/global state. |
| Independence | Removing one component preserves other independent components' observable contributions. Revertibility alone is insufficient. |
| Commutativity | Relevant shared operations/inverses can be reordered without changing observable behavior. Order-sensitive relations must be explicit. |
| Interaction Algebra | Classification of independently composable vs order-sensitive interactions and the structure needed for each. |
| Recoverable System Boundary | State/resources the runtime can legitimately own/restore/compensate; physical emissions outside it may be irreversible. |
| Observational Equivalence | Correctness through public/relevant behavior rather than bit-identical private state. |
| Confluence | After legal composition changes settle, observable composition truth matches a clean construction of the same final desired composition. |
| Quiescence | Relevant lifecycle/reconciliation/realtime-reader transitions have settled enough for safe release/comparison. |
| Everything is a Plugin | Justified long-lived runtime capabilities obey a common composition/lifecycle protocol; it does not mean every feature, AudioNode, payload or buffer is a Plugin. |
| Event Service | If Qianqian needs generic domain Fact/Event semantics, start as an ordinary capability/service/plugin until evidence earns a kernel primitive. |
| Formal exploration | Risk-driven state-space evidence for concrete state/interleaving collisions. It is not a second architecture authority. |
| Experimental playback evidence | Existing old playback Rust/tests/TLA retained to reuse bug reproducers and techniques, not to dictate new architecture vocabulary. |
| Reference Playback v1 | Historical playback experiment preserved as git ref `playback-reference-v1`; opt-in evidence only. |
| Pre-Rust archive | Repository state before Architecture v2 reset, preserved as git ref `pre-rust-v2`; opt-in evidence only. |

---

# Core mental model

The accepted foundations separate four reasoning lenses (not four mandatory runtime subsystems):

```text
Composition Plane
    Context / Capability / Fiber / Effect / Reconcile
    -> who exists, depends, withdraws

Execution / Control Plane
    Command / workflow / Capability-Service call
    -> asks the system to do work

Fact Plane
    semantic commit -> Fact -> observers/projections/persistence/UI
    -> describes what has happened

Realtime Data Plane
    pre-bound graph/view -> PCM -> device
    -> moves hot data under timing constraints
```

Normative definitions live in `ADR-PBK-001.md` (constitution and four lenses §1–§2; publication/reclamation contract P1–P5 §6; research ladder §12; formalization policy §13). This file is a vocabulary/status index only, not a second authority.

---

# Base Kernel mental model

K0 remains implemented/current. (K0 semantic authority: `docs/architecture/composition-kernel-0-design.md`; representation decisions: `docs/architecture/composition-kernel-0-implementation-adr.md`.)

```text
Context
Capability
Fiber
Effect
Reconcile
```

The kernel decides reachability/lifecycle and must remain ignorant of:

```text
PCM
AudioGraph
FFmpeg
WASAPI
seek
track/playlist meaning
UI product state
```

K0 does not gain Event/Session/Audio primitives merely because an external system uses them.

---

# Plugin mental model

Plugin is not the data flow itself.

A Plugin may register/provide:

```text
Capability/Service
control hook
Fact observer
resource/factory
realtime graph participant
```

But payloads do not automatically flow as:

```text
Plugin A -> Plugin B -> Plugin C
```

Whether an audio processing stage deserves its own Plugin identity is a granularity/lifecycle decision, not a consequence of being on the PCM path.

---

# Execution / Fact / Realtime mental models

These are one-line summaries; the normative contracts live in `ADR-PBK-001.md`:

```text
Execution / Control   Command is intent, not fact                -> ADR §2.2
Fact                  semantic commit -> Fact publication;        -> ADR §2.3
                      one designated authority per
                      (fact kind, subject scope);
                      projection is read-only visibility;
                      Event Sourcing/durability/replay remain OPEN
Realtime Data         PCM flows through pre-bound realtime        -> ADR §2.4, §6
                      execution state; publication/reclamation
                      follows the normative P1–P5 contract
                      (this line is a summary; ADR §6 is the
                      authority)
```

---

# Playback reset mental model

There is currently **no accepted playback state-machine vocabulary** beyond the foundational plane separation in `ADR-PBK-001`.

These legacy implementation names are explicitly unfrozen:

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Active / Prepared
Generation
Dual Window
Physical Fence
```

They may still appear in Rust/tests/TLA. Treat them as experimental evidence only.

Do not infer current design from them.

---

# Current research order

The one normative research/implementation ladder lives in `ADR-PBK-001.md` §12 (composition reality → minimal PCM contract → direct data flow → publication/reader overlap → real decoder → real output → only then playback semantics). Other documents link it; they do not carry a second copy.

---

# Formalization mental model

Ask first:

> **Which independently legal states/events can interleave and collide into an illegal state?**

No concrete collision -> prefer types/ownership/tests/static checks.

The old PlaybackTemporal/PlaybackOwnership models are not current architecture acceptance gates.

The first post-reset formal target was realtime graph publication vs reader quiescence / resource release — delivered. `specs/realtime-publication/` proved the collision exhaustively at model level (TLC + negative controls); its semantic conclusions are frozen normatively as P1–P5 in `ADR-PBK-001.md` §6; the implementation mechanism remains DEFERRED; §12 Phase D validates candidate mechanisms against P1–P5 (it does not re-litigate whether P1–P5 are required). Evidence and mechanism comparison: `docs/architecture/realtime-publication-lifetime-decision.md`.

---

# Current code status

Workspace currently includes:

```text
qianqian-composition
qianqian-audio-api
qianqian-app
qianqian-playback
qianqian-headless
```

The Composition Kernel (K0) is current. Current canonical vocabulary: ADR-PBK-002.

Playback-specific code from the prior design (`MusicKernel` / `TransportKernel`) no longer lives in production `src/`; it survives as test-local executable evidence in `qianqian-audio-api` `tests/playback_temporal_traces/` and may be changed or removed by later research without compatibility obligation.

Bootstrap APIs are not compatibility contracts unless a later accepted authority explicitly says so.

---

# Historical refs

Historical evidence is preserved as Git refs:

```text
pre-rust-v2
playback-reference-v1
```

Use only when the current task explicitly needs historical evidence.
