# #321 — analyst panic × blocked stderr: causal report

```text
STATUS        E2E_CONFIRMED (executed, two independent runs in closeout-3;
              replayable here)
DEFECT CLASS  latent liveness / classification defect
IMPACT        no DisposeVerdict, no episode close, no output-inverse release,
              no process exit code; no data corruption
PRECONDITION  analyst panic (not product-reachable today)
              × blocked/unread diagnostic sink
              × the decode worker's funnel join (current production shape)
FIX           #322: class-α registration at the analyst thread's first statement
```

## 1. Thread and wait DAG

```text
T_main        runtime.dispose() -> K0 dispose_root -> settle()
              -> LIFO relation unwind -> PcmDecode inverse
                 edge.stop(); worker.join()                 session.rs
              -> (later) AudioOutput inverse: release, stop_and_join

T_decode      "qianqian-decode" (class α)
              decode loop -> single exit funnel:
                 worker_exited(terminal)                    D11 Fact committed
                 abort_stranded_seek()
                 observation.close()
                 analyst.join()                             <-- WAIT EDGE

T_analyst     "qianqian-observation"
              catch_unwind(analyst_loop)
                 panic -> production hook
                    baseline: is_alpha_producer() == false
                              -> previous hook -> std default -> raw fd 2 write
                              -> blocks on full/unread pipe BEFORE catch returns
                    fix:      is_alpha_producer() == true
                              -> note_critical_panic(), previous hook NOT called
                              -> unwinds into the catch, thread exits, join returns
```

Program-order edges (the proof does not depend on scheduling):

```text
hook runs synchronously before unwinding reaches catch_unwind
  => previous-hook completion < catch return < analyst thread exit
  < analyst.join() return < decode worker exit < worker.join() return
  < PcmDecode inverse return (Discharge) < later output inverse
  < dispose_root's authoritative DisposeVerdict
```

The saturated pipe, the exact number of 64 KiB chunks, and which thread wins a
scheduling race are observed but not used as proof.

## 2. Executed witness (baseline)

```text
PREVIOUS-HOOK-ENTERED-ANALYST          (no ...RETURNED-ANALYST)
TERMINAL=Completed                    D11 committed BEFORE the wedge
IS-CLOSED=true                        close() returned; funnel is at analyst.join()
DISPOSE-ENTERED                       (no DISPOSE-RETURNED)
PROC comm=qianqian-observ  state=S wchan=anon_pipe_write   hook inside write(2)
PROC comm=qianqian-decode  state=S wchan=futex_do_wait     analyst.join()
PROC comm=qianqian-headle  state=S wchan=futex_do_wait     dispose -> worker.join()
pipe unread=65536                     saturated and unread
```

The independent `gdb` stacks in `raw/08-manual-wedge-gdb-stacks.txt` show the
analyst inside `default_hook`'s fd-2 `write`, the decode worker in
`analyst.join()`, and dispose in `worker.join()`. The kill at the end of the
oracle is fail-closed cleanup, not the proof.

## 3. Controls and falsifiers

| Run | Result | What it isolates |
|---|---|---|
| healthy, no panic | `Discharged`, analyst gone, exit 0 | normal close/join |
| broken stderr (EBADF) + panic | previous hook returns, `Discharged` | chaining and panic alone are not enough; BLOCKING is the trigger |
| saturated stderr, no panic | `Discharged`, `critical_panics=0` | the sink alone is not a trigger |
| blocked stderr + panic, baseline | `Wedged` | the subject |
| fix: analyst registers α (before the protected region) | `Completed` / `Discharged`, `critical_panics=1`, previous hook never entered | the classification correction removes the sink from the join |
| negative control: registration moved AFTER the protected region | `Wedged` | ordering is load-bearing: registration must precede the panic |
| remove the funnel join | `Completed` but analyst still blocked and detached (`analyst_gone=false`) | the join is the sole propagation edge; detaching is not a fix |

The join-decoupling falsifier is recorded in the closeout-3 receipts and is
**rejected**: it trades a teardown wedge for a leaked, permanently blocked thread
and a poisoned process stderr.

## 4. Contract classification

**D11 terminal outcome — not violated.** The terminal Fact is committed before
the funnel reaches the join (`TERMINAL=Completed` observed while dispose is still
blocked). D11's current-realization note already says terminal commit does not
acknowledge decode/render thread termination.

**K0 mandatory teardown / Discharge — violated (liveness).** `composition-kernel-0-design`
§G.6/§H.7 and `fiber.rs` define the relation inverse as a total obligation with
exactly two verdicts (`Discharged` / `Violated`); `dispose_root` returns only after
`settle()`. The `PcmDecode` inverse never returns, so no authoritative verdict
exists and the later LIFO output inverse never runs. ADR-PBK-001 §17.2's OPEN
panic/unwind model does not cover this: no unwind escapes; the block is in the
ordinary teardown path.

**DG-4b isolation contract — violated (classification).** The adopted criterion
is the thread's own class-α registration fact. The analyst's progress is awaited
(the funnel join), but it did not register, so a panic chained to an unknown
synchronous sink before its catch. N-01's unbounded-sink residual is admissible
only on non-class-α threads whose progress no join/completion awaits.

## 5. Fix

```rust
// crates/qianqian-playback/src/observation.rs — first statement of the REAL
// "qianqian-observation" thread closure:
let _diagnostics = qianqian_diagnostics::AlphaRegistration::register();
```

- one registration per fresh thread, created and dropped on that thread;
- no detach, no second lifecycle, no Plugin, no new noun;
- normal path unchanged (cold-path registration, one pending analysis bound);
- the registration is deliberately before the protected region: the hook consults
  the thread fact before `catch_unwind`.

**Failure boundary, stated narrowly.** `register()` is a cold-path allocation plus
registry lock and rejects a duplicate on a fresh thread by contract; it performs no
diagnostic write and cannot block on a sink. A panic *inside registration* is not
α-isolated (the fast path is not yet armed), but it is not sink-dependent. The
registration isolates panics **after** it; it does **not** bound an analyst hang
caused by the analysis path itself. The normal-path join still waits for at most
one pending analysis, not an arbitrary timeout.

## 6. Evidence integrity

```text
closeout-3 baseline  main@98ff343 (original receipts)
replay baseline      main@61d834a (pre-#322; includes #318/#319/#320)
fix tree             3af2f48 + #322 review fixes; the product-tree delta from
                     61d834a is the analyst registration and comments only
harness              evidence/analyst-join-321/harness/analyst_join_e2e_tests.rs
overlay              evidence/analyst-join-321/overlay/000-harness.patch
negative control     evidence/analyst-join-321/overlay/001-negative-control-registration-after-panic.patch
raw receipts         not retained (2026-11-01 evidence diet); the witness
                     lines quoted in this report are the recorded result
replay               evidence/analyst-join-321/replay.sh
rustc                1.97.1
```

## 7. NOT_RUN / limits

- Windows runtime, WASAPI and audible output: NOT_RUN (Linux host).
- Real SongCore/FFmpeg media decode: NOT_RUN; Decode/Output are ports doubles.
- Diagnostics facility/drainer: not installed in the child; the fix changes the
  hook arm, not the drain algorithm.
- Product-reachable analyst panic: none exists today; the injection is evidence-only.
- The harness is not a CI gate; it is a reproducible evidence script. The permanent
  in-tree oracle pins the registration fact without the injection.
