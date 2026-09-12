# Qianqian / 千千·现代

Qianqian is a local-first, lightweight, cross-platform music player and a testbed for Rust composability/runtime architecture.

The repository is in **Architecture v2**. The first verified playback experiment was frozen, `main` was reset, and the implementation is being rebuilt boundary-first on a small generic Composition Kernel; the playback-specific foundation is **accepted** in `ADR-PBK-001` (plane-boundary constitution — production playback semantics remain open until real Audio Runtime experiments earn them).

## Architecture in 30 seconds

下图是 Architecture v2 reset 阶段留下的 Playback Foundations 历史 proposal 海报（图内自标 `PROPOSED / RESET`），仅为保留当时的视觉设计上下文；它不是当前架构的完整投影，也不是 normative authority。

[![ARCH-003 Playback Foundations](docs/architecture/diagrams/ARCH-003-playback-foundations-v2-poster-original.png)](docs/adr/ADR-PBK-001.md)

当前规范语义以 [`ADR-PBK-001`](docs/adr/ADR-PBK-001.md)（ACCEPTED）为准，K0 语义以 [`composition-kernel-0-design.md`](docs/architecture/composition-kernel-0-design.md) 为准。当前架构图应从仓库内 Mermaid 源（`docs/architecture/diagrams/`）生成，而不是手绘海报。

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.** (K0 design authority: `docs/architecture/composition-kernel-0-design.md`.)

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
Publication/reclamation P1–P5      NORMATIVE (formal evidence, PR #91)
Minimal PCM contract evidence      DELIVERED (Phase B, PR #93)
Direct-flow graph evidence         DELIVERED (Phase C, PR #96)
Realtime-view publication/reclamation
mechanism evidence                 DELIVERED (PR #97; Issue #94 closed)
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

See `docs/adr/ADR-PBK-001.md` for the accepted foundation and `specs/playback/README.md` for the evidence status. The realtime publication/reclamation semantics (P1–P5) are normative in ADR §6, and the Realtime Runtime responsibility was earned by executable mechanism evidence (`docs/architecture/realtime-view-publication.md`); production playback semantics remain open (ADR §10). Normative term definitions (Plugin, Fiber, Capability, Runtime, Realtime Runtime, Fact, Reclamation, …) live in ADR §16.

## Everything is a Plugin

“Everything is a plugin” means every **justified** long-lived runtime capability participates in one common composition/lifecycle protocol. It does **not** mean one feature == one plugin, one plugin == one crate/DLL, or every DSP node is a Fiber. Plugin/Component granularity and the boundary questionnaire: `docs/adr/ADR-PBK-001.md` §3 + `AGENTS.md` ("Plugin / Fiber discipline").

## Interaction correctness

Commutative relations may compose as independent effects; non-commutative relations need explicit dependency/order/integration structure — registration order or container iteration is never semantic topology. Restoration is judged by **observational equivalence**, not bit identity, and already-rendered sound cannot be “unplayed” (physical emission lies outside the recoverable system boundary). Guardrails: `docs/architecture/composition-kernel.md`.

## Realtime boundary

Per audio callback/block there must be no Context lookup, capability resolution, Fiber reconciliation, generic event dispatch, filesystem/network I/O, UI round trips, or unbounded allocation/blocking. PCM flows through pre-bound direct data edges. Normative list: `docs/adr/ADR-PBK-001.md` §2.4.

## Formal verification policy

TLA+ is risk-driven evidence for concrete state/interleaving collisions, not a second architecture. The current earned formal target is realtime publication/lifetime: P1–P5 are normative in `ADR-PBK-001.md` §6, and mechanism validation evidence is in `docs/architecture/realtime-view-publication.md`. Old playback formal models are experimental evidence, not an acceptance gate. See `specs/README.md`.

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
qianqian-composition
qianqian-audio-api
qianqian-app
qianqian-playback
qianqian-headless
```

The generic Composition Kernel (K0) is implemented. Current canonical vocabulary and production boundaries: ADR-PBK-002. The earlier experiment code (`MusicKernel` / `TransportKernel`) survives only as test-local executable evidence under `crates/qianqian-audio-api/tests/playback_temporal_traces/` — not current authority and not a compatibility contract.

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
- `docs/architecture/composition-kernel.md` — generic composition guardrails (derived summary; K0 authority: `composition-kernel-0-design.md`).
- `specs/README.md` — risk-driven formalization policy and model registry.
- `CONTRIBUTING.md` — contribution entry point.

## Scope

Qianqian is still a music player, not a generic framework product. The composability work exists to test whether strong component/lifecycle properties survive a real media runtime with explicit ordering, physical-system boundaries and realtime constraints.
