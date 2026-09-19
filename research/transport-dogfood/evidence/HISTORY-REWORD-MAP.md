# History reword map (2026-09-19, commit-conformance only)

The campaign branch commit SUBJECTS were rewritten twice (message-only,
tree-identical) before the PR was opened so that every commit passes
tools/check_commit_convention.py (allowed types, <=72-char headers,
lowercase subject start). Evidence files recorded head_sha values at RUN
time; those values refer to the OLD commits below. Trees are
byte-identical across the rewrite: git diff old_tip new_tip is empty.

- test(dogfood): add the ConPTY TUI driver and scenario matrix scaffolding
  old: 210388230bf82b27bbb78df48e7a153cbba67ba5 -> new: 18fde8ed51063fa9a02dfc074052019240898045
- test(dogfood): repair ConPTY harness after run-D classification
  old: 98c7cd3b5e2b4c28d3e99cdce0ee204abbeae9a7 -> new: c26dff25514e3f794523237165c4aefcb44609fc
- test(dogfood): run E and F physical evidence — 26/26 and 7/7
  old: 00e5021d7e1a4b3d37e8cbca02bb1a13f0b9fc4d -> new: aae74fcd38fac5768f88115cbc3d6992e8124f28
- docs(dogfood): stage-A report — TRANSPORT_DOGFOOD_PASS
  old: 7288aee0ac5058a0294c6a7573e677edbc9076d8 -> new: 477bd8c15f5cd3ed2b177a01921d210aa2de7852
- refactor(output): host-render-windows-conformance-corrective-1
  old: 104ffd491580434a6104521a4b7fde13037f4d60 -> new: 57d60b5479f5d0503c69e0933b792ee4ba915a7f
- test(dogfood): run G — Windows semantic-equivalence gate GREEN
  old: cbd59b18e9c178d9a2a76b6261d99e7a65f9c228 -> new: e031f5e16019da8068f5b8ac4787f9a970decc98
- fix(output): pin host-render teardown ownership ordering
  old: e32497ef4fe971b1c9d0a003e7d5e5bfdec01ee8 -> new: 3503f05d0bdc4ca0871656a1bb91682125f8266d
- docs(playback): close Stage-B audit minors
  old: 5ff9d271cbc9a689a1ac0dadb460c6faa3ebeb7c -> new: f2536d3ed8706e2519977ad40c5b5e2197653b3c
- feat(tui): truthful v1 shell closure with diagnostics and resize pins
  old: 0cc89d7eb1ae59c397a4e046b462ec5347605a44 -> new: 20da5a85883ff50e337b6d094952fd206f0c3f96
- docs(adr): record PBK-003 differential RESOLVED and tense-fix section 1
  old: 1e60105d886f9ebdd492718f0db3a922f31c5390 -> new: b5f42aec1ec5b20d8563fe4b6e1c2814b574f506
- test(dogfood): closure scenarios C6 to C17 and wide-grid driver fix
  old: 5f162f8e681fc4c9bf62acaffe8694df2ca561ea -> new: fb739cbb689aa05a552713412fd62ce85767e6e1
