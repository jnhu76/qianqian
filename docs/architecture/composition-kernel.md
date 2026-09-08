# Composition Kernel

This document is the canonical Architecture v2 authority for Qianqian's generic composition/control-plane kernel.

Playback-specific semantics have a **proposed replacement** in `docs/adr/ADR-PBK-001.md` (PROPOSED / FORMAL CORE PASS). The historical #53 audit remains the registered ARCH-003 authority until the ADR is ACCEPTED and the registry is updated; its playback-specific ownership conclusions are historical inputs under that proposed replacement. Do not combine the two playback models in one implementation.

It defines both:

1. the **preconditions** that must hold before a kernel implementation is justified; and
2. the invariants the eventual kernel must enforce.

Gate chain:

```text
#53 COMPONENT-BOUNDARY-A0        PASS / CLOSED (audit: component-boundary-a0.md)
        ↓
#67 COMPOSITION-KERNEL-0 DESIGN  semantic design MERGED via PR #68
                                  (composition-kernel-0-design.md)
        ↓
#70 COMPOSITION-KERNEL-0 IMPL    IMPLEMENTED via PR #71 (70 kernel tests / 75 workspace tests, 743eb86)
        ↓
ADR-PBK-001 PLAYBACK ARCH        PROPOSED / FORMAL CORE PASS (PR #78 + #79)
```

## Kernel constitution

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

The generic kernel maintains composition rules. It does not implement the product.

Its expected primitive budget remains:

```text
Context
Capability
Fiber
Effect
Reconcile
```

It must not know:

```text
TrackSession / DecodeSession
playback cursor / Active / Prepared
Generation admission / Physical Fence
PCM / MediaSpan
codec formats
FFmpeg
WASAPI
PocketJS
KuiklyUI
playlist policy
music commands
UI payload schemas
```

But the primitive list is **not** the architecture starting point.

## Boundary design comes first

The Base Kernel K0 (PR #71) implements Context/Fiber/Effect/Reconcile. The product decomposition was decided by the component boundary audit (#53), with playback-specific corrections proposed by ADR-PBK-001 (PROPOSED, pending acceptance).

Required design order:

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Interaction Algebra
        ↓
Effect / System Boundary
        ↓
Global lifecycle ordering
        ↓
Confluence oracle
        ↓
Composition Kernel implementation
```

The calculus gives strong properties only after component/key/operation boundaries are good enough.

It does not provide a unique automatic decomposition.

### Component granularity

A component is not independent because it has a different feature name.

For every candidate boundary, review:

```text
state/resource ownership
required capabilities
provided capabilities
cross-boundary operations/data edges
observable contract
operation locality
commutativity / explicit ordering
recoverable system boundary
provider/consumer lifecycle dependency
configuration/naming/cognitive cost
```

Apparent cycles such as:

```text
A requires B
B requires A
```

should trigger a mediation/integration-component audit. Bidirectional interaction may sometimes be decomposed into clearer one-way bindings.

However, finer decomposition is not automatically better. Component count, configuration and cognitive cost are first-class engineering costs.

Current generic composition authority: **#67 / PR #68** (`composition-kernel-0-design.md`) plus the implementation ADR. Historical decomposition evidence and registered ARCH-003 authority: **#53 COMPONENT-BOUNDARY-A0**. Proposed playback-specific replacement: **ADR-PBK-001** (PROPOSED / FORMAL CORE PASS, pending acceptance).

## Control plane vs data plane

The Composition Kernel belongs to the control plane.

```text
                         CONTROL PLANE

                 desired composition
                          |
                          v
                      Reconcile
                          |
                          v
                     Fiber Graph
                          |
                 provide / require
                    owned effects
                          |
                          v
                 Context / Capability

-------------------------------------------------------------
                          |
                     bind/connect
                          v
                         DATA PLANE

              service.method(application_payload)
                          |
                          v
                       provider

      Encoded Media -> Decoder -> Processing -> AudioOutput
```

The kernel decides:

```text
who may reach whom
which dependency is satisfied
who owns a mutation/resource
which component should be alive
```

It does not own the application payload after the relationship is established.

> **Capability plane != Data plane.**

PCM/data blocks must not travel through Context or a generic event bus per callback/block.

## Capability / service definition

A Capability identifies a contract that can be required and provided.

```text
Capability / Service Definition
             ^
        +----+----+
        |         |
    Provider   Consumer
```

Across plugin seams, consumers depend on the definition rather than a concrete provider implementation.

The public operations of that capability are part of its composability contract.

For each shared mutable capability, boundary design must make clear:

```text
what state each operation may read/write
what result is intentionally observable
which contributions can coexist independently
how a caller removes only its own contribution
which operations are order-sensitive
```

An operation on one key/capability must not secretly read/write unrelated shared keys or hidden global state. Cross-key dependency must be explicit.

Type-safe identities/contracts are preferred over unstructured string-only keys when practical, but version/schema/marketplace machinery is not currently authorized.

## Interaction algebra

### Revertibility is not independence

Within one component/Fiber, an effect may produce an inverse and later unwind in LIFO order.

That only proves local revertibility.

If:

```text
S0 --A--> S1 --B--> S2
```

and:

```text
A^-1(S1) = S0
```

this does **not** prove that applying `A^-1` in `S2` preserves B.

For cross-component independent removal, design/review must account for the relevant forward/inverse interactions, conceptually including:

```text
A    with B
A    with B^-1
A^-1 with B
A^-1 with B^-1
```

Foreign transformations also must not alter another component's intended inverse/continuation/public behavior in a way that breaks its contract.

### Different keys

Operations on genuinely distinct keys should be key-local and independent by construction.

This property is lost if implementation escapes into hidden globals or undeclared cross-key state.

### Same key

Multiple contributions to one shared interface are not automatically commutative.

When the semantics fit, contribution-oriented operations may help:

```text
register(value) -> opaque token
unregister(token)
```

where removing A's token cannot damage B's contribution.

Opaque identity is useful when exposing incidental sequence/position information would create needless observable non-commutativity.

### Non-commutative relationships

If:

```text
A -> B != B -> A
```

then the interaction is ordered.

Freeze:

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

DSP/pipeline ordering is a canonical example. Semantic topology must never depend on registration timing, hash iteration, or incidental mount order.

The architecture does not eliminate order; it makes order explicit.

## Effect and system boundary

An Effect records kernel-visible mutation/resource provenance owned by a component/Fiber.

Typical locally reversible examples:

```text
capability binding
listener/callback registration
timer registration
watcher registration
local handle/resource ownership
buffer allocation owned by the runtime
```

(Child-fiber mounting is deliberately absent: child mounting is out of K0 scope — [PAPER] design context only, deferred per `composition-kernel-0-design.md` §S.)

Within one Fiber, deterministic reverse/LIFO unwind is the default local rule unless a stronger contract says otherwise.

But not every action is reversible.

Classify actions when relevant as:

```text
Reversible
Transactional
Compensatable
Irreversible / emitted outside system boundary
```

These labels are a system-boundary/action taxonomy for reasoning about actions — not runtime variants of a kernel Effect type; the frozen K0 Effect has exactly one shape (reversible composition-lifecycle mutation + total inverse, `composition-kernel-0-design.md` §H.5/§H.7).

For a player, already-rendered audio is outside rollback: the runtime cannot “unplay” sound already emitted to the physical world.

Do not wrap an irreversible action in an Effect closure and call that rollback correctness.

## Observational equivalence

Recovery/removal correctness does not require private implementation state to become bit-for-bit identical.

The relevant oracle is public/architecturally observable behavior.

After removing A:

> the system should be observationally equivalent to a world where A never contributed, while independent B/C contributions remain.

Useful observable facts include:

```text
reachable capabilities
active/pending/disposed lifecycle truth
public service behavior
remaining contributions/bindings
absence of ghost effects/resources
```

Opaque IDs, allocator layout, private generations and incidental token values need not match.

## Global lifecycle ordering

Provider disappearance is a dependency-order problem.

Required semantic shape:

```text
provider begins withdrawal
        ↓
provider stops satisfying new dependency resolution
        ↓
dependents detect invalidation and deactivate
        ↓
dependents complete teardown while required teardown access is valid
        ↓
provider finally removes/reclaims binding/resources
```

Do not physically destroy the provider first and let dependents fail later.

The eventual Fiber lifecycle must enforce this invariant even if the internal state-machine names differ.

## Confluence

A major correctness target is **confluence at quiescence**:

> **After any legal load/unload/replacement history reaches quiescence, the observable runtime is equivalent to a clean construction of the final desired composition.**

Example history:

```text
load source A
insert EQ
switch output
remove EQ
replace decoder
settle
```

If the final desired composition is:

```text
source A + decoder B + output C
```

then the settled runtime should match a clean root directly composed as that final graph in all relevant observable respects.

This is stronger than:

```text
no crash
all disposers ran
zero leaked handles
```

because it also detects history-dependent topology, stale bindings and ghost contributions.

Playback position/state continuity is **not** automatically composition-confluence truth; that belongs to playback/domain policy and ADR-PBK-001.

## Event semantics

A generic public EventBus is not automatically a Composition Kernel primitive.

If a product/domain needs event semantics, model that initially as a normal Event Service/plugin. Listener ownership may still be tracked as Effects.

Kernel-internal dependency invalidation is not the same thing as a public product event bus.

## Future kernel primitives

### Context

Capability/dependency view visible to a Fiber. Not a global `HashMap<TypeId, Any>` escape hatch and not a payload bus.

### Fiber

Live plugin instance owning identity, scope/parent, requirements, provided bindings, effects and lifecycle state.

### Effect

Owned kernel-visible mutation/recovery provenance.

### Reconcile

Moves a running Fiber graph toward desired composition without collapsing the architecture into a privileged imperative `boot()` function.

Exact Rust APIs/state enum names remain free to evolve subject to the already-implemented K0 semantics.

## Everything is a Plugin

In Qianqian this means:

> Every **justified** long-lived runtime capability eventually participates in the common composition/lifecycle protocol.

It does not mean:

```text
one feature name == one plugin
one plugin == one crate
one plugin == one dynamic library
everything is hot-loaded
everything is rollbackable
every payload goes through Context
```

The historical #53 audit proposed the MVP composition boundaries. ADR-PBK-001 (PROPOSED) proposes a refined internal playback authority/lifetime model without turning MusicKernel, TransportKernel, TrackSession, DecodeSession, Active, Prepared, or ordinary DSP nodes into independent Composition plugins.

## Realtime specialization boundary

The generic Composition Kernel is not the audio engine.

A future specialized audio graph/runtime may own:

```text
AudioGraph
clock
buffer pool
format negotiation
RT scheduling
graph publication/swap
```

if that boundary is later earned.

Realtime callback/block execution must not perform:

```text
Context lookup
capability resolution
Fiber reconciliation
arbitrary generic event dispatch
filesystem/network I/O
UI/runtime round trips
unbounded allocation/blocking
```

PCM travels through pre-bound data-plane graph edges.

## Relationship to playback authorities

`ADR-PBK-001` (PROPOSED) replaces the old shorthand “Playback Kernel = MusicKernel” in its proposed model. That proposed playback structure is:

```text
MusicComponent   = composition lifecycle root
MusicKernel      = music/product semantic authority
TransportKernel  = playback temporal authority
```

Nested runtime includes `TrackSession(s)` and `DecodeSession(s)`. Active/Prepared are temporal roles inside TransportKernel.

The generic Composition Kernel sees the `MusicComponent` and its capability/provider relationships; it does not own or interpret playback cursor, Window roles, Generation admission, Physical Fence state, or raw playback evidence.

After binding, PCM flows on direct/pre-bound data edges. Raw playback evidence is interpreted by TransportKernel; MusicKernel receives typed derived facts for product decisions.

## R0 migration authority

RUST-ARCH-R0 created provisional bootstrap shapes:

```text
qianqian-core::base
qianqian-runtime::AppRuntime
AppRuntime::new()
with_audio_output()
other direct R0 capability fields/accessors
```

They are not compatibility contracts.

The Base Kernel K0 implementation (PR #71) may replace R0 bootstrap shapes rather than preserve them for compatibility.

## Current gate

Current authority chain:

```text
#53 COMPONENT-BOUNDARY-A0
        ↓ PASS / CLOSED (historical decomposition audit: component-boundary-a0.md)
#67 COMPOSITION-KERNEL-0 DESIGN — semantic design MERGED via PR #68
        ↓ (composition-kernel-0-design.md; Revisions 1–6)
#70 COMPOSITION-KERNEL-0 IMPL — IMPLEMENTED via PR #71
        ↓ (70 kernel tests / 75 workspace tests, 743eb86)
ADR-PBK-001 PLAYBACK ARCH — PROPOSED / FORMAL CORE PASS
        ↓ (PR #78 + #79; implementation authorization remains separate)
```

#53 remains evidence for the original component-decomposition audit and the registered ARCH-003 authority until ADR-PBK-001 is ACCEPTED. Playback-specific semantics have a PROPOSED replacement in ADR-PBK-001. Generic K0 semantics remain current in the K0 design/implementation ADRs and code/tests.
