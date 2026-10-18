# #288 Windows Runtime Verification — Evidence Ledger

Campaign: WINDOWS-ERROR-CONE-RUNTIME-1. Branch `evidence/windows-runtime-288`
(from main `fc81b68`; PR #287 implementation `b86a9ce` confirmed ancestor).
This ledger is the execution record; oracle design authority is the frozen
`SCENARIO_MATRIX.md`; causal chains are `EXECUTION_DAGS.md`. Raw logs: `raw/`.

## Environment identity (R0)

```text
BASE_SHA              = b86a9ce (PR #287, the implementation under test)
CURRENT_MAIN_SHA      = fc81b68 (campaign base; PR #293 merged)
CAMPAIGN_HEAD         = see final commit (this branch)
RUST_TOOLCHAIN        = rustc/cargo 1.97.1 (WSL2 cross host; pinned toolchain)
TARGET_TRIPLE         = x86_64-pc-windows-gnu (mingw-w64 GCC 13-win32 linker)
WINDOWS_HOST          = Windows 11 Pro, build 26300 (real host, not a container)
BACKEND_IDENTITY      = WASAPI shared-mode, event-driven (windows crate 0.62.2)
ENDPOINT_IDENTITY     = IMMDeviceEnumerator default eRender/eMultimedia endpoint
AUDIO_DEVICE          = Realtek High Definition Audio (status OK; the open line's
                        negotiated evidence shows the device mix format refused
                        44.1 kHz float32 directly → Tier-2 engine SRC every open:
                        "opened: 44100 Hz, 2 channels, mask 3, buffer 1036 frames
                        (shared, event-driven, engine SRC)")
BUILD_LANE            = cross-compiled in WSL2, executed on the real Windows host
                        via PowerShell (.NET Process pipes for sink control)
BUILD_IDENTITY        = cargo test --no-run, debug profile; per-run BIN_SHA256 in
                        each raw log
ELEVATION             = FALSE (non-admin session; matters only for A6-E)
```

Evidence levels used below (never mutually promoted):
`WINDOWS_REAL_ENDPOINT` (a real device open occurred), `DETERMINISTIC_FAULT_INJECTION`
(injected at a declared seam), `PRODUCTION_PATH_OBSERVED` (unmodified production cone).

## Sub-gate verdicts

### A1 — O-CONE-NECESSITY = PASS (WINDOWS_REAL_ENDPOINT + PRODUCTION_PATH_OBSERVED)

Receipts: `raw/a1_a2_gate_on.log`, `raw/a1_a2_gate_on_v2.log`, `raw/a1_a2_gate_off_v2.log`,
`raw/full_suite_gate_on.log` (23/23), `raw/a4_inj_healthy_postswap.log` (independent
third run inside the sink-control harness).

- Precondition witnessed (synchronization-observed, never sleep-inferred): the
  injection flag set on the render thread before the panic; reads ≥ 3.
- The real cone settled with NO external stop/dispose: `wait_terminal` returned
  `Failed` in 14–33 ms across runs (bound 15 s).
- Witness rows: `reads=3 injected=true edge_cuts=2 verdict_reached=true
  verdict_snapshot=1 terminal=Failed relayed_verdict=Aborted { reason:
  Some(AbortReason { class: Panic, message: "render loop panicked" }) }`.
- Gate on and gate off both green; facility leg (`raw/a1_evidence_facility.log`)
  additionally rendered sites #9/#8 through the real drainer and shut down with
  `drained=true undelivered=0 copied_out=2 exported=2`.

### A2 — O-CONE-ORDER = PASS (WINDOWS_REAL_ENDPOINT + PRODUCTION_PATH_OBSERVED)

Receipts: same logs. Program-order counter A (edge cut, incremented after the
real stop forwards) vs synchronous verdict observer B inside `complete()`:
`verdict_snapshot=1 ≥ 1` in every run; monotonicity explained (edge_cuts=2 =
cone stop + `stop_and_join` at dispose). Negative control M2 pins the oracle.

### A3 — PRODUCTION MUTATION CONTROLS = PASS (both mutants caught; bytes restored)

Executed in the isolated worktree `/home/hoo/Source/qn288-mut` (detached at the
campaign commits; `raw/mutation_base_hash.txt`).

```text
MUTANT  BASE_HASH(sha256 of wasapi.rs)                    PATCH_HASH              EXECUTION                EXPECTED FAILURE                 OBSERVED FAILURE                                                                    CAUSAL_MATCH  RESTORE_HASH           RESULT
M1      433bd0ff…c6c0 (a53ffbf)                           3fb7eaee…648ad          a3_m1_necessity.log      A1 red at settle bound            "O-CONE-NECESSITY RED: wait_terminal did not return within 15s" (15.18 s;      exact         433bd0ff…c6c0          CAUGHT
                                                                                                                                            injection backtrace proves the real cone path)
M1/A2   (same build)                                      —                       a3_m1_order.log          order undecidable                 "O-CONE-ORDER INCONCLUSIVE: … did not settle" (wedge class)                       exact         (restored with M1)    CAUGHT
M2      433bd0ff…c6c0 (restored, then patched)            39ead036…c0d480         a3_m2_order.log          A2 red snapshot(A)==0             "O-CONE-ORDER RED: … with 0 edge cuts recorded"                                     exact         47225c68… (branch)    CAUGHT
M2/A1   (same build)                                      —                       a3_m2_necessity.log      A1 stays green                    green; witness `verdict_snapshot=0` shows the swap while settle proceeds        exact         (restored with M2)    INDEPENDENT
```

M1 and M2 are independent controls (deletion wedges; swap keeps settle but breaks
order) — exactly the matrix's requirement. Worktree clean after restore
(`git status` empty; final sha == branch sha recorded in `raw/preswap_branch_sha.txt`).

### A4 — E-7 TIER-1 WRITER-SWAP CONTROLS = PASS (broken ≡ healthy; blocked residual documented)

Harness: `harness/run_sink_row.ps1` (.NET Process pipes; only std handles
inherited — the Windows-side independence check of #290's forkling lesson).
Sink premises proven IN THE CHILD: broken = calibration write must fail
(observed `os error 232` = ERROR_PIPE_CLOSED / broken pipe); blocked = the
child's flooding writer observes its own write parking (counter frozen, writer
not finished; the .NET anonymous pipe filled at ≤ 8 KiB in every run).

All children ran with the DG-4b panic-hook posture (product shape) unless a row
says `defhook`; `--nocapture` rows noted (libtest capture otherwise routes
`eprintln!`/default-hook prints into the in-memory capture buffer, not fd 2 —
harness lesson recorded below).

Post-swap rows (CURRENT implementation; sites #8/#9/#10 are class-α records):

```text
ROW (raw log)                                 DRIVE      SINK     GATE  EXIT  SETTLE/TERMINAL                          ACCOUNTING
a4_inj_healthy_postswap                       injected   healthy  on    0     33 ms / Failed                            #9+#8 rendered; drained, exported=2
a4_inj_broken_postswap (dg4b)                 injected   broken   on    0     33 ms / Failed  (== healthy)             copied_out=2 export_failed=2 self_diag=1 critical_panics=1; drained
a4_inj_broken_postswap_defhook (dg4b arm had  injected   broken   on    0     33 ms / Failed  (== healthy)             copied_out=2 export_failed=2 self_diag=1 critical_panics=0
  not fired: true default hook — kept as the
  default-hook broken row)
a4_inj_blocked_postswap                       injected   blocked  on    0     15 ms / Failed   (== healthy)             shutdown ABANDONED at 1.0001 s; undelivered=2 counted; exit unchanged
a4_inj_broken_gateoff                         injected   broken   off   0     14 ms / Failed  (== healthy)             copied_out=1 (panic record only — gate guards the sites)
a4_vol_healthy_postswap                       vol-stop   healthy  on    0     23 ms / Stopped                           #9+#10+#8-stop rendered (3 exported); #10 fired via seam
a4_vol_broken_postswap                        vol-stop   broken   on    0     21 ms / Stopped  (== healthy)             #10 record lost+counted; terminal unchanged
a4_vol_blocked_postswap                       vol-stop   blocked  on    0     23 ms / Stopped   (== healthy)            drainer wedged; bounded abandon; terminal unchanged
a4_vol_broken_gateoff                         vol-stop   broken   off   0     21 ms / Stopped  (== healthy)            no site records (gate off)
```

Terminal class / exit code / settle latency are identical across sink states —
the T5-4/T5-5 claims hold on real Windows for all three sites, both drives,
both gate states. Blocked rows document the residual exactly as scoped: the
DRAINER wedges inside the sink (its own export), shutdown is bounded-or-honest
(abandoned at the deadline, undelivered counted, report attempted off-path),
and producers stay isolated.

Pre-swap controls (worktree builds restoring the verbatim `b86a9ce^` shapes;
each restored byte-identical after its rows; all rows `--nocapture`):

```text
ROW (raw log)                       SITE   SINK     CONTROL/EXPECTED HAZARD                  OBSERVED
a4_pre8_healthy_nocap_control       #8     healthy  control: settle normal                  GREEN (17 ms, Failed; eprintln wrote to real stderr)
a4_pre8_broken_n01                  #8     broken   N-01: eprintln panics BEFORE stop/       RED at the 15 s bound, exit 101: "wait_terminal did not
                                             complete → both publications skipped            return within 15s" — the N-01 hang, bounded by the oracle
a4_pre8_blocked_nocap               #8     blocked  B F5: eprintln parks before stop/        RED at the bound, then the child hung past the parent's
                                             complete → unbounded publication delay          90 s outer bound (killed) — the delay hazard as a wedge
a4_pre9_vs_healthy_nocap            #9     healthy  control                                    GREEN (Stopped)
a4_pre9_vs_broken_nocap             #9     broken   post-activation abort / Failed{device}   RED: terminal=Failed in 207 µs vs healthy Stopped —
                                             misattribution                                   the misattribution reproduced
a4_pre10_vs_healthy_nocap           #10    healthy  control (recoverable stays recoverable)  GREEN (Stopped; #10 eprintln printed)
a4_pre10_vs_broken_nocap            #10    broken   AF-4: recoverable volume failure          RED: terminal=Failed in 308 µs vs healthy Stopped —
                                             escalated to Failed{device}                     the escalation reproduced
```

Every pre-swap hazard the design documented is reproducible in this harness on
real Windows, and every post-swap row is identical to its healthy control —
the harness demonstrably sees what it claims to guard (matrix requirement).

**#10 reachability proof (seam necessity)**: on a healthy endpoint a
recoverable-grade `SetAllVolumes` failure is not constructible — levels are
`u8`-clamped factors (always legal), channel count is the service's own, and
the only observable failure grade that is not device loss requires the device
to refuse a legal value. The design names the seam as the one minimal
injection (migration §7.3); it landed as `volume_apply_seam` — `cfg(test)`-only,
no public API, no production indirection (non-test builds keep the direct COM
call), armed only by the campaign children with a declared HRESULT. Negative
controls: VF=0 vs VF=1 rows (record appears/disappears; terminal never moves).

**Observed residual, honestly scoped (NOT a writer-swap regression)**: with a
BLOCKED sink and the DEFAULT panic hook and `--nocapture`
(`a4_inj_blocked_defhook_nocapture`, `a4_inj_blocked_defhook_nocap_nofac`), the
panicking render thread wedges INSIDE the default hook's synchronous stderr
write (the pipe is full and the flooding writer holds the process stderr lock):
settle-bound RED and the child never exits (parent kills at 90 s). This is the
documented #11/T5-8 class (panic-hook synchronous sink — explicitly NOT fixed
by the writer swap). The DG-4b posture (what the product host installs before
any emitting thread, machine_input.rs:217) eliminates it:
`a4_inj_blocked_dg4b_nocapture` settles normally. Controls:
`a4_inj_healthy_defhook_nocapture` green; with libtest capture active
(no `--nocapture`) the default hook never touches fd 2 — a harness fact, not a
product claim.

### A5 — N-01 FOUR-PREMISE OBSERVATION = PREMISES_CONFIRMED (pre-swap) / ELIMINATED (post-swap)

Row `a4_pre8_broken_n01` (pre-swap #8, gate on, broken sink, machine-order
harness — `wait_terminal` before any dispose):

```text
(i)   gate enabled          QIANQIAN_AUDIO_LOG=1 in the child env (row header); the
                             print demonstrably EXECUTED (its panic is the wedge)
(ii)  stderr write FAILS    proven in-child before the drive: calibration write
                             returned os error 232 (broken pipe) — a full pipe
                             would BLOCK, and the blocked rows are the other mode
(iii) episode unresolved    no terminal committed: the settle never returned
                             (bound fired); no other publisher existed
(iv)  machine path          the harness's settlement observation is the bounded
                             blocking wait_terminal with no prior dispose — the
                             machine order (entry.rs finish_episode:553-554 shape)
```

RESULT: pre-swap the hang reproduces (RED at the bound); post-swap
(`a4_inj_broken_postswap`) the same four-premise configuration settles Failed in
33 ms with exit 0. N-01 is confirmed as a pre-swap defect and confirmed
eliminated by the landed swap on real Windows.

### A6 — CARRIER B = PIPELINE PASS / REAL-ENDPOINT-SMOKE BLOCKED_ENVIRONMENT

A6-P (deterministic pipeline witness, DETERMINISTIC_FAULT_INJECTION through the
REAL cone/session/presentation on a real opened endpoint):

- Panic-class (`raw/a1_a2_gate_on_v2.log` etc.): `relayed_verdict` shows the
  `AbortReason` riding the verdict through `complete()`; the resolver settles
  `Failed` on the discriminant only (control-inert: enrichment is Device-only,
  and `failure_diagnostic` stayed `Some("device")` — the Panic reason does NOT
  reach the presentation, by current design).
- Device-class (`raw/a6_device_reason_pipeline.log`): the seam armed with
  `AUDCLNT_E_DEVICE_INVALIDATED` → the REAL mechanism's device-loss branch
  constructed `AbortReason { class: Device, message: "stream volume apply
  failed (device invalidated): 0x88890004" }` → relay witness observed it at
  the completion seam → settle `Failed` → presentation ENRICHED:
  `failure_diagnostic = "device: stream volume apply failed (device
  invalidated): 0x88890004"`. The full C1→C7 chain (EXECUTION_DAGS DAG-3) on
  real Windows; display did not bend the session authority (terminal class
  unchanged by the reason's presence).

A6-E (device-initiated abort on the real endpoint): `BLOCKED_ENVIRONMENT` —
the host session is non-elevated (`raw/a6e_elevation_check.txt`:
`IsInRole(Administrator) = False`); `Disable-PnpDevice` requires elevation,
and autonomously triggering an interactive UAC prompt on the owner's desktop
is out of scope. The pipeline row above opened the real endpoint (that part is
WINDOWS_REAL_ENDPOINT); the device-INITIATED failure row stays honestly
NOT_RUN/BLOCKED. An elevated owner rerun is the remaining gap.

### A7 — WINDOWS CLOSE PROTOCOL = PASS (WINDOWS_DEVICE_FREE_RUNTIME + subprocess witnesses)

- Diagnostics unit suite on real Windows: 16/16
  (`raw/a7_diagnostics_units.log`) — includes the four close-race
  quiescence/delivery-honesty oracles (α and β in-flight delivered by the
  final sweep; outlasting the window never reported drained), pool retirement
  lifecycle, real-thread handoff, flush-failure self-diagnostic class.
- Integration subprocess oracles on Windows: `shutdown_admission` 2/2 (a
  class-α presentation after shutdown is dropped AND counted — real Windows
  child), `stderr_delivery_failure` 2/2 (a broken stderr descriptor counted as
  failed delivery — real Windows child).
- Headless host exit boundary on Windows: 372/372
  (`raw/a7_headless_units.log`) — including
  `an_abandoned_drainer_never_makes_the_host_exit_path_wait_on_its_sink` and
  `a_drained_report_never_makes_the_host_exit_path_wait_on_its_sink`: the
  production `shutdown_host_diagnostics` path (bounded abandon + detached,
  never-joined report writer) executes and the child exits on its own when
  the real stderr sink is wedged through the facility.
- Playback session machinery on Windows: 207/207
  (`raw/a7_qianqian_playback_suite.log`, 118 s — includes the D14.9 volume
  command-seam oracles and the completion/teardown suites on real Windows
  threads).
- The A4 facility rows double as A7 evidence at the exit boundary: healthy
  drain 32 ms; broken drain 7 ms with counted losses; blocked ABANDON at the
  1 s deadline with the honest report and unchanged exit path.

Time bounds stated locally only: HOST_DIAGNOSTIC_EXIT_DEADLINE (500 ms) and
the facility's PUBLISHER_QUIESCENCE_WINDOW (250 ms) are facility parameters;
no system-wide shutdown deadline is claimed anywhere.

### A8 — E-7 TIER-2 PER-OUTLET INERTNESS = DEFERRED_TO_BOUNDED_CHILD_ISSUE

Inertness evidence already held (recorded, not claimed as tier-2 completion):
resolver discriminant-only behavior witnessed in every A6 row (reason
presence/absence never changed a terminal); sink-state invariance of terminal
class/exit path/settle latency across all A4 rows for the cone sites; the
close-race honesty oracles re-run on Windows; the D14.9 command-seam suite
green on Windows. The tier-2 obligation — four §6 controls for EVERY
diagnostic outlet (reader protocol writes are E-9's; the panic-hook chain;
the av_log bridge; machine report rows) — is a bounded child campaign of its
own and is not claimed here.

## Findings (triaged)

```text
F1  PRODUCT_DEFECT (pre-swap shapes only, all restored): the three documented
    hazards N-01 / B F5 / #9 misattribution / AF-4 all reproduce on real
    Windows — and all are already eliminated by the landed writer swap.
    Current production: ZERO confirmed defects in any executed row.
F2  DOCUMENTED RESIDUAL CONFIRMED (#11/T5-8 class, not a swap site): default
    panic hook + blocked stderr + no capture wedges the panicking thread in
    the hook's synchronous write. Product posture (DG-4b hook, installed
    before any emitting thread) eliminates it — verified. Any future binary
    that skips the posture keeps this hazard (worth an owner note; no code
    change proposed in this round).
F3  BLOCKED-SINK RESIDUAL (declared): a wedged drainer delays DIAGNOSTIC
    delivery unboundedly and adds ≤ deadline to exit; bounded-or-honest
    accounting verified on Windows (abandoned report, counted undelivered).
    Never isolation — recorded as such in every blocked row.
F4  HARNESS LESSON (Windows, #290-independent): (a) libtest's output capture
    routes eprintln!/default-hook prints away from fd 2 — pre-swap eprintln
    controls REQUIRE --nocapture (the campaign's pre-swap rows all use it);
    (b) an `--exact` libtest filter needs the full test path (substring
    filters otherwise); (c) PowerShell wraps native stderr lines as error
    records — the final harness redirects child stdout/stderr to files.
F5  ENVIRONMENT NOTE: native/build/artifacts-mingw/libsongcore.a is
    byte-identical (sha256 408d7d6e…) to the Linux artifact — not a real
    mingw archive. Nothing in this campaign linked it (no SongCore lane was
    needed); recorded so the #172 packaging lane does not trust it silently.
F6  A6-E blocked: non-elevated host session (see A6).
```

## PRODUCT_DEFECT / ORACLE_DEFECT summary

```text
CONFIRMED_PRODUCT_DEFECTS = 0 (current implementation; pre-swap hazards reproduced
                             only inside restored-mutant builds, all byte-recovered)
ORACLE_DEFECTS            = 0 (every oracle that fired red did so at its designed
                             bound with the designed signature; A3 matrix clean)
PREMISE_REFUTATIONS       = 0 sink premises refuted (one harness self-correction
                             before any row was recorded: the `-Hook none`
                             convention bug — fixed, affected row renamed and kept
                             as the genuine default-hook variant, then re-run)
```

## Claim ceiling (what this round does NOT claim)

- No device-INITIATED abort was executed (A6-E blocked; elevation).
- No claim about MSVC-target behavior (all evidence is windows-gnu flavor;
  WASAPI semantics are OS-level, but the toolchain flavor is part of build
  identity, recorded per run).
- No claim that blocked sinks are isolated (they are not — declared residual).
- No E-7 tier-2, no E-9, no #231/#243/#195 scopes.
- No production behavior change: the only source-adjacent additions are
  cfg(test)-only (witness printouts, child oracles, the volume seam module);
  in the non-test build `wasapi.rs` gains only the inlined
  `apply_stream_volume` wrapper whose `cfg(not(test))` body is the identical
  direct COM call it replaced — no gate, no branch, no state.
