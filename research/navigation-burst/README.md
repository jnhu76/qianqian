# research/navigation-burst — Navigation Burst Root Cause & Boundary 0

Campaign: `QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0`
Trigger: real listening defect — rapid `N`/`P` bursts (`NNNNN`,
`NNNPPNNP`, …) cause visible/audio stalls and the impression playback
temporarily loses sound.

```text
BASE_SHA:  ad4e09dc9064077b97cc6f26dd9139d81dae6e99 (origin/main, PR #169)
HEAD_SHA:  (this branch; see ENV files for the Windows-leg binary sha256s)
BRANCH:    research/navigation-burst-boundary-0
WORKTREE:  clean at campaign start; test-only + research-only deltas
```

HARD STOP honored: no production fix implemented. Deltas are test-only
instruments (I1 count matrix, I2 waterfall test), harness additions
(scenarios + additive marker capture in research/transport-dogfood), a
prototype crate outside every production path, and these reports.

## Reports

| File | Content |
|---|---|
| `R0-CURRENT-PATH.md` | source-linked N/P call graph, threads, blocking points, Q1–Q7 queue inventory, authority boundaries |
| `R1-BURST-EVIDENCE.md` | reproducer protocol, count tables (App layer + real Windows), queue-owner proof, **BURST_FULL_REPLACEMENT_CONFIRMED** |
| `R2-TRANSITION-WATERFALL.md` | manual N vs natural EOF decomposition; what dominates each |
| `R3-BOUNDARY-ADJUDICATION.md` | where intent/coalescing live, D13 adjudications, virtual cursor, precedence rules, minimum architecture |
| `R4-FUTURE-SOURCE-CACHE.md` | forward-compatible source/cache/prefetch study (no implementation) |
| `ADVERSARIAL-REVIEW-1.md` | fresh-context review against campaign §51's twenty questions |

## Evidence & instruments

```text
evidence/                     ENV identity files, per-scenario JSON /
                              transcripts / markers / raw streams (Windows
                              runs 1–2), Linux I1/I2 outputs, prototype table
tools/run-burst.sh            cross-build + stage + detached ConPTY run
tools/analyze_markers.py      count/timing analysis over the transcripts
prototype/                    quiet-window coalescing prototype (real App
                              commits, virtual-clock policy; never production)
```

Instruments (all allowed by campaign §1):
- **I1** test-only burst count matrix in `apps/headless/src/player.rs`
  tests (real App/kernel/session; mechanism doubles at the ports).
- **I2** test-only primitive waterfall
  `crates/qianqian-playback/tests/navigation_waterfall.rs`.
- **I3** burst scenarios in the existing ConPTY harness; additive
  timestamped marker capture; reuses the EXISTING off-by-default
  `QIANQIAN_AUDIO_LOG` mechanism lines. No production source changed.

## One-paragraph answer (§54)

Today `NNNPPNNP` executes **eight complete episode replacements** —
each raw key runs the full frozen D14.6 sequence: probe, old-episode
stop/settle/dispose, fresh K0 root with a real decoder open and a real
device open, then commit. Keys pressed while a replacement runs queue
in the OS console input buffer and are replayed one replacement each
(nothing in Qianqian coalesces them; `NP` costs two full replacements
even when it lands back on the playing track). Every intermediate
track reaches probe → decoder → PCM → device and commits, so the user
hears churn: each replacement is a real device open + episode swap
(a warm single replacement totals 85–92 ms; inside fast bursts the
AVERAGE per-replacement cost is ≈63 ms — e.g. five serialized
replacements completing 317 ms after the first key), plus
intermediate-track blips; the final target only becomes audible
after K serialized replacements (measured on the real Windows host). A single transition is dominated by decoder start +
device open (~81 of ~92 ms); the old tail is abandoned immediately on
manual navigation and fully drained (audibly) on natural EOF — both
correct as designed. What should change FIRST: coalesce relative
navigation intents (pending virtual target through a playlist pure
preview + quiet window, ~75–125 ms, event-loop-driven, App-owned — no
Plugin, no D14.6 amendment) so a burst causes exactly ONE final Open
and `NP` causes zero. What should NOT change yet: the replacement
sequence itself, PCM cutover (nothing earned), single-transition
latency work, networking/cache. The generic navigation logic belongs
in a UI-neutral intent seam into the App, so a future GUI reuses it.

## Stop gate (§52)

```text
BURST_ROOT_CAUSE:            RC-A INPUT BURST REPLAY (confirmed on both
                             hosts; one full replacement per raw key)
SINGLE_TRANSITION_ROOT_CAUSE: DECODER_START + OUTPUT_OPEN dominant
                             (~81/92 ms warm); old settlement minor;
                             PCM tail NOT a manual-stop factor
NAVIGATION_OWNER:            ReferencePlayerApp (App-owned pending target +
                             quiet window; UI-neutral intents from adapters;
                             playlist gains a pure from-position preview)
NAVIGATION_PLUGIN_ADMISSION: PLUGIN_NOT_EARNED (D13 table in R3)
PCM_CUTOVER_CHANGE:          NOT_EARNED (manual cut already abandons the
                             tail; invariant structural in whole-root
                             replacement)
FUTURE_SOURCE_BOUNDARY:      MediaRef → MediaSource → Seekable ByteSource
                             behind the EXISTING decode-provider boundary;
                             encoded-byte cache primary; intent upstream of
                             source/prefetch work (R4; design only)
CACHE_PLUGIN_ADMISSION:      DEFERRED (adjudicate when network exists; R4)
```

READY_FOR_ARCHITECTURE_REVIEW

STOP — no production fix PR, no performance campaign, no
networking/cache implementation follows this campaign.
