# Error-system migration and acceptance — R1 companion (#231, #280)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY / NOT IMPLEMENTED.**
> This file plans how the candidate goals in
> [error-system-target-contract-r1.md](error-system-target-contract-r1.md) would be
> adopted, what would break, and what evidence each adoption would owe. It performs no
> migration, changes no authority, and approves nothing. Baseline:
> `5e60da4c3bd5f0fa93b4215670f4e898e2ac844e` (facts in
> [error-system-reconciliation-r1.md](error-system-reconciliation-r1.md)).

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
Docs      R0 §5.6 wording already correct; no authority change
Acceptance  test: fault → read last_error → issue another call → assert the copied
          snapshot is unaffected by the newer diagnostic
```

### 1.3 Partial success × seek/stop/close interleaving (R1 §2 matrix)

```text
Current   proven matrix: next read surfaces; Applied erases via reset_decode_state;
          stop/teardown win by first-wins; RefusedUnchanged production-dead
Target    PRESERVE the behavior table as the regression baseline; optionally T3-1
          (superseded-fatal diagnostic witness — OPEN-DECISION D-5)
Modify    none for preservation; adapter+session if D-5 adopted
Docs      none (behavior is ABI+worker-conformant; no frozen text contradicts it)
Acceptance  E-1 (R1 §8): real-provider interleave test asserting exactly one of
          {Failed{pending decode error}} or {Applied continuation with no Failed};
          counterexample to the baseline table itself would falsify R1 §2, not the goal
```

### 1.4 Output verdict and reason delivery (T2-5, D-10)

```text
Current   LoopOutcome::Aborted{message} exists as gated stderr + thread-local only;
          session settles Failed{"device"} with the reason unrecoverable (L7)
Target    bounded owned reason snapshot delivered alongside the verdict (session-internal
          cell first); verdict-only settlement semantics unchanged
Modify    wasapi.rs abort path (writer), playback session (cell + observation surface or
          internal-only), tests/common mock OutputBehavior (if it crosses the port —
          option (b) only)
Docs      PBK-003 interpretation note IF option (b); D14.2 untouched either way (content
          is Class-C diagnostic, control-inertness witness required)
Removal   the QIANQIAN_AUDIO_LOG-only carrier for the abort reason may be demoted to a
          debug line AFTER the reliable carrier exists (never before)
Acceptance  stop_seam-style: identical outcome class with snapshot present/absent;
          reason text asserted present at the diagnostic surface on a mock abort;
          counterexample: any resolve()/App branch reading the snapshot
```

### 1.5 Observer / exit-funnel integrity (N-01, N-02, T5-4, D-3)

```text
Current   decode funnel + render post-catch cone run outside every catch with
          poison-sensitive locks; gated abort-line panic skips stop+complete (N-01);
          volume print panic upgrades a recoverable failure (ledger #10)
Target    cone sites (#8/#10/#11) use a non-panicking writer; funnel/post-catch ordering
          documented as load-bearing; D-7 (panic→latch in teardown) stays OPEN pending
          K0 review
Modify    wasapi.rs two sites + a small writer helper; machine_input reader write path is
          already catch-contained (no change); entry.rs report sites are host-operation
          failures by classification (protocol rows — no silencing)
Docs      playback-execution-model hazard rows updated when the corrective lands;
          R0 §6 hazard list marked resolved-at-<sha> at that time
Removal   none
Acceptance  E-7 negative controls per outlet: blocked sink / write error / consumer
          stopped — with AUDIO_LOG on AND off, asserting identical terminal class,
          exit path, and wakeup ordering; the N-01 hang must become impossible by
          construction (writer cannot panic), demonstrated by the write-error control
```

### 1.6 Partial activation and cleanup failure (D14.6 baseline; D-7 open)

```text
Current   five raise classes unwind LIFO; failure_clean_start distinguishes
          ActivationFailedClean vs FailStop; panicking inverse crashes (R1 §3.4)
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
Acceptance  E-7/E-8: broken-pipe harness per site asserting the two classes diverge
          exactly as classified
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
          control); a green run binds the chain; absence of the test keeps the R1 C-1
          corrected claim ("constructible, not yet constructed")
```

### 1.9 Text coupling and formal-mapping migration (F10 census; R1 §5)

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

## 2. Authority-amendment correspondence table

Only documents actually touched by some candidate goal are listed. "CONDITIONAL" =
amendment text exists below but activates only if the owner adopts the corresponding
option. **No ACCEPTED document is modified by this PR.**

| Document | Touched by | Verdict |
|---|---|---|
| ADR-PBK-001 (foundations) | — (no goal creates a new realtime view or publication overlap; T2-5 content is Class C; P1-P5 untriggered) | NO CHANGE |
| ADR-PBK-002 (D11/D14.*/U2) | C-8/D-4 (drain semantics — resolver precedence is authority-OPEN, AI-7); D14.5 SEEK_UNSUPPORTED reclass (C-9); D14.9 untouched by T5-4 (grading preserved); D14.2 untouched by T1-3/T2-5 (diagnostic stays presentation, control-inertness witnessed); U2 untouched (#277 separate) | NO MANDATORY AMENDMENT; CONDITIONAL candidates §3.2/§3.3 |
| ADR-PBK-003 (Output boundary) | T2-5 option (b) (public port shape) | CONDITIONAL §3.4 |
| songcore-binding-architecture | T1-1/T1-2 (binding-internal retention note; parity unchanged) | NOTE-LEVEL §3.5 (not an amendment) |
| playback-execution-model | T5-4 corrective (hazard rows), §1.7 two-class statement | UPDATE-WHEN-LANDED (evidence doc, non-authority status per its FROZEN scope — its hazard rows describe execution facts; changes ride with the implementation PR) |
| dsp-product-model | — (T3-1 doesn't touch DSP; refuse/apply/fail triad already authoritative) | NO CHANGE |
| composition-kernel-0-design / implementation ADR | D-7 option (a) only | CONDITIONAL §3.6 (requires its own K0 authority review; not drafted as adoptable text) |
| R0 contract (this PR's file) | T5-4 resolution, T2-5 adoption, D-2..D-11 outcomes | updated by future rounds; never promoted to authority by merge |

## 3. Concrete amendment candidate texts (CANDIDATE — not adopted; differences stated)

### 3.1 No mandatory amendments

The six-module goals as recommended (T1-1/T1-2 internal retention, T5-4 corrective,
T2-5 option (a)) require **zero** changes to accepted authority: they add no terminal
variant, no semantic subclass, no public control surface, no K0 primitive, and no realtime
mechanism. This is the round's primary design claim; the conditional texts below exist so
that an owner choosing otherwise starts from concrete words, not vibes.

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

Trigger: T2-5 option (b) (public port extension). Candidate differential:

```text
+ (CANDIDATE) The backend-neutral AudioOutput contract MAY carry a bounded,
  control-inert diagnostic attachment on abort delivery (a reason snapshot beside
  the two-valued drain verdict). The attachment is Class-C diagnostic: it MUST NOT
  alter D11 inputs, terminal classification, or any control decision, and its
  absence MUST be legal. Backend identity remains non-semantic (PBK-003 §core);
  the attachment carries no backend-specific type.
```

Difference: one optional diagnostic parameter class on an existing delivery point;
verdict semantics, Output Plugin identity, and backend ownership untouched. Option (a)
(session-internal cell) needs **no** PBK-003 text.

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
| T2-5 | verdict-only settlement | mock-abort tests, snapshot present vs absent | terminal differs when the snapshot is dropped |
| T5-4 | cone isolation | E-7 four negative controls × {gate on, gate off} × {reader, render-abort, volume, hook} | any terminal/exit-path change keyed on sink state |
| T3-1 (if adopted) | witness inertness | interleave test (E-1) asserting classification never reads the witness | any Failed/Completed divergence caused by the witness |
| E-2 entry | real-chain binding | one feature-gated end-to-end on curated fixtures + healthy control | stage-family mismatch or hang |
| §1.9 migration | oracle coherence | table regeneration + TLC refinement + census update in one commit | byte-compare red; unfrozen stage panic; stale text assertions |
| §1.10 | assembly feedback | forced-violation test | Ok(()) returned with a latched Blocked |

## 5. Independent-review entry points

For a fresh reviewer picking this package up:

1. **Fastest falsification targets** (R1 HANDOFF mirrors): re-derive R1 §2 row 2 (seek
   erases the latch) from songcore_ffmpeg.c:1111-1156/1161-1226 + session.rs:601-643 —
   the single most consequential correction this round makes to scan A.
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

Delivered by this PR round: the three R1 companion documents + R0 attribution/index
update. Not delivered, by explicit constraint: production changes, test changes, build
changes, authority status changes, experiment execution, issue closure, merge. The
NEEDS_RUNTIME_EVIDENCE registry (R1 §8) is the bridge to any future evidence round;
OPEN-DECISION D-2..D-11 are the bridge to any future owner ruling.
