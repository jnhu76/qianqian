# KMP FFI Handoff — Frozen Native Boundary

Status: **NEXT PHASE ENTRY CONTRACT**

This document freezes the handoff from Native Runtime Closure v1 to the first
Kotlin Multiplatform integration slice. It is intentionally small: the next
phase proves that Kotlin can consume the already-frozen native runtime; it is
not an authorization to redesign the audio stack.

## Goal

Prove one real lifecycle through the public C ABI only:

```text
Kotlin / KMP
    |
    v
thin platform binding
    |
    v
player_engine.h + qianqian.dll
    |
    v
pe_create -> pe_open -> pe_play -> pe_snapshot -> ENDED -> pe_destroy
```

The first slice may use polling and a minimal console or tiny desktop UI. The
purpose is to validate the language/runtime boundary before introducing an
application architecture.

## Frozen boundary

KMP is a **consumer** of the native runtime. It must not know about or depend
on:

- SongCore internals or private headers;
- PlayerEngine C++ internals;
- FFmpeg or libswresample headers/libraries;
- WASAPI types or device negotiation;
- implementation archives such as `libsongcore.a`;
- native render-thread ownership or lifecycle details.

The application-facing native surface remains the frozen `song_*` + `pe_*` C
ABI exported by the single `qianqian` runtime library.

## Hard STOP condition

**If KMP integration appears to require changing PlayerEngine playback
semantics, the SongCore ABI, the WASAPI renderer contract/behavior, or the
frozen FFmpeg closure, STOP the KMP phase and open a separate native-layer
corrective with its own evidence.**

Do not weaken or route around this condition by adding a convenience C ABI,
exporting C++ symbols, linking implementation archives into the KMP side, or
moving device-format negotiation into PlayerEngine merely to make the binding
easier.

A genuine missing primitive must be demonstrated at the frozen C boundary
before any native change is authorized.

## First-slice non-goals

Do not add these merely to prove FFI consumption:

- playlist/library architecture;
- repository/DI layers;
- Flow/Rx/callback frameworks when polling is sufficient;
- waveform or advanced seek UX;
- Android/Linux audio backends;
- audio effects, mixer, or new resampler work;
- generalized cross-platform player abstractions;
- broad UI design work.

UI design starts only after the Kotlin consumer can complete the real native
playback lifecycle through the frozen ABI.

## Exit evidence

The KMP FFI slice is complete when a Kotlin-owned process can, without private
native dependencies:

1. load/use the `qianqian` runtime;
2. create a player;
3. open a real supported audio file;
4. play it through the existing native backend;
5. observe monotonic progress and terminal `ENDED` at the native snapshot
   boundary; and
6. destroy the player cleanly.

Anything beyond this is a later application/UI phase.
