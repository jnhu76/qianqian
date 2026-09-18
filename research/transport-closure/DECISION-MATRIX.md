# DECISION MATRIX + AUTHORITY-PROMOTION PLAN

Status: reviewed design proposals, not canonical authority.

---

## 1. Final decision matrix (campaign §49)

| Domain | Decision | Rejected alternatives | Evidence / reason |
|---|---|---|---|
| F6 owner | App composition operation replacing the whole episode composition (fresh `QianqianApp` per episode) | `PlaybackSessionHandle::open(path)` (same-episode command — wrong owner, D6); per-episode component definitions C1 (unbounded catalog, registry-by-another-name); K0 per-entry config payload C2 (kernel widening); App-owned config source + revision re-incarnation C4 (more authority surface, zero v1 gain; recorded as first candidate to revisit for preload/multi-session) | D14.6 expected shape; kernel staged-replacement reality (`kernel.rs step()`); provider re-mount cost trivial (`SongcoreDecode::new`, `WasapiOutput::new`); device open is per-episode under every candidate |
| new source preflight | App-owned probe BEFORE any destructive step, via one public decode-provider mechanism query (`probe_media`: open → probe → close, no PCM read) | destructive-first (Candidate A — loses valid playback on invalid files); probe inside new activation (= A); probe Capability + one-shot probe Fiber (K0 abuse); App constructs its own decode mechanism instance (H1 boundary breach) | Campaign §7; `SongcoreDecode` crate-private today (no seam exists — this is the narrow decision D14.6 deferred); probe owns no RT resource ⇒ not an overlap; gated by F6-S-PROBE — evidence-only, BEFORE promotion (RED reopens this row) |
| replacement boundary | stop intent (iff unsettled) → `wait_terminal` → `dispose()` → authoritative disposal outcome `Discharged` → mount fresh composition → authoritative activation result `Activated` | committing before discharge (violates D14.6); reading commit decisions off `CompositionSnapshot` / fiber states (read-side projection — diagnostics only, PBK-001 §2.3) | D14.6 frozen ordering term-by-term; no-overlap structural, P1–P5 untriggered; commit consumes authority-owned operation results (F6 §5) |
| failed Open | probe refusal ⇒ inert (old untouched, diagnostic); activation failure after old is gone ⇒ visible `activation_error`, no episode, NO rollback; latched teardown violation (disposal outcome `TeardownViolated`) ⇒ process FAIL-STOP — no further replacement, restart required | hidden retry/reopen; rollback to a destroyed/unproven old episode; auto-fallback to another candidate; "later Open after a violated disposal" | §6 failure classes; D14.6 (old truth immutable); campaign §8; K0 §G.6 — the violated latch has no exit and the violated fiber is never eligible for unload/removal |
| playlist owner | reference-player App: `Vec<PathBuf> + Option<usize>` | PlaylistPlugin / Playback Session / K0 / providers / database abstraction | D13 oracle table (NAVIGATION §8); nothing outside the App ever reads it |
| index commit | commit-on-activation: index moves only on F6 commit evidence (authoritative disposal outcome `Discharged` ∧ authoritative activation result `Activated`) | optimistic (cursor lies on failure) | NAVIGATION §3; index is navigation state, never playback truth |
| Next/Prev ends | inert at first/last; no wrap | wrap; auto-stop at ends | smallest deterministic rule; campaign §18 |
| EOF auto-next | NO in v1 | auto-Next on Completed | campaign §19: avoids an autonomous composition trigger against D11/F6 for zero first-closure need |
| volume owner | App owns desired level (application configuration); session routes it; output mechanism realizes it per stream | VolumePlugin/mechanism-owned truth/global store (D14.9 forbidden list) | VOLUME §2–§3, §11 |
| volume range | integer `VolumeLevel` 0..=100, step 5, clamped | f32 0..1 (NaN/equality hazards); decibels (no dB UI, no curve promise) | VOLUME §4 |
| Windows volume mechanism | `IAudioStreamVolume` via `GetService` on the episode's own render client; `SetAllVolumes` all channels; applied at open + loop-top-on-change | ISimpleAudioVolume (session-scoped: hits the other qianqian instance; SndVol-coupled; persistent across restarts; needs session-GUID machinery); IAudioEndpointVolume (system master; Microsoft says avoid for shared-mode); IChannelAudioVolume (session channel volume, unneeded); software PCM gain (permanent per-sample RT tax; recorded fallback for future platforms) | Microsoft Learn: IAudioStreamVolume / Session Volume Controls / Endpoint Volume Controls (VOLUME §5 citations); the candidate is SELECTED, but its physical RT placement does not close before V-PROBE (V4/V5 perturbation measurements; no bounded/non-blocking claim) |
| volume persistence | desired level survives Open/Next/Previous (App-owned; re-applied at each new stream's open) | per-episode reset (ownership would be wrong); on-disk persistence (no config file in v1) | VOLUME §8; campaign §31 cross-check |
| mute | none — `Volume = 0` suffices for v1 | separate Mute state | campaign §32; no restore semantics needed yet |

## 2. Authority-promotion plan (campaign §43)

Performed LATER by a tiny dedicated slice, after F5-IMPLEMENTATION
merges AND the F6-S-PROBE evidence slice is GREEN (F6 §3), against
human-accepted decisions from this package. This branch performs NO
promotion. Expected promotion is small and mechanical:

```text
F6 gate → ADR-PBK-002 (amend D14.6 / new D14 block)
    condition        : F6-S-PROBE GREEN first — an evidence-only slice
                       between the F5 merge and this promotion. A RED
                       S-PROBE reopens the F6 mechanism decision:
                       Candidate B and its "invalid source never kills
                       live playback" property are NOT promotable
                       without the physical evidence (the
                       probe-after-settlement fallback would change
                       that property — an authority change, not a
                       representation detail).
    canonical target : §14 open item "open/session CONFIG mechanism";
                       D14.6 amendment
    propositions     : Open is an App composition Command (no new Fact);
                       probe-before-destruction (invalid source never
                       kills live playback; probe ≠ episode, no P1–P5);
                       replacement commit = authoritative disposal
                       outcome Discharged ∧ authoritative activation
                       result Activated — control consumes
                       authority-owned operation results and NEVER
                       CompositionSnapshot (PBK-001 §2.3; snapshots
                       stay diagnostic); no rollback after destructive
                       teardown; failure-class table (F6 §6)
    mechanism        : whole-episode-composition replacement at the App
                       boundary; authoritative disposal-outcome and
                       activation-result seams (ordinary operation
                       results, not Facts; Rust spelling open);
                       decode-provider public probe query — an
                       INTENTIONAL decode-provider public-surface
                       amendment, contract frozen narrow: SourceFacts
                       only, never SongcoreDecode / DecodedPcmStream /
                       song handles / service internals;
                       check_plugin_boundaries.py synced by the
                       implementing slice; canonical topology
                       refinement recorded — one process-level
                       reference-player host sequentially owns multiple
                       non-overlapping QianqianApp composition roots,
                       one per playback episode; promotion updates D1
                       (App realization note) + D5 (provider lifetime
                       "spans episodes" → "spans one episode") + D14.6
                       together, not D14.6/D5 alone
    still open       : probe_surface Rust spelling; disposal/activation
                       result Rust spelling; Open input UX; C4
                       re-incarnation mechanism (only if a future
                       preload/multi-session gate needs it)

NAVIGATION gate → ADR-PBK-002 §14 item "playlist / queue authority"
    propositions     : playlist/index = application navigation state;
                       commit-on-activation; inert boundaries; no
                       auto-next; no auto-skip; Open replaces playlist
    mechanism        : none above the App (this CLOSES the open item
                       with "no new authority")
    still open       : direct selection key; startup-args grammar
                       details

VOLUME gate → ADR-PBK-002 (amend D14.9)
    canonical target : §14 item "volume authority / mechanism"
    propositions     : desired volume = application configuration
                       (Command family), App-owned, survives episode
                       replacement; stream-local realization; zero
                       suffices, no Mute; TUI shows desired only;
                       volume change is a non-event for D11/D14.7/D14.8
    mechanism        : promotion should read — Volume owner/semantics
                       CLOSED; Windows candidate mechanism =
                       IAudioStreamVolume (session-owned output-level
                       control routed like pause intent; software gain
                       recorded as the portable fallback); physical RT
                       placement (apply points, perturbation bounds)
                       PENDING V-PROBE — the apply mechanism does not
                       fully close before V-PROBE (VOLUME §10; no
                       bounded/non-blocking claim is promoted)
    still open       : output-level control Rust spelling; RenderRequest
                       field shape; non-Windows mechanisms

TUI gate → no ADR promotion (presentation); Issue #119 roadmap rows only
```

Promotion must not import this package's prose wholesale: each slice
rewrites the frozen propositions in ADR voice, updates the §14 open
list and the Amended ledger, and leaves representation marked open per
row above.

## 3. Cross-gate collision review (campaign §44)

| Collision | Resolution (existing owner) |
|---|---|
| Open × Seek in flight | App records stop intent; frozen D14.5 rules decide the cut (pre-cut refusal ⇒ inert; otherwise the episode settles via existing precedence — the cut never commits after stop wins the data plane). Replacement proceeds after settlement. No new sync. |
| Open × Paused | Frozen D14.7 stop-from-paused: mid-play ⇒ `Stopped`; post-EOF drain ⇒ `Completed`. New episode starts unpaused (pause intent is old-episode Command state). |
| Open × terminal drain | Stop during drain plays the tail out and settles `Completed` (D14.7 frozen); replacement then proceeds; old truth immutable. |
| Next × failed candidate | Probe refuses before any destructive step: index unchanged, playback untouched, diagnostic (NAVIGATION §5). |
| Next × paused current track | Same as Open × Paused — replacement of a paused episode; no navigation-specific rule. |
| Previous × in-flight Seek | Identical to Open × Seek — every navigation action IS an Open. |
| Volume × Open replacement | Desired level is App state above episodes: re-applied at the new stream's open (VOLUME §8). No interaction. |
| Volume × paused replacement | Same as above; the new episode opens at the desired level whether the user then pauses or plays. |
| Volume × output activation failure | New episode never opened ⇒ nothing to apply; desired level unchanged, still applied at the next successful episode. Diagnostic via existing activation path. |
| Quit × replacement | Commands are App-thread-sequential (TUI §3): Q processed only after the in-flight Open commits or aborts; Q then stops/disposes whatever is active. No interleaving exists to design. |
| Stop × Open | S settles the current episode (`Stopped`); a following O replaces the settled episode (skip stop/wait, F6 §7). Two ordinary sequential commands. |

All eleven reduce to: existing episode seam commands + F6's single
replacement serialization point + App-level configuration. No
cross-feature state was created.
