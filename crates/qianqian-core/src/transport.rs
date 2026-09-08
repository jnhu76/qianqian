//! Transport Kernel: playback temporal authority.
//!
//! `TransportKernel` owns the meaning of playback time: active/prepared
//! temporal roles, generation admission, discontinuity execution, physical
//! fence coordination, and interpretation of raw playback evidence.
//!
//! This module intentionally does not freeze the final playback state-machine
//! representation or media APIs. Those shapes must emerge under executable
//! implementation pressure. `TransportKernel` is a semantic authority role,
//! not a Composition plugin boundary.
//!
//! This shell aligns code vocabulary with the proposed Playback architecture
//! (ADR-PBK-001, PROPOSED / FORMAL CORE PASS). It does not constitute
//! production Playback implementation authorization.

/// Authority for playback-temporal semantics.
///
/// The initial shell is deliberately state-free: repository code now carries
/// the frozen authority vocabulary without prematurely encoding Window,
/// Generation, Fence, TrackSession, or DecodeSession representations.
#[derive(Debug, Default)]
pub struct TransportKernel;

impl TransportKernel {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_kernel_shell_is_constructible() {
        let _kernel = TransportKernel::new();
    }
}
