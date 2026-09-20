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
source commit      4498dec… (fix(tui) R3) + VOLUME-TAPER-1 (R4, §11.6)
                   + c7efdbe (fix(tui) R5 switch transient, §11.8)
base (origin/main) 8b1a739 (PR #168 merge)
package            dist/qianqian-windows-x86_64.zip
package sha256     ebb8d9d5c3260ee8a29f881094185be26751a997c60958f7ea1ad994bdbb5680 (zip, R5 build)
                   (superseded: 3f39c225… [R4 taper], d28c0084… [R3 keyboard fixes], 64f64cbc… [Tier-2 build], 95be430b… [U2 probesize build], 2abe029c… [RUN2])
qianqian.exe       sha256 218b995d2ed5f25fe5ebdce9c527566086d3f40f5b405a5e28c98054c0dddc72 (the extracted, executed artifact; built with --remap-path-prefix — no build-host paths inside)
                   (superseded: 2d479f55… [R4], 4122a04e… [R3], add21897… [Tier-2], 9909ce8d… [RUN2])
songcore (mingw)   05cbb12aaf33e44e5f0784deca0b410a0948f625d142bb4f7192fb61040fe7cc
                   (native/build/artifacts-mingw/libsongcore.a; FFmpeg
                   n9.0.1 bf1b838f LGPL closure statically inside;
                   RUN2's archive was 17323291…)
rustc              1.97.1 (8bab26f4f 2026-07-14)
target/profile     x86_64-pc-windows-gnu / release / features=playback
host               Microsoft Windows 11 专业版, Realtek High Definition
                   Audio, default render endpoint (shared mode)
                   (NOTE: at the RUN3 attempt the host had NO active
                   render endpoint — see §11 / the RUN3 blocked note)
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
record `evidence/ENV-LR1-RUN2.txt`; per-scenario transcripts
`evidence/transcripts-run2/`. (RUN1 in the same tree is the earlier
run of the SAME scenario set against the pre-review-fix artifact; the
three review minors were fixed in between, so RUN2 is the evidence of
the delivered package.)

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

## 6. Fresh adversarial engineering review (§49)

Independent fresh-context reviewer, 24-point gate: **0 Critical /
0 Required / 0 Major / 3 Minor**, 24/24 PASS. All three minors fixed
on this branch and the physical evidence re-run (see
`ADVERSARIAL-REVIEW-1.md` for the full verdict table, the fixes, and
the recorded observations): non-Unicode argv no longer panics the
product startup; the shipped exe carries no build-host paths
(remap + fail-closed gate); the packaged manifest cites a commit
reachable from the delivered history.

## 7. Standard engineering gates (local, final product HEAD)

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

## 8. Acoustic status — ACOUSTIC_WITNESS = UNAVAILABLE (at packaging)

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

## 9. Human listening handoff

```text
PACKAGE:
dist/qianqian-windows-x86_64.zip

SHA256:
ebb8d9d5c3260ee8a29f881094185be26751a997c60958f7ea1ad994bdbb5680 (zip) / extracted exe 218b995d2ed5f25fe5ebdce9c527566086d3f40f5b405a5e28c98054c0dddc72

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

## 10. Architecture delta

```text
PLUGIN DELTA            none (no new ComponentSpec; input preparation
                        remains an App function per D13)
PLAYBACK AUTHORITY      none (D11/D14 untouched; playlist policy still
                        App-owned; Playback Session playlist-blind)
PCM/BACKEND DELTA       none
scan/dedup/reporting    App-layer host input preparation only
```

## 11. Field reports and corrective (U2) — post-RUN2 human testing

The user performed real listening on the RUN2 package against a real
100-file MP3 corpus (embedded cover art on the tracks) and reported
two defects:

```text
R1  TUI corruption: frames of "[mp3 @ ...] Could not find codec
    parameters for stream 1 (Video: mjpeg, none)" interleaved with the
    full-screen UI (200+ such lines during a folder scan).
R2  Delay: folder open took seconds before the first sound; each
    track switch left roughly 2 seconds of silence, with the timeline
    only moving once audio arrived.
```

Root cause (one mechanism, two symptoms): every `song_open` ran
`avformat_find_stream_info` with FFmpeg's DEFAULT 5 MB probesize, and
the cover-art attached-picture stream carries codec parameters that no
decoder in this trimmed LGPL build can resolve — so the parameter hunt
ALWAYS burned the full 5 MB budget and logged its failure to stderr.
Amplification: the folder scan probes every candidate (100 × 5 MB ≈
500 MB of avoidable I/O ≈ the multi-second startup), and every track
switch opens the candidate TWICE by design (the frozen D14.6
probe-before-destruction sequence: pre-probe + activation open;
2 × 5 MB ≈ the 2 s switch gap). Position truth was NEVER wrong — the
render leg publishes handed-off-minus-padding only (D14.8); the
"timeline moved while silent" reading was wall-clock silence during
the opens.

Corrective (commit 9995f6b; product-only delta over the RUN2 build):

```text
SongCore      AVFormatContext.probesize capped at 1 MiB (audio
              parameters resolve from the first frames; only the
              unresolvable-picture hunt is bounded); FFmpeg's default
              logger replaced by a discarding callback by default
              (QIANQIAN_FFMPEG_LOG=1 restores it).
WASAPI crate  the per-episode mechanism diagnostics ([qianqian-wasapi]
              opened / render aborted / volume note) are silent by
              default (QIANQIAN_AUDIO_LOG=1 restores them); the
              abort-path data-plane stop is unchanged. Any stderr
              write corrupts a full-screen UI; failures still surface
              through the session's typed activation/terminal
              evidence.
Tests         committed synthetic MP3 with an embedded mjpeg cover
              (apps/headless/tests/fixtures/mp3-cbr-cover.mp3) stays
              playable through the scan (input::real_probe); new
              physical gate U2-cover-clean pins the clean console for
              a full cover-art episode in the LR1 matrix.
```

Measured on the user's real corpus (Linux dev build, same C code):
108 budget-exhaustion warnings at probesize (5000000) before the fix
→ 0 warnings and probesize (1048576) under QIANQIAN_FFMPEG_LOG=1
after; default-run stderr clean.

### 11.1 Second round: the 48 kHz refusal (field-earned Tier-2 SRC)

With an output device reconnected, the user's next real corpus
(`D:\文件\音频`, mixed 44.1/48 kHz) surfaced one more defect: track 1
(44.1 kHz) played, `N` to a 48 kHz track failed the episode with
`stream initialize failed: 0x88890008 (device refused the float32
source format)` — the shared-mode mixer refuses a source rate that
differs from the mix format, and the backend was Tier-1-only (the
previously deferred OPEN mechanism choice). The screen corruption in
the same report is the OLD artifact (the `[qianqian-wasapi]` and
`[mp3float @ …]` lines visible in the screenshots are silent-by-
default since 9995f6b — the still-running extraction predates it).

Corrective: Tier-2 engine-SRC fallback (commit 6661d33; PBK-003 §8
amendment note under PR review) — Tier 1 first (bit-perfect direct
submission); on exactly `AUDCLNT_E_UNSUPPORTED_FORMAT`, retry with
`AUTOCONVERTPCM | SRC_DEFAULT_QUALITY` so the engine's mix thread does
the conversion while the data plane still carries float32 at the
source rate (edge, D14.8 accounting and the realtime firewall
unchanged). Formats both tiers refuse still fail honestly. New
physical gate `U2-rate-mix`: a 44.1→48 kHz playlist where BOTH tracks
must render with honest source-format lines.

### 11.2 RUN4 — the physical evidence of record (all GREEN)

With the endpoint reconnected, the full matrix ran end to end against
the Tier-2 package (zip 64f64cbc…):

```text
core    9 GREEN  (U1-idle, U1-folder-open, U2-shuffle-start,
                  U2-cover-clean, U2-rate-mix, U2-help,
                  C11-longpath, C12-cjk, A15)
fmatrix 4 GREEN  (folder-mixed, all-corrupt, duplicate-roots,
                  truncated-next)
large   1 GREEN  (1000-file list)
huge    1 GREEN  (5000-entry list)
scenario_groups_green: 4/4
```

U2-cover-clean (the R1 gate) and U2-rate-mix (the R2 gate) are both
part of the pass. Evidence: `evidence/logs/lr1-run4-*.summary`,
`evidence/transcripts-run4/`, `evidence/ENV-LR1-RUN4.txt`.

### 11.3 X-realdir — the field operation witnessed on the real corpus

The ad-hoc `X-realdir` scenario replays the user's exact operation
(idle launch → O dialog → typed `D:\文件\音频` → Enter, then N) against
their real mixed-rate corpus, on the Tier-2 package:

```text
verdict            GREEN
operation          O dialog typed + Enter at T+0.599s
first track        Track: 1/6 rendered at T+0.654s (55 ms after
                   Enter; scan 6 candidates + probe + open + device
                   start) — the R-delay class the user reported as
                   "seconds" is sub-100 ms here (warm cache; the
                   mechanism bound is the probesize cap, 5× less I/O)
track switch       N at T+1.680s → Track: 2/6 (48 kHz source,
                   Tier-2 engine SRC) confirmed at T+1.781s —
                   ~100 ms wall-clock, versus the ~2 s silence the
                   user reported on the RUN2 build
console            zero FFmpeg/WASAPI mechanism lines post-mark
exit               clean quit, code 0
```

These are mechanism/latency observations, not audibility claims; the
ACOUSTIC_WITNESS checklist (§9) remains the human's alone.

### 11.4 Third round: Enter restarts the track; the keyboard "dies" (R3)

Playing the real corpus, the user reported (with a full-screen
screenshot, Order/Repeat/Track all still at their startup values):

1. **Enter mid-play restarted the track from its head.** In the common
   state the selected row IS the committed episode's row (selection
   follows every commit), and `Enter` ran the whole frozen Open
   replacement for it — a restart by construction. There was no
   same-track inert rule in the grammar.

2. **Then every key "stopped working"** — N/P, R, L, +/-, O, ?, S, Q,
   G all dead while playback and the position line stayed visibly
   alive, and G "had nowhere to type into". Root cause (proven on a
   ratatui `TestBackend` probe, not inferred): the now-playing panel
   was pinned at `Min(10)` and the vertical layout handed all leftover
   rows to the playlist pane, so at the field's ~30-row console a live
   episode filled the panel EXACTLY (blank + Source + Format +
   Position + bar + Terminal + Track = 8 content rows). The status
   block (2 rows) and the ACTIVE Open/GoTo input line (1 row) rendered
   below the fold — invisible. Pressing G activated a modal the user
   could not see; the modal precedence (Issue #166 §28) then swallowed
   every later keypress into the invisible line. The keyboard was not
   dead — it was typing into a black hole. The earlier physical runs
   could not catch this: the harness console is 120x40, where both
   lines were visible (probe: goto visible from ~34 rows up).

   The frozen-loop alternative was ruled out by the same screenshot:
   mid-replacement the App's `active` is already taken, so any hang
   inside the Open replacement renders the IDLE panel ("No music
   loaded.") — the screenshot shows a fully populated live episode
   instead.

Corrective (commit 4498dec, App-layer only):

- **Enter inert rule**: one App truth predicate — selected cursor ==
  committed cursor AND the episode's D11 terminal not yet committed —
  and the shell refuses to re-invoke the replacement, saying so in the
  status line ("already playing the selected track"). A settled
  episode (Completed/Stopped/Failed) still replays; Enter on a
  DIFFERENT browsed row still plays it (unchanged, U2-enter).
- **Content-driven panel budget**: the now-playing layout constraint
  is derived from the same line builder the panel renders (one
  builder, two readers), so the operation feedback and the active
  input lines always fit; the playlist absorbs the difference by
  clipping rows — its documented degradation. A hidden modal was a
  keyboard black hole; a scrolled playlist is still a playlist.

New physical gate `U3-keys-field`, run at the FIELD terminal size
120x30 through a new per-scenario pseudoconsole size (harness default
unchanged): Enter on the live row is inert and VISIBLE, G opens a
VISIBLE line at 30 rows, the typed seek routes, L labels, S stops.
Plus unit pins: the runtime Enter-inert/replay tests and a 120x30
visibility test over the full live panel.

### 11.5 RUN5 — the physical evidence of record (all GREEN)

Against the R3 package (zip d28c0084…, exe 4122a04e…, HEAD 4498dec):

```text
core   10 GREEN  (U1-idle, U1-folder-open, U2-shuffle-start,
                  U2-cover-clean, U2-rate-mix, U2-help,
                  U3-keys-field [NEW, at 120x30], C11-longpath,
                  C12-cjk, A15)
fmatrix 4 GREEN  (folder-mixed, all-corrupt, duplicate-roots,
                  truncated-next)
large   1 GREEN  (1000-file list)
huge    1 GREEN  (5000-entry list)
scenario_groups_green: 4/4
```

U3-keys-field witness: Enter mid-play → "already playing the selected
track" rendered, no `play: opened` re-open, position kept advancing;
G → the `Go to: [` line rendered at 120x30; typed `0:10` → seek
requested, new position sample; L → "Repeat: All"; S → "Terminal:
Stopped"; clean exit 0. X-realdir re-run on the same package: GREEN
(first track ~240 ms after the O-dialog Enter; N to track 2 ~120 ms;
console clean; clean exit). Evidence: `evidence/logs/lr1-run5-*.summary`,
`evidence/transcripts-run5/` (incl. X-realdir), `evidence/ENV-LR1-RUN5.txt`.

### 11.6 Fourth round: the volume control did not feel linear (R4)

Everything from R3 held in real listening; one report remained: the
+/- volume control "feels non-linear". The plumbing was honest — the
TUI's ±5 steps land in the App's desired level, the read side shows
exactly that — but the session's FACTOR realization was the literal
division `level/100.0`: linear amplitude. Human loudness perception
is roughly logarithmic in amplitude, so the linear control put all of
its audible travel into the bottom quarter: presses above ~70 were
inaudible, and the same 5-step press meant −0.45 dB at 95 but −6 dB
at 25. (The D14.9 text never promised a linear curve — "no dB curve
promise; 50 makes no half-perceived-loudness claim" — the linear
parenthetical was the recorded realization, and the field evidence is
that it does not feel like a volume control.)

Corrective (commit on this branch; PBK-002 D14.9 dated amendment
VOLUME-TAPER-1): the session-owned routing seam now realizes the
desired level as a perceptual taper —

```text
100 → exactly 1.0   (unity preserved; Tier-1 bit-transparent
                     submission untouched at the top of the control)
  0 → exactly 0.0   (true silence)
else → 10^(−0.03·(100−level))  (−60 dB control range;
                     exactly 3 dB per 5-step press)
```

so 50 is −30 dB and every press feels alike near the top and near the
bottom. NO D14.9 proposition changed: owner, truth class (desired
control position, never an acoustic level or readback), apply points,
zero/mute ruling and terminal inertness are untouched; the volume
labels, the ±5 step and the 0–100 range are unchanged. Pinned
executably (`handle::desired_level_tests`: exact endpoints, 3 dB per
5-step property, monotonicity; the volume-seam conformance suite
re-anchored). Whether the control now FEELS even is the human ear's
verdict (ACOUSTIC_WITNESS) — the mechanism claim is only the mapping.

### 11.7 RUN6 — the physical evidence of record (all GREEN)

Against the R4 package (zip 3f39c225…, exe 2d479f55…):

```text
core   10 GREEN  (…incl. U3-keys-field at 120x30)
fmatrix 4 GREEN / large 1 GREEN / huge 1 GREEN — groups 4/4
X-realdir on the real corpus: GREEN (first track ~0.24 s after the
O-dialog Enter, N to track 2 ~0.11 s, console clean, clean exit)
```

The volume taper is not ConPTY-witnessable (amplitude is not on
screen; audibility is never claimed by this harness) — its executable
evidence is the unit pin above, and its acceptance item is the human
listening checklist (§9). Evidence: `evidence/logs/lr1-run6-*.summary`,
`evidence/transcripts-run6/` (incl. X-realdir), `evidence/ENV-LR1-RUN6.txt`.

### 11.8 Fifth round: a blank playlist row flashed on every switch (R5)

With the R3/R4 fixes holding, the user observed (with a screenshot):
pressing `N` made the playlist pane gain a blank row after the last
track and the whole frame "stretch" — and correctly diagnosed the
cause themselves: the now-playing panel's timeline disappears during
the switch. Mechanism: the progress bar and the timeline rendered
only when BOTH evidence sides existed, and a freshly committed
episode has no position sample yet (~1 Hz publication), so for the
first sampling window after every switch the panel lost its bar row;
the content-driven layout budget (§11.4 fix) shrank with it and the
freed row went to the playlist pane.

Corrective (commit c7efdbe), implementing the field's own suggestion
("like other players — show the timeline as 0 while switching"): a
LIVE episode whose position evidence has not arrived yet renders the
timeline at its start (`00:00 / <total>`) with the bar present and
empty; without a known total the bar renders against `--:--`. A
SETTLED episode without position evidence keeps the honest dashes
and no bar — a dead timeline has no start, and the no-evidence state
grows no fabricated zero (negative control pinned). The scriptable
transport's raw `--:--` evidence shapes are untouched: the policy
lives in the TUI model only. The panel row count is now invariant
across the transient (pinned: `the_panel_row_count_is_stable_across_
the_switch_transient`), and dogfood tripwires assert no post-switch
`--:--` frame renders (U2-rate-mix, X-realdir).

### 11.9 RUN7 — the physical evidence of record (all GREEN)

Against the R5 package (zip ebb8d9d5…, exe 218b995d…, HEAD c7efdbe):

```text
core   10 GREEN  (…incl. U3-keys-field at 120x30 and the new
                  switch-transient tripwire in U2-rate-mix)
fmatrix 4 GREEN / large 1 GREEN / huge 1 GREEN — groups 4/4
X-realdir on the real corpus: GREEN (clean switch, no collapsed
timeline frame, console clean, clean exit)
```

Evidence: `evidence/logs/lr1-run7-*.summary`,
`evidence/transcripts-run7/` (incl. X-realdir), `evidence/ENV-LR1-RUN7.txt`.

