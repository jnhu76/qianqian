# F5-GATE mechanism evidence — Seek discontinuity (Issue #119)

```text
QIANQIAN-F5-SEEK-DISCONTINUITY-GATE-1

VERDICT:        READY_FOR_HUMAN_REVIEW
BASE_SHA:       8765b00e3236e4b9af6afc826ee8f9ff6c13e5e4 (main; PR #152 merged)
BRANCH:         research/f5-seek-discontinuity-gate-1
STOP_REASON:    READY_FOR_HUMAN_REVIEW (gate stops before production Seek code)
IMPLEMENTATION_AUTHORIZED_IN_THIS_PR = NO
```

Pre-flight: PR #152 verified MERGED (2026-09-17T11:09:02Z, merge commit =
BASE_SHA); F4 production implementation is on main; Issue #119 REV.3
authorizes F5 as the current gate behind four mechanism mini-gates, and
ADR-PBK-002 §14 lists the open decision this gate closes:
"seek physical-output cutover mechanism beyond D14's frozen stale-PCM
invariant".

---

## 1. Authority read (what was already frozen before this gate)

- **D14.5** (ADR-PBK-002 §20) already freezes the Seek semantic spine:
  same-episode ownership (no new Plugin/Fiber/lifecycle noun), the
  stale-PCM invariant, reservoir accounting (worker staging, PcmEdge,
  submitted output/device buffer), no single vague success bit, and —
  critically — **the physical output cutover mechanism stays OPEN**;
  F5 production implementation must stop at that gap. §14 repeats it as
  the open decision. **This gate's job is to close that gap and the
  surrounding policy decisions, not to implement.**
- **D14.8** freezes Position as the episode-local device-consumed
  Projection and its F5 rule: the monotone publication MUST NOT mix
  pre- and post-cutover handed-off totals; how the accumulator is
  rebased and how the landing enters the projection is owned by the F5
  cutover decision (this gate).
- **D14.7** freezes the pause gate (park strictly before GetBuffer, no
  device buffer held across a park), the output-tail-quiescence reading
  (`GetCurrentPadding() == 0` after engagement, physically evidenced on
  a Windows host at 28–30 ms drain), and the truth-class firewall:
  internal mechanism parks must not become user Paused.
- **D11** owns terminal settlement; late-command stability (a command
  after decisive evidence cannot relabel the outcome). §17 still-OPEN
  includes "seek product-state vocabulary / actual-landing authority
  beyond D14 minimum" — partially addressed by this gate (§9 below).
- **PBK-001 §6 P1–P5**: triggered only by live old/new RT resource
  overlap. Issue #119 REV.3 already records the verdict direction:
  same-resource protocol defaults to NO P1–P5; only option B/C (resource
  replacement) would enter it.
- **SongCore ABI v1** (header contract): `song_seek(handle,
  requested_position_us, out_actual_position_us)` — clamps against known
  duration, flushes decoder state, next read belongs to the landing,
  reports the actual landing (−1 = unknown, never manufactured),
  SEEK_UNSUPPORTED/SEEK_ERROR error paths, format/metadata unchanged.

## 2. Root invariant (frozen attack target, unchanged from D14.5)

> Let C be the committed seek cutover boundary. After C, no PCM that
> semantically belongs to the pre-seek side may later contribute to
> output — including stale PCM already decoded, staged, queued in
> PcmEdge, pulled by render, copied into a device buffer, or submitted
> into WASAPI padding.

Before commit, old output is legal (the device finishing the old tail is
not a violation). After commit it must be impossible, not unlikely.

## 3. Production inventory and stale-PCM map (current reality at BASE)

Pipeline: `decode_worker` (owns `Box<dyn DecodedPcmStream>` + 1024-frame
staging) → blocking `edge.write` → `PcmEdge` (8192-frame ring ≈ 185 ms)
→ `read_frames` directly into `GetBuffer` memory → `ReleaseBuffer` →
shared-mode device padding → device-consumed (D14.8 Position evidence).
There is no command path into the worker today; the edge has no
non-terminal flush; the render loop has no cut concept; the position
cell has no rebase. `song_seek` exists in the ABI but is not surfaced by
`DecodedPcmStream`.

Stale-PCM reservoir map (measured/inventory result):

| Stage | Can contain pre-seek PCM? | Owner | Can invalidate? | Cost | RT impact |
|---|---|---|---|---|---|
| decoder internal (native packets/codec state) | YES until `song_seek` (which flushes per ABI) | Decode provider (SongCore) | `song_seek` only path | 12–307 µs measured (E1) | off-RT (worker thread) |
| decode worker staging (1024 frames) | YES — freshly decoded, not yet written | worker local | serialization-point discipline: discard at loop top (nothing to free — buffer is reused) | zero | off-RT |
| PcmEdge ring (8192 frames) | YES — up to full capacity | Session (episode-owned) | new non-terminal invalidate under the edge lock, protocol-guaranteed quiescent endpoints (two-phase; §6) | O(1) cursor reset | consumer parked — none |
| render-held device buffer memory (GetBuffer..ReleaseBuffer window) | YES — one iteration's fill | Output provider (render leg) | park at loop top: leg never holds a buffer across a park (D14.7 frozen invariant) | zero | park = none |
| WASAPI queued padding | YES — up to one device buffer (measured 984 frames ≈ 22 ms; full-buffer 984 observed at engage in E3) | device stream | **natural drain to padding==0** (mechanism A, selected); Stop/Reset measured and rejected (§7) | ≤ one buffer duration (31.2 ms measured) | parked leg, bounded drain wait |
| device-consumed | irreversibly gone (pre-commit consumption is legal old output) | — | N/A | — | — |

## 4. E1 — decoder seek reality (measured; evidence/f5seek-run{1,2,3}.log)

3 runs × 4 committed fixtures (MP3 CBR, FLAC 16/44, ALAC 16/44, ALAC
long), every probe internal assertion green in runs 2–3 (run 1 is the
superseded probe version whose 24 "failures" are probe bugs — µs→frame
truncation, near-EOF windows, a negative-target test at an exhausted
cursor — kept as raw record; the corrected probe is runs 2–3).

```text
requested vs reported landing   FLAC/ALAC: reported landing = first retained
                                frame ± 1 frame (µs quantization) — honest
                                MP3: reported landing sits at/before the
                                requested target; the first CONTENT-exact
                                window converges at skip = 3456 frames
                                (3 MP3 frames) — the codec's documented
                                seek tolerance; no content-level exactness
                                is promised for lossy
pre-target emission             lossless YES: landing is block-aligned at/
                                before target (up to ~648 frames ≈ 15 ms
                                early, e.g. requested 88200 → retained 87552)
                                — the basis must be the LANDING, never the
                                requested target (D14.8 "unknown ≠ zero"
                                honesty, applied to targets)
clamp beyond duration           SONG_OK, lands at the last container block
                                (MP3 176256, FLAC/ALAC last block); EOF
                                follows naturally; tail counts consistent
seek before any read            works (landing 0, content exact)
back-to-back seeks              second landing honored exactly (no state
                                contamination at decoder level)
negative target                 INVALID_ARGUMENT in ~140 ns (pre-parse);
                                decoder remains usable (post-rejection
                                decode locates correctly) — pre-cut failure
                                class leaves playback intact
post-failure usability          verified only for the validation-failure
                                class (INVALID_ARGUMENT); SEEK_ERROR/
                                SEEK_UNSUPPORTED post-state NOT exercisable
                                on this corpus — recorded as a limitation;
                                SEEK_UNSUPPORTED not exercised (all corpus
                                formats seek) — ABI contract only
seek latency                    12–31 µs (MP3), 150–310 µs (FLAC/ALAC) —
                                bounded, far below the output drain cost
format stability                re-probe after every seek: unchanged
```

Decoder decision: target unit = **source-relative media time in
microseconds** (the ABI's native unit — zero conversion on the command
path); the retained-PCM start is `out_actual_position_us` (−1 = unknown,
never manufactured, never replaced by the requested target); seek does
NOT promise sample exactness for lossy (measured above), and the product
must not claim it.

## 5. E2 — edge cut protocol (evidence/f5edge-run{1,2,3}.log)

Probe carries a minimal faithful copy of the PcmEdge synchronization
shape (Mutex ring + two condvars + first-wins monotone terminal) with
structured frames, plus the candidate **non-terminal invalidate**
primitive (`read_pos=write_pos=buffered=0` under the state lock;
terminal untouched; both condvars notified). 3 runs × 255 scenarios:

```text
deterministic / backward-seek / landing-zero   clean (no stale output)
blocked-producer (T4/T5: producer parked       clean — the two-phase cut
inside write() on a full edge at cut time)     resolves it (see below)
randomized ×200 (sizes, landings, jitter)      clean
rogue-staging negative control ×51             FIRED 51/51 (stale output
                                               witnessed whenever the worker
                                               skips the staging discard)
```

**Why two phases.** A session-side invalidate while the consumer is
parked empties the queue and unblocks a producer blocked on a full edge —
but that producer's in-flight block then lands in the emptied ring, and
an old producer would keep re-filling. The stale exclusion is therefore
NOT carried by the primitive; it is carried by the worker's
serialization point: at its loop top the worker discards staging, runs
its own invalidate (phase 2), publishes landing, and only then writes —
program order guarantees no old write after the phase-2 cut by the only
producer thread. The negative control proves the discipline is
load-bearing: the primitive alone is not safe. The flush never touches
the terminal (flush ≠ Eof/Failed/Stopped; first-wins terminal semantics
intact), so the acceptance rule "data plane must be Open" survives.

Linearization: the invalidate is an ordinary lock-held reset; it needs
no new lifecycle concept because the protocol — not the primitive —
guarantees no endpoint is inside read/write across the commit (consumer
parked, producer at its own serialization point). Loom coverage of the
real edge lands with the F5 implementation gate (the probe documents the
protocol; loom_edge_tests already explore the unmodified edge shape).

## 6. E3 — output-side physical cut (evidence/f5cut-run{1,2,3}.log)

Windows host (WSL2 → Windows staging), shared-mode event-driven, 44.1 kHz
stereo float32, AUTOCONVERTPCM leg (endpoint refuses the exact format —
same finding as f4probe), buffer 984 frames ≈ 22 ms. 3/3 green runs.

**Experiment A — park + natural drain (the selected mechanism):**

```text
padding at engage            984 (a full device buffer of old audio)
drain observations           984 → 543 → 102 → 0 (2 ms poll)
drain latency                31.2 ms engage → first zero (consistent with
                             D14.7's physically measured 28–30 ms)
device position              advanced through the old tail while the leg
                             was parked (pos 10648 → 21664 ≥ padding) —
                             the device physically consumed the old
                             frames; GetPosition never reset
after commit (padding==0)    refill with the new signal: GetBuffer/
                             ReleaseBuffer normal, padding grows and
                             drains normally, position continues
                             monotonically (21664 → 38864)
```

The correctness reading is the one D14.7 already froze and f3probe
already evidenced physically: padding is exactly this stream's
queued-to-play frames, so a zero observation after engagement proves
nothing submitted before engagement remains queued-to-play. Mechanism A
adds **no removal mechanism at all** — it waits for consumption; after
commit only new frames exist to submit. This gate's new physical claim
is only that the composition behaves as that reading requires on real
hardware, and it is evidenced above (3 runs, raw logs retained). The
audible-cutover semantics (post-commit old PCM cannot be audible)
therefore inherits D14.7's physical evidence class; no new acoustic
experiment was required by the selected mechanism. `PHYSICAL_PRODUCTION_
SMOKE` for the eventual F5 implementation remains NOT-RUN (that gate
will re-run its own).

**Experiment B — Stop/Reset/Start (comparison record; NOT selected):**

```text
after Stop                   padding stays 984 (mid-buffer audio frozen —
                             D14.7's measured objection, reproduced)
Reset                        Ok; padding → 0 (queued PCM discarded — the
                             discard itself works on this endpoint)
Reset state consequence      device position → 0 (the stream clock loses
                             its origin — a state change every consumer
                             of device-position semantics would inherit)
Start + refill               works on this endpoint (event loop survives)
```

Rejected for v1: it adds a stream-state machine to every seek for a
latency win (~0 ms vs ~31 ms) that is inaudible in context, carries the
frozen-mid-buffer objection, resets device position, and would need its
own cross-endpoint physical campaign. Re-earnable only by a new narrow
authority decision (same posture as D14.7's mechanism B).

## 7. Cutover commit point (the central deliverable)

Selected commit boundary — the **earliest point that is actually safe**:

```text
CommitCut holds when ALL of:
    landing            worker published "decoder repositioned at L"
                       (strictly after its phase-2 edge invalidate)
    edge clean         edge invalidated (phase-2, by the worker itself)
    tail quiesced      padding == 0 observed while the leg is parked
                       (D14.7 evidence class)
    leg parked         render leg holds no device buffer (D14.7 invariant)
    episode unsettled  no terminal Fact committed (stop/failure wins first)

Owner:  Playback Session (semantic role) — mechanism components supply
        evidence; the session evaluates and records the commit, then
        routes release+basis to the render leg (D14.8 rebase point).
Before commit: old PCM may legitimately be heard (device tail draining).
After commit:  old PCM is impossible — device queue empty, edge empty,
               staging discarded, worker post-reposition, leg parked, and
               the only producer's program order excludes any later old
               write (E2 + formal model + negative controls).
```

Truth class: protocol state owned by the session. NOT a Fact, NOT a new
terminal variant, NOT public positive state; the observable consequence
is the Position jump and the absence of stale audio. (Subtraction
preferred — §21 of the gate charter honored.)

## 8. Position: rebase rule and the F4 amendment

Frozen rule (proposed for D14.8's seek paragraph):

```text
post_cut_position = retained_source_origin + post_cut_device_consumed_frames

retained_source_origin   the decoder's reported landing converted to
                         source PCM frames (rounded; ±1 frame µs
                         quantization is mechanism noise, not a lie);
                         landing == −1 (unknown) → NO basis exists: the
                         projection is withdrawn (observation derives
                         None — unknown stays unknown, never zero, never
                         the requested target)
post_cut_device_consumed the render leg's own handed-off accounting
                         RESET at the commit point (plain writer-local),
                         so no pre/post totals are ever mixed (D14.8
                         no-mixing constraint, now mechanized)
```

Same cell, writer-side rebase — the core F5 architecture decision on the
Position surface:

```text
same cell rebase (SELECTED)   exactly ONE writer exists (the render leg);
                              the rebase is a plain store on that same
                              execution path at commit release (basis=L,
                              local accounting reset), followed by the
                              existing monotone max-publication. No
                              concurrent writer exists to race; within
                              one epoch the cell stays monotone.
new cell per cutover (REJECT) live old/new cell replacement = reader-
                              visible pointer swap = P1–P5 trigger; and
                              it is secretly a Generation mechanism.
```

F4 monotonicity amendment (required, explicit): Position is monotone
**between committed discontinuities**. A committed cutover may move the
published sample backward (to the landing); that backward step IS the
discontinuity, it happens exactly once per commit, on the writer's path,
and never mixes evidence across the boundary. Everything not after a
committed discontinuity keeps the frozen F4 rules (never backward, never
above handed-off, never fabricated, undefined after terminal).

Pre-cut behavior: during the park+drain the leg keeps publishing from
its park slices (the sample rises toward the frozen pre-seek total —
truthful: the device is still consuming old frames). The rebase store
then moves it to L. A reader can therefore observe a backward step
exactly at the commit — authorized by the amended scope, and the only
truthful option (blocking the rise or jumping at command time would
fabricate).

## 9. Target semantics (frozen)

```text
unit            source-relative media time, microseconds (ABI-native;
                no device units, no percentages)
target zero     source start
negative        structurally unrepresentable in the proposed API
                (Duration); the ABI also rejects (< 0 → INVALID_ARGUMENT
                in 140 ns measured) — belt and suspenders
beyond duration NOT rejected by the session (Duration is optional
                evidence, NEVER semantic authority — M adversarial item);
                the decoder/provider decides (ABI clamps when duration
                known; lands at last block — measured); unknown-duration
                sources remain seekable if the decoder supports them
exact EOF       a legal seek: landing at the end → natural EOF →
                normal drain → Completed (T17/T18; no special case)
duration role   UI may bound requests for convenience; semantic
                correctness never consults it
```

## 10. Pause × Seek (decision A — pause intent survives seek)

Frozen: **pause intent survives seek; seek never implicitly resumes and
never rejects because of pause.** Mechanism support is clean:

```text
paused episode: leg parked at the gate, tail already quiesced
    (padding==0 — the output-side cut precondition is already true)
seek: worker cut + edge invalidate proceed with the leg parked;
    commit does not require any pause-state change; the leg stays
    parked (pause routing untouched); Position rebases to L (truthful:
    the stream now sits at L, zero post-cut consumed); paused()
    evaluates exactly as before the seek.
playing episode: the seek parks the leg internally (it must — the cut
    needs the leg out of GetBuffer and the tail drained). This internal
    quiescence is NOT a pause: it never routes pause_requested, never
    emits pause engagement evidence, and never satisfies paused()
    (which requires pause intent AND engagement AND tail quiescence AND
    unsettled). The mechanism park is reused physically; the truth
    classes stay separated by attribution: pause_engagement evidence
    counts only pause-attributed engagements; a seek park is attributed
    to the cut protocol and is invisible in the public observation
    (no new observation field — §34 bias).
```

This freezes #119's preferred hypothesis ("pause Command state should
not be silently rewritten by another command") and answers its semantic
gate item: seek rejected in the "edge Eof but D11 not Completed" window
(§11), never rejected while merely paused.

## 11. Stop × Seek and terminal precedence (D11 untouched)

```text
committed terminal Fact      seek is inert command history (same family
                             as late stop/pause). It cannot relabel any
                             outcome (D11 late-command rule).
stop linearizes before the   stop wins: edge terminal → Stopped makes
seek's cut                   the acceptance check (data plane Open) fail;
                             an in-flight cut aborts (worker re-checks the
                             edge terminal at its serialization point and
                             after song_seek; the session never commits
                             once stop intent is recorded); D11 settles
                             Stopped per the existing precedence.
seek's cut in progress when  stop still wins: stop releases the gate and
stop arrives                 stops the edge; the commit conditions can no
                             longer be satisfied (edge not clean / leg
                             released); the protocol aborts; no commit,
                             no rebase, no partial state.
seek cannot block stop       all seek steps are bounded (song_seek measured
                             ≤ 310 µs; drain ≤ one buffer; edge invalidate
                             O(1)) and every step re-checks the terminal.
EOF window                   edge Eof is a terminal (monotone, first-wins)
                             → seek rejected while D11 not yet Completed.
                             This freezes #119's recommended acceptance
                             boundary: "data plane still Open" is the only
                             acceptable window; the drain window after EOF
                             is explicitly not seekable in v1. (Reopening
                             an Eof edge or drain-window seeking is a real
                             design door — left closed.)
```

## 12. Multiple seeks (smallest truthful policy)

Frozen: **one seek in flight; a second request before the current cut
commits or aborts is rejected (inert command, no semantic effect).**
After commit/abort, a new seek is accepted; T13 back-to-back = two
sequential full protocol instances (decoder-level evidence in E1).
Rationale: rejection is the only policy whose stale-exclusion argument
needs no request identity — and that is exactly why no SeekId/Epoch
becomes necessary (§15). Coalescing/latest-wins would each need an
identity story; none is earned by any current product need (no arrow-key
seek exists yet).

## 13. Seek failure policy (pre-cut vs post-cut)

```text
pre-cut failures (episode unchanged, playback continues from the old
    position; diagnostic-class, NOT terminal):
    - seek while one is in flight (§12 rejection)
    - data plane not Open (edge terminal ≠ Open), episode settled,
      stop intent already recorded
    - SEEK_UNSUPPORTED / SEEK_ERROR / INVALID_ARGUMENT reported by
      song_seek — the edge and output were never touched, the failure
      classification is command-level; the decoder remains usable
      (measured for the validation class in E1)
post-cut: the selected mechanism makes the destructive boundary
    (edge invalidate + output drain) start only after song_seek has
    already succeeded, so the "decoder moved but edge flush failed"
    window is structurally excluded (the flush is an O(1) lock-held
    reset that cannot fail mid-way; the drain cannot fail into commit —
    it just keeps waiting or the terminal wins). A device failure
    DURING the drain settles the episode through the existing D11
    precedence (device failure class), exactly like any other render
    abort — seek introduces no new terminal outcome.
    LANDING UNKNOWN (−1): the cutover still commits (stale exclusion is
    independent of landing knowledge; the decoder has already moved —
    rollback does not exist), and Position is withdrawn (None) for the
    rest of the episode rather than fabricated (§8). Not expected on
    the current corpus (E1 never observed −1 on a successful seek);
    fail-closed honesty if it ever appears.
```

No AUTHORITY_GAP remains here: the pre/post distinction is real but the
selected mechanism confines post-cut failure to the existing device-
failure path; nothing new had to be invented.

## 14. P1–P5 check (verdict: NOT triggered — recorded, not named)

```text
Does an RT reader see a pointer/resource replaced live?   NO — same edge,
    same render stream, same device session, same position cell, same
    worker/render threads throughout the cut. The rebase is a value
    store by the existing single writer, not a view publication.
Can old and new position/edge/render resources overlap?   NO — the
    protocol parks both legs at their serialization points; the edge is
    purged in place; the cell is never swapped.
Who retires the old world?    There is no old world — same-resource
    discontinuity (Issue #119 REV.3's earned category), the direct
    descendant of the D14.7 park (which established the same verdict
    for pause).
What proves no old reader remains?    The parked leg holds no buffer
    across the park (D14.7 frozen invariant); the parked producer is
    outside write by its serialization-point discipline (E2 + formal
    negative controls).
```

If a future feature replaces any of these resources live (option B/C,
gapless, device switch), THAT enters PBK-001 §6 + P1–P5 — not this one.

## 15. Abstraction razor review (§40) — every candidate noun REJECTED

```text
Generation / Epoch / SeekId / DiscontinuityId
    What race does it solve?  Stale-work discrimination — solved without
    it: at most one seek in flight (frozen policy), both legs parked at
    serialization points, single writer program order (E2 negative
    control shows the discipline, not a token, carries the guarantee).
    Why does ownership + local barrier not suffice?  It does — that IS
    the selected mechanism.
    Runtime cost?  A versioned token would tax every block or every
    write forever (the exact permanent tax §28 forbids).
    REJECT.
TimelineSegment
    Position post-cut = retained_origin + post_cut_consumed — a two-term
    writer-local sum, no segment object. REJECT.
SeekState (public)
    No consumer needs an in-flight seek state; the TUI observes the
    Position jump. Command state (one-in-flight flag) is ordinary
    session state, not an architecture noun. REJECT as public surface.
SeekOperation as Plugin
    D13 negative oracle #1: the Playback Session can own it without
    losing composition correctness — REJECT (already recorded in #119).
```

## 16. Realtime cost (frozen shape)

```text
normal playback   ZERO new per-quantum work. The worker's loop-top
                  command check is one Mutex try on a session-owned slot
                  per staging block (decode-side, off the RT path). The
                  render loop is untouched except that the loop-top park
                  condition gains one more session-owned flag next to
                  the existing pause flag (same lock, same check site —
                  not per-block versioning).
during seek       bounded control work OFF the quantum path: song_seek
                  (≤ 310 µs measured), edge invalidate (O(1)), drain
                  wait (≤ one buffer, 31.2 ms measured), one rebase
                  store. No allocation, no dispatch, no K0 visibility.
```

## 17. Formal model (specs/f5-seek-discontinuity — BOUNDED-CLEAN)

Small TLA+ model of the discontinuity protocol only (boolean-abstract
reservoirs; no audio). `specs/check.sh f5`:

```text
base     PASS — InvStaleOutput + InvPositionNoMixing + InvCommitPurged
         over 300 distinct states (exhaustive at MAX_TAIL=2)
witness  3 × MUST-FAIL-OK (commit reachable; pre-commit old output
         reachable = legal; pre-commit edge-old reachable = reservoir
         non-vacuous)
mutation 5 × COUNTEREXAMPLE-WITNESSED (each guard proven load-bearing):
         M1 commit-before-tail-purge, M2 cut-mid-write (staging not
         discarded), M3 park-while-held (render-held block), M4 stale
         position writer, M5 commit-before-landing
```

This is exactly the mutation set §31 demanded (commit before purge; old
producer write after edge reset; old submission after commit — realized
as the held-block mutation; stale Position writer). Results class:
CHECKED-IN-MODEL; it proves the protocol's guard set is sufficient
within the abstraction, not production correctness.

## 18. Public API proposal (propose, do not implement)

```text
PlaybackSessionHandle::request_seek(&self, target: Duration)
    Command only, infallible, idempotent-with-rejection semantics:
    invalid moments (in-flight seek, non-Open data plane, settled
    episode, stop already recorded, decoder refusal) are silently
    inert like late stop/pause — the command records intent when
    accepted; outcomes stay observable only through truthful evidence
    (Position jump) and terminal truth. Duration is non-negative and
    source-relative; beyond-duration passes through to the provider.
    No SeekManager, no SeekSession, no transaction, no new observation
    field, no public positive state (§34 subtraction bias). A future
    CLI that needs rejection diagnostics gets them as command-history
    diagnostics if a narrow authority decision ever earns them.
```

## 19. Decision table (§39, complete)

| Concern | Selected rule | Evidence | Rejected alternatives |
|---|---|---|---|
| Seek target | source-relative media time (µs); zero=start; negative unrepresentable; provider decides validity; Duration never consulted | ABI native unit; E1 clamp/EOF behavior | percent-of-duration (UI convenience — invented semantics); PCM frame index (extra conversion layer for zero product gain; ABI speaks µs) |
| Decoder landing | actual reported landing (`out_actual_position_us`) is the retained-PCM start; −1 = unknown → Position withdrawn, never requested-target | E1 (landing honest ±1 frame lossless; MP3 3-frame content tolerance) | requested target as basis (the §26 lie — measured to differ by up to ~648 frames ≈ 15 ms) |
| Edge invalidation | non-terminal invalidate primitive + TWO-PHASE protocol (session phase-1 unblocks; worker phase-2 at its serialization point is the load-bearing cut) | E2 (255×3 clean; 51/51 rogue control) | `drain old edge naturally` (unbounded latency, doesn't stop old production); edge replacement (P1–P5); flush-cures-everything (the negative control disproves it) |
| Output cut | park + natural drain to padding==0 (mechanism A; same D14.7 evidence class, zero stream-state changes) | E3 A (31.2 ms; position continuous; refill normal) | Stop/Reset/Start (E3 B: freezes mid-buffer, resets device position; unnecessary); stream replacement (heavier lifecycle); pause gate alone (never removes queued PCM — §47) |
| Cutover commit | session-owned CommitCut = landing ∧ edge-clean ∧ tail-quiesced ∧ leg-parked ∧ unsettled | §7; formal model guards | decoder-repositioned alone (decoder-only fallacy); first-new-submission (too late — drain already proves safety) |
| Position rebase | same cell, writer-side: basis = landing frames; local handed-off reset at commit; publication stays monotone per epoch; unknown landing → withdraw | §8; formal M4 | new cell per cutover (P1–P5 + secret Generation); reader-side clamp (forbidden by D14.8); command-time jump (fabrication) |
| Pause interaction | A: pause intent survives seek; internal seek park is invisible to pause evidence | §10 | implicit resume (rewrites another command's state); reject-while-paused (no mechanism reason) |
| Terminal precedence | committed terminal always wins; stop-before-cut aborts the cut; late seek inert; EOF window not seekable (data-plane-Open-only acceptance) | §11 | seek as second terminal authority; drain-window seeking (edge lifecycle reopening — a closed design door) |
| Multiple seeks | one in flight; second rejected until commit/abort | §12 | coalescing / latest-wins (each needs a request-identity story → would earn SeekId; nothing needs it) |
| Seek failure | pre-cut = inert diagnostic, playback continues; post-cut structurally confined to existing device-failure path; unknown-landing commits with Position withdrawn | §13, E1 | seek failure ⇒ terminal Failed (forged terminal); invented recovery outcomes |

## 20. Environment and limitations

```text
E1  Linux host, static libsongcore (ABI v1), committed corpus (4
    fixtures, all seekable). SEEK_UNSUPPORTED and SEEK_ERROR post-
    states NOT exercisable on this corpus — their policy is ABI-contract
    reasoning, not measured. MP3 content exactness only after 3 codec
    frames (lossy tolerance, documented by the ABI).
E2  mechanism probe (protocol shape copy), not the production edge;
    loom exploration of the real edge + invalidate belongs to the F5
    implementation gate. No scheduling guarantee beyond bounded waits.
E3  one Windows endpoint (WSL2 host, shared mode, AUTOCONVERTPCM leg);
    3 green runs, raw logs retained. Software padding/position readings
    are not acoustic proof — but the selected mechanism's audible claim
    rests on D14.7's frozen padding semantics (physically evidenced by
    f3probe), not on new acoustic measurement; the implementation gate
    still owes its own physical production smoke.
Formal  boolean abstraction; safety-only; bounded MAX_TAIL=2.
```

## 21. Fresh-context adversarial review (attacks A–N)

Recorded in §22 below after a fresh reviewer pass; findings and their
resolutions are listed there with file:line evidence.
