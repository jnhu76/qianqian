# TYPE-SYSTEM-GUARANTEES — Phase B0 representation audit

> STATUS: EVIDENCE (campaign QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B0).
> Not an authority. Audits the *current* Rust reality at CAMPAIGN_BASE_SHA
> `44e9ed0b26d031a7985d67d179600fdce3af8117` and decides, per property,
> whether an expensive verifier must re-earn it.
>
> Scope of "RUST-YES": the property is discharged by the compiler /
> ownership / visibility / representation alone, for every legal program.
> "RUST-PARTIAL": the representation carries part of the property; a
> control-flow or semantic invariant remains. "RUST-NO": pure runtime
> invariant. The TARGET TOOL column names the cheapest layer that still
> needs to challenge the property; tools *above* that layer need not
> duplicate it.

Sources read: `crates/qianqian-composition/src/{kernel,fiber,capability,component,context,desired,diagnostic,lib}.rs`,
`crates/qianqian-playback/src/{edge,completion,session,lib}.rs`,
`crates/qianqian-audio-api/src/ports.rs`, unsafe/FFI inventory over all crates.

## A. Composition kernel (qianqian-composition)

| # | PROPERTY | GUARANTEED BY RUST TYPE SYSTEM? | RUNTIME CHECK STILL NEEDED? | TARGET TOOL |
|---|----------|--------------------------------|-----------------------------|-------------|
| A1 | One `EffectPayload::Inverse` closure value is invoked at most once | **YES** — `Box<dyn FnOnce>` is consumed by the call; `run_unwind`/`dispose_effect` take the payload out via `mem::replace(.., Violated)` before invoking, so a second invocation would have to read the `Violated` tombstone, which is `unreachable!` (panic, not silent double-call) | The closure value ≠ the semantic obligation: "one EffectRecord is globally discharged exactly once across all paths (clean unload, partial unwind, dispose, retire)" is control flow, not ownership | Kani K3 |
| A2 | A `Violated` tombstone is never unwound or invoked again | **PARTIAL** — representation: the FnOnce is gone once consumed; the `Violated` arm panics (`unreachable!`) | That the tombstone *stays in the accumulator* (provenance/authority visible, removal blocked) under every legal history is a kernel invariant | Kani K3 (tombstone scenario) |
| A3 | Stale `FiberId { idx, generation }` cannot resolve to a reused slot | **PARTIAL** — representation: generation stored per slot, compared in `fiber_opt`/`fiber_mut` (`f.id == fid`); `FiberId` fields are `pub(crate)` — no external forgery | Removal bumps generation (`wrapping_add`) and resolution honors it only if every path checks; that is a runtime invariant over all interleavings of remove/reuse | Kani K1 |
| A4 | `FiberId` generation ABA after 2^32 removals in one slot | **NO** (`wrapping_add`) | Accepted bounded-representation limit, documented here; not reachable within any realistic process lifetime; bounded verifiers cannot reach it either | Documented bound (no tool) |
| A5 | Serialized control plane (kernel operations never race) | **YES** — `CompositionKernel` contains `Cell<u64>` and `Rc<_>` ⇒ auto `!Send + !Sync`; sharing across threads does not compile and no `unsafe impl` exists in the crate | Nothing for a verifier; concurrency enters only *below* the kernel (playback legs) | None (compiler); Loom targets the playback slice instead |
| A6 | Required-single at new resolution (`activation_ready` exactly-one) | **NO** | Pointwise single-source + exactly-one-active-provider over all legal histories | Kani K6 (+ existing TLA+); unit tests for happy path |
| A7 | `relied_on` removal guard (committed consumer blocks provider unload) | **NO** | All legal step sequences | Kani K2 (+ existing TLA+) |
| A8 | Mount-time capability-overlap withholding (#126 corrective) | **NO** | All legal step sequences incl. violation-latched replacements | Kani K6 |
| A9 | Removal discipline (removed fiber owes nothing: empty effects, no committed view, no violation) | **PARTIAL** — the candidate predicate is a conjunction of observable fields; the *ordering* that guarantees a fiber can only reach removal through that predicate over every legal history is runtime | Every step of every legal history | Kani K4 |
| A10 | `is_quiet()` truth (quiet ⇒ `step()` would settle; no owed work, no violation) | **NO** — `has_pending_work` restates the five candidate predicates; the two codings can drift | Consistency between `quiet_now` and the transition selectors over all states | Kani K5 |
| A11 | LIFO unwind order of the owned-effect accumulator | **PARTIAL** — representation is a `Vec` stack; pop-from-end discipline is control flow | Order observable under unwind/dispose in all paths | Kani K3 (covers via ordered inverse recording) |
| A12 | Episode close empties accumulator; effects cannot fire after their episode | **NO** | Unwind runs to completion on clean paths; violated latch keeps the fiber in `Unloading` forever (no exit while latched) | Kani K3/K4 |
| A13 | Capability identity is the `TypeId`; NAME is vocabulary only | **YES** at resolution time (`CapabilityKey::of::<K>`, equality on `id`); duplicate-NAME registration refusal is a runtime registration check, already unit-tested | None beyond existing unit tests | Unit tests (existing) |
| A14 | Typed service storage: `unerase_service::<K>` downcast cannot fail | **PARTIAL** — guaranteed *constructionally* (provisions only inserted by `provide::<K>`, same `K` ⇒ same `TypeId`); code keeps a defensive `Unresolved` fallback | Constructional argument + existing tests suffice; no verifier | Unit tests (existing) |
| A15 | `Revision` raw/fresh token domains are disjoint; kernel compares, never derives | **PARTIAL** — newtype with no arithmetic; disjointness enforced by constructor `assert` (runtime, not type-level); `fresh()` counter monotone via `AtomicU64`, no wrap into raw domain below 2^63 fresh calls | Counter-wrap bound documented (same class as A4); construction asserted | Unit tests + documented bound |
| A16 | Plan refusal keeps previous desired intact (validate before swap) | **NO** (algorithmic) but deterministic and entry-bounded | Property tests / unit tests are sufficient — no interleaving risk (single-threaded, no partial mutation on error) | Property/unit tests |
| A17 | Plan validation terminates; cycle DFS reports back edges | **NO** | Recursion depth bounded by entry count (stack, not correctness); existing unit tests | Unit tests |

## B. Playback concurrent surface (qianqian-playback)

| # | PROPERTY | GUARANTEED BY RUST TYPE SYSTEM? | RUNTIME CHECK STILL NEEDED? | TARGET TOOL |
|---|----------|--------------------------------|-----------------------------|-------------|
| B1 | Ring-buffer indices never over/underrun; partial-frame handling | **PARTIAL** — all ring state under one `Mutex<EdgeState>` (mutual exclusion by `std`); the arithmetic invariants themselves are code | All interleavings of write/read/terminal that respect the mutex | Loom L-series + property tests |
| B2 | Terminal monotonicity: first terminal wins (EOF not downgraded by late stop; failure not downgraded) | **NO** | Races between `close_eof` / `fail` / `stop` | Loom (stop × EOF × failure) |
| B3 | Stop/failure/EOF unblock both endpoints (no wedged reader/writer) | **NO** | Every interleaving incl. blocked-in-`wait` at terminal-set time | Loom L4 |
| B4 | `SessionCompletion` outcome resolved exactly once; decode failure authoritative over stop/drain | **NO** | Races between `decode_failed` / `worker_exited` / drain verdict / `stop` | Loom L2 + unit tests |
| B5 | `DrainSignal` publishes exactly one verdict, first-wins | **PARTIAL** — mutex + `Option` first-wins; semantic "exactly one mechanism fact" is runtime | Trivial; covered by unit tests | Unit tests (existing) |
| B6 | Disposal ordering stop → join → release (session inverse registration) | **PARTIAL** — kernel LIFO (A11) guarantees the order given the registration order in `session.rs`; the *claim* that the order is correct lives in the session's activation contract | Kernel-level: Kani K3; system-level: session/edge lifecycle tests + B4 stress | Kani K3 + system stress |
| B7 | Decode worker panics cannot escape or wedge the data plane | **NO** (`catch_unwind` + `edge.fail()` control flow) | Worker body panic path | Unit tests (existing, session_activation) |
| B8 | `RenderPcmInput: Send + Sync`, `DecodedPcmStream: Send` (not Sync) — legs movable, endpoints externally serialized | **YES** — trait bounds; violating implementations rejected at compile time | None for the trait contract; provider-side truth is FFI territory (D1) | None (compiler); FFI: B3 Miri/native |

## C. FFI / unsafe (qianqian-decode-songcore, qianqian-output-wasapi)

| # | PROPERTY | GUARANTEED BY RUST TYPE SYSTEM? | RUNTIME CHECK STILL NEEDED? | TARGET TOOL |
|---|----------|--------------------------------|-----------------------------|-------------|
| C1 | `file_read`/`file_seek`/`file_size` callbacks: raw `ud` → `&mut File`, `from_raw_parts_mut` on the C-provided buffer | **NO** — safety invariant (ud validity, buffer size ≥ requested, lifetime) owned by the native contract, restated in crate docs | Yes, but the native library cannot run under Miri | NOT VERIFIED BY MIRI; native integration gate (songcore fixture loop) + future ASan/UBSan on the FFI harness |
| C2 | `unsafe impl Send for SongcoreDecodeStream` | **NO** — manual promise that the native decode handle is movable across threads | The promise's truth is the native library's contract | Documented authority (native contract); no Rust tool can check it |
| C3 | WASAPI COM session (CoInitialize, device activate, render loop, `from_raw_parts_mut` on `GetBuffer`) | **NO** — Win32/COM safety | Windows-only; unreachable on this server | NOT VERIFIED BY MIRI; Windows real-device gate (deferred unless a Windows runner exists) |
| C4 | Pure-Rust crates contain no `unsafe` | **YES** (inventory: zero `unsafe`/`extern "C"` outside the two FFI crates at base SHA) | Regressable — but a static check, not a verifier target | `cargo geiger`-style grep in CI if it ever matters; grep evidence recorded in campaign report |

## D. Compile-fail executable type-system evidence (existing, kept)

`crates/qianqian-audio-api/tests/{pcm_edge_contract,direct_pcm_flow}/compile_fail/*`
(trybuild) already prove negatively that borrows prevent retaining decode/edge
blocks past their call — the type system carries these lifetime properties and
no verifier re-earns them.

## Consequences for tool choice (B1+)

1. Kani (FV-RUST-0) owns **K1 stale FiberId, K2 relied_on guard, K3 effect
   discharge discipline (incl. violated tombstone), K4 removal discipline,
   K5 quiet truth, K6 single-source incl. #126 overlap guard** — exactly the
   rows with RUNTIME CHECK STILL NEEDED = YES and no concurrency.
2. Loom (FV-CONC-0) owns the playback slice (B1–B4): the mutex-guarded edge
   and completion races — the properties whose risk is interleaving, per the
   risk-driven formalization policy.
3. Miri (FV-UB-0) can only cover the pure-Rust crates; the FFI surface is
   C1–C3 and must be reported as NOT VERIFIED BY MIRI, not silently skipped.
4. Nothing above re-runs what A1/A5/A8/A13/B5/B8/D already have from the
   compiler and existing tests.
