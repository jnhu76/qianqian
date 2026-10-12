# R-H H0 consolidated remediation — mutant matrix

Subject: `daad414` → branch `test/h0-consolidated-remediation`. All mutants ran in isolated
disposable worktrees (`/home/hoo/Source/qianqian-h0-probe`, `/home/hoo/Source/qianqian-h0-mutant`);
every restore verified (`git checkout -- .` + byte-identity where claimed). Full unfiltered logs
under `/home/hoo/Source/qianqian-h0-evidence/runs/<RUN_ID>/` (metadata: argv, subject sha,
rc, log sha256, toolchain).

Pre-registration rule (#260 G2): the illegal behavior and expected signature were named
BEFORE the repair landed. Deviations are classified inline — a mutant that survives is NOT
washed; it is diagnosed and recorded.

## H0 falsifiers (this campaign's pre-registered mutants)

| ID | Group | Mutation (exact) | Expected signature | Observed | Verdict |
|---|---|---|---|---|---|
| M-P106a | R3B-P1-06 | delete `‖ guard.teardown_released` from the SECOND hold only | new case RED | `ok` — property GREEN | **SURVIVES (classified)**: masked by the first hold's twin conjunct. The two holds are a redundant sample pair for this condition; single-site deletion is property-equivalent. Recorded as redundancy evidence; not a repair gap. Run `20261012T135034-570017-23909`. |
| M-P106b | R3B-P1-06 | delete the conjunct from BOTH holds | new case RED on `seek_in_flight()` | RED at completion.rs:2130 (the new case's assert), `0 passed; 1 failed` | **KILLED** — `20261012T135134-570685-13005` |
| M-P201@base | R3B-P1-03 | delete `request_resume()` in `s0_a_paused_episodes_quiesced_park_commits_the_cut` at `daad414` | forever-park (suite hang) | test running >60s at external `timeout 90` kill, rc=124 | **HANG WITNESSED** (the defect) — `20261012T134908-568785-27163` |
| M-P201@HEAD | R3B-P1-03 | same mutant at the fixed HEAD | named bounded RED | `operation exceeded 10s` (watchdog, common/mod.rs:1033), FAILED in 10.00s | **FAILURE-BOUNDED** — `20261012T140447-572905-25150`. The watchdog is the failure bound, NOT a correctness oracle; the semantic oracles are untouched. |
| P-P105 probe | R3B-P1-05 | NOT a mutant: premise probe inserting `consume_release()==Some(Committed{42})` right after the illegal `clear_seek_in_flight()` at `daad414` | assertion HOLDS ⇒ payload unconsumed at the free point (defect witnessed) | GREEN with the probe assert passing | **DEFECT WITNESSED** — `20261012T134814-568156-31126` |
| Twin: PartiallyLeakingCopyStage | R3B-P2-01 | same-interface stage copying exactly quantum 2 of 3 into fresh leaked storage | strengthened all-blocks oracle RED; old last-block sample would PASS (control in-test) | kill test GREEN (= twin caught, in-repo permanent falsifier); controls: `first_block_storage_addr == flow`, `observed_storage_addr() == flow` (last block honest) | **KILLED (in-repo)** — `partial_middle_block_storage_substitution_is_killed_by_storage_identity`, run `20261012T135753-571385-24750` |

Correction note (kept per the failure-evidence contract): the FIRST pre-registration of the
R3B-P1-06 falsifier was single-site (M-P106a) and it SURVIVED. Per that contract the
preserved log was diagnosed before any rerun; classification: oracle/premise mismatch
(redundant twin conjunct), not a production defect. The pre-registration was then widened to
both holds (M-P106b) and killed. Both runs are retained.

## Structural hygiene groups (R3A-A1/A2/A3/A6, R3A-P2, R3B-P1-05)

No semantic mutants are claimed for these (their defects live on failure paths a green run
never reaches, or are premise/fixture legality). Evidence class = repair + focused green +
the PR #254/#257 precedent that the release-before-join idiom kills its causal mutant (H3
CAUSAL_KILL_PROVEN, value discriminant; not re-proven here — no duplicated generic runtime
was introduced).

## Registered negative controls re-run at this HEAD (W4-2 duty)

| Control | Mutation | Expected | Observed at H0 HEAD | Verdict |
|---|---|---|---|---|
| PR #257 M2 | delete `‖ guard.worker_gone`, second hold (ordinal 2 of 2, one Open sample between) | straddle witnesses RED | BOTH registered witnesses FAILED: `an_open_sampled_seek_straddling_the_worker_exit_is_never_accepted` (completion.rs) + `a_seek_straddling_the_real_worker_exit_funnel_is_never_accepted` (session.rs) | **RED, control intact** — `20261012T141018-573453-23204` |
| PR #257 M3 | delete both `worker_gone` conjuncts (ordinals 1+2) | straddle witnesses RED | both FAILED as above | **RED, control intact** — `20261012T141033-573799-14367` |
| restore proof | — | byte-identical tree | `git checkout -- .`; `completion.rs` sha256 `2324002418535356dd5a5cd61974a2c19481b514c0bc8b527f9ece739d1c65c0` == `git show 45edeac:…` | **VERIFIED** |

- specs/f5 M1 / episode-terminal M8/M10 (TLA+/TLC): **NOT_RUN** — `tla2tools.jar` is not
  cached locally (specs/tools absent; download requires network). Scope note: this PR changed
  no `.tla`/`.cfg`/formal artifact, so the frozen models' subject is byte-identical to
  `daad414` and the existing formal evidence remains applicable to the unchanged models.
  Recorded NOT_RUN, never reported as PASS.
- Loom lane at HEAD: run `final-head-loom-lane` (see execution evidence index).
