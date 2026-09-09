# Documentation router

Load only the documentation needed for the current task. Historical material is evidence, not current authority unless explicitly routed here.

## Read by task

| Task | Read first |
|---|---|
| Repository/agent rules | `../AGENTS.md` |
| Stable vocabulary | `../CONTEXT.md` |
| Current architecture | `architecture/overview.md` |
| Generic component/plugin/composition semantics | `architecture/composition-kernel.md` + `architecture/composition-kernel-0-design.md` |
| Historical component-boundary audit | `architecture/component-boundary-a0.md` — closed #53 evidence; its playback-specific ownership conclusions are historical inputs superseded by accepted `adr/ADR-PBK-001` semantics (see the transition note inside that file) |
| Composition Kernel representation decisions | `architecture/composition-kernel-0-implementation-adr.md` |
| Playback authority / timeline / media session / PCM boundaries (ARCH-003) | `adr/ADR-PBK-001.md` (**ACCEPTED**; registered playback-specific ARCH-003 authority; no FFmpeg/WASAPI production implementation implied) |
| Playback formal evidence | `../specs/README.md` + `../specs/playback/README.md` |
| Product introduction / repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| Rust workspace/build/test | Current `Cargo.toml` / crate manifests / CI |
| Music/product semantics | `adr/ADR-PBK-001.md` + current `qianqian-core::music` code/tests |
| Playback temporal semantics | `adr/ADR-PBK-001.md` + current `qianqian-core::transport` code/tests + playback specs when state-collision evidence is needed |
| Decoder / Processing / AudioOutput | `adr/ADR-PBK-001.md` + `architecture/overview.md`; generic Composition Kernel never owns media payload contracts |
| Realtime audio path | `adr/ADR-PBK-001.md` + `architecture/overview.md`; PCM stays off Context/event routing |
| UiHost / presentation | `architecture/overview.md` + current presentation contract |
| Historical playback evidence | git tag `playback-reference-v1` (also branch `research/playback-reference-v1`); not present in the working tree — inspect via `git show playback-reference-v1:<path>` |
| Pre-Rust repository history | git tag `pre-rust-v2` (also branch `archive/pre-rust-v2`); not present in the working tree — inspect via `git show pre-rust-v2:<path>` |

## Authority model

Use the authority closest to the fact:

```text
agent work rules                 -> AGENTS.md
stable vocabulary                -> CONTEXT.md
current architecture             -> docs/architecture/overview.md
generic composition semantics    -> docs/architecture/composition-kernel*.md
historical boundary evidence     -> docs/architecture/component-boundary-a0.md
registered ARCH-003 authority    -> docs/adr/ADR-PBK-001.md (ACCEPTED)
playback formal evidence         -> specs/playback/*
implemented behavior             -> code + tests + current contracts
historical experimental fact     -> preserved reference/history
current task scope               -> current issue/task
```

`ADR-PBK-001` is the registered playback-specific ARCH-003 authority after passing the blocking formal core and receiving human acceptance on 2026-09-09. `component-boundary-a0.md` remains historical #53 evidence; its conflicting playback-specific ownership/granularity conclusions must not be combined with the accepted ADR in new implementation. The accepted transition covers MusicKernel vs TransportKernel authority, TrackSession/DecodeSession structure, Dual Window, Generation admission, and Physical Fence semantics.

When documentation and implementation disagree, do not silently choose one. Identify the drift source and correct only the authority that is stale.

## Playback authority split

ADR-PBK-001 (**ACCEPTED**) freezes three distinct concepts:

```text
MusicComponent   = composition lifecycle root
MusicKernel      = music/product semantic authority
TransportKernel  = playback temporal authority
```

Nested playback lifetime:

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession(s)
    └── DecodeSession(s)
```

Active/Prepared are temporal roles inside `TransportKernel`, not independent plugins or standalone lifetime resources.

Raw playback evidence such as Decoder EOF, late decode, submitted/rendered evidence, seek landing, and Physical Fence verdict is interpreted by `TransportKernel`; `MusicKernel` receives derived domain facts and decides product behavior.

## Architecture design order

For plugin/composition work, do **not** start from API shape.

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

The Base Kernel K0 is already implemented. Playback architecture adds a separate temporal correctness boundary rather than expanding the generic kernel.

## Control plane vs data plane

### Composition/control plane

```text
reachability
capability resolution
plugin-instance lifecycle
effect ownership
provider withdrawal
desired graph / reconcile
```

belong to the generic Composition Kernel.

### Playback/product/data plane

```text
music/product meaning
playback cursor / windows / generations
PCM/audio formats
realtime blocks
EOF/render/fence evidence
UI/domain payloads
```

belong to the relevant domain/data-plane authority, not Context.

> **Capability plane != Data plane.**

Do not solve a playback problem by turning Context into a global state bag or message bus.

## Formal verification policy

Formalization is risk-driven.

> **TLA+ is used to find state collisions, not to model every architectural noun.**

For Playback, the blocking core is deliberately small: Dual Window, Generation admission, Physical Fence, submitted/rendered accounting, and EOF/drained/ENDED terminalization. Ownership models and additional mutations are supporting evidence unless they expose a real architecture contradiction.

## Documentation growth rule

Create a new long-lived document only when a durable fact needs its own authority. Prefer linking to the existing authority over copying specifications into multiple files.

Good reasons include:

- a stable kernel or cross-layer contract exists;
- a platform build/run procedure exists;
- a testing/formalization policy is repeatedly needed;
- a research result must remain reproducible;
- an architectural decision needs durable rationale.

Bad reasons include:

- filling a planned directory tree;
- mirroring archived docs;
- documenting APIs that do not exist;
- creating a second authority for a fact already owned by an ADR.

## Historical material

Architecture v2 begins from the post-reset `main`. The historical evidence below is preserved as **git refs, not working-tree directories**:

```text
pre-rust-v2              git tag (branch: archive/pre-rust-v2)
playback-reference-v1    git tag (branch: research/playback-reference-v1)
```

Neither tag is present in the current working tree; inspect via `git show <tag>:<path>`. These are opt-in evidence sources, not current source-layout or ownership authorities.
