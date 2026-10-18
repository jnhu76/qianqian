# #288 Windows Runtime Verification — Frozen Oracle & Scenario Matrix (R2)

Frozen BEFORE the systematic execution round. Evidence levels use the ladder
from `EXECUTION_DAGS.md`; a row may never be reported above its executed level.
Every RED is triaged through `§11` classes before any production claim.

Common harness facts (apply to every scenario below unless a row overrides):

- Target triple `x86_64-pc-windows-gnu` (mingw-w64 13-win32 cross linker),
  built on the WSL2 host (rustc 1.97.1), executed on the real Windows host
  (Windows 11 Pro build 26300) via PowerShell. This is
  `WINDOWS_NATIVE_BACKEND_RUNTIME` / `WINDOWS_REAL_ENDPOINT` when a device
  open occurs (Realtek High Definition Audio is the default render device
  candidate; the mechanism opens the eRender/eMultimedia default endpoint).
- The child process under test is the cross-compiled
  `qianqian-output-wasapi` test binary (or a mutant/pre-swap build of it for
  A3/A4 controls); binary SHA256 recorded per run.
- Precondition witnesses are synchronization-observed (flags/counters set
  before the event they witness, read with Acquire; never sleep-based).
- Cleanup: every scenario releases the device (the fixture's dispose /
  release_fixture); a RED run's cleanup is recorded as cleanup, never as a
  property result.
- #290 lesson applied on Windows independently: pipe/handle inheritance
  (.NET `Process` redirects create fresh anonymous pipes; only std handles
  are inherited), and every sink premise is proven IN THE CHILD by a
  calibration write (broken) or a flooding witness (blocked) before the
  property observation starts — a sink premise never assumed from the
  parent's view.

## Matrix

```text
SCENARIO_ID  PROPERTY (authority)                     ACTION                        ORACLE / OBSERVATION                                  NEGATIVE CONTROL
-----------  ---------------------------------------  ----------------------------  ----------------------------------------------------  ------------------------------------------
A1-N   cone settles itself, no external stop    injected k-th-read panic      wait_terminal returns ≤15 s; terminal==Failed;        M1: delete render_input.stop()
       (migration §7.1; PBK-002 D11)            (frozen regime)               dispose quiet; precondition flag (injected==true)     → settle bound fires (RED)
A2-O   stop-before-complete (migration §7.2)    same drive                   verdict_reached && snapshot(A)≥1 at settle            M2: swap stop/complete
                                                                                                                                     → snapshot(A)==0 (RED); A1 stays green
A1-E   witness printout evidence row            same drive + evidence test    stdout records reads/injected/edge_cuts/snapshot/      (covered by M1/M2; print uses non-panick-
       (facility leg also exercises #8/#9                                   failure_diagnostic; facility leg renders              ing writer; broken-sink rows unaffected)
       through the real drainer)                                            #9 open + #8 abort records on stderr
A3-M1  mutation control for A1                  M1 patch in isolated worktree  A1 RED exactly at settle bound; A2 not claimed          byte-identical restore (sha256);
                                                                                                                                     unrelated panic ≠ control result
A3-M2  mutation control for A2                  M2 patch in isolated worktree  A2 RED snapshot(A)==0; A1 still green                  byte-identical restore (sha256)
A4-8-B post-swap #8 × gate=1 × BROKEN sink      child: cone test, stderr pipe  terminal/exit == A4-8-H (healthy) row; exit code       pre-swap #8 (eprintln at pre-#287
       read end closed by parent                read end closed pre-spawn      0/1 per test harness; settle bounded                   position) + broken: settle bound
                                                                                                                                     fires = N-01 hang reproduced (A5)
A4-8-BL post-swap #8 × gate=1 × BLOCKED sink    child floods own stderr to     settle + terminal unchanged; drainer residual           pre-swap #8 + blocked: settle bound
       (pipe full, flooding witness)            fill, then drive cone          documented (bounded shutdown abandon ≤500 ms)          fires = B F5 delay reproduced
A4-8-H post-swap #8 × gate=1 × healthy sink     baseline child run             reference row for equality                              —
A4-9-* #9 rows: same shape as #8 rows ×         open succeeds → #9 executes    post-swap: open succeeds + episode behaves as          pre-swap #9 + broken: post-activation
       {gate=1 broken / blocked / healthy}      (witness: Opened + open        the healthy row (misattribution class gone)            abort / N-01 cascade (recorded class)
                                                 record reaches drainer)
A4-10  #10 × gate × sink: volume recoverable    volume-apply fault seam        post-swap: recoverable failure stays recoverable       pre-swap #10 + broken: Failed{device}
       failure forced (SEAM — see below)        (test-only) forced at the      (terminal/exit == healthy row); ONE diagnostic         misattribution reproduced (AF-4)
                                                 SetAllVolumes boundary
A5-N01 four-premise end-to-end observation      pre-swap #8 + gate=1 +         PREMISES each witnessed: (i) QIANQIAN_AUDIO_LOG=1;      (the pre-swap row IS the control;
       (reconciliation §3 N-01)                 broken stderr + episode        (ii) stderr write FAILS (in-child calibration          post-swap row shows the hazard gone)
                                                 unresolved + wait_terminal     write Err); (iii) episode unresolved (no terminal
                                                 before dispose (cone harness   commit observed); (iv) wait_terminal path (oracle
                                                 = same shape as machine path)  helper thread). RESULT: settle bound fires (hang)
A6-P   carrier B pipeline witness               cone drive + evidence test     reason survives to mirror; resolver ignores it;         reason-drop control: late verdict
       (DETERMINISTIC_PIPELINE_WITNESS)                                       failure_diagnostic / observation at settle            counting (late_drain_verdicts) —
                                                                                                                                     existing first-wins oracles re-run
A6-E   real-endpoint device-initiated abort     disable default render device mid-  AbortReason{Device} constructed by REAL          safe/reversible device control;
       (REAL_ENDPOINT smoke; separate           playback (pnputil, elevated)     device loss; class/message reach C4/C6;               if elevation unavailable or unsafe:
       statement from A6-P)                                                      settle Failed{device}                                  row = BLOCKED_ENVIRONMENT
A7-D   diagnostics close protocol on Windows    cross-compiled diagnostics     existing close-race/admission oracles green on          (existing loom/Miri layers unchanged;
       (admission word, final sweep,            test binary, full suite        real Windows threads; subprocess witnesses             not re-derived)
       drainer bounded wait, detached           + headless shutdown rows       for exit path
       report writer, process exit)
A7-H   host exit path with stuck sink           headless-shaped child (or      bounded abandon (≤ deadline); exit code unchanged;     healthy-sink shutdown row (report
       (blocked stderr)                        facility child) + blocked      detached report writer does not hand exit to sink      line delivered) as the contrast row
A8-DEC scope decision                          control-inertness evidence     existing lib-side Linux witness + A6-P resolver-        deferred rows listed explicitly
                                                inventory                     inert observation recorded; tier-2 rows NOT claimed
```

## Decisions frozen before execution

1. **A1/A2 pass criteria**: precondition witness (injection fired on the render
   thread) + settle within bound + terminal==Failed + (A2) snapshot(A)≥1. A
   timeout with the precondition NOT reached is `PRECONDITION_REFUTED` /
   `BLOCKED_ENVIRONMENT` (TOOLING-DEVICE evidence), never a property result —
   per the oracle's own module contract (`cone_oracle_tests.rs:62-69`).
2. **A3 mutants**: single-property patches in an isolated worktree; source
   hash before/after; each mutant built AND executed on the real Windows host
   (a compile failure is not a control result); byte-identical restore
   verified by sha256. M1 expectation: A1 RED via settle bound (the wedge);
   M2 expectation: A2 RED via snapshot(A)==0 while A1 remains green.
3. **A4 sink premises are proven in-child**: broken = a calibration write to
   stderr returns Err BEFORE the drive starts (printed to stdout as the
   premise witness); blocked = the child's flooding writer observes its own
   write blocking (counter freeze + not-returned flag) before the drive
   starts. The parent (PowerShell) only creates/closes/withholds the pipe.
4. **A4 pre-swap controls** restore the exact `b86a9ce^` shapes (verbatim
   hunks extracted in `EXECUTION_DAGS.md` DAG-2 table), run in the isolated
   worktree lane, then restore — identical discipline to A3.
5. **#10 volume-apply fault seam** is admitted ONLY if code inspection
   (recorded in the ledger) proves a real `SetAllVolumes` recoverable failure
   is not constructible on a healthy device; the seam must be test-only
   (cfg(test)-gated), add no public ABI, no resident runtime state, and ship
   with its own negative control (seam armed vs not armed flips the
   diagnostic's presence while the terminal class never moves).
6. **A6 rows are reported separately**: A6-P (deterministic pipeline witness
   through the real cone chain) can never be presented as device evidence;
   A6-E attempts a real device-initiated abort via PnP disable of the default
   render endpoint with automatic re-enable in a finally-shaped cleanup; if
   elevation/safety fails the row is `BLOCKED_ENVIRONMENT`, not FAIL.
7. **Blocked-sink rows never claim isolation**: they document the residual
   (drainer-side blocking, bounded-or-honest shutdown) only.
8. **Time bounds are local**: SETTLE_BOUND (15 s), PRECONDITION_BOUND (10 s),
   HOST_DIAGNOSTIC_EXIT_DEADLINE (500 ms) are harness/facility parameters —
   no claim of system-wide shutdown deadlines is made anywhere.
9. **A8** is expected to be `DEFER_TO_BOUNDED_CHILD_ISSUE` given A1–A7 scope;
   existing control-inertness evidence and the remaining tier-2 gap list are
   recorded either way.
