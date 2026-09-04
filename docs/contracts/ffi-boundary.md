# FFI boundary

> Authority: Normative
> Scope: application 层（Kotlin/KMP 或任何 UI 技术）与 native runtime 之间的
> 加载、依赖与变更边界

## Purpose

KMP（及任何 application 技术）是 native runtime 的 **consumer**。本契约
冻结消费者能看到什么、永远不能看到什么，以及边界不足时的变更程序。

## Public surface

```text
Application (Kotlin / KMP / Swift / Qt / future)
      |  thin platform binding (JNI / JNA / Panama / cinterop)
      v
songcore.h + player_engine.h   (frozen C ABI, v1)
      v
qianqian runtime  (qianqian.dll / libqianqian.so)
      exports: 9 pe_* + 15 song_* symbols, nothing else
```

- The application-facing native surface is exactly the frozen `song_*` +
  `pe_*` C ABI exported by the single `qianqian` runtime library.
- Loaders gate on `songcore_abi_version()` / `player_engine_abi_version()`.
- SongCore caller-side rules (borrowed views, PCM, errors):
  [songcore-api.md](songcore-api.md)。PlayerEngine 行为：
  [player-api.md](player-api.md)。

## Consumer boundary rules

The application MUST NOT know about or depend on:

- SongCore internals or private headers;
- PlayerEngine C++ internals;
- FFmpeg or libswresample headers/libraries;
- WASAPI types or device negotiation;
- implementation archives such as `libsongcore.a`;
- native render-thread ownership or lifecycle details.

Implementation archives (`songcore_static` / `songcore_shared`) stay
internal to the qianqian product runtime: the product application never
links them. This rule does not retire the standalone SongCore consumer
path (`libsongcore.a` / `libsongcore.so` / `songcore.dll`), which is
owned by [songcore-api.md](songcore-api.md) and
[../development/release.md](../development/release.md).

## Cross-layer change rule

**If application integration appears to require changing PlayerEngine
playback semantics, the SongCore ABI, the WASAPI renderer
contract/behavior, or the frozen FFmpeg closure, STOP the integration work
and open a separate native-layer corrective with its own evidence.**

Do not weaken or route around this condition by adding a convenience C
ABI, exporting C++ symbols, linking implementation archives into the
application side, or moving device-format negotiation into PlayerEngine
merely to make the binding easier.

A genuine missing primitive MUST be demonstrated at the frozen C boundary
before any native change is authorized.

## Ownership and lifetime

- The FFI wrapper owns the language-side object lifetime; native handles
  stay opaque. One wrapper object ⇔ one native handle; a wrapper `close()`
  MUST reach the native destroy exactly once.
- Control calls are caller-serialized; snapshots may be polled
  concurrently (per-contract threading rules).
- Borrowed native views (metadata/artwork) have contract-limited
  lifetimes; copy out if they must outlive them.

## Compatibility

- ABI version functions gate loading; layout is fixed; compatible
  additions use reserved fields or new functions; any break is ABI v2
  (explicit version bump + new layout snapshot).
- FFmpeg types never cross this boundary — there are none in the ABI
  ([negative-capability-manifest](../architecture/negative-capability-manifest.md)
  guards the core side).

## Required verification

- External consumers compile against ONLY the public headers + the
  documented link line (`tests/consumer/songcore_static_smoke.c`).
- Shared/PE export audits: exactly the frozen symbol set, nothing else
  (`tests/songcore/regression.py`, `bench/results/songcore-v1/`).
- The Kotlin/Native boundary probe (historical evidence:
  [archive/experiments/kotlin-boundary-probe.md](../archive/experiments/kotlin-boundary-probe.md))
  proved a Kotlin-owned process completing a real playback lifecycle
  through this boundary.
