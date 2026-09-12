//! Qianqian Audio API: shared audio contracts and vocabulary.
//!
//! Owns the portable product capability seam currently required by
//! production composition: the `AudioOutput` port consumed by
//! `qianqian-playback`. The historical playback experiment code
//! (`MusicKernel` / `TransportKernel`) no longer lives here; it survives
//! as test-local executable evidence under
//! `tests/playback_temporal_traces/` and carries no production API
//! identity (Playback Foundations authority: `docs/adr/ADR-PBK-001.md`).
//!
//! The generic Composition Kernel lives outside this crate: a generic
//! kernel must not depend on product semantics.
//!
//! This crate must stay portable product semantics: no platform,
//! native-media, or UI implementation dependency.

pub mod ports;
