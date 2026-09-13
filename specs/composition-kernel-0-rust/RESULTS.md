# FV-RUST-0 RESULTS — qianqian-composition scenario verification

> STATUS: EVIDENCE (campaign QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B0/B1).
> Result vocabulary per issue #124. Nothing here states "the architecture is
> proven correct"; every result is bounded exactly as described.

BASE: CAMPAIGN_BASE_SHA `44e9ed0b26d031a7985d67d179600fdce3af8117` (main, post-#127).
Branch: `verification/fv-rust-0`.

## Toolchain

```text
rustc/cargo (workspace): 1.98.1
kani-verifier:           0.67.0 (bundled nightly-2025-11-21, rustc 1.93.0-nightly)
miri:                    nightly 2026-09-12 (miri 0.1.0)
host:                    Fedora, 20 cores, 64 GB RAM
```

## Harness shape (bounds, stated precisely)

`crates/qianqian-composition/src/kernel_verify.rs` — a child module of
`kernel` (private registry access, zero production changes; two `#[cfg]`
lines in `kernel.rs`). The properties are evaluated over **exhaustively
enumerated concrete scenario matrices**:

```text
K1  stale FiberId          2 scenarios  (slot-0 reuse; slot-1 reuse behind filler)
K2  relied_on guard        4 scenarios  (all keep/remove rewrites of {p,c})
K3  effect discipline     12 scenarios  (dispose × double-dispose × 3 routes)
K4a removal (clean)        8 scenarios  (activation raise × dispose × 2 routes)
K4b removal (violations)  16 scenarios  (inv1/inv2/teardown violation lattice × 2 routes)
K5  quiet truth           16 + 8 scenarios (all plan subsets × activation failure; sweep from maximal state)
K6  single-source         10 scenarios (5 legal phase-2 plans × p1 §G.6 latch)
```

Every scenario is drained a bounded number of `step()` transitions
(≤ 14) and the property invariant is re-asserted **after every step**.
Desired plans are injected directly (`force_desired`) — observationally
equivalent to `set_desired` on a legal plan, kept out of the verified
formula; plan-time validation itself is covered by existing unit tests.
`dispose_root` is replaced by the equivalent (empty-desired + drain).

## Results

| ITEM | RESULT (vocabulary #124) | ENGINE |
|------|--------------------------|--------|
| K1 stale FiberId never re-addresses a reused slot | BOUNDED-CLEAN (scenario bounds above) | native test + MIRI-CLEAN |
| K2 relied provision stays resolvable behind open views | BOUNDED-CLEAN | native test + MIRI-CLEAN |
| K3 inverse at most once, LIFO, tombstone authority | BOUNDED-CLEAN | native test + MIRI-CLEAN |
| K4a clean removal discharges everything owed | BOUNDED-CLEAN | native test + MIRI-CLEAN |
| K4b violated verdict latches; removal blocked; P_RELIED holds | BOUNDED-CLEAN | native test + MIRI-CLEAN |
| K5 quiet ⇔ step settles (FAILED/Pending quiet-legal covered) | BOUNDED-CLEAN | native test + MIRI-CLEAN |
| K6 single-source incl. #126 withheld-mount latch | BOUNDED-CLEAN | native test + MIRI-CLEAN |
| M-K1 (ignore FiberId generation) | COUNTEREXAMPLE-WITNESSED | mutation → native/Miri channel |
| M-K2 (drop relied_on guard) | COUNTEREXAMPLE-WITNESSED | mutation → native/Miri channel |
| M-K3 (drop mount overlap guard) | COUNTEREXAMPLE-WITNESSED | mutation → native/Miri channel |
| Miri over all 7 matrices | MIRI-CLEAN (18.9 s; full UB/leak/overflow checking) | cargo +nightly miri |

## Kani engine status: TOOLING-INSUFFICIENT (symbolic), honestly recorded

The harnesses are dual-gated: under the Kani toolchain they compile as
proof harnesses. Symbolic runs did not converge to usable runtimes:

```text
k1, symbolic filler bit, unwind 16:  aborted > 40 min CPU / > 15 GB, no result
k1, fully concrete,       unwind 16:  aborted > 40 min CPU, memory still climbing
k2, concrete,             unwind 20:  > 40 min CPU at timeout (2400 s), no result
```

CBMC formula expansion over the std-collection-heavy kernel
(`BTreeMap`/`String`/`Vec` machinery, syntactically unrolled per loop
bound) is the dominant cost, not the symbolic input count. Per the
campaign's own rule this is recorded as TOOLING-INSUFFICIENT for the
symbolic-Kani layer this round; it does NOT invalidate the property
evidence above, which is carried by the exhaustive concrete matrices
(native + Miri) plus the TLA+/TLC symbolic layer (#125, PR #125:
baseline BOUNDED-CLEAN, 263314 states). A future round could earn the
Kani layer with collection-light verification seams — that is a
dedicated design question, not a silent API widening.

## Classification notes (harness evolution, no production defect)

1. K3 route-0 initially forgot the drain after replacing `dispose_root`
   — HARNESS DEFECT, fixed before any result was recorded.
2. The original P_RELIED ("provider still installed") was too weak: with
   the relied_on guard mutated away, the kernel kept the provider
   *installed* (Pending) behind an open view — only its provision had
   been discharged. The invariant was strengthened to what the guard
   actually protects (provision resolvability); M-K2 then produced a
   counterexample. HARNESS ORACLE evolution; production behavior was
   identical with and without the fix, no production change was made.
