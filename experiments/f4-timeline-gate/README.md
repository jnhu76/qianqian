# experiments/f4-timeline-gate — F4-GATE mechanism evidence

Executable mechanism evidence for the Phase-F **F4 Position / Duration
gate** (Issue #119; proposed ADR-PBK-002 §20 D14.8 amendment). This
crate is **not production architecture**: it is workspace-excluded,
imports no `qianqian-*` crate on the Windows path, and nothing here
ships in the product path. The frozen contract lands in ADR-PBK-002;
the product implementation lands in a later F4 slice.

## What is here

```text
src/timeline.rs        the position projection algebra + deterministic
                       interleaving oracles (all platforms): capping,
                       unknown≠zero, one-block torn-read bound, clamp
                       monotonicity, pause freeze at tail quiescence,
                       EOF rise to the exact decoded total, terminal
                       withdrawal, scripted-interleaving fuzz
src/bin/f4probe.rs     physical WASAPI probe (Windows only):
                       Experiment A — submitted / padding / derived
                       consumed across steady → pause → engage → tail
                       drain → quiesced park → resume → EOF → drain
                       Experiment B — IAudioClock frequency/position
                       comparison incl. the 44.1 kHz AUTOCONVERTPCM
                       unit leg
src/bin/f4duration.rs  duration provenance over the SongCore ABI
                       (non-Windows): Experiment C — container /
                       stream duration vs exact decoded total on the
                       committed corpus + truncated adversarial copies
evidence/              raw probe output (3 × f4probe, 3 × f4duration)
RESULTS.md             the evidence record: measurements, decision
                       table, authority proposal status
```

## The proposed evidence shape

Two session-owned mechanism-evidence cells plus one derived projection
(the normative text is the D14.8 amendment in this same branch):

```text
submitted   source frames handed to the render leg by the edge read
            path (== frames submitted into the device buffer on all
            non-terminal paths); monotone; one relaxed store per block
tail        the output mechanism's latest GetCurrentPadding reading in
            source frames; NOT monotone; published per loop iteration,
            per park slice, and on the drain path — the same reading
            D14.7 already trusts for output-tail quiescence

raw position = submitted - min(tail, submitted)      (None before the
                                                      first tail read)
position     = max(last, raw)                        (monotone clamp)
```

Derivation error bound: ± one in-flight block (1024 source frames
≈ 21 ms at 48 kHz) in either direction — a one-iteration-stale tail can
lead the truth, an independent pair read can tear backward; there is no
accumulating error, and the clamp removes only the backward half. The
probe is a mechanism twin in the production render-loop order (see
RESULTS.md §3); it links no production crate, so its numbers measure the
mechanism shape, not production code.

## Running

```bash
# algebra oracles (any platform)
cargo test --manifest-path experiments/f4-timeline-gate/Cargo.toml

# physical probe (Windows host / WSL interop; plays ~12 s of quiet
# 440 Hz tone through the default endpoint)
cargo run --manifest-path experiments/f4-timeline-gate/Cargo.toml \
    --target x86_64-pc-windows-gnu --bin f4probe

# duration provenance (any platform with the SongCore native artifact)
cargo run --manifest-path experiments/f4-timeline-gate/Cargo.toml \
    --bin f4duration
```

Whether a displayed number *looks* right is deliberately not claimed
by any executable here — that is the human reviewer's acceptance.
