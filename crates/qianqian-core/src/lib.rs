//! Qianqian product core.
//!
//! Owns portable product semantics for Architecture v2: capability ports and
//! the Presentation seam, plus the `music` / `transport` modules carried over
//! from an earlier Playback architecture experiment.
//!
//! **Experimental Playback evidence:** the `music` / `transport` types
//! (e.g. `MusicKernel`, `TransportKernel`) originate from an earlier Playback
//! architecture experiment. Their presence does not establish current
//! architecture authority or compatibility requirements; the current Playback
//! Foundations proposal is `docs/adr/ADR-PBK-001.md` (PROPOSED / REOPENED).
//!
//! The generic Composition Kernel lives outside this crate: a generic kernel
//! must not depend on product semantics. `base` is an R0 bootstrap witness,
//! not the kernel's mandated home.
//!
//! This crate must stay portable product semantics: no platform,
//! native-media, or UI implementation dependency.

pub mod base;
pub mod music;
/// PCM-CONTRACT-A0 experimental harness (evidence only, not stable API).
pub mod pcm_contract_a0;
pub mod ports;
pub mod presentation;
pub mod transport;
