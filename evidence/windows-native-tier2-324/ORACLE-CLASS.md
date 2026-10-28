# ORACLE-CLASS — the four LIVENESS REDs, and why no program-state oracle replaces them

```text
TRUTH CLASS   verification-evidence analysis (about the evidence), NOT architecture authority
SUBJECT       REVIEW.md finding 1 / D-1: ffmpeg-C1, read-C1, read-C2, seek-C1
SCOPE         analysis only. No cell was re-run, no verdict changed, no source
              or receipt edited. This note answers a question D-1 left open; it
              does not re-verdict Review C.
```

## Question

Review C recorded a strict objection: four of the twelve counterfactual REDs are
produced by an in-test bounded progress deadline rather than by reading an
illegal program state. D-1 suggested a possible redesign ("a producer-side
progress latch independent of the receipt channel") and left the decision to
the owner.

This note answers the prior question:

> **Does a program-state oracle exist for these four cells, under the mutation
> each cell is required to apply?**

Answer: **no** — not without changing the mutation away from the task's
mandated fault, over-constraining the oracle, coupling it to the mutation's own
arbitrary constant, or introducing an unearned OS-thread-introspection
mechanism. The four REDs are not an unresolved oracle defect; they are the only
finite oracle class for a progress property.

## 1. Property class: state vs progress

The other eight cells assert a **state predicate**: a counter identity
(`attempted = accepted + rejected`), a report field (`drained` / `abandoned` /
`undelivered_at_exit`), or a delivered/lost classification. The mutant reaches
an illegal state and stays there; the assertion reads it. A single state read at
a synchronized point is sufficient.

The four in question assert a **progress predicate**:

| Cell | Predicate | Task-mandated mutation |
|---|---|---|
| ffmpeg-C1 | the native producer completes while the diagnostic consumer is held | put the blocking sink/consumer wait back on the producer path |
| read-C1 | the decode producer returns while the real sink is blocked | synchronous sink write on the producer path |
| read-C2 | `shutdown` returns inside its bound while the consumer is stopped | remove/disable the bounded shutdown deadline |
| seek-C1 | the seek producer finishes while the sink is blocked | synchronous sink write on the producer path |

For each, the healthy receipt shows that the event occurred *while the fault was
in force*; the mutant receipt shows that it did not. "Occurred" is an event, not
a state: `¬◇event` is not a program state at any point — it is the *absence* of
a state change over an unbounded interval. Any finite test of `¬◇event` must
either stop waiting at a bound (a liveness assertion) or never stop (a hang,
which the task forbids as evidence).

## 2. Per-cell: why a program-state read cannot decide the property

### ffmpeg-C1

```text
MUTATION   Pool::publish waits (30 s cap) for a free slot where the real code
           rejects and counts; base 9f923666… → mutant be6d3976… → restored 9f923666…
HEALTHY    raw/healthy/ffmpeg-C1.log
           QFB-C1-PRODUCER … elapsed_us=3241
           QFB-PREMISE EXPORTER_ENTERED=1 SINK_STILL_BLOCKED=1
           REGISTRATION attempted=4 accepted=2 rejected=2
MUTANT     raw/mutants/ffmpeg-C1-red.log
           PRODUCER-NOT-BACKPRESSURED: the real native producer did not complete
           within 10s while the diagnostic consumer was held      exit 101
```

Until the producer's wait ends, every program-visible counter equals the healthy
run's value at the same point (`attempted=4 accepted=2`); the only difference is
that the producer is parked. After the wait ends there are two worlds:

1. still full → the mutant rejects, and the final counters are *identical* to
   the healthy run — no state distinguishes them;
2. the oracle releases the sink → the mutant publishes, and the counters
   differ — but they differ because **the oracle released the fault**, not
   because the product violated isolation.

Reaching either world deterministically requires the oracle to outwait the
mutant's own 30 s constant, i.e. an oracle fitted to the mutation's number, not
a property test. No program state separates the two runs at the point of
violation.

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

The mutant's write copies **zero bytes** (the pipe has no space), so no
counter, record, or sink content changes while it is parked. After a release the
pipe contains the extra line — but that line is the mutation's *added
operation*, not its blocking; asserting it would pass/fail on "a write statement
exists", which the mutation adds by construction. The only faithful falsifier of
"the producer must not synchronously write a blocked sink" is that the producer
does not return while the sink is blocked.

### read-C2

```text
MUTATION   remove the bounded shutdown deadline
           (if false && started.elapsed() >= deadline);
           base 98891d1d… → mutant 413a8348… → restored 98891d1d…
HEALTHY    C2-SHUTDOWN … drained=false abandoned=true, returned inside the bound
MUTANT     C2-TEARDOWN: shutdown did not return inside its bounded deadline:
           Timeout                                               exit 101
```

To read *any* report the oracle must first stop the consumer — but stopping the
consumer is the oracle's own interference, and a report that then says
`drained=true` is legal for that schedule. Asserting `abandoned=true` after a
release would over-constrain the contract: it would forbid a slow but successful
drain, which the contract permits. The contract's content is precisely "returns
bounded while the consumer is stopped", i.e. a completion-within-bound.

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
hang. The reviewer's LIVENESS classification is therefore correct and is kept
unchanged.

## 3. The three degenerate state oracles

Every attempt to remove the bound lands in one of these:

| Shape | Example | Why it is invalid |
|---|---|---|
| outwait the mutant's own constant | ffmpeg-C1: wait 40 s, then compare counters | the oracle is fitted to the mutation's 30 s cap, not to the property; a different mutation defeats it |
| release the fault, then assert the post-release state | read-C2: release the consumer, assert `abandoned` | the asserted state is legal under the interference schedule; for read-C1 the distinguishing bytes are the mutation's added operation |
| sample state at a fixed instant, no wait | read-C1: poll counters once | races the healthy path and yields false REDs; the observation point itself becomes the oracle |

Each is strictly weaker evidence than a named, bounded, in-child assertion: it
either proves less than the property or depends on the mutation's internals.

## 4. The one non-temporal alternative, and why it was not adopted

A real state oracle exists in principle: suspend the producer thread at a chosen
point and read its instruction pointer/stack to prove it is parked inside
`NtWriteFile` (or the std mutex wait) while the sink is blocked. That observes a
state — the thread execution context — rather than a deadline.

It was not adopted because:

- it is a different observation mechanism (thread suspension, module-range
  resolution, IP symbolization) whose own premise needs independent proof;
- suspension itself perturbs the schedule it observes, and a single sample can
  miss the window, so it reintroduces either sampling loops (temporal again) or
  false negatives;
- the campaign does not need it to establish the claimed property, and adding a
  new low-level observation mechanism is not earned by this test-only evidence
  package.

It is recorded as an option, not implemented and not counted as evidence.

## 5. Task-wording check

The task forbids "仅外层 timeout" — the outer cleanup watchdog — and separately
requires "先设计可证明触发的同步控制，并允许在捕获断言所需的证据后有界释放或清理故障 sink".
Both are satisfied across all 36 arms:

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
D-1 STATUS        the "possible redesign" is now answered: no program-state oracle
                  exists for these four mutations without one of the degeneracies
                  in §3 or the unearned mechanism in §4
OWNER DECISION    remains, but its content is now a class-acceptance decision:
                  "is a named, bounded, in-child progress assertion acceptable as a
                  PRECISE_RED for C1/C2?" — not "is a missing state oracle unbuilt?"
```

This note adds no architecture noun, no product change, and no new claim. It is
analysis of receipts already in the package.
