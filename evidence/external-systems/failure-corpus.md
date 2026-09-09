# External System Failure Corpus

- **Status**: LIVING_EVIDENCE
- **Seed source**: `deepseek-ai/deepseek-harness`
- **Seeded**: 2026-09-09
- **Source ledger**: `source-ledger.yml`
- **Authority**: evidence only; never architecture/spec/implementation authority

> This file is **opt-in evidence**. Ordinary ADR, architecture, formal-spec, implementation, and PR reviews must not load it unless the current task explicitly asks for external failure evidence.

## 1. Purpose

This corpus converts failures observed in other systems into adversarial inputs for Qianqian.

The workflow is:

```text
external failure
    -> verify source / reproduction
    -> classify stable failure family
    -> ask whether Qianqian has an analogous state transition / lifecycle / seam
    -> construct a local adversarial trace only when the analogy is real
    -> let Qianqian evidence decide whether code/spec/ADR moves
```

A new external report does not automatically create a new Qianqian invariant. Several source IDs may be independent reproductions of one already-known failure family.

> External failures are ammunition, not authority.

## 2. Evidence separation

The durable units are deliberately separate:

```text
source-ledger.yml
    = what has already been reviewed / skipped / rejected

failure-corpus.md
    = stable failure families + current Qianqian disposition

intake-protocol.md
    = how future scans discover only new/changed evidence
```

Do not duplicate the complete source ledger here. Future maintenance must read `source-ledger.yml` first so known source IDs can be skipped without rereading this document.

## 3. Failure families

### EF-01 — Cancellation is not quiescence

```text
cancel accepted
    != child/tool/device settled
    != process tree stopped
    != physical side effect stopped
```

A logical cancellation acknowledgement can race with already-started mechanism work. Completion requires definitive settlement/quiescence evidence.

**Qianqian disposition**: `EXISTING_INVARIANT + FUTURE_TEST_CANDIDATE`

- Physical Fence already separates logical invalidation from physical silence.
- Future Decoder/AudioOutput/provider work must distinguish cancellation request from mechanism quiescence.

---

### EF-02 — No orphan protocol obligation

Once a protocol-visible obligation is opened, it needs a terminal disposition.

```text
tool/call        -> success / failure / explicit abandonment
submitted media  -> rendered / flushed / explicitly discarded
fence requested  -> definitive verdict
child started    -> terminal child outcome
```

A start without a matching terminal result can poison later state.

**Qianqian disposition**: `OPEN_GAP / PR-82-CORRECTIVE-CANDIDATE`

PR #82 review exposed an analogous risk: after admission closes, fail/abandon can strand accepted-but-never-submitted media unless those frames receive an explicit terminal disposition.

---

### EF-03 — Persisted state is not live mechanism truth

A crash can leave a syntactically valid durable record while the semantic transaction never completed.

```text
persisted: running / claimed / started
live mechanism after restart: absent
```

Cold start therefore needs semantic reconciliation, not only torn-write repair.

**Qianqian disposition**: `FUTURE_TEST_CANDIDATE`

Future persistence/resume must not restore `Playing`, an in-flight Physical Fence, or device/session truth merely because persisted projection says it existed before the crash.

---

### EF-04 — Replacement is generational; capability identity must survive loading topology

Two related failure shapes share one deeper problem:

1. old provider/plugin generation remains live while replacement activates, causing duplicate registration;
2. the same logical capability loaded through two physical module copies gets incompatible identities.

**Qianqian disposition**: `FUTURE_K0/PLUGIN-GRAPH_TEST_CANDIDATE`

- old provider generation must withdraw/discharge before replacement can collide with its registrations;
- if dynamic/out-of-tree plugins are introduced, logical Capability identity must not accidentally depend on one physical module/library instance.

---

### EF-05 — Failure domains need isolation and recovery paths

One plugin/extension activation failure can abort unrelated healthy components and leave no safe recovery path.

**Qianqian disposition**: `WATCH / FUTURE_K0_TEST_CANDIDATE`

Future provider activation pressure should test whether independent healthy components survive an unrelated activation failure and what rollback domain Reconcile actually owns.

---

### EF-06 — Parent lifetime closes only after children settle

Parent shutdown/crash can leave subordinate work orphaned or splice incomplete child state into the parent.

```text
MusicComponent
  -> TrackSession
      -> DecodeSession
```

**Qianqian disposition**: `EXISTING_INVARIANT + FUTURE_TEST_CANDIDATE`

This reinforces the immediate-lifetime-owner rule. Real Decoder work should attack late results during TrackSession retirement and verify parent release happens after subordinate settlement.

---

### EF-07 — Validate and normalize before authoritative commit

Malformed external evidence is often survivable until it is committed into durable/authoritative state; after commit/replay, a tiny malformed event can poison every later operation.

```text
receive
 -> normalize
 -> validate provenance / identity
 -> validate admission / authority
 -> commit authoritative state
```

**Qianqian disposition**: `PARTIALLY_COVERED + FUTURE_MECHANISM-SEAM_RULE`

PR #82 already earned evidence-integrity checks such as `DecodeSessionId <-> GenerationId` binding and decode-backed submission.

---

### EF-08 — Published facts require freshness/provenance across seams

A fact may be correct when produced and wrong when consumed after the system enters a newer episode/configuration. Once the fact leaves the producer's retractable queue, producer-local invalidation is insufficient.

**Qianqian disposition**: `OPEN_GAP / PR-82-CORRECTIVE-CANDIDATE`

PR #82 currently exposes this shape for delayed `TransportFact::NaturallyDrained`: after `take_derived_facts()`, later promotion/new episode can invalidate the fact before `MusicKernel` consumes it. The executable seam must earn causal freshness metadata or a structural delivery protocol that makes cross-episode delayed consumption impossible.

## 4. Qianqian disposition table

| Family | Current Qianqian state | Next useful action | Escalation boundary |
|---|---|---|---|
| EF-01 Cancellation != quiescence | Physical Fence distinction already accepted | attack real Decoder/AudioOutput cancellation once mechanisms exist | ADR only if current fence semantics prove insufficient |
| EF-02 No orphan obligation | PR #82 exposes accepted-but-never-submitted abandon hole | deterministic regression + Rust/spec corrective | ADR only if fail-closed boundary itself must change |
| EF-03 Persisted != live truth | mechanism absent | cold-start reconciliation test when persistence appears | architecture only when persistence semantics are actually designed |
| EF-04 Generational replacement / identity | K0 withdrawal exists; dynamic loading absent | provider replacement test; later dynamic-plugin identity test | do not invent ABI/module rules before dynamic loading exists |
| EF-05 Failure-domain isolation | generic Reconcile exists | independent-provider activation failure test when activation path exists | architecture only if real rollback domain becomes ambiguous |
| EF-06 Parent waits for child settlement | ownership invariant exists | late DecodeSession result during TrackSession retirement | formalize only if real interleaving becomes complex |
| EF-07 Validate before commit | PR #82 partially covers | keep mechanism evidence validation ahead of state mutation | no ADR change unless authority boundary changes |
| EF-08 Fact freshness | PR #82 cross-seam gap | delayed-delivery regression; earn provenance/revision or structural delivery | spec only if temporal model starts modeling asynchronous derived-fact delivery |

## 5. Promotion rule

Create/update a Qianqian issue only when at least one is true:

1. current Qianqian code can reproduce an analogous failure;
2. the next implementation step introduces the risky mechanism;
3. a current PR already exposes the analogous gap;
4. multiple independent sources strengthen a family that directly intersects an accepted Qianqian invariant;
5. external evidence disproves an assumption currently written in Qianqian code/spec/ADR.

Do not create an issue merely because a report is interesting.

## 6. Escalation rule

```text
representation-only local problem
    -> code + tests

ADR-open policy but current spec assumes another policy
    -> code + specs + tests
    -> rerun relevant formal core

accepted semantic boundary must change
    -> stop implementation
    -> ADR corrective + specs + tests
    -> fresh-context adversarial review

external source only; no local reproduction/pressure
    -> evidence update only
```

This corpus must never silently become a second architecture authority.

## 7. Seed-source provenance

The initial DeepSeek Harness source keys, classifications, review dates, duplicate/rejected status, and notes live only in `source-ledger.yml`.

Canonical source URL shape:

```text
https://github.com/deepseek-ai/deepseek-harness/discussions/<number>
```
