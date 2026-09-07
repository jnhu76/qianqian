# Architecture overview

This document is the repository-local semantic overview for Qianqian Architecture v2.

Detailed generic composition semantics live in [`composition-kernel.md`](composition-kernel.md). The current design gate is GitHub issue **#53 COMPONENT-BOUNDARY-A0**.

## Architecture constitution

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

Architecture v2 also distinguishes:

> **Domain kernels own domain semantics.**
>
> **Capabilities expose contracts; providers own mechanisms.**
>
> **Fibers own plugin-instance lifetime.**
>
> **Effects own attributable mutation/recovery provenance.**
>
> **Profiles declare desired composition; Reconcile determines the running graph.**

But these are not permission to start by implementing a kernel API.

## Boundary-first architecture

The first architecture question is not “how should `ctx.effect()` look?”

It is:

> **How should the product be decomposed so that ownership, dependencies, interactions, ordering and recovery boundaries are explicit enough for composability to mean something?**

Current design order:

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

A feature name is not a component proof. `Decoder`, `AudioOutput`, `UI`, `DSP`, etc. remain candidate boundaries until the decomposition audit justifies their exact granularity.

## Component granularity

Component boundaries are judged by:

```text
ownership
requires/provides relationships
state/resource lifetime
operation locality
interaction ordering
recoverability boundary
configuration/naming/cognitive cost
```

An apparent cycle:

```text
A requires B
B requires A
```

is a signal to inspect whether a mediation/integration component should make the real one-way relationships explicit.

However, finer decomposition is not automatically better. A mathematically elegant graph can still be an engineering failure if it explodes component count/configuration/cognitive load.

## Capability and interaction boundary

Across plugin seams, consumers should depend on service/capability definitions rather than concrete providers.

```text
Capability Definition
        ^
   +----+----+
   |         |
Provider  Consumer
```

Cross-component interaction should not rely on arbitrary concrete references, hidden globals, undeclared cross-key mutation, or implicit startup order.

After capability resolution/binding, real payload normally flows directly through the service/data edge.

## Interaction algebra

A disposer/inverse is not sufficient evidence of independent composition.

For relationships expected to be independently removable, the shared operations and their inverse behavior must satisfy the relevant independence/commutativity contract.

Freeze:

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

The canonical player example is DSP ordering:

```text
EQ -> Compressor
```

is generally not equivalent to:

```text
Compressor -> EQ
```

Therefore registration timing, mount timing, map order or iteration order must never silently become DSP topology.

Where semantically valid, a contribution-oriented interface such as:

```text
register(value) -> opaque token
unregister(token)
```

can make independent ownership/removal explicit. It is not a trick for making genuinely ordered pipelines commutative.

## Observational equivalence

Correct removal/recovery is judged by observable contract behavior, not bit-identical private implementation state.

After removing A, the system should behave like a world where A never contributed, while independent B/C contributions still exist.

Opaque tokens, allocator layouts, internal generations and incidental IDs may differ.

## Control plane and data plane

```text
                         CONTROL PLANE

                 desired composition
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
```

> **Capability plane != Data plane.**

Context must not become a universal message bus, product-state bag, or audio-buffer transport.

## System boundary

`Everything is Plugin` does not mean `Everything is rollbackable`.

Effects/resources must be classified when relevant:

```text
Reversible
Transactional
Compensatable
Irreversible / emitted outside system boundary
```

Examples:

```text
listener/callback registration -> reversible
buffer/local handle            -> locally reversible
already-rendered sound         -> outside rollback boundary
```

The architecture must not promise an inverse for a real-world emission that cannot be undone.

## Global lifecycle

Provider disappearance is a global dependency-order problem, not merely a registry delete.

Required semantic shape:

```text
provider begins withdrawal
        ↓
provider stops satisfying new resolution
        ↓
dependents are invalidated and deactivate
        ↓
dependents finish required teardown
        ↓
provider finally removes/reclaims binding/resources
```

Do not destroy a provider first and let consumers discover failure later.

## Confluence

A major future correctness oracle is:

> **After any legal load/unload/replacement history reaches quiescence, the observable runtime is equivalent to a clean construction of the final desired composition.**

Example:

```text
load source A
insert EQ
switch output
remove EQ
replace decoder
settle
```

If the final desired graph is:

```text
source A + decoder B + output C
```

then the settled runtime should be observationally equivalent to a clean root directly composed as that final graph.

This tests far more than “did not crash”: it detects ghost bindings, stale lifecycle state, leaked contributions and history-dependent composition.

## Composition Kernel

After boundary design passes, the generic kernel is expected to remain centered on:

```text
Context
Capability
Fiber
Effect
Reconcile
```

It must remain domain-agnostic and know nothing about:

```text
Track
PCM
FFmpeg
WASAPI
PocketJS
KuiklyUI
playlist semantics
UI payload schemas
```

The exact implementation is **not yet authorized**; #53 must pass first.

## Everything is a Plugin

Every justified long-lived runtime capability should ultimately participate in the common composition/lifecycle protocol.

This does not imply:

```text
one feature == one plugin
one plugin == one crate
one plugin == one dynamic library
everything is hot reloadable
everything is rollbackable
```

Candidate boundaries such as Music, Decoder, DSP stages, AudioOutput, Presentation and UiHost remain subject to #53 granularity analysis.

## Music / media / UI

`MusicKernel` remains a music-domain semantic authority but is not the global composition authority.

The generic Composition Kernel is not the Audio Engine. A future `AudioRuntime` may own:

```text
AudioGraph
clock
buffer pool
format negotiation
RT scheduling
graph publication/swap
```

UiHost remains a normal plugin/capability candidate. Current platform intent:

```text
Windows   -> PocketJS
Linux     -> PocketJS
Android   -> KuiklyUI
iOS       -> KuiklyUI
HarmonyOS -> KuiklyUI
macOS     -> KuiklyUI candidate / replaceable
```

## Realtime boundary

Realtime audio is a data-plane island. Per callback/block it must not perform:

```text
Context lookup
capability resolution
Fiber reconciliation
arbitrary generic event dispatch
filesystem/network I/O
UI/JS/managed-runtime round trips
unbounded allocation/blocking
```

Future graph changes should be prepared on the control plane and published at an RT-safe boundary.

## Native media evidence

#48 remains valid:

```text
Decoder    : encoded media -> canonical PCM
Processing : PCM -> PCM
AudioOutput: canonical PCM -> physical device + evidence
```

When FFmpeg is reintroduced, Decoder/Processing must continue to share one FFmpeg closure authority rather than duplicating dependencies.

Logical component/plugin/capability boundary does not imply a separate crate/static library/shared library/dynamic library.

## Current code and gate

RUST-ARCH-R0 established:

```text
qianqian-core
qianqian-runtime
qianqian-headless
```

Current `qianqian-core::base`, `qianqian-runtime::AppRuntime`, `AppRuntime::new()` and direct capability fields are bootstrap witnesses, not compatibility contracts.

But the next step is **not** to replace them immediately with a Composition Kernel implementation.

Current authority chain is:

```text
#53 COMPONENT-BOUNDARY-A0
        ↓ PASS / CLOSED (component-boundary-a0.md)
#67 COMPOSITION-KERNEL-0 DESIGN — current gate
        ↓ proposed semantic authority: PR #68 (composition-kernel-0-design.md)
future COMPOSITION-KERNEL-0 IMPLEMENTATION (only after #67/PR #68 PASS + merge)
```

Until #67/PR #68 passes and merges, no new kernel implementation/API is authorized.

## Historical evidence

```text
archive/pre-rust-v2
pre-rust-v2
research/playback-reference-v1
playback-reference-v1
```

These are opt-in evidence sources, not current source-layout templates.
