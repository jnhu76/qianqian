# Composition Kernel

This document is the canonical Architecture v2 authority for Qianqian's generic composition/lifecycle kernel.

Playback-specific architecture is currently **reopened**. `docs/adr/ADR-PBK-001.md` is PROPOSED and deliberately does not freeze the previous MusicKernel/TransportKernel/TrackSession/Generation model. Old playback code/specs are experimental evidence only.

The Base Kernel K0 itself remains implemented/current.

---

# Kernel constitution

> **Kernel controls reachability, composition-visible ownership and lifecycle; it does not own application payloads.**

Expected primitive budget:

```text
Context
Capability
Fiber
Effect
Reconcile
```

The generic kernel must not know:

```text
PCM
AudioGraph
FFmpeg
WASAPI
track/playlist semantics
seek/playback state
UI product payloads
```

The primitive list is not the design starting point.

---

# Boundary-first design

Required order:

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Interaction / execution semantics
        ↓
Effect / system boundary
        ↓
Global lifecycle ordering
        ↓
Confluence oracle
        ↓
Context / Fiber / Effect / Reconcile representation
```

A feature name, Rust type, file, crate or UI panel is not proof that something deserves its own Plugin.

For every candidate Plugin/component boundary, review:

```text
state/resource lifetime
required capabilities
provided capabilities
cross-boundary execution/data edges
observable contract
operation locality
commutativity / explicit ordering
provider/consumer lifecycle dependency
configuration/naming/cognitive cost
```

Apparent bidirectional dependency should trigger a mediation/integration audit, but finer splitting is not automatically better.

---

# Context / Capability / Service

`Context` is the capability namespace/dependency view visible to a Fiber.

It controls:

```text
which provider is reachable
whether a requirement is satisfied
which dependency is valid now
```

It must not become:

```text
global product state
universal event bus
message broker
PCM transport
UI payload store
undeclared get-anything service locator
```

Across Plugin seams, consumers depend on capability/service definitions rather than concrete provider classes.

```text
Capability / Service Definition
              ^
         +----+----+
         |         |
      Provider   Consumer
```

> **Capability plane != payload transport.**

Once a capability is bound, ordinary execution uses that service contract or a pre-bound data edge.

---

# Fiber

A Fiber is the live runtime instance of a Plugin definition.

It owns:

```text
runtime identity
scope/dependency view
provided capability bindings
requirements
composition-visible Effects/resources
lifecycle state
```

The runtime composition unit is the Fiber, not the source package or dynamic library.

---

# Effect and system boundary

An Effect records composition-visible mutation/resource provenance owned by a Fiber.

Typical locally reversible examples:

```text
capability binding
listener/callback registration
timer/watcher registration
local handle/resource ownership
```

Within one Fiber, deterministic reverse/LIFO unwind is the default local rule unless a stronger contract says otherwise.

But not every real-world action is reversible.

Reason about actions when relevant as:

```text
Reversible
Transactional
Compensatable
Irreversible / emitted outside the recoverable system boundary
```

These are reasoning categories, not new K0 Effect variants.

Already emitted physical sound, network transmission or external side effect cannot be made correct merely by wrapping it in an Effect closure.

---

# Interaction algebra

## Revertibility is not independence

If:

```text
S0 --A--> S1 --B--> S2
```

and `A` has an inverse, that only proves local rollback from the state where the inverse contract applies. It does not prove removing A after B preserves B.

For independent composition, cross-component interaction must preserve the other component's observable contribution.

## Same/different capability keys

Operations on genuinely independent capability/state keys should stay key-local.

Same-key multi-contributor semantics must be explicit; do not assume commutativity.

Contribution-oriented shapes may be useful where semantically correct:

```text
register(value) -> opaque token
unregister(token)
```

## Ordered relationships

If:

```text
A -> B != B -> A
```

then the relationship is ordered.

Freeze:

> **Commutative relation -> may compose independently.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

Never derive semantic order from registration time, mount order or hash iteration.

Realtime/DSP ordering is an important example, but the Composition Kernel does not itself become the audio processing graph.

---

# Provider withdrawal

Provider disappearance is dependency-ordered.

Minimum semantic shape:

```text
provider begins withdrawal
        ↓
provider stops satisfying new resolution
        ↓
dependents are invalidated/deactivate
        ↓
dependents finish teardown while required teardown access is still valid
        ↓
provider removes final composition bindings/resources
```

Do not destroy a provider first and let consumers discover the failure on a later call.

For realtime consumers, generic provider withdrawal may need an additional domain-specific quiescence condition before the underlying resource can actually be freed. The generic kernel does not invent that realtime mechanism; the owning domain runtime must expose/prove the safe release condition.

---

# Observational equivalence

Recovery/removal correctness is judged through public/architecturally relevant behavior rather than private bit identity.

After removing A:

> the observable system should match a world where A never contributed, while independent B/C contributions remain.

Opaque IDs, allocator layout and private generations need not match.

---

# Confluence

A core correctness target is **confluence at quiescence**:

> **After any legal composition history settles, observable composition truth is equivalent to a clean construction of the same final desired composition.**

This is stronger than:

```text
no crash
all disposers ran
no obvious leaked handles
```

because it also detects stale bindings, ghost contributions and history-dependent topology.

Domain continuity (for example playback position) is not automatically composition-confluence truth.

---

# Composition Plane vs other planes

The current architecture reset distinguishes:

```text
Composition Plane
    Context / Capability / Fiber / Effect / Reconcile

Execution / Control Plane
    Commands / workflow / Capability-Service calls

Fact Plane
    committed facts / projections / persistence / observers

Realtime Data Plane
    pre-bound hot data such as PCM
```

The generic Composition Kernel owns only the first plane.

It establishes the lifetime/reachability conditions that the other planes may depend on, but it does not become their universal router.

---

# Event / Fact semantics

A generic EventBus is not automatically a Composition Kernel primitive.

If product/domain code needs committed Fact/Event semantics, begin with a normal capability/service/plugin.

The K0-scoped rule is only that these are **not** kernel primitives. The normative Fact contracts — semantic-commit definition, one designated authority per fact type, projection read-side firewall, Fact publication vs Realtime-view publication — live in the reset proposal `../adr/ADR-PBK-001.md` §2.3; this document summarizes and does not carry a second normative copy.

Repository-wide Event Sourcing, append-only persistence and waterfall/middleware remain unfrozen as K0 primitives.

---

# Everything is a Plugin

In Qianqian this means:

> **Every justified long-lived runtime capability ultimately participates in the common composition/lifecycle protocol.**

It does not mean:

```text
one feature name = one plugin
one plugin = one crate
one plugin = one dynamic library
everything is hot-loaded
everything is rollbackable
every payload goes through Context
every AudioNode is automatically a Fiber
```

A Plugin can provide factories/resources that participate in another specialized runtime without turning every produced object into another Plugin.

---

# Realtime specialization boundary

The generic Composition Kernel is not the audio engine.

A specialized Audio Runtime may later own:

```text
PCM contract
realtime graph/view
clock/buffers
format negotiation
RT scheduling
graph publication/swap
reader quiescence
parameter publication
```

if those boundaries are earned.

Realtime callback/block execution must not perform:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic Fact/Event fan-out
filesystem/network/UI round trips
unbounded allocation/blocking
```

Hot data travels through pre-bound realtime edges. (Playback-side normative form of the forbidden list: `../adr/ADR-PBK-001.md` §2.4.)

---

# Relationship to playback reset

ARCH-003 is currently reopened.

The generic Composition Kernel currently promises only that playback/audio capabilities can participate in normal Plugin/Fiber lifecycle and dependency management without polluting K0 with music/audio concepts.

It does **not** currently promise that these legacy playback nouns are correct (full reopened list: ADR-PBK-001 §0/§10):

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Generation
Active / Prepared
Dual Window
Physical Fence
```

Those names may remain in experimental code/specs, but they are not generic K0 requirements and not current playback authority.

The reset Playback ADR will re-earn whatever domain/control/realtime abstractions real experiments require.

---

# R0/bootstrap compatibility

Older bootstrap shapes such as `AppRuntime` convenience fields/accessors are not compatibility contracts unless an accepted authority explicitly says so.

Do not preserve a direct field/service-locator escape hatch merely because it existed before K0 composition.

---

# Current gate

```text
Base Kernel K0                         IMPLEMENTED / CURRENT
Playback Foundations / ARCH-003        NEXT / REOPENED
ADR-PBK-001                             PROPOSED
old playback code/specs                 EXPERIMENTAL EVIDENCE ONLY
```

Generic K0 design/implementation authority remains current. Playback-specific conclusions must come from the reset ADR and new executable evidence, not from stale examples in historical documents.
