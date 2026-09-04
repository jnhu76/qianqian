# Qianqian Desktop

The Qianqian desktop application: a Windows-first local music player built as
a plain Kotlin/JVM Compose Desktop application.

## Current stage: DESKTOP-BOOTSTRAP-1 (#31)

This is the minimal application shell. It proves only:

> Qianqian now has a production-owned Desktop application shell with correct
> repository ownership and build structure.

The native runtime is NOT connected yet. No playback, no audio, no file
picking — those arrive in DESKTOP-NATIVE-BRIDGE-1 and DESKTOP-PLAYER-MVP-1.

## Technology

- Plain Kotlin/JVM (no Kotlin Multiplatform source sets).
- [Compose Multiplatform](https://www.jetbrains.com/lp/compose-multiplatform/)
  Desktop (JVM) for the window/toolkit.
- Gradle is self-contained inside `apps/desktop/`; the repository root remains
  the Xmake workspace for the native runtime. Gradle does not build native.

## Development environment

Development happens under WSL/Linux; the product target is Windows Desktop.
WSL runs prove compile, unit tests, and (with WSLg) that the window opens on
Linux. Windows-specific truth (jpackage packages, `qianqian.dll`, WASAPI
playback) is explicitly out of scope for WSL validation.

## Run

```bash
cd apps/desktop
./gradlew run
```

A window titled "Qianqian" with the text `Desktop bootstrap OK` should open.
Requires a JDK 17+ and a GUI environment. Under WSLg, Skiko's hardware OpenGL
path may fail (`Cannot create Linux GL context`); run with software rendering
in that case:

```bash
SKIKO_RENDER_API=SOFTWARE ./gradlew run
```

## Build and test

```bash
cd apps/desktop
./gradlew compileKotlin test
```

## Intentionally absent (do not add here without an issue)

Native runtime loading / JNA binding, player controls, file picker, seek bar,
playlist, library, settings, themes/design system, packaging claims. See the
DESKTOP campaign stages and `AGENTS.md` in this directory.
