//! Qianqian product core.
//!
//! Owns product and music semantics for Architecture v2:
//! R0 bootstrap composition witnesses, Music Kernel semantics,
//! capability ports, and the Presentation seam.
//!
//! The future generic Composition Kernel is NOT expected to live in
//! this crate: a generic kernel must not depend on product semantics.
//! `base` is an R0 bootstrap witness, not the kernel's mandated home.
//!
//! This crate must stay portable product semantics: no platform,
//! native-media, or UI implementation dependency.

pub mod base;
pub mod music;
pub mod ports;
pub mod presentation;
