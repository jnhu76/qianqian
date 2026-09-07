---
title: Composition Kernel K0 Oracles
status: VALIDATED
---

# Composition Kernel K0 Oracles

<StatusBadge status="VALIDATED" />

## Does the Base Kernel satisfy its semantic design invariants?

---

## 01 Question

Does the implemented Base Kernel K0 satisfy its frozen semantic design across single-Fiber cleanup, cross-Fiber independent removal, same-key contribution safety, ordered interaction, provider-disappearance ordering, and confluence after mutation history?

---

## 02 Baseline

The semantic design (`composition-kernel-0-design.md`) defines precise invariants derived from the Cordis formal model. Before implementation, these were theoretical guarantees.

---

## 03 Hypothesis

An executable oracle campaign can validate all six semantic guarantee groups by constructing specific composition histories and asserting observable outcomes match the design.

---

## 04 Method

Tests are organized as **adversarial oracles** — each test constructs a specific scenario that would violate an invariant if the implementation were incorrect.

| Oracle Group | What it tests |
|-------------|--------------|
| Single-Fiber cleanup | Owned effects unwind LIFO; fiber reaches terminal |
| Cross-Fiber removal | Removing fiber A preserves independent fibers B/C |
| Same-key safety | Contributions compose without hidden cross-key mutation |
| Ordered interaction | Non-commutative relations use explicit structure |
| Provider-disappearance | Dependents finish teardown before provider release |
| Confluence | History → quiescence ≡ clean build |

Additional oracles (A1–A20) cover adversarial edge cases identified during review.

---

## 05 Evidence

<ClaimBadge role="evidence" />

| Evidence | Detail |
|----------|--------|
| Test count | 74 tests |
| All pass | At merge commit `743eb86` |
| Test location | `crates/qianqian-kernel/tests` |
| Adversarial oracles | A1–A20 |
| Implementation corrective-1 | `de46bd1` (P0-1..P1-5 fixes + A16–A20) |
| Implementation corrective-2 | `048ebed` (review 5128815134) |

**Evidence quality:** Executable Rust tests, run in CI. Each test asserts specific observable outcomes from the frozen semantic design.

**Last verified commit:** `743eb86`

---

## 06 Result

<ClaimBadge role="evidence" />

All 74 tests pass. The six semantic guarantee groups are validated:

1. **Single-Fiber local cleanup** — Effects unwind in LIFO order; fiber reaches a clean terminal state.
2. **Cross-Fiber independent removal** — Removing a fiber preserves the observable contributions of unrelated fibers.
3. **Same-key contribution safety** — Multiple fibers contributing to the same capability key compose without hidden cross-key mutation.
4. **Ordered interaction** — Non-commutative operations use explicit dependency/order structure rather than implicit registration order.
5. **Provider-disappearance ordering** — Dependents finish teardown (with committed access) before the provider's final release.
6. **Confluence** — After any legal mutation history reaches quiescence, the observable result matches a clean construction of the final desired composition.

---

## 07 Architectural Consequence

<ClaimBadge role="authority" />

The Base Kernel K0 is the first validated instance of a composition kernel implementing the Cordis-inspired five-primitive model. It proves:

- The five primitives (Context, Capability, Fiber, Effect, Reconcile) are sufficient for the K0 scope
- Confluence is testable, not just theoretical
- Provider withdrawal can be made safe through explicit ordering
- Domain-agnostic composition can enforce lifecycle invariants

This enables the next frontier: the Playback Kernel (MusicKernel) as a domain-specific component built on top of the validated composition infrastructure.

---

## Test Organization

```text
crates/qianqian-kernel/tests/
├── single_fiber_cleanup.rs       — LIFO effect unwind, fiber terminal
├── cross_fiber_removal.rs        — independent removal preserves B/C
├── same_key_contribution.rs      — shared key composition
├── ordered_interaction.rs        — non-commutative structure
├── provider_disappearance.rs     — withdrawal ordering
├── confluence.rs                 — history → quiescence ≡ clean
├── adversarial_review.rs         — A1–A20 adversarial oracles
└── ...
```

---

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md']"
  :decisions="[{ issue: 67 }, { pr: 68 }]"
  :implementation="[{ issue: 70 }, { pr: 71 }]"
  :evidence="['crates/qianqian-kernel/tests']"
  lastVerified="743eb86"
/>
