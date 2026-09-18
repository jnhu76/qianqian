# F6-S-PROBE RESULTS — verdict record

```text
campaign:      QIANQIAN-F6-NAVIGATION-VOLUME-AUTONOMOUS-1, Stage A
base:          main @ 76c205a74ad2e4d1e00facd02259825ad2d648ca
               (tree 5f0d57c6eb5e945e546af26d193f00aee1e81771)
branch:        research/f6-s-probe-1
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

```text
host:       Microsoft Windows 11 Pro build 26200 (zh-CN), real console
endpoint:   Realtek High Definition Audio, default render endpoint,
            shared mode, event-driven (harness log:
            "opened: 44100 Hz, 2 channels, mask 0x3, buffer 970 frames")
binary:     sprobe.exe x86_64-pc-windows-gnu release
            all 3 runs used the SAME exe:
            sha256 d589f843701de94e8206a94ed7c0ee7368f9121bc04d1c8e5007898f14bc6ead
            (an earlier pre-fix build was used only for the first S1
            smoke; after it, one harness-oracle boundary fix — straddling
            sample pairs must not read as in-window absence — was made
            and the matrix was run entirely on the fixed build; per-run
            exe/fixture SHA256s in evidence/ENV-RUN{1,2,3}.txt)
media:      MAIN  = main45.mp3 — synthetic 45 s 44.1 kHz stereo CBR MP3
                    (ffmpeg sine 440 Hz; sha256 in ENV files)
            valid candidates = repo fixtures mp3-cbr-id3v23.mp3,
                    flac-16-44-stereo.flac, alac-long.m4a
            invalid candidates = garbage.mp3 (1 KiB random),
                    empty.mp3 (0 B), truncated.mp3 (300 B of the MP3),
                    missing-file.mp3 (nonexistent path)
runs:       3 independent runs × 11 scenarios = 33 processes, 33/33
            matched expectations; logs in evidence/logs/, environment
            identities in evidence/ENV-RUN{1,2,3}.txt; runner
            tools/run-windows.sh records the exact commands
```

## Per-scenario results (all 3 runs identical in outcome)

| Scenario | Window | Evidence observed | Verdict |
|---|---|---|---|
| S1 live × valid MP3 probe ×6 | ~3.6 s | old Position advanced 4410 → 163611 frames in 3.6 s = **44161 frames/s vs 44100 source rate (100.1%)** while 6 probe ops ran (max 869 µs each) | GREEN ×3 |
| S2 live × valid FLAC probe ×6 | ~3.6 s | same advance shape; FLAC probe facts 44100/16/2ch/4 s | GREEN ×3 |
| S3 live × corrupt candidate ×6 | ~3.6 s | all 6 probes refused (`song_open status 104` UNSUPPORTED_CONTAINER); old episode advanced at source rate, settled Stopped, quiet | GREEN ×3 |
| S4 repeated probe/open/drop ×12 (4 held 250 ms) | ~8 s | 16 ops all correct; old playback continuous through open→hold→drop cycles | GREEN ×3 |
| S5 paused episode × 4 probes | ~8 s | pause projection established; probes ran while frozen; freeze oracle held (≤16384-frame tail bound); resume advanced again; settled Stopped | GREEN ×3 |
| S6 probes around a seek | ~4.4 s | exactly one Position regression, at the seek: 79380 → 50832 frames (landing ≈ 1.0 s = 44100 frames — the commanded target's neighborhood), advance continuous before and after; probes interleaved | GREEN ×3 |
| S7 three invalid candidates ×2 | ~3.6 s | missing path (host IO error), empty file (104), garbage (104) all refused; old playback untouched | GREEN ×3 |
| S8 successful probe held 1 s then dropped | ~3.6 s | ALAC probe facts 44100/24/2ch/6 s; drop clean; old playback continued | GREEN ×3 |
| S9 alternating valid/invalid ×10 | ~4 s | every verdict matched (10/10); old playback continuous | GREEN ×3 |
| S10 held handle + second candidate probes | ~4 s | one native handle held OPEN 3.2 s across 3 probes of another file (all sub-ms), then closed, then probed again; no interference | GREEN ×3 |
| NEG dead-episode control | ~2.8 s | stop commanded at window start → position withdrew; oracle FIRED ("position absent in 10 playing-window samples — the episode was not live/rendering") | RED as expected ×3 |

Settlement evidence in every S-run: terminal `Stopped` (commanded by
the harness settle step), no failure diagnostic, no activation error,
dispose snapshot quiet, process exit 0.

## What this proves (and what it does not)

```text
PROVEN (mechanism, physical, ×3):
  - a second SongCore native handle can open + probe + close a candidate
    while the live episode decodes/renders at full source rate
  - failed/corrupt candidate probes do not damage the live episode
  - probe handles may be held open across seconds and multiple handles
    may coexist with the episode's handle (S4/S10) without disturbance
  - the probe verdict oracle is falsifiable: a non-rendering old episode
    is detected (NEG fired 3/3)

NOT PROVEN here (honest limits):
  - acoustic continuity: Position-at-source-rate + no-Failed + clean
    settle is the mechanical continuity evidence; the audible check
    remains a human-ear item for review (F5 smoke precedent)
  - Windows session/global audio state beyond this endpoint/process was
    not sampled; S10 covers handle-level isolation only
  - thread-count stability was observed only as clean process exit;
    no live thread census was taken
```

## Verification statement

```text
target:  F6-S-PROBE concurrency question (the F6-OPEN-GATE.md §3
         disclosure), on the exercised endpoint
tools:   sprobe.exe harness (this crate) over unmodified production
         crates at the base commit; cmd.exe interop from WSL; ffmpeg
         (fixture generation); probecheck host sanity
bounds:  3 runs × 11 scenarios; one 45 s synthetic MAIN; candidates
         limited to the listed fixtures/shapes; shared-mode default
         endpoint; position-sampling at 200 ms with a ≥70 % liveness
         floor and a ≤2.5 s seek-regression allowance
result:  S_PROBE_GREEN — Candidate B (probe-before-destruction) is
         physically evidenced and may proceed to the authority-
         promotion slice; a RED would have reopened the F6 mechanism
         decision instead
```
