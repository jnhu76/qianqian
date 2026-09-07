# CONTEXT.md

This file carries stable vocabulary and the repository mental model. It is not a substitute for current code, contracts, or task-specific evidence.

## Stable vocabulary

| Term | Meaning |
|---|---|
| Qianqian / 千千·现代 | A local-first, lightweight, cross-platform music player and a testbed for composable runtime architecture. |
| Architecture v2 | Boundary-first plugin architecture: component/capability/interaction boundaries are designed first; a Rust Composition Kernel later controls reachability, ownership and lifetime. |
| Boundary-first design | The rule that component granularity, capability dependencies, interaction algebra, effect/system boundary, lifecycle ordering, and confluence are decided before kernel API/runtime machinery. |
| Component Granularity | The architecture decision of what deserves to be one runtime component/plugin. Feature names do not determine granularity; ownership, dependencies, interaction semantics, ordering, and cognitive cost do. |
| Integration / Mediation Component | A component introduced to make a real ordering or cross-component relationship explicit, especially when an apparent bidirectional dependency should be decomposed into clearer one-way bindings. |
| Composition Kernel | The generic control-plane kernel. It may know Context, Capability, Fiber, Effect, and Reconcile; it must not know music, PCM, UI payloads, FFmpeg, or platform product semantics. |
| Context | The capability namespace/dependency view visible to a Fiber. It controls reachability/availability; it is not an application payload bus or global state bag. |
| Capability | A named/typed service contract that can be required/provided. Capability identity is distinct from concrete provider implementation. |
| Service Definition | The domain-facing interface/contract represented by a capability. Consumers depend on the definition across plugin seams. |
| Provider | A plugin/Fiber that makes a capability/service available while it satisfies the lifecycle contract. |
| Consumer | A plugin/Fiber declaring a requirement on a capability/service. |
| Plugin | A component definition participating in the common composition/lifecycle protocol after its boundary has been justified. Plugin does not imply dynamic library. |
| Fiber | A live plugin instance with identity, scope, requirements, provided capabilities, owned effects, and lifecycle state. It is the runtime composition unit, not the crate/package. |
| Effect | Kernel-visible mutation/resource provenance owned by a Fiber, with inverse/teardown where reversal is valid. An inverse alone does not prove cross-Fiber composability. |
| Reconcile | Moves the running Fiber graph toward desired composition while respecting dependency/lifecycle invariants. It is an implementation mechanism used after boundary design. |
| Profile | Desired product/platform composition; eventually declares the plugin graph rather than a privileged imperative boot sequence. |
| Control Plane | Composition concerns: desired graph, capability resolution, Fiber lifecycle, effect ownership, dependency invalidation, reconcile. |
| Data Plane | Product payload flow after binding: service calls, media/audio graph edges, domain events/streams, UI/domain payloads. |
| Capability plane != Data plane | Context determines who can reach whom; payload normally flows directly through the resolved service/data edge. |
| Operation Locality | An operation on one shared key/capability should only read/write the state represented by that contract. Hidden cross-key/global mutation breaks the model. |
| Independence | One component's removal preserves other independent components' observable contributions. Revertibility alone does not imply independence. |
| Commutativity | Shared operations and their inverses can be reordered without changing relevant observable behavior. Same-key mutation is not assumed commutative. |
| Interaction Algebra | Classification of cross-component operations as independently composable/commutative versus order-sensitive/non-commutative, together with the explicit ordering structure for the latter. |
| Non-commutative Relation | An interaction whose observable behavior depends on order. It must use explicit dependency/order/integration structure, not implicit registration order. |
| Contribution-oriented Interface | An interface where callers receive an opaque handle/token for their own contribution and can remove only that contribution, when this matches the semantics. |
| Observational Equivalence | Correctness criterion comparing public/relevant behavior rather than bit-identical private state. |
| Recoverable System Boundary | State/resources the runtime can legitimately own and restore/transactionally manage/compensate. Emissions outside the boundary may be irreversible. |
| Confluence | After a legal sequence of composition changes reaches quiescence, the observable result is equivalent to a clean build of the final desired composition. |
| Quiescence | A point at which relevant lifecycle/reconciliation transitions have settled, allowing confluence comparison. |
| Domain Kernel | A semantic authority inside a domain component/plugin, such as `MusicKernel`; not the global composition authority. |
| Music Kernel | The music/player semantic authority for track/session/playback/queue/buffering/recovery/ENDED meaning; expected to live inside/behind a justified Music component. |
| Presentation | Domain-to-UI seam. Product/domain data, not a generic Composition Kernel primitive. |
| UiHost | UI rendering/input capability/plugin candidate. Owns UI mechanism, not music semantics or realtime correctness. |
| Decoder | Encoded media -> canonical PCM mechanism/capability candidate; exact plugin granularity remains subject to boundary audit. |
| Processing / DSP | PCM -> PCM transforms. Individual EQ/Gain/Resampler/Mixer granularity and order are boundary-design questions, not assumed independent plugins. |
| AudioOutput | PCM -> physical output + physical evidence; device discovery/session/renderer granularity remains subject to boundary audit. |
| Audio Runtime | Future specialized plugin/runtime that may own AudioGraph, clock, buffers, format negotiation, RT scheduling, and graph publication. |
| Realtime island | Bounded audio hot path using pre-bound data edges; no per-block generic Context resolution/reconcile. |
| Event Service | Product/domain event semantics, if needed, should begin as a normal service/plugin rather than an automatic generic-kernel primitive. |
| Everything is a Plugin | All justified long-lived runtime capabilities ultimately obey a common composition/lifecycle protocol; it does not mean every feature name is exactly one plugin or every component is dynamically loaded. |
| Reference Playback v1 | Frozen playback experiment proving a local-file -> decode -> PCM -> physical output path and important playback truths. |
| Pre-Rust archive | Complete repository state before Architecture v2 reset: `archive/pre-rust-v2` / `pre-rust-v2`. |
| Behavioral oracle | Historical verified behavior used to preserve playback correctness while architecture changes. |

## Core mental model

The shortest constitution is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

But the design process starts **before** the Kernel:

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

#53 COMPONENT-BOUNDARY-A0 is PASS/CLOSED (`component-boundary-a0.md`). The #67 COMPOSITION-KERNEL-0 semantic design is MERGED (PR #68, `composition-kernel-0-design.md`). Current gate: the **pre-implementation review (Corrective-4)**.

No new generic kernel implementation/API is authoritative until that review is accepted and a separate implementation issue is opened.

## Interaction mental model

For independently composable relationships:

```text
commutative operations
        ↓
independent contributions
        ↓
independent removal
```

For order-sensitive relationships:

```text
A -> B != B -> A
        ↓
explicit order / dependency / integration component
```

Do not use registration timing, container iteration, or mount order as hidden product semantics.

## Control plane / data plane

```text
                      CONTROL PLANE

             desired component graph
                       |
                       v
                Composition Kernel
             Context / Capability / Fiber
                 Effect / Reconcile

-------------------------------------------------
                       |
                  resolve/bind
                       v
                     DATA PLANE

   MediaSource -> Decoder -> DSP/Processing -> AudioOutput

   User/Input -> domain/presentation service -> provider
```

Context establishes reachability. It does not carry PCM blocks.

## Lifecycle mental model

Provider removal is not “delete binding, then see who fails”.

```text
provider starts withdrawal
        ↓
no new resolution sees it as available
        ↓
dependents deactivate / tear down
        ↓
provider releases final binding/resources
```

The implementation may refine this sequence, but dependent safety is the invariant.

## Confluence mental model

Given a legal history:

```text
load A
insert B
replace C
remove B
settle
```

compare the settled runtime to:

```text
clean root
  ↓
build final desired graph directly
```

Relevant observable truth should match even if private IDs/tokens/layout differ.

## Current code status

RUST-ARCH-R0 created:

```text
qianqian-core
qianqian-runtime
qianqian-headless
```

`qianqian-core::base` and `qianqian-runtime::AppRuntime` constructor composition are bootstrap witnesses, not compatibility contracts.

They may later be replaced through the future implementation issue; a generic kernel must not live inside the product core (dependency direction: generic kernel ← product semantics).

## Historical refs

```text
archive/pre-rust-v2
pre-rust-v2

research/playback-reference-v1
playback-reference-v1
```

Use these as opt-in evidence, never as automatic source-layout/ownership authority.
