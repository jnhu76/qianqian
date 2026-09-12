# DIRECT-PCM-FLOW — Direct-flow Graph Executable Evidence

**Status:** EVIDENCE / ENGINEERING RECORD — **NOT NORMATIVE AUTHORITY**
**Scope:** Issue #95 / ADR-PBK-001 §12 evidence ladder Phase C (direct-flow graph); targets ADR §14 gate G4 (direct realtime data-flow executable experiment — DELIVERED by this evidence, PR #96)
**Authority gate:** This document is *not* an ADR and adds no normative fact beyond [`ADR-PBK-001.md`](../adr/ADR-PBK-001.md). It records executable evidence and representation-specific observations. If any conclusion here ever earns normative status, it must go through human-reviewed ADR clarification, not this file.
**ADR impact: NONE.** ADR-PBK-001 already carries the plane separation (§1–§2), the realtime firewall (§2.4), P1–P5 (§6), and the ladder (§12). This experiment adds executable evidence only.

---

## 1. Reality audit

Base of this experiment is main at `686abf7` (merge of PR #93, the corrective that zeroed campaign identity and renamed pull ownership). Verified by inspection before construction; nothing was reset, forced, or replayed.

- `ADR-PBK-001.md` is **ACCEPTED** and is the sole normative playback constitution; `docs/architecture/pcm-contract-a0.md` is PCM edge evidence (EVIDENCE, NOT NORMATIVE AUTHORITY), and its `tests/pcm_edge_contract/` harness is executable PCM edge evidence on main.
- The generic Composition Kernel (K0) is implemented and current: `Context / Capability / Fiber / Effect / Reconcile`, with synchronous control-plane `&mut Kernel`, activation-only `ActivationCtx::resolve`, and `Kernel::debug_op_count()` — a `#[doc(hidden)]` witness counting every public kernel operation (implementation ADR D9).
- The architectural precedent for resolve-once/bind-outside is two-layered. **At this experiment's base**, the production `AppRuntime` carried that witness: it resolved `AudioOutputCapability` once during `on_activate` and cached the `Rc` service outside kernel storage. **On current main**, that hardcoded witness has been removed from the composition root (PR #103); the root keeps no parallel service-handle state outside the kernel. The pattern survives as executable evidence in this harness's `flow_assembler` composition (§3) — test instrumentation, not a normative production authority.
- Issue #92 (PCM edge contract) was OPEN at this experiment's base and has since been closed; PR #93 merged with commit `686abf7`. Issue #12 (SRC/DSP research) and Issue #90 (OCR) are separate downstream tracks and were not touched.
- Working tree: only `tests/`, one test-shared instrumentation file, `crates/qianqian-audio-api/Cargo.toml` (dev-dependency on `qianqian-composition`), and `crates/qianqian-audio-api/tests/pcm_edge_contract/main.rs` (allocator extraction) differ from main. **Production `src/` delta is zero** (§13). Local untracked `.gitignore` modification from the user's machine was preserved untouched.

---

## 2. Boundary being tested

The direct-flow evidence boundary (ADR §12 ladder):

```text
Source -> one processing stage -> Sink
```

with the runtime hot path **not** entering Context / Reconcile / EventBus. Concretely, this experiment answers:

> Can a minimal executable direct PCM path run every quantum through pre-bound data edges — zero per-quantum composition-plane operations, zero re-resolution, zero allocation — without inventing a production AudioGraph framework?

The boundary under test is the seam between the composition plane (kernel-mediated setup) and the realtime data plane (per-quantum execution). What crosses it: three pre-bound participant references and one reusable block buffer, established exactly once during activation. What must not cross it per quantum: Context lookup, capability resolution, Fiber Reconcile, generic event fan-out, plugin registry traversal, and unbounded allocation (ADR §2.4).

Not in scope (firewall): SRC / DSP / gain / EQ / limiter, WASAPI or any device runtime, publication/replacement mechanisms (a later ladder rung, validated against §6 P1–P5), old playback vocabulary (`MusicKernel`, `TransportKernel`, `TrackSession`, `DecodeSession`, `Generation`, `Active`, `Prepared`, `Dual Window`, `Physical Fence`, `PlaybackEnded`) — none of those names appear anywhere in the test tree.

---

## 3. Setup path vs hot path

**Setup (composition plane, runs exactly once per flow):** a real K0 kernel hosts four components — `stream_source`, `processing_stage`, `stream_sink`, `flow_assembler`. Each participant component provides a capability whose service is `RefCell<dyn PcmSource | PcmStage | PcmSink>`; the assembler component requires all three and, during its activation, calls `ctx.resolve` exactly once per capability, then constructs `PreboundPcmFlow` and stores it in a `FlowSlot` (`Rc<RefCell<Option<PreboundPcmFlow>>>`) outside kernel storage. This mirrors the resolve-once/bind-outside witness the production `AppRuntime` carried at this experiment's base (§1): resolution is an activation-episode act, and the result outlives the episode as plain data.

**Hot path (realtime data plane, per quantum):** `PreboundPcmFlow` holds only three `Rc<RefCell<dyn …>>` participant references plus one `Vec<Sample>` block buffer. It has no kernel handle, no registry, no capability table — re-entry into the composition plane is structurally impossible, not merely unexercised. `run_quantum(frames)` performs exactly:

```text
source.lend_next_block(&mut block_storage, frames)   // participant invocation
stage.process(block)                                  // participant invocation
sink.consume(&processed)                              // participant invocation
```

The kernel's `&mut Kernel` is not borrowed on this path; the control-plane may settle or change the desired composition concurrently (single-threaded test model) without revoking the already-extracted flow — a hazard witness (H1), not a correctness property, because this experiment has no retirement mechanism.

---

## 4. Test topology

```text
crates/qianqian-audio-api/tests/
    common/counting_allocator.rs        thread-scoped counting allocator (shared with the PCM edge experiment)
    pcm_edge_contract/harness.rs        PCM edge harness, reused verbatim (Sample, PcmFormat,
                                        PcmView/PcmViewMut, SyntheticProducer, SyntheticConsumer,
                                        TransferError)
    pcm_edge_contract/main.rs           PCM edge tests; allocator block extracted to common/
    direct_pcm_flow/
        participants.rs                 role traits + honest participants + PreboundPcmFlow
                                        + seam-placed observation doubles (ObservedSource /
                                        ObservedStage / ObservedSink + VisitLog)
        composition.rs                  kernel-mediated setup: capabilities, provider components,
                                        flow assembler (resolve-once), directory-lookup control
                                        assembler, CompositionFixture
        mutations.rs                    anti-shape controls + adversarial twins (kill_tests)
        main.rs                         8 scenario tests + compile-fail subprocess runner
        compile_fail/
            honest_stage_use_compiles.rs        positive control (rustc subprocess)
            stage_retains_block_past_call.rs    negative control (rustc subprocess)
```

19 tests total: 8 scenario, 9 kill/control, 2 compile-fail (both executed as `rustc` subprocesses). All test-only; no library API added or changed.

**Participants.** `PcmSource::lend_next_block` fills caller-provided storage and returns a checked whole-frame `PcmViewMut`; `PcmStage::process<'block>(&mut self, PcmViewMut<'block>) -> PcmView<'block>` transforms in place; `PcmSink::consume` verifies and records. Honest participants: `StreamingSource` (wraps the PCM edge `SyntheticProducer`, cross-block cursor), `IdentityStage`, `HalfAmplitudeStage` (deterministic per-sample halving — a value transform, not SRC/DSP), `VerifyingSink` (format identity + PCM edge per-sample oracle in stream order, `with_sample_scale` for the 0.5 twin).

**Anti-shape control.** `DirectoryLookupFlow` is built through the *same* kernel-mediated setup but consults a `ParticipantDirectory` every quantum — a per-quantum lookup seam of any kind, not necessarily the kernel itself. It delivers identical correct data; only the seam-placed directory log detects it. This is the control proving data-correctness ≠ architecture-correctness.

---

## 5. Instrumentation

Every oracle is external to the participants — no cooperative self-report counters exist anywhere in this harness.

```text
MEASURED                  kernel witness: Kernel::debug_op_count() delta over 4096 quanta;
                          counting allocator (setup vs steady state, per-quantum copy twin)
EXECUTABLE ORACLE         seam-placed observation doubles record visits to a shared VisitLog;
                          ParticipantDirectory records every lookup access at the seam;
                          value oracle (PCM edge SyntheticProducer/Consumer) fires on real state
STRUCTURAL CODE EVIDENCE  storage pointer identity (sink observes the flow's block storage);
                          PreboundPcmFlow's field set (no kernel handle, by construction)
TYPE-SYSTEM EVIDENCE      PcmStage::process<'block> shape (implicit for<'block>): block
                          lifetime cannot be retained in self; executed via rustc subprocess
                          positive + negative fixtures
CURRENT CODE FACT         statements about today's repository, checkable by inspection
ADR NORMATIVE INPUT       plane separation / firewall / ladder already normative in the ADR;
                          listed, not re-earned
```

**Witness sensitivity:** the kernel-op-count zero is only meaningful if the witness can move; after the 4096-quantum loop a control-side `settle()` demonstrably bumps the counter. **Allocation:** thread-scoped counting allocator (`#[global_allocator]` + per-thread armed windows), shared code with the PCM edge experiment so both evidence records measure through one implementation. **Value oracle strength:** the deterministic generator makes every sample a function of (frame, channel), so any reorder, duplicate, or loss that shifts stream position fires on the first misplaced sample.

---

## 6. Adversarial mutations

Anti-shape control plus mutations M1–M7 from the experiment plan. Every row is executed; honest baselines are in §7 and the value oracle is deliberately kept in the harness so its blindness to architecture-only defects is demonstrated rather than asserted.

| # | class | twin | bug class | honest baseline | oracle that kills it | executed? | killed? |
|---|-------|------|-----------|-----------------|----------------------|-----------|---------|
| 0 | CONTROL | `DirectoryLookupFlow` | per-quantum lookup seam (M1, generic seam) — data still correct | value oracle passes | seam-placed directory log: 30 accesses / 10 quanta (3/quantum), kernel ops unchanged | YES | YES |
| 1 | KILL | `HiddenLookupStage` | M2: hidden lookup inside a helper | value oracle passes | seam log: 10 accesses / 10 quanta | YES | YES |
| 2 | KILL | `FrameDroppingStage(drop=5)` | M3: silent mid-stream block loss | value oracle passes | value oracle fires at first post-drop block (cursor shift) **and** conservation cross-check (produced ≠ consumed) | YES | YES |
| 3 | KILL | `FrameDroppingStage(drop=10)` | M3: silent final-block loss | — | frame-conservation cross-check → `FramesLost` (36 of 40 delivered) | YES | YES |
| 4 | KILL | `FramePermutingStage` | M4: frame reorder inside a block | — | value oracle → `SampleMismatch` | YES | YES |
| 5 | KILL | `ReplayingSink` | M4: duplicate block delivery | — | value oracle → `SampleMismatch` | YES | YES |
| 6 | KILL | `LeakingCopyStage` | M5: storage substitution through the same stage interface (values still correct) | storage identity: sink observes flow's block storage | storage pointer identity differs (leaked addr) | YES | YES |
| 7 | KILL | `LeakingCopyStage` | M6: per-quantum allocation on substitution | steady state: 0 allocations / 64 quanta | counting allocator: ≥ 4 allocations / 4 quanta | YES | YES |
| 8 | KILL | `RetainingStage` (compile-fail) | M7: downstream retention past lifetime | honest positive fixture compiles | `rustc` rejection — executed subprocess: `error: lifetime may not live long enough` | YES | YES |
| 9 | KILL | `FormatSwappingSource` | mid-stream format change (rate-only swap; sample values alone would verify) | format identity per block | `FormatMismatch` | YES | YES |

Row 0 is the load-bearing control: the anti-shape and the honest flow deliver byte-identical values through byte-identical setup; only the seam log sees the difference. Rows 2–3 split M3 deliberately: mid-stream loss misaligns the stream-order value oracle, final-block loss leaves nothing to misalign — so the conservation cross-check (produced vs consumed from real participant cursors, `FramesLost` classification being test-side bookkeeping) is the killer there. Stated plainly so no oracle is overstated.

Rows 6–7 are one same-interface twin (`LeakingCopyStage`, `Box::leak` — an intentional test-only leak over a few quanta): it implements the exact stage trait and its values verify in order, so the lifetime typing alone cannot vouch for storage provenance; only the storage-identity and allocation oracles see the substitution.

---

## 7. Results

All 19 tests green (`cargo test --workspace`: `direct_pcm_flow` 19 passed, `pcm_edge_contract` 34 passed; full workspace 20 suites ok).

**Hot-path firewall (MEASURED):** setup performs real composition work (`debug_op_count > 0`); then 4096 quanta of 8 frames perform **zero** kernel operations (witness delta 0), and a post-loop `settle()` proves the witness is live. The bound flow runs 1 / 3 / 17 / 4096 quanta and block sizes 1 / 3 / 17 / 256 with 100-frame streams including partial final blocks, all frame-conserved (source produced == sink consumed).

**Allocation (MEASURED):** setup > 0 allocations (kernel registry, participants, block storage — real work); 64 steady-state quanta = 0 allocations. The same-interface copy twin measures ≥ 4 allocations / 4 quanta, i.e. the zero is a measurement, not an unobservable path.

**Ordering (MEASURED):** over 5 quanta the visit log is exactly `source → stage → sink` repeated — each participant visited exactly once per quantum in stream order; the stage hands every incoming frame onward (frames_in == frames_out == quantum totals).

**Storage identity (STRUCTURAL):** the sink observes the flow's own block storage address on the honest path and on the in-place transform path — no intermediate storage is inserted; the same-interface copy twin fails this identity.

**Lifetime (TYPE-SYSTEM EVIDENCE):** positive fixture (sequential borrow + storage reuse) compiles against the real trait shape; the retention twin fails to compile with the lifetime error family. The `for<'block>` shape makes block retention past the call unnameable. **Storage provenance is a separate property:** the same-interface copy twin (rows 6–7) substitutes storage under this exact signature and is caught only by the storage-identity and allocation oracles.

**Withdrawal witness (EXECUTABLE, HAZARD / PRESSURE WITNESS):** after 32 frames flow correctly, the source provider leaves the desired composition and the kernel settles (reconcile runs on the control side); the already-extracted flow remains callable and verifies without re-resolution. This is recorded as a hazard witness, not a correctness requirement: K0 withdrawal alone does not revoke already-extracted pre-bound references, and nothing here claims new realtime entries after withdrawal are legal (see H1).

**Anti-shape (EXECUTABLE ORACLE):** the per-quantum-lookup control delivers correct data end to end (value oracle silent) while the seam log records 30 accesses across 10 quanta and the kernel witness stays flat — the lookup seam bypassed the kernel entirely and is invisible to both the value oracle and the kernel counter, visible only at the seam.

---

## 8. Earned facts

**EARNED** (for this tested representation, with the oracles named):

- E1 — A pre-bound direct flow can run arbitrary quanta with zero composition-plane operations per quantum (MEASURED, kernel witness, 4096 quanta; sensitivity proven).
- E2 — Steady-state quanta can be allocation-free (MEASURED, counting allocator, 64 quanta).
- E3 — Source → stage → sink visits each participant exactly once per quantum in stream order (MEASURED, seam doubles).
- E4 — The tested pre-bound flow does not contain a kernel/registry handle, so runtime invocation does not itself re-resolve through the composition plane (STRUCTURAL).
- E5 — Data correctness is not architecture correctness: a per-quantum lookup control passes the value oracle and is killed only by the seam log (EXECUTABLE ORACLE, control row).
- E6 — The tested stage trait prevents retaining the borrowed input block past the call (retention lifetime, enforced by type; executed rustc fixtures). Storage provenance is NOT encoded by this trait and is independently checked by storage-identity and allocation evidence (rows 6–7).
- E7 — Silent loss (mid-stream and final-block), reorder, duplicate delivery, hidden intermediate storage, per-quantum allocation, and mid-stream format change are all killed by honest external oracles (EXECUTABLE ORACLE rows 1–9).

**HAZARD / PRESSURE WITNESS** (recorded as input for the mechanism-validation rung, not as a correctness requirement):

- H1 — Removing the source provider from K0 composition does not invalidate an already-extracted Rc-based flow; the stale bound reference remains callable because no realtime retirement/publication mechanism exists in this experiment (EXECUTABLE). This is NOT a claim that new realtime entries after withdrawal are legal: composition withdrawal proves loss of K0 reachability, not retirement or revocation of an already-published realtime execution view.

**OBSERVED FOR TESTED REPRESENTATION** (facts about this harness's shapes, not general claims):

- O1 — `Sample = f32`, `PcmFormat { sample_rate, channel_count }` with zero channels unconstructable, whole-frame checked `PcmView`/`PcmViewMut`.
- O2 — Participant references are `Rc<RefCell<dyn …>>` (single-threaded); per-quantum participant invocation is a direct pre-bound trait call — virtual dispatch at the participant call, not composition-plane generic dispatch. No thread-safety, cache, or real-device latency claims.
- O3 — Block handoff is borrow-based with one reusable flow-owned buffer; the "zero intermediate storage" fact is about this shape (a copy shape is legal and pays measured allocation — PCM edge evidence row 9 precedent).
- O4 — All measurements are about these test implementations, not about "the edge never allocates" in general.

**OPEN** (untouched by design, per the ladder):

- Publication/replacement mechanism for the graph — a later ladder rung, validated against P1–P5; explicitly not exercised (control-side composition withdrawal is not realtime graph replacement; see H1).
- Real decoder / real output pressure; SRC/DSP/gain — Issue #12; both downstream and must not retro-design this evidence.

**REJECTED:**

- R1 — "The composition kernel must be on the hot path for correctness." E1/E4: the bound path delivers verified data with zero kernel ops; the kernel's role is setup and control.
- R2 — "A per-quantum lookup is acceptable as long as data stays correct." Control row: correct data, killed by the seam log.
- R3 — "Cooperative self-report counters are instrumentation." This harness contains none; every observation is seam-external (the PCM edge record removed its cooperative `Accounting` counters for the same reason).
- R4 — "Intermediate storage with identical values is invisible to every oracle." Rows 6–7: storage identity and the allocator see it.

---

## 9. Representation-specific observations

- The dyn-participant shape (O2) means the "generic dispatch == 0" claim is scoped: zero *composition-plane* generic dispatch (Context lookup, capability resolution, Reconcile, event fan-out, registry traversal). The direct pre-bound trait calls are the participant invocations that must be > 0.
- The `FlowSlot` handoff (`Rc<RefCell<Option<Flow>>>`) is a test-side seam for extracting the flow out of activation; it is a representation choice, not a contract, and is likely to be revisited when the publication/replacement mechanism validation decides the publication mechanism.
- Block storage ownership (flow-owned) is likewise a representation choice; source-owned storage would change the address-identity observation but not the boundary claims.
- The PCM edge harness's `DestinationTooSmall`/`TrailingScalar`/`ZeroChannelCount` guards still apply at each edge; this experiment exercises them as a reused harness rather than re-earning them.

---

## 10. Rejected assumptions

- That the kernel-op-count zero could be a stuck counter — disproven by the sensitivity `settle()` check (§7).
- That value correctness suffices as an architecture oracle — disproven by the control row (§6 row 0).
- That ```` ```compile_fail ```` doc tests prove rejection — they do not run (PCM edge record, §12); this experiment uses executed `rustc` subprocesses with both a positive and a negative fixture.
- That steady-state allocation-freedom can be claimed by inspection — it is measured, and the per-quantum-copy twin shows the measurement fires.

---

## 11. Open questions

- What exactly G4 (ADR §14) requires as acceptance beyond this experiment's evidence — whether G4 is satisfied by this rung's executable record or expects more (e.g., a real-shaped consumer) is a human gate disposition, not something this document decides. (Post-authoring: ADR §14 records G4 as DELIVERED with this record, PR #96.)
- Whether the participant references should be monomorphized for the eventual realtime path — out of scope here (Issue #12 territory), but the boundary claims do not depend on it.
- Where block storage should live in a production-shaped flow (source-owned vs flow-owned) — deferred to publication/replacement and real-device pressure.

---

## 12. Next pressure

- **Publication/replacement mechanism validation** (the ADR §12 ladder's Phase D) — validate graph publication/replacement mechanisms against P1–P5 with the RT/control evidence list; the single static flow here is the baseline graph that a mechanism would publish, and H1 is the pressure input it must answer.
- **Issue #12** — real decoder / real device pressure may reject representation choices (sample type, layout, shape); conversely it must not retro-design the earned boundary semantics (PCM edge §11 firewall applies unchanged).
- **G4** — this document is the delivered G4 executable evidence (ADR §14, PR #96); the gate's final disposition is a human decision.

---

## 13. Production delta and naming gate

- `git diff origin/main -- 'crates/**/src/**' 'apps/**/src/**'` is **empty** — production `src/` delta is zero; the only non-test change is a `[dev-dependencies]` entry for `qianqian-composition` in `crates/qianqian-audio-api/Cargo.toml` (test-only, with an explicit comment that the kernel keeps its own empty `[dependencies]` firewall).
- Campaign-name gate: `rg` for the campaign identity pattern over `crates apps` shows **zero new hits** vs main — no `Phase C`/`phase_c`/`PHASE-C`/`C0`/`C1`/`A0`/`experiment-1`/issue-number/PR-number strings in filenames, modules, types, functions, test helpers, or comments anywhere in the test tree. (This document's header is the one permitted provenance mention.)
