# Qianqian / 千千·现代

Qianqian is a local-first, lightweight, cross-platform music player and a testbed for Rust composability/runtime architecture.

The repository is in **Architecture v2**. The first verified playback experiment was frozen, `main` was reset, and the new implementation is being rebuilt as a boundary-first plugin graph with a small generic Composition Kernel and explicit playback semantic authorities.

## Architecture in 30 seconds

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

But the project does **not** start by writing kernel APIs.

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
Composition Kernel implementation
```

Current architecture milestones:

```text
Component boundary audit          PASS / CLOSED              (PR #66)
Composition Kernel semantic design MERGED                   (PR #68)
Base Kernel K0                     IMPLEMENTED              (PR #71)
Playback ADR-PBK-001               ACCEPTED (PR #78 + #79; accepted via #81)
```

ADR-PBK-001 is ACCEPTED and is the registered playback-specific ARCH-003 authority (together with `docs/architecture/overview.md`); `docs/architecture/component-boundary-a0.md` remains closed #53 historical evidence whose conflicting playback-specific conclusions are superseded for new playback implementation. Architecture acceptance is not implementation completion: the deterministic executable oracle is the implementation entry, and production FFmpeg/WASAPI integration is still a separate implementation task.

## Control plane and data plane

```text
                         CONTROL PLANE

                 desired composition
                          |
                          v
                 Composition Kernel
             Context / Capability / Fiber
                  Effect / Reconcile
                          |
                    resolve / bind
                          |
-------------------------------------------------------------
                          |
                         DATA PLANE
                          |
      Encoded Media -> Decoder -> Processing -> AudioOutput
```

> **Capability plane != Data plane.**

Context controls reachability/dependency truth. It does not carry PCM blocks or become a universal product message bus.

## Playback authority split

Playback is not one giant `MusicKernel`.

```text
MusicComponent                  one composed lifecycle root
├── MusicKernel                 music/product semantic authority
├── TransportKernel             playback temporal authority
└── TrackSession(s)
    └── DecodeSession(s)
```

### MusicKernel

Owns product/music meaning such as:

```text
play / pause meaning
seek intent meaning
next / previous
repeat / shuffle
playlist policy
selection semantics
user-visible PlaybackState meaning
what to do after a terminal transport outcome
```

### TransportKernel

Owns playback-temporal meaning such as:

```text
playback cursor semantics
Active / Prepared temporal roles
Generation admission
MediaSpan timeline authority
window promotion / invalidation
discontinuity execution
Physical Fence coordination
raw playback evidence interpretation
```

`Kernel` here means **semantic authority role**, not an independent Composition plugin.

Raw playback evidence is interpreted once by `TransportKernel`; `MusicKernel` receives derived domain facts rather than independently reinterpreting cursor/render/EOF truth.

## TrackSession / DecodeSession

`TrackSession` is the media identity/source lifetime root. It may contain multiple `DecodeSession`s at once.

A `DecodeSession` owns one independently advancing decoder cursor/handle. This is required for same-track seek preparation:

```text
TrackSession A
├── DecodeSession gen17 @72s   -> Active role
└── DecodeSession gen18 @100s  -> Prepared role
```

Active/Prepared are temporal roles inside `TransportKernel`; they are not independent plugins or standalone lifetime resources.

## Dual Window, Generation and Physical Fence

The MVP temporal shape is:

```text
1 Active
0..1 Prepared
```

Therefore this classic check is forbidden:

```text
result.generation != global_current_generation => stale
```

A generation is valid when its temporal role still admits that operation.

Hard seek/stop/replacement also requires a real physical boundary:

```text
decoded != queued != submitted != rendered
logical invalidation != physical stop
```

Generation invalidation cannot replace a successful Physical Fence/flush verdict.

Formal exploration found a real `stop × natural ENDED` race: terminalization must not destroy the active temporal state required by an in-flight Physical Fence.

## Everything is a Plugin

“Everything is a plugin” means every **justified** long-lived runtime capability ultimately participates in one common composition/lifecycle protocol.

It does **not** mean:

```text
one feature == one plugin
one plugin == one crate
one plugin == one dynamic library
everything is hot-loaded
everything is rollbackable
every DSP node is a Fiber
```

The Composition topology and the ordered Audio Processing Graph are different structures. Gain/EQ/SRC/Limiter nodes do not become Composition plugins merely because they have state.

## Interaction correctness

A disposer/inverse is not enough to prove that independently mounted components can be removed safely.

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

DSP ordering is the obvious player example: `EQ -> Compressor` is generally not equivalent to `Compressor -> EQ`. Registration timing or container iteration must never become hidden semantic topology.

Restoration is judged by **observational equivalence**, not private bit-for-bit identity.

## System boundary

`Everything is Plugin` does not mean `Everything is rollbackable`.

Already-rendered sound cannot be “unplayed”. Claimed Physical Fence and physical emission are also different facts: a claimed fence is past the cancellation point; rendered audio is outside the recoverable system boundary.

## Realtime boundary

Per audio callback/block, do not perform:

```text
Context lookup
capability resolution
Fiber reconciliation
generic event dispatch
filesystem/network I/O
UI/managed-runtime round trips
unbounded allocation/blocking
```

PCM flows through pre-bound direct data edges.

## Formal verification policy

Formal models are **risk-driven evidence**, not a second implementation of the whole architecture.

> **TLA+ is used to find state collisions, not to formally model every architectural noun.**

The blocking playback core checks only the high-risk temporal interactions: Dual Window, Generation admission, Physical Fence, submitted/rendered accounting, and EOF/drained/ENDED terminalization. Additional ownership models remain supporting evidence.

See `specs/README.md` and `specs/playback/README.md`.

## UI strategy

Current platform intent:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI candidate / replaceable
```

UiHost owns rendering/input mechanism, not music semantics or realtime correctness.

## Current implementation status

Current Rust workspace:

```text
qianqian-core
qianqian-kernel
qianqian-runtime
qianqian-headless
```

The generic Base Kernel K0 is implemented. Product code now carries separate `MusicKernel` and `TransportKernel` authority shells without prematurely implementing the full playback state machine.

Build/test authority:

```bash
cargo run -p qianqian-headless
cargo test --workspace
```

## Historical preservation

Complete pre-Rust repository:

```text
branch: archive/pre-rust-v2
tag:    pre-rust-v2
```

Frozen playback reference:

```text
branch: research/playback-reference-v1
tag:    playback-reference-v1
```

The playback reference is a behavioral oracle, not a source-layout template.

## Repository entry points

- `AGENTS.md` — repository-wide agent governance and hard rules.
- `CONTEXT.md` — stable vocabulary and mental model.
- `docs/README.md` — task-oriented documentation router.
- `docs/architecture/overview.md` — current Architecture v2 overview.
- `docs/adr/ADR-PBK-001.md` — playback authority/session/timeline/data-plane decisions.
- `docs/architecture/composition-kernel.md` — generic Composition Kernel authority.
- `specs/README.md` — risk-driven formalization policy and model registry.
- `CONTRIBUTING.md` — contribution entry point.

## Scope

Qianqian is still a music player, not a generic framework product. The composability work exists to test whether strong component/lifecycle properties survive a real media runtime with explicit ordering, physical-system boundaries and realtime constraints.
