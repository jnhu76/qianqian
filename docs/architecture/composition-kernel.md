# Composition Kernel

This document is the canonical Architecture v2 authority for Qianqian's generic composition/control-plane kernel.

It defines what belongs in the kernel, what must stay outside it, and the observable invariants the implementation must prove.

## Kernel constitution

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

The generic kernel maintains composition rules. It does not implement the product.

It may know:

```text
Context
Capability
Fiber
Effect
Reconcile
```

It must not know:

```text
Track
PCM
Decoder formats
FFmpeg
WASAPI
PocketJS
KuiklyUI
playlist rules
music commands
UI payload schemas
```

The product should be able to grow substantially while the generic kernel stays small.

## Control plane vs data plane

The kernel belongs to the control/composition plane.

```text
                         CONTROL PLANE

                 Profile / desired tree
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
                  Fiber / Effect state

-------------------------------------------------------------
                          |
                     bind/connect
                          v
                         DATA PLANE

              service.method(application_payload)
                          |
                          v
                       provider

      MediaSource -> Decoder -> Processing -> AudioOutput
```

The kernel decides **who can reach whom**, **who owns which mutation/resource**, and **which component should be alive**.

It does not own the payload being processed after those relationships exist.

Therefore:

> **Capability plane != Data plane.**

## Primitive 1: Context

A Context is the world visible to a fiber.

At minimum it provides a capability/dependency view:

```text
visible capability definitions
active providers/bindings
consumer requirements
effect ownership scope
parent/child scope information when scopes are introduced
```

Context is not merely a global `HashMap<TypeId, Any>`.

It must support lifecycle semantics: provider availability and removal affect dependents.

Context must not become:

```text
global product state
universal event bus
arbitrary message broker
PCM buffer store
UI payload store
service-locator escape hatch that ignores declared requirements
```

A consumer should normally declare what it requires rather than performing arbitrary opportunistic lookup.

Cross-key dependency must be explicit. An operation associated with one capability/coeffect key must not secretly read or mutate unrelated shared keys or hidden globals.

Child contexts, isolation, interception, and overrides are plausible future extensions, but they are not required before the base lifecycle calculus works.

## Primitive 2: Capability

A Capability identifies a service/contract that can be provided and required.

```text
Service/Capability Definition
          ^
          |
     +----+----+
     |         |
 Provider   Consumer
```

The provider implementation is not the capability identity.

Across plugin seams, consumers depend on capability/service definitions rather than concrete providers.

The first Rust representation should prefer type-safe identities/contracts over unstructured string-only keys where practical.

Do not prematurely build semver solvers, schema registries, or capability marketplaces.

### Operations are part of the capability contract

A shared capability is not only `key -> value`. Its public operations determine whether multiple fibers can compose safely.

For each shared mutable capability, the provider/service definition must make clear:

```text
what state each operation may read/write
what observable result it exposes
what inverse/disposer removes only the caller's contribution
which operations may coexist independently
which operations are order-sensitive
```

Interface design therefore participates directly in composability.

Opaque handles/tokens are preferred when exposing incidental physical identity would create unnecessary observable ordering or non-commutativity.

## Primitive 3: Fiber

A Fiber is a live plugin instance.

The fiber—not a crate, module, package, shared library, or plugin definition—is the unit the composition kernel schedules and owns.

A fiber owns at least:

```text
identity
parent/scope relationship
plugin definition/configuration
requirements
provided capabilities/context bindings
owned effects
lifecycle state
in-flight transition state as needed
```

One plugin definition may have multiple fiber instances in different contexts/configurations.

### Lifecycle semantics

The target lifecycle is equivalent to:

```text
PENDING --dependencies satisfied--> LOADING --> ACTIVE
   ^                                      |
   |                                      |
   +---------- UNLOADING <---------------+
                  |
                  +--> PENDING
                  +--> DISPOSED

Activation failure may enter FAILED according to the implementation contract.
```

Hard invariants:

- a fiber must not be ACTIVE while a required capability is unsatisfied;
- dependency appearance can make a pending fiber eligible to load;
- dependency disappearance must proactively remove the dependent from ACTIVE state;
- component teardown must be dependency-aware, not deferred until a later failed service call;
- transitions must avoid impossible overlap/reentrancy states.

Provider teardown must preserve dependency ordering: the provider is logically withdrawn from new resolution, dependents leave ACTIVE while teardown access is still safe where required, then provider-owned bindings/resources are finally removed.

## Primitive 4: Effect

Effect is the ownership/write boundary for kernel-visible reversible mutation.

The purpose is to eliminate split-brain lifecycle code such as:

```text
activate(): register A, B, C
shutdown(): hopefully remember to unregister C, B, A
```

Instead, a fiber owns the mutations/resources it introduces.

Typical reversible local effects include:

```text
provide/bind a capability
register a listener
create a timer
mount a child fiber
register a watcher
own a local runtime handle
```

The effect record must retain enough teardown/inverse information for deterministic disposal.

Within one fiber, owned effects should normally unwind in reverse/LIFO order unless a stronger contract proves otherwise.

### LIFO is not cross-fiber composability

An inverse only proves that an effect can undo itself in the state for which that inverse was produced.

If another fiber has modified the world in between, this is not automatically enough:

```text
S0 --A--> S1 --B--> S2

A^-1(S1) = S0
```

does not by itself prove that `A^-1(S2)` correctly removes only A while preserving B.

Therefore:

> **Revertibility is necessary but not sufficient for independent removal.**

Cross-fiber composition additionally requires **Independence** and **Commutativity** properties at the shared-operation boundary.

### Independence / commutativity contract

For two effects/components A and B to be independently removable, the relevant forward and inverse transformations must not interfere with one another. Engineering review must consider at least the equivalent relationships of:

```text
A    with B
A    with B^-1
A^-1 with B
A^-1 with B^-1
```

Foreign transformations must also not change the other component's intended inverse/continuation in a way that changes its public contract.

This is stronger than checking only that forward(A) and forward(B) look harmless.

### Different keys

Operations confined to genuinely distinct capability/coeffect keys should be designed so that they do not read or write one another's state. This is the preferred route to independence.

### Same key

When multiple fibers mutate the same shared key, independence is not automatic.

The key/provider interface must locally define or demonstrate how concurrent/interleaved operations compose.

A good shape is often contribution-based:

```text
register(value) -> opaque token
unregister(token)
```

where removing token A cannot damage token B.

### Non-commutative relationships

Some interactions are inherently ordered.

Example:

```text
middleware A -> B
```

may not be observationally equivalent to:

```text
middleware B -> A
```

Such interactions must **not** be mislabeled as independent effects.

Freeze this rule:

> **Commutative relation -> independent effect composition.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

The architecture does not try to eliminate ordering. It requires ordering to be explicit.

### Effect is not magical rollback

Not every action is reversible.

External network writes, destructive filesystem operations, money movement, or third-party side effects may require:

```text
transactional semantics
compensating action
explicit irreversible classification
```

Do not claim correctness by wrapping an irreversible action in an `Effect` closure.

## Observational equivalence

Correct restoration does not require private physical state to become bit-for-bit identical to its prior representation.

Opaque IDs, allocator layouts, generations, or internal handles may differ after undo.

The correctness criterion is public behavior:

> After removing A, the observable system should be equivalent to a world where A never contributed, while independent B/C contributions remain present.

Tests should therefore prefer public/observable invariants such as:

```text
which capabilities are reachable
which fibers are ACTIVE/PENDING/DISPOSED
which public service behavior remains
which contribution tokens/bindings remain
whether owned effects have been retired
```

Do not overfit tests to irrelevant private bit identity.

## Boundary design and decomposition

The calculus does not magically discover the one correct component/key split.

Architecture work must decide:

```text
who owns the state
which capabilities are truly required
what state an operation may read/write
whether operations commute
whether an ordering dependency is real
whether an API exposes unnecessary observable information
whether an inverse removes only the caller's contribution
whether the state lies inside the recoverable system boundary
whether finer decomposition improves composability enough to justify complexity
```

A mathematically fine-grained decomposition can still be an engineering failure if it produces excessive components, naming, configuration, or cognitive cost.

Conversely, a convenient coarse component is not acceptable if it relies on arbitrary shared references or hidden global mutation.

## Primitive 5: Reconcile

Reconcile converts desired composition into a running Fiber graph.

```text
Profile / Patch / desired tree
            |
            v
        Reconcile
            |
            v
      Fiber Graph
```

This prevents application composition from silently becoming a privileged imperative `boot()` function.

The first implementation does not need a rich config language or patch system. A minimal in-memory desired composition is enough if it proves that mount/unmount/lifecycle changes are mediated by the same kernel protocol.

Do not add YAML, dynamic loading, filesystem watching, HMR, or remote plugin discovery before the base reconciler semantics exist.

## Everything is a plugin

In Qianqian, this means:

> Every long-lived product capability that participates in application runtime composition must enter through the same Context/Capability/Fiber/Effect lifecycle protocol.

It does not mean:

```text
everything is a shared library
everything is runtime downloaded
everything uses string keys
every payload is an event
every function call goes through Context
```

Examples of future ordinary plugins include:

```text
Music
AudioRuntime
Decoder provider
Processing/DSP provider
AudioOutput provider
Presentation
UiHost
Library
MediaKeys
FilePicker
Analyzer
```

Some plugins may own internal domain kernels/graphs/runtimes. They remain ordinary residents from the generic kernel's perspective.

## Service calls and business payloads

Once capability resolution has produced a provider/service endpoint, application payload should normally move directly through that contract.

```text
Consumer requirement
       |
       v
     Context
       |
    resolves
       v
Service endpoint/provider
       |
       v
service.method(payload)
```

Context participates in resolution/binding, not necessarily every payload byte or realtime operation.

This distinction is mandatory for media/realtime paths.

## Events

A general application Event Bus is not one of the five generic kernel primitives.

If product/domain event semantics are required, they should initially be modeled as a normal Event Service/plugin.

The kernel may own listener-registration teardown as Effects.

Kernel-internal dependency notifications are part of capability/fiber lifecycle implementation and are not a public product event bus.

## Replacement and provider disappearance

Provider replacement/disappearance must be expressible without privileged product logic.

```text
B requires X
A1 provides X

A1 active -> B active
unmount A1 -> B leaves active state
mount A2 providing X -> B becomes eligible to activate again
```

A consumer must not continue running under a satisfied-dependency assumption after its provider has been logically withdrawn.

## Root disposal

After successful root disposal, the kernel should be observationally equivalent to a clean root:

```text
zero active fibers
zero visible service/capability bindings owned by disposed fibers
zero outstanding owned reversible effects
no dependent remains active against a removed provider
```

This is the anti-ghost-state oracle.

## Realtime specialization boundary

The generic Composition Kernel is not the audio engine.

A future AudioRuntime plugin may own:

```text
AudioGraph
clock
buffer pool
format negotiation
RT scheduling
graph swap/publication
```

The realtime thread must not perform generic kernel operations per block/callback:

```text
Context lookup
capability resolution
reconciliation
arbitrary generic event dispatch
unbounded allocation/blocking
UI/runtime round trips
```

Graph changes should be prepared on the control plane and published at a realtime-safe boundary.

PCM/data blocks travel directly through pre-bound graph edges.

## Relationship to MusicKernel

`MusicKernel` remains a domain semantic authority.

It is expected to live inside/behind a Music plugin rather than serve as the application's generic kernel.

```text
Composition Kernel
       |
       v
   Music Fiber
       |
       +-- owns MusicKernel
       +-- requires media/audio capabilities
       +-- provides music/transport/state services
```

## R0 migration authority

RUST-ARCH-R0 created constructor-only composition as a bootstrap proof.

The following shapes are explicitly provisional:

```text
qianqian-core::base
qianqian-runtime::AppRuntime constructor composition
with_audio_output()
other direct R0 capability fields/accessors
```

The Composition Kernel implementation task is authorized to remove/refactor them.

Do not maintain R0 API compatibility unless a real current consumer requires it.

## First implementation oracle

The first kernel implementation should remain intentionally small and prove:

```text
1. mount Consumer B requiring X
   -> B PENDING

2. mount Provider A1 providing X
   -> A1 ACTIVE
   -> X available
   -> B ACTIVE

3. mount independent Provider/Consumer pair on Y
   -> Y-side contribution becomes observable

4. unmount A1
   -> X logically withdrawn
   -> B leaves ACTIVE
   -> B-owned effects unwind
   -> A1 teardown completes
   -> Y-side observable contribution remains intact

5. mount Provider A2 providing X
   -> B may activate again

6. dispose root
   -> observationally equivalent to clean root
```

Tests must separately prove:

```text
single-Fiber LIFO cleanup
cross-Fiber independence
same-key contribution safety where introduced
non-commutative relations rejected or made explicitly ordered
```

The task does not need audio, FFmpeg, UI, async networking, dynamic libraries, config files, or HMR.

## Non-goals for the first kernel task

Do not automatically add:

```text
string-key plugin marketplace
semver dependency solver
dynamic library loading
filesystem plugin discovery
YAML/TOML profile parser
hot reload / HMR
remote plugins
full child-context isolation/interception system
generic event middleware stack
audio graph runtime
FFmpeg/WASAPI/PocketJS/KuiklyUI
```

Prove the calculus before growing the ecosystem.
