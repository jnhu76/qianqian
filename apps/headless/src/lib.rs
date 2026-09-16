//! Headless shell vocabulary for Architecture v2.
//!
//! The library target owns the product-facing CLI grammar and the
//! status text projection so both stay separable from the playback
//! mechanism code the binary wires up. Since F1 the binary wires
//! `stop`; since F2 it also wires `status`, rendering the playback
//! seam's coherent observation truthfully. Recognizing the other
//! intents is still not control; they wait for their Phase-F slice.

pub mod cli;
pub mod status;
