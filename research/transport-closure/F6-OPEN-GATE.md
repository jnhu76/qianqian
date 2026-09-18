# F6 — Open / fresh-source replacement (design gate)

Status: reviewed design proposal, not canonical authority.
Authority base: ADR-PBK-002 D14.6 (no-overlap replacement v1 +
`F6 CONFIG-MECHANISM-OPEN`), D14.10 (stop-list), D11, D13; ADR-PBK-001
§2.2/§2.3, §6 (P1–P5).

---

## 1. Root proposition

F6 is not "open a path". It is **replacement of the currently active
source/episode**. The frozen v1 semantic requirement is D14.6's
no-overlap constraint, quoted here only as the base:

> The old playback episode must be fully retired from K0 before the new
> playback episode becomes live.

Root safety invariant (restatement of D14.6's direction):

```text
OLD / NEW NO-OVERLAP
Once the new episode becomes live, the old episode must no longer own
any live RT-visible playback resource capable of contributing output.
```

Frozen consequences already carried by D14.6 and kept unchanged here:
Open creates a new episode (it is not Seek); an audible gap is
acceptable in v1; `Generation`/`Window`/Realtime Audio Runtime are not
earned; the old episode keeps any already-committed terminal truth; no
`Superseded`/`Preempted` terminal variant is invented.

## 2. Open is a Command

```text
Candidate:  Open(path)
Truth class: Command (intent)
```

No `Opening` / `Opened` / `OpenCompleted` / `SourceTransition` Fact,
state, or fact kind is proposed: no consumer needs one. The
user-visible result is observed through the **existing** read side: the
new episode's D14.2 observation (source format/duration evidence,
Position projection, terminal outcome), exactly as the first episode is
observed today. `Open` returning is not truth; the new episode's
observation is. If Open succeeds/fails is application composition
feedback (§8), not a playback semantic.

## 3. Pre-destructive validation — Candidate B selected

One of the two load-bearing F6 decisions.

```text
Candidate A (rejected)
    Open(new) → immediately stop old → discover new is invalid
    ⇒ old playback lost unnecessarily

Candidate B (selected)
    Open(new) → validate/probe new source OFF the live playback path
              → invalid  : old episode untouched, diagnostic shown
              → usable   : replace old using the no-overlap contract
```

**Probe ≠ episode.** A source probe opens the media container, reads
the probe facts (format, declared duration) and closes. It owns no
render stream, no edge, no worker, no device session — nothing
RT-visible. It is therefore **not** a second live playback episode and
**not** an old/new overlap in the D14.6/P1–P5 sense; D14.6 forbids a
second *live render/decode episode*, not harmless metadata reads.
Truth class of probe output: **mechanism evidence used for an
application composition decision** (validated against its contract:
"this source opened and declared X at probe time"). It is advisory; it
never becomes episode truth. The new episode's own activation
re-opens the source and publishes the authoritative `source_format` /
`source_duration` evidence (D14.8 shape) as today.

**Who probes (the narrow mechanism decision).** Current reality: the
decode mechanism (`SongcoreDecode`) is crate-private (plugin-boundary
hardening H1), and capability services are resolvable only inside
Fiber activation — the App cannot reach `PcmDecode::open_media`. The
probe therefore needs one narrow new surface, selected from candidates:

```text
P1  no probe (Candidate A)                     REJECTED (loses playback
                                               on invalid files)
P2  App-callable probe query published by the  SELECTED
    decode provider crate — one public, stateless
    mechanism query (open → probe → close), no
    Capability, no Plugin, no composition identity
    (a plain function call; D13-oracle-clean:
    "one-shot read operation", owned by the caller)
P3  probe inside the new session's activation  REJECTED (activation runs
                                               after old teardown — this
                                               IS Candidate A)
P4  probe via a new probe Capability consumed  REJECTED (a Capability
    by a one-shot probe Fiber                  consumer must be a
                                               mounted Fiber; composing
                                               a probe episode per Open
                                               abuses K0 for a function
                                               call)
```

Proposed shape (representation open, for the implementation gate):

```text
qianqian-decode-songcore:
    pub fn probe_media(path) -> Result<SourceFacts, DecodeOpenError>
    SourceFacts { format: PcmFormat, duration: Option<Duration> }
```

It reuses the existing open/probe machinery internally (SongCore
`song_open` + `song_probe`); it reads no PCM frames. Failure classes of
the probe are exactly `DecodeOpenError` today.

**Boundary amendment record.** Making `probe_media` public is an
INTENTIONAL amendment of the decode-provider public-surface contract
(today: the plugin constructor only; `SongcoreDecode` crate-private per
H1; enforced by `tools/check_plugin_boundaries.py`). The promoted
contract is narrow: the App may call ONE provider-owned, stateless
preflight query exposing `SourceFacts` only — never `SongcoreDecode`,
`DecodedPcmStream`, a song handle, or capability/service internals. The
F6-IMPLEMENTATION slice must update `check_plugin_boundaries.py`
together with the surface, and the promotion plan records the amendment
(DECISION-MATRIX §2).

**Concurrency disclosure (S-PROBE — evidence slice BEFORE promotion).**
The probe runs while the old episode's decode worker may still hold its
own SongCore handle — production's first two-concurrent-native-handles
scenario (today `open_media` is strictly one-handle-at-a-time in
effect, one episode per process). SongCore is expected to support
independent per-handle contexts, but that is a native-behavior claim no
document in this package can prove. The evidence order therefore
matches the F5 E3 discipline — mechanism physical fact → evidence →
authority freeze → implementation:

```text
F5-IMPLEMENTATION merge
    ↓
F6-S-PROBE                evidence-only slice, no production feature
                          work: probe during live playback on a Windows
                          host, ×3 green runs — old stream content and
                          cadence unaffected, probe verdicts correct
    ↓ GREEN               Candidate B (probe-before-destruction) may be
                          frozen by the authority-promotion slice
    ↓ RED                 Candidate B is NOT promotable: the fallback
                          (probe after settlement) abandons the product
                          property "an invalid source never kills live
                          playback", which is an F6 authority change —
                          the mechanism decision REOPENS instead of
                          silently degrading to Candidate A
    ↓
AUTHORITY-PROMOTION → F6-IMPLEMENTATION
```

An undocumented native change is never a fallback.

## 4. Replacement mechanism — the narrow config decision

D14.6 leaves the configuration/handoff mechanism
(`F6 CONFIG-MECHANISM-OPEN`) explicitly to a narrow authority decision
and forbids a coding agent from inventing one. This is that decision,
proposed. Candidates evaluated against current kernel reality:

```text
C1  per-episode concrete definitions            REJECTED
    (register "playback_session#N" per Open):
    definition names are &'static str (leak/pool
    needed), the catalog grows without bound, and
    the definition table becomes a registry by
    another name — the exact shape D14.6 forbids.

C2  per-instance config payload on DesiredEntry REJECTED for v1
    (K0 learns to carry opaque config):
    widens the kernel's desired-composition datum
    for one feature; heavyweight; K0 stays
    domain-agnostic only at the price of new
    plumbing no other consumer needs.

C3  WHOLE-EPISODE-COMPOSITION REPLACEMENT       SELECTED
    at the App boundary (a fresh QianqianApp
    per episode; the file is a constructor
    argument, so no config channel exists at all)

C4  App-owned single-use config source +        REJECTED for v1
    Revision::fresh() re-incarnation of one
    session definition:
    K0-native (staged replacement, see below)
    and keeps providers mounted, but needs a
    playback-crate API change, a precisely
    disciplined mutable config cell, and its own
    frozen ownership text — strictly more
    authority surface than C3 for zero
    measurable v1 gain (providers are stateless
    and remount-trivial; the device is opened
    per episode under every candidate).
    Recorded as the first candidate to revisit
    if a later multi-session/preload topology
    needs providers to survive across episodes.
```

**Why C3 satisfies D14.6 with zero new source-configuration
mechanism.** The App already
builds exactly one episode-composition per run
(`apps/headless/src/main.rs start_episode`): register decode/output/
session definitions → `revise_desired` → episode. C3 makes Open repeat
that operation mid-process:

```text
Open(path)
    ↓ App probes path (§3)                    — invalid ⇒ REFUSED,
                                                old episode untouched
    ↓ if no current composition root          — first episode / after
      (first episode, or previous start        ActivationFailedClean:
      ended ActivationFailedClean):            old-side clear holds;
      skip stop/wait/dispose                   no disposal outcome is
                                                forged (§5)
    ↓ else, if old episode unsettled:
        old_handle.request_stop()             — intentional stop command
        old_handle.wait_terminal()            — D11 authority-owned
                                                settlement observed
    ↓ old_runtime.dispose()                   — K0 retires every Fiber,
                                                teardown/discharge runs;
                                                the operation yields its
                                                authoritative disposal
                                                outcome (§5)
    ↓ disposal outcome == Discharged          — old fully retired per
                                                the authority-owned
                                                result (not a Fact, not
                                                a snapshot read — §5);
                                                TeardownViolated is
                                                FAIL-STOP (§6)
    ↓ start_episode(path)                     — fresh composition:
                                                new definitions, new
                                                handle, revise_desired,
                                                activation probes/opens;
                                                FAILURE-CLEAN — a start
                                                that does not establish
                                                disposes the attempted
                                                root itself before
                                                returning (§6)
    ↓ activation result == Activated          — new episode live
                                                (authority-owned start
                                                outcome, §5)
```

Every step runs on an existing mechanism except the narrow seams this
gate already declares (§3 probe query, §5 result seams): those are new
API surfaces, not new source-configuration machinery — no config
channel, no registry, no hot-replacement mechanism exists because the
file is a constructor argument of a fresh composition. The ordering
matches D14.6's frozen product-level sequence term by term
(intentional stop → D11 settlement → old Fiber withdrawn +
teardown/discharge complete → new episode may become live), and the
no-overlap invariant holds **structurally**: the old kernel is fully
disposed before the new one is constructed, so no old/new RT-world
overlap can exist even transiently. **P1–P5 are not triggered** — there
is no publication of a replacement view into a world whose readers may
still hold the old one; the old world is gone first.

Per-Open cost, honestly stated: two stateless provider re-activations
(`SongcoreDecode::new` = one ABI-version check; `WasapiOutput::new` =
trivial) plus the per-episode device open that every candidate pays
(`RenderStream` owns the device session per episode today). Provider
lifetime property changes from "spans playback episodes" (D5's current
property, not a requirement) to "spans one episode". C3 also refines
the Qianqian App's canonical realization: one process-level
reference-player host sequentially owns multiple non-overlapping
`QianqianApp` composition roots, one per playback episode. The
authority-promotion slice records both explicitly, updating D1
(App realization note), D5 (provider lifetime) and D14.6 (mechanism)
together (DECISION-MATRIX §2).

## 5. Replacement commit point — authority-owned operation results

```text
replacement commit
    := old-side clear
       ∧ new episode's authoritative activation result == Activated

old-side clear
    := no current composition root
           (first episode, or the previous start ended
            ActivationFailedClean — §6)
       OR the current composition's authoritative disposal
          outcome == Discharged
```

No disposal outcome is forged when no root exists: `old-side clear`
makes the no-current-root case (first startup, Open after a clean
failed start) explicit instead of pretending a `Discharged` verdict
that never happened. One formula covers first startup, Open after
`ActivationFailedClean`, and Open after a previously clean disposal.

Both operands are **synchronous results of the authority-owned control
operations the App itself invokes** (the disposal operation and the
start-episode composition operation). They are NOT Facts (no fact kind,
no public surface, no consumer outside the App), NOT new K0 primitives,
and NOT snapshot reads. The frozen rule (PBK-001 §2.3 firewall):

```text
F6 / Navigation control correctness MUST NOT depend on
CompositionSnapshot.

Disposal success and activation success are supplied by
authoritative control-operation outcomes, or by validated
authority-owned evidence from the authority that ran the
operation (e.g. the session's own activation outcome).

CompositionSnapshot remains exactly what its own contract
says: a read-side projection for diagnostics/tests — "the
snapshot is an observation, not a success certificate."
```

This is not a new architecture noun: it is the ordinary Rust result of
an operation the App already performs. Current production reality makes
the gap explicit — `dispose()` returns only the diagnostic snapshot,
and the headless wiring infers activation from a snapshot fiber state;
both are diagnostic uses. The F6-IMPLEMENTATION slice therefore
introduces the two narrow result seams (representation deferred to that
slice; candidate shapes `DisposeOutcome::{Discharged, TeardownViolated}`
or `Result<(), TeardownViolation>`, and
`StartOutcome::{Activated, ActivationFailedClean(diagnostic)}` — the
failure variant returned only after the attempted root is
authoritatively discharged, §6), recorded in
DECISION-MATRIX §2 as part of the promoted mechanism.

`Activated` is defined over the WHOLE fresh composition, never over
one diagnostic:

```text
Activated
    := the fresh desired composition successfully established
       the required Playback Session episode

NOT:  absence of a session activation diagnostic — a Decode/Output
      provider failure or an unresolved dependency can leave the
      session with nothing to report;
NOT:  a snapshot FiberState read (the frozen rule above).
```

The start operation's authority-owned result therefore covers provider
activation failure, unresolved dependency and session activation
failure alike — supplied by the K0/App control operation's own
outcome, or by a validated activation acknowledgment inside the start
operation (representation deferred to F6-IMPLEMENTATION).

This is D14.6's own "old Playback Session Fiber withdrawn +
teardown/discharge complete" boundary, made explicit so the App has
exactly one place where "the replacement happened" is decided
(navigation index commit, NAVIGATION-GATE §3, consumes precisely these
operation results).

## 6. Failure classes (explicitly separated)

| Class | Rule | Old playback |
|---|---|---|
| new-source validation failure | Open REFUSED before any destructive step; diagnostic surfaced (§9 of TUI gate) | **continues untouched** |
| old-episode settlement failure | governed by the existing D11/frozen D14.5/D14.7 semantics; Open waits on `wait_terminal`. D11 promises no liveness: a hung decoder blocks Open — no timeout is invented in v1 | settles as itself |
| old teardown failure | authoritative disposal outcome `TeardownViolated` ⇒ **FAIL-STOP**. A latched teardown violation means discharge was **not proven**: K0 keeps the violated fiber mounted — it is never eligible for unload or slot removal, the violated latch has no exit, and the effect records remain as provenance tombstones (`kernel.rs` §G.6 reality). No new episode is constructed, and NO further Open / Next / Previous replacement is attempted in this process. The App RETAINS ownership of the violated composition root until process termination — it is never dropped as if cleanly disposed (`drop` runs no teardown inverses); Q/Ctrl+C take the immediate-termination path with no graceful-disposal claim. The TUI shows `Fatal teardown violation — restart required` and keeps only Q/Ctrl+C. Recovery-after-violation is not assumed; it would have to be earned as its own separate authority decision | old world **NOT proven discharged**; truth immutable |
| new activation failure (after old is gone) | **the start operation is failure-clean** (ladder below): the attempted fresh root is disposed by the start operation itself before any failure result returns — cleanup `Discharged` ⇒ `ActivationFailedClean(diagnostic)`, NO runtime remains, the diagnostic surfaces through the existing `activation_error` D14.2 seam, and a later Open is legal; that cleanup disposal reporting `TeardownViolated` ⇒ FAIL-STOP (row above, applied to the attempted root). No rollback, no hidden reopen: the old world stays gone (D14.6: replacement cannot relabel committed truth). "Activation did not establish" never means "the attempted root may be dropped": only authoritative discharge retires a composition | **gone** (stopped); attempted fresh root cleaned up or fail-stop |

No rollback exists at any point. The disposal outcome has exactly two
honest readings. `Discharged` proves the old world retired: replacement
may proceed, and the only "undo" a user has is issuing a new
replacement command — a new Open/N with the old source is a new
replacement, not a rollback. `TeardownViolated` proves the opposite —
that discharge was NOT achieved: the K0 violated-teardown latch has no
exit, the violated fiber stays mounted (never eligible for unload or
removal, effect records remain as tombstones), so no new episode and
no further replacement may be attempted in this process — fail-stop
until restart (row above). "Later Open starts from a fresh
composition" is available only after a `Discharged` disposal or a
clean process start, never after a latched violation.

**The start operation is failure-clean.** "Did not successfully become
an episode" does not mean "that fresh composition root no longer
exists": `QianqianApp` has no `Drop` teardown — only authoritative
discharge retires a composition, and a failed attempted root can still
hold Active Decode/Output fibers. A failed fresh-composition start may
therefore return control to the reusable TUI only after that attempted
composition itself has been authoritatively discharged:

```text
construct fresh QianqianApp
    ↓ attempt activation
    ├─ Activated            → ActiveEpisode(runtime, handle, file)
    └─ activation did not establish
         → dispose the attempted fresh runtime
         ├─ Discharged       → ActivationFailedClean(diagnostic);
         │                     no runtime remains; a future Open is
         │                     legal
         └─ TeardownViolated → FAIL-STOP: the violated runtime is
                               RETAINED by the App until process
                               termination — never dropped as if
                               cleanly disposed — and no further
                               Open/Next/Previous runs
```

This is not a rollback: the old episode is already gone; the operation
only cleans up the failed new world. Skipping the cleanup would leak
the attempted root's live fibers on `drop` and let a following Open
start a third composition while the failed one has no discharge
evidence — breaking the no-overlap invariant this gate exists to hold.

## 7. F6 required decision table (campaign §12)

| Concern | Selected rule | Why |
|---|---|---|
| Open owner | App composition operation (owns current episode runtime + handle; Playback Session never replaces itself) | §4; D14.6 expected shape; session = exactly one episode (D6) |
| validation timing | before any destructive step; App-owned probe (decode-provider query) | §3; invalid file must not kill valid playback |
| old stop boundary | `request_stop()` on the old handle iff unsettled; frozen D14.4 semantics | existing Command; idempotent |
| teardown boundary | `dispose()` after settlement observed; the authoritative disposal outcome (§5) is the discharge evidence | existing K0 mechanism + one narrow authority-owned result seam (F6-IMPLEMENTATION) |
| new activation boundary | only after old-side clear (§5: no current root, or a `Discharged` disposal outcome) | D14.6 no-overlap, structural |
| invalid new source | Open refused; diagnostic; old untouched | §3 Candidate B |
| new activation failure | start operation is failure-clean: attempted root authoritatively disposed before the failure returns (`Discharged` ⇒ `ActivationFailedClean`, no runtime remains; cleanup `TeardownViolated` ⇒ FAIL-STOP retaining the root); diagnostic via `activation_error`; no episode; no rollback | §6; honest dead-end; `drop` ≠ teardown |
| paused old episode | stop-from-paused follows frozen D14.7 terminal interactions (mid-play → `Stopped`; post-EOF drain → `Completed`) | D14.7 frozen; no new rule |
| already-terminal old episode | skip stop/wait; dispose directly | settlement already committed; truth immutable |
| Open during Seek | record stop intent; the frozen D14.5 refusal/precedence rules govern the in-flight cut (refuses pre-cut, or abandons without commit; episode settles per existing precedence); replacement proceeds after settlement | replacement owns the larger lifecycle; same-episode Seek cannot outlive it; no navigation/open-specific seek sync invented |
| repeated Open | App-thread-serialized: each Open is one complete replace operation; a second Open is processed after the first commits or aborts | one App control thread (§39/TUI gate); no concurrent replacement state |

## 8. v1 exclusions (frozen)

```text
NO preload          NO crossfade          NO gapless
NO old/new render overlap
NO speculative second output stream
NO transactional reopen / rollback semantics
NO multi-session topology
```

No future topology (zero-gap, preload) is built into v1; any
overlap-bearing successor must be earned separately under the
abstraction-earning rule, as D14.6 already states.

## 9. What F6 adds to the public surface (proposal, not promoted)

```text
App layer (apps/headless): Open(path) composition operation, yielding
                           the authoritative start outcome (§5);
                           failure-clean per §6
composition App/kernel:    authoritative disposal outcome on dispose
                           (§5) — both are ordinary synchronous
                           operation results, not Facts and not K0
                           primitives; Rust spelling deferred to
                           F6-IMPLEMENTATION
qianqian-decode-songcore:  probe_media(path) mechanism query (§3) — an
                           intentional, narrow public-surface amendment
                           of the decode-provider contract
Playback Session seam:     UNCHANGED in this gate (no open/replacement
                           method on PlaybackSessionHandle — Open is not
                           a same-episode command; §10 of the campaign
                           verified: PlaybackSessionHandle::open(path)
                           would put a composition operation inside the
                           one-episode owner and is rejected)
New nouns:                 NONE (no OpenFact, no Opening state, no
                           SourceTransition, no ReplacementId; the §5
                           operation results are return values, not
                           taxonomy)
New Plugins:               NONE (D13 oracles: TrackOpenOperation /
                           probe query / replacement operation are all
                           "existing Plugin can own / plain call" cases)
```
