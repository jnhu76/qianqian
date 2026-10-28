# COMMANDS — exact replay on a Windows x86_64 host

```text
TRUTH CLASS   reproduction procedure for the executed evidence
SUBJECT       #324 Windows native Tier-2 campaign
```

## 0. What you need to check out

The harnesses and this package are **committed by the #324 evidence PR**; they
do not exist at the campaign's base commit. Check out the PR head (or apply the
PR branch), then create a disposable worktree from it:

```bash
cd /c/Users/fred1/source/qianqian
git fetch origin
git worktree add --detach /c/Users/fred1/source/qianqian-324 <PR-HEAD-SHA>
cd /c/Users/fred1/source/qianqian-324
```

The owner's main checkout is never modified.

```text
WT   = <the worktree above>
PKG  = $WT/evidence/windows-native-tier2-324
```

## 1. Host shell and toolchain

All builds run on the Windows host. Git Bash supplies `sha256sum` (needed by
`crates/qianqian-songcore-sys/build.rs`) and the POSIX shell used by the
orchestrator; the compiler/linker are native Windows binaries.

```bash
export PATH="/c/Users/fred1/tools/mingw64/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-gcc-ar
```

`/c/Users/fred1/tools/mingw64/bin` is WinLibs GCC 16.2.0 (`x86_64-w64-mingw32-gcc`,
`x86_64-w64-mingw32-gcc-ar`, binutils 2.47). `.cargo/config.toml` already names
that linker for `x86_64-pc-windows-gnu`; only `ar` needs the override because
WinLibs ships `x86_64-w64-mingw32-gcc-ar`, not a bare prefixed `ar`.

**Python**: use a real interpreter. On this host the Store-alias `python3`
exits 49; the working one is
`C:\Users\fred1\AppData\Local\Programs\Python\Python313\python.exe`
(also exposed as `/c/Users/fred1/tools/bin/python3`).

## 2. Put the harnesses into the test tree

The PR commits the harnesses under `evidence/…/harness/` (they are not product
tests). Copy them in before building:

```bash
cd "$WT"
cp "$PKG"/harness/native_ffmpeg_bridge_controls.rs \
   "$PKG"/harness/native_decode_read_controls.rs \
   "$PKG"/harness/decode_seek_controls.rs \
   "$PKG"/harness/windows_native_smoke.rs \
   "$PKG"/harness/windows_ffmpeg_callback_ownership.rs \
   crates/qianqian-decode-songcore/tests/
```

This is the only working-tree change the replay makes; it is untracked and is
not part of any product build or CI job.

## 3. Windows target artifact (`ARTIFACT.md`)

WSL2 is build assistance only; the output is a Windows COFF archive. Pick any
WSL-local build root (`$WSLROOT`).

```bash
export WSLROOT=/home/$USER/q324
wsl.exe -d Ubuntu-24.04 -- bash -lc "
  set -e
  rm -rf $WSLROOT && mkdir -p $WSLROOT
  cd /mnt/c/Users/fred1/source/qianqian-324
  tar -cf - --exclude=native/build native | (cd $WSLROOT && tar -xf -)
  cd $WSLROOT/native
  bash scripts/fetch-ffmpeg          # downloads + verifies the pinned zip
  python3 tools/ffmpeg_import.py --recipe windows-x86_64
  xmake f -p mingw -a x86_64 -m release -y
  xmake build songcore || true       # see the shared-target note below
  ls -l build/artifacts/libsongcore.a
  sha256sum build/artifacts/libsongcore.a build/ffmpeg-xmake/manifest.json
  mkdir -p /mnt/c/Users/fred1/source/qianqian-324/native/build/artifacts
  cp build/artifacts/libsongcore.a \
     /mnt/c/Users/fred1/source/qianqian-324/native/build/artifacts/
  cp build/ffmpeg-xmake/manifest.json \
     /mnt/c/Users/fred1/source/qianqian-324/native/build/
"
```

Notes:

- `xmake build songcore` also attempts `songcore_shared`, whose Windows DLL link
  fails on BCrypt (FFmpeg `libavutil/random_seed.c`). The `windows-x86_64`
  recipe declares `artifacts.shared = false`, so that failure is expected and
  irrelevant to this campaign; the **static** archive is produced and merged
  before the DLL link. `|| true` keeps the build going past it; verify the
  static archive exists afterwards.
- The archive hash is path-dependent (absolute build paths in debug info). A
  rebuild in another directory yields a different sha256; the stable identity is
  `native/src/songcore_ffmpeg.c` (`55eab243…`) plus the manifest. If you rebuild,
  update `EXPECTED_ARTIFACT_SHA256` in the three control harnesses **and** the
  consumer smoke expectation in `ARTIFACT.md`.
- The reference artifact of record is
  `90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2`.

Format / ABI audit:

```bash
cd "$WT"
export PATH="/c/Users/fred1/tools/mingw64/bin:$PATH"
objdump -f native/build/artifacts/libsongcore.a | grep -c "file format pe-x86-64"   # 280
objdump -f native/build/artifacts/libsongcore.a | grep -c "file format elf"         # 0
nm --defined-only native/build/artifacts/libsongcore.a | grep -c " T song"          # 15
```

## 4. Windows native smoke (S1 / S2 / S3)

```bash
cd "$WT"
export PATH="/c/Users/fred1/tools/mingw64/bin:/c/Users/fred1/tools/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-gcc-ar
bash "$PKG/harness/run_smoke.sh"
```

Writes `raw/smoke/{fixture-identity,consumer-smoke,s2-rust-adapter,s3-ffmpeg-callback}.txt`.
Expected: `CONSUMER_SMOKE=PASS`, `S2_VERDICT=PASS`, and for S3
`ffmpeg=0` (unset), `ffmpeg=0` (`0`), `ffmpeg>=1` (`1`) with a delivered
decode-fault record in the negative cases.

## 5. The twelve cells (healthy + counterfactual + restored)

```bash
cd "$WT"
export PATH="/c/Users/fred1/tools/mingw64/bin:/c/Users/fred1/tools/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-gcc-ar
export QIANQIAN_NATIVE_WORKTREE="$WT"
python3 evidence/windows-native-tier2-324/harness/counterfactual_runner.py
```

Replay one cell into its own receipts (it does **not** overwrite the 12-cell
JSON):

```bash
Q324_ONLY=seek-C1 python3 evidence/windows-native-tier2-324/harness/counterfactual_runner.py
# writes raw/healthy-only-seek-C1.json and raw/counterfactuals-only-seek-C1.json
```

Raw layout:

```text
raw/healthy/<cell>.log          one fresh process, healthy control
raw/mutants/<cell>-red.log      mutation applied, assertion RED
raw/restored/<cell>-green.log   bytes restored, same cell GREEN
raw/builds/<cell>-*.log         cargo build transcripts (normal / mutant / restored)
raw/healthy.json                healthy verdicts + artifact and source hashes
raw/counterfactuals.json        RED/GREEN records + mutation/restore/binary hashes
```

Each receipt begins with `===CELL_BEGIN===` and records `exit_code`,
`timed_out` (the OUTER watchdog only), `exe_sha256`, the child environment, the
parent's stderr policy (`drained_by_parent` or `never_read=true`), and the
child's stdout/stderr.

## 6. Reading a receipt

```text
CELL-PASS / *-PASS / *-VERDICT=PASS   the child's own pass marker
ASSERTION …                          this harness's stdout panic hook
test result: ok. 1 passed            the test binary executed exactly one test
test result: FAILED …                the mutant arm
PRODUCER-NOT-BACKPRESSURED           ffmpeg C1 counterfactual signature
```

`timed_out=True` is the **outer** `run_cell.ps1` watchdog (180 s) killing a
wedged child; it is never counted as a RED. A bounded-liveness cell may itself
assert "did not complete within N seconds" **inside** the child — that is the
cell's own predicate, it prints a named assertion, and it is recorded as
`assertion_red` (see REVIEW.md, finding D-1).

## 7. Probe the Windows sink primitives (optional, but recommended)

The preflight probe sources are shipped:

```bash
cd "$PKG/harness/probes"
export PATH="/c/Users/fred1/tools/mingw64/bin:$PATH"
for f in probe pipeinfo grow; do
  rustc --target x86_64-pc-windows-gnu -C linker=x86_64-w64-mingw32-gcc -O $f.rs -o $f.exe
done
./probe.exe; ./pipeinfo.exe; ./grow.exe
```

This reproduces `raw/smoke/sink-primitives.txt`: `SetStdHandle` redirection,
`ERROR_NO_DATA (232)` on a closed-read-end pipe, the parked 64 KiB write, the
process-global stderr lock.

## 8. Cleanup

```bash
cd /c/Users/fred1/source/qianqian
git worktree remove --force /c/Users/fred1/source/qianqian-324
```

Blocked/abandoned cells deliberately leave threads parked inside `WriteFile`
(in-process pipes that are never read). Process exit is the cleanup; no HANDLE
or reader is leaked into the next cell because every cell is a fresh process.
