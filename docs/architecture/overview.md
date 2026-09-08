# Architecture overview

This document is the repository-local semantic overview for Qianqian Architecture v2.

Detailed generic composition semantics live in [`composition-kernel.md`](composition-kernel.md). Playback-specific authority lives in [`../adr/ADR-PBK-001.md`](../adr/ADR-PBK-001.md). The closed `component-boundary-a0.md` audit remains historical decomposition evidence; where its playback-specific ownership statements differ from ADR-PBK-001, the ADR wins.

## Architecture constitution

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**
>
> **Semantic authorities own the meaning of their facts.**
>
> **Capabilities expose contracts; providers own mechanisms.**
>
> **Fibers own plugin-instance lifetime.**
>
> **Effects own attributable composition mutation/recovery provenance.**
>
> **Profiles declare desired composition; Reconcile determines the running graph.**

Architecture v2 is boundary-first. A feature name or Rust type is not proof of component independence.

## Design order

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

The generic Base Kernel K0 is already implemented. Playback correctness is not solved by adding more generic kernel primitives; it has its own domain/temporal authority boundaries.

## Control plane and data plane

```text
                         CONTROL PLANE

                 desired composition
                          |
                          v
                  Composition Kernel
             Context / Capability / Fiber
                  Effect / Reconcile

-------------------------------------------------------------
                          |
                    resolve / bind
                          v
                         DATA PLANE

      Encoded Media -> Decoder -> Processing -> AudioOutput
```

> **Capability plane != Data plane.**

Context establishes reachability and dependency validity. It does not carry PCM blocks, playback position, Window state, UI payloads, or arbitrary domain events.

## Playback composition boundary

The first playback slice has one composed `Music` component that binds replaceable providers such as Decoder and AudioOutput/PcmSink.

```text
Composition topology

MusicComponent  ---> Decoder provider
      |
      +---------> AudioOutput / PcmSink provider
      |
      `---------> future independent Processing provider, only if earned
```

`MusicComponent` is the **composition lifecycle root** for subordinate playback runtime state. It may contain semantic authorities and nested runtime resources without turning each one into a Composition plugin.

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession(s)
    └── DecodeSession(s)
```

The terms below must remain distinct:

```text
composition lifecycle root
immediate lifetime owner
semantic authority
```

Do not collapse them into one ambiguous `owns` relation.

## MusicKernel

`MusicKernel` is the **music/product semantic authority**.

It owns the meaning of:

```text
play / pause product semantics
seek intent meaning
next / previous
repeat / shuffle
playlist policy
selection semantics
user-visible PlaybackState meaning
what to do after a terminal transport outcome
```

It is not the playback timeline authority and must not independently reinterpret raw cursor/render/EOF/fence facts.

## TransportKernel

`TransportKernel` is the **playback temporal authority**.

It owns the meaning of:

```text
playback cursor
MediaSpan timeline
Active / Prepared temporal roles
Generation admission
window promotion / invalidation
discontinuity execution
Physical Fence coordination
raw playback evidence interpretation
```

`Kernel` in `MusicKernel` / `TransportKernel` means semantic authority role, not Composition plugin boundary.

Raw playback evidence such as:

```text
Decoder EOF
seek landing
late decode result
submitted evidence
rendered evidence
Physical Fence verdict
```

is interpreted once by `TransportKernel`. Other authorities receive typed derived facts.

## TrackSession / DecodeSession

`TrackSession` is the media identity/source lifetime root.

```text
TrackSession
├── source identity
├── media descriptor
├── duration / probe truth
└── 0..N DecodeSession
```

Each `DecodeSession` owns one independently advancing decoder cursor/handle plus its generation-local decode/EOF/seek state.

Same-track seek can therefore legally have two decoder cursors at once:

```text
TrackSession A
├── DecodeSession gen17 @72s   -> Active role
└── DecodeSession gen18 @100s  -> Prepared role
```

Active/Prepared are temporal roles/slots inside `TransportKernel`. They are not independent plugins and are not standalone lifetime resources.

## Dual Window and Generation admission

The MVP temporal shape is:

```text
1 Active
0..1 Prepared
```

Therefore this check is forbidden:

```text
result.generation != global_current_generation => stale
```

A generation is stale when the owning temporal role no longer admits that operation.

During preparation, the Prepared generation may accept decode/prime results but cannot become output authority before promotion.

After retirement, a generation cannot re-enter admission or submit new audible media.

## Physical Fence and physical truth

Playback keeps these facts distinct:

```text
decoded != queued != submitted != rendered
logical invalidation != physical stop
```

Hard stop/seek/replacement must cross a real Physical Fence when old submitted audio must cease.

```text
close old admission
    -> prevent new old-generation submission
    -> physical fence / flush handshake
    -> definitive verdict
    -> promote / stop / fail closed
```

A claimed Physical Fence is past the cancellation point; already rendered sound is outside the recoverable system boundary. These are related but different truths.

Formal exploration found a real `stop × natural ENDED` race. While a hard-stop/discontinuity fence is in flight, natural EOF/drain evidence must not prematurely terminalize the active temporal state needed to finish that fence.

## PCM and processing topology

PCM is Qianqian's canonical decoded-audio data plane:

```text
Encoded Media
    -> Decoder
    -> Canonical PCM
    -> Audio Processing Graph
    -> AudioOutput
```

Metadata, commands, EOF evidence, render evidence, device status and UI state are not PCM payloads.

The **Composition topology** and the **Audio Processing Graph** are different structures.

Composition topology manages independently composed providers and lifecycle. Processing topology manages ordered PCM transforms such as:

```text
Gain
EQ
SRC
Limiter
Mixer
```

Normal DSP node insertion/removal/parameter updates do not automatically become Fiber reconciliation.

Player volume defaults to `PlayerGain` in the processing graph. Optional device/system volume belongs to AudioOutput/platform control.

## Interaction algebra

A disposer/inverse is not sufficient evidence of independent composition.

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

DSP ordering is canonical:

```text
EQ -> Compressor
```

is generally not equivalent to:

```text
Compressor -> EQ
```

Semantic order must never come from registration time, mount order, hash iteration or discovery order.

## Provider withdrawal

Provider disappearance is dependency-ordered:

```text
provider begins withdrawal
        ↓
no new resolution sees it as available
        ↓
dependents invalidate / deactivate
        ↓
dependents finish teardown while teardown access is valid
        ↓
provider releases final bindings/resources
```

For playback this means DecodeSession/RT-edge teardown must complete before the corresponding provider mechanism disappears. The exact nested implementation is not a reason to move playback payloads into Context.

## Confluence

At the composition level:

> **After any legal composition history reaches quiescence, observable composition truth should match a clean construction of the same final desired graph.**

Composition confluence does not imply historical playback position/state magically survives. Domain/temporal continuity is a separate policy and must use explicit checkpoints or behavioral probes where required.

## Realtime boundary

Realtime audio is a bounded data-plane island. Per callback/block do not perform:

```text
Context lookup
capability resolution
Fiber reconciliation
generic event dispatch
filesystem/network I/O
UI/JS/managed-runtime round trips
unbounded allocation/blocking
```

Graph changes are prepared on the control side and published at an RT-safe boundary.

## Formal verification boundary

Playback formalization is intentionally risk-driven.

Blocking temporal evidence covers only state combinations with real collision risk:

```text
Dual Window
Generation admission
Physical Fence
submitted vs rendered
EOF / drained / ENDED terminalization
```

Additional ownership models remain supporting evidence. Formal models are not a second architecture authority and must not silently promote modeling assumptions into production semantics.

## Current code status

Current Rust workspace:

```text
qianqian-core
qianqian-kernel
qianqian-runtime
qianqian-headless
```

The generic Composition Kernel is implemented. Product code now has separate `MusicKernel` and `TransportKernel` semantic-authority shells; final Window/Generation/Fence/TrackSession/DecodeSession representations remain deliberately unfrozen until executable implementation work earns them.

## Authority chain

```text
Generic composition
    docs/architecture/composition-kernel-0-design.md
    docs/architecture/composition-kernel-0-implementation-adr.md
    docs/architecture/composition-kernel.md

Playback architecture
    docs/adr/ADR-PBK-001.md
    specs/playback/* as evidence

Historical decomposition evidence
    docs/architecture/component-boundary-a0.md
    playback-specific statements superseded where ADR-PBK-001 differs
```

## Historical evidence

```text
archive/pre-rust-v2
pre-rust-v2
research/playback-reference-v1
playback-reference-v1
```

These are opt-in evidence sources, not current source-layout or ownership templates.
