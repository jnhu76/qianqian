# F6-S-PROBE — second native handle probes while live playback continues

Evidence slice for the F6 Open gate (ADR-PBK-002 D14.6
`F6 CONFIG-MECHANISM-OPEN`; F6-OPEN-GATE.md §3 concurrency disclosure).
Sequenced between the F5 merge and the F6 authority promotion, exactly
like the F5 E3 discipline: mechanism physical fact → evidence →
authority freeze → implementation.

## The load-bearing question

```text
Can an OLD live episode keep decoding/rendering normally
while a SECOND SongCore native handle probes a candidate media source?
```

F6 Candidate B ("probe the new candidate FIRST; only destroy the old
episode after the candidate is known usable") is promotable only if the
answer is yes. A RED answer reopens the F6 mechanism decision instead
of degrading to Candidate A (destroy-then-probe), because that would
abandon the frozen product property "an invalid Open candidate never
kills live playback".

## What the probe is (and is not)

The second handle mirrors the production open path —
`song_open` → `song_probe` → `song_close`, with the production ABI
version check and host-IO callback guards — and stops there:

```text
probe DOES      open the container, read format/duration evidence, close
probe DOES NOT  read PCM frames, start a Playback Session, open an
                output stream, create a render leg, create an episode,
                touch current-episode state, publish any Fact
```

It is temporary source-mechanism evidence only. The production decode
crate is NOT modified: the harness drives `qianqian-songcore-sys`
directly, so production behavior delta on the base commit is zero.

## Harness

`sprobe.exe` (one process per scenario) starts ONE live episode through
the unmodified production wiring (`QianqianApp` + decode/output/session
components, exactly `apps/headless/src/main.rs start_episode`), runs a
per-scenario probe schedule on a second native handle, and samples the
old episode's D14.2 observation seam at 200 ms. Oracles:

```text
probe verdict   every probe op matches its expectation (valid → the
                open+probe sequence succeeds and its facts are
                RECORDED, not semantically validated; invalid →
                refused at open/probe/host-IO)
position        old-episode Position advances ≥70% of
                source_rate × elapsed over the playing window;
                regressions only within 2.5 s of a seek command
pause freeze    S5: position moves ≤16384 frames while the D14.7 Paused
                projection is established
terminal        D11 settles Stopped (commanded), no failure diagnostic,
                no activation error, dispose snapshot quiet
NEG control     same schedule with the old episode stopped at window
                start — the position oracle MUST fire (expected RED),
                proving the harness detects a non-rendering episode
```

The audible continuity property stays a human-ear item (F5 smoke
precedent); what this harness proves mechanically is that the old
render leg keeps consuming at source rate, never settles Failed, and
probe verdicts stay correct — plus process stability (clean exit).

## Scenarios

```text
S1  live × valid MP3 probe (×6)              S7  3 invalid candidates ×2
S2  live × valid FLAC probe (×6)             S8  successful probe held
S3  live × corrupt candidate (×6)                1 s, dropped; old plays on
S4  12 probes + 4 held open/drop             S9  alternating valid/invalid ×10
    cycles (16 ops)                          S10 hold handle open 3.2 s while
S5  paused episode × 4 probes                    probing a second candidate,
S6  probes around a seek (1.0 s)                 then close; probe again
NEG dead-episode negative control
```

## Physical runs (real Windows host)

Cross-build (from WSL, mingw COFF SongCore archive staged so
`build.rs` finds `include/songcore.h` + `build/artifacts/libsongcore.a`
for the windows target):

```bash
cd experiments/f6-source-probe
mkdir -p /tmp/qn-sprobe-stage/native/build/artifacts
ln -s "$REPO/native/include" /tmp/qn-sprobe-stage/native/include
cp "$REPO/native/build/artifacts-mingw/"*.a /tmp/qn-sprobe-stage/native/build/artifacts/
QIANQIAN_NATIVE_DIR=/tmp/qn-sprobe-stage/native \
    cargo build --release --target x86_64-pc-windows-gnu
```

Run the matrix (3 independent runs; the runner stages the exe + media,
generating the synthetic MAIN and the invalid candidates itself):

```bash
tools/run-windows.sh 1   # then 2, 3
```

The MAIN file is a synthetic 45 s 44.1 kHz stereo CBR MP3 (ffmpeg
sine); candidates are the repo fixtures (mp3-cbr-id3v23.mp3,
flac-16-44-stereo.flac, alac-long.m4a) plus generated invalid shapes
(garbage bytes, empty file, truncated container, missing path).

Host-side sanity (no playback): `cargo run --bin probecheck -- <files>`
against the ELF archive validates the probe FFI sequence and the
valid/invalid expectations before any Windows run.

## Results

See `RESULTS.md`. Verdict recorded there governs; this README does not
carry the verdict.
