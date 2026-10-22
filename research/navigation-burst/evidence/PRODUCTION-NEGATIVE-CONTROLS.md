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
