# Architecture overview

This document is the repository-local semantic overview for Qianqian Architecture v2.

It describes ownership and dependency direction. Detailed generic composition semantics live in [`composition-kernel.md`](composition-kernel.md).

## Architecture constitution

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

Architecture v2 further distinguishes:

> **Composition Kernel owns composition invariants.**
>
> **Domain kernels own domain semantics.**
>
> **Capabilities expose contracts; providers own mechanisms.**
>
> **Fibers own plugin-instance lifetime.**
>
> **Effects own reversible mutation provenance.**
>
> **Profiles declare desired composition; reconciliation determines the running graph.**

Cross-Fiber removal adds one more rule:

> **A disposer/inverse is not sufficient evidence of composability; independent removal also needs independence/commutativity, or explicit order when operations do not commute.**

Rust is the product architecture language.

Native/platform/UI technologies are implementation substrates and plugins above the generic Composition Kernel.

## Control plane and data plane

Architecture v2 separates composition/control from application data flow.

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
               Composition Kernel
            Context / Capability / Fiber
                Effect / Reconcile

-------------------------------------------------------------
                          |
                    resolve / bind
                          v
                         DATA PLANE

      service.method(payload) -------> provider

      MediaSource -> Decoder -> Processing -> AudioOutput

      domain event -> Event Service/listeners (when needed)
```

The control plane decides **who can reach whom, who owns a change, and who should be alive**.

The data plane carries **what the product is actually processing**.

Therefore:

> **Capability plane != Data plane.**

Context must not become a universal message bus, product-state bag, or audio-buffer transport.

## Composition Kernel

The generic kernel is centered on five concepts:

```text
Context
Capability
Fiber
Effect
Reconcile
```

It must remain domain-agnostic. It does not know:

```text
Track
PCM
Decoder internals
FFmpeg
WASAPI
PocketJS
KuiklyUI
playlist semantics
UI payload schemas
```

The Composition Kernel establishes:

- capability/service visibility and binding;
- provider/consumer dependency relationships;
- plugin-instance lifecycle through fibers;
- mutation/resource ownership through effects;
- desired-vs-running composition reconciliation.

See [`composition-kernel.md`](composition-kernel.md) for the detailed model and invariants.

## Everything is a plugin

Every long-lived product capability should enter the runtime through the same composition/lifecycle protocol.

```text
Plugin definition
      |
      v
Fiber instance
      |
      +-- requires Capability A/B
      +-- provides Capability C
      +-- owns Effects
      +-- activates/deactivates with dependency availability
```

This does not require one crate, process, dynamic library, or downloaded package per plugin.

The runtime composition unit is the **Fiber**, not the source package.

No product capability should become globally privileged merely because it was historically constructed directly inside `AppRuntime`.

## Service definition, provider, consumer

A capability/service definition is distinct from its provider.

```text
              AudioOutput definition
                    ^
          +---------+----------+
          |                    |
     Wasapi provider      PipeWire provider
          ^                    ^
          +---------+----------+
                    |
               Consumer
```

Across plugin boundaries, consumers should depend on definitions/capabilities rather than concrete providers.

This preserves replaceability without requiring dynamic loading.

## Domain kernels

A domain kernel is the semantic authority for one domain but is an ordinary resident above the Composition Kernel.

### Music Kernel

`MusicKernel` owns music/player semantics as they are implemented over time:

- current track and playback session;
- play/pause/stop intent;
- seek semantics;
- user-visible playback state and position;
- queue/playlist/repeat/shuffle policy;
- track transitions;
- buffering interpretation;
- recovery and product-level error meaning;
- ENDED semantics.

Conceptually it is owned by a Music plugin:

```text
Music Plugin
   |
   +-- owns MusicKernel
   +-- requires media/audio capabilities
   +-- provides transport/player-state/domain services
```

It is **not** the global composition authority.

Mechanism layers produce facts/evidence. Domain semantic owners decide product meaning.

## Presentation and UiHost

Presentation is a domain/product seam, not a generic kernel primitive.

```text
Music/domain semantics
        |
   Presentation
        |
     UiHost
```

`UiHost` is an ordinary plugin/capability for rendering/input adaptation.

Current direction:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI UiHost by default, replaceable by composition/profile
```

UI frameworks do not own music semantics and do not participate in realtime audio correctness.

## Media and audio data plane

Decoder, Processing, and AudioOutput remain logical media capabilities:

```text
Decoder    encoded media -> canonical PCM
Processing canonical PCM -> canonical PCM
AudioOutput canonical PCM -> physical device + output evidence
```

But PCM is **data-plane data**.

Do not design:

```text
Decoder -> Context/EventBus -> Processing -> Context/EventBus -> Output
```

The eventual audio graph should use pre-bound direct edges:

```text
MediaSource -> Decoder -> Processing -> AudioOutput
```

When FFmpeg is reintroduced, Decoder and Processing should share one FFmpeg closure authority rather than duplicate dependency closure.

The canonical PCM contract must be established by media implementation work, not by the generic Composition Kernel.

## Audio Runtime specialization

The generic kernel should not grow audio-specific scheduling or format concepts.

A future `AudioRuntime` is expected to be a normal domain/runtime plugin that may own:

```text
AudioGraph
clock
buffer pool
format negotiation
realtime scheduling
graph publication/swap policy
```

This allows different implementations—realtime desktop, offline renderer, web/AudioWorklet, etc.—without contaminating the generic Composition Kernel.

## Realtime boundary

The audio realtime path is a data-plane mechanism island.

Per callback/block it must not perform:

```text
Context lookup
capability resolution
fiber reconciliation
arbitrary generic event dispatch
filesystem/network I/O
UI/JS/managed-runtime round trips
unbounded allocation/blocking
```

When runtime graph mutation exists, the expected pattern is:

```text
Control plane builds/prepares Graph B
          |
          v
validate / acquire resources
          |
          v
publish at RT-safe boundary
          |
          v
Audio thread switches Graph A -> Graph B
          |
          v
retire/reclaim Graph A off realtime path
```

The exact mechanism is not frozen yet.

## Events

A generic application event bus is not automatically a Composition Kernel primitive.

If a domain needs event semantics, it may provide an Event Service/plugin. Listener registrations can still be owned/unwound through Effects.

Kernel-internal dependency invalidation is an implementation concern of capability/fiber lifecycle and should not be confused with a public product event bus.

## Effects, independence, and restoration

Effects are the kernel write/ownership boundary for reversible local runtime mutation.

Typical examples:

```text
service/capability binding
listener registration
timer registration
child fiber mount
watcher/local handle registration
```

Within one Fiber, owned effects normally unwind in reverse/LIFO order.

That does **not** prove cross-Fiber independent removal. When effects from multiple Fibers interleave, the shared operations must satisfy the relevant independence/commutativity contract, or the interaction must carry explicit ordering/dependency semantics.

Useful interface shape:

```text
register(value) -> opaque token
unregister(token)
```

where each caller removes only its own contribution.

An ordered middleware/pipeline interaction that changes behavior under reordering is not an independent effect and must be modeled as ordered composition.

Restoration is judged by **observational equivalence** through public contracts rather than bit-for-bit restoration of incidental private state.

Not every external action is reversible. Transactional, compensating, or irreversible effects require explicit semantics when such behavior is introduced.

## Reconciliation

Profiles describe desired composition rather than hard-coding a privileged imperative boot graph.

```text
desired plugin tree
        |
        v
    Reconcile
        |
        v
 running Fiber graph
```

The reconciler is responsible for mount/unmount/update decisions consistent with dependency/lifecycle rules.

Early versions may support only a minimal subset, but the architecture should not silently regress into a monolithic `boot()` function that bypasses the plugin protocol.

## Current R0 implementation and migration

RUST-ARCH-R0 established three packages:

```text
qianqian-core
qianqian-runtime
qianqian-headless
```

The current `qianqian-core::base` module and `qianqian-runtime::AppRuntime` constructor-only composition are bootstrap witnesses.

They are **not compatibility contracts**.

The next Composition Kernel task is authorized to refactor/delete/replace those R0 shapes to establish the Context/Capability/Fiber/Effect/Reconcile model.

A dedicated dependency-pure kernel crate is a likely design because it creates a strong firewall against music/media/UI concepts, but the physical split should still be justified by the implementation task rather than treated as a diagram requirement.

## Historical evidence

Complete pre-Rust tree:

```text
archive/pre-rust-v2
pre-rust-v2
```

Frozen playback reference:

```text
research/playback-reference-v1
playback-reference-v1
```

The playback reference is a behavioral oracle, not a source-layout template. It proved a complete Windows playback path and important physical truths around decode/queue/submit/render, seek/flush, stale rejection, timeline, and physical drain.

Architecture v2 may change language, ownership, directories, types, and composition while preserving those verified behaviors when functionality is rebuilt.

## Non-goals

The architecture does not require:

- every plugin to be dynamically loaded;
- one crate/binary per capability;
- Context to transport all application data;
- a universal global event bus;
- audio-specific types in the generic kernel;
- UI frameworks in the generic kernel;
- restoring the old repository hierarchy;
- treating a disposer as proof of cross-Fiber composability;
- pretending irreversible external actions can always be rolled back.

Implementation should grow one verified invariant at a time.
