# CONTEXT.md

This file carries stable vocabulary and the current repository mental model. It is not a substitute for current code, contracts, ADRs, or task-specific evidence.

Playback architecture is currently **reopened**. Stable vocabulary must therefore describe the new foundations, not legacy playback nouns that happen to remain in code.

---

# Stable vocabulary

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
| Fact | A typed observation/truth that has already been committed by its owning authority/mechanism. |
| Fact Event | Publication of a committed Fact to observers. Fan-out semantics do not make the Fact a mutable middleware object. |
| Fact Plane | Committed facts plus their observation, projection, persistence and presentation paths. |
| Projection | Derived read model/materialized view built from committed facts and/or authoritative snapshots. Projection is not an authority/writer. |
| Commit-first publication | Authoritative commit occurs before publishing the corresponding Fact; observer failure does not retroactively make the committed fact unhappen. |
| Realtime Data Plane | High-frequency bounded data flow such as PCM, executed through pre-bound realtime-safe graph edges rather than generic Context/Event dispatch. |
| PCM | Canonical family of decoded audio payloads for realtime processing. Exact Qianqian `PcmBlock` representation is currently unfrozen. |
| Realtime Graph/View | Pre-built/pre-bound processing view consumed directly by the realtime path. Exact representation is unfrozen. |
| Graph Publication | Control-side act of validating/building a new realtime graph/view and making it visible at an RT-safe boundary. |
| Reader Quiescence | State where no old realtime reader/queued reference can still dereference an old published graph/resource. |
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

The current constitution is:

```text
Composition Plane
    Context / Capability / Fiber / Effect / Reconcile
    -> who exists, depends, withdraws

Execution / Control Plane
    Command / workflow / Capability-Service call
    -> asks the system to do work

Fact Plane
    authoritative commit -> Fact -> observers/projections/persistence/UI
    -> describes what has happened

Realtime Data Plane
    pre-bound graph/view -> PCM -> PCM -> device
    -> moves hot data under timing constraints
```

The four planes cooperate but are not one mechanism.

---

# Base Kernel mental model

K0 remains implemented/current.

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

# Execution mental model

```text
Command
   ↓
domain/controller/workflow
   ↓
Capability / Service
   ↓
mechanism / authority
```

Execution may eventually need middleware/waterfall-like interception, but that is currently an unfrozen extension mechanism.

A hook that can modify/reject/wrap execution belongs to control semantics, not post-commit Fact observation.

---

# Fact mental model

Minimum ordering:

```text
validate / decide
      ↓
commit truth
      ↓
publish Fact
      ↓
fan-out
  ├─ projection
  ├─ persistence
  ├─ UI
  ├─ telemetry
  └─ reactions/new commands
```

The same committed Fact is observed; it is not passed through a mutable plugin pipeline.

Projection folds facts into useful read state but is not a writer.

Qianqian has **not** yet chosen complete Event Sourcing/CQRS as a repository-wide architecture.

Open questions include:

```text
which facts are durable
append-only log or not
replay authority
snapshotting
memory commit vs disk durability
```

---

# Realtime mental model

PCM is not an Event.

```text
CONTROL SIDE

config/composition/parameters
        ↓
build + validate realtime graph/view
        ↓
publish

REALTIME SIDE

load current view
        ↓
process hot PCM directly
        ↓
device/output
```

Per block/callback:

```text
no Context lookup
no Capability resolution
no Reconcile
no generic EventBus fan-out
no plugin registry traversal
no filesystem/network/UI
no unbounded allocation/blocking
```

---

# Graph lifetime mental model

Published realtime references cannot outlive their backing resources incorrectly.

```text
withdraw/replace A
      ↓
A excluded from future graph build
      ↓
publish graph without A
      ↓
old readers stop entering old graph
      ↓
old readers/queued refs quiesce
      ↓
release old graph refs
      ↓
final release A
```

Exact mechanism (`RCU`, epoch, snapshot, Arc, lease, double-buffer, etc.) is intentionally unfrozen.

---

# Parameter mental model

```text
parameter update
    ≠ automatically composition mutation
```

Changing volume/filter coefficients should be able to use a cheap RT-safe control path if the eventual architecture permits it.

Topology/provider changes may require composition/graph rebuild/publication.

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

```text
1. confirm K0 composition reality
2. earn minimal PCM contract
3. direct-flow Source -> processing -> Sink experiment
4. graph publication / replacement / reader overlap
5. real decoder experiment
6. real output experiment
7. only then re-earn playback semantics
```

Real mechanism pressure, not historical nouns, decides whether concepts such as cursor/session/generation/fence are needed.

---

# Formalization mental model

Ask first:

> **Which independently legal states/events can interleave and collide into an illegal state?**

No concrete collision -> prefer types/ownership/tests/static checks.

The old PlaybackTemporal/PlaybackOwnership models are not current architecture acceptance gates.

The likely first new formal target is realtime graph publication vs provider/resource release, but only after executable evidence earns it.

---

# Current code status

Workspace currently includes:

```text
qianqian-core
qianqian-kernel
qianqian-runtime
qianqian-headless
```

The Base Kernel K0 is current.

Playback-specific core code/tests/specs from the prior design remain in the repository as experimental evidence and may be changed or removed by later research without compatibility obligation.

Bootstrap APIs are not compatibility contracts unless a later accepted authority explicitly says so.

---

# Historical refs

Historical evidence is preserved as Git refs:

```text
pre-rust-v2
playback-reference-v1
```

Use only when the current task explicitly needs historical evidence.
