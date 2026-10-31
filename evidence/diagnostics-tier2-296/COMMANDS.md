# Reproduction commands and evidence identities

Run from the repository root in native PowerShell 7. WASAPI requires a real
Windows render endpoint. Native decode/FFmpeg are deliberately not built
against an unverified archive.

```powershell
git fetch origin main
git rev-parse origin/main
rustc -vV
cargo -V
./research/diagnostics-tier2/mutations.ps1
./research/diagnostics-tier2/matrix.ps1
./research/diagnostics-tier2/results.ps1
./research/diagnostics-tier2/baseline-oracle.ps1 -Python C:/Users/fred1/AppData/Local/Programs/Python/Python313/python.exe
npm ci --prefix website
```

The full original gate runs in a separate native Linux LF checkout:

```bash
export PATH=/home/hoo/.local/bin:/home/hoo/.local/share/fnm/node-versions/v24.15.0/installation/bin:$PATH
bash /mnt/c/Users/fred1/.codex/worktrees/diagnostics-tier2-4534/qianqian/research/diagnostics-tier2/linux-gate.sh \
  /home/hoo/Source/qianqian \
  /mnt/c/Users/fred1/.codex/worktrees/diagnostics-tier2-4534/qianqian \
  /home/hoo/Source/q296-verification-8517c1f \
  8517c1f5abe473b8bc8e585e89fb079c7b5ad95d
```

Choose a new destination directory for reproduction; the script refuses to
overwrite an existing one. This run initially omitted the Lefthook PATH entry;
that failed receipt is retained. The completed command is the unchanged
`lefthook run pre-push --all-files` in the indicated LF checkout with the
full PATH above. The gate writes its log and environment JSON under `raw/`
(local and git-ignored since the 2026-11-01 evidence diet).
own that gate result. Website/tooling fetches happen before the gate.

Windows full-gate attempts are retained as failed environment/oracle
receipts, including a CRLF-sensitive existing mutation that was not applied.
The temporary LF adaptation was withdrawn after its clean guard failed.
`gate.ps1` retains only its Windows interpreter/UTF-8 setup; it is not the
successful whole-gate recipe and skips no checks.

`build.ps1` invokes:

```powershell
cargo test -p qianqian-output-wasapi -p qianqian-headless --lib --no-run --message-format=json
```

It discovers actual compiler-artifact executables. The row runner then invokes
each executable with `--exact <filter> --nocapture --test-threads=1` and child-only
Q296_MODE/Q296_OUTLET/QIANQIAN_AUDIO_LOG variables. Each raw row records the
actual executable SHA256, filter, outlet, mode, UTC, pipe/gate control,
stdout/stderr capture, exit, watchdog and runner result. Blocked stderr is
kept unread rather than copied into the archive; its lock-acquisition witness
travels over healthy stdout.

Each mutation logs its build, exact row signature, baseline target-file hash
and result in `raw/mutations.json` (local, git-ignored). Restoration is
byte-for-byte in finally.
`results.ps1` rejects incomplete receipts and creates the 48-cell ledger plus
a SHA256 manifest of the test/runner/source inputs. Normal production code
restoration is independently visible in the final Git diff.

Pinned FFmpeg source acquisition:

```powershell
Invoke-WebRequest -Uri https://raw.githubusercontent.com/FFmpeg/FFmpeg/bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa/libavutil/log.c -OutFile evidence/diagnostics-tier2-296/ffmpeg-log.c
Get-FileHash evidence/diagnostics-tier2-296/ffmpeg-log.c
```

The source URL/commit and expected SHA256 are in REPORT.md. This source
receipt is not a linked native binary identity or a callback runtime result.

GitHub issue/PR snapshots were obtained through the connected GitHub API.
The `#293` / `#294` merged/head/merge identities and the ancestry exit values
at the base were read during the run and are not retained (2026-11-01
evidence diet). The #290
body refresh and accounting comment do not close the issue.

This run adds no device-loss, audible-output, Miri, Loom, TLA+, ABI parity,
or native FFmpeg runtime claim. Existing accepted evidence retains its
original target, scope and evidence grade.
