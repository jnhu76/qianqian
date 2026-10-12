# R-H H0 consolidated remediation — lessons learned

Non-authoritative campaign document (#260 §3). Each lesson is evidence-backed by a run in
`h0-execution-evidence-index.md`; each states where it should NOT be generalized.

## 1. Vacuity and premise-by-timing hide inside fixtures, not assertions

- **Root cause.** Four of the eighteen H0s (R3A-P2, R3A-P1, R3A-P3×2) have sound semantic
  oracles whose PREMISE rests on a fixture timing fact: a finite source "still playing" at
  stop, a sleep standing for a teardown release, a 200ms sleep standing for a worker poll
  boundary. The assertion passes; the property under test was exercised only because the
  schedule usually cooperated.
- **Evidence.** R3A-P2's `seconds_of_audio(30)` is capacity-relative against decode drain;
  R3A-P1's own comment admits the safe-interleaving degradation; R3A-P3's in-comment
  third-cutover observation (Windows runner) proves the retry-loop alternative is wrong in
  both directions.
- **Reusable review rule.** For every wait/sleep/finite fixture in a concurrency test, ask:
  *what premise does this stand for, and is that premise structural (an endless source, a
  held probe, a fence) or schedular (a duration)?* Schedular premises are registered debts,
  not silent greens — and the fix is a structural twin (e.g. `endless_audio`), never a longer
  sleep.
- **Where not to generalize.** A bounded `wait_until` poll with a NAMED failure is not a
  schedular premise; it is a diagnosis boundary. Do not convert every duration into a
  structural witness — some windows (one worker poll slice) have no public structural
  witness at all, and inventing a production seam for them is an owner decision (OQ-1/OQ-6).

## 2. Identity masking: a surviving mutant is information, not a failure of the exercise

- **Root cause.** R3B-P1-06's acceptance conditions are checked by TWO holds sampling the
  same conjuncts. The first pre-registered mutant (delete the conjunct at the second hold
  only) SURVIVED — masked by the first hold's twin. The reflex "rerun until red" would have
  been mutation of the test until it kills, not evidence.
- **Evidence.** M-P106a survives (`20261012T135034-570017-23909`); M-P106b (both holds)
  kills (`20261012T135134-570685-13005`). Diagnosis came from the preserved log, before any
  rerun, per the failure-evidence contract.
- **Reusable review rule.** When a conjunct appears at more than one sample point,
  single-site deletion is masked FOR EVERY EXISTING TEST whose executions keep the
  sample points ordered — pre-register the both-site deletion, or explicitly record the
  masking as a finding. But do NOT call the single-site mutant "property-equivalent":
  if the sample points are not one atomic predicate (locks dropped between them, as in
  request_seek's two holds), a concurrent writer in the inter-sample window can
  distinguish them — the mutant is only unobserved-by-current-tests, and killing it
  needs a twin that straddles the window (round-1 adversarial correction of this very
  document's first wording).
- **Where not to generalize.** Redundant sampling is not automatically a defect; here it is
  the documented cheap-filter/atomic-decision design. Do not "fix" redundancy by deleting a
  hold — that is production semantics.

## 3. Freeing a resource while its payload is still unconsumed fabricates a legal-looking state

- **Root cause.** R3B-P1-05's fixture mirrored the production slot-free call but skipped the
  payload-consumption step the production order guarantees first — and the comment asserted
  the skipped step had happened. Downstream oracles were then proven against debris.
- **Evidence.** Probe `20261012T134814-568156-31126`: the cycle-1 `Committed{42}` payload was
  still routed at the free point.
- **Reusable review rule.** White-box fixtures that drive an ordered protocol must traverse
  the production-permitted states only; every "consume/free/release" step in the comment must
  have a corresponding observable call in the code. A comment that narrates a step the code
  does not perform is a finding by itself.
- **Where not to generalize.** Probes may legally construct forbidden states to prove
  production forbids them — the distinction is whether the surviving oracles are grounded in
  the legal transition or in the debris.

## 4. False RED is the mirror image of vacuity, and the harness can cause it

- **Root cause.** R3A-P2's finite fixture could complete the episode before the stop landed
  (false RED on `Stopped`); in this campaign a NEW harness witness (`Vec` growth per block)
  tripped the binary's measured zero-allocation oracle (5 allocations) — the same class from
  the other side: a witness heavy enough to change the property it observes.
- **Evidence.** Failed attempt `20261012T135455-571083-473` (measured 5), fixed by an O(1)
  witness; run `20261012T135753-571385-24750`.
- **Reusable review rule.** In binaries that pin measured budgets (allocation, kernel ops),
  any new per-iteration state must be O(1) and allocation-free BY CONSTRUCTION; check the
  witness against the binary's own budget oracles before running the suite.
- **Where not to generalize.** O(1) is sufficient here because the witness records
  provenance classes (same/different, first address), not per-block history. If a property
  ever needs the full per-block sequence, the binary needs an explicit budget amendment, not
  a silent Vec.

## 5. Release != join; a detached witness leg is a test-suite correctness bug

- **Root cause.** R3A-A1/A2's spawned legs discarded their handles; premise failures and
  recv failures stranded parked threads until process exit, and post-release joins were
  unbounded (a broken-release regression would hang the suite instead of failing).
- **Evidence.** The file-local idiom (`fail_premise_releasing_leg`, now generalized) existed
  in-file from PR #254 — the defect was coverage, not knowledge: six later tests in the same
  file and three in the sibling file had not adopted it.
- **Reusable review rule.** In test files that spawn legs: (a) every `thread::spawn` handle
  is either joined on the success path or routed through a panic-safe release+collect helper
  on every failure path; (b) every post-release `join` is bounded by an `is_finished` witness
  with a named panic; (c) a leg parked under pause intent has a DIFFERENT exit (the pause
  release) than one parked under a seek hold — a seek-only release is consumed MID-PARK and
  the leg keeps parking.
- **Where not to generalize.** Do not convert deterministic single-threaded tests into
  thread-DAGs to make them "hygienic"; the discipline applies only where legs exist. A
  watchdog (`within`) bounds failure reporting — it must never be claimed as the correctness
  oracle (R3B-P1-03's oracles stayed semantic). The same holds for every bounded wait this
  campaign added (5s join bounds, 10s `wait_entered_bounded`, the 10s watchdog): they are
  machine-speed FAILURE bounds that turn a hang into a named RED — they are not semantic
  oracles, and a red they produce must be diagnosed as "bounded wait expired", never read as
  the property being false (round-1 adversarial review, on-the-record).

## 6. Watchdogs, RAII guards and scope-limited mechanisms: state what each can and cannot do

- **Root cause.** R3A-A6's suggested remedy ("add Drop release to the gate") cannot fire in
  the exact stranding scenario it targets, because the blocked destructor holds its own Arc
  clone — a Drop that provably never runs would have been decorative, and claiming it as the
  fix would have been a false claim.
- **Evidence.** The implemented stack: `wait_entered_bounded` (named premise failure),
  `GateReleaseGuard` (release-on-unwind held by the test), non-panicking `Drop` (documented
  scope: shapes where the last handle drops while a destructor waits). Each mechanism's
  coverage is written on the type.
- **Reusable review rule.** For every proposed harness mechanism, write the failure-path DAG
  and check the mechanism fires in it; a mechanism whose precondition cannot occur in the
  target scenario is scope creep even when harmless.
- **Where not to generalize.** RAII guards are for harness-owned resources (test-only files);
  product teardown ordering stays under the ADR authority (D14.7) and must never borrow the
  harness pattern as precedent.

## 7. Executor/runtime provenance and honest NOT_RUN are part of the result

- **Root cause.** Three attempts of the R3B-P2-01 commit failed before green (two compile,
  one semantic). Under log-washing pressure each could have disappeared; the semantic failure
  (lesson 4) was load-bearing evidence about the binary's own oracle.
- **Evidence.** All attempts retained with distinct RUN_IDs; the final full matrix and
  NOT_RUN lanes (Windows, TLC, FFI, Miri, device) recorded with reasons in
  `h0-execution-evidence-index.md`.
- **Reusable review rule.** Full log + cmd + sha + env + rc BEFORE any rerun; reruns are new
  files; a red is diagnosed (identity + signature) from its preserved log; channels that did
  not run are written NOT_RUN with the reason — never inferred from a green elsewhere.
- **Where not to generalize.** NOT_RUN with a scope note (formal models byte-identical to the
  frozen subject) is a statement about applicability, not a claim that the formal property
  was re-verified.

## 8. Campaign-shape lessons (ONE_PR_MULTIPLE_COMMITS)

- The #260 group table's 18-row arithmetic (1+1+2+6+1+1+1+1+1+1+2) was reconstructed
  independently from the #258 inventory CSV (sha256 verified) before any edit; two anchors
  had drifted line numbers and were re-pinned at the actual subject before work started.
- The suggested commit order (premise corrections → storage falsifier → gate hygiene → loop
  exit → OQ-conditioned → docs) kept every commit independently reviewable; the only
  cross-commit coupling was the shared file-local idiom in render_gate_seek.rs (A1 before A2),
  which matched the audit's own dependency note.
- The OQ-conditioned rows (R3A-P1, R3A-P3×2) took the longest to decide and changed the least
  code (zero): the correct outcome of "investigate a legal deterministic witness" was
  sometimes "none exists without owner authority" — recording that honestly is the
  deliverable, not a forced fix.
