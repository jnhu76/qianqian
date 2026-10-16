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
loom_l1   3 threads: write([7,9]) × stop × read-to-terminal
          ring: 1 channel × 2 samples. Asserts: FIFO ring integrity under
          every interleaving, EOF unreachable, terminal monotone Stopped,
          consumer exits through the terminal.
loom_l3a  2 threads: close_eof × stop. Asserts first-terminal-wins and
          consumer outcome == final terminal (read distinguishes the
          winner: Eof vs Stopped).
loom_l3b  2 threads: fail × stop. Asserts first-terminal-wins at the
          identity level: the winner (Failed or Stopped) is preserved in
          edge.terminal() under every schedule — a failure is never
          downgraded into a stop state and the two are never merged —
          while the collapsed consumer outcome is Stopped for both.
loom_l4a  pre-filled 1-sample edge; 2 threads × 2 params: blocked
          write([9]) × {stop, fail}. Asserts the producer wakes and
          returns Stopped under every schedule (no lost wakeup, no
          wedge); after a fail the edge terminal is Failed.
loom_l4b  2 threads × 3 params: consumer blocked on empty ×
          {close_eof, stop, fail}. Asserts the consumer wakes with the
          matching outcome; the fail arm reads the collapsed Stopped
          while edge.terminal() stays Failed.
```

Five test functions, eight loom model explorations in total. Full
interleaving exploration per model (loom 0.7.2, release build); the
suite completes in ~30 s wall. Bounds: ≤ 3 threads, ≤ 2 buffered
samples, ≤ 3 operations per thread. Scope note: the models share the
edge through loom's own `Arc`, so std `Arc` refcount drop-orderings are
not part of the modeled slice. A clean run is SCHEDULE-CLEAN within
exactly these bounds.

## Results

| ITEM | RESULT | ENGINE |
|------|--------|--------|
| L1 write×read×stop: FIFO integrity, no wedge | SCHEDULE-CLEAN | loom 0.7.2 |
| L3a first-terminal-wins (EOF × stop) | SCHEDULE-CLEAN | loom 0.7.2 |
| L3b first-terminal-wins (failure × stop), Failed identity preserved | SCHEDULE-CLEAN | loom 0.7.2 |
| L4a blocked producer × {stop, fail} wakes | SCHEDULE-CLEAN | loom 0.7.2 |
| L4b blocked consumer × {EOF, stop, fail} wakes | SCHEDULE-CLEAN | loom 0.7.2 |
| M-L1 (drop `data_ready.notify_all`) | COUNTEREXAMPLE-WITNESSED — loom reports the deadlocked schedule (consumer blocked forever) | mutation → loom |

Negative-control sensitivity: M-L1 removes the notify inside the shared
`set_terminal` path, which serves EOF, failure and stop alike — the
witnessed deadlock schedule therefore exercises the same lost-wakeup
mechanism the new fail arms depend on. No separate failure-path mutation
was added; one mechanism-sensitive control is the honest count, not a
coverage number.

C6 (failure × completion publication authority) and the
`SessionCompletion` resolve decision table are exercised by the native
`session_activation`/`edge_lifecycle` suites under CPU pressure (Phase
B4 stress) and by the Miri pass; they are recorded there, not claimed
as loom results.

## Lane extension — production class-α pool (2026-10-16, PR #287 CI fix)

The error-system implementation landed the production class-α
ownership-handoff pool in `qianqian-diagnostics`, registered from the
playback session (the decode worker's only creation site). That dependency
edge puts the pool crate inside this lane's playback loom build
(`RUSTFLAGS` is global; the playback `loom` feature now carries
`qianqian-diagnostics/loom`), so the lane also runs the pool's own two
schedule explorations over the production type:

```text
pool-loom-base       2 threads (producer × drainer), ≤ 2 records, 3 publish
                     attempts × 2 drains. Asserts exclusivity (each copied
                     record whole), order (no skip/duplicate), the admission
                     bound and both accounting identities — under every
                     explored schedule.
pool-loom-cross-wrap same protocol with counters at u32::MAX-1: slot parity
                     and the modular admission distance are explored through
                     the natural wrap.
```

Duty split (implementation record §2): loom verifies the synchronizes-with
structure of the protocol (which the Release/Acquire loads control); the
`UnsafeCell` payload data race is Miri's duty — the Relaxed-recycle negative
control is red under Miri, not here. The `cfg(not(loom))` arithmetic-oracle
partition of `pool.rs` stays in the ordinary (non-loom) diagnostics suite.
A clean run is SCHEDULE-CLEAN within the stated bounds, not a general proof.
