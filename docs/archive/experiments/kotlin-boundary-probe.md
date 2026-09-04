# Kotlin / Native Boundary Probe v0 — Evidence Record

> Historical / non-authoritative。one-time 集成实验证据。durable FFI 边界
> 契约：[contracts/ffi-boundary.md](../../contracts/ffi-boundary.md)；
> phase 记录：[../closeouts/kmp-ffi-handoff.md](../closeouts/kmp-ffi-handoff.md)。

Status: **BOUNDARY PROOF COMPLETE**（phase exit evidence，见
[../closeouts/kmp-ffi-handoff.md](../closeouts/kmp-ffi-handoff.md)）。

This is a boundary proof, not a new application architecture. Zero production
native code changed; the probe consumes the frozen runtime exactly as a
KMP consumer would.

## Verdict

```text
Proof A  libsongcore.a external static link + frozen song_* ABI   PASS
Proof B  Kotlin-owned process -> frozen C ABI -> qianqian runtime PASS (Windows REAL)
                                                                PASS (Linux SIMULATED)
ABI      runtime export surface after probe                     24 / 24, zero drift
```

## Revision

BASE: b7f5f6b6ea1e69013b7712689b5a3f9da66fbc68 (main, Native Runtime Closure v1)
BRANCH: feat/kotlin-native-boundary-probe
WORKTREE: clean except intended `tests/kotlin_probe/` + this document.

## What was proven

### Proof A — libsongcore.a (existing permanent gate, reused)

The pre-existing external-consumer gate
(`python3 tests/songcore/consumers.py --out --core`) passed all three
core gates on Linux: shared export audit (exactly 15 `song_*`),
ctypes decode consumer (Common Formats corpus), and the static archive
consumer `tests/consumer/songcore_static_smoke.c` compiled by plain `cc`
against ONLY `include/songcore.h` + the merged `libsongcore.a`
(`-lsongcore -lm -lpthread`; FFmpeg closure merged inside — no other
Qianqian archive on the link line). The same external consumer also
decoded the real target song (FLAC and MP3, 44.1 kHz stereo) directly
from its original path.

### Proof B — Kotlin consumer (tests/kotlin_probe/)

Kotlin/Native 2.1.21-RC2, `cinterop` over a 7-line `.def` whose binding
surface is `include/player_engine.h` (+ `include/songcore.h`) plus a
minimal local CRT stdio declaration header for the host `FILE*`
`song_io`. One thin `ProbeHost` seam absorbs platform differences
(sleep; CRT seek/tell surface — `_fseeki64`/`_ftelli64` on Windows,
`fseek`/`ftell` on Linux). Link: against the runtime
import library (`qianqian.dll.a` on Windows, `libqianqian.so` on Linux).
No private header, no implementation archive, no backend knowledge.

Corrective (post first REAL pass): the Windows host initially bound
`fseek` and cast the frozen 64-bit `song_io` offsets to mingw's 32-bit
`long` — silent ±2GB clipping that the 9MB target song never exercises.
The Windows binding now declares `_fseeki64`/`_ftelli64` so the probe
expresses the full frozen contract; Linux keeps `fseek`/`ftell`
(glibc `long` is 64-bit). Probe-only change: no `src/`, no ABI delta.

Windows (qianqian.dll + WASAPI render thread, default endpoint), target
song 隐形的翅膀 (MP3, 44100 Hz stereo, duration 224080544 us), re-run
after the corrective:

```text
create -> EMPTY snapshot
open   -> READY, duration known
play   -> PLAYING, REAL render progression (position_us advancing)
monotonic position while PLAYING
pause  -> PAUSED, position frozen; resume -> PLAYING
seek   -> target 212080544 us, landing 212062040 us (CONFIRMED)
ENDED  reached through the real output at position == duration
         (224080544 us, exact)
stop   -> READY @0
destroy (NULL destroy documented no-op)
Exit 0. Classification: REAL.
```

Linux (libqianqian.so, engine-only runtime flavor per the runtime closure
doc): the complete song_* matrix passes and the PlayerEngine control
lifecycle (create/open/READY/play/pause/resume/seek/stop/destroy) passes;
render progression and ENDED are not exercisable because this flavor
carries no platform output. Classification: SIMULATED (honest, per the
runtime closure's own evidence taxonomy).

## Target audio

隐形的翅膀 (张韶涵). FLAC 44.1/16/2 (24620616 bytes) and MP3 44.1/2
(9136912 bytes), from the user's local Downloads; NOT committed.
The FLAC has a corrupt frame at ~216.7 s: a native full sequential decode
(`qn_pcm_dump`, independent of Kotlin, WSL session) fails with FFmpeg
`invalid sync code` -> status 108; the engine fail-closed to ERROR per the
frozen contract. The MP3 decodes end to end and is the REAL-lifecycle
target above.

## ABI audit

```text
qianqian.dll (rebuilt from this commit, mingw; re-audited after the
                                                probe corrective: unchanged):
  9 pe_* + 15 song_* = 24 exports, nothing else
  imports: bcrypt / KERNEL32 / msvcrt / ole32 only
libqianqian.so (linux): same 24 dynamic exports, nothing else
libsongcore.a: 15 song_* defined; FFmpeg closure merged
                (avformat_open_input / avcodec_find_decoder / swr_init)
```

## Gates before/after

```text
player_gates / player_consumer_c / player_real_songcore_smoke / ffi_smoke:
  PASS on this branch (Linux; ffi_smoke correctly SIMULATED there).
  ffi_smoke on Windows (rebuilt dll): REAL render progression + ENDED.
consumers.py --core (export audit, ctypes, static archive): PASS.
SongCore regression: 6 .mka gate failures — PRE-EXISTING on this
  machine's main (recorded during the runtime-closure session, before
  this branch; this branch has zero src/ delta). Untouched.
Post-corrective re-run: Linux probe rebuilt + PASS (SIMULATED, exit 0);
  Windows probe rebuilt, MP3 REAL lifecycle exit 0 (twice), ABI 24/24.
```

## Reproduce

```bash
# Proof A (linux)
xmake build songcore && python3 tests/songcore/consumers.py --out --core

# Linux runtime + probe
xmake build qianqian_runtime
~/toolchains/kotlin-native-prebuilt-linux-x86_64-2.1.21-RC2/bin/cinterop \
    -def tests/kotlin_probe/qianqian.def -o build/kotlin-probe/qianqian \
    -target linux_x64
~/toolchains/kotlin-native-prebuilt-linux-x86_64-2.1.21-RC2/bin/kotlinc-native \
    tests/kotlin_probe/PlayerEngineProbe.kt tests/kotlin_probe/ProbeLinux.kt \
    -l build/kotlin-probe/qianqian.klib \
    -o build/artifacts/runtime/PlayerEngineProbe -target linux_x64
LD_LIBRARY_PATH=build/artifacts/runtime \
    ./build/artifacts/runtime/PlayerEngineProbe.kexe <audio-file>
```

Windows staging layout used (temporary, `C:\qianqian-kotlin-probe\`,
not a second source of truth): `include/` (two public headers),
`lib/libqianqian.dll.a`, `qianqian.dll`, `probe/` (def + kt sources);
compile with `C:\kotlin\bin\cinterop.bat -target mingw_x64` then
`kotlinc-native.bat ... -target mingw_x64`, run `PlayerEngineProbe.exe`
next to `qianqian.dll`.

## cinterop notes (the only friction found)

- Declarations arriving from SYSTEM headers through an include chain
  (`<stdio.h>`) are not bound, and listing unrelated headers separately
  in `headers` loses the second header's declarations. Both are avoided
  by one local binding header (`probe_abi.h` -> `probe_stdio.h`, seven
  explicit CRT declarations: fopen/fclose/fread shared, seek/tell split
  per platform — `_fseeki64`/`_ftelli64` on Windows, `fseek`/`ftell`
  on Linux) — a binding-surface concern only, no native change.
- Forward-declared opaque handles (`typedef struct song_handle
  song_handle;`) resolve under `cnames.structs.*`, not the library
  package.
- `uint8_t*` binds as `CPointer<UByteVar>?`; `expect/actual` is
  unavailable in plain Kotlin/Native modules (hence the `ProbeHost`
  seam).
