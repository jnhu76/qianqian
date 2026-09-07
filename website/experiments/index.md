---
title: Experiments
status: CURRENT
---

# Experiments

Experiment results with provenance. Every experiment follows the same contract:

```text
01 Question
02 Baseline
03 Hypothesis
04 Method
05 Evidence
06 Result
07 Architectural Consequence
```

---

## Experiment Registry

| ID | Name | Status |
|----|------|--------|
| EXP-FFMPEG-001 | [FFmpeg Closure Minimization](/experiments/ffmpeg-minimization) | <StatusBadge status="HISTORICAL_EVIDENCE" /> |
| EXP-COMPOSITION-KERNEL-001 | [Composition Kernel K0 Oracles](/experiments/composition-kernel-oracles) | <StatusBadge status="VALIDATED" /> |

---

## Evidence Discipline

<ClaimBadge role="authority" />

Machine facts prefer machine artifacts:

```text
machine artifacts
>
current machine-fact prose in old issues
```

Issue numbers remain historical provenance, not current machine truth. If artifact files disagree with issue text, machine artifacts win.

---

## Claim Taxonomy

Every serious claim carries a visible source role:

| Role | Meaning | Requirement |
|------|---------|-------------|
| <ClaimBadge role="authority" /> | Accepted architecture/project decision | Must point to canonical authority |
| <ClaimBadge role="evidence" /> | Measured/tested/implemented fact | Must point to test/result/artifact |
| <ClaimBadge role="interpretation" /> | Human-oriented explanation | Must not masquerade as authority |

---

<ProvenancePanel
  :authority="['docs/experiments/registry.yml']"
  lastVerified="743eb86"
/>
