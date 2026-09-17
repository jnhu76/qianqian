# experiments/f4-timeline-gate — F4-GATE mechanism evidence

Executable mechanism evidence for the Phase-F **F4 Position / Duration
gate** (Issue #119; proposed ADR-PBK-002 §20 D14.8 amendment). This
crate is **not production architecture**: it is workspace-excluded,
imports no `qianqian-*` crate on the Windows path, and nothing here
ships in the product path. The frozen contract lands in ADR-PBK-002;
the product implementation lands in a later F4 slice.

## What is here

```text
src/timeline.rs        the position-evidence shape + deterministic
                       oracles (all platforms): undefined≠zero, exact
                       per-instant samples, tail capping, monotone
                       publication under a regressing queue, pause
                       freeze at tail quiescence, EOF rise to the exact
                       decoded total, terminal withdrawal, the REJECTED
                       two-cell reader pair as a negative control, and
                       scripted-interleaving fuzz
src/bin/f4probe.rs     physical WASAPI probe (Windows only):
                       Experiment A — handed-off / padding / published
                       sample across steady → pause → engage → tail
                       drain → quiesced park → resume → EOF → drain
                       Experiment B — IAudioClock frequency/position
                       comparison incl. the 44.1 kHz AUTOCONVERTPCM
                       unit leg
src/bin/f4duration.rs  duration provenance over the SongCore ABI
                       (non-Windows): Experiment C — container /
                       stream duration vs exact decoded total on the
                       committed corpus + truncated adversarial copies
evidence/              raw probe output (3 × f4probe, 3 × f4duration);
                       f4probe-pairtear-run*.log is the superseded
                       two-cell reader shape's raw record (commit
                       36104ce8), kept as the counterexample that
                       motivated F4-GATE-CORRECTIVE-1
RESULTS.md             the evidence record: measurements, decision
                       table, authority proposal status, both review
                       rounds
```

## The proposed evidence shape

One session-owned mechanism-evidence cell plus the render leg's own
accounting (the normative text is the D14.8 amendment in this same
branch):

```text
handed_off  source frames this episode has submitted into the device
            buffer, accounted by the render leg itself (plain
            mechanism-local accounting; never published, never read by
            the observation)
tail        the render leg's GetCurrentPadding reading in source
            frames; NOT monotone; taken in the steady loop, in every
            park slice, and on the drain path — the same reading D14.7
            already trusts for output-tail quiescence

writer      estimate = handed_off - min(tail, handed_off)
            published = max(published, estimate)     monotone relaxed
                                                     update
reader      position() = ONE pure load     (None while undefined)
```

Both derivation inputs belong to the render leg's own execution path, so
there is no cross-cell composition anywhere — no torn pair, no reader
state, and monotonicity is owned by the publication rather than by the
reader. That is what keeps the observation a pure read (D14.2).

A reader-side clamp over two separately published cells was this gate's
first draft; it is rejected (`src/timeline.rs` keeps it as an executable
negative control) because `observe()` must stay pure. No "± one block"
freshness bound is claimed either: a pure load returns the latest
published sample, and how old that sample is depends on the reader's own
poll interval and the mechanism's publication cadence — an asynchrony
property, not a correctness invariant. The probe reports the measured
publication cadence instead, and is a mechanism twin in the production
render-loop order (see RESULTS.md §3): it links no production crate, so
its numbers measure the mechanism shape, not production code.

## Running

```bash
# algebra oracles (any platform)
cargo test --manifest-path experiments/f4-timeline-gate/Cargo.toml

# physical probe (Windows host / WSL interop; plays ~12 s of quiet
# 440 Hz tone through the default endpoint). Build the bin explicitly:
# the duration bin is cfg(not(windows)) and does not link on Windows.
cargo build --manifest-path experiments/f4-timeline-gate/Cargo.toml \
    --target x86_64-pc-windows-gnu --bin f4probe
./experiments/f4-timeline-gate/target/x86_64-pc-windows-gnu/debug/f4probe.exe

# duration provenance (any platform with the SongCore native artifact)
cargo run --manifest-path experiments/f4-timeline-gate/Cargo.toml \
    --bin f4duration
```

Whether a displayed number *looks* right is deliberately not claimed
by any executable here — that is the human reviewer's acceptance.
