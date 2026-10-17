# Error-system migration and acceptance (#231, #280)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY.**
> This file plans how the candidate goals in
> [error-system-target-contract.md](error-system-target-contract.md) would be
> adopted, what would break, and what evidence each adoption would owe. It performs no
> migration, changes no authority, and approves nothing. Baseline:
> `5e60da4c3bd5f0fa93b4215670f4e898e2ac844e` (facts in
> [error-system-reconciliation.md](error-system-reconciliation.md)).

> **Status (2026-10-17) — document authority vs implementation state, kept separate:**
>
> ```text
> DOCUMENT_ROLE                     = per-boundary migration + acceptance-by-nature plan
> DOCUMENT_AUTHORITY                = UNCHANGED: DESIGN_CANDIDATE / NOT_NORMATIVE_AUTHORITY
> ORIGINAL_BASELINE                 = 5e60da4c facts; plan text frozen at the PR #280 round
> CURRENT_IMPLEMENTATION_REFERENCE  = the migrations whose goals landed via PR #287 (merged
>                                     b86a9ce 2026-10-16) are indexed, with the actual
>                                     deletions/replacements, in
>                                     error-system-implementation-record.md §1/§3; this
>                                     plan's own "Current" columns still describe the
>                                     pre-implementation baseline and were NOT rewritten
> CURRENT_ACCEPTANCE_BOUNDARY       = Windows acceptance designs pending #288; the §7.6 F5
>                                     obligations pending the D-C ruling #289
> RESOLVED_BY_OWNER_RULING         = D-2/D-10 (RATIFIED 2026-10-17; decision-record §6;
>                                     receipt PR #291 comment 6229433676). The §3
>                                     conditional texts those rulings adopted are now
>                                     applied: §3.4 as the PBK-003 §5 minimal
>                                     clarification, §3.5 as the binding note in
>                                     songcore-binding-architecture §6.2
> UNRESOLVED_OWNER_DECISIONS        = §3.2/§3.3 candidates keep their own triggers
>                                     (C-9 / D-4); D-4/D-5/D-7/D-9/D-11 unchanged
> ```
>
> Round closeout: [#283](https://github.com/jnhu76/qianqian/issues/283#issuecomment-6228758527).

## 1. Per-boundary migration matrix

Columns: current path (frozen fact) → target contract → producers/consumers to modify →
document impact → old-path removal → acceptance evidence (by nature + counterexample, not
by count). "—" means nothing to change. Goal IDs reference the target-contract file.

### 1.1 Typed/native cause retention and association (T1-1, T1-2)

```text
Current   adapter drops song_status + native_code at read (lib.rs:288-291); native_code
          read points = 0; probe formats decimal status only (lib.rs:188-200)
Target    adapter-internal DecodeFault{status, native_code, message}; probe reads
          last_error() before close; message-only public face unchanged
Modify    qianqian-decode-songcore only (lib.rs read/seek/probe paths)
Docs      songcore-binding-architecture: NOTE that retention is binding-internal; other
          bindings are not obligated to replicate it (parity oracle untouched)
Removal   nothing removed; no deprecated aliases exist today (verified)
Acceptance  binding-crate unit tests: per synthesized fault, assert captured
          (status, native_code) pairs; probe test on a garbage fixture asserts the
          diagnostic rides in the message. Counterexample: any public-surface diff, or a
          session/App branch reading the retained class
```

### 1.2 Cause not polluted by later operations (T2-3, T1-2)

```text
Current   song_error valid until next same-handle call (songcore.h:99-102); adapter
          copies immediately (lib.rs:213-226) — conforming; probe currently closes
          without reading
Target    PRESERVE the copy-before-next-call rule as a review checklist item; probe
          reads before its own close; owned copies only cross into async/diagnostic hands
Modify    checklist/doc only (code already conformant post T1-2)
Docs      disposition-contract §5.6 wording already correct; no authority change
Acceptance  test: fault → read last_error → issue another call → assert the copied
          snapshot is unaffected by the newer diagnostic
```

### 1.3 Partial success × seek/stop/close interleaving (reconciliation §2 matrix)

```text
Current   proven matrix: next read surfaces; Applied erases via reset_decode_state;
          stop/teardown are a COMMIT RACE, not stop-determinism (R1.1, reconciliation §2.3.1 —
          the default worker position lets the pending fatal win; a stop wins only
          from stop-observing positions); RefusedUnchanged production-dead
Target    PRESERVE the behavior table (as corrected) as the regression baseline;
          optionally T3-1 (superseded-fatal diagnostic witness — OPEN-DECISION D-5;
          currently unobservable across the ABI, R1.1)
Modify    none for preservation; ABI/SongCore + adapter+session if D-5 adopted
Docs      none (behavior is ABI+worker-conformant; no frozen text contradicts it)
Acceptance  E-1 (reconciliation §8, R1.2 attribution rules): observe the FIRST read after the
          Applied landing; old-vs-new attribution rests on the one-shot injection
          record, the call phase, and a controlled-healthy post-cut region (verified
          on the no-injection control) — the "pending decode error" wording means
          DEFERRED REPORTING, not fault identity, and is corroboration only. The
          stop-race half is a separate observable: a stop inside the window yields
          Stopped OR Failed{pending decode error} with the worker's actual exit
          surface recorded (default position / mid-write / remainder-flush — §2.3.1's
          R1.2 enumeration; a canceled pending seek is not an exit) — never an
          assertion that stop alone forces Stopped; freezing the pre-R1.1 wording in
          an oracle is the failure mode this row exists
          to prevent. A counterexample to the corrected table itself would falsify
          reconciliation §2.3, not the goal
```

### 1.4 Output verdict and reason delivery (T2-5, D-10)

```text
Current   LoopOutcome::Aborted{message} exists as gated stderr + thread-local only;
          session settles Failed{"device"} with the reason unrecoverable (L7)
Path      allocation: session creates the cell before open_stream
          → handoff: CROSSES qianqian-audio-api in every carrier — (a') a
          RenderRequest field carrying an audio-api-typed cell (the pattern
          OutputLevel/PositionEvidence already use, ports.rs:962-985; wasapi's deps
          are audio-api+composition only, so nothing playback-typed can reach the
          render thread) or (b) a parameter on the drain.complete-adjacent delivery
          API (DrainSignal is likewise audio-api-typed)
          → write: render thread at abort stores a bounded owned copy into the cell
          → read: session publication / internal diagnostic surface (exposure on the
          public observation plane is a separate T1-3/D-9 question)
          → retirement: episode teardown drops the session's cell; the mechanism
          held only a clone
Target    bounded owned reason snapshot delivered alongside the verdict; verdict-only
          settlement semantics unchanged
Modify    qianqian-audio-api (the handoff type, either carrier) + wasapi.rs abort path
          (writer) + playback session (cell + read side); tests/common mock
          OutputBehavior follows whichever carrier D-10 picks
Docs      PBK-003 interpretation note applies to T2-5 in EITHER carrier (§3.4, R1.1);
          D14.2 untouched either way (content is Class-C diagnostic; control-inertness
          witness required)
Removal   the QIANQIAN_AUDIO_LOG-only carrier for the abort reason may be demoted to a
          debug line AFTER the reliable carrier exists (never before)
Acceptance  stop_seam-style: identical outcome class with snapshot present/absent;
          reason text asserted present at the diagnostic surface on a mock abort;
          counterexample: any resolve()/App branch reading the snapshot
```

### 1.5 Observer / exit-funnel integrity (N-01, N-02, T5-4, D-3)

Unified remediation table (R1.1 — one table, hazard class separated from fix class;
"writer swap" fixes panic, never blocking):

| Ledger row | Site | Panic hazard | Blocking hazard | Covered by the D-3 corrective? |
|---|---|---|---|---|
| #8 | wasapi.rs:261 post-catch abort line | skips `render_input.stop`+`drain.complete` → N-01 hang (write-FAILURE mode, four preconditions, reconciliation §3.2) | delays wakeup/drain publication (B F5) | YES (writer swap) — panic removed; blocking REMAINS until framework |
| #9 | wasapi.rs:538-547 opened line | caught by catch #1 AFTER `Opened` published → post-activation abort, `Failed{"device"}` misattribution, or N-01 cascade via #8 | delays first Start (playback onset) | YES (writer swap) — same partial scope |
| #10 | wasapi.rs:678-686 volume recoverable failure | caught by catch #2 → recoverable failure escalated to `Aborted` → `Failed{"device"}` (A F-07) | render-loop stall → device underrun (B F6) | YES (writer swap) — same partial scope |
| #11 | panic default hook + chain-to-previous | reentrancy only | blocks the panic translation itself; chain restores default sync stderr for all non-reader threads (B F7) | NO — process-global hook, not fixable by a writer swap; mechanism OPEN (T5-8) |
| #5 | machine_input.rs:230-233 reader protocol writes | caught (:119) → `HostFailure::Panic` → episode terminated by a diagnostic write | full pipe → reader stuck → `seal()` waits forever | NO — protocol class with its OWN acceptance (E-9, R1.2): its write failure is a legitimate host-operation failure by T5-7 and must NOT be held to the logging-inertness oracle; the two-class split (§1.7) is the contract |

```text
Current   decode funnel + render post-catch cone run outside every catch with
          poison-sensitive locks; gated abort-line panic skips stop+complete (N-01);
          volume print panic upgrades a recoverable failure (#10); opened-line print
          (#9) panics into catch #1 after the Opened verdict published
Target    the THREE wasapi gated sites (#8/#9/#10) swap to a non-panicking writer —
          a PARTIAL remediation: panic/skip escalations removed, full-pipe BLOCKING
          remains on every synchronous site until the T5 framework lands (T5-2/T5-3);
          #11 and #5 keep their own classes per the table above; funnel/post-catch
          ordering documented as load-bearing; D-7 (panic→latch in teardown) stays
          OPEN pending K0 review
Modify    wasapi.rs three sites + a small writer helper; nothing else this round
Docs      playback-execution-model hazard rows updated when the corrective lands;
          disposition-contract §6 hazard list marked resolved-at-<sha> at that time — for the PANIC
          class only, with the residual blocking hazard stated in the same row
Removal   none
Acceptance  E-7 tier 1 (reconciliation §8, R1.1): per changed site (#8/#9/#10), broken-sink
          negative control × {gate on, gate off} — terminal class, exit path and
          wakeup ordering identical to the sink-healthy control (N-01's write-failure
          mode impossible by construction; demonstrated by the write-error control);
          PLUS a blocked-sink control that documents the RESIDUAL (publication still
          delayed — expected, recorded as residual; NOT sold as isolation). The hook
          (#11) keeps its E-7 tier-2 registry entry for the future framework round;
          the reader (#5) is validated by E-9 against its OWN documented classes
          (R1.2) — the acceptance set names only objects with a matching expectation
```

### 1.6 Partial activation and cleanup failure (D14.6 baseline; D-7 open)

```text
Current   five raise classes unwind LIFO; failure_clean_start distinguishes
          ActivationFailedClean vs FailStop; panicking inverse crashes (reconciliation §3.4)
Target    PRESERVE D14.6 machinery; D-7 records the crash-propagation posture explicitly
          or sends catch→Violated to K0 authority review (never implemented ad hoc)
Modify    none now; qianqian-app lib.rs / kernel.rs only after a K0 review decision
Docs      composition-kernel-0-design §G.6 amendment IF (and only if) D-7 option (a)
Acceptance  existing LIFO witnesses (establishment:142, teardown_gate_precondition)
          remain green; a new test would be owed only by an actual D-7 option-(a) change
```

### 1.7 Protocol-output failure vs diagnostic-log failure (ledger §4)

```text
Current   protocol rows panic → host-operation failure (reader: episode terminated;
          main pre-dispose: exit 101 skipping dispose); diagnostic rows: same panic
          semantics today (the conflation T5-4 removes for diagnostics)
Target    classification codified: protocol write failure = host-operation failure
          (visible, may terminate); diagnostic sink failure = diagnostic loss only
Modify    docs/checklists; code only via T5-4 corrective
Docs      playback-execution-model D3/D5 rows gain the explicit two-class statement
Acceptance  E-9 (reconciliation §8, R1.2 — protocol-write conformance, its own oracle): per stream
          (stdout/stderr) broken/closed-sink harness around the reader's status writes
          asserting the DOCUMENTED host-failure path (caught → HostFailure::Panic →
          conditional request_stop → episode terminates; seal obligations met), and
          E-8 for the main-thread report sites — the two classes must diverge exactly
          as classified; a protocol write is never silently dropped like a diagnostic
```

### 1.8 Real SongCore → Session composition entry (E-2)

```text
Current   real chain compiles only behind headless --features playback; no automated
          test; f1-physical-gate.ps1 is a manual script; workspace exclude keeps the
          binding crates out of --workspace runs
Target    (owner-gated) one feature-gated test binding the real provider to a session-
          level Failed/Completed settlement on curated fixtures; production dependency
          direction untouched (test-target dev-dep or gated test inside headless)
Modify    apps/headless tests + Cargo dev-target wiring; NO production Cargo change
Docs      playback-execution-validation contract gains an evidence-mode row IF adopted
Removal   none
Acceptance  the test itself (fixtures with known failure stage families + one healthy
          control); a green run binds the chain; absence of the test keeps the reconciliation C-1
          corrected claim ("constructible, not yet constructed")
```

### 1.9 Text coupling and formal-mapping migration (F10 census; reconciliation §5)

```text
Current   presentation text pinned by ~6 assertion families; decision-table oracle
          freezes the class structure + decode-prefix/device markers; TLA is stage-agnostic;
          render_order_oracle pins source text
Target    on any diagnostic-wording migration: regenerate the decision table
          (QIANQIAN_UPDATE_DECISION_TABLE=1) + rerun specs/check.sh terminal; update the
          census list alongside; keep the fail-closed classify() aligned with the new
          stage families
Modify    tests only (production wording change is whatever motivated the migration)
Docs      none normative; the census list in this file is the living checklist
Removal   old spellings removed in the same commit as the oracle regeneration (no
          dual-spelling window)
Acceptance  TLC refinement PASS + byte-compare gate green after regeneration;
          presentation tests updated in the same change; zero production dispatch on the
          old strings (re-greppable invariant, CE6)
```

### 1.10 Assembly-time §G.6 visibility (U39, F-09)

```text
Current   revise_desired returns Ok even when settle latched Blocked (qianqian-app
          lib.rs:81-85); visible only at dispose/snapshot
Target    (owner-gated, narrow) surface the blocked state in the App-root assembly
          result as an operation outcome/diagnostic; K0 untouched
Modify    qianqian-app lib.rs only
Docs      none normative (K0 design already says "loudly visible blocked state"; this is
          presentation parity)
Acceptance  test: force a teardown violation, assert the assembly result reports it
          while dispose still returns the authoritative verdict
```

### 1.11 Host I/O reason retention via adapter-owned userdata context (T1-6, R1.1)

```text
Current   file_read/file_seek return -1 on any io::Error; the OS detail never crosses
          the callback RETURN value (lib.rs:70-93); open-path Display carries
          (os error N) but mid-read does not
Target    candidate (compared — replaces the prior "not retainable" verdict):
          userdata points at an adapter-private context {file, last_io_error}; the
          callback records the io::Error before returning -1; the adapter clears the
          slot at each binding-level operation start and reads it after the SongCore
          call returns — no ABI or callback-shape change (userdata is binding-created,
          lib.rs:163-168)
Obligations
          before adoption: per-operation association (attribute the recorded error
          to the right binding-level call — read vs seek vs probe); stale-error
          clearing discipline; multiple internal callbacks per call (FFmpeg may issue
          several reads — define first/last-wins); context ownership/lifetime with
          the endpoint (Box<File> becomes Box<IoContext>)
Modify    qianqian-decode-songcore only (callbacks + context + read sites)
Docs      binding-architecture note rides with T1-1/T1-2 (§3.5)
Acceptance  fault-injecting source behind the io seam (E-3's harness): assert the
          retained (kind, raw_os_error) matches the injected failure AND that a
          successful operation clears the slot; counterexample: a stale error
          attributed to a later successful operation
```

## 2. Authority-amendment correspondence table

Only documents actually touched by some candidate goal are listed. "CONDITIONAL" =
amendment text exists below but activates only if the owner adopts the corresponding
option. **No ACCEPTED document is modified by this PR.**

| Document | Touched by | Verdict |
|---|---|---|
| ADR-PBK-001 (foundations) | — (no goal creates a new realtime view or publication overlap; T2-5 content is Class C; P1-P5 untriggered) | NO CHANGE |
| ADR-PBK-002 (D11/D14.*/U2) | C-8/D-4 (drain semantics — resolver precedence is authority-OPEN, AI-7); D14.5 SEEK_UNSUPPORTED reclass (C-9); D14.9 untouched by T5-4 (grading preserved); D14.2 untouched by T1-3/T2-5 (diagnostic stays presentation, control-inertness witnessed); U2 untouched (#277 separate) | NO MANDATORY AMENDMENT; CONDITIONAL candidates §3.2/§3.3 |
| ADR-PBK-003 (Output boundary) | T2-5 — EITHER carrier: the reason handoff crosses the backend-neutral port crate in both shapes (R1.1 path drawing, §1.4; previously scoped to option (b) only) | CONDITIONAL §3.4 |
| songcore-binding-architecture | T1-1/T1-2 (binding-internal retention note; parity unchanged) | NOTE-LEVEL §3.5 (not an amendment) |
| playback-execution-model | T5-4 corrective (hazard rows), §1.7 two-class statement | UPDATE-WHEN-LANDED (evidence doc, non-authority status per its FROZEN scope — its hazard rows describe execution facts; changes ride with the implementation PR) |
| dsp-product-model | — (T3-1 doesn't touch DSP; refuse/apply/fail triad already authoritative) | NO CHANGE |
| composition-kernel-0-design / implementation ADR | D-7 option (a) only | CONDITIONAL §3.6 (requires its own K0 authority review; not drafted as adoptable text) |
| R0 contract (this PR's file) | T5-4 resolution, T2-5 adoption, D-2..D-11 outcomes | updated by future rounds; never promoted to authority by merge |

## 3. Concrete amendment candidate texts (CANDIDATE — not adopted; differences stated)

### 3.1 No mandatory amendments — scope narrowed in R1.1

T1-1/T1-2 internal retention and the T5-4 writer-swap corrective require **zero**
changes to accepted authority: they add no terminal variant, no semantic subclass, no
public control surface, no K0 primitive, and no realtime mechanism. **R1.1 narrowing:
T2-5 no longer belongs in the zero-change list** — its handoff crosses
`qianqian-audio-api` in every carrier (§1.4 path), so the PBK-003 interpretation note
(§3.4) is cited by both D-10 options; whether that note must become an accepted
amendment is part of the D-10 ruling, not pre-decided here. The conditional texts
below exist so that an owner choosing otherwise starts from concrete words, not vibes.

### 3.2 CONDITIONAL — ADR-PBK-002 D14.5, SEEK_UNSUPPORTED provability note

Trigger: owner adopts C-9 reclassification AFTER E-6 evidence. Candidate differential to
D14.5's refusal set (current text lists SEEK_UNSUPPORTED among the destructive-by-name
statuses):

```text
+ CORRECTIVE-F5-5 (CANDIDATE, preconditions unmet): if runtime/FFmpeg-level evidence
  (falsifiable design: reconciliation-r1 §8 E-6) establishes that a failing
  container-has-no-seek seek leaves the decoder continuation bit-identical to a
  never-seeked control — the same oracle shape already earned for INVALID_ARGUMENT —
  then SONG_ERR_SEEK_UNSUPPORTED MAY be moved into the proven-refusal set
  (RefusedUnchanged). Until that evidence exists and this amendment is accepted,
  the conservative destructive routing remains frozen. No product behavior changes
  by this text alone.
```

Difference from current authority: one status moves between named sets, evidence-gated;
the "unprovable means destructive" rule itself is unchanged.

### 3.3 CONDITIONAL — ADR-PBK-002 §17/D11-adjacent, drain-timeout evidence class

Trigger: owner adopts D-4 option (b) after E-5 runtime data. **No text is drafted**:
resolver precedence is explicitly OPEN (ADR-PBK-002:751-754) and the space of honest
options (keep-as-tuning / new evidence class / wording change) depends on measurements
that do not exist. Drafting a candidate now would manufacture authority from zero
evidence — declined by this round.

### 3.4 CONDITIONAL — ADR-PBK-003, AudioOutput diagnostic attachment

Trigger: T2-5 adoption, **either carrier** (the reason handoff crosses the
backend-neutral port crate — `RenderRequest` field or drain-adjacent parameter;
R1.1 §1.4). Candidate differential:

```text
+ (CANDIDATE) The backend-neutral AudioOutput contract MAY carry a bounded,
  control-inert diagnostic attachment on abort delivery (a reason snapshot beside
  the two-valued drain verdict). The attachment is Class-C diagnostic: it MUST NOT
  alter D11 inputs, terminal classification, or any control decision, and its
  absence MUST be legal. Backend identity remains non-semantic (PBK-003 §core);
  the attachment carries no backend-specific type.
```

Difference: one optional diagnostic parameter class on an existing delivery point;
verdict semantics, Output Plugin identity, and backend ownership untouched. R1.1: a
carrier whose cell is read only session-internally still uses a PBK-003-governed port
type for the handoff; the note is written so it covers both carriers without mandating
observation-surface exposure.

2026-10-16 design-round note (DG-2, BASE e44aa80): the recommended carrier is B — the
attachment rides **inside** the drain-verdict value delivered by the existing
`DrainSignal::complete` seam (bounded `AbortReason{class, message}` payload on the
Aborted variant), so the "parameter class on an existing delivery point" wording above
covers it as written: the delivery point is the single completion call, no second cell
or parameter is introduced, and the observer signature is unchanged. The note's
trigger and conditionality are unchanged; only the concrete shape reference is
recorded.

2026-10-17 implementation note: carrier B is now the LANDED shape (PR #287, `b86a9ce`
— `DrainVerdict::Aborted { reason: Option<AbortReason> }`, 512-byte cap, backend-neutral
`OutputAbortClass`). 2026-10-17 owner ruling: D-10 = RATIFIED_CARRIER_B and this
candidate note is ADOPTED as the PBK-003 §5 minimal clarification (applied verbatim in
substance as an Amendment note under TERMINAL EVIDENCE; receipt
[PR #291 comment 6229433676](https://github.com/jnhu76/qianqian/pull/291#issuecomment-6229433676),
decision-record §6.2/§6.3). This section remains the historical candidate record; the
normative home of the adopted text is now ADR-PBK-003 §5.

### 3.5 NOTE-LEVEL — songcore-binding-architecture

If T1-1/T1-2 are adopted, add one paragraph to the binding-class notes: "the reference
Rust binding retains typed status/native_code internally for diagnostics; this is a
binding-internal realization choice, not a binding-parity obligation; the canonical ABI
(songcore.h) is unchanged." No authority hierarchy change. (2026-10-17: T1-1/T1-2
substance landed via #287, and the D-2 ruling — RATIFIED_OPTION_A, receipt
[PR #291 comment 6229433676](https://github.com/jnhu76/qianqian/pull/291#issuecomment-6229433676) —
adopts this note; it is applied in songcore-binding-architecture §6.2 as the
binding-internal-retention note plus two classified table rows. This section remains
the historical candidate record.)

### 3.6 K0 documents — intentionally not drafted

D-7 option (a) (catch→Violated at the dispose boundary) would alter §G.6's violation
semantics — a kernel semantic change that must originate in a K0 authority review with
its own falsifiable counterexamples, not in an error-system document. This round records
the posture (b) (crash-propagation, documented) as the honest default.

## 4. Acceptance-by-nature (how adoptions would be proven)

Principles: each adoption's acceptance is the counterexample that would expose its
violation, executed through the real boundary the claim names; test count, repeat stress
runs, and all-green suites are not proof. The mechanism x risk obligations of
TEST-QUALITY-STANDARD-v1 apply to any future test authoring this plan induces.

| Adoption | Property | Nature of evidence | Counterexample that fails it |
|---|---|---|---|
| T1-1/T1-2 | retention fidelity, copy-before-next-call | binding-crate unit tests on synthesized faults + garbage fixture (real ABI) | public diff ≠ ∅; a stale snapshot after a newer same-handle call |
| T1-6 userdata context (if adopted) | OS-reason fidelity + slot hygiene | E-3's fault-injecting harness: retained (kind, raw_os_error) matches the injected failure; successful operations clear the slot | a stale error attributed to a later successful operation |
| T2-5 (either carrier) | verdict-only settlement | mock-abort tests, snapshot present vs absent | terminal differs when the snapshot is dropped |
| T5-4 (writer-swap tier, #8/#9/#10) | panic-escalation removal only | E-7 tier 1: broken-sink control per site × {gate on, off}, identical terminal/exit path vs sink-healthy; blocked-sink control DOCUMENTS the residual delay (not isolation) | any terminal/exit-path change keyed on sink state; residual blocking presented as isolation |
| T5 framework (future — diagnostic outlets only, incl. #11; R1.2) | producer/terminal independence from DIAGNOSTIC sink state | E-7 tier 2: four disposition-contract §6 controls per diagnostic outlet | any semantic change (episode class, exit path beyond documented ones) |
| Protocol writes (#5) | conformance to the documented host-failure classes (NOT terminal invariance) | E-9 (R1.2): broken/closed stdout+stderr harness asserting the T5-7 classes | a protocol-write panic escaping the catch; protocol rows silently dropped like diagnostics; seal proceeding unacknowledged |
| T3-1 (if adopted — AFTER the observability gap is closed by an ABI/SongCore amendment, R1.1) | witness inertness | interleave test (E-1) asserting classification never reads the witness | any Failed/Completed divergence caused by the witness |
| E-2 entry | real-chain binding | one feature-gated end-to-end on curated fixtures + healthy control | stage-family mismatch or hang |
| §1.9 migration | oracle coherence | table regeneration + TLC refinement + census update in one commit | byte-compare red; unfrozen stage panic; stale text assertions |
| §1.10 | assembly feedback | forced-violation test | Ok(()) returned with a latched Blocked |

## 5. Independent-review entry points

For a fresh reviewer picking this package up:

1. **Fastest falsification targets** (reconciliation HANDOFF mirrors): re-derive reconciliation §2.3 row 2
   (seek erases the latch) from songcore_ffmpeg.c:1111-1156/1161-1226 + session.rs:601-643 —
   the single most consequential correction this round makes to scan A. R1.1 adds a
   second: re-derive the stop × pending-fatal commit race (reconciliation §2.3.1) from
   completion.rs:1238-1262 (worker_failure precedence; request_stop commits nothing)
   + session.rs:827-893 (no terminal check between loop-top and read; unconditional
   `decode_failed` in the Err arm) + edge.rs:233-239 — confirming that in the default
   worker position the pending fatal beats the stop, the opposite of the pre-R1.1
   wording.
2. **N-01**: read wasapi.rs:240-273 and confirm the :261 print is outside both catches;
   then confirm finish_episode's order (entry.rs:418-419) makes the consequence a
   pre-dispose hang.
3. **C-1**: check root Cargo.toml exclude + apps/headless/Cargo.toml features +
   assembly.rs:24-37 to verify "constructible, not constructed".
4. **C-7**: decision_table_oracle.rs classify vs EpisodeTerminalSettlementTable.tla
   OutcomeClassOf — confirm TLA is stage-agnostic.
5. **R0 attribution**: the only repository record of the prior review is the owner PR
   comment; this package's wording ("prior static review, identity unverified") should be
   checked against that record, not against the removed "independent Astra" phrasing.
6. **DeepSeek mapping**: spot-check two verdicts against the pinned REFERENCE_SHA
   (R1 code/message split in error.ts:13-22; R7 containment at fiber.ts:120-137).
7. **Scope guard**: verify no file in this PR touches production code, tests, build
   config, or any ACCEPTED authority document, and that every candidate is labeled.

## 6. Round boundary

Delivered by this PR round: the three companion documents + R0 attribution/index
update; R1.1 (first audit round) corrected all eight REVISE findings in place — the
stop × pending-fatal commit race (§1.3), the T2-5 delivery path (§1.4), the unified
cone remediation table with #9 included and panic/blocking separated (§1.5), the
userdata retention candidate (§1.11), the narrowed zero-amendment claim (§3.1/§3.4),
the E-1/E-5/E-6 redesigns and the E-7 two-tier split (§4). R1.2 (second audit round,
six findings) further corrected: the §2.3.1 stop-exit list rebuilt from actual control
flow (a canceled pending seek is release-only bookkeeping, not an exit; the zero-frame
arm is window-unreachable with the real provider), the audibility vocabulary extended
to evidence levels with no audibility asserted, E-1 attribution decoupled from message
text, E-7 tier 2 narrowed to diagnostic outlets with E-9 carrying protocol-write
conformance, the queue-cost claims corrected against the pinned toolchain's std (no
advantage verdict asserted), and T5-8's chain-to-previous made a condition rather
than an unconditional duty. R1.3 (third audit: **APPROVE_DESIGN_CANDIDATE**, one
non-blocking P3) applied the T2-2 lock-hold wording fix (notify after the guard is
dropped), added E-9's three-mode stdout checkpoint (flush-Err currently ignored and
unconstructed — a checkpoint, not a proven defect), and executed the owner's rename
instruction: stable filenames without round suffixes, links and shorthand references
updated. The approval is design-candidate level only — it authorizes no
implementation and no merge; the OPEN items (D-2..D-11, exporter exit mechanism,
hook isolation, teardown panic posture) are the entry points for future design and
implementation work and are not resolved by any green gate. Not delivered, by
explicit constraint: production changes, test changes, build changes, authority
status changes, experiment execution, issue closure, merge. The NEEDS_RUNTIME_EVIDENCE
registry (reconciliation §8) is the bridge to any future evidence round;
OPEN-DECISION D-2..D-11 are the bridge to any future owner ruling.

---

## 7. Verification-instrument designs (2026-10-16 design round, BASE_SHA e44aa80)

Designs only — **no oracle, harness or production injection seam is implemented by
this round**. Every item below carries the six-question authoring record of
TEST-QUALITY-STANDARD-v1 (property authority / precondition witness / schedule
construction / settlement observation / failure signature / negative control /
cleanup-timeout evidence boundary) so the implementing round writes tests, not
rediscovery. The real-cone oracles share their fault-injection machinery with E-7
tier 1 (build them together, per #283 CC-1/D-E).

> **Implementation status (2026-10-16)**: BUILT, NOT RUN AT RUNTIME — the oracle lives in
> `crates/qianqian-output-wasapi/src/cone_oracle_tests.rs` (Windows-only, real endpoint),
> compile-checked for `x86_64-pc-windows-gnu`; its two negative-control mutations are
> documented there and not executed (no Windows host in this round). See
> `error-system-implementation-record.md` §5.

### 7.1 O-CONE-NECESSITY — the real failure-publication cone must prove itself without external help

**Property authority.** D11 settlement ownership (PBK-002 §17): the cone's own
`render_input.stop()` + `drain.complete()` (wasapi.rs:263-273) must propagate a
render-leg failure to a waiting host with **no external stop/dispose** —
`wait_terminal` returns and the episode commits `Failed{device}`. This is the
machine-progress premise the terminal model's FinishTeardown guard 1 now records as
an environment assumption (#283 AF-1); the oracle is what makes that assumption
executable.

**Why existing tests cannot serve (#283 REV.2, re-verified).** The never-draining
teardown oracle binds `TestOutput`, whose own test leg calls `input.stop()` /
`drain.complete()` (tests/common/mod.rs:805-810) and which drives `dispose()` itself
while teardown's decode-relation `edge.stop()` ends the episode independently — no
line of wasapi.rs executes. A new oracle must execute the real cone.

**Real-cone drive — corrected platform facts (R2 review 2026-10-16).** The earlier
"T1 — platform-neutral open-failure cone via the real `run_render_thread` on
non-Windows" tier is **withdrawn as unexecutable**: the whole `wasapi` module is
`#[cfg(windows)]` (output-wasapi lib.rs:35-36), so on every other platform the cone
code does not exist in the binary at all — the Output Plugin's activation refuses
at `selected_backend()` with an explicit platform error (lib.rs:67-75), an
*activation* failure (K0 `activation_error`), never a live episode. Equally, an
open failure — even on Windows, `OpenVerdict::Failed`/timeout →
`abort_render_thread` + `Err(OutputError)` (wasapi.rs:168-179) — is
activation-stage evidence: it must never be asserted as a live episode's
`wait_terminal → Failed{device}`. No platform-neutral real-cone tier exists; an
oracle on non-Windows would verify nothing. The corrected tiers:

| Tier | How the abort is produced | What is real | Cost / limit |
|---|---|---|---|
| **N-A — Windows real-path steady-loop abort (the necessity oracle)** | open succeeds on a real device; a `RenderPcmInput` **wrapper** (test-owned, wrapping the session's real edge-backed input) panics on its k-th `read_frames` → the real catch → `Aborted` → the real post-catch `render_input.stop()` + `drain.complete()` + join all execute | the entire production cone: catch, stop, complete, steady loop, device release, join | needs a Windows host; the injected fault is at the input-trait boundary — the cone itself is unmodified production code |
| **N-B — activation-abort ordering (separate property, optional)** | on Windows, force `OpenVerdict::Failed`/timeout and pin `abort_render_thread`'s own ordering (input stop + gate release before join, wasapi.rs:168-179) | the open-abort path only | **not a D11 property**: observed at the activation boundary (`Err(OutputError)` / activation error), never via `wait_terminal` |

If off-Windows schedule control for the ordering *discipline* (catch→stop→complete
shape, not backend integration) is ever wanted, the only honest route is an
explicitly-marked mechanism-extraction seam — a copy of the cone shape — which this
design does not recommend and which AGENTS.md's real-path rules exclude as evidence
for any host-backend claim. The TestOutput-based never-draining teardown oracle
remains session-level coverage that cannot speak for wasapi.rs (unchanged from the
REV.2 withdrawal).

**Precondition witnesses (no sleep-based guessing; corrected set — R4 review
2026-10-16).** (i) The wrapper's k-th read has actually been entered and
panicked: the wrapper sets a test-visible flag immediately before `panic!`
(same thread, happens-before the catch). (ii) The k−1 prior deliveries
completed: the wrapper's own call counter (a real observation on the
consumer thread). (iii) The frozen-consumer / no-EOF / no-fault regime, each
element a script or fixture fact cited in the authoring record: the provider
never returns `Eof`, never returns a decode error, never returns the
zero-frame stall response, no seek command exists anywhere in the trace, and
the test body between (i) and settlement contains no
`request_stop`/`dispose`/`edge.stop` call (structural — the harness's single
code path).
**Withdrawn precondition** (R4): the R3 form of (ii) — "the producer is in
the target capacity wait, witnessed by produced blocks r minus consumed
slices k−1 ≥ C_blocks". Three objections, all correct against the real
pipeline: staging-block frame counts and render-pull frame counts are
different units, so the two counters cannot be subtracted; the difference
also contains produced-but-unwritten staging and any retained remainder, so
even same-unit frame arithmetic would not be edge occupancy; and a
genuinely full edge still would not prove the worker has executed into
`wait_for_space`. The R3-withdrawn "unsettled ⇒ parked in `wait_terminal`"
precondition stays withdrawn: the detection is "settlement becomes decisive
without external stop", observed by a harness thread whose `wait_terminal`
call (issued before or after the injection, both covered by the bound)
returns only if the outcome committed.

**Necessity proof — outcome level, no parked-at-injection claim (R4).** The
oracle no longer proves *where* the worker stands at injection; it proves
that under the constructed regime the worker has **no legal exit unless the
edge is terminalized**:

- The injection **is** the consumer freeze (program order on the render
  thread, not a scheduling assumption): the k-th `read_frames` panics inside
  the render thread, the panic unwinds into the cone's `catch_unwind`, the
  render loop is dead — edge consumption stays frozen at the k−1 completed
  calls for the rest of the run.
- The decode worker's exits are enumerable (`decode_worker`,
  session.rs:576-931): the read-Err arm (:889-893), the EOF arm (:885-888),
  the two processing-failure arms (:820-824/:849-860), the panic path
  (:897-910), the MutatedThenFailed seek arm (:656-668), and the
  terminal-observed exits — mid-block `WriteStep::Stopped` (:986-987),
  mid-remainder return (:798-801), the zero-frame terminal check (:831-835),
  and the two Applied-loop terminal returns (:718-728/:746-765). The regime
  excludes every non-terminal exit (no decode fault, no EOF, default no-op
  processor, healthy fixture, no seek command ⇒ the MutatedThenFailed and
  Applied arms are unreachable), and every terminal-observed exit requires
  `edge.terminal() != Open`.
- Therefore: with the cone's `render_input.stop()` deleted and no external
  stop, nothing ever makes the edge terminal ⇒ the worker cannot exit ⇒
  `worker_exited(...)` is never published ⇒ the settle never becomes
  decisive ⇒ `wait_terminal` never returns within the harness bound.

The proof deliberately does not use the edge's occupancy or the worker's
position at injection: mid-write, in `wait_for_space`, or between
operations — the exit enumeration forbids exit in all three cases.

**Schedule construction.** Tier N-A: activate on Windows → confirm `Opened`
verdict via the open slot (witness) → the wrapper delivers k−1 healthy
slices (real consumption; the edge fills against the eventually-frozen
consumer, but the proof above never uses its occupancy) → k-th read panics
(flag) → cone → settlement. Tier N-B: drive
an open failure/timeout and observe at the activation boundary only (its own
ordering assertion; never `wait_terminal`).

**Settlement observation points.** `wait_terminal` returns; outcome is
`Failed{device}`; the drain mirror holds `Aborted`; the worker-exit evidence arrived
(worker terminal published — see the negative control for why this is load-bearing);
the render-thread join returned (the cone's own `stop_and_join` consumer: the session
teardown relation, observed after settle via `dispose` completing).

**Failure signature (healthy run).** Any of: `wait_terminal` does not return within
the harness bound; outcome ≠ `Failed{device}`; drain mirror ≠ `Aborted`; the abort
flag never set (injection misfire — TOOLING-FAIL, not a pass).

**Negative control — deletion of `render_input.stop()` (wasapi.rs:266) must be
caught.** The mutation is executed under the same frozen-consumer / no-EOF /
no-fault regime as the healthy run (witnesses (i)–(iii)); the R3 gating
"witnessed capacity-wait condition" is withdrawn with the arithmetic it stood
on. Why it must go red — by the exit enumeration above, not by any
parked-ness claim: with the
cone's stop deleted and no external stop, the worker has no legal exit (only
terminal-observed exits remain, and nothing terminalizes the edge), so
`worker_exited(...)`
is never published; `Aborted + worker=None` is outside the decisive domain (the
decision contract returns None — model EvidenceConsistent corner, README
"activation 失败下无证据"), and with no external stop the settle never becomes
decisive → `wait_terminal` never returns → harness bound fires. The observation
that catches the mutation is therefore **worker-exit evidence + wait_terminal
return under a no-external-stop regime** — exactly the healthy oracle's
assertions. Executed only in an isolated environment (never committed to the
final branch).

**Cleanup/timeout evidence boundary.** The harness bound is generous and its firing
is reported as the failure signature (it IS the oracle's red); a timed-out healthy
run is inconclusive, not pass; cleanup after a failed run (dispose with external
stop) is used only to release the fixture — the post-cleanup state is never asserted
as the success path (a cleanup path masquerading as success is the classic
self-driving-`dispose` failure the REV.2 withdrawal documented).

### 7.2 O-CONE-ORDER — stop-before-complete is a pinned production order, observed at runtime

**Property authority.** Not a D11 semantic: it is the cone's internal ordering
discipline (`render_input.stop()` at wasapi.rs:266 precedes `drain.complete()` at
:270) whose consumers are the session's mirror→resolver and the teardown relation.
The contract being pinned: the verdict becomes visible to observers only after the
PCM edge has been cut, so a settled episode never has a live producer writing into
the edge its verdict describes. (Today this order is pinned only as source text by
the render_order_oracle; this oracle adds the runtime pin.)

**Risk being guarded.** With complete-before-stop, the verdict observer (session
mirror) and any dispose-driving waiter can race a still-open edge: the worker keeps
producing into an edge whose episode has already published `Aborted` — the settle
waits on worker evidence that only the (delayed) stop will produce, so the visible
symptom is an unbounded settle latency window plus a semantically awkward
"settled-verdict + live-producer" overlap that no current invariant forbids but no
current authority licenses.

**Observation of the two production events (program-order counters; R2 review
2026-10-16).** The earlier witness — "the wrapper's blocked `read_frames` returns,
and its return path bumps counter A" — is **withdrawn**: in the panic tier the
panicking read never returns (the panic unwound it), and any *other* reader's
wakeup order relative to the verdict observer is a scheduling fact, not program
order — legal code could bump B first. The corrected witnesses:

- **A — edge-cut witness, recorded where the real stop completes.** The test's
  `RenderPcmInput` wrapper implements `stop()` as *forward to the real input's
  `stop()`, then bump counter A* — on whichever thread invoked it. In the cone the
  invoker is the render thread itself, in the post-catch arm, strictly before
  `drain.complete()` (wasapi.rs:263-273): A's increment happens-before the verdict
  observer by same-thread program order — no scheduling dependence at all.
- **B — verdict witness.** A `DrainSignal` observer (the existing
  `with_on_complete` seam, ports.rs:122-133) runs synchronously inside
  `complete()`, before it returns (ports.rs:135-155): it snapshots counter A and
  bumps B.

Assertion at the settlement boundary (after `wait_terminal` returns): on an
Aborted-class outcome, **B's snapshot of A must be ≥ 1** — the verdict became
visible only after the edge cut completed. Counter A is monotonic and shared with
the other legitimate stop callers (`stop_and_join` forwards through the same
wrapper, wasapi.rs:206-213), so later extra increments cannot weaken the
assertion.

**Mutation and its specific failure criterion.** Swap the two lines (complete
before stop): the observer runs while the cone's stop arm has not executed — B
snapshots A == 0 → red. Deleting the stop call: A never increments on the render
thread — B snapshots A == 0 → red (the same assertion also covers deletion;
§7.1's wedge-based control additionally catches the producer-stranding
consequence). **Deleting a call is not order verification** — necessity and
ordering remain two controls.

**Authoring record.** Precondition witness: activation settled + wrapper's k-th read
entered (flag before panic). Schedule: the N-A drive (§7.1, Windows) — the order
oracle is **N-A-only, honestly scoped**: the injected panic is what forces the
post-catch stop arm, and only the real Windows path executes the cone.
Settlement observation: `wait_terminal` return, then compare the snapshot recorded
by witness B. Failure signature: B's snapshot of A is 0 on an Aborted-class
outcome. Negative control: the line-swap and line-deletion mutations (isolated
environment). Cleanup boundary: as §7.1.

### 7.3 E-7 tier 1 — writer-swap negative controls per cone site (#8/#9/#10)

**Property authority.** T5-4 candidate (disposition-contract §6 firewall; target
M5): a diagnostic-write failure must never change the terminal class, the exit path,
or the wakeup ordering — measured per site × {`QIANQIAN_AUDIO_LOG` on, off}.

**Site-reached proof (#8/#9/#10 each).** #8: the N-A abort drive (§7.1) — the post-catch
print executes because the catch ran (witness: abort flag). #9: the opened-line print
executes only after the `Opened` verdict publishes (witness: open-slot verdict read);
forcing it requires a successful Windows open. #10: the volume branch executes only
on a `SetAllVolumes` failure — constructible on Windows by an invalid-channel-mask
session? Honest scoping: forcing a real SetAllVolumes failure deterministically needs
a fault seam at the volume call (test-only wrapper of the COM binding or a
headless-only build flag); the design names this as the one site whose reachability
needs a small injection seam in `qianqian-output-wasapi` (same machinery tier as
§7.1's input wrapper — a volume-apply wrapper at the session boundary, not inside the
cone).

**Sink classes — separated, never conflated.** (i) **broken sink**: stderr fd is a
pipe whose read end is closed (EPIPE) — `eprintln!` panics today; expected
classification **before** the D-3 swap: #8 → N-01 hang (`wait_terminal` never
returns — the harness bound fires and that IS the recorded hazard); #10 →
`Failed{device}` misattribution (AF-4); #9 → post-activation abort or N-01 cascade.
**After** the swap: all three sites behave identically to the sink-healthy control
(terminal class / exit path / wakeup ordering byte-identical), with the log line
lost and counted. (ii) **blocked sink**: stderr is a full pipe with no reader —
writes block; the control **documents the residual delay** (first-print latency,
settled-vs-printed gap) and never presents it as isolation. (iii) gate off: no print
executes at all — controls that the gate genuinely guards the sites.

**Subprocess isolation.** Global stdout/stderr manipulation (closed/full pipes) is
process-global state: each configuration runs in a **child process** (the headless
binary or a dedicated test binary main), the parent setting up the pipes before
spawn and observing exit code/timing/stdout protocol rows. In-process parallel
tests are never exposed to the manipulated fds. The synchronous writer-swap's
blocking residual is honestly retained in every report (H).

**Authoring record.** Precondition witness: child-process liveness + the site-reached
witnesses above. Schedule: static per configuration (no interleaving needed — the
hazard is unconditional under the sink condition). Settlement observation: parent
observes child exit (or bound timeout for the N-01 hang class — the hang IS the
pre-swap expected outcome, recorded, not retried away). Failure signature: any
post-swap divergence between broken-sink and sink-healthy runs. Negative control:
pre-swap runs (the hang/misattribution must be reproduced — proving the harness can
see the hazard it claims to guard). Cleanup: child processes are reaped by the
parent; the evidence boundary is the recorded per-configuration transcript.

### 7.4 E1-ABI — real SongCore latch calibration at the ABI boundary
> **Implementation status (2026-10-16)**: EXECUTED — 8 tests green including both gate
> legs; recorded in `error-system-implementation-record.md` §4.


**Property authority.** The ABI's partial-success/pending-fatal freeze (songcore.h;
reconciliation §2.1) — not any D11 claim. E1-ABI witnesses what the C core actually
does; the worker/session-level consequences are E1-WORKER's.

**Harness shape (no production change).** The binding crate's test target links the
real native lib and constructs `song_io` **directly** with test-owned callbacks that
serve crafted bytes from an in-memory fixture — the injection seam is the io-callback
boundary the ABI already defines (the same seam E-3 uses). Determinism: the fixture
is a byte vector; "corruption" is a chosen offset range; no real filesystem.

**Calibration step (mandatory precondition; corrected logic — R2 review
2026-10-16).** A healthy control proves only the healthy path; the partial-success
shape is established by the **fault configuration**, never asserted beside it. Both
legs must be green before any experiment row is recorded: (i) *healthy control* —
the same container/codec with **no** corrupted region decodes to clean EOF with
`SONG_OK` throughout (proves the fixture and harness are sound); (ii) *fault
configuration* — the corrupted-region fixture yields the partial-success shape:
frames > 0 returned with `SONG_OK`, then a latched fatal on the next read
(decoder-dependent — R1.2 precondition (iii)). The chosen container/codec plus both
fixture hashes are part of the evidence artifact.

**Independent latch proof (three separate observations, not message-text
identification).** (i) *latch exists*: read#2 hits the corrupt region and fails;
read#3 — with **no new corrupt bytes consumed between #2 and #3** — fails again
with the same typed status; identification is by injection record + behavior, with
the "pending decode error" text as corroboration only (R1.2: message text is never
the identity). (ii) *Applied actually happened*: a `song_seek` past the corruption
returns `SONG_OK` (Applied), and post-cut reads deliver **healthy** frames —
witnessed against a control twin file (same bytes minus the corruption) with a
bit-exact continuation comparison. (iii) *post-cut region health*: the twin
comparison above, on a bounded read budget.

**Destination matrix (rows the harness records).** With a latch pending:
next `song_read_pcm` → re-published fatal (status preserved; message text generic —
the C-side `ok(h)` clear on partial success means the first-failure text is already
lost at the latch; typed status + native_code survive); `song_seek` Applied → latch
erased (row 2: post-cut reads healthy); `song_seek` Refused (INVALID_ARGUMENT) →
latch retained (next read re-fails); `song_seek` Mutated (generic SEEK_ERROR) →
failure replaces/continues per the C-side seek latch; `song_close` → clean teardown;
session-level stop → not an ABI concern (E1-WORKER's row).

**RefusedUnchanged production-reachability honesty.** The ABI harness CAN construct
refused seeks (bogus arguments), but the real adapter only ever passes user-target
durations through `classify_seek_status` — a real-adapter RefusedUnchanged requires
a status the production path can actually produce; the harness record therefore
never cites its own INVALID_ARGUMENT row as proof that production reaches
RefusedUnchanged (that would be forging reachability).

**Seam constructibility.** Fully constructible today: test-side callbacks + the
excluded binding crate's existing native linkage; **no test-side native support
beyond linking is required** (no new C symbols, no fixture hooks in SongCore).

### 7.5 E1-WORKER — deterministic interleavings on the real Session/worker
> **Implementation status (2026-10-16)**: EXECUTED — W1–W6 plus the DG-2 pair, 12 tests,
> every trace in its predicted §2.3.1 cell. The fence the design required landed inside
> `request_stop` (`completion.rs`), one arming point, test-only. See
> `error-system-implementation-record.md` §4.


**Property authority.** Reconciliation §2.3.1 (commit ordering, exit-surface
enumeration) + PBK-002 D11/D14.5: which terminal class a stop-in-window episode
commits, and that no canceled-seek is misread as worker exit. The stop-race half is
RUST_VERIFICATION_PREFERRED (#283 FORMAL-PLAN §3.2): the harness observes the real
locks and the real exit surface.

**Harness shape.** Real `PlaybackSession` with its real worker loop, real
completion, real edge; the **decode provider is scripted** — a test `PcmDecode`
implementation whose `read_frames`/`seek` follow a script with park points
(barriers the test controls). Script-provider protocol validation is thereby
distinguished from the real C latch validation (E1-ABI): the two harnesses
together cover both halves; neither substitutes for the other.

**Seam inventory — what exists vs what must be added (R2 review 2026-10-16).**
Exists today: the scripted decode provider (the stateful-probe pattern), completion
white-box snapshots (the settlement-contract seam), `with_on_complete`, edge
terminal reads, and the established **test-only fence pattern** (completion.rs
`test_sync`, `#[cfg(all(test, not(loom)))]`, built for the seek straddle window —
armed for one completion, inert everywhere else, absent from production builds).
Must be added (the only addition this design requires): one fence arming point
covering the window inside `request_stop` between the completion-lock release and
its **own** `edge.stop()` (completion.rs:552-562) — the same pattern, not a new
mechanism. Nothing is added for remainder observability: `remainder` is a
**worker-local** variable (session.rs:607), not session state — the earlier W2
witness "session white-box remainder state" named a position that does not exist
and is withdrawn; the traces below witness it through provider-script state and
edge content instead.

**Real execution positions the traces must respect (all verified at HEAD).**
(a) `request_stop` records intent under the completion lock, releases the gate,
drops the lock, and then calls **its own** `edge.stop()` before returning
(completion.rs:552-562) — there is no "request_stop returns while deferring the
teardown relation's edge stop" position (withdrawn). (b) The worker finishes a
preserved remainder **before** any new decode (session.rs:775-805, "Finish a
preserved remainder BEFORE any new decode") — a read-time error never coexists
with a held remainder; the only failure-with-retained-remainder position is inside
the seek serialization point (`MutatedThenFailed` with `remainder.is_some()`).

**Trace family (each rebuilt on a real position; sync points and observability).**

| Trace | Real position × injected event | Witnesses | Expected commit |
|---|---|---|---|
| W1 loop-top fatal | script parks before first read; release; `read_frames` returns Err → `decode_failed` + `edge.fail()` + return (session.rs:864-870) | script park flag; completion white-box `decode_failure` set before worker exit | `Failed{decode}` |
| W2 destructive-seek failure with retained remainder | script produces a cut point (write parks the leg mid-block, remainder retained), then the serialization-point `seek` returns `MutatedThenFailed` → `decode_failed` + return, remainder discarded with the pre-cut world (session.rs:655-663) | provider script state (it returned MutatedThenFailed after the cut); post-exit edge content — no remainder white-box exists (worker-local) | `Failed{decode}`; the retained remainder never surfaces |
| W3 stop during remainder flush | remainder flushing into a full edge (capacity-wait park, session.rs:786-805); `request_stop` → edge terminal ≠ Open → the mid-remainder exit returns | flush interruption is terminal-observed: `write_some` == 0 ∧ `edge.terminal() != Open`; worker-exit publication | `Stopped` — one of the two true stop exits |
| W4 stop-intent recorded, request_stop's own edge stop not yet executed | the real window: completion lock released, `edge.stop()` (its own, completion.rs:552-562) not yet run — constructed by parking the real request at the new fence arming point (seam inventory above) | at the fence park, `stop_requested == true` ∧ `edge.terminal() == Open` holds by construction (the fence IS inside the window) | worker continues to its next boundary; commit per §2.3.1 precedence (`Failed{decode}` if a pending fatal is decisive first, `Stopped` at a mid-write capacity-wait exit) |
| W5a stop during provider call — `Applied` | shared setup: an OLD pending-fatal latch is latched (script state); script parks inside the serialization-point `seek`; test issues stop (drain `Aborted` publishes, non-decisive while the worker sits in the provider); release → `Applied` → **old latch erased** (the ABI's Applied row; real-latch proof belongs to E1-ABI — here the script emulates it) → cutover decision sees stop → `Aborted` (completion.rs:856-869 `episode_ending` → `CutoverDecision::Aborted`) → fall-through; the next read observes the stopped edge terminal → exit | script park flag; `stop_requested` sampled before release; script's post-cut script hands the leg a healthy slice (never a fault) | **`Stopped`** — and `Failed{decode}` here has NO legal source (no new fault, latch erased): it would prove a script protocol violation or an implementation defect, and must fail the oracle, not widen the allowed set |
| W5b-N stop during provider call — `RefusedUnchanged`, **no remainder** | construction (R5: the R4 "first command + read-park" form is withdrawn — a park inside the provider read lets that same first seek be observed by the following `write_observing_seek` pass and create its own remainder, session.rs:966-981, exactly the W5b-R route): (1) leg NOT parked, no seek command ever issued, prior blocks all written `Whole`; (2) plant the seek command (first ever); (3) from here the script provider returns **zero-frame reads** — session.rs:831-835 is the arm that deliberately runs NO write: the worker spins loop-top → serialization point (park not yet true → no provider call) → read → `Frames(0)` → loop-top, and `write_observing_seek` never executes while the command pends; (4) the test sets the park latch (completion white-box seam) **after the last non-zero read**; (5) the next serialization-point pass sees parked ∧ edge Open ∧ not aborted → `decode_stream.seek(target)` → the script parks inside the provider seek; (6) test issues stop (`request_stop`'s own `edge.stop()` terminalizes the edge); release → refusal → **latch retained** (ABI row) → leg released → remainder **provably `None`** — no `write_observing_seek` loop-top ever evaluated `observed ∧ parked` (witness: the provider call history shows whole blocks before the plant, only zero-frames between plant and park, and the park latch was set after the last non-zero read; a CutPoint requires a write pass, and none ran with the command present) → flush skipped → no processing update → the next `read_frames` returns the OLD latched fatal (the refusal-preserved latch resurfacing — the script's emulation of the ABI row) → `decode_failed` (session.rs:889-893) | script park flag; provider call history (whole blocks → zero-frames → parked seek → fatal read); park latch set after the last non-zero read; refusal recorded; fatal-read queued in the script | **`Failed{decode}`** — single legal outcome; `Stopped` here would mean the fatal read never ran (script protocol violation or an implementation defect, not a widened allowed set) |
| W5b-R stop during provider call — `RefusedUnchanged`, **retained remainder** | construction (W2's first half, then the park): seek #1 becomes observable during a mid-flight block write with the leg parked → `WriteStep::CutPoint` → the unwritten tail preserved as `remainder`; the loop top promotes it into the serialization point; script parks inside the provider `seek`; test issues stop — `request_stop`'s own `edge.stop()` (completion.rs:552-562) terminalizes the edge before the release; release → refusal → latch retained but never reached → the loop-top remainder flush runs first (session.rs:786-804): `write_some` == 0 ∧ terminal ≠ Open → the mid-remainder `return` (:798-801) | script park flag; CutPoint observed (the block was split); `stop_requested` sampled before release | **`Stopped`** — the same terminal-observed exit shape as W3; the retained old latch is dropped with the old world (like W2's remainder, it never surfaces) |
| W5c stop during provider call — `MutatedThenFailed` | same park+stop; release → destructive outcome → the serialization point's destructive arm fires immediately (session.rs:655-663) | script park flag + destructive outcome | **`Failed{decode}`** |
| W6 canceled seek is not an exit | issue seek, then stop before the worker picks it up; the serialization point's `seek_aborted` branch releases the canceled seek (session.rs:625-630) and **falls through** to production | worker still alive at the next script park point | no worker exit; final class from the actual decisive evidence |

**No-sleep discipline.** Every schedule is constructed through real synchronization
seams (script parks, completion white-box snapshots, edge terminal reads); the
authoring record for each trace names the seam that proves the precondition, per
`development/concurrency-test-design.md`. No generic scheduler or DSL is proposed —
the scripts are plain test code with barriers.

**Failure signatures.** Any committed class outside the §2.3.1 cell for the observed
worker position/decisive evidence; a canceled-seek path that exits the worker; a
worker exit with no `worker_exited` publication.

**Negative control (design-level).** Both one-directional over-strong oracle
shapes must fail on the split family. "Stop in window ⇒ `Stopped`" asserted
unconditionally fails on W5b-N/W5c (a stop was issued, the episode still
commits `Failed{decode}` — the old retained latch / the destructive arm are
legitimate decisive evidence). "Stop during provider call ⇒ `Failed{decode}`"
fails on W5b-R (R4: after a refusal the worker flushes a preserved remainder
BEFORE any new read — session.rs:786-804 — and a stop that terminalized the
edge turns that flush into the legal mid-remainder `Stopped` exit; the old
latch never surfaces). Each subcase's precondition is what separates them:
W5b-N's zero-frame-loop construction witnesses "no remainder" (no
`write_observing_seek` pass ever ran with the command present — the
provider call history plus the park-latch ordering prove it); W5b-R's
CutPoint construction witnesses "remainder present" — neither may borrow
the other's outcome. W4's own expectation is proven by W4's
fence-position construction alone (R3 review: one trace's broad outcome set may
not serve as another's proof); W5a's single-legal-outcome row (`Stopped`, with
`Failed{decode}` a signature) is what pins it from the Applied side.

### 7.6 F5 model-extension recommendation (GAP-01, D-C)
> **Implementation status (2026-10-16)**: GATE SATISFIED, EXTENSION NOT BUILT — the
> acceptance-gated trigger now holds (E1-ABI green on both legs; W1–W6 all in their
> predicted cells, verified this round). The extension itself stays with the owner's D-C
> weighing (residual interleaving risk vs harness adequacy vs model maintenance cost);
> no model action was changed and the F5 suite passes unchanged. See
> `error-system-implementation-record.md` §6.


**Recommendation: EXTEND_EXISTING_MODEL** — extend `f5-seek-discontinuity` with the
pending-fatal latch variables per #283 FORMAL-PLAN §3.1 (Q5 minimal variables, Q6
lock-split atomicity, Q9 scoped safety incl. the Applied-erasure witness, Q10
trace-first mutations D-T1..D-T4, Q11 retained witnesses), **conditional on an
acceptance-gated trigger, and with the stop-race half still primarily
Rust-verified**.

Why not `RUST_VERIFICATION_SUFFICIENT_WITH_STATED_SCOPE`: E1-WORKER as designed
(§7.5) enumerates six trace families; the GAP-01 matrix is a cross product
(workerPos × stop timing × provider class × latch state — on the order of a hundred
cells). The properties at stake ("for **every** interleaving the committed cell is
legal; no post-Applied old-cursor resume; latch retained after Refused; latch never
surfaces after Applied") are exhaustive small-state collision claims — exactly the
formalization policy's entry question, and the F5 model already owns the seek
window with overlapping mutations (M2/M6/M7). A green E-1 run proves the six
scheduled traces, not the cross product.

Why not `NEED_EVIDENCE_BEFORE_DECISION`: the design coverage delivered here is
sufficient to decide — the interleaving is finite-state, the atomicity split is
derivable from the lock map (FORMAL-PLAN §3.1 Q6), and E-1's role is calibration,
not coverage.

**Acceptance-gated trigger (verifiable conditions; corrected references — R2
review).** Build the extension when **both** hold: (1) E1-ABI's calibration has run
green **on both legs** — the healthy control (clean EOF, `SONG_OK` throughout) and
the fault configuration (partial success then latched fatal) — establishing that
the real latch behaves as §2's static matrix predicts (or the differential is
resolved and the model freezes the corrected matrix); and (2) the E1-WORKER trace
family W1–W6 — rebuilt on the real execution positions above, with the one new
fence arming point — has executed with every trace landing in its predicted §2.3.1
cell (no divergence). Until the W4/W2 rebuild, this gate was **not executable**
(the earlier traces referenced non-existent positions); it is now defined against
positions verified at HEAD. If any E-1 trace diverges from the static matrix, the
extension is deferred and the differential is resolved first — the model must never
be frozen against a falsified ground truth. This makes D-C's weighing concrete:
coverage (TLC cross product vs six traces) argues for the extension; calibration
risk argues for the gate.

**What the extension must contain (already answered by FORMAL-PLAN §3.1, restated
as the implementing round's obligations):** legal-trace family and defect traces
D-T1..D-T4 written before any mutation; fused actions only where a lock hold is
cited (`edge.stop()` stays a separate action — completion lock is released before
it, completion.rs:559-561; provider-call begin/return split from concurrent stop
actions — single-thread program order is not cross-thread atomicity); the
old-fatal-EOF safety scoped to the pre-Applied continuation with the
post-Applied-new-continuation-EOF witness retained; every mutation tied to its
defect trace (no pre-committed must-red lists); protocol Stop never read as D11
Stopped; no fairness assumption that evidence arrives, native calls return, or
sinks never block.

**This round changes no model action and adds no .tla** (constraint honored); the
recommendation above is the delivered verdict for D-C.
