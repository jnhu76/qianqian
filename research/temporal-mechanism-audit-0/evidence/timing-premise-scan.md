# Repository-wide timing-premise scan — temporal-mechanism-audit-0

> RESEARCH EVIDENCE. Scan agent's full inventory (PR #196 head
> 5d14261; read-only). Classification vocabulary per the audit
> commission §18. This file is the recording of record for the
> residual scheduler-premise debt; see RESULTS.md §18 for the summary.

Method: grep sweep over crates/ for `sleep(`, `thread::sleep`,
`from_millis/from_secs/from_micros`, `wait_until`, `within(`,
`timeout`/`recv_timeout`, `consumed`, `processed`, negation idioms;
full reads of every candidate file. Load-bearing semantic fact used
throughout: a worker-failure origin settles `Failed` immediately,
without the drain verdict (completion.rs `resolve()`); a Stopped/
device-Failed settles only from `DrainVerdict::Aborted`, published
after the consume loop exits. This separates "wagered" freezes from
"anchored" freezes.

## Summary counts

```text
LIVENESS_BOUND                    ~120 sites (all legitimate)
PERFORMANCE_MEASUREMENT           2 harnesses
DIAGNOSTIC_ONLY                   6
SAFETY_ORACLE_WITH_EXPLICIT_ACK   ~70
SCHEDULER-PREMISE_RISK            7 discrete sites + 1 family (~50 sites)
```

## Ranked residual risks (SCHEDULER-PREMISE_RISK)

| # | Rank | Location | Premise | False-RED likelihood | F1–F4-equivalent? |
|---|---|---|---|---|---|
| 1 | P2 | stateful_probe_tests.rs:712-719 | 300ms sleep ≻ legal post-failure drain (Failed settles from `worker_failure`; leg still draining) | High under full-suite load — F4 verbatim in a sibling file | YES — recorded in PR RESULTS.md §21 (medium) |
| 2 | P2 | gain_tests.rs:299-305 | same | High under load | YES — recorded in PR §21 |
| 3 | P2 | tests/seek_seam.rs:529-540 | same (decode-failure Failed) | High under load | YES — recorded in PR §21 |
| 4 | P2 | tests/seek_seam.rs:987 | `wait_for_position_past(7s)`: poll must land inside ~170ms before terminal withdraws `position` | Moderate (one >170ms descheduling event in a specific window) | F3-family — **NOT in PR §21; recorded by this audit** |
| 5 | P3 | tests/seek_seam.rs:444 | 200ms covers the one-seek slot-free worker slice | Low (~100× nominal margin) | weaker F2/F3 shape; recorded |
| 6 | P3 | tests/seek_seam.rs:577-592 | 200ms covers commit→payload-consumption→mid-pause rebase | Low | recorded |
| 7 | P3 | tests/seek_seam.rs:701 | same as 5 | Low | recorded |
| 8 | P3 | src/live_tests.rs:1491 (n4) | 400ms window for the wall-clock-ramp mutant to diverge | Low (negative-control gate) | not recorded |
| 9 | P3 | tests/position_seam.rs:762-766 | unsettled-snapshot window in the terminal-withdrawal matrix (~256ms paced source) | Very low | not recorded |
| 10 | P3 (family, ~50 sites) | position-threshold entry waits: gain_tests 56-60/108-112/202-206, stateful_probe 323-327/488-495/591-598/676-683/757-764, eq_tests 1030/1112-1121/1202/1311, live_tests 416…2902, seek_seam 281/588/598/685/710 | poll must land before terminal withdrawal of `position` | Low per site (windows ≈90%+ of wall at SlowConsume paces) | F3-family, mitigated; not recorded as a class |

## Analytical refinement

A consumed-freeze after `Failed` is a wager ONLY when the Failed
settles from `worker_failure` (decode/processing origin — sites 1-3).
When it settles from the drain verdict (device origin, e.g.
seek_seam.rs:917-922), the freeze is already anchored: the leg exited
before the Fact published, so `consumed` is structurally frozen and
the sleep is harmless redundancy. This distinction explains why three
sites can flake and their look-alikes never will, and should steer the
future repairs.

## PR §21 residual-list accuracy

All six PR-named entries verified verbatim at the cited locations and
fairly graded. Misses: item 4 above (P2) and class-level recording of
items 8-10 (P3). The pause-negation rows (live_tests.rs:858-874,
1032-1038) are ack-anchored (Paused = TailQuiesced + full-edge
witness) — structurally frozen, false-PASS direction only, NOT
false-RED; same for their unnamed siblings (pause_seam.rs:480-486,
position_seam.rs:552-562/916-934, seek_seam.rs:770-775/941-942,
edge_lifecycle_tests.rs:79-83/146-147/166-167,
settlement_contract_tests.rs:402-406).

## Harness facts

`processing_support::wait_until` returns `false` on timeout and does
NOT panic; every call site in the tree asserts the result (no
silent-ignore found). `tests/common/mod.rs::within` catches and
re-raises body panics via `resume_unwind` — which is why the
pre-fix `under_cpu_load` leak path was the COMMON failure path, not
an exotic one (fixed by commit e453bc8).

## The two NEW PR tests' hygiene (both CLEAN)

- eq_tests.rs `s0_replay_mutated_then_failed_prefix_law_...`
  (1566-1637): observation anchored by dispose() join; schedule-free
  prefix law failing at a deterministic index; witness/counter
  agreement structural; distribution prints diagnostic-only;
  `under_cpu_load(4)` documented diagnostic load; `within(60s)`
  liveness bound.
- live_tests.rs `s0_replay_the_seek_stop_race_...` (2886-2980): the
  entry wait cannot lose to terminal withdrawal (paced source never
  EOFs before the test's own stop — the race designed out, not slept
  over); membership oracle by content; terminal `Stopped` asserted for
  both endings; distribution println-only.
