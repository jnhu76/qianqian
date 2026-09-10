# Realtime-View Publication and Reclamation — Mechanism Evidence

**Status:** EVIDENCE / ENGINEERING RECORD — **NOT NORMATIVE AUTHORITY**

**Tracking:** roadmap mechanism-validation rung / Issue #94 (Gate 3 evidence); validates the normative publication/reclamation contract P1–P5 of `docs/adr/ADR-PBK-001.md` §6 on a real Rust mechanism candidate.

**Authority gate:** this document is *not* an ADR and adds no normative fact beyond `ADR-PBK-001.md`. It records executable evidence, mechanism comparison and representation-specific observations. If any conclusion here ever earns normative status, it must go through human-reviewed ADR clarification, not this file.

**ADR impact: NONE.** ADR-PBK-001 already carries the plane separation (§1–§2), the realtime firewall (§2.4), the P1–P5 publication/reclamation contract (§6), and the research ladder (§12). This experiment validates a mechanism against the already-frozen semantics; it does not re-litigate them.

---

## 1. Reality audit

Base of this experiment is main at `4423762` (merge of the direct-flow evidence PR). Verified by inspection before construction; nothing was reset, forced, or replayed. The user's local `.gitignore` modification was left untouched.

- `ADR-PBK-001.md` is **ACCEPTED** and is the sole normative playback constitution; `docs/architecture/pcm-contract-a0.md` (PCM edge) and `docs/architecture/direct-pcm-flow.md` (direct flow) are evidence records (EVIDENCE, NOT NORMATIVE AUTHORITY), each with its executable test harness on main.
- The formal model `specs/realtime-publication/` has already closed the semantic question (publication/reader-overlap collision is real; P1–P5 are the normative conclusions). The mechanism comparison record `docs/architecture/realtime-publication-lifetime-decision.md` records the concept-level candidate matrix and the engineering leaning; its §12 declared two flip risks: (a) queued-reference dominance collapsing the fast read path, (b) final-drop destructor authority (dimension O).
- The direct-flow evidence established the execution model prior: the realtime side holds a **pre-bound flow acquired once at activation and executed across many quanta** (`PreboundPcmFlow` in `crates/qianqian-core/tests/direct_pcm_flow/`), and its hazard witness H1 is precisely the seam this experiment must answer: composition withdrawal alone does not revoke an already-extracted flow.
- Issue #94 (independent realtime runtime/kernel question) remains OPEN; Issue #12 (SRC/DSP/device) remains a downstream firewall.
- Working tree differs from main only in `crates/qianqian-core/tests/realtime_view_publication/` (new test-only harness). **Production `src/` delta is zero** (§18).

## 2. Normative P1–P5 mapping

The normative definitions live in ADR-PBK-001 §6 (P1 coherent publication, P2 retired-view closure, P3 quiescence before reclamation across all generations, P4 retirement ≠ reclaimability ≠ release, P5 conditional reclamation progress). This experiment maps each onto executable scenarios:

| Normative contract | Executable scenario | Test |
| --- | --- | --- |
| P1 coherent publication | acquisition sees whole N or whole N+1, never a mix | `acquisition_observes_whole_views_only`, split twin kill |
| P2 retired-view closure | new acquisition never enters a retired view; existing holders finish | `new_acquisition_never_enters_a_retired_view`, stale twin kill |
| P3 quiescence across generations | N/N+1/N+2 overlap; shared resources; queued references; stalled readers | `three_generations_overlap_with_shared_resources`, `queued_reference_counts_against_reclamation`, `stalled_reader_blocks_reclaimability_until_it_exits` |
| P4 lifecycle separation | retired / reclaimable / released are distinct observed states | `reclaimable_does_not_mean_released`, `stalled_reader_blocks_reclaimability_until_it_exits` |
| P5 conditional progress | reader exit enables recognition; no timeouts as semantic failures | `stalled_reader_blocks_reclaimability_until_it_exits`, `queued_reference_counts_against_reclamation` |

## 3. Baseline: the direct-flow view

The direct-flow evidence's `PreboundPcmFlow` is the static graph/view baseline: three pre-bound participant references plus one reusable block-storage arena, acquired once at activation, executed per quantum with zero composition-plane operations. This experiment reuses that shape as the view payload model:

```text
RealtimeView
  identity                 monotonic view identity
  topology_version         participant topology generation label
  participants             the bound participant names (source → stage → sink)
  resources                the dereferenceable surface (participant handles and
                           buffers modeled as tracked resources)
```

A reader acquires one pre-bound `Arc<RealtimeView>` at bind time and executes quanta through already-held handles; per-quantum execution never touches the publication mechanism. The view's dereferenceable surface is modeled by tracked resources so every dereference is externally observable. No PCM, decoder, device, SRC/DSP, or playback semantics are introduced (cage, §5).

## 4. Candidate mechanisms and early rejection

The decision record's concept-level matrix (§11–§12) already compared five mechanism families. This experiment re-derives the comparison for the **pre-bound execution model** (acquisition once per flow, long-held queued reference, publication rare):

| # | Candidate | Disposition for this execution model |
| --- | --- | --- |
| A | refcounted immutable view (Arc-like) | **SELECTED and validated** (§15). Acquisition = one `Arc::clone` at bind time (an atomic RMW on the strong count); the clone *is* the lifetime, so release-before-quiescence is structurally excluded in safe Rust; per-resource reclamation via resource reference counts. |
| B | epoch / RCU-like | **Early rejected.** Batch-level reclamation latency (a stalled reader delays the whole batch, not just its resource); `pin` cannot express a pre-bound queued reference held across a queue (long-pin anti-pattern — it would have to re-acquire per quantum, which is the lookup anti-shape the direct-flow evidence killed); unsafe surface (crossbeam-epoch). The pre-bound model gets none of epoch's scale benefits. |
| C | explicit reader-count / lease | **Subsumed by A in the pre-bound model.** A reader count and an `Arc` strong count cost the same RMW per acquisition; the difference is that a hand-rolled count is *decoupled* from the view lifetime (enabling release-with-holder bugs — the M1 twin demonstrates the class), while `Arc` couples them by construction. C's explicit count adds a control-side polling/等待 surface without adding safety. Answers to the candidate-C questions (§30) are recorded in §14. |
| D | hazard-pointer-like | **Early rejected.** Hazard slots are published at dereference time; they cannot express a pre-bound queued reference (the dominant shape here). Protocol complexity and unsafe surface are the decision record's stated costs, and the reader scale (1–2) does not justify them. |
| E | double-buffer / bounded slot | **Early rejected.** A fixed slot count cannot express N → N+1 → N+2 overlap while a reader still holds the first slot (the third publication must wait, block, or overwrite). The P3 multi-generation requirement (formal M4 class) is structurally at odds with bounded slots. |

Early rejection is grounded in the execution model, not in preference; the decision record's §12 flip-risk analysis is the source for B/D/E costs and is re-derived here for the pre-bound shape.

## 5. Evaluation cage

```text
NO real decoder            resources are synthetic tracked objects
NO real audio device       no OS/device API anywhere
NO SRC / DSP / gain        no value processing at all
NO UI                      no product payloads
NO playback commands       no seek/next/stop/EOF semantics
NO Fact system             no fact publication
NO playlist/session        no session authority
NO production API          test-only harness; production src delta = 0
NO unsafe                  std-only safe Rust
```

Threads are used only to manufacture publication/read interleavings and final-drop execution contexts (§12), not to claim a threading model.

## 6. Mechanism under test (test-only representation)

The validated mechanism, in `crates/qianqian-core/tests/realtime_view_publication/`:

```text
CONTROL SIDE
  build next immutable view                (control code; may allocate)
  publish(next)                            atomic swap of the current slot;
                                           old view moves into the retirement
                                           ledger; publication never touches
                                           resources and never waits for readers
  certify_reclaimable(identity)            moves a retired view to the
                                           reclaimable ledger iff its reader
                                           count is zero (quiescence certification)
  release_reclaimable(identity)            drops the ledger's last reference ->
                                           physical release on the control thread

REALTIME SIDE
  acquire()                                one clone of the current view (bind time)
  execute_quantum(...)                     dereferences already-held handles only
  drop the clone                           release; the ledger still holds a
                                           reference until control releases, so
                                           destruction is deferred off the RT path
```

Representation details, all test-only and unfrozen:

- publication slot: `Mutex<Arc<RealtimeView>>` — safe-Rust lock-free reads would need `unsafe` (as ArcSwap does); the std-safe form takes one lock per bind-time acquisition. Per-quantum execution takes no lock (it never touches the mechanism).
- views: immutable `Arc<RealtimeView>`; resources are `Arc<TrackedResource>` shared across views by clone, so reclamation is **per-resource**: a resource dies only when the last view referencing it dies.
- quiescence predicate: `Arc::strong_count(view) == 1` (only the ledger's own reference) — per-`Arc`, so it inherently covers every holder of every generation; there is no per-generation accounting that could forget an older one.
- certification/retirement/release: control-side operations on a single guarded ledger; the semantic states (live / retired / reclaimable / released) are observed externally by the tests, not stored as a production enum.

## 7. Executable scenarios and results

All 22 tests pass (`cargo test -p qianqian-core --test realtime_view_publication`; 5 consecutive runs green). Scenario-to-test mapping and evidence classes:

| Scenario | What it shows | Test | Class |
| --- | --- | --- | --- |
| S1 coherent publication | acquisition observes whole N or whole N+1 | `acquisition_observes_whole_views_only` | EXECUTABLE ORACLE |
| S1 adversarial | split publication is observed as a mixed view | `split_publication_produces_mixed_generation_observation` | EXECUTABLE ORACLE (twin) |
| S2 closure | new acquisition never enters a retired view; existing holder finishes | `new_acquisition_never_enters_a_retired_view` | EXECUTABLE ORACLE |
| S3 old reader finishes | resources stay valid until the old reader exits | `old_reader_finishes_after_publication` | EXECUTABLE ORACLE |
| S4 queued reference | acquired-but-queued reference counts against reclamation, may execute later | `queued_reference_counts_against_reclamation` | EXECUTABLE ORACLE |
| S5 N→N+1→N+2 overlap | three generations overlap; eligibility per generation | `three_generations_overlap_with_shared_resources` | EXECUTABLE ORACLE |
| S6 shared resources | per-view retirement does not release shared resources | `shared_resources_survive_per_view_retirement` | EXECUTABLE ORACLE |
| S7 stalled reader | retirement without reclaimability is legal, not a timeout | `stalled_reader_blocks_reclaimability_until_it_exits` | EXECUTABLE ORACLE |
| S8 eventual quiescence | reader exit enables recognition and release | `stalled_reader_blocks_reclaimability_until_it_exits` | EXECUTABLE ORACLE |
| S9 release separation | reclaimable ≠ physically released | `reclaimable_does_not_mean_released` | EXECUTABLE ORACLE |
| S10 final-drop thread | destruction context observed (control vs reader thread) | `final_destruction_runs_on_the_control_thread_when_reader_releases_first`, `final_drop_on_the_reader_thread_destroys_there_hazard_witness` | MEASURED (thread identity) |
| S11 acquire/release allocation | bind-time acquire and release are allocation-free | `acquisition_and_release_are_allocation_free` | MEASURED (counting allocator) |
| S12 blocking classification | quantum path carries no mechanism lock | `pre_bound_reader_executes_without_the_publication_mechanism` | STRUCTURAL CODE EVIDENCE |
| S13 control waits don't block RT | publish does not wait for readers; waiting twin blocks RT | `publication_does_not_wait_for_readers`, `publisher_waiting_for_reader_blocks_reader_acquisition` | EXECUTABLE ORACLE (both) |
| S14 publication atomicity | failed validation leaves the current view untouched | `failed_publication_leaves_the_current_view_untouched` | EXECUTABLE ORACLE |
| S15 release order | resources destroyed only after the last referencing view is released | `resources_are_destroyed_only_after_all_referencing_views_are_released` | EXECUTABLE ORACLE |
| stress | real thread interleavings; whole views only; no released dereference | `concurrent_publication_and_readers_observe_only_whole_views` | EXECUTABLE ORACLE (stress) |

## 8. Formal → Rust mutation mapping

The formal model's mutation classes are mapped to real Rust twins; the model filenames and mutation numbers stay out of the code (test names describe behavior).

| Formal witness | Rust mutation | Rust oracle | killed? |
| --- | --- | --- | --- |
| release before quiescence | `ReleaseAtPublication` publishes by dropping the old view with no ledger (weak-handle reader) | `WeakHandlesReader` dereference fails with `UseAfterRelease` after publication | YES |
| split publication | `SplitPublication` publishes the topology half and the resource half in two steps | `SplitViewObservation::is_coherent()` detects topology-from-N+1 with resources-from-N | YES |
| stale entry | `StaleAcquisition::acquire_stale` resolves a cached identity without revalidating closure | acquired identity ≠ current identity after publication | YES |
| forgets older retirement | `CertificationTwin` (latest-retired-only predicate) certifies every retired view when the latest is quiescent | reclaimable-with-holder detected (claim contradicts `reader_hold_count`) | YES |
| queued reference forgotten | `CertificationTwin` (active-executions-only predicate) certifies a view with a queued reference | reclaimable-with-queued-holder detected; honest certification refuses | YES |
| publisher waits on reader | `PublisherWaitsForReader` waits for the reader while holding the shared slot lock | realtime reader's acquisition is observably blocked | YES |

The honest mechanism passes every corresponding positive scenario (S1–S15); every twin is executed and killed by its oracle. Notably, the release-before-quiescence class is **structurally excluded** for strong pre-bound readers in the tested mechanism (the reader's clone *is* the lifetime — physical release with a holder is impossible in safe Rust); the twin demonstrates the class via the decoupled weak-handle shape, which is exactly the shape the direct-flow evidence's anti-lookup control warned about.

## 9. Realtime acquire/release evidence

**Allocation (MEASURED, thread-scoped counting allocator):** bind-time `acquire()` = 0 allocations; reader `release()` = 0 allocations; 64 acquire/release cycles = 0 allocations. (Per-quantum execution allocates nothing in the mechanism by construction — it touches only held handles; the direct-flow evidence already measured 0 allocations per quantum on a real PCM-shaped path.)

**Blocking (structural classification, honest):**

```text
bind-time acquisition    Mutex lock on the publication slot — BLOCKING POSSIBILITY
                         under contention; happens once per flow at bind time
per-quantum execution    no lock, no mechanism access — NON-BLOCKING by construction
reader release          Arc drop — no blocking
control publish          one lock — no waiting on readers
control certification   one lock — no waiting on readers
control release         one lock — physical drop on the control thread
```

The quantum path is lock-free not by tuning but by shape: `execute_quantum` is a method on the view and has no mechanism parameter (§12 test drops the mechanism entirely and the reader still executes).

## 10. Destructor / final-release evidence

The task's central destructor question — *which thread performs final destruction, and can it be the realtime path* — is answered with measured thread identity:

- **Control-side release (deferred disposal):** when a reader releases its clone on its own thread while the ledger still holds the view, no resource is destroyed there; the destruction happens on the control thread when `release_reclaimable` drops the ledger's last reference (`final_destruction_runs_on_the_control_thread_when_reader_releases_first`).
- **Hazard witness:** in the extracted shape — the pre-bound flow handed to the realtime side as its *only* strong reference, with no control-side ledger (the direct-flow H1 shape) — the final drop runs wherever the flow dies; the test moves the only strong reference to a named reader thread and observes destruction with the reader thread's identity (`final_drop_on_the_reader_thread_destroys_there_hazard_witness`). This is a HAZARD WITNESS: the mechanism's ledger is what keeps destruction off the RT path; the extracted shape without a ledger retains the hazard. **Memory safety is not realtime safety**: the resources stay memory-safe in both cases (safe Rust), but the destructor cascade (and any blocking/allocation inside it) runs on the reader thread in the second case.
- Deferred disposal is therefore a property of the tested mechanism (the ledger's reference), not an added `DisposalThread`: physical release is simply not reachable from the reader path while the ledger exists. This is the mechanism's answer to the decision record's dimension O.

## 11. Multi-generation / resource-sharing evidence

- N → N+1 → N+2 overlap executes with three distinct holders (`three_generations_overlap_with_shared_resources`): eligibility is per generation; N is reclaimable only after its own reader exits, independent of N+1/N+2.
- Resource A shared across all three generations is destroyed only when the last referencing view is released (`shared_resources_survive_per_view_retirement`, `resources_are_destroyed_only_after_all_referencing_views_are_released`); per-view retirement is never treated as per-resource reclaimability (formal M4 class, killed).
- Quiescence certification covers ANY generation because the predicate is per-`Arc` (a count of every clone, of every generation) — there is no generation bookkeeping to get wrong. The certification twin that *does* keep per-generation shortcuts (latest-retired-only) is killed.

## 12. Queued-reference evidence

A queued reference is an acquired clone whose execution has not started. In the tested mechanism it is a counted holder: it blocks certification (S4), may legally execute later against the retired view, and only its exit enables progress. There is no separate queued-reference representation to forget — a clone is a clone, whether executing or queued; the certification twin that tries to distinguish (active-executions-only) is killed. This is the mechanism's structural answer to the formal model's queued-reference fold: the safety predicate cannot distinguish them, and that is exactly why it is safe.

## 13. Hazard witnesses

```text
H1  final drop context = last-strong-reference drop location; the extracted
    (no-ledger) shape allows destruction on the RT reader thread (MEASURED)
H2  release at publication in a decoupled-lifetime design is an observable
    use-after-release (twin killed; excluded structurally in the Arc shape)
H3  control-side wait for readers while holding the shared lock blocks the
    realtime reader (twin killed; honest publish never waits)
```

None of these are correctness claims about the honest mechanism; they are the pressure witnesses the mechanism must answer, each answered by an executable positive scenario.

## 14. Candidate comparison (for the pre-bound execution model)

| Property | A: refcounted immutable view (VALIDATED) | B: epoch/RCU | C: reader-count | D: hazard | E: double-buffer |
| --- | --- | --- | --- | --- | --- |
| P1 coherent acquisition | single immutable view (whole) | whole view via epoch read | needs a single-handle view | needs a single-handle view | fixed-slot + slot tags |
| P2 closure to new readers | swap closes; per-Arc identity | closure by epoch check | closure by current-slot check | closure by slot publication | slot overwrite semantics |
| P3 all-generation quiescence | per-Arc count (no generation bookkeeping) | batch epoch advance | per-view count | per-object scan | slot turnover (bounded) |
| P4 lifecycle expressibility | observed states, no enum required | retired vs collectable | count == 0 vs released | retired list vs scan | slot states |
| P5 progress | last drop → reclaimable (conditional on reader exit) | epoch advance (batch) | count → 0 (control polls) | scan progress | slot turnover |
| RT acquire allocation | 0 (MEASURED) | 0 | 0 | 0 | 0 |
| RT acquire blocking | bind-time Mutex (once per flow) | none (pin) | RMW only | TLS slot | none |
| RT release allocation | 0 (MEASURED) | 0 | 0 | 0 | 0 |
| RT release blocking | none | epoch read | RMW | TLS clear | slot clear |
| final destruction on RT possible | no (ledger defers; hazard only in no-ledger extracted shape) | no (collector) | no (control after count==0) | no (control retire) | no (control slot reuse) |
| queued pre-bound refs | native (clone) | long-pin anti-pattern | native (counted) | not expressible | not expressible |
| N/N+1/N+2 | native | native | native | native | slot bound |
| stalled reader | delays only its own resource | delays whole batch | delays that view | delays that object | blocks slot chain |
| shared resources | per-resource refcount | batch-level | per-view count (resource sharing needs per-resource) | per-object | per-slot |
| proof surface | matches formal model directly (clone=holder, drop=exit, last drop=release) | epoch→certification argued outside model | count→certification direct | protocol bookkeeping | slot reuse outside model |
| safe-Rust std-only | yes (tested) | no (crossbeam) | yes | no (canonical impls) | yes |
| campaign/leakage risk | minimal representation | guard discipline in API | explicit count in API | protocol in API | slot states in API |

For candidate C's own questions (§30): counter increment/decrement = `Arc::clone`/drop in the tested shape; ABA/generation mismatch = structurally excluded (per-view `Arc`, monotonic identities, no slot reuse); multi-view overlap = native; resource sharing = per-resource clones; queued refs = counted clones; last reader = last clone drop; overflow/underflow = the atomic count is monotonic per `Arc` and cannot underflow (drop only decrements owned references). The reason C is not independently implemented is the coupling finding: an explicit count decoupled from lifetime is what the M1 twin demonstrates as dangerous, and `Arc` is the safe expression of the same count with the lifetime attached.

## 15. Mechanism recommendation

**The refcounted immutable view (candidate A) with per-resource reference counting and control-side certification/release is the validated mechanism** for the pre-bound execution model, satisfying P1–P5 under the test cage with the following boundaries:

- what authority it would own: published realtime-view identity, publication (retirement) ordering, reader acquisition legality, quiescence certification, reclamation eligibility, resource-release coordination;
- what API seam is unavoidable: a control-side publication point (build → publish) and a bind-time acquisition point (acquire → pre-bound clone); per-quantum execution needs neither;
- what remains representation-specific: the `Mutex` slot, the `Arc`/per-resource layout, certification latency, memory-ordering details (see OPEN);
- why K0 cannot own it: K0 is the domain-agnostic composition kernel (existence/reachability/lifecycle) and must not know realtime views or PCM; the publication contract is a realtime-plane responsibility, and the direct-flow H1 witness shows composition withdrawal alone does not provide retirement/revocation semantics.

The recommendation is an evidence-based engineering finding, **not a production freeze**. Production adoption requires the human gate and must re-derive representation under real decoder/device pressure (Issue #12 firewall).

## 16. Earned facts

**EARNED** (for the tested representation, with oracles named):

- E1 — A reader acquires whole views only: identity, participant topology and resource membership always belong to the same publication (EXECUTABLE ORACLE, S1 + stress; split twin killed).
- E2 — Retirement closes a view to new acquisitions while existing holders finish legally (EXECUTABLE ORACLE, S2/S3; stale twin killed).
- E3 — Quiescence eligibility covers every generation and every holder, including queued references; N→N+1→N+2 overlap executes with per-generation eligibility (EXECUTABLE ORACLE, S4/S5; certification twins killed).
- E4 — Shared resources survive per-view retirement and are released only when the last referencing view is released (EXECUTABLE ORACLE, S6/S15).
- E5 — Retired, reclaimable and released are distinct observed states; reclaimable does not imply physical release (EXECUTABLE ORACLE, S7/S9).
- E6 — Conditional progress: a stalled reader blocks reclaimability; its exit enables certification and release; no timeout is treated as a semantic failure (EXECUTABLE ORACLE, S7/S8).
- E7 — Bind-time acquisition and reader release are allocation-free (MEASURED, counting allocator, 0/0/0 over 64 cycles).
- E8 — Per-quantum execution carries no mechanism lock: after acquisition the mechanism can be dropped entirely and the reader still executes (STRUCTURAL CODE EVIDENCE).
- E9 — Publication does not wait for readers; a control-side wait for readers blocks the realtime reader and is killed as a twin (EXECUTABLE ORACLE).
- E10 — Failed publication validation leaves the current view untouched (EXECUTABLE ORACLE).
- E11 — Final destruction runs on the control thread under the ledger shape; the no-ledger extracted shape permits destruction on the reader thread — the destructor-authority fact the decision record's dimension O required (MEASURED, thread identity).
- E12 — The release-before-quiescence bug class is structurally excluded for strong pre-bound readers (a clone is the lifetime) and demonstrated as observable on the decoupled weak-handle shape (TYPE-SYSTEM / STRUCTURAL + EXECUTABLE ORACLE).

**HAZARD / PRESSURE WITNESSES** (inputs for the mechanism decision, not correctness claims): H1 final-drop context; H2 decoupled-lifetime use-after-release; H3 publisher-reader coupling (§13).

**OBSERVED FOR TESTED REPRESENTATION** (facts about these test shapes, not general claims): views/resources are `Arc`-based safe-Rust test objects; the publication slot is a `Mutex` (bind-time); certification uses `Arc::strong_count`; threads are test threads, not a threading model.

**OPEN** (untouched by design): memory-ordering details (SeqCst used; minimal required ordering unexamined), certification recognition latency, publication slot lock-free variants (require unsafe), parameter updates vs topology updates (ADR §7), production view layout, real decoder/device pressure (Issue #12), and every production naming question.

## 17. Representation-specific observations

- The `Mutex` on bind-time acquisition is a safe-Rust consequence: a lock-free read path (ArcSwap-style) requires `unsafe` internally. In the pre-bound model this matters little (one lock per flow lifetime); it would matter in a per-quantum-acquire execution model, which the direct-flow evidence does not support.
- `Arc::strong_count` as the quiescence predicate is exact in these deterministic tests; production certification would need to decide recognition semantics (poll/collect) and ordering under real concurrency.
- The `Mutex<Vec<...>>` observer log allocates on the destruction path; the measured 0-allocation claims are scoped to acquire/release, and per-quantum allocation is covered by the direct-flow evidence instead.
- The `ViewState` readout is a test-side classification of the P4 semantics, not a production requirement (consistent with ADR §6: P4 is semantic, not representation).

## 18. Production delta and naming gates

- `git diff origin/main -- 'crates/**/src/**' 'apps/**/src/**'` is **empty** — production `src/` delta is zero; the only change is the new test directory under `crates/qianqian-core/tests/`.
- Campaign-name gate over `crates apps`: **zero new hits** — test names, module names, types and comments use only semantic vocabulary (`realtime_view_publication`, `PublishedViews`, `RealtimeView`, `reader`, `queued`, `retirement`, `quiescence`, `reclamation`, `release`). The existing hits in the tree are pre-dating this experiment (kernel implementation-ADR references, legacy experimental code) and are untouched.
- Durable-doc naming gate: this document contains no stage-campaign tokens (the A–E ladder labels) in its body; roadmap and issue provenance appears only as tracking references (§1, §19), which is its permitted role.

## 19. Issue #94 implication

Issue #94's gates, in the issue's own terms:

```text
Gate 1 — Data language (minimal PCM edge semantics)      already earned (PCM edge evidence)
Gate 2 — Independent execution (pre-bound Source→Stage→Sink
         without composition/control machinery)          already earned (direct-flow evidence)
Gate 3 — Independent runtime responsibility (a real
         publication/replacement/reclamation mechanism
         satisfying P1–P5 with a stable responsibility)   THIS EXPERIMENT
```

This experiment validates a mechanism satisfying P1–P5 on the pre-bound execution model, and shows the responsibility is a coherent unit: published realtime-view identity, publication/retirement ordering, reader acquisition legality, quiescence certification and resource-release coordination are all maintained by one mechanism, and the certification twins demonstrate that shortcutting or fragmenting that logic reproduces the formal bug classes. K0 does not own it (domain-agnostic composition), and the direct-flow H1 witness shows a plain service convention does not provide it.

On the strength of this evidence: **INDEPENDENT RUNTIME RESPONSIBILITY EARNED** (mechanism evidence). Whether that responsibility deserves a stable runtime/kernel name (and which) remains a human decision; no production runtime, kernel, or new API is created here, and Issue #94 is deliberately left OPEN.

## 20. Gates

```text
cargo fmt --check                                                PASS
cargo check --workspace                                          PASS
cargo test --workspace                                           PASS (22 new tests; full workspace green)
cargo clippy --workspace --all-targets --all-features -D warnings PASS
git diff --check                                                 PASS
production src delta                                             ZERO
campaign naming gate (crates apps)                               ZERO new hits
```

The new suite: 22 tests (16 scenarios/stress + 6 mutation kills), including the concurrent stress test and thread-identity destructor oracles.
