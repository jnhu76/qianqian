# F6-OPEN-SMOKE RESULTS — OPEN_SMOKE_GREEN (conditional)

Campaign: QIANQIAN-F6-NAVIGATION-VOLUME-AUTONOMOUS-1, Stage C
(F6 Open implementation). Authority under test: ADR-PBK-002 D14.6
(F6-AUTHORITY-PROMOTION-1 + the Stage D playlist closure, same
amendment). Branch: `feat/volume-1` (per-run `branch:` and `repo_head_sha:` in
the ENV files). The current evidence set (O1–O7, three runs) was
produced by ONE cross-build from the committed Stage F tree (exe sha
5eb295d9…, identical across the three ENV files — the binding
evidence, not per-run rebuilds, and committed as the build input's
own commit); the earlier sets (4754edc2… Stage C, 212193bc…
Stage D, and the interim builds between harness fixes) are superseded
by this re-run and named in the commit history.

## Verdict

**OPEN_SMOKE_GREEN ×3 — 21/21 scenario-runs GREEN** (Stage D added
O6 navigation; Stage F added O7 volume; the full matrix re-ran fresh
with the Stage F binary) — conditional on the same UNAVAILABLE
acoustic human-ear witness as S-PROBE (below).

## Environment (all runs)

```text
host:        Windows 11 (windows_caption_raw in ENV-RUN<N>.txt)
audio:       default render endpoint, shared mode, event-driven
             (Realtek speaker; per-run identity in ENV files)
main media:  main45.mp3 — synthetic 45 s 44.1 kHz stereo CBR MP3 (ffmpeg sine)
candidates:  mp3-valid.mp3, flac-valid.flac (repo fixtures);
             garbage.mp3 (1024 random bytes, regenerated per run)
endpoint:    [qianqian-wasapi] opened: 44100 Hz, 2 channels, mask 0x3,
             buffer 970 frames (shared, event-driven) — every run's stderr
```

## Scenario matrix

| run | O1 | O2 | O3 | O4 | O5 | O6 (nav) | O7 (vol) |
|-----|----|----|----|----|----|----------|----------|
| 1   | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |
| 2   | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |
| 3   | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |

Representative measured evidence (full JSON per scenario in
`evidence/logs/`):

- **O1 replacement**: old settles `Stopped` with stop intent recorded;
  new episode established (44100 Hz × 2, mask 0x3) and consuming;
  replacement wall time 52/51/51 ms (run1/2/3 O1 JSONs of the current
  evidence set); quit disposal `Discharged`.
- **O2 refusal-before-destruction** (the frozen product property):
  garbage candidate ⇒ `Refused` with the provider diagnostic;
  `post_refusal_stop_requested = false`; the old episode's position
  publication kept advancing (4/4 post-refusal windows); a later
  valid Open still committed. An invalid Open candidate never kills
  live playback — physically witnessed.
- **O3 paused replacement**: the D14.7 frozen establishment
  conjunction became true on the real mechanism before the Open;
  stop-from-paused settled `Stopped`; the new episode established.
- **O4 seeked replacement**: the D14.5 cut physically landed
  (position 30429 → 1805607 frames ≈ 40.9 s into the 45 s source);
  the replacement then settled the episode `Stopped`; new episode
  established and consuming.
- **O5 repeated replacement**: three consecutive replacements
  (MP3 → FLAC → MP3) each settled the previous episode `Stopped` and
  committed the next; final quit `Stopped` + `Discharged`, quiet.
- **O7 volume (Stage F)**: the desired stream factor routed to 60
  mid-episode — the episode stayed unsettled and kept consuming; at
  factor 0.0 (silence) the position publication KEPT advancing
  (silence is still submitted frames — frame accounting, not
  loudness); the desired level survived episode replacement (delivered
  before activation); quit Stopped + Discharged. A volume command
  never settled terminal truth — physically witnessed on the product
  path.
- **O6 navigation (Stage D)**: a 3-file startup playlist walked with
  next/previous through the SAME real replacement — every step in
  BOTH directions witnessed the committed source equals the selected
  entry (`source_ok`), the new episode consuming (position
  publication advancing), and the previous episode settled `Stopped`;
  the last-entry next was inert; previous at the first entry was
  inert; final quit `Stopped` + `Discharged`, quiet.
  Commit-on-activation and the inert ends are pinned executably by
  the Stage D unit matrix; this run witnesses them on real devices.

## Findings produced by this evidence round (all fixed on-branch)

1. **probe_media did not compile** — the first cross-build of the
   workspace-excluded decode crate (which no CI compiles) caught the
   Stage-C probe reading nonexistent struct fields; it now reads the
   declared facts through the endpoint trait (acd1b57). No semantic
   change.
2. **Harness oracle startup hazard** — the render leg legitimately
   takes ~0.5 s on this host from device open to its FIRST position
   publication; the liveness oracle sampled inside that gap and fired
   on healthy episodes (a4712a0's fix waits, bounded, for first
   publication). The "advance at exactly source rate" observation
   (11025 frames per 250 ms) came from an interactive `QN_OSMOKE_TRACE`
   diagnostic run whose log is not committed; the committed evidence is
   the 15 GREEN JSONs produced by the first-publication-waiting oracle.
3. **The REAL binary never compiled with `--features playback`** —
   the default-feature test battery does not compile the gated
   main.rs; a windows-gnu playback-feature check caught the machine
   path still consuming the pre-F6 snapshot return (24ae4e7). The
   playback-feature cross-compile is now part of this slice's local
   gate evidence.

## Boundary of this evidence

- Proves: the frozen D14.6 replacement sequence end to end on real
  devices and real media — probe refusal ordering, D11 settlement of
  replaced episodes, authoritative establishment of new ones, the
  D14.7 and D14.5 interactions, repeated replacement stability, and
  clean quit/disposal.
- Does NOT prove: audible continuity (human-ear witness UNAVAILABLE —
  the mechanical witness is the render leg's own position publication
  advancing at source rate). A failed ear check reopens this verdict.
- Process stability: every scenario exited cleanly (exit 0 / GREEN);
  the 120 s evidence watchdog never fired.
