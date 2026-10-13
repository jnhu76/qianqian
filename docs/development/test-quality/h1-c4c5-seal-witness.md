# H1 C4/C5 seal witness — bounded execution evidence

Non-normative test evidence. Authority remains playback execution-model D3/D5
§8.2 and §9, PBK-001/PBK-002; no authority amendment. Depends on PR #268
HEAD `f27cb5208642ca5c0bd9c56cf17d09f42f0222ba`. Initial main
`0668f798d9de5930b311138a86ccea8d53f404f4`. H-D is outside this work.

## Meaning and source argument

C4: admitted normal output/response -> actual Drop ACK -> final seal read ->
owner reporting. C5 additionally records the first failure before mandatory
response; correct Some(Read) cannot substitute for response/ACK completion.

I1: admit rejects closure and asserts no active operation; one invocation reader,
or synchronous spawn failure when no reader exists. I2: line owns Operation
across dispatch/output, its inner unwind catcher and failure response;
fail_with_response borrows the guard and returns only after response.
I3: only Operation::drop clears active, under the bookkeeping mutex;
recording failure leaves it active. I4: seal closes admission, loops on active,
then reads first_failure under the same mutex. Entry finish_episode calls seal
before final reports/exit. Assumptions: legal invocation use, no forgotten guard,
normal non-poisoned mutex return, owner progress. No callback deadline.

Minimum actual DAG (letters are explanatory events, not runtime states):

```text
ADMIT -> [F record ->] response -> R -> Drop K     PROGRAM_ORDER
K mutex release -> seal mutex acquire           SYNCHRONIZES_WITH
active=false -> L                               STATE_DEPENDENCY
L -> T -> final reporting                       PROGRAM_ORDER
```

## Observation and fixture

One invocation-local cfg(test) SealWitness carries two atomics. The independent
response owner writes its completion marker; seal captures that marker and
operation_active immediately after the actual final result read, while still
holding the bookkeeping mutex. Compare-exchange preserves the first snapshot.
No waits, callbacks, assertions or second mutex occur at capture. Shared's extra
field and the observation call disappear outside cfg(test). The ordinary local
result binding changes no production semantics or representation.

Four separate tests earn four cuts: C4 actual status callback held; C5 actual
fail_with_response(Read) callback held; and each response already returned with
a legitimately admitted guard still owned. Entry channel establishes the held
owner and explicit precondition assertions check active/failure/response state.
Closure observation acquires the same mutex: an illegal no-wait final read in
that critical section necessarily captures before test release. Correct wait
releases the mutex without reading L, and can read L only after actual Drop.
This removes the sealer-return/result-sender preemption window without a send
Oracle. The response marker is written at the callback's final statement; actual
return-before-Drop is still an I2 source obligation. A forged early active-clear
with completed response is outside these two mutants: I1-I3 must remain reviewed.
This is not a universal proof of every possible ACK writer or all schedules.

Snapshot bits: 1=observed, 2=active, 4=response owner completion. Required=5;
missing=0 fails. Mutant callback-held=3; returned-response/held-guard=7.
Assertions occur after worker and sealer actual joins. Release sender drops on
parent unwind, unblocking recv through disconnect; no callback timeout releases
the ownership premise. wait_closed/entry deadlines are failure watchdogs only.
There is no assertion racing try_lock and no sleep/probabilistic RED premise.

## Executed falsification

[Raw initial evidence](evidence/h1-c4c5-initial-evidence.zip) contains full build,
discovery/test logs, source snapshots, patches, toolchain, RUN_ID and source/
patch/compiled executable SHA256 for every lane, plus the replay driver.
Isolated mutation worktree starts at #268 and applies only the witness delta.
Each build removes the headless package artifacts; executable discovery must
find exactly four tests before execution. Actual executable paths come from
Cargo compiler-artifact output, and execution uses those paths directly.

| Lane | C4 held callback | C4 held guard | C5 held callback | C5 held guard |
| --- | --- | --- | --- | --- |
| Correct | GREEN | GREEN | GREEN | GREEN |
| M-S removes active wait | target Oracle RED | target Oracle RED | target Oracle RED | target Oracle RED |
| M-F waits only while active AND failure absent | GREEN | GREEN | target Oracle RED | target Oracle RED |
| Restored | GREEN | GREEN | GREEN | GREEN |

Every RED is the named final-read ordering assertion (snapshot 3 or 7 vs 5),
not a compile error, timeout, fixture panic or deadlock. Correct/restored
machine-input sibling groups each execute 21 tests / 0 failures. This supersedes
the old C4/C5 executable-evidence debt only within these legal paths and source
assumptions. Earlier research/Option A records remain historical evidence.

Initial interactive setup encountered a private-method compile error before
acceptance; visibility was corrected before all four evidence lanes. It is not
counted as semantic RED. Initial OCR connectivity check was unavailable due to
provider usage limit; two fresh-context agent reviews are the requested independent
review channels. Final PR/head/review/integration receipts belong in issue/PR
comments and final integration evidence; initial evidence is not final-main proof.

Linux x86_64 device-free Rust only here. No physical audio, Windows runtime,
SongCore ABI, TLS termination, reader physical-read completion, external stdout
consumption or TLC claim. Final required CI and local gates are separately recorded.
