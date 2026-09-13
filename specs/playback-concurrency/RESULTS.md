# FV-CONC-0 RESULTS — playback concurrency (Loom)

> STATUS: EVIDENCE (campaign QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B2).
> Result vocabulary per issue #124. SCHEDULE-CLEAN means: no bad schedule
> exists within the modeled slice and bounds. Not a general proof.

Branch: `verification/fv-conc-0`.

## B2.1 — Where does concurrency actually begin?

Extracted from production code (not invented):

```text
PcmEdge (edge.rs)        1 Mutex<EdgeState> + 2 Condvars; producer
                         (decode worker) × consumer (render leg) — THE
                         interleaving core.
SessionCompletion        Mutex + Condvar; decode worker publishes, App waits.
DrainSignal (audio-api)  Mutex + Condvar; render thread publishes once.
session.rs               thread spawn/join + edge stop in the LIFO inverses.
```

Collision inventory (all real, none fabricated): C1 stop × producer
write, C2 stop × consumer read, C3 stop × EOF, C4 stop × decoder
failure, C5 EOF × drain, C6 failure × completion publication, C7 dispose
× natural worker completion, C8 producer exit × consumer blocked.

## B2.2 — Abstraction discipline

The loom model checker explores the **real `PcmEdge` code**: the only
production change is a `cfg(loom)` selection of `Mutex`/`Condvar`
(`loom`'s types are drop-in and preserve the synchronization
semantics). No test copy of the edge exists. Outside a loom build the
file is semantically unchanged; normal `cargo test` never compiles the
loom path.

Explicit layering choice: `SessionCompletion::wait()` uses
`Condvar::wait_timeout`, which loom does not model. Forcing it under
loom would require changing production synchronization semantics —
that fails the campaign's TOOLING-NOT-EARNED bar. The completion
races are instead covered natively (see below) and under Miri.

## Models and bounds

`crates/qianqian-playback/tests/loom_edge.rs` (whole file `cfg(loom)`):

```text
loom_l1  3 threads: write([7,9]) × stop × read-to-terminal
         ring: 1 channel × 2 samples. Asserts: FIFO ring integrity under
         every interleaving, EOF unreachable, terminal monotone Stopped,
         consumer exits through the terminal.
loom_l3  2 threads: close_eof × stop. Asserts first-terminal-wins and
         consumer outcome == final terminal.
loom_l4a pre-filled 1-sample edge; 2 threads: blocked write([9]) × stop.
         Asserts the producer wakes and returns Stopped under every
         schedule (no lost wakeup, no wedge).
loom_l4b 2 threads × 2 params: consumer blocked on empty × {close_eof,
         stop}. Asserts the consumer wakes with the matching outcome.
```

Full interleaving exploration per model (loom 0.7.2, release build);
the suite completes in ~28 s wall. Bounds: ≤ 3 threads, ≤ 2 buffered
samples, ≤ 3 operations per thread. A clean run is SCHEDULE-CLEAN
within exactly these bounds.

## Results

| ITEM | RESULT | ENGINE |
|------|--------|--------|
| L1 write×read×stop: FIFO integrity, no wedge | SCHEDULE-CLEAN | loom 0.7.2 |
| L3 first-terminal-wins (EOF × stop) | SCHEDULE-CLEAN | loom 0.7.2 |
| L4a blocked producer × stop wakes | SCHEDULE-CLEAN | loom 0.7.2 |
| L4b blocked consumer × {EOF, stop} wakes | SCHEDULE-CLEAN | loom 0.7.2 |
| M-L1 (drop `data_ready.notify_all`) | COUNTEREXAMPLE-WITNESSED — loom reports the deadlocked schedule (consumer blocked forever) | mutation → loom |

C4/C6 (failure × completion publication authority) and the
`SessionCompletion` resolve decision table are exercised by the native
`session_activation`/`edge_lifecycle` suites under CPU pressure (Phase
B4 stress) and by the Miri pass; they are recorded there, not claimed
as loom results.
