# Architecture overview

This document is the repository-local semantic overview for Qianqian Architecture v2.

It describes ownership and dependency direction. It does not require a specific future directory tree, crate split, binary split, or dynamic-plugin system.

## Core rules

Architecture v2 is governed by:

> **Kernel owns semantics.**
>
> **Capability owns mechanism.**
>
> **Profile chooses implementation.**

Rust is the product architecture language.

Native/platform/UI technologies are implementation substrates behind explicit boundaries.

## Logical layers

```text
Layer 0  Rust Base Kernel
         composition / lifecycle / profile assembly

Layer 1  Rust Music Kernel
         player and music-domain semantics

Layer 2  Rust Presentation
         stable UI-facing state/actions

Layer 3  Capabilities / host implementations
         Decoder / Processing / AudioOutput / UiHost / platform services

Below   Native and platform mechanism substrate
        FFmpeg / DSP / WASAPI / CoreAudio / PipeWire / Android audio / UI runtimes
```

The physical repository layout should emerge from implementation pressure. Do not create one crate per box merely to mirror this diagram.

## Base Kernel

The Base Kernel should remain domain-light.

It owns explicit composition, lifecycle ordering, profile construction, and startup validation when those needs become real.

It should not become a service locator, runtime registry, plugin marketplace, or dependency-injection framework by default.

MVP composition should prefer ordinary Rust construction and static profile assembly.

## Music Kernel

The Music Kernel is the authority for music/player product semantics, including as they are implemented over time:

- current track and playback session;
- play/pause/stop intent;
- seek semantics;
- user-visible playback state and position;
- queue/playlist/repeat/shuffle policy;
- track transitions;
- buffering interpretation;
- recovery and product-level error meaning;
- ENDED semantics.

It must not depend on FFmpeg object types, platform audio-device handles, PocketJS/KuiklyUI widget trees, or UI runtime internals.

Mechanism layers produce evidence. The Music Kernel decides product meaning.

## Presentation

UI frameworks do not consume Music Kernel internals directly.

The intended seam is:

```text
Music Kernel
     |
Presentation
     |
   UiHost
```

Presentation exposes UI-relevant state/actions without leaking session generations, decoder objects, PCM ownership, audio-device handles, or other mechanism internals.

The exact Rust types and serialization/ABI are not frozen until implementation establishes them.

## UiHost

`UiHost` is a capability/plugin boundary for presentation rendering and user-input translation.

Plugin means a replaceable architecture boundary. It does not imply runtime discovery or a dynamic library.

Current profile intent:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI UiHost by default, replaceable by profile
```

A UiHost may own framework-specific rendering/state adaptation and window-level UI lifecycle integration. It does not own music semantics.

## Capability rule

Do not create a capability merely because an interface can be imagined.

A capability should be justified by at least one real pressure such as:

- multiple real implementations;
- a platform variation axis;
- a research/replacement axis;
- deterministic fake/testing value;
- independent resource/lifecycle ownership.

Logical capability boundaries do not require one crate, static library, shared library, or dynamically loaded plugin per capability.

## Native media

The media boundary keeps Decoder and Processing logically distinct:

```text
Decoder    encoded media -> canonical PCM
Processing canonical PCM -> canonical PCM
AudioOutput canonical PCM -> physical device + output evidence
```

When FFmpeg is reintroduced, Decoder and Processing must share one FFmpeg closure authority rather than each shipping duplicated dependencies.

The canonical PCM contract must be established explicitly by implementation work: sample representation, rate/layout semantics, frame units, ownership/lifetime, partial production, and EOF/drain behavior.

Do not add steady-state PCM copies merely to make abstraction boundaries look cleaner.

## Realtime island

The audio realtime path is a mechanism island.

It must not perform per-period round trips through PocketJS, KuiklyUI, general UI state, filesystem/network logic, or unbounded control work.

Native/Rust realtime mechanisms may retain only the physical state needed for bounded correctness, such as submitted/rendered counters, buffer occupancy, flush completion, device failure, or stale-rejection tokens.

The Music Kernel consumes bounded evidence/snapshots/events and determines product semantics.

## Product profiles

A profile selects implementations without changing shared product semantics.

Initial Windows direction:

```text
Rust Core
+ PocketJS UiHost
+ Decoder backed by the proven media work as it is reintroduced
+ Processing (bypass first unless real DSP is authorized)
+ WASAPI AudioOutput
```

Android is the second architecture test:

```text
same Rust Music Kernel
same Presentation semantics
+ KuiklyUI UiHost
+ Android platform capabilities
```

Android integration must not create a second Track/Queue/PlaybackSession/ENDED/seek semantic implementation. If a platform appears to require such a fork, review the architecture instead of copying the domain layer.

## Historical evidence

Architecture v2 does not discard the earlier engineering work.

Complete pre-Rust tree:

```text
archive/pre-rust-v2
pre-rust-v2
```

Frozen playback reference:

```text
research/playback-reference-v1
playback-reference-v1
```

The playback reference is a behavioral oracle, not a source-layout template. It proved a complete Windows playback path and important physical truths around decode/queue/submit/render, seek/flush, stale rejection, timeline, and physical drain.

Architecture v2 may change language, ownership, directories, types, and composition while preserving those proven behaviors when the corresponding functionality is rebuilt.

## Non-goals

Architecture v2 does not currently require:

- one UI framework for every platform;
- runtime UI hot swapping;
- a plugin marketplace;
- dynamic dependency resolution;
- one crate or binary per capability;
- rewriting proven C/C++ solely for language uniformity;
- restoring the old repository hierarchy.

Implementation should grow one verified slice at a time.
