# V-PROBE RESULTS — V_PROBE_GREEN ×3 (no reopen condition fired)

Campaign: QIANQIAN-F6-NAVIGATION-VOLUME-AUTONOMOUS-1, Stage E.
Authority under test: ADR-PBK-002 D14.9 (the volume amendment's
pending item: the physical realtime apply placement of the
`IAudioStreamVolume` candidate). Branch: `research/v-probe-1`,
commit 74cd004; the exe was cross-built ONCE from the committed tree
(exe sha 06bcff69…, identical across all three ENV files).

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
  sample, both directions, all runs.
- **V1b other-process**: a child process (same exe) pinned its stream
  factor at 0.5 and polled 40× over 4 s while this process churned
  0.25↔0.9 — the child observed exactly [0.5, 0.5] in every run.
- **V2a player→mixer**: stream factors set to 0.3/0.7/0.05/1.0; the
  session-master factor never moved (±0.01).
- **V2b mixer→stream**: session master set to 0.3/0.6/0.3/restored;
  the stream factors stayed pinned at 1.0 (±0.01) throughout. The
  AUDIBLE change the master move causes is EXPECTED by D14.9 and is
  an ear-witness item (UNAVAILABLE — recorded conditional, same
  posture as S-PROBE/OPEN-SMOKE).
- **V3 lifecycle**: the stream factor 0.4 survived client Stop →
  Start on the same device in every run.
- **V4 apply placement (the decision input)**: 2000 alternating
  `SetAllVolumes` calls at a render loop top (wait → pad check →
  submit; the call runs BETWEEN submissions, never inside one):

  | run | median | p99 | max | position clock |
  |-----|--------|-----|-----|----------------|
  | 1   | 78.9 µs | 116.9 µs | 504.4 µs | monotone |
  | 2   | 80.2 µs | 130.3 µs | 236.8 µs | monotone |
  | 3   | 79.0 µs | 131.5 µs | 592.1 µs | monotone |

  Every call ≤ 0.6 ms; the position clock never went backward; the
  stream survived all churn. The candidate placement (apply once at
  stream open + re-apply at the loop top when the routed value
  changed) is measured bounded and non-perturbing at this granularity.
- **V5 failure signals**: `SetAllVolumes(&[])` fails typed
  (HRESULT 0x80070057 E_INVALIDARG); `GetService` on an uninitialized
  client fails typed (HRESULT 0x88890001 AUDCLNT_E_NOT_INITIALIZED) —
  two distinguishable control-failure signals, so a log-and-pretend
  implementation has no excuse. The device-loss class
  (AUDCLNT_E_DEVICE_INVALIDATED, 0x88890004) was NOT physically
  triggered: its trigger (endpoint disable) requires admin and
  disrupts the host; the frozen D14.9 routing for that class (into the
  existing device-failure policy) is implementation-gated in the
  Stage F slice and is reviewed there.

## Consequence for Stage F

Per D14.9, with V_PROBE_GREEN the pending item closes: the candidate
mechanism (IAudioStreamVolume via GetService, SetAllVolumes across
all channels, level/100.0) and the candidate apply placement (once at
stream open before first meaningful submission; re-apply at the
render loop top when the routed value changed; never inside the
quantum) are now physically grounded. Stage F implements the frozen
owner/semantics (App-owned desired 0..=100, step 5, episode seam
command, stream-local realization) against these facts.

## Boundary

- Proves: factor-level isolation and independence on the exercised
  endpoint, lifecycle persistence, apply-call boundedness and clock
  continuity, and the existence of typed failure signals.
- Does NOT prove: audible-level behavior (no ear witness), behavior on
  endpoints other than the exercised one, or the device-loss routing
  (documented, implementation-gated).
