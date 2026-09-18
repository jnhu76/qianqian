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
VERDICT: PASS
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
