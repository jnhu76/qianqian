# Documentation router

This directory is intentionally small. Load only the documentation needed for the current task.

Do not recursively read archived or historical material by default.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary | `../CONTEXT.md` |
| Current architecture | `architecture/overview.md` |
| Product introduction / current repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| Rust workspace/build/test | Read the actual workspace/build files once they exist; do not invent a development manual before implementation establishes authority. |
| Music Kernel semantics | Current issue + architecture overview + the eventual canonical Music Kernel contract/tests. |
| Decoder / Processing / AudioOutput | Current issue + architecture overview + implementation-local tests/contracts created by that work. |
| UiHost / presentation | Current issue + architecture overview + presentation/UiHost contract created by that work. |
| Historical playback evidence | Inspect `research/playback-reference-v1` / `playback-reference-v1` only when the task needs reference evidence. |
| Pre-Rust repository history | Inspect `archive/pre-rust-v2` / `pre-rust-v2` only when the task explicitly needs historical source/docs. |

## Authority model

Use the authority closest to the fact:

```text
agent work rules            -> AGENTS.md
stable vocabulary           -> CONTEXT.md
current architecture        -> docs/architecture/overview.md
implemented behavior        -> code + tests + current contracts
historical experimental fact-> preserved reference/history
current task scope          -> current issue/task
```

When documentation and implementation disagree, do not silently choose one. Audit the actual repository, identify whether the drift is in code, docs, or the task premise, and make only the authorized correction.

## Documentation growth rule

Do not recreate the old documentation hierarchy wholesale.

Create a new long-lived document only when a real implementation or engineering policy creates a durable fact that needs an authority.

Good reasons include:

- a stable cross-layer contract now exists;
- a platform build/run procedure now exists;
- a testing policy is repeatedly needed;
- a research result must remain reproducible;
- an architectural decision needs durable rationale.

Bad reasons include:

- filling a planned directory tree;
- mirroring the archived docs structure;
- documenting APIs that have not been designed or implemented;
- creating placeholder manuals for future platforms.

## Historical material

Architecture v2 begins from a clean `main`.

The old repository was preserved rather than migrated in place:

```text
archive/pre-rust-v2
pre-rust-v2
```

The validated playback experiment remains independently frozen:

```text
research/playback-reference-v1
playback-reference-v1
```

These are opt-in evidence sources, not default reading lists.
