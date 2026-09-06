# AGENTS.md

Qianqian is a local-first, lightweight, cross-platform music player and an architecture experiment in composable runtime design. This file defines repository-wide rules for coding agents. It is a governance entry point, not a feature manual.

## Start here

Before changing code or long-lived documentation:

1. Read the current issue or task.
2. Read `CONTEXT.md` for stable vocabulary.
3. Use `docs/README.md` to load only the minimum relevant documentation.
4. Read `docs/architecture/overview.md` before changing architecture boundaries.
5. For Composition Kernel work, also read `docs/architecture/composition-kernel.md`.
6. Audit the current repository before assuming a path, module, API, or build rule exists.

Do not recursively preload archived source or documentation.

## Architecture constitution

Architecture v2 is governed by these rules:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**
>
> **Domain kernels own domain semantics.**
>
> **Capabilities expose contracts; providers own mechanisms.**
>
> **Fibers own plugin-instance lifetime.**
>
> **Effects make kernel-visible mutation attributable and reversible where reversal is actually valid.**
>
> **Profiles declare desired composition; reconciliation determines the running fiber graph.**

And one important composability rule:

> **Inverse is not enough: independent removal also requires independence/commutativity, or explicit ordering when operations do not commute.**

Rust is the product architecture language.

The generic Composition Kernel must not know music, PCM, FFmpeg, WASAPI, PocketJS, KuiklyUI, tracks, playlists, or UI payload schemas. Product and platform behavior lives in plugins and domain services above it.

## Everything is a plugin

“Everything is a plugin” does **not** mean every component is a DLL, dynamically downloaded, or hot-loaded.

It means every long-lived product capability that participates in the runtime must enter through the same composition/lifecycle protocol:

```text
Plugin definition
      |
      v
Fiber instance
      |
      +-- requires Capabilities/Services
      +-- provides Capabilities/Services
      +-- owns Effects
      +-- participates in lifecycle/reconciliation
```

No product capability gets a privileged bypass merely because it is convenient to store directly in `AppRuntime`.

A dynamic library, runtime discovery, out-of-tree loading, or hot reload is an optional deployment feature, not the definition of a plugin.

## Composition Kernel primitive budget

The generic kernel is expected to stay centered on five concepts:

```text
Context
Capability
Fiber
Effect
Reconcile
```

`Service` is the domain-facing contract exposed through a capability; provider and consumer are roles around that contract, not reasons to add a second hidden registry.

Do not add a new kernel primitive without an architecture issue demonstrating that the existing calculus cannot express the required invariant cleanly.

The kernel should remain small even if the product grows large.

## Context is not a data bus

`Context` is the capability namespace and dependency view visible to a fiber. It answers questions such as:

```text
What capability is visible here?
Which active provider satisfies it?
Which fibers depend on that availability?
Which owner is responsible for the binding/effect?
```

It must not become a universal payload transport, global application state bag, message broker, or audio buffer store.

Remember:

> **Capability plane != data plane.**

After a consumer is bound to a provider, ordinary business payload should flow through the service contract or a pre-bound direct data edge.

For realtime audio:

```text
Decoder -> DSP/Processing -> AudioOutput
```

PCM must not traverse `Context` or a generic event bus per block/callback.

## Service definition and provider separation

Consumers depend on service/capability definitions, not concrete providers across plugin boundaries.

Preferred conceptual shape:

```text
Service Definition
      ^
      +-- Provider A
      +-- Provider B
      +-- Consumer
```

Avoid topology such as:

```text
MusicPlugin -> WasapiOutput
UiPlugin    -> ConcreteMusicKernel
```

when the dependency is semantically on an abstract capability.

Direct imports are fine inside one cohesive implementation; the prohibition applies to architectural plugin seams.

## Fiber lifecycle

The runtime unit of composition is a **Fiber**, not a crate, source module, package, or dynamic library.

A fiber owns at least:

```text
identity
parent/scope
requirements
provided capabilities
context view
effects
lifecycle state
```

The target lifecycle semantics include states equivalent to:

```text
PENDING -> LOADING -> ACTIVE -> UNLOADING -> PENDING/DISPOSED
                         \
                          -> FAILED (when activation fails)
```

Exact Rust enum names are implementation details. The invariant is more important: a component is active only while its required capabilities are satisfied, and dependency loss must drive lifecycle change rather than producing a later null-service failure.

Provider teardown must respect dependency ordering: stop advertising availability, let dependents leave the active state, then release the provider binding/resources.

## Effect boundary

Kernel-visible mutation must have an explicit owner.

Typical reversible effects include:

```text
service/capability binding
listener registration
timer registration
child fiber mount
local watcher/handle registration
other local runtime registrations
```

A fiber must not rely on a separately handwritten “remember to undo everything” shutdown path when the mutation can be represented as an owned effect.

Within one Fiber, owned effects normally unwind in deterministic reverse/LIFO order.

Do not pretend every side effect is reversible. External writes such as network requests, irreversible filesystem mutation, money movement, or other non-local actions need explicit transactional/compensating/irreversible semantics when they eventually exist. `Effect` is not magic rollback.

## Independent removal and shared operations

A disposer/inverse proves only local revertibility. It does **not** automatically prove that a Fiber can be removed after other Fibers have modified shared state.

For cross-Fiber composability, review the shared-operation contract.

### Different keys

Operations on distinct capability/coeffect keys should be local: they must not secretly read/write unrelated keys or hidden global shared state.

### Same key

When multiple Fibers mutate one shared key, do not assume the operations commute.

The provider/service definition must establish how contributions compose and how one caller's inverse removes only that caller's contribution.

Prefer contribution-oriented interfaces when semantically appropriate, for example:

```text
register(value) -> opaque token
unregister(token)
```

### Non-commutative interactions

If order changes observable behavior, represent that order explicitly through dependency/composition/integration structure.

Do not hide ordered middleware/pipeline semantics behind a false “independent effects” abstraction.

### Restoration oracle

Judge restoration by **observational equivalence** through public contracts, not by irrelevant bit-for-bit identity of private IDs/layout/generations.

Kernel tests must separately distinguish:

```text
single-Fiber LIFO cleanup
cross-Fiber independence
same-key contribution safety
explicit handling of non-commutative order
```

## Reconciliation

Composition must not silently collapse back into a giant imperative `boot()` function.

The intended control flow is:

```text
desired plugin tree / profile
          |
          v
      Reconcile
          |
          v
   running Fiber graph
```

The initial implementation may be deliberately small, but product composition should evolve through the same protocol rather than hard-coding privileged product services into bootstrap runtime.

## Events are not a generic kernel payload primitive

Application events may be useful, but a general event bus is not automatically part of the Composition Kernel.

If a product/domain needs event semantics, model the event facility as a service/plugin unless evidence shows the generic kernel itself requires a primitive.

Listener registration and ownership can still be managed by Effects.

Internal dependency invalidation inside the kernel is not the same thing as a public application event bus.

## Domain kernels and product semantics

A domain kernel such as `MusicKernel` may own product semantics including track/session/state, play/pause/seek meaning, queue policy, buffering interpretation, recovery, and ENDED semantics.

A domain kernel is **not** the composition authority of the whole application. It should live behind/inside an ordinary plugin and expose domain capabilities/services like any other product component.

Mechanism layers provide facts/evidence. Domain semantic owners interpret those facts.

## Realtime boundary

The audio realtime path is a data-plane mechanism island.

Do not perform per-period/callback:

```text
Context lookup
capability resolution
fiber reconciliation
arbitrary event dispatch
filesystem/network I/O
UI/JS/managed-runtime round trips
unbounded allocation/blocking
```

Composition changes must be prepared on a control thread/control plane and published to realtime execution at a bounded safe boundary when realtime graph work is introduced.

## UI boundary

UI is an ordinary plugin/capability consumer/provider, not an architecture authority.

Current platform intent remains:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI UiHost by default, replaceable by composition/profile
```

A UiHost consumes presentation/domain services. It must not become the owner of music semantics or participate in realtime audio correctness.

## R0 bootstrap code is provisional

RUST-ARCH-R0 intentionally introduced minimal witnesses such as `qianqian-core::base` and `qianqian-runtime::AppRuntime` static composition.

Those shapes are **not compatibility contracts** and are not authoritative evidence that Architecture v2 should remain constructor-only composition.

The next Composition Kernel task is explicitly allowed to refactor/delete/replace those R0 witnesses to establish the Context/Capability/Fiber/Effect/Reconcile model.

Do not preserve an R0 API merely because it already exists.

## Work mode

Use reality-first development:

- inspect before designing around assumed files or APIs;
- make the smallest implementation that proves the requested invariant;
- use adversarial tests for lifecycle, dependency removal, rollback/unwind, stale state, independence, and ordering;
- do not create parallel registries, service locators, DI containers, or hidden global state beside the Composition Kernel;
- do not move domain payloads into Context for convenience;
- do not treat a disposer as proof of cross-Fiber composability;
- do not perform unrelated cleanup in the same task.

When a task conflicts with repository reality, classify the mismatch and either make the smallest authorized corrective or stop with evidence.

## Historical evidence

The pre-Rust repository is preserved at:

```text
archive/pre-rust-v2
pre-rust-v2
```

The validated playback experiment is preserved at:

```text
research/playback-reference-v1
playback-reference-v1
```

These refs are historical/reference evidence, not current source-layout or composition authority.

## Verification

Choose verification from the actual changed surface.

For Composition Kernel changes, tests should prefer observable invariants such as:

```text
consumer pending before provider exists
provider arrival activates dependent
provider loss deactivates dependent before provider teardown completes
replacement provider can reactivate dependent
fiber disposal unwinds owned effects
independent Y contribution survives removal of X-side fiber
root disposal is observationally equivalent to a clean root
```

Report what was actually verified and never label an unrun platform/device check as PASS.

## Documentation

`docs/README.md` is the documentation router. Keep durable facts in one clear authority and link instead of duplicating them.

## Local AGENTS policy

There are no local `AGENTS.md` files by default.

Create one only when a directory has a genuine stable local rule that cannot be expressed cleanly by the root rules and the current task explicitly justifies it.

## Delivery discipline

Keep commits and PRs focused. State scope and non-scope. Do not automatically continue into the next architecture phase after the current acceptance gate passes.
