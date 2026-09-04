# apps/desktop local AGENTS

This file extends the repository root AGENTS.md; it does not replace it.
Only Desktop-local rules live here.

## Local scope

`apps/desktop/` owns the application / product layer: window, screens, UI
state, user-facing text, file selection, product error presentation, and
(later) queue / library / navigation policy. Native owns decode, playback
state, timeline, and the audio backend; Desktop only consumes it.

## Forbidden dependencies

- No FFmpeg headers or types (`AV*`) anywhere in this module.
- No dependency on `native/src/**`, private native headers, PlayerEngine C++
  internals, `songcore_static` / `songcore_shared`, or WASAPI types.
- Native consumption is only through `native/include/songcore.h` +
  `native/include/player_engine.h` and the runtime library
  (`qianqian.dll` / `libqianqian.so`) via the frozen public ABI.
- Do not add DI/state-machine/navigation frameworks without a current issue
  authorizing them.
- Gradle must not define native compilation targets; the repository root
  remains the Xmake workspace.

## Required authorities

Before changing this module, read:

- [DESKTOP-IA-1 (#30)](https://github.com/jnhu76/qianqian/issues/30) — frozen
  application architecture (runtime model, ownership, threading, error model).
- [docs/architecture/overview.md](../../docs/architecture/overview.md) and
  [docs/contracts/ffi-boundary.md](../../docs/contracts/ffi-boundary.md).
- When consuming the runtime: [docs/contracts/player-api.md](../../docs/contracts/player-api.md).

## Local rules

- Do not duplicate the PlayerEngine state machine; native playback state is
  the single truth, Desktop only projects it.
- Do not change the native ABI for UI convenience; a genuine C-boundary gap
  gets its own contract issue.
- User-facing text, recovery, and navigation policy belong here, never in
  native.
- Compose must not leak into native adapter logic; the adapter layer does not
  know Compose types.
- Windows-first product truth: WSL/Linux runs prove compile, unit tests, and
  control-path only. Real WASAPI playback, jpackage, and DLL search behavior
  require Windows evidence (`CODE_COMPLETE_PENDING_WINDOWS_VALIDATION`, never
  promoted to PASS).
- Significant UI design changes require UI-WORTH evidence per root AGENTS and
  DESKTOP-IA-1; keep architecture proportional to current product size.

## Local verification

From `apps/desktop/`:

```bash
./gradlew compileKotlin test
```

GUI-dependent behavior may additionally be recorded as
`CODE_COMPLETE_PENDING_GUI_RUNTIME_VALIDATION` when the environment is
headless. Never fake a PASS.
