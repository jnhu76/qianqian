# Independent evidence review

Reviewer: independent `testing-reality-checker` subagent. Reviewed 2026-10-28
Asia/Shanghai, starting at 00:14, against base
`98ff343467b66890a979edd54d21e14f0275c574`. This is a local independent review,
not a GitHub review approval or owner acceptance.

## Scope and method

Read the repository governance, concurrency/test-quality rules, relevant error
records, actual outlet implementation, final test support, WASAPI and headless
children, process runner, mutation runner, draft report, pinned `log.c`, and
retained control logs. No test or mutation was run by this reviewer, and no
production file was edited. The author was running sequential mutations while
this review read files; transient mutant diffs are not treated as delivered
production changes. Final restoration and local-gate receipts require their
own execution evidence.

The reviewer independently enumerated 32 final control logs: each contains
`running 1 test`, `EXIT=0`, and `RUNNER_RESULT=GREEN`. Those runner values do
not turn the three non-alpha previous-hook controls into isolation PASS.
Native SongCore/FFmpeg runtime was not available or executed in this review.

## Causal and oracle assessment

- C1 now constructs a real stderr-lock dependency. Lock acquisition precedes
  an mpsc send/receive, work begins after that receive, and no actor can release
  the lock during the property. The parent retains the pipe without reading.
  This proves the controlled lock stall, not OS buffer fullness or a pending
  kernel syscall. The earlier nominal-buffer comparison is correctly withdrawn
  as FAIL_ORACLE.
- C2 parks the actual sole exporter on a seed before starting alpha work.
  Send/receive and the unreleased condition establish the consumer premise.
  C3 additionally fills the selected producer's actual current alpha pool
  immediately before the real WASAPI emission. It does not substitute another
  thread's pool or the beta queue.
- WASAPI target emission returns before HITS Release; await_hit Acquire
  precedes intentional Stop for #9/#10. #8 independently settles Failed
  without a rescue Stop, then its diagnostic return and dispose/join are
  checked. Terminal visibility is not substituted for leg join.
- The critical panic hook uses its own first-publication path. At full
  capacity the first panic adds one rejection; the second is one-shot
  suppressed. Exactly two critical panics and no previous-hook invocation
  are checked after the worker joins. This matches the pool implementation.
- C4 closes the real stderr read end before GO and corroborates Err inside
  the child. Target filtering prevents earlier non-target records from
  disabling the chosen export path. Drain settlement and accounting then
  expose real target-export loss. The exit-report child additionally observes
  the actual detached writer's returned result through a test-only witness.
- Exporter disable and flush self-reports drive distinct real production
  branches. The flush case closes admission to reach final-sweep flush and
  uses its persistent self_diagnostics count before the bounded blocked
  shutdown observation. It correctly does not invent a record export loss
  for a flush error.
- C2/C3 NOT_APPLICABLE is justified for direct self-reports and the detached
  exit report: neither has a separate application consumer/admission queue.
  Their external stderr backpressure/failure is exercised as C1/C4. This
  does not exempt upstream alpha outlets from stopped-drainer testing and
  does not claim that synchronous self-report writers complete while blocked.
- The runner now fails closed on child failure, watchdog/harness failure,
  missing one-test execution, or missing the expected result marker. Process
  termination of held actors is cleanup, not a join or quiescence claim.

## Source differential and native limits

The reviewer independently found the `QIANQIAN_FFMPEG_LOG="0"` differential:
Rust declines bridge installation for `0`, while native code installs discard
only for an absent variable. In a fresh process this leaves the pinned default
callback available. Classifying this as a source/configuration-contract defect
while explicitly keeping native runtime impact NOT_RUN is supportable. It
must not be expanded to a reproduced device/decoder hang. Isolating a separate
predicate-alignment proposal preserves the production-change restriction.

The retained log.c SHA256 matches the report. Its callback dispatch loads the
callback and calls it directly; the default callback's own mutex is not held
around replacement callbacks. This is local-source evidence, not a claim that
arbitrary FFmpeg callers hold no locks. Native artifact and callback runtime
NOT_RUN rows remain necessary and are not grounds for an unauthorized product
change.

## Review findings and resolved corrections

1. RESOLVED: REPORT.md and results.json now match the separately executed
   flush-report C1/C4/healthy controls. Non-alpha C4 records execution of the
   inherited default hook on broken stderr and join; its overall result
   remains KNOWN_RESIDUAL for arbitrary predecessor behavior.
2. In non-alpha C1, the channel is sent by a wrapper **before** calling the
   inherited default hook. It proves wrapper entry and the next program-order
   call, not that the default hook body has already executed. Retain the
   residual conclusion. RESOLVED: the final raw marker and report explicitly
   distinguish wrapper entry from unobserved default-call entry.
   The controlled C2 predecessor really enters its release-dependent gate;
   the broken C4 join really proves the default hook returned.
3. RESOLVED: M11's former generic `Q296-` signature was replaced by unique
   `M296-SHUTDOWN-DEADLINE-BYPASSED` plus required `WATCHDOG_RED`. Both final
   self-report and flush-report receipts contain those exact witnesses and
   RUNNER_RESULT=RED, with no Q296-PASS. M3/M10 likewise use outlet-specific
   assertion signatures; nonqualifying suppressed-payload runs remain
   explicitly separate in round1.
4. RESOLVED for evidence receipt completeness: results.json now contains all
   48 cells, with 22 PASS, 3 KNOWN_RESIDUAL, 11 NOT_APPLICABLE and 12 NOT_RUN.
   mutations.json contains 23/23 exact_red=true receipts. The two harness
   negatives reject an unbroken supposed-broken sink and a zero-test run.
   The reviewer checked the then-final 14-file source SHA256 manifest against the
   working files and found zero mismatches. These checks do not substitute
   for the repository's complete pre-push gate.

Follow-up reviewed at 2026-10-28 00:29 Asia/Shanghai. At that read,
`raw/pre-push.log` still ended GATE_EXIT=1: plugin-boundary could not resolve
`public-api` from the configured replacement registry in offline mode.
Subsequent checks were skipped by that failed gate. This is an environment
block, not a new diagnostics product failure, and is **not a gate PASS**.
The author must retain a later complete receipt if that prerequisite is
repaired. No gate was executed by this reviewer.

## Cross-platform gate follow-up

Reviewed 2026-10-28 01:00 Asia/Shanghai. Subsequent Windows attempts did not
establish a green full gate. Tooling/dependency/line-ending failures must
remain visible rather than being counted as diagnostic product failures or
erased by a later Linux result.

baseline-oracle.ps1 extracts the unmodified main-base render_order_oracle.rs
and wasapi.rs, constructs LF and CRLF twins, and runs the same two named tests
with the native Windows compiler. Its four retained receipts show LF 2/2 GREEN
and CRLF 2/2 RED with the same current audio-api rlib identity. The CRLF
failures report empty violation lists after literal LF mutation anchors fail
to match. This isolates an existing oracle-input line-ending defect,
classified FAIL_ORACLE. It is a standalone source-oracle control, not a clean
build of the entire baseline workspace or a production rendering failure.
Leaving those existing oracles unchanged fits this PR's bounded scope.

The temporary Windows LF-normalization wrapper has been withdrawn. The
reviewed gate.ps1 supplies an explicit Python command and UTF-8 mode to the
original full gate without editing checker rules or skipping negative
controls. Native Windows full-gate status remains RED for the documented
attempts even if a separate Linux LF checkout passes.

The Linux run uses subject commit
`8517c1f5abe473b8bc8e585e89fb079c7b5ad95d`, a real LF checkout, its own toolchain
and environment manifest, and the unchanged full Lefthook command. This is
Linux regression evidence only: cfg(windows) tests do not run there, and
Windows Tier-2 runtime receipts remain a separate lane. The first green Linux
log without a reliable outer exit receipt cannot substitute for a final
GATE_EXIT=0 receipt.

At the first cross-platform follow-up the Linux gate was still running and
its initial environment manifest listed 14 source paths. The replay script
then needed to account for two added outer research scripts that are absent
from the fixed Rust subject commit. RESOLVED: linux-gate.sh now explicitly
overlays only the outer .ps1/.sh runners, plus the report/router evidence.
Its source manifest records these overlays rather than silently copying
uncommitted Rust or checker code.

Final receipt checked 2026-10-28 01:03 Asia/Shanghai: pre-push-linux.log
contains all eight checks GREEN and GATE_EXIT=0, elapsed 187.41 seconds.
The retained environment identifies Linux/WSL2, rustc 1.97.1 and subject
8517c1f; its refreshed manifest has 16 paths and eight explicit outer-script
overlays. Its provenance note correctly states that overlay hashes were
refreshed after the successful gate and keeps the initial environment
receipt separately. This is not retroactively presented as 16 identical
pre-run inputs. No Rust/checker subject was changed for that refresh.

The reviewer rechecked the current Windows 16-file manifest with zero
mismatches, the 11 mutation-class restoration receipts with zero mismatches,
and the final entry.rs diff: non-test builds retain the original diagnostic
write statement, with only cfg(test) entry/result observation added. Linux
gate PASS does not turn the Windows CRLF source-oracle failures into PASS.

## Delivery conclusion

The controlled Windows boundary tests and their evidence are acceptable as a
scoped verification delivery. The four review findings and the replay-script
gap are resolved. The original complete local gate is GREEN on the separately
identified Linux LF subject; the failed Windows attempts and baseline
FAIL_ORACLE attribution remain disclosed. No additional product fix is
requested in this PR. This review does not approve #296 closure:
native decode/FFmpeg execution and owner residual decisions remain open.
#290 may receive its scoped accounting recommendation, and #317 may be closed
by its owner as a delivered audit/routing record only, without representing
Tier-2 isolation as complete.
