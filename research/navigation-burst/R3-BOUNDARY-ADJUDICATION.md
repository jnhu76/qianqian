# R3 — BOUNDARY ADJUDICATION

Campaign: QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0
Base: `ad4e09dc9064077b97cc6f26dd9139d81dae6e99`

Recommendation set for the burst defect (RC-A, R1) and the boundary
questions in campaign §16–§26/§38/§43. MINIMUM architecture; rejected
alternatives included. Nothing here is implemented.

## 1. Answers (§43)

```text
Where should NavigationIntent live?
    A UI-NEUTRAL interaction-intent layer ABOVE the App (the shell
    adapters translate raw events into RelativeNavigation intents);
    the PENDING target itself is App-owned state (it is navigation
    state, and the playlist traversal policy it must respect is
    already the App's).

Where should temporal coalescing live?
    The SAME layer, as pending-state + quiet-window deadline driven by
    the normal event loop — never a sleep (§46). NOT in the TUI
    adapter, NOT in K0, NOT a Plugin.

Should playlist expose pure preview traversal?
    YES — a from-position preview is the ONE API gap the experiment
    demonstrated: `TemporaryPlaylist::manual_step(&self)` is already a
    pure preview, but only FROM THE COMMITTED cursor; chaining a burst
    needs `preview_step(from_position, forward) -> Option<usize>`
    (same policy: Sequential/Shuffle permutation, Repeat Off/One
    inert, All wraps). Coalescing MUST reuse the traversal policy
    through the playlist so it cannot drift from it (§22) — a local
    `index += delta` reimplementation would break Shuffle/wrap
    semantics by construction.

Does ReferencePlayerApp need API changes?
    ONLY the pending state's home: App-owned
    `next_intent()/previous_intent()/tick/poll_pending()` style
    methods (naming open) that move the pending target through the
    playlist preview and — on quiet-window expiry with
    virtual != committed — invoke the EXISTING Open replacement once.
    The frozen D14.6 sequence, commit-on-activation and every failure
    class are untouched. No new command, no new Fact, no new state
    machine in the session.

Does D14.6 require amendment?
    NO normative amendment required: coalescing happens BEFORE any
    Open — one deliberate Open replaces K implicit ones; repeated Open
    stays App-thread-serialized; no-overlap, failure classes, and
    commit evidence unchanged. The U2 navigation invariants (inert
    boundaries, Shuffle traversal, Repeat One never traps manual N/P,
    commit-on-activation) must be preserved THROUGH the playlist
    preview API, which is why that API belongs to the playlist. A
    short implementation-note amendment (representation-only, "the
    shell may hold a pending navigation target as App interaction
    state") is the honest paperwork if review wants the state named.

Is a new Plugin earned?
    NO (D13 adjudication below: every candidate fails the admission
    invariant).
```

## 2. Candidate homes for coalescing (§16) — attack and verdict

| Candidate | Attack | Verdict |
|---|---|---|
| A. TUI-only debounce (crossterm → debounce → App) | Duplicated by every future GUI/media-key/remote adapter; TUI timing detail leaks into semantics; also the TUI tick (150 ms) granularity would define product behavior | REJECTED |
| B. Generic shell / interaction-policy layer (TUI/GUI/MediaKey → Navigation Intent Policy → App) | Right FUNCTION, but as a SEPARATE component it needs access to the playlist traversal state and to the App's commit path — a second owner of navigation state, or a new coupling surface | FUNCTION ADOPTED, home corrected: the policy lives IN the App (which already owns navigation state), INTENTS arrive UI-neutrally from adapters |
| C. ReferencePlayerApp itself (next_intent/previous_intent/tick) | "Mixes interaction timing policy with product navigation policy" — accepted ONLY because the timing policy is thin (one deadline + one pending target) and inseparable from the traversal preview it must call; the playlist remains pure | RECOMMENDED |
| D. Playlist object owning time/debounce | The playlist is pure traversal/model (its strength — the prototype reused `manual_step`'s purity); giving it a clock pollutes the model | REJECTED for time; ADOPTED for the pure preview API |
| E. New Plugin | D13 below | REJECTED |

## 3. D13 admission (§17)

| Candidate | Existing owner possible? | Independent lifecycle? | Correctness lost if resource? | D13 result |
|---|---|---|---|---|
| NavigationIntentPolicy | Yes — the App (owns navigation state + commit path) | No (dies with the App; no resources) | No | **App state/methods, NOT a Plugin** |
| NavigationBurstCoalescer | Yes — same owner (one deadline field) | No | No | App state, NOT a Plugin |
| TransitionCoordinator | Yes — the replacement is already App-owned and App-thread-serialized (D14.6) | No | No (a second coordinator would FORK commit ownership) | Not needed |
| MediaSource (future) | — | — | — | Deferred to R4 |
| ContentCache (future) | — | — | — | Deferred to R4 |
| Prefetcher (future) | — | — | — | Deferred to R4 |

No Plugin is earned by the burst work. Nothing here touches K0.

## 4. Virtual navigation cursor (§21) — validated by prototype

The prototype (`prototype/`, outside production; results in
`evidence/quiet-window-prototype.txt`) implements exactly this model —
committed position + pending virtual position + quiet window — and
drives REAL `ReferencePlayerApp::open` commands for every commit:

- collapse occurs exactly when key cadence g < window w;
- `NNNNN@10ms`: 4 opens (today) → **1 open**; `NNNPPNNP@10ms`: 8 → **1**;
- `NP` with w > gap → **0 opens** (the zero-Open outcome §21
  requires; the boundary is strict — w == gap does NOT collapse, the
  window expires exactly when the second key lands), and
  `NNNPPNNP` still lands on the policy-correct final track in every
  cell of the matrix;
- every commit still rides the frozen replacement (commit-on-activation
  unchanged).

One prototype-earned policy correction: an INERT key (boundary reached)
must NOT cancel a pending window — it is still user activity; the
window restarts. (The first prototype run swallowed a whole burst's
commit otherwise.)

## 5. Explicit target vs relative burst (§19)

Relative navigation (N/P, GUI Next/Prev, media keys) → the pending +
quiet-window path. Explicit target activation (playlist row Enter,
GUI double-click, direct Open) names the FINAL target already — commit
IMMEDIATELY (the existing Enter live-row rule stays). Do not debounce
every Open.

## 6. Pending vs EOF auto-next precedence (§24) — specified, not implemented

```text
A committed Completed Fact arrives while a manual target is pending:
    the pending manual target WINS: resolve it (one replacement; the
    completed episode is retired by that replacement's frozen sequence
    anyway). The EOF policy does not also fire — the episode record's
    consumed-flag discipline (player.rs:531-535) already guarantees
    exactly-one automatic attempt; the pending rule sits IN FRONT of
    poll_eof_policy in the loop.
Pending resolves to virtual == committed (N then P back to current):
    no-op; the normal Completed policy then applies to the live
    episode as if the burst had not happened.
D11 is untouched: Completed remains the Playback Session's terminal
truth; precedence is App decision order, never evidence relabeling.
```

## 7. Pending state is not playback truth (§25)

The pending target is interaction/UI-App control state. It is NOT a
CurrentTrack/Navigation Fact, NOT episode identity, NOT activation
truth; the read side stays the D14.2 observation; until the Open
commits, the old episode remains the committed playback episode.

`playing`, `selected`, and `pending` remain THREE distinct concepts:

```text
playing   = committed App navigation state; moves only on Open commit
selected  = presentation browsing state; moving it never changes playback
pending   = uncommitted relative-navigation intent during the quiet window
```

A burst MUST NOT mutate the existing `selected` cursor merely to render
its pending target. Doing so would collapse browsing state into
uncommitted playback intent and would make the existing `>` marker mean
two different things. The App therefore keeps pending state separately.
A shell MAY project the pending target with transient status text or a
future dedicated affordance, but that projection must not reuse or
redefine `selected`. On commit, the existing U2 rule remains unchanged:
manual navigation commit moves `playing` and then makes `selected`
follow the committed row. No new playback Fact or third playlist cursor
is introduced by this research decision.

## 8. Quiet window: value and mechanism (§20/§46/§47/§48)

- Measured trade-off (prototype + R1 timing): single-N feels immediate
  when total latency ≈ today's ~90 ms + w. w=50 ms collapses only
  cadences <50 ms; w=200 ms adds 200 ms to every single press.
  Collapse requires w > burst cadence; field bursts are 10–150 ms.
- **Recommended evidence-based range: 75–125 ms** (collapses the
  observed fast-burst band; single-press total ≈ 165–215 ms worst
  case, comparable to the warm single-switch the field already
  accepts; final choice is a product decision, not frozen here). NOT
  "150 ms because the TUI tick is 150 ms".
- Mechanism (§46): pending state + deadline checked by the NORMAL
  event-loop progression (TUI: the existing loop's tick — with the
  honest caveat that a sub-tick deadline commits at the next tick, so
  the loop needs a small timeout slice or deadline-relative poll;
  future GUI: its own timer). NO sleep in the key handler; no
  blocking of any kind.
- §47's two-state machine (NoPending / Pending(target, deadline)) is
  SUFFICIENT; explicit-target actions bypass it.

## 9. Boundary table (§38)

| Concern | Recommended owner | Plugin? | Reason |
|---|---|---:|---|
| TUI key mapping | TUI adapter (runtime/model) | No | presentation/input |
| Navigation burst policy (pending + quiet window) | ReferencePlayerApp (App state + methods) | **No** | D13: no independent lifecycle/resource; owns nothing |
| UI-neutral intent vocabulary (RelativeNavigation ±1 / explicit target) | adapter→App seam (plain method calls suffice today; a type only when a second adapter with different transport earns it) | No | explanatory vocabulary, not architecture (AGENTS razor) |
| Playlist traversal (incl. NEW pure preview_step) | App playlist model | No/current | pure product state |
| Episode replacement | ReferencePlayerApp via D14.6 (unchanged) | No/current | App composition owner |
| Decoder | existing Decode Plugin | Yes | already earned |
| PCM edge | session-owned mechanism (unchanged) | No | current ownership |
| Output | Output Plugin (+ owned backend, PBK-003) | Yes | already earned |
| Remote source / Source cache / Prefetch | future — see R4 | TBD | D13 result deferred |

## 10. PCM cutover change? (§26/§29/§30)

```text
PCM_CUTOVER_CHANGE: NOT_EARNED
```
R2 §4: manual navigation already abandons the old tail (no drain
wait); the cutover invariant is structural in whole-root replacement.
If a future design ever overlaps old/new render worlds, PBK-001 P1–P5
+ a D14.6 amendment are prerequisites — out of scope.

## 11. Negative design controls (§45) — self-check

No TUI-only debounce; no sleep()/blocking in the handler; no per-raw-
key Open retained under the policy; no delta-index navigation (preview
goes through the playlist policy — Shuffle/permutation preserved);
Repeat One cannot trap manual navigation (preview keeps the U2 rule:
manual N/P inert at the boundary under Off/One); manual-pending vs EOF
precedence specified (§6); selection never promoted into playback
truth; no new CurrentTrack/Navigation Fact; no new Plugin; no async/
cancellation machinery (the App thread stays the single serializer);
no multi-episode predecode (intermediate targets stay control-plane:
probe none, decode none, PCM none — only the FINAL target earns the
data plane); no PCM generation tags (no evidence); no whole-song PCM
cache; no URL-only cache identity (R4); no networking refactor leaking
into the local player (R4 is design-only).

## 12. Fresh-reviewer open questions

See `ADVERSARIAL-REVIEW-1.md` — recorded independently against §51's
twenty questions.
