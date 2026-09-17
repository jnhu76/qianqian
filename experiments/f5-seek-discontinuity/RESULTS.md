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
                                before target (measured up to 3816 frames
                                ≈ 87 ms early mid-stream — flac pct-10:
                                requested 17640 → landed 13824 — and up
                                to 10512 frames at the duration clamp;
                                the b2b 88200 → 87552 case is 648)
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
protocol of the D14.5 amendment as corrected by F5-GATE-CORRECTIVE-1
(three-class provider outcome). 3 runs × 294 scenarios:

```text
deterministic / backward-seek / landing-zero   clean (no stale output)
blocked-producer (T4/T5: producer blocked      clean — the bounded-slice
inside its write on a full edge at command     write observes the command
time; the seek proceeds with no pre-purge)     and stops at the written
                                               prefix, preserving the
                                               in-flight block; the seek
                                               runs with no pre-purge
seek-refused-unchanged (INVALID_ARGUMENT-      inert AND content-exact:
class refusal)                                 no purge, no landing, no
                                               commit; the preserved
                                               remainder is finished and
                                               production continues to EOF;
                                               the consumed sequence EQUALS
                                               the no-seek control (0..pre
                                               contiguous, epoch 0) — zero
                                               content loss (REFUSAL-EQUIV)
blocked-producer-refused                       same, from a full-edge
                                               blocked writer: the abandoned
                                               prefix is resumed after the
                                               refusal and the sequence is
                                               still exact
seek-destructive-failure (generic SEEK_ERROR   FAIL-CLOSED: no purge, no
after flush — the ABI's dual-phase reality)    landing, no commit, no
                                               post-seek content; the worker
                                               stops producing and the
                                               episode ends via the failure
                                               route (only a contiguous
                                               pre-seek prefix ever left the
                                               edge)
randomized honest ×200 (sizes, landings,       clean
jitter)
random-refused-unchanged ×30                   REFUSAL-EQUIV exact
random-destructive-failure ×5                  FAIL-CLOSED exact
rogue-staging negative control ×51             FIRED 51/51 (a stale block
                                               entering the edge AFTER the
                                               commit is always witnessed)
drop-remainder must-fire control ×1            FIRED 1/1 (dropping exactly
                                               ONE preserved remainder frame
                                               after a refusal trips
                                               REFUSAL-EQUIV — the
                                               zero-loss oracle is not
                                               vacuous)
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
implementation remains NOT-RUN (that gate will run its own physical
production smoke).

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

## 13. Seek failure policy (three-class provider outcome)

Round-3 review found the v1 two-class policy (success vs "SEEK_* error
= inert refusal") unsound: the SongCore implementation returns generic
`SEEK_ERROR` from TWO different phases, and only one refusal status is
provably pre-mutation. Frozen classification (SongCore source audit,
`native/src/songcore_ffmpeg.c song_seek`):

```text
Phase 0 (pre-av_seek_frame, PROVABLY non-mutating):
    INVALID_ARGUMENT (null handle / negative target), NOT_OPEN —
    pure parameter/state checks; the decoder is untouched. E1
    measured the validation class (decoder usable after rejection).
    => RefusedUnchanged: inert refusal, old playback continues with
       ZERO content loss (the in-flight staging remainder is preserved
       and finished — §5 REFUSAL-EQUIV).
Phase 1 (av_seek_frame failed): SEEK_UNSUPPORTED / SEEK_ERROR —
    FFmpeg does not promise that a failed av_seek_frame leaves the
    demuxer state undisturbed (it may have read/discarded packets).
    Not provably clean; nothing on this corpus exercises the
    post-state (recorded limitation). => conservative: treat as
    MutatedThenFailed.
Phase 2 (av_seek_frame SUCCEEDED, then failure): avcodec_flush_buffers
    + reset_decode_state HAVE run (decoder repositioned — destructively
    mutated, by construction). Landing decode failure returns generic
    SEEK_ERROR; landing conversion failure returns STREAM_CHANGE /
    DECODE_ERROR. A handle-level read afterwards would produce
    NEW-cursor PCM. => MutatedThenFailed.
    The same status code SEEK_ERROR therefore covers two different
    realities (phase 1 and phase 2); a status code alone cannot prove
    inertness.
```

Policy per class:

```text
RefusedUnchanged   inert diagnostic — the episode continues its
                   pre-command content exactly (never terminal Failed).
                   Preserve-and-finish the staging remainder: the
                   refused-seek output equals the no-seek control
                   (E2 REFUSAL-EQUIV; formal InvRefusalContentContinuous;
                   the drop-one-frame mutation is witnessed by BOTH the
                   E2 must-fire control and formal M6).
Applied            the cut protocol (§5–§7); landing unknown (−1): the
                   cutover still commits (stale exclusion is independent
                   of landing knowledge; the decoder has already moved —
                   rollback does not exist) and Position is withdrawn
                   (None) for the rest of the episode rather than
                   fabricated (§8). Not observed on the current corpus
                   (E1 never saw −1 on a successful seek); fail-closed
                   honesty if it ever appears.
MutatedThenFailed  the old decoder continuation is not guaranteed — the
                   episode NEVER resumes old-cursor production and the
                   failure is NOT papered over as a refusal: it routes
                   through the ordinary D11 decode-failure path
                   (failure mechanism evidence → terminal Failed). No
                   new terminal variant, no recovery semantics (formal
                   InvFailClosed; resuming after the failure is
                   witnessed as the pre/post-mixing hazard by M7).
                   Conservative rule: unprovable means destructive.
                   Upgrading a class (e.g. proving SEEK_UNSUPPORTED
                   non-mutating on real unsupported fixtures) requires
                   a narrow SongCore provider-contract corrective with
                   its own evidence — explicitly out of scope here.

pre-cut rejection (before any provider call): seek already in flight
    (§12), data plane not Open (edge terminal ≠ Open, including the
    post-EOF drain window), episode settled, stop intent recorded —
    inert commands, no semantic effect.
post-cut: the selected mechanism confines failure to the existing D11
    device-failure path. The purge is a fail-fast O(1) reset that
    happens only after song_seek succeeded; a device failure during
    the drain settles through the existing precedence exactly like any
    other render abort.
```

No AUTHORITY_GAP remains here: the classification gap the round-3
review surfaced is resolved by the conservative rule above, frozen in
the D14.5 corrective; a stronger transactional SongCore seek contract
is a possible future provider-contract amendment, not a gate
prerequisite.

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
normal playback   no new per-quantum allocation, no new lock
                  acquisition, no dispatch, no K0/Capability work, no
                  version/epoch comparison. The worker's loop-top
                  command check is one Mutex try on a session-owned slot
                  per staging block (decode-side, off the RT path). The
                  render loop is untouched except that its existing
                  loop-top gate check gains one more session-owned
                  seek-park flag test next to the existing pause flag
                  (same lock, same check site — not per-block
                  versioning). Strictly: not ZERO added work — one flag
                  test at an existing check site.
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
         + InvRefusalContentContinuous + InvFailClosed over 672
         distinct states (exhaustive at MAX_TAIL=2)
witness  6 × MUST-FAIL-OK (commit reachable; pre-commit old output
         reachable = legal; pre-commit edge-old reachable = reservoir
         non-vacuous; seek-refusal path reachable; destructive-failure
         route reachable; remainder-outstanding seek reachable = the
         M6 precondition shape)
mutation 7 × COUNTEREXAMPLE-WITNESSED:
         M1 commit-before-tail-purge, M2 seek-mid-write (staging not
         discarded), M3 park-while-held (render-held block), M4 stale
         position writer, M5 commit-before-landing, M6
         refusal-drops-remainder (caught by
         InvRefusalContentContinuous), M7 resume-after-mutated-seek
         (caught by InvFailClosed — the pre/post-mixing hazard of
         resuming old-cursor production after a destructive failure)
```

Corrective-1 note: the model was extended so the refusal split is
actually carried — `SeekRefusedUnchanged` returns a remainder-bearing
worker to `writing_partial` (FinishWriteOld completes it; M6 drops it
and is caught), and `SeekMutatedThenFailed` moves the episode to the
decode-failure route (`episodeFailed`), with M7 proving that resuming
production after it is detectable. The pre-corrective model abstracted
`SeekRefused` as a bare `"seeking" → "idle"` step and could not see
either hazard.

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
    episode, stop already recorded, a RefusedUnchanged decoder outcome)
    are silently
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
| Decoder landing | actual reported landing (`out_actual_position_us`) is the retained-PCM start; −1 = unknown → Position withdrawn, never requested-target | E1 (landing honest ±1 frame lossless; MP3 3-frame content tolerance) | requested target as basis (the charter §26 lie — measured to differ by up to 3816 frames ≈ 87 ms mid-stream, 10512 at the duration clamp) |
| Edge invalidation | non-terminal invalidate primitive performed BY THE WORKER strictly after song_seek success, with the leg's parked evidence in hand; production keeps flowing until the parked evidence (no strand-inside-read stall); bounded-slice write so the serialization point is always reachable | E2 (294×3 clean; 51/51 rogue control; 1/1 drop-remainder must-fire; refused/destructive families) | `drain old edge naturally` (unbounded latency, doesn't stop old production); edge replacement (P1–P5); session-side pre-purge (destructive-before-outcome — the round-1 MAJOR, removed); flush-cures-everything (the negative control disproves it) |
| Output cut | park + natural drain to padding==0 (mechanism A; same D14.7 evidence class, zero stream-state changes) | E3 A (30.2–31.9 ms; position continuous; refill normal) | Stop/Reset/Start (E3 B: freezes mid-buffer, resets device position; unnecessary); stream replacement (heavier lifecycle); pause gate alone (never removes queued PCM) |
| Cutover commit | session-owned CommitCut = landing ∧ edge-clean ∧ tail-quiesced ∧ leg-parked(acknowledged) ∧ unsettled | §7; formal model guards | decoder-repositioned alone (decoder-only fallacy); first-new-submission (too late — drain already proves safety) |
| Position rebase | same cell, writer-side: basis = landing frames; local handed-off reset at commit; publication monotone per published stretch; unknown landing → withdraw | §8; formal M4 note | new cell per cutover (P1–P5 + secret Generation); reader-side clamp (forbidden by D14.8); command-time jump (fabrication) |
| Pause interaction | A: pause intent survives seek; internal seek park is cut-attributed, invisible to pause evidence; transient pause-during-cut routes normally | §10 | implicit resume (rewrites another command's state); reject-while-paused (no mechanism reason) |
| Terminal precedence | committed terminal always wins; stop-before-cut aborts the cut; late seek inert; EOF window not seekable (data-plane-Open-only acceptance) | §11 | seek as second terminal authority; drain-window seeking (edge lifecycle reopening — a closed design door) |
| Multiple seeks | one in flight; second rejected until commit/abort | §12 | coalescing / latest-wins (each needs a request-identity story → would earn SeekId; nothing needs it) |
| Seek failure | three-class provider outcome: RefusedUnchanged (proven pre-mutation only — the INVALID_ARGUMENT class) = inert, zero content loss (remainder preserved+finished); Applied = the cut, landing known or withdrawn; MutatedThenFailed (everything else, incl. generic SEEK_ERROR which the ABI also returns after a destructive reposition+flush) = ordinary D11 decode-failure route, never resumed as old playback; unknown-landing commits with Position withdrawn | §13, E1, SongCore source audit, E2 REFUSAL-EQUIV/FAIL-CLOSED, formal M6/M7 | status-code-classed refusal (the round-3 MAJOR — SEEK_ERROR is dual-phase); seek failure ⇒ forged NEW terminal variant (D11's existing Failed authority suffices); invented recovery outcomes; treating SEEK_UNSUPPORTED as inert without evidence |

## 20. Environment and limitations

```text
E1  Linux host, static libsongcore (ABI v1), committed corpus (4
    fixtures, all seekable). SEEK_UNSUPPORTED and SEEK_ERROR post-
    states NOT exercisable on this corpus — which is exactly why the
    frozen classification is conservative (§13): only the measured
    INVALID_ARGUMENT class is RefusedUnchanged; everything else is
    treated as destructive, with the SongCore implementation source
    (not the corpus) as the classification evidence. MP3 content
    exactness only after 3 codec frames (lossy tolerance, documented
    by the ABI). eof-tail bound is advisory for lossy (decoder-delay
    artifact recorded).
E2  mechanism probe (protocol shape copy), not the production edge;
    loom exploration of the real edge + invalidate belongs to the F5
    implementation gate. No scheduling guarantee beyond bounded waits.
    The REFUSAL-EQUIV oracle compares against the closed-form no-seek
    control sequence (0..pre_frames contiguous, epoch 0) — by
    construction the exact output a no-seek run of the same budget
    produces, so the equivalence is stronger and strictly less flaky
    than a run-vs-run comparison.
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

Round 2 (fresh reviewer, independent of round 1, against the corrected
branch at 59deaca): verdict **PASS — no MAJOR findings**. Battery:
R2-A refusal destruction (the M-1 class), R2-B race holes, R2-C
authority forgery / vocabulary, R2-D evidence truthfulness, R2-E
formal honesty (live check.sh run), R2-F scope discipline (zero
production delta), R2-G P1–P5 / realtime firewall, R2-H position &
terminal authority, R2-I documentation consistency, R2-J ABI
soundness, R2-K gates & hygiene (all green at HEAD), R2-L stop
conditions. The load-bearing attacks (A, B, C, F, G, H, J, K, L)
passed on the frozen text itself. 5 MINOR + 3 NIT, all documentation/
evidence-number hygiene, all fixed on this branch and re-verified:

```text
R2-D  ADR/RESULTS quoted stale numbers vs retained logs: drain range
      29.9–31.7 ms (logs: 30.2/31.9/31.9), "255×3" scenarios (logs:
      runs=256), landing-early bound "~648 frames ≈ 15 ms" (logs: up
      to 3816 frames ≈ 87 ms mid-stream, 10512 at the duration
      clamp). FIXED to the logged values (ADR amendment text, §3
      row, §19 rows).
R2-E  specs/f5-seek-discontinuity/RESULTS.md still recorded the
      superseded two-phase run (997/300, 3 witnesses, M2
      CutMidWrite). FIXED: rewritten to the current refusal-first
      model record (1349/426, 4 witnesses, M2 SeekMidWrite), the
      superseded record marked historical.
R2-I  ADR header "Amended" ledger lacked the 2026-09-17 F5-GATE
      entry, and the D14.5 spine row "physical cutover gate stays
      OPEN" had no local annotation. FIXED: ledger entry added;
      annotation added after the spine block; the superseded REV.3
      step sketch got a dated supersession pointer; the D14.8
      "never backward" freshness bullet now names the amended
      between-committed-discontinuities scope.
R2-A  stale "two-phase" protocol naming survived in the experiments
      README and the specs RESULTS gloss (the frozen refusal-first
      order itself was verified correct in ADR, harness, and model).
      FIXED: both spots now describe the frozen ordering.
NIT   §6 "NOT-REPRODUCED" leftover sentence contradicted the §6
      reproduction narrative. FIXED (clause removed — it predates
      the endpoint recovery).
```

Round 3 (fresh reviewer, independent of rounds 1–2, against 90ed0e4):
verdict **CHANGES_REQUIRED — 2 MAJOR + 1 MINOR**, all concentrated in
the seek failure/refusal semantics (the successful-cut protocol, output
natural-drain mechanism, edge purge ownership, stale-output invariant,
same-cell Position rebase, pause interaction, single-seek policy and
the Generation/Epoch rejection were all re-confirmed PASS). Fixed here
as **F5-GATE-CORRECTIVE-1**; every claim re-verified against raw
evidence before fixing:

```text
MAJOR-1  the "inert refusal" actually dropped content: the E2 worker
      advanced src_pos/produced accounting by the WHOLE block before
      the bounded-slice write, abandoned the unwritten remainder
      mid-block when the command + parked evidence were observed, and
      published only seek-failed on refusal — the frames [off..n) of
      the abandoned block never reached the edge (a silent content
      gap), while the same authority claimed refusal was "seamless".
      The refusal oracle never checked output continuity and the
      formal model abstracted the remainder away entirely.
      FIXED: the frozen protocol now preserves the in-flight staging
      block at its written prefix; a RefusedUnchanged outcome finishes
      the remainder exactly (zero content loss); an Applied outcome
      discards it with the cut. New E2 REFUSAL-EQUIV oracle (refused
      output == no-seek control, verified GREEN across 32 refused
      scenarios ×3) + drop-one-remainder-frame must-fire control
      (FIRED 1/1 = RED proof). Harness comment claiming "at most one
      staging buffer is the only content a seek can ever cost"
      removed.
MAJOR-2  the two-class failure policy was unsound: the frozen policy
      classed SEEK_ERROR / SEEK_UNSUPPORTED / INVALID_ARGUMENT alike
      as pre-cut inert refusals, but the SongCore implementation
      (native/src/songcore_ffmpeg.c song_seek) returns generic
      SEEK_ERROR from TWO phases — a failed av_seek_frame (phase 1)
      AND again after av_seek_frame succeeded and
      avcodec_flush_buffers + reset_decode_state destructively
      repositioned the decoder (phase 2, "could not reach a landing
      point") — and a post-flush failure leaves the handle producing
      NEW-cursor PCM (fail() records only a diagnostic, fatal_error is
      untouched), so resuming "old playback" on a SEEK_ERROR can mix
      pre/post content with NO committed cutover. FFmpeg likewise does
      not promise a failed av_seek_frame leaves the demuxer
      undisturbed, so even phase-1 statuses are not provably clean.
      FIXED: provider outcome frozen THREE-class — RefusedUnchanged
      (provably pre-mutation only: the parameter/state checks,
      INVALID_ARGUMENT), Applied, MutatedThenFailed (conservative
      rule: unprovable means destructive; routes through the ordinary
      D11 decode-failure path, terminal Failed, never resumed as old
      playback). E2 gains destructive-failure scenarios (6 ×3 runs,
      FAIL-CLOSED oracle: only a contiguous pre-seek prefix may ever
      leave the edge); the formal model splits the refusal
      (SeekRefusedUnchanged / SeekMutatedThenFailed + episodeFailed
      route) and gains InvRefusalContentContinuous + InvFailClosed,
      witnesses 5–6 and mutations M6/M7 (both COUNTEREXAMPLE-
      WITNESSED). Reclassifying a provider result upward is explicit:
      a narrow SongCore provider-contract corrective with its own
      evidence, out of scope here.
MINOR   "ZERO new per-quantum work" was inaccurate (the render loop's
      existing loop-top check gains one more flag test). FIXED: §16
      and the ADR realtime-cost entry now enumerate exactly what is
      and is not added.
```

Verification after the corrective (all green, this branch): cargo
fmt/clippy/test (workspace + experiments, both host and
x86_64-pc-windows-gnu), E2 294×3 green runs with the new oracles and
both must-fire controls (rogue 51/51, mutiny 1/1), TLC f5 suite
(base 672 distinct states PASS, 6 witnesses MUST-FAIL-OK, 7 mutations
COUNTEREXAMPLE-WITNESSED), production delta still zero.
