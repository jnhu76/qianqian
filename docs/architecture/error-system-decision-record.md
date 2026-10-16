# Error-system decision record — owner directions + design-round recommendations

> **Truth class: DECISION RECORD / ROUTING INDEX. NOT AUTHORITY.**
> This file records (a) directions the owner has approved, (b) mechanism
> recommendations produced by design rounds, (c) authority-amendment
> candidates, and (d) pending evidence — each in its own labeled class so
> nothing silently becomes adopted. The designs themselves live in the four
> stable-named error-system documents; accepted architecture semantics live
> only in the ADRs and documents `docs/README.md` lists as NORMATIVE
> AUTHORITY. Nothing here changes D11, D14.*, U2, the SongCore ABI, K0
> semantics, or any accepted document.

**Version:** v1 — 2026-10-16 design round (post-#283).
**BASE_SHA:** `e44aa806def5729fa360be19daeb21f5436d7fd9` (all code anchors).
**Inputs:** issue #283 REV.3 (final reports); the four error-system documents
([target contract](error-system-target-contract.md) / [reconciliation](error-system-reconciliation.md) /
[migration](error-system-migration.md) / [disposition contract](error-disposition-contract.md));
the owner round authorization for this design round (gate wiring CC-2/CC-3,
directions A–J, DG-1..DG-4c designs, real-cone oracle + E-1 harness designs,
F5 extension recommendation).
**DeepSeek-harness reference (re-verified this round):**
`deepseek-ai/deepseek-harness` @ `571902028a75c64c1d767f4ae5fd099485f0ddf0`.

## 1. OWNER_APPROVED_DIRECTION (approved for design; scope stated; nothing beyond)

Recorded verbatim in scope from the round authorization. Approval of a
direction is **not** approval of any specific mechanism, queue type, closing
posture, hook-chaining rule, or carrier named in §2 — those remain
LOCAL_MECHANISM_RECOMMENDATION until the owner adopts them. The open decision
entries D-2..D-11 (target-contract §Owner decision points) are unchanged by
this record.

| # | Direction (as approved) | Scope notes (what it does and does not fix) |
|---|---|---|
| A | Error propagation rides reliable operation results / execution evidence; logging rides a lossy diagnostic outlet. A log is never a precondition of terminal outcome, resource release, join, or Discharge. | Fixes the A/B-vs-C separation principle. Does not choose any logger mechanism (DG-1), does not itself remove the current `eprintln!` sites (that remains D-3/CC-1 territory). |
| B | Error normalization happens at real boundaries: keep useful stable classification, original cause, diagnostics; never control flow on presentation text; never promote an external native/SDK code directly into a Playback product class. | T1-4 stands (classes are minted at provable layers). SongCore `song_status`/native codes stay transport-level facts (DG-3), never D11 inputs. |
| C | Producers provide error facts; the operation owner decides disposition. No new `retryable` boolean, no recovery strategies; #277 and the existing NOT_EARNED recovery admissions stay as they are. | Closes the door the DeepSeek reference leaves open (its policy-side retryable-code list is reference material only — R5 verdict unchanged). |
| D | Logging references the DeepSeek shape: named facade, structured records, exporter separation, owner-managed registration/exit. No Error Plugin, no Recovery Plugin, no Logging Plugin, no EventBus, no K0 primitive. | The reference's *implementation* of exporter invocation (synchronous, inline, on the producer's call stack — logger.ts:154-159) is explicitly NOT part of the adopted reference (see E). |
| E | Qianqian's critical producers do not call arbitrary exporters/sinks directly. The realtime path, the failure-publication cone and the cleanup path get their respective cost and fault isolation. | The DeepSeek logger's synchronous exporter call cannot serve as isolation evidence. Isolation mechanism itself = DG-1 §queue/drainer (recommendation, not adopted). |
| F | Output verdict keeps its existing semantics; reasons explain, they never adjudicate the terminal. Reason delivery and log export are two different things; the backend never gains a reverse dependency on playback. | DG-2's carrier recommendation (§2) is inside this direction; adopting the carrier is still D-10 + the migration §3.4 note. |
| G | Adapter retention structures must be tied to concrete consumer obligations. No idle long-lived state "for future use". Probe copies the available cause before close — an explicit design goal. | DG-3 names the consumers (DG-1 record fields + presentation); adoption still rides D-2. |
| H | The complete logging design may absorb WASAPI ledger sites #8/#9/#10. A writer swap is neither a correctness-mandated prerequisite nor the completion bar for full isolation. | D-3's (a)/(b) remain correctness-equal; the landing PR states the residual blocking whichever lands first. |
| I | Keep synchronous publish/resolve/commit with post-unlock notify; no async resolver, no consumer-triggered settlement, no new terminal states. | The terminal model's Publish+AuthoritySettle split is a proof device (CC-3 comment fix re-anchored this). |
| J | No Lean 4; no six-module mega-model. F5 extension is decided by residual interleaving risk vs Rust verification coverage — an E-1 pass does not auto-cancel it. | This round's concrete recommendation: §2 (EXTEND_EXISTING_MODEL behind acceptance-gated triggers). |

## 2. LOCAL_MECHANISM_RECOMMENDATION (evidence-based; NOT adopted)

Full designs with compared alternatives, rejections and evidence live in the
named sections; this index records only the recommended mechanism.

| Area | Recommendation (one line) | Design |
|---|---|---|
| DG-1 logging | Two-class producer admission (R3): **class α (isolation-obligated: cone, render leg, decode worker, cleanup, critical threads) takes NO queue — atomics-only: ownership-handoff two-record pool per registered producer (R5; history: R2 "atomic swap" withdrawn — no single-exchange record; R4 seq-tagged plain payload withdrawn — Rust data race, discard can't undo UB); bounded two-record in-flight loss counted at the publish check; binding acceptance gates in order: no-data-race first, then snapshot integrity, loss bound, producer cost; #8's emission point moves after `drain.complete()` so the cone contains zero logging structurally**; class β (ordinary host threads): std bounded `sync_channel` + `try_send` — P-cap yes, **P-sync no** (fixed slots at creation; CAS+backoff, no step bound; post-success `notify()` takes an internal Mutex — verified against pinned 1.97.1 source), acceptable only because no isolation obligation attaches there; drainer discovery = conditional poll-frequency convention only (after each β batch, ≥ once per `recv_timeout` window while idle) — **no delivery deadline while an exporter blocks** (declared unbounded residual; the R4 poll-interval latency bound is withdrawn); an opt-in diagnostic-mode relaxation for render sites = explicit owner decision, default no; message composition = whitelisted segments (static tags / core integer impls / bounded byte copies — arbitrary `Display` banned); shutdown = admission contract A + exit abandon B (recommended, not adopted) | target-contract §DG-1 |
| DG-2 output reason | Carrier B: bounded `AbortReason{class, message}` payload inside `DrainVerdict::Aborted`, delivered by the existing single `DrainSignal::complete` seam (first-wins; classification untouched; verdict loses `Copy` — one bounded clone on the cold publication path); **late-vs-terminal policy**: decode-failure-first drops the later verdict+reason at `publish_evidence`'s settled early-return — drop + late-verdict counter recommended, side-slot reserved for a named consumer; withholding settlement rejected | target-contract §DG-2; migration §3.4 note |
| DG-3 adapter retention | Copy-at-error-point `DecodeFault{status, native_code, message}` consumed immediately by the DG-1 record + presentation; probe reads `song_last_error` before `song_close`; no long-lived `last_fault` | target-contract §DG-3 |
| DG-4a io callback | Adapter-owned `IoContext` with bounded io-fault slot tagged by **call-boundary id** (attribution only within the same top-level operation; success renders its boundary's record non-attributable; partial success permanently non-attributable) + count; io fault = correlated evidence, never auto root cause; callbacks panic-free by construction. **Known loss (owner-gated, R3)**: partial-success retention trade-off — a fault inside an ultimately-successful boundary can never be attributed later; delayed-fatal cause fidelity is NOT claimed | target-contract §DG-4a |
| DG-4b panic hook | Single installer; critical-thread isolation (**atomics only**: counter increment + at most one publication into the panicking thread's own DG-1 ownership-handoff pool — no concurrent record access exists to race; no write, no queue push, no lock — a non-panicking write is not a non-blocking exit) + conditional chaining elsewhere; never-restore stands; isolation grants no recovery | target-contract §DG-4b |
| DG-4c av_log | Default discard stays; opt-in bridge tiered — **β structured-only** (level + static fmt-string copy + class name; no varargs) recommended first, **α C-shim in-callback `vsnprintf`** only if a named consumer needs interpolated values (no copy-the-line tier: no owned line exists at callback time); env switch + host-installed callback before first open; no FFmpeg re-entry; FFmpeg-internal-lock question stays a stated assumption; per-handle routing rejected | target-contract §DG-4c |
| Real-cone oracle | Windows real-path steady-loop abort via input-trait wrapper injection (N-A) is the necessity oracle, executed under the frozen-consumer / no-EOF / no-fault regime: the R3 capacity-wait arithmetic witness is withdrawn (R4 — unit mismatch between staging blocks and render pulls, difference includes unwritten staging/remainder so ≠ occupancy, full edge ≠ worker in `wait_for_space`); necessity is proven at the outcome level by exit enumeration (no external stop + no EOF/fault/seek ⇒ the worker can exit only through an edge terminal, so deleting the cone's stop makes `wait_terminal` unreturnable) — no parked-at-injection claim; the parked-waiter precondition stays withdrawn (detection = settlement commits without external stop); the non-Windows open-failure tier is withdrawn (wasapi is `cfg(windows)`; open failure is activation-stage, never `wait_terminal`); optional N-B pins the activation-abort ordering at the activation boundary; ordering control = wrapper's post-stop-return program-order counter A vs synchronous verdict observer B (N-A only) | migration §7.1–7.2 |
| E-7 tier 1 | Per-site × {gate on/off} broken/blocked-sink controls in child processes (closed/full pipes); pre-swap runs reproduce the hazards (N-01 hang / #10 misattribution); post-swap: broken-sink ≡ sink-healthy; blocked-sink documents residual | migration §7.3 |
| E-1 harness | E1-ABI: real SongCore via test-owned `song_io` callbacks over in-memory fixtures; calibration = two legs (healthy control proves the fixture; the fault configuration proves partial-success-then-latch); latch proved by injection record + behavior (never message text); Applied/post-cut health vs control twin. E1-WORKER: real Session/worker + scripted provider; traces W1–W6 rebuilt on real execution positions, **W5 split by provider outcome (R3) then by remainder state (R4): W5a Applied → latch erased → `Stopped` single-legal-outcome; W5b-N Refused with no remainder (witnessed zero-frame-loop construction, R5: whole blocks → plant first command → zero-frame reads run NO write path (session.rs:831-835) → park latch set after the last non-zero read → serialization point fires the parked seek with remainder provably None) → next read surfaces the old fatal → `Failed{decode}`; W5b-R Refused with retained remainder (witnessed CutPoint) → loop-top flush hits the stopped edge → mid-remainder `Stopped` (session.rs:786-804), old latch never surfaces; W5c Mutated → destructive arm → `Failed{decode}`** (request_stop's own edge-stop window via one new test-fence arming point); not every stop ⇒ Stopped — killed by W5b-N/W5c; stop-during-provider ⇒ Failed — killed by W5b-R | migration §7.4–7.5 |
| F5 extension (D-C) | **EXTEND_EXISTING_MODEL** (pending-fatal into `f5-seek-discontinuity`, FORMAL-PLAN §3.1 shape), behind acceptance-gated triggers: E1-ABI calibration green on both legs AND W1–W6 (rebuilt) land in their predicted §2.3.1 cells; any divergence defers the extension until the differential is resolved. Stop-race half stays primarily Rust-verified | migration §7.6 |

## 3. AUTHORITY_AMENDMENT_CANDIDATE (changes to accepted semantics — owner-gated)

1. **PBK-003 §3.4 note (AudioOutput diagnostic attachment)** — the existing
   migration §3.4 candidate text, now annotated with DG-2's concrete carrier-B
   shape (payload inside the drain-verdict value, single completion seam).
   Conditional on T2-5 adoption (D-10); the note text itself is unchanged in
   substance.
2. Nothing else. In particular: no D14.x amendment is proposed by this round;
   §3.2/§3.3 candidates remain as-is with their own triggers (E-6/E-5); no K0
   text (§3.6 posture stands); no ABI change (D-5(b) recommended posture
   stands).

## 4. EVIDENCE_PENDING (what is missing and how to get it, executably)

| Gap | Executable acquisition |
|---|---|
| E-7 tier 1 runtime rows (#8/#9/#10 × gate × sink) | migration §7.3 harness on a Windows host; pre-swap rows must reproduce the documented hazards (N-01 hang, #10 misattribution) before any post-swap claim |
| E-1 calibration + W1–W6 | migration §7.4/§7.5 harnesses; platform-neutral (E1-ABI needs the native lib built; E1-WORKER is pure Rust) |
| FFmpeg av_log lock behavior at the pinned version | fetch the FFmpeg source pinned by `native/ffmpeg/pin.json`, read `libavutil/log.c` for the callback's lock context; record as assumption-verified or revised (DG-4c) |
| N-01 four-precondition end-to-end observation | the §7.1 T2 tier + §7.3 broken-sink pre-swap rows jointly produce it |
| Real-cone oracle existence | implement §7.1–7.2 (shares machinery with E-7 tier 1); D-E's cone production mutation only after it exists |
| M7 targeted-oracle blinding follow-up | seek_seam's producer-freeze witness no longer catches the M7 defect after 8542386 (this round re-pinned the gate to the stateful continuation probe — see the gate's header note and the incident log); decide whether the freeze witness should be repaired or retired as the M7 oracle |
| Queue producer-path contention datum (only if crossbeam is ever reconsidered) | a measured try-send latency/contention profile on the drainer under the real producer set — not required for the std recommendation |

## 5. Relationship to the open decisions

D-2 (adapter retention) ← DG-3 supplies the consumer bundle the CC-4 REV.2
shape (i) demanded. D-3 (writer swap) ← DG-1 records the correctness-equal
ordering and the absorption option (H). D-9/D-10 (consumer bar / carrier) ←
DG-2 recommends carrier B and names the presentation obligation. D-A/D-B
(gate wiring) ← executed this round as CC-2 (see the PR evidence log; the
f5-impl gate's CI home is the `f5-implementation-mutations` job). D-C ← §2's
F5 recommendation with its acceptance-gated triggers. All remain owner
rulings; this record attaches recommendations to them, not outcomes.
