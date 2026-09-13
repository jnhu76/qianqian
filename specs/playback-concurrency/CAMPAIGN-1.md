# QIANQIAN-VERIFICATION-CAMPAIGN-1 — Final Report

> STATUS: EVIDENCE. Result vocabulary per issue #124. This report claims
> exactly what was run, at the stated bounds, on the stated host — nothing
> more. No layer's result is a claim that "the architecture is proven
> correct".

```text
BASE_SHA:        44e9ed0b26d031a7985d67d179600fdce3af8117  (main after #127)
BRANCHES:        verification/fv-rust-0 (Phase B0/B1)
                 verification/fv-conc-0 (Phase B2 + this report)
HOST:            Fedora server, 20 cores, 64 GB RAM
TOOLCHAIN:       rustc/cargo 1.98.1 · kani-verifier 0.67.0 · miri nightly 2026-09-12 · loom 0.7.2
TLA+/TLC:        NOT RE-RUN this round — PR #125 evidence stands
                 (baseline BOUNDED-CLEAN, 263314 states; M1–M5 counterexamples;
                  probes witnessed; production corrective #126 merged)
```

## Phase A — #127 oracle corrective (precondition)

```text
#127:            CLASSIFICATION   ORACLE-DEFECT / REFINEMENT-GAP
                 CORRECTIVE_HEAD  a6bc8cc (merged as 44e9ed0)
                 PRE_FIX_STRESS   13/40 iterations FAILED (2-core × 2 threads)
                 POST_FIX_STRESS  0/40 iterations FAILED (committed head)
                 MERGED           yes — HARD PRECONDITION satisfied
ISSUE #121:      retitled (inaccurate join() framing removed) +
                 classification comment; contract != procfs observation.
```

## Cross-layer property matrix (filled from actual results)

```text
PROPERTY                    TYPE   TLA+   KANI(sym)  MATRIX+MIRI   LOOM   SYSTEM
single-source (#126)          -     ✓     T-I        ✓             -      tests
relied_on / provision
  resolvability               -     ✓     T-I        ✓             -      tests
inverse once + LIFO +
  tombstone retention    partial  ✓     T-I        ✓ (K4b)       -      tests
generation safety (K1)     partial  -     T-I        ✓             -      tests
removal discipline (K4)        -    ✓     T-I        ✓             -      tests
quiet truth (K5)               -    ✓     T-I        ✓             -      tests
edge FIFO / terminal
  monotonicity / wakeup        -    -     -          ✓ (native)    ✓      stress
completion exactly once        -    -     -          ✓ (native)    -      tests/stress
unsafe pointer validity        -    -     -          MIRI-CLEAN
  (pure-Rust crates)                                       (scope below)  -
procfs visibility              -    -     -          -             -      ✓
WASAPI / native decode         -    -     -          -             -      NOT RUN
```

Legend: ✓ = executed result recorded; T-I = TOOLING-INSUFFICIENT (recorded
below, honest non-result); "-" = not this layer's property.

## Per-layer results

### Type system (Phase B0 — no verifier)

`specs/composition-kernel-0-rust/TYPE-SYSTEM-GUARANTEES.md`. Highlights:
`CompositionKernel` is `!Send + !Sync` by construction (compiler enforces
the serialized control plane); `Box<dyn FnOnce>` + take-then-call
discharge one closure value per record; generational `FiberId` fields are
`pub(crate)` (no forgery outside the crate). Everything the compiler
already proves is explicitly excluded from verifier scope.

### FV-RUST-0 (Phase B1) — kernel scenario verification

Details: `specs/composition-kernel-0-rust/RESULTS.md`.

```text
K1 stale FiberId / K2 relied provision / K3 inverse-once+LIFO /
K4 removal discipline (clean + violation lattice, tombstone retention) /
K5 quiet truth / K6 single-source incl. #126 withheld mount:
    BOUNDED-CLEAN over exhaustively enumerated concrete scenario
    matrices (62 distinct-behavior scenarios; P_RELIED/P_SINGLE
    re-asserted after every step in K2/K6, drain-completion assertions
    elsewhere); MIRI-CLEAN (≈19 s).
NEGATIVE CONTROLS: M-K1, M-K2, M-K3 → COUNTEREXAMPLE-WITNESSED each.
KANI (symbolic engine): TOOLING-INSUFFICIENT — no convergence within
    40 min CPU / >15 GB on representative harnesses (documented;
    formulas dominated by std BTreeMap/String expansion, not by input
    symbolism). The symbolic protocol layer remains TLA+ (#125).
HARNESS EVOLUTION: P_RELIED was strengthened after M-K2 exposed the
    weak first encoding ("provider installed" → "provider's provision
    resolvable"); no production behavior difference; no production change.
```

### FV-CONC-0 (Phase B2) — playback concurrency

Details: `specs/playback-concurrency/RESULTS.md`.

```text
LOOM 0.7.2 over the REAL PcmEdge (cfg(loom) drop-in sync primitives):
    L1 write×read×stop (FIFO ring integrity, terminal monotone)
    L3 first-terminal-wins (EOF × stop)
    L4a blocked producer × stop wakes
    L4b blocked consumer × {EOF, stop} wakes
    → SCHEDULE-CLEAN (full interleaving exploration; ≤3 threads, ≤2 samples)
NEGATIVE CONTROL: M-L1 drop data_ready notify → COUNTEREXAMPLE-WITNESSED
    (loom reports the deadlocked schedule).
SessionCompletion: explicitly NOT loomed (wait_timeout unmodeled);
    covered natively + stress + Miri-adjacent runs.
```

### FV-UB-0 (Phase B3) — Miri per crate

```text
qianqian-composition         MIRI-CLEAN   79 tests (lib + integration + matrices)
qianqian-playback
  --test edge_lifecycle      MIRI-CLEAN   14 tests
  --test k0_firewall         MIRI-UNSUPPORTED — the 10 s watchdog oracle assumes
                             native speed; Miri's interpreter slowdown trips it
                             (oracle scope, not a code finding)
  --test session_activation
  --test test_oracles        MIRI-UNSUPPORTED — /proc/self/task leak oracle uses
                             opendir; Miri isolation forbids it (system-layer
                             oracle by design; covered natively + stress)
qianqian-audio-api           lib has no tests; trybuild compile-fail suites are
                             type-system evidence, excluded from Miri by nature
qianqian-decode-songcore     NOT VERIFIED BY MIRI — FFI into the native songcore
  qianqian-output-wasapi     static lib (Linux) / WASAPI COM (Windows-only)
```

### FV-INTEGRATION-0 (Phase B4) — system evidence

```text
2 pinned cores × 2 test threads × 40 reps   0/40 iterations failed
  (session_activation + edge_lifecycle; leak oracle active every rep;
   covers mount/activate/play/EOF, stop-mid-play, decode-failure,
   provider-withdrawal, dispose lifecycle loops)
1 pinned core  × 1 thread  × 10 reps        0/10 iterations failed
unrestricted  × 8 threads  × 10 reps        0/10 iterations failed
Native decoder gate (songcore FFI):         NOT RUN — the prebuilt native
  artifact is absent on this host; the crate fails closed (recorded, not
  fabricated). Windows real-device gate:    DEFERRED — no Windows runner.
```

## Differentials & classification

```text
1. Kani symbolic TOOLING-INSUFFICIENT while TLA+ and the matrix channel
   are clean  → not a contradiction: the symbolic Kani layer simply was
   not earned this round (cost, not soundness). Recorded as a tooling
   gap with concrete bounds; a future round needs collection-light
   verification seams (design question — Authority resolution path).
2. M-K2 exposed a weak harness oracle (P_RELIED v1)  → HARNESS/ORACLE
   DEFECT, fixed; production identical before/after. The mutation
   control did its job: it caught the verifier, not just the code.
3. Miri watchdog + /proc isolation failures  → ORACLE/environment scope
   mismatches, classified MIRI-UNSUPPORTED; no production claims made.
4. No Case A/B/C/D contradiction (clean-in-one-layer ×
   counterexample-in-another) was observed between layers this round.
```

## Production defects found (this campaign round)

```text
none  (the #121→#127 oracle corrective was Phase A, pre-campaign base;
       B1–B4 produced no new production counterexample)
```

## Authority gaps

```text
none identified. (A possible future design question, not a gap: whether
collection-light verification seams should exist to make the kernel
Kani-symbolic-verifiable — if pursued, it goes through Authority
resolution, not silent refactoring.)
```

## Unverified surfaces (explicit)

```text
1. Kani symbolic proofs of K1–K6 (TOOLING-INSUFFICIENT; TLA+ holds the
   symbolic layer, matrices hold the Rust layer)
2. songcore FFI memory safety (native artifact absent here; needs the
   equivalence/ASan gate on a host with the artifact)
3. WASAPI COM render path (Windows; deferred)
4. SessionCompletion under loom (wait_timeout unmodeled; native+stress only)
```

## Final fresh-context adversarial review

Two independent reviewers, no prior campaign context:

```text
Reviewer A (architecture / authority):  REQUEST CHANGES → resolved
    MAJOR: the K3 row claimed "tombstone authority" although no harness
    exercised the tombstone dimension. Fixed: K4b now asserts the
    violated inverse retains its provenance tombstone in the
    accumulator, and the claim rows name K4b. All MINOR findings
    (route dedup, K5 one-directional wording, scenario-count
    reconciliation, native-channel runner, report committed) applied.
Reviewer B (verification soundness):    APPROVE (with findings)
    Independently re-executed all three kernel mutation controls and
    the loom negative control, spot-checked Miri CLEAN/UNSUPPORTED
    claims, confirmed harnesses exercise production code (no copies)
    and that the Kani non-result is recorded with concrete bounds.
    Its doc-accuracy findings were applied in the same pass.
```

## FINAL VERDICT

```text
PASS_WITH_LIMITATIONS
— every property ran with explicit authority, explicit property,
  explicit bounds, and at least one negative control where the campaign
  demanded them; no layer claims more than it ran; the Kani symbolic
  layer's absence is recorded as a tooling result, not papered over.
```
