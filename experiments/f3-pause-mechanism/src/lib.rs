//! F3-GATE mechanism evidence for Pause/Resume (Issue #119; ADR-PBK-002
//! D14.7 gate).
//!
//! This crate is **executable evidence, not production architecture**
//! (D14.7: F3 may prototype or audit the mechanism outside the product
//! path). It contains no qianqian-* dependency, no product code import,
//! and no pause behavior in the product path.
//!
//! What it establishes, per the F3-GATE mechanism experiment:
//!
//! 1. **Synchronization-shape evidence** (all platforms,
//!    `tests/scenarios.rs`): a faithful copy of the production
//!    synchronization shape — the bounded ring edge (product
//!    `PcmEdge`'s Mutex/Condvar semantics), the decode worker shape
//!    (product `decode_worker`'s write/terminal loop), and the render
//!    loop shape (product `steady_loop`'s wait → padding → GetBuffer →
//!    read → release order) — driven through both candidate pause
//!    mechanisms:
//!
//!    ```text
//!    A. explicit render-loop pause gate located BEFORE GetBuffer
//!    B. the same gate + IAudioClient::Stop / Start (device-level
//!       freeze; mock device here, real device in the probe)
//!    ```
//!
//!    The scenario suite proves the liveness/truthfulness properties the
//!    gate decision needs: decoder progression stops through bounded
//!    backpressure, stop wakes every blocked participant, no device
//!    buffer is ever held across a parked pause, edge contents survive a
//!    pause/resume cycle untouched, and pause × stop / EOF / failure
//!    produce exactly the evidence histories the already-merged D11
//!    decision table classifies (48-tuple exhaustive oracle, PR #144) —
//!    no resolver change is required by the gate shape.
//!
//! 2. **Physical WASAPI evidence** (`bin/f3probe.rs`, Windows only):
//!    the same two mechanisms against the real shared-mode event-driven
//!    device — padding timelines, already-submitted-audio fate, engage/
//!    resume latency, GetBuffer-while-paused counter, device continuity
//!    (one session lifetime, no reopen), stop-from-paused exit bound.
//!
//! Evidence mapping to product code (copies, deliberately not imports —
//! `PcmEdge` and the render loop are crate-private):
//!
//! ```text
//! src/edge.rs      ≈ crates/qianqian-playback/src/edge.rs      (sync shape)
//! src/worker.rs    ≈ crates/qianqian-playback/src/session.rs decode_worker
//! src/render.rs    ≈ crates/qianqian-output-wasapi/src/wasapi.rs steady_loop
//! bin/f3probe.rs   ≈ the real mechanism, instrumented
//! ```
//!
//! The gate decision and its normative record live in ADR-PBK-002 D14.7
//! and `RESULTS.md`; nothing here is authority.

pub mod edge;
pub mod events;
pub mod gate;
pub mod render;
pub mod sim;
pub mod worker;
