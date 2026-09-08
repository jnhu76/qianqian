//! Qianqian product core.
//!
//! Owns portable product semantics for Architecture v2:
//! `MusicKernel` for music/product meaning, `TransportKernel` for playback
//! temporal meaning, capability ports, and the Presentation seam.
//!
//! The generic Composition Kernel lives outside this crate: a generic kernel
//! must not depend on product semantics. `base` is an R0 bootstrap witness,
//! not the kernel's mandated home.
//!
//! This crate must stay portable product semantics: no platform,
//! native-media, or UI implementation dependency.

pub mod base;
pub mod music;
pub mod ports;
pub mod presentation;
pub mod transport;
