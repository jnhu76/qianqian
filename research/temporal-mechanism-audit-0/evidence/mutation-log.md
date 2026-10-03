# Mutation / negative-control log — temporal-mechanism-audit-0

> RESEARCH EVIDENCE for the PR #196 adversarial audit. Every mutation
> was applied in an isolated git worktree created from the exact PR
> head 5d1426112937facb1a1643acbd3651c4a6fb3d8f, run, recorded, and
> restored (`git checkout --`); nothing was committed anywhere. Main
> tree untouched. Host: 20-core Linux/WSL2.

## S0 negative controls (executed by the auditor; worktree /tmp/qn-s0mut)

Command per row: `cargo test -p qianqian-playback --lib -- s0_operation_evidence`

| Mutant | Mutation site | Result | Evidence |
|---|---|---|---|
| M-S0a | completion.rs request_seek: `guard.seek_landing = None;` removed (acceptance-time reset) — stale landing latches cross seek cycles | **RED** | `failures: completion::tests::s0_operation_evidence_cannot_cross_cycles_and_cleared_parks_ground_nothing` — `test result: FAILED. 0 passed; 1 failed` |
| M-S0b | completion.rs apply_gate_event: `GateEvent::SeekDisengaged` arm emptied — stale park evidence survives the park exit | **RED** | same probe FAILED (`!leg_parked_evidence()` assertion) |

Both are the §23-required "stale operation evidence survives into next
seek" mutants; both are caught by the cross-cycle probe.

## F3 mutants (mutation agent; worktree /tmp/qn-mut; engine-side only)

Scenario baseline on this host: as-shipped distribution 15/15 no-cut
(the idle 20-core host never realizes a cut ending — see P3-E in
RESULTS.md §24). Mutants whose kill depends on a cut ending were
additionally run with a 50ms `SourceBehavior::Paced` strengthener that
makes the cut ending deterministic; both pure distributions were
verified GREEN for the oracle (all-no-cut as shipped; all-cut under
the strengthener) before killing.

Command per row: `cd /tmp/qn-mut && cargo test -p qianqian-playback --lib s0_replay_the_seek_stop_race 2>&1 | tail -14`

| Mutant | Mutation (file:line) | Result | RED evidence |
|---|---|---|---|
| M1 stale pre-cut frame after cut | tests/common/mod.rs:171-257 — `TestDecodeStream` gains `stale_inject`: the Applied seek records the pre-cut cursor; the first post-cut read emits that stale frame | as-shipped GREEN (17.30s) — mutant never activates (no cut ending); with 50ms strengthener **RED** (1.21s) | `panicked at live_tests.rs:2927: post-cut stretch: frame 0 = 1 is not the exact tag ramp` |
| M2 duplicate one segment | tests/common/mod.rs:171-228 — `dup_done`: once the cursor reaches 1500 (inside the consumed window), rewind 1024 once | **RED** (0.61s, first unloaded iteration) | `panicked at live_tests.rs:2923: post-cut stretch: frame 0 = 476 is not the exact tag ramp` |
| M3 drop one cut-boundary frame (post-cut side) | tests/common/mod.rs:245-248 — Applied arm: `cursor = landing + 1` (classic decoder priming-skip defect); 50ms strengthener | **RED** (1.18s) | `panicked at live_tests.rs:2927: post-cut stretch: frame 0 = 1 is not the exact tag ramp` |
| M3' drop one pre-cut-side boundary frame | (analytical variant of M3) drop the LAST pre-cut frame: `[0..k−1][0..m]` still satisfies both exact-ramp asserts and the >1000 step test | **GREEN by analysis** — genuine oracle blindness for this variant (P3-C). Frame conservation at the cut is pinned elsewhere: eq_tests.rs:1147-1151 asserts `values.len() == cut + landing_frames` exactly. | — |
| M4 produce two cuts (as placed) | tests/common/mod.rs:171-228 — `restarts`: landing applied at 1500 twice (also tried 1500/1800, 600/1200) | **SURVIVES** (8.87s / 8.72s) — one visible exact restart-to-0 is content-identical to the LEGAL seek cut; the second restart falls beyond the stop truncation (~2050 consumed frames) so it never reaches the witness (P2-B scope bound) | — |
| M4' two cuts, both observable | cut at 1500 + second restart at post-cut cursor 1000 (inside the witness); 1500ms strengthener | **RED** (2.11s) | `panicked at live_tests.rs:2927: post-cut stretch: frame 1000 = 0 is not the exact tag ramp` |
| M5 mis-land by one frame | src/live_tests.rs:2950 — script `Applied { landing: Some(0) }` → `Some(1)`; 50ms strengthener | **RED** (1.24s); as-shipped GREEN (no cut ending to mis-land) | `panicked at live_tests.rs:2927: post-cut stretch: frame 0 = 1 is not the exact tag ramp` |

## F4 mutants (mutation agent; worktree /tmp/qn-mut; engine-side)

Command per row: `cargo test -p qianqian-playback --lib s0_replay_mutated_then_failed`

| Mutant | Mutation | Result | Evidence |
|---|---|---|---|
| F4-attempt-1 (scope-bound probe): worker resumes production post-Failed without failing the plane | session.rs:568-580 mutant form: `release_seek_without_commit(); decode_failed(...); pending_seek = None;` (no `edge.fail()`, no return) + mock `MutatedThenFailed` arm repositions `cursor` to the 5s target (tests/common/mod.rs:254) | **GREEN (0.32s)** — informative scope bound (P3-F): the new-cursor frames queue behind the 8192 buffered pre-failure frames and dispose()'s edge stop abandons them before the 1ms-paced leg drains them; the prefix law is a consumed-content law, exactly as the test message words it ("reached the device") | baseline run: `s0 eq replay: consumed at Failed fact = 23040, final after join = 23040` |
| **F4-REQUIRED: destructive seek actually applies, new-cursor content reaches the consumer** | session.rs:568-611 mutant form: the arm runs the applied-cut obligations (`remainder = None; processing.invalidate_signal_history(); edge.invalidate();`), refills the edge from the repositioned cursor via a read/stage/write loop, THEN `release_seek_without_commit() + decode_failed(...)`; mock cursor repositioned as above | **RED at a deterministic index (0.48s)** | `panicked at eq_tests.rs:1618: the FINAL consumed content must be exactly the control's prefix — ... first divergence at index 22528: actual=223420.42, control=22611.623` (223420.42 = EQ of the mutated new-cursor tag ~220500 = the 5s target; the stream is bit-exact before the jump) — and under the same mutant both pre-existing family oracles also RED: `a_mutated_then_failed_seek_never_reconstructs_eq_continuation` + `stateful_probe_tests::a_mutated_then_failed_seek_never_reconstructs_the_old_continuation` (`2 failed; 0 passed`) |

## Baselines at the PR head (all GREEN, /tmp/qn-mut before mutations)

```text
cargo test -p qianqian-playback --lib s0_replay_the_seek_stop_race   1 passed; 17.19s
cargo test -p qianqian-playback --lib s0_replay_mutated_then_failed  1 passed; 0.28s
cargo test -p qianqian-playback --lib a_mutated_then_failed          2 passed; 0.71s
```

## under_cpu_load leak reproduction (independent)

A standalone reproduction of the pre-fix helper (short thread names to
survive the 15-byte comm truncation) confirmed: after normal return
both spinners gone; after a body panic both spinners stayed alive
burning CPU. Fixed by commit e453bc8; the fix's panic path was
verified by a scratch suite (`panicking_body_stops_the_spinners`,
`finished in 0.00s`, no `qianqian-s0-load-*` thread alive after).
