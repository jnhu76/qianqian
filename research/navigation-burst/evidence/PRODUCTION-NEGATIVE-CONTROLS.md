# Production negative controls — mutation runs (navigation-burst fix)

Fix branch `fix/nav-burst-coalesce` at `4374a40` (+ the toggle-order
P1 fix commit). Base `9f4b954`. Runs executed in a throwaway detached
worktree at the base with the fix sources copied in (scratch tree,
discarded after); each mutation was a minimal source patch applied to
the COPY only, run with `cargo test -p qianqian-headless --lib --`
over the named filters, then restored. Full logs archived beside
this note.

```text
NEGATIVE A  perform_navigation re-wired to the direct per-key API
            (request_*_intent calls replaced by next_track()/
            previous_track(); the pending path bypassed)
  patch     dispatch.rs: let resolved = next_track()/previous_track()
  run       cargo test -- tui::runtime n_and_p o_folder
  log       NEG-A-direct-per-key.log
  result    RED: n_and_p_route_the_manual_navigation FAILED
                 o_folder_seeds_the_list_and_navigates FAILED
            (56 passed, 2 failed) — the "registration opens
            nothing" asserts caught the per-key Open.
  restored  GREEN (fix sources re-copied)

NEGATIVE B1  virtual cursor committed at REGISTRATION
  patch     player.rs: request_navigation_intent's Some(target) arm
            calls playlist.commit_navigation(target) before arming
  run       cargo test -- the_quiet_window an_np_burst
                 intent_burst_matrix an_explicit_open
  log       NEG-B1-early-commit.log
  result    RED: the_quiet_window_boundary_is_strict_and_deadline_
                 _first FAILED ("Still pending: neither cursor has
                 moved"); intent_burst_matrix... FAILED
            (2 passed, 2 failed)
  restored  GREEN

NEGATIVE B2  cursor committed on a REFUSED Open
  patch     player.rs: poll_pending_navigation commits unless the
            outcome is FailStop (i.e. commits on Refused too)
  run       cargo test -- the_pending_open_rides
  log       NEG-B2-commit-on-refused.log
  result    RED: the_pending_open_rides_the_frozen_failure_classes
                 FAILED ("a refusal moves nothing")
            (0 passed, 1 failed)
  restored  GREEN (the shipped tree's own full gate re-ran GREEN)
```

## Owner-review regression (PR #311 REVISE round, 2026-10-22)

Review P2: the direct `next_track()`/`previous_track()` API bypassed
the pending-burst cancellation, so after `request_next_intent()` armed
a burst, a direct `previous_track()` committed its own episode and the
STALE virtual target still opened a second episode at the deadline.
The regression `direct_manual_steps_supersede_the_pending_burst` pins
both directions (a committed direct step and an inert one at the
Repeat-Off boundary each end the window). RED/GREEN provenance:

```text
pre-fix bytes  player.rs sha256 61ea8067f190f99a09678ada706c5157…
               35c8e2a8dad71533baa0c63b4dc2fc5b
RED            cargo test -p qianqian-headless --lib
               direct_manual_steps_supersede_the_pending_burst
               rc=101 "the direct step cancelled the burst
                       left: Some(Instant…) right: None"
               REV-P2-RED-pre-fix.log (+ .meta.txt: cmd/env/sha)
GREEN          same filter with the navigate() cancel line
               rc=0  REV-P2-GREEN-fixed.log
mutation RED   the cancel line removed from the FINAL bytes, same run
               rc=101, same signature
               REV-P2-RED-mutation-final-bytes.log
restored GREEN bytes identical to the final tree, rc=0
               player.rs sha256 7ce3ee545c8b8acd3840a9be75c4a58ecb0…
               0b47f374e114d7c3235ace2e4683c
               REV-P2-GREEN-restored.log
full suite     429 lib + 31 shell/integration tests, 0 failed; the
               historical direct-API burst matrix untouched and green
               (log retained outside the repo:
               ../qianqian-navburst-logs/review-round-2/)
```
