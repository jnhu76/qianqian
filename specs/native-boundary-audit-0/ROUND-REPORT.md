# QIANQIAN-NATIVE-AUDIT-AND-F1 — Round report

> STATUS: EVIDENCE + STATUS. This is the round-level report; the two
> parts' detailed evidence lives in `REPORT.md` (PART A) and in the F1
> PR (PART B). It is not architecture authority and promotes nothing.

```text
AUDIT_BASE_SHA    c3064782d68020b46bb2babcc8e80fd5c02c4367 (main at round start)

MERGED (in the round's order)
                  #131 corrective/native-boundary-songcore-1
                       head 5ae7d2b  -> merge 1714670
                  #132 corrective/native-boundary-wasapi-1
                       head e707291 + b9eeb55 (windows-compile-gate CI)
                       -> merge ae6502c
                  #133 phase-f/f1-cli-stop
                       head 86bb785 + 7268e7f (buffered_frames ruling,
                       gate-script async drains) -> merge 179cb24

FINAL_MAIN_SHA    179cb2424ed44591d141a9eae475ac34037bedb8
                  (= CLOSURE_MAIN_SHA; what the closure gate validated)

PRE-MERGE EVIDENCE TIP   gate/f1-integration 2a2b56b (local integration
                  of the round's branches; PRE-MERGE INTEGRATION
                  EVIDENCE ONLY, not final main)
HOSTS             WSL2 x86_64 (construction + Linux gates)
                  Windows x86_64 native, real Realtek endpoint (reality
                  and closure gates on a C:\qianqian-gate checkout
                  pinned to exact SHAs; no \\wsl.localhost build)
PRs               #131 songcore corrective      MERGED (1714670)
                  #132 wasapi correctives       MERGED (ae6502c)
                  #133 F1 stop seam             MERGED (179cb24)
                  #134 this report              (documentation)
```

## PART A — native boundary audit

Two boundaries audited against the in-repo normative contract
(`native/include/songcore.h`, ABI v1) and the real artifacts/devices. Full
case-by-case tables: `REPORT.md` §PART 1 / §PART 2.

| ID | Boundary | Class | Severity | Status |
|---|---|---|---|---|
| S1 | SongCore FFI | SAFETY-DEFECT — zero-length `&mut [u8]` from a null pointer (UB) | MAJOR | FIXED 5ae7d2b, Miri regression + negative control (re-verified CLEAN on nightly 1.100.0-nightly 2026-09-13, `-Zmiri-disable-isolation`) |
| S2 | SongCore FFI | REFINEMENT-GAP + DOC — `SONG_OK` with null handle carried forward | MINOR | FIXED 5ae7d2b |
| N1 | native impl | RESOURCE-LEAK, latent, unreachable from the wrapper | MINOR | DEFERRED (native rebuild) |
| S4 | native contract | DOC — header does not state handle movability | FYI | RECORDED |
| H1 | WASAPI | RESOURCE-LEAK — one kernel event handle per episode (`windows` HANDLE has no Drop) | MAJOR | FIXED 2ab25b9, 30-cycle real-device loop (+0 growth) |
| W1 | Windows repo | PRODUCTION-DEFECT — the WASAPI crate did not compile on Windows at all (3 sites) | MAJOR | FIXED 8c9ee91 + ec596f8 |
| C1 | WASAPI | REFINEMENT-GAP — `CoInitializeEx` not fail-closed | MINOR | FIXED 29475ed |
| RV-A1 | playback (F1) | REFINEMENT-GAP — a device abort with no stop request resolved as a user stop | MAJOR | FIXED 089316e, regression + negative control |
| RV-B1 | WASAPI | PRODUCTION-DEFECT — open-verdict mutex held across the render-thread join (deadlock on the timeout path) | MINOR | FIXED 9161d78 |
| RV-B2 | playback tests | TEST-DEFECT — vacuous stop-race assertion + unwitnessed empty-edge shape | MINOR | FIXED 86bb785 |
| RV-B3 | Windows tests | HYGIENE — Windows-only unused import | MINOR | FIXED e707291 |
| L1/L2 | repo | stale committed decode-crate `Cargo.lock`; out-of-workspace test warnings | FYI | RECORDED (not this round's cleanup) |

**Prevention for the W1 class: CLOSED via #132.** Every pre-existing
workflow ran on ubuntu-latest, so the cfg(windows) surface was never
compiled by a persistent gate. `.github/workflows/windows-compile-gate.yml`
(b9eeb55, in #132) now compiles all workspace targets on windows-latest
and runs the device-free playback/headless suites. Hosted Windows CI is
NOT real-device WASAPI evidence; runtime device tests stay in the
Windows-native gate.

**Formalization triage: NOT EARNED** for both boundaries. No new
independently-written temporal protocol exists; F1's new shared state
(stop intent + first-wins edge stop) introduced no second writer and no
new independently-legal collision, and the round's residual is
information loss / refinement (cause-carrying evidence), not an
unexplored state-space interaction. The interleavings that matter are
already covered by `specs/realtime-publication/` and the Campaign-1 loom
suite. No new Loom/TLA target.

## Windows evidence

### PRE-MERGE INTEGRATION EVIDENCE (gate/f1-integration 2a2b56b)

```text
cargo test --workspace (MSVC)          259 passed / 0 failed / 0 warnings
  incl. WASAPI handle suite 2/2        (+0 handle growth over 30 cycles)
gnu FFI consumer tests (mingw artifact) 16/16
physical binary (gnu release)           built at 2a2b56b
physical stop gate                      20/20
physical EOF gate                       10/10
```

CORRECTION (recorded at closure): this round's PRs originally reported
the integration-tip workspace suite as "275 passed". A clean
single-invocation recount at 2a2b56b during closure measured 259 passed /
0 failed — identical to final main. The 275 figure was an aggregation
artifact (the earlier command summed `test result` lines across several
cargo invocations in one shell command), not a property of the tree.
0 failed at both SHAs; no test was lost in any merge.

### POST-MERGE FINAL-MAIN CLOSURE EVIDENCE (GitHub main, 179cb24)

```text
FINAL_MAIN_WINDOWS_GATE
cargo test --workspace (MSVC)          259 passed / 0 failed / 0 warnings
cargo fmt --all -- --check             clean
cargo clippy --workspace --all-targets -- -D warnings   clean
WASAPI handle loop                     30-cycle canonical loop, +0 growth,
                                       no crash, no hang
gnu FFI consumer tests (mingw artifact) 16/16 (open/decode/EOF/close
                                       over the real artifact)
physical binary (gnu release)           built at 179cb24
physical stop gate                      5/5 — witness -> stop -> exit 0 ->
                                       'stopped before completion' -> quiet
                                       disposal
physical EOF gate                       3/3 — 'EOF: played out completely',
                                       quiet disposal
FINAL_MAIN_WINDOWS_GATE = PASS
```

The closure gate also consumed the hardened physical-gate script
(async stdout/stderr drains; 7268e7f) and the real-device host was the
same Realtek endpoint used all round.

## PART B — PHASE-F1-HEADLESS-STOP

**Seam decision: option B, session-owned control endpoint.** Option A
(composition withdrawal) is host-initiated teardown, not an in-episode
control, and resolves nothing to hand back; option C (an explicit
`PlaybackControl` capability) is the right long-term shape for
pause/seek/status but premature for one verb and would create a second
control authority. Under B the App holds `SessionCompletion`, calls
`request_stop()`, and the outcome is still decided solely by `resolve()`
from observed legs. `PlaybackControl` remains OPEN per ADR-PBK-002 §14.

**Transport T1:** the F0 CLI's frozen line parser with exactly one wired
verb; `stop` on stdin → `request_stop()`. The CLI never touches `PcmEdge`,
WASAPI, or SongCore.

**Stop semantics:** stop while playing → `Stopped` (both legs woken);
stop before binding → applied at bind, episode ends `Stopped`; stop during
activation failure → activation stays failed; stop × decode failure →
decode failure wins; stop × device abort → abort without a stop request is
`Failed{"device"}` (RV-A1); repeated stop → idempotent; stop after a
terminal → no-op.

**Firewall:** direct SongCore calls from headless = 0; direct WASAPI = 0;
headless pumps PCM = NO; per-block K0 work = 0.

**`buffered_frames()` seam ruling (closure):** it stays `pub` only
because its integration tests live outside the crate; the doc now states
explicitly that it is diagnostic mechanism evidence only — NOT
PlaybackState, NOT product semantic truth, NOT UI-facing authority — and
that F2 must re-admit or retire it (7268e7f). Lock discipline verified at
closure: `request_stop`/`bind_stop_target` never hold the completion
mutex across `edge.stop()`.

## Reviews (fresh context, all closed)

```text
Reviewer A        architecture/authority    REQUEST CHANGES -> resolved (RV-A1)
Reviewer B        lifecycle/concurrency     ACCEPT-WITH-NITS -> resolved (RV-B1..B3)
Closure reviewer  F1 closure (6 questions)  PASS — 5 FYI findings, no blocker
                                            (refreshed branch 7268e7f vs main)
```

Closure-reviewer FYIs are recorded as F2 input (see below): the module
doc should carry the residual-overlap note inline once a cause-carrying
verdict exists; `buffered_frames()` is the one completion→edge lock
nesting and stays inversion-free only because no reverse ordering exists;
`stop_requested()` deserves the same re-admit-or-retire ruling at F2;
the test double duplicates `EDGE_CAPACITY_FRAMES` deliberately; the gate
script's post-witness window relies on current output volume.

Residual findings are recorded, not hidden: a device abort racing a user
stop can still resolve `Stopped` (needs a cause-carrying drain verdict);
stop cannot shorten the post-EOF device flush; a wedged device call has no
Host-side escape yet; the zero-frame decode branch loops hot;
`SessionCompletion` is consume-once and unenforced.

## Merge order (executed)

```text
1. #131 corrective/native-boundary-songcore-1   -> 1714670
2. #132 corrective/native-boundary-wasapi-1     -> ae6502c
3. #133 phase-f/f1-cli-stop                     -> 179cb24
4. #134 specs/native-boundary-audit-0           (documentation)
```

The hard gate held: #133 was refreshed onto and merged after #132's
corrective was in main, so the event-handle path F1's docs claim against
is the corrected one.

## FINAL VERDICT

```text
NATIVE-BOUNDARY-AUDIT-0              PASS_WITH_CORRECTIVES — CLOSED
PHASE-F1-HEADLESS-STOP               EARNED — stop control seam in main
QIANQIAN-NATIVE-AUDIT-AND-F1         CLOSED — PASS_WITH_LIMITATIONS

- 4 findings at the base (3 MAJOR: S1 safety-UB, H1 handle leak, W1
  Windows gate breakage; 1 MINOR C1) fixed with regression or
  negative-control evidence in separate corrective branches;
- the review rounds added 1 MAJOR + 3 MINOR, all fixed with evidence;
- final GitHub main revalidated on real Windows hardware: 259/0/0
  workspace, fmt/clippy clean, 30-cycle handle loop +0 growth,
  FFI 16/16, physical stop 5/5, physical EOF 3/3;
- W1 prevention closed (windows-compile-gate CI in #132);
- formalization NOT EARNED (recorded above).

LIMITATIONS (explicit, carried into F2):
  1. device abort × user stop causal ambiguity (cause information is
     lost before semantic resolution)
  2. a wedged device call has no Host-side escape
  3. stop cannot shorten the post-EOF hardware drain
  4. zero-frame decode branch loops hot
  5. SessionCompletion consume-once per episode, unenforced
  6. no ASan/UBSan native closure
  7. no unplug / default-device-switch / sleep-wake gate
  8. no hours-scale soak
```

## NEXT AUTHORIZED INPUTS — F2

Questions only; no F2 implementation happened in this round or in this
PR.

```text
- cause-carrying device/drain evidence: can the drain verdict carry the
  abort cause so a device abort racing a user stop cannot resolve
  `Stopped`?
- observable playback state: what is the PlaybackState contract F2
  owes (and which of buffered_frames / stop_requested must be
  re-admitted or retired under it)?
- wedged native-call observability: is there a Host-side escape or at
  least a diagnostic for a device call that never returns?
- zero-frame progress semantics: should a decoder that produces no
  frames yield or fail instead of looping hot?
- SessionCompletion one-episode consumption: enforce or document the
  consume-once contract?
```
