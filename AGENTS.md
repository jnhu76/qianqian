# AGENTS.md

Qianqian is a local-first, lightweight, cross-platform music player and an architecture experiment in composable runtime design. This file defines repository-wide rules for coding agents. It is governance and routing, not a feature manual.

## Start here

Before changing code or long-lived documentation:

1. Read the current issue/task.
2. Read `CONTEXT.md` for current vocabulary and status.
3. Use `docs/README.md` to load only the minimum relevant authority.
4. Read `docs/architecture/overview.md` before changing architecture boundaries.
5. For generic composition work, read `docs/architecture/composition-kernel.md` and the K0 design/implementation authority.
6. For playback/audio work, read `docs/adr/ADR-PBK-001.md` — the **only normative Playback Foundations constitution**, now **ACCEPTED**.
7. Inspect current repository reality before assuming a path, type, crate, test, TLA variable, or prior design is still authoritative.

Do not recursively preload historical refs or external failure evidence.

---

# Playback Foundations authority

```text
Normative Playback Foundations constitution:
    docs/adr/ADR-PBK-001.md        (ACCEPTED Playback Foundations authority)

Do not treat as current architecture unless a new experiment re-earns them:
    MusicKernel
    TransportKernel
    TrackSession
    DecodeSession
    Generation
    Active / Prepared
    Dual Window
    Physical Fence
```

Old playback code/specs — `qianqian-core::music`, `qianqian-core::transport`, `playback_temporal_traces`, `specs/playback/*` — are **experimental / executable evidence only**:

```text
allowed:   reuse bug reproducers, test techniques, negative controls, concrete counterexamples
forbidden: claiming current authority from old type names, preserving old APIs for
           compatibility without an explicit requirement, forcing production state
           to mirror old TLA variables
```

> **Preserve the bug, not necessarily the old solution.** (Full inherit/forbid lists: `ADR-PBK-001.md` §11.)

The full normative contracts — minimal constitution, command/fact authority, fact-authority identity (one designated authority per fact kind + subject scope), projection read-side firewall, semantic-commit definition, Fact publication vs Realtime-view publication, realtime lifetime invariant, publication/reclamation contract (P1–P5), and the one normative research ladder — live in `ADR-PBK-001.md` §1–§2, §6 and §12. Do not restate them normatively anywhere else; link instead.

---

# Base Kernel K0

The generic Composition Kernel is implemented and current.

Primitive budget (no sixth primitive without a dedicated architecture issue demonstrating K0 cannot express the invariant cleanly):

```text
Context / Capability / Fiber / Effect / Reconcile
```

The generic kernel must not know:

```text
PCM / AudioGraph / FFmpeg / WASAPI
track/playlist semantics / seek / playback cursor
platform UI payloads
```

An Event/Fact system is **not automatically a new K0 primitive**. Start it as a normal capability/service/plugin unless evidence demonstrates that it belongs in the kernel.

`Context` is a capability namespace/dependency view. It must not become global product state, a universal event bus, a message broker, PCM transport, a UI payload store, or a get-anything service locator.

---

# Plugin / Fiber discipline

A Plugin is a long-lived capability/lifecycle participant whose boundary has been justified; a Fiber is its live runtime instance.

“Everything is a Plugin” means justified long-lived runtime capabilities enter the common composition/lifecycle protocol. It does **not** mean one feature = one plugin, one AudioNode = one Fiber, or everything is hot-loaded/rollbackable.

For every proposed Plugin boundary, answer:

```text
What lifetime/resource does it own?
What capability does it provide / require?
Does it need independent replacement/withdrawal?
What execution/data edges cross the boundary?
What state is intentionally public?
What is the configuration/cognitive cost of splitting it?
```

---

# Boundary-first design

Do not begin by inventing APIs. Required order remains:

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Interaction / execution semantics
        ↓
Fact / state authority
        ↓
Lifetime / withdrawal ordering
        ↓
Realtime boundary where relevant
        ↓
Executable evidence
        ↓
API / representation
```

A different Rust type, file, crate, feature name, or test helper is not proof that a new Plugin or authority is needed.

---

# Formalization policy

Formalization is risk-driven.

> **Which independently legal states/events can interleave and collide into an illegal state?**

If there is no concrete collision, prefer types, ownership, unit/property tests, static checks, or executable stress tests. The old playback formal core is evidence, not a blocking acceptance gate. See `specs/README.md`.

---

# UI boundary

UI is not playback authority and never participates in realtime correctness. UiHost remains an ordinary capability/plugin candidate; platform intent stays replaceable and must not leak into Base Kernel semantics.

---

# External evidence isolation

External failure mining lives under `evidence/` and is opt-in.

Do not load it during ordinary architecture/ADR/implementation review unless the task explicitly asks for external failure evidence or adversarial inspiration. When external systems inspire a design distinction, re-derive and state the Qianqian invariant locally; do not turn the external project's implementation into our authority.

---

# Review discipline

Fresh-context reviewers for new playback work should prioritize:

```text
lens/plane confusion
Context/event/PCM misuse
hidden global authority
command/fact confusion
fact-authority forgery (mechanism evidence posing as a semantic fact, or rename/scope-slicing manufacturing a second writer of the same truth)
projection used as a control-correctness authority
provider/resource release before realtime readers quiesce
plugin boundary over-fragmentation
accidental preservation of old playback assumptions
```

Do not reject a new design merely because it differs from old PlaybackTemporal or old core types.

---

# Verification

Report what was actually verified. Never mark an unrun device/platform/audio check PASS.

For architecture reset work, green Cargo tests are regression evidence, not architecture acceptance.

---

# Documentation

`docs/README.md` is the documentation router.

Keep one current authority per durable fact. `ADR-PBK-001.md` is the single normative Playback Foundations constitution; every other document links or summarizes it and carries no second normative copy. Git history stores the old architecture; do not grow amendment/supersession chains in the working tree when a clean rewrite is possible.

---

# Delivery discipline

- inspect before assuming;
- keep changes narrow to the current gate;
- distinguish evidence from authority;
- do not perform unrelated cleanup;
- do not silently preserve stale architecture for compatibility;
- stop after opening the requested PR unless explicitly authorized to merge.
