# NAVIGATION — playlist / Next / Previous (design gate)

Status: reviewed design proposal, not canonical authority.
Authority base: D14.6 ("Next/Previous are navigation/selection decisions
followed by the same no-overlap Open semantics"), D13, D14.10
("playlist/queue authority" stop-list item); this gate proposes that
decision.

---

## 1. Root question

Not "which Plugin owns the playlist", but: **what is Next actually
doing?**

```text
Next/Previous = choose another PathBuf, then invoke the F6 Open
                replacement.
```

If that is true — and current reality supports it (the session owns
exactly one episode; nothing about a queue exists anywhere in the
production path) — then:

```text
Navigation is application composition, not playback authority.
```

The playlist never crosses an architectural boundary: no Capability, no
Fact, no mechanism reads it. K0, the Playback Session, and the
Decode/Output providers remain unaware that a playlist exists.

## 2. Minimal playlist model (selected)

```rust
// reference-player App state (apps/headless), plain ownership:
playlist: Vec<PathBuf>,
current_index: Option<usize>,   // None = nothing current
```

Owned by the reference-player App/TUI. Rejected owners: Playback
Session (one-episode owner; would force it to become a queue), K0
(domain-agnostic, forbidden), Output/Decode Plugins (mechanism
providers; D5). No database/library abstraction; no dedup, no metadata
reading, no sorting; duplicate paths are allowed and are distinct
entries (v1 reference behavior, smallest rule).

## 3. Index truth — commit-on-activation (B selected)

`current_index` is **application navigation state**. It is NOT the
identity of the currently audible source, and it is never read as
playback truth (read side stays the D14.2 observation; PBK-001 §2.3
firewall).

```text
Candidate A  optimistic (index moves on keypress)     REJECTED
             ⇒ the playlist cursor lies whenever Open later fails.

Candidate B  commit-on-activation                     SELECTED
    N/P pressed
        → candidate index := current_index ± 1 (computed, not committed)
        → probe candidate (F6 §3): failure ⇒ REFUSED, index unchanged,
          old playback untouched, diagnostic shown
        → F6 replacement runs
        → only when the replacement commit evidence exists
          (F6 §5: old disposal quiet ∧ new episode Active)
          does current_index := candidate
```

The smallest activation evidence the App can use is exactly the F6 §5
commit evidence — kernel composition snapshots the App already reads
(`dispose()` snapshot quiet; `composition_snapshot()` fiber Active). No
`NavigationFact`, no index event, no new observable; the index is
App-private state.

Corollary (post-destruction activation failure): if the probe passed
but the new episode's activation fails, the old episode is already
settled (user-intentional replacement) and the index stays at the old
entry. The cursor then names a track that no longer plays — honest,
because the cursor is navigation state, not audible-source truth; the
Track/State panel (driven by the observation seam) shows "no episode"
plus the diagnostic. No auto-retry, no auto-skip.

## 4. Navigation actions (v1 frozen semantics)

```text
Next (N)     current_index := Some(i+1) if i+1 < len else INERT
Previous (P) current_index := Some(i-1) if i > 0     else INERT
Open (O)     playlist := [path]; current_index := Some(0) — on commit
Startup args playlist := every positional path (in order);
             current_index := Some(0); first episode starts
```

Boundary policy: **inert at both ends** — no wrap, no stop-the-player
side effect. A boundary-hitting N/P is command history only. No
`RepeatOne` / `RepeatAll` / `Shuffle` (not in scope; each would need its
own authority reasoning).

**Open × playlist policy (one simple v1 rule):** Open **replaces** the
playlist with the single opened path and selects index 0. Rejected
alternatives: append+select (needs dedup/position policy — more state
and more semantics for zero v1 need) and play-outside-playlist (drives
a wedge between the `>` marker and the audible track — two truths about
"current"). "What you opened is what you have" is the smallest
predictable reference-player behavior. Navigation and Open therefore
share one path: both end in "commit index on F6 replacement evidence";
Open just rewrites the list first. Directory/multi-file Open: **one
file only** in v1 — the file picker (an input line, TUI gate §6) yields
exactly one path; populating a playlist remains startup-args territory.
Open must not grow into library scanning.

Direct playlist selection (jump-to-item): not in the v1 key map
(campaign §2); recorded as a later minimal addition that would reuse
this exact candidate→probe→replace→commit path. EOF auto-next: **NO**
(campaign §19 recommendation kept): it would create an autonomous
composition trigger interacting with D11/F6 replacement for zero first-
closure need; after `Completed` the TUI shows the terminal outcome and
the user presses N. Revisit only with its own authority reasoning.

## 5. Failed-track navigation (frozen)

```text
If the Next/Previous candidate fails validation (probe):
    navigation index unchanged; current episode keeps playing;
    diagnostic surfaced (TUI gate §7). No automatic skip, no search/
    retry behavior, no silent multi-track advance.
If activation fails after replacement began:
    see §3 corollary — index unchanged, no episode, diagnostic.
```

One keypress advances at most one candidate; failures never cascade.

## 6. Navigation + Seek / Pause — reduction to F6

Because every navigation action composes F6 replacement:

```text
Next while paused       → replacement of a paused episode; frozen
                          D14.7 stop-from-paused rules settle the old
                          episode; new episode starts unpaused (pause
                          intent is per-episode Command state on the
                          old handle and does not leak into the new
                          one)
Previous during seek    → Open during Seek (F6 §7): replacement owns
                          the larger lifecycle; frozen D14.5 precedence
                          governs the in-flight cut
Direct sel. during seek → same as Previous-during-seek
Volume during all of it → VOLUME-GATE §8: desired level survives,
                          re-applied to the new episode
```

No navigation-specific synchronization exists: there is nothing to
synchronize — replacement is the single serialization point (App-thread
sequential commands, F6 §7 last row).

Fail-stop propagation (from F6 §6): a latched teardown violation during
any replacement permanently disables further replacement in this
process — Next/Previous/Open become inert until restart. No navigation
recovery path exists, and none may be invented around the F6 fail-stop
rule.

## 7. Playlist display

```text
>  committed navigation current item (current_index)
   every other entry
```

`>` means exactly `current_index` **after commit** — never the hover,
never the candidate, never a requested-but-failed item. Candidate
feedback during an in-flight Open is ephemeral status-line text only
(TUI gate §7), never authoritative state. During the replacement window
the `>` marker stays on the old item until the commit moves both the
index and the episode together; after a refused candidate nothing
moves.

## 8. D13 admission check (record)

```text
candidate            independent   independent   existing owner   Plugin?
                     composition   K0 lifecycle  suffices?
                     truth?        ordering?
Playlist             NO            NO            YES (App state)   NO
PlaylistPlugin       NO            NO            YES               NO
NavigationPlugin     NO            NO            YES               NO
NextOperation        NO            NO            YES               NO
```

No new Plugin, Capability, Fact kind, lifecycle noun, or K0-visible
state is earned by navigation. Playlist/queue authority stays App-level
— this gate's proposal is precisely that the answer to the OPEN
"playlist/queue authority" item is "application composition state; no
new authority".
