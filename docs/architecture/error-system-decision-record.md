# Error-system decision record — owner directions + design-round recommendations

> **Implementation round (2026-10-16)**: the DG-1..DG-4c recommendations and the approved
> directions this round could implement without a new ruling have landed; the round's
> mechanism choices, executed evidence, NOT_RUN items and the still-open rulings (D-C above
> all) are indexed in `error-system-implementation-record.md`. This record's classifications
> are unchanged, and an OPEN item stays un-authorized.
> **Ratification round (2026-10-17)**: D-2 and D-10 remain formally OPEN. Their
> implementation substance landed with #287, and the #283 closeout verified exactly that
> differential while explicitly declining to rule ("D-C/D-D/D-2/D-4/D-7/D-9/D-10 的 owner
> 裁决（全部仍待裁，本文仅核实状态）"). Prepared ratification proposals now sit in §6;
> nothing there is adopted until the owner answers. IMPLEMENTED != OWNER_RATIFIED.

> **Truth class: DECISION RECORD / ROUTING INDEX. NOT AUTHORITY.**
> This file records (a) directions the owner has approved, (b) mechanism
> recommendations produced by design rounds, (c) authority-amendment
> candidates, and (d) pending evidence — each in its own labeled class so
> nothing silently becomes adopted. The designs themselves live in the four
> stable-named error-system documents; accepted architecture semantics live
> only in the ADRs and documents `docs/README.md` lists as NORMATIVE
> AUTHORITY. Nothing here changes D11, D14.*, U2, the SongCore ABI, K0
> semantics, or any accepted document.

**Version:** v1 — 2026-10-16 design round (post-#283); 2026-10-17 addition:
§6 ratification proposals for D-2/D-10 (prepared, not adopted) and the §5
implementation-state note.
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

Implementation state (2026-10-17, from #287 and the #283 closeout): D-2, D-3
and D-10 have landed implementations — D-2/D-10 as the §6 ratification
proposals, D-3's acceptance riding #288 A4; D-4/D-5/D-7/D-9/D-11 have no landed
implementation and no new evidence.

## 6. RATIFICATION_PROPOSAL (prepared 2026-10-17; NOT adopted — owner-gated)

Two decisions now have landed implementations but no recorded owner ruling. The
#283 closeout verified exactly that differential and explicitly declined to
rule; no ruling exists elsewhere in the round record either — #283's comments
are the owner's audit reports and closeout (recommendations and status
verifications only), and the #284/#287 conversations carry no human ruling.
Each proposal below states what a YES ratifies, what it does not, and what
happens on a NO. The owner may answer each independently. Until an answer is
recorded here, both formal lines stay PENDING and no ACCEPTED ADR text changes.

### 6.1 D-2 — probe diagnostic read + adapter retention (T1-1/T1-2, CC-4, DG-3)

```text
D-2_RECOMMENDATION       = RATIFY_OPTION_A
SCOPE                    = LOCAL_REPRESENTATION_AND_DIAGNOSTIC_RETENTION
PUBLIC_ABI_CHANGE        = NONE
PLAYBACK_SEMANTIC_CHANGE = NONE
```

What a YES ratifies — each item verified by reading `b86a9ce` (this round, not
assumed from the implementation record):

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

What a YES does NOT do: approve D-9, T1-3 public fields, or any recovery
policy; create a binding-parity obligation for other bindings (the migration
§3.5 note-level text — "binding-internal realization choice, not a
binding-parity obligation; the canonical ABI is unchanged" — is the companion
that lands only upon this YES); change `songcore.h` or any accepted document.

On a NO: option (b) is recorded; the landed retention is unbundled from the
formal decision and the resulting authority differential returns to the owner
(reverting production is a separate instruction, not a docs action).

### 6.2 D-10 — Output reason snapshot carrier (T2-5, DG-2)

```text
D-10_RECOMMENDATION      = RATIFY_CARRIER_B
CARRIER                  = EXISTING_DRAIN_COMPLETION
REASON                   = BOUNDED_DIAGNOSTIC_ATTACHMENT
CLASS                    = CLASS_C
D11_EFFECT               = NONE
BACKEND_AUTHORITY        = NONE
NEW_PUBLIC_PLUGIN        = NONE
D9_DECISION              = UNCHANGED
```

What a YES ratifies — the eight correspondence questions, each answered
against `b86a9ce` (this round):

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
   Whether that reading suffices, or migration §3.4's candidate note should
   be adopted to make the bounded-diagnostic-attachment permission explicit
   normative text, is part of THIS ruling, not pre-decided (migration §3.1
   already says so).
8. **D-9 stays independently OPEN?** Yes: the presentation-consumer bar is
   untouched; carrier B's exposure on the public observation plane rides the
   same `failure_diagnostic: Option<String>` surface that already existed.

Until the owner rules:

```text
D10_IMPLEMENTATION = LANDED (carrier B, #287, b86a9ce)
D10_RATIFICATION   = PENDING_OWNER
ADR_AMENDMENT      = PROPOSED_ONLY (migration §3.4 candidate; ADR-PBK-003
                     itself is untouched by the implementation and by this PR)
```

On a YES: a follow-up reviewed commit applies the minimal §3.4 differential —
existing completion boundary MAY carry a bounded control-inert diagnostic
attachment; verdict semantics unchanged; attachment absence MUST be legal;
backend identity stays non-semantic; the reason never participates in terminal
classification or control — and records the ruling here. It does NOT touch
Output Plugin identity, D11, K0 or backend ownership. On a NO: same
differential-return path as D-2.
