# experiments/f5-seek-discontinuity — F5-GATE mechanism evidence

Executable mechanism evidence for the Phase-F **F5 Seek discontinuity
gate** (Issue #119 F5 SEEK DISCONTINUITY PROTOCOL REV.3; proposed
ADR-PBK-002 §20 D14.5 amendment). This crate is **not production
architecture**: it is workspace-excluded, imports no `qianqian-*`
production crate, and nothing here ships in the product path. The
frozen contract lands in ADR-PBK-002; the product implementation lands
in a later F5 slice, strictly behind the human merge of this gate.

## What is here

```text
src/bin/f5seek.rs      E1 — decoder seek reality over the SongCore ABI
                       (non-Windows): requested vs reported landing vs
                       content-matched first retained PCM on the
                       committed corpus; seek latency; failure paths
                       (negative target, beyond-duration, EOF tail,
                       back-to-back, seek-before-read, post-failure
                       usability, format stability)
src/bin/f5edge.rs      E2 — edge-cut protocol evidence (all platforms):
                       a faithful copy of the PcmEdge synchronization
                       shape plus the candidate non-terminal invalidate
                       primitive, driven through the frozen
                       seek-discontinuity protocol (refusal-first, the
                       worker's own single purge, park-acknowledged
                       commit) with a stale-output oracle; randomized
                       scenarios + blocked-producer
                       (T4/T5) shape + rogue-staging negative control
src/bin/f5cut.rs       E3 — WASAPI physical cutover probe (Windows
                       only): Experiment A = park → drain-to-zero →
                       commit → refill (the candidate output-side cut)
                       with padding/position traces; Experiment B =
                       Stop/Reset/Start comparison record (the rejected
                       alternative, with its measured state consequences)
evidence/              raw runs (3 × each probe); f5seek-run1.log is the
                       superseded probe version (24 artifact "failures"
                       = probe rounding/window bugs, kept as raw record)
RESULTS.md             the gate report: production inventory, stale-PCM
                       map, decision table, policies, razor review,
                       adversarial review, authority proposal status
```

The formal companion is `specs/f5-seek-discontinuity/`
(safety model of the cutover protocol; base PASS + 5 mutations RED).

## Rule

Production code delta of the F5 gate is zero. Every mechanism here is
experiment-local. Do not import any of it from a product crate.
