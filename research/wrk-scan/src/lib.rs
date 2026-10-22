//! WRK-SCAN-v0/R2-0-v3 Gate I research core (issue #306).
//!
//! One `WorkloadCore` shared by H0 and every future mechanism arm (H1–H4),
//! one independent REF, one independent oracle (O1–O10), a dual-mode
//! observer sink, and a clean std-only H0 executor.
//!
//! Provenance and authority:
//! - The frozen authority is the #304 freeze suite (`WRK-SCAN-v0/R2-0-v3`,
//!   SUITE_SHA256 = 6a00d0a8…); `FREEZE-AUTHORITY-MAP.md` records the
//!   recovery and byte-identity verification of every frozen source file.
//! - `workload.rs` / `ledger.rs` / `oracle.rs` reuse the frozen probe
//!   sources (APP-A/APP-B) with the minimal extensions recorded in
//!   FREEZE-AUTHORITY-MAP §7; the probe batch's behavior stays byte-level
//!   identical to the frozen source — guarded by the golden-vector tests,
//!   including the frozen `AGG_VALUE = 0x980a9dc0d8d8cee6` receipt.
//! - `h0.rs` is a clean H0 written for this round: it must NOT inherit the
//!   G2 probe's scheduling logic (no polling `wait_timeout`, no mutant
//!   fields in the shipped path).
//!
//! This crate is research-only evidence infrastructure. It is not a product
//! component, defines no player semantics, and reports no performance
//! numbers (Gate I is non-timed correctness).

#![forbid(unsafe_code)]

pub mod h0;
pub mod k0_session;
pub mod ledger;
pub mod observer;
pub mod oracle;
pub mod scenario;
pub mod session_core;
pub mod workload;
