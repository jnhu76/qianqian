# TRANSPORT-DOGFOOD — Stage A evidence (Transport Dogfood → Integrated
# Audit → TUI v1 Closure campaign)

Workspace-independent evidence crate for
QIANQIAN-TRANSPORT-DOGFOOD-INTEGRATED-AUDIT-TUI-V1-CLOSURE-1 (Stage A).
It exercises the REAL production reference player — the actual
`qianqian-headless.exe play …` TUI process (SongCore decode + WASAPI
output) — on the real Windows host, through a Windows pseudoconsole
(ConPTY).

Nothing in this directory may be imported by any `qianqian-*` crate.
It pins no production contract; it measures one.

## Layout

```text
CORPUS.md                corpus census (repo fixtures + declared synthetic media)
src/bin/tuidriver.rs     the ConPTY driver (spawn, key injection, VT grid capture,
                         scenario steps, resource checkpoints, JSON verdicts)
src/scenarios.rs         the A1–A20 scenario matrix scripts
tools/run-tui.sh         cross-host runner (staging + detached launch + evidence)
tools/run-machine.sh     the §7 machine-mode (`--machine play`) regression
evidence/                ENV identity files + per-scenario JSON/transcripts/logs
```

## How a run works

1. Cross-build (WSL side): `tuidriver.exe` and
   `qianqian-headless.exe` (`--features playback`, mingw SongCore
   staging via `QIANQIAN_NATIVE_DIR`).
2. `tools/run-tui.sh <run> SCENARIO...` stages both binaries and the
   corpus under `C:\Users\Public\qianqian-dogfood\`, records the run
   identity (git HEAD, binary/corpus SHA256, toolchain, Windows and
   endpoint identity — addendum §5) into
   `evidence/ENV-TUI-RUN<N>.txt`, and launches the driver DETACHED
   (see "ConPTY lessons" below).
3. The driver spawns `qianqian-headless.exe play <files>` under a
   120×40 pseudoconsole, sends scripted VT key events, emulates the
   terminal cell grid, appends every changed full frame to a frame
   history, and evaluates scenario steps against that history plus
   process exit codes and bounded child-resource measurements.
4. Per scenario: `<SCENARIO>.json` (verdict, per-step log, resource
   samples), `<SCENARIO>.txt` (frame-history transcript),
   `<SCENARIO>.raw.txt` (raw VT stream) under
   `C:\Users\Public\qianqian-dogfood\evidence\`, copied back by the
   campaign report.

## ConPTY lessons (harness-relevant, recorded here so they are not re-discovered)

```text
1. STARTUPINFOEXW.lpAttributeList must be assigned the initialized
   attribute list. Without it, CreateProcessW succeeds but the child
   silently gets NO pseudoconsole (it renders on the parent console
   instead; the pipe stays empty).
2. The ConPTY pipe ends may be closed only AFTER CreateProcessW
   (documented-sample order).
3. Under WSL interop the launched process is attached to the interop
   console, and children spawned from there IGNORE the pseudoconsole
   attribute and inherit that console. The driver must therefore be
   started detached (e.g. PowerShell Start-Process with a fresh/hidden
   window). Start-Process stdout/stderr redirection silently reverts to
   the shared-console launch — the driver writes its own summary file
   instead.
4. ratatui's diff renderer never re-emits unchanged cells (spaces
   included), so a plain VT-stream strip cannot reconstruct labels; a
   cell grid with cursor/erase semantics is required, plus a frame
   history for after-mark assertions.
5. A detached Start-Process launched from a Linux-cwd shell starts in
   `C:\Windows\System32`, and the child inherits that working
   directory: every relative O-dialog candidate then honestly fails
   with os error 2. The driver pins the child's working directory to
   the program's own directory (and the runner passes
   -WorkingDirectory).
6. The diff renderer also leaves STALE characters wherever new text
   aligns over old (a `[1C` cursor jump over an unchanged cell keeps
   the previous glyph), so substring oracles over the reconstructed
   grid are unsafe at diff boundaries. The driver therefore jiggles
   the pseudoconsole width (narrower, then back) after each key write:
   ratatui repaints every cell on resize, restoring a coherent grid.
7. The shell renders what the user TYPED, not a canonicalized path:
   O-open feedback and Source lines echo the typed relative path;
   navigation feedback renders the absolute seeded path. Needles must
   match the rendered form, and refusal needles must use stable
   semantic prefixes (`open refused`, `next refused`) rather than
   diagnostic text the shell does not own.
8. The app's stderr (ffmpeg `[mp3float]` skips, wasapi open/abort
   lines) interleaves with the TUI on the pseudoconsole and can
   pollute single rows; frames self-heal on the next repaint, and the
   raw stream keeps the mechanism evidence readable.
```

## Instrumentation (added after run D)

Every scenario writes `{name}.chrono.txt`: a wall-clock chronology of
spawn (cwd + cmdline), each key write (bytes + content), each output
chunk (bytes), wait begin/ok/timeout, and child death. Waits fail fast
with the child's exit code when the process dies mid-wait, and a RED
captures the child's state (alive + threads, or exit code) BEFORE the
cleanup TerminateProcess. This is what distinguishes a dead child from
a silent-but-alive app from a failed key write.

## Oracle classes and their honest scope

```text
label witnesses        the shell's own truth-class-pinned renderings
                       (Terminal: Stopped, Paused: true,
                       Volume: 80/100 (desired), operation feedback lines)
position witnesses     NEW published Position samples (D14.8 Projection)
                       after a mark — a liveness/consumption witness,
                       never an audibility claim
exit-code witnesses    the pinned transport exit-code contract
absence witnesses      "teardown violated" / "warning: disposal" never
                       appearing in the post-mark window
resource witnesses     bounded child thread/working-set checkpoints
                       (explicit measurement, never crash inference)
```

Explicitly NOT claimed mechanically by this harness:

```text
stale-audio ABSENCE after a seek cutover — the D14.5 cutover protocol's
  own oracles plus the recorded human-ear item own that (ear witness:
  UNAVAILABLE, per F5/F6 precedent);
any acoustic loudness / audibility claim;
any semantic truth beyond what the D14.2 read side publishes.
```

## Limitations

```text
- Assertions are timing-bounded (per-step windows) but not
  schedule-exact; a RED is reproducible evidence, a GREEN is a bounded
  run, not a proof.
- The scenario key scripts drive the frozen TUI grammar only.
- Resource bounds (thread delta, working set) are generous tripwires
  against unbounded growth, not exact leak oracles.
- Non-ASCII text is captured in the grid (UTF-8 assembled), but some
  width-2 glyph placements may be off by one column in the transcript.
- The decode-EOF drain window (D14.5 stop⇒Completed mapping) is
  sub-second and not reachable deterministically from the TUI's 1 Hz
  position display; A16-drain-stop therefore witnesses a clean single
  terminal settlement for a LATE stop (either legal terminal), and the
  mapping itself stays with the D11 conformance suite.
- Fixture lengths are load-bearing: scripts must finish inside the
  occupied episode's duration, or natural EOFs falsify later
  expectations (the run-D lesson that reshaped A14/A15/A20).
```

## Run ledger

```text
run A/B/C  harness bring-up (ConPTY attach, grid emulator, panics);
           scenarios individually green as fixed
run D      first full matrix: 9 GREEN / 17 RED — every RED classified
           as harness-side (CWD, needles, fixture timing, oracle
           design) except an unreproduced total-silence anomaly
           (A2/A3/A4/A7/A9/A11; see the Stage-A report §findings)
run E      post-repair rerun of all 17 REDs: green individually;
           exposed lessons 5–7 above
run F      EVIDENCE RUN: full 26-scenario matrix, one detached
           invocation, 26/26 GREEN, driver exit 0 (ENV-TUI-RUNF.txt);
           machine-mode regression 7/7 GREEN (machine-runF2.summary)
```
