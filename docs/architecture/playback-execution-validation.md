# Playback Execution Validation Contract

```text
STATUS = FROZEN
VALIDATION_CONTRACT_VERSION = PLAYBACK-EXECUTION-VALIDATION-v1
ARCHITECTURE_REVIEW_SUBJECT_SHA = a225d524046609dbbf9bef32d96fe84800bddd92
EXECUTION_MODEL_BLOB_SHA = 815b8e7e1218ae8cfdbd5fcc60dc2ce6d22d8d3a
OWNER = #212
EXECUTION_STAGE = #211
DOWNSTREAM = #210 (Stage 7 architecture freeze), #211 (Stage 8 validation execution)
```

Governance note: `STATUS = FROZEN` inside this candidate artifact describes the
state this exact blob will have **once merged and recorded by #212**. The PR
that carries it does not itself freeze the contract; the freeze record is the
#212 final checkpoint binding `VALIDATION_CONTRACT_VERSION`,
`VALIDATION_CONTRACT_HEAD_SHA` and `VALIDATION_CONTRACT_BLOB_SHA`. This
document deliberately does not contain its own blob/commit identity (no
recursive self-reference). After freeze, no verdict-bearing change may occur
without an explicit version amendment and impact analysis (§20).

Truth class: `VALIDATION_CONTRACT` (governance). This document defines how
Stage 8 will decide verdicts. It owns **no** architecture semantics, changes
**no** authority, and every architecture claim it cites remains owned by its
existing authority. On any conflict between this contract and an authority,
the authority governs and §15.1 (architecture-gap route) applies.

---

## 1. Role and authority boundary

Accepted-architecture inputs this contract binds to (verified at the subject
SHA; blob identities are frozen in #209 comment 5982230419):

| Authority | Blob at subject | Role in this contract |
| --- | --- | --- |
| `docs/architecture/playback-execution-model.md` | `815b8e7e…` | claim source for D1–D6 framing and the 20 minimum-core rows |
| `docs/adr/ADR-PBK-001.md` | `5be152b4…` | playback foundations; formalization policy (§13); P1–P5 |
| `docs/adr/ADR-PBK-002.md` | `8640a5ff…` | current vocabulary; D11 terminal authority; D14.x protocol predicates |
| `docs/adr/ADR-PBK-003.md` | `468b98a9…` | Output Plugin / host-render backend boundary |
| `docs/architecture/composition-kernel-0-design.md` | `0fa623bf…` | K0 semantics (§F–§H, §G.6) |
| `docs/architecture/composition-kernel-0-implementation-adr.md` | `5c764d78…` | K0 representation decisions |
| `docs/architecture/dsp-product-model.md` | `6f3714b5…` | DSP product semantics; §7.3 live-update admission |
| `docs/architecture/playback-temporal-semantics.md` | `11007c8a…` | temporal vocabulary (command → admission → … → quiescence) |

Verification authority boundary (AGENTS.md) applies unchanged: validation
challenges the architecture; it never defines it. A counterexample must be
classified (§14) before any production change. A clean bounded run is absence
of counterexamples within stated bounds, never architecture acceptance.

## 2. Purpose of validation

```text
Architecture acceptance asked:
    "Is this architecture coherent and sufficiently explicit?"
    — answered by #209: PASS_ARCHITECTURE_ACCEPTED.

Validation asks:
    "Can the accepted architecture claims survive predefined
     falsification attempts on the declared evidence surfaces?"
    — answered by #211 under this contract.
```

Validation is NOT:

```text
general CI greenness
bug-free proof
universal hardware proof
performance certification (OUT-OF-SCOPE here, §11)
a second architecture-design stage
```

Every cell below validates a claim already accepted at the subject SHA. No
cell may be designed, tuned, or reinterpreted after observing Stage-8 results.

## 3. Evidence-mode vocabulary

Every cell declares at least its primary evidence mode(s) from the table below.
Composite declarations (a cell naming two modes) are strengthening; removing
or weakening a declared mode requires a version amendment (§20). Substituting
one mode for another without §20 is forbidden.

| Mode | Repository runner / form |
| --- | --- |
| STATIC / STRUCTURAL CONTRACT CHECK | source/registry predicates: `tools/check_plugin_boundaries.py` (+ `--negative-controls`), `tools/check_architecture_vocabulary.py`, documented source-inspection predicates |
| COMPILE-TIME / TYPE-SYSTEM CHECK | `cargo check`/compile-fail proofs; privacy-refusal negative control (Windows gate M2); `specs/composition-kernel-0-rust/TYPE-SYSTEM-GUARANTEES.md` matrices |
| DETERMINISTIC RUNTIME ORACLE | `cargo test` oracle suites (seek/stop/pause/volume/read/position seams, `settlement_contract_tests`, `establishment_tests`, `live_tests`, `machine_input` tests, `decision_table_oracle`) |
| FAILURE-INJECTION ORACLE | production mutation negative controls: `specs/playback-concurrency/mutations/`, `specs/composition-kernel-0-rust/mutations/`, `specs/f5-seek-implementation/` (M1–M11), TLA mutation batteries per suite |
| SCHEDULE / RACE ORACLE | real-PcmEdge Loom suite `crates/qianqian-playback/src/loom_edge_tests.rs` via `specs/playback-concurrency/check.sh` |
| SHUTDOWN / QUIESCENCE ORACLE | `teardown_gate_precondition.rs`, `navigation_waterfall.rs`, settlement teardown-boundary tests, `open_abort.rs` tests, bounded-reader tests |
| BOUNDS / BACKPRESSURE ORACLE | Loom L4a/L4b, `pause_seam` backpressure, literal-constant inventory (VC-D6-INVARIANT) |
| PLATFORM COMPILE | `windows-compile-gate.yml` (windows-latest, all targets); `rust-regression.yml` fmt/check/clippy (Linux) |
| PLATFORM DEVICE-FREE RUNTIME | windows-latest `cargo test -p qianqian-playback -p qianqian-headless` (no audio endpoint — explicitly not device evidence) |
| PLATFORM PHYSICAL-DEVICE RUNTIME | manual Windows real-device render gate (VC-E-WINDEV; precedent: native-boundary audit round record, PR #134 lineage) |
| STRESS / SOAK | SUPPLEMENTARY only (§11) |
| PERFORMANCE / REGRESSION | OUT-OF-SCOPE for v1 (§11) |
| MIRI | `cargo +nightly miri` composition matrices via `specs/check.sh rust` (verification-rust-gate) |
| LOOM | same runner as SCHEDULE / RACE ORACLE |
| MODEL CHECKING | TLA+/TLC via `specs/check.sh current` (formal-semantic-gate): K0 control-plane, realtime publication, episode-terminal settlement, f5 seek discontinuity |

Not every claim deserves a runtime test: ownership, authority routing,
forbidden dependencies, API topology, traceability and some bounds are
validated structurally. No meaningless runtime tests are manufactured to fill
the matrix.

## 4. Validation-cell schema and default profiles

Every verdict-bearing cell carries `CELL_ID`, `CLAIM`, `AUTHORITY`, `COVERS`
(D-items / minimum-core rows), `EVIDENCE_MODE`, `CLASS`
(`MANDATORY` | `EARNED-CONDITIONAL` | `SUPPLEMENTARY` | `OUT-OF-SCOPE`),
`PLATFORM/HARNESS`, `PRECONDITIONS`, `ORACLE_OR_STRUCTURAL_PREDICATE`,
`PASS_CONDITION`, `FAIL_CONDITION`, `INCONCLUSIVE_CONDITION`, `DEPENDENCIES`
(authority blobs, code paths/symbols, test/harness paths, platform
assumptions), `REPRODUCIBILITY_METADATA`, `REUSE_INVALIDATION_RULE`.

`DEPENDENCIES` is mandatory for verdict-bearing cells: Stage 8 must decide
evidence reuse after a corrective from the cell record alone; "no obvious
relation" is not dependency reasoning.

To keep the registry auditable without repeating boilerplate, cells inherit
these default profiles and override only what differs:

- `PLATFORM/HARNESS` default: Linux x86_64 CI host (ubuntu-latest), stable
  toolchain, `cargo test --locked` semantics; Windows variants say so.
- `PRECONDITIONS` default: working tree at the recorded
  `VALIDATION_EXECUTION_HEAD_SHA`; clean tree for mutation-based runners.
- `PASS_CONDITION` default: the named oracle suite(s) pass on the declared
  harness, including their declared negative controls (where present) being
  RED under mutation — i.e. a non-vacuous pass with at least one positive
  assertion count.
- `FAIL_CONDITION` default: a reproducible oracle failure that, after §14
  classification, is not `VALIDATION_ORACLE_DEFECT` and not
  `VALIDATION_ENVIRONMENT_FAILURE`.
- `INCONCLUSIVE_CONDITION` default: required evidence cannot be produced
  (environment/runner failure), the oracle is declared unable to distinguish
  the claim, or the counterexample classification cannot be resolved.
- `REPRODUCIBILITY_METADATA` default: §17 baseline record.
- `REUSE_INVALIDATION_RULE` default: `REUSE_INVALIDATED` if any DEPENDENCIES
  entry changes semantically (authority blob, code-path semantics,
  test/harness semantics, platform runner); `REUSE_VALID` only via the §16
  intersection analysis proving the changed surface disjoint from every
  dependency; edits to shared substrate (`completion.rs`, `session.rs`,
  `edge.rs`) force at least `REUSE_PARTIAL` with rerun of the affected cell
  family.

## 5. Cell registry

### 5.1 D1 — execution identity / attachment

**VC-D1-1 · fresh-core attachment precondition** — MANDATORY
- CLAIM: one fresh PlaybackSessionHandle/completion core attaches to one
  episode's establishment attempt; clones share that core; retry/restart/
  replacement requires a fresh core+spec+attempt; failed establishment does
  not authorize reuse. Claim strength: playback-domain **precondition**
  (unsupported-not-forbidden), NOT a generic structural K0 prohibition.
- AUTHORITY: PBK-002 D11/D14.6; execution model D1.
- COVERS: D1; EA-A01.
- EVIDENCE_MODE: STATIC / STRUCTURAL CONTRACT CHECK (+ fixture tripwires).
- PLATFORM/HARNESS: source predicate + `cargo test -p qianqian-playback` fixtures.
- PREDICATE: `crates/qianqian-playback/src/establishment.rs` (attempt slot
  created per attempt, written only by Session activation), `handle.rs`
  (`new`/`Clone` share one core), `session.rs` factories consume the slot
  once (`take()` tripwires are fixture-scoped only), `apps/headless/src/player.rs`
  replacement paths build fresh root+handle. No production path reattaches a
  consumed/failed core.
- CONSTRAINT: this cell MUST NOT be strengthened into a generic one-shot
  enforcement claim (that would exceed the accepted architecture — §8 attack
  B guard).
- DEPENDENCIES: PBK-002 blob; execution-model blob; paths above;
  `establishment_tests.rs`.

**VC-D1-2 · establishment/replacement lifecycle oracles** — MANDATORY
- CLAIM: episode attachment behaves as a precondition in execution: fresh
  establishment per attempt; replacement retires the old episode and
  establishes a new one end-to-end.
- AUTHORITY: PBK-002 D11/D14.6/D6; execution model D1; K0 §F–§G.
- COVERS: D1 (execution side); EA-A01, EA-J01 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `crates/qianqian-playback/src/establishment_tests.rs`;
  `crates/qianqian-playback/tests/session_activation.rs`;
  `apps/headless/src/player.rs` (`replace_episode`, `failure_clean_start`) via
  headless/player oracles; `cargo test -p qianqian-playback -p qianqian-headless`.
- DEPENDENCIES: establishment/session/player paths above; K0 design blob.

### 5.2 D2 — whole fresh-composition establishment

**VC-D2-1 · canonical establishment family (with negative controls)** — MANDATORY
- CLAIM: D14.6 `Activated` over the WHOLE fresh composition is carried only by
  `Established`; its absence only by `NotEstablished { diagnostic }`; generic
  K0 success, FiberState, source evidence, diagnostics and "not terminal" are
  never establishment truth; `Established` survives an immediate terminal;
  activation failure is never D11 Failed; machine and reference hosts consume
  the same classification.
- AUTHORITY: PBK-002 D14.6 (+D13 admission routing); execution model D2/C1.
- COVERS: D2; EA-K02 (failure classes), EA-H01 (commit authority, partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE + FAILURE-INJECTION (the
  negative controls pin non-derivability).
- HARNESS: assembly/establishment oracles — provider / dependency / Session /
  admission-failure classes; `terminal_before_host_consumption_does_not_erase_establishment`;
  `active_fiber_and_absent_diagnostic_cannot_replace_the_session_result`;
  `consumers_cannot_override_not_established_with_source_or_active_evidence`;
  `admission_failure_has_one_machine_reference_semantic_result`.
  Paths: `apps/headless/src/assembly.rs`, `crates/qianqian-playback/src/establishment_tests.rs`,
  `crates/qianqian-playback/tests/session_activation.rs`.
- DEPENDENCIES: assembly.rs/establishment paths; PBK-002 blob; execution-model blob.

**VC-D2-2 · sole-writer establishment structural check** — MANDATORY
- CLAIM: `record_establishment` is written only by the Session activation
  path after acquisition + inverse registration; no second writer exists.
- AUTHORITY: PBK-002 D14.6; execution model D2.
- COVERS: D2; EA-B02 (owner/writer).
- EVIDENCE_MODE: STATIC / STRUCTURAL CONTRACT CHECK (documented source
  predicate; rerun at execution head).
- DEPENDENCIES: `crates/qianqian-playback/src/session.rs`, `establishment.rs`,
  `apps/headless/src/assembly.rs`.

### 5.3 D3 — host input failure and result settlement

**VC-D3-1 · host admission / first failure / seal / post-seal inertness** — MANDATORY
- CLAIM: host spawn/read/caught-panic failures are invocation-local host truth
  (never D11 Failed); mandatory response = record first failure first, then
  route existing `request_stop` to an established unsettled episode; admission
  closes at seal; post-seal commands/reports/failures are inert; a blocked
  stdin reader is not required for host-result settlement.
- AUTHORITY: PBK-002 D11 boundary; execution model §8/D3.
- COVERS: D3; D4 (stdin-operations row); EA-C01, EA-E02 (host admission).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `apps/headless/src/machine_input/tests.rs` — spawn/read/panic routes;
  `preseal_failure_preserves_natural_completed_and_runtime_failed`;
  `existing_user_stop_and_host_failure_keep_one_genuine_terminal`;
  `postseal_commands_reports_and_failures_are_inert`;
  `finite_episode_returns_while_stdin_blocked_and_late_wake_is_inert`.
- DEPENDENCIES: machine_input.rs/machine.rs paths; PBK-002 blob.

**VC-D3-2 · host settlement ordering and exit cut** — MANDATORY
- CLAIM: `HOST_RESULT_SETTLEMENT_BOUNDARY` = owner seal after
  establishment/terminal/disposal; fixed first-failure cut (pre-seal recorded
  failure ⇒ nonzero exit even if the episode Completed); order
  wait_terminal → dispose → observe → seal → reports; no host mutex across
  stop/wait/dispose/I/O.
- AUTHORITY: execution model D3/§8; PBK-002 D11; K0 §G.6/§L.1.
- COVERS: D3; EA-K01, EA-K02 (host failure domain), EA-Q02 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `apps/headless/src/entry.rs` (`finish_episode`), `machine.rs`
  (`machine_exit_code`), machine_input ack-ordering tests;
  `cargo test -p qianqian-headless`.
- DEPENDENCIES: entry.rs/machine.rs/machine_input.rs; K0 blobs.

### 5.4 D4 — outstanding-work fate (protocol-specific; never collapsed)

**VC-D4-SEEK · seek fate family** — MANDATORY
- CLAIM (D14.5): two-hold `request_seek` records intent; semantic Accepted
  iff the edge is Open at plant; a non-Open plant is never-Accepted
  Refused/Inert bookkeeping (no false acceptance); one-seek slot (second seek
  inert); pause×seek commit/rebase while parked; park-handover evidence gap is
  Pending, never an abort; a committed cut's release payload is consumed once
  and the slot is held until consumption; `worker_gone` ordering strands the
  seek (accepted seek cannot outlive its worker exit); teardown-release
  routing cannot wedge the join.
- AUTHORITY: PBK-002 D14.5; temporal §6.1; execution model §6/D4.
- COVERS: D4; EA-E02, EA-E04, EA-F01, EA-O02 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE (+ existing FAILURE-INJECTION
  controls inside the suites).
- HARNESS: `crates/qianqian-playback/tests/seek_seam.rs`;
  completion slot/refusal/abort oracles; session real-worker seek oracle;
  representative oracles: `sampled_open_busy_then_stopped_free_plant_is_never_accepted`,
  `stopped_record_has_no_seek_effects_while_open_record_executes_cut`,
  `seek_acceptance_racing_worker_eof_settles_without_wedge`,
  `seek_acceptance_fails_closed_on_every_frozen_condition`,
  `a_paused_episode_commits_its_seek_and_rebases_while_still_paused`,
  `an_accepted_seek_cannot_outlive_its_worker_exit`.
- DEPENDENCIES: `completion.rs` (request_seek/seek_cutover_decision),
  `session.rs`, `edge.rs`, `crates/qianqian-audio-api/src/ports.rs`
  (set_seek_hold/release_seek_hold); PBK-002 blob; temporal blob.

**VC-D4-SEEK-FORMAL · seek discontinuity model conformance** — MANDATORY
- CLAIM: the D14.5 seek protocol's formal model (discontinuity/acceptance
  semantics) has no counterexample within its stated bounds, and the model's
  abstraction maps to the current realization as documented in the suite.
- AUTHORITY: PBK-002 D14.5; specs/f5-seek-discontinuity (RESULTS.md bounds).
- COVERS: D4 (model level); EA-E02 (conformance witness).
- EVIDENCE_MODE: MODEL CHECKING.
- HARNESS: `specs/check.sh current` (formal-semantic-gate; TLC pinned by
  sha256, fail-closed).
- PASS caveat: BOUNDED-CLEAN = no counterexample within the suite's stated
  bounds/abstraction — not implementation conformance except where the suite
  documents an explicit refinement mapping.
- DEPENDENCIES: `specs/f5-seek-discontinuity/`; `completion.rs` (trigger-path
  coupling via the formal gate); temporal blob.

**VC-D4-PAUSE · pause/resume fate family** — MANDATORY
- CLAIM: pause gate attribution (pause vs seek park) never misroutes; stop
  releases the gate under intent-recording hold; pause routed after stop or
  after teardown release cannot repark/wedge; park invariant holds (parked leg
  submits nothing; fresh probe per park).
- AUTHORITY: PBK-002 D14.7; execution model §6/D4.
- COVERS: D4; EA-E04, EA-O02 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE (+ BOUNDS/BACKPRESSURE rows in
  the same suites).
- HARNESS: `crates/qianqian-playback/tests/pause_seam.rs` (e.g.
  `pause_routed_after_stop_cannot_repark_the_released_episode`,
  `pause_routed_after_teardown_release_cannot_wedge_the_join`,
  `a_park_handover_evidence_gap_is_pending_and_never_an_abort`).
- DEPENDENCIES: pause_seam.rs; ports.rs gate; PBK-002 blob.

**VC-D4-STOP · stop fate + terminal precedence** — MANDATORY
- CLAIM: stop intent is monotone; terminal outcome resolution follows the
  frozen decision table (failure ≻ stop ≻ completed evidence classes;
  first-wins; single commit; observe/wait purity; late stop cannot relabel a
  decided outcome).
- AUTHORITY: PBK-002 D11; execution model §6/D4.
- COVERS: D4; EA-E01, EA-H01, EA-H02 (partial), EA-K01 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE (exhaustive enumeration).
- HARNESS: `crates/qianqian-playback/tests/stop_seam.rs`;
  `crates/qianqian-playback/src/decision_table_oracle.rs` (exhaustive
  resolver-table freeze; artifact-coupled to
  `specs/episode-terminal-settlement/CurrentDecisionTable.tla`).
- DEPENDENCIES: completion.rs resolver; decision_table_oracle.rs; stop_seam.rs;
  PBK-002 blob; settlement TLA artifact.

**VC-D4-DSP · processing pending/accept/apply fate** — MANDATORY
- CLAIM: DSP live updates: Desired ≠ Accepted (worker pickup compile) ≠
  Applied; latest-wins depth-1 pending; complete-in-flight; refusal
  preservation; late setters inert for terminal semantics; no post-terminal
  acceptance; no synthetic PCM.
- AUTHORITY: dsp-product-model §7.3; PBK-002 D14.11; execution model §6/D4.
- COVERS: D4; EA-E02, EA-E04, EA-H02 (partial), EA-O02 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `crates/qianqian-playback/src/live_tests.rs` (latest-wins,
  complete-in-flight, refusal preservation); settlement routing tests;
  `volume_seam.rs` / `processing` oracles for the applied-history side.
- DEPENDENCIES: live.rs; dsp blob; PBK-002 blob.

**VC-D4-PCM · buffered/fetched PCM vs terminal failure coexistence** — MANDATORY
- CLAIM: a Failed terminal Fact can coexist with a full edge ring and already
  fetched PCM (NO aggregate single-ring-capacity bound — S6-F01 corrective
  provenance); the terminal-check-before-data policy abandons buffered data;
  no semantic claim is derived from mechanism evidence.
- AUTHORITY: PBK-002 D11/D6; temporal §6.6; execution model §10.
- COVERS: D4; D6 (non-guarantee witness); EA-O01.
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: the exhaustive oracle lives as a lib-target test in
  `crates/qianqian-playback/src/completion.rs`
  (`failed_fact_can_coexist_with_full_ring_and_fetched_pcm`); run via
  `cargo test -p qianqian-playback`.
- DEPENDENCIES: edge.rs; completion.rs; temporal blob; PBK-002 blob.

**VC-D4-LATE · late retained-handle command inertness** — MANDATORY
- CLAIM: commands issued on a retained handle after replacement/teardown
  routing are inert for terminal semantics, while DSP/level history may still
  mutate locally per its own protocol; no forged terminal, no resurrection.
- AUTHORITY: PBK-002 D14.4/.5/.7; execution model §6/D4.
- COVERS: D4; EA-C01.
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: completion refusal/abort oracles; `stop_seam.rs`; `live_tests.rs`;
  navigation waterfall tests.
- DEPENDENCIES: completion.rs; handle.rs; player.rs.

**VC-D4-STDIN · stdin operation admission fate** — MANDATORY
- CLAIM: every admitted stdin Operation completes or loses admission at the
  seal (no pre/post split); post-closure reader input is not admitted.
- AUTHORITY: execution model D3/§8.
- COVERS: D4 (stdin row); EA-C01.
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: machine_input admission/seal tests (shared with VC-D3-1).
- DEPENDENCIES: machine_input.rs.

### 5.5 D5 — shutdown / quiescence scopes

**VC-D5-MILESTONE · milestone distinctness oracles** — MANDATORY
- CLAIM: the shutdown milestones (stop intent; terminal Fact commit; worker
  exited; worker joined; render stop requested; render joined; relations
  discharged; root disposed; host admission closed/drained/sealed/returned;
  stdin reader ended; process exited) are distinct observations/obligations:
  recording ≠ acceptance ≠ execution ≠ commit; terminal Fact ≠ edge terminal;
  terminal commit ≠ joins ≠ process quiescence; K0 Discharged ≠ stdin end.
  Validation proves exactly the scopes the architecture guarantees (episode
  quiescence ≠ host quiescence ≠ process quiescence) and no more.
- AUTHORITY: PBK-002 D6/D11/D14.6; K0 §G.6/§L.1; PBK-003 §5; execution model D5/§9.
- COVERS: D5; EA-Q01, EA-Q02, EA-K01 (partial).
- EVIDENCE_MODE: SHUTDOWN / QUIESCENCE ORACLE.
- HARNESS: `crates/qianqian-playback/tests/teardown_gate_precondition.rs`
  (released-gate witnesses before joins); `navigation_waterfall.rs`;
  `settlement_contract_tests.rs` (no decisive evidence uncommitted after
  teardown — t15); `crates/qianqian-output-wasapi/src/open_abort.rs` tests
  (gate closed before join); machine_input bounded-reader tests; entry.rs
  reader-not-observed documentation check.
- DEPENDENCIES: session.rs inverses; kernel.rs dispose/run_unwind;
  open_abort.rs; ports.rs; K0 blobs; PBK-003 blob.

**VC-D5-K0 · K0 lifecycle/discharge and resource lifetime oracles** — MANDATORY
- CLAIM: fiber teardown follows registered inverse order (render first,
  decode last); discharge verdicts are observed, not assumed; withdrawn/
  pending activation behaves as specified; resources owned by effects are
  released through the unwind; leak/double-discharge negative oracles hold.
- AUTHORITY: K0 design §F–§H, §G.6; K0 implementation ADR; PBK-002 D6.
- COVERS: EA-J01, EA-R01; D5 (episode side).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `crates/qianqian-composition` kernel oracles (lifecycle matrices,
  e.g. `dependent_consumer_violation_loci_preserve_k0_guard_semantics`);
  `crates/qianqian-playback/tests/session_activation.rs` withdrawal/pending;
  dispose/leak tests; `specs/check.sh rust` native matrices.
- DEPENDENCIES: kernel.rs; session.rs; K0 blobs.

### 5.6 D6 — bounds / liveness

**VC-D6-INVARIANT · local-bounds inventory** — MANDATORY
- CLAIM: the logical local bounds and policies are exactly: edge ring
  `EDGE_CAPACITY_FRAMES = 8192`; staging `STAGING_FRAMES = 1024`; worker wait
  slice 2 ms; park slice 10 ms; open timeout 10 s; event timeout 100 ms;
  drain cap 5 s (tuning, not authority); seek slot cardinality 1; DSP pending
  depth 1; host first-failure cell 1; one active effect. Policies:
  BLOCK/REFUSE/LATEST-WINS/FIRST-WINS/DROP/INERT as documented. Logical
  counts ≠ allocator bytes.
- AUTHORITY: PBK-002 D14.5/D14.11; dsp §7.3; execution model §10/D6.
- COVERS: D6; EA-O01.
- EVIDENCE_MODE: STATIC / STRUCTURAL CONTRACT CHECK (literal source predicate
  over the symbols below; re-derived at execution head).
- DEPENDENCIES: `crates/qianqian-playback/src/session.rs` (edge/staging/wait
  constants), `edge.rs`, `live.rs`, `crates/qianqian-audio-api/src/ports.rs`
  (park slice), `crates/qianqian-output-wasapi/src/wasapi.rs` (open/event/
  drain constants), `apps/headless/src/machine_input.rs` (first-failure cell);
  execution-model blob (§10 inventory).
- REUSE rule override: ANY change to these constants/symbols ⇒
  `REUSE_INVALIDATED` for this cell and triggers re-derivation of dependent
  bounds cells.

**VC-D6-BACKPRESSURE · backpressure / bounded-wait behavior** — MANDATORY
- CLAIM: `write_some`/wait backpressure bounds the producer at the edge;
  parked render legs hold no buffer; wait slices cap only the requested wait,
  never scheduling/lock/acknowledgement; drain-cap exit yields Aborted
  evidence for the resolver (not a synthetic terminal).
- AUTHORITY: PBK-002 D8/D14.5/D14.7; PBK-003 §5; execution model §10/D6.
- COVERS: D6; EA-O02, EA-D01 (partial).
- EVIDENCE_MODE: BOUNDS / BACKPRESSURE ORACLE + SCHEDULE / RACE ORACLE (loom
  L4a/L4b) + pause_seam backpressure rows.
- HARNESS: `specs/playback-concurrency/check.sh` (loom suite incl. L4);
  `pause_seam.rs`; `read_seam.rs`.
- DEPENDENCIES: edge.rs; ports.rs; loom runner + negative control.

**VC-D6-RACE · real-PcmEdge schedule suite** — MANDATORY
- CLAIM: within the suite's stated thread/op bounds, the real PcmEdge
  terminal/backpressure machinery has no deadlock or lost-wakeup schedule
  (terminal first-wins races L3a/L3b included); the dropped-`data_ready`
  mutation is caught (negative control M-L1).
- AUTHORITY: PBK-002 D8/D11; execution model D4/D6; specs/playback-concurrency
  RESULTS.md bounds.
- COVERS: D4/D6 race collisions; EA-D01, EA-Q02 (partial).
- EVIDENCE_MODE: SCHEDULE / RACE ORACLE (+ FAILURE-INJECTION negative control).
- HARNESS: `specs/playback-concurrency/check.sh` (fail-closed: clean mutated
  run = TOOLING-FAIL; refuses dirty `edge.rs`).
- DEPENDENCIES: edge.rs; loom_edge_tests.rs; mutations/M-L1 patch.

**VC-D6-NONGUARANTEE · non-guarantee ledger consistency** — MANDATORY
- CLAIM: the declared non-guarantees remain declared and no validation cell
  depends on their opposite: NO aggregate ring+fetch capacity bound; NO
  end-to-end deadline; NO global fairness; NO whole-process memory bound; NO
  stdin/native/device progress guarantee (worker-exit progress depends on
  scheduling and outstanding native calls); an accepted seek may remain
  pending indefinitely absent ending evidence; quiescence ≠ reclamation.
- AUTHORITY: execution model §9/§10; temporal §6.6/§9; PBK-001 §17.
- COVERS: D6; guards §8 attack B contract-wide.
- EVIDENCE_MODE: STATIC / STRUCTURAL CONTRACT CHECK (ledger present and
  consistent with cell registry; no mandatory cell assumes a non-guarantee).
- PASS: ledger verified at execution head; FAIL: any cell found assuming the
  opposite (contract-design defect ⇒ §15.3 route).
- DEPENDENCIES: execution-model blob; this contract.

**VC-D6-MIRI · composition matrices under Miri** — MANDATORY (scoped)
- CLAIM: the composition kernel's scenario matrices are clean under Miri
  within their stated scope (the ~1-minute 7-matrix pass over
  qianqian-composition; not a cross-workspace campaign).
- AUTHORITY: K0 implementation ADR (representation substrate); AGENTS.md
  verification policy.
- COVERS: execution substrate for EA-J01, EA-Q01, EA-R01.
- EVIDENCE_MODE: MIRI.
- JUSTIFICATION (architecture relevance, not "CI runs it"): the kernel's
  unwind/discharge machinery underpins every lifecycle claim; a memory-safety
  violation there would undermine EA-J01/EA-R01 evidence wholesale.
- HARNESS: `specs/check.sh rust` (verification-rust-gate; nightly Miri
  channel, fail-closed preflight).
- FAILURE_MEANING: candidate `IMPLEMENTATION_DEFECT` (classify first).
- DEPENDENCIES: composition kernel sources; matrices; toolchain contract.

### 5.7 Cross-cutting structural / formal cells

**VC-X-BOUNDARY · plugin boundary / export surface / vocabulary** — MANDATORY
- CLAIM: no unadmitted internal dependency edge; the concrete WASAPI
  mechanism is not reachable by external consumers (compiler privacy
  enforcement, Windows M2 negative control; Linux export-surface watcher);
  no retired vocabulary identifiers in active surfaces.
- AUTHORITY: PBK-002 D4/D12/D13; PBK-003; AGENTS.md vocabulary rules.
- COVERS: EA-B02, EA-D01 (topology), EA-R01 (ownership edges).
- EVIDENCE_MODE: STATIC / STRUCTURAL CONTRACT CHECK (+ COMPILE-TIME negative
  control on Windows).
- HARNESS: `tools/check_plugin_boundaries.py` (+ `--negative-controls`);
  `tools/check_architecture_vocabulary.py`; windows-compile-gate M2 step.
- DEPENDENCIES: Cargo graph; boundary checker; vocabulary checker.

**VC-X-WRITERS · owner / writer / serializer / authority table** — MANDATORY
- CLAIM: the mutable-state inventory and its owner/writer/serializer/authority
  attribution (execution model §C/§4) is re-derivable from source at the
  execution head with no conflation (owner = lifecycle/teardown responsibility;
  serializer = lock discipline; authority = PBK-001 §2.3 designations).
- AUTHORITY: PBK-001 §2.2–§2.3; PBK-002 D6/D11; dsp §7.3; execution model §C.
- COVERS: EA-B01, EA-B02.
- EVIDENCE_MODE: STATIC / STRUCTURAL CONTRACT CHECK.
- PREDICATE: per-symbol source inspection at execution head; any new
  unsynchronized writer or authority conflation ⇒ FAIL.
- DEPENDENCIES: completion.rs/edge.rs/live.rs/ports.rs/wasapi.rs/machine_input.rs;
  PBK-001/PBK-002 blobs.

**VC-X-DATAPLANE · steady data-plane kernel-freedom witness** — MANDATORY
- CLAIM: the steady per-quantum data path performs zero K0 kernel work (no
  Context lookup, no capability resolution, no reconcile, no generic dispatch,
  no fact fan-out) — the PCM firewall realized.
- AUTHORITY: PBK-001 § realtime firewall; PBK-002 D8; execution model §7.
- COVERS: EA-D01, EA-B02 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `crates/qianqian-playback/tests/k0_firewall.rs`
  (`steady_data_plane_performs_zero_kernel_work`); gate/drain observers are
  off-quantum (locks released before session entry — asserted by the suite).
- DEPENDENCIES: edge.rs; session.rs; k0_firewall.rs.

**VC-X-PROJECTION · position projection firewall** — MANDATORY
- CLAIM: position is a Projection with a single render-leg writer; the
  observation gate is (unsettled ∧ no activation failure); the projection is
  withdrawn at terminal (terminal ⇒ `position.is_none()`); observers never
  become correctness authority.
- AUTHORITY: PBK-001 §2.3; PBK-002 D14.8/D14.6; execution model §N.
- COVERS: EA-I01, EA-H02 (partial).
- EVIDENCE_MODE: DETERMINISTIC RUNTIME ORACLE.
- HARNESS: `crates/qianqian-playback/tests/position_seam.rs` (withdrawal);
  `read_seam.rs` (observe/wait purity);
  `crates/qianqian-playback/src/settlement_contract_tests.rs` observe/wait
  purity rows (t8 `t8_observe_neither_settles_nor_mutates`,
  t9 `t9_wait_terminal_is_not_the_settlement_trigger`,
  t11 `t11_late_stop_cannot_relabel_a_decisive_failed`);
  `crates/qianqian-output-wasapi/src/render_order_oracle.rs` (single
  render-leg writer: publish-before-submit pairing).
- DEPENDENCIES: completion.rs observation gate; ports.rs callbacks;
  render_order_oracle.rs; PBK-001/PBK-002 blobs.

**VC-X-SETTLEMENT-FORMAL · episode terminal settlement conformance** — MANDATORY
- CLAIM: the D11 terminal-resolution contract (single immutable commit;
  writer identity = authority; Observe/Wait purity; late-command stability;
  teardown settlement boundary over the full shape trigger domain;
  activation-failure firewall; Completed/Stopped necessary conditions;
  conditional settlement progress) has no counterexample within the suite's
  bounds; the Rust refinement oracle pins production resolver ⇒ frozen
  decision table, and TLC pins table ⇒ formal decision functions.
- AUTHORITY: PBK-002 §17/D11; PBK-001 §2.2–§2.3;
  specs/episode-terminal-settlement (bounds + mutation battery M1–M10).
- COVERS: EA-H01, EA-H02, EA-K01 (terminal side), EA-C01 (decision boundary).
- EVIDENCE_MODE: MODEL CHECKING (+ the refinement seam is a DETERMINISTIC
  RUNTIME ORACLE executed by VC-D4-STOP's exhaustive enumeration).
- HARNESS: `specs/check.sh current` (formal-semantic-gate) +
  `decision_table_oracle.rs` (verification-rust-gate playback regression).
  The no-second-settlement-writer realization witness
  (`assert_no_settlement_thread`,
  `crates/qianqian-playback/src/settlement_contract_tests.rs`) belongs to the
  runtime side of this claim and is executed with the playback suites.
- DEPENDENCIES: completion.rs; decision_table_oracle.rs;
  settlement_contract_tests.rs; CurrentDecisionTable.tla; settlement suite;
  PBK-002 blob.

**VC-X-FORMAL-K0 · K0 control-plane and realtime-publication models** — MANDATORY (as constituted)
- CLAIM: (a) K0 relied_on guard / inverse-exactly-once / violation tombstone /
  removal discipline / single-source mount / FAILED settlement have no
  counterexample within stated bounds (B1–B6); the §G.6 teardown-closure locus
  is covered by the Rust oracle (differential ruling FORMALIZATION_NOT_EARNED
  recorded), not by TLA — this split is part of the claim. (b) The
  realtime-publication P1–P5 protocol model is bounded-clean — this is a
  PROTOCOL-level result only: no multi-generation publication mechanism exists
  in production (ADR-PBK-001 §6/§12 keeps the representation OPEN); this cell
  MUST NOT be reported as production behavior.
- AUTHORITY: K0 design §E.4/§F.5/§G/§G.6/§H/§K.4; PBK-001 §6; suite RESULTS.md.
- COVERS: EA-J01, EA-F01 (protocol level); P1–P5 protocol (not production).
- EVIDENCE_MODE: MODEL CHECKING.
- HARNESS: `specs/check.sh current` (formal-semantic-gate, fail-closed,
  mutation batteries M1–M4 + probes).
- DEPENDENCIES: specs/composition-kernel-0; specs/realtime-publication;
  kernel.rs; K0 blobs; PBK-001 blob.

**VC-X-WINPORT · Windows device-free portability corroboration** — MANDATORY
- CLAIM: the semantic oracle suites that carry the D1–D6 verdicts
  (qianqian-playback + qianqian-headless) also pass on Windows, device-free
  (cfg(windows) surfaces compile and behave identically where the suites
  reach them). This corroborates platform-neutrality of the semantic claims;
  the verdict surface remains the Linux host (§9).
- AUTHORITY: execution model (platform-neutral semantic claims); PBK-003 §5
  (backend-neutral contract; concrete mechanism owned).
- COVERS: portability corroboration for all §6 D-item cells it executes; NOT
  a primary evidence surface for any D-item.
- EVIDENCE_MODE: PLATFORM DEVICE-FREE RUNTIME.
- PLATFORM/HARNESS: windows-latest (`windows-compile-gate` test job):
  `cargo test -p qianqian-playback -p qianqian-headless` — no audio endpoint,
  explicitly not device evidence.
- PASS_CONDITION (override): the two device-free suites pass on windows-latest
  at the bound execution head.
- FAIL_CONDITION (override): a Windows-only reproducible failure is classified
  via §14 with platform recorded (VALIDATION_ORACLE_DEFECT /
  VALIDATION_ENVIRONMENT_FAILURE differentiation first, then
  IMPLEMENTATION_DEFECT route); it never silently amends the §9 matrix.
- INCONCLUSIVE_CONDITION (override): the windows-latest runner is unavailable
  ⇒ this cell INCONCLUSIVE ⇒ campaign INCONCLUSIVE (§13).
- DEPENDENCIES: qianqian-playback + qianqian-headless suites (same identities
  as the cells they corroborate); windows-compile-gate workflow; PBK-003 blob.

### 5.8 EARNED-CONDITIONAL cells

**VC-E-F5MUT · seek-spine production mutation guard** — EARNED-CONDITIONAL
- TRIGGER: any corrective touching the seek spine (`completion.rs`
  request_seek/seek_cutover_decision/release bookkeeping, `session.rs` seek
  paths, `edge.rs` invalidate). Not part of ordinary Stage-8 PASS.
- CLAIM: the seek production spine's distinguishing power survives change —
  the f5 mutation battery (M1–M11) is caught by the current oracles.
- EVIDENCE_MODE: FAILURE-INJECTION ORACLE.
- HARNESS: `specs/check.sh f5` (fail-closed runner).
- NOT EARNED/NOT TRIGGERED ⇒ permitted NOT_RUN (explicitly excluded from the
  PASS computation, §13).

**VC-E-WINDEV · Windows physical-device render gate** — EARNED-CONDITIONAL
- See §10 for the full decision. TRIGGER: a release/portability gate that
  claims Windows audible output, or a corrective touching the
  `qianqian-output-wasapi` render path.
- CLAIM: the WASAPI backend renders on a real host endpoint (device smoke +
  provenance), per AGENTS.md real-path policy.
- EVIDENCE_MODE: PLATFORM PHYSICAL-DEVICE RUNTIME (manual; acceptance
  recorded as an evidence artifact, not a CI green).
- VALIDATES: real backend/device integration only. Does NOT validate
  temporal/concurrency semantics (those are the device-free cells above) and
  does not substitute for any MANDATORY cell.
- NOT EARNED/NOT TRIGGERED ⇒ permitted NOT_RUN.

### 5.9 SUPPLEMENTARY cells (never verdict-bearing)

**VC-S-WORKSPACE · Linux workspace regression** — SUPPLEMENTARY
`cargo fmt --check` + `cargo check` + `cargo test --workspace` + clippy
(rust-regression gate). Hygiene substrate. Explicitly NOT validation: a green
workspace run contributes nothing to §13; a red run blocks execution hygiene
but classifies through §14 before any verdict.

**VC-S-STRESS · stress / soak** — SUPPLEMENTARY
Any soak exercise. A soak anomaly is an EARNED FOLLOW-UP signal (§11), never a
mandatory-cell failure; random/stress evidence without §17 replay metadata is
supporting only.

### 5.10 OUT-OF-SCOPE for v1

```text
performance / throughput certification (no accepted claim is quantitative)
universal hardware proof / audible-quality certification
macOS / Android compile or runtime evidence (no repository/product basis today)
general (non-triggered) mutation campaigns
cross-workspace Miri
```

Adding any of these later requires a version amendment (§20); Stage 8 may not
promote them retroactively.

## 6. D1–D6 coverage map — 6/6

| D-item | Primary cells |
| --- | --- |
| D1 identity/attachment | VC-D1-1, VC-D1-2 |
| D2 whole establishment | VC-D2-1, VC-D2-2 |
| D3 host input/result settlement | VC-D3-1, VC-D3-2 |
| D4 outstanding-work fate | VC-D4-SEEK, VC-D4-SEEK-FORMAL, VC-D4-PAUSE, VC-D4-STOP, VC-D4-DSP, VC-D4-PCM, VC-D4-LATE, VC-D4-STDIN |
| D5 shutdown/quiescence | VC-D5-MILESTONE, VC-D5-K0 |
| D6 bounds/liveness | VC-D6-INVARIANT, VC-D6-BACKPRESSURE, VC-D6-RACE, VC-D6-NONGUARANTEE, VC-D6-MIRI |

Design-time coverage = 6/6. This is contract coverage, NOT Stage-8 PASS
evidence.

## 7. Minimum-core coverage matrix — 20/20

| Row | Claim (subject SHA §12) | Primary cell(s) | Evidence mode | MANDATORY |
| --- | --- | --- | --- | --- |
| EA-A01 | execution identity/attachment | VC-D1-1, VC-D1-2 | STATIC + RUNTIME | YES |
| EA-B01 | mutable state and writers | VC-X-WRITERS | STATIC | YES |
| EA-B02 | owner vs serializer vs authority | VC-X-WRITERS, VC-X-BOUNDARY | STATIC | YES |
| EA-C01 | command admission and ending fate | VC-D3-1, VC-D4-STDIN, VC-D4-LATE (+ D4 family) | RUNTIME | YES |
| EA-D01 | communication model | VC-X-DATAPLANE, VC-D6-BACKPRESSURE, VC-X-BOUNDARY | RUNTIME + STATIC | YES |
| EA-E01 | local serialization | VC-D4-STOP (lock-shape oracles), VC-D3-1 | RUNTIME | YES |
| EA-E02 | acceptance/linearization | VC-D4-SEEK, VC-D4-DSP, VC-D3-1, VC-D4-SEEK-FORMAL | RUNTIME + MODEL | YES |
| EA-E04 | in-flight ownership/debt | VC-D4-SEEK, VC-D4-PAUSE, VC-D4-DSP | RUNTIME | YES |
| EA-F01 | ordering | VC-D4-SEEK (cut spine), VC-D5-MILESTONE, VC-X-SETTLEMENT-FORMAL, VC-X-FORMAL-K0 | RUNTIME + MODEL | YES |
| EA-H01 | commit authority | VC-X-SETTLEMENT-FORMAL, VC-D4-STOP, VC-D2-1 | MODEL + RUNTIME | YES |
| EA-H02 | evidence/commit/visibility stages | VC-X-SETTLEMENT-FORMAL, VC-X-DATAPLANE, VC-D4-DSP | MODEL + RUNTIME | YES |
| EA-I01 | projection firewall | VC-X-PROJECTION; VC-D2-1 (partial — Fiber/source/diagnostic evidence cannot classify establishment) | RUNTIME | YES |
| EA-J01 | lifecycle relation | VC-D5-K0, VC-D1-2, VC-X-FORMAL-K0 | RUNTIME + MODEL | YES |
| EA-K01 | failure responsibility | VC-X-SETTLEMENT-FORMAL, VC-D3-2, VC-D5-MILESTONE | MODEL + RUNTIME | YES |
| EA-K02 | failure domains | VC-D2-1, VC-D3-1, VC-D3-2; VC-D4-DSP (partial — processing-failure domain resolves D11 Failed with truthful origin) | RUNTIME | YES |
| EA-O01 | boundedness | VC-D6-INVARIANT, VC-D4-PCM | STATIC + RUNTIME | YES |
| EA-O02 | backpressure/busy policy | VC-D6-BACKPRESSURE, VC-D4-SEEK (refuse), VC-D4-DSP (latest-wins) | RUNTIME + RACE | YES |
| EA-Q01 | shutdown order | VC-D5-MILESTONE, VC-D5-K0 | QUIESCENCE ORACLE | YES |
| EA-Q02 | quiescence proof | VC-D5-MILESTONE, VC-D6-RACE (partial) | QUIESCENCE + RACE | YES |
| EA-R01 | resource lifetime | VC-D5-K0, VC-X-BOUNDARY | RUNTIME + STATIC | YES |

Design-time coverage = 20/20. Contract coverage only — NOT Stage-8 PASS
evidence.

## 8. CI green ≠ validation

```text
broad CI green              != architecture validation PASS
cargo test --workspace PASS != D1–D6 validation by itself
Windows compile PASS        != Windows runtime PASS
Windows device-free PASS    != Windows physical-device PASS
stress PASS                 != semantic-oracle PASS
Miri PASS                   != device behavior proof
Loom PASS                   != physical platform proof
TLA+/TLC PASS               != implementation conformance
  (except at the explicit decision-table refinement seam, VC-X-SETTLEMENT-FORMAL)
```

CI workflows may execute cell harnesses, but Stage 8 must extract and classify
the actual verdict-bearing evidence per cell (CELL_ID + evidence mode + head
SHA), and the hosted runs on a Stage-6.5 contract PR are PR-HYGIENE ONLY.

## 9. Platform matrix

Frozen from repository reality (inspectable in `.github/workflows/` and the
workspace), not from aspirational product targets:

| Platform surface | COMPILE | DEVICE-FREE RUNTIME | PHYSICAL-DEVICE RUNTIME |
| --- | --- | --- | --- |
| Linux x86_64 (CI host) | REQUIRED (`rust-regression`) | REQUIRED — primary verdict surface for all deterministic/race/formal cells | not applicable (no Linux host backend in product scope) |
| Windows x86_64 | REQUIRED (`windows-compile-gate`: `cargo check --workspace --all-targets` + M2 privacy negative control) | REQUIRED — **VC-X-WINPORT** (`cargo test -p qianqian-playback -p qianqian-headless` on windows-latest) as portability corroboration of the semantic cells | EARNED-CONDITIONAL (VC-E-WINDEV) |
| macOS | NOT_REQUIRED (v1) | NOT_REQUIRED (v1) | NOT_REQUIRED (v1) |
| Android | NOT_REQUIRED (v1) | NOT_REQUIRED (v1) | NOT_REQUIRED (v1) |

Rules:

- The semantic (D1–D6) claims are platform-neutral; their verdict surface is
  the Linux host. A Windows-only device-free oracle failure is classified via
  §14 with platform recorded (oracle/environment differentiation first, then
  implementation-defect route); it never silently amends the platform matrix.
- `NOT_REQUIRED (v1)` is a frozen scope decision: earning a macOS/Android
  product claim later requires a contract amendment, not silent Stage-8
  addition.
- A required platform runner being unavailable ⇒ that cell is INCONCLUSIVE
  (§13); a permitted NOT_RUN applies only to EARNED-CONDITIONAL cells whose
  trigger has not fired.

## 10. Physical-device evidence decision

```text
PHYSICAL_DEVICE_RUNTIME = EARNED-CONDITIONAL
```

Why: the accepted Stage-6 claims are host-execution-architecture semantics
(lifecycle, fate, settlement, bounds, quiescence), all validated on device-free
surfaces; PBK-003 deliberately makes the concrete backend an owned, replaceable
mechanism behind the backend-neutral `AudioOutput` contract; and repository
reality provides no hosted real-audio-device evidence (the Windows gate
documents that its runner has no audio endpoint; the one real-device campaign
on record — the native-boundary audit round, PR #134 lineage — was
audit-scoped and manual). Making device smoke mandatory for architecture
validation would let one successful sound-output smoke masquerade as
temporal/concurrency evidence — exactly the substitution §8 forbids.

If earned/triggered, VC-E-WINDEV requires:

```text
platform/device class : Windows 10+ host, real render endpoint
reproducibility       : AGENTS.md real-path evidence record (commit, target,
                        toolchain, SongCore ABI identity, backend identity,
                        device identity, input media hash, decoded frame/sample
                        counts, deterministic PCM hash or reference output
                        where applicable, runtime log, exact commands)
acceptance mode       : manual, recorded as a durable evidence artifact in
                        #211 (deterministic pipeline evidence kept distinct
                        from audible smoke evidence)
validates             : real backend/device integration for the touched path
does NOT validate     : D1–D6 semantics, race freedom, quiescence scopes
unavailability means  : the earned claim stays INCONCLUSIVE for its scope;
                        untriggered cells are permitted NOT_RUN and do not
                        block §13 for the semantic campaign
```

## 11. Stress / performance / formal / mutation / device PASS-gate statuses

| Family | STATUS | JUSTIFICATION | COVERED CLAIMS | FAILURE_MEANING |
| --- | --- | --- | --- | --- |
| STRESS / SOAK | SUPPLEMENTARY | collision classes are pinned by bounded deterministic oracles + Loom + TLA bounds; no accepted claim is duration-scoped | none (supporting) | EARNED FOLLOW-UP signal; never a mandatory-cell failure |
| PERFORMANCE / REGRESSION | OUT-OF-SCOPE (v1) | no D1–D6/20-row claim is performance-quantitative; the one performance-adjacent claim (zero kernel work per quantum) is validated by VC-X-DATAPLANE, not throughput numbers | none | n/a (must be earned by amendment) |
| MIRI | MANDATORY (scoped: existing composition matrices) | kernel unwind/discharge machinery is the execution substrate of EA-J01/Q01/R01; cheap (~1 min), fail-closed | substrate safety for lifecycle rows | candidate IMPLEMENTATION_DEFECT (classify first) |
| LOOM | MANDATORY | real-PcmEdge terminal/backpressure interleavings are the D4/D6 collision core; includes negative control | EA-D01/O02/Q02, D4 races | reproducible schedule counterexample ⇒ classify via §14 |
| MODEL CHECKING (TLA+/TLC) | MANDATORY (as constituted) | D11 settlement + D14.5 seek + K0 lifecycle + P1–P5 protocol are the semantic spine of the accepted claims; suites are fail-closed with mutation batteries | EA-H01/H02/K01, D4 model, EA-J01/F01 protocol level | counterexample ⇒ classify (model-spec mismatch / production defect / authority gap / oracle gap) |
| MUTATION TESTING | EARNED-CONDITIONAL | embedded negative controls inside mandatory gates are MANDATORY as their constituents; a general mutation campaign is OUT-OF-SCOPE (v1); new mutations required when a corrective touches a pinned surface (VC-E-F5MUT pattern) | oracle distinguishing power | an uncaught embedded mutation fails its gate (TOOLING-FAIL semantics) |
| PHYSICAL DEVICE | EARNED-CONDITIONAL | see §10 | real backend/device integration only | unavailability ⇒ INCONCLUSIVE for the earned scope only |

No family was made mandatory because CI already runs it, nor supplementary
because it is expensive; the justifications above are the architecture
relevance.

## 12. Negative-control / mutation-witness policy

A negative control is MANDATORY where an oracle's distinguishing power is not
self-evident from a directly inspectable predicate, and EARNED-CONDITIONAL
otherwise. Concretely:

```text
MANDATORY today (already constituted, fail-closed):
  loom dropped-notify mutation (specs/playback-concurrency M-L1)
  composition matrix production mutations (specs/composition-kernel-0-rust)
  TLA mutation batteries (K0 M1–M4; realtime M1–M4; settlement M1–M10)
  plugin-boundary mutation battery (+ negative controls run)
  vocabulary-gate workflow negative control
  establishment non-derivability negative controls (VC-D2-1)
EARNED-CONDITIONAL:
  seek-spine mutation refresh after seek-spine correctives (VC-E-F5MUT)
  new/adapted controls whenever a corrective changes a pinned surface
NOT REQUIRED:
  meaningless mutations for structural checks whose predicate is directly
  inspectable (e.g. literal-constant inventory, boundary topology listings)
```

## 13. PASS / FAIL / INCONCLUSIVE — final semantics

```text
PASS requires ALL of:
  - every MANDATORY cell has PASS evidence at an allowed execution head
    (FREEZE_RECORD_SHA lineage; §17 binding)
  - no mandatory cell is INCONCLUSIVE
  - no unresolved architecture-level counterexample (§14)
  - no required platform evidence missing (§9)
  - all verdict-bearing evidence binds to allowed execution SHA(s)
  - all reuse/invalidation decisions recorded and coherent (§16)

FAIL requires:
  - at least one MANDATORY cell with a reproducible counterexample, after
    §14 classification establishes IMPLEMENTATION_DEFECT or
    ARCHITECTURE_AUTHORITY_GAP; a validation-oracle bug never proves the
    architecture failed

INCONCLUSIVE applies when:
  - required evidence cannot be produced
  - an oracle cannot distinguish its claim strongly enough (declared or
    demonstrated)
  - the environment prevents mandatory evidence
  - a counterexample classification cannot be resolved
```

A MANDATORY cell that was never run ⇒ the campaign is INCONCLUSIVE (never
PASS). INCONCLUSIVE is never upgraded to PASS because downstream work is
waiting. Stage 8 may not promote supplementary evidence into the PASS gate
retroactively, nor omit mandatory evidence because deterministic tests passed.

A TRIGGERED EARNED-CONDITIONAL cell that produces a reproducible failure
classifies via §14. If the classification is IMPLEMENTATION_DEFECT or
ARCHITECTURE_AUTHORITY_GAP, the **earned claim is FAIL for its declared
scope** and routes per §15.2/§15.1 — but the semantic-campaign verdict remains
governed by this section's mandatory-cell rule. The #211 ledger records the
earned-scope verdict on the triggered cell's own row (PASS / FAIL /
INCONCLUSIVE / NOT_RUN).

## 14. Counterexample taxonomy

Every Stage-8 anomaly is classified BEFORE any repair:

| Class | Evidence threshold | Corrective owner | Reopens architecture freeze? | Cell invalidation | Stage 8 continues elsewhere? |
| --- | --- | --- | --- | --- | --- |
| IMPLEMENTATION_DEFECT | reproducible counterexample against frozen authority on the cell's declared surface | separate bounded corrective issue/PR (not #211) | NO (unless it reveals an authority gap) | DEPENDENCIES-intersecting cells ⇒ REUSE_INVALIDATED; rerun after fix at new execution head | YES for unaffected cells, with §16 records |
| ARCHITECTURE_AUTHORITY_GAP | supported-product counterexample the frozen authorities cannot resolve, or authority conflict | §15.1 route | YES | affected D-items'/rows' cells REUSE_INVALIDATED; old evidence reclassified | NO for affected claims |
| VALIDATION_ORACLE_DEFECT | the oracle cannot validly distinguish its claimed property (flaky, vacuous, over/under-strong) | separate harness/oracle corrective under #212 governance | NO | affected cell evidence discarded; rerun at new harness identity | YES |
| VALIDATION_ENVIRONMENT_FAILURE | runner/toolchain/device/infrastructure failure independent of the subject | environment fix; rerun | NO | affected cell ⇒ INCONCLUSIVE until rerun | YES |
| UNSUPPORTED_BEHAVIOR / NON-GUARANTEE | observation falls inside a declared non-guarantee (§5.6 VC-D6-NONGUARANTEE ledger) | none (not a defect) | NO | none — record and stop | YES |
| INCONCLUSIVE | evidence threshold above cannot be met or classification unresolved | per §13 | NO (gate stays CLOSED) | affected cell not counted | NO (campaign verdict is INCONCLUSIVE) |

Classification worksheet is mandatory in the evidence record: observed
behavior → surface → authority consulted → threshold met → class → route.

## 15. Corrective and reopen protocols

### 15.1 ARCHITECTURE_AUTHORITY_GAP — reopen route

```text
1. stop ordinary Stage-8 progression for the affected claims
2. mark the architecture freeze UNDER REVIEW / REOPENED (#198/#210)
3. identify affected D-items and minimum-core rows
4. open a bounded architecture corrective
5. fresh independent Stage-6-style review on the corrected subject
6. produce a new accepted architecture subject + freeze version
7. decide whether this contract needs a version amendment (§20)
8. produce a new Stage-7 freeze record
9. classify ALL old validation evidence: REUSE_VALID / REUSE_INVALIDATED /
   REUSE_PARTIAL (§16)
10. resume Stage 8 only against the new identity
```

No semantic fix is allowed underneath an old freeze identity.

### 15.2 IMPLEMENTATION_DEFECT route

Separate corrective issue + PR; cite the frozen authority; change no
architecture semantics; record the new execution head SHA; map
DEPENDENCIES-affected cells; rerun INVALIDATED cells; explicitly justify every
reused verdict-bearing cell via §16. Architecture identity remains frozen
unless the defect reveals an authority gap (then §15.1).

### 15.3 VALIDATION_ORACLE_DEFECT route

Never classify the product implementation as failed on an oracle defect alone.
Open a bounded harness/oracle corrective; demonstrate distinguishing power
(prefer a negative control / mutation witness); record the new harness/execution
identity; rerun the affected cells. A flaky or logically weak oracle cannot
establish architecture failure.

### 15.4 VALIDATION_ENVIRONMENT_FAILURE

Runner unavailability, inaccessible device, unrelated toolchain failure,
artifact/log corruption, platform outage are product-failure classes only by
evidence, never by default. Missing mandatory evidence yields INCONCLUSIVE
(§13); missing untriggered EARNED-CONDITIONAL evidence is permitted NOT_RUN
(§5.8). No improvisation in Stage 8.

## 16. Evidence reuse / invalidation

States: `REUSE_VALID` / `REUSE_INVALIDATED` / `REUSE_PARTIAL`.

For every verdict-bearing reuse decision, record:

```text
CHANGED_SURFACE      = commits/files/symbols that changed
CELL_DEPENDENCIES    = the cell's DEPENDENCIES entries (authority blob /
                       code path / test-harness path / platform runner)
INTERSECTION         = the precise overlap analysis
REUSE_CLASS          = REUSE_VALID | REUSE_INVALIDATED | REUSE_PARTIAL
RATIONALE            = why the class follows from the intersection
```

Prohibited justifications: "looks unrelated", "tests were green before",
"only a small change". Default rule (§4): shared-substrate edits
(`completion.rs`, `session.rs`, `edge.rs`) force at least REUSE_PARTIAL with
rerun of the affected family; authority blob changes invalidate every cell
citing that authority.

## 17. Reproducibility contract

For every verdict-bearing evidence record, as applicable:

```text
ARCHITECTURE_SUBJECT_SHA        (a225d524046609dbbf9bef32d96fe84800bddd92)
FREEZE_RECORD_SHA               (Stage-7 record)
VALIDATION_CONTRACT_VERSION     (PLAYBACK-EXECUTION-VALIDATION-v1)
VALIDATION_CONTRACT_BLOB_SHA    (recorded by #212 at merge)
VALIDATION_EXECUTION_HEAD_SHA   (exact head the run executed)
CELL_ID
platform / OS version / toolchain (rustc/cargo channels; Java+TLC version for
  formal; nightly for Miri)
features / backend identity
harness/test identifier (suite + case)
seed / schedule / iteration count (loom bounds, soak iterations)
exact command
workflow-run ID (for CI-hosted runs)
logs / artifact references (retained in #211)
counterexample minimization data
```

Minimum retained evidence to keep a blocking counterexample alive:
`VALIDATION_EXECUTION_HEAD_SHA` + `CELL_ID` + exact command + full runner log
artifact + minimized reproduction (minimal input, or the failing
schedule/trace for Loom/TLA) + the §14 classification worksheet. Random or
stress observations without this replay metadata are SUPPORTING ONLY.

## 18. Accepted Stage-6 P3 debt (provenance only)

Recorded from #209 final checkpoint; none is a Stage-8 failure and none was
found to invalidate a validation oracle or required evidence surface during
contract design:

```text
B26  poisoned-lock funnel abort  — session.rs catch-unwind funnel `expect`s;
      no contract covers panic-abort/poisoned-lock recovery; locks are not
      held across provider/panic-prone calls, so no accepted claim is violated
B27  reader panic-hook name filter — install_reader_panic_hook filters by
      thread name; declared replaceable diagnostic mechanism, non-semantic
B28  EstablishmentAttempt::finish non-consuming (Rc clone) — naming debt only;
      single-attempt precondition is D1's and is validated as a precondition
```

If Stage 8 observes one of these materially corrupting an evidence surface,
classify via §14 — do not silently convert it into a mandatory failure.

## 19. Product-work gate

```text
PASS          ⇒ PRODUCT_WORK_GATE = OPEN
FAIL          ⇒ PRODUCT_WORK_GATE = CLOSED
INCONCLUSIVE  ⇒ PRODUCT_WORK_GATE = CLOSED
```

#212 freezes NO narrower exception now. Architecture-dependent downstream work
gated by this rule includes at minimum: #187 (Observation Plane), #188
(TUI v2). The gate flips only on the #211 final campaign verdict recorded
under this contract version.

## 20. Contract amendment and versioning

Any verdict-bearing change (cell set, evidence modes, PASS/FAIL semantics,
platform matrix, statuses in §11, taxonomy, protocols, reuse rules) requires:

```text
1. new version identity (PLAYBACK-EXECUTION-VALIDATION-vN)
2. impact analysis against accepted D1–D6 and the 20 rows
3. #212 record binding the new blob identity
4. #210/#211 rebinding to the new identity
```

Non-verdict-bearing edits (typo/routing/reproducibility-metadata additions)
may be recorded as patch-level notes in #212 without changing cell semantics.
No post-freeze change is valid that was designed after observing Stage-8
results for the affected cells (no retrospective criteria).

## 21. Design self-audit record (frozen with v1)

```text
D1–D6 mapped                                  = 6/6  (§6)
minimum core mapped                           = 20/20 (§7)
every mandatory cell has one evidence mode    = YES (§5)
every mandatory cell has PASS/FAIL/INCONCLUSIVE = YES (defaults §4 + overrides)
every mandatory cell has dependency/reuse     = YES (DEPENDENCIES + §4/§16)
no cell validated by broad CI green alone     = YES (each names its oracle; §8)
platform substitutions explicit               = YES (§8, §9)
stress/perf/Miri/Loom/model-checking/device   = frozen (§11)
counterexample taxonomy complete              = YES (§14)
architecture reopen path complete             = YES (§15.1)
implementation corrective path complete       = YES (§15.2)
oracle corrective path complete               = YES (§15.3)
environment failure path complete             = YES (§15.4)
reproducibility metadata complete             = YES (§17)
product-work gate mechanical                  = YES (§19)
no architecture claim changed by this contract = YES (§1; hunk ledger in PR)
```
