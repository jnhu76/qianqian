# Test-quality standard: adequacy, admission, and evidence

```text
STATUS      = ADOPTED — TEST-QUALITY-STANDARD-v1  (2026-10-15)
TRUTH CLASS = TEST-PROCESS AUTHORITY
              how a test claim is authored, reviewed, admitted and evidenced
NOT         = product semantics, runtime mechanism, architecture vocabulary,
              CI policy, or any new authority over production behavior
ORIGIN      = Umbrella #244 (QIANQIAN-TEST-EXECUTION-0)
              R0 #245 / #246 · R1 design #247 (candidates #248 / #249)
              R2 pilot PR #250 · R3 #251 (A/B) · Wave A PR #254
              R3-B #255 (+ independent review) · method #262
SUPERSEDES  = the R1 candidate texts in #248 / #249 IN THEIR CANDIDATE ROLE.
              They remain audit history in the tracker; this file is the single
              repository-side test-process authority.
APPROVAL    = R1 independent gate `APPROVE_DESIGN_FOR_R2` (#247), R2/R3 pilots and
              the R3-B independent portfolio review supply the executable evidence;
              the owner adoption decision is recorded in #247 and the PR that
              adds this file.
```

This document explains how to decide whether a test is **adequate for the property it claims**, how the repository records that judgement, and what evidence must be preserved. It defines no playback state, ordering guarantee, lifecycle, K0 primitive, ADR noun or product semantic. Product truth stays with the ADRs and the design authorities routed by `docs/README.md`; tests are counterexample-seeking evidence.

## 1. Purpose and non-goals

```text
The question this standard answers:
    What makes a test adequate for the property it claims,
    as opposed to merely successful?

Non-goals (explicitly out of scope, not merely deferred):
    new Test Runtime / DSL / property registry / central oracle
    automatic state extraction or a generic Rust scheduler
    new K0 primitives or product semantics
    universal mutation-score or MC/DC targets
    forcing an execution DAG on a pure-function test
    retroactive bulk rewriting of the existing suite
    promoting a fixture scheduling assumption into product authority
```

The optimization objective is not the largest number of tests, nor the smallest test at any price:

> For each accepted property, use the smallest resource footprint and the narrowest sufficient observed behavior, while keeping genuinely necessary integration and real-boundary tests and causally valid oracles.

## 2. Adequacy is six orthogonal dimensions

```text
D1 EXECUTION    the test is really discovered and executed at the claimed SHA,
                platform and feature configuration (DECLARED -> DISCOVERED ->
                EXECUTED -> CI-COVERED are four distinct facts)
D2 STRUCTURAL   the relevant production decisions/paths are reached
                (coverage is a lower-bound fact field, never a gate)
D3 SEMANTIC     the assertion covers the authority-legal state/transition/outcome
                space, including the illegal transitions that must be refused
D4 CAUSAL       every cross-boundary shape the assertion relies on is carried by a
                real happens-before edge or is constructively established;
                sleeping/timing may only serve a role this standard assigns it
D5 ORACLE       the assertion samples at the boundary that OWNS the truth;
                every legal outcome is either constructed away or independently
                asserted
D6 NON-VACUITY  a named falsifier exists; high-risk properties carry a committed
                negative control or executed targeted mutation; non-empty-run
                guards are in place
```

The asymmetry is the practical lesson of R0-B: all-green D1 and D2 prove nothing about D3 through D6, and the confirmed defects concentrated in D4/D5/D6. Governance budget therefore goes to causality, oracle boundary and non-vacuity, not to more execution plumbing.

```text
PASS        is a regression fact (a D1 property)
ADEQUATE    is the conjunction of the six dimensions for the CLAIMED property
```

A test may pass forever and still be inadequate (a vacuous oracle), and an adequate test may fail occasionally (it exposed a real defect). The two require opposite responses and must never be conflated.

## 3. Two independent axes: mechanism M and risk R

A single linear tier scale is wrong in both directions: a critical pure function must not be forced to draw a concurrency DAG, and an ordinary threaded test must still prove its happens-before edge. Mechanism and risk are therefore independent axes; verification methods are **derived** from them, never chosen freely.

```text
M -> which methods are APPLICABLE to the property
R -> how DEEP each applicable method must go, and who reviews
```

### 3.1 M axis — the execution mechanism of the CLAIMED PROPERTY

```text
M0  deterministic data transformation / pure function
    synchronous, single-threaded, no internal state transition

M1  complex decision / branch-dense logic
    condition combination decides the output; no cross-thread timing

M2  state machine / lifecycle / protocol sequencing
    authority-frozen states and transitions; the property holds through a
    single-owner sequential progression (resolver, episode protocol ordering)

M3  concurrency / scheduling dependence
    the property holds through cross-thread order, synchronization or lifetime
    mechanisms in production code (async completion, locks and wakeups,
    teardown/release linearization)

M4  cross-layer / FFI / platform
    ABI, native artifacts, real backend or device, platform behavior
```

Classification asks about **the claimed property times the production code**, never about the strength of the existing test:

```text
Does the property hold only through cross-thread order/synchronization/lifetime
mechanisms in production?          -> M3
Only through single-owner sequential progression? -> M2
Only through condition combination?               -> M1
Pure computation?                                 -> M0
```

Never classify by asking whether deleting production synchronization would turn the current test red. That is circular: a vacuous concurrent test can miss the removed synchronization and be misclassified as non-concurrent, then exempted. Weak detection is a D5/D6 adequacy problem recorded in the verdict; it does not change the class. Never classify by test name, file name, thread count or the presence of a keyword.

### 3.2 R axis — the cost of the property being wrong

```text
R-L  local convenience / internal detail     breaking it annoys developers only
R-M  product behavior                        user-visible behavioral regression
R-H  semantic authority / high-impact edge   D11 terminal truth, P1-P5,
                                             FFI ABI, teardown/release order,
                                             realtime boundary, consumed-content
                                             integrity witness
```

The R question is: **if this oracle were wrong or vacuous, what is the worst outcome?** Hiding a semantic-authority or lifetime defect is R-H; a user-visible behavior regression is R-M; everything else is R-L.

Anchor set (prevents risk inflation). Explicitly **not** R-H: presentation detail such as diagnostic text (the R0B-04 lesson is exactly that presentation was pinned as semantic strength), UI layout, log wording, non-authoritative projections.

### 3.3 Tier labels T0-T3 are DISPLAY ONLY

```text
T0 = M0 (any R)                       ordinary unit test, light admission
T1 = M1 or M2 (R <= R-M)              decision/state test, medium admission
T2 = M3 (any R) or (M1|M2 and R-H)    concurrent or critical decision/state test
T3 = M4 (any R) or (R-H cross-layer)  high-impact property, independent falsifier
```

T0-T3 exist for summarising and communication. They are **not an inheritance chain**. The only source of prescriptive obligation is the matrix in section 4; any statement of the form `T2 = T1 + X` or `T3 = T2 + Y` is repealed, and on conflict **the matrix wins**. In particular:

```text
R-H with M1/M2 shows the T2 label and still OWES NO EXECUTION DAG
    (an execution DAG is NOT APPLICABLE to M0/M1/M2).
M4 owes its real mechanism class baseline plus cross-layer declarations --
    it is not the union of everything below it.
R changes DEPTH, never the method set.
```

Negative calibrations (both directions, from the accepted R1 contract):

```text
(a) R-H x M1, non-concurrent: R-H adds depth only (decision-table form,
    independent falsifier evidence, independent review) and never an
    execution DAG, even though the label reads T2.
(b) M3 with a weak oracle, any R including R-L: weak detection does not
    remove the DAG/premise obligation. It is a D4/D6 defect recorded in the
    verdict; the method set is unchanged.
```

## 4. The prescriptive matrix

Legend: `REQ` REQUIRED (missing means the test is not adequate unless an explicit EXC is registered) · `REC` RECOMMENDED (missing must be justified in review) · `OPT` OPTIONAL · `N-A` NOT APPLICABLE (writing it is noise and review should reject it) · `EXC` EXCEPTION allowed only with a ledger record of the reason.

### 4.1 Table A — mechanism M x method V (applicability)

| Method V \ Mechanism M | M0 pure | M1 decision | M2 state/lifetime | M3 concurrent | M4 cross-layer |
|---|---|---|---|---|---|
| meaningful assertion + non-empty run | REQ | REQ | REQ | REQ | REQ |
| boundary / negative input | REQ | REQ | REQ | REC | REQ |
| branch / path reachability (fact field) | REC | REQ | REQ | REQ | REC |
| decision table (aligned to authority) | N-A | REQ | REC | N-A | N-A |
| MC/DC | N-A | OPT | N-A | N-A | N-A |
| state-transition analysis (authority vocabulary) | N-A | REC | REQ | REC | N-A |
| illegal-transition / forbidden-branch assertion | REC | REQ | REQ | REQ | REC |
| minimal execution DAG sketch | N-A | N-A | EXC 1 | REQ | N-A |
| premise carried by a real synchronization edge (S1) | N-A | N-A | REC | REQ | N-A |
| legal-outcome enumeration (one assertion per branch) | N-A | REC | REQ | REQ | REC |
| observation at the owning settlement boundary | REQ | REQ | REQ | REQ | REQ |
| committed negative control | OPT 2 | OPT 2 | REC | REC | OPT |
| targeted mutation / property / formal anchor | OPT | OPT | OPT | OPT | REC |
| Loom (bounded interleaving model) | N-A | N-A | N-A | OPT 3 | N-A |
| Miri / Kani | OPT 4 | N-A | N-A | OPT 3 | N-A |
| TLA+ (bounded model check) | N-A | OPT | OPT | OPT | N-A |
| real-path / platform evidence claim | N-A | N-A | N-A | N-A | REQ |
| failure-path cleanup (release / nothing dangling) | N-A | N-A | REC | REQ | REC |
| exit witness / bounded join | N-A | N-A | OPT 5 | REC | OPT |

1. EXC: a single-threaded M2 test whose assertions do not depend on any cross-thread shape is exempt; it then simply carries the ordinary M2 obligations.
2. R-H raises this to REQUIRED (Table B).
3. Expanding Loom/Miri requires the earning rule in `concurrency-test-design.md` §C: a concrete interleaving risk, a contract-owned invariant, stated bounds, and an argument that the existing tools are insufficient. The existing PcmEdge slice is an earned boundary.
4. Miri has real value on M0 unsafe/FFI-adjacent code such as the songcore-sys binding layer, hence OPT rather than N-A.
5. A single-owner sequential M2 property (for example a white-box resolver) generally has no join; the presence of a join means the property drifts toward M3.
6. Row `premise carried by a real synchronization edge (S1)`: satisfied by latch/lock/channel/join/atomic publication semantics, or by a bounded poll of a **persistent** state (once true it stays true, so observing it establishes the edge). A bare sleep before acting, or a checked wait placed **after** the dependent action, is only a detector and does not satisfy this row, however clearly its failure message describes itself. Such sites fall into the S6 premise window: fix it if fixable (a defect), otherwise register the debt. Neither disposition changes the fact that the row is unsatisfied.

### 4.2 Table B — risk R depth modifier

| Obligation | R-L | R-M | R-H |
|---|---|---|---|
| six-question record (section 5) | M0: two questions | REQ | REQ + visible in the PR |
| independent falsifier evidence (one of: negative control / targeted mutation / formal anchor) | - | - | REQ |
| independent (fresh-context) review | not needed | sampling | REQ |
| decision-table form (M1/M2) | - | REC | REQ |
| ledger depth | row exists | + execution/CI fields | + full fields + authority chain |
| re-review trigger on production/ADR change | follows normal | follows normal | REQ explicit evaluation |

Usage rules:

```text
- Read Table A down the M column to get the applicable method set, then Table B
  across to deepen it by risk.
- Classify per TEST, not per file: one file may contain several mechanism classes.
- N-A is a prohibition signal: an execution DAG for M0, or Loom for M4, is
  over-engineering and review should reject it.
- Every EXC must be recorded in the ledger with its reason, otherwise the row is
  treated as unmet.
```

## 5. The authoring record: six questions, scaled by mechanism

When adding or materially modifying a test, answer the questions the mechanism class requires. A compact comment block or the PR rationale is enough: these are reasoning prompts, not a metadata framework, and not a new production type. They are compatible with the five-question contract in `concurrency-test-design.md`, which this standard incorporates as the M3 case.

```text
PROPERTY        What invariant is claimed? Cite the owning ADR/design clause. If it
                is only implementation-regression evidence, say so explicitly as a
                narrowed scope; never disguise it as a product guarantee.
PRECONDITION    Which starting states and ordering edges must hold, and which real
                event/lock/latch/channel/join/persistent-state poll establishes
                each one? A checked wait placed AFTER the dependent action is only
                a detector (section 7). Can the fixture make the awaited state
                unreachable?
LEGAL OUTCOMES  Which outcomes are legal under those premises? Is one deliberately
                constructed, or is a race intentionally left open?
OBSERVATION     At which truth-owning boundary does the assertion hold? Boundaries
                are distinct owners, not one timeline; do not sample above the
                boundary that owns the fact (section 7).
FALSIFIER       What specific illegal behavior turns this test red? Could the
                assertion be vacuously true? State the predicted failure signature.
                Mark it STATIC, EXECUTED_MUTATION or INCONCLUSIVE honestly.
EXECUTION       Did the intended test actually run, non-empty, in which
                platform/feature configuration, with which bounds? On failure,
                distinguish RELEASE, actual JOIN and process termination, and show
                the failure-path cleanup.
```

Per-mechanism obligation (T labels shown only for navigation):

```text
M0 (T0)      PROPERTY + OBSERVATION, one sentence each; may live in the code
M1/M2        all six questions (one comment block is fine). R-H adds depth from
             Table B -- decision-table form, independent falsifier, independent
             review -- and NEVER an execution DAG
M3 (T2)      all six questions + a minimal execution DAG sketch (text in the PR
             description is enough) with the real edges labelled by publisher and
             primitive
M4 (T3)      the baseline of its real mechanism class + a real-path/platform
             evidence claim + a named falsifier (one of negative control,
             targeted mutation, formal/property evidence); REQUIRED independent
             falsifier when R-H
```

## 6. Sleep and timing taxonomy S1-S6

Sleeping is not forbidden; it is classified, once per site. Every confirmed R0 defect was a case of a sleep or a sample carrying a role it cannot carry.

```text
S1 SYNCHRONIZATION
   Establishes happens-before. Legal primitives: lock/atomic publication/mpsc/
   Condvar/join/spawn, and bounded polling of a PERSISTENT condition (wait_until
   on a state that stays true once true -- an event log, a monotone projection,
   a published name). A sleep is NEVER S1.
   Rule: a shape that only S1 can guarantee must have an S1 edge or be
   constructively established, otherwise the test has a premise gap (D4).

S2 DIAGNOSTIC PERTURBATION
   Temporarily inserted to amplify a suspected interleaving. Legal only if
   labelled, predicted in advance, with site/count/platform recorded, and REMOVED
   after reproduction. Left in the code = violation.

S3 LOAD SIMULATION
   Intentional source pacing or external-delay modelling (wall-clock pacing).
   Legal; the property must still be verified at its real observation boundary;
   the purpose must be commented.

S4 SETTLEMENT WINDOW
   A bounded absence window after a settled boundary. Legal only when it carries
   an ABSENCE assertion (some state/event does not appear within the window) and
   never a positive must-happen premise. Window and purpose commented. A positive
   shape asserted after the window is S6, however clearly its failure message
   describes itself.

S5 WATCHDOG
   A failure sentinel (timeout = red). Never evidence of ordering; declare the
   budget.

S6 PREMISE WINDOW (PREMISE-DEBT)
   A time window used as a positive premise that X must happen (S1 in disguise).
   This is a statically derivable D4 causal gap, independent of the margin
   multiple: if the counterpart worker/leg/settler is not scheduled inside the
   window, the dependent action lands before the premise holds, and a later
   checked wait can only DISCOVER that, never restore the missing edge. A clear
   failure message improves diagnosability, not premise correctness; a real
   flake rate is UNMEASURED unless measured.
   Disposition order:
     (i)  an S1 construct exists (witness/latch/persistent-state poll) or is
          cheap -> this is an oracle defect and MUST be fixed; debt registration
          is not available;
     (ii) no constructive substitute exists under current authority -> the site
          may remain, but the ledger must record
          adequacy_verdict = LIMITED_EVIDENCE_WITH_PREMISE_DEBT (or
          NOT_ADEQUATE), with the counterexample chain and an UNMEASURED
          frequency; it must not be cited as deterministic or sufficient
          evidence for the property, and its only value is that it detects a
          defect with unknown probability and reds on correct production with
          unknown probability.
   NON-PRECEDENT: a debt site is not a precedent. Permitting THIS existing test to
   remain as limited regression evidence does not admit THE SHAPE for future
   tests; each new site passes S6 (i)/(ii) on its own.
```

## 7. OBSERVATION is a truth-owner boundary, not a timeline

The following are DISTINCT fact/ownership classes. They are not one universal temporal total order, and an earlier/later relation between two of them must be argued from real edges for the specific property -- never assumed from the list order.

```text
Command intent issued
    != command accepted
    != semantic commit
GateEvent (mechanism evidence)
    != episode terminal Fact
Failed Fact != PcmEdge terminal != leg joined != leg exited normally (needs a witness)
Fact != quiescence
join completed != no panic was swallowed (a join can swallow a panic -> witness)
```

The matrix of owners is authoritative; the sampling point for a given property is determined by the execution DAG of that property. An arrow-shaped ladder must not be used to teach or check ordering.

**Detection is not establishment.** A wait or check placed AFTER the dependent action can only discover that the premise was not satisfied; the premise edge must exist BETWEEN the establishing event and the dependent action. A self-describing failure message is an OBSERVATION-quality property (diagnosability), never a PRECONDITION-quality property (D4). A premise gap with a clear timeout message is still a premise gap.

## 8. LEGAL OUTCOMES enumeration

```text
Rule 1  If the fixture deliberately fixes one branch (using an S1 edge), assert that
        branch and declare that the fixture establishes a premise STRONGER than the
        general product guarantee, which must never be promoted back into a product
        guarantee.
Rule 2  If a real race is left open, give every legal outcome its own correctness
        assertion (content/side-effect witness). A merged
        assert!(matches!(a | b | c)) that checks no branch invariant is not an
        oracle.
Rule 3  Never add an unreachable or unauthorized outcome to the enumeration to make
        a flake turn green.
Rule 4  If the legal-outcome set depends on an unresolved authority question
        (currently #195), STOP and report an authority gap; do not choose the
        semantics for the owner.
```

## 9. Sufficiency, and why the tier never implies the verdict

```text
M0 adequate    := production function really executes + a non-vacuous assertion
                  + boundary inputs covered + no empty run
M1/M2 adequate := the M0 baseline + six questions + a decision/transition space
                  aligned with authority + at least one forbidden-branch assertion
                  (R-H adds decision-table form, an independent falsifier and
                  independent review; still no DAG obligation)
M3 adequate    := six questions + a minimal DAG sketch + every payload premise
                  S1/constructive + legal-outcome enumeration + observation at the
                  owning settlement boundary + failure-path cleanup
M4 adequate    := the baseline of its real mechanism class + real-path/platform
                  claim + one independent falsifier + platform/configuration
                  registration
All classes    := a ledger row exists and adequacy_verdict is adjudicated;
                  an unregistered premise debt (S6(ii) unregistered, or S6(i)
                  fixable but unfixed) is inadequate no matter how high the class;
                  identity is discoverable and platform/feature limits are visible
```

The matrix decides which obligations a test OWES; `adequacy_verdict` decides whether it has PAID them. A tier label is not a verdict, and a test can carry debt while keeping its mechanism class. Any rollup that counts adequacy may count only rows whose verdict is exactly `ADEQUATE`; no prefix grouping, and `DEEP_REVIEWED` is not `ADEQUATE`.

## 10. Existing-suite governance: proportional and event-driven

```text
Default state is bookkeeping, not re-audit.
  - Every discovered test gets a ledger row (identity at least).
  - R-M/R-L keep a MINIMUM screen: mechanism class, risk, obvious red flags.
  - DEEP review is prioritized by risk and by observed cost, one property at a time.
  - There is no periodic full re-audit, no numeric quota and no retroactive
    promotion of a green test to ADEQUATE.
```

Review status (exactly one per test at any time):

```text
UNREVIEWED     booked, discoverable, no semantic review. Running green for years
               keeps a test UNREVIEWED: a green run is an execution fact, not a
               review fact.
SCREENED       a reviewer classified source: mechanism, risk, red flags; no full
               six-question deep analysis, or an unfinished one.
DEEP_REVIEWED  the full six questions plus DAG (when M3), outcome enumeration and
               falsifier assessment are written down.
REMEDIATED     a finding existed; the fix landed and was verified (PR/commit cited).
DEFERRED       an explicit owner/reviewer decision not to review or fix now, with
               the debt written down. Not a failure state -- a known-and-stated one.
```

Independent axes, filled separately per row:

```text
discovered         compiled into a target and listable by cargo
executed           actually run, where/when/in which configuration
execution_status   DISCOVERED | RUN | NOT_RUN | SKIPPED | FAILED | TOOLING_ERROR
                   (COMPILE_ONLY is recorded separately, never merged with RUN)
semantic_review    the review status above + reviewer/turn + review SHA
screening_result   SCREENED_NO_CONFIRMED_DEFECT -- a screening outcome, NEVER an
                   adequacy verdict (the legacy ADEQUATE(screening-depth) string
                   is retired; it must not reappear as a verdict value)
adequacy_verdict   ADEQUATE | NOT_ADEQUATE | LIMITED_EVIDENCE_WITH_PREMISE_DEBT |
                   DEFERRED | NOT_ASSESSED
finding classes    CONFIRMED_STATIC | CONFIRMED_RUNTIME | SUSPECTED | INCONCLUSIVE |
                   NONISSUE | HISTORICAL_RESOLVED | DESIGN_INTENTIONAL -- and the
                   defect owner (test / runner / production) is stated separately
```

Ledger identity and the two descriptive axes:

```text
IDENTITY  The stable key is the COMPOSITE
              TEST_ID + TARGET + PLATFORM + CFG_FEATURE
          A bare TEST_ID is not unique (a cfg-gated symbol can legitimately appear
          twice). Ledger tools must use the composite key.
SIZE      S (single process, non-blocking, hermetic candidate) | M (single host:
          threads, blocking waits, subprocesses, local interfaces) | L (multi-host,
          external network, physical-machine dependent) | UNKNOWN.
          Record OBSERVED and CLAIMED size separately, with the resource witness.
          A #[test] in src/ is not automatically small; a thread hidden behind a
          helper still counts.
SCOPE     N (narrow property) | C (component interaction/contract) |
          B (system/broad behavior) | UNKNOWN. Record validated property/authority
          separately from merely executed dependencies.
MECHANISM M0-M4, multi-tag, from section 3.1.
RISK      R-L | R-M | R-H, from section 3.2. Independent of size and of assertion
          count. Google 80/15/5 is a rough descriptive heuristic and is NEVER a
          quota; do not force tests into percentages or re-label evidence to fit.
```

Invalidation and the remediation queue:

```text
SHA-scoped      A semantic review claim belongs to its subject SHA plus authority
                revision. A production or ADR change marks affected claims STALE
                and requires a NEW evaluation; a green CI run is not a fresh review.
One property    One confirmed property -> one child issue -> one scoped PR (the only
                exception is an inseparable dual positive/negative control proven to
                be one property).
No bulk waves   Do not batch unrelated tests, do not migrate the suite to a common
                runtime or fixture, do not re-classify every sleep-bearing test as a
                defect, and do not reopen protected positive controls or already
                verified repairs by association.
Cost is last    A cost optimization is a hypothesis until it is measured on the
                changed code and shown to preserve the SAME property, the SAME
                legal-outcome set and the SAME falsifier; it ranks below every
                oracle item (FALSE_RED and FALSE_GREEN first, then failure-path
                hygiene, then coverage gaps, then cost, then readability).
```

## 11. The machine layer: what may and may not hard-fail

```text
STRUCTURAL_FACT (mechanically checkable; may hard-fail)
  the test exists and is discovered by the runner; target/cfg/feature are declared;
  the run is non-empty (positive execution count); a required field or record is
  present; an honest result class is declared; a mutation restored the tree and the
  checksum matches; kill vs TOOLING_FAIL are separated for runner steps.
  These prove the FILING is complete -- never that the test is adequate.

REVIEW_SIGNAL (never a verdict, never a hard fail)
  presence of sleep / recv_timeout / try_recv / join / detached spawn; test length
  or assertion density; textual property-statement disagreement under one key.
  Output is a ranked review queue.
  Textual comparison proves neither a real collision nor its absence: two different
  sentences can express one property, and one sentence can hide two owners.

REQUIRES DOMAIN REVIEW (a script must never adjudicate this)
  causal sufficiency of a premise (S1/persistent poll vs S6 window); applicability
  of an S6(ii) debt registration; correctness of the observation boundary;
  completeness of the legal-outcome enumeration; whether a cited authority really
  owns the property; non-vacuity.

NOT IN SCOPE (neither a script nor a reviewer may silently extend it)
  product semantics; K0 primitives; promoting an auxiliary or model variable into
  authority.
```

An opt-in prototype checker exists at `tools/test-quality/README.md`. It is a REVIEW_SIGNAL generator with a frozen calibrated identity and known false-positive classes (`open_abort` exit-witness idiom, H5 positional pairing). It is not installed in lefthook, not in any CI job, and its output is never a verdict. It can be used to rank review, and nothing else.

## 12. Execution-evidence contract

Any real execution — a gate, a suite, a mutation, a probe — is recorded like this, or it is not evidence:

```text
1. Run, and persist UNFILTERED stdout and stderr to a NEW immutable path.
2. Capture, before any retry: the command vector (argv AND the command string),
   the subject SHA, the worktree identity, start/end timestamps, the exit code,
   platform/kernel, toolchain versions, and a digest of every artifact.
3. Classify the result honestly: PASS | FAIL | TOOLING_FAIL | NOT_RUN | INCONCLUSIVE.
4. Rerun only under a NEW immutable log filename. Never overwrite, never truncate,
   never filter.
```

Never pipe a gate through `grep | head`: a filtered live view destroys the artifact. Lost output is `EVIDENCE_LOST` permanently — no later run repairs it, and it is recorded as unexplained rather than explained away.

GitHub writes use write -> fetch -> compare. A body written with the wrong flag is corrected and re-read; only the then-current state is claimed as verified, and the intermediate state is not retroactively blessed.

Known defect class, recorded from the R3-B evidence archive: a capture harness that builds its command record inside a `finalize()` function, where the shell `"$@"` resolves to the (empty) positional parameters of that function, emits metadata with an empty `argv`/`command_string` for every run while every log and digest stays valid. Freeze the command vector at top level, before `finalize`, and pass it in. A run record that is missing its command is a DEFECTIVE RECORD even when all other fields are self-consistent.

Evidence packages that leave the repository carry an in-repo identity record:

```text
identity   package name, byte length, outer sha256, tar member/directory/file
           counts, run count, per-file checksum result, and the count of files with
           no recorded sum (normally the manifest itself, which cannot hash itself)
status     DELIVERED_AND_VERIFIED | ARCHIVED_LOCALLY | NOT_PUBLISHED |
           NOT_INDEPENDENTLY_HASH_VERIFIED | LOCAL_COPY_NOT_FOUND
           A package may NOT be described as delivered, published or verified on the
           strength of its own digest table.
disclosure do not add a binary blob to repository history, do not cut a product
           release to publish logs, and do not publish unredacted local paths or
           environment data by default. The default pickup for a large log archive is
           private direct handover, with the verification recipe recorded in the
           repository so the handover is checkable by a second party.
partial    if a required field (for example the command) is missing from the frozen
           package, record the defect, the affected scope, the independent
           reconstruction basis and its confidence per row (for example
           EXACT_SCRIPT / EXACT_REPORT / PARTIAL_ARGV0_ONLY with a cross-check
           count). Do not rewrite the frozen archive, and do not present a
           reconstruction as the recorded value.
```

## 13. Enforcement topology (current, deliberately minimal)

```text
local pre-push (lefthook)  UNCHANGED. This standard adds no gate step and no
                           measurable cost to the fast local rejection layer.
opt-in tooling             the REVIEW_SIGNAL checker may be run by an author or
                           reviewer; it fails nothing by itself.
PR review                  ADVISORY. A materially changed high-risk test should
                           carry its authoring record; the reviewer checks the
                           claim, not the ceremony. Low-risk deterministic tests
                           keep near-zero ceremony.
server-required checks     NONE ADDED. Installing a required check needs a separate
                           owner decision plus a measured false-positive budget and
                           measured cost, and must be limited to STRUCTURAL_FACT
                           checks.
rulesets / branch policy   out of scope here.
new runtime / scheduler / DSL / property registry   NOT AUTHORIZED.
```

## 14. Prohibited outcomes

```text
- labelling every sleep as defective
- forcing an execution DAG on M0/M1/M2, or on an R-H deterministic property
- declaring every existing test inadequate, or declaring the suite adequate because
  it is green
- counting a mutation build failure, a timeout, a zero-test selection or a tooling
  error as a killed mutant
- satisfying the Google 80/15/5 heuristic numerically, or re-labelling evidence to fit
- changing product authority so that a test passes
- claiming platform, device or FFI success from compilation, from a mock, or from a
  different target
- treating a green suite as architecture acceptance
- upgrading SCREENED or DEEP_REVIEWED into ADEQUATE, or counting a debt verdict in an
  adequate rollup
- closing a debt as fixed without a registered negative control whose RED was
  OBSERVED, not predicted
- claiming that files were delivered or verified when only a local path is known
```

## 15. Adoption, amendment and provenance

```text
VERSION    TEST-QUALITY-STANDARD-v1, 2026-10-15
ADOPTION   owner decision recorded in #247; introduced by the PR that adds this
           file. Before this file, the R1 test-process contract existed only as
           candidate documents inside the issue tracker; this file is now the single
           repository-side test-process authority.
AMENDMENT  a change is a normal reviewed PR: state the differential (what the old
           text required, what the new text requires, which evidence motivates the
           change) and obtain owner review. Never relax a rule by editing a test or a
           report, and never let an issue comment silently supersede this file.
PRODUCT    product semantics remain with ADR-PBK-001 / ADR-PBK-002 / ADR-PBK-003,
           the K0 design documents, the DSP product model and the SongCore binding
           architecture. Where this standard and an ADR appear to disagree about
           product behavior, the ADR is right and this file is the thing to fix.
RELATED    concurrency-test-design.md remains the concurrency sub-domain method and
           is incorporated here as the M3 case; where the two differ on test-process
           obligation this standard wins, and where either touches product behavior
           the ADR wins. measurement-oracle-validity.md governs instrument validation
           before a measurement adjudicates anything and composes with section 12.
HISTORY    #244 umbrella; #245 / #246 R0; #247 R1 gate; #248 / #249 candidates;
           PR #250 R2 pilot; #251 R3; #252 R3-A archive; PR #254 Wave A; #255 R3-B;
           #256 / PR #257 the R3-B P1-01 falsifier (CLOSED, completed);
           #262 agent/method reliability.
```

The evidence behind this adoption, its declared gaps and its remaining debt are indexed in [test-quality/README.md](test-quality/README.md).
