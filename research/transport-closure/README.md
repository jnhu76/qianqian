# POST-F5 TRANSPORT CLOSURE DESIGN-1 — reviewed design package

```text
Campaign:   QIANQIAN-POST-F5-TRANSPORT-CLOSURE-DESIGN-1
Branch:     research/post-f5-transport-closure-design-1
Base:       main @ feee7a32ecc09f8921a9c0372ae70df2e562d670
            (PR #154 F5-GATE-CORRECTIVE-1 merged; F5 production
             implementation NOT merged and still blocked by the frozen
             D14.5 text)
Status:     REVIEWED DESIGN — not canonical authority
Production delta in this branch:        ZERO
Canonical authority delta in this branch: ZERO
```

This package designs the remaining reference-player transport closure in
one documentation-only campaign:

```text
F6-OPEN-GATE.md       Open / fresh-source replacement
NAVIGATION-GATE.md    playlist / Next / Previous
VOLUME-GATE.md        player-local volume adjustment
TUI-CLOSURE.md        integrated TUI closure contract
DECISION-MATRIX.md    final decision matrix + authority-promotion plan
ADVERSARIAL-REVIEW.md adversarial reviews + fresh-context verdict record
```

## 0. Parallel-work rule (kept)

The concurrent F5 implementation branch (`feat/f5-seek-1`) was not based
on, cherry-picked from, or consulted for production content. This branch
starts independently from `main @ feee7a3` and modifies **no** production
file, no canonical authority file, no spec, and no issue checkpoint. The
only paths touched are under `research/transport-closure/`.

## 1. Authority baseline this design is built on

Loaded and followed (not restated normatively — link-only discipline):

```text
ADR-PBK-001 (ACCEPTED)   foundations: four lenses, Command/Fact/
                         Projection contracts, fact-authority identity,
                         P1–P5 publication/reclamation protocol
ADR-PBK-002 (ACCEPTED)   vocabulary, Plugin/Fiber taxonomy, D11 terminal
                         authority, D13 admission invariant, D14 guard:
                         D14.5 seek (F5-GATE frozen; implementation
                         blocked), D14.6 no-overlap replacement v1 +
                         F6 CONFIG-MECHANISM-OPEN, D14.7 pause/resume,
                         D14.8 position/duration, D14.9 volume/device
                         (not frozen), D14.10 agent stop-list
CONTEXT.md               current status + execution hypotheses
docs/README.md           router; truth classes
Issue #119               roadmap; latest checkpoint = F5-GATE-
                         CORRECTIVE-1 resolution on PR #154
```

Two D14 openings are exactly what this campaign must decide (as reviewed
proposals for a later tiny authority-promotion slice — **not promoted
here**):

```text
F6 CONFIG-MECHANISM-OPEN   how a fresh session gets its source/config
                           (D14.6; D14.10 stop-list item)
volume authority/mechanism session- vs provider- vs episode-level
                           (D14.9; D14.10 stop-list item)
```

## 2. Current production reality this design was checked against

All read-only, at `main @ feee7a3`:

```text
crates/qianqian-playback/src/handle.rs      episode seam: request_stop/
                                            request_pause/request_resume/
                                            observe/wait_terminal; no
                                            seek, no open, no volume
crates/qianqian-playback/src/session.rs     playback_session_spec(file,
                                            handle) captures the file in
                                            the definition closure;
                                            activation: resolve caps →
                                            open_media probe → edge →
                                            render stream → worker
crates/qianqian-composition/src/kernel.rs   step(): staged replacement —
                                            revision change retires the
                                            old fiber, drains/discharges
                                            it, removes the slot, and
                                            only then mounts+activates
                                            the fresh incarnation
crates/qianqian-app/src/lib.rs              QianqianApp: register/revise_
                                            desired/dispose; definitions
                                            fixed for kernel lifetime;
                                            dispose returns only the
                                            diagnostic snapshot — the
                                            authoritative disposal/
                                            activation outcome seams of
                                            F6 §5 are implementation-
                                            slice surface
apps/headless/src/main.rs                   start_episode(file) builds one
                                            runtime per process run
crates/qianqian-decode-songcore/src/lib.rs  SongcoreDecode is crate-
                                            private (H1): no public probe
                                            surface exists today
crates/qianqian-output-wasapi/src/wasapi.rs event-driven shared-mode
                                            render loop (padding →
                                            GetBuffer → pull → release);
                                            no volume code anywhere;
                                            WasapiOutput::new() trivial
apps/headless/src/tui/                      Space/S/Q(+Ctrl+C) only;
                                            renders the seam observation
```

## 3. Result shape (one paragraph)

Open, Next, Previous and Volume all close with **zero new Plugins, zero
new Facts, zero new lifecycle nouns, and no P1–P5 trigger**:

```text
Open          = an application composition operation that replaces the
                WHOLE episode composition (old runtime disposed with an
                authoritative `Discharged` outcome before the new one
                is built; commit consumes authority-owned operation
                results, never composition snapshots), after an
                App-owned source probe refuses invalid files without
                touching old playback
Navigation    = App-owned Vec<PathBuf> + Option<usize>; Next/Previous
                select a candidate and invoke the same Open replacement;
                the index commits only when the new episode is live
Volume        = App-owned desired level (application configuration)
                routed as a command to the episode's output mechanism,
                realized per-stream by WASAPI IAudioStreamVolume
TUI           = one reference-player shell whose App state is composition/
                navigation/config only; every playback truth stays behind
                the existing D14.2 observation seam
```

## 4. Verification statement

```text
target:      this design package (documentation only)
tools:       authority documents listed above; production source reading;
             Microsoft Learn WASAPI documentation (VOLUME-GATE §7);
             review rounds: (1) fresh-context adversarial review (§48)
             VERDICT PASS, 0 MAJOR / 5 MINOR; (2) human review of
             PR #155 VERDICT CHANGES_REQUIRED, 2 MAJOR / 1 MINOR;
             (3) human review of PR #155 VERDICT CHANGES_REQUIRED,
             1 MAJOR / 4 MINOR / 2 NIT — all findings fixed; records
             in ADVERSARIAL-REVIEW.md §5
bounds:      no production code was run or modified; no Windows host was
             driven; all mechanism-behavior claims for volume are
             documentation-cited and carry a mandated physical
             confirmation probe (V-PROBE) before the volume apply
             mechanism closes; the F6 probe-during-live-playback
             concurrency claim carries the same class of obligation
             (F6-S-PROBE) sequenced as an evidence-only slice BETWEEN
             the F5 merge and authority promotion — a RED S-PROBE
             reopens the F6 mechanism decision instead of promoting
             (exactly the F5 E3 discipline: physical fact → evidence →
             authority freeze → implementation)
result class: reviewed design, READY_FOR_HUMAN_REVIEW
```

## 5. Reading order

`F6-OPEN-GATE.md` first (Navigation composes it), then
`NAVIGATION-GATE.md`, then `VOLUME-GATE.md`, then `TUI-CLOSURE.md`
(integrates all three), then `DECISION-MATRIX.md`, then
`ADVERSARIAL-REVIEW.md`.

Citation note: "campaign §N" references throughout this package point
at the tasking document `QIANQIAN-POST-F5-TRANSPORT-CLOSURE-DESIGN-1`
(delivered with the task; it is not stored in this repository and is
not authority). Nothing load-bearing dangles on it: every requirement
those citations mark is restated inline where it is used.
