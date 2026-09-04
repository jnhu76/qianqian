# KMP FFI Handoff — Native Runtime Closure v1 → first Kotlin slice (closed)

> Historical / non-authoritative。这是 Native Runtime Closure v1 → 第一个
> KMP 集成切片的 phase entry contract，该阶段已完成（exit evidence 见
> [kotlin-boundary-probe](../experiments/kotlin-boundary-probe.md)）。
>其中的 durable FFI 边界规则已提炼为 current contract：
> [contracts/ffi-boundary.md](../../contracts/ffi-boundary.md)——以该契约
> 为准，本文只保留 phase 目标、非目标与完成证据。

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
