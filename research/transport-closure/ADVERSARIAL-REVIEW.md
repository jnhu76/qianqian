# ADVERSARIAL REVIEW — transport closure design package

Status: reviewed design, not canonical authority.
§1–§3 are the design's own adversarial passes (campaign §45–§47).
§4 is the cross-gate collision table (campaign §44; full table in
DECISION-MATRIX.md §3). §5 records the fresh-context final review
(campaign §48).

---

## 1. F6 attacks (campaign §45)

```text
A. Can the new episode become live before old teardown finishes?
   NO — structurally impossible in the selected mechanism: the fresh
   composition is CONSTRUCTED only after old_runtime.dispose() has
   returned (teardown inverses run synchronously inside dispose: stream
   stop_and_join joins the render leg, worker inverse joins the
   producer). There are two independent kernel instances; no desired
   entry of the new kernel can mount while the old kernel exists.
   The probe runs before teardown but owns no RT resource (§ H).

B. Can an invalid new file unnecessarily kill valid old playback?
   NO — probe refusal is the first step, before any destructive one.
   Honest residual (accepted, disclosed): the probe is advisory, so a
   file that passes probing but fails between probe and activation
   (TOCTOU, device failure) lands in the post-destruction failure
   class (F6 §6) — old is gone, diagnostic visible. That is "old
   intentionally replaced then replacement failed", not "invalid file
   killed old playback": the file opened and declared audio at probe
   time.

C. Can the old render thread survive into the new episode?
   NO — dispose() returns only after stop_and_join has joined the
   render leg and the edge-stop+join inverse has joined the worker
   (session.rs teardown ordering; K0 runs inverses during retire/
   drain before dispose returns). The old device session is released
   inside stop_and_join; the new episode's open_stream happens after.

D. Does Open accidentally become same-episode semantics?
   NO — no open/replacement method exists or is proposed on
   PlaybackSessionHandle (F6 §9). The old episode settles its own D11
   terminal Fact; the new episode is a fresh semantic scope with its
   own handle. "Open" names only the App composition operation.

E. Is rollback promised after destructive old teardown?
   NO — explicitly frozen out (F6 §6): once disposal has run, the old
   world is gone; no reopen/rollback semantics exist. The only
   recovery is a new replacement command.

F. Does Open during Seek leave a stale seek worker?
   NO — by contract: replacement always passes through old-episode
   settlement (wait_terminal), and the frozen D14.5/D14.7 teardown
   obligations require the stop to wake every parked participant and
   the teardown to join the worker before discharge. wait_terminal
   returning proves the committed Fact: the worker has published its
   terminal evidence and is exiting; the join itself completes inside
   dispose()'s teardown inverses (F6 §4 orders wait_terminal →
   dispose), which is where the no-stale-worker guarantee actually
   rests. F6 adds no seek-specific handling and must not: the
   dependency is "F5 implementation honors its frozen join
   obligations", which its gate text already freezes.

G. Does a terminal old episode get "reopened"?
   NO — a settled episode is disposed, never resumed; Open always
   creates a NEW episode (new handle, new scope). The old handle stays
   read-only (observe only); its committed truth is immutable.

H. Is preflight source probing incorrectly treated as a live episode?
   NO — the probe is classified as a composition-decision mechanism
   query that owns no render stream, no edge, no worker, no device
   session (F6 §3). Guards recorded: it must not read PCM frames
   (no decode pumping), must not cache media content, and its result
   never becomes episode truth — the new activation's own
   open/probe publishes the authoritative source evidence.
```

## 2. Navigation attacks (campaign §46)

```text
A. Does the index change before Open succeeds?
   NO — commit-on-activation (NAVIGATION §3): the index moves only on
   the F6 commit evidence.

B. Can the playlist cursor claim track B while A still plays?
   NO — during a replacement the cursor still names A (it has not
   moved); at the commit instant the cursor and the live episode move
   together. There is no window in which the marker names B while A is
   the audible/current episode.

C. Does a failed Next silently skip multiple tracks?
   NO — one keypress advances exactly one candidate; probe failure
   refuses in place; no auto-skip, no retry loop (NAVIGATION §5).

D. Does navigation create a second playback authority?
   NO — navigation issues composition commands and owns navigation
   state only; playback truth remains D11 + the D14.2 seam. The index
   is explicitly non-authoritative (PBK-001 §2.3 discipline).

E. Is PlaylistPlugin actually necessary?
   NO — D13 admission table (NAVIGATION §8): no independent desired
   composition identity, no independent K0 lifecycle ordering, the App
   owns it without correctness loss.

F. Does EOF accidentally trigger auto-next despite the v1 exclusion?
   NO — auto-next is excluded and nothing is wired to observe terminal
   Facts: the App renders the terminal state and waits for a key. No
   subscription, no watchdog, no deferred auto-advance exists in the
   design.

G. Does direct Open corrupt playlist semantics?
   NO — exactly one frozen policy exists (Open replaces the playlist
   with [path], index 0, on commit); navigation and Open share the
   same candidate→probe→replace→commit path. No second "play outside
   the playlist" concept exists that could diverge from the marker.

H. Are first/last boundary rules deterministic?
   YES — inert at both ends, no wrap, plain index arithmetic on a
   total order; a boundary N/P is inert command history.
```

## 3. Volume attacks (campaign §47)

```text
A. Can volume control alter system-wide output?
   NO — selected mechanism is per-stream IAudioStreamVolume; the
   endpoint-master candidate was rejected outright (VOLUME §5-C).

B. Can another qianqian stream/session be affected unexpectedly?
   NO — stream-local by contract; the session-master candidate
   (ISimpleAudioVolume) was rejected precisely because it would affect
   every stream in the process's session (VOLUME §5-B1).

C. Does 50% claim a false loudness meaning?
   NO — explicit no-claim rule (VOLUME §2/§4): the level is a request
   to the mechanism; Microsoft's own loudness/amplitude relationship
   is cited, not promised by us.

D. Does the mechanism add permanent per-sample cost unnecessarily?
   NO — the engine applies stream volume; player-side per-quantum cost
   is one relaxed load+compare at the existing loop-top, applied
   on-change (disclosed, VOLUME §6). Software gain (the permanent-tax
   candidate) is rejected on this path.

E. Does volume reset on every Open?
   NO (audibly) — the App's desired level persists; each new stream is
   initialized from it at open, before first submission (VOLUME §8).
   The engine-side per-stream value resetting with the stream is
   unobservable because the re-application precedes first submission.

F. Does a volume change affect pause/seek truth?
   NO — non-terminal, same-episode, no topology cut, no Position
   reset, no discontinuity (VOLUME §7).

G. Is Volume incorrectly called a Fact?
   NO — application configuration / Command state, routed like pause
   intent; never a Fact, never semantic truth (VOLUME §2).

H. Does zero require a separate Mute state?
   NO — stream-level zero suffices for v1; no restore semantics are
   needed yet (VOLUME §9).

I. Can external OS/session changes make the TUI value a lie?
   NO — the TUI displays the DESIRED level (the App's request), not a
   mechanism/system reading; no external actor can change what the App
   desires. The audible product remains the documented product of
   several factors, which the player never displays or claims.
   The mixer-coupling scenario that WOULD have created this lie
   (ISimpleAudioVolume) was rejected on those grounds (VOLUME §5-B1,
   §9).

J. Does the backend mechanism leak into the generic application API?
   NO — the seam is `request_output_level(VolumeLevel)` with
   `VolumeLevel(u8)`; no WASAPI type crosses the seam;
   IAudioStreamVolume exists only inside the output provider; the
   RenderRequest control is mechanism-generic (another platform
   realizes it with its own mechanism, software gain being the
   recorded fallback).
```

## 4. Cross-gate collisions (campaign §44)

Resolved table: DECISION-MATRIX.md §3 (all eleven collisions reduce to
existing owners; no cross-feature state). Summary of the mechanism:
every collision enters through exactly one of three doors — an episode
seam Command, the F6 replacement serialization point, or App-level
configuration — and each door's behavior is already frozen by
D14.4/D14.5/D14.7 or defined by this package without new nouns.

## 5. Fresh-context final review (campaign §48)

Reviewer: fresh-context agent, no prior involvement in this campaign;
loaded AGENTS.md, ADR-PBK-001/002, and this package; reviewed for
ownership consistency, authority duplication, new-noun necessity,
cross-gate collisions, F5 assumptions, D11 compatibility, P1–P5
implications, RT permanent cost, TUI scope creep.

```text
Round 1 (fresh-context):  VERDICT: PASS  (0 MAJOR, 5 MINOR — fixed)
Round 2 (human, PR #155): VERDICT: CHANGES_REQUIRED
                          (2 MAJOR, 1 MINOR — fixed below)
Round 3 (human, PR #155): VERDICT: CHANGES_REQUIRED
                          (1 MAJOR, 4 MINOR, 2 NIT — fixed below)
Round 4 (human, PR #155): VERDICT: CHANGES_REQUIRED
                          (1 MAJOR, 2 MINOR — fixed below)
Final state: all findings resolved; READY_FOR_HUMAN_REVIEW
```

Review rounds:

### Round 1 — fresh-context reviewer

Independent fresh-context agent; loaded AGENTS.md, both ADRs, and this
package; spot-checked every reality claim against the production source
at `main @ feee7a3`; judged all twelve checklist points (ownership
consistency, authority duplication, new-noun necessity, cross-gate
collisions, F5 assumptions, D11 compatibility, P1–P5 implications, RT
permanent cost, TUI scope creep, internal consistency, reality
accuracy, honesty).

```text
VERDICT: PASS  (0 MAJOR, 5 MINOR — all fixed in this package before
                stopping, per campaign §48)
```

Findings and resolutions:

```text
MINOR-1  README §4 asserted a review record that did not exist yet.
         FIXED: this section now carries the actual round record and
         verdict; README §4 updated to match.

MINOR-2  Track-panel rendering described two ways (TUI §7 vs
         NAVIGATION §3) for the post-replacement activation-failure
         state. FIXED: TUI §7 now states the single rendering rule —
         active episode ⇒ Track names the episode's file and the
         observation seam drives Format/timeline/State; no episode ⇒
         Track/State render the no-episode state plus diagnostic while
         the playlist '>' stays on the committed index entry.

MINOR-3  probe_media during live playback means two concurrent native
         SongCore handles (production's first such scenario) —
         undisclosed. FIXED: F6 §3 now discloses it and mandates
         S-PROBE (probe-during-live-playback confirmation on a Windows
         host, ×3 green runs) at F6-IMPLEMENTATION, analogous to
         V-PROBE; README §4 bounds updated.
         (Placement corrected by Round 2 MAJOR-2: S-PROBE moved to an
         evidence-only slice BEFORE authority promotion — see below.)

MINOR-4  ADVERSARIAL §1.F claimed "wait_terminal returning means the
         worker has exited" — imprecise. FIXED: wait_terminal proves
         the committed Fact (evidence published, worker exiting); the
         join completes inside dispose()'s teardown inverses, which is
         where the guarantee rests (wording tightened above).

MINOR-5  "campaign §N" citations point at a document that is not in
         the repository. FIXED: README §5 now explains what they
         reference and that the load-bearing content is restated
         inline.
```

### Round 2 — human review of PR #155

Independent human review of the package at `60dec50`; verdict
`CHANGES_REQUIRED` with 2 MAJOR + 1 MINOR; Navigation judged PASS,
Volume ownership/mechanism selection judged PASS. All findings fixed
in place (this corrective is POST-F5-TRANSPORT-CLOSURE-CORRECTIVE-1);
no gate document was rewritten.

```text
MAJOR-1  teardown violation must be FAIL-STOP. The package claimed
         "process stays usable — a later Open starts from a fresh
         composition" after a `quiet == false` disposal. That
         contradicted K0 reality, verified in kernel.rs: a violated
         fiber is never eligible for unload (eligible_unload) or slot
         removal (removal_candidate), the violated latch has no exit
         ("the episode may not close over a violated teardown
         contract"), and violated effect records remain as provenance
         tombstones — so quiet == false means discharge is NOT proven,
         not "old world gone but unhealthy". Letting a new episode
         mount on an unproven-discharged world would violate the F6
         no-overlap root invariant itself.
         FIXED: F6 §6 now rules quiet == false ⇒ FAIL-STOP (no new
         episode, no further Open/Next/Previous replacement in this
         process, restart required); the §4 replacement ladder marks
         quiet == false as fail-stop; TUI §3 adds the fail-stop guard
         and fatal banner (only Q/Ctrl+C remain); TUI §5 marks it
         non-transient; NAVIGATION §6 records the propagation
         (replacement permanently disabled until restart);
         DECISION-MATRIX §1 updated. No recovery authority is assumed;
         recovery-after-violation would need its own earned decision.
         (Verdict source renamed by Round 3 MAJOR-1: the fail-stop
         trigger is now the authoritative disposal outcome —
         `TeardownViolated` — not a snapshot `quiet` read.)

MAJOR-2  S-PROBE ordering inverted — the plan promoted F6 authority
         before validating its load-bearing physical premise. The
         fallback (probe after settlement) abandons the product
         property "an invalid source never kills live playback",
         which is an F6 authority change, not a representation
         detail — so Candidate B is not promotable without the
         physical evidence.
         FIXED: the route now matches the F5 E3 discipline
         (mechanism physical fact → evidence → authority freeze →
         implementation): F5 merge → F6-S-PROBE (evidence-only slice,
         no production feature work) → promotion only on GREEN; a RED
         S-PROBE reopens the F6 mechanism decision instead. TUI §8
         ladder reordered (S-PROBE as step 2, promotion as step 3);
         F6 §3 rewritten; DECISION-MATRIX §2 promotion plan gated.
         Volume promotion split accordingly: owner/semantics CLOSED,
         Windows candidate mechanism = IAudioStreamVolume, physical
         RT placement pending V-PROBE — the apply mechanism does not
         fully close before V-PROBE.

MINOR-1  SetAllVolumes was described as "bounded, non-blocking" —
         a claim the official documentation does not make (it
         contracts stream-local scope and the 0.0–1.0 domain only;
         the GetService/Release thread note is lifetime discipline,
         not an RT-safety proof).
         FIXED: VOLUME §6 withdraws the claim ("no non-blocking /
         bounded-latency guarantee is claimed anywhere in this
         design"); V-PROBE V4 was rewritten and V5 added — measure
         render iteration latency and underrun/glitch behavior across
         repeated +/- presses and paused changes, plus the
         device-invalidated failure path; a materially perturbing
         result reopens the apply-point/ownership decision before
         VOLUME-IMPLEMENTATION freezes it.
```

### Round 3 — human review of PR #155 (CORRECTIVE-2)

Independent human review of the package at `d07da30`; verdict
`CHANGES_REQUIRED` with 1 MAJOR + 4 MINOR + 2 NIT. The round-2
corrective was judged PASS on its own terms: the fail-stop semantics,
the S-PROBE-before-promotion ordering, D11 terminal-authority
discipline, D13 navigation, the P1–P5 judgment and volume ownership
all passed. The remaining blocker was an ADR-firewall regression. All
findings fixed in place
(POST-F5-TRANSPORT-CLOSURE-CORRECTIVE-2); no gate document rewritten.

```text
MAJOR-1  CompositionSnapshot was used as control correctness
         authority — a direct PBK-001 §2.3 violation, in two places:
         F6 gated new-episode creation on `dispose()`'s snapshot
         `quiet`, and Navigation committed `current_index` on
         `composition_snapshot()`'s `FiberState::Active`. The
         snapshot's own production contract declares it a read-side
         projection for diagnostics/tests ("an observation, not a
         success certificate"); control decisions consumed it anyway.
         The projection firewall built in F2/F4 must not be re-entered
         through the K0 snapshot back door.
         FIXED: F6 §5 freezes the rule — F6/Navigation correctness
         MUST NOT depend on CompositionSnapshot; replacement commit
         consumes two authority-owned control-operation results (the
         authoritative disposal outcome and the authoritative
         activation result, with candidate Rust shapes recorded and
         spelling deferred to F6-IMPLEMENTATION). The §4 ladder, §6
         failure classes, §7 decision table, NAVIGATION §3, TUI §3/§9
         and DECISION-MATRIX §1/§2 were rewritten accordingly. No new
         Fact kind, no snapshot rename, no new K0 primitive — ordinary
         synchronous operation results.

MINOR-1  C3's canonical topology refinement was under-recorded: a
         fresh QianqianApp per episode changes the Qianqian App's
         realization from "the one application composition root" to
         "one process-level reference-player host sequentially owning
         multiple non-overlapping composition roots, one per episode".
         FIXED: DECISION-MATRIX §2 promotion now explicitly records
         the refinement sentence and updates D1 (App realization
         note) + D5 (provider lifetime property) + D14.6 (mechanism)
         together; F6 §4 points at it.

MINOR-2  `probe_media` is an intentional decode-provider
         public-surface amendment (today the public surface admits
         only the plugin constructor; enforced by
         check_plugin_boundaries.py), not a plain helper. FIXED: F6 §3
         records the amendment and freezes the narrow contract — the
         App may call ONE provider-owned, stateless preflight query
         exposing SourceFacts only, never SongcoreDecode /
         DecodedPcmStream / song handles / service internals; the
         implementing slice must sync check_plugin_boundaries.py;
         DECISION-MATRIX §2 records the amendment.

MINOR-3  The PR #155 body lagged the package (still round-1 text:
         one review, S-PROBE at F6-IMPLEMENTATION, quiet-snapshot
         commit evidence). FIXED: the body was rewritten to the
         current state — three review rounds, S-PROBE before
         promotion, authority-owned commit results.

MINOR-4  TUI §8 key-unlock numbering misread its own ladder
         ("N/P after 4; +/- after 5; O after 3"). FIXED: ←/→ after 1;
         O after 4; N/P after 5; +/- after 7.

NIT-1    Fail-stop Q still described the graceful shutdown path, but a
         world with a latched teardown violation is proven not
         normally dischargeable. FIXED: TUI §3 freezes two Q paths —
         normal Q = stop → wait → dispose → exit; fail-stop Q /
         Ctrl+C = immediate process termination, no stop/wait/dispose
         attempted, no graceful-disposal claim, no recovery designed.

NIT-2    Blocking-Open UI liveness was implicit. FIXED: TUI §3 states
         that v1 accepts blocking replacement control (D11 promises no
         terminal liveness) and that responsive cancellation of a hung
         replacement is NOT promised.
```

### Round 4 — human review of PR #155 (CORRECTIVE-3)

Independent human review of the package at `792a3e4`; verdict
`CHANGES_REQUIRED` with 1 MAJOR + 2 MINOR. Round 3's snapshot fix was
judged truly fixed; the alignment matrix passed every contract except
"explicit composition teardown ownership on the failed-new-root
path". All findings fixed in place
(POST-F5-TRANSPORT-CLOSURE-CORRECTIVE-3).

```text
MAJOR-1  Activation failure did not mean the fresh QianqianApp was
         gone. `drop` runs no teardown inverses (the lib.rs contract),
         the failed attempted root can still hold Active Decode/Output
         fibers (session Failed/Pending or never run), and the design
         let `active = None` drop it while a later Open could start a
         third composition with no discharge evidence for the failed
         one — breaking D14.6 no-overlap on the failure path.
         FIXED: the start operation is FAILURE-CLEAN (F6 §6): a
         fresh-composition start that does not establish disposes the
         attempted root itself before returning — cleanup Discharged ⇒
         ActivationFailedClean(diagnostic), no runtime remains, a
         future Open is legal; that cleanup disposal reporting
         TeardownViolated ⇒ FAIL-STOP. Fail-stop additionally RETAINS
         the violated composition root until process termination —
         never dropped as if cleanly disposed. `Activated` is defined
         over the whole fresh composition ("successfully established
         the required Playback Session episode") — covering provider
         activation failure, unresolved dependency and session
         activation failure alike; never absence-of-session-diagnostic,
         never a snapshot read (F6 §5). NAVIGATION §3/§5 record the
         no-orphan consequence; TUI §2/§3 record no failed-root holder
         + fail-stop retention; DECISION-MATRIX §1/§2 updated. Not a
         rollback: the old episode stays gone; only the failed new
         world is cleaned up.

MINOR-1  V-PROBE V1 measured the wrong isolation target: stream A vs
         another PROCESS's stream B — a test even the rejected
         ISimpleAudioVolume could pass, since another process usually
         has another audio session. FIXED: V1 split into V1a (the
         discriminating experiment: two simultaneously rendering
         streams in the SAME process / same default audio session;
         SetAllVolumes(A) leaves B unchanged) and V1b (secondary:
         other-process stream unchanged; cannot substitute for V1a).
         A V1a coupling failure reopens the §5 mechanism decision.

MINOR-2  "zero new mechanism" overclaimed: the package adds probe_media
         and two result seams — new API surfaces if not new
         subsystems. FIXED: reworded to "zero new source-configuration
         mechanism" (no config channel / registry / hot-replacement
         machinery) in F6 §4 and the PR body; F6 §4 states the honest
         seam inventory.
```
