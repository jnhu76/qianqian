# PRODUCTION CLOSEOUT — navigation burst (RC-A)

Research campaign `QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0`
(Issue #165, PR #170) produced the diagnosis and the boundary
adjudication; this note records what landed in production and where
the corrective evidence lives. It is a closeout pointer, not a second
authority: R3 remains the design reference for the coalescing
semantics (§4/§6/§8), and the code comments + tests are the executable
contract.

## What changed

```text
apps/headless/src/playlist.rs
    TemporaryPlaylist::preview_step(from_position, forward):
    the manual-step policy parameterized by the from-position.
    manual_step is now a one-line delegation, so a burst's virtual
    target walks the SAME traversal policy (Sequential / Shuffle
    permutation / Repeat Off / One inert / All wrap / single-entry
    inertia) and cannot drift from it.

apps/headless/src/player.rs
    App-owned pending navigation intent: ONE
    PendingNavigation { virtual_position, deadline } on
    ReferencePlayerApp, driven by request_next_intent(now) /
    request_previous_intent(now) / pending_navigation_deadline() /
    poll_pending_navigation(now) / cancel_pending_navigation(),
    with NAVIGATION_QUIET_WINDOW = 100 ms (R3 §8's recommended
    75–125 ms band). Expiry with net displacement runs the UNCHANGED
    D14.6 replacement exactly once (commit-on-success, failure
    classes untouched); expiry at the committed row collapses to zero
    opens. A pending burst defers poll_eof_policy (manual wins over
    auto-next, R3 §6); explicit targets (open / play_selected /
    play_current), direct next_track()/previous_track() steps,
    traversal-geometry edits (remove / clear / set_order) and Stop
    cancel it; Quit drops it.

apps/headless/src/tui/runtime/{mod,dispatch,picker}.rs
    The shell translates N/P keys and transport buttons into intent
    registrations (no Open, no busy-interval drain at registration);
    the event loop shortens event::poll to
    min(TICK, time-to-deadline) and settle_timers() resolves the
    burst BEFORE the EOF policy each iteration. Stop / Play-Pause /
    Seek cancel the pending burst. The direct next_track()/
    previous_track() API is unchanged and still pinned by the
    historical burst matrix.
```

No new Plugin, runtime, thread, timer, Fact, or K0 participant. No
crates/ change. D11 / D14.6 semantics untouched.

## Effect (deterministic, kernel-mediated counts)

App-level oracle `intent_burst_matrix_coalesces_to_at_most_one_
replacement` over the real player (real kernel + session, fake
providers; 10 ms cadence, 8-track Sequential/Repeat-Off, committed
start 3):

```text
input      keys  old opens (R1 §2)   new opens   final
N             1        1                 1         5/8
NN            2        2                 1         6/8
NNNNN         5        4 (5th inert)     1         8/8
NP            2        2                 0         4/8
NNNPPNNP      8        8                 1         6/8
NPNPNP        6        6                 0         4/8
NNPPNNPP      8        8                 0         4/8
```

Probe / episode-start / retirement counts equal the Open count per
row (each commit rides the whole frozen replacement; a collapsed
burst attempts nothing). Deadline boundary: 1 ms early = nothing,
`now >= deadline` = the one Open; a key landing exactly on a deadline
sees the expired burst commit first, then arms its own window (R3 §4:
the strict boundary does not collapse).

## Negative controls (mutation-verified RED, restored GREEN)

Recipes, run filters, results and full logs archived at
`evidence/PRODUCTION-NEGATIVE-CONTROLS.md` +
`evidence/NEG-A-direct-per-key.log` / `NEG-B1-early-commit.log` /
`NEG-B2-commit-on-refused.log`:

```text
A  shell re-wired to the direct per-key API
   → n_and_p_route_the_manual_navigation,
     o_folder_seeds_the_list_and_navigates RED
B1 virtual cursor committed at registration
   → the_quiet_window_boundary_is_strict_and_deadline_first,
     intent_burst_matrix_coalesces_to_at_most_one_replacement RED
B2 cursor committed on a refused Open
   → the_pending_open_rides_the_frozen_failure_classes RED
```

## Owner review round (PR #311 REVISE, 2026-10-22)

Review P2: the direct `next_track()`/`previous_track()` API bypassed
the cancellation — after a pending burst was armed, a direct step
committed its own episode and the STALE virtual target still opened a
second episode at the deadline. Fixed in `navigate()` itself (the two
public direct entries are its only callers): a direct step supersedes
the burst exactly like a direct Open, committed or inert. The
regression `direct_manual_steps_supersede_the_pending_burst` is RED
without the fix (pre-fix run and mutation-at-final-bytes run, both
rc=101) and GREEN with it; provenance and full logs at
`evidence/PRODUCTION-NEGATIVE-CONTROLS.md` § Owner-review regression.
A fresh-context delta review swept every App entry that opens an
episode or rebuilds traversal state: no second hole of this class
(DELTA_VERDICT FIXED_CLEAN). One informational note it added:
`establish_playlist` still relies on caller discipline (it rides an
Open commit in every production path), so a future shell adapter must
keep that order — not a reachable hole today.
Windows native acceptance remains NOT_RUN — an owner-side pre-merge
platform gate, separate from this review round.
