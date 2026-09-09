# External Failure Intake Protocol

- **Status**: ACTIVE_PROCESS
- **Ledger**: `source-ledger.yml`
- **Corpus**: `failure-corpus.md`
- **Primary seed**: `deepseek-ai/deepseek-harness`
- **Purpose**: incremental ingestion of external failures without repeatedly rereading known material

> This process is opt-in. Do not run it, and do not load its corpus/ledger, during ordinary ADR, architecture, formal-spec, implementation, or PR review unless the current task explicitly requests external failure evidence.

## 1. Operating principle

External bug reports are adversarial evidence, not authority.

```text
load source ledger first
 -> discover only new/recently-updated candidates
 -> deduplicate before deep reading
 -> validate source reality
 -> classify failure family
 -> compare with Qianqian only when useful
 -> create the smallest earned local action
 -> update ledger checkpoint
```

The long-lived unit of knowledge is normally the **failure family**, not the external issue number.

## 2. Read order

Always start with:

```text
source-ledger.yml
```

Read `failure-corpus.md` only when:

- an unknown source needs family classification;
- a known source delta may change its family;
- a Qianqian disposition may change;
- a genuinely new family might be needed.

Do **not** read the entire historical corpus merely to learn which issue IDs are already known. That is the ledger's job.

## 3. Incremental scan

Read from the ledger checkpoint:

```text
last_incremental_scan
+ default_overlap_days
```

Search each watched source for material **created or updated** since:

```text
last_incremental_scan - overlap_window
```

The overlap protects against delayed indexing, reopened reports, late comments, and old IDs with new evidence.

Discovery should be broad enough to find failure mechanisms we do not already have names for. Useful dimensions include:

```text
lifecycle / cancellation / quiescence
partial transaction / missing terminal outcome
crash / restart / replay / persistence
provider/plugin replacement
identity / duplicate registration
parent-child lifetime / orphan resources
malformed or stale evidence
queue / concurrency / delayed delivery
failure isolation / rollback blast radius
```

Do not search only with Qianqian vocabulary.

## 4. Stable-key dedup before deep reading

Normalize candidates to:

```text
<owner>/<repo>/<kind>#<number>
```

Example:

```text
deepseek-ai/deepseek-harness/discussion#3400
```

Then consult `source-ledger.yml`.

### Unknown key

Perform full triage sufficient to establish:

- observed failure;
- reproduction/concrete evidence;
- root-cause confidence;
- system boundary involved;
- whether it matches an existing failure family.

### Known key, no meaningful update

```text
SKIP
```

Do not reread it for context refresh.

### Known key with meaningful update

Perform **delta review only**:

1. inspect new comments/status/reproducer/fix evidence;
2. read only enough earlier context to interpret the delta;
3. decide whether source status, family, or Qianqian disposition changes;
4. update `last_reviewed` and any source revision marker available.

Meaningful updates include maintainer confirmation/rejection, a new reproducer, changed root cause, upstream fix/rollback, wrong-repository correction, or a newly exposed state transition. Generic `+1` reports and duplicate symptoms normally do not require reclassification.

### New key matching an existing family

Record as:

```text
DUPLICATE_EVIDENCE
```

unless it adds a genuinely new failure relation. A new issue number is not a new system problem by itself.

## 5. Source validation gate

Before absorption, answer:

```text
V1 Is the failing component actually part of the claimed repository/system?
V2 Is there concrete/reproducible evidence rather than only speculation?
V3 Is root cause confirmed, inferred, or unknown?
V4 Does later maintainer evidence contradict the report?
V5 Is this a new mechanism or another reproduction of an existing family?
```

If V1 fails, keep a ledger row with `REJECTED` and explain why. Rejected rows are valuable because they prevent a later scan from repeating the same investigation.

## 6. Failure-family classification

Prefer an existing family when the underlying state transition is the same even if surface symptoms differ.

For example:

```text
tool/call with no tool/result
submitted audio with no render/flush/discard
child task with no terminal outcome
```

belong to the deeper family:

```text
No orphan protocol obligation
```

Create a new `EF-*` family only when the failure relation is materially new. `NEW FAILURE FAMILIES = 0` is a healthy and common result.

## 7. Qianqian disposition

For each useful family/source choose the smallest honest local disposition:

```text
NO_LOCAL_ANALOGUE
WATCH
EXISTING_INVARIANT
FUTURE_TEST_CANDIDATE
OPEN_GAP
ISSUE_CANDIDATE
SPEC_CANDIDATE
ADR_CANDIDATE
PARTIALLY_COVERED
COVERED
```

External evidence alone cannot earn `ADR_CANDIDATE`. First show that the analogous Qianqian boundary exists and that current accepted semantics are insufficient.

## 8. Promotion into local work

Create/update a Qianqian issue only when one of these is true:

1. current Qianqian code can reproduce the analogous failure;
2. the next implementation step is about to introduce the risky mechanism;
3. a current PR already exposes the analogous gap;
4. repeated independent evidence intersects an accepted Qianqian invariant strongly enough to justify an adversarial trace.

When promoting, reference:

```text
failure family id
external stable source keys
Qianqian reproduction / proposed adversarial trace
```

Name the local issue after Qianqian semantics, not after the upstream issue title.

## 9. Code / spec / ADR escalation

Use Qianqian's authority hierarchy:

```text
external evidence = evidence only
ADR               = semantic authority
specs             = formal executable evidence
code/tests        = implementation evidence
```

Classify local pressure:

### Representation-only

```text
code + tests
```

### ADR-open policy but current spec assumes another policy

```text
code + specs + tests
rerun relevant formal core
```

### Accepted semantic boundary must change

```text
STOP implementation
ADR corrective + specs + tests
fresh-context adversarial review
```

### External-only observation

```text
ledger/corpus update only
```

Never modify an ADR simply to mirror an upstream bug or upstream fix.

## 10. Update transaction

A normal maintenance run should touch only:

```text
source-ledger.yml
    new/updated source rows
    review status
    checkpoint

failure-corpus.md
    only if family meaning or Qianqian disposition changed

Qianqian issue/spec/ADR/code
    only when independently earned
```

Do not erase old classifications to hide prior judgment. If a source is corrected or retracted, retain it and move it to `SUPERSEDED`/`REJECTED` with a reason.

## 11. Required output

Every scan reports:

```text
SCAN WINDOW
NEW SOURCES
UPDATED KNOWN SOURCES
SKIPPED KNOWN SOURCES
REJECTED SOURCES
DUPLICATE EVIDENCE
NEW FAILURE FAMILIES
CHANGED QIANQIAN DISPOSITIONS
LOCAL ISSUE ACTIONS
ADR/SPEC IMPACT
CORPUS CHECKPOINT
```

Zero is a valid result. Do not manufacture findings to make the scan look productive.

---

# Reusable agent task

```text
You are maintaining Qianqian's opt-in external-system failure intelligence.

Repository:
  jnhu76/qianqian

IMPORTANT CONTEXT ISOLATION RULE

This task explicitly opts into external evidence. Outside tasks must not load this evidence tree during ordinary ADR/architecture/formal/implementation review.

Read first:
  evidence/external-systems/source-ledger.yml

Read only when classification/disposition needs it:
  evidence/external-systems/failure-corpus.md

Process:
  evidence/external-systems/intake-protocol.md

Primary watched source:
  deepseek-ai/deepseek-harness

GOAL

Incrementally inspect newly created or meaningfully updated issues/discussions and turn verified external failures into adversarial evidence for Qianqian without rereading already-reviewed history.

STEP 1 — LOAD LEDGER ONLY

Read source-ledger.yml before external discovery.
Record:
  last_incremental_scan
  default_overlap_days
  every stable source key
  current source status/family

Do not open known historical discussions for context refresh.

STEP 2 — INCREMENTAL DISCOVERY

Search source material created/updated since:
  last_incremental_scan - overlap_window

Search broadly across lifecycle, cancellation, partial transactions, crash/restart, replay/persistence, plugin/provider replacement, identity, resource release, malformed/stale evidence, concurrency, and failure isolation.

STEP 3 — DEDUP BEFORE READING

For each candidate normalize:
  <owner>/<repo>/<kind>#<number>

- unknown key -> full triage;
- known key, no meaningful update -> SKIP;
- known key with meaningful update -> DELTA REVIEW ONLY;
- new report with same state-transition family -> DUPLICATE_EVIDENCE unless it adds a genuinely new mechanism.

STEP 4 — VALIDATE SOURCE

Confirm repository/component ownership, concrete evidence, root-cause confidence, later maintainer corrections, and whether the report is really new rather than another reproduction.

STEP 5 — CLASSIFY

Only now read failure-corpus.md when needed.
Prefer an existing EF-* family. Create a new family only for a materially new failure relation.

For each useful source answer:
1. What independently legal states/events collided?
2. What obligation/freshness/lifetime/authority assumption failed?
3. Does Qianqian have the analogue now, in an open PR, or only in a future subsystem?
4. What is the smallest honest local disposition?

STEP 6 — PROVE BEFORE PROMOTION

Do not change Qianqian ADR/spec/code because the external bug exists.
First construct the smallest local adversarial trace/code inspection when a present analogue exists.

- representation problem -> code/tests only;
- ADR-open policy contradicts current formal assumption -> specs + tests and rerun formal core;
- accepted boundary must change -> stop and propose ADR + specs + tests corrective;
- no local analogue -> evidence only.

STEP 7 — UPDATE

Update source-ledger.yml for every new/changed/rejected/duplicate source and advance the scan checkpoint.
Update failure-corpus.md only when a family or Qianqian disposition actually changes.
Preserve provenance.

STEP 8 — REPORT

SCAN WINDOW = ...
NEW SOURCES = ...
UPDATED KNOWN SOURCES = ...
SKIPPED KNOWN SOURCES = ...
REJECTED SOURCES = ...
DUPLICATE EVIDENCE = ...
NEW FAILURE FAMILIES = ...
CHANGED QIANQIAN DISPOSITIONS = ...
LOCAL ISSUE ACTIONS = ...
ADR/SPEC IMPACT = ...
CORPUS CHECKPOINT = ...

Stop after the evidence/update PR or explicitly requested local issue updates. Do not implement unrelated Qianqian features and do not merge without explicit authorization.
```
