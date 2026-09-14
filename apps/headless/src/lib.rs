//! Headless shell vocabulary for Architecture v2.
//!
//! The library target owns the product-facing CLI grammar so that parsing
//! stays separable from the playback mechanism code the binary wires up.
//! Since F1 the binary wires one control command: a `stop` line on stdin
//! becomes a request through the session's application-facing seam
//! (`SessionCompletion::request_stop`). Since F2 it also wires one
//! read-side command: a `status` line renders the session's truthful
//! observation (`status::format_status` over
//! `SessionCompletion::observation`). Recognizing the other intents is
//! still not control; they wait for their Phase-F slice.

pub mod cli;
pub mod status;
