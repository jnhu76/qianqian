# R-H H0 consolidated remediation — property graph and decision tables

Campaign: #260 (ONE_PR_MULTIPLE_COMMITS) · Subject: `daad414582b2a31eddc4a5b3fdb85bcce4bd60b0`
… → see `h0-before-after-ledger.csv` (18 rows), `h0-mutant-matrix.md`,
`h0-execution-evidence-index.md`, `h0-lessons-learned.md`.

This document is non-authoritative execution evidence (per #260 §3: not a product ADR).
Every property's normative authority stays in `ADR-PBK-002` (D14.5/D14.7/D14.8, D11).

Statuses used: `FIXED_AND_FALSIFIED`, `FIXED_HYGIENE_VERIFIED`,
`KEEP_WITH_EXPLICIT_OWNER_DEBT`, `AUTHORITY_BLOCKED`, `NEEDS_REVIEW` (#260 §3).

## Group R3A-P2 — stop-from-established-pause premise (1 test)

- **Property.** Stop from an established pause settles the episode `Stopped`; late
  pause/resume commands stay inert command history; settlement is never relabelled (D11).
- **Causal DAG (timing-bearing premise).** `registered_runtime(source)` → episode playing →
  `request_pause` → engagement + current-engagement tail quiescence → `Paused` projection →
  `request_stop` → gate release → leg reaches stopped edge → `Stopped` fact.
- **Decision table (legal outcomes).** terminal=`Stopped`; `pause_requested` stays recorded;
  `paused()==false` after settle; dispose quiet.
- **Defect (pre-fix).** The source fixture `seconds_of_audio(30)` (`EofAfter(1_323_000)`) is
  capacity-relative against decode drain scheduling: on a slow machine the decode could drain
  toward EOF while the pause established, and an EOF-completed settle would false-RED the
  `Stopped` oracle (premise-by-timing, latent).
- **Repair.** `endless_audio(30)` (`Paced`, never returns Eof while the episode lives) — the
  "still playing at stop" premise is structural, not timing-based. 1 line.
- **Falsifier.** None claimable (premise correction): the Paced source's endlessness is
  structural, so no reachable mutant can turn the premise false without changing the fixture.
- **Disposition.** `FIXED_HYGIENE_VERIFIED`.

## Group R3B-P1-05 — seek-slot freed with an unconsumed committed payload (1 test)

- **Property.** Per-cycle operation evidence (landing/commit) cannot cross seek cycles; a
  cleared park grounds nothing (D14.5 one-seek slot).
- **Causal/state table.**

  | step | production-legal order (session.rs:743-768) | pre-fix fixture order |
  |---|---|---|
  | commit routes `Committed{42}` | yes | yes |
  | leg consumes payload | yes (slot waits for `seek_release_pending()` to clear) | **skipped** |
  | `SeekDisengaged` | yes | yes |
  | slot freed | after consumption observed | **with payload still routed** |

- **Defect (pre-fix).** The fixture called `clear_seek_in_flight()` while cycle 1's
  `Committed{42}` payload was still routed — a production-forbidden state — and the comment
  claimed "release consumed". Cycle 2's resets were thus proven against debris.
- **Repair.** Consume+assert `Committed{42}` then `None` BEFORE `SeekDisengaged` + slot free;
  truthful comment. The probe run at baseline (payload still routed at the free point,
  `20261012T134814-568156-31126`) is the defect witness; the post-fix test traverses the legal
  transition with unchanged cycle-2 oracles.
- **Falsifier.** The pre-fix probe IS the witness (it demonstrates the illegal state exists);
  no semantic mutant is claimable for a fixture-legality repair.
- **Disposition.** `FIXED_HYGIENE_VERIFIED`.

## Group R3B-P1-06 — `teardown_released` conjunct oracle-less (1 test)

- **Property.** Seek acceptance fails closed on every frozen second-hold condition (D14.5).
- **Decision table of acceptance conditions** (each must reject the plant):
  no data plane · non-Open edge (EOF window) · failed edge · recorded stop intent ·
  **teardown released (`release_pause_gate`)** · settled episode. Positive control: paused
  episode is seekable.
- **Defect (pre-fix).** The `teardown_released` conjunct (both holds) had no oracle at this
  seam; `release_pause_gate` had zero test callers at `daad414` (production caller only,
  session.rs:433; the E2E `teardown_gate_precondition.rs` covers a different seam). The test
  name over-claimed "every".
- **Repair.** One `release_pause_gate()` negative case beside the five existing classes.
- **Falsifier (pre-registered).** Delete `|| guard.teardown_released` → the new case must go
  RED on its registered semantic assertion (`seek_in_flight()` true).
  - M-P106a (second hold only): **SURVIVES** — masked by the first hold's twin conjunct.
    This is recorded evidence that the two holds are a redundant sample pair for this
    condition; single-site deletion is property-equivalent.
  - M-P106b (both holds): **KILLED** — RED at the new case (`20261012T135134-570685-13005`).
- **Disposition.** `FIXED_AND_FALSIFIED`.

## Group R3B-P2-01 — storage-identity sampled at the last block only (1 test)

- **Property.** The honest pre-bound flow delivers every block through the flow's own
  reusable block storage — no per-quantum intermediate storage on ANY block (PCM realtime
  firewall, data-plane evidence).
- **Causal DAG.** `run_quantum`: source lends into the flow arena → stage `process` → sink
  `consume`. A compliant stage returns a view over the same storage (reborrow ⇒ same
  address); a substituting stage returns a view over different storage (actual copy ⇒
  different address). Address identity is the copy/reborrow discriminator.
- **Defect (pre-fix).** `VerifyingSink` overwrote its single `observed_storage_addr` per
  block, so only the LAST block was witnessed; a stage copying some-but-not-all quanta passed.
- **Repair.** O(1) allocation-free all-blocks witness (`StorageIdentityWitness`:
  first-block address + later-block-on-different-storage flag + block count) on the test-local
  `PcmSink` trait; the honest-path oracle asserts it over all 8 quanta. A per-block `Vec`
  witness was tried and REJECTED by the binary's own zero-steady-state-allocation oracle
  (failed attempt `20261012T135455-571083-473`, measured 5 allocations; preserved).
- **Falsifier (pre-registered).** `PartiallyLeakingCopyStage` — a same-interface twin copying
  exactly ONE mid-stream quantum into fresh leaked storage; the strengthened oracle must RED
  while the twin's honest final block satisfies the old last-block sample (control asserted
  inside the kill test `partial_middle_block_storage_substitution_is_killed_by_storage_identity`).
  A legitimate reborrow (IdentityStage) keeps every address equal and stays GREEN.
- **Disposition.** `FIXED_AND_FALSIFIED`.

## Group R3A-A1 — detached witness leg on the failed-observation path (1 test)

- **Property.** A failed tail observation ends the seek park reported (`TailProbeFailed`) and
  settled (abort payload via the ONE exit funnel; no quiescence published).
- **Failure-path DAG (pre-fix).** premise `wait_until(parked)` fails ⇒ panic ⇒ spawned leg
  detached (handle discarded) — leg runs/parks until process exit. `recv_timeout` fails ⇒
  same stranding.
- **Repair.** Handle retained; both failure paths route through the file-local
  `fail_premise_releasing_leg` (release → bounded `is_finished` → join → panic carrying
  collection evidence), generalized over the leg payload type.
- **Falsifier.** Structural hygiene (release-before-join on unwinding paths); the idiom's
  mutation-killing bite is the PR #254 H3 precedent (CAUSAL_KILL_PROVEN), not re-proven here.
- **Disposition.** `FIXED_HYGIENE_VERIFIED`.

## Group R3A-A2 — six seek-park tests: bare premise asserts + unbounded joins (6 tests)

- **Properties.** (unchanged, one per test): park publishes only Seek events in order;
  committed payload consumed exactly once at the park exit; committed cut rebases a
  pause-parked leg MID-PARK; a new hold wipes a stale unconsumed release; a closed gate never
  seek-parks again; the tail observation runs only while parked.
- **Failure-path DAG (pre-fix, common shape).** premise assert fails ⇒ parked leg stranded
  (no release before unwind); post-release `join()` unbounded ⇒ broken-release regression
  hangs the suite.
- **Repair.** Per premise: `fail_premise_releasing_leg` (seek-held legs) or the new
  `fail_premise_releasing_pause_leg` (the pause-parked leg whose only exit is the pause
  release — a seek release alone is consumed MID-PARK and the leg keeps parking). Per join:
  `join_released_leg` (bounded `is_finished` witness + named panic). Every existing semantic
  oracle preserved verbatim.
- **Falsifier.** Same structural class as R3A-A1.
- **Disposition (all 6).** `FIXED_HYGIENE_VERIFIED`.

## Group R3A-A3 — render_gate park joins unbounded (2 tests)

- **Properties.** Full park cycle publishes engagement/quiescence/disengagement once each and
  returns; each NEW engagement re-observes quiescence (stale latch never satisfies a later
  pause).
- **Failure path.** The four post-release `join()`s (one + three) are unbounded; a
  release-failing gate hangs the suite at the first join.
- **Repair.** File-local `join_released_leg` (bounded witness + named panic); releases still
  precede every join; premise polls were already bounded with messages.
- **Disposition (both).** `FIXED_HYGIENE_VERIFIED`.

## Group R3A-A6 — DestructorGate: no release on failure paths, unbounded entry wait (1 test)

- **Property.** Physical destruction of the retired view runs OUTSIDE the publication lock —
  acquire progresses while a destructor is blocked.
- **Failure-path DAG (pre-fix).** Destructor blocked in `enter_and_wait`; any later assert's
  unwind skips `release()` ⇒ control thread blocked until process exit. `wait_entered`
  unbounded ⇒ a mechanism that never drops the view hangs the suite. (Under a REGRESSED
  lock-holding destructor, the probe would additionally block on the publication mutex — the
  property the test exists to pin.)
- **Repair.** `wait_entered_bounded` (named premise failure), `GateReleaseGuard` RAII release
  held by the test (covers unwind while the blocked destructor holds its own Arc clone — the
  case a `Drop` impl provably cannot fire in), and a non-panicking `Drop` for DestructorGate
  for shapes where the last handle drops while a destructor waits. Scope of each mechanism is
  documented on the type (an honest deviation from the C4 "Drop release" shorthand, recorded
  here and in the lessons document).
- **Falsifier.** Harness lifecycle hygiene; green path unchanged.
- **Disposition.** `FIXED_HYGIENE_VERIFIED`.

## Group R3B-P1-03 — consume_release forever-park without a module watchdog (1 test)

- **Property.** A paused episode's quiesced park commits the cut on the pause-attributed
  park pair; the committed payload is consumed; no routed debris.
- **Failure-path DAG.** `consume_release` drives a REAL `park_loop_top` with an
  always-Pending probe. Pre-fix, a regression keeping the pause routed parks it FOREVER and
  the module has no watchdog ⇒ suite hang. Witnessed at baseline by the pre-registered
  deletion mutant (M-P201): external `timeout 90` reached with the test still running
  (`20261012T134908-568785-27163`, rc=124).
- **Repair.** Body runs under the shared `within(10s)` (in-file idiom from PR #257). The
  watchdog bounds FAILURE REPORTING ONLY — it is not a correctness oracle and must never be
  reported as one; the semantic oracles are untouched. The same mutant at the fixed HEAD
  produces the named RED `operation exceeded 10s` in 10.00s
  (`20261012T140447-572905-25150`).
- **Disposition.** `FIXED_HYGIENE_VERIFIED`.

## Group R3A-P1 — teardown-release linearization vacuity window (1 test) — DECISION_REQUIRED

- **Property.** A pause that linearizes AFTER the teardown release must not re-park the leg
  or wedge `stop_and_join`.
- **Defect (standing).** The release-before-hostile-pause premise rests on `sleep(100ms)`
  (pause_seam.rs:811 at the frozen subject); no public witness of `release_pause_gate`
  exists on the integration seam. Extreme scheduling degrades the test to the safe
  interleaving (vacuous pass); the test's own comment admits this.
- **Investigation (this campaign).** `RenderGate`'s public surface (ports.rs:391-555 at
  `daad414`) exposes no paused-state reader; `seek_release_pending()` witnesses seek payload
  awaits only. The only deterministic routes are (a) a white-box `teardown_released`
  suspension/witness seam in the production teardown path — exactly what OQ-1 gates — or
  (b) accepting the vacuity window as debt. #260 G0 forbids inventing test-access production
  API to meet "18 fixes".
- **Disposition.** `AUTHORITY_BLOCKED` — `DECISION_REQUIRED (OQ-1)`: fund a white-box
  teardown-release witness, or register the vacuity window as accepted debt. NO code change;
  ruling history preserved; this PR must not be represented as fixing this test.

## Group R3A-P3 — wall-clock slot-free settle, two tests — owner debt standing

- **Property (test 1).** A committed cut resolves while pause-parked so the paused episode is
  seekable AGAIN; the second cut rebases mid-pause to its own landing.
- **Property (test 2).** An unknown landing commits (stale exclusion independent of landing
  knowledge) and withdraws the Position projection for the rest of the episode.
- **Defect (standing).** Both fire cut #2 after a 200ms wall-clock sleep (~100x the 2ms
  worker poll slice): the rebase becomes observable when the parked leg consumes the payload,
  but the one-seek slot frees only at the worker's NEXT bounded poll — the premise is
  premise-by-timing (Windows timer granularity can inflate the slice; the in-comment
  third-cutover observation proves a retry loop is wrong in both directions: inert inside the
  window, a legal-but-unintended third cutover after slot-free).
- **Investigation (this campaign).** No legal deterministic slot-free witness exists on the
  public seam: `seek_release_pending()` (ports.rs:478) flips at consumption, one poll BEFORE
  the slot frees; `seek_in_flight()` is crate-test-only; a public slot-free ack would be a
  new slot API, which #260 §1 forbids for this campaign.
- **Disposition (both).** `KEEP_WITH_EXPLICIT_OWNER_DEBT` — OQ-6 keep-with-debt ruling
  preserved (not revoked); investigation recorded for the owner's re-confirmation at the
  next re-baseline. NO code change.

## Sum

`1+1+1+1+1+6+2+1+1+1+2 = 18` tests across 11 property groups. Disposition counts:
`FIXED_AND_FALSIFIED 2 · FIXED_HYGIENE_VERIFIED 13 · KEEP_WITH_EXPLICIT_OWNER_DEBT 2 ·
AUTHORITY_BLOCKED 1 · NEEDS_REVIEW 0`.
