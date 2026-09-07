# Documentation router

This directory is intentionally small. Load only the documentation needed for the current task.

Do not recursively read archived or historical material by default.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary | `../CONTEXT.md` |
| Current architecture | `architecture/overview.md` |
| Component boundary / plugin granularity / interaction algebra | `architecture/composition-kernel.md` boundary-design sections + closed audit `architecture/component-boundary-a0.md` (#53 PASS/CLOSED) |
| Composition Kernel semantics design (#67) | `architecture/composition-kernel-0-design.md` + `architecture/composition-kernel.md` + issue #67 |
| Composition Kernel / Context / Capability / Fiber / Effect / Reconcile implementation | **Only after the pre-implementation review (Corrective-4) is accepted and an implementation issue exists**; then read `architecture/composition-kernel.md` + `architecture/composition-kernel-0-design.md` + the implementation issue |
| Product introduction / current repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| Rust workspace/build/test | Current `Cargo.toml` / crate manifests / CI once present; do not invent a separate manual before repeated operational complexity exists. |
| Music/domain semantics | Current issue + `architecture/overview.md` + implemented Music plugin/kernel tests/contracts. |
| Decoder / Processing / AudioOutput | Current issue + `architecture/overview.md`; generic Composition Kernel does not own media payload contracts. |
| Realtime audio graph/runtime | Current issue + `architecture/overview.md`; realtime data must stay off Context/event routing. |
| UiHost / presentation | Current issue + `architecture/overview.md` + implementation-local presentation/UiHost contract when established. |
| Historical playback evidence | Inspect `research/playback-reference-v1` / `playback-reference-v1` only when the task needs behavioral evidence. |
| Pre-Rust repository history | Inspect `archive/pre-rust-v2` / `pre-rust-v2` only when the task explicitly needs historical source/docs. |

## Authority model

Use the authority closest to the fact:

```text
agent work rules                 -> AGENTS.md
stable vocabulary                -> CONTEXT.md
current architecture             -> docs/architecture/overview.md
component-decomposition audit     -> closed issue #53 + component-boundary-a0.md + composition-kernel boundary sections
composition-kernel-0 semantics   -> docs/architecture/composition-kernel-0-design.md (merged via PR #68) + issue #67
composition-kernel invariants    -> docs/architecture/composition-kernel.md
implemented behavior             -> code + tests + current contracts
historical experimental fact     -> preserved reference/history
current task scope               -> current issue/task
```

When documentation and implementation disagree, do not silently choose one. Audit the repository, identify whether drift is in code, docs, or the task premise, and make only the authorized correction.

## Architecture design order

For plugin/composition work, do **not** start from API shape.

Use this order:

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Interaction Algebra
        ↓
Effect / System Boundary
        ↓
Global lifecycle ordering
        ↓
Confluence oracle
        ↓
Context / Fiber / Effect / Reconcile implementation
```

#53 `COMPONENT-BOUNDARY-A0` is PASS/CLOSED (`architecture/component-boundary-a0.md`); the #67 `COMPOSITION-KERNEL-0` semantic design is MERGED (PR #68, `architecture/composition-kernel-0-design.md`). Current gate: the **pre-implementation review (Corrective-4)**.

Until that review is accepted and a separate implementation issue is opened, implementation work on a new `qianqian-kernel`, Context API, Fiber state machine, or Reconcile engine is not authorized.

## Core routing distinction

### Composition/control plane

Questions about:

```text
reachability
capability resolution
plugin-instance lifecycle
effect ownership
provider disappearance
dependency activation/deactivation
desired plugin tree / reconciliation
```

belong to the generic Composition Kernel authority.

### Domain/data plane

Questions about:

```text
music/player semantics
PCM/audio formats
realtime audio blocks
UI payloads
library models
service method payloads
domain events
```

belong to the relevant domain/plugin contract, not Context.

Do not solve a data-plane problem by expanding Context into a universal bus.

## Boundary-design questions

Before a candidate becomes a plugin/capability, ask:

```text
who owns its state/resources?
what does it require/provide?
what operations cross the boundary?
which operations commute?
where is non-commutative order explicit?
what is reversible vs outside the system boundary?
which dependents must exit before provider teardown?
does finer granularity justify its cognitive/configuration cost?
```

A different feature name is not evidence of component independence.

## Documentation growth rule

Do not recreate the old documentation hierarchy wholesale.

Create a new long-lived document only when a real implementation or engineering policy creates a durable fact that needs an authority.

Good reasons include:

- a stable kernel or cross-layer contract exists;
- a platform build/run procedure exists;
- a testing policy is repeatedly needed;
- a research result must remain reproducible;
- an architectural decision needs durable rationale.

Bad reasons include:

- filling a planned directory tree;
- mirroring archived docs;
- documenting APIs that have not been implemented or seriously designed;
- creating placeholder manuals for future platforms.

## Historical material

Architecture v2 begins from the post-reset `main`.

The old repository is preserved at:

```text
archive/pre-rust-v2
pre-rust-v2
```

The validated playback experiment remains independently frozen:

```text
research/playback-reference-v1
playback-reference-v1
```

These are opt-in evidence sources, not default reading lists or current composition authorities.
