# R-H H0 consolidated remediation — final review and merge gate

Status at PR open: **DRAFT — not merge-ready by construction** (#260 G4: two fresh-context
independent reviews required; PR stays DRAFT until the owner's Ready decision; no self-merge).

## Completion record (exit rule)

- 18/18 H0 tests explicitly dispositioned in `h0-before-after-ledger.csv`:
  `FIXED_AND_FALSIFIED 2 · FIXED_HYGIENE_VERIFIED 13 · KEEP_WITH_EXPLICIT_OWNER_DEBT 2 ·
  AUTHORITY_BLOCKED 1 · NEEDS_REVIEW 0`.
- Because one row is `AUTHORITY_BLOCKED` (R3A-P1 / OQ-1) and two rows carry the OQ-6
  keep-with-debt ruling, **this PR must not be represented as "18 H0 fixed"**: 15 rows carry
  code-level repairs (2 of them falsifier-killed), 3 rows are owner-decision debts preserved
  with fresh investigation evidence.
- Zero unowned/unknown tests touched; one test identity ADDED
  (`partial_middle_block_storage_substitution_is_killed_by_storage_identity`), zero removed;
  seven test bodies materially changed (premise/fixture/failure-path hygiene); no production
  file outside `crates/qianqian-playback/src/completion.rs` tests module and the four
  test-only harness files was touched; no public API, CI/branch policy, Lefthook,
  dependency, or ADR change.

## Gates executed at the test-tree HEAD

Full matrix and RUN_IDs: `h0-execution-evidence-index.md`. Summary: workspace 862/0 (+1 new
test vs the 861 baseline), loom lane 12/12, fmt clean, clippy `-D warnings` clean, negative
controls PR #257 M2/M3 RED as expected, TLA+/Windows/FFI/Miri/device lanes NOT_RUN with
reasons.

## Merge gate decision (owner)

- REQUIRED: owner ruling on OQ-1 (fund white-box `teardown_released` witness vs accept the
  vacuity window) — the only AUTHORITY_BLOCKED row.
- REQUESTED: owner re-confirmation of the OQ-6 keep-with-debt standing (investigation
  recorded; no superseding proposal made).
- The RH-tag delta (H0 → resolved / debt) is a DERIVED delta to apply to #258's bookkeeping
  only AFTER a merge decision — the frozen 918-row baseline is not edited here.

## Fresh-context independent reviews (G4)

### Round 1 (both reviewers ran against HEAD `cb18af5`; read-only, fresh context)

- **(a) Adversarial oracle/causal reviewer: APPROVE** — no BLOCKER/MAJOR. Independent
  re-runs of the focused suites matched every documented count. Two MINOR findings, both
  documentation-only, both FIXED in this commit: (1) the M-P106a classification was
  overstated as "property-equivalent" — the two request_seek holds are not one atomic
  predicate (lock dropped between them), so single-site deletion is only
  *unobserved-by-existing-sequential-tests*, distinguishable by a concurrent
  `release_pause_gate` in the straddle window (wording corrected in h0-mutant-matrix.md,
  h0-property-graph.md, h0-lessons-learned.md); (2) an explicit on-the-record sentence that
  all new 5s/10s bounded waits are machine-speed failure bounds, never semantic oracles.
- **(b) Architecture/scope/evidence reviewer: APPROVE** — no BLOCKER/MAJOR. Scope verified
  test/docs-only (15 files; completion.rs hunks inside the tests module); no public API; no
  H1/H-D/R-M items touched; independent suite re-runs reproduced all documented counts;
  disposition counts consistent (2/13/2/1=18); frozen #258 baseline untouched. Four MINOR
  findings, all documentation-only, all FIXED in this commit: PR-body commit count
  (11, not 12); GitHub CI outcomes folded into the evidence index; the M2 first-attempt
  invalid run narrated (cargo argv error, retained); the run/commit subject relation
  (test-then-commit + header-rewrite SHA mapping) stated explicitly.

Round-1 blocker fix commits: NONE required (no blocker; the six MINOR corrections above land
in this documentation-only commit and the PR-body correction).

### Round 2

PENDING — re-run of both fresh-context reviewers over the final HEAD including this commit;
results recorded here before the PR leaves DRAFT.
