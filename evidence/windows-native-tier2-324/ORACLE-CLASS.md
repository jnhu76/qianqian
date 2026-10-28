# ORACLE-CLASS — the four LIVENESS REDs: what oracle class is achievable

```text
TRUTH CLASS   verification-evidence analysis (about the evidence), NOT architecture authority
SUBJECT       REVIEW.md finding 1 / D-1: ffmpeg-C1, read-C1, read-C2, seek-C1
VERSION       v2 — corrected after a fresh-context adversarial review of v1
              (see REVIEW.md "Delta review of ORACLE-CLASS"). v1's flat claim
              "no program-state oracle exists" was an overclaim: it is retracted
              and replaced by the qualified result below.
SCOPE         analysis only. No cell was re-run, no verdict changed, no source
              or receipt edited.
```

## Question

Review C recorded a strict objection: four of the twelve counterfactual REDs are
produced by an in-test bounded progress deadline rather than by reading an
illegal program state. D-1 suggested a possible redesign ("a producer-side
progress latch independent of the receipt channel") and left the decision to
the owner.

This note answers:

> **What oracle class is achievable for these four cells, under the mutation
> each cell is required to apply?**

## Answer (v2)

```text
No in-process, product-visible state oracle exists for any of the four cells.
That is the claim the package needs, and it holds.

An OUT-OF-PROCESS thread-context oracle does exist in principle for three of
the four: read-C1 and seek-C1 (without mutation awareness) and ffmpeg-C1 (only
mutation-aware). It is a debugger with a hardware execute breakpoint filtered
to the producer thread, waiting on debug events instead of timers. It is NOT
adopted and NOT executed here: it is a new observation mechanism, it changes
the evidence class from program-visible behavior to sampled thread context,
and for ffmpeg-C1 it depends on an address that exists only in the mutant.

read-C2 has no such construction: its property is completion-within-bound, and
the mutant exposes no state that marks deadline expiry.
```

So the four REDs are the strongest oracle available inside the package's
earned mechanisms, not the only oracle that could ever be built. Adopting the
thread-context mechanism would be a new harness/observation scope — an owner
decision, not a documentation fix.

## 1. Property class: state vs progress

The other eight cells assert a **state predicate**: a counter identity
(`attempted = accepted + rejected`), a report field (`drained` / `abandoned` /
`undelivered_at_exit`), or a delivered/lost classification. The mutant reaches
an illegal state and stays there; one state read at a synchronized point is
enough.

The four in question assert a **progress predicate**:

| Cell | Predicate | Task-mandated mutation |
|---|---|---|
| ffmpeg-C1 | the native producer completes while the diagnostic consumer is held | put the blocking sink/consumer wait back on the producer path |
| read-C1 | the decode producer returns while the real sink is blocked | synchronous sink write on the producer path |
| read-C2 | `shutdown` returns inside its bound while the consumer is stopped | remove/disable the bounded shutdown deadline |
| seek-C1 | the seek producer finishes while the sink is blocked | synchronous sink write on the producer path |

The violation is `¬◇event` — non-completion. That is not a program-visible state
at any point; it is the absence of a state change over an unbounded interval.
A finite in-process observation of it can only be "not yet", i.e. a bounded
wait. §4 shows the one class of observation that escapes this: reading the
producer thread's execution context out of process.

## 2. Per-cell: why program-visible state cannot decide the property

### ffmpeg-C1

```text
MUTATION   Pool::publish waits (30 s cap per full attempt) for a free slot
           where the real code rejects and counts;
           base 9f923666… → mutant be6d3976… → restored 9f923666…
HEALTHY    raw/healthy/ffmpeg-C1.log
           QFB-C1-PRODUCER … elapsed_us=3241
           QFB-PREMISE EXPORTER_ENTERED=1 SINK_STILL_BLOCKED=1
           REGISTRATION attempted=4 accepted=2 rejected=2
MUTANT     raw/mutants/ffmpeg-C1-red.log
           PRODUCER-NOT-BACKPRESSURED: the real native producer did not complete
           within 10s while the diagnostic consumer was held      exit 101
```

At the park the mutant is `attempted=3, accepted=2, rejected=0` — the
`attempted` increment precedes the wait (`pool.rs:153-158`), so this is a legal
prefix state the healthy producer also passes through, not a state the healthy
run never shows. After the waits end (two full-pool attempts, each capped at
30 s), the mutant's final counters are `attempted=4 accepted=2 rejected=2`,
identical to the healthy run. No program-visible counter, record, or report
distinguishes the two runs at any quiescent point. Any discriminating oracle
must either outwait the mutant's completion (~60 s) or assert a duration — both
temporal, neither a state read.

### read-C1

```text
MUTATION   emit_decode_fault performs the real write_stderr_line (the same
           function StderrExporter uses) on the producer path;
           base 08eee845… → mutant ff429f56… → restored 08eee845…
SINK       proven-full anonymous pipe:
           SINK_HOLD_ENTERED queued_bytes=65536 flood_outstanding=1
                             second_write_outstanding=1
HEALTHY    C1-PRODUCER_COMPLETED_WHILE_HELD … SINK_STILL_BLOCKED=1 exported=0
MUTANT     C1-PRODUCER-PROGRESS: real decode producer did not return while the
           real sink was blocked … : Timeout                    exit 101
```

The real `StderrExporter` drainer is already parked inside
`write_stderr_line` on the full pipe, and that call holds the process-wide std
stderr `ReentrantMutex` (`lib.rs:1242-1247`). The mutated producer's
`write_stderr_line` therefore blocks acquiring that mutex **before it reaches
the pipe** and copies zero bytes. The receipt keeps the task's required
distinction: the premise is OS pipe backpressure (two parked raw writes); the
mutant parks on the std lock, which the sink blockage keeps held. No counter,
record, or sink content changes while it is parked.

### read-C2

```text
MUTATION   remove the bounded shutdown deadline
           (if false && started.elapsed() >= deadline);
           base 98891d1d… → mutant 413a8348… → restored 98891d1d…
HEALTHY    C2-SHUTDOWN … drained=false abandoned=true, returned inside the bound
MUTANT     C2-TEARDOWN: shutdown did not return inside its bounded deadline:
           Timeout                                               exit 101
```

The consumer is already stopped. To read a report at all, the oracle must first
**release** it — but that release is the oracle's own interference, and a report
that then says `drained=true` is legal for that schedule. Asserting
`abandoned=true` after a release would over-constrain the contract: it would
forbid a slow but successful drain, which the contract permits. The contract's
content is precisely "returns bounded while the consumer is stopped", i.e. a
completion-within-bound.

### seek-C1

```text
MUTATION   synchronous write_stderr_line in the seek producer path, while the
           test owns the process-wide std::io::stderr() lock;
           base 08eee845… → mutant b45981ec… → restored 08eee845…
HEALTHY    C1-PRODUCER … finished_while_held=true sink_still_blocked=true elapsed_ms=2
MUTANT     C1-PRODUCER … finished_while_held=false sink_still_blocked=false
           elapsed_ms=10003   ASSERTION … exit 101
```

`finished_while_held` is a genuine state read, but its value is decided at the
instant the cell's 10 s fail-closed valve releases the lock — an instant chosen
by a bound. Without the valve the mutant parks on the lock forever, which is a
hang. The reviewer's LIVENESS classification is correct and is kept unchanged.

## 3. The degenerate in-process substitutes

Every in-process attempt to remove the bound lands in one of these:

| Shape | Example | Why it is invalid |
|---|---|---|
| outwait the mutant's completion | ffmpeg-C1: wait past both full-pool waits (≥60 s), then compare counters | temporal, and the comparison is only meaningful after the waits; it also depends on the mutant's cap |
| duration threshold | read-C1/seek-C1: assert `elapsed_us < 5 s` | a bounded-time predicate, not a state read; for seek-C1 the measured value is the valve's own 10 s |
| release the fault, then assert the post-release state | read-C2: release the consumer, assert `abandoned` | the asserted state is legal under the interference schedule; for read-C1 the distinguishing bytes are the mutation's added operation |
| sample state at a fixed instant, no wait | read-C1: poll counters once | races the healthy path and yields false REDs; the observation point itself becomes the oracle |

Each is strictly weaker evidence than a named, bounded, in-child assertion: it
either proves less than the property or depends on the mutation's internals.

## 4. The thread-context oracle that does exist (not adopted, not executed)

A genuine non-temporal oracle exists for three cells as an **out-of-process
monitor**. Sketched by the v1 adversarial review; recorded here as an option,
not as evidence:

```text
MECHANISM  spawn the child under DEBUG_ONLY_THIS_PROCESS; resolve
           write_stderr_line (and the mutex wait) from the child's symbols;
           set a HARDWARE execute breakpoint (Dr0–Dr3 via SetThreadContext,
           so no code byte is modified) filtered to the producer thread's ID;
           wait on debug events only — no timers.
ASSERTION  the producer thread reaches write_stderr_line and its context is
           inside the stderr-mutex wait while the test-held sink is blocked.
EVENTS     {producer write-path breakpoint, producer thread EXIT}
FIRES      mutant: breakpoint (RED); healthy: only EXIT (PASS), because the
           healthy producer never calls write_stderr_line — the thread filter
           is mandatory, since the drainer does.
TERMINATION one of the two events occurs in each run; no outer watchdog needed.
```

Per cell:

```text
read-C1      construction applies without mutation awareness
seek-C1      same construction; the drainer also calls write_stderr_line, so the
             producer thread-ID filter is mandatory
ffmpeg-C1    the mutant spins in userspace (no wait-function event), so the
             breakpoint must target the mutant's own publish wait-loop address —
             mutation-aware, which is the construction's weakness
read-C2      no construction: the property is completion-within-bound, and the
             mutant exposes no state that marks deadline expiry; a debugger stack
             sample cannot distinguish "polling inside the bound" from
             "polling after it" without a clock
```

Why it is not adopted:

- it is a new observation mechanism (debug privilege, symbolization, hardware
  breakpoint accounting) whose own premise needs independent proof;
- it changes the evidence class from program-visible behavior to sampled thread
  execution context;
- the ffmpeg-C1 variant is mutation-aware;
- none of it is executed here. Building it would be a new harness scope and a
  fresh run — a mechanism/scope decision, not a documentation fix.

## 5. Task-wording check

Both quoted constraints are from the #324 campaign brief, not repository text
(the issue body does not contain them); they are cited as the task statement.

The task forbids "仅外层 timeout" — the outer cleanup watchdog — and separately
requires bounded release/cleanup of the fault sink after the evidence is
captured. All 36 arms satisfy both:

```text
timed_out=false                    36 / 36  (12 healthy + 12 mutant + 12 restored)
run_cell.ps1 180 s watchdog        never fired
mutant receipts                    running 1 test / named assertion / exit 101 /
                                   test result: FAILED / no PASS marker / no build error
fault-sink cleanup                 seek-C1 10 s fail-closed valve; parked pipe writes
                                   die with the fresh child; no leaked process
```

The bounded waits are the cells' own predicates and are owned by the fresh child
process; they are not the parent's cleanup watchdog.

## 6. Effect on verdicts

```text
VERDICTS          unchanged: 12/12 cells, healthy PASS / PRECISE_RED / restored GREEN
REVIEW C          unchanged: PASS_WITH_EXPLICIT_GAP (strict reading: FAIL)
D-1 STATUS        answered precisely: no in-process product-visible state oracle
                  exists for the four; an out-of-process thread-context oracle
                  exists in principle for three (one mutation-aware), none for
                  read-C2; it is not adopted here
OWNER DECISION    class acceptance ("is a named, bounded, in-child progress
                  assertion acceptable as a PRECISE_RED for C1/C2?") — or
                  authorization to build the thread-context harness as a new
                  scope with a fresh run
```

## Corrections applied in v2

The v1 adversarial review corrected six factual statements; v2 incorporates all
of them:

```text
1  "only finite oracle class"           → no in-process product-visible state
                                          oracle; the thread-context option exists
2  ffmpeg-C1 park state                 → attempted=3 accepted=2 rejected=0 (a legal
                                          prefix), final counters equal healthy
3  ffmpeg-C1 "fitted to 30 s"           → two waits ≈60 s; any discriminating
                                          oracle must outwait completion (temporal)
4  read-C1 "copies zero bytes at pipe"  → the producer blocks on the std stderr
                                          mutex before reaching the pipe
5  read-C2 "must first stop the consumer"→ the consumer is already stopped; the
                                          oracle must release it
6  §3 "wait 40 s" example               → must exceed both 30 s waits (≥60 s)
```

Attribution: the corrections came from a fresh-context adversarial review that
did not write v1; the record is in REVIEW.md.
