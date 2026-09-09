# Documentation router

Load only the documentation needed for the current task. Git history and external evidence are opt-in; current authority lives in the working tree.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary / current mental model | `../CONTEXT.md` |
| Current architecture overview | `architecture/overview.md` |
| Generic Composition Kernel semantics | `architecture/composition-kernel.md` + `architecture/composition-kernel-0-design.md` + `architecture/composition-kernel-0-implementation-adr.md` |
| Playback/audio foundations | `adr/ADR-PBK-001.md` — **PROPOSED / REOPENED** |
| Current Rust behavior | current code + tests |
| Playback experimental evidence | `../specs/playback/*` + `qianqian-core` playback code/tests — evidence only, not authority |
| Historical component-boundary audit | `architecture/component-boundary-a0.md` — historical evidence only |
| Product/repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| External failure evidence | `../evidence/README.md` — opt-in only when explicitly requested |

---

# Authority model

Use the authority closest to the fact:

```text
agent work rules              -> AGENTS.md
stable vocabulary             -> CONTEXT.md
generic composition semantics -> composition-kernel*.md
current architecture overview -> architecture/overview.md
playback foundation proposal  -> adr/ADR-PBK-001.md (PROPOSED / REOPENED)
implemented behavior          -> code + tests
experimental playback evidence-> specs/playback/* + prior executable core
historical evidence           -> git refs / explicitly historical docs
current task scope            -> current issue/task
```

Playback currently has **no accepted production state-machine authority**. The old accepted playback model was reopened and rewritten; current `ADR-PBK-001` only proposes foundational plane separation.

Do not use old type names or formal variables to close new architecture questions automatically.

---

# Current playback foundation

The current proposal separates four concerns:

```text
Composition Plane
    Context / Capability / Fiber / Effect / Reconcile

Execution / Control Plane
    Command / workflow / Capability-Service call

Fact Plane
    authoritative commit -> Fact -> observers/projections/persistence

Realtime Data Plane
    pre-bound realtime graph/view -> PCM -> device
```

Key distinctions:

```text
Command != Fact
Fact != hot PCM data
Projection != authority
Plugin/Fiber identity != per-block dispatch
Dependency topology != realtime processing topology
```

---

# Playback experimental evidence

Existing code/specs may still contain:

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Generation
Active / Prepared
Physical Fence
```

During the reset these are:

```text
EXPERIMENTAL EVIDENCE
NOT STABLE VOCABULARY
NOT CURRENT ARCHITECTURE AUTHORITY
```

Load them only when the task needs a prior bug reproducer, executable comparison, or formal/testing technique.

Do not make an ordinary architecture reviewer preload them as if they were current design.

---

# Generic Composition Kernel vs playback reset

K0 remains implemented/current and domain-agnostic.

When reading `architecture/composition-kernel.md`, generic K0 semantics remain authoritative. Any old playback-specific examples or references to the previously accepted playback state model are illustrative/history only while ARCH-003 is reopened.

If generic K0 semantics and the reset ADR appear to conflict, identify the exact generic invariant first; do not silently import an old playback-specific conclusion from a K0 document.

---

# Fact/Event policy

Qianqian currently freezes only:

```text
commit authoritative truth
    before
publish corresponding Fact
```

and:

```text
Projection = derived read model, not writer
```

Complete Event Sourcing, append-only durable logs, replay authority and a generic Event primitive are **not yet architecture decisions**.

---

# Realtime policy

Realtime PCM never travels through generic Context/Event/Plugin dispatch per block.

Control-side graph changes are built/validated before publication; realtime consumes a pre-bound graph/view directly.

Resource/provider final release must not race a realtime reader that can still dereference the old graph/view.

Exact graph/lifetime mechanism remains unfrozen.

---

# Architecture design order

For new composition/playback work, do not start from API shape or legacy type names.

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Execution / control semantics
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

---

# Formal verification policy

Formalization is risk-driven.

> **TLA+ finds concrete state/interleaving collisions; it is not a second architecture authority.**

The old PlaybackTemporal/PlaybackOwnership suite remains evidence only during the reset.

Do not expand or preserve it merely to keep old nouns alive.

A new formal model requires a concrete newly earned collision risk.

---

# External evidence isolation

External issue/discussion mining lives under `../evidence/`.

> **Do not load `evidence/` during ordinary ADR, architecture, formal-spec, implementation, or PR review.**

Only opt in when the current task explicitly asks for external failure evidence, upstream issue/discussion mining, failure-corpus maintenance, or adversarial inspiration.

When opted in, start at `../evidence/README.md`; for incremental scans, read `../evidence/external-systems/source-ledger.yml` before the corpus.

External systems can inspire tests and distinctions; they do not become Qianqian authority.

---

# Historical material

Historical evidence is preserved primarily as Git history/refs, not as a second current authority tree.

Useful refs include:

```text
pre-rust-v2
playback-reference-v1
```

Inspect only when the task explicitly needs history.

---

# Documentation growth rule

Create a new long-lived document only when a durable fact needs its own authority.

Prefer rewriting one current authority cleanly over building amendment/supersession chains while the architecture is still young.

When documentation and implementation disagree, identify whether the code is experimental evidence or whether the current authority is stale; do not silently choose one.
