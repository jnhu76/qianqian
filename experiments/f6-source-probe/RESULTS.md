# F6-S-PROBE RESULTS — verdict record

```text
campaign:      QIANQIAN-F6-NAVIGATION-VOLUME-AUTONOMOUS-1, Stage A
base:          main @ 76c205a74ad2e4d1e00facd02259825ad2d648ca
               (tree 5f0d57c6eb5e945e546af26d193f00aee1e81771)
branch:        research/f6-s-probe-1 (evidence built at af53ab2)
harness:       experiments/f6-source-probe (workspace-excluded; the only
               paths touched in this slice are under experiments/f6-source-probe/)
production delta:         ZERO (no production file modified)
canonical authority delta: ZERO (promotion is a separate later slice)
```

## VERDICT: S_PROBE_GREEN

All three required independent physical runs matched every scenario
expectation: S1–S10 GREEN (old live playback undisturbed, probe
verdicts correct, clean settlement) and the NEG negative control RED
exactly as designed (the oracle fired).

**Conditionality (review round 1, MAJOR-1):** the gate's GREEN phrase
is "old stream content and cadence unaffected". Cadence (and the
episode's terminal truth, diagnostics, and settle) is mechanically
evidenced below. The **content** leg — that the old stream still
*sounds* undisturbed — has no witness in this autonomous run: no human
ear was attached to the endpoint. The acoustic-continuity item is
therefore recorded as **UNAVAILABLE (open review item)**, the same
class as the F5 smoke's ear item. This GREEN authorizes the
authority-promotion slice on the mechanical evidence; the human ear
check remains an outstanding acceptance item on PR #157 for the human
reviewer, and a failed ear check reopens this verdict.

## Physical evidence

```text
host       Windows 11 Pro build 26200 (raw localized caption preserved
           in ENV-RUN{1,2,3}.txt `windows_caption_raw`; zh-CN)
endpoint   Realtek High Definition Audio, default render endpoint,
           shared mode event-driven — every scenario's committed
           stderr carries the production output plugin's line:
           "opened: 44100 Hz, 2 channels, mask 0x3, buffer 970 frames"
binary     sprobe.exe x86_64-pc-windows-gnu release, ONE exe for all
           3 runs, built from the committed tree at af53ab2:
           sha256 f2f74bcecd820cd468620d1981bb0482920cbe1c93b4a8d450f5b54b5e162f0a
media      MAIN = main45.mp3 — synthetic 45 s 44.1 kHz stereo CBR MP3
           (ffmpeg sine 440 Hz)
           valid candidates = repo fixtures mp3-cbr-id3v23.mp3,
             flac-16-44-stereo.flac, alac-long.m4a
           invalid candidates = garbage.mp3 (1 KiB random), empty.mp3
             (0 B), truncated.mp3 (300 B of the MP3), missing-file.mp3
runs       3 independent runs × 11 scenarios = 33 processes, 33/33
           matched; per-run JSON + stderr in evidence/logs/,
           environment identities in evidence/ENV-RUN{1,2,3}.txt;
           tools/run-windows.sh records the exact commands
host-side  evidence/host-probecheck.txt — probecheck (no playback) on
sanity     the committed source: valid fixtures probe with correct
           facts (mp3 44100/2/4 s, flac 44100/16/2/4 s, m4a
           44100/24/2/6 s); garbage/empty/missing refused (104 / host
           IO) — exit-code oracles matched, exit 0
```

## Per-scenario results (run 1 numbers; runs 2–3 identical in outcome)

| Scenario | Window (measured) | Probes | Old-episode advance while probing | Verdict |
|---|---|---|---|---|
| S1 live × valid MP3 probe | 3.60 s | 6 (max 799 µs) | 26460 → 185661 frames = **44161 frames/s vs 44100 rate (100.1%)** | GREEN ×3 |
| S2 live × valid FLAC probe | 3.61 s | 6 | 44149 frames/s (100.1%) | GREEN ×3 |
| S3 live × corrupt candidate | 3.61 s | 6 | all probes refused (`song_open status 104`); 44137 frames/s (100.1%) | GREEN ×3 |
| S4 probe/open/drop cycles | 6.01 s | 16 (12 probes + 4 held 250 ms) | 44107 frames/s (100.0%) | GREEN ×3 |
| S5 paused episode × probes | 3.81 s | 4 | freeze oracle held during the established pause; advance before/after | GREEN ×3 |
| S6 probes around a seek (1.0 s) | 3.41 s | 5 | exactly one regression, at the cut: 79380 → 50832 frames (≈1.15 s landing); advance continuous around it | GREEN ×3 |
| S7 three invalid candidates ×2 | 2.60 s | 6 | missing path / empty / garbage all refused; 44202 frames/s (100.2%) | GREEN ×3 |
| S8 successful probe held 1 s, dropped | 3.00 s | 4 | 44173 frames/s (100.2%) | GREEN ×3 |
| S9 alternating valid/invalid | 3.61 s | 10 | 10/10 verdicts correct; 44137 frames/s | GREEN ×3 |
| S10 held handle + second-candidate probes | 3.21 s | 4 (≤780 µs each) | one native handle held open ~3.2 s across 3 probes of another file, then closed, then probed again | GREEN ×3 |
| NEG dead-episode control | 2.20 s | 4 | stop at window start → position withdrew; oracle FIRED ("position absent in 10 playing-window samples — the episode was not live/rendering") | RED as expected ×3 |

Settlement evidence in every S-run: terminal `Stopped` (commanded by
the harness settle step), no failure diagnostic, no activation error,
dispose snapshot quiet, process exit 0.

## What this proves (and what it does not)

```text
PROVEN (mechanism, physical, ×3, from a committed source tree):
  - a second SongCore native handle can open + probe + close a candidate
    while the live episode decodes/renders at full source rate
  - failed/corrupt candidate probes do not damage the live episode
  - probe handles may be held open across seconds and multiple handles
    may coexist with the episode's handle (S4/S10) without disturbance
  - the probe verdict oracle is falsifiable: a non-rendering old episode
    is detected (NEG fired 3/3)

NOT PROVEN here (honest limits):
  - acoustic continuity: the human-ear witness is UNAVAILABLE in this
    autonomous run — recorded above as the conditionality on this GREEN
  - S10 proves handle-level coexistence, not a survey of Windows global
    audio state beyond this process/endpoint
  - thread stability observed as clean process exit; no live thread
    census
  - 3 runs × 11 scenarios × one 45 s synthetic MAIN; candidates limited
    to the listed fixture shapes
  - S6's 2.5 s seek-regression allowance spans most of its 3.4 s
    window, so S6 alone cannot localize a probe-induced regression near
    the cut; the no-seek scenarios (S1/S2/S3/S7/S8/S9) carry that
    coverage for the matrix as a whole
```

## Verification statement

```text
target:  F6-S-PROBE concurrency question (the F6-OPEN-GATE.md §3
         disclosure), on the exercised endpoint
tools:   sprobe.exe harness (this crate) over unmodified production
         crates at the base commit; cmd.exe interop from WSL; ffmpeg
         (fixture generation); probecheck host sanity
bounds:  3 runs × 11 scenarios; one 45 s synthetic MAIN; candidates
         limited to the listed shapes; shared-mode default endpoint;
         position sampling at 200 ms with a ≥70 % liveness floor, a
         ≤2.5 s seek-regression allowance and a 16384-frame pause-freeze
         bound; settle watchdog at 30 s
result:  S_PROBE_GREEN (conditionally, per the UNAVAILABLE acoustic
         witness) — Candidate B (probe-before-destruction) may proceed
         to the authority-promotion slice; a RED would have reopened
         the F6 mechanism decision instead
```

## Evidence provenance note

The first evidence set (commit 3cf2a51) was produced by a pre-fix exe
whose exact source binding was not pinned (untracked tree at run time).
After review round 1, the harness source was fixed, committed
(af53ab2), and the entire matrix was re-run from a fresh build of that
committed tree (exe sha256 above, recorded in every ENV file). The
current logs/ENV set replaces the first set wholesale; the first set's
33/33 result was consistent with it, and its S1 smoke (pre-fix exe)
matched as well.
