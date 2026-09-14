# Composition Kernel

This document summarizes the generic composition/lifecycle kernel for Architecture v2. It is a **derived summary/router, not the normative K0 semantic authority**. K0 semantics live in `composition-kernel-0-design.md`; representation decisions live in `composition-kernel-0-implementation-adr.md`.

Playback foundations: `../adr/ADR-PBK-001.md`. Current Qianqian vocabulary / Plugin-Fiber taxonomy / static playback composition: `../adr/ADR-PBK-002.md`.

---

# Kernel guardrails

> **K0 controls composition reachability/lifecycle; it does not transport application payloads or understand domain internals.**

Primitive budget:

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

---

# K0 and Plugin

Current architecture mapping (PBK-002 D4/D12):

```text
K0
  ↓ manages
Plugin definition
  ↓ current Rust/formal representation: ComponentSpec
Fiber
  ↓ optional dependency seams
Capability / Service
```

A Plugin is an independently K0-composed lifecycle/behavior unit. A Fiber is one live mounted instance.

`ComponentSpec` remains the K0 implementation/formal term used by the paper-derived kernel semantics. It is **not** a second product architecture category competing with Plugin.

Therefore current Qianqian code may still say:

```text
register_component(ComponentSpec)
component definition
```

while architecture prose says:

```text
register/mount Plugin definition
Plugin instance -> Fiber
```

That mapping changes no K0 state-machine semantics.

---

# Everything is a Plugin — bounded meaning

In Qianqian:

> **Every independently K0-composed lifecycle/behavior unit participates in the common Plugin/Fiber protocol.**

A Plugin:

```text
may require capabilities
may provide capabilities/services
may provide none
may be long-lived
may be episode-scoped
```

Plugin identity is not determined by crate/DLL/trait/thread/feature names.

“Everything is a Plugin” does **not** imply:

```text
every object == Plugin
every command/fact == Plugin
every payload/buffer == Plugin
every endpoint/worker == Plugin
every AudioNode == Plugin
```

A subordinate object stays an owned resource when it does not need its own independent K0 composition identity/lifecycle.

---

# Boundary-first design

Required order:

```text
Plugin granularity / ownership
        ↓
Capability / dependency boundary
        ↓
Interaction / execution semantics
        ↓
Effect / system boundary
        ↓
Global lifecycle ordering
        ↓
Confluence / semantic oracle
        ↓
Context / Fiber / Effect / Reconcile representation
```

A feature name, Rust type, file, crate, UI panel or audio stage is not proof that a new Plugin is needed.

For a candidate Plugin boundary ask:

```text
Does it require independent K0 composition identity?
Does it need independent activation/invalidation/withdrawal?
What lifetime/resources/domain behavior does it own?
What capabilities does it require and optionally provide?
Why is it not a subordinate resource of an existing Plugin?
What execution/data edges cross the boundary?
What is the configuration/cognitive cost?
```

---

# Context / Capability / Service

`Context` is the capability namespace/dependency view visible to a Fiber.

It controls:

```text
which provider is reachable
whether a requirement is satisfied
which committed dependency is valid
```

It must not become:

```text
global product state
universal event bus
message broker
PCM transport
UI payload store
undeclared service locator
```

Across Plugin seams, consumers depend on Capability/Service definitions rather than concrete provider classes.

```text
Capability definition
        │
        └── reaches Service value from provider Plugin
```

> **Capability plane != payload transport.**

Once a service/data edge is bound, ordinary execution uses the already-bound contract/resource.

---

# Fiber

A Fiber is one live Plugin instance under K0 lifecycle.

It owns K0-visible composition state:

```text
runtime identity
committed dependency view
provided capability bindings
requirements
composition Effects/provenance
lifecycle state
```

The runtime composition unit is the Fiber, not the source package or dynamic library.

---

# Plugin-owned domain resources vs K0 Effects

Do not conflate semantic/resource ownership with kernel data.

A Plugin can own domain resources such as:

```text
decoder endpoint
worker thread
PCM edge
render stream
file/session handle
```

without K0 gaining domain-specific fields for them.

K0 `Effect` remains the frozen generic shape from the K0 authority: composition-lifecycle reversible mutation/provenance + total inverse. Domain teardown obligations remain inside Plugin code; K0 observes only its generic inverse/discharge contract.

Current Playback Session Plugin illustrates this boundary:

```text
Playback Session Plugin
    requires Decode + Output capabilities
    opens/binds episode resources
    registers generic relation cleanup/inverses
    owns domain choreography

K0
    orders Fiber lifecycle / dependency / inverse execution
    does not know PCM/decoder/render internals
```

---

# Interaction algebra

Revertibility is not independence.

If:

```text
S0 --A--> S1 --B--> S2
```

an inverse for A does not prove removing A after B preserves B. Independently composable effects/relations require the existing K0 locality/commutativity contracts or explicit ordering structure.

Never derive semantic order from registration, mount or hash iteration order.

---

# Provider withdrawal

Provider disappearance remains dependency-ordered:

```text
provider begins withdrawal
        ↓
stops satisfying new resolution
        ↓
dependents invalidate/deactivate
        ↓
dependents finish teardown with committed teardown access
        ↓
provider final composition resources release
```

For realtime-owned resources, generic K0 ordering may be insufficient to prove physical release safe. If RT readers can still dereference a retiring resource, the owning domain must prove the PBK-001 P1–P5 quiescence/reclamation condition.

---

# Composition Plane vs other planes

```text
Composition Plane
    Plugin/Fiber + Context/Capability + Effect/Reconcile

Execution / Control Plane
    Commands / workflow / Capability-Service calls

Fact Plane
    committed facts / projections / observers

Realtime Data Plane
    pre-bound hot data such as PCM
```

K0 owns only generic composition semantics. It establishes reachability/lifetime conditions used by the other planes; it is not their universal router.

---

# Current playback mapping

Current production units:

```text
Decode Plugin
    long-lived provider Plugin

Output Plugin
    long-lived provider Plugin

Playback Session Plugin
    episode-scoped Plugin
    requires Decode + Output capabilities
    owns the episode-scoped lifetime/teardown of:
        endpoint / worker / PCM edge / render relation / completion
        (allocation/implementation stays with the provider Plugins)
```

All three use the same K0 `ComponentSpec → Fiber` substrate. The earlier “Playback Session is Component but not Plugin” distinction was taxonomy, not a K0 runtime constraint.

---

# PCM data-plane firewall

Current playback data path:

```text
DecodedPcmStream
      ↓
decode worker
      ↓
bounded PcmEdge
      ↓
RenderPcmInput
      ↓
output mechanism
```

Realtime callback/block execution must not perform:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic Plugin dispatch
generic Fact/Event fan-out
filesystem/network/UI round trips
unbounded allocation/blocking
```

`PcmEdge`, `PcmBlock`, endpoints and render streams are resources/data edges, not Plugins merely because Plugins own them.

---

# Event / Fact semantics

A generic EventBus is not automatically a K0 primitive.

If product/domain code needs committed Fact/Event semantics, begin with an ordinary Plugin-provided Capability/Service unless evidence earns a kernel primitive.

Normative Fact contracts live in PBK-001 §2.3. Current episode terminal-outcome authority lives in PBK-002 D11.

---

# Realtime specialization boundary

The generic Composition Kernel is not the audio engine.

A specialized Realtime Audio Runtime may later be earned for:

```text
RT execution views
publication / retirement
reader quiescence
RT-visible lifetime legality
graph/parameter publication
```

Do not pre-create it merely because seek/open/device-switch sound complex. First try the current Plugin/Fiber + owned-resource model. Trigger specialization only from a concrete old/new-world overlap or RT-reader lifetime counterexample.

---

# Phase-F reduction rule

Future playback work should attempt, in order:

```text
1. same existing Plugin/Fiber semantics
2. existing Capability/Service mechanism
3. owned-resource operation/quiescence/replacement
4. only then a new Plugin or RT view mechanism if a counterexample requires it
```

Do not reintroduce `Window`, `Generation`, `Preempted`, etc. merely because historical models used them.

---

# R0/bootstrap compatibility

Older bootstrap shapes are not compatibility contracts unless accepted authority says so. Do not preserve direct field/service-locator escapes merely because they predate K0 composition.

---

# Current gate

```text
Composition Kernel K0                          IMPLEMENTED / CURRENT
Playback Foundations / PBK-001                 ACCEPTED
Current Plugin/Fiber taxonomy / PBK-002        ACCEPTED (D12; admission invariant D13)
Playback Session Plugin classification         episode-scoped Plugin (PBK-002 D6);
                                               code already runs on the common K0 substrate
Phase-F playback semantics                     OPEN (PBK-002 §14); new implementation
                                               paused pending authority design
```
