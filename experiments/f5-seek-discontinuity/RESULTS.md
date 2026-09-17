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

The gate task (work charter "QIANQIAN-F5-SEEK-DISCONTINUITY-GATE-1")
demanded, among the deliverables recorded here: the stale-PCM location
map (§3), the decision table (§19), the abstraction-razor review
(§15), the adversarial execution reasoning (§21–§22 of this report),
the minimal formal model (§17), and the STOP before implementation
(§14/§18). Charter section references below are to that task document.

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
  included "seek product-state vocabulary / actual-landing authority
  beyond D14 minimum" — addressed by this gate (§8); the ADR records
  the closure.
- **PBK-001 §6 P1–P5**: triggered only by live old/new RT resource
  overlap. Issue #119 REV.3 already records the verdict direction:
  same-resource protocol defaults to NO P1–P5; only resource
  replacement would enter it.
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
not a violation). After commit it must be impossible, not unlikely —
with the same device-consumed boundary D14.7/D14.8 freeze: the claim
covers this stream's queued-to-play set, never the acoustic instant.

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
| decoder internal (native packets/codec state) | YES until `song_seek` (which flushes per ABI) | Decode provider (SongCore) | `song_seek` only path | 12 µs–0.4 ms measured (E1) | off-RT (worker thread) |
| decode worker staging (1024 frames) | YES — freshly decoded, not yet written | worker local | serialization-point discipline: discard at the seek (the buffer itself is reused) | zero | off-RT |
| PcmEdge ring (8192 frames) | YES — up to full capacity | Session (episode-owned) | new non-terminal invalidate under the edge lock, performed by the worker itself strictly after song_seek succeeded, with the leg parked | O(1) cursor reset | consumer parked — none |
| render-held device buffer memory (GetBuffer..ReleaseBuffer window, incl. the one block a leg can hold in flight) | YES — one iteration's fill | Output provider (render leg) | park at loop top: leg never holds a buffer across a park (D14.7 frozen invariant) | zero | park = none |
| WASAPI queued padding | YES — up to one device buffer (measured 984 frames ≈ 22 ms; full-buffer observed at engage in E3) | device stream | **natural drain to padding==0** (mechanism A, selected); Stop/Reset measured and rejected (§6) | ≤ one buffer duration (29.9–31.7 ms measured) | parked leg, bounded drain wait |
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
clamp beyond duration           SONG_OK, lands at the last container block;
                                EOF follows naturally. Tail counts after a
                                near-end seek: consistent with the landing
                                remainder for lossless; for MP3 the
                                post-seek decode emitted ~1105 frames MORE
                                than the landing-implied remainder (lossy
                                decoder-delay artifact; recorded as
                                advisory — the probe prints the check but
                                does not gate on it for lossy)
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
seek latency                    measured across all runs: MP3 ≤ 58 µs,
                                FLAC/ALAC ≤ 379 µs (floor ~12 µs) —
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
terminal untouched; both condvars notified), driven through the FROZEN
protocol of the D14.5 amendment. 3 runs × 256 scenarios:

```text
deterministic / backward-seek / landing-zero   clean (no stale output)
blocked-producer (T4/T5: producer blocked      clean — the bounded-slice
inside its write on a full edge at command     write observes the command
time; the seek proceeds with no pre-purge)     and abandons the in-flight
                                               block; the seek runs with
                                               no pre-purge
seek-refused (song_seek rejection path)        protocol stayed inert: no
                                               purge, no landing, no commit;
                                               production continued to EOF;
                                               zero post-command loss beyond
                                               the bounded staging block
randomized ×200 (sizes, landings, jitter)      clean
rogue-staging negative control ×51             FIRED 51/51 (a stale block
                                               entering the edge AFTER the
                                               commit is always witnessed)
```

**Why the exclusion is the discipline, not the primitive.** The worker
is the only producer; the frozen protocol has it perform the ONE purge
itself, strictly after song_seek succeeded, on its own execution path,
with the render leg parked. Program order then guarantees no old write
after the purge. The negative control proves the discipline is
load-bearing: a worker that skips the staging discard and writes its
in-flight pre-cut block after the purge always produces stale output.
(E2 run-round 1 also caught a protocol draft bug — an early production
hold could strand the leg inside a blocked read on an emptied edge and
stall the protocol; the frozen ordering now keeps production flowing
until the leg's parked evidence arrives, and the harness pins that. The
harness's commit is additionally gated on the leg's park acknowledgment,
mirroring the real commit precondition.)

The flush never touches the terminal (flush ≠ Eof/Failed/Stopped;
first-wins terminal semantics intact), so the acceptance rule "data
plane must be Open" survives. Linearization: the invalidate is an
ordinary lock-held reset; it needs no new lifecycle concept because the
protocol — not the primitive — guarantees no endpoint is inside
read/write across the purge (leg parked at its gate, producer on its own
serialization path). Loom coverage of the real edge lands with the F5
implementation gate (loom_edge_tests already explore the unmodified edge
shape).

## 6. E3 — output-side physical cut (evidence/f5cut-run{1,2,3}.log)

Windows host (WSL2 → Windows staging), shared-mode event-driven, 44.1 kHz
stereo float32, AUTOCONVERTPCM leg (the endpoint refuses the exact
format — same finding as f4probe), buffer 984 frames ≈ 22 ms. The probe
was amended once (review finding m-5: the superseded verdict line
compared raw device-position units against frame counts — the unit is
endpoint-specific) and the amended probe was then reproduced on the
host: the three green runs below carry the unit-converted verdict
(`clock_freq=352800` at this endpoint — byte-class, confirming the
D14.8 GetFrequency finding). An endpoint outage (0x80070490, default
render endpoint momentarily absent) interrupted the session for ~7
minutes and recovered; the interrupted attempts are not evidence and
are not retained.

**Experiment A — park + natural drain (the selected mechanism):**

```text
padding at engage            984 (a full device buffer of old audio)
drain observations           984 → 543 → 102 → 0 (2 ms poll)
drain latency                30.2 / 31.9 / 31.9 ms across the three runs
                             (consistent with D14.7's physically measured
                             28–30 ms)
device position              advanced through the old tail while the leg
                             was parked, measured in converted units:
                             11264 units ≥ 7872 units (= 984 frames × 8
                             bytes/frame at this endpoint) — the device
                             physically consumed the queued tail;
                             GetPosition never reset
after commit (padding==0)    refill with the new signal: GetBuffer/
                             ReleaseBuffer normal, padding grows and
                             drains normally
```

The correctness reading is the one D14.7 already froze and f3probe
already evidenced physically: padding is exactly this stream's
queued-to-play frames, so a zero observation after engagement proves
nothing submitted before engagement remains queued-to-play. Mechanism A
adds **no removal mechanism at all** — it waits for consumption; after
commit only new frames exist to submit. The probe evidences that the
composition behaves as that reading requires on real hardware and
quantifies the cut latency; the audible-cutover semantics (post-commit
old PCM of this stream cannot be rendered) rests on the frozen D14.7
padding semantics, with the device-consumed boundary of §2 — it makes no
acoustic-instant claim. `PHYSICAL_PRODUCTION_SMOKE` for the eventual F5
implementation remains NOT-RUN (that gate will run its own), and the
amended verdict line is recorded as NOT-REPRODUCED (endpoint
unavailable).

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
latency win (~0 ms vs ~30 ms) that is inaudible in context, carries the
frozen-mid-buffer objection, resets device position, and would need its
own cross-endpoint physical campaign. Re-earnable only by a new narrow
authority decision (same posture as D14.7's mechanism B).

## 7. Cutover commit point (the central deliverable)

Selected commit boundary — the **earliest point that is actually safe**:

```text
CommitCut holds when ALL of:
    landing            worker published "decoder repositioned at L"
                       (strictly after its purge, which strictly follows
                       song_seek success)
    edge clean         edge invalidated (by the worker itself)
    tail quiesced      padding == 0 observed while the leg is parked
                       (D14.7 evidence class)
    leg parked         render leg holds no device buffer (D14.7 invariant;
                       the commit is additionally gated on the leg's park
                       acknowledgment, not on the session's hope)
    episode unsettled  no terminal Fact committed (stop/failure wins first)

Owner:  Playback Session (semantic role) — mechanism components supply
        evidence; the session evaluates and records the commit, then
        routes release+basis to the render leg (D14.8 rebase point).
Before commit: old PCM may legitimately be heard (device tail draining).
After commit:  old PCM of this stream is impossible as queued-to-play —
               device queue empty, edge empty, staging discarded, worker
               post-reposition and production-held, leg parked, and the
               only producer's program order excludes any later old write
               (E2 + formal model + negative controls).
```

Truth class: protocol state owned by the session. NOT a Fact, NOT a new
terminal variant, NOT public positive state; the observable consequence
is the Position jump and the absence of stale audio (subtraction
preferred, per the gate charter §21).

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
post_cut_device_consumed the render leg's own handed-off accounting,
                         RESET at the commit point (plain writer-local),
                         so no pre/post totals are ever mixed (D14.8
                         no-mixing constraint, now mechanized). The
                         protocol's production hold between landing and
                         release makes the basis EXACT: every pre-commit
                         submission is pre-landing, so nothing post-
                         landing can hide in the reset.
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
                              one published stretch the cell stays
                              monotone.
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
                in ~140 ns measured) — belt and suspenders
beyond duration NOT rejected by the session (Duration is optional
                evidence, NEVER semantic authority — the gate charter's
                "Duration inflation" attack); the decoder/provider
                decides (ABI clamps when duration known; lands at last
                block — measured); unknown-duration sources remain
                seekable if the decoder supports them
exact EOF       a legal seek: landing at the end → natural EOF →
                normal drain → Completed (charter T17/T18; no special
                case)
duration role   UI may bound requests for convenience; semantic
                correctness never consults it
```

## 10. Pause × Seek (decision A — pause intent survives seek)

Frozen: **pause intent survives seek; seek never implicitly resumes and
never rejects because of pause.** Mechanism support is clean:

```text
paused episode: leg parked at the gate, tail already quiesced
    (padding==0 — the output-cut precondition is already true)
seek: worker seek + purge proceed with the leg parked; commit does not
    require any pause-state change; the leg stays parked (pause routing
    untouched); Position rebases to L (truthful: the stream now sits at
    L, zero post-cut consumed); paused() evaluates exactly as before.
playing episode: the session parks the leg for the cut (it must — the
    commit needs the leg parked and the tail quiesced). This internal
    quiescence is NOT a pause: it never routes pause_requested, never
    publishes pause engagement evidence, and never satisfies paused()
    (which requires pause intent AND engagement AND tail quiescence AND
    unsettled). The mechanism park is reused physically; the truth
    classes stay separated by attribution: pause_engagement evidence
    counts only pause-attributed engagements; a seek park is attributed
    to the cut protocol and is invisible in the public observation
    (no new observation field — the charter's §34 subtraction bias).
transient: pause intent arriving WHILE a playing episode's cut is in
    flight routes normally (the seek park is cut-attributed); the leg is
    already parked, and no pause-attributed engagement exists until a
    post-release re-park — the frozen attribution rule determines the
    outcome uniquely; no third behavior is needed.
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
                             an in-flight cut aborts (the worker's seeks
                             wait and production paths are terminal-
                             aware; the session never commits once stop
                             intent is recorded); D11 settles per the
                             existing precedence.
seek's cut in progress when  stop still wins: stop releases the gate and
stop arrives                 stops the edge; the commit conditions can no
                             longer be satisfied (unsettled check);
                             protocol aborts; no commit, no rebase, no
                             partial state.
seek cannot block stop       all seek steps are bounded (song_seek ≤
                             ~0.4 ms measured; purge O(1); drain ≤ one
                             buffer; every wait terminal-aware) and
                             every step re-checks the terminal.
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
After commit/abort, a new seek is accepted; back-to-back seeks are two
sequential full protocol instances (decoder-level evidence in E1).
Rationale: rejection is the only policy whose stale-exclusion argument
needs no request identity — and that is exactly why no SeekId/Epoch
becomes necessary (§15). Coalescing/latest-wins would each need an
identity story; none is earned by any current product need (no arrow-key
seek exists yet).

## 13. Seek failure policy (pre-cut vs post-cut)

```text
pre-cut failures (the episode continues its pre-command content; NEVER
    terminal Failed):
    - seek already in flight (§12 rejection)
    - data plane not Open (edge terminal ≠ Open, which includes the
      post-EOF drain window), episode settled, stop intent recorded
    - song_seek refusal (SEEK_UNSUPPORTED / SEEK_ERROR /
      INVALID_ARGUMENT): under the frozen ordering the refusal happens
      BEFORE any invalidation, so edge, tail and leg continue
      seamlessly; the only content a refusal can cost is one abandoned
      in-flight staging block (≤ one staging buffer, present only when
      the worker was mid-block). E1 measured the validation class
      (decoder usable after rejection); SEEK_ERROR/UNSUPPORTED post-
      state remains unexercised on this corpus (recorded limitation).
post-cut: the selected mechanism confines failure to the existing D11
    device-failure path. The purge is a fail-fast O(1) reset that
    happens only after song_seek succeeded; a device failure during the
    drain settles through the existing precedence exactly like any other
    render abort. Seek introduces no new terminal variant and no
    recovery semantics.
    LANDING UNKNOWN (−1): the cutover still commits (stale exclusion is
    independent of landing knowledge; the decoder has already moved —
    rollback does not exist), and Position is withdrawn (None) for the
    rest of the episode rather than fabricated (§8). Not observed on
    the current corpus (E1 never saw −1 on a successful seek);
    fail-closed honesty if it ever appears.
```

No AUTHORITY_GAP remains here: the pre/post distinction is real but the
frozen ordering confines post-cut failure to the existing device-failure
path; nothing new had to be invented.

## 14. P1–P5 check (verdict: NOT triggered — recorded, not named)

```text
Does an RT reader see a pointer/resource replaced live?   NO — same edge,
    same render stream, same device session, same position cell, same
    worker/render threads throughout the cut. The rebase is a value
    store by the existing single writer, not a view publication.
Can old and new position/edge/render resources overlap?   NO — the
    protocol parks the leg and holds production; the edge is purged in
    place; the cell is never swapped.
Who retires the old world?    There is no old world — same-resource
    discontinuity (Issue #119 REV.3's earned category), the direct
    descendant of the D14.7 park (which established the same verdict
    for pause).
What proves no old reader remains?    The parked leg holds no buffer
    across the park (D14.7 frozen invariant); the producer is outside
    write at the purge by its serialization-point discipline (E2 +
    formal negative controls).
```

If a future feature replaces any of these resources live (resource
replacement, gapless, device switch), THAT enters PBK-001 §6 + P1–P5 —
not this one.

## 15. Abstraction razor review (charter §40) — every candidate noun REJECTED

```text
Generation / Epoch / SeekId / DiscontinuityId
    What race does it solve?  Stale-work discrimination — solved without
    it: at most one seek in flight (frozen policy), the leg parked and
    the producer on its serialization path, single-writer program order
    (E2's negative control shows the discipline, not a token, carries
    the guarantee).
    Why does ownership + local barrier not suffice?  It does — that IS
    the selected mechanism.
    Runtime cost?  A versioned token would tax every block or every
    write forever (the exact permanent tax the charter §28 forbids).
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
                  (≤ ~0.4 ms measured), purge (O(1)), drain wait (≤ one
                  buffer, ~30 ms measured), one rebase store. No
                  allocation, no dispatch, no K0 visibility.
```

## 17. Formal model (specs/f5-seek-discontinuity — BOUNDED-CLEAN)

Small TLA+ model of the discontinuity protocol only (boolean-abstract
reservoirs; no audio; no timing). `specs/check.sh f5`:

```text
base     PASS — InvStaleOutput + InvPositionNoMixing + InvCommitPurged
         over 426 distinct states (exhaustive at MAX_TAIL=2)
witness  4 × MUST-FAIL-OK (commit reachable; pre-commit old output
         reachable = legal; pre-commit edge-old reachable = reservoir
         non-vacuous; seek-refusal path reachable = the frozen protocol's
         failure half is modeled, not decorative)
mutation 5 × COUNTEREXAMPLE-WITNESSED:
         M1 commit-before-tail-purge, M2 seek-mid-write (staging not
         discarded), M3 park-while-held (render-held block), M4 stale
         position writer, M5 commit-before-landing
```

Honesty note (round-1 review, m-6): M1/M2/M3/M5 relax real base-model
guards and are genuine counterexample witnesses. M4 instead INJECTS a
defective writer: `InvPositionNoMixing`'s base-model guarantee comes
from the construction — a single writer rebasing on its own path —
which the model asserts rather than derives; M4 only proves the injected
defect is reachable and detectable. Results class: CHECKED-IN-MODEL; it
proves the protocol's guard set is sufficient within the abstraction,
not production correctness.

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
    field, no public positive state (the charter's §34 subtraction
    bias). A future CLI that needs rejection diagnostics gets them as
    command-history diagnostics if a narrow authority decision ever
    earns them.
```

## 19. Decision table (charter §39, complete)

| Concern | Selected rule | Evidence | Rejected alternatives |
|---|---|---|---|
| Seek target | source-relative media time (µs); zero=start; negative unrepresentable; provider decides validity; Duration never consulted | ABI native unit; E1 clamp/EOF behavior | percent-of-duration (UI convenience — invented semantics); PCM frame index (extra conversion layer for zero product gain; ABI speaks µs) |
| Decoder landing | actual reported landing (`out_actual_position_us`) is the retained-PCM start; −1 = unknown → Position withdrawn, never requested-target | E1 (landing honest ±1 frame lossless; MP3 3-frame content tolerance) | requested target as basis (the charter §26 lie — measured to differ by up to ~648 frames ≈ 15 ms) |
| Edge invalidation | non-terminal invalidate primitive performed BY THE WORKER strictly after song_seek success, with the leg's parked evidence in hand; production keeps flowing until the parked evidence (no strand-inside-read stall); bounded-slice write so the serialization point is always reachable | E2 (256×3 clean; 51/51 rogue control; refusal scenario) | `drain old edge naturally` (unbounded latency, doesn't stop old production); edge replacement (P1–P5); session-side pre-purge (destructive-before-outcome — the round-1 MAJOR, removed); flush-cures-everything (the negative control disproves it) |
| Output cut | park + natural drain to padding==0 (mechanism A; same D14.7 evidence class, zero stream-state changes) | E3 A (29.9–31.7 ms; position continuous; refill normal) | Stop/Reset/Start (E3 B: freezes mid-buffer, resets device position; unnecessary); stream replacement (heavier lifecycle); pause gate alone (never removes queued PCM) |
| Cutover commit | session-owned CommitCut = landing ∧ edge-clean ∧ tail-quiesced ∧ leg-parked(acknowledged) ∧ unsettled | §7; formal model guards | decoder-repositioned alone (decoder-only fallacy); first-new-submission (too late — drain already proves safety) |
| Position rebase | same cell, writer-side: basis = landing frames; local handed-off reset at commit; publication monotone per published stretch; unknown landing → withdraw | §8; formal M4 note | new cell per cutover (P1–P5 + secret Generation); reader-side clamp (forbidden by D14.8); command-time jump (fabrication) |
| Pause interaction | A: pause intent survives seek; internal seek park is cut-attributed, invisible to pause evidence; transient pause-during-cut routes normally | §10 | implicit resume (rewrites another command's state); reject-while-paused (no mechanism reason) |
| Terminal precedence | committed terminal always wins; stop-before-cut aborts the cut; late seek inert; EOF window not seekable (data-plane-Open-only acceptance) | §11 | seek as second terminal authority; drain-window seeking (edge lifecycle reopening — a closed design door) |
| Multiple seeks | one in flight; second rejected until commit/abort | §12 | coalescing / latest-wins (each needs a request-identity story → would earn SeekId; nothing needs it) |
| Seek failure | pre-cut = inert (refusal BEFORE any invalidation; only cost ≤ 1 staging block mid-block); post-cut confined to existing device-failure path; unknown-landing commits with Position withdrawn | §13, E1 | seek failure ⇒ terminal Failed (forged terminal); invented recovery outcomes |

## 20. Environment and limitations

```text
E1  Linux host, static libsongcore (ABI v1), committed corpus (4
    fixtures, all seekable). SEEK_UNSUPPORTED and SEEK_ERROR post-
    states NOT exercisable on this corpus — their policy is ABI-contract
    reasoning, not measured. MP3 content exactness only after 3 codec
    frames (lossy tolerance, documented by the ABI). eof-tail bound is
    advisory for lossy (decoder-delay artifact recorded).
E2  mechanism probe (protocol shape copy), not the production edge;
    loom exploration of the real edge + invalidate belongs to the F5
    implementation gate. No scheduling guarantee beyond bounded waits.
E3  one Windows endpoint (WSL2 host, shared mode, AUTOCONVERTPCM leg);
    3 green runs of the unit-converted probe, raw logs retained.
    Software padding/position readings are not acoustic proof — the
    selected mechanism's audible claim rests on D14.7's frozen padding
    semantics (physically evidenced by f3probe), not on new acoustic
    measurement. The implementation gate still owes its own physical
    production smoke.
Formal  boolean abstraction; safety-only; bounded MAX_TAIL=2; the
    position no-mixing base guarantee is constructive (single writer),
    not model-derived (see §17 honesty note).
```

Post-evidence source note: after the recorded runs, all three probes
were reorganized for gate hygiene only — the platform-specific probe
bodies moved into cfg-gated modules (the f4probe pattern) so the
crate builds and clippy-clean for both the Linux host and the
x86_64-pc-windows-gnu target, plus two mechanical lint fixes (an
identity `as f32` cast removed, one function renamed snake_case).
These are code motions with identical experiment logic; they are
verified by two-target compile plus fresh Linux smoke runs of E1/E2
(green), not by new physical E3 runs — the retained E3 logs remain
the runs of the semantically identical pre-reorganization source.

## 21. Adversarial execution reasoning (charter §30, T1–T20)

The charter's twenty scenarios are covered by the frozen policy +
evidence as follows: T1 (steady seek), T2 (paused), T3 (post-resume) —
§10; T4/T5 (full edge / producer blocked) — E2 blocked-producer + the
bounded-slice write requirement; T6 (render has acquired a device
buffer) — D14.7 no-buffer-across-park + formal M3; T7 (non-zero device
padding) — E3 A (drain to zero before commit); T8 (decoder EOF before
Completed) — §11 acceptance boundary (not seekable); T9 (concurrent
stop), T12 (teardown) — §11 (terminal wins; all waits terminal-aware);
T10 (decode failure) — existing D11 decode-failure precedence (seek
adds nothing); T11 (device failure during cut) — §13 post-cut rule;
T13 (back-to-back) — §12 + E1 b2b; T14/T15 (backward/forward) — E2
backward + E1 sweep; T16 (target zero) — E1 fresh-zero + landing-zero
harness scenario; T17 (near EOF) — E1 at-duration; T18 (beyond
Duration) — E1 beyond-duration; T19 (unknown Duration) — §9
(provider decides); T20 (approximate landing) — E1 lossless vs lossy
measurements + §8 basis rule.

## 22. Fresh-context adversarial review (charter §41, attacks A–N)

Round 1 (fresh reviewer, against commits b5a3cf6/ce316c4/2e5d72b):
verdict CHANGES_REQUIRED — 1 MAJOR, 7 MINOR, 5 NIT. Attacks A, B, C,
F, H, J, K, L, M, N came back clean with file:line evidence; every
finding below was fixed on this branch and the affected evidence
regenerated.

```text
M-1 (attack I)  the originally frozen ordering purged the edge BEFORE
    song_seek, making a refusal destructive and contradicting the
    frozen failure policy. FIXED: protocol reordered — song_seek runs
    at the serialization point strictly before any invalidation; the
    worker performs the ONE purge strictly after success; production
    hold only from landing to release. ADR amendment + E2 + formal
    model regenerated (WorkerSeek/SeekRefused split replaces the
    conflation; SessionPurge action removed).
m-1  the report promised a §22 review record that did not exist and
    cited bare charter section numbers. FIXED: this section IS the
    record; citations now name the charter document explicitly.
m-2  D14.10 still listed the seek cutover item as a MUST-STOP. FIXED:
    the item left the list with a dated annotation.
m-3  seek-latency ranges overstated vs raw logs. FIXED: MP3 ≤ 58 µs,
    FLAC/ALAC ≤ 379 µs (floor ~12 µs) now quoted from the logs.
m-4  "tail counts consistent" false for MP3. FIXED: qualified lossless-
    only; the MP3 discrepancy (~1105 frames, lossy decoder delay) is
    recorded as advisory.
m-5  E3 verdict compared raw device units against frame counts. FIXED:
    probe converts through GetFrequency and was REPRODUCED green 3×
    after a transient endpoint outage (see §6, §20).
m-6  InvPositionNoMixing vacuous in the base model; M4 is an injection,
    not a guard-relaxation. FIXED: documented honestly in §17 and in
    the model header.
m-7  "old PCM impossible" lacked the device-consumed boundary. FIXED:
    the amendment (and §2 here) scope the claim to this stream's
    queued-to-play set, never the acoustic instant.
n-1  pause-intent-while-cut-in-flight now stated explicitly (§10).
n-2  "within one epoch" replaced by the defined phrase in the frozen
    amendment text (explanatory vocabulary kept out of authority text).
n-3  f5edge barrier-snapshot comment inverted (snapshot-before-read is
    the LENIENT direction; the gap is closed by the park-acknowledged
    commit). Comment fixed AND the commit precondition added.
n-4  NO-SURVIVOR demoted in the docstring to what the code does
    (diagnostic; the verdict is the stale-output oracle).
n-5  drain latency quoted as a range (29.9–31.7 ms); specs README's
    heldOld mapping widened to the pull→submit window (the load-bearing
    property covers both).
```

Round 2 (fresh reviewer, against the corrected branch): verdict and
record kept with this report at gate close.
