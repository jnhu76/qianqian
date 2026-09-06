# Contributing to Qianqian

Qianqian is rebuilding on Architecture v2. Contributions should preserve the new ownership model rather than restore assumptions from the archived repository.

## Before you start

Read:

1. the current issue or task;
2. `AGENTS.md`;
3. `CONTEXT.md`;
4. the minimum relevant documents selected through `docs/README.md`.

Do not recursively preload historical docs or use `archive/pre-rust-v2` as current architecture authority.

## Issue-first changes

Architecture, capability, platform, media, UI-host, and cross-layer contract changes should have a clearly scoped issue/task before implementation.

A focused task should state:

- the behavior or architecture truth being established;
- what is explicitly out of scope;
- the evidence/verification required;
- the gate for stopping before the next phase.

## Focused implementation

Prefer the smallest cohesive change that proves the requested boundary or behavior.

Do not combine unrelated cleanup, speculative framework work, UI redesign, platform expansion, and media-core changes in one PR.

In particular, avoid preemptively creating:

- plugin registries;
- service locators;
- large dependency-injection frameworks;
- dynamic loading systems;
- many empty crates/modules;
- local `AGENTS.md` files without genuine local divergence.

## Architecture changes

Current architecture is summarized in `docs/architecture/overview.md`.

The core rules are:

```text
Kernel owns semantics
Capability owns mechanism
Profile chooses implementation
```

If implementation pressure appears to require violating one of these rules, surface that conflict explicitly for review rather than working around it silently.

## Historical code reuse

The old repository and playback experiment are preserved as refs. They may be inspected for behavior, measurements, build research, algorithms, or proven mechanism code.

Do not copy an old component into `main` merely because it already exists.

Reuse should answer:

- what proven behavior or mechanism is being retained;
- why its old ownership still fits Architecture v2, or how an adapter narrows it;
- what historical assumptions are deliberately not restored.

## Tests and verification

Verification must match the changed surface.

Examples include:

- Rust compile/test/lint gates once the workspace exists;
- deterministic Music Kernel tests for product semantics;
- headless integration tests for playback correctness;
- corpus/regression evidence for decoder changes;
- physical-device evidence for platform audio claims;
- UI-host tests for presentation/render/input behavior.

Do not report an unrun platform/device check as PASS.

## Documentation

Keep durable documentation small and authoritative.

Use `docs/README.md` as the router. Add a new long-lived document only when the current task creates a durable fact that needs a stable home.

Do not duplicate the same architecture rule across many files. Link to the canonical document instead.

## PR expectations

A PR should explain:

- what changed;
- why this is the smallest useful change;
- verification performed;
- architecture/dependency impact;
- explicit non-scope;
- any remaining environmental/manual validation.

Do not automatically continue into the next milestone after the current task passes.
