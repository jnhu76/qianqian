# Error-system decision record — owner directions + design-round recommendations

> **Implementation round (2026-10-16)**: the DG-1..DG-4c recommendations and the approved
> directions this round could implement without a new ruling have landed; the round's
> mechanism choices, executed evidence, NOT_RUN items and the still-open rulings (D-C above
> all) are indexed in `error-system-implementation-record.md`. This record's classifications
> are unchanged, and an OPEN item stays un-authorized.
> **Ratification round (2026-10-17)**: D-2 and D-10 remained formally OPEN until
> this round. Their implementation substance landed with #287, and the #283 closeout
> verified exactly that differential while explicitly declining to rule ("D-C/D-D/D-2/
> D-4/D-7/D-9/D-10 的 owner 裁决（全部仍待裁，本文仅核实状态）"). This record first
> carried prepared ratification proposals; the owner then ruled — **Gate B closed
> 2026-10-17**: D-2 = RATIFY_OPTION_A, D-10 = RATIFY_CARRIER_B, plus the PBK-003
> minimal clarification (migration §3.4 candidate, applied as the ADR §5 amendment)
> and the binding note (§3.5, applied in songcore-binding-architecture §6.2). Receipt:
> [PR #291 comment 6229433676](https://github.com/jnhu76/qianqian/pull/291#issuecomment-6229433676)
> (posted from the owner-authenticated account; the decision source is that explicit
> instruction, not a backfilled historical approval). The four historical classes
> below (§1–§4) keep their original meanings; §5/§6 are later additions — proposals
> first, now the ratified decisions — and nothing outside D-2/D-10 changed class.
> IMPLEMENTED != OWNER_RATIFIED remains the rule; here the two finally coincide,
> with the ratification effective from the receipt, not from #287's merge.

> **Truth class: DECISION RECORD / ROUTING INDEX. NOT AUTHORITY.**
> This file records (a) directions the owner has approved, (b) mechanism
> recommendations produced by design rounds, (c) authority-amendment
> candidates, (d) pending evidence, and (e) owner-ratified decisions (§6:
> D-2/D-10, 2026-10-17) — each in its own labeled class so nothing silently
> becomes adopted. Classes (a)–(d) keep their original meanings; adding (e)
> changed no other class, and a ratified decision binds only its stated scope.
> §7 is class (b)-shaped but stronger-labeled: a DECISION PACKAGE, now
> carrying the owner's RATIFY_OPTION_B_SCOPED ruling (§7.0, 2026-10-18).
> The designs themselves live in the four
> stable-named error-system documents; accepted architecture semantics live
> only in the ADRs and documents `docs/README.md` lists as NORMATIVE
> AUTHORITY. Nothing here changes D11, D14.*, U2, the SongCore ABI, K0
> semantics, or any accepted document.

**Version:** v1 — 2026-10-16 design round (post-#283); 2026-10-17 additions:
§6 ratification proposals (superseded in place by the rulings below), §5
implementation-state note; 2026-10-17 Gate B — §6 became OWNER_RATIFIED_DECISIONS
and the companion authority clarifications were applied; 2026-10-17 #290 —
§7 D-D decision package; 2026-10-18 — §7.0 owner ruling RATIFY_OPTION_B_SCOPED
(recorded with the owner's scope + merge/closure bounds).
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
this record. (Time anchor: this section is the 2026-10-16 design-round scope.
D-2 and D-10 were subsequently ruled by the owner on 2026-10-17 — §6 is the
current authority for those two lines; the other entries stand as written.)

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

## 2. LOCAL_MECHANISM_RECOMMENDATION (evidence-based; NOT adopted at the original design freeze)

Full designs with compared alternatives, rejections and evidence live in the
named sections; this index records only the recommended mechanism.
(Time anchor: the freeze statement above is the 2026-10-16 state. The DG-3
and DG-2 mechanisms were subsequently RATIFIED through the D-2/D-10 owner
rulings of 2026-10-17 — §6; the rows below keep their original
recommendation wording as the design-freeze record.)

| Area | Recommendation (one line) | Design |
|---|---|---|
| DG-1 logging | Two-class producer admission (R3): **class α (isolation-obligated: cone, render leg, decode worker, cleanup, critical threads) takes NO queue — atomics-only: ownership-handoff two-record pool per registered producer (R5, corrected R6; history: R2 "atomic swap" withdrawn — no single-exchange record; R4 seq-tag plain payload withdrawn — Rust data race; R5 recycle load was Relaxed — no happens-before, withdrawn R6): pub/cons generation atomics with BOTH directions Release/Acquire (publish + recycle), admission = wrapping pub−cons ≤ 1, counters wrapping NATURALLY through 0 with empty = counter equality (R7: the R6 skip-0 cycle is withdrawn — at u32::MAX it maps two consecutive generations to one slot and passes distance 3; slot = generation % 2 alternates through the natural wrap), in-order no-skip consumption, loss ONLY at the producer admission check; pool-boundary accounting attempted = accepted + rejected, accepted = copied_out + outstanding (the R6 written = delivered + drops identity withdrawn — rejections precede writes and outstanding was missing); pool state machine {d=0 empty, d=1 one published, d=2 full→drop+count} with consume landing at pub_at_release − released (concurrent-publish interleavings preserved); design-candidate model EXECUTED (test-only error_pool_model_tests.rs: arithmetic incl. the R7 counterexample values, loom SCHEDULE-CLEAN, Miri clean over 33 seeds; negative controls R6-rule RED ×2 oracles and Relaxed-recycle RED under many-seed Miri — dist X15); binding acceptance gates in order: no-data-race (two-direction hb; Loom/Kani/Miri over first publish / two consecutive publishes / full-pool rejection / reuse after consumption / cross-wrap), then snapshot integrity, loss accounting (written = delivered + producer drops), producer cost; #8's emission point moves after `drain.complete()` so the cone contains zero logging structurally**; class β (ordinary host threads): std bounded `sync_channel` + `try_send` — P-cap yes, **P-sync no** (fixed slots at creation; CAS+backoff, no step bound; post-success `notify()` takes an internal Mutex — verified against pinned 1.97.1 source), acceptable only because no isolation obligation attaches there; drainer discovery = conditional poll-frequency convention only (after each β batch, ≥ once per `recv_timeout` window while idle) — **no delivery deadline while an exporter blocks** (declared unbounded residual; the R4 poll-interval latency bound is withdrawn); an opt-in diagnostic-mode relaxation for render sites = explicit owner decision, default no; message composition = whitelisted segments (static tags / core integer impls / bounded byte copies — arbitrary `Display` banned); shutdown = admission contract A + exit abandon B (recommended, not adopted) | target-contract §DG-1 |
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
   substance. (Outcome, 2026-10-17: applied — the D-10 ruling adopted this
   candidate as the ADR-PBK-003 §5 TERMINAL EVIDENCE Amendment note; this
   section retains the original candidate wording for provenance.)
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
F5 recommendation with its acceptance-gated triggers. All of these remain
owner rulings to which this record attaches recommendations, not outcomes —
with the two exceptions now ruled (D-2/D-10, §6: recommendations that became
ratified decisions on 2026-10-17).

Implementation state (2026-10-17, from #287 and the #283 closeout): D-2, D-3
and D-10 have landed implementations — D-2/D-10 now RATIFIED by the owner
(§6, receipt PR #291 comment 6229433676), D-3's acceptance riding #288 A4;
D-4/D-5/D-7/D-9/D-11 have no landed implementation and no new evidence.
(2026-10-17 addition: D-C was subsequently decided by the owner in its own
receipt — §6.4 below; the "not adopted" sentence in the D-2/D-10 receipt and
in §6.3 remains the correct history for that ruling's scope.)

## 6. OWNER_RATIFIED_DECISIONS (ratified 2026-10-17; receipt: [PR #291 comment 6229433676](https://github.com/jnhu76/qianqian/pull/291#issuecomment-6229433676))

Two decisions had landed implementations but no recorded owner ruling; the
#283 closeout verified that differential and explicitly declined to rule, and
no ruling existed elsewhere in the round record — #283's comments are the
owner's audit reports and closeout (recommendations and status verifications
only), and the #284/#287 conversations carry no human ruling. This record
therefore carried prepared proposals; the owner then ruled on 2026-10-17
(D-2 = RATIFY_OPTION_A, D-10 = RATIFY_CARRIER_B, plus the two companion
clarifications in §6.3). The proposals' verification bullets are preserved
below as the ratified scope's audit basis — they were re-verified at
`b86a9ce` before the ruling was recorded. The ratification is effective from
the receipt, not from #287's merge: implemented facts are #287's; formal
approval starts here.

### 6.1 D-2 — probe diagnostic read + adapter retention (T1-1/T1-2, CC-4, DG-3) — RATIFIED_OPTION_A

```text
D-2_DECISION        = RATIFIED_OPTION_A (2026-10-17)
D-2_IMPLEMENTATION  = LANDED_PR_287 (DG-3 shape (a))
D-2_AUTHORITY_SCOPE = LOCAL_REPRESENTATION_ONLY
D-2_ACCEPTANCE      = EXISTING_LINUX_EVIDENCE
```

What the ratification covers — each item verified by reading `b86a9ce` before
the ruling (not assumed from the implementation record):

- Adapter-internal `DecodeFault{status, native_code, message}` copied at the
  error point (`qianqian-decode-songcore/src/lib.rs:238`), consumed
  immediately, no long-lived `last_fault` (DG-3 shape (a); direction G).
- `song_probe` failure reads the borrowed diagnostic via `song_last_error`
  BEFORE `song_close` (lib.rs:400-413), inside the ABI's
  valid-until-next-same-handle-call window; the owned copy then rides
  `DecodeOpenError.message` text only. `song_open`'s no-diagnostic contract is
  untouched (a failed open publishes no handle to query — E1-ABI witness).
- The named consumers are the DG-1 record fields (`with_status` /
  `with_native_code`, lib.rs:264-271) and the App-facing message text. No
  control-flow consumer exists: `DecodeError`/`DecodeOpenError` remain
  `{message: String}` (ports.rs:1156-1164), so no downstream branch can even
  read a typed status — control-inertness is structural, not just tested.
- SongCore C ABI and native sources are unchanged since the R0 baseline
  (`git diff 524c6ef..b86a9ce -- native/` is empty).
- Executed evidence: decode-songcore lib tests 36/36 green (Linux), including
  E1-ABI 8/8 and the typed-fault outlet witness
  (`e1_abi_tests.rs:585 adapter_read_failure_emits_the_typed_fault_to_the_diagnostic_outlet`).

What the ratification does NOT do: approve D-9, T1-3 public fields, or any
recovery policy; create a binding-parity obligation for other bindings; change
`songcore.h` or any accepted document beyond the note-level clarification
applied in songcore-binding-architecture §6.2 (§6.3 below) — "binding-internal
realization choice, not a binding-parity obligation; the canonical ABI is
unchanged".

Alternative not taken: option (b) record-only (the decision fork is preserved
in the owner-decision table and the migration §1.1 history).

### 6.2 D-10 — Output reason snapshot carrier (T2-5, DG-2) — RATIFIED_CARRIER_B

```text
D-10_DECISION       = RATIFIED_CARRIER_B (2026-10-17)
D-10_CARRIER        = EXISTING_DRAIN_COMPLETION
D-10_REASON_CLASS   = DIAGNOSTIC_ONLY
D-10_REASON_OPTIONAL= YES
D-10_D11_EFFECT     = NONE
D-10_IMPLEMENTATION = LANDED_PR_287
```

What the ratification covers — the eight correspondence questions, each
answered against `b86a9ce` before the ruling:

1. **Single completion interface?** Yes: the reason is a payload inside
   `DrainVerdict::Aborted { reason: Option<AbortReason> }` (ports.rs:162-167)
   delivered by the existing `DrainSignal::complete` — no second cell,
   parameter, seam or lifecycle.
2. **Bounded owned payload?** Yes: `AbortReason{class, message}` with the
   512-byte char-boundary cap (ports.rs:126-150); `OutputAbortClass` is
   backend-neutral (`Device`/`Panic`; `Cone`/`Open` are documented absences,
   not names).
3. **Resolver reads only the semantic discriminant?** Yes: `resolve` matches
   `Some(DrainVerdict::Aborted { .. })` (completion.rs:1360) — the pattern
   ignores the reason field.
4. **Can the reason change D11 classification?** No: structurally (the
   resolver pattern cannot see it) and on the read side — the one production
   reader enriches `failure_diagnostic` presentation text only AFTER the
   terminal is committed and only for `Failed` + `OutputAbortClass::Device`
   (completion.rs:1126-1136); the public observation vocabulary is unchanged.
   Witness: `dg2_a_reason_cannot_change_the_committed_terminal`
   (e1_worker_tests.rs:1186) shows the identical committed terminal with the
   reason present and absent.
5. **Late delivery fate explicit?** Yes: `publish_evidence` first-wins
   early-return drops a late verdict+reason; `late_drain_verdicts` counts it
   (completion.rs:284/416/1224); witness
   `dg2_a_late_verdict_is_dropped_and_counted` (e1_worker_tests.rs:1233).
6. **New legal diagnostic shape on the backend-neutral port?** The attachment
   type is audio-api-typed and backend-neutral; it IS a new shape on the port
   surface (the `Copy` removal went through the audio-api surface gate's own
   reviewed regeneration). This is precisely why PBK-003 correspondence is
   part of this ruling — see below.
7. **Authorized by current ADR-PBK-003?** PBK-003 §5 TERMINAL EVIDENCE
   obliges "report drain/abort mechanism evidence through the existing owner
   seam without becoming D11 semantic authority". Carrier B satisfies that
   clause as written (same seam, evidence-only payload, no D11 authority).
   The owner's ruling adopts the explicit clarification (§6.3) so the
   bounded-diagnostic-attachment permission is stated normatively instead of
   resting on interpretation.
8. **D-9 stays independently OPEN?** Yes: the presentation-consumer bar is
   untouched; carrier B's exposure on the public observation plane rides the
   same `failure_diagnostic: Option<String>` surface that already existed.

Ruling outcome (2026-10-17):

```text
D10_IMPLEMENTATION = LANDED (carrier B, #287, b86a9ce)
D10_RATIFICATION   = RATIFIED_CARRIER_B (receipt PR #291 comment 6229433676)
ADR_AMENDMENT      = APPLIED — PBK-003 §5 TERMINAL EVIDENCE now carries the
                     minimal clarification, adopted from migration §3.4's
                     candidate text (§6.3)
```

The ratified shape keeps everything the questions verified: the
backend-neutral `OutputAbortClass`, the bounded owned reason, the
discriminant-only `resolve()`, the late-verdict drop + count policy, the
existing output/playback authority relationship, and the presentation
projection semantics. Alternative not taken: carrier (a') the
`RenderRequest`-field cell (decision fork preserved in the owner-decision
table and migration §1.4).

### 6.3 Companion authority clarifications applied by the same ruling

```text
PBK-003
  DECISION       = ADOPT_MINIMAL_CLARIFICATION
  SOURCE         = ERROR_SYSTEM_MIGRATION_SECTION_3_4
  SCOPE          = OPTIONAL_CONTROL_INERT_DIAGNOSTIC_ATTACHMENT
  NEW_AUTHORITY  = NONE

SONGCORE_BINDING
  DECISION       = ADOPT_NOTE_LEVEL_CLARIFICATION
  SOURCE         = ERROR_SYSTEM_MIGRATION_SECTION_3_5
  CROSS_BINDING_PARITY_OBLIGATION = NONE
```

Applied as: the ADR-PBK-003 §5 amendment note (terminal-evidence diagnostic
attachment; MAY-carry, absence-legal, no D11/control effect, backend-neutral,
no backend obligated to construct a reason) and the songcore-binding-
architecture §6.2 note + table rows (binding-internal retention, canonical ABI
unchanged, no parity obligation). Both cite the receipt. Explicitly NOT
adopted by the same ruling: D-9 (open), D-C (#289), D-D (#290), Windows
runtime acceptance (#288), T1-3 public fields, and any recovery policy.

### 6.4 D-C — F5 pending-fatal model extension (#289) — EXTEND_EXISTING_MODEL (decided 2026-10-17)

```text
D-C_DECISION        = EXTEND_EXISTING_MODEL (2026-10-17)
D-C_TARGET          = specs/f5-seek-discontinuity/
D-C_ISSUE           = #289
D-C_CALIBRATION     = E1_ABI_PRIOR_RECORDED_PASS + E1_WORKER_PRIOR_RECORDED_PASS
D-C_STOP_RACE       = RUST_VERIFICATION_PRIMARY (TLC = bounded safety search)
D-C_SEMANTIC_AUTHORITY = ADR-PBK-002 D14.5/D14.8 + D11 (MODEL_IS_AUTHORITY = NO)
D-C_NOT_AUTHORIZED  = production change / ABI change / K0 change
D-C_RECEIPT         = issue #289 comment 6229911387
```

Receipt: [issue #289 comment 6229911387](https://github.com/jnhu76/qianqian/issues/289#issuecomment-6229911387)
(posted 2026-10-17 from the owner-authenticated account; the decision source is
that explicit instruction for #289 — a NEW decision, not a backfilled ruling of
the #287 implementation round, and not implied by the D-2/D-10 receipt, which
listed D-C among its explicitly-not-adopted items).

What the decision covers: extending the existing F5 model with the GAP-01
pending-fatal window per #283 FORMAL-PLAN §3.1 — Q5 minimal state budget
(each variable justified or dropped), Q6 lock-cited atomicity splits
(`edge.stop()` separate from intent recording; provider begin/return split;
fusions only where a lock hold is cited), Q9 scoped safety with the mandatory
Applied-erasure Completed witness, Q10 trace-first defect ledger D-T1..D-T4
written before any mutation, Q11 retained witnesses — with the acceptance-
gated trigger of migration §7.6 recorded as satisfied (E1-ABI green on both
legs; E1-WORKER W1–W6 in their predicted §2.3.1 cells; the trigger's
verification is part of the implementation record's 2026-10-16 note, re-cited
by the receipt). The stop-race half stays primarily Rust-verified; TLC results
are bounded interleaving-search evidence, never architecture acceptance, and a
model counterexample authorizes no production change.

Execution record: the implementing round and its evidence live in
`error-system-implementation-record.md` (§13) and
`specs/f5-seek-discontinuity/` (PENDING_FATAL_DAG.md frozen ledger; extended
SeekDiscontinuity.tla; RESULTS.md).

## 7. D-D decision package (#290, 2026-10-17) — the `:274` flush-Err disposition (RATIFIED 2026-10-18)

### 7.0 OWNER RULING — RATIFY_OPTION_B_SCOPED (2026-10-18)

Ruled by the owner in the #290 evidence round (session directives of
2026-10-17 review + 2026-10-18 confirmation; the review is not a GitHub
comment — its verdict block was `D_D_OWNER_RULING=HOLD_PENDING_EVIDENCE_REPAIR`).

```text
D_D_OWNER_RULING = RATIFY_OPTION_B_SCOPED
    接受当前条件下的 flush 结果丢弃，不修改产品行为，
    并保留 T1–T4 重审条件（owner's words, 2026-10-17 review）
```

Scope of the ratification (owner-stated, binding):

```text
current writer (the :273-274 print!+flush closure)
+ current output format ('\n'-terminated single-writer rows)
+ verified std implementation (LineWriter, toolchain pinned §14.8)
+ its defined environments (§7.2 scope) ONLY
— never "flush Err impossible everywhere on all platforms";
  all of T1–T4 (§7.4) reopen D-D, especially native Windows
  and a real terminal sink
```

Repair precondition: the owner's P1 (v1 probe premature flush +
cross-process PA-2) was repaired by the committed v2 tests
(`e9_flush_probe_*`, §7.2 repair note); the ruling followed that repair.

Merged-with-this-ruling bounds: evidence PR #293 merge held by the owner
pending the B2 test-oracle round (implementation record §14.8 amendments,
B2 scheduling-sensitivity round); issue #290 closure must keep the NOT_RUN
lanes (native Windows broken-sink, terminal sink) explicitly NOT counted as
PASS — owner: “不能把它们悄悄计入 PASS”.

Sections 7.1–7.5 below are the decision PACKAGE the ruling was made from,
preserved as written (including its agent-recommendation label; the package
text dates from 2026-10-17, before this ratification); §7.5's
"moves from PENDING to the owner's recorded choice" has now happened, to
RATIFY_OPTION_B_SCOPED.

---

### 7.1 The decision being asked for

`machine_input.rs:273-274` — the reader's stdout output closure:

```text
print!("{line}");
let _ = io::stdout().flush();
```

The explicit flush result is discarded. D-D decides the disposition:

```text
Option A — CLASSIFY_AND_RECORD     a flush Err becomes a classifiable
                                   host failure (admitted-operation record +
                                   conditional Stop), like the print! panic
Option B — ACCEPT_DOCUMENTED_LOSS  the discarded flush result stays
                                   discarded; the loss's scope is written
                                   down, with explicit revisit triggers
```

Option A's own gate (frozen §14.6-B3): it is available ONLY if an independent
flush Err is reachable WITHOUT reshaping production output, the std writer, or
ordering.

### 7.2 Executed evidence (§14.8.3)

Verdict: **INJECTED_WRITER_ONLY** — with the current production shape there is
NO independent error window at the flush:

- every reader stdout writer emits '\n'-terminated rows (RQ1: `format_status`
  8/8 push paths end '\n'; transport rows likewise);
- Rust `Stdout` is a LineWriter independent of sink type (RQ2, demonstrated
  behaviorally by the committed PB test: a no-newline `print!` stays buffered
  on a real pipe);
- a successful '\n'-terminated `print!` leaves an EMPTY buffer — committed v2
  probe `e9_flush_probe_newline_row_flush_ok_on_proven_broken_sink`: the row
  is OBSERVED in the pipe before the parent closes the read end, the arm's
  single `flush()` then returns Ok, and a second `print!` on the SAME sink
  panics with `Broken pipe` — so the sink was broken when the flush returned
  Ok and that Ok can only mean no write was attempted;
- a flush Err therefore requires a non-newline tail in the buffer (committed
  PB test: `Err(BrokenPipe)` demonstrated — the only flush performs the
  write), a shape no production writer has; a foreign non-newline tail is
  carried to the fd by the NEXT row's own `print!` (committed INTERLEAVE
  test), i.e. it surfaces in the already-classified print!-panic path
  (`HostFailure::Panic` via the `#R-dispatch` catch), never in the trailing
  flush.

Repair note: the round's v1 scratch probe was withdrawn at this point by the
owner's P1 review (2026-10-17) — its PA/interleave arms flushed explicitly
before the observed flush and its PA-2 control ran on a different process's
sink, so its Ok observations were non-discriminating (§14.8.3). The v2 probe
is committed tests (`e9_flush_probe_*`, cfg(unix)), CI-executed green by the
repair tip's Linux lane (run 39046043339 at 1c62399, §14.8.8-g); the
instrument no longer depends on this record's transcription.

Scope: Linux pipe + regular-file probes; terminal NOT_RUN (no tty in the
harness; the interop console proxy is a pipe — §14.5/§14.7); std EBADF
swallowing recorded; toolchain 1.97.1 (8bab26f4f) pinned in §14.8. In-repo L1:
the current swallow is INDIFFERENT to a flush Err — no record, no Stop, seal
clean (`flush_err_swallow_is_indistinguishable_from_success`, with an in-test
control proving the harness distinguishes swallow from panic).

### 7.3 What each option would mean today

```text
Option A  adds a record/Stop path for a state the executed evidence says
          production cannot reach standalone (the print! fails first and is
          already classified). New classification surface; covers hypothetical
          writer reshapes in advance.
Option B  records that the flush result is discarded BECAUSE the
          '\n'-terminated single-writer shape leaves the flush nothing to do;
          every reachable failure already surfaces at the print!. No cost
          today; the risk is silent staleness if the output shape changes —
          which the revisit triggers below retire.
```

### 7.4 Recommendation (the round's agent; NOT a ruling)

B-leaning: Option A's evidence gate is not met in the scoped environment. The
loss B accepts is exactly and only: "after a successful newline-terminated
print!, the flush performs no write; its Err is unreachable standalone."
Option B comes with these revisit triggers — any ONE reopens D-D:

```text
T1  any production stdout writer that can emit a non-newline-terminated tail
    (the LineWriter buffer would carry it into the flush)
T2  any reshaping of the :274 closure (write!+flush batching, a different
    writer, a buffered-by-default writer swap)
T3  a std contract change to Stdout buffering (LineWriter) — the committed
    v2 PA probe is the automated tripwire for a buffering regression: it
    makes `e9_flush_probe_newline_row_flush_ok_on_proven_broken_sink` fail
    via its premise, flush-value, or brokenness oracle
T4  flush-Err evidence from any §14.7 NOT_RUN lane — a native Windows
    console session or a real terminal sink on any platform (the interop
    proxy cannot reach either)
```

### 7.5 What this package does not decide

No ratification, no production change, no ADR amendment. The whole #290 round
carries PRODUCTION_BEHAVIOR_CHANGE=NONE / D11_CHANGE=NONE / K0_CHANGE=NONE /
ABI_CHANGE=NONE / ADR_CHANGE=NONE. If the owner rules A, the product work is a
separate post-ruling PR; if B, the disposition record is this section plus the
§14.8.3 evidence, and D_D_OWNER_RULING moves from PENDING to the owner's
recorded choice.
