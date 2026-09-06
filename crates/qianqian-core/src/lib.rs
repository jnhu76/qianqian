//! Qianqian product core.
//!
//! Owns product and music semantics for Architecture v2:
//! Base Kernel composition concepts, Music Kernel semantics,
//! capability ports, and the Presentation seam.
//!
//! This crate must stay portable product semantics: no platform,
//! native-media, or UI implementation dependency.

pub mod base;
pub mod music;
pub mod ports;
pub mod presentation;
