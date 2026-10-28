# TEST-QUALITY — authoring record for the twelve cells

```text
TRUTH CLASS   test-process record required by docs/development/test-quality-standard.md
SUBJECT       #324 Windows native Tier-2 harnesses
```

## Classification (per test, not per file)

```text
M3 (T2)  lifecycle/settlement/timing-sensitive, real threads, real OS sinks
         → six questions + a minimal execution DAG (EXECUTION_DAGS.md)
         + independent falsifier (the twelve mutations)
         + independent fresh-context review (REVIEW.md)
R-H      high risk (lifecycle isolation, real OS handles, producer/terminal
         independence) → the mutation is the required independent falsifier.
```

The twelve cells are grouped because they share one authoring shape; each cell
carries its own premise, producer and mutation (MATRIX.md).

## D1 — coverage disclosure (required by the standard)

```text
CI COVERED?  NO.
```

`qianqian-decode-songcore` is workspace-excluded (it links the separately built
SongCore artifact), and no CI job compiles it. These harnesses are Windows-only
(`#![cfg(windows)]`) evidence tests whose PR commit lives under
`evidence/…/harness/`, not in the product test tree. They are therefore
**executed evidence with an archived receipt**, not a regression gate. If they
are later promoted into CI, that promotion needs its own runner/artifact
decision (#172 territory for the artifact lane).

## Six questions

### PROPERTY

```text
Disposable diagnostics must not control reliable decode results, mandatory
producer progress, terminal settlement, or required teardown.
```

Owning clauses: `docs/architecture/error-system-reconciliation.md` §8 E-7
Tier-2 (the four controls), with DG-1 (α/β outlet) and DG-4b/DG-4c posture in
`docs/architecture/error-disposition-contract.md` §6 / the diagnostics crate
contract. No new architecture authority is claimed; see REVIEW.md pass D.

### PRECONDITION

Every premise is established by a real Windows synchronization seam **before**
the dependent action, never by a sleep:

| Control | Premise | How it is established |
|---|---|---|
| C1 (ffmpeg/read) | the diagnostic sink is blocked | in-process anonymous pipe installed with `SetStdHandle`; read end held and never read; a 64 KiB raw `WriteFile` is parked **and** an independent 1-byte raw write is parked; `PeekNamedPipe` reports the queue |
| C1 (seek) | the sink is blocked | a helper holds the real process-wide `std::io::stderr().lock()` and signals acquisition over an mpsc channel |
| C2 | the sole consumer is stopped inside `export` | `Exporter::export` sends `entered` over an mpsc channel and then waits on a Condvar gate that is never released during the property |
| C3 | the producer's own α pool is saturated | `emit` on the producer thread until `Disposition::AlphaRejected`; `outstanding == 2` observed |
| C4 | writes really fail | canary `write_stderr_line` returns `Err` with `ERROR_NO_DATA (232)` / `ERROR_BROKEN_PIPE (109)` on the process stderr handle before producer work |

### LEGAL OUTCOMES

```text
healthy    producer returns; reliable result unchanged; teardown bounded and
           honest; accounting identities hold
mutant     the ONE mutation makes the forbidden dependency real, so the cell's
           own assertion must fail with its named signature
```

For C2/C3 the "race" (drainer held) is deliberately constructed, not left open.
For C1 the blocked sink is deliberately constructed. No legal outcome is left
undecided by an oracle.

### OBSERVATION

Assertions are read at the owning boundary:

```text
producer result      the adapter's returned DecodeError / ProviderSeekOutcome
                     (the same value the production caller sees)
producer progress    the receipt channel, sent AFTER the last native call and
                     after drop(stream) (= song_close)
pool accounting      AlphaRegistration counters (the pool's own boundary)
delivery/loss        Diagnostics::counters() / ShutdownReport (the facility's
                     boundary; drained/abandoned are its own judgment)
OS sink state        PeekNamedPipe / GetFileType / the canary's raw_os_error
```

Nothing is sampled above its owner: the harness never infers a terminal Fact
from a counter, never infers `drained` from timing, and never reads another
process's counters.

### FALSIFIER

`EXECUTED_MUTATION` for all twelve. One minimal source mutation per cell, applied
by `harness/counterfactual_runner.py`, with the predicted signature in
`MATRIX.md`. Predicted failure signatures are named assertions; the runner
rejects a build error, a zero-test arm, a missing PASS marker mismatch, or an
outer-watchdog kill. Four cells are bounded-liveness assertions (REVIEW.md D-1).

### EXECUTION

```text
platform   Windows 10.0.26300 x86_64, real host
process    one fresh process per cell (facility + av_log callback are
           process-global)
binary     PE x86_64, target x86_64-pc-windows-gnu, sha256 per run in
           raw/healthy.json and raw/counterfactuals.json
bounds     outer watchdog 180 s (never an oracle); in-cell liveness bounds
           10 s producer / 3 s teardown / 10 s seek valve
failure    the child's own assertion; blocked writers die with the process;
           no HANDLE/reader is carried into the next cell
```
