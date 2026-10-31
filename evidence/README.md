# Evidence

This tree stores **non-authoritative records**: the external failure corpus and
the *conclusions* of executed research campaigns. It is deliberately outside
`docs/architecture`, `docs/adr`, `specs`, and `docs/experiments` because none of
it was produced by Qianqian's own architecture process, and none of it defines
architecture semantics or grants release authority.

## Default loading rule

> **Do not load this tree during ordinary ADR, architecture, formal-spec,
> implementation, or PR review.**

Load `evidence/` only when the current task explicitly asks for one of the
following:

- external failure evidence;
- failure-corpus maintenance;
- upstream issue/discussion mining;
- adversarial inspiration from other systems;
- comparison against a named external failure family;
- the measured result or limitation of one named research campaign.

This rule exists to prevent external incidents from contaminating a
fresh-context review of Qianqian's own authorities.

## Authority boundary

```text
external evidence       -> evidence/*
architecture semantics  -> docs/adr + docs/architecture
formal evidence         -> specs/*
local experiment result -> docs/experiments/*
implementation truth    -> code + tests
```

Evidence may motivate a Qianqian adversarial trace or issue, but it never
becomes architecture authority merely by being recorded here.

## Retention standard (2026-11-01)

Git keeps **results, not the research process**. A campaign is represented by
its conclusion document, which is self-contained:

```text
Question -> Source SHA -> Measurement -> Verdict -> Limitation -> Decision
```

Not:

```text
Campaign -> Steps -> Agents -> Raw Logs -> Receipts -> Review Logs -> More Logs
```

What is versioned:

- the conclusion document for a topic (`RESULTS.md`, or the topic's existing
  result documents where they already carry the full result);
- result tables that cannot be expressed accurately in prose (small
  JSON/CSV — e.g. `songcore-172/q2-crossroot/cmp/all-verdicts/`);
- executable verification assets that a tracked script, CI workflow or
  contract consumes;
- frozen receipts a release or contract clause names explicitly.

What is **not** versioned, and stays local or outside the repository
(`.gitignore` enforces this): raw run logs, per-run receipts, intermediate
JSON, build transcripts, agent workflow records, temporary plans, duplicate
reports, and intermediate build products of completed campaigns. Git history
before the cleanup commit still holds the removed bytes.

**Consequence, stated honestly:** a campaign whose raw receipts were removed
keeps its numbers, verdict, method and limitation, but its evidentiary grade
drops from "independently re-checkable from committed receipts" to "recorded
conclusion". Do not cite a removed raw log as still verifiable; cite the
conclusion document and its stated limitation.

## Topics

| Topic | Issue(s) | Conclusion document | Verdict |
|---|---|---|---|
| `external-systems/` | — | `failure-corpus.md`, `intake-protocol.md`, `source-ledger.yml` | living failure corpus (data, not a campaign) |
| `diagnostics-tier2-296/` | #317 / #290 / #296 | `REPORT.md`, `VERIFICATION.md`, `REVIEW.md`, `PRODUCT_DIFFERENTIAL.md` | logging/diagnostics closeout; native FFmpeg/decode rows NOT_RUN |
| `native-tier2-296/` | #296 | `README.md`, `MATRIX.md` | 12/12 PASS on Linux x86_64; Windows 12/12 NOT_RUN |
| `windows-native-tier2-324/` | #324 | `README.md`, `ARTIFACT.md`, `MATRIX.md`, `ORACLE-CLASS.md` | Windows native 12/12 DISPOSITIONED |
| `windows-runtime-288/` | #288 | `LEDGER.md`, `SCENARIO_MATRIX.md`, `EXECUTION_DAGS.md` | Windows error-cone runtime campaign |
| `analyst-join-321/` | #321 / #322 | `REPORT.md` | E2E_CONFIRMED latent liveness defect, fixed by #322 |
| `songcore-172/` | #172 / #326–#337 | `RESULTS.md` + `verdicts/*.md` | campaign result record; release authority stays `docs/release/songcore-release-contract.md` |

`external-systems/` contains the living failure corpus, source ledger, and
incremental intake protocol. The current seed source is
`deepseek-ai/deepseek-harness`; future sources may be added without changing
the authority model above.

## Executable assets retained on purpose

These are inputs of tracked tooling, not process residue:

```text
songcore-172/suite/csuite_consumer.c
    ops/run-csuite.ps1

songcore-172/linux/verify/package/external-consumer-example.c
    ops/package-rc.ps1 default --consumer-example, and the
    evidence/songcore-172/linux/verify/package/external-consumer-example.c
    path trigger of .github/workflows/songcore-release-substrate.yml
    and lefthook.yml

songcore-172/package/repro/**
    the receipt that ops/package-rc.ps1 writes into
    manifest.reproducibility.evidence

songcore-172/package/package-summary.json
songcore-172/verdicts/review-C.md
    docs/architecture/registry.yml evidence entries

windows-native-tier2-324/README.md, ARTIFACT.md, ENVIRONMENT.md
native-tier2-296/README.md
    docs/release/songcore-release-contract.md S-325 / S-323
```

Frozen receipts (`package/repro/`, `linux/verify/**`) are historical records;
they are not rewritten when a path inside them no longer resolves in the
working tree.
