# PCM-CONTRACT-A0 — Minimal PCM Edge Experiment

**Status:** EVIDENCE + PROPOSED CONTRACT — **NOT NORMATIVE AUTHORITY**
**Scope:** Issue #92 / ADR-PBK-001 §12 Phase B (minimal PCM contract)
**Authority gate:** This document is *not* an ADR and adds no normative fact beyond [`ADR-PBK-001.md`](../adr/ADR-PBK-001.md). It records executable evidence and a proposed minimal contract. If any conclusion here ever earns normative status, it must go through human-reviewed ADR clarification, not this file.
**This revision:** corrected after an adversarial audit of the first draft (see §12); the harness was moved out of the production API and every over-claimed MUST was re-minimized or downgraded.

---

## 1. What this experiment is trying to earn

The smallest set of PCM data-edge truths that a future Qianqian playback implementation cannot safely avoid — **without** designing a player, SRC, DSP, audio runtime, ring buffer, or WASAPI callback.

The artifacts are:

- a synthetic harness (`SyntheticProducer → candidate PCM edge → SyntheticConsumer`), test-only;
- adversarial twins (deliberate bug implementations) with oracles that kill them;
- *measured* allocation evidence (thread-scoped counting allocator) and *structural* copy evidence (storage pointer identity);
- a proposed minimal contract in `MUST / MAY / OPEN / REJECTED` form.

This experiment does not use the old playback vocabulary (`MusicKernel`, `TransportKernel`, `TrackSession`, `DecodeSession`, `Generation`, `Active`, `Prepared`, `Dual Window`, `Physical Fence`, `PlaybackEnded`); those names remain historical evidence only.

### Evidence classes used throughout

```text
MEASURED                  observed by real instrumentation (counting allocator)
EXECUTABLE ORACLE          a test executes a buggy twin and the oracle fires on real state
TYPE-SYSTEM EVIDENCE       enforced by rustc; the rejection itself is executed by a test
STRUCTURAL CODE EVIDENCE   observed from structure (e.g. storage pointer identity)
REASONED / NOT EXECUTED    argument only; no executable backing
ADR NORMATIVE INPUT        already normative in ADR-PBK-001; listed, not re-earned
CURRENT CODE FACT          statement about today's repository, checkable by inspection
```

---

## 2. Position on the research ladder

The one normative ladder is ADR-PBK-001 §12:

```text
Phase A  composition runtime reality                 (done: K0)
Phase B  minimal PCM contract                        (this experiment)
Phase C  direct-flow graph                           (next)
Phase D  graph publication/replacement mechanism     (validation against §6 P1–P5)
Phase E  real decoder / real output
Phase F  playback semantics
```

**Phase C is its own rung, not Issue #12.** Per ADR §12, Phase C proves `Source → one processing stage → Sink` with the hot path outside Context/Reconcile/EventBus, using the semantics Phase B earned. Issue #12 (SRC/DSP/gain/EQ/limiter/libswresample/SoXR/device conversion/worker-vs-callback/SIMD) remains downstream research that consumes — but must not retro-design — this contract. Phase D validates publication/reclamation mechanisms against the already-frozen P1–P5; it does not belong to this PR.

---

## 3. Harness

Test-only; no `qianqian-core` library API is added or changed.

```text
crates/qianqian-core/tests/pcm_edge_contract/
    main.rs             instrumentation + scenario tests + compile-fail runner
    harness.rs          types + deterministic sample oracle (fail-closed construction)
    candidates.rs       four candidate edge shapes (semantic names)
    mutations.rs        adversarial twins + kill tests
    compile_fail/       rustc fixtures: positive + negative borrow-lifetime controls
```

Run:

```bash
cargo test -p qianqian-core --test pcm_edge_contract
```

Key types (`harness.rs`):

```text
Sample              = f32                      (first-implementation prior, not a MUST)
PcmFormat           = { sample_rate, channel_count }  (zero channels unconstructable)
PcmView<'a>         borrowed read-only interleaved view   (checked construction + access)
PcmViewMut<'a>      borrowed mutable interleaved span      (checked access, freeze())
PcmBlock            owned interleaved block
FillDestination     pull seam: producer fills consumer-owned destination
SyntheticProducer   deterministic oracle generator
SyntheticConsumer   deterministic oracle verifier (format identity + per-sample check)
```

Instrumentation (`main.rs`): a thread-scoped counting `#[global_allocator]` (test binary only) measures allocations performed by the measured closure's own thread; foreign-thread allocations (harness coordination, test dispatch) are excluded, which keeps the measurement deterministic under parallel test execution.

### Candidate shapes (`candidates.rs`)

| semantic name (code)      | legacy label | shape                                          |
|---------------------------|--------------|------------------------------------------------|
| `BorrowedReadOnlyFlow`    | C1           | producer lends a read-only view per block (push) |
| `BorrowedInPlaceFlow`     | C2           | producer lends a mutable span (push)            |
| `OwnedTransferFlow`       | C3           | block ownership moves to the consumer (push)    |
| `ConsumerFilledFlow`      | C4           | consumer owns destination (pull)                |

The legacy C1–C4 / M1–M8 labels are provenance for this record only; the code and any future vocabulary use the semantic names. No candidate is "the default architecture"; they are experiment comparisons.

---

## 4. Mutation-kill matrix

Every row is an executed test. Rows marked **KILL** are adversarial twins: an honest baseline passes, then a deliberately buggy implementation runs for real and the named oracle fires on the twin's actual execution state. Rows marked **PIN** are honest-path boundary pins: they pin a checked boundary with executed assertions (valid corners resolve, invalid ones are rejected) but contain no active twin. Test names are from `mutations.rs` / `main.rs`.

| # | kind | bug class (twin) / pinned boundary | baseline | mutated behavior / pinned edge | oracle | executed? | killed / held? |
|---|------|------------------------------------|----------|--------------------------------|--------|-----------|---------|
| 1 | KILL | frame count read as scalar count (legacy M1) | 4 frames into 4-frame buffer OK | requests 8 frames out of 8 scalars | capacity check → `DestinationTooSmall` | YES | YES |
| 2 | KILL | trailing partial frame payload | 8 stereo scalars → 4 frames OK | 5 stereo scalars offered as payload | checked construction → `TrailingScalar` | YES | YES |
| 3 | KILL | zero-channel format | 2 channels OK | `PcmFormat::new(_, 0)` | construction rejection → `ZeroChannelCount` | YES | YES |
| 4 | KILL | channel order corrupted in transit (legacy M2) | unmodified payload verifies | channels 0/1 physically swapped in every frame | per-sample value oracle fires at the first swapped sample | YES | YES (plus sensitivity precondition test: oracle values differ per channel) |
| 5 | PIN | out-of-range channel/frame access | last frame/last channel resolve | stereo `sample(0, ch=2)`, `sample(4, 0)`, `usize::MAX` | checked access returns `None`, never another sample | YES | held (aliasing itself is unexecutable through the checked API) |
| 6 | KILL | in-place modification unobserved | idempotent write verifies | +0.5 written at (frame 2, ch 1) through span | verifier mismatch at exactly (2, 1) | YES | YES |
| 7 | KILL | retained borrow past storage reuse (legacy M3) | sequential borrow + reuse compiles (positive fixture) | view retained while storage reused | `rustc` compile failure (E0499 family) — executed as a subprocess test | YES | YES |
| 8 | KILL | hidden intermediate storage copy (legacy M4) | producer/consumer storage addresses equal | per-block `to_vec` before handover (values still verify) | storage pointer identity differs **and** allocation count > 0 | YES | YES (both oracles; value oracle alone is demonstrably blind) |
| 9 | KILL | per-transfer allocation (legacy M5) | reused-buffer flows measure 0 allocations | owned shape allocates per block (by design) | thread-scoped counting allocator measures ≥ 1/block | YES | detection YES (cost is legal for that shape) |
| 10 | KILL | silent format change mid-edge (legacy M6) | explicit new edge with new format legal | producer switches 44.1k→48k on same edge (rate-only change; sample values alone would verify) | format-identity check → `FormatMismatch` | YES | YES |
| 11 | KILL | zero-frame payload read as EOF (legacy M7) | empty payload passes through, stream continues | driver stops at first empty payload (delivers 4 of 8) | conservation check on real cursor counts → `FramesLost {8, 4}` | YES | YES |
| 12 | KILL | partial acceptance drops remainder (legacy M8) | explicit remainder re-offered conserves | accept 3 of 5, discard 2 unreported | conservation check on real cursor counts → `FramesLost {5, 3}` | YES | YES |
| 13 | PIN | pull destination capacity slack | whole frames fill exactly | 7-scalar stereo destination | whole-frame fill (3 frames), slack sample slot untouched (sentinel) | YES | held |

Rows 11–12 note: the conservation oracle is a driver-side check computing `produced vs consumed` from the twin's real producer/consumer cursor counts — the inputs are measured execution state, the `FramesLost` classification is test-side bookkeeping, stated plainly here so the oracle's weight is not overstated.

### Compile-fail evidence details (row 7)

The first draft placed a ` ```compile_fail ` doc test inside a `#[cfg(test)]` module; rustdoc never collects those, so CI showed `Doc-tests qianqian_core: running 0 tests` — the proof did not run. Replaced with:

- `compile_fail_check::positive_control_sequential_borrow_compiles` — compiles `compile_fail/valid_borrow_use.rs` with a real `rustc` subprocess and asserts success;
- `compile_fail_check::negative_control_retained_borrow_fails_to_compile` — compiles `compile_fail/retained_borrow_past_storage_reuse.rs` and asserts failure with an E0499/E0502/E0505/E0597 rejection.

Sensitivity check (one-off manual verification, recorded not committed): with a harness copy whose `lend_read_only` signature is weakened to return a storage-decoupled `PcmView<'static>` (copy + leak), the same negative fixture **compiles** — i.e. the oracle flips exactly when the lifetime constraint is weakened. Claims below are therefore about the tested borrowed candidate's type-level enforcement, not about all possible PCM representations.

---

## 5. Copy and allocation evidence — what each number actually is

**Copy (rows 6–8):** storage pointer identity (`producer storage address == consumer-observed address`) is STRUCTURAL CODE EVIDENCE that the reference borrowed/in-place/pull paths insert no intermediate storage buffer. Scope limits, stated plainly: it says nothing about CPU/cache-level data movement, and a faithful copy with matching values is invisible to the value oracle — only pointer identity (and, for copies that allocate, the allocator) can see it.

**Allocation (row 9):** steady-state loop allocation counts are MEASURED by the thread-scoped counting allocator:

```text
BorrowedReadOnlyFlow  steady state (64 frames / 8-frame blocks): 0 allocations
BorrowedInPlaceFlow   steady state:                                0 allocations
ConsumerFilledFlow    steady state:                                0 allocations
OwnedTransferFlow     steady state:                                ≥ 1 per block (by design)
```

These are facts about the tested implementations, not claims that "the edge never allocates" in general — the owned shape is legal and pays a measured per-block allocation.

**Removed:** the first draft's cooperative `Accounting` counters (self-incremented `memcpy_count` / `steady_allocations` / …). Code that merely reports its own honesty proves nothing; those numbers were not measurements.

---

## 6. Earned contract (PROPOSED, not normative)

Each MUST survives the minimality question: *if we switch to planar layout, owned transfer, device pull, FFI, SRC, or DSP tomorrow, does this still have to hold?*

### MUST

| # | rule | evidence | class |
|---|------|----------|-------|
| 1 | **Whole-frame integrity.** A payload whose scalar count is not a whole number of frames must not be silently truncated or reinterpreted; zero channels are undefined and rejected at construction. | rows 1–3, 13 | EXECUTABLE ORACLE |
| 2 | **Unambiguous addressing.** An out-of-range frame or channel must never resolve to another sample (no cross-frame aliasing). | rows 4–5 | EXECUTABLE ORACLE |
| 3 | **Storage lifetime.** No consumer may dereference PCM storage after that storage's valid lifetime/reuse/reclamation boundary. | row 7 (borrowed candidate); semantic requirement independent of mechanism | TYPE-SYSTEM EVIDENCE (for the tested candidate) |
| 4 | **Format stability.** PCM interpretation must not silently change while data is still flowing under the previous format context. | row 10 | EXECUTABLE ORACLE |
| 5 | **Frame conservation.** Delivery semantics must never silently lose frames that were not consumed. | rows 11–12 | EXECUTABLE ORACLE |
| 6 | **Terminal disambiguation.** Payload shape alone must not ambiguously encode terminal state; in the tested candidates a zero-frame payload is ordinary data and terminal state is out-of-band. | row 11 | EXECUTABLE ORACLE (candidate-scoped) |
| 7 | Per-quantum PCM does not traverse the generic composition plane — per ADR §2.4's frozen list this excludes Context lookup, capability resolution, Fiber Reconcile, generic event fan-out, plugin registry traversal, filesystem/network/UI, and unbounded blocking/allocation (full list authoritative in the ADR). | ADR-PBK-001 §2.4, §6 | ADR NORMATIVE INPUT — listed, not re-earned |

### MAY

- Interleaved frame layout (tested prior only).
- `f32` samples (sufficient for this synthetic harness; nothing here proves production must be f32).
- Borrowed read-only views as a first-implementation shape *for the synthetic context*.
- Owned transfer / consumer-filled pull as alternative shapes with their measured trade-offs.
- Explicit new edge/session (or another proven mechanism) as a legal form of format change.
- Out-of-band terminal signaling.

### OPEN

- Layout: planar vs interleaved vs other; FFI-safe representation.
- Sample representation: integer formats, bit depth, packing, dither.
- Minimal metadata field set: `sample_rate` + `channel_count` is what *this harness* used; it is **not** proven to be the universal minimum (channel count drives scalar→frame grouping; sample rate does not; channel maps/interpretation are unresolved).
- Whether format travels per block or is established per edge/session.
- Flow-control mechanism: partial acceptance with explicit remainder, all-or-nothing with retry, would-block, and pull capacity are all legal models (rows 11–12 baselines) — conservation is the requirement, the mechanism is open.
- Terminal-state representation (out-of-band *how* is unresolved).
- Block size, deadlines, and realtime budgeting (Phase C+).
- sample_rate validation rules (in this harness the rate participates in format identity — the row-10 twin swaps rate only — but no timing semantics are interpreted, so beyond identity no rate rule is earned here).

### REJECTED (in the tested candidates)

- Silent truncation of a trailing partial frame.
- Out-of-range channel resolving to a neighbouring frame's sample.
- Silent format change on a live edge.
- Silent frame loss under any acceptance model.
- Zero-frame payload overloaded as EOF.
- Cooperative self-reported counters as "measurement".

---

## 7. Representation vs semantics

| semantic truths (representation-independent) | first-implementation priors (this experiment) | candidate-only representations |
|---|---|---|
| whole frames; unambiguous addressing; storage lifetime; format stability; frame conservation; terminal disambiguation | interleaved layout; f32; block-loop push with reused buffer; per-channel deterministic value oracle | `PcmView`/`PcmViewMut`/`PcmBlock`/`FillDestination` shapes (C1–C4) |

---

## 8. Candidate trade-offs (observed)

| dimension | borrowed read-only (C1) | borrowed in-place (C2) | owned transfer (C3) | consumer-filled pull (C4) |
|---|---|---|---|---|
| intermediate storage | none (pointer identity) | none | fresh per block | none |
| steady-state allocations (measured) | 0 | 0 | ≥ 1 per block | 0 |
| lifetime coupling | consumer finishes before reuse (compiler-enforced in the tested shape) | same, mutable | decoupled | decoupled (consumer owns dest) |
| direction | push | push | push | pull |
| observed risk | retention blocked by borrowck — cross-thread handoff needs a different shape | mutability prevents read fan-out | per-block allocation cost | producer must respect destination capacity |

No recommendation beyond "these are the trade-offs Phase C should run against"; the first draft's "C1 as default edge" is withdrawn as unfounded.

---

## 9. ADR impact gate

**ADR impact: NONE.** ADR-PBK-001 already carries the plane separation (§1–§2), the realtime publication/reclamation contract P1–P5 (§6), and the ladder (§12). This experiment adds executable evidence and candidate trade-offs only. If downstream phases believe a conclusion here deserves normative status, the path is a human-reviewed ADR clarification — not an automatic amendment, and not this file.

---

## 10. Phase C handoff

Phase C (ADR §12, its own rung — **not Issue #12**) should build the smallest executable direct flow:

```text
real-shaped producer → earned PCM edge semantics → real-shaped consumer
```

inside one processing stage, hot path outside Context/Reconcile/EventBus, to validate a real direct-flow boundary instead of synthetic-only candidate comparison. Phase C does **not** take on SRC/DSP/EQ/limiter/device policy/ring-buffer selection (Issue #12, downstream) and does **not** start publication-mechanism comparison (Phase D, validates against P1–P5).

---

## 11. Issue #12 firewall

Issue #12 (SRC/DSP research) consumes this evidence under real decoder/device pressure and may reject any prior here (layout, sample type, shape). Phase B deliberately did not pre-select what Issue #12 will need; conversely, Issue #12 pressure must not retro-design the earned minimal semantics above — if a "MUST" here fails downstream, that is new evidence requiring a new record.

---

## 12. Corrections vs the first draft (audit record)

The first draft of this PR was audited adversarially; this revision corrects:

1. **Production API contamination:** the harness lived at `qianqian-core/src/pcm_contract_a0.rs` behind `pub mod` — a real library API surface regardless of "experimental" comments. Moved to `tests/pcm_edge_contract/` (production src delta vs `main` is zero).
2. **compile_fail that never ran:** a doc-comment `compile_fail` inside `#[cfg(test)]` is not collected by rustdoc (CI: "running 0 tests"). Replaced with executed `rustc`-subprocess fixtures plus positive control and a recorded sensitivity check.
3. **Silent truncation:** `scalars / channel_count` discarded trailing partial scalars while comments claimed whole frames. Construction is now fail-closed (`TrailingScalar`), and zero-channel formats are unconstructable.
4. **Cross-frame aliasing:** `index = frame * channels + channel` without a channel bound let stereo channel 2 read frame 1 channel 0 while docs claimed a panic. Access is now checked (`Option`), and the adversarial tests pin the boundary.
5. **Cooperative accounting presented as measurement:** self-incremented copy/allocation counters replaced by a thread-scoped counting allocator (MEASURED) and storage pointer identity (STRUCTURAL).
6. **Over-claimed MUSTs downgraded:** interleaved order → MAY prior; borrowed view → one mechanism under a semantic lifetime rule; per-block format metadata → OPEN; partial acceptance → one legal model under a conservation MUST; `sample_rate + channel_count` as universal minimum → OPEN; zero-frame-EOF rejection → candidate-scoped; "hot path must not allocate" → measured property of reused-buffer candidates, not a universal rule.
7. **Phase C handoff corrected:** was written as "Phase C (Issue #12 / SRC-DSP downstream)"; ADR §12 Phase C is the direct-flow rung. Issue #12 remains downstream.
8. **Monolith split by role:** one 984-line production module mixing candidates, generator, counters, mutations and tests → four test-only modules with single responsibilities.
