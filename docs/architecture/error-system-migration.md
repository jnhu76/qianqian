# Error-system migration and acceptance (#231, #280)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY / NOT IMPLEMENTED.**
> This file plans how the candidate goals in
> [error-system-target-contract.md](error-system-target-contract.md) would be
> adopted, what would break, and what evidence each adoption would owe. It performs no
> migration, changes no authority, and approves nothing. Baseline:
> `5e60da4c3bd5f0fa93b4215670f4e898e2ac844e` (facts in
> [error-system-reconciliation.md](error-system-reconciliation.md)).

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

### 3.5 NOTE-LEVEL — songcore-binding-architecture

If T1-1/T1-2 are adopted, add one paragraph to the binding-class notes: "the reference
Rust binding retains typed status/native_code internally for diagnostics; this is a
binding-internal realization choice, not a binding-parity obligation; the canonical ABI
(songcore.h) is unchanged." No authority hierarchy change.

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
