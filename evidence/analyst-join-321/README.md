# analyst-join-321 — executed E2E evidence for the analyst class-α correction

```text
TRUTH CLASS   executed verification evidence / implementation-record input
              NOT architecture authority and NOT a product semantics statement
SUBJECT       #321: observation-analyst panic × blocked stderr wedges the
              decode worker's exit funnel (analyst.join) and K0 Discharge
BASELINE      main before #322 (defect present)
FIX           #322: AlphaRegistration at the analyst thread's first statement
VERDICT       E2E_CONFIRMED on main; Completed/Discharged with the fix
PLATFORM      Linux (WSL2 kernel): /proc task inspection, pipe occupancy,
              subprocess re-exec. Windows runtime: NOT_RUN.
```

This directory is the versioned evidence package. The product tree contains **no**
panic-injection seam: `replay.sh` copies the harness into a scratch worktree and
applies the evidence-only overlay there. The injection is test code; the panic, the
production hook installer, the real analyst thread, the real decode worker, the real
session and the real K0 `dispose()` are production mechanisms.

## Contents

```text
README.md                       this index
REPORT.md                       causal DAG, contract classification, controls, limits
replay.sh                       reproducible baseline-RED / fix-GREEN / negative-control run
harness/analyst_join_e2e_tests.rs
                                subprocess E2E harness (not compiled by the product build)
overlay/000-harness.patch       adds the harness module + the env-gated panic INSIDE the
                                real protected analyst region
overlay/001-negative-control-registration-after-panic.patch
                                moves the α registration AFTER the protected region; the
                                same oracle must wedge again
raw/                            closeout-3 receipts (see raw/README.md)
```

## Reproduce

```bash
# From a checkout of the fix branch (Linux only):
./evidence/analyst-join-321/replay.sh origin/main HEAD

# Optional explicit revisions / log directory:
OUT=/tmp/analyst-join-logs ./evidence/analyst-join-321/replay.sh <baseline-rev> <fix-rev>
```

The script creates two detached scratch worktrees, applies the overlay, and requires:

```text
baseline + blocked stderr + analyst panic   ORACLE observed=Wedged
                                            previous hook entered, not returned
                                            analyst in anon_pipe_write
                                            decode worker in analyst.join
                                            TERMINAL=Completed (D11 already committed)
                                            no DISPOSE-RETURNED

fix + blocked stderr + analyst panic        ORACLE observed=Completed
                                            DISPOSE-RETURNED verdict=Discharged
                                            CRITICAL-PANICS=1
                                            previous hook never entered

fix controls                                healthy -> Discharged
                                            broken stderr (EBADF) + panic -> Discharged
                                            blocked sink, no panic -> Discharged,
                                              CRITICAL-PANICS=0

fix + negative control (registration
moved after the protected region)           ORACLE observed=Wedged again
```

`REPLAY_RESULT=PASS` is the only success line. Every run writes its logs and an
`ENVIRONMENT.txt` (UTC, revisions, rustc, host).

## Scope

- The panic is injected because no product input reaches an analyst panic today. This
  is a latent-liveness defect, not a currently reachable crash.
- Decode/Output are the repository's ports doubles; this proves the session thread/join
  topology, not media decode or a real audio device.
- The diagnostics drainer/facility is not installed in the child; the fix changes which
  hook arm runs, not the draining algorithm.
- Windows, WASAPI and audible output are NOT_RUN. `raw/` receipts are Linux-only.
- The harness is evidence, not a CI gate. The permanent in-tree oracle pins the
  registration fact; the E2E replay is intentionally separated because it needs the
  test-only injection overlay.

## Provenance

The raw receipts were produced by the closeout-3 independent verification on
`main@98ff343` and the fix branch `3af2f48`; both agents' records are in the repository
history for this evidence package. The replay harness at that time injected the panic
immediately before the `catch_unwind`; the versioned overlay now injects it **inside**
the protected region, which is the stricter shape (the fix's catch must return). The
observed outcomes are unchanged.

Authority remains: `docs/architecture/error-system-target-contract.md` §DG-4b
(DESIGN_CANDIDATE / NOT AUTHORITY), `error-system-implementation-record.md` §2/§6/§15
(implementation record), and `error-system-reconciliation.md` N-01 addendum. D11 and
K0 authority are unchanged.
