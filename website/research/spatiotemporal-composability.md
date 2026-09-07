---
title: Spatiotemporal Composability
status: VALIDATED
---

# Spatiotemporal Composability

<StatusBadge status="VALIDATED" />

## Source

*A Programming Paradigm for Spatiotemporal Composability*

Yifan Shi, Wei Zhang, Tianyi Cui (Peking University; DeepSeek-AI)

arXiv:2608.25512v1, 92 pp.

<ClaimBadge role="authority" />

---

## What the Paper Says

The paper introduces **Cordis**, a formal model for composable runtime systems. Key concepts:

- **Context transformation with inverses** — every effect carries an inverse; inverses compose in LIFO order
- **Typed key table Σ** — dependencies are a typed key table; `provide = set` (reversible), `consume = get`
- **Fiber as composition unit** — each fiber has identity, context, effects, lifecycle
- **Reconcile** — moves the running graph toward desired composition
- **Confluence** — after legal histories reach quiescence, the result matches a clean build

The paper proves:

| Theorem | What it proves |
|---------|---------------|
| Thm 5/7 | Effects compose in twisted (LIFO) order |
| Thm 15 | Local revertibility is per-application |
| Thm 70 | Teardown-access window for withdrawal |
| Thm 73 | Quiescence/progress |
| Thm 80 | Confluence after composition history |

---

## What Qianqian Borrows

<ClaimBadge role="authority" />

| Paper Concept | Qianqian Instantiation |
|--------------|----------------------|
| Context Σ (typed key table) | **Context** — capability namespace/dependency view |
| Fiber lifecycle | **Fiber** — live plugin instance with identity/scope/lifecycle |
| Effect with inverse | **Effect** — owned composition-lifecycle mutation with total inverse |
| Desired → running graph | **Reconcile** — moves graph toward desired composition |
| Capability provision/consumption | **Capability** — named/typed service contract |
| LIFO effect composition | Intra-Fiber effects unwind in LIFO order |
| Confluence | Tested by oracle campaign (74 tests) |

---

## What Qianqian Does NOT Borrow

<ClaimBadge role="interpretation" />

| Paper Feature | Why Not |
|--------------|---------|
| Generator-style iteration (Def 17–18) | K0 activation is one bounded step; no generator machinery needed |
| Child-fiber instantiation | Removed from K0 scope — parent/child is paper design context only |
| HMR (Hot Module Replacement) | Not in scope for a native music player |
| Specific key representation (symbol/TypeId) | K is abstract in K0; implementation choice |
| Dynamic module loading | Logical plugin ≠ dynamic library |

---

## What Qianqian Changed

<ClaimBadge role="evidence" />

| Paper | Qianqian | Rationale |
|-------|---------|-----------|
| Universal context bus | Context = capability namespace only | "Capability plane != Data plane" prevents Context from becoming a global state bag |
| All effects reversible | Five-label action taxonomy; only Reversible composition-lifecycle effects in K0 | Already-rendered sound cannot be "unplayed" — the architecture must not promise undo for irreversible actions |
| Full HMR support | N/A | Static composition, not a web framework |
| Parent/child fibers | Removed from K0 | MVP scope control; not needed for initial composition guarantees |

---

## Executable Evidence

<ClaimBadge role="evidence" />

The Base Kernel K0 implements the five borrowed primitives and validates them through 74 oracle tests. The tests prove:

- **Thm 5/7 instantiation** — LIFO effect unwind verified in single-Fiber cleanup tests
- **Thm 15 instantiation** — Local revertibility: a disposer proves local revert, not cross-Fiber independence
- **Thm 70 instantiation** — Teardown-access window enforced in provider-disappearance tests
- **Thm 80 instantiation** — Confluence tested by comparing mutation histories against clean builds

---

## Open Questions

- How should child-fiber semantics be introduced (if ever) while preserving K0 guarantees?
- Can the formal model be extended to cover realtime audio graph composition?
- What is the correct abstraction for ordered DSP pipelines within the composition model?

---

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md §A.4']"
  :decisions="[{ issue: 67 }, { pr: 68 }]"
  :evidence="['crates/qianqian-kernel/tests']"
  lastVerified="743eb86"
/>
