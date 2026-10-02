# Temporal Mechanism Audit 0 — independent adversarial audit of PR #196

> STATUS: RESEARCH EVIDENCE. This report is not authority. It audits
> PR #196 (`test(playback): temporal-model-0 provenance and replays`)
> and independently reconstructs Qianqian's current temporal mechanism
> per the audit commission (sections 0–30 of the task). Authority
> remains ADR-PBK-001/002/003; nothing here amends it.
>
> Date: 2026-10-03. Auditor: independent agent, adversarial posture.
> Reviewed head: 5d14261 (original PR head) + two audit-corrective
> commits applied during the audit (0a79a33, e453bc8 — see §25/§29).

## 1. Executive verdict

```text
CURRENT_TEMPORAL_MECHANISM = SUFFICIENT_BUT_IMPLICIT
PR_196 (original head 5d14261) = CHANGES_REQUIRED   (P1: required CI red)
PR_196 (with audit correctives) = APPROVE, conditional on:
    - push of commits 0a79a33 + e453bc8 and a green CI run on that head
    - the P2 dispositions in §24 recorded (done: this report)
```

One-sentence summary: **Qianqian really does carry temporal correctness
today without clocks, generations, or SeekIds — the campaign's central
claim survives independent reconstruction — and PR #196's S0/F3/F4
evidence is genuine, mutation-sensitive, and honestly reported; but the
original PR head failed its own required CI (loom build), the
`under_cpu_load` helper leaked spinners on panic, and one F3-equivalent
residual risk was missing from the recorded debt.** All three defects
were small and corrective commits for the first two are already on the
branch; the third is recorded as debt by this report.

## 2. Live identities (frozen at audit start, re-read live)

```text
LIVE_MAIN_SHA    = c63cb754033cd44cc7c4213abe53c160db72adbd
PR_196_BASE_SHA  = c63cb754033cd44cc7c4213abe53c160db72adbd  (matches expected)
PR_196_HEAD_SHA  = 5d1426112937facb1a1643acbd3651c4a6fb3d8f  (matches expected;
                   verified unchanged on GitHub at audit close)
PR_196_STATE     = OPEN, not draft
PR_196_MERGEABLE = MERGEABLE (mergeStateStatus UNSTABLE: failing required check)
```

CI on the original head (run 37056813681 et al., 2026-10-02):

```text
vocabulary-check                          SUCCESS
Validate commit messages / PR title       SUCCESS
TLA+/TLC current suites                   SUCCESS
Plugin boundary gate                      SUCCESS
fmt + check + test + clippy (Linux)       SUCCESS
cfg(windows) compile + device-free tests  SUCCESS
CodeRabbit                                SUCCESS
matrices + Miri + mutations + loom        FAILURE  <-- P1, see §25
```

## 3. Temporal-domain vocabulary (audit commission §3)

| Domain | Example in tree | Correctness authority? |
|---|---|---|
| wall-clock time | `WORKER_WAIT_SLICE` poll sleep (session.rs:633), test `within`/`wait_until` bounds | NO — liveness backstops only |
| source/media time | seek `target: Duration`, provider landing (source frames) | YES within source semantics (D14.5) |
| presentation position | device-consumed Position projection (D14.8; position cell) | YES — writer-side monotone between committed discontinuities |
| program order | one decode worker (the ONLY producer); one render leg (the ONLY submitter) | YES locally — the load-bearing wall for stale-PCM exclusion |
| lock order | the ONE completion Mutex linearizes commands + evidence + settlement; completion→slot→gate nesting | YES — frozen discipline (completion.rs:675-680) |
| protocol cycle | seek cycle (accept→resolve→slot-free), pause engagement cycle | YES, implicit (no tokens — see §5/§6) |
| mechanism evidence | Engaged/SeekEngaged/TailQuiesced latches, landing, drain verdict | YES as evidence, never Facts |
| commit state | `cut_committed`, `outcome: Option<SessionOutcome>` | YES — the semantic authorities (D14.5 commit boundary; D11 first-wins) |
| observation time | `observe()`/`wait_terminal()` call placement | NO by itself — proven in §7 |

## 4. Current ordering mechanism inventory (audit commission §2/§10)

The generic shape (commission §4) instantiated from code:

```text
APPLICATION COMMAND (request_seek / request_pause / request_stop / set_*)
      │  handle.rs → SessionCompletion
      ▼
acceptance / linearization boundary
   ONE completion-lock hold: re-validate frozen conditions, plant
   command under slot lock, reset per-cycle operation evidence, route
   gate intent  (completion.rs:649-719 seek; 576-593 pause; 523-553 stop)
      ▼
mechanism-specific in-flight state
   ├─ worker evidence: SeekSlot (one-deep), landing/refused latches,
   │   worker_gone (exit funnel, completion.rs:1026-1031)
   ├─ render evidence: gate latches engaged/seek_engaged +
   │   tail_quiesced/seek_tail_quiesced (apply_gate_event, 1184-1221)
   └─ device/drain evidence: DrainSignal verdict, edge terminal
      ▼
authority-owned commit predicate
   seek: seek_cutover_decision — ONE three-valued sample (847-871)
   terminal: resolve() — pure function of the lock-held record (1249-1309)
      ▼
observable semantic result
   outcome: Option<SessionOutcome> (first-wins), Position projection,
   Paused projection — all read via one coherent observe_snapshot (1076-1120)
```

**T-inventory** — every item verified in code, none inherited from the
PR's report:

```text
T1  Episode-scoped ownership        SessionCompletion owns gate/slot/position/
                                    output_level/processing (completion.rs:284-326)
T2  Single-writer program order     decode worker = only producer; render leg =
                                    only submitter; position cell single writer
                                    (edge.rs module doc; completion.rs:298-305)
T3  Mutex linearization boundaries  the one completion lock; nested slot/gate
                                    locks one-directional (completion.rs:675-680)
T4  One-operation-in-flight         SeekSlot.in_flight (one-seek, no queueing,
                                    completion.rs:691-702); terminal first-wins;
                                    pause/stop intent idempotent
T5  Acceptance-time reset           seek_landing/seek_refused/cut_committed reset
    of operation evidence           at acceptance (completion.rs:711-713)
T6  Current-world latches /         Engaged/SeekEngaged fence: each engagement
    engagement fences               resets its own tail evidence + prior
                                    disengagement (completion.rs:1194-1219);
                                    request_pause hygiene-clear (586)
T7  Atomic commit predicates        seek_cutover_decision one-sample three-valued
                                    (corrective-3); publish_evidence settles inside
                                    one hold (1223-1243)
T8  Bounded queue semantics         PcmEdge 8192 frames, FIFO ring, first-wins
                                    terminal, FAILED/STOPPED abandon buffered
                                    (edge.rs:225-265, 189-200)
T9  Join / mechanism                stop_and_join joins the leg; release payload
    acknowledgements                awaits consumption (slot freed only after,
                                    session.rs:647-672); worker_gone published
                                    before stranded-seek cleanup (1026-1061)
T10 Content-derived witnesses       TEST-side only (tag ramps, EQ prefix laws) —
                                    this is oracle discipline, not runtime
```

Omission check: nothing in production plays the role of a clock,
generation, seqnum, or event log. `ProcessingControl` has no generation
— its substitutes are T5-style (§8). **The T-list above is the whole
mechanism; no eleventh item was found.** One non-token discipline the
list must name explicitly, because it carries correctness that T1–T9
only imply: **worker program order at the serialization point**
(`purge → landing → production hold`, session.rs:595-598) — the ADR
says this, not any latch, is the load-bearing stale-PCM wall
(ADR-PBK-002.md:1277-1280). It is part of T2 but is called out because
two code comments overstate what the latches alone prove (§5, gap 2).

## 5. Seek mechanism (D14.5) — reconstruction and attacks

**Acceptance.** Linearization point = the second completion-lock hold
(completion.rs:681-718): re-validate (outcome/stop/teardown/activation/
worker_gone), plant under slot lock (`one-seek; no queueing, no
coalescing, no request identity — this is why no SeekId exists`,
693-695), reset per-cycle operation evidence (711-713), route
`set_seek_hold(true)` inside the same hold (717). The edge-terminal
check sits outside the locks (672-674) and can go stale; the defense is
layered (second-hold re-validation, worker serialization re-check
session.rs:551-553, exit cleanup `abort_stranded_seek` 1045-1061 with
`worker_gone` published first — both orderings pinned by
`an_accepted_seek_cannot_outlive_its_worker_exit`, completion.rs:1890+).

**Park evidence classification** (independently derived; matches the
PR's classification where they overlap):

| Latch | Set / cleared | Class | Why |
|---|---|---|---|
| `engaged` | Engaged / Disengaged | WORLD-STATE | asserts a pause-attributed park *currently exists*; survives seek commits mid-park; only the park ending clears it |
| `tail_quiesced` | once per engagement; cleared by Engaged AND Disengaged | OPERATION-specific (engagement-scoped) | the current engagement's drain observation; refill invalidates it |
| `seek_engaged` | SeekEngaged / SeekDisengaged (NOT reset at acceptance) | WORLD-STATE | same reasoning as `engaged`; deliberate non-reset lets it carry across a cycle boundary **while the park is physically continuous** |
| `seek_tail_quiesced` | once per seek park; cleared by SeekEngaged and SeekDisengaged | OPERATION-specific (park-scoped) | every park re-derives quiescence from a fresh probe (`quiesced_published` local, ports.rs:683) |

**Provider results.** RefusedUnchanged → `seek_refused()` + release
park, remainder finished exactly, slot freed only after (zero content
loss; narrowed to provably-pre-mutation class by corrective-2).
Applied → discard remainder → invalidate processing history →
`edge.invalidate()` (the ONE purge) → landing published (first-wins
latch) → production hold → commit poll. MutatedThenFailed →
`release_seek_without_commit()` → `decode_failed()` (D11 Failed) →
`edge.fail()` → worker return; never resumes old-cursor production
(session.rs:556-580).

**Cut commit.** `seek_cutover_decision` (completion.rs:847-871), one
lock hold, three-valued:

```text
episode_ending (stop_requested ∨ outcome ∨ teardown_released)  → Aborted
seek_landing.is_some()                       [current cycle; reset at acceptance]
  ∧ ((engaged ∧ tail_quiesced) ∨ (seek_engaged ∧ seek_tail_quiesced))
                                             [current park pair, gate events]
→ Committed (records cut_committed, routes Committed{landing} payload)
else → Pending (keeps waiting; never an abort)
```

"Edge invalidated" is the caller's program order (already true when the
worker asks), not a lock conjunct — exactly as the ADR freezes it
(ADR-PBK-002.md:1291-1294).

**Why previous-seek evidence cannot satisfy the current seek, without a
SeekId** — the answer the commission demanded, demonstrated:

1. The one-seek slot only frees after the previous protocol fully
   resolved (release payload consumed by the leg; session.rs:647-672).
   A second request before that is *inert*, not queued (completion.rs:697).
2. Acceptance resets the operation latches (711-713), and only the
   single worker thread can publish a new landing, strictly after the
   slot was re-occupied (session.rs:598 follows the pickup of the new
   command).
3. The park conjunction reads *current world state*: an exited park has
   published SeekDisengaged on the leg's own thread (ports.rs:718 —
   including the probe-failed exit), and every new park re-derives
   quiescence from a fresh probe.

**Adversarial attacks (independently constructed; agent corroboration):**

- **A3 — continuous-park carry (real, benign, unpinned).** On the
  refusal/abandon routes the slot frees *without* the leg observing
  hold=false (release_seek_park routes an abort payload; the slot frees
  after the remainder finishes, session.rs:707). If a new seek is
  accepted before the leg wakes, `set_seek_hold(true)` re-arms the same
  in-flight `seek_park` wait — no SeekDisengaged, no fresh SeekEngaged.
  Cycle 1's `seek_engaged ∧ seek_tail_quiesced` then ground cycle 2's
  commit. **Not a correctness break**: the leg never left the gate,
  holds no device buffer, and the device tail cannot refill while
  parked (D14.7 frozen invariant) — the latches are continuously true
  statements about the world. But no probe pins this shape; the S0
  cross-cycle probe pins only the *exited*-park case.
- **A2 — commit on a just-vacated park (real, benign).** Disengaged in
  flight while the worker samples the commit: benign because the edge
  is already purged and the worker holds production until the release
  is consumed; the rebase lands at the leg's next gate entry. The code
  comment at session.rs:543-545 ("requires FRESH paired park +
  quiescence evidence") overstates latch freshness — the honest
  load-bearing wall is the worker program order. (P3 comment debt,
  pre-existing.)
- No interleaving was found where *discontinuous* stale park evidence
  satisfies a new commit; `SeekDisengaged` loss is confined to
  post-settlement (publish_evidence early-return, completion.rs:1234),
  where commit is unreachable.

## 6. Pause mechanism (D14.7)

Intent (Command, idempotent; routing withheld after stop/teardown/
settlement — completion.rs:576-593) → Engaged (leg parks at the
pre-GetBuffer gate; real leg verified at wasapi.rs:615-649 before
GetBuffer at :712) → TailQuiesced (once per engagement, `padding == 0`)
→ Disengaged (every park exit, including probe-failed) → resume is
Command only (no `Resumed` projection; AUTHORITY-CORRECTIVE, PR #150).

**What makes evidence belong to the current pause engagement without a
generation counter:** the Engaged event IS the fence — same-leg event
ordering makes Engaged happen-after any prior Disengaged, and Engaged
resets `tail_quiesced` and `disengagement_observed` (completion.rs:
1194-1198). A stale-cycle case is pinned by the delayed-delivery oracle
(completion.rs:1357-1413): Disengaged #1 arriving after pause #2 is
cleared only by Engaged #2. `Paused` = unsettled ∧ pause_requested ∧
TailQuiesced (handle.rs:225-229), the frozen formula.

**Paused episodes remain seekable** — frozen authority text
(ADR-PBK-002.md:1340-1344): "pause intent SURVIVES seek … A paused
episode's already-quiesced tail satisfies the output-cut precondition".
Code: pause intent appears in no acceptance condition; the worker
accepts `engaged` as the parked precondition (`leg_parked_evidence() =
engaged || seek_engaged`, completion.rs:747-750); a committed cut
rebases a paused leg MID-PARK (ports.rs:601-624). **Dual attribution is
contract-frozen, not accidental** — and attribution is structurally
separate in the other direction (seek events never touch pause latches,
so a cut park can never fabricate `Paused`). A pause-attributed park
lawfully satisfies a seek cut because the commit boundary reads a
physical conjunction (leg parked at the pre-GetBuffer gate ∧ tail
drained) that both park kinds prove equally.

## 7. Terminal settlement (D11)

Publication → settlement in ONE lock hold (`publish_evidence`,
completion.rs:1232-1243): if settled, later evidence returns early
("first-wins, later evidence cannot relabel"). `resolve()` (1249-1309):
worker failure dominant (decode/processing origin truthfully spelled,
D14.11), then drain verdict × worker terminal, with recorded stop
intent as the discriminator. Stop intent is recorded under the same
lock BEFORE the data-plane stop is released (523-553) — decision-time
stability: a stop linearized before the decisive publication
necessarily participates; a later one cannot relabel. Edge terminals
are first-wins (edge.rs:189-200), so a stop cannot overwrite a
committed EOF. Late commands: seek rejected (settled check), pause
recorded but routes nothing, resume inert history. Observation is one
coherent lock-held snapshot (1076-1120); the position projection is
withdrawn at settlement/activation failure.

**Can observation timing relabel a terminal outcome? NO — proven:** the
outcome is a memoized pure function of the lock-protected record at
publication time; `wait_terminal()` reads the committed value;
`observe()` derives from the same record; nothing writes `outcome`
after the first Some. Verified from code; the episode-terminal-settlement
TLA+ suite pins the decision table (specs/episode-terminal-settlement/).

## 8. Live DSP update mechanism (D14.11 / D4)

`ProcessingControl` (live.rs:80-98): desired whole configuration +
depth-1 latest-wins pending slot + last refusal diagnostic, one mutex.
`update_desired` composes+validates+commits under ONE hold (racing
field commands cannot lose a field — MUTANT N9 pins the split-lock
defect). `bind_for_activation` folds any pending update into the
initial applied configuration in the same hold. Worker pickup:
`take_pending()` exactly once per fresh staging block; apply boundary =
next whole unprocessed staging block; processed remainder never
reprocessed; Model C dual-processor crossfade, sample-driven, new side
from rest. Seek: RefusedUnchanged preserves everything; Applied drops
the transition and recompiles from rest (live.rs module doc; session
worker path).

**What substitutes for desired/applied generation:** the depth-1 slot
with take-once semantics + the one-hold atomic commit + the fresh-block
pickup boundary. Each accepted update is applied at most once, never
mid-block, and rapid updates coalesce latest-wins — the frozen contract
("complete-in-flight, latest-wins pending depth one"). **Sufficient for
ENGINE correctness**: no block ever mixes configurations outside the
designed crossfade; the settled stream is exactly a fresh instance of
the accepted configuration. **Explicitly NOT sufficient for a future
UI read model**: an external observer cannot tell which generation
produced block N; the module itself defers that ("the D5 read model is
a separate product decision", live.rs:111-114). The runtime-vs-
observation distinction the commission asked for is real and already
drawn in the code.

## 9. PcmEdge / drain mechanism (commission §9 — the F4 foundation)

PcmEdge: bounded 8192-frame FIFO ring (frames × channels samples),
one producer, one consumer, first-wins terminal, both endpoints woken
on every state change (edge.rs). Key structural facts, each cited:

```text
edge.rs:235-237   read_frames checks terminal BEFORE data: a FAILED or
                  STOPPED edge returns Stopped immediately and ABANDONS
                  buffered frames — there is no post-failure edge drain.
edge.rs:189-200   terminals first-wins; EOF is never downgraded.
session.rs:577-578  MutatedThenFailed order: decode_failed() (publishes
                  the Failed FACT wait_terminal awaits) THEN edge.fail()
                  — two statements on the worker thread with a
                  scheduler-open window between them.
```

**After producer failure is semantically committed, what PCM may still
be consumed lawfully?** Exactly:

```text
consumed_final ≤ stopped_at
                + (frames buffered in the edge at Failed-fact time)
                + (one render pull whose terminal check preceded fail())
                ≤ stopped_at + EDGE_CAPACITY_FRAMES   [8192 frames]
```

Derivation: between the Failed Fact publication (which anchors
`stopped_at`) and `edge.fail()` running on the (possibly descheduled)
worker thread, the edge is still Open and the render leg may lawfully
drain everything already buffered — at most one full edge capacity.
After `edge.fail()`, no further read succeeds (abandonment, above).
The render leg's read granularity in the test world is 256 frames
(tests/common/mod.rs:781), well under the capacity, so the in-flight
term is covered. Units: both sides count FRAMES (edge capacity is
frames; `consumed.fetch_add(n)` counts frames; the witness records one
value per frame). **The PR's bound `consumed ≤ stopped_at +
EDGE_CAPACITY_FRAMES` is SOUND and in correct units.** One precision:
the PR comment's narrative ("the consumer may still legally drain …
the drain is capped by the edge's capacity") is loose about WHERE the
drain happens — it is the pre-`edge.fail()` window, not a post-failure
edge drain; edge.rs abandons buffered frames once FAILED. The bound
and its units are right; the comment's mechanism story is imprecise
(P3, in-PR text).

## 10. Current temporal architecture summary

See §4 (T1–T10 plus the program-order call-out). The system's temporal
correctness is carried by **structure, not timestamps**: episode-scoped
ownership, single-writer program order on each actor, one linearizing
mutex per episode, one-operation-in-flight slots, acceptance-time reset
of operation evidence, engagement fences over world-state latches,
atomic three-valued commit sampling, bounded-queue abandonment rules,
join/consumption acknowledgements — and, on top, TEST-side
content-derived witnesses.

## 11. Does a shared temporal gap exist?

```text
TEMPORAL_ARCHITECTURE = IMPLICIT_BUT_SUFFICIENT
```

SUFFICIENT: every reconstruction attack failed; the TLA+ f5 model
attacks the safety collision directly (stale-PCM resurrection after
commit; 672 states, 7 counterexample-witnessed mutations); loom
explores the real edge under every interleaving; the D11 decision
table is model-checked. IMPLICIT: the cross-cycle attribution argument
is nowhere stated in one place — it is distributed across acceptance
reset + slot-free-on-resolution + gate-event fences + program order,
and two comments misstate pieces of it (§5 gaps). **Why, then, have
multiple flakes occurred? Because every recorded flake (F1–F4) is a
TEST observation-protocol defect, not a runtime defect**: the runtime
committed facts the tests then failed to observe premise-freely. The
recurrence is a methodology pattern (oracle timing premise), exactly as
the PR's report concludes — and this audit's independent reconstruction
reaches the same conclusion by a different route.

## 12. PR #196 scope audit (commission §13)

Diff classification (base c63cb75 → head 5d14261):

| File | Class | Notes |
|---|---|---|
| src/completion.rs (+206) | test-only | additions inside `mod tests` only; no production line touched |
| src/eq_tests.rs (+109/−5) | test-only | replaces sleep-wager with protocol bound; imports `crate::session::EDGE_CAPACITY_FRAMES` (already `pub(crate)`; no visibility change) |
| src/live_tests.rs (+109) | test-only | new race replay test + helper |
| tests/common/mod.rs (+31) | test-only | `under_cpu_load` helper |
| research/temporal-model-0/RESULTS.md (+408) | research evidence | audited in §21 |
| research/temporal-model-0/evidence/f1f2-load-replay.log (+16) | research evidence | matches §12 description |

```text
PRODUCTION_CHANGE  = NONE  (verified: no diff hunk outside test modules)
AUTHORITY_CHANGE   = NONE  (no ADR/docs/governance file touched)
```

Watch-item results: no `pub` visibility changes; the new completion.rs
tests initially leaked into loom builds (P1, §25) — fixed by audit
commit 0a79a33; comments added by the PR do not assert normative
authority (the F4 comment's drain narrative is imprecise but scopes
itself to the test; P3).

## 13. S0 provenance probe review (commission §14)

The four probes (completion.rs:1947-2141) drive the REAL publication
boundary: `publish_evidence`/`apply_gate_event` is the exact function
the render gate's observer runs in production (completion.rs:358-370),
and `request_seek/request_pause/seek_landing_published/
seek_cutover_decision` are the real command/worker surfaces. What they
bypass is only the actor threads — the standard white-box posture, and
the module comment says so.

- **S0-A (pended seek, absent pause intent):** proves the ATTRIBUTION
  lattice, verified against the real gate routing: with no pause intent
  `pause_park` returns Proceeded with zero events (ports.rs:588-596)
  and only `seek_park` publishes Seek* (ports.rs:661-682) — so the
  only park evidence a pended seek can act on absent pause intent IS
  its own cut park. It does not (and need not) prove the leg always
  parks promptly; the comment correctly calls the window length "a
  scheduling fact". This closes issue #195's UNVERIFIED hypothesis
  (ordinary data-waits do NOT latch park evidence — a starved leg
  waits inside its read, after the gate).
- **Starvation probe:** (a) intent-free loop-top parks nothing and
  publishes nothing — verified against the gate implementation; (b)
  pause attribution — the frozen dual attribution (D14.5), quoted in
  §6. Correctly models the starvation location: ordinary starvation is
  inside the read, after gate processing.
- **Cross-cycle probe:** covers landing, commit, park latches, release
  payload lifecycle (consumed exactly once, then None). NOT covered:
  `seek_refused` persistence across cycles (the reset of `seek_refused`
  is asserted nowhere) and the continuous-park carry (attack A3).
  Both are minor: the refusal latch feeds no commit predicate, and A3
  is physically-true world state. (P3 coverage notes.)
- **Paused-seek probe:** frozen authority (quoted in §6), not a
  test-created special case; the probe's ordering comment (resume
  before consume_release) is correct gate behavior.
- **Hygiene:** each probe releases or consumes all routed intent
  (verified by reading; probes assert the final `consume_release ==
  None`).
- **Could a probe pass while the production mechanism is wrong?** The
  cross-cycle probe would catch acceptance-reset removal and
  SeekDisengaged-clear removal — both demonstrated by this audit's
  mutations M-S0a/M-S0b (§23, both RED). The probes drive the same
  functions production drives, so a production regression in the
  attribution rule flips the corresponding probe.

**Negative controls required by §23:** executed by this audit (§23):
both required S0 mutants RED.

## 14. (merged into §13/§16/§23)

## 15. F3 seek×stop replay review (commission §15)

The oracle (live_tests.rs:2886-2980): each race iteration asserts
terminal `Stopped` in every legal ending and classifies the content by
STRUCTURE — exact tag ramps (values[i] ≈ i at absolute indices), one
allowed large downward step splitting pre/post-cut stretches. Answers
to the commission's seven challenges:

1. **All legal endings?** Yes for this scenario: the seek either
   grounds actionability before the stop (cut executes; landing Some(0)
   restarts the ramp under the same identity configuration) or the stop
   wins (no cut). A third shape (cut with UNKNOWN landing → position
   withdrawal) is not reachable here: `Applied { landing: Some(0) }` is
   scripted. The legal family is complete *for the scripted provider*.
2. **Corrupt stream accidentally matching?** A corrupt stream must be
   an exact concatenation of two exact ramps to pass — the class is
   tight enough that accidental membership requires reproducing the
   contract's own shape.
3. **Classifier complete?** Within its stated membership claim, yes:
   the accepted language is exactly `{single exact ramp from 0} ∪
   {[exact 0..k][exact 0..m] : k > 1001}` — which IS the two legal
   families. Corners found by mutation (§23): a pre-cut-side boundary
   frame drop is invisible (P3-C; frame conservation is pinned
   elsewhere, eq_tests.rs:1147-1151); the 1e-3·i tolerance admits
   ±0.1%-of-index corruptions at large indices (P3-D, pure sensitivity
   loss — the f32 tags are exact below 2^24); channel 1 is recorded but
   not asserted in this replay (P3-G; pinned in the eq/gain matrices).
4. **Multiple downward transitions?** A *visible* second cut is caught
   deterministically — any extra restart breaks the containing
   stretch's exact-ramp assert (mutation-proven: RED at "post-cut
   stretch: frame 1000 = 0"). As-placed, a two-cut mutant whose second
   cut falls beyond the stop truncation SURVIVES (§23 M4) — because
   the observable content of "one legal cut + an unobserved second
   cut" is literally identical to the legal one-cut ending. No
   consumed-content oracle could distinguish them; the finding bounds
   the oracle's claim (membership only, exactly as its comment says)
   rather than contradicting it (P2-B, recorded).
5. **Partial output / truncation hiding a stale segment?** The stop
   truncates output — a stale pre-cut segment after the cut could
   theoretically be cut off by the truncation itself. The oracle does
   NOT claim to catch stale content that never reached the witness;
   its proposition is about what the device consumed. This is the
   correct scope: the D14.5 invariant governs audible output, and the
   stale-exclusion mechanism itself is carried by the worker program
   order + TLA+ f5, not by this test.
6. **`Stopped` guaranteed for all legal outcomes?** Yes — request_stop
   is issued before wait_terminal in every iteration, and stop intent +
   aborted drain ⇒ Stopped (resolve discriminator; §7).
7. **Scheduler distribution change?** The oracle pins MEMBERSHIP, never
   the distribution (the ending counts are `println!` diagnostics
   only) — correct under §24's rule.

Mutation evidence: §23 (agent-executed; M2/M3-post-cut/M5 RED, M1/M5
as-shipped need a cut-path strengthener to activate, M4 survives as an
analyzed membership-claim bound — full table and outputs in
evidence/mutation-log.md).

## 16. F4 eq_tests fix review (commission §16)

**Bound correctness:** sound; units correct (frames vs frames);
derivation and precision note in §9. The import-from-production
(`EDGE_CAPACITY_FRAMES`) makes the bound track the real geometry — the
right direction of coupling.

**Prefix oracle:** the final consumed content must equal the control's
prefix, checked after the dispose() join ("the join IS the quiescence
acknowledgement"). Distinguishing power, mutation-proven: the
engine-realistic destructive-apply mutant (provider actually applies,
edge refilled with new-cursor content, then Failed) goes RED at a
deterministic index (divergence at index 22528: actual 223420.4 =
EQ of the 5s-target tag vs control 22611.6), and under the same mutant
both pre-existing oracles in the family also go RED. The failed seek
carries NO landing (MutatedThenFailed), so no landing-ambiguity exists;
the content is the EQ'd position-tag ramp, so new-cursor content
differs from the control prefix by ~10^5 in magnitude at the first
divergent frame. A common-mode EQ defect shared by both legs passes —
out of scope for this differential oracle (owned by the stage-level
bit-exact reference oracles in the same file). One scope bound,
empirically demonstrated (P3-F): the prefix law is a *consumed*-content
law — a "resume production post-Failed without failing the plane"
mutant whose new-cursor frames stay queued behind the 8192 buffered
pre-failure frames SURVIVES, because dispose()'s edge stop abandons
the still-buffered frames before the paced leg drains them. The test's
own failure message is worded accurately ("new-cursor production *that
reached the device*"); the producer-side obligation is pinned by this
oracle up to consumption-before-join, and beyond that by the worker
program order (the worker returns immediately after `edge.fail()`,
session.rs:579).

## 17. under_cpu_load review (commission §17)

Original implementation (tests/common/mod.rs:52-74): **panic-unsafe —
a panicking `f` skipped the stop flag and the joins, leaving four CPU
spinners alive process-wide for the rest of the test binary.** That is
exactly the hazard the commission names: the diagnostic-load helper
itself corrupting later tests' scheduling — in a campaign about load
sensitivity. Hazard assessment: manifests only on failure paths
(the test is already RED), but converts one failure into potential
timing-cascades elsewhere in the same binary. **P2. FIXED by audit
commit e453bc8** (catch_unwind + stop + join + resume_unwind); panic
path verified by a scratch suite in an isolated worktree (a panicking
body leaves no `qianqian-s0-load-*` thread alive); happy path re-verified
green; clippy clean. Remaining characteristics, acceptable: spinners
always joined on the normal path; machine-size dependence is real
(4 spinners on a 2-core CI runner slows the wrapped body ~5-10%,
measured §24) but the helper is diagnostic-only, documented as such,
and no correctness conclusion rests on it (both consumers say so in
prose and the distribution outputs are println-only).

## 18. Repository-wide timing-premise scan (commission §18)

Full inventory produced by the audit's scan agent (method: grep sweep
over crates/ + full reads of every candidate file; summary here, the
complete table is in the agent transcript archived at
research/temporal-mechanism-audit-0/evidence/):

```text
LIVENESS_BOUND                    ~120 sites  (within/wait_until/recv_timeout
                                              asserting a protocol commitment
                                              on timeout) — all legitimate
PERFORMANCE_MEASUREMENT           2 harnesses (navigation_waterfall phase
                                              timings; stage-cost bounds)
DIAGNOSTIC_ONLY                   6 (s0 distribution prints, under_cpu_load
                                              users, pacing sleeps)
SAFETY_ORACLE_WITH_EXPLICIT_ACK   ~70 (wait_terminal-anchored content
                                              oracles, join-anchored prefix
                                              laws, F4 protocol bound,
                                              structural negations)
SCHEDULER-PREMISE_RISK            7 discrete sites + 1 family (~50 sites)
```

The seven discrete risks, ranked:

| # | Rank | Location | Premise | F1–F4-equivalent? |
|---|---|---|---|---|
| 1 | P2 | stateful_probe_tests.rs:712-719 | 300ms sleep ≻ legal post-failure drain | YES — F4 verbatim; **recorded in PR §21 (medium)** |
| 2 | P2 | gain_tests.rs:299-305 | same | YES — F4; **recorded in PR §21** |
| 3 | P2 | tests/seek_seam.rs:529-540 | same (decode-failure Failed) | YES — F4; **recorded in PR §21** |
| 4 | P2 | tests/seek_seam.rs:987 | `wait_for_position_past(7s)`: poll must land inside ~170ms before terminal withdraws position | F3-family; **NOT in PR §21 — recorded by this audit** |
| 5 | P3 | tests/seek_seam.rs:444 | 200ms covers slot-free worker slice | weaker F2/F3 shape; recorded |
| 6 | P3 | tests/seek_seam.rs:577-592 | 200ms covers commit→payload-consumption→mid-pause rebase | recorded |
| 7 | P3 | tests/seek_seam.rs:701 | same as 5 | recorded |
| 8 | P3 | src/live_tests.rs:1491 | 400ms window for n4 mutant exposure | not recorded (P3) |
| 9 | P3 | tests/position_seam.rs:762-766 | unsettled-snapshot window (~256ms source) | not recorded (P3) |
| 10 | P3 family (~50 sites) | position-threshold entry waits across all suites | poll must land before withdrawal of `position` | F3-family, mitigated by SlowConsume pacing; not recorded as a class |

Key analytical refinement this audit adds (missing from the PR's §21
row): **a consumed-freeze after `Failed` is a wager only when the
Failed settles from `worker_failure` (decode/processing origin);
when it settles from the drain verdict (device origin, e.g.
seek_seam.rs:917) the freeze is already anchored** — the leg exited
before the Fact published. This explains why three sites flake and
three look-alikes never will, and should steer the future repairs.

The two NEW tests are CLEAN (no scheduler premise): the eq replay's
observation is anchored by the dispose() join; the race replay designs
the withdrawal race out (paced source cannot EOF before the test's own
stop) and pins membership, not schedule.

PR residual-list accuracy: all six named entries verified verbatim and
correctly graded; misses are item 4 above (P2) and the class-level
recording of items 8-10 (P3).

## 19. Oracle discipline (commission §19)

The audit supports formulating the rule, and the repository has already
converged on it in practice (wait_until bounds with asserted timeouts;
join-anchored content laws; protocol bounds). Recommended formulation
(test methodology guidance, NOT architecture authority):

> For safety assertions, a scheduler-positioned observation is not
> semantic evidence. Anchor every safety oracle in one of: an explicit
> mechanism acknowledgement (join, wait_terminal, a committed Fact, a
> protocol quiescence point, payload consumption), a bounded structural
> invariant (queue capacity, first-wins terminal), or a content-derived
> witness (membership in the contract-legal family). Wall-clock waits
> are for liveness failure detection, performance characterization, and
> diagnostic stress only. A negative assertion ("nothing changed")
> anchored only by a sleep is the canonical defect shape; a negative
> assertion anchored by an ack that structurally freezes the quantity
> is legitimate.

Placement recommendation: this belongs in a test-infrastructure README
or the repository testing guide as *guidance*; AGENTS.md already
carries the general principle ("Report what was actually verified",
"Real-path testing") and should not grow a second authority. Until a
governance home is chosen, this report records it. **Not promoted into
ADR authority by this audit** (no authority change requested or made).

## 20. Does #196 really solve the class? (commission §20)

| Flake family | Before | PR #196 | Residual risk | Verdict |
|---|---|---|---|---|
| cross-run frame identity (F1) | premise: two runs' streams frame-identical | F1 oracle recast to content-vs-control divergence (eq applied-seek oracle, 1030-1098) | none known in family | FIXED_INSTANCE |
| processed-vs-consumed witness (F2) | TransitionStarted.at_frame treated as consumed | anchor = content onset (−1), m·1024 enumeration, MAX_INFLIGHT_LEAD=1 | none known | FIXED_INSTANCE |
| observed-but-not-actionable window (F3) | premise: pended seek stays unactionable until stop | premise-free two-family oracle + S0 provenance probes pinning attribution | family recurrence at seek_seam.rs:987 (P2, now recorded) + ~50-site mitigated family (P3) | FIXED_INSTANCE + CLASS_METHOD_APPLIES; **family recording was incomplete → completed by this audit** |
| fixed drain sleep (F4) | 300ms wager on drain completion | protocol bound + join-anchored prefix law | 3 verbatim siblings remain, recorded as medium debt (§18 items 1-3) | FIXED_INSTANCE; CLASS_METHOD_NOT_YET_APPLIED to siblings (recorded) |
| consumed-freeze sleeps elsewhere | same shape | method demonstrated, not applied | 3 P2 sites (recorded) | CLASS_METHOD_NOT_YET_APPLIED (recorded debt) |
| linearization sleeps | 200ms wagers on internal ordering | recorded debt (low) | low likelihood (100× nominal margins) | SEPARATE_VALID_TIMEOUT-class debt, recorded |
| pause "nothing changed" sleeps | negation after fixed sleep | recorded debt (low); audit verification shows these are ack-anchored (structural freeze) → false-PASS direction only | none false-RED | SEPARATE_VALID_TIMEOUT (mis-graded direction in PR list; P3) |
| future async #187 stale result | hypothetical read-model staleness | declared out of scope; model-check argument only | none current | UNRESOLVED is wrong word — OUT_OF_SCOPE_BY_DECLARATION |

```text
CLASS_ROOT_CAUSE = ORACLE_TIMING_PREMISE (tests asserting scheduling
                   outcomes as semantic facts)
CLASS_METHOD     = anchor every safety oracle in acknowledgement /
                   commit state / structural bound / content witness
CLASS_STATUS     = the method is defensible, demonstrated on F1–F4,
                   and its remaining applications are now fully
                   recorded as debt (the recording gap closed by
                   §18 item 4 and items 8-10 of this report)
```

The PR never claims "class closed" (verified by grep; its verdict is
`TEMPORAL_MODEL_0 = EXISTING_MODEL_SUFFICIENT` with scoped residuals),
so the honest-recording condition is now met end-to-end.

## 21. RESULTS.md evidence audit (commission §21)

Audited claim-by-claim (agent-executed with git/issue/PR corroboration;
full table in the audit transcript):

```text
F1 history                      SUPPORTED (commit chain + verbatim quotes)
F2 history                      SUPPORTED (5 review rounds; sub-mechanisms
                                verbatim in PR #194 comments)
F3 provenance (#195)            SUPPORTED (issue text verbatim; file:line
                                citations exact in the working tree)
F4 root cause                   SUPPORTED (old assertion verbatim at base;
                                issue #190 D6 records it; mechanism matches
                                session.rs/edge.rs; one imprecision: the
                                "drain" is the pre-edge.fail() window — §9)
K8s analogy                     SUPPORTED (substance-vocabulary, explicitly
                                future; no mechanism-equivalence claim)
GStreamer analogy               SUPPORTED ("maps 1:1" is strong but every
                                mapped element exists in code)
DeepSeek comparison             SUPPORTED as framing (used to REJECT M3;
                                external quotes unverifiable, isolated per
                                AGENTS.md external-evidence rule)
M1 sufficiency                  SUPPORTED for the flake family (real in-tree
                                probes/replays + TLA+/mutation assets);
                                general sufficiency is a razor argument the
                                report owns and labels
Lean4/TLA+ decisions            SUPPORTED (specs coverage claims verified;
                                TLA+ CI suite green on head)
"production already embodies    SUPPORTED at the report's scope; §1's
the substance" / "no gap"       unqualified opener is the one sentence
                                broader than its evidence class (P3)
"class closed" language         NOT PRESENT anywhere in the report
```

Residual debt recorded by the PR (§21+§22, extracted verbatim by the
audit): the three consumed-freeze siblings (medium), the four
linearization sleeps (low), the two pause negations (low), #195's
authority question (open, owner S2 ruling), corrective-3's recorded
unbounded-Pending stall (pre-existing, unchanged), one unexplained
probe-authoring hang (classified environmental, honestly disclosed).

## 22. Lean4 / TLA+ independent verdict (commission §22)

After reconstruction: **every safety invariant identified is defensibly
carried today.**

```text
- stale-PCM-after-commit (the D14.5 root invariant):
  carried by specs/f5-seek-discontinuity TLA+ (safety model, 7
  counterexample-witnessed mutations) + worker program order + loom on
  the real edge.
- terminal relabeling (D11): carried by the episode-terminal-settlement
  TLA+ decision table + first-wins structure.
- cross-cycle evidence attribution: carried by acceptance-reset
  structure + the two S0 negative controls (M-S0a/M-S0b RED) + the seek
  matrices. The remaining subtlety — the continuous-park carry (A3) and
  slot-free-before-Disengaged ordering — is real but its safety rests
  on physical continuity; it is the ONE candidate for a small permanent
  safety theorem.
```

```text
LEAN4    = NOT_NEEDED_NOW
TLA_PLUS = NOT_NEEDED_NOW (existing suites suffice; if #195's S2 ruling
           narrows the actionable predicate, extend the existing f5 TLA+
           model — the natural home the PR names — rather than a new one)
```

The exact theorem Lean4 could carry, if anyone ever wants it (recorded,
not recommended): *"For all seek cycles c2 accepted after cycle c1
resolved: c2's commit requires a landing publication that happens-after
c2's acceptance."* The acceptance reset makes this nearly trivial
(the latch is None at acceptance and single-writer between); formal
ceremony would not currently change any decision. No formal-method
ceremony is warranted.

## 23. Mutations / negative controls (commission §23)

Required by the commission: S0 stale-evidence, F3 stale-pre-cut-leak,
F4 post-failure new-cursor — all must RED. Executed:

**S0 (executed by the auditor, isolated worktree /tmp/qn-s0mut from the
PR head; restored after):**

```text
M-S0a  remove the acceptance-time seek_landing reset
       (completion.rs:711)          → s0_operation_evidence_cannot_cross_cycles
                                      RED (0 passed; "landing cannot cross
                                      cycles")                    [REQUIRED: RED ✓]
M-S0b  make GateEvent::SeekDisengaged clear nothing
       (completion.rs:1216-1219)    → same probe RED (!leg_parked_evidence)
                                                                    [REQUIRED: RED ✓]
```

**F3/F4 (executed by the audit's mutation agent in the isolated
worktree /tmp/qn-mut from the PR head; every mutant engine-side — the
mock decode double or the test script — never inside the oracle; tree
restored after each row):**

```text
F3 M1  stale pre-cut frame after cut     as-shipped GREEN (mutant non-
         (TestDecodeStream stale_inject)   activation: 15/15 no-cut on this
                                           idle 20-core host — the seek is
                                           never called); with a 50ms cut-
                                           path strengthener: RED ("post-cut
                                           stretch: frame 0 = 1"). The kill
                                           run proves the class is caught
                                           once a cut ending occurs.
F3 M2  duplicate one segment (rewind     RED unloaded, first iteration
         1024 inside the consumed window)  ("post-cut stretch: frame 0 = 476")
F3 M3  drop one cut-boundary frame,      post-cut side: RED (same class);
         post-cut side                     PRE-cut side: GREEN by analysis —
                                           [0..k−1][0..m] satisfies both
                                           exact-ramp asserts (P3-C; frame
                                           conservation pinned at
                                           eq_tests.rs:1147-1151)
F3 M4  produce two cuts                  as-placed SURVIVES (genuine scope
                                           bound, P2-B): one visible exact
                                           restart-to-0 is content-identical
                                           to the LEGAL seek cut, and the
                                           second restart fell beyond the
                                           stop truncation; a VISIBLE second
                                           cut is RED ("post-cut stretch:
                                           frame 1000 = 0"). The oracle
                                           verifies membership, never
                                           attribution — exactly as its
                                           comment claims.
F3 M5  mis-land by one frame             RED with strengthener ("post-cut
         (script Some(0) → Some(1))        stretch: frame 0 = 1"); pins the
                                           scripted landing exactly.
F4 M   post-failure new-cursor content   RED at a deterministic index
         reaches the consumer              (eq_tests.rs:1618; divergence at
         (engine-realistic: provider       22528: 223420.4 vs control
         applies, edge refilled from the   22611.6 = EQ of the 5s tag);
         mutated cursor before failing)    under the same mutant the two
                                           pre-existing family oracles also
                                           RED (2 failed).               [REQUIRED: RED ✓]
```

F4 supplementary (scope bound, informative): a cruder mutant that
resumes worker production post-Failed WITHOUT failing the plane
survives — the new-cursor frames queue behind the 8192 buffered
pre-failure frames and dispose()'s stop abandons them unobserved
(§16 P3-F). The required mutant shape (content actually reaching the
consumer) is caught deterministically.

Detailed commands and outputs: the mutation agent's transcript is
archived under research/temporal-mechanism-audit-0/evidence/. No
mutant was committed anywhere; all worktrees restored.

```text
NEGATIVE_CONTROL_VERDICT = the three REQUIRED mutants all RED (S0
                           M-S0a/M-S0b; F4 destructive-apply at a
                           deterministic index). F3's M2/M3-post/M5 RED
                           outright; M1/M5-as-shipped need a cut-path
                           strengthener because the idle-host
                           distribution never realizes a cut ending
                           (P3-E); M4-as-placed survives as an analyzed
                           scope bound of the membership claim (P2-B).
```

## 24. Remaining debt (consolidated, this audit is the recording of record)

```text
P2  seek_seam.rs:987  wait_for_position_past(7s) — F3-equivalent ~170ms
    withdrawal window, missing from the PR's §21 list. DISPOSITION:
    recorded here; repair with the premise-free pattern (widen the
    threshold, wait on a committed fact, or wait_until with asserted
    timeout) in a follow-up test-debt pass.
P2→P3 three consumed-freeze siblings (stateful_probe:712, gain:299,
    seek_seam:529) — already recorded by the PR as medium debt;
    repair is mechanical (the eq_tests bound/join pattern).
P3  the ~50-site position-threshold entry-wait family — record once,
    generically, as mitigated debt (SlowConsume pacing keeps windows
    wide).
P3  F4 comment imprecision (the "drain" is the pre-edge.fail() window).
P3  pre-existing doc contradiction: ports.rs:279-282 and
    completion.rs:1181-1183 say "a pause park never satisfies a seek
    commit" while the predicate (851-852), the frozen authority
    (D14.5 pause interaction) and the pinning probe make it a frozen,
    tested behavior. A future agent taking the comment at its word
    could "fix" the predicate and break paused-seek reuse. Comment-only
    correction recommended in a follow-up (not done here: it would
    widen this PR's test-only scope).
P3  session.rs:543-545 comment overstates latch freshness ("requires
    FRESH paired park + quiescence evidence"); the honest load-bearing
    wall is worker program order (ADR-PBK-002.md:1277-1280).
P3  continuous-park cross-cycle carry (attack A3) unpinned by any probe
    (benign; physical-continuity argument in §5).
P2-B  F3 oracle bound (recorded, no PR overclaim): the oracle verifies
    ending MEMBERSHIP, never cut attribution — a single visible exact
    restart-to-0 is content-identical to the legal seek cut, so a
    spurious-restart engine defect would pass this oracle (and any
    consumed-content oracle; §23 M4). Cut causality is pinned by the
    white-box seek matrices + TLA+ f5, not by this replay.
P3-C  F3 replay does not pin frame-count conservation at the cut joint
    (pre-cut-side drop invisible; pinned elsewhere, eq_tests.rs:1147).
P3-D  F3 ramp tolerance (1e-3·i) admits ±0.1%-of-index corruptions at
    large indices — pure sensitivity loss (f32 tags are exact).
P3-E  F3 cut-family coverage is host-dependent: on an idle 20-core
    host the as-shipped scenario realized 0 cuts in 15 runs (even
    under 4 spinners), so the cut leg of the oracle — and the
    cut-dependent mutants — never exercise there. Both pure
    distributions demonstrated GREEN (all-no-cut as shipped; all-cut
    via strengthener), so the oracle is distribution-correct; per-run
    evidential coverage of the cut family is simply zero on fast idle
    hosts. A deterministic cut-path strengthener would make coverage
    host-independent.
P3-F  F4 prefix law is a consumed-content law (§16): misproduced
    content still buffered at dispose() is abandoned unobserved; the
    test's wording ("reached the device") matches what it verifies.
P3-G  F3 replay asserts channel 0 only (channel 1 recorded, asserted in
    the eq/gain matrices instead).
P3  RESULTS.md §1 unqualified opener slightly broader than its evidence
    class (§21).
```

## 25. CI / exact-head gate (commission §25)

Original head 5d14261: six of seven checks green; **`matrices + Miri +
mutations + loom (specs/check.sh rust)` FAILED** — classified, not
retried: a deterministic compile failure of the `--cfg loom` lib-test
build (11 errors: the four new S0 probes referenced
`cfg(not(loom))`-gated helpers/import vocabulary without being gated
themselves). Reproduced locally byte-for-byte (11 errors); the base
commit's loom suite is green (12 passed), isolating the regression to
this PR. The RESULTS.md report claimed only the *local* gate (which
indeed excludes loom), so the claim is literally accurate but the PR
was not CI-complete.

**Audit corrective 0a79a33** gates the four probes `#[cfg(not(loom))]`
(matching every other thread-spawning suite in the file); loom suite
re-verified green locally (12 passed, 165s). **Audit corrective
e453bc8** makes under_cpu_load panic-safe (§17). Full local pre-push
gate (`lefthook run pre-push --all-files`) green on the corrected
tree: vocabulary, plugin-boundary, fmt, clippy, rust-test (159 passed),
docs-verify.

## 26. Stress methodology and evidence (commission §24)

Stress is not correctness proof; it answers only whether the
premise-free oracles stay stable under scheduling variation. Shapes
run (all on the corrected head, `s0_replay_the_seek_stop_race` +
`s0_replay_mutated_then_failed` unless noted):

```text
unloaded, 3 repeats                 3× GREEN (17.7-17.8s)
8 external spinner processes, 3×    3× GREEN (18.2-18.5s)  [the pressure
                                    class that used to flip F1-F3 endings]
8 spinners + --test-threads=8,      GREEN — 159 passed / 0 failed /
full lib suite (159 tests)           69.24s (63.7s unloaded baseline)
```

An additional host-dependence datum (P3-E): on the idle 20-core audit
host the F3 race realized 15/15 no-cut endings, i.e. the CUT leg of
the two-family oracle never exercised locally; the mutation agent
demonstrated the cut leg green under an all-cut distribution via a
strengthener, so the oracle is correct on both pure distributions.

Per §24's rule, both legal race endings were NOT required to appear;
the oracle pins membership and passed with whichever ending the
scheduler picked. No assertion rests on the ending distribution
(println-only). Machine dependence noted: the load shapes add ~3-5%
wall time locally; the tests' `within` bounds have 60× headroom.

## 27. Severity classification summary (commission §27)

```text
P0  none found.
P1  loom-build compile failure on the required CI job (original head).
    → FIXED by commit 0a79a33; verdict against the original head:
      CHANGES_REQUIRED.
P2  under_cpu_load spinner leak on panic.        → FIXED by commit e453bc8.
P2  seek_seam.rs:987 F3-equivalent risk omitted
    from the recorded debt.                      → recorded (§24); repair follow-up.
P3  F4 comment imprecision; pre-existing attribution-doc contradiction
    (ports.rs:279-282, completion.rs:1181-1183); session.rs:543-545
    freshness overstatement; A3 unpinned; RESULTS.md off-by-one
    citations (session.rs:852→853; completion.rs:694→693-695);
    pause-negation debt rows mis-graded direction (false-PASS, not
    false-RED); F3/F4 scope bounds and recordings of §23/§24
    (P2-B claim bound; P3-C/D/E/F/G).  → recorded (§24).
```

## 28. Central adversarial question (commission §30)

> If I deliberately perturb scheduling as aggressively as possible,
> what semantic predicate in these tests remains invariant?

For the PR's new oracles the invariant predicates are exactly of the
grounded kind:

```text
"the protocol committed X"        wait_terminal() returned Failed/Stopped;
                                  the outcome is first-wins under the lock
"the mechanism acknowledged X"    dispose() joined the leg (no writer can
                                  remain); release payload consumed
"the final content belongs to     exact ramp membership (F3); control
 legal family X"                  prefix law (F4)
"the bounded queue structurally   consumed ≤ stopped_at + EDGE_CAPACITY_FRAMES
 limits X"
```

Not one of the new oracles reduces to "the thread probably hasn't
reached X yet". The repository's residual scheduler-premise sites
(§18) are exactly the places where that reduction still exists, and
they are now recorded.

## 29. Final verdicts

```text
CURRENT_TEMPORAL_MECHANISM =
    SUFFICIENT_BUT_IMPLICIT

TEMPORAL_CORRECTNESS_IS_GROUNDED_BY =
    episode-scoped ownership;
    single-writer program order per actor (decode worker; render leg);
    the one completion-mutex linearization boundary;
    one-operation-in-flight slots (seek; terminal first-wins);
    acceptance-time reset of per-cycle operation evidence;
    engagement-fenced world-state latches (pause pair; seek pair);
    atomic commit predicates (three-valued cutover sample; resolve());
    bounded-edge semantics (capacity; first-wins terminal; abandon-on-
    FAILED/STOPPED);
    join / payload-consumption acknowledgements;
    worker program order at the serialization point (purge → landing →
    hold) as the load-bearing stale-PCM wall;
    [test side] content-derived witnesses + ack-anchored oracles.
```

```text
PR_196_HEAD = 5d1426112937facb1a1643acbd3651c4a6fb3d8f
              (+ audit correctives 0a79a33, e453bc8 on the branch)
P0 = none
P1 = loom compile failure on required CI (original head) — fixed
P2 = under_cpu_load panic-unsafety — fixed;
     seek_seam.rs:987 debt-recording omission — recorded
P3 = §24 list (comment contradictions, imprecision, unpinned A3,
     family recordings)

F1 = FIXED_INSTANCE (content-vs-control oracle, verified)
F2 = FIXED_INSTANCE (content-onset anchor, verified)
F3 = FIXED_INSTANCE + CLASS_METHOD_APPLIES (two-family membership
     oracle + S0 provenance; verified mutation-sensitive within its
     membership claim — §23 M4 bound recorded as P2-B; family
     recording completed by this audit)
F4 = FIXED_INSTANCE (protocol bound sound + join-anchored prefix law;
     negative controls RED); siblings remain recorded debt

CLASS_ROOT_CAUSE       = ORACLE_TIMING_PREMISE (confirmed independently)
CLASS_METHOD           = acknowledgement/commit/structure/content-anchored
                         oracles (§19); defensible and demonstrated
CLASS_REMAINING_DEBT   = §24 (recorded; three mechanical repairs + one
                         narrow repair + recordings)

PRODUCTION_CHANGE  = NONE (verified; unchanged by the audit)
AUTHORITY_CHANGE   = NONE (verified; unchanged by the audit)

LEAN4    = NOT_NEEDED_NOW
TLA_PLUS = NOT_NEEDED_NOW
```

```text
PR_196 = CHANGES_REQUIRED   (as reviewed at 5d14261: required CI red)

With corrective commits 0a79a33 + e453bc8 pushed and a green CI run on
the corrected head, this audit's recommendation becomes APPROVE with
the §24 debt dispositions — no further code change required.
```

Do not merge until the corrected head's CI (including the loom/miri/
mutations job and the Windows gate) is green; the corrections touch
test-partitioning and test-infrastructure, exactly the surfaces CI
re-validates.
