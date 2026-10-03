# QIANQIAN-TEMPORAL-MODEL-0 — flake taxonomy and minimal causal-order experiment

- **Status**: RESEARCH_EVIDENCE (never architecture/spec/implementation authority)
- **Executed**: 2026-10-03
- **Campaign prompt**: "Do Qianqian's recurring scheduler/load-sensitive flakes reveal a missing shared temporal/causal model, and can a minimal model inspired by Kubernetes generation/observedGeneration and GStreamer seqnum/cut semantics eliminate those unstable assumptions without introducing a global event framework?"
- **Verification class**: T0 archaeology (issues/PRs/git) + S0 white-box provenance probes (deterministic) + T2 premise-free replays under artificial load. All runnable code lives in the working tree as test-only changes; the full local pre-push gate (`lefthook run pre-push --all-files`) passes twice at the recorded head.

---

## 1. Executive verdict

The four reconstructed flake episodes (F1–F4) share ONE root pattern, and it is **oracle-side, not engine-side**: each failing oracle consumed a **scheduler-positioned witness as if it were a stable semantic identity or commit point** (a cross-run transition-start frame; a processed-frames probe value read as a consumed index; an "observed but not yet actionable" scheduling window treated as a state; a fixed 300 ms sleep wagered to cover a bounded device drain). In every reconstructed episode the engine was verified correct; every fix was oracle-side.

Qianqian's **runtime temporal semantics are already sufficient**. The substance of the K8s and GStreamer ideas is already present in the current architecture in cheaper forms:

- GStreamer's "results belong to the operation" is realized by the **one-seek-in-flight slot + per-cycle evidence reset** (D14.5 corrective-1 C3); the S0 probes below pin that *operation* evidence (landing, refusal, commit) **cannot cross seek cycles** — no SeekId needed, exactly the F5 razor review's recorded position (§15 of `experiments/f5-seek-discontinuity/RESULTS.md`).
- GStreamer's flushing-seek cut maps 1:1 onto the frozen park → provider → invalidate → landing → commit conjunction; no epoch noun is missing.
- K8s `generation`/`observedGeneration` is the right **future vocabulary** for the two deliberately-deferred read models (#187 observation staleness, #188 DSP UI read model) — as a **local pair** on the respective seam when those models are earned, not now.

What the flakes actually lacked were **observation commit points** — explicit acknowledgements tests could await instead of sleep/window wagers. The repository already knows three honest ack forms (join, `DrainSignal`/gate events, content-derived anchors); the T2 replays show that once oracles use them, all four historical false-REDs are eliminated under load.

TEMPORAL_MODEL_0 = **EXISTING_MODEL_SUFFICIENT**. No production change, no authority change, no Lean4, no new TLA+ model. #195 remains an open **local** D14.5 authority question (park-attribution for actionability), and this campaign's S0 evidence is its answer to #195's pinned S0 step.

## 2. Live repository identities

```text
LIVE_MAIN_SHA      = c63cb754033cd44cc7c4213abe53c160db72adbd
                     (verified 2026-10-03: git pull --ff-only → "Already up to date";
                      matches the campaign prompt's expected SHA)
remote             = github.com:jnhu76/qianqian
campaign baseline  = c63cb75 (post-DSP-campaign main, PR #194 merge)
working tree       = main + test-only changes (4 files, +436 lines net before the
                     eq P3 fix; all inside qianqian-playback test code):
                     crates/qianqian-playback/src/completion.rs   (S0 white-box probes)
                     crates/qianqian-playback/src/live_tests.rs   (F3 load replay)
                     crates/qianqian-playback/src/eq_tests.rs     (F4 replay + the
                      owner-authorized P3 assertion fix)
                     crates/qianqian-playback/tests/common/mod.rs (under_cpu_load helper)
gate               = lefthook run pre-push --all-files → PASS (vocabulary,
                     plugin-boundary, rust-fmt, rust-clippy -D warnings,
                     rust-test workspace, docs-verify) — run twice during
                     the campaign; re-runnable with the documented command

Run-claim replayability: the F3/F4 load replays are self-contained in the
tree (in-tree `under_cpu_load` helper) and replay with `cargo test -p
qianqian-playback --lib -- s0_`. The F1/F2 load replay used EXTERNAL
spinner processes (not the in-tree helper) and is archived with its exact
command at `evidence/f1f2-load-replay.log` (both oracles GREEN, 8.00 s
run). Gate runs and the 5× suite stability repeats were session-run.
```

## 3. Historical flake inventory (T0)

All four episodes occurred 2026-10-02 during the DSP-vNext campaign (#190, PRs #191–#194). Reconstructed from commit messages, diffs, PR review comments, and issues.

### F1 — D3 applied-seek oracle

- Timeline: `4fca41f` (probe) → `9f44aa0` (correctives) → **`e34b596` "test(playback): anchor the live-probe seek oracle to its own run"** → `dbfb487` (PR #193 merge).
- Unstable assumption: the applied-seek oracle compared the seeked run against a **separate no-seek control run** and located the cut by content divergence — presuming both runs' in-flight transitions start at the **same frame**. Commit message: "Transition start frames are timing-dependent across runs, so the control's blend stretch is not the seeked run's blend stretch; Linux passed by scheduling luck and Windows CI failed at live_probe_tests.rs:702."
- Resolution: **test bug; production correct.** The reference now anchors at the run's own content-derived `wait_transition_started` boundary. Bonus: the N3 mutant gate compared a full stream against a landing-length slice (vacuous) — now judges `values[cut..]`.

### F2 — D4 observed-seek / cut-world reference failures

- Timeline: `fea1a08` (D4 production) → `981a311` "atomic live commands, activation bind, vocabulary" → `9c05808` "premise-free observed-seek endings, truthful gate prose" → `50d5b79` "derive observed-seek anchors from content" → `c63cb75` (PR #194 merge, 5 review rounds).
- Three distinct failure sub-mechanisms in the review rounds:
  1. **Dead oracle**: the mock clamps scripted landings, so `Some(44100)` restarted the cursor behind the consumed stream and the classifier could never fire; a cut can also land after a 64-frame transition already settled inside one staging block (no invalidation event).
  2. **Stale observation**: the probe's `TransitionStarted.at_frame` is a PROCESSED-frames witness, trusted as a consumed index (it can lead consumption by one in-flight frame).
  3. **Timing-window premise**: after `9c05808`, the "replacement-family anchors" (`MAX_INFLIGHT_LEAD = 1`, `m * FAST_BLOCK` enumeration) were still schedule assumptions; the 14/20 ms command-gap counterexamples produced false-RED at 4/5 and 6/6 anchors **on a verified-correct engine**.
- Resolution: **engine correct in every false-RED round; oracles wrong.** `50d5b79` derives every anchor from content ("a blend's first frame is the pure old side, so anchor = onset − 1"; outside a blend the applied configuration is bit-exact identity) and verifies the whole consumed stream as a member of the family of ALL legal endings. Post-fix: 14 ms 5/5, 20 ms 5/5, 40 ms 4/4, pristine 20/20, blinded-classifier mutant still RED. (The same review separately found two REAL production P1s — split-lock RMW and activation bind — fixed in `981a311`; those are independent of the flakes.)
- Carried inside round 1→2 was the **#195 oracle-side fix**: "the observed-seek oracle's 'seek never becomes actionable' premise was schedule-dependent (a starved render leg's park evidence IS the actionable evidence). The oracle now verifies every legal ending."

### F3 — Issue #195 (OPEN; no production fix; classification: INDEPENDENT_D14_5_AUTHORITY_QUESTION)

- Issue #195 (opened 2026-10-02T12:31Z): "D14.5: a pended seek's actionability is load-sensitive (park evidence)".
- Failure chain (issue body, verified in code by this campaign): a seek is actionable when the command is armed AND `completion.leg_parked_evidence()` holds (`session.rs:537`); `leg_parked_evidence()` is `engaged || seek_engaged` (`completion.rs:747-750`). "Under load (parallel cargo test), the render leg's parked evidence is true at a loop-top serialization check often enough that the pended seek fires" — ~1-in-3 full-suite runs pre-`930f5f8`; 0/11 after. The failing assertion was "a seek that never became actionable performs no cut"; the recorded D14.5 Applied cut executed. **Both endings are contract-conforming — no production defect implicated.**
- The issue's own hypothesis — "ordinary gate waits latched `engaged`" — was UNVERIFIED. This campaign's S0 probes answer it (§11): **refuted for that scenario**; the only possible latch source in the observed-seek scenario is the seek's own cut park (`SeekEngaged`).
- Owner guidance pins the guard rail: narrowing to `seek_engaged` only could break paused-seek progress (frozen D14.5 semantics) — "that is the dangerous mis-fix this issue must guard against". Pinned order: S0 provenance probe → S1 scenario matrix → S2 authority review (Cases A/B/C).

### F4 — D6 eq_tests pre-existing P3

- Classified in the #190 final campaign summary (2026-10-02T16:28Z, `D6_AUDITED_SHA = c63cb75`): "a pre-existing load-sensitive assertion in eq_tests.rs 'no post-failure production' — false-RED once under full-suite load, present verbatim at pre-campaign 7f418b1, production verified correct, over-tightens device-drain slack; one-line fix if the file is touched again".
- The assertion (`eq_tests.rs:1330-1336`): `consumed()` sampled after `wait_terminal() == Failed`, `sleep(300ms)`, assert consumed unchanged. Premises: (1) the terminal Fact's publication coincides with an instant consumption freeze; (2) 300 ms is always enough device/drain slack for the bounded edge's already-produced PCM; (3) the observer and the drainer are coupled. None is contractual. Under load the render leg can still be legally draining buffered, correctly-produced PCM when the sleep expires → false RED.
- Resolution this campaign: the owner-authorized fix is applied (§12/§21) — the freeze wager is replaced by the protocol bound; production remains untouched.

### Additional documented flakes / false-REDs (same family)

- #170 (closed, research): navigation-burst root cause — measurement-window sensitivity, no production defect.
- #127 (2026-09-13): "make the test oracles truthful" — an earlier round of the same discipline.
- `8e6ff54` + `128d2dd`: leak oracle took a one-shot /proc snapshot → bounded-poll oracle (observer-grace fix).
- `cf2b70f`: pause drain "is now a seeded standoff … rather than a race against how much the leg happens to have queued" — the same cure in miniature.
- `96941bc`, `33e7965`, `35f2074`: earlier premise-pinning rounds.

## 4. Flake taxonomy

| Flake | Correct-engine false-RED? | Wall-clock/sleep premise? | Missing op identity? | Missing generation? | Missing commit point? | Stale observation? | Safety or liveness? |
|---|---:|---:|---:|---:|---:|---:|---|
| F1 D3 applied-seek | YES | cross-run frame identity | no (wrong witness identity across runs) | no | no | YES | safety (oracle) |
| F2 D4 observed-seek | YES | YES (gap anchors, lead enumeration) | partially (processed-frames witness ≠ consumed index) | no | no | YES | safety (oracle) |
| F3 #195 | YES (both endings legal) | YES ("not yet actionable" = scheduling window read as state) | no (one-seek slot already excludes cross-cycle) | no | observation-point premise (unbounded window) | no (evidence current) | safety premise |
| F4 eq_tests P3 | YES | YES (300 ms drain-slack wager) | no | no | YES (no consumption ACK; fixed by join/bound) | no | safety (oracle) |

```text
RECURRENCE = SHARED_TEMPORAL_PATTERN
```

Evidence for a shared pattern across ≥2 semantically distinct protocol areas: live-DSP transition probes (D3/D4), the D14.5 seek protocol (#195), and D6 failure/drain accounting (eq_tests) — three distinct areas, one root pattern:

```text
FLAKE_ROOT_PATTERN =
    an oracle consumed a scheduler-positioned witness
    (a frame index, a window, a sleep) as if it were a
    stable semantic identity or commit point.

The runtime temporal semantics were sufficient in every episode;
the missing concept was an explicit OBSERVATION COMMIT POINT
(a mechanism-provided acknowledgement), never a generation counter.
```

## 5. Kubernetes mechanism study (primary sources)

- `metadata.generation` — "a sequence number representing a specific generation of the desired state. Set by the system and monotonically increasing, per-resource. May be compared, such as for RAW and WAW consistency." (API conventions; ObjectMeta reference.)
- `status.observedGeneration` — "the generation most recently observed by the component responsible for acting upon changes to the desired state … ensure that the reported status reflects the most recent desired status." (API conventions, "Typical status properties".)
- `condition.observedGeneration` — "if .metadata.generation is currently 12, but the .status.conditions[x].observedGeneration is 9, the condition is out of date with respect to the current state of the instance." (metav1.Condition comment.)
- Reconciliation is level-triggered: "the system's behavior is *level-based* rather than *edge-based*"; "you can't count on having seen it turn from `false` to `true`, only that you now observe it being `true`" (SIG-API-MACHINERY controllers doc) — watches are lossy/unordered; the controller re-derives from the level and stamps results with the generation they were computed from.

**Reusable idea (verbatim-minimal):** desired state belongs to generation G; every asynchronous result records the G it was computed from; `observed < current ⇒ stale ⇒ discard/mask`. Two integers and one comparison. No event bus, no timestamps, no ordering of events.

## 6. GStreamer mechanism study (primary sources)

**Operation identity (seqnum).** Design doc `seqnums.md`: "Seqnums are integers associated to events and messages. They are used to identify a group of events and messages as being part of the same *operation* over the pipeline … for example, flushes, segments and EOS that are related to a seek event started by the application." And the SEEK rule: "when handling the seek, the element might push FLUSH_START, FLUSH_STOP and a segment event. All these events should have the seqnum of the received seek event." Consequential results carry the operation's identity; duplicates are dropped by seqnum; attribution does not depend on arrival timing.

**Cut / stream epoch (flushing seek).** `gst_event_new_flush_start`: "It marks pads as being flushing and will make them return GST_FLOW_FLUSHING when used for data flow … Elements should unlock any blocking functions and exit their streaming functions as fast as possible … typically generated after a seek to flush out all queued data in the pipeline so that the new media is played as soon as possible." `gst_event_new_flush_stop`: "typically sent after sending a FLUSH_START event to make the pads accept data again … can process this event synchronized with the dataflow since the preceding FLUSH_START event stopped the dataflow." Seeking doc: "If a seek operation is requested using the GST_SEEK_FLAG_FLUSH flag, all pending data in the pipeline is discarded and playback starts from the new position immediately." The pair is an explicit two-phase boundary: invalidate/unlock (old world's in-flight work cannot report into the new world) → serialized reopen + new SEGMENT (commit).

**Qianqian mapping:** the frozen D14.5 protocol IS the flush pair in cheaper clothes — park (unlock/no device buffer held) → provider seek → `edge.invalidate()` (the ONE purge) → landing → commit conjunction (the boundary). What Qianqian does NOT have is a seqnum copied onto consequential evidence; the S0 probes show it does not need one, because the one-seek slot + per-cycle reset already excludes cross-operation attribution (F5 razor §15, confirmed executably in §11).

## 7. Lamport conceptual comparison

Happened-before (Lamport 1978) is a partial order: "a → b means that it is possible for event a to causally affect event b"; wall-clock ordering is explicitly NOT the relation ("We will therefore define the 'happened-before' relation without using physical clocks"). Scalar logical clocks exist only to extend the partial order to a consistent TOTAL order across processes. Every Qianqian protocol actor here is single-writer over its own evidence (one worker thread, one render leg, one completion lock), so causal attribution is already pairwise via program order + lock boundaries; no cross-actor total ordering is required by any demonstrated problem. Lamport clocks: NOT_NEEDED. Wall-clock remains legitimate for diagnostics/liveness bounds only (the repository's existing posture).

## 8. DeepSeek Harness escalation comparison

`deepseek-ai/deepseek-harness` (dsh) — "an open-source agent harness … built on an everything-is-a-plugin architecture and powered by Cordis" (Cordis is the K0 upstream reference, arXiv:2608.25512). Its session subsystem (primary docs): "`seq` is the monotonic position in the log (`seq = log.length`); `time` is epoch ms"; explicit causal references: "System, user, and tool surface events may cite a complete non-empty set of unique earlier events when source attribution or replacement coverage requires it" (`sourceEventSeqs: SessionSeq[]`); commit boundary: "every message-producing event must declare how it joins the surface, the sole source of derived model history" (append / replace-with-range); derived projection seam: registered units "fold committed events incrementally".

Its mined failure families are already in `evidence/external-systems/failure-corpus.md` (EF-01…EF-08) — cancellation ≠ quiescence, orphan obligations, persisted ≠ live truth, generational replacement, fact freshness/provenance across seams (EF-08).

**What M3 would obtain that M1 does not:** a durable, totally-ordered cross-protocol history; replay/reconstruction of any past state; repair of poisoned/dangling state by truncation (DSH #3708/#5182); many-to-many causal references across ALL actors. **No current Qianqian problem demonstrates any of these needs**: episodes are short-lived, each protocol family is single-writer with a bounded obligation set, and projections are forbidden as correctness authority (PBK-001 §2.3). M3 = REJECTED_AS_OVERDESIGN (no counterexample; §16 below).

## 9. Qianqian current temporal semantics inventory (condensed)

The full inventory (with file:line) was compiled during the campaign; the load-bearing facts:

```text
Seek (D14.5)     acceptance = one atomic unit under the completion lock:
                 re-validation + one-seek plant + per-cycle evidence reset
                 (seek_landing/seek_refused/cut_committed = None/false/false)
                 + gate hold routing. Actionability = armed command ∧
                 leg_parked_evidence() (= engaged ∨ seek_engaged — dual
                 attribution BY CONTRACT, the frozen paused-seek reuse).
                 Commit = ONE atomic three-valued sample:
                 (engaged ∧ tail_quiesced) ∨ (seek_engaged ∧ seek_tail_quiesced)
                 ∧ landing ∧ ¬episode-ending → Committed/Aborted/Pending.
                 "No queueing, no coalescing, no request identity — this is
                 why no SeekId exists." (completion.rs:694-695)
Pause (D14.7)    establishment = unsettled ∧ pause intent ∧ Engaged ∧
                 TailQuiesced; current-engagement fence (Engaged clears stale
                 disengagement); park evidence = mechanism evidence only.
Terminals (D11)  evidence → resolve → commit under ONE lock hold; first-wins;
                 late-command rule; stop-intent discriminator.
Live DSP (D14.11)desired (App-owned ProcessingControl, depth-1 latest-wins
                 pending) vs applied (engine). Read-model states deliberately
                 UNFROZEN (deferred to #188's D5 decision). Update identity =
                 slot occupancy; correctness by lock order + atomic
                 desired+pending move (mutant N9) + activation bind.
Existing         composition Revision(u64) fetch_add (desired-composition
identities       incarnation); FiberId.generation (stale-id detection);
                 PositionEvidence published fetch_max (stretch monotonicity)
                 + rebase (the ONE legal backward step). In playback proper:
                 NO request/generation/epoch counters for seeks, pause
                 cycles, updates, or episodes.
PCM edge         no world/epoch tag on blocks; stale exclusion is contractual
                 program order ("the discipline, not the primitive").
Identity gaps    10 places distinguish old vs new only by ordering/first-wins
                 (listed in the campaign working notes); ALL are covered by
                 structural exclusion (one-seek slot, single writer, one lock)
                 so none is a demonstrated defect — each is a candidate place
                 to ADD explicit identity IF a future counterexample arrives.
```

## 10. M1 — the minimal generation/operation model, after the razor

Candidate vocabulary challenged per campaign §8 ("delete any field without demonstrated value"):

| Candidate | Disposition | Why |
|---|---|---|
| `EpisodeEpoch` | **REJECT** | Episode identity is the episode-scoped handle; all cells are episode-owned and dropped with it. An E7 result under E8 is structurally impossible today (no cross-episode channel). Nothing demonstrated. |
| `OperationGeneration<T>` (seek_seq) | **REJECT for production** | One-seek-in-flight + per-cycle reset already exclude cross-cycle operation evidence (S0 probe 3 pins it executably; F5 razor §15 predicted exactly this). A TEST-LOCAL cycle counter is permitted if a future oracle needs to NAME cycles — no production surface, no new authority. |
| `Witness{episode, operation, provenance}` | **ALREADY EXISTS (evidence stream)** | `GateEvent::{Engaged, SeekEngaged, …}` + per-cycle latches ARE the provenance stream; the S0 probes read provenance from it deterministically. `leg_parked_evidence()` collapses it to a bool — but that collapse IS the frozen dual-attribution contract (paused-seek progress), not a defect. |
| `DesiredGeneration` / `AppliedGeneration` | **DEFER (guidance, not code)** | Correct future shape for #187 (stale FFT: `observed_generation < current_cut_generation ⇒ stale ⇒ drop`) and #188 (DSP UI: display current iff `applied_generation == desired_generation` of the displayed state). Both read models are deliberately unfrozen (D5/#187); when earned, they should adopt this LOCAL pair. Adding it now would be an unearned production field with no consumer and no demonstrated bug. |
| `CommitBoundary` | **ALREADY EXISTS** | The frozen commit conjunction + atomic three-valued decision sample (D14.5 corrective-3). |
| `OBSERVATION COMMIT POINT` (ack) | **THE ACTUAL FINDING — mostly exists** | The honest ack forms are join-on-exit (dispose), mechanism events (`DrainSignal`, `GateEvent`, probe events), and content-derived anchors. F3/F4 needed only to USE them. The one true ACK gap found: slot-free/payload-consumed has no observable signal, which is why `tests/seek_seam.rs` uses the sleep-200ms "linearization-sleep idiom" (over-waits — safe direction, no false-RED demonstrated). |

**M1 final shape:** no new runtime nouns. The existing Mechanism-Evidence + per-cycle reset + commit-boundary discipline already embodies GStreamer's operation-identity and cut-epoch substance, and the local generation PAIR is the designated future vocabulary for the deferred read models. M1 is sufficient (§14).

## 11. #195 provenance experiment (S0)

Four deterministic white-box probes (in `completion.rs` `mod tests`, campaign section) driving the REAL publication boundary (`apply_gate_event`, `leg_parked_evidence`, `seek_cutover_decision`):

1. **`s0_a_pended_seeks_actionability_is_its_own_cut_park_absent_pause_intent`** — in the #195 scenario (no pause intent), between acceptance and the leg's next loop-top the seek is genuinely "observed but not actionable"; the ONLY event that can close the window is `SeekEngaged` (the seek's OWN cut park, routed at acceptance). Provenance is cut-attributed; `engaged` never fires.
2. **`s0_starvation_produces_no_park_evidence_and_pause_attribution_is_the_frozen_reuse`** — an intent-free loop-top parks nothing and publishes nothing (campaign question 2 answered: ordinary data starvation does NOT fabricate engagement; a starved leg blocks inside its read, after the gate). And pause attribution IS the frozen dual-attribution reuse (probe 2b).
3. **`s0_operation_evidence_cannot_cross_cycles_and_cleared_parks_ground_nothing`** — cycle-1 landing/commit cannot satisfy cycle-2 (acceptance resets them; a cleared park grounds nothing; a fresh landing alone is Pending); cycle-2 commits only on its OWN park + quiescence + landing. Campaign question 5 answered: stale operation evidence from S41 cannot satisfy S42 — without any SeekId.
4. **`s0_a_paused_episodes_quiesced_park_commits_the_cut`** — the paused episode's seek cuts on the pause-attributed park pair (D14.5's "already-quiesced tail" clause), pinned.

**The S0 answer for #195:** the failing runs' actionability was grounded by the seek's own cut park (`seek_engaged`), not by pause engagement and not by starvation — the issue's "ordinary gate waits latched engaged" hypothesis is refuted for this scenario. The window "observed but not actionable" is real, its length is a scheduling fact, and BOTH endings are contract-conforming. The deeper attribution rule the probes pin for the S2 authority review:

```text
Park/quiescence latches are WORLD-STATE evidence: they assert a current
physical fact ("the leg is parked; this stream's queued-to-play set is
empty"), are attribution-blind by contract (both classes prove the same
physical fact), and may legitimately persist across a cycle boundary
WHILE STILL TRUE.

Landing/refusal/commit latches are OPERATION evidence: they are bound to
one cut cycle by the acceptance-time reset, and only the current cycle's
own publications can satisfy its commit boundary.

A cut is therefore never committed on stale OPERATION evidence; it may
commence on residual WORLD-STATE evidence only when that evidence is
still physically true of the current world.
```

Process note (recorded honestly): during probe authoring, three probe-code live-locks were introduced and fixed — calling `consume_release` while pause intent was still routed parks forever BY DESIGN (the D14.7 park waits for a release only a resume routes). The gate's behavior is correct; the probes now release/consume every routed intent before dropping the completion (test hygiene the existing suite already follows).

End-to-end load replay (`live_tests::s0_replay_the_seek_stop_race_admits_only_legal_endings_under_load`): the seek × stop race run 3× unloaded and 12× under 4 spinner threads; every run's consumed stream is a member of the two legal content families (one exact tag ramp, or two ramps joined by the landing-0 downward step) and every terminal is `Stopped`. Observed distributions (diagnostic only): unloaded (3, 0), loaded (12, 0) — the cut ending did not occur at this load level; the historical flake needed full-suite contention. The oracle pins membership, not schedule, so both distributions are green.

## 12. Historical-flake replay (T2)

| Flake | OLD_PREMISE | NEW_STABLE_PREDICATE | MODEL_FIELDS_REQUIRED | FALSE_RED_UNDER_STRESS |
|---|---|---|---|---|
| F1 | two runs share a transition-start frame | anchor at the run's OWN content-derived blend onset; verify family membership | none (content witness) | **NO** — `an_applied_seek_during_a_transition_lands_fresh_under_the_accepted_config` ok under 8-spinner load |
| F2 | processed-frames witness ≙ consumed index; enumerated lead/block anchors | anchors DERIVED from content; whole-stream family-of-all-legal-endings verification | none | **NO** — `a_merely_observed_seek_does_not_block_the_update_pickup` ok under 8-spinner load |
| F3 | "observed but not actionable" persists for the observation window | both legal endings verified by content (ramp / two-ramps) + terminal Stopped; provenance pinned by S0 probes | none | **NO** — 12/12 loaded + 3/3 unloaded legal; 5× suite repeats green |
| F4 | 300 ms sleep ≻ drain of already-produced PCM | producer bound: `consumed ≤ stopped_at + EDGE_CAPACITY_FRAMES` (schedule-free protocol bound), plus replay `s0_replay_mutated_then_failed_prefix_law_needs_no_drain_slack_wager` = final consumed content is EXACTLY the control's prefix after the join (structural quiescence, no sleep) | none | **NO** — ok under 4-spinner load |

Stress method (per row): F1/F2 were replayed under **8 external spinner processes** (bash busy-loops; exact command and GREEN output archived at `evidence/f1f2-load-replay.log`); F3/F4 run under the in-tree `test_common::under_cpu_load(4)` (spinner threads; diagnostic load only — no correctness conclusion rests on their timing). T2 verdict: **every historical flake's premise-free oracle holds under the load class that produced the original false-RED.**

## 13. Future-client sanity checks

**#187 Observation Plane (stale FFT), NOT implemented — model check only.** "Observation submitted under cut generation C8; seek Applied ⇒ current becomes C9; result observed_generation = C8 ≠ C9 ⇒ stale ⇒ drop" is expressible in M1 as a LOCAL pair on the observation seam, no new scheme: the observation plane already has the two ingredients (the cut boundary is the epoch event — `invalidate_signal_history()` is its DSP twin today; episode-owned cells already make "E7 result under E8" structurally impossible). M1 expresses it; nothing in M1 must be invented for it.

**#188 DSP read model, NOT implemented — model check only.** A `desired_generation / applied_generation` pair on the existing `ProcessingControl`/engine value-pair would let #188's TUI answer "is the displayed state current?" by pure comparison (`applied_generation == desired_generation`), never by timing — the K8s shape exactly. This is guidance for D5's deliberately-unfrozen read-model decision, not a production change now (no consumer, no demonstrated bug; D14.11/D14.10 stop rules apply).

## 14. M1 sufficiency verdict

Against campaign §10's success criteria:

1. #195 explained without scheduler-window assumptions — **YES** (§11: window = scheduling fact; both endings legal; provenance pinned).
2. Paused-seek progress remains expressible — **YES** (dual attribution pinned as contract, probe 2b/4).
3. Stale seek evidence cannot satisfy a new seek cycle — **YES for operation evidence** (probe 3); world-state evidence persists only while physically true (the pinned attribution rule).
4. D3/D4 oracles premise-free — **YES, already landed** and re-verified under load (§12).
5. D6 eq_tests P3 rewritten as protocol/content predicate — **YES** (bound + join-prefix replay; the authorized fix applied to the shipped test).
6. #187 stale-FFT rejection expressible naturally — **YES** (local generation pair; §13).
7. No global event log or global clock required — **YES**.

```text
MINIMAL_TEMPORAL_MODEL = SUFFICIENT
```

## 15. M2 experiment — NOT REQUIRED

M2 (local causal references: `seq` + `caused_by/source_seq`) is entered only on partial success. M1 is sufficient. The identity gaps §9 lists could each be spelled with M2 references if a future counterexample demands it; none does today.

## 16. M3 justification — REJECTED_AS_OVERDESIGN

Anti-overengineering gate (§24): no concrete historical or reproducible Qianqian counterexample requires local operation identity + generation + provenance to fail into a shared ordered log. The DeepSeek capabilities Qianqian would gain (cross-protocol ordered history, replay repair, durable chronology) answer needs Qianqian does not have: episodes are short, protocols single-writer, obligations bounded, projections barred from authority. **STOP ESCALATION.**

## 17. Formal-method ladder decision

```text
Used:   F0 (S0 provenance trace, deterministic white-box probes)
        F1 (premise-free oracles + mutation-discipline tests + load stress)
Existing: F2 assets cover the seek protocol (specs/f5-seek-discontinuity:
        BOUNDED-CLEAN, 672 states, 7 mutations; f5-seek-implementation M1–M11)
Stopped at: F1 — the lowest level giving defensible evidence. No NEW
temporal collision was found that the existing models/tests do not cover;
per AGENTS.md verification-authority boundary, no new model is owed.
```

## 18. Lean4 decision

```text
LEAN4_DECISION = NOT_NEEDED_NOW
```

All load-bearing claims are small and already pinned executably: cross-cycle operation-evidence exclusion (probe 3 + existing mutation gates), the commit-boundary conjunction (TLA+ model + M1–M11), the attribution rule (probes 1–4). Should the #195 S2 ruling freeze a permanent park-attribution predicate, the natural home is a narrow extension of the existing `specs/f5-seek-discontinuity` TLA+ model (F2), not Lean4. No subtle invariant remains that tests/existing models cannot carry.

## 19. Authority impact

- **No authority change required or made.** D14.10's stop list (Generation/Window/epoch) was not triggered: no new runtime noun is proposed for production. D14.5's razor review (§15 of the F5 gate report) is *confirmed by executable evidence*, not overturned. D14.7's dual-attribution park evidence is *confirmed* as the load-bearing contract behind paused-seek progress.
- #195 stays scoped to D14.5 and remains OPEN with its classification (`INDEPENDENT_D14_5_AUTHORITY_QUESTION`). This campaign delivers its pinned **S0** step: the provenance answer (§11) plus the world-state/operation-evidence attribution rule for the **S2** authority review. The S1 scenario matrix remains the owner's next step.
- Future guidance (no change now): when #187/#188 earn their read models, the local `desired/applied` (and `observed`) generation pair is the pre-validated vocabulary; any production adoption goes through the normal narrow authority amendment.

## 20. Minimal recommended production change

```text
PRODUCTION CHANGE = NONE.
```

Test-side changes delivered in the working tree (all test-only, gate-green):

1. S0 white-box provenance probes (completion.rs) — recommended to keep as permanent attribution oracles.
2. F3/F4 premise-free load replays (live_tests.rs, eq_tests.rs) — recommended to keep.
3. The #190-recorded eq_tests P3 debt fix (owner pre-authorized "one-line fix if the file is touched again"): the 300 ms freeze wager replaced by the schedule-free protocol bound `consumed ≤ stopped_at + EDGE_CAPACITY_FRAMES`.
4. `under_cpu_load` helper (tests/common/mod.rs).

## 21. Tests/oracles that should be rewritten (ranked)

| Oracle | File:line | Idiom | Suggested replacement | Urgency |
|---|---|---|---|---|
| eq P3 freeze | eq_tests.rs:1330 (pre-fix) | sleep 300 ms + equality | **DONE** (protocol bound) | — |
| Same freeze idiom | stateful_probe_tests.rs:715-721; gain_tests.rs:299-305; tests/seek_seam.rs:533-540 | sleep + consumed-freeze | same bound / join-then-final-prefix | medium (identical premise) |
| Linearization-sleep idiom | tests/seek_seam.rs:444, 577, 637, 701 | sleep 200 ms to outlast the 2 ms worker wait-slice | over-waits (safe direction; no false-RED demonstrated) — a slot-free/payload-consumed ACK would remove the wall clock; defer until a failure demonstrates the need | low |
| Steady-state negation under pause | live_tests.rs:858-874, 1032-1038 | sleep T + "nothing advanced" | false-pass direction possible under extreme scheduling; the seeded-standoff pattern (cf2b70f) is the repo's known cure | low |

## 22. Remaining unexplained flakes

1. **#195's authority question itself** — not a flake but the residual open item: whether BOTH park attributions (and under what authority Cases A/B/C) should ground actionability. Explained mechanism-wise (§11); the policy choice is the owner's S2 ruling. Scope: #195/D14.5.
2. **Corrective-3's recorded unbounded-Pending stall** (D14.5 corrective 3): an applied cut whose device tail neither quiesces nor fails, with no stop and no teardown, pends forever; a timeout was ruled NEW authority and deliberately not invented. Recorded, unchanged, unchanged by this campaign.
3. **Probe-authoring live-locks (resolved)** — three hangs during S0 authoring, all traced to probe code parking a gate nothing releases (by-design gate behavior); fixed by probe hygiene. One early occurrence (pre-marker build, `--test-threads=1`, WSL2) was never fully root-caused before it stopped reproducing across ~15 subsequent runs in both threading modes; the recorded evidence is futex-parked test threads at 0% CPU with no surviving lock owner identifiable without ptrace. Honest classification: environmental/authoring artifact, no production implication, no campaign conclusion rests on it.

## 23. Final escalation decision

The minimal model is sufficient; the escalation ladder terminates at M1-as-vocabulary. The repository's flake recurrence is an ORACLE discipline ("every scheduler-positioned witness needs an explicit acknowledgement or a content-derived anchor"), which this campaign both demonstrated and applied.

---

## Required final matrix

| Problem | Existing model | M1 K8s+GStreamer | M2 causal refs | M3 ordered log |
|---|---:|---:|---:|---:|
| #195 seek actionability | SOLVES (explained; both endings legal; provenance pinned) | SOLVES (same, with explicit vocabulary) | NOT_NEEDED | NOT_NEEDED |
| stale seek evidence | SOLVES (per-cycle reset; probe-pinned) | SOLVES (operation-vs-world attribution rule) | NOT_NEEDED | NOT_NEEDED |
| paused-seek progress | SOLVES (dual attribution, frozen) | SOLVES (unchanged) | NOT_NEEDED | NOT_NEEDED |
| D3/D4 false-RED anchors | SOLVES (content-derived anchors, landed + re-verified) | SOLVES (same) | NOT_NEEDED | NOT_NEEDED |
| D6 eq_tests flake | SOLVES (bound + join ACK; fix applied) | SOLVES (same) | NOT_NEEDED | NOT_NEEDED |
| DSP desired/applied | PARTIAL (value pair; read model deferred) | SOLVES (local generation pair when #188 earns it) | NOT_NEEDED | NOT_NEEDED |
| #187 stale FFT result | PARTIAL (cut boundary exists; staleness rule unearned) | SOLVES (`observed < current ⇒ stale` locally) | NOT_NEEDED | NOT_NEEDED |
| episode replacement | SOLVES (episode-owned cells; structurally impossible) | SOLVES (unchanged) | NOT_NEEDED | NOT_NEEDED |

## Anti-overengineering gate answers

- Beyond M1? "Give one concrete Qianqian counterexample M1 cannot represent correctly." — **None exists.** Every reconstructed failure is expressed by existing semantics + oracle discipline. STOP ESCALATION.
- M3? "Give one counterexample that local operation identity + generation + provenance cannot represent without a shared ordered log." — **None exists.** M3 = REJECTED.

## Final verdict

```text
TEMPORAL_GAP = NONE
    (the runtime temporal semantics are sufficient; the recurrence is an
     oracle-methodology pattern. #195's park-attribution policy is a LOCAL
     D14.5 authority question, already recorded there.)

FLAKE_ROOT_PATTERN = ORACLE_TIMING_PREMISE
    (scheduler-positioned witness consumed as semantic identity/commit point)

M1_K8S_GSTREAMER = SUFFICIENT_AS_VOCABULARY
    (production already embodies their substance: per-cycle reset ≙
     generation comparison; park/invalidate/commit ≙ flush pair; the local
     generation pair is the designated future shape for #187/#188)

M2_LOCAL_CAUSAL_REFS = NOT_NEEDED
M3_DEEPSEEK_STYLE = REJECTED

LEAN4 = NOT_NEEDED_NOW
TLA_PLUS = NOT_NEEDED_NOW
    (existing specs/f5-seek-discontinuity + implementation mutations cover
     the seek protocol; optional narrow extension after #195's S2 ruling)

PRODUCTION_CHANGE = NONE
AUTHORITY_CHANGE = NONE

NEXT =
    1. Land the test-only probe/replay changes (gate-green) on a branch.
    2. Hand §11 (S0 provenance + attribution rule) to issue #195 as its
       pinned S0 deliverable; owner proceeds to S1 matrix + S2 ruling.
    3. Create QIANQIAN-TEMPORAL-MODEL-0 as the RECORD of the flake
       taxonomy + oracle discipline (evidence: §3/§4/§12) — linking #195
       as one instance, not the owner of temporal semantics.
    4. At #187/#188 design time, adopt the local generation pair (M1).
    5. Optionally schedule the §21 oracle rewrites (medium/low urgency).
```

TEMPORAL_MODEL_0 = EXISTING_MODEL_SUFFICIENT

---

## Review record

- **Round 1 (fresh-context adversarial review agent, 2026-10-03):** one
  [BLOCKING] evidence-integrity finding — §12 originally described the
  F1/F2 stress method as the in-tree `under_cpu_load` harness, but those
  rows were actually produced with 8 EXTERNAL spinner processes and no
  archived artifact. Corrected: method description fixed per row, the
  F1/F2 replay re-run under the archived command (GREEN, both oracles,
  8.00 s) and archived at `evidence/f1f2-load-replay.log`, run-claim
  replayability noted in §2. All other load-bearing claims (code file:line
  spot-checks, authority citations, verdict format, M2/M3 rejection
  soundness, no evidence-to-truth promotion) verified by the reviewer.
  No verdict line changed.
