# Documentation router

Load only the documentation needed for the current task. Git history and external evidence are opt-in; current authority lives in the working tree.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary / current mental model | `../CONTEXT.md` |
| Current architecture overview | `architecture/overview.md` |
| Generic Composition Kernel semantics | `architecture/composition-kernel.md` + `architecture/composition-kernel-0-design.md` + `architecture/composition-kernel-0-implementation-adr.md` |
| Playback/audio foundations | `adr/ADR-PBK-001.md` — **ACCEPTED** |
| Realtime publication lifetime evidence + mechanism comparison | `architecture/realtime-publication-lifetime-decision.md`（evidence/decision record，非第二 authority）+ `../specs/realtime-publication/` |
| Minimal PCM edge experiment (ADR §12 Phase B evidence) | `architecture/pcm-contract-a0.md` + `../crates/qianqian-core/tests/pcm_edge_contract/`（test-only harness）— evidence only, not authority |
| Direct-flow graph experiment (ADR §12 Phase C evidence) | `architecture/direct-pcm-flow.md` + `../crates/qianqian-core/tests/direct_pcm_flow/`（test-only harness）— evidence only, not authority |
| Current Rust behavior | current code + tests |
| Playback experimental evidence | `../specs/playback/*` + `qianqian-core` playback code/tests — evidence only, not authority |
| Historical component-boundary audit | `architecture/component-boundary-a0.md` — historical evidence only |
| Product/repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| External failure evidence | `../evidence/README.md` — opt-in only when explicitly requested |

---

# Authority model

Every document belongs to exactly one truth class. Only files explicitly listed as normative authority below may define architecture semantics; everything else summarizes, routes, realizes or proves.

```text
NORMATIVE AUTHORITY (defines what the architecture means)
  Playback Foundations constitution -> adr/ADR-PBK-001.md (ACCEPTED;
                                      incl. §16 vocabulary, §6 P1–P5)
  K0 semantic design                -> architecture/composition-kernel-0-design.md
  K0 implementation decisions       -> architecture/composition-kernel-0-implementation-adr.md

PRODUCTION REALITY (what the program does today)
  -> main-branch source + Cargo dependency graph + actual public APIs

EVIDENCE (why we believe a mechanism/property; never authority)
  -> tests / specs / TLA+ / experiment harnesses / specs/playback/* /
     architecture evidence records (pcm-contract-a0.md, direct-pcm-flow.md,
     realtime-view-publication.md, realtime-publication-lifetime-decision.md,
     component-boundary-a0.md)

DERIVED PROJECTION (how humans discover/understand; define nothing)
  -> README.md, CONTEXT.md, AGENTS.md summaries, this router,
     architecture/overview.md, architecture/registry.yml,
     architecture/composition-kernel.md (summary), website/**, diagrams
```

Routing entry points by task remain:

```text
agent work rules              -> AGENTS.md
stable vocabulary index       -> CONTEXT.md (summaries; definitions: ADR §16)
current architecture overview -> architecture/overview.md (projection)
playback foundations          -> adr/ADR-PBK-001.md (ACCEPTED)
implemented behavior          -> code + tests
experimental playback evidence-> specs/playback/* + prior executable core
historical evidence           -> git refs / explicitly historical docs
current task scope            -> current issue/task
```

Playback currently has **no accepted production state-machine authority**. The old accepted playback model was reopened and rewritten; the accepted `ADR-PBK-001` freezes foundational plane separation, not a production playback state machine.

Do not use old type names or formal variables to close new architecture questions automatically.

---

# Current playback foundation

The accepted foundations separate four reasoning lenses — Composition, Execution/Control, Fact, Realtime Data — summarized in `architecture/overview.md` and frozen normatively in `adr/ADR-PBK-001.md` §1–§2. Do not restate the constitution normatively here.

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
Dual Window
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

When reading `architecture/composition-kernel.md`, treat it as a derived summary: generic K0 semantics are authoritative only in `architecture/composition-kernel-0-design.md` (representation decisions: `architecture/composition-kernel-0-implementation-adr.md`). Any old playback-specific examples or references to the previously accepted playback state model are illustrative/history only; the accepted ARCH-003 foundations do not re-freeze them.

If generic K0 semantics and the reset ADR appear to conflict, identify the exact generic invariant first; do not silently import an old playback-specific conclusion from a K0 document.

---

# Fact/Event policy

Normative authority: `adr/ADR-PBK-001.md` §2.3 — semantic commit precedes Fact publication; one designated semantic authority per (fact kind, subject scope); projections are read-only visibility; Event Sourcing, durability, replay and a generic Event primitive are **not yet architecture decisions**.

---

# Realtime policy

Normative authority: `adr/ADR-PBK-001.md` §2.4 and §6 — realtime PCM never travels through generic Context/Event/Plugin dispatch per quantum; publication/reclamation follows the normative P1–P5 semantic contract (P1 is coherent publication; the full set is not restated here). The exact graph/lifetime mechanism remains unfrozen.

---

# Architecture design order

The boundary-first design order is normative in `../AGENTS.md` ("Boundary-first design"). Do not start from API shape or legacy type names.

---

# Formal verification policy

Formalization is risk-driven (normative policy: `adr/ADR-PBK-001.md` §13).

> **TLA+ finds concrete state/interleaving collisions; it is not a second architecture authority.**

The old PlaybackTemporal/PlaybackOwnership suite remains evidence only during the reset; do not expand or preserve it merely to keep old nouns alive. A new formal model requires a concrete newly earned collision risk.

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

When documentation and implementation disagree, apply the authority resolution rule in `../AGENTS.md` ("Authority resolution"): record the differential, decide which side is wrong, amend the authority explicitly if the code is better — never let "code is newer" silently win, and never force the implementation to mechanically copy a stale authority.
