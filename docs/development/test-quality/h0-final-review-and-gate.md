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

Round 1 (adversarial oracle/causal + architecture/scope/evidence): **PENDING** — this
section is updated in a documentation-only commit after each review round; blockers, if any,
land as separate commits with revalidated lanes.
