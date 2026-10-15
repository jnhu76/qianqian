# tools/test-quality — R2 opt-in checker pilot (NOT a mandatory gate)

```text
STATUS      = OPT-IN PILOT (R2 calibration, #244/#248/#249 campaign)
              NOT installed in lefthook, NOT in any GitHub CI job,
              Since TEST-QUALITY-STANDARD-v1
              (docs/development/test-quality-standard.md section 11) its output
              is defined as a REVIEW_SIGNAL: it may rank review and can never
              adjudicate adequacy. See
              docs/development/test-quality/r2-checker-calibration.md
              for the calibration record behind this copy.
CHECKER     = check_test_quality_prototype.py  (prototype grade:
              stdlib Python 3, regex + brace walking, NO Rust AST)
VERSION-ID  = sha256 ab704dd309818fd7bab3c759223445ce46c12bad8e6a9f30b0fc8d594d3105d8
              (16028 bytes; byte-identical to the R1-B REV.3 prototype at
              /home/hoo/Source/qianqian-r1-reports/flash-b/checker-prototype/
              and to the source inlined in #248 issuecomment-6108683301)
SELF-TEST   = python3 tools/test-quality/check_test_quality_prototype.py self-test
              -> SELF-TEST OK (6/6), exit 0 (verified from this copy)
```

## What this is

A **read-only heuristic signal generator** for test-quality review. It never
runs Rust code, never modifies files, and — by design boundary — **its output
is a Review Signal, never a verdict that a test is semantically wrong**
(#249 REV.5 machine-layer boundary; the R1-B checker own doc header states
the same). Every signal goes to human/agent domain review for a three-value
adjudication (confirm / refute / defer).

## Usage (all commands run from the repository root)

```bash
# 1. Checker's own negative controls (must print SELF-TEST OK (6/6))
python3 tools/test-quality/check_test_quality_prototype.py self-test

# 2. H1-H4 signal scan of a test-bearing Rust file (exit is ALWAYS 0;
#    signals are reported, they do not fail anything)
python3 tools/test-quality/check_test_quality_prototype.py scan crates/qianqian-audio-api/tests/render_gate_seek.rs

# 3. TER/Evidence Record structural validation (exit 1 on structural
#    violation; P-class checks are structural ONLY — a PASS says the
#    filing is complete, nothing about test adequacy)
python3 tools/test-quality/check_test_quality_prototype.py record <record.md> [subject-file.rs]

# 4. H5 oracle-weakening shape scan on a before/after pair (exit ALWAYS 0)
python3 tools/test-quality/check_test_quality_prototype.py oracle-diff <before.rs> <after.rs>
```

## Signal catalogue (summary)

| ID | shape | known calibration result (R2, HEAD 735328e) |
|----|-------|---------------------------------------------|
| H1 | bare (non-poll-loop) `sleep` inside a `#[test]` fn | fires on premise sleeps (R0B-01 BASE :173) AND on documented diagnostic windows (fixed render_gate_seek :199) — lexical layer cannot tell them apart; review classifies per S1–S6 (S6 = PREMISE_DEBT) |
| H2 | inline asserted single `try_recv` sample | fires on its fixtures; no R2 sample in this repo yet |
| H3 | spawned handle never joined / unbound spawn whose closure never joins | fires ×2 on the open_abort exit-witness + join-watcher idiom — documented FP class, review refutes per-signal (no file exemption exists) |
| H4 | doc-comment claim word (`post-EOF`, `transition`, `parked`, …) with no witness line carrying the same token | weak lexical signal; fires on legal absence-properties (pause_seam negative control :561) and helper-witnessed claims (:429/:601) |
| H5a/b/c | assert-count drop / `assert_eq!`→`assert!` / wait-budget ≥10× amplification | **on real fix pairs all R2 firings were positional-zip misalignment FPs** — see calibration §H5 before trusting it |
| P2a/P3/P4a | TER structural checks (record mode) | fail-closed; P3 path resolution checks CWD-relative paths |

## Caveats carried verbatim from the prototype (do not "fix" silently)

- `record` mode resolves cited paths relative to the **current working
  directory** (plus one absolute fallback to a Flash-B working path that does
  not exist in this checkout) — run it from the repository root.
- H5 pairs before/after assert lines **positionally**; any inserted
  assert-like line (including `assert_no_leg_threads();`, which matches the
  substring `assert`) shifts the pairing and fabricates H5b signals.
- H5c compares an after-file wait line against the **first** same-unit
  `Duration` in the whole before file, which is frequently an unrelated
  struct field.
- Changing any of these would produce a NEW checker version; this copy is
  frozen at the calibrated sha256 so the calibration record stays traceable.
  Alterations must go through the campaign's review path (#248/#249), not a
  local edit.

## Provenance and authority

- Source of truth for the design: #248 (R1-B REV.3; prototype source inlined
  in issuecomment-6108683301, calibration report in issuecomment-6108681112)
  and #249 REV.5 (adequacy/machine-layer vocabulary).
- R1 final adjudication accepted the design **for controlled R2 validation
  only**; Skill/Checker/CI installation as a norm was NOT authorized. This
  directory is the opt-in pilot artifact, not an installation.
