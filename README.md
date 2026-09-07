# Qianqian / 千千·现代

Qianqian is a local-first, lightweight, cross-platform music player and a testbed for Rust composability/runtime architecture.

The repository is in **Architecture v2**. The first verified playback experiment was frozen, `main` was reset, and the new implementation is being rebuilt as a boundary-first plugin graph with a small generic Composition Kernel.

## Architecture in 30 seconds

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

But the project does **not** start by writing kernel APIs.

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

Gate status:

```text
#53 COMPONENT-BOUNDARY-A0    PASS / CLOSED (PR #66 merged)
#67 COMPOSITION-KERNEL-0     semantic design MERGED (PR #68)
Corrective-4 review          CURRENT PRE-IMPLEMENTATION GATE
implementation               NOT YET AUTHORIZED
```

Until the pre-implementation review (Corrective-4 on #67) is accepted and a separate implementation issue is opened, no new `qianqian-kernel`, Context API, Fiber lifecycle engine, or Reconcile implementation is authorized.

## Control plane and data plane

```text
                         CONTROL PLANE

                 desired composition
                          |
                          v
                 Composition Kernel
             Context / Capability / Fiber
                  Effect / Reconcile
                          |
                    resolve / bind
                          |
-------------------------------------------------------------
                          |
                         DATA PLANE
                          |
      MediaSource -> Decoder -> DSP -> AudioOutput

      input -> domain/presentation service -> provider
```

> **Capability plane != Data plane.**

Context controls reachability/dependency truth. It does not carry PCM blocks or become a universal product message bus.

## Everything is a Plugin

In Qianqian, “everything is a plugin” means every **justified** long-lived runtime capability ultimately participates in one common composition/lifecycle protocol.

It does **not** mean:

```text
one feature name == one plugin
one plugin == one crate
one plugin == one dynamic library
everything is hot-loaded
everything is rollbackable
```

The exact granularity of Music, Decoder, DSP, AudioOutput, Presentation, UiHost and other candidates is deliberately being audited before kernel implementation.

## Interaction correctness

A disposer/inverse is not enough to prove that independently mounted components can be removed safely.

Architecture v2 distinguishes:

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

DSP ordering is the obvious player example: `EQ -> Compressor` is generally not equivalent to `Compressor -> EQ`. Registration timing or container iteration must never become hidden semantic topology.

Restoration is judged by **observational equivalence**, not private bit-for-bit identity.

## Confluence target

A future core correctness oracle is:

> **After any legal load/unload/replacement history settles, the observable runtime is equivalent to a clean construction of the final desired composition.**

This is stronger than “no crash” or “all disposers ran”; it should catch ghost bindings, stale lifecycle state and history-dependent composition.

## System boundary

`Everything is Plugin` does not mean `Everything is rollbackable`.

Local registrations/handles may be reversible; external emissions may not be. Already-rendered sound cannot be “unplayed”. Architecture work must distinguish reversible, transactional, compensatable and irreversible/outside-boundary actions where relevant.

These labels are a system-boundary/action taxonomy for reasoning about actions — they are not runtime variants of a kernel Effect type. The Composition Kernel design freezes exactly one Effect shape: a reversible composition-lifecycle mutation with a total inverse (`docs/architecture/composition-kernel-0-design.md`).

## Domain semantics

`MusicKernel` remains the authority for music/player meaning, but it is a **domain kernel**, not the global composition kernel.

Its exact surrounding Music/Transport/Presentation component boundary is part of the current #53 audit rather than frozen prematurely.

## UI strategy

Current platform intent remains:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI candidate / replaceable
```

UiHost is a plugin/capability candidate. UI owns rendering/input mechanism, not music semantics or realtime correctness.

## Current implementation status

RUST-ARCH-R0 established the first compiling Rust workspace:

```text
qianqian-core
qianqian-runtime
qianqian-headless
```

R0 `base` and constructor-only `AppRuntime` composition are bootstrap witnesses, not compatibility contracts.

The decomposition they were waiting for is done (#53, closed) and the kernel semantic design is merged (PR #68). The future implementation is authorized to replace R0 bootstrap shapes rather than preserve them for compatibility — but only through the separate implementation issue, after the current pre-implementation review gate is accepted.

Build/test authority:

```bash
cargo run -p qianqian-headless
cargo test --workspace
```

## Historical preservation

Complete pre-Rust repository:

```text
branch: archive/pre-rust-v2
tag:    pre-rust-v2
```

Frozen playback reference:

```text
branch: research/playback-reference-v1
tag:    playback-reference-v1
```

The playback reference is a behavioral oracle, not a source-layout template.

## Repository entry points

- `AGENTS.md` — repository-wide agent governance and hard gates.
- `CONTEXT.md` — stable vocabulary and mental model.
- `docs/README.md` — task-oriented documentation router.
- `docs/architecture/overview.md` — Architecture v2 overview.
- `docs/architecture/composition-kernel.md` — detailed Composition Kernel/precondition authority.
- issue **#46** — architecture authority index.
- issue **#67** — Composition Kernel semantic design gate (merged via PR #68).
- `CONTRIBUTING.md` — contribution entry point.

## Scope

Qianqian is still a music player, not a generic framework product. The composability work exists to test whether strong component/lifecycle properties survive a real media runtime with explicit ordering, system-boundary and realtime constraints.
