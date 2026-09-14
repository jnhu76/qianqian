//! Headless shell vocabulary for Architecture v2.
//!
//! The library target owns the product-facing CLI grammar so that parsing
//! stays separable from the playback mechanism code the binary wires up.
//! Since F1 the binary wires one command: a `stop` line on stdin becomes
//! a request through the session's application-facing seam
//! (`SessionCompletion::request_stop`). Recognizing the other intents is
//! still not control; they wait for their Phase-F slice.

pub mod cli;
