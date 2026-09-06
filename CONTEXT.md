# CONTEXT.md

This file carries stable vocabulary and the repository mental model. It is not a substitute for current code, contracts, or task-specific evidence.

## Stable vocabulary

| Term | Meaning |
|---|---|
| Qianqian / 千千·现代 | A local-first, lightweight, cross-platform local music player and a testbed for composable runtime architecture. |
| Architecture v2 | The current architecture epoch: a Rust Composition Kernel controlling plugin reachability/ownership/lifetime, with domain/product capabilities implemented as plugins/services above it. |
| Composition Kernel | The generic control-plane kernel. It knows Context, Capability, Fiber, Effect, and Reconcile; it must not know music, PCM, UI payloads, FFmpeg, or platform-specific product semantics. |
| Context | The capability namespace/dependency view visible to a fiber. It resolves who can reach which active service/provider. It is not an application payload bus or global state bag. |
| Capability | A named/typed contract that can be provided and required. Capability identity is separate from the concrete provider implementation. |
| Service Definition | The domain-facing interface/contract represented by a capability. Consumers depend on the definition, not directly on a concrete provider across plugin boundaries. |
| Provider | A plugin/fiber that makes a capability/service available while ACTIVE. |
| Consumer | A plugin/fiber that declares a requirement on a capability/service. |
| Plugin | A component definition that participates in the common composition/lifecycle protocol. Plugin does not imply a dynamic library. |
| Fiber | A live plugin instance with identity, context/scope, requirements, provided capabilities, owned effects, and lifecycle state. The fiber—not the crate/package—is the runtime composition unit. |
| Effect | A kernel-owned record of a mutation/resource registration attributable to a fiber, with deterministic teardown/inverse where reversal is actually valid. |
| Reconcile | The process that moves the running fiber graph toward a desired plugin/profile composition while respecting dependency and lifecycle invariants. |
| Profile | Declarative desired composition for a product/platform configuration. A profile describes the plugin tree; it should not become an imperative privileged boot graph. |
| Control Plane | Composition data: profiles, reconciliation, fiber lifecycle, capability resolution, effect ownership, and dependency invalidation. |
| Data Plane | Real application payload flow after bindings exist: service calls, direct media/audio edges, domain events, streams, and other product data. |
| Capability plane != Data plane | Context decides who should be connected/visible; payload normally flows directly through the resolved service/data edge rather than through Context. |
| Independence | The property that one component/effect can be removed without damaging the observable contribution of another interleaved component/effect. Revertibility alone does not imply independence. |
| Commutativity | A property of shared operations and their inverses that allows independent ordering/removal. Same-key mutation is not assumed commutative; its provider/interface must establish the contract. |
| Non-commutative relation | An interaction whose behavior depends on order. It must be represented by explicit dependency/order/integration structure rather than mislabeled as an independent effect. |
| Observational Equivalence | Restoration correctness criterion: public behavior is equivalent to the world where the removed contribution never existed, even if private IDs/layout/generations are not bit-identical. |
| Operation locality | A capability/coeffect operation should read/write only the state represented by its declared shared key/contract. Hidden cross-key/global mutation breaks the composability model. |
| Recoverable system boundary | The state the runtime can legitimately own and restore/compensate. External or concurrently modified real-world state may require transactional, compensating, or irreversible semantics. |
| Domain Kernel | A semantic authority inside a domain plugin, such as `MusicKernel`. It owns domain meaning but is not the global composition authority. |
| Music Kernel | The music/player semantic authority: track/session/state, playback intent, queue behavior, buffering interpretation, recovery, and user-visible playback truth. It should be owned/exposed by a normal Music plugin. |
| Presentation | Domain-to-UI seam exposing stable UI-facing state/actions. Presentation is product/domain data, not a Composition Kernel primitive. |
| UiHost | UI rendering/input plugin/capability. It owns UI mechanism, not music semantics or realtime audio correctness. |
| Decoder | Encoded media -> canonical PCM mechanism/capability. |
| Processing | PCM -> PCM transformation such as bypass, SRC, gain, EQ, limiter, or other DSP. |
| AudioOutput | PCM -> physical device plus physical render/output evidence. |
| Audio Runtime | Future domain/runtime plugin that may own the realtime audio graph, clock, buffer policy, format negotiation, and RT scheduling. It is above the generic Composition Kernel. |
| Realtime island | The bounded audio hot path. It must use pre-bound direct data edges and must not resolve capabilities or reconcile fibers per callback/block. |
| Event Service | If product/domain events are needed, they are expected to live as a service/plugin unless the generic kernel proves it needs an event primitive. |
| Everything is a Plugin | Every long-lived product capability enters through the same Context/Capability/Fiber/Effect lifecycle protocol; it does not mean every component is dynamically loaded. |
| Reference Playback v1 | Frozen playback experiment proving a complete local-file -> decode -> PCM -> output path and important physical playback truths. |
| Pre-Rust archive | Complete repository state before Architecture v2 reset, preserved at `archive/pre-rust-v2` / `pre-rust-v2`. |
| Behavioral oracle | Historical verified behavior used to preserve playback correctness while allowing ownership, language, directories, and types to change. |

## Core mental model

The shortest version is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

And:

```text
Composition Kernel owns composition invariants.
Domain kernels own domain semantics.
Capabilities expose contracts.
Providers own mechanisms.
Fibers own lifetime.
Effects own reversible mutation provenance.
Profiles declare desired composition.
```

A second rule is equally important:

> **Inverse is not enough: independent removal also needs operation independence/commutativity, or explicit ordering when operations do not commute.**

Restoration is judged by **observational equivalence**, not irrelevant private bit identity.

Conceptually:

```text
                       CONTROL PLANE

             Profile / desired plugin tree
                         |
                         v
                    Reconcile
                         |
                         v
                    Fiber Graph
                         |
              provide / require / effect
                         |
                         v
                Composition Kernel
             Context / Capability / Fiber
                 Effect / Reconcile

-------------------------------------------------------------
                         |
                    bind/connect
                         v
                       DATA PLANE

   MediaSource -> Decoder -> Processing -> AudioOutput

   User/Input  -> Presentation/Domain Service -> Provider

   Domain event producer -> Event Service/listeners (when needed)
```

Context establishes reachability. It should not carry PCM blocks or become a universal message bus.

## Current code status

RUST-ARCH-R0 created a deliberately tiny bootstrap with:

```text
qianqian-core
qianqian-runtime
qianqian-headless
```

Current `qianqian-core::base` and `qianqian-runtime::AppRuntime` constructor composition are bootstrap witnesses, not compatibility contracts. The next Composition Kernel task may replace/refactor them to establish the Context/Capability/Fiber/Effect/Reconcile model.

The actual physical crate split must remain evidence-driven, although a dedicated dependency-pure Composition Kernel crate is a likely pressure because it provides a strong firewall against music/UI/platform concepts.

## Historical refs

The old repository tree is intentionally not the current architecture:

```text
archive/pre-rust-v2
pre-rust-v2
```

The validated playback specimen is independently preserved at:

```text
research/playback-reference-v1
playback-reference-v1
```

Use those refs as evidence when needed; do not treat their directory structure or ownership as the new default.
