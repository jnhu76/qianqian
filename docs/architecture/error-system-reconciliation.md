# Error-system directed evidence and A/B reconciliation (#231, #280)

> **Truth class: INVESTIGATION RECORD / DESIGN INPUT. NOT AUTHORITY.**
> **Evidence anchor (frozen read tree):** `SOURCE_BASE_SHA = 5e60da4c3bd5f0fa93b4215670f4e898e2ac844e`
> (= PR #280 HEAD at round start = `INITIAL_DOCUMENT_HEAD_SHA`; production sources byte-identical
> to `main@524c6efeeabcfc94ceaf8e9cf1392e88861288c3`).
> **Inputs:** #281 (scan A: forward census), #282 (scan B: reverse tracing), #231 audit
> `issuecomment-6187764428` + erratum `issuecomment-6188419040`, PR #280 body/comments,
> the cited ADRs/architecture docs, and the frozen sources themselves.
> **Method:** directed re-verification of named gaps only (no new blanket census); every
> load-bearing conclusion re-anchored to `file:line` at the frozen SHA. Zero execution:
> no test, binary, device or fault-injection run. Companion documents:
> [target contracts](error-system-target-contract.md),
> [migration/acceptance](error-system-migration.md), [R0 contract](error-disposition-contract.md).

> **Status (2026-10-17) — document authority vs implementation state, kept separate:**
>
> ```text
> DOCUMENT_ROLE                     = frozen directed-evidence / A-B reconciliation record
>                                     (unified boundary ledger, GAP re-verification,
>                                     NEEDS_RUNTIME_EVIDENCE registry)
> DOCUMENT_AUTHORITY                = UNCHANGED: INVESTIGATION_RECORD / NOT_NORMATIVE_AUTHORITY
>                                     (the unified ledger in §4 remains the current fact
>                                     authority for the frozen baseline it describes)
> ORIGINAL_BASELINE                 = 5e60da4c frozen read tree; every file:line anchor in
>                                     the body is that tree and was NOT rewritten
> CURRENT_IMPLEMENTATION_REFERENCE  = the target designs this record fed are implemented
>                                     in defined scope by PR #287 (merged b86a9ce
>                                     2026-10-16); index = error-system-implementation-record.md
> CURRENT_ACCEPTANCE_BOUNDARY       = executed evidence Linux-only; Windows runtime
>                                     acceptance pending #288; the NEEDS_RUNTIME_EVIDENCE
>                                     registry rows keep their own carriers (#288/#289/#290)
> RESOLVED_BY_OWNER_RULING         = D-2/D-10 (RATIFIED 2026-10-17; decision-record §6;
>                                     receipt [PR #291 comment 6229433676](https://github.com/jnhu76/qianqian/pull/291#issuecomment-6229433676))
> UNRESOLVED_OWNER_DECISIONS        = D-4/D-5/D-7/D-9/D-11 unchanged; D-C #289; D-D #290
> ```
>
> Round closeout: [#283](https://github.com/jnhu76/qianqian/issues/283#issuecomment-6228758527).

This document answers four questions the two scans left open or conflicting:

1. What actually happens when a latched (pending) SongCore fatal error interleaves with the
   next legal operation (GAP-01).
2. What each worker's `catch_unwind` really covers, and what thread exit, resource release,
   terminal commit and `join` each independently guarantee (GAP-02).
3. A point-by-point ledger of host-facing output, resolving the A/B conflict about when
   reports run relative to disposal (GAP-03).
4. What the formal models, tests and historical experiments actually prove versus merely
   pin as text (GAP-04), plus the secondary reads (GAP-05).

It then publishes the unified boundary ledger, the A/B adjudication table with errata,
and the registry of claims that need runtime evidence. Original scans #281/#282 are NOT
edited; the errata live here.

---

## 0. Version record

```text
SOURCE_BASE_SHA           = 5e60da4c3bd5f0fa93b4215670f4e898e2ac844e  (frozen read tree)
INITIAL_DOCUMENT_HEAD_SHA = 5e60da4c3bd5f0fa93b4215670f4e898e2ac844e  (PR #280 HEAD at round start)
PR-head-vs-base delta     = PR branch == SOURCE_BASE_SHA; no re-basing of conclusions needed.
                            (PR branch adds only docs/architecture/error-disposition-contract.md
                             on top of main@524c6ef; production code identical.)
READ_TREE                 = isolated worktree, clean, detached at SOURCE_BASE_SHA
WRITE_SCOPE               = docs/architecture/{error-disposition-contract.md (edit),
                            error-system-reconciliation.md, error-system-target-contract.md,
                            error-system-migration.md} on the PR #280 branch
EXECUTION                 = NONE (no test/build/device/fault-injection run; all "test exists"
                            statements are code-existence readings of the frozen tree)
R1.1 (2026-10-15)         = static adversarial audit of 847e9d1 returned REVISE with eight
                            findings; all eight re-verified against the frozen tree and
                            accepted. Corrections in this file: §2.3 rows 1/5/6 + new
                            §2.3.1 (stop × pending-fatal is a commit race, not stop-determinism;
                            audibility split), §2.4 DAG annotation, N-01 condition narrowing
                            (write-failure vs full-pipe modes; four preconditions), ledger #9
                            consequence fix (Opened verdict publishes BEFORE the print),
                            E-1/E-5/E-6 redesigns and E-7 two-tier split. Companion files carry
                            T1-3/T1-6/T2-5/T3-1/T5-2/T5-4/T5-5/T5-8, D-3/D-5/D-9/D-10 and
                            migration §1.3/§1.4/§1.5/§2/§3/§4 + new §1.11. SOURCE_BASE_SHA
                            unchanged; still zero execution.
R1.2 (2026-10-16)         = second static audit of 4bbe2fe returned REVISE with six findings;
                            all six re-verified and accepted. Corrections: §2.3.1 stop-exit
                            list rebuilt from actual control flow (canceled pending seek is
                            release-only bookkeeping, NOT a worker exit — session.rs:613-618 has
                            no return; the zero-frame arm is unreachable in this window with the
                            real provider because C:1111 returns the latch before any zero-frame
                            OK; genuine stop-exits are the mid-write capacity-wait terminal
                            observation and the preserved-remainder flush break), audibility
                            vocabulary extended (ReleaseBuffer can fail after the pull,
                            wasapi.rs:721-723 — the error system asserts NO audibility level),
                            E-1 attribution decoupled from message text (pending wording =
                            deferred reporting, not fault identity), E-7 tier 2 narrowed to
                            diagnostic outlets with new E-9 for protocol-write conformance,
                            queue-cost claims corrected against the pinned toolchain's std
                            (bounded sync_channel documents a pre-allocated fixed buffer —
                            rustc 1.97.1 std/src/sync/mpsc.rs; no per-record-allocation or
                            crossbeam-advantage verdict is asserted), T5-8 chain-to-previous
                            made co-satisfiable with the isolation goal.
R1.3 (2026-10-16)         = third audit returned APPROVE_DESIGN_CANDIDATE with one
                            non-blocking P3; applied: T2-2 lock-hold wording fixed
                            (notify runs AFTER the guard is dropped, completion.rs:1246-1247 —
                            the earlier "notify in one lock hold" was wrong), E-9 gained the
                            three-mode stdout checkpoint (print panic / flush-Err — the
                            explicit flush result is ignored at machine_input.rs:232, an
                            unconstructed checkpoint, not a proven defect / blocking).
                            Owner instruction executed: the four documents renamed to stable
                            names without round suffixes (error-disposition-contract.md,
                            error-system-{reconciliation,target-contract,migration}.md); all
                            in-file links and document-shorthand references updated
                            ("R0 §"/"R1 §" → "disposition-contract §"/"reconciliation §");
                            round labels (R0/R1/R1.1/R1.2/R1.3) survive only as history in
                            version records and §6. Approval is design-candidate level only:
                            it authorizes no implementation and no merge; the OPEN items
                            (D-2..D-11, exporter exit mechanism, hook isolation, teardown
                            panic posture) remain owner-gated entry points, not resolved by
                            any green gate.
REVIEW ATTRIBUTION NOTE   = PR #280's "independent Astra review" is recorded by the owner's
                            PR comment of 2026-10-15 (issue comment, verifiable) as a static
                            assessment shared off-GitHub at 3c866cfa. The reviewer's identity
                            and independence are NOT verifiable from repository records.
                            This round therefore cites it as "the prior static review
                            (owner-attributed to Astra; identity unverified)" and asserts no
                            independence beyond the owner's record.
```

Evidence labels: `CODE_FACT` (read directly at the frozen SHA), `AUTHORITY` (accepted
normative text), `CANDIDATE` (PR #280 text, non-authority), `STATIC_INFERENCE` (stated
inference chain), `EXISTING_TEST_EVIDENCE` (in-tree test source, not executed),
`RUNTIME_EVIDENCE_PRIOR` (#231 E-series / experiment artifacts, not re-run),
`NEEDS_RUNTIME_EVIDENCE` (statically unprovable; experiment designed in §8).

---

## 1. Unified boundary ledger

Unified IDs (`U-xx`) cluster the two scans' boundaries. Both source ID sets are preserved;
neither scan is renumbered. Where a scan has no entry, the cell says `—`.

| U | Cluster | Scan A | Scan B | Relation |
|---|---|---|---|---|
| U1 | Host byte I/O callback (`file_read/seek/size` ↔ `io_read/io_seek`) | B-01 | B-T3(ingress 1), F13 | SAME |
| U2 | `song_open` (no-diagnostic ABI contract) | B-02 | B-P1(2) | SAME |
| U3 | `song_probe` (diagnostic available, not read) | B-03 | B-P1(3) | SAME (F2=L3) |
| U4 | `song_read_pcm` incl. partial-success / latched fatal | B-04 | B-T3(ingress 1) + cross-cut 1 | SAME + **re-adjudicated reachability (§2)** |
| U5 | `song_last_error` borrowed diagnostic | B-05 | B-L5 | SAME |
| U6 | `song_seek` + three-class `classify_seek_status` | B-06 | B-S2/S4 | SAME |
| U7 | `song_close` / endpoint Drop | B-07 | B-W1(drop) | SAME |
| U8 | Capability resolve / `open_media` | B-08 | B-A2 | SAME |
| U9 | Decode worker loop / staging | B-09 | B-W1 | SAME |
| U10 | DSP staging / live-update | B-10 | B-D1..D3 | SAME |
| U11 | Decode-worker panic translation | B-11 | B-W2 | SAME |
| U12 | PcmEdge data plane (first-wins terminal, `PcmPull` collapse) | B-12 | cross-cut 3 | SAME |
| U13 | Seek protocol (record→park→provider→purge→commit→rebase) | B-13 | B-S1/S3 | SAME |
| U14 | Render open verdict handshake (10 s) + open abort | B-14 | B-O1/O2 | SAME (§5.4 adds late-verdict detail) |
| U15 | Render steady-loop failures / drain / volume | B-15 | B-O3/O4, B-T5, B-D4/D5 | SAME + **NEW post-catch gap (§3, N-01)** |
| U16 | Render-thread panic translation | B-16 | B-W2 | SAME |
| U17 | Render leg terminal publication (`render_input.stop` → `drain.complete`) | B-17 | B-T1 | SAME |
| U18 | DrainSignal observer seam | B-18 | B-F8 | SAME |
| U19 | D11 resolve / settlement | B-19 | B-T1..T5, DAG-1 | SAME |
| U20 | Worker exit funnel (`worker_exited` → stranded-seek → observation close/join) | B-20 | B-W1 | SAME + **catch-coverage correction (§3)** |
| U21 | Stop intent routing | B-21 | B-T2 | SAME |
| U22 | Activation chain + `activation_failed` | B-22 | B-A2 | SAME |
| U23 | Teardown relations (LIFO, join, Discharge) | B-23 | B-V1 | SAME |
| U24 | Observation plane (pure reads) | B-24 | B-T6 | SAME |
| U25 | DSP command refusal (`last_refusal`) | B-25 | B-D2 | SAME |
| U26 | Open preflight probe | B-26 | B-P1/P2 | SAME |
| U27 | Open replacement sequence / fail-stop | B-27 | B-A3 | SAME |
| U28 | EOF policy (U2 Completed-only) | B-28 | B-U3 | SAME |
| U29 | K0 verdicts / §G.6 latch / `revise_desired` | B-29 | B-A1, B-V1 | SAME (§6.3 adds assembly-visibility candidate) |
| U30 | Stdin reader / HostFailure / seal | B-30 | B-W3 | SAME |
| U31 | Process exit codes | B-31 | B-U4 | SAME |
| U32 | Diagnostic outlets (stdout protocol / stderr / TUI) | B-32 | B-L1/L3, B-U1/U2 | **CONFLICT adjudicated in §4** |
| U33 | Render gate (pause/seek park) | B-33 | DAG-4 | SAME |
| U34 | FFmpeg av_log suppression | B-34 | B-L4 | SAME |
| U35 | `song_select_stream` declared-unused ABI | B-35 | — | A-only (verified: 0 Rust callers) |
| U36 | Panic default hook as ungated stderr outlet | A F-08 | B-L2/F7 | SAME |
| U37 | Discarded `join` results | A F-13 | B-W1 | **CONFLICT adjudicated in §3** |
| U38 | Post-catch publication cone (stop/complete outside every catch) | — (implied by F-06) | — (F5 covers blocking only) | **NEW_BOUNDARY (§3, N-01/N-02)** |
| U39 | Assembly-time §G.6 visibility (`revise_desired` swallows Blocked) | A F-09 | — | A-only, confirmed (§6.3) |
| U40 | main-thread uncought panic → exit 101 bypass | A F-07 | — | A-only, confirmed (§3.5) |

Findings cross-map: A F-01..F-05 ≈ B F1..F4 (same four information losses, same numbering
offset); A F-04 ≈ B B-O4/U5-unknown (drain cap); A F-06 ≈ B F5/F6 (stderr hazards); A F-07
≈ B F5/F6 + main-thread part A-only; A F-08 ≈ B F7; A F-09 = U39 (A-only); A F-13 = U37;
A F-14 (seek clears latched fatal) — **corrected by §2**; A F-15 ≈ B F15; A F-16 ≈ B (assembly
Debug formatting, unverified by B — kept A-only); A F-17 (absolute paths) ≈ B (not covered);
A F-18 ≈ B F11 (e2e gap — adjudicated §7.1); A F-19 ≈ B F11/F12/F13 (zero-test matrix).
B-only: F8 (observer panic), F9=F10(A)-family exit compression, F10 (text-pinning tension),
F16 (SEEK_UNSUPPORTED product consequence), U1-U9 unknowns.

---

## 2. GAP-01 — latched fatal vs the next operation (interaction matrix)

### 2.1 The ABI's actual promise (AUTHORITY + CODE_FACT)

`songcore.h` (frozen tree, partial-success clause):

> "Partial-success: if a decode error occurs after some frames were produced in the current
> call, the call returns SONG_OK with those frames and **the error is surfaced by the NEXT
> call** (with zero frames produced)."

`native/src/songcore_ffmpeg.c`:

- `song_read_pcm` entry re-publishes the latch: `fatal_error != SONG_OK` →
  `set_error(h, h->fatal_error, h->last_native, "pending decode error")` (C:1111-1113).
- The failing iteration latches `h->fatal_error = st` and writes the one-shot diagnostic
  "decode error" (C:1138-1139); `produced > 0 → return ok(h)` clears only the diagnostic
  struct, never the latch (C:1155-1156 → `clear_error` C:133-138; the latch fields are
  separate handle state, C:82-83).
- `reset_decode_state` clears the latch: `h->fatal_error = SONG_OK; h->last_native = 0;`
  (C:255-256). Its only production callers are `song_open` (C:1034) and the **success path
  of `song_seek`** (C:1196), i.e. after `av_seek_frame` returned ≥ 0 and before the landing
  frame is decoded.

**Scope discipline.** The ABI promises surfacing on the next `song_read_pcm`. No sentence
extends that to "any next operation must fail". Neither scan asserted that extension; it is
recorded here as a non-claim so later rounds do not invent it.

### 2.2 Worker-side program order (CODE_FACT)

`session.rs::decode_worker` loop order is: loop-top seek pickup (:601-605) → serialization
point / provider seek (:612-775) → preserved-remainder flush (:786-804) → DSP live-update
pickup (:820-824) → `read_frames` (:827) → write (:869-883). Two facts matter:

- The seek pickup and the provider seek run **before** the next `read_frames` in program
  order, so a seek recorded while a fatal is latched can execute before the read that would
  have surfaced it.
- The interruptible write observes the seek slot mid-block (`write_observing_seek`,
  :933-944): a seek recorded during the partial-success block's write stops the write,
  preserves the remainder, and the next loop-top runs the serialization point — the seek
  still precedes any new read.

Eligibility at pickup: `seek_aborted()` false, `leg_parked_evidence()` true, and
`edge.terminal() == Open` (:613, :619, :639). In the partial-success state the edge **is
Open** — the latched fatal has published nothing to the session; `decode_failed` has not
run; the render leg is alive. The eligibility checks do NOT exclude this state.

### 2.3 Interaction matrix

Legend — **A** = proven current behavior (static, code-anchored); **B** = statically
reachable ordinary schedule, not executed this round; **C** = candidate target-contract
choice (belongs to the target-contract document, not current behavior).

| Next operation after a partial success (fatal latched, N frames delivered) | Path | Effect on the latched fatal | Episode outcome | Class |
|---|---|---|---|---|
| Next `read_frames` (default) | C:1111-1113 → adapter `_` arm (lib.rs:288-291) → `decode_failed("… pending decode error")` (session.rs:889-893) | surfaced, then consumed by the failure | `Failed{decode: … pending decode error}`; no rollback. Audibility (R1.2): only frames the render leg pulled past the edge and successfully submitted to the device CAN be heard — and even those may be lost to a stop or device failure before consumption; frames merely admitted to the edge are abandoned outright (render-side terminal-before-data, edge.rs:233-239). The error system asserts no audibility level (§2.3.1 vocabulary) | A |
| `request_seek`, worker picks it up, `song_seek` **succeeds** (`Applied`) | C:1177 `av_seek_frame` ≥ 0 → C:1195-1196 flush + `reset_decode_state` → **latch cleared**; worker: remainder discarded, `edge.invalidate()` purge, landing published (session.rs:683-694) | **erased without ever surfacing** | episode continues from the new position; the decode anomaly in the replaced region never reaches D11 or any diagnostic | A (mechanism) / B (schedule observed only statically) |
| `request_seek`, provider returns `RefusedUnchanged` | `classify_seek_status` (lib.rs:254-260): only `SONG_ERR_INVALID_ARGUMENT` classifies so; `fail_arg` (C:1166) overwrites the diagnostic but not the latch; worker resumes old cursor (session.rs:644-655) | retained (diagnostic overwritten) | next read surfaces it → `Failed` as row 1. Note: from the production adapter `INVALID_ARGUMENT` is unreachable (`Duration` targets are ≥ 0; handle non-null) — the class is contract-live via mocks/ABI controls, production-dead | A (mapping) / B (schedule) |
| `request_seek`, provider returns `MutatedThenFailed` | `fail()` overwrites diagnostic; latch retained but moot; worker: `release_seek_without_commit` + `decode_failed("seek failed: …")` + `edge.fail()` (session.rs:656-667) | superseded by the seek failure | `Failed{decode: seek failed: …}` (D14.5 conservative routing) | A |
| `request_stop` | `request_stop` records intent, releases gate, `edge.stop()` (completion.rs:532-562) — **it commits no outcome itself** | depends on worker position (§2.3.1) | **either** `Stopped` **or** `Failed{decode: pending decode error}` — both legal; in the default position (worker at/after loop-top with edge space; a canceled pending seek is NOT an exit — the release at :613-618 is bookkeeping and control falls through to the read) the next read surfaces the latch and `worker_failure` wins the commit race (completion.rs:1262 checks it first, before any stop-intent consult); `Stopped` results only from a genuine stop-exit: the mid-write capacity-wait terminal observation (`WriteStep::Stopped`, :985-988) or the preserved-remainder flush break (:798-802) — see §2.3.1 for the full control-flow enumeration | A (both arms code-derived; R1.2 corrects the R1.1 exit list) |
| Teardown / replacement / quit | retire path records stop first (`request_stop` + `wait_terminal`, player.rs:723-743) → waits for whichever terminal commits | as row 5 | old episode settles `Stopped` **or** `Failed{pending decode error}` per the same commit race; replacement proceeds on the committed terminal either way | A (R1.1) |
| Pause / resume / volume / DSP-refusal | no provider call; worker keeps producing | retained | later read surfaces it (row 1) unless a seek/stop intervenes first | A |
| EOF | unreachable with a latched fatal: `song_read_pcm` returns the fatal before any EOF branch (C:1111-1113 precedes; C:1157-1158 ordering) | — | — | A |

### 2.3.1 Commit ordering, not command ordering (R1.1 correction; exit list rebuilt in R1.2)

`first-wins` (completion.rs:1238-1249) is a property of **terminal commit**, not of
command arrival. `request_stop` publishes no outcome: it records `stop_requested`,
releases the gate, and stops the edge. Which terminal an episode in the pending-fatal
window takes is decided by where the decode worker is when the stop lands. The exit
inventory below is rebuilt from the actual control flow of the worker closure —
**canceling a seek, stopping a write, and exiting the worker are three different
things**, and only the third produces `worker_exited`:

- **Default position (loop-top reached, edge has space, no in-flight write or
  remainder — the state after the partial-success block's write completed):** nothing
  checks the edge terminal between loop-top and `read_frames`, so the next read runs
  regardless of the stop, re-publishes the latch (C:1111-1113), returns `Err`, and the
  Err arm calls `decode_failed` unconditionally (session.rs:889-892) → `worker_failure`
  → the resolver's FIRST rule commits **Failed** (completion.rs:1262 precedes every
  stop-intent consult). A stop intent recorded earlier cannot relabel it. In this
  window **the pending fatal beats the stop**, the opposite of the pre-R1.1 wording.
- **Canceled pending seek is NOT an exit (R1.2):** the loop-top `seek_aborted` branch
  executes only `release_seek_without_commit(); pending_seek = None;`
  (session.rs:613-618) — no `return`, no `continue`. Control falls through the
  remainder flush (none), the DSP pickup, and INTO `read_frames`: the latch surfaces
  and the episode commits **Failed** exactly as the default position. The R1.1 list
  wrongly counted this as a stop-exit.
- **Genuine stop-exits (worker returns without another read):**
  (a) **mid-block write, capacity wait:** `write_observing_seek` returns
  `WriteStep::Stopped` only when a write made zero progress AND the edge terminal is
  no longer Open (:985-988); the caller returns (:877) → `worker_exited(Stopped)`;
  (b) **preserved-remainder flush:** the refused-seek tail breaks out of its flush
  loop on a terminal (:798-802) and returns → `Stopped` (the latch is still retained
  here — a RefusedUnchanged seek never cleared it — and it is erased by the exit
  without surfacing).
- **Provider-general exits that are UNREACHABLE in this window with the real
  provider (R1.2):** the `Frames(0)` arm's terminal check (:831-834) can never observe
  a stop caused by a pending fatal — `song_read_pcm` returns the latched error at
  entry (C:1111-1113) before any zero-frame success can occur — and the EOF arm
  likewise never runs. Both remain live for mock providers and other states, which is
  why the production worker keeps them.
- **In-flight-seek returns, for completeness:** the edge-terminal returns inside the
  Applied cutover wait (:718-728, :746-764) do exit the worker, but by then the
  provider seek has already completed — an `Applied` outcome has already cleared the
  latch (C:1196), so those exits belong to row 2's "erased" story, not to the
  pending-fatal race; a `RefusedUnchanged` outcome reaches exit (b) through its
  remainder.

Every cell of this matrix is therefore bounded by four phases: (1) read start,
(2) error publication (`decode_failed`), (3) stop-intent recording, (4) terminal
commit. A claim about which signal "wins" is well-formed only relative to a phase
ordering; acceptance designs must observe the phases (E-1), not just the final
outcome class.

**Evidence-level vocabulary for delivered PCM (R1.2, replacing the R1.1 audibility
split):** "decoded" (produced by the C loop) → "admitted to the edge" (write
completed) → "pulled from the edge" (render leg copied it out) → "submitted to the
device" (`ReleaseBuffer` returned OK — it CAN fail after a successful pull,
wasapi.rs:721-723, aborting the episode) → "mechanism-reported consumed" (padding
observations) → "physically audible". The error system owns evidence only up to the
fifth level and **asserts no audibility**: even successfully submitted frames can be
lost to a stop or device invalidation before consumption. Statements in this package
claim at most "submitted, may have been heard".

### 2.4 Execution DAG (pending-fatal window)

```text
[worker] read_frames → song_read_pcm
    decode fails mid-loop → latch fatal_error + last_native (C:1138)
    produced = N > 0 → return ok(h) → diagnostic cleared, LATCH SURVIVES (C:1156)
    adapter: Ok(Frames(N)) — no failure published (lib.rs:286)
  → stage DSP block → write N frames to edge (consumers may hear them)
  → loop top ──┬── seek command pending ∧ leg parked ∧ edge Open
               │     → decode_stream.seek (session.rs:643)
               │       ├─ Applied  → C reset_decode_state → LATCH ERASED (C:1196)
               │       │             → invalidate/purge → continue (no D11 trace)
               │       └─ Mutated/Refused → rows 3/4 above
               └── no seek → read_frames → C:1111-1113 "pending decode error"
                     → Err → decode_failed → edge.fail → resolve Failed (DAG-A/1)
  [any time] stop/teardown → edge.stop → outcome depends on the worker's actual
        exit surface (§2.3.1): a canceled pending seek is NOT an exit (falls through
        to the read → Failed); genuine stop-exits are the mid-write capacity-wait
        terminal observation and the preserved-remainder flush break → Stopped
```

### 2.5 Adjudication

- **Scan A's F-14 ("production-unreachable") is retracted.** Its premise — "edge already
  Failed → seek no longer called (session.rs:639)" — covers only the post-`edge.fail`
  state. In the partial-success state the edge is Open and the eligibility checks pass;
  the seek-before-next-read order is the worker's ordinary program order. Static
  reachability is proven; a runtime observation remains `NEEDS_RUNTIME_EVIDENCE` (§8 E-1).
- **The seek-erases-latch behavior is NOT adjudicated a defect.** The latched fatal belongs
  to the replaced continuation; D14.5 `Applied` discards the old world by design, and the
  ABI never promises surfacing except on the next read. Candidate target-contract
  obligations (diagnostic witness of a superseded fatal, wording) are C-class items
  recorded in [target contracts §M3](error-system-target-contract.md); choosing one is
  an owner decision, not a correction.
- **"下次 read 报错" was not extended by either scan.** The matrix is the complete
  operation set; no other operation surfaces the latch.

---

## 3. GAP-02 — catch coverage, thread exit, cleanup, join (per worker)

### 3.1 Decode worker (`session.rs::decode_worker`)

```text
thread entry
  bind_observation_reader()            :589  OUTSIDE catch; completion lock .expect (poison-sensitive)
  observation.spawn_worker()           :590  OUTSIDE catch; spawn failure → Ok(None) → close(); no panic
  catch_unwind (                       :591  COVERS: staging alloc, pending_seek/remainder, full loop
     loop-top seek pickup / provider seek / remainder flush / DSP pickup / read / write)
  catch arm: decode_failed + edge.fail :897-910  INSIDE nothing — itself uncaught;
                                        both take locks via .expect (poison-sensitive)
  exit funnel (OUTSIDE every catch):
    worker_exited(edge.terminal())     :921  publish worker_terminal + worker_gone under one lock hold
    abort_stranded_seek()              :922  seek-slot lock .expect + gate release
    observation.close()                :927  poison-TOLERANT (observation.rs:268-270 into_inner)
    analyst.join() (result discarded)  :929  analyst body wrapped in its own catch (observation.rs:233)
```

Resource release: the decode endpoint (`Box<dyn DecodedPcmStream>`) is moved into the
thread closure and drops during unwind — normal or caught — so `song_close` runs on worker
exit paths. The funnel's `expect`s are the only poison-sensitive steps between a caught
panic and `worker_gone`.

### 3.2 Render thread (`wasapi.rs::run_render_thread`)

```text
thread entry
  catch_unwind #1 (:251)  COVERS: CoInitializeEx guard, open_session (verdict publication),
                          ComApartment/DeviceSession drops (run during unwind, wasapi.rs:299,317-319),
                          catch_unwind #2
  catch_unwind #2 (:320)  COVERS: steady_loop only; Err → Aborted{"render loop panicked"}
  post-catch publication cone (OUTSIDE every catch):
    eprintln! (QIANQIAN_AUDIO_LOG gated)  :261   → N-01 below
    render_input.stop()                   :266   edge lock .expect → N-02 below
    drain.complete(verdict)               :270-273  verdict stored first-wins under the drain lock,
                                                    observer invoked OUTSIDE the verdict lock (ports.rs:135-155)
```

**N-01 (NEW_BOUNDARY, this round; conditions narrowed in R1.1).** The gated abort line at
`wasapi.rs:261` sits after both catches. Two distinct sink-failure modes must not be
conflated at this line:

- **Write failure (e.g. broken pipe) → panic → publication skipped.** `eprintln!` panics
  on a write *error*; nothing catches it; `render_input.stop()` and `drain.complete()`
  are **skipped**. All four preconditions are required: (i) `QIANQIAN_AUDIO_LOG`
  enabled; (ii) the stderr write *fails* — a full pipe does NOT panic, it blocks (next
  bullet); (iii) no other stop/failure publisher has already settled the episode (a
  committed outcome makes the skipped `drain.complete` inert — `publish_evidence` is
  first-wins and late evidence is dropped); (iv) the hang itself: `wait_terminal`
  (which the machine path runs BEFORE dispose, entry.rs:418-419) then finds no
  committed terminal, and the decode worker wedges on a full edge with no data-plane
  stop — the process hangs before it could clean up or report.
- **Full pipe → block → publication delayed (B F5's case, same line).** A full stderr
  blocks the `eprintln!`; `render_input.stop()`/`drain.complete()` eventually run, so
  the consequence is unbounded *delay* of wakeup/drain publication, not a skip. This
  mode is remediable only by removing the synchronous write (T5-2/T5-3 framework), not
  by a non-panicking writer.

Scan A's F-07 covered the volume-apply print (inside `steady_loop`, hence caught and
escalated to `Aborted`); scan B's F5 covered blocking of the same line. The
panic-skips-publication case at :261 is in neither scan. The panic hook's own stderr
write and its chain-to-previous (which restores the default hook's synchronous stderr
for every non-reader thread) share the blocking mode (ledger #11) — no platform bound
is claimed for it (T5-8). Class: mechanism CODE_FACT; reachability requires the gated
log plus the matching sink condition (`STATIC_INFERENCE`, not executed).

**N-02 (structural, no identified trigger).** If the edge lock was poisoned (a prior panic
while holding it — no production point identified today), post-catch `render_input.stop()`
panics on `.expect("pcm edge lock")` (edge.rs:90) with the same skip-the-rest consequence.
Same family as B-F8; recorded for completeness.

Observer semantics: `drain.complete` stores the verdict BEFORE invoking the observer and
releases the verdict lock first (ports.rs:136-151). An observer panic therefore loses only
the completion **mirror**, not the stored verdict — later `complete` calls are first-wins
no-ops, so the mirror never re-runs (B-F8 mechanism confirmed verbatim). In the natural-EOF
case with nothing else decisive the episode never settles; B's "no identified trigger"
assessment stands (the observer body is `publish_evidence`; its only panic entry is a
poisoned completion lock).

### 3.3 Stdin reader / analyst (for completeness)

- Reader: `catch_unwind` around dispatch (:119), around the whole reader (:253), around
  each OS read (:277); per-thread panic-hook silencing via a process-global hook swap that
  chains to the previous hook for every non-`qianqian-stdin` thread and is deliberately
  never restored (:192-207). Reader write failures (`print!` on stdout status responses,
  :230-233) panic into the catch → `HostFailure::Panic` → `request_stop` if unsettled.
- Analyst: own `catch_unwind` (observation.rs:233); its join result is telemetry
  (`session.rs:929`).

### 3.4 K0 teardown and the main thread

- `run_unwind` (kernel.rs:713-742) has **no panic isolation**: a teardown inverse that
  panics (e.g. `edge.stop()` on a poisoned lock) propagates out of `dispose_root` into the
  caller on the main thread; remaining LIFO inverses never run; `Violated` is only produced
  by an inverse explicitly returning `Discharge::Violated` — never by a panic and never by
  a hanging join. The session's two relation inverses return `Discharged` unconditionally
  (session.rs:435, :447).
- The main thread has **zero** `catch_unwind` (grep: entry.rs 0, player.rs 0). Any panic on
  the main thread — report printing (entry.rs:400 `playing {file}` runs before
  `finish_episode`), player logic, a panicking teardown inverse — unwinds out of `main`:
  process exit 101, skipping `wait_terminal`, dispose, seal, reports and the exit-code
  table (A F-07 main-thread scenario confirmed).
- `stop_and_join` (`wasapi.rs:206-211`) and the worker join (`session.rs:446`) discard
  join results (`let _ =`). **A discarded join still waits** — "ignored result" ≠ "no
  waiting". What is lost is the thread's death cause: a join `Err` is unobservable today.

### 3.5 Adjudication of the A/B claims

- **"内部 catch 使 join Err 不可达" (A F-13) is UNSUPPORTED and corrected.** The catches are
  bounded regions, not thread-wide: pre-catch initialization (`bind_observation_reader`),
  the catch arm itself, and the entire exit funnel — plus the render thread's whole
  post-catch publication cone — run **outside** every catch and take poison-sensitive
  locks. Join `Err` is therefore structurally reachable (lock-poisoning cascade), although
  no first-panic production trigger is identified today (B-F8's "no identified trigger"
  stands, now with the precise coverage map above). The accurate statement is:
  *worker panics inside the loop are translated; panics in the funnel/post-catch cone are
  not; join `Err` is possible, unobservable, and its discard does not affect waiting.*
- **"忽略 join 返回值" ≠ "没有等待线程结束"** — both scans got this right; recorded once
  here because target-contract wording must preserve the distinction (module 4).
- **Terminal / thread exit / resource release / join / discharge are five independent
  facts.** Code-enforced relations: terminal commit is first-wins and owner-published
  (completion.rs:1238-1249); `worker_gone` publication precedes stranded-seek cleanup
  (:1036-1071); device release happens during thread unwind (ComApartment/DeviceSession
  drops, wasapi.rs:281, 317-319) so *for the render thread* a completed join implies the
  device was released (stop_and_join doc :198-201); for the decode worker, endpoint drop
  happens in-thread, so join implies `song_close` ran; but neither join implies the
  terminal committed, and the terminal does not imply any join happened. `dispose_root`
  returns `TeardownViolated` only from a latched explicit `Violated` (kernel.rs:219-227) —
  a hang produces a hang, not a violation (B B-V1 confirmed).

---

## 4. GAP-03 — host output ledger (point-by-point)

Full sweep performed over production code (tests excluded; noted where load-bearing).
Gating keys: `QIANQIAN_AUDIO_LOG` (wasapi mechanism lines), `QIANQIAN_FFMPEG_LOG`
(av_log restore). "Broken-pipe" = Rust `println!/print!/eprintln!` panic on write error
(std semantics; no `panic=abort`, no SIGPIPE handling in either scan's build-config check,
re-confirmed: no `[profile]` in root Cargo.toml, `.cargo/config.toml` linker only).

| # | Site | Thread | Lifecycle stage | Locks held | Purpose | Write fails / blocks |
|---|---|---|---|---|---|---|
| 1 | entry.rs:70,74,102-103 (usage/version/arg error) | main | startup, pre-episode | none | CLI feedback | panic → exit 101 bypasses nothing (no episode yet); blocks → startup delay |
| 2 | entry.rs:236-238 `scanning {} ...` + explicit flush | main | playlist scan | none | CLI feedback | panic → exit 101 pre-episode; explicit flush isolates scan line ordering |
| 3 | entry.rs:243,252,269,271,279,285,294 (open refused / activation report / fail-stop / shell failure / startup disposal warnings) | main | open phase, before or after old-episode teardown per DAG-2 | none (post-decision reads) | machine/CLI feedback | panic → exit 101; in the failure-clean path the attempted root is disposed only after the report (entry.rs:380-381) — **the NotEstablished activation report precedes `dispose()`**; `QianqianApp` has **no Drop teardown** (qianqian-app lib.rs:10-11), so a skipped dispose leaks the root without any inverse running |
| 4 | entry.rs:395-400 `source:` + `playing {} ...` | main | after activation, BEFORE reader spawn (:403) and `finish_episode` (:418) | none | machine protocol stdout rows (`playing {file}` is the F1 physical gate's activation witness, f1-physical-gate.ps1:38) | panic → exit 101 skipping wait_terminal/dispose/seal (A's main-thread case, confirmed; no-Drop fact above applies) |
| 5 | machine_input.rs:230-233 status responses (`print!`+flush stdout; `eprintln!` stderr) | stdin reader | live episode, per command (main thread is inside `wait_terminal`) | none (`Operation` is "NEVER a mutex guard", machine_input.rs:150) | **machine protocol stdout** | panic → caught (:119 dispatch catch) → `HostFailure::Panic` → request_stop → episode terminated by a diagnostic write (A confirmed; a *query* kills playback); **full pipe → reader stuck inside its admitted operation → `seal()` waits forever** (machine_input.rs:84-100; module doc :4 warns "an active output effect can delay seal") — process exit blocked, NEEDS_RUNTIME_EVIDENCE for pipe-capacity reachability (§8 E-7) |
| 6 | machine.rs pure renderers (outcome_report / activation_failure_report / disposal warnings / StartFailure) via entry.rs:259-260,447-451 | main | AFTER wait_terminal + dispose + seal (finish_episode order :418-438) | none | transport report rows (protocol stdout / stderr) | panic → exit 101 skipping later reports only (episode already disposed); blocks → exit delay |
| 7 | entry.rs:380 NotEstablished activation report | main | before attempted-root dispose (:381) | none | transport report | panic → exit 101; attempted root leaks (no dispose) |
| 8 | wasapi.rs:261 `[qianqian-wasapi] render aborted` | render | post-catch, BEFORE stop/complete | none | gated diagnostic | **blocks → delays wakeup/drain publication (B F5)**; **panics → skips stop+complete → worker wedge + wait_terminal hang (N-01)** |
| 9 | wasapi.rs:538-547 open line | render | open phase, after verdict, before Start | none | gated diagnostic | **R1.1 correction** (the `Opened` verdict publishes at :534, BEFORE this print): panic → caught by catch #1 → `Aborted{"render thread panicked"}` while activation has already reported success — the post-catch cone then re-prints (#8) on the same sink: if it still fails, the N-01 hang cascade follows; if it recovered, the episode aborts post-activation and settles `Failed{"device"}` (no stop intent) with a diagnostic that misattributes a logging failure to the device. blocks → delays first Start (playback onset; activation result unaffected) |
| 10 | wasapi.rs:678-686 volume recoverable failure | render | steady loop | none | gated + latched diagnostic | panic → caught by catch #2 → recoverable failure escalated to `Aborted` → `Failed{"device"}` (A confirmed); blocks → render-loop stall → device underrun (B F6) |
| 11 | panic default hook (all threads except machine-transport reader, which swaps it) | any | any panic | whatever the panicking site held | ungated stderr | blocking → delays catch translation (B F7); hook write itself is std-synced stderr |
| 12 | songcore_ffmpeg.c av_log callback (:50-61) | FFmpeg-calling threads | any SongCore call | none | default **empty callback** (chatter discarded); `QIANQIAN_FFMPEG_LOG` restores default logger per `song_open` (:822) | restored-logger write failures are FFmpeg-internal; default path writes nothing |
| 13 | C product code printf/fprintf/perror | — | — | — | **zero hits** (grep) | — |
| 14 | TUI (state.rs:397-401,528-537; dispatch/audio views) | TUI runtime | live/terminal display | none | display interpolation only; zero direct `print!` in TUI production (both scans agree; spot-checked) | render-layer failures are TUI-internal, outside this error system |

**Adjudication of the A/B conflict:**

- B's B-L3 sentence "report rows are all rendered after dispose/seal, no locks held" is
  **true only for the finish_episode transport rows (#6) and false as a universal**:
  the `source:`/`playing` rows (#4) precede the episode's end; per-command status rows
  (#5) run mid-episode on the reader thread; the NotEstablished activation report (#7)
  precedes the attempted root's dispose. A's corresponding objections (#4/#5/#7, plus the
  render-thread in-loop prints #8-#10) are confirmed point-by-point.
- **Protocol vs diagnostic separation is real and must survive any logging work:**
  machine stdout rows (#4,#5,#6-stdout) are the script contract; the stderr diagnostic
  lines (#8-#11) and av_log (#12) are loss-bearing diagnostics. A machine-protocol write
  failure is a host-operation failure (today: reader panic terminates the episode);
  a diagnostic sink failure must be isolated as diagnostic loss, never allowed to
  upgrade/terminate the episode (today's #10 panic path violates exactly this — it is a
  known hazard, not a design).
- No output point holds the completion lock, the edge lock, or a device call while
  printing — the interference class is *thread-progress* (blocking/panic), not lock-order
  deadlock. (`render_order_oracle` pins that position publication precedes GetBuffer; none
  of the print sites is between them.)

---

## 5. GAP-04 — what the formal / test / experiment evidence actually proves

### 5.1 Terminal decision table (semantic freeze; stage text only via a fail-closed verifier)

- `decision_table_oracle.rs` drives the real resolver over the full 48-row domain and
  freezes the **outcome class** (`undecided|completed|stopped|failed-decode|failed-device`).
  The TLA table collapses both failed classes to `"Failed"` (`EpisodeTerminalSettlementTable.tla`
  `OutcomeClassOf`); **no machine-checked TLA content references stage text or Rust
  identifiers** (those appear in comments only).
- The only stage-text coupling is the verifier's `classify` (`starts_with("decode")` /
  `== "device"` / else `panic!`), deliberate and fail-closed, self-described:
  "`Failed.stage` naming is realization detail: a stage outside the two frozen names fails
  the test rather than being silently absorbed."
- Therefore: changing a stage **spelling within the frozen families** ("decode: …") breaks
  nothing formal; changing the **class marker** ("device") or introducing an unfrozen
  stage trips the Rust gate first; regeneration (`QIANQIAN_UPDATE_DECISION_TABLE=1`) plus a
  TLC rerun (`specs/check.sh terminal`) is required — **a migration obligation, not an
  authority violation** (AI-2/D14.2: diagnostic stage text is not frozen semantics).
  (Minor drift noted: `EpisodeTerminalSettlementTable.tla:11-13` cites the oracle's
  pre-F2 path — comment-only.)

### 5.2 render_order_oracle (source witness, not semantics)

P1-P13 pattern-match `include_str!("wasapi.rs")` byte offsets against raw code anchors and
declare themselves "a REGRESSION PIN, not a semantic proof". Moving/reordering the
error-path code fails the matching P-items loudly. It proves the shipped text keeps the
order D14.8's derivation assumes; it never executes the render loop. Classification:
**textual witness** — useful, honest, and *not* evidence of runtime behavior.

### 5.3 Behavioral oracles that DO freeze semantics (on doubles, not real boundaries)

seek_seam:645 bit-exact refused-seek continuation (content equality against a no-seek
control); :752 negative control proving the freeze oracle non-vacuous; :681 destructive-seek
never-resume witnessed producer-side; stop_seam:279 the "a stop nobody requested is not a
stop" discriminator pinning `failure_diagnostic == Some("device")` exactly; teardown
gate-precondition witness (:226/:276 — release-before-join ownership). These freeze
session semantics on scripted providers/outputs; the real-boundary versions are the
gap below.

### 5.4 Real-boundary evidence inventory

- `decode_endpoint_tests.rs` (workspace-excluded crate): real SongCore success identity
  (frame counts, full-drain PCM SHA-256 vs an independent reference manifest, :45/:73);
  ABI refusal classes (:377 NOT_OPEN→destructive; :399 INVALID_ARGUMENT continuation
  bit-identical to a never-seeked control). The truncated-file test (:137) states its own
  goal — "end deterministically — a clean EOF or a typed decode error — never a crash or a
  hang" — so accepting either terminal is **within its declared scope**; it pins
  termination, not fidelity, and does not exclude the `Completed` outcome (L13's risk
  shape). Not a mis-used semantic oracle; a deliberately weak liveness smoke.
- Experiments (read in full): S-PROBE (real probe/open/close concurrency with a live
  episode; success + refused-open paths only), f6-open-smoke (real D14.6 replacement;
  refused-open only; found "the real binary never compiled with `--features playback`
  under the default test battery"), V-PROBE (the **only real WASAPI failure-signal
  evidence in the tree** — E_INVALIDARG / NOT_INITIALIZED; device-loss class NOT
  physically triggered), songcore-call-comparison (both-language ABI parity on the
  success path), F5 E1-E3 (real `song_seek` latency + INVALID_ARGUMENT post-state;
  SEEK_ERROR/SEEK_UNSUPPORTED post-states NOT exercisable on the corpus; E2 is a protocol
  copy, not the production edge).
- #231 E-series (RUNTIME_EVIDENCE_PRIOR, artifact-bound to `b7a526c` with a recorded
  transferability argument to `524c6ef`): the only **real SongCore decode failure → D11
  Failed** observations that exist (E3/E7), the open-stage I/O failures with preserved OS
  codes (E9/E10), and the truncation pair E7/E8 proving neither terminal is an integrity
  witness. No mid-stream device failure was ever physically injected.
- Composition entry: the real provider/host chain exists only behind headless
  `--features playback` (`assembly.rs:24-37 establish`, feature-gated; a compile-time
  type check at :295-296 is the feature's only in-tree test touch). No osmoke binary in
  the tree; physical gates live in `apps/headless/tests/scripts/f1-physical-gate.ps1` and
  the experiment records.

### 5.5 Text-assertion census and the "semantic contract" question

- Production control flow branching on diagnostic text: **zero** (re-grepped; the only
  text-branching production code is `status.rs:141-160 forbidden_status_claim`, a
  vocabulary-policing negative control consumed by tests, and the verifier-only
  `decision_table_oracle::classify`). CE6 stands.
- Presentation tests pinning exact text: the census in the migration document lists them;
  every one is an `assert!` in test code (gain origin-truthfulness, seek route prefixes,
  `"device"` discriminator, machine report lines, hook-chaining witness). They are
  **presentation tests** (wording change ⇒ test update, no semantics) except the two
  verifier items above (fail-closed classifier = verifier-side freeze; render_order = source
  witness).
- **Adjudication:** "精确文字测试自动形成稳定语义契约" is UNSUPPORTED. The semantic freeze
  is (i) the decision-table class structure (TLA-coupled) and (ii) the named behavioral
  oracles of §5.3. Exact-text assertions create **migration cost** and two *deliberate*
  verifier-side text pins; none of them converts wording into authority. The counterpart
  truth (B F10) stands: the "presentation may change freely" promise is bounded in
  practice by the census list.

---

## 6. GAP-05 — secondary reads (scope-limited conclusions)

1. **D14.7 pause teardown wake (AUTHORITY).** Frozen mechanism requirement: "stop (and
    teardown) MUST wake every parked participant with bounded latency"; park sits before
    GetBuffer, no device buffer held across a park; teardown order `release_pause_gate`
    before `stop_and_join` is the session relation's documented precondition
    (session.rs:423-431). No new boundary: the wake obligation is owned by the existing
    gate; the target contract only has to preserve it. (The unbounded-join UNKNOWN —
    worker stuck in a native call — is a different mechanism, recorded in §8 E-4.)
2. **DSP live-update (AUTHORITY, dsp-product-model §7.3).** Refusal (invalid data/compile
    failure) keeps the old configuration and records `last_refusal` — never a processing
    failure, never a partial configuration; post-acceptance failure routes through the
    D11 processing class with no partial-world resurrection. Worker pickup matches
    (session.rs:820-824, :849-860). No new boundary; the existing three-way (refuse /
    apply-whole / fail-whole) is complete for the admitted four operation classes.
3. **K0 §G.6 + `revise_desired` (CODE_FACT + AUTHORITY).** `revise_desired`
    (qianqian-app lib.rs:81-85) returns `Ok(())` after `set_desired()?; settle();` even
    when settle latched §G.6 Blocked; the violation surfaces only at `dispose()` or via
    the snapshot. A-F-09 confirmed. This is an App-root presentation question, not a K0
    semantic one — the candidate (surface the blocked state as an assembly-time operation
    result) is narrow and touches only the App root; recorded as a target-contract item,
    owner decision.
4. **Open handshake late/failed results (CODE_FACT).** `open_stream` waits 10 s
    (`wasapi.rs:148-181`); `abort_render_thread` = `gate.close_and_release` →
    `render_input.stop` → join, with the release-before-join order documented as the
    deadlock-avoidance core (open_abort.rs:28-42; the verdict mutex is never held across
    join, wasapi.rs:154-161). A late successful verdict after abort: the thread's own
    DeviceSession drops on the render thread; the activation fails with the timeout
    message while the real (late-success) outcome is unreachable to the session — the
    timeout error text is honest ("did not reach a verdict in time") but the true cause
    class is `NEEDS_RUNTIME_EVIDENCE` (§8 E-5). A panic inside `open_session` yields no
    verdict → full 10 s wait → timeout-shaped activation failure; the panic line prints
    only under the audio log (#9 above).

---

## 7. A/B adjudication table (errata delivered here; #281/#282 untouched)

Relation labels: SAME / COMPLEMENTARY / CONFLICT / UNSUPPORTED (over-claim) /
NEW_BOUNDARY / DESIGN_DECISION.

| # | Claim | Source | Verdict | Correction (anchored) |
|---|---|---|---|---|
| C-1 | "真实 SongCore→D11 端到端结构性不可能" | A F-18 / MIGRATION §2.2 | UNSUPPORTED (over-claim) | The structural facts are real (workspace exclude keeps binding crates out of `--workspace` runs; `qianqian-playback` does not depend on the decode crate; the `playback` feature has no tests). "Impossible" is not: the same headless feature already path-depends on the real chain, and a test-side dev-dependency or feature-gated test binds it without touching production dependency direction (B HANDOFF's own observation). Corrected claim: **no automated end-to-end exists today; a narrow entry is constructible; actually running it is NEEDS_RUNTIME_EVIDENCE (§8 E-2).** |
| C-2 | "内部 catch 使 join Err 不可达" | A F-13 | UNSUPPORTED | §3.5: catch regions are bounded; funnel and post-catch cone are outside them and poison-sensitive. Join `Err` structurally reachable, unobservable (results discarded), and discarded-join still waits. |
| C-3 | "pending fatal 清除在生产不可达" | A F-14 | CONFLICT → corrected | §2: reachable via partial-success + seek-Applied before the next read; premise ("edge already Failed") false in that state. Not adjudicated a defect (§2.5). |
| C-4 | "日志只延迟、不可能改变结果" | B F5/F6/F7 summary | UNSUPPORTED (blocking-only truth) | True for **blocking**; false for **write-failure panics**: reader print panic terminates the episode (machine_input.rs:119/:253); volume print panic escalates a recoverable failure to `Aborted`→`Failed{"device"}` (wasapi.rs:320-327); main-thread report panic exits 101 past dispose. Plus N-01: the post-catch abort-line panic skips publication entirely. |
| C-5 | "所有宿主报告都在清理后" | B B-L3 | UNSUPPORTED (universal) | §4 ledger: true for finish_episode transport rows only; `source:`/`playing` and per-command status rows run mid-episode; the NotEstablished activation report precedes dispose. |
| C-6 | "人类/TUI 不是具体消费者" | A MIGRATION §5 (disposition-contract §5.1 reading) | DESIGN_DECISION | Code fact: the only consumers of `failure_diagnostic`/`activation_error` are presentation surfaces (machine rows, TUI, quit report); zero control-flow consumers. Whether presentation consumers satisfy disposition-contract §5.1's "concrete consumer" bar is an owner design ruling, not a code fact. |
| C-7 | "精确文字测试自动形成稳定语义契约" | (implicit in A's oracle list) | UNSUPPORTED | §5.5: semantic freezes are the decision-table class structure (TLA-coupled) + the §5.3 behavioral oracles; stage text couples only through the fail-closed verifier `classify`; TLA is stage-agnostic; exact-text tests are presentation tests / migration cost. |
| C-8 | "drain 超时证明设备损坏 / 误判" | A F-04 (second half) | SPLIT | Proven current behavior: deadline → `Aborted` → with worker `Eof`/`Failed` → `Failed{"device"}` via the intent-blind `Some(_)` arm (completion.rs:1306-1310) — even a recorded stop intent cannot relabel it; only the `Stopped`-terminal arm consults intent. "Proves device damage" is false (the cap is declared local tuning, wasapi.rs:746-747). "误判" is DESIGN_DECISION inside the authority-declared OPEN resolver-precedence space (AI-7; ADR-PBK-002:751-754) — an owner ruling, not a correction. Related nuance: a stop during the EOF-drain window that still drains in time resolves **Completed** (the `Drained` branch never consults intent; stop-into-EOF is a late command, D14.4-consistent). |
| C-9 | "SEEK_UNSUPPORTED 返回证明旧续段未变" | (question raised in review) | UNSUPPORTED (as a provability claim) | C-side, a failing `song_seek` returns before flush/reset (C:1178-1194), so SongCore's own state is untouched; whether `av_seek_frame` already mutated demuxer internals is unproven without FFmpeg sources. D14.5's destructive routing for SEEK_UNSUPPORTED is a frozen authority choice listed by name; reclassifying requires an authority amendment plus FFmpeg-level evidence (§8 E-6). B F14's "authority-conservative, not a defect" stands. |
| C-10 | Observer-panic verdict loss (B F8) | B | COMPLEMENTARY (confirmed + narrowed) | §3.2: verdict is stored before the observer runs; only the completion mirror is lost; trigger requires a poisoned completion/observer lock — no production trigger identified. Stays a defensive-structure note, not a defect. |
| C-11 | N-01 post-catch abort-line panic | — (neither scan) | NEW_BOUNDARY | §3.2/N-01 with the wait_terminal-hangs-before-dispose consequence chain. |
| C-12 | Open-fail path also mirrors `drain_verdict=Aborted` into completion (B B-O1) | B | SAME (verified, harmless) | No worker exists → never decisive; observation does not expose `drain_verdict`. |

Claims NOT corroborated by agreement alone: every CONFLICT/UNSUPPORTED row above was
re-derived from the frozen tree (the relevant sections cite the re-verification), not
accepted because both scans or neither scan said so.

---

## 8. NEEDS_RUNTIME_EVIDENCE registry (with falsifiable experiment designs)

Static analysis cannot settle these; each names the observation that would expose the
answer. None is claimed as run, passed, or failed.

| E | Question | Minimal design (owner-gated; not authorized by this PR) | Falsification condition |
|---|---|---|---|
| E-1 | Pending-fatal × seek-Applied interleave under a real scheduler | Test-side (no production change): real SongCore provider + a ONE-SHOT scripted decode fault at a known pre-cut offset, landing in a verified-healthy post-cut region. **R1.2 attribution rules — the message text is never the identity:** the "pending decode error" wording only means DEFERRED REPORTING (a partial-success wrapper around any fault — old or new — produces it on the next read, C:1104-1158), so it cannot distinguish the pre-seek fault from a post-seek one. Attribution rests on three preconditions, each independently witnessed: (i) the injection record proves the old latch existed (the harness observed the fault fire pre-cut and the partial delivery that deferred it); (ii) the Applied verdict is observed (seek return + landing); (iii) the post-cut region is controlled-healthy (verified on the no-injection control run), so any post-Applied failure is anomalous and gets investigated, not auto-classified. Under those preconditions: the FIRST read after the Applied landing must succeed — a failure there falsifies row 2 (the old latch survived `reset_decode_state`); message signatures are corroboration only. The stop-race half (§2.3.1) is a separate observable: a stop issued inside the window must produce `Stopped` or `Failed{pending decode error}` with the worker's actual exit surface recorded (default position / mid-write / remainder-flush), never an assertion that stop alone forces `Stopped` | A failure of the first post-Applied read (with preconditions (i)-(iii) all witnessed) falsifies §2.3 row 2; a later failure under the same preconditions is an anomaly to investigate, not a row-2 verdict |
| E-2 | Real SongCore error → D11 end-to-end | Feature-gated test (dev-dep on `qianqian-decode-songcore` in the test target only) driving `establish` with a real native handle against a corrupted fixture; assert the Failed terminal + stage family | A green run proves the chain binds; any stage-family mismatch falsifies the assumed passthrough |
| E-3 | Mid-read host I/O failure (U1) | A seekable fault-injecting source behind the existing io callback seam in a binding-crate test (the callback closure is currently fixed to `File::read` — the test lives in the excluded crate; the same harness serves the T1-6 userdata-context candidate, migration §1.11) | If SongCore surfaces the read failure as anything other than latched 103/`pending`/`decode error` family, the C mapping table is wrong |
| E-4 | Worker stuck in an uninterruptible native call | Filesystem-backed hang injection (e.g. FUSE or a stuck NFS mount) during steady decode; observe dispose/`wait_terminal` blocking and the absence of any Violated/timeout | Any automatic escape (timeout, Violated) would falsify "dispose has no escape" |
| E-5 | Slow/normal devices vs DRAIN_CAP=5 s | Instrumented drain timing on real endpoints (healthy + Bluetooth-class suspended sink). **R1.1 rewording:** the experiment produces the cap-exceedance datum; it does not adjudicate the classification. A healthy device exceeding 5 s shows the cap is exceeded in practice; whether settling that episode `Failed{device}` is a misclassification remains the D-4 owner ruling (C-8's DESIGN_DECISION half, inside the authority-declared OPEN resolver-precedence space) — that verdict belongs to the target contract, not to the measurement | All tested devices draining < 5 s downgrades the tuning risk to unobserved; an exceedance alone proves only the exceedance |
| E-6 | SEEK_UNSUPPORTED post-state | Real unseekable container (raw ADTS) + seek attempt + bit-exact continuation probe at the ABI layer (extends F5 E1's method to the unmeasured statuses). **R1.1 scope obligations:** state the result's coverage with the result — container/format family, FFmpeg closure version, sample set; a bit-identical continuation on the tested corpus establishes the provability fact FOR THAT CORPUS only. Class-level reclassification additionally needs either broader coverage or an FFmpeg-internal state argument (what `av_seek_frame` touches on an index-less demuxer before failing); one sample never proves the class | A non-identical continuation on any tested sample keeps the conservative routing; an identical result does not by itself amend D14.5 (migration §3.2 preconditions) |
| E-7 | N-01 / stderr hazards under blocked & broken sinks — **DIAGNOSTIC outlets only** | **R1.1 two-tier split; R1.2 scope: tier 2 covers diagnostic sinks (#8/#9/#10/#11, av_log #12 when restored) — the protocol outlet #5 is NOT a member (its write failure is a legitimate host operation failure by T5-7; see E-9).** Tier 1 (writer-swap corrective, sites #8/#9/#10): broken-sink negative control per site × {gate on, gate off} — terminal class, exit path and wakeup ordering identical to the sink-healthy control (N-01's write-failure mode impossible by construction); plus a blocked-sink control that documents the RESIDUAL blocking hazard (publication still delayed — expected and recorded as residual until the T5 framework lands; it is not an isolation pass). Tier 2 (future framework adoption): the four disposition-contract §6 controls (blocked, stopped consumer, queue full, write failure) per diagnostic outlet, asserting producer/terminal independence | Tier 1: any terminal/exit-path change keyed on sink state, or the residual blocking presented as isolation. Tier 2: any observed semantic change (episode class, exit path beyond the documented ones) marks the corresponding outlet a defect rather than a hazard |
| E-9 | Protocol-write failure conformance (#5, R1.2 — separated from the logging-inertness matrix) | Broken/closed stdout and stderr harnesses around the reader's status-response writes (machine_input.rs:230-233), per stream: assert the DOCUMENTED host-failure path, not terminal invariance — write panic → caught (:119) → `HostFailure::Panic` recorded (`first_failure`, :156-163) → conditional `request_stop` (only if no terminal is already committed, :160-162) → episode terminates, `seal()` obligations still met; a full stdout pipe is the blocking variant (reader stuck inside its admitted operation; seal delay documented, ledger #5). The two-class split of migration §1.7 (protocol = host-operation failure, visible; diagnostic = loss-only) is the acceptance object. **R1.3 variants (audit checkpoint):** stdout failure must be observed as THREE separately-recorded modes — print panic (write error inside `print!`), flush returning `Err`, and blocking. The explicit `let _ = io::stdout().flush()` at :232 currently IGNORES the flush result; an independent flush failure under real status output has not been constructed, so the flush-Err mode is a recorded checkpoint, not a proven defect. Each mode's observed outcome must land in the documented classes | Any behavior OUTSIDE the documented classes (e.g. a protocol-write panic escaping the catch; a diagnostic-style silent-drop being applied to protocol rows; seal proceeding while the reader is unacknowledged; a flush-`Err` whose handling diverges from the print-panic class without an owner ruling) falsifies the classification |
| E-8 | Broken-pipe exit-101 reachability on the main thread | Pipe-closed stdout/stderr harness around the binary at each report site (#1-#7) | Confirms A's F-07 main-thread scenario end-to-end; absence would falsify the std-panic premise on this platform |

---

## 9. What this round did NOT do

No production, test, build or ABI change; no ADR status change; no experiment executed;
no recovery policy adopted; #277 untouched; #281/#282 not edited; PR #280 not merged.
All candidate designs live in the companion documents and are marked CANDIDATE.
