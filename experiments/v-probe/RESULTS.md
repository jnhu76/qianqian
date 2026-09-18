# V-PROBE RESULTS — V_PROBE_GREEN ×3 (no reopen condition fired)

Campaign: QIANQIAN-F6-NAVIGATION-VOLUME-AUTONOMOUS-1, Stage E.
Authority under test: ADR-PBK-002 D14.9 (the volume amendment's
pending item: the physical realtime apply placement of the
`IAudioStreamVolume` candidate). Branch: `research/v-probe-1`; the committed evidence set was
produced by ONE cross-build from the committed review-fix tree
(exe sha 0d43e2b7…, identical across all three ENV files). Two
earlier evidence sets (exe 06bcff69…, whose V4 measured a cross-thread
call instead of the candidate shape, and whose ENV files honestly
recorded branch: main) are superseded by this re-run; their findings
drove the harness fixes recorded in the commit history.

## Verdict

**V_PROBE_GREEN ×3 — 21/21 scenario-runs GREEN.** Neither reopen tier
fired: no V1a cross-stream coupling, no V2a/V2b factor-writing
violation (so the MECHANISM decision stands), and the V4 measurement
found the loop-top apply bounded and non-perturbing at the measured
granularity (no apply-point/ownership reconsideration is forced).

## Environment

Real Windows 11 host, Realtek High Definition Audio, default render
endpoint (shared mode, event-driven float32 44.1 kHz stereo, exactly
the production open shape). Per-run identities in
`evidence/ENV-RUN<N>.txt`.

## Scenario matrix

| run | V1a | V1b | V2a | V2b | V3 | V4 | V5 |
|-----|-----|-----|-----|-----|----|----|----|
| 1   | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |
| 2   | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |
| 3   | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |

Measured evidence (full JSON per scenario in `evidence/logs/`):

- **V1a isolation**: stream A set to 0.3 then 0.6; stream B sampled
  20× at 50 ms after each set — B held its baseline (±0.01) in every
  sample, both directions, all runs (per-sample arrays committed in
  each JSON, with A's own readbacks).
- **V1b other-process**: a child process (same exe) pinned its stream
  factor at 0.5 and polled 40× over 4 s while this process churned
  0.25↔0.9 — the child observed exactly [0.5, 0.5] in every run.
- **V2a player→mixer**: stream factors set to 0.3/0.7/0.05/1.0; the
  session-master factor never moved (±0.01).
- **V2b mixer→stream**: session master set to 0.3/0.6/0.3/restored,
  with a readback after each write proving the write effective
  (e.g. run1: 0.300/0.600/0.300/1.000); the stream factors stayed
  pinned at 1.0 (±0.01) throughout. The master is restored on every
  exit path via a guard. The AUDIBLE change the master move causes is
  EXPECTED by D14.9 and is an ear-witness item (UNAVAILABLE —
  recorded conditional, same posture as S-PROBE/OPEN-SMOKE).
- **V3 lifecycle**: the stream factor 0.4 survived client Stop →
  Start on the same device in every run (readback recorded, e.g.
  0.40000000596 in run1).
- **V4 apply placement (the decision input)**: the CANDIDATE shape,
  executed by the render-pump thread itself — one relaxed load +
  compare per loop top, `SetAllVolumes` applied BETWEEN the event
  wait and GetBuffer (never inside the quantum), with the main thread
  only routing the desired value into the cell. 200 routed changes at
  30 ms (slower than the ~10 ms loop cadence, so each change lands on
  its own loop top; the designed coalescing of faster routing was
  observed and recorded — 1000 changes at 2 ms produced ~26 applies
  at the cadence):

  | run | applied | median | p99 | max | iteration p99 | position |
  |-----|---------|--------|-----|-----|---------------|----------|
  | 1   | 200/200 | 260.0 µs | 412.4 µs | 490.7 µs | 10.60 ms | advancing, monotone |
  | 2   | 200/200 | 262.2 µs | 431.0 µs | 516.5 µs | 10.53 ms | advancing, monotone |
  | 3   | 200/200 | 251.7 µs | 399.3 µs | 427.5 µs | 10.56 ms | advancing, monotone |

  Every apply ≤ 0.52 ms; the iteration cadence held at the device
  period (p99 ≈ 10.5 ms); the position clock stayed advancing and
  monotone throughout; the stream survived all churn. The candidate
  placement is measured bounded and non-perturbing at this
  granularity, ON the submitting thread.
- **V5 failure signals**: `SetAllVolumes(&[])` fails typed
  (HRESULT 0x80070057 E_INVALIDARG); `GetService` on an uninitialized
  client fails typed (HRESULT 0x88890001 AUDCLNT_E_NOT_INITIALIZED) —
  two distinguishable control-failure signals, so a log-and-pretend
  implementation has no excuse; a setup failure inside the scenario is
  a RED reason, never evidence (the vacuous-GREEN path is closed). The
  device-loss class
  (AUDCLNT_E_DEVICE_INVALIDATED, 0x88890004) was NOT physically
  triggered: its trigger (endpoint disable) requires admin and
  disrupts the host; the frozen D14.9 routing for that class (into the
  existing device-failure policy) is implementation-gated in the
  Stage F slice and is reviewed there.

## Consequence for Stage F

The physical facts D14.9 deferred are now measured: the candidate
mechanism (IAudioStreamVolume via GetService, SetAllVolumes across
all channels, level/100.0) shows factor isolation and independence on
the exercised endpoint, and the candidate apply placement (once at
stream open before first meaningful submission; re-apply at the
render loop top when the routed value changed, on the submitting
thread; never inside the quantum) is measured bounded and
non-perturbing. No reopen condition fired. The D14.10 stop-list entry
(volume realtime apply placement) closes by AUTHORITY, not by this
document: the Stage F implementation PR carries the narrow ADR
amendment recording this evidence as the grounding of the candidate
placement, and that amendment is subject to the same fresh review as
the implementation.

## Boundary

- Proves: factor-level isolation and independence on the exercised
  endpoint, lifecycle persistence, apply-call boundedness and clock
  continuity, and the existence of typed failure signals.
- Does NOT prove: audible-level behavior (no ear witness), behavior on
  endpoints other than the exercised one, or the device-loss routing
  (documented, implementation-gated).
