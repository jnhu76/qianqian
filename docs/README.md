# Documentation router

This directory is intentionally small. Load only the documentation needed for the current task.

Do not recursively read archived or historical material by default.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary | `../CONTEXT.md` |
| Current architecture | `architecture/overview.md` |
| Composition Kernel / Context / Capability / Fiber / Effect / Reconcile | `architecture/composition-kernel.md` + current issue/task |
| Product introduction / current repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| Rust workspace/build/test | Current `Cargo.toml` / crate manifests / CI once present; do not invent a separate manual before repeated operational complexity exists. |
| Music/domain semantics | Current issue + `architecture/overview.md` + the implemented Music plugin/kernel tests/contracts created by that work. |
| Decoder / Processing / AudioOutput | Current issue + `architecture/overview.md`; remember the generic Composition Kernel does not own media payload contracts. |
| Realtime audio graph/runtime | Current issue + `architecture/overview.md` + Composition Kernel control/data-plane boundary; realtime data must stay off Context/event routing. |
| UiHost / presentation | Current issue + `architecture/overview.md` + implementation-local presentation/UiHost contract when established. |
| Historical playback evidence | Inspect `research/playback-reference-v1` / `playback-reference-v1` only when the task needs behavioral evidence. |
| Pre-Rust repository history | Inspect `archive/pre-rust-v2` / `pre-rust-v2` only when the task explicitly needs historical source/docs. |

## Authority model

Use the authority closest to the fact:

```text
agent work rules                 -> AGENTS.md
stable vocabulary                -> CONTEXT.md
current architecture             -> docs/architecture/overview.md
composition-kernel semantics     -> docs/architecture/composition-kernel.md
implemented behavior             -> code + tests + current contracts
historical experimental fact     -> preserved reference/history
current task scope               -> current issue/task
```

When documentation and implementation disagree, do not silently choose one. Audit the repository, identify whether drift is in code, docs, or the task premise, and make only the authorized correction.

## Core routing distinction

For architecture questions, first classify the subject:

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
