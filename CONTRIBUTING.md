# Contributing to Qianqian

Qianqian is rebuilding on Architecture v2 as a boundary-first plugin architecture with a small generic Rust Composition Kernel.

## Before you start

Read:

1. the current issue/task;
2. `AGENTS.md`;
3. `CONTEXT.md`;
4. the minimum relevant documents selected through `docs/README.md`.

For plugin/composition work, read `docs/architecture/composition-kernel.md`.

Gate status: #53 COMPONENT-BOUNDARY-A0 is PASS/CLOSED (PR #66 merged); the #67 COMPOSITION-KERNEL-0 semantic design is merged (PR #68). The Composition Kernel (K0) is IMPLEMENTED (PR #71, 70 kernel tests / 75 workspace tests, 743eb86).

Do not recursively preload historical docs or use `archive/pre-rust-v2` as current architecture authority.

## Boundary-first contribution rule

Do not start architecture work by inventing `Context`, `Fiber`, `Effect`, registry, loader, or Reconcile APIs.

The required order is:

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

Until the pre-implementation review is accepted and a separate implementation issue is opened, implementation of a new generic kernel runtime is out of scope.

## Component entry checklist

Before proposing a long-lived plugin/component, answer:

```text
What state/resources does it own?
What capabilities does it require?
What capabilities does it provide?
What operations/data edges cross the boundary?
What information is intentionally observable?
Which shared operations commute?
Where is order explicit when they do not commute?
Which effects are reversible/transactional/compensatable/irreversible?
Which consumers must deactivate before provider teardown?
Does finer granularity justify the extra configuration/naming/cognitive cost?
```

A different feature name is not evidence that something deserves a separate plugin.

If A/B appear mutually dependent, audit whether an integration/mediation component should expose the real one-way relations. Do not accept cycles casually, but do not fragment the product endlessly merely to obtain a prettier graph.

## Architecture model

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

The generic Composition Kernel stays centered on:

```text
Context
Capability
Fiber
Effect
Reconcile
```

Domain/product components own:

```text
music semantics
media/audio behavior
UI behavior
library/product behavior
service payload schemas
```

The five primitive names are a mechanism budget, not permission to add new primitives without reviewed pressure.

## Service/provider separation

Consumers should depend on capability/service definitions rather than concrete provider implementations across plugin boundaries.

```text
Capability Definition
        ^
   +----+----+
   |         |
Provider  Consumer
```

Do not hard-wire `Music -> WasapiOutput` when the semantic dependency is on an AudioOutput contract.

## Context and data flow

Remember:

> **Capability plane != Data plane.**

Context resolves/binds capabilities. Real application payload normally flows through the resolved service or a direct/pre-bound data edge.

Do not route PCM blocks, realtime buffers, UI payloads, or arbitrary product messages through Context.

## Interaction algebra

A disposer/inverse proves only local revertibility.

Cross-component independent removal additionally requires the shared operations to satisfy the relevant independence/commutativity properties.

Freeze:

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

For same-key contribution-style interfaces, designs such as:

```text
register(value) -> opaque token
unregister(token)
```

may be useful when they genuinely match the semantics.

Do not use this pattern to disguise an intrinsically ordered pipeline. DSP topology is a required adversarial example.

## Effects and system boundary

Kernel-visible local mutation/resources should have explicit ownership.

Do not claim every effect is rollbackable. Distinguish when relevant:

```text
Reversible
Transactional
Compensatable
Irreversible / outside system boundary
```

Already-rendered audio, for example, cannot be undone.

These labels are a system-boundary/action taxonomy for reasoning about actions — not runtime variants of a kernel Effect type. The frozen K0 Effect has exactly one shape: a reversible composition-lifecycle mutation with a total inverse (`docs/architecture/composition-kernel-0-design.md` §H.5/§H.7). Do not implement an Effect-class enum.

Restoration correctness uses observational equivalence through public/relevant contracts rather than private bit identity.

## Provider disappearance

Provider teardown must preserve dependency ordering:

```text
provider starts withdrawal
        ↓
no new resolution sees it as available
        ↓
dependents deactivate / teardown
        ↓
provider finally removes/reclaims binding/resources
```

Do not destroy a provider first and let dependents fail later.

## Confluence

Architecture work should preserve a future test oracle:

> **After any legal load/unload/replacement history reaches quiescence, the observable runtime is equivalent to a clean construction of the final desired composition.**

This should detect history-dependent state, ghost bindings and leaked contributions—not only crashes or handle leaks.

## Focused implementation

Prefer the smallest cohesive change that proves the current requested invariant.

Do not combine unrelated cleanup, platform work, UI redesign, media integration and kernel evolution in one PR.

Do not create:

- hidden global product state;
- arbitrary concrete cross-plugin references;
- undeclared cross-key mutation;
- implicit semantic order from startup/registration/map iteration;
- a universal Context payload/message bus;
- dynamic-library infrastructure merely to satisfy the word “plugin”;
- many empty crates/modules without ownership/dependency evidence;
- local `AGENTS.md` files without genuine local divergence.

## R0 compatibility policy

RUST-ARCH-R0 APIs are bootstrap witnesses.

Historical R0 names are intentionally preserved here as history:

```text
qianqian-core::base
AppRuntime::new()
with_audio_output()
```

Those historical shapes were not compatibility contracts. Their current canonical successors live under `qianqian-audio-api`, `QianqianApp`, and the current Composition Kernel vocabulary; this does not rewrite what R0 was called at the time.

They could be redesigned as the Composition Kernel (K0) implementation (PR #71) replaced the R0 bootstrap shapes.

## Verification

Verification must match the phase.

### Local pre-push gate (Lefthook)

Lefthook is the repository Git hook orchestrator. Its `pre-push` gate is the fast local rejection layer: it proves the working tree passes fast deterministic checks and validates the commit headers ahead of the push base. It is developer-side policy, not repository authority — GitHub Actions remains the authority for platform, formal (TLA+), Miri/Loom, mutation, Windows and release evidence.

Install once per clone (Lefthook 2.1.16 or newer; binary only, no root Node package):

```bash
# 1. install the lefthook binary:
#    Linux:   https://github.com/evilmartians/lefthook/releases
#             (standalone binary; or the Cloudsmith apt repo)
#    macOS:   brew install lefthook
#    Arch:    yay -S lefthook-bin
#    Windows: download the Windows binary from the releases page
#             (python3 must also be on PATH for the python checks)
lefthook install

# 2. only needed once, and only for docs/website pushes:
cd website && npm ci && cd ..
```

Before every push (this is the canonical manual entrypoint for humans and coding agents):

```bash
lefthook run pre-push --all-files
```

The gate is checks-only: no formatting, no auto-fixing, no staging, no commits, no network. A failed pre-push leaves the working tree unchanged. What stays CI-only: `specs/check.sh current|rust`, the Windows compile gate, the VitePress build, OpenCodeReview.

### Boundary-design work

Evidence is architectural analysis, matrices, graphs, adversarial cases and explicit unresolved blockers—not green Cargo tests.

### Later Composition Kernel work

Tests must distinguish:

```text
single-Fiber local cleanup
cross-Fiber independent removal
same-key contribution safety
explicit non-commutative ordering
provider-disappearance ordering
history-vs-clean-build confluence
```

### Media/realtime work

Requires direct data-plane/realtime-safety evidence. Physical-device claims require physical-device validation.

Never report an unrun platform/device check as PASS.

## Documentation

Keep durable documentation small and authoritative.

Use `docs/README.md` as the router. Composition guardrails: `docs/architecture/composition-kernel.md` (derived summary); the K0 semantic authority is `docs/architecture/composition-kernel-0-design.md`. Current canonical vocabulary and first earned static playback composition are authoritative in `docs/adr/ADR-PBK-002.md`.

## Historical code reuse

The old repository and playback experiment may be inspected for behavior, measurements, algorithms, or proven mechanism code.

Do not copy an old component into `main` merely because it already exists. Reuse must fit the current boundary/ownership/interaction model.

## PR expectations

A PR should explain:

- what changed;
- which invariant/design fact it establishes;
- verification/evidence performed;
- architecture/dependency impact;
- explicit non-scope;
- unresolved blockers/manual validation.

STOP at the current gate. Do not automatically continue into the next milestone.
