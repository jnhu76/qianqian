# Logging / diagnostics closeout, #317 / #290 / #296

Truth class: execution evidence and routing, not architecture authority.
Base: `98ff343467b66890a979edd54d21e14f0275c574` (current origin/main at fetch).
Run: 2026-10-27 UTC through 2026-10-28 Asia/Shanghai. Native Windows 11
10.0.26300 x86_64, `x86_64-pc-windows-msvc`, rustc/cargo 1.96.0
(ac68faa20, LLVM 22.1.2); default eRender endpoint "扬声器 (Realtek(R)
Audio)", status OK, shared-mode. The raw environment JSON was removed by the
2026-11-01 evidence diet; these identities are the recorded values.

`PRODUCTION_BEHAVIOR_CHANGE=NONE`. Test-only observation/prefill seams and
executable evidence only. No D11/D14/K0/ABI changes. Temporary production
mutants are byte-restored; they are not delivered product changes.

## Baseline and #290 accounting

PR #293 is merged at `fc81b68b9ebd1cf7624b217ad04872d12b3e0395`, head
`d7d9f2e39bb3723c365c54a264df4e76c895a401`. PR #294 is merged at
`798175f10d68204e5f89c788020e074d8643d9c4`, head
`4c4cd69bbedffcdde7b21e31beba0ac623ed1f5c`. Both merge commits are ancestors
of the base. Issue/PR snapshots were consulted during the run and are not
retained (2026-11-01 evidence diet); their historical body prose is not
treated as current PR state.

The current #290 accounting update is
[comment 6446817440](https://github.com/jnhu76/qianqian/issues/290#issuecomment-6446817440).
The existing implementation record §14.8 owns the evidence ledger; decision
record §7.0 records the owner ruling. This report does not promote either
record to a playback authority.

- E-9 B1/B2, B4/B5 and E-8 B6 were delivered by #293. B3 is
  `INJECTED_WRITER_ONLY`, based on v2 committed `e9_flush_probe_*`; withdrawn
  v1 PA/interleave observations are not evidence.
- The B2 child-aware watchdog, broken-sink canary and fork/exec spawn-window
  lock are merged. This round does not repeat those accepted Tier-1 / E-9
  campaigns as new Tier-2 evidence.
- `RATIFY_OPTION_B_SCOPED` applies only to the current print!+flush writer,
  newline-terminated single-writer rows, the verified std/LineWriter
  implementation and its defined environments. It is not a universal
  impossibility claim for flush errors.
- Early #293 raw logs were lost, as disclosed in implementation record
  §14.8.8. Committed oracles and GitHub-held CI receipts survive. Interop
  values explicitly labelled transcription-only remain transcription-only.
- Native Windows E-9 broken-sink reader-under-playback, Windows E-8 broken
  report sites, terminal-sink probe, and all other §14.7/§14.8.6 unexecuted
  lanes remain **NOT_RUN**. Windows compilation does not discharge them.
- T1 non-newline production tails, T2 writer/closure/buffering changes,
  T3 Stdout/LineWriter contract change, or T4 flush-error evidence from a
  native Windows / real-terminal NOT_RUN lane each reopens D-D (§7.4).

Recommendation: owner may close the completed scoped accounting/decision
checkpoint while retaining the NOT_RUN register and T1–T4. This run does not
close #290 or extend its ruling.

## R0: actual outlet inventory

Source paths below are repository-relative. Exact executing test binaries
are hashed in each raw row. No fabricated site-tag emitter is used as a
substitute for WASAPI production sites.

| Outlet | Actual producer / admission / gate | Consumer and lifetime owner | Evidence scope |
|---|---|---|---|
| WASAPI #8 abort | render α; `wasapi::emit_render_diagnostic(Error)` after real stop/complete cone; own two-slot pool | host diagnostics drainer/exporter; render registration retires at leg exit; host closes facility | Real Windows endpoint, scripted decode provider, real Session/edge/cone/dispose; injected render failure, no rescue Stop |
| WASAPI #9 opened | same render α pool; QIANQIAN_AUDIO_LOG; Info | same | Real backend open, actual emission must return before intentional Stop |
| WASAPI #10 volume | same render α pool; audio gate + once-per-episode latch; Warn | same | Actual volume-apply seam returns recoverable E_INVALIDARG, actual Warn emission must return before Stop |
| panic #11 critical α | actual headless startup hook → note_critical_panic; count + first-panic pool publication, no β send | same drainer; registration owner is panicking thread | Actual installed production hook, two caught panics, thread join; no new standalone D11 claim |
| panic #11 non-α | actual hook synchronously chains captured previous hook; no queue | arbitrary previous hook owner, not facility | Controlled predecessor enters and cannot unwind until explicit release; known residual |
| FFmpeg #12 unset default | native per-open discard callback, no record admission | SongCore callback installation; process global | Source inspection only this round; runtime installation NOT_RUN |
| FFmpeg #12 opt-in bridge | real av_log callback; β-shaped bounded payload but registered α pool ONLY; unregistered drop + BRIDGE_DROPPED | decode activation installs once; shared host drainer; no per-handle callback owner | Native runtime NOT_RUN; no invented bridge β queue |
| decode read/seek fault | actual emit_decode_fault at adapter read/seek error arms; worker α registration in Session; ordinary emit would use β only for an unregistered caller | worker/Session resources; host diagnostic consumer | Native adapter runtime NOT_RUN; returned DecodeError is a separate Class-A result |
| exporter disable self-report | deliver Err → self_diagnostics++ → synchronous write_stderr_line on drainer; result deliberately dropped | existing drainer; no separate queue/worker | Forced exporter error drives real self-report, with real stderr controls; host shutdown boundary |
| exporter flush self-report | flush_exporter Err → self_diagnostics++ → synchronous write_stderr_line on drainer | existing drainer; no queue | Real final-sweep flush error trigger, separately controlled; not inferred from disable branch |
| host exit report | entry::attempt_exit_report → detached writer → write_stderr_line; optional bounded wait | host invokes; writer is never joined in production | Actual function plus cfg(test) entry/write-result witnesses; no facility-drainer edge |

No other production `emit` caller was found in the Rust source inventory.
Ordinary β transport exists in the facility, but is not a new independently
named product outlet. Test seed/fill records only control that existing
transport and are not counted as product sites.

Excluded: machine stdout/stderr protocol responses; main-thread activation,
outcome, fail-stop, CLI and reference-player reports; returned typed errors;
test/tool/build outputs. These retain their E-8/E-9 / reliable-result
obligations. A string containing "diagnostic" does not make it disposable.

## R1: final per-outlet result matrix

C1 = BLOCKED_SINK; C2 = STOPPED_CONSUMER; C3 = CAPACITY_EXHAUSTION;
C4 = WRITE_FAILURE. PASS is scoped to the controlled boundary named below,
not all device, scheduling or platform behavior. The matrix below is the
retained cell ledger; the per-cell logs were removed by the 2026-11-01
evidence diet.

| Outlet | C1 | C2 | C3 | C4 |
|---|---|---|---|---|
| WASAPI #8 | PASS | PASS | PASS | PASS |
| WASAPI #9 | PASS | PASS | PASS | PASS |
| WASAPI #10 | PASS | PASS | PASS | PASS |
| panic critical α | PASS | PASS | PASS | PASS |
| panic non-α previous chain | KNOWN_RESIDUAL | KNOWN_RESIDUAL | NOT_APPLICABLE | KNOWN_RESIDUAL |
| FFmpeg unset discard | NOT_APPLICABLE | NOT_APPLICABLE | NOT_APPLICABLE | NOT_APPLICABLE |
| FFmpeg opt-in bridge | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |
| decode read fault | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |
| decode seek fault | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |
| exporter disable self-report | PASS* | NOT_APPLICABLE | NOT_APPLICABLE | PASS* |
| exporter flush self-report | PASS* | NOT_APPLICABLE | NOT_APPLICABLE | PASS* |
| host exit report | PASS* | NOT_APPLICABLE | NOT_APPLICABLE | PASS* |

`*` Host progress / bounded honest shutdown, not completion of the blocked
diagnostic writer. The real self-report remains a synchronous drainer
residual; the detached exit writer may remain alive until process exit.
Neither result promotes those consumers to nonblocking workers.

N/A arguments: discard calls no sink and has no queue, conditional on the
unset gate and correct installation; its installation runtime is separately
NOT_RUN. Non-α previous hook has no α slot/β queue. Self-reports and detached
exit report have no independent application consumer/drainer admission and
no queue to exhaust: external stderr backpressure is C1, not an invented
C2 consumer. No fake β-full row is used for these direct writes.

The non-α C2 residual is a controlled predecessor gate witness. C1 proves entry to the predecessor wrapper under the held stderr lock; inherited default invocation is next in program order, but its actual call entry is not independently observed and is not claimed. C4 executes the real inherited default hook on corroborated broken stderr and joins the panicking thread. That concrete predecessor returned, but arbitrary predecessor behavior remains a KNOWN_RESIDUAL, not an alpha isolation PASS.

## Controls, causal edges and observations

Given/When/Then for the four alpha outlets:

- C1: helper obtains **real std stderr lock**, sends acquired through mpsc,
  then waits on an unreleasable test-owned Condvar while holding the lock.
  The parent retains stderr open and unread. Only after acquisition does
  the child execute the actual producer. Producer return, required terminal
  and dispose/join occur while that lock remains held. Shutdown reports
  abandoned, never drained. This is a controlled real stderr-lock stall;
  it does **not** claim a particular kernel WriteFile is pending.
- C2: β seed enters the actual sole exporter; entered.send synchronizes
  with arm.recv; that exporter is never released during the property.
  Actual α producer / mandatory terminal / join still return. Queue
  delivery is not promised; shutdown is honestly abandoned.
- C3: same stopped-drainer seam, then cfg(test) prefill on the **actual
  render thread's current α pool immediately before the selected site**.
  First AlphaRejected proves the two-slot capacity. The selected actual
  emit adds exactly one rejection. Panic uses its own registered pool:
  first panic rejects once at full capacity, second is one-shot suppressed,
  critical_panics remains exactly two. No hidden consumer release occurs.
- C4: parent closes stderr read end **before stdin GO**; child corroborates
  real write Err before work. Actual target exporter uses real StderrExporter,
  and loss accounting / one disable / drain identity are checked after
  shutdown settlement. Earlier non-target records are filtered by the test
  exporter, so another outlet cannot pre-disable the selected target.

PO/SW DAGs: install → spawn; lock-acquired send SW receive → producer;
exporter-entered send SW receive → render spawn; site actual emit return PO
HITS Release SW await_hit Acquire → intentional Stop for #9/#10 → terminal
→ dispose/join. #8's cone stop PO complete PO diagnostic; terminal is Failed
without external Stop, and its verdict observer snapshots edge-cut order.
Critical hook thread sends completion SW recv then joins. Terminal visibility
alone never substitutes for dispose/join.

The exit-report C4 asserts the **actual detached writer's returned Err** via
a cfg(test) Release/Acquire witness, not just the main-thread canary. C1
asserts report entry and no returned write while stderr lock is held, after
host function return. The disable self-report runs the real deliver error
branch; self_diagnostics is its persistent entry witness. The self-report's
own write loss has deliberately no recursive loss counter.

All polling deadlines are watchdogs. Timeout is RED. No sleep establishes
SW; no rescue Stop produces a property result. Killed children are bounded
process cleanup, not successful thread joins. Held consumers are intentionally
not joined and die with the child.

## Reproduce the scoped evidence

From the repository root in native PowerShell 7 with the MSVC Rust toolchain:

```powershell
./research/diagnostics-tier2/mutations.ps1
./research/diagnostics-tier2/matrix.ps1
./research/diagnostics-tier2/results.ps1
```

Build commands discover test executables from Cargo compiler-artifact JSON,
not hard-coded file hashes. A real Windows render endpoint is required for
WASAPI rows. Exact FFmpeg source fetch URL and SHA are recorded below.

The original complete `lefthook run pre-push --all-files` is run separately
in a native Linux LF checkout by `research/diagnostics-tier2/linux-gate.sh`.
Its four arguments are the local repository, evidence checkout, a new
validation directory and the verification commit. Node/npm, Python, Cargo,
the pinned nightly and Lefthook must be on that environment's PATH.
`COMMANDS.md` retains the exact setup.
This gate is Linux regression evidence, not Windows or native decode proof.
Native Windows workspace regression and the explicit 32 armed controls are
recorded independently.

Windows whole-gate attempts remain failures, not PASS. Their retained
receipts distinguish GBK decoding, missing offline tool cache, the clean-tree
guard and the existing CRLF-sensitive negative-control anchor. A temporary
LF attempt also triggered the Windows Git clean guard; that adaptation was
withdrawn. The final Linux checkout runs the original checker and all its
negative controls with canonical repository LF bytes, without bypasses.
The retained Windows `gate.ps1` reproduces its interpreter/UTF-8 setup only;
it does not claim to repair the unrelated CRLF gate oracle.

The additional native Windows `cargo test --workspace` run is **FAIL_ORACLE**,
not a green workspace claim: WASAPI finished 26/28, with the existing
`a_park_slice_that_skips_its_quiescent_publication_is_rejected` and
`a_publication_moved_into_a_comment_is_rejected` failing. The baseline-only
reproducer `baseline-oracle.ps1` extracts both oracle and source from main
`98ff343` unchanged. Both cases pass with LF and both fail with CRLF on the
same Windows compiler; the baseline-oracle source/executable hashes were
recorded at the time and are not retained (2026-11-01 evidence diet). Their multi-line mutations were not applied
to CRLF input. This is independent baseline evidence, not an inference from
the changed diagnostic helper. Subsequent workspace suites after Cargo's
early stop are not claimed executed. No existing oracle is rewritten here.

## Oracle repairs and falsification

Initial OS-pipe probes (not retained, 2026-11-01 evidence diet): GetNamedPipeInfo
reported nominal 4096 bytes while PeekNamedPipe observed 8192. The original
`available >= nominal` comparison therefore did not prove a full buffer or
a pending syscall. Those original claimed C1 passes are **FAIL_ORACLE**, not
product failures. Final C1 uses the constructive real-lock seam above.

The initial exit-report blocked assertion also incorrectly required the
unrelated, empty facility to be abandoned. The exit function returned, while
the empty facility correctly drained. This is **FAIL_ORACLE**, repaired by
observing the detached writer itself. An initial exit-report stopped row
controlled the wrong consumer and is withdrawn, not promoted to PASS.

Initial panic counter mutations ran RED but hid assertion payloads either on the registered critical thread or through the test predecessor, so their exact signatures were unobservable; that non-qualifying round-1 receipt is not retained. The test predecessor and the specific full-pool rejection mismatch now print to healthy stdout; final mutation receipts require
both RED and the named signature. Runner also rejects missing one-test
execution, missing result markers, child failure and watchdog/harness errors.

Final expectations are 32 armed control executions, 48 classified cells, 23 exact mutation REDs and two harness-negative REDs. The source manifest identifies the tested input bytes; each raw child receipt hashes its actual executable. Results validation rejects incomplete or generic M11 receipts.

The rerunnable mutation script is `research/diagnostics-tier2/mutations.ps1`.
Each target file is backed up as bytes and restored in finally, with SHA256
identity checked. The mutation ledger recorded per-outlet signatures and
results, with a separate build log and raw child log per mutant; those
receipts are not retained (2026-11-01 evidence diet). Mutations
exercise producer synchronous stderr, consumer waits, α rejection accounting,
export-loss accounting, critical previous-hook chaining, joining the detached
report writer and misreporting its Err as success. Mutant build errors do not
count as kills. These are verification interventions, not product fixes.

## Pinned FFmpeg source and artifact boundary

Pin: `n9.0.1`, commit `bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`.
Fetched exact-commit [libavutil/log.c](https://github.com/FFmpeg/FFmpeg/blob/bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa/libavutil/log.c),
retained as `ffmpeg-log.c`, SHA256
`0b0c949a1124bbfc0af710b7c6c63b7f767d5bd38857e41dd75c6b9c5199cfa5`.
Lines 398–438 put the file mutex **inside av_log_default_callback**. Lines
460–469 load the callback atomically and call it directly; av_vlog does not
hold that mutex around a replacement callback. Lines 492–494 install with
an atomic store. This only establishes log.c's local context. Callers may
hold unrelated FFmpeg locks; bridge no-reentry/no-lock/no-allocation rules
remain necessary. No whole-FFmpeg lock-freedom claim is made.

#172 is still open with no later issue comments supplying verified target
provenance. Neither this checkout nor the inspected local native trees supply
a usable archive/manifest. #294 explicitly rejected the historical MinGW
archive as Windows evidence because it was byte-identical to Linux. It is
not reused. Windows native FFmpeg callback and native decode error rows are
**NOT_RUN**. Linux native rows are also NOT_RUN in this environment. Generic
Cargo tests exclude the native decode crate and cannot change these verdicts.

## Isolated product differential and owner decisions

`FAIL_PRODUCT` at **source/configuration-contract level**, runtime impact
**NOT_RUN**: Rust `av_log_bridge::bridge_requested` treats
`QIANQIAN_FFMPEG_LOG="0"` as off/discard, while native
`songcore_quiet_ffmpeg_logging` installs discard only when getenv is NULL.
In a fresh process with value `0`, neither side installs the promised
discard/bridge; the pinned default callback initially points at the synchronous
stderr logger. Source-confirmed gate composition contradicts Rust's discard
claim. Native reachability and actual blocking/error impact still require a
verified native artifact and a real fresh-process open/av_log reproduction.

Required separate proposal: owner settles whether `0` means off at this
cross-language boundary, aligns the two existing predicates if so, and adds
fresh-process real-native unset/0/1 controls plus a deliberate predicate
mismatch negative. This PR changes neither predicate. No new logging service,
Plugin, bus, lifecycle or ABI is proposed.

Owner-scoped residuals: non-α synchronous previous-hook chain; synchronous
StderrExporter/self-report drainer stalls with bounded honest abandonment;
native/decode NOT_RUN rows and the `0` gate differential. This evidence does
not accept those residuals on the owner's behalf.

## Closure assessment

- **#296 does not meet closure criteria.** Native FFmpeg and adapter rows
  and owner residual disposition remain open. Controlled Windows rows do not erase that debt.
  No automatic closure or merge is performed.
- **#317 can remain a delivered audit/routing record and is eligible for
  owner closure as such**, with this follow-up linked. That closure must not
  mean Tier-2 isolation is complete. Its historical "no product defect found"
  statement remains true at its audited base/scope; the new source gate
  differential belongs to this follow-up, not a retroactive rewrite.
- #290 has a scoped accounting/closure recommendation only. Its Windows
  broken-sink/terminal NOT_RUN lanes are not covered by the separate native
  Windows diagnostic outlet tests in this PR.

Verification commands, exact test execution, gate logs, mutation results,
source/package identities and the independent review are retained beside
this report. A clean local gate is regression evidence, not architecture
acceptance, native artifact validation or audible/device-loss certification.




