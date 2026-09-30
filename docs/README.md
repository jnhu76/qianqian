# Documentation router

Load only the documentation needed for the current task. Git history and external evidence are opt-in; current authority lives in the working tree.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary / current role names | `adr/ADR-PBK-002.md` — current authority (Issue #138 records the corrective rationale/history) |
| Current architecture overview | `architecture/overview.md` |
| Generic Composition Kernel semantics | `architecture/composition-kernel.md` + `architecture/composition-kernel-0-design.md` + `architecture/composition-kernel-0-implementation-adr.md` |
| Playback/audio foundations | `adr/ADR-PBK-001.md` — **ACCEPTED** |
| Plugin/Fiber taxonomy + admission invariant + earned static playback composition | `adr/ADR-PBK-002.md` D1/D4/D5/D6/D12/D13 |
| Output Plugin / host-audio backend boundary, cross-platform output work | `adr/ADR-PBK-003.md` — stable Output Plugin identity + backend-neutral Host Render Contract; concrete backend owned mechanism by default |
| SongCore one-core/many-bindings architecture, raw vs ergonomic binding boundary, binding parity oracle, target support maturity (BUILD…RELEASE) | `architecture/songcore-binding-architecture.md` — binding-architecture authority (Issue #173); the canonical ABI description itself is `../native/include/songcore.h` |
| Episode terminal outcome semantic authority | `adr/ADR-PBK-002.md` §17 / D11 |
| Playlist ordering / repeat / EOF-navigation policy + TUI keymap (Issue #166 U2) | `adr/ADR-PBK-002.md` §20 D14.6 — the 2026-09-19 U2 amendment (App-owned product policy; the Playback Session establishes terminal Facts and nothing else). The shipped temporary playlist is a realization of that decision, not a second authority |
| Terminal Fact commit-ownership boundary evidence（F2 seam 前置 formal campaign） | `../specs/f2-terminal-commit-boundary/` — campaign evidence only，**未裁决**；结论是 CURRENT_CONTRACT_UNDERSPECIFIED，等 ADR/code gap audit |
| F5 seek 实现 guardrail 证据（M1–M11 生产突变 + fail-closed runner） | `../specs/f5-seek-implementation/` — executable evidence only；seek 语义权威 = `adr/ADR-PBK-002.md` §20 D14.5 |
| Current architecture corrective basis | Issue #138 — design input only, **not authority** |
| Minimal PCM contract evidence | `architecture/pcm-contract-a0.md` + `../crates/qianqian-audio-api/tests/pcm_edge_contract/` — Phase B evidence only |
| Direct-flow graph evidence | `architecture/direct-pcm-flow.md` + `../crates/qianqian-audio-api/tests/direct_pcm_flow/` — Phase C evidence only |
| Realtime publication lifetime evidence + mechanism comparison | `architecture/realtime-publication-lifetime-decision.md` + `../specs/realtime-publication/` — evidence only |
| Current Rust behavior | current code + tests |
| Retired pre-reset playback models/harness | Git history / PR records — historical evidence only; removed from main by the post-#139 spec reset (still-current races are covered by today's specs/tests, see `../specs/README.md`) |
| Historical component-boundary audit | `architecture/component-boundary-a0.md` — historical evidence only |
| Plugin boundary conformance audit (post-#145 headless path) | `audits/plugin-boundary-conformance-audit.md` — audit evidence record; hardening plan accepted, **not yet implemented** |
| Audio Processing Plugin architecture audit (post-`ba545ee`) | `audits/audio-processing-plugin-audit.md` — audit evidence record; verdict GO_WITH_CORRECTIVES; **D13 negative ruling: no Processing Plugin/Capability today**; authority freeze (D14.11 candidate) required before any DSP implementation |
| Product/repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| External failure evidence | `../evidence/README.md` — opt-in only |

---

# Authority model

Every document belongs to exactly one truth class.

```text
NORMATIVE AUTHORITY
  Playback foundations
      -> adr/ADR-PBK-001.md
         plane separation, Command/Fact/Projection contracts,
         realtime data-plane firewall, P1–P5

  Current vocabulary + Plugin/Fiber taxonomy +
  earned static playback composition + D11
      -> adr/ADR-PBK-002.md

  Stable Output Plugin / pluggable Host Render Backend boundary
      -> adr/ADR-PBK-003.md
         backend-neutral AudioOutput obligations;
         concrete host backend is an owned mechanism by default

  SongCore binding architecture (one core / many bindings,
  authority hierarchy, binding classes, maturity vocabulary,
  binding-parity oracle)
      -> architecture/songcore-binding-architecture.md
         (canonical SongCore ABI description:
          ../native/include/songcore.h)

  K0 semantic design
      -> architecture/composition-kernel-0-design.md

  K0 representation decisions
      -> architecture/composition-kernel-0-implementation-adr.md

PRODUCTION REALITY
  -> current main-branch source + Cargo graph + public APIs

EVIDENCE
  -> tests / specs / TLA+ / experiment harnesses /
     architecture evidence records (pcm-contract-a0.md,
     direct-pcm-flow.md, realtime-view-publication.md,
     realtime-publication-lifetime-decision.md,
     component-boundary-a0.md, first-audible-slice.md) /
     audit records (audits/plugin-boundary-conformance-audit.md,
     audits/audio-processing-plugin-audit.md) /
     historical refs

DERIVED PROJECTION
  -> README.md / CONTEXT.md / AGENTS.md summaries /
     this router / architecture/overview.md /
     architecture/composition-kernel.md / registry.yml /
     website / diagrams
```

Issue #138 is an **architecture corrective basis / design-input record**. It may explain why authority is being changed but does not itself define current architecture.

---

# Current canonical architecture mapping

Current vocabulary is routed to PBK-002, with the Output/backend refinement owned by PBK-003:

```text
Composition Kernel (K0)
    runtime that manages Plugin/Fiber composition

Plugin
    independently K0-composed lifecycle/behavior unit

ComponentSpec
    current K0 formal/Rust representation of a Plugin definition

Fiber
    one live Plugin instance

Capability
    typed dependency/reachability contract

Service
    executable object reached through a Capability

Plugin-owned domain resource
    subordinate endpoint/worker/edge/stream whose internals remain outside K0

Host Render Backend
    concrete platform mechanism owned by Output Plugin by default;
    realizes the backend-neutral AudioOutput contract; not a Plugin merely
    because it is platform-specific or replaceable

PCM Data Plane
    pre-bound payload flow; not a Plugin and not routed through K0 per quantum
```

Current earned Plugin roles:

```text
Decode Plugin
Output Plugin
Playback Session Plugin (episode-scoped)
```

Concrete WASAPI / ALSA / PipeWire / CoreAudio / CPAL-backed host mechanisms are not additional Plugin roles by default; PBK-003 owns that boundary.

Do not use the older `Component vs Plugin` taxonomy as current architecture. K0 design documents may use `component` as their formal/paper term; PBK-002 defines the Qianqian architecture mapping.

---

# Current playback foundation

The accepted foundation remains the four reasoning lenses in PBK-001:

```text
Composition
Execution / Control
Fact
Realtime Data
```

PBK-002 maps current production onto them; PBK-003 further requires the Output Plugin's concrete host backend to remain below the stable composition identity:

```text
Qianqian App
    ↓ desired composition
K0
    ↓ manages
Decode Plugin + Output Plugin + Playback Session Plugin
                  │
                  └── owns selected Host Render Backend mechanism
    ↓ Playback Session binds/owns episode resources
kernel-free PCM data plane
```

Playback Session is also the D11 designated semantic authority for **one episode's terminal outcome**:

```text
Completed / Stopped / Failed
```

The designation attaches to the Playback Session semantic role for one playback episode; its current composition realization is the episode-scoped Playback Session Plugin/Fiber (semantic role != Fiber identity by definition). This is one fact authority, not a complete playback state machine.

---

# Plugin / resource routing rule

Use PBK-002 D4/D12 for current taxonomy and D13 for the admission invariant. Use PBK-003 for the explicit Output Plugin / Host Render Backend application of that invariant.

Do not infer:

```text
feature == Plugin
Capability provider == only kind of Plugin
long-lived == required for Plugin
resource/object == Plugin
platform backend == Plugin
replaceable mechanism == Plugin
```

A Plugin may provide no Capability and may be episode-scoped.

Subordinate resources stay resources unless they need independent K0 composition identity/lifecycle — if an existing Plugin can own the candidate without losing composition correctness or lifecycle ordering, it stays an owned resource/effect (D13). Domain resource ownership does not widen K0 kernel data: K0 still knows only its frozen Fiber/Capability/Effect/Discharge semantics.

For host audio specifically, Output Plugin is the stable composition role; the selected host backend is an owned mechanism behind the backend-neutral `AudioOutput` contract unless a future D13 review earns independent Plugin identity.

---

# Generic Composition Kernel vs playback

K0 remains implemented/current and domain-agnostic.

When reading `architecture/composition-kernel-0-design.md`, treat `component` as the K0/paper formal unit. Current product architecture maps an independently K0-composed unit to **Plugin**, represented today by `ComponentSpec`, with each mounted instance represented by a Fiber.

Do not rewrite K0 semantics merely to rename `component`; do not resurrect a peer product taxonomy from the formal term either.

---

# Realtime / PCM policy

Normative authority: PBK-001 §2.4 and §6; current static mapping: PBK-002 D8/D9; host-render backend refinement: PBK-003.

Per quantum PCM must not perform:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic Plugin dispatch
generic Fact/Event fan-out
filesystem/network/UI round trip
```

K0 composes owners on setup/control boundaries; already-bound domain resources carry PCM directly.

Backend-specific mechanism evidence must refine the platform-neutral host-render obligations rather than redefine them. In particular, WASAPI `GetBuffer` / `GetCurrentPadding` / `padding` are the current Windows refinement, not generic architecture vocabulary.

If a future feature creates overlapping old/new RT worlds whose release depends on reader quiescence, trigger PBK-001 P1–P5 and earn the minimum specialized mechanism. Do not pre-create Window/Generation because historical models had them.

---

# Fact/Event policy

Normative authority: PBK-001 §2.3.

```text
Command != Fact
Mechanism Evidence != Semantic Fact unless explicitly designated
semantic commit precedes Fact publication
one designated semantic authority per (fact kind, subject scope)
Projection != Authority
```

Current designated playback fact: PBK-002 D11 episode terminal outcome. Other PlaybackFacts and publication topology remain OPEN.

---

# Phase-F architecture rule

Future pause/seek/open/next/volume work must first attempt the simplest expression using:

```text
existing Plugin/Fiber lifecycle
existing Capability/Service seams
Plugin-owned domain resources
small orthogonal fact/control state
```

Only concrete counterexamples may earn extra Plugin boundaries, Window/Generation, or a specialized Realtime Audio Runtime mechanism.

Phase-F playback semantics: F3 pause/resume authority is frozen
(D14.7, incl. the AUTHORITY-CORRECTIVE that removed `Resumed` as an
application-facing projection — Paused is the earned Projection,
resume is Command-only, disengagement is Mechanism Evidence) and
implemented behind the episode seam; F4 position/duration authority is
frozen by the D14.8 amendment (Position = episode-local
device-consumed Projection, read as one pure load of a monotone
mechanism-evidence sample published by the render leg; Duration =
optional source-scoped Mechanism Evidence whose unknown stays unknown)
and implemented behind the same seam (PR #152) — pending human review;
the remaining transport semantics stay OPEN
(PBK-002 §14) pending their authority design.

For host-render wording in those frozen mechanisms, PBK-003 is the interpretation boundary: platform-specific WASAPI counters/calls are current Windows realization/evidence, while another backend must refine the same platform-neutral obligation rather than clone the same API.

---

# Playback experimental evidence

Legacy nouns remain evidence only:

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

Load them only for a prior reproducer, executable comparison, or verification technique. Do not preserve them because a new feature happens to resemble the old design.

---

# Architecture design order

Follow `../AGENTS.md`:

```text
Plugin granularity / ownership
→ Capability / dependency
→ execution semantics
→ fact/state authority
→ lifetime/withdrawal
→ realtime boundary if earned
→ evidence
→ representation
```

---

# Formal verification policy

Formalization is risk-driven (normative policy: `adr/ADR-PBK-001.md` §13; verification guardrails: `../AGENTS.md` "Verification authority boundary").

> **TLA+ finds concrete state/interleaving collisions; it is not a second architecture authority.**

No concrete collision -> prefer types/ownership/tests/static checks.

---

# External evidence isolation

Do not load `evidence/` during ordinary architecture/ADR/implementation work unless the task explicitly asks for external failure evidence/adversarial inspiration. External systems can inspire tests/distinctions; they never become Qianqian authority.

---

# Historical material

Historical evidence is preserved primarily as Git history/refs:

```text
pre-rust-v2
playback-reference-v1
```

Inspect only when the task needs history.

---

# Documentation growth rule

Create a new long-lived document only when a durable fact needs its own authority.

Prefer rewriting current authority cleanly over growing amendment/supersession chains while the architecture is young. PBK-003 is intentionally separate because the host-render backend boundary is a durable cross-platform authority in its own right rather than another Phase-F feature amendment; it narrows PBK-002's platform interpretation without duplicating playback semantics.

When production and authority disagree: record the differential, decide which side is wrong, amend authority explicitly if needed, and only then treat the new shape as current architecture.
