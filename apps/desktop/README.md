# Qianqian Desktop

The Qianqian desktop application: a Windows-first local music player built as
a plain Kotlin/JVM Compose Desktop application.

## Current stage: DESKTOP-NATIVE-BRIDGE-1 (#33)

The application shell now consumes the frozen native runtime through a
narrow typed JVM adapter:

> The production Desktop JVM application can load the staged qianqian
> runtime, gate both frozen ABI versions, create an engine, serve real
> `song_io` host I/O from JVM file callbacks, and drive a legal control
> lifecycle — using only the public C ABI (`songcore.h` +
> `player_engine.h`).

Still intentionally absent: player UI (file picker, transport controls,
seek bar), product recovery policy, playlists. Those belong to later
DESKTOP stages. The bootstrap window is unchanged.

## Native bridge

- **Binding**: [JNA](https://github.com/java-native-access/jna) 5.19.1
  (current stable, Maven Central). Plain C calling convention; the
  production interface maps only the symbols this stage needs (both ABI
  version gates + the 8 `pe_*` lifecycle/observation functions).
- **Bridge layout** (`qianqian.desktop`):
  - `player/` — application-safe surface: `PlayerPort`, `PlayerSnapshot`
    (immutable; media microseconds preserved as reported by native),
    typed `PlayerBridgeException` hierarchy (status-code based).
  - `nativebridge/` — JNA mappings (`NativeTypes`), the `NativeApi` seam,
    the JNA-backed loader (`NativeRuntimeLoader`: explicit absolute-path
    load + ABI gates, typed `AbiMismatch` fail-fast), the `song_io` file
    source (`SongIoSession`: token-slot userdata registry, companion-held
    callback singletons, bounded file I/O only), and the confined
    `NativePlayerAdapter` (single-thread control dispatcher, 10 Hz
    snapshot poller → `StateFlow`, close-exactly-once semantics).
- **Threading**: control calls never run on a Compose/UI thread; they are
  serialized on one daemon executor. `pe_get_snapshot` is polled
  concurrently (contract-legal). Native decode threads attach to the JVM
  through JNA for callbacks; callbacks touch only file I/O.
- **Windows truth**: pending. qianqian.dll loading, Windows JNA calling
  convention, and WASAPI render progression require real Windows evidence
  (`CODE_COMPLETE_PENDING_WINDOWS_VALIDATION`). On x86-64 Windows there is
  a single calling convention, so the plain-C JNA mapping is expected to
  hold — but that is not a substitute for a Windows run.

## WSL/Linux proof scope

The Linux runtime is the engine-only ABI flavor: no real audio output
backend. Linux runs therefore prove — with real staged `libqianqian.so`
and real corpus files — ABI gating, engine lifecycle, real JVM `song_io`
callbacks, decode, seek, snapshots, GC-stress callback survival, and
resource release. They do NOT (and cannot) prove audible playback,
render progression, or ENDED-through-render; Linux `play()` legally
reports PLAYING with no advancing media position. Windows remains the
product playback truth.

## Technology

- Plain Kotlin/JVM (no Kotlin Multiplatform source sets).
- [Compose Multiplatform](https://www.jetbrains.com/lp/compose-multiplatform/)
  Desktop (JVM) for the window/toolkit.
- JNA for the native bridge; kotlinx-coroutines for confinement and
  `StateFlow`.
- Gradle is self-contained inside `apps/desktop/`; the repository root
  remains the Xmake workspace for the native runtime. Gradle never
  compiles native code — it only invokes Xmake and copies the canonical
  runtime artifact into the app-owned staging location.

## Development environment

Development happens under WSL/Linux; the product target is Windows Desktop.
WSL runs prove compile, unit tests, and the real-runtime bridge
integration tests. Windows-specific truth (jpackage packages,
`qianqian.dll`, WASAPI playback) is explicitly out of scope for WSL
validation.

## Native runtime staging

The application never reads the repository build output directly. Stage
the canonical Xmake artifact into the app-owned dev location:

```bash
cd apps/desktop
./gradlew stageNativeRuntime
# → build/native-dev/linux-x86_64/libqianqian.so
```

`stageNativeRuntime` runs `xmake build -y qianqian_runtime` at the
repository root (the only native build authority) and copies
`build/artifacts/runtime/libqianqian.so` into the staging tree. The
Windows equivalent stages `qianqian.dll` from the mingw artifact
directory.

## Build, test, and bridge proof

```bash
cd apps/desktop
./gradlew test                        # pure JVM bridge logic (no native)
./gradlew stageNativeRuntime          # build + stage the runtime (Xmake)
./gradlew nativeBridgeIntegrationTest # real-runtime lifecycle proof
```

`test` covers pure JVM behavior (ABI fail-fast, snapshot mapping, control
serialization, close semantics, poller lifetime) with a fake `NativeApi`.
`nativeBridgeIntegrationTest` is a separate task because it requires the
staged native runtime; staging stays an explicit developer action.

## Run

```bash
cd apps/desktop
./gradlew run
```

A window titled "Qianqian" with the text `Desktop bootstrap OK` should
open. (The window is still the bootstrap shell; the bridge is exercised
through tests, not UI.) Requires a JDK 17+ and a GUI environment. Under
WSLg, Skiko's hardware OpenGL path may fail (`Cannot create Linux GL
context`); run with software rendering in that case:

```bash
SKIKO_RENDER_API=SOFTWARE ./gradlew run
```

## Intentionally absent (do not add here without an issue)

Player UI (transport, file picker, seek bar, timeline), playlists,
library, settings, themes/design system, packaging claims, product
recovery policy. See the DESKTOP campaign stages and `AGENTS.md` in this
directory.
