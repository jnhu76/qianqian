# ENVIRONMENT — Windows x86_64 host identity

```text
TRUTH CLASS   executed verification evidence (environment record)
SUBJECT       #324 Windows native Tier-2 campaign
```

## Host

| Fact | Value |
|---|---|
| OS | Windows 10.0.26300 (Windows 11 class build, real host — not a container, not WSL) |
| Architecture | X64 |
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H |
| Shell used for host capture | PowerShell 5.1.26100.9711 (Desktop) |
| Native Windows shell for builds | Git Bash (MSYS2 runtime) invoking the Windows toolchain |
| Elevation | not required and not used |
| Repository checkout | `C:\Users\fred1\source\qianqian-324` — a `git worktree` at `9ffcaba945eb8f47b9e30dddc67c805a5bda078a`, **detached**, so the owner's main worktree is untouched |
| Line endings | `core.autocrlf=false`, LF worktree (required: the mutation/receipt anchors are exact-byte) |
| Working tree at start | clean at `9ffcaba`; the host capture in `raw/smoke/environment.txt` was taken after the five harness files were copied into `crates/qianqian-decode-songcore/tests/` and before this evidence directory was written, which is why that snapshot lists only the five files. `git diff HEAD` stayed empty throughout: no product source was modified. |

### Rust

```text
rustc 1.96.0 (ac68faa20 2026-05-25)
host triple    x86_64-pc-windows-msvc
target triple  x86_64-pc-windows-gnu      (the test executables are PE x86_64)
cargo 1.96.0 (30a34c682 2026-05-25)
installed targets: x86_64-pc-windows-gnu, x86_64-pc-windows-msvc
```

Cargo environment used for every build in this campaign (the repository's
`.cargo/config.toml` supplies `linker = x86_64-w64-mingw32-gcc`):

```bash
export PATH="/c/Users/fred1/tools/mingw64/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-gcc-ar
cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml \
  --target x86_64-pc-windows-gnu --test <bin> --no-run
```

`sha256sum` is present at `/usr/bin/sha256sum` (Git coreutils 8.32) and is what
`crates/qianqian-songcore-sys/build.rs` shells out to. See the artifact note
about its Windows path escaping.

### C toolchains

| Role | Identity |
|---|---|
| Windows-native consumer/build (WinLibs) | `gcc.exe (MinGW-W64 x86_64-msvcrt-posix-seh, built by Brecht Sanders, r1) 16.2.0`, target `x86_64-w64-mingw32`, binutils 2.47.20260726, at `C:\Users\fred1\tools\mingw64\bin` |
| Windows linker used by rustc (windows-gnu) | the same `x86_64-w64-mingw32-gcc`, via `.cargo/config.toml` |
| Target-artifact build (cross, allowed build assistance) | WSL2 Ubuntu 26.04.1, kernel `6.18.54.1-microsoft-standard-WSL2`, `x86_64-w64-mingw32-gcc (GCC) 13-win32`, binutils `2.45.90.20260125` (`ar`/`ld`), `xmake v2.9.7+20250101` — captured in `raw/smoke/artifact-toolchain.txt` |

The SongCore archive is a **Windows target** artifact (COFF `pe-x86-64`), built
by the target recipe's declared mingw-w64 cross toolchain and then consumed,
linked and executed by Windows-hosted PE binaries. No Linux ELF artifact or
Linux runtime is used anywhere in this campaign; see ARTIFACT.md for the
format/identity audit and the prohibition it satisfies.

### Support tools

```text
Windows Python 3.13.14  (C:\Users\fred1\AppData\Local\Programs\Python\Python313\python.exe)
Git coreutils sha256sum 8.32
PowerShell 5.1 / .NET Process + Win32 anonymous pipes
```

## Windows sink primitives measured on THIS host

These premises are what the twelve cells stand on. They were measured with a
standalone probe (`raw/smoke/sink-primitives.txt`) before any cell ran, because
the Linux premises (`/dev/full`, `dup2`/CRT fd 2, POSIX `EPIPE`) do not
transfer.

| # | Fact | Observation |
|---|---|---|
| 1 | Rust std resolves `STD_ERROR_HANDLE` on **every** write | `std`'s `sys/stdio/windows.rs` says so explicitly ("Don't cache handles … such as if a process calls `SetStdHandle`", rust-lang#40490); confirmed: after `SetStdHandle(2, pipe)` a `write_stderr_line` reaches the new pipe |
| 2 | A closed-read-end anonymous pipe fails real writes | `CreatePipe` + `CloseHandle(read)` + `SetStdHandle(2, write)` → `write_all` returns `Err(Os { code: 232, kind: BrokenPipe })` = `ERROR_NO_DATA` |
| 3 | `dup2`/CRT fd 2 is **not** the Windows mechanism | Rust's std never consults CRT fd 2 on Windows; the campaign therefore uses `SetStdHandle` + Win32 handles, never `dup`/`dup2`/`/dev/full` |
| 4 | An anonymous pipe's queue is not 4 KiB-fixed | a single 4 KiB write completes; a 64 KiB write to a no-reader pipe **parks**; `PeekNamedPipe` reports the queued bytes. The campaign therefore does not claim "pipe buffer = 4096"; it witnesses quota exhaustion directly (a 64 KiB write parked **and** an independent 1-byte write parked) |
| 5 | `std::io::stderr().lock()` is a real process-wide mutex on Windows | a helper thread holding it blocks another thread's `write_all` for >500 ms; the seek C1 cell uses this (and says so) instead of pretending it is pipe backpressure |
| 6 | `NUL` is not a failing sink | not used as a failure; it is only used (if at all) as a post-test restore target in the throwaway probe, never as the C4 premise |

`raw/smoke/sink-primitives.txt` is the probe's own output; it is a
platform-premise record, not a cell.

## Windows-specific differences from the Linux campaign (#323)

```text
Linux premise                          Windows replacement in this campaign
-------------------------------------  --------------------------------------------
/dev/full for C4                       anonymous pipe with the READ end closed
dup2(2, fd) / AsRawFd                  SetStdHandle(STD_ERROR_HANDLE, handle)
POSIX EPIPE                             ERROR_NO_DATA (232) / ERROR_BROKEN_PIPE (109)
pipe capacity assumed 4 KiB             quota exhaustion witnessed by two parked writes
std stderr lock                        kept — verified process-global on Windows
pthread                                 std threads (Condvar/mpsc), same shape
cargo test runner (bash/timeout -k)      run_cell.ps1 (.NET Process + Win32 pipe)
Unix ffmpeg_log_gate.rs (#320)           windows_ffmpeg_callback_ownership.rs
                                         (observes the facility, not fd 2)
```

## Out-of-scope build observations (reported to #172, not fixed here)

1. `crates/qianqian-songcore-sys/build.rs` hashes the archive by shelling out to
   `sha256sum`. For an absolute Windows path containing backslashes, GNU
   coreutils escapes the file name with a leading `\`, and `build.rs` takes the
   first whitespace token, so `QN_SONGCORE_ARTIFACT_SHA256` is
   `\90bde1ca…` rather than `90bde1ca…`. The harness records both the raw and
   the normalized value and compares the normalized digest; the defect is
   recorded for #172 because it is a build-provenance bug, not a verification
   result.
2. `xmake build songcore` also builds `songcore_shared`, whose Windows DLL link
   fails on `BCryptOpenAlgorithmProvider` / `BCryptGenRandom` /
   `BCryptCloseAlgorithmProvider` (FFmpeg's `libavutil/random_seed.c`). The
   `windows-x86_64` recipe declares `artifacts.shared = false`, so the shared
   artifact is not part of the #324 gate; the static archive built and merged
   correctly.
