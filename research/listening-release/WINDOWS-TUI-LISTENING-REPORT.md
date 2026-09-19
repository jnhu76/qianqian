# WINDOWS-TUI-LISTENING-RELEASE-1 — evidence report

Campaign: `WINDOWS-TUI-LISTENING-RELEASE-1` (Issue #166 remaining
closure; parent roadmap Issue #165).

This file is **executable-evidence and handoff coordination only**. It
is not architecture authority. The engineering gates it records were
produced by the tooling named below; the acoustic section records what
a human actually heard and nothing else.

Status: **PACKAGE_READY_FOR_HUMAN_LISTENING** (engineering pass) —
`ACOUSTIC_WITNESS` was UNAVAILABLE at packaging time; see §7.

---

## 1. Build / package identity

```text
source branch      feat/windows-tui-listening-release-1
source commit      9a3cc5fb1d876e6589468aba206714f978b19669
                   (the commit the packaged binary was built and every
                   physical run below executed; the branch's later
                   commits touch evidence/docs/README routing only —
                   `git diff 9a3cc5f..HEAD -- apps crates tools` is
                   empty, so this artifact IS the product-HEAD build)
base (origin/main) 8b1a739 (PR #168 merge)
package            dist/qianqian-windows-x86_64.zip
package sha256     f5b09d256096d9a11512975fb95bdfd41ce98c885f420b62ee630ade8f2e7f43 (zip) / extracted exe 7b48308240251f385ae5d3702bd42754425c665c8f5f7d054e53491171a18f62
qianqian.exe       sha256 7b48308240251f385ae5d3702bd42754425c665c8f5f7d054e53491171a18f62  (the extracted, executed artifact)
songcore (mingw)   17323291fc7b53997a36746e1ce7f86b8863a63f71be79389b9b196a05175a2a
                   (native/build/artifacts-mingw/libsongcore.a; FFmpeg
                   n9.0.1 bf1b838f LGPL closure statically inside)
rustc              1.97.1 (8bab26f4f 2026-07-14)
target/profile     x86_64-pc-windows-gnu / release / features=playback
host               Microsoft Windows 11 专业版, Realtek High Definition
                   Audio, default render endpoint (shared mode)
reproducibility    packaged twice from one commit: same file set, same
                   manifest semantics, same dependency verdict; the exe
                   is NOT byte-stable across rebuilds (mingw toolchain),
                   so artifact identity is the recorded SHA256
```

Package contents (ZIP content gate GREEN):

```text
qianqian-windows-x86_64/
├── qianqian.exe              (the ONLY binary; no qianqian-headless)
├── QUICKSTART.md
├── LICENSE  LICENSE-MIT  LICENSE-APACHE
├── THIRD_PARTY_NOTICES.md    (FFmpeg LGPL-2.1+ closure + Rust deps)
└── BUILD-MANIFEST.txt        (commit, toolchain, artifact + file SHA256s)
```

## 2. Runtime dependency audit

`objdump -p qianqian.exe` imports (packaging script audits this
fail-closed on every build):

```text
KERNEL32 USER32 USERENV WS2_32 msvcrt ntdll ole32 oleaut32 propsys
rpcrt4 combase bcrypt bcryptprimitives api-ms-win-core-synch-l1-2-0
api-ms-win-core-winrt-error-l1-1-0
```

All Windows system DLLs. SongCore + the trimmed FFmpeg decode closure
are statically linked; no MinGW runtime DLL, no libgcc/libstdc++, no
codec DLLs, no shipped runtime files beyond qianqian.exe. The package
does not call itself single-file AND ship undeclared runtime files —
there are none.

## 3. Isolated-package dogfood (Windows, real host)

Class: **ISOLATED-PACKAGE DOGFOOD** — same physical host as the
development tree (no clean VM available to the agent), but every launch
below used ONLY the extracted ZIP: system-only PATH
(`C:\Windows\System32;C:\Windows`), `cwd=C:\Windows\Temp` for CLI
gates, no repository in the picture, no `QIANQIAN_NATIVE_DIR`.
Runner: `research/listening-release/tools/run-lr1.sh`; environment
record `evidence/ENV-LR1-RUN1.txt`; per-scenario transcripts
`evidence/transcripts-run1/`.

### 3.1 Extraction at three locations

```text
C:\Users\Public\qn-lr1\qianqian-windows-x86_64            (plain)
C:\qn lr1 spaces\qianqian-windows-x86_64                  (spaces)
C:\Users\Public\千千播放器\qianqian-windows-x86_64         (CJK)
```

### 3.2 Non-TUI isolated gates

`--version` and `--help` from EACH location, system-only PATH,
cwd=`C:\Windows\Temp`: all exit 0 (6/6 GREEN, ENV record §
non_tui_isolated_gates).

### 3.3 New-console allocation (double-click equivalent)

`qianqian.exe` launched from the CJK location in a fresh console with
reduced PATH stayed alive on the idle page for 4 s (then terminated by
the harness): no missing-DLL dialog, no instant-exit. The interactive
Explorer double-click itself belongs to the human session (§7).

### 3.4 ConPTY TUI scenarios against the extracted package exe

Driver: `research/transport-dogfood` (gains `--cwd` and
`ExpectExitEither` this campaign). Summary (wall-clock per group is in
the ENV record):

| group   | scenarios                                                              | result |
|---------|------------------------------------------------------------------------|--------|
| core    | U1-idle U1-folder-open U2-shuffle-start U2-help C11-longpath C12-cjk A15 | 7/7 GREEN (12 s) |
| fmatrix | LR1-folder-mixed LR1-all-corrupt LR1-duplicate-roots LR1-truncated-next | 4/4 GREEN (37 s) |
| large   | LR1-large (1,000 entries)                                              | GREEN (4 s) |
| huge    | LR1-huge (5,000 entries)                                               | GREEN (5 s) |

Notes on what each LR1 gate witnessed (oracles are read-side labels,
feedback lines, exit codes — never audibility):

- **LR1-folder-mixed (§37 F1/F3/F5/F7/F8/F9)** — a folder with real
  FLAC/M4A (flat, nested, CJK, >112-char name), five quiet non-audio
  files (cover.jpg/folder.png/lrc/txt/ini), a renamed-garbage "track",
  a zero-byte .mp3, and an access-denied subfolder: `scanning …` line,
  `5 candidates, 5 skipped, 2 unplayable`, bounded
  `not playable: broken.flac` / `not playable: zero.mp3` detail, a
  `scan warning: cannot read …` for the denied subfolder, playback +
  selection working on the survivors.
- **LR1-all-corrupt (§17)** — every audio-looking file probe-rejected:
  honest `open refused: no playable audio files found; 2 unplayable`,
  the shell stayed up, argv-driven exit code 1, no panic.
- **LR1-duplicate-roots (§18)** — the same folder named twice on argv:
  `2 candidates, 2 duplicates removed`, first-occurrence order,
  navigation normal.
- **LR1-truncated-next (§17/F4)** — a FLAC cut at ~40 %: the scan
  ACCEPTED the container (its header is valid — exactly the
  probe-passes-but-damaged-later class §17 names), navigation reached
  it, and the episode committed the truthful
  `Terminal: Failed` with `playback failed: decode: decode error`,
  exit code 1, NO auto-skip, no panic, no teardown violation. This is
  the runtime-failed contract exercised end-to-end on the real
  package: the user sees the failure and chooses what is next.
- **LR1-large / LR1-huge (§19)** — 1,000 / 5,000-entry folders:
  the whole scenario (scan + open + browsing + quit) finished in 4 s /
  5 s wall-clock (ENV record) — bounded practical time by a wide
  margin, no freeze, viewport windows around the selection, Up/Down
  responsive. Observation, not an SLA. (The 5,000 entries are NTFS
  hard links over ten seed files — the per-file 1,023-link cap —
  byte-identical content, path-sorted order.)

The `A15` scenario now pins the NEW scan contract: an explicitly named
invalid file is probe-rejected before the playlist seeds (`not
playable: garbage.bin`), and navigation walks the surviving entries
with inert boundaries.

## 4. Real-file hardening shipped (code)

- scan-time media-probe validation of every candidate through the
  existing decode provider seam (no parallel format detector);
  probe-refused candidates never enter the playlist
- exact-duplicate accepted-path dedup (first occurrence; lexical rule
  recorded: case-folded + slash-folded on Windows, exact on Unix — no
  content identity, no inode authority)
- bounded, three-class scan reporting: quiet skips / counted
  unplayables with capped name detail / capped filesystem warnings
- one `scanning <path> …` line before the synchronous argv scan
- user-facing decode refusal wording ("cannot open this file …",
  "cannot decode this file …") with the decoder status preserved as
  root cause
- runtime `Failed` tracks still never auto-skip (policy unchanged)

## 5. Help surfaces consistency

QUICKSTART.md ↔ `--help` ↔ TUI `?` overlay pinned by tests
(`apps/headless/tests/quickstart_usage.rs`,
`tui::view::tests::the_help_overlay_advertises_every_quickstart_key`):
same key set, same order/repeat/open/seek/quit semantics.

## 6. Standard engineering gates (local, final product HEAD)

```text
cargo fmt --all --check                         PASS
cargo check --workspace                         PASS
cargo test --workspace                          PASS (46 suites green)
cargo clippy --workspace --all-targets
  --all-features -- -D warnings                 PASS
qianqian-decode-songcore crate tests            PASS (18/18)
python3 tools/check_architecture_vocabulary.py  PASS (negative control verified)
python3 tools/check_plugin_boundaries.py        PASS
python3 tools/check_plugin_boundaries.py
  --negative-controls                           PASS
git diff --check                                PASS
commit-convention (8b1a739..HEAD)               PASS
HOSTED_CI                                       UNAVAILABLE for this local run (branch not yet pushed during the gates); the repository workflows (architecture-vocabulary, plugin-boundary-gate, verification-rust-gate, windows-compile-gate, commit-convention, docs) run hosted on PR — to be confirmed on the PR, never locally claimed PASS
```

## 7. Acoustic status — ACOUSTIC_WITNESS = UNAVAILABLE (at packaging)

The packaged build has NOT been heard by a human yet. No audibility is
claimed anywhere in this report; every automated oracle above is a
label/position/exit-code witness. Per campaign §52 the correct current
verdict is:

```text
ENGINEERING_GATES_PASS
PACKAGE_READY_FOR_HUMAN_LISTENING
```

`WINDOWS_TUI_LISTENING_RELEASE_PASS` requires the human listening
session (§8 checklist) to be performed and reported.

## 8. Human listening handoff

```text
PACKAGE:
dist/qianqian-windows-x86_64.zip

SHA256:
f5b09d256096d9a11512975fb95bdfd41ce98c885f420b62ee630ade8f2e7f43 (zip) / extracted exe 7b48308240251f385ae5d3702bd42754425c665c8f5f7d054e53491171a18f62

EXTRACT AND RUN:
qianqian.exe play --shuffle "D:\Music"
```

Checklist (each line needs a human YES):

```text
[ ] I can hear music
[ ] the audible song matches the TUI
[ ] auto-next audibly changes track
[ ] N/P work
[ ] pause becomes silent
[ ] resume continues
[ ] seek changes audible position
[ ] volume changes audibly
[ ] shuffle/repeat behave correctly
[ ] no obvious pop/corruption/dropout blocker
[ ] I listened for at least 30 minutes
```

## 9. Architecture delta

```text
PLUGIN DELTA            none (no new ComponentSpec; input preparation
                        remains an App function per D13)
PLAYBACK AUTHORITY      none (D11/D14 untouched; playlist policy still
                        App-owned; Playback Session playlist-blind)
PCM/BACKEND DELTA       none
scan/dedup/reporting    App-layer host input preparation only
```
