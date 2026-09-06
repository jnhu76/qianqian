//! Capability ports: mechanism seams consumed by the core.
//!
//! These traits are intentionally incomplete in R0. They freeze the
//! authorized capability names and dependency direction only; the
//! final media APIs must emerge under real implementation pressure.
//!
//! Logical boundary != crate boundary: these are not plugin loading
//! points and imply nothing about physical packaging.

/// Encoded media -> canonical PCM.
pub trait Decoder {}

/// Canonical PCM -> canonical PCM.
pub trait Processing {}

/// Canonical PCM -> physical device, plus output evidence.
pub trait AudioOutput {}
