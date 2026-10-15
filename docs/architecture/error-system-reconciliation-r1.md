# Error-system directed evidence and A/B reconciliation — R1 companion (#231, #280)

> **Truth class: INVESTIGATION RECORD / DESIGN INPUT. NOT AUTHORITY. NOT IMPLEMENTED.**
> **Evidence anchor (frozen read tree):** `SOURCE_BASE_SHA = 5e60da4c3bd5f0fa93b4215670f4e898e2ac844e`
> (= PR #280 HEAD at round start = `INITIAL_DOCUMENT_HEAD_SHA`; production sources byte-identical
> to `main@524c6efeeabcfc94ceaf8e9cf1392e88861288c3`).
> **Inputs:** #281 (scan A: forward census), #282 (scan B: reverse tracing), #231 audit
> `issuecomment-6187764428` + erratum `issuecomment-6188419040`, PR #280 body/comments,
> the cited ADRs/architecture docs, and the frozen sources themselves.
> **Method:** directed re-verification of named gaps only (no new blanket census); every
> load-bearing conclusion re-anchored to `file:line` at the frozen SHA. Zero execution:
> no test, binary, device or fault-injection run. Companion documents:
> [target contracts](error-system-target-contract-r1.md),
> [migration/acceptance](error-system-migration-r1.md), [R0 contract](error-disposition-contract-r0.md).

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
                            (PR branch adds only docs/architecture/error-disposition-contract-r0.md
                             on top of main@524c6ef; production code identical.)
READ_TREE                 = isolated worktree, clean, detached at SOURCE_BASE_SHA
WRITE_SCOPE               = docs/architecture/{error-disposition-contract-r0.md (edit),
                            error-system-reconciliation-r1.md, error-system-target-contract-r1.md,
                            error-system-migration-r1.md} on the PR #280 branch
EXECUTION                 = NONE (no test/build/device/fault-injection run; all "test exists"
                            statements are code-existence readings of the frozen tree)
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
| Next `read_frames` (default) | C:1111-1113 → adapter `_` arm (lib.rs:288-291) → `decode_failed("… pending decode error")` (session.rs:889-893) | surfaced, then consumed by the failure | `Failed{decode: … pending decode error}`; already-delivered N frames stay in the edge and are heard; no rollback | A |
| `request_seek`, worker picks it up, `song_seek` **succeeds** (`Applied`) | C:1177 `av_seek_frame` ≥ 0 → C:1195-1196 flush + `reset_decode_state` → **latch cleared**; worker: remainder discarded, `edge.invalidate()` purge, landing published (session.rs:683-694) | **erased without ever surfacing** | episode continues from the new position; the decode anomaly in the replaced region never reaches D11 or any diagnostic | A (mechanism) / B (schedule observed only statically) |
| `request_seek`, provider returns `RefusedUnchanged` | `classify_seek_status` (lib.rs:254-260): only `SONG_ERR_INVALID_ARGUMENT` classifies so; `fail_arg` (C:1166) overwrites the diagnostic but not the latch; worker resumes old cursor (session.rs:644-655) | retained (diagnostic overwritten) | next read surfaces it → `Failed` as row 1. Note: from the production adapter `INVALID_ARGUMENT` is unreachable (`Duration` targets are ≥ 0; handle non-null) — the class is contract-live via mocks/ABI controls, production-dead | A (mapping) / B (schedule) |
| `request_seek`, provider returns `MutatedThenFailed` | `fail()` overwrites diagnostic; latch retained but moot; worker: `release_seek_without_commit` + `decode_failed("seek failed: …")` + `edge.fail()` (session.rs:656-667) | superseded by the seek failure | `Failed{decode: seek failed: …}` (D14.5 conservative routing) | A |
| `request_stop` | `request_stop` records intent, releases gate, `edge.stop()` (completion.rs:532-562) → worker's write/loop observes terminal → return → `worker_exited(Stopped)` | never surfaced | `Stopped` (intent recorded) — stop wins over the pending fatal; correct per first-wins | A |
| Teardown / replacement / quit | retire path records stop first (`request_stop` + `wait_terminal`, player.rs:723-743) → same as stop | never surfaced | `Stopped` on the old episode; replacement proceeds | A |
| Pause / resume / volume / DSP-refusal | no provider call; worker keeps producing | retained | later read surfaces it (row 1) unless a seek/stop intervenes first | A |
| EOF | unreachable with a latched fatal: `song_read_pcm` returns the fatal before any EOF branch (C:1111-1113 precedes; C:1157-1158 ordering) | — | — | A |

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
  [any time] stop/teardown → edge.stop → worker observes terminal → Stopped
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
  recorded in [target contracts §M3](error-system-target-contract-r1.md); choosing one is
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

**N-01 (NEW_BOUNDARY, this round).** The gated abort line at `wasapi.rs:261` sits after
both catches. With `QIANQIAN_AUDIO_LOG` enabled and a broken/full stderr, `eprintln!`
panics; nothing catches it; `render_input.stop()` and `drain.complete()` are **skipped**.
Consequences: the decode worker wedges on a full edge with no data-plane stop;
`wait_terminal` (which the machine path runs BEFORE dispose, entry.rs:418-419) blocks
indefinitely — the process hangs before it could clean up or report. Scan A's F-07 covered
the volume-apply print (inside `steady_loop`, hence caught and escalated to `Aborted`);
scan B's F5 covered blocking of the same line. The panic-skips-publication case at :261 is
in neither scan. Class: mechanism CODE_FACT; reachability requires the gated log + a broken
stderr (`STATIC_INFERENCE`, not executed).

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
| 9 | wasapi.rs:538-547 open line | render | open phase, after verdict, before Start | none | gated diagnostic | panic → caught by catch #1 → Aborted (activation fails with HRESULT message); blocks → delays first Start |
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
| C-6 | "人类/TUI 不是具体消费者" | A MIGRATION §5 (R0 §5.1 reading) | DESIGN_DECISION | Code fact: the only consumers of `failure_diagnostic`/`activation_error` are presentation surfaces (machine rows, TUI, quit report); zero control-flow consumers. Whether presentation consumers satisfy R0 §5.1's "concrete consumer" bar is an owner design ruling, not a code fact. |
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
| E-1 | Pending-fatal × seek-Applied interleave under a real scheduler | Test-side (no production change): real SongCore provider + scripted decode fault after N frames (adapter-level fault injection or a truncated-at-offset source), request seek during the window, observe either the `Applied` continuation without any Failed, or the `pending decode error` Failed | Observing `Failed{pending decode error}` despite a successful Applied seek would falsify §2.3 row 2 |
| E-2 | Real SongCore error → D11 end-to-end | Feature-gated test (dev-dep on `qianqian-decode-songcore` in the test target only) driving `establish` with a real native handle against a corrupted fixture; assert the Failed terminal + stage family | A green run proves the chain binds; any stage-family mismatch falsifies the assumed passthrough |
| E-3 | Mid-read host I/O failure (U1) | A seekable fault-injecting source behind the existing io callback seam in a binding-crate test (the callback closure is currently fixed to `File::read` — the test lives in the excluded crate) | If SongCore surfaces the read failure as anything other than latched 103/`pending`/`decode error` family, the C mapping table is wrong |
| E-4 | Worker stuck in an uninterruptible native call | Filesystem-backed hang injection (e.g. FUSE or a stuck NFS mount) during steady decode; observe dispose/`wait_terminal` blocking and the absence of any Violated/timeout | Any automatic escape (timeout, Violated) would falsify "dispose has no escape" |
| E-5 | Slow/normal devices vs DRAIN_CAP=5 s | Instrumented drain timing on real endpoints (healthy + Bluetooth-class suspended sink) | A healthy device exceeding 5 s turns C-8's "tuning risk" into an observed misclassification; all devices < 5 s downgrades it |
| E-6 | SEEK_UNSUPPORTED post-state | Real unseekable container (raw ADTS) + seek attempt + bit-exact continuation probe at the ABI layer (extends F5 E1's method to the unmeasured statuses) | Bit-identical continuation would establish the provability fact the authority chose not to assume (amendment input, not automatic reclassification) |
| E-7 | N-01 / stderr hazards under blocked & broken sinks | The four negative controls R0 §6 already names (blocked, stopped consumer, queue full, write failure), per outlet (#5/#8/#10/#11), on a pipe-farmed stderr | Any observed semantic change (episode class, exit path beyond the documented ones) marks the corresponding outlet a defect rather than a hazard |
| E-8 | Broken-pipe exit-101 reachability on the main thread | Pipe-closed stdout/stderr harness around the binary at each report site (#1-#7) | Confirms A's F-07 main-thread scenario end-to-end; absence would falsify the std-panic premise on this platform |

---

## 9. What this round did NOT do

No production, test, build or ABI change; no ADR status change; no experiment executed;
no recovery policy adopted; #277 untouched; #281/#282 not edited; PR #280 not merged.
All candidate designs live in the companion documents and are marked CANDIDATE.
