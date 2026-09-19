# U2 physical evidence — temporary playlist / order / repeat / EOF / TUI controls

Campaign: `WINDOWS-TUI-USABILITY-CLOSURE-1`, Issue #166, stage **U2**
(PR: `feat/166-playlist-usability-closure`).

This is the executable-evidence record for the U2 slice, produced on the
REAL Windows host through the ConPTY driver in this crate. It is
**evidence, not authority**: the semantics the scenarios check are the
2026-09-19 U2 amendment in `docs/adr/ADR-PBK-002.md` §20 D14.6, and this
document neither restates nor extends them.

## 1. Build identity (per run, recorded in `evidence/ENV-TUI-RUN<N>.txt`)

```text
head SHA            5a2246cdd759361f76f86b6e12a11acf6bea3a3c
                    (feat/166-playlist-usability-closure)
binary              cargo build --release --target x86_64-pc-windows-gnu
                    --features playback -p qianqian-headless
                    QIANQIAN_NATIVE_DIR=/tmp/qn-u2-stage/native
qianqian-headless    f2354f3d48b532dc161036de8bd5190785ad3c3fc4dbcf5d4c42a9b4a9de573c
                    (identical for runs 6–10)
tuidriver            be35615fdc2f0f5acbf1f14665f1bd08fe9475acc6dee4d32d2fb54bf52d9ebd
songcore (mingw)    17323291fc7b53997a36746e1ce7f86b8863a63f71be79389b9b196a05175a2a
                    (same artifact as the F6/U1 gates)
host                Microsoft Windows 11, Realtek High Definition Audio,
                    default render endpoint, shared mode, event-driven
                    (each transcript's wasapi stderr line)
corpus              SHA256 per file in every ENV file; the U2 additions
                    (vtest01..24, u2soak/soak01..20) are declared
                    SYNTHETIC in CORPUS.md
```

`ACOUSTIC_WITNESS = UNAVAILABLE`: no scenario claims audibility. Every
oracle below is a read-side projection, an operation-feedback line, a
pane row, a published Position sample, an exit code or a resource bound.

## 1a. Canonical product binary identity (this corrective)

Runs 6–10 above exercised `qianqian-headless.exe`. The frozen U2
campaign requires the canonical product binary, so this evidence
corrective (PR #168) additionally built and executed `qianqian.exe`:

```text
source tree         73f4451 worktree (product tree identical to the
                    frozen 5a2246c; worktree clean before the build;
                    5a2246c→73f4451 touches evidence/report files only)
build command       QIANQIAN_NATIVE_DIR=<mingw staging> cargo build
                    --release --target x86_64-pc-windows-gnu
                    --features playback -p qianqian-headless
                    (the package's two [[bin]] targets — qianqian and
                    qianqian-headless — build in this one invocation)
qianqian.exe        97af808b6d165de92381cc14f22450a08422e2639e84fb21e3f9e0df8a709f80
                    (RUN11, RUN12 and the canonical CLI checks below
                    all executed this exact artifact; ENV-TUI-RUN11.txt
                    records the same hash at run time)
qianqian-headless   3e57d2b3959aec280af72bd59c5497f032d88ba2701c0681ce77a5ec3b6d1bfc
(built alongside;  NOT the f2354f3d artifact of runs 6–10)
toolchain           rustc 1.97.1, x86_64-pc-windows-gnu, release,
                    playback features, the same SongCore mingw artifact
                    (17323291…, the F6/U1/U2 staging)
```

The two binaries are the two `[[bin]]` wrappers of one package and
share `entry::run` (they differ only in the name they report), but the
canonical gate below does not rely on that equivalence: it executes
`qianqian.exe` itself. Note on identity: rebuilds of this toolchain
are not byte-stable (a content-identical rebuild yields different
bytes), so binary identity is carried by the recorded SHA256 of the
executed artifact, not by bit-reproducibility.

## 2. Runs

```text
RUN6  U2 matrix            17/17 GREEN   evidence/logs/tui-run6.summary
RUN7  TUI regression       10/10 GREEN   evidence/logs/tui-run7.summary
RUN8  TUI regression        8/8  GREEN   evidence/logs/tui-run8.summary
RUN9  machine regression    7/7  GREEN   evidence/logs/machine-run9.summary
RUN10 light soak            see §4       evidence/logs/tui-run10.summary
```

All five runs above executed `qianqian-headless.exe`
(f2354f3d…): they are shared-implementation evidence, and are NOT
canonical-product-binary runs.

### Canonical product gate (this corrective; `qianqian.exe`)

```text
RUN11         U2 matrix           17/17 GREEN   evidence/logs/tui-run11.summary
CANONICAL-CLI --version/--help/   all GREEN     evidence/logs/canonical-cli.txt
              play --shuffle
              <real folder>
RUN12         machine regression   7/7  GREEN   evidence/logs/machine-run12.summary
```

- RUN11 re-runs the FULL U2 matrix of §3 (same 17 scenarios, same
  oracles, same repaint/mark/liveness discipline — nothing weakened)
  against `qianqian.exe` (97af808b…); the run identity is
  `evidence/ENV-TUI-RUN11.txt` (same driver be35615f…, same SongCore
  17323291…, same corpus hashes as runs 6–10).
- The canonical CLI checks exercise the non-TUI surfaces of the
  product executable: `--version` reports `qianqian 0.1.0` (the
  canonical name), `--help` prints the shipped usage (including
  `play --shuffle` and the folder-expansion text), and a detached
  `qianqian.exe play --shuffle <real folder>` launch on `u1music`
  expands the folder (`opened …\u1music\flac4.flac (2 candidates)`),
  starts in `Order: Shuffle`, advances in-folder on natural EOF
  (`auto-next: opened …\u1music\synth45.mp3`, Track 2/2) and opens the
  real render endpoint twice (two `[qianqian-wasapi] opened:` stderr
  lines) before the harness force-terminates it at the bounded window.
  Those are mechanism/consumption witnesses; no audibility is claimed.
- RUN12 re-runs the §7 machine regression unchanged against
  `qianqian.exe`; `--machine play` stayed functional and the transport
  contract held (grammar, truthful status projection, outcome lines,
  exit codes). Per-scenario artifacts are `evidence/logs/m12-M*.{out,err}`
  (renamed from the harness's `m-M*` names so the committed RUN9
  artifacts stay RUN9's; the RUN12 stdout projections are byte-identical
  to RUN9's because the machine transport carries no binary name).
- Harness delta of this corrective (product code untouched): the two
  staging wrappers gained a binary selector
  (`QIANQIAN_TUI_BIN`/`QIANQIAN_MACHINE_BIN`, default
  `qianqian-headless.exe` — semantics unchanged when unset) because the
  ConPTY driver is binary-agnostic (`--exe`) but the wrappers named the
  executable; the canonical matrix and machine runs above used this
  final harness.

Runs 7/8 are the U1 + Stage-C scenario set (idle launch, folder open,
playback/seek/pause/volume, navigation, resize, long path, CJK, Ctrl+C,
drain-stop) re-run against the U2 binary — the U2 slice changes the
controls/help text and the pane layout, so the older scenarios are part
of this slice's regression surface. One scenario needed an honest
update: `C11-longpath` asserted the pre-U2 controls line (`N  Next`) and
now asserts the frozen U2 keymap (`N/P  Next/Prev`).

## 3. What the U2 matrix witnesses (scenario → property)

```text
U2-pane            the pane renders the startup list with file-name
                   labels and the committed row's `▶ >` markers; the
                   cursor is entry 1/3 (the startup discipline is
                   untouched)
U2-select          browsing with ↓/↑ never changes playback: the
                   committed row (`▶     1  flac4.flac`), the Source
                   line and Track 1/3 are unchanged while the browsed
                   row carries `  >   2  mp3cbr.mp3` — the two markers
                   are independent on a real screen
U2-enter           Enter plays the SELECTED row through the same Open
                   replacement (`play: opened <abs>`, Track 2/3,
                   committed row moves)
U2-nav             N/P walk the traversal; at the last row under Repeat
                   Off a further N opens nothing (absence of a
                   wrap-shaped `next: opened` line over repainted
                   frames) and P walks back
U2-order           R toggles to `Order: Shuffle` without moving the
                   committed entry, never says "Random", and toggling
                   back restores Sequential
U2-repeat-labels   L cycles Off → All → One → Off with the traversal
                   unchanged
U2-eof-advance     a completed entry advances exactly one position
                   through the same Open replacement (`auto-next:
                   opened <abs>`, Track 2/3)
U2-eof-stays       the LAST entry's completion stays put: over a
                   repaint-forced 6 s window no transition line appears
                   and the cursor/source do not move (Repeat Off)
U2-eof-all-wrap    Repeat All wraps at the end: entry 2 opens, then
                   entry 1 opens again, cursor back at 1/2
U2-eof-one-replays Repeat One re-opens the same entry on natural EOF
                   (Track stays 1/2) and manual N still navigates to
                   2/2 — Repeat One never traps the user
U2-stop-no-advance S settles the episode `Stopped` and NOTHING follows:
                   over repainted frames no auto-next line appears and
                   the cursor stays at 1/2 (a terminal-driven advance
                   would have been caught here)
U2-seek-30         Shift+Right moves the published Position past 00:30
                   and Shift+Left brings it back — the ±30 s steps of
                   the SAME seek command (liveness, not audibility)
U2-goto            G parses one typed time with the shared reader and
                   the published Position lands at 00:20; an unreadable
                   token sends nothing and keeps the line open; Esc
                   cancels with playback continuing
U2-help            the overlay lists the shipped keymap (including the
                   ±30 s seek, the order/repeat keys and the exact-seek
                   key), advertises no M3U/mouse/library, and closes
U2-shuffle-start   `qianqian play --shuffle …` starts in Shuffle order
                   with the FIRST accepted candidate still committed
U2-cjk             a CJK file name renders in the pane and the
                   committed cursor walks to the second entry
U2-viewport        a 24-entry list scrolls: the selection stays visible,
                   the committed row (and its marker) leaves the
                   viewport, browsing is inert, and Enter commits the
                   browsed row (Track 23/24)
```

## 4. Light soak (RUN10)

`U2-soak`: 20 synthetic 100 s tracks under `u2soak\`, played END TO END
with Repeat Off through the natural-EOF policy — ~33 minutes of real
playback on the real endpoint, with, at every one of the 19 automatic
transitions: the transition's own feedback line naming the source it
opened, the cursor advance, a NEW published Position sample, and a
bounded child-resource checkpoint (thread delta ≤ 6, working set ≤ 400
MB). The run ends at the traversal end (no wrap, no cascade) and quits
cleanly with the completed-episode exit contract.

Measured across the 20 checkpoints (retained in
`evidence/logs/u2-soak.verdict.json`): threads 5–8 (baseline 6, bound
6 + 6), handles constant at 181, working set 13.1 MB → 14.0 MB over the
whole run — 20 replacement cycles with no unbounded growth. These are
tripwires against unbounded growth, not a leak oracle.

RUN10 was and stays a `qianqian-headless.exe` run; it is NOT relabelled
as a canonical soak. The corrective did not repeat it, on this evidence
argument: the product code under test in RUN10 was the 5a2246c tree,
the current final PR product tree is that same product code (the
post-5a2246c commits touch evidence/report files only), and the
canonical and headless wrappers enter the same shared `entry`
implementation — so the soak's subject (the shared playback
implementation) is unchanged by the binary swap. RUN11 additionally
exercises the canonical wrapper itself across all 17 U2 scenarios.

## 5. Oracle hygiene (what these scenarios deliberately do NOT do)

```text
no fixed sleep used as a correctness oracle: every wait is a bounded
    observable (a rendered line, an exit code, a resource sample)
no process-alive claim: liveness is a NEW published Position sample
no stale-cell claim: the harness repaints (width jiggle) before every
    absence witness, and the absence needles name only what the
    forbidden behavior itself would print — never a bare file name that
    the Source line already carries
no duplicate-entry claim is inferred from a shuffle: per-cycle
    uniqueness is proven structurally (unit/property tests), the
    physical run only walks the permutation the pane shows
no repeated auto-next claim: exactly-once is pinned in the unit matrix
    (100 refreshes over one Completed Fact with an event-count oracle);
    the physical run witnesses the single observable transition and the
    absence of a second one
```

## 6. Raw transcripts

The per-scenario frame-history transcripts, JSON verdicts and
chronologies are large (the soak's is ~95 MB) and stay on the host that
produced them: `C:\Users\Public\qianqian-dogfood\evidence\<SCENARIO>.{txt,json,chrono.txt}`.
What is committed here is the run identity (`ENV-TUI-RUN<N>.txt`), the
per-run step-by-step summary (`logs/tui-run<N>.summary`,
`logs/machine-run9.summary`) and this report.

## 7. Limitations

```text
- Every claim is bounded by this harness's windows and by the corpus;
  a GREEN is a bounded run, not a proof.
- Audibility is not claimed (no ear witness was recorded).
- The scenarios drive the frozen key grammar only; they say nothing
  about mouse or about keys the shell does not ship.
- The soak exercises one process for ~33 minutes; it is a light soak,
  not a leak oracle (the resource rows are tripwires).
- Hosted CI evidence is unavailable: the repository's hosted jobs do
  not start a runner at present, so no GitHub Actions result is claimed
  for this corrective. Local gates were green before this slice; the
  physical canonical Windows gate is this corrective's RUN11/RUN12/
  CANONICAL-CLI record.
```
