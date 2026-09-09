# Evidence

This tree stores **non-authoritative evidence collected from outside Qianqian**.

It is deliberately outside `docs/architecture`, `docs/adr`, `specs`, and `docs/experiments` because the material was not produced by Qianqian's own architecture process, formal models, or experiments.

## Default loading rule

> **Do not load this tree during ordinary ADR, architecture, formal-spec, implementation, or PR review.**

Load `evidence/` only when the current task explicitly asks for one of the following:

- external failure evidence;
- failure-corpus maintenance;
- upstream issue/discussion mining;
- adversarial inspiration from other systems;
- comparison against a named external failure family.

This rule exists to prevent external incidents from contaminating a fresh-context review of Qianqian's own authorities.

## Authority boundary

```text
external evidence       -> evidence/*
architecture semantics  -> docs/adr + docs/architecture
formal evidence         -> specs/*
local experiment result -> docs/experiments/*
implementation truth    -> code + tests
```

External evidence may motivate a Qianqian adversarial trace or issue, but it never becomes architecture authority merely by being recorded here.

## External systems

`external-systems/` contains the living failure corpus, source ledger, and incremental intake protocol. The current seed source is `deepseek-ai/deepseek-harness`; future sources may be added without changing the authority model above.
