//! Headless shell vocabulary for Architecture v2.
//!
//! The library target owns the product-facing CLI grammar so that parsing
//! stays separable from the playback mechanism code the binary wires up.
//! In Phase F0 the grammar is parse-only: recognizing an intent here is
//! not control, and no playback semantics are attached to any command.

pub mod cli;
