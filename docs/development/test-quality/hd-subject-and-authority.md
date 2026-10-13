# H-D subject and authority — ADR-driven disposition

Non-normative audit/evidence derived from existing accepted authority. This
record changes neither ADR nor the frozen execution model or 918-row inventory.

```text
MODE = ADR_DRIVEN_H_D_DISPOSITION
BASE_MAIN_SHA = bc0b317b4f502f986d8dc480b8b0d4e29c7bf525
READ_ONLY_SUBJECT_SHA = bc0b317b4f502f986d8dc480b8b0d4e29c7bf525
REMOTE_FETCH = origin/main re-fetched; unchanged from task reference
H_D_INITIAL = 3
MEANING_DECIDED = YES (all three; before Rust edits)
AUTHORITY_DECISION_REQUIRED = NONE
RESOLVED_BY_EXISTING_AUTHORITY = ALL_THREE
```

## Frozen objects and authority reconciliation

The source positions below refer to the read-only subject; use symbols after
removal shifts lines. Exact blobs, SHA256, before/after sources, full run logs
and commands are retained in the accompanying execution evidence.

| H_D_ID | Effective test identity (default non-loom Linux; no device) | Authority | Source at subject |
| --- | --- | --- | --- |
| R3A-H4 | qianqian-headless lib / machine_input::tests::preseal_failure_preserves_natural_completed_and_runtime_failed | PBK-001 §2.2–2.3; PBK-002 D11 (including 2026-09-15 commit corrective), D14.2–.4; execution model D3, §6–9 | apps/headless/src/machine_input/tests.rs:206; machine_input.rs:53/84/156/166/180; completion.rs:548/1277/1294; entry.rs finish_episode |
| R3B-P1-04 | qianqian-playback lib / completion::tests::a_prior_cycle_disengagement_never_answers_a_later_cycle_after_reengagement | PBK-002 D14.7, including 2026-09-17 AUTHORITY-CORRECTIVE removing Resumed; D14.5 current-cut attribution; PBK-001 §2.3 | completion.rs:146/207/391/598/1229/1984; handle.rs:227; audio-api ports.rs pause_park |
| R3B-P1-02 | qianqian-playback integration seek_seam / seek_acceptance_racing_worker_eof_settles_without_wedge | PBK-002 D14.5 implementation corrective-1 C2 and later correctives; D11; temporal semantics §6.1; frozen execution model §6–7, D5/D6 | seek_seam.rs:1237; session.rs decode_worker, :921–922; completion.rs:681/1068/1087 |

PBK-002's later D14.5 correctives (data-plane ending checks, atomic cut decision,
Failed tail escape), D14.7 engagement/teardown corrections and D14.11 processing
failure/seek obligations are present in the current authority. None reinstates
Resumed, requires a disengagement bool, forbids late Stop history, or removes
worker exit cleanup. The execution model's D1–D6 are ACCEPTED #209/FROZEN #210;
historical “proposed” wording in its inherited tables is read with that status.
No OPEN semantics is selected by this work.

Original classification: [#258](https://github.com/jnhu76/qianqian/issues/258)
918 rows at daad414, OQ-2 H4 pin/OQ-3 latch/OQ-4 source order;
[#259 C3](https://github.com/jnhu76/qianqian/issues/259#issuecomment-6124864610)
and [C4](https://github.com/jnhu76/qianqian/issues/259#issuecomment-6124865619)
registered owner choices, not new product contracts. Current ADR already decides
the needed semantics, so these do not require a new owner semantic choice.

[#263 final H1](https://github.com/jnhu76/qianqian/issues/263#issuecomment-6140460263)
confirms #267/#269/#268/#270 merged (last merge is this base), four original H1
properties disposed in their stated scope, H-D explicitly unchanged. No H1
witness, seal capture, render failed-probe test or Loom oracle is changed here.
[#257](https://github.com/jnhu76/qianqian/pull/257) added deterministic straddle
witnesses; [#261](https://github.com/jnhu76/qianqian/pull/261) added ending/Open
boundary witnesses and repaired real post-seek-read premises. They supersede the
“only hammer evidence” premise, but do not prove production funnel source order
by an executed swapped-order mutant. Historical owner rulings and evidence stay
intact; no blanket H2 promotion or parent closure follows.

## R3A-H4 — meaning, DAG and disposition

Required properties: admitted host failure survives to host result; established
unsettled episodes receive record-then-Stop; committed Completed/Failed remain
unchanged; disposal finishes before host return. Distinguish four propositions:
A host failure must be recorded; B terminal cannot be relabeled; C late Stop may
record intent; D host need only route the failure response while unsettled.
D11 explicitly permits late command history. D3 requires response for an
established unsettled episode, not prohibition of optional late intent history.
`stop_requested` is public command history, not an exact host dispatch counter:
a user/other clone can write it, and an observation/Stop race can record it after
natural terminal. The current implementation chooses to skip already-terminal
response; keeping that implementation does not make its history pin normative.

```text
admit (host mutex, active=true)
 -> fail_with_response (first_failure=Read under host mutex; unlock)
 -> terminal Fact observation / response decision
 -> [request_stop (completion mutex intent; unlock; edge stop)]
 -> Operation Drop ACK (host mutex active=false, notify)
 -> owner wait_terminal -> root dispose -> seal closes admission
 -> wait any remaining ACK -> final host failure read -> reports/exit 1
```

Actual test: natural Completed and Failed, each with failure recorded before the
response wait or terminal obtained before fail(). The callback-held path really
calls request_stop after wait_terminal; production fail() on the terminal-first
path skips Stop. Natural Completed stays Completed, natural Failed stays Failed;
host exit is 1 and resources dispose in all four combinations. In a genuine
Stop/natural race, completion-lock publication makes late Stop history harmless;
if an unsettled abort wins with recorded intent it is Stopped unless failure has
precedence. EOF plus Drained yields Completed. Terminal is not proof of join;
finish_episode obtains disposal independently.

| Assertion | Value | Effect of removal / illegal change rejected |
| --- | --- | --- |
| wait_terminal == expected inside controlled cut and final check | NORMATIVE_SEMANTIC_ASSERTION | Keeps D11 precedence/immutability; host-forged Failed or late Stop relabel fails |
| host mutex try_lock succeeds during terminal wait | NECESSARY_MECHANISM_ASSERTION | Existing controlled single-reader cut checks D3 lock firewall |
| exit == 1 | NORMATIVE_SEMANTIC_ASSERTION | Lost host failure classification fails |
| disposed.get() | NECESSARY_MECHANISM_ASSERTION | Independent real fixture disposal witness; missing cleanup fails |
| Completed failure_diagnostic is None | NORMATIVE_SEMANTIC_ASSERTION | D14.2 truth-class separation; no assertion that Failed must have diagnostic |
| !observe().stop_requested after terminal-first fail | INCIDENTAL_IMPLEMENTATION_PIN | No independent contract lost; does not prove actual host dispatch identity |

Why test exists / lost protection if entire test deleted: unique four-way host
failure vs natural terminal boundary plus nonzero host result and cleanup. The
sibling read_and_panic_failures_stop_real_episode_without_forging_failed,
existing_user_stop_and_host_failure_keep_one_genuine_terminal and final owner
report tests cover unsettled response, genuine Stop and distinct diagnostics;
they complement rather than replace this natural-terminal matrix. Cost remains
proportional. Removing one incidental assertion retains those observations.

```text
H4_AUTHORITY = PBK-002 D11/D14.4 + frozen execution-model D3
H4_REQUIRED_PROPERTY = recorded host failure + immutable terminal + cleanup
H4_ASSERTION_VALUE = incidental terminal-first command-history pin
H4_DISPOSITION = ADR_DECIDED_REMOVE_ASSERTION
H4_PRODUCTION_CHANGE = NO
```

## R3B-P1-04 — whole-repository reachability and meaning

```text
FIELD -> WRITERS -> READERS -> CONSEQUENCE
CompletionState.disengagement_observed
 -> constructor false; new-pause false; Engaged false; Disengaged true
 -> sole read: local latched closure in the targeted cfg(test) test
 -> latch-only assertions; no control, settlement, observation or diagnostic effect
```

Whole tracked/untracked source search (hidden included, excluding .git and build
artifacts) found exactly six code occurrences: declaration, initialization,
three writes, one test read; one historical research report reference.
CompletionState is a private struct, with no derive, repr(C), serializer, FFI,
reflection, macro-generated accessor or platform/cfg alternate definition. All
CompletionState type uses are private construction, Mutex storage, local closures
and functions in completion.rs. Closures receive &mut CompletionState, but their
field accesses are explicit. Rust privacy prevents another module's indirect
field accessor. No unsafe layout/byte observation consumes it. cfg(loom) selects
synchronization types, not an additional reader; scripts and boundary checkers
guard the absence of a public disengagement latch, not this private bool.
Historical research text is evidence and is not rewritten for grep cleanliness.

If all writes disappear, no product observation changes. Existing event path
ports pause_park -> emit -> GateObserver -> weak core upgrade -> publish_evidence
-> apply_gate_event remains intact. Engaged sets engaged=true and clears
current tail; Disengaged sets engaged=false and clears tail. PauseEngagement
reads only that pair; paused() reads unsettled + pause intent + TailQuiesced.
D14.5 cut decision reads pause or seek parked/quiesced pairs and its operation
latches; D11 resolve never reads the deleted bool. request_stop, request_resume,
teardown release, gate lock order and seek evidence are unchanged. Storage size
is private representation; mutex/API/ABI/ownership/execution branches stay the
same apart from eliminating the bool-only pause reset conditional.

| Deleted test assertion | Value | Independent contract role |
| --- | --- | --- |
| pause_requested true | REDUNDANT_ASSERTION | Existing command/seam tests protect pause intent |
| delayed old Disengaged sets latch | INCIDENTAL_IMPLEMENTATION_PIN | Establishes only the deleted bool's own writer |
| Engaged clears latch | INCIDENTAL_IMPLEMENTATION_PIN | Pins unused representation; not Paused or seek evidence |
| Resume leaves latch false | INCIDENTAL_IMPLEMENTATION_PIN | Resume has no Resumed projection |
| current Disengaged sets latch | INCIDENTAL_IMPLEMENTATION_PIN | Same unused writer |
| terminal remains None | REDUNDANT_ASSERTION | Nonterminal pause/resume protected on real seam |

The comment claiming this bool keeps PauseEngagement honest is UNSUPPORTED_CLAIM:
there is no such read edge. Deleting this test loses a mutant rejection for an
unused writer, not a required witness. Removing only a write would preserve dead
representation; removing the bool and its sole-consumer test is smaller in total
maintenance cost. No replacement test needed.

Retained independent witnesses: pause_establishes_resume_releases_and_cycles_stay_truthful
holds cycle-2 tail nonquiescent and requires Engaged but not Paused before fresh
tail establishment; s0_starvation_produces_no_park_evidence_and_pause_attribution_is_the_frozen_reuse,
s0_operation_evidence_cannot_cross_cycles_and_cleared_parks_ground_nothing,
s0_a_paused_episodes_quiesced_park_commits_the_cut and
 a_second_accepted_seek_gets_fresh_cut_evidence exercise actual pause/seek pairs
and operation evidence. A stale tail that establishes cycle 2 prematurely fails
the seam observation; cut evidence leakage fails the current-cut tests. The
retired Resumed/public latch remains guarded by the existing plugin-boundary
negative controls. None needs this bool.

```text
P104_READERS = one test-only closure; zero production readers
P104_MECHANISM_VALUE = NONE after Resumed withdrawal
P104_PRODUCT_OBSERVABLE_DELTA = NONE (SEMANTICS_PRESERVING)
P104_DISPOSITION = ADR_DECIDED_REMOVE_STATE
P104_TEST_DISPOSITION = delete sole-consumer self-pinning test
```

## R3B-P1-02 — real funnel and evidence boundary

```text
request_seek H1 completion hold -> edge Open sample -> H2 completion hold
 H2: recheck -> slot plant -> cut reset -> route hold (one lock unit)
       <serialized against>
worker_exited: completion publish (worker_terminal + worker_gone; resolve)
 -> abort_stranded_seek: seek-slot clear -> Aborted gate release if stranded
 -> observation.close -> analyst.join -> worker returns -> teardown worker.join
```

If H2 records first, cleanup finds command or in_flight and clears it. If
worker-gone publishes first, H2 rejects. No lock is held across edge sampling;
slot is not held while taking completion lock. Abort clears picked or unpicked
records and routes Aborted without cut/rebase. release_seek_hold removes the
hold and notifies; the leg consumes-or-ignores the payload and the terminal data
plane decides. Normal continuing worker slot-free waits for committed-release
consumption; ending cleanup instead clears and releases.

All normal decode-loop exits occur inside catch_unwind's closure: EOF closes
edge, decoder/processor error publishes failure and fails edge, stop/ending and
seek-abort branches return from that closure. Caught unwind publishes decode
failure and fails edge, then joins the same funnel outside catch. No supported
loop branch returns from decode_worker before its worker_exited/abort pair.
The pre-catch setup calls decode_stream.format(), bind_observation_reader and
observation.spawn_worker before this catch boundary. A panic there bypasses the
funnel; this pre-existing setup scope is not covered by the loop/unwind claim
and is recorded as a residual, not silently repaired by this deletion task.
Spawn failure creating no decode worker follows activation/open-abort cleanup;
observation analyst spawn failure instead retires telemetry only. panic=abort,
poisoned mutex, allocation abort or panic inside the post-catch diagnostic/funnel
also fall outside the verified funnel scope; no unconditional liveness guarantee
for native calls/OS/device or telemetry join is claimed. Observation shutdown is
after seek cleanup and cannot postpone the release by joining the analyst first.

| Existing witness | Oracle/value | Evidence boundary |
| --- | --- | --- |
| an_accepted_seek_cannot_outlive_its_worker_exit | NECESSARY_MECHANISM_ASSERTION: accepted pre-exit slot clears; one Aborted then None; post-exit plant absent | Completion calls production methods in correct sequence; does not invoke actual worker or kill swapped production order |
| an_open_sampled_seek_straddling_the_worker_exit_is_never_accepted | NECESSARY_MECHANISM_ASSERTION: unmasked H2 worker_gone rejects; no command/release; terminal completes | #257 M2/M3 red at that rejection, inherited execution; no order-mutant claim |
| a_seek_straddling_the_real_worker_exit_funnel_is_never_accepted | NECESSARY_MECHANISM_ASSERTION + terminal semantic assertion: real EOF funnel, unmasked H2 rejection, real leg drains | Actual worker exit already complete before H2; cannot expose the between-two-calls order window |
| sampled_open_busy_then_stopped_free_plant_is_never_accepted + session ending/Open twin | Necessary Refused/Inert classification/no provider mutation, cleanup, real release/failure | #261 boundary evidence distinguishes private record from semantic Accepted |
| seek_acceptance_racing_worker_eof_settles_without_wedge | NORMATIVE_SEMANTIC_ASSERTION: Completed, terminal Position withdrawn; NECESSARY_MECHANISM_ASSERTION: disposal quiet | Actual integration liveness for visited schedules; 200 requests/2ms are load, not synchronization proof |

Why keep integration test: it runs assembled worker/edge/gate/drain/disposal;
white-box twins do not subsume that wiring. Its finite workload is existing cost,
not increased. Concrete bad second-hold admission is rejected deterministically
by #257 twins; missing cleanup rejects completion boundary checks and may wedge
the integration test on an exposed schedule. Swapping only the production pair
has no separately executed deterministic mutation witness here. The test's old
“pinned” wording is narrowed to separate source order from boundary/integration
evidence; body, schedule and oracles stay unchanged. #263 H1 work adds no
seek-funnel order witness and requires no repeat remediation here.

```text
P102_ADR_ORDER = D14.5 corrective-1 C2: liveness publication BEFORE abort duty
P102_CODE_ORDER = worker_exited(edge.terminal()); abort_stranded_seek();
P102_ALL_REAL_EXIT_PATHS = loop EOF/error/stop/caught-unwind funnel; pre-catch setup panic excluded
P102_DETERMINISTIC_EXISTING_EVIDENCE = completion + real-worker straddle + #261 boundaries
P102_INTEGRATION_EVIDENCE = retained 200-request EOF liveness sweep
P102_DISPOSITION = KEEP_WITH_EXPLICIT_EVIDENCE_LIMIT
P102_RESIDUAL_EVIDENCE_LIMIT = order SOURCE_PROOF; swapped-order mutant NOT_RUN; pre-catch setup panic scope
```

## Governance and validation scope

All three are H-D_DISPOSITIONED by existing authority; this is not
H2_FULLY_PROVEN. One assertion and one private field/sole-consumer test are
removed; the EOF integration test remains. No scheduler, observer, test API,
Loom/TLA model, hook, retry, sleep or workload is added. Production semantic,
public API and ABI delta NONE. ADR/execution model and historical inventory
unchanged. Append-only #258 disposition preserves other risks/debts.

Evidence levels: SOURCE_PROOF for audited local dataflow and program order;
TEST_EXECUTED_PASS only for recorded commands with nonempty test counts;
MUTANT_EXECUTED_RED only inherited #257/#261/#263 evidence at their recorded
subjects (no new mutant executed); DERIVED_ONLY for semantic preservation from
source and authority; NOT_RUN for exact swapped-order mutation, physical audio,
FFI, macOS/Android/iOS runtime and new formal/model campaigns. Linux/Windows CI
is target/run-specific device-free evidence, never real endpoint proof.

Targeted baseline and final suites, complete pre-push, exact PR-head CI and two
independent review verdicts are recorded separately in the execution receipts.
