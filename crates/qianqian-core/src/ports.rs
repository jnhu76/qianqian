//! Capability/data-plane mechanism seams consumed by product code.
//!
//! These traits are intentionally incomplete. `Decoder` and `AudioOutput`
//! represent independently composed provider seams already used by the Music
//! component boundary. `Processing` names the canonical PCM-transform seam,
//! but does not by itself imply that every processing node—or even the whole
//! processing graph—must be an independent Composition plugin.
//!
//! Final media APIs must emerge under real implementation pressure.
//! Logical boundary != crate boundary: these traits imply nothing about
//! physical packaging or dynamic loading.

/// Encoded media -> canonical PCM provider seam.
pub trait Decoder {}

/// Canonical PCM -> canonical PCM processing seam.
pub trait Processing {}

/// Canonical PCM -> physical device, plus physical/output evidence.
pub trait AudioOutput {}
