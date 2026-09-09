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
| Playback authority / timeline / media session / PCM boundaries (ARCH-003) | `adr/ADR-PBK-001.md` (**ACCEPTED**; registered playback-specific ARCH-003 authority) |
| Proposed plugin-composed audio data-plane corrective | `adr/ADR-PBK-002.md` (**PROPOSED**) + `architecture/plugin-composed-audio-data-plane.md`; read only when reviewing/implementing this corrective — it does not become registered authority until acceptance |
| Playback formal evidence | `../specs/README.md` + `../specs/playback/README.md` |
| Product introduction / repository entry | `../README.md` |
| Contribution workflow | `../CONTRIBUTING.md` |
| Rust workspace/build/test | Current `Cargo.toml` / crate manifests / CI |
| Music/product semantics | `adr/ADR-PBK-001.md` + current `qianqian-core::music` code/tests |
| Playback temporal semantics | `adr/ADR-PBK-001.md` + current `qianqian-core::transport` code/tests + playback specs when state-collision evidence is needed |
| Decoder / Processing / AudioOutput current authority | `adr/ADR-PBK-001.md` + `architecture/overview.md`; if the task explicitly reviews the proposed plugin-composed corrective, also load `adr/ADR-PBK-002.md` + `architecture/plugin-composed-audio-data-plane.md` |
| Realtime audio path current authority | `adr/ADR-PBK-001.md` + `architecture/overview.md`; PCM stays off Context/event routing. Proposed plugin-composed direct-flow semantics are opt-in via `ADR-PBK-002` until accepted |
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
proposed data-plane corrective   -> docs/adr/ADR-PBK-002.md (PROPOSED; not registered authority)
playback formal evidence         -> specs/playback/*
implemented behavior             -> code + tests + current contracts
historical experimental fact     -> preserved reference/history
current task scope               -> current issue/task
```

`ADR-PBK-001` remains the registered playback-specific ARCH-003 authority after passing the blocking formal core and receiving human acceptance on 2026-09-09. `ADR-PBK-002` is a proposed amendment that would change only the Audio Data Plane plugin/composition granularity if accepted; while it is PROPOSED, ordinary implementation/review must not silently treat it as current authority.

`component-boundary-a0.md` remains historical #53 evidence; its conflicting playback-specific ownership/granularity conclusions must not be combined with the accepted ADR in new implementation. The accepted transition covers MusicKernel vs TransportKernel authority, TrackSession/DecodeSession structure, Dual Window, Generation admission, and Physical Fence semantics.

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

## Proposed plugin-composed audio data plane

When — and only when — the task explicitly reviews `ADR-PBK-002`, use this target model:

```text
Composition Kernel
    -> creates/binds/withdraws durable Audio Data Plane Plugin/Fiber participants

ProcessingTopologyAuthority
    -> defines ordered PCM edges and publishes a pre-bound RT graph

PCM
    -> flows directly between those already-bound plugin instances
    -> never re-enters Context/Reconcile/generic dispatch per block
```

The proposal keeps `PcmBlock`, `MediaSpan`, `TrackSession`, `DecodeSession`, Generation/Window/Fence state as data/nested runtime resources rather than Plugins.

The proposal also adds a new lifecycle requirement for RT data-plane participants:

```text
provider withdrawal
    -> publish graph without provider
    -> wait old graph readers/references quiesce
    -> only then final-release provider
```

This is a **PROPOSED** corrective until its boundary/formal gates pass.

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

For Playback, the blocking temporal core is deliberately small: Dual Window, Generation admission, Physical Fence, submitted/rendered accounting, and EOF/drained/ENDED terminalization. Ownership models and additional mutations are supporting evidence unless they expose a real architecture contradiction.

`ADR-PBK-002` proposes one new formal escalation only for a confirmed high-risk lifecycle interleaving: **data-plane plugin final release vs still-published RT graph readers/references**. It does not authorize a complete formal model of DSP ordering.

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

## External evidence isolation

External issue/discussion mining lives outside the documentation authority tree under `../evidence/`.

> **Do not load `evidence/` during ordinary ADR, architecture, formal-spec, implementation, or PR review.**

Only opt in when the current task explicitly asks for external failure evidence, failure-corpus maintenance, upstream issue/discussion mining, or adversarial inspiration from other systems. This keeps fresh-context authority reviews grounded in Qianqian's own ADR/spec/code evidence instead of biasing them with outside incidents.

When explicitly opted in, start at `../evidence/README.md`; for incremental external-system scans, read `../evidence/external-systems/source-ledger.yml` before the failure corpus so already-reviewed unchanged sources can be skipped without rereading history.
