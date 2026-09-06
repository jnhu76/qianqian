# Composition Kernel

This document is the canonical Architecture v2 authority for Qianqian's generic composition/control-plane kernel.

It defines what belongs in the kernel, what must stay outside it, and the observable invariants the first implementation must prove.

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

### Future scope features

Child contexts, isolation, interception, and overrides are plausible future extensions, but they are not required by the first kernel task unless needed to prove the base invariants.

Do not prebuild the entire scope system before the minimal lifecycle calculus works.

## Primitive 2: Capability

A Capability identifies a service/contract that can be provided and required.

The conceptual roles are:

```text
Service/Capability Definition
          ^
          |
     +----+----+
     |         |
 Provider   Consumer
```

The provider implementation is not the capability identity.

Across plugin seams, a consumer should depend on a capability definition rather than a concrete provider implementation.

The first Rust representation should prefer type-safe identities/contracts over unstructured string-only keys where practical.

Do not prematurely build semver solvers, schema registries, or capability marketplaces. Interface identity/versioning can grow when real compatibility pressure appears.

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

Exact enum names are not frozen.

The hard invariants are:

- a fiber must not be ACTIVE while a required capability is unsatisfied;
- dependency appearance can make a pending fiber eligible to load;
- dependency disappearance must proactively remove the dependent from ACTIVE state;
- component teardown should be dependency-aware, not deferred until a later failed service call;
- transitions must avoid impossible overlap/reentrancy states.

If load/unload transition inertia is required to avoid races, establish it with tests before adding more states.

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

Owned effects should unwind in reverse order unless a more specific contract proves otherwise.

### Effect is not magical rollback

Not every action is reversible.

Examples such as external network writes, destructive filesystem operations, money movement, or third-party side effects may require:

```text
transactional semantics
compensating action
explicit irreversible classification
```

Do not claim correctness by wrapping an irreversible action in an `Effect` closure.

The first Composition Kernel task should focus on genuinely reversible in-process/local runtime effects.

## Primitive 5: Reconcile

Reconcile converts desired composition into a running Fiber graph.

The architectural intent is:

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

The first implementation does not need a rich config language or patch system.

A minimal in-memory desired composition is enough if it proves that mount/unmount/lifecycle changes are mediated by the same kernel protocol.

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

Some of those plugins may own internal domain kernels/graphs/runtimes. They remain ordinary residents from the generic kernel's perspective.

## Service calls and business payloads

Once capability resolution has produced a provider/service endpoint, application payload should normally move directly through that contract.

Conceptually:

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

Context participates in the resolution/binding step, not necessarily every payload byte or realtime operation.

This distinction is mandatory for media/realtime paths.

## Events

A general application Event Bus is not one of the five generic kernel primitives.

If product/domain event semantics are required, they should initially be modeled as a normal Event Service/plugin.

The kernel may own listener-registration teardown as Effects.

Kernel-internal dependency notifications are part of capability/fiber lifecycle implementation and are not a public product event bus.

This prevents the kernel from turning into a universal middleware/message system.

## Provider disappearance ordering

Provider teardown is a first-class correctness problem.

The desired ordering is conceptually:

```text
Provider begins UNLOADING
        |
        v
provider no longer counts as ACTIVE/available for new resolution
        |
        v
dependents are refreshed / leave ACTIVE state
        |
        v
dependent effects/resources unwind as required
        |
        v
provider binding/resources are finally removed/released
```

The implementation may use a different internal sequence, but it must prove the externally relevant invariant:

> A consumer must not continue running under a satisfied-dependency assumption after its provider has been logically withdrawn.

## Replacement and reactivation

A provider replacement should be expressible without privileged product logic.

Minimal target behavior:

```text
B requires X
A1 provides X

A1 active -> B active
unmount A1 -> B leaves active state
mount A2 providing X -> B becomes eligible to activate again
```

The first kernel experiment should prove this deterministically.

## Root disposal

Disposing the root composition should provide a strong cleanliness oracle.

After successful root disposal, the kernel should be able to demonstrate equivalent observable truth to:

```text
zero active fibers
zero visible service/capability bindings owned by disposed fibers
zero outstanding owned reversible effects
no dependent remains active against a removed provider
```

This is one of the most important anti-ghost-state tests.

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

Conceptually:

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

This separation allows the Composition Kernel to remain reusable/domain-agnostic while MusicKernel can grow rich product semantics.

## R0 migration authority

RUST-ARCH-R0 created constructor-only composition as a bootstrap proof.

The following current shapes are explicitly provisional:

```text
qianqian-core::base
qianqian-runtime::AppRuntime constructor composition
with_audio_output()
other direct R0 capability fields/accessors
```

The Composition Kernel implementation task is authorized to remove/refactor them.

Do not maintain R0 API compatibility unless a real current consumer requires it.

## First implementation oracle

The first kernel implementation should remain intentionally small and prove this scenario:

```text
1. mount Consumer B requiring X
   -> B is PENDING

2. mount Provider A1 providing X
   -> A1 becomes ACTIVE
   -> X becomes available
   -> B becomes ACTIVE

3. unmount A1
   -> A1 begins unloading
   -> X becomes unavailable to dependents
   -> B leaves ACTIVE and returns to PENDING (or equivalent waiting state)
   -> B's owned effects unwind
   -> A1 teardown completes

4. mount Provider A2 providing X
   -> X becomes available again
   -> B activates again

5. dispose root
   -> all fibers disposed
   -> all owned reversible effects unwound
   -> no service/capability bindings remain from disposed fibers
```

The task should use deterministic fakes and adversarial tests. It does not need audio, FFmpeg, UI, async networking, dynamic libraries, configuration files, or HMR.

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
