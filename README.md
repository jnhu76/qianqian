# Qianqian / 千千·现代

Qianqian is a local-first, lightweight, cross-platform music player and a testbed for Rust composability/runtime architecture.

The repository is in **Architecture v2**. The first verified playback experiment was frozen, `main` was reset, and the implementation is being rebuilt boundary-first on a small generic Composition Kernel; the playback-specific foundation is **accepted** in `ADR-PBK-001` (plane-boundary constitution — production playback semantics remain open until real Audio Runtime experiments earn them).

## Architecture in 30 seconds

这是当前 Playback Foundations（ARCH-003）的架构图；规范语义以 [`ADR-PBK-001`](docs/adr/ADR-PBK-001.md) 为准。

[![ARCH-003 Playback Foundations](docs/architecture/diagrams/ARCH-003-playback-foundations-v2-poster-original.png)](docs/adr/ADR-PBK-001.md)

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

But the project does **not** start by writing kernel APIs.

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

Current architecture milestones:

```text
Component boundary audit          PASS / CLOSED              (PR #66)
Composition Kernel semantic design MERGED                   (PR #68)
Base Kernel K0                     IMPLEMENTED              (PR #71)
Playback Foundations reset         ACCEPTED                 (ADR-PBK-001, PR #87)
```

The previous playback architecture (the pre-reset ARCH-003 experiment, briefly accepted in former ADR revisions) was **deliberately reopened from first principles**; the reset foundation itself is accepted in `docs/adr/ADR-PBK-001.md` (**ACCEPTED**). Old playback implementation, specs and formal models remain in the repository as **experimental evidence only** — they preserve failure witnesses and test techniques, not current authority. Production playback semantics remain unauthorized until real Audio Runtime experiments earn them (ADR §10 remains OPEN).

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
      Encoded Media -> Decoder -> Processing -> AudioOutput
```

> **Capability plane != Data plane.**

Context controls reachability/dependency truth. It does not carry PCM blocks or become a universal product message bus.

## Playback status: foundations accepted

There is still **no accepted playback state-machine vocabulary**. The accepted foundations (`docs/adr/ADR-PBK-001.md`) freeze only foundational boundaries — composition vs execution/control vs facts vs realtime data — and deliberately keep all playback-specific nouns unfrozen.

Names still present in old code/specs, such as:

```text
MusicKernel / TransportKernel
TrackSession / DecodeSession
Generation / Active-Prepared / Dual Window / Physical Fence
```

are **experimental evidence**: they preserve real failure witnesses (for example the formal exploration of the `stop × natural ENDED` race) and test techniques, but they are not current architecture and must not be preserved for compatibility unless a future accepted authority re-earns them.

See `docs/adr/ADR-PBK-001.md` for the accepted foundation and `specs/playback/README.md` for the evidence status.

## Everything is a Plugin

“Everything is a plugin” means every **justified** long-lived runtime capability ultimately participates in one common composition/lifecycle protocol.

It does **not** mean:

```text
one feature == one plugin
one plugin == one crate
one plugin == one dynamic library
everything is hot-loaded
everything is rollbackable
every DSP node is a Fiber
```

The Composition topology and the ordered Audio Processing Graph are different structures. Gain/EQ/SRC/Limiter nodes do not become Composition plugins merely because they have state.

## Interaction correctness

A disposer/inverse is not enough to prove that independently mounted components can be removed safely.

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

DSP ordering is the obvious player example: `EQ -> Compressor` is generally not equivalent to `Compressor -> EQ`. Registration timing or container iteration must never become hidden semantic topology.

Restoration is judged by **observational equivalence**, not private bit-for-bit identity.

## System boundary

`Everything is Plugin` does not mean `Everything is rollbackable`.

Already-rendered sound cannot be “unplayed”. A claimed fence and physical emission are also different facts: a claimed fence is past the cancellation point; rendered audio is outside the recoverable system boundary.

## Realtime boundary

Per audio callback/block, do not perform:

```text
Context lookup
capability resolution
Fiber reconciliation
generic event dispatch
filesystem/network I/O
UI/managed-runtime round trips
unbounded allocation/blocking
```

PCM flows through pre-bound direct data edges.

## Formal verification policy

Formal models are **risk-driven evidence**, not a second implementation of the whole architecture.

> **TLA+ is used to find state collisions, not to formally model every architectural noun.**

The old playback formal models (Dual Window, Generation admission, Physical Fence, submitted/rendered accounting, EOF/drained/ENDED terminalization) are **experimental evidence** under explicit assumptions during the reset — not a blocking acceptance gate for new playback design.

The current formal target earned by the reset is realtime publication/lifetime: `specs/realtime-publication/` proves the publication/reclamation collision at model level (TLC + mutation negative controls), and its semantic conclusions are frozen as the normative P1–P5 contract in ADR-PBK-001 §6; the implementation mechanism remains deferred (ADR §12 Phase D validates candidate mechanisms against P1–P5).

See `specs/README.md`, `specs/realtime-publication/README.md` and `specs/playback/README.md`.

## UI strategy

Current platform intent:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI candidate / replaceable
```

UiHost owns rendering/input mechanism, not music semantics or realtime correctness.

## Current implementation status

Current Rust workspace:

```text
qianqian-core
qianqian-kernel
qianqian-runtime
qianqian-headless
```

The generic Base Kernel K0 is implemented. The playback code currently in the product crates (`qianqian-core::music`, `qianqian-core::transport`) is **experimental evidence** from the earlier architecture experiment — not current authority and not a compatibility contract.

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

- `AGENTS.md` — repository-wide agent governance and hard rules.
- `CONTEXT.md` — stable vocabulary and mental model.
- `docs/README.md` — task-oriented documentation router.
- `docs/architecture/overview.md` — current Architecture v2 overview.
- `docs/adr/ADR-PBK-001.md` — Playback Foundations constitution (ACCEPTED).
- `docs/architecture/composition-kernel.md` — generic Composition Kernel authority.
- `specs/README.md` — risk-driven formalization policy and model registry.
- `CONTRIBUTING.md` — contribution entry point.

## Scope

Qianqian is still a music player, not a generic framework product. The composability work exists to test whether strong component/lifecycle properties survive a real media runtime with explicit ordering, physical-system boundaries and realtime constraints.
