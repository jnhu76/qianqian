# QIANQIAN-NATIVE-AUDIT-AND-F1 — Round report

> STATUS: EVIDENCE + STATUS. This is the round-level report; the two
> parts' detailed evidence lives in `REPORT.md` (PART A) and in the F1
> PR (PART B). It is not architecture authority and promotes nothing.

```text
AUDIT_BASE_SHA    c3064782d68020b46bb2babcc8e80fd5c02c4367 (main)
BRANCH TIPS       corrective/native-boundary-songcore-1   5ae7d2b
                  corrective/native-boundary-wasapi-1     e707291
                  phase-f/f1-cli-stop                     86bb785
                  specs/native-boundary-audit-0            (this branch)
                  gate/f1-integration                      2a2b56b
                  (local merged state; evidence only, not a deliverable)
HOSTS             WSL2 x86_64 (construction + Linux gates)
                  Windows x86_64 native, real Realtek endpoint (reality gate
                  on a C:\qianqian-gate checkout pinned to the integration SHA)
PRs               #131 songcore corrective      DRAFT, MERGED: NO
                  #132 wasapi correctives       DRAFT, MERGED: NO
                  #133 F1 stop seam             DRAFT, MERGED: NO
                  #134 this report              DRAFT, MERGED: NO
```

## PART A — native boundary audit

Two boundaries audited against the in-repo normative contract
(`native/include/songcore.h`, ABI v1) and the real artifacts/devices. Full
case-by-case tables: `REPORT.md` §PART 1 / §PART 2.

| ID | Boundary | Class | Severity | Status |
|---|---|---|---|---|
| S1 | SongCore FFI | SAFETY-DEFECT — zero-length `&mut [u8]` from a null pointer (UB) | MAJOR | FIXED 5ae7d2b, Miri regression + negative control |
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

**Formalization triage: NOT EARNED** for both boundaries. F1's new shared
state (stop intent + first-wins edge stop) introduced no second writer and
no new independently-legal collision; the interleavings that matter are
already covered by `specs/realtime-publication/` and the Campaign-1 loom
suite. No new Loom/TLA target.

**Windows reality gate (final merged tip 2a2b56b, real device):**

```text
cargo test --workspace (MSVC)          275 passed / 0 failed / 0 warnings
  incl. WASAPI handle suite 2/2        (+0 handle growth over 30 cycles)
gnu FFI consumer tests (mingw artifact) 16/16
physical binary (gnu release)           built at 2a2b56b
physical stop gate                      20/20
physical EOF gate                       10/10
```

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

## Reviews (fresh context, both closed)

```text
Reviewer A  architecture/authority      REQUEST CHANGES -> resolved (RV-A1)
Reviewer B  lifecycle/concurrency       ACCEPT-WITH-NITS -> resolved (RV-B1..B3)
```

Residual findings are recorded, not hidden: a device abort racing a user
stop can still resolve `Stopped` (needs a cause-carrying drain verdict);
stop cannot shorten the post-EOF device flush; a wedged device call has no
Host-side escape yet; the zero-frame decode branch loops hot;
`SessionCompletion` is consume-once and unenforced.

## Merge order

```text
1. #131 corrective/native-boundary-songcore-1
2. #132 corrective/native-boundary-wasapi-1
3. #133 phase-f/f1-cli-stop
   (#134 documentation, independent)
```

Taking #133 without #132 ships the pre-corrective event-handle path, which
is what F1's module docs claim against.

## FINAL VERDICT

```text
PART A (native boundary audit)      PASS_WITH_CORRECTIVES
PART B (F1 stop seam)               PASS — construction complete, both
                                    review lenses closed with evidence
ROUND                               READY_FOR_REVIEW
- 4 findings at the base (3 MAJOR: S1 safety-UB, H1 handle leak, W1
  Windows gate breakage; 1 MINOR C1) found and fixed with regression or
  negative-control evidence in separate corrective branches;
- the review round added 1 MAJOR + 3 MINOR, all fixed with evidence;
- Windows reality gate green on the merged tip (275/0/0, FFI 16/16,
  stop 20/20, EOF 10/10) including the real-device handle-leak loop;
- MERGED: NO — all four PRs are drafts awaiting human review, in the
  stated merge order.
```
