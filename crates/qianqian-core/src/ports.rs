//! Capability/data-plane mechanism seams consumed by product code.
//!
//! `AudioOutput` is the production output seam: `qianqian-runtime`'s
//! `AudioOutputCapability::Service` binds `dyn AudioOutput`. It is
//! intentionally incomplete — final media APIs must emerge under real
//! implementation pressure. Logical boundary != crate boundary: this
//! trait implies nothing about physical packaging or dynamic loading.

/// Canonical PCM -> physical device, plus physical/output evidence.
pub trait AudioOutput {}
