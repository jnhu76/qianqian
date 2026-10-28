# EXECUTION_DAGS — happens-before structure per cell

```text
TRUTH CLASS   execution-shape record (evidence, not architecture)
SUBJECT       #324 Windows native Tier-2 campaign
```

Legend:

```text
SW    a synchronizes-with edge the oracle is allowed to rely on
      (mpsc send/recv, Condvar signal, SeqCst store→load, an OS write that
      has not returned while the child keeps making progress)
SCHED a scheduling observation. Used ONLY as a fail-closed watchdog
      ("fail instead of hanging"), never as the property oracle.
LIVE  a bounded-liveness assertion INSIDE the cell: the property under test is
      "the producer/teardown completes while the consumer is held", so the
      cell asserts a named deadline inside the child (10 s producer, 3 s
      teardown, 10 s seek lock valve). This is the cell's predicate, not the
      outer watchdog; it exits 101 with a named assertion.
```

Every cell is ONE fresh process. The runner is not in any happens-before path
of an assertion: it owns only process spawn, the independent stderr pipe, the
outer timeout and the receipt.

## Common parent/child DAG

```text
parent (run_cell.ps1)                       child (test executable, fresh process)
─────────────────────                       ────────────────────────────────────
Process.Start(child) ───────────────────▶  process start
  stderr = own anonymous pipe                fresh process-global facility slot
  ignore -> never read, poll PeekNamedPipe   fresh av_log callback state
  drain  -> read concurrently                 │
                                              ├─ premise establishment (SW below)
                                              ├─ Diagnostics::install (one per process)
                                              ├─ producer thread: AlphaRegistration
                                              │   (class-α, its own two-slot pool)
                                              ├─ real native producer call
                                              │   FFmpeg av_log | song_read_pcm | song_seek
                                              ├─ producer returns AND stream dropped
                                              │   (drop = song_close)      ◀── SW: receipt
                                              ├─ assertions on counters/flags
                                              ├─ bounded shutdown + report assertions
                                              └─ process exit (parked writers die with it)
wait/exit + receipt only  ◀──────────────────  receipts on stdout (never the SUT stderr)
```

Why the receipt is a SW edge: the producer thread sends it **after** its last
native call and after `drop(stream)`, so receiving it proves the producer
returned and its mandatory release completed. The bounded `recv_timeout` is a
watchdog; on the counterfactual it fails the assertion instead of hanging.

## C1 BLOCKED_SINK

| Arm | Premise (SW) | Producer observation | Teardown |
|---|---|---|---|
| ffmpeg-bridge / decode-read | `CreatePipe` → `SetStdHandle(2,write)`; read end held, never read. A 64 KiB raw `WriteFile` is issued and **cannot complete** with no reader (measured: a 4 KiB write completes, 64 KiB parks); an independent 1-byte raw write also cannot complete. `PeekNamedPipe` reports the queue. | receipt received while `exported==0 && export_failed==0` and both raw writers are still parked ⇒ the drainer entered export and neither succeeded nor failed | `shutdown(50 ms)` → `abandoned && !drained` |
| decode-seek | real process-wide `std::io::stderr().lock()` held by a helper (verified cross-thread on Windows); helper signals acquisition (SW) and waits on a Condvar for `producer_done` | `finished_while_held = !released` read immediately after the last native call; `sink_still_blocked = in_export` | helper released only after those reads; `shutdown(5 s)` → `drained` |

The two constructions are deliberately different and are **not** reported as
each other: the seek arm blocks on the std lock, the other two arms block on OS
pipe backpressure.

## C2 STOPPED_CONSUMER

```text
child: Exporter::export(seed)  ──SW (entered.send)──▶  cell's wait_stopped()
       │  then waits on a Condvar gate that is never released
       ├ producer thread runs the real native fault to completion
       ├ receipt received                       ◀── SW
       ├ assertions: exported unchanged, admission identity, pool bound
       └ shutdown(50 ms) on a worker thread; report received with a bounded
         recv_timeout                        ◀── SW (report channel)
         report must be abandoned && !drained
```

The `drained`/`abandoned` fields are the facility's own judgment; the cell does
not infer them from timing.

## C3 CAPACITY_EXHAUSTION

```text
child: drainer stopped in the exporter (same seam as C2)
       ├ ffmpeg-bridge: producer thread fills its OWN pool first (bounded loop
       │   of ≤3 `emit`s; the first rejection is the two-slot bound witness),
       │   then the real av_log + fault presentations hit a full pool.
       │   target_attempts > 0, target_accepted == 0,
       │   target_rejected == target_attempts
       ├ decode-read: real fault path presents ≥4 records into a 2-slot pool;
       │   accepted == 2, rejected == attempted - accepted, outstanding == 2
       └ decode-seek: fill until first rejection, then the real seek fault;
           rejected_after > rejected_before, accepted unchanged, outstanding ≤ 2
```

No allocator instrumentation is claimed: the evidence is structural (two
preallocated slots, one bounded publish) plus the observed `outstanding ≤ 2`.

## C4 WRITE_FAILURE

| Arm | Premise (SW) | Producer/export observation | Teardown |
|---|---|---|---|
| ffmpeg-bridge / decode-read | `CreatePipe` + `CloseHandle(read)` + `SetStdHandle(2,write)`; canary `write_stderr_line` returns `Err` with `ERROR_NO_DATA (232)` **before** any producer work | real exporter write fails ⇒ `exported==0`, `export_failed == copied_out+beta_consumed`, `self_diagnostics==1`; producer receipt unchanged | `shutdown(5 s)` → `drained`, `undelivered==0` |
| decode-seek | same broken pipe, installed between a healthy real-write phase and the failing phase; canary proves the failure on the handle the real exporter uses | healthy phase delivers the typed `(110, AVERROR_INVALIDDATA)` record; failing phase delivers nothing and the producer's two reliable results are byte-identical to the healthy phase | `shutdown(5 s)` → `drained`, identity holds |

## Counterfactual arms (one mutation per cell)

```text
base GREEN  ──▶ MUTATION_APPLIED (file sha changes) ──▶ build (no error)
            ──▶ MUTANT EXECUTED (running 1 test) ──▶ EXACT ASSERTION RED
                (ASSERTION + named signature, exit 101, no PASS marker,
                 not timed out)
            ──▶ BYTE RESTORATION (sha equals the pre-mutation sha)
            ──▶ RESTORED GREEN (same cell, exit 0, PASS marker)
```

The outer watchdog is not part of any RED: `timed_out=True` invalidates the
arm, and the runner rejects it (`not_timed_out` must be true). Four cells use a
LIVE assertion instead of a STATE read (ffmpeg-C1, read-C1, read-C2, seek-C1);
their RED is still the child's own named assertion, never a kill.

seek-C1's helper release is a fail-closed valve: the helper waits for
`producer_done` **with a 10 s timeout** and then releases the lock even if the
producer never finished. That is what lets the mutant terminate and observe the
illegal state (`finished_while_held=false`: the producer returned only after the
release). On the healthy path the release happens after the producer-side reads,
so `finished_while_held=true` and `sink_still_blocked=true` are read while the
lock is still held.
