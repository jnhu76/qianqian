# Qianqian Desktop

The Qianqian desktop application: a Windows-first local music player built as
a plain Kotlin/JVM Compose Desktop application.

## Current stage: DESKTOP-PLAYER-MVP-1 (#35)

The application now has a minimal, functional local-file player UI on top of
the production native bridge (PR #32, PR #34):

> Launch the app, choose one local audio file through the platform file
> chooser, open (→ READY, never autoplay), Play / Pause / Resume, Seek by
> drag-release commit, observe native state / position / duration, Stop,
> and see typed product errors — with the native engine as the single
> playback truth.

Intentionally still absent: playlists/queue, library, settings, metadata
display (filename only), artwork, volume, design system, keyboard shortcuts,
packaging. Those belong to later DESKTOP stages.

## Player architecture (application layer)

```text
Compose UI (PlayerScreen)
        ↓ collects
PlayerUiState (app projection: enablement, seek preview, product errors)
        ↓ coordinated by
PlayerScreenModel (application coordination owner)
        ↓ commands / snapshot flow
PlayerPort (suspend boundary)
        ↓
NativePlayerAdapter → JNA → frozen C ABI → PlayerEngine → SongCore → FFmpeg
```

- **Single playback truth**: the native `PlayerEngine` state machine. The UI
  never sets a playback state optimistically; every rendered playback value
  (state, position, duration) comes from `PlayerSnapshot` via the 10 Hz
  poller. App-owned state is limited to operation-in-flight, seek
  preview/commit display, selected file, and product error classification.
- **Open semantics**: open → READY, no autoplay (native contract preserved).
  A failed candidate is never promoted to "current track"; the previous
  successfully opened track remains displayed. Per the frozen contract, a
  failed `pe_open` leaves the engine EMPTY (the previous handle is dropped
  during open) — the UI reflects that truthfully and recovery is "choose
  another file".
- **Seek UX**: dragging only updates a local preview (zero native calls);
  releasing commits exactly one native seek; the engine-reported landing
  stays displayed until a snapshot lands within its window (display
  reconciliation, not a causal fence). Unknown duration renders
  truthfully as `--:--` with the slider disabled — never a fake `0:00`.
- **File picker**: the JDK platform dialog (`java.awt.FileDialog`) — real
  native chooser on Windows/GTK, single selection, cancel-safe, no new
  dependencies. No extension filter: filename is not codec truth; native
  open stays authoritative.
- **Command ownership**: Compose passes user intent only; the
  `PlayerScreenModel` owns every user-command coroutine, its admission
  (at most one in flight), and cancel-and-drain. No blocking native call
  ever runs on a Compose EDT. Window close drains model-owned coroutines,
  then closes the `PlayerPort` exactly once before the application exits.
- **Errors**: typed bridge failures classify into a minimal product set
  (file unavailable / could not open track / command failed / runtime
  unavailable / chooser failed). Diagnostic strings are never parsed;
  `snapshot.lastError` is log-only.

## Native bridge

- **Binding**: [JNA](https://github.com/java-native-access/jna) 5.19.1
  (current stable, Maven Central). Plain C calling convention; the
  production interface maps only the symbols this stage needs (both ABI
  version gates + the 8 `pe_*` lifecycle/observation functions).
- **Bridge layout** (`qianqian.desktop`):
  - `app/` — application layer: `PlayerScreenModel` + `PlayerUiState`
    (coordination/projection), `PlayerScreen` (Compose), `FilePicker` +
    `AwtFileDialogPicker`, `AppRuntime` (startup connection + exit
    ownership), `TimeFormat` (pure mm:ss / h:mm:ss formatting).
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
- **Windows truth**: validated (DESKTOP-WINDOWS-VALIDATION-1, #39) on a
  native Windows checkout: the staged mingw x86_64 `qianqian.dll` (WASAPI
  runtime flavor) loads through the explicit staged path, both ABI gates
  pass on the Windows JVM, `song_io` JVM callbacks serve real decode, real
  FLAC playback is audible, media position progresses from WASAPI render
  truth, and a short fixture reaches ENDED with replay-from-ENDED —
  machine evidence in `WindowsRuntimeLifecycleTest` plus the human audible
  gate. Known residual: playing seeks replay up to one device buffer of
  pre-seek audio before the new position lands audibly (#40, separate
  native corrective). jpackage app-image validation remains open.

## WSL/Linux proof scope

The Linux runtime is the engine-only ABI flavor: no real audio output
backend. The WSLg GUI smoke therefore proves — with real staged
`libqianqian.so` and real corpus files — the full interaction slice:
window + real platform file chooser, open → READY with real duration,
play/pause/resume/stop state projection, one-commit seek, product error
paths, and clean shutdown. It does NOT (and cannot) prove audible playback,
render progression, or ENDED-through-render: Linux `play()` legally reports
PLAYING with position holding at the seek/segment landing. Windows remains
the product playback truth.

## Technology

- Plain Kotlin/JVM (no Kotlin Multiplatform source sets).
- [Compose Multiplatform](https://www.jetbrains.com/lp/compose-multiplatform/)
  Desktop (JVM) for the window/toolkit; Material 2 (the layer already
  present at bootstrap).
- JNA for the native bridge; kotlinx-coroutines for confinement and
  `StateFlow`. No DI/state/navigation framework, no file-picker library.
- Gradle is self-contained inside `apps/desktop/`; the repository root
  remains the Xmake workspace for the native runtime. Gradle never
  compiles native code — it only invokes Xmake and copies the canonical
  runtime artifact into the app-owned staging location.

## Development environment

Development happens under WSL/Linux; the product target is Windows Desktop.
WSL runs prove compile, unit tests, the real-runtime bridge integration
tests, and the WSLg GUI smoke. Windows validation (verified commands,
DESKTOP-WINDOWS-VALIDATION-1):

```powershell
# native runtime (one-time mingw session config; SDK at C:\mingw64, WinLibs GCC 13.3 UCRT)
xmake f -p mingw --mingw=C:\mingw64 -m release --av_manifest=build/manifests/windows-mingw-x86_64/codec-base/manifest.json
xmake build -y qianqian_runtime     # → build\artifacts\windows-mingw-x86_64\runtime\qianqian.dll

cd apps\desktop
.\gradlew.bat stageNativeRuntime            # stage into build\native-dev\windows-x86_64\
.\gradlew.bat test                          # pure JVM suite
.\gradlew.bat nativeBridgeIntegrationTest   # real-runtime proof (WindowsRuntimeLifecycleTest)
.\gradlew.bat run                           # audible playback through WASAPI
```

The FFmpeg compile closure for the mingw session is machine-derived once
(`windows-mingw-x86_64` target recipe) and replayed by Xmake from
`build/manifests/`; the manifest path is passed via `--av_manifest`. The
mingw `bin` directory must be on PATH while running until the runtime
stops importing `libwinpthread-1.dll` (deferred packaging finding).

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
./gradlew test                        # pure JVM application + bridge logic (no native)
./gradlew stageNativeRuntime          # build + stage the runtime (Xmake)
./gradlew nativeBridgeIntegrationTest # real-runtime lifecycle proof
```

`test` covers pure JVM behavior: application coordination over a fake
`PlayerPort` (open/play/pause/stop/seek/error/close paths, seek-call
bounding, no optimistic state), UI-state projections, time formatting, and
bridge logic (ABI fail-fast, snapshot mapping, control serialization,
close semantics, poller lifetime) with a fake `NativeApi`.
`nativeBridgeIntegrationTest` is a separate task because it requires the
staged native runtime; staging stays an explicit developer action.

## Run

```bash
cd apps/desktop
./gradlew run
```

A window titled "Qianqian" opens with the minimal player screen:
Open File, current track, native state, snapshot timeline, Play/Pause +
Stop, and product error text. Requires a JDK 17+ and a GUI environment.
Under WSLg, Skiko's hardware OpenGL path may fail (`Cannot create Linux GL
context`); run with software rendering in that case:

```bash
SKIKO_RENDER_API=SOFTWARE ./gradlew run
```

The runtime is loaded at startup from the staged location
(`NativeRuntimeLoader.devStagedLibraryPath`); a missing/unrejected staged
runtime shows a truthful "Playback runtime unavailable." state instead of
crashing.

## Intentionally absent (do not add here without an issue)

Playlists/queue, library, folder scan, settings, search, metadata display
(beyond filename), artwork, lyrics, EQ/DSP, volume system, device
selector, system tray, hotkeys, media-session, visualizer, skins/themes,
custom title bar, keyboard shortcuts, packaging claims, telemetry. See
the DESKTOP campaign stages and `AGENTS.md` in this directory.
