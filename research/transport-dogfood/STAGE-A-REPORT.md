# QIANQIAN-DOGFOOD-STAGE-A-REPORT

Campaign: QIANQIAN-TRANSPORT-DOGFOOD-INTEGRATED-AUDIT-TUI-V1-CLOSURE-1
Stage A — Transport Dogfood on the real Windows/WASAPI path.
Status: **COMPLETE — TRANSPORT_DOGFOOD_PASS** (evidence run F: 26/26
TUI GREEN + machine regression 7/7 GREEN, both after final repairs).

> **Scope statement (addendum §10, verbatim obligation):** Stage A does
> NOT close PBK-003 conformance and does not authorize any Linux
> backend work. Stage A establishes the BEFORE Windows/WASAPI
> behavioral baseline that the Stage-B
> HOST-RENDER-WINDOWS-CONFORMANCE-CORRECTIVE must preserve ("architecture
> identity changed, playback semantics did not").

## 1. Authority and branch identity

```text
campaign branch   hardening/transport-dogfood-tui-v1-closure-1
rebased onto      origin/main ed240e377cef4c9ec902f2ce1b890b2166b1e3d0
                  (ADR-PBK-003 Stable Output Plugin / Pluggable Host
                  Render Backend Boundary, PR #163)
authority loaded  ADR-PBK-001 (foundations, P1–P5), ADR-PBK-002
                  (D11/D14 vocabulary + transport semantics),
                  ADR-PBK-003 (backend boundary), issue #119 (Phase F v2)
addendum honored  Stage A NOT restarted; harness work retained; no
                  Linux work entered; no backend corrective performed
                  during Stage A
stage-A commits   2103882 (harness), 98c7cd3 (repairs), 00e5021 (evidence)
product delta     ZERO (git status contains only
                  research/transport-dogfood/* at all times)
```

## 2. Build identity (strong, per run — addendum §5)

All runs executed against the SAME production binary:

```text
headless_sha256   cb29662a18fe040a266de333edb621dc576635e3f8ce32db83b5c46bb7fbe255
build             cargo build --release --target x86_64-pc-windows-gnu
                  --features playback -p qianqian-headless
                  QIANQIAN_NATIVE_DIR=/tmp/qn-dogfood-stage/native (mingw COFF staging)
songcore_sha256   17323291fc7b53997a36746e1ce7f86b8863a63f71be79389b9b196a05175a2a
rustc             1.97.1 (8bab26f4f 2026-07-14)
host              Microsoft Windows 11 Pro (caption read via CIM; GBK
                  mojibake in ENV files is a decode artifact of the
                  query, not of the product), Realtek High Definition
                  Audio, default render endpoint, shared mode,
                  event-driven (as opened in each transcript's wasapi
                  stderr line: "buffer 970 frames (shared, event-driven)")
per-run driver    tuidriver sha256 recorded per ENV-TUI-RUN<N>.txt;
                  runs D/E/F share the headless hash above (no binary
                  drift behind the run-D anomaly)
corpus            SHA256 per file in every ENV file (identical across runs)
```

## 3. Corpus census (addendum §6; full document: CORPUS.md)

```text
flac4.flac mp3cbr.mp3 alac4.m4a alac6.m4a   repo committed fixtures (renamed only)
synth45.mp3 synth30.flac                    LOCALLY GENERATED synthetic
                                            media (ffmpeg sine), declared
                                            as such, SHA256-pinned
garbage.bin                                 deterministic invalid candidate
```
No personal or copyrighted audio was used. ACOUSTIC_WITNESS = UNAVAILABLE
(honest, per campaign; human-ear items remain recorded and outstanding).

## 4. Harness design and limitations

Documented in `research/transport-dogfood/README.md` (design, eight
ConPTY/diff-render lessons, instrumentation, run ledger, limitations)
and `CORPUS.md`. In short: `tuidriver.exe` spawns the REAL
`qianqian-headless.exe play …` TUI under a 120×40 ConPTY, injects the
frozen key grammar, emulates the VT cell grid (full-repaint jiggle per
key write to defeat diff-render stale cells), and evaluates bounded
oracles — truth-class-pinned labels, new-Position liveness samples,
exit codes, absence witnesses, bounded resource checkpoints — against
an append-only frame history, with a wall-clock chronology and
child-liveness capture per scenario. Waits are bounded observables,
never fixed sleeps; failure classes are separated per addendum §3.

## 5. Matrix result (evidence run F)

Full 26-scenario matrix, ONE detached invocation, driver exit 0 —
`evidence/logs/tui-runF/` (JSON verdict + transcript + raw VT stream +
chronology per scenario), identity in `ENV-TUI-RUNF.txt`.

```text
A1-flac4/mp3cbr/alac4/alac6  GREEN   natural EOF → Completed, quiet exit
A2                           GREEN   pause/resume incl. rapid double presses
A3                           GREEN   forward seeks + natural EOF after cutover
A4                           GREEN   backward seeks, monotone progression
A5                           GREEN   rapid seek burst, no wedge, playback continues
A6                           GREEN   pause×seek both orderings, intent survives
A7                           GREEN   volume walk 100→0→70; 0 keeps consuming
A8                           GREEN   volume while paused
A9                           GREEN   volume across seek cutovers, desired kept
A10                          GREEN   A→B→A open replacements, old Source absent
A11                          GREEN   invalid opens refuse; old episode keeps consuming
A12                          GREEN   open while paused → fresh episode unpaused
A13                          GREEN   open right after seek; bounded resources
A14                          GREEN   n/p navigation + inert boundaries (Track 1/3→3/3)
A15                          GREEN   refused candidate: "next refused: SongCore
                                     refused '<abs>': status 104"; no auto-skip
A16 (+paused/after-seek)     GREEN   stop while active/paused/after-seek → Stopped,
                                     terminal immutable on repeat press
A16-drain-stop               GREEN   near-EOF stop dogfood: late stop settles
                                     EXACTLY ONE truthful terminal — Completed
                                     OR Stopped by actual command timing
                                     (run F observed Stopped). Does NOT
                                     establish the drain-window boundary;
                                     see the scope note below and §7
A17/A18                      GREEN   stop→open and open→stop routing
A19                          GREEN   12 open replacements; threads 7 vs base 6,
                                     ws 13MB (bounds 6 delta / 300MB)
A20                          GREEN   full-control soak + resource checkpoint
```

Machine-mode regression (campaign §7) — `machine-runF2.summary`: M1–M7
**7/7 GREEN** (natural EOF exit 0; status/stop; pause/resume truthful
command state; seek rebase witnessed in a bounded window; invalid seek
tokens inert; garbage candidate → activation report, exit 1, no forged
terminal; unknown command = presentation noise).

A16-drain-stop scope note (oracle honesty, not weakening): this
scenario is a **near-EOF stop dogfood**. Its legitimate result is
Completed OR Stopped, by actual command timing (run F observed
Stopped — the pressed timing landed outside the decode-EOF drain
window). This scenario does NOT establish the exact decode-EOF
drain-window classification boundary. Exact D11 drain-window
semantics are covered by the dedicated D11 conformance/regression
suite (PR #144 shared truth table, exhaustive 48-tuple Rust/TLC
byte-compare), not this wall-clock TUI dogfood case. The scenario
name is retained for run-D/E/F/G evidence continuity only; no sleep
was manufactured to force one outcome deterministically.

## 6. Findings by truth class (addendum §3 taxonomy)

```text
DOGFOOD PRODUCT FAILURE          0 observed (runs E/F)
MECHANISM FAILURE                0 observed (runs E/F)
ORACLE-HARNESS FAILURE          10 recorded, ALL fixed in harness only
                                 (see H-1..H-10 below; each repair is in
                                 commit 98c7cd3 and was re-verified)
ARCHITECTURE CONFORMANCE
DIFFERENTIAL                     1 known, recorded NOT fixed (K-1; the
                                 Stage-B closure-blocking gate)
DOCUMENTATION DRIFT              0 found
ENVIRONMENT FAILURE              0 (host/endpoint stable across runs)
UNREPRODUCED ANOMALY             1 recorded honestly (U-1 below;
                                 unclassifiable from run-D evidence)
```

Harness failures (all ORACLE-HARNESS, all fixed, none touched product):

```text
H-1   missing lpAttributeList assignment → child silently without
      pseudoconsole (runs A/B)
H-2   WSL-interop console hijack → detached launch required; no
      Start-Process output redirection (run A/B)
H-3   char-boundary panic in timeout tails over multi-byte glyphs;
      panic hook added (run C, logs/runC-panic.txt)
H-4   child CWD not pinned: detached launch from a Linux-cwd shell
      starts in C:\Windows\System32 → every relative O-open honestly
      refused (os error 2). THE run-D open family (run D)
H-5   diff-render stale cells at diff boundaries made substring
      oracles unsafe; fixed by pseudoconsole width jiggle → full
      repaint per key write (run E)
H-6   needle/render mismatches: O-open feedback and Source echo the
      TYPED path; navigation refusal prefix is "next refused:";
      navigation feedback is absolute (run E)
H-7   JSON evidence malformed: step strings unescaped (run D)
H-8   A15 expected a refusal from a VALID next track — impossible
      expectation; playlist now carries garbage.bin as track 2 (run D)
H-9   A14/A20 fixture timing: 4–5 s fixtures under 20–60 s scripts EOF
      mid-script and falsify later expectations; playlists resized
      (run D)
H-10  machine M4 feeder: sleeps in a stdin FILE execute at authoring
      time; position needle over-specified for the 1 Hz projection →
      piped feeder + bounded window witness (run F)
```

K-1 — KNOWN_ARCHITECTURE_DIFFERENTIAL (recorded, not fixed in Stage A):
current production opens WASAPI directly inside the output plugin
(`qianqian-output-wasapi` / `wasapi_output_plugin()`), i.e. the Output
Plugin and the Windows Host Render Backend are one fused unit, while
ADR-PBK-003 §2 requires "Output is the Plugin; the backend is an owned
mechanism behind the backend-neutral AudioOutput contract" (§11 records
this exact differential). Per the addendum this becomes the
closure-blocking HOST-RENDER-WINDOWS-CONFORMANCE-CORRECTIVE gate of the
Integrated Audit (Stage B), with the addendum's forbidden list (no D11/
F3–F6/nav/volume semantic changes, no Linux backend selection, no
TailQuiesced/stale-PCM weakening, no BackendManager/registry/live
switching). Every A-scenario in §5 doubles as the BEFORE baseline for
the corrective's semantic-equivalence rerun.

U-1 — NON-BLOCKING KNOWN ANOMALY (preserved; NOT classified fixed):

```text
Observed:                     run D contained six total-silence
                              scenarios (A2/A3/A4/A7/A9/A11): the app
                              emitted NO output after its first render
                              (raw streams end; no position ticks at
                              1 Hz; no key feedback) while the process
                              was presumptively alive.
Reproduced after
instrumentation:              NO — all six scenarios passed in run E
                              under full instrumentation and again in
                              run F.
Binary identity difference:   NONE — the production binary is
                              byte-identical across D/E/F.
Known root cause:             NONE — run D's driver lacked
                              chronology/liveness instrumentation, so
                              no truth class can be honestly assigned;
                              candidate causes (host timing,
                              stderr/ConPTY races) are unprovable from
                              run-D evidence.
Subsequent instrumented runs: GREEN.
Closure status:               NON-BLOCKING KNOWN ANOMALY — standing
                              watch item for the audit reviewers and
                              the final Windows regression; the
                              instrumentation now in place will
                              classify it immediately should it recur.
```

## 7. Physical evidence boundary

Verified mechanically on the real Windows/WASAPI path: command routing
over the frozen grammar, truth-class-correct labels and feedback, D14.8
position liveness, D11 terminal commitment and immutability, D14.5
refusal/cutover behavior at the UI level, episode replacement and
navigation, volume desired-factor routing, disposal quietness, exit
codes, bounded resources. NOT mechanically claimed: audibility, stale-
audio absence after cutovers (owned by D14.5's own oracles + the
recorded human-ear item), and the drain-window stop⇒Completed mapping
(owned by the D11 conformance suite — PR #144 shared truth table,
exhaustive 48-tuple Rust/TLC byte-compare). ACOUSTIC_WITNESS =
UNAVAILABLE throughout.

## 8. Audit inputs handed to Stage B

```text
- the §5 matrix as the BEFORE semantic baseline (post-corrective rerun
  must satisfy: "architecture identity changed, playback semantics did
  not");
- K-1 as the corrective's scope definition (with the addendum's
  forbidden list);
- corpus + build identity procedure (ENV files) for every audit run;
- chronology/liveness instrumentation as the audit's wedge classifier;
- U-1 as a standing watch item for audit reviewers.
```
