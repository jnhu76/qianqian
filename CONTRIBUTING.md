# Contributing to Qianqian

Qianqian is rebuilding on Architecture v2 around a generic Rust Composition Kernel and domain/product plugins.

## Before you start

Read:

1. the current issue or task;
2. `AGENTS.md`;
3. `CONTEXT.md`;
4. the minimum relevant documents selected through `docs/README.md`.

For Composition Kernel work, read `docs/architecture/composition-kernel.md` before coding.

Do not recursively preload historical docs or use `archive/pre-rust-v2` as current architecture authority.

## Issue-first changes

Architecture, kernel, capability/service, plugin lifecycle, platform, media, UI-host, and cross-layer contract changes should have a clearly scoped issue/task before implementation.

A focused task should state:

- the invariant or behavior being established;
- what is explicitly out of scope;
- the evidence/verification required;
- the STOP gate before the next phase.

## Architecture model

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

The generic Composition Kernel is responsible for:

```text
Context
Capability
Fiber
Effect
Reconcile
```

Domain/product plugins own:

```text
music semantics
media/audio behavior
UI behavior
library/product behavior
service payload schemas
```

Do not put product payloads into Context merely because Context is globally reachable.

## Plugin entry checklist

For a long-lived product capability entering the runtime, be able to answer:

```text
What is the plugin definition?
What fiber instance owns its lifetime?
What capabilities/services does it require?
What capabilities/services does it provide?
What effects/resources does it own?
What disappears when the fiber unloads?
What should happen when a required provider disappears and later returns?
```

If those questions cannot be answered, the component is probably bypassing the composition model or the kernel contract is incomplete.

## Service/provider separation

Consumers should depend on capability/service definitions rather than concrete provider implementations across plugin boundaries.

Prefer:

```text
AudioOutput definition
    ^
    +-- Wasapi provider
    +-- PipeWire provider
    +-- Music/Audio consumer
```

Avoid hard-coded topology such as `Music -> WasapiOutput` when the semantic dependency is `Music -> AudioOutput`.

## Context and data flow

Remember:

```text
Capability plane != Data plane
```

Context resolves/binds capabilities. Ordinary payload flows through the resolved service or direct data edge.

Do not route PCM blocks, realtime buffers, UI payloads, or arbitrary product messages through Context.

A domain Event Service may exist when needed, but a general event bus is not automatically a Composition Kernel primitive.

## Effects and teardown

Kernel-visible reversible mutation should be owned by a fiber and represented through the Effect protocol rather than duplicated activate/deactivate bookkeeping.

Examples include service binding, listener registration, timers, child fibers, and local runtime handles.

Do not claim an external irreversible action is safely rollback-able merely because it is wrapped in an Effect. Use explicit transactional/compensating/irreversible semantics when that class of behavior appears.

## Focused implementation

Prefer the smallest cohesive change that proves the requested invariant.

Do not combine unrelated cleanup, speculative platform work, UI redesign, media integration, and kernel evolution in one PR.

In particular, do not create:

- a second registry beside Context;
- an unrelated DI/service-locator framework;
- hidden global product state;
- product-specific types inside the generic Composition Kernel;
- a universal Context payload/message bus;
- dynamic-library infrastructure merely to satisfy the word “plugin”;
- many empty crates/modules without a dependency or ownership reason;
- local `AGENTS.md` files without genuine local divergence.

## R0 compatibility policy

RUST-ARCH-R0 APIs were bootstrap witnesses.

`qianqian-core::base`, `AppRuntime::new()`, `with_audio_output()`, and similar R0 composition shapes may be removed or redesigned when implementing the Composition Kernel. Do not preserve them for compatibility unless a real current consumer makes that compatibility valuable.

## Tests and verification

Verification must match the changed surface.

Composition Kernel work should emphasize adversarial lifecycle tests, including:

```text
consumer pending when dependency is absent
provider arrival activates dependent
provider loss deactivates dependent before teardown finishes
replacement provider reactivates dependent
fiber disposal unwinds owned effects in deterministic order
root disposal leaves zero live bindings/fibers/effects
```

Media/realtime work additionally needs direct data-plane and realtime-safety evidence. Physical-device claims require physical-device validation.

Do not report an unrun platform/device check as PASS.

## Documentation

Keep durable documentation small and authoritative.

Use `docs/README.md` as the router. Add a new long-lived document only when the task creates a durable fact that needs a stable home.

Do not duplicate detailed kernel semantics across README, AGENTS, issues, and implementation comments; link to `docs/architecture/composition-kernel.md` for the canonical model.

## Historical code reuse

The old repository and playback experiment may be inspected for behavior, measurements, algorithms, or proven mechanism code.

Do not copy an old component into `main` merely because it already exists. Reuse must fit the current plugin/capability/lifecycle ownership model.

## PR expectations

A PR should explain:

- what changed;
- which invariant it establishes;
- verification performed;
- architecture/dependency impact;
- explicit non-scope;
- any remaining environmental/manual validation.

Do not automatically continue into the next milestone after the current task passes.
