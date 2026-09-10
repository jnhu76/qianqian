# PCM-CONTRACT-A0 — Minimal PCM Edge Experiment

**Status:** EVIDENCE + PROPOSED CONTRACT  
**Scope:** PLAYBACK-PHASE-B / Issue #92  
**Authority gate:** This document is *not* an ADR and does not add normative facts beyond [`ADR-PBK-001.md`](../adr/ADR-PBK-001.md). It records executable evidence and a proposed minimal contract for downstream review.

---

## 1. What this experiment is trying to earn

The goal is the smallest set of PCM data-edge truths that a future Qianqian playback implementation cannot safely avoid, **without** designing a full player, SRC, DSP, audio runtime, ring buffer, or WASAPI callback.

The artifacts are:

- a synthetic harness (`SyntheticProducer → Candidate PCM Edge → SyntheticConsumer`);
- mutation oracles that turn common PCM bugs into deterministic failures;
- measurements of copies, allocations, and ownership transfers;
- a proposed contract in `MUST / MAY / OPEN / REJECTED` form;
- a decision matrix comparing four candidate edge shapes.

This experiment intentionally avoids the old playback vocabulary (`MusicKernel`, `TransportKernel`, `TrackSession`, `DecodeSession`, `Generation`, `Active`, `Prepared`, `Dual Window`, `Physical Fence`, `PlaybackEnded`). Those names remain historical evidence only.

---

## 2. Issue #12 firewall

**Issue #12 researches SRC/DSP downstream of the PCM edge. Phase B must not pre-select the contract that Issue #12 will consume.**

Phase B earns:

- the irreducible PCM edge semantics (frame vs sample, format metadata, ownership/lifetime, partial acceptance, EOF);
- a measurement framework for copies, allocations, and borrow safety;
- a short list of candidate edge shapes with trade-offs.

Phase B does **not**:

- choose the final decoder→DSP→output wiring;
- freeze SRC requirements;
- commit to a block size, sample type, or channel layout for production.

Issue #12 will read this evidence and decide which candidate survives real SRC/DSP/device pressure.

---

## 3. Claim matrix

| Claim | Classification | Evidence / Authority |
|---|---|---|
| PCM is a direct typed realtime data flow; per-quantum PCM must not re-enter Context/Capability/Reconcile/EventBus/filesystem/network/UI. | `ADR_NORMATIVE` | `ADR-PBK-001.md` §2.4, §6 P1–P5 |
| The repository currently has no implemented PCM producer/consumer contract; `Decoder`, `Processing`, and `AudioOutput` are empty marker traits. | `CURRENT_CODE_FACT` | `crates/qianqian-core/src/ports.rs:14–20` |
| `AudioOutputCapability` binds an empty `dyn AudioOutput` through the Composition Kernel; the kernel owns reachability, not PCM payload. | `CURRENT_CODE_FACT` | `crates/qianqian-runtime/src/lib.rs:26–45` |
| Old playback code in `qianqian-core::music` and `qianqian-core::transport` uses “frames” as temporal accounting units, not PCM sample frames. | `CURRENT_CODE_FACT` | `crates/qianqian-core/src/music.rs`, `crates/qianqian-core/src/transport.rs` |
| A historical `PcmSink.bind(PcmSourceEndpoint)` shape existed but was not implemented as a stable contract. | `HISTORICAL_EVIDENCE` | `docs/architecture/component-boundary-a0.md` |
| Old `specs/playback/*` TLA models are executable evidence for testing techniques, not current architecture authority. | `HISTORICAL_EVIDENCE` | `specs/playback/README.md` |
| Interleaved frame layout is a reasonable first implementation prior for the experiment. | `OPEN` | synthetic harness `pcm_contract_a0.rs` validates it only |
| `f32` per sample is sufficient for the synthetic oracle and for a first implementation. | `OPEN` | no quantization, dither, or device format evidence collected |
| SRC/DSP requirements will determine whether planar layout, multi-rate edges, or non-interleaved FFI shapes are required. | `DOWNSTREAM_RESEARCH` | Issue #12 |
| Borrowed immutable views can enforce use-after-reuse safety at the Rust type level. | `CURRENT_CODE_FACT` | `compile_fail` doc test + unit test in `pcm_contract_a0.rs` |
| A `PcmFormat` carrying `sample_rate` and `channel_count` is the minimum metadata needed to interpret a scalar slice as frames. | `PROPOSED_CONTRACT` | this document §6 |
| Zero-frame blocks must not be interpreted as EOF. | `PROPOSED_CONTRACT` | experiment H, `pcm_contract_a0.rs` |

---

## 4. Problem decomposition (D1–D17)

These dimensions frame the experiment. They do not presuppose a final API.

1. **D1 — Sample vs frame.** A *sample* is one channel at one time point. A *frame* is all channels at one time point. The edge must keep the two counts distinct.
2. **D2 — Scalar count vs frame count.** Buffer length is a scalar count; capacity in frames is `scalars / channel_count`. Confusing them is a memory-safety/ correctness bug.
3. **D3 — Channel-interleaved layout.** For this experiment, frame-major interleaved layout is the first prior: `[L0, R0, L1, R1, …]`.
4. **D4 — Planar layout.** An alternative where each channel is a separate slice. Kept `OPEN`; not validated here.
5. **D5 — Sample type.** `f32` is the first prior. Quantization, integer formats, and dither are `OPEN`.
6. **D6 — Format metadata.** The minimum is `sample_rate` and `channel_count`. Channel maps, bit depth, and packing are `OPEN`.
7. **D7 — Format travel.** Whether format travels with every block or is negotiated once per edge/session is an `OPEN` design choice.
8. **D8 — Buffer ownership.** Candidate edge shapes: borrowed view, borrowed mutable view, owned block, consumer-provided destination.
9. **D9 — Mutability on the edge.** Mutable borrows enable in-place processing but prevent sharing; immutability enables zero-copy fan-out.
10. **D10 — Push vs pull.** Push: producer decides block size and offers a view. Pull: consumer provides a destination and the producer fills up to capacity.
11. **D11 — Partial acceptance.** Producer offers N frames; consumer may accept fewer. The contract must report accepted vs remaining, not silently drop frames.
12. **D12 — Zero-frame block.** A block with `frames == 0` is *not* a terminal signal. EOF must be explicit and out-of-band.
13. **D13 — Use-after-reuse.** A consumer must not retain a borrowed view after the producer reuses the backing buffer.
14. **D14 — Hidden copies.** Copies across the edge must be explicit and accountable, not accidental performance losses.
15. **D15 — Steady-state allocation.** Per-quantum allocation on a realtime path is a measurable cost and is rejected for hot paths.
16. **D16 — Format discontinuity.** A format change requires an explicit edge/session boundary; silent format change is rejected.
17. **D17 — Plane separation.** Realtime PCM never travels through generic Context/Capability/Reconcile/EventBus per quantum (already normative in ADR-PBK-001).

---

## 5. Synthetic harness

Location: `crates/qianqian-core/src/pcm_contract_a0.rs`  
Exposed as: `qianqian_core::pcm_contract_a0` (experimental, not stable API).

Core types:

```text
Sample              = f32
PcmFormat           = { sample_rate: u32, channel_count: u16 }
PcmView<'a>         = borrowed immutable interleaved view
PcmViewMut<'a>      = borrowed mutable interleaved view
PcmBlock            = owned interleaved block
PullProducer        = trait for consumer-provided destination
SyntheticProducer   = deterministic oracle generator
SyntheticConsumer   = deterministic oracle verifier
Accounting          = explicit copy / allocation / transfer / borrow counters
```

The deterministic sample oracle is:

```rust
fn deterministic_sample(frame: usize, channel: usize) -> f32
```

It produces a stable, non-trivial value per `(frame, channel)`, making channel-order bugs and frame/sample-count confusion trivially observable.

Run the experiments:

```bash
cargo test -p qianqian-core pcm_contract_a0
```

Run the full workspace gate:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

All four commands pass on the branch tip.

---

## 6. Experiments A–H

| Experiment | What it validates | Key result |
|---|---|---|
| **A** — Frame correctness | Mono, stereo, and 3-channel deterministic ordering match the oracle. | `stereo_frame_ordering_matches_oracle`, `mono_frame_ordering_matches_oracle`, `three_channel_frame_ordering_matches_oracle` pass. |
| **B** — Block sizes | Arbitrary block sizes (`1`, `3`, `17`, `256`) all deliver the full stream. | Block size is a scheduling concern, not a contract invariant. |
| **C** — Lifetime / reuse | A borrowed view cannot outlive the producer’s buffer reuse. | `compile_fail` doc test and runtime borrow test pass. |
| **D** — Copy accounting | C1 baseline performs zero copies; a hidden-copy adapter is counted. | `c1_baseline_has_zero_copy`, `hidden_copy_is_counted` pass. |
| **E** — Allocation accounting | C1 has zero steady-state allocations; C3 has one allocation per block. | `c1_baseline_has_zero_steady_allocation`, `c3_owned_has_steady_allocation` pass. |
| **F** — Format discontinuity | Silent format change is rejected; explicit boundary works. | `silent_format_change_is_detected`, `explicit_format_boundary_works` pass. |
| **G** — Flow control | Pull respects consumer capacity; push accepts exact offered block. | `c4_pull_respects_consumer_capacity`, `c1_push_partial_block_exact_size` pass. |
| **H** — EOF negative control | A zero-frame block does not end the stream; terminal signal is out-of-band. | `zero_frame_block_is_not_eof`, `explicit_terminal_signal_is_required` pass. |

---

## 7. Mutation oracles M1–M8

Each oracle models a bug class that the contract must detect or prevent.

| Oracle | Bug class | Detection mechanism |
|---|---|---|
| **M1** — frame count treated as scalar count | Buffer sized for `N` frames ( stereo → `2N` scalars) is asked to hold `2N` frames. | Capacity check returns `Err` before out-of-bounds write. |
| **M2** — channel swap | Consumer reads channel 0 but interprets it as channel 1. | Deterministic oracle mismatch. |
| **M3** — use-after-reuse | Consumer retains a borrowed view after the producer reuses the buffer. | Rust borrow checker rejects at compile time (`compile_fail` doc test). |
| **M4** — hidden copy | Adapter silently copies borrowed data before handing it on. | `Accounting.memcpy_count` and `bytes_moved` surface the copy. |
| **M5** — steady-state allocation | Per-block heap allocation on the hot path. | `Accounting.steady_allocations` is non-zero. |
| **M6** — silent format change | Producer switches `sample_rate` or `channel_count` mid-edge. | Edge format check rejects the block. |
| **M7** — zero-frame block as EOF | Empty block interpreted as end-of-stream. | Stream continues after the empty block. |
| **M8** — partial acceptance drops frames | Producer offers 5 frames, consumer capacity is 3, remaining 2 vanish. | Explicit `accepted` / `remaining` accounting; second block completes the stream. |

---

## 8. Candidate decision matrix

| Dimension | **C1** Borrowed immutable view | **C2** Borrowed mutable view | **C3** Owned block transfer | **C4** Consumer-provided destination (pull) |
|---|---|---|---|---|
| Copies | 0 | 0 | 0 (one ownership transfer) | 0 |
| Steady allocations | 0 | 0 | 1 per block | 0 |
| Producer/consumer lifetime coupling | Yes — consumer must finish before producer reuses buffer | Yes, and consumer must not retain mutable borrow | No — consumer owns block independently | No — consumer owns destination |
| Direction | Push | Push | Push | Pull |
| Best fit | Decoder → processing / output when prompt consumption is guaranteed | In-place DSP / effects | Cross-thread handoff, producer/consumer decoupling | Device callbacks, consumer-driven pacing |
| Risk | Consumer retention → use-after-reuse | Mutability prevents sharing; harder to fan out | Allocation cost on realtime path | Producer must not overfill destination |

**Proposed Phase B recommendation:**

- **Default edge shape:** C1 borrowed immutable view, because it gives zero-copy, zero-allocation, statically enforced use-after-reuse prevention.
- **Escape hatches:** C2 for in-place DSP, C3 for decoupled cross-thread handoff, C4 for device pull callbacks.

No candidate is declared the final production contract. Issue #12 will re-evaluate under SRC/DSP/device pressure.

---

## 9. Earned contract (PROPOSED, not normative)

### MUST

| # | Rule | Evidence |
|---|---|---|
| M1 | A PCM edge must distinguish **frame count** from **scalar count**. | `mutation_m1_frame_count_as_scalar_count` returns `Err` when the two are confused. |
| M2 | A PCM edge must preserve per-channel sample order within an interleaved frame. | `mutation_m2_swap_channels` mismatch against the channel-1 oracle. |
| M3 | A borrowed PCM view must prevent the consumer from retaining it after the producer reuses the backing buffer. | `compile_fail` doc test + `borrowed_view_lifetime_prevents_reuse_while_held`. |
| M4 | Any copy of PCM data across the edge must be explicit and accountable. | `hidden_copy_is_counted` shows `memcpy_count` / `bytes_moved`. |
| M5 | A hot-path PCM edge must not perform steady-state allocations. | `c1_baseline_has_zero_steady_allocation`. |
| M6 | Format metadata on a block must match the edge/session format; a format change requires an explicit boundary. | `silent_format_change_is_detected`, `explicit_format_boundary_works`. |
| M7 | A zero-frame block must not be interpreted as EOF. | `zero_frame_block_is_not_eof`. |
| M8 | Partial acceptance must report accepted and remaining frame counts; frames must not be silently dropped. | `m8_partial_acceptance_does_not_drop_frames`. |
| M9 | Per-quantum PCM must not travel through generic Context/Capability/Reconcile/EventBus/filesystem/network/UI. | `ADR-PBK-001.md` §2.4, §6 (normative, not re-earned here). |
| M10 | A PCM edge must carry enough format metadata to interpret scalars as frames (at minimum `sample_rate` and `channel_count`). | `PcmFormat` and all consumer oracles rely on it. |

### MAY

- The first implementation may use **channel-interleaved** layout.
- The first implementation may use **`f32` samples**.
- The first implementation may use **C1 borrowed immutable views** as the default zero-copy edge.
- A consumer-provided destination (**C4 pull**) may be used when the consumer drives pacing.
- An owned block (**C3**) may be used when producer and consumer lifetimes must be decoupled.

### OPEN

- Planar vs interleaved vs other memory layouts.
- Integer sample formats, bit depth, packing, and dither.
- Channel maps / semantic channel layouts beyond `channel_count`.
- Multi-rate edges and SRC placement.
- Block-size negotiation and realtime deadline constraints.
- FFI-safe representation for cross-language runtimes.
- Device clock / sample-rate synchronization.

### REJECTED

- Treating scalar count as frame count.
- Silent format change on the same edge/session.
- Zero-frame block as EOF.
- Per-quantum PCM flowing through Context/EventBus/generic plugin dispatch.
- Unaccounted copies or allocations on the realtime hot path.

---

## 10. ADR impact gate

**ADR impact: NONE.**

`ADR-PBK-001.md` already contains the normative plane separation and realtime publication/reclamation contract (§1–§2, §6, §12). PCM-CONTRACT-A0 adds no new normative semantic fact; it provides executable evidence and a proposed concrete edge shape. If Issue #12 later demonstrates that a new normative fact is required (for example, a mandatory sample format or a realtime reclamation shape), that will be proposed as a separate ADR amendment with its own review.

---

## 11. Phase C handoff

Phase C (Issue #12 / SRC-DSP downstream) should consume this evidence and answer:

1. Which candidate (C1–C4, or a hybrid) survives real SRC/DSP/device constraints?
2. Does planar layout or a non-interleaved FFI representation become a MUST?
3. What sample formats and channel maps are required for production?
4. Where does block-size negotiation and realtime deadline accounting live?
5. Is a new ADR amendment required to freeze the chosen edge shape?

Until those questions are answered, this contract remains **proposed and experimental**.
