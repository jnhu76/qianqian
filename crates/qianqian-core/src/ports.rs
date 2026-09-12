//! Capability/data-plane mechanism seams consumed by product code.
//!
//! These are the production seams earned by the first audible slice
//! (`docs/architecture/first-audible-slice.md`): `AudioOutput` is the
//! output seam, `PcmDecode` the decode seam. Logical boundary != crate
//! boundary: a trait implies nothing about physical packaging or dynamic
//! loading. Contracts speak PCM, never decoder/vendor vocabulary.
//!
//! Capability identity is the key-type definition site in this module —
//! not any concrete provider implementation. Consumers depend on the
//! definition across the plugin seam; providers own mechanisms. No
//! provider or consumer is wired here.

use std::path::Path;
use std::sync::Arc;

use qianqian_kernel::Capability;

/// Why a render stream could not be opened.
#[derive(Debug)]
pub struct OutputError {
    pub message: String,
}

/// One pull outcome on the consumer side of the bounded PCM edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmPull {
    /// `n > 0` frames were written into the caller's destination slice.
    Frames(usize),
    /// The edge is empty and the producer has committed to EOF: drain
    /// whatever was already consumed and finish.
    Eof,
    /// The data plane was stopped (or failed): abort consuming. Failure
    /// detail lives in the session-owned completion signal, not here.
    Stopped,
}

/// The consumer half of the bounded PCM edge, pre-bound to one render
/// stream at open time. The steady render path touches only this trait —
/// never the kernel, the filesystem or a decoder.
///
/// Terminals are reachable from both ends: `stop` unblocks a blocked
/// reader (and, symmetrically, the producer the edge carries) so stop,
/// failure and EOF can never wedge the data plane.
pub trait PcmFrameSource: Send + Sync {
    /// Read frames into `dst` (interleaved float32), blocking until at
    /// least one frame, a terminal, or `stop`. Returns `Frames(n)` with
    /// n > 0, or the terminal outcome.
    fn read_frames(&self, dst: &mut [f32]) -> PcmPull;

    /// Request the data plane to stop. Idempotent; wakes every blocked
    /// endpoint. `read_frames` then returns `Stopped`.
    fn stop(&self);
}

/// Session-owned drain signal: the mechanism reports one terminal render
/// verdict exactly once. It knows nothing about sessions — it only
/// publishes the mechanism fact "this stream finished draining" (or
/// aborted).
#[derive(Clone, Debug, Default)]
pub struct DrainSignal {
    inner: Arc<DrainInner>,
}

#[derive(Debug, Default)]
struct DrainInner {
    verdict: std::sync::Mutex<Option<DrainVerdict>>,
    ready: std::sync::Condvar,
}

/// How the render leg terminated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrainVerdict {
    /// EOF was reached and every submitted frame played out.
    Drained,
    /// The stream aborted before draining (stop, failure).
    Aborted,
}

impl DrainSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish the terminal verdict (exactly once; later calls are no-ops).
    pub fn complete(&self, verdict: DrainVerdict) {
        let mut guard = self.inner.verdict.lock().expect("drain verdict lock");
        if guard.is_none() {
            *guard = Some(verdict);
            self.inner.ready.notify_all();
        }
    }

    /// Block until a verdict exists.
    pub fn wait(&self) -> DrainVerdict {
        let mut guard = self.inner.verdict.lock().expect("drain verdict lock");
        loop {
            if let Some(v) = *guard {
                return v;
            }
            guard = self.inner.ready.wait(guard).expect("drain verdict wait");
        }
    }

    /// Current verdict, if any (non-blocking observation).
    pub fn peek(&self) -> Option<DrainVerdict> {
        *self.inner.verdict.lock().expect("drain verdict lock")
    }
}

/// Request for one playback-specific render stream: the source format to
/// negotiate, the pre-bound PCM frame source, and the session-owned drain
/// signal. All data-plane pieces bind once, here.
pub struct RenderRequest {
    pub format: PcmFormat,
    pub source: Arc<dyn PcmFrameSource>,
    pub drain: DrainSignal,
}

/// One acquired render stream: owns its render thread and the physical
/// device session for one playback episode.
pub trait RenderStream: Send {
    /// The format the device actually accepted (diagnostic truth).
    fn negotiated_format(&self) -> PcmFormat;

    /// Signal stop, wait for the render thread to exit, release the
    /// device. Consumes the stream: stop -> join -> release in one
    /// owner-local inverse, on the mechanism side.
    fn stop_and_join(self: Box<Self>);
}

/// Output capability service: opens local render streams. Long-lived
/// mechanism provider; the acquired stream belongs to the caller.
pub trait AudioOutput {
    /// Open a render stream for `request`. Blocks for a bounded open
    /// verdict: device-open or negotiation failure is returned here, so
    /// activation can fail fast and cleanly (no half-open stream).
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError>;
}

/// Capability key for the output contract. Identity is this definition.
pub struct AudioOutputCapability;

impl Capability for AudioOutputCapability {
    const NAME: &'static str = "AudioOutput";
    type Service = dyn AudioOutput;
}

/// Source PCM format truth: interleaved float32 at the source rate/layout
/// (first-audible-slice design §1.1). `channel_mask == 0` means unknown;
/// consumers must not guess channel order from the count alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub channel_mask: u64,
}

/// One frame-level terminal outcome of a decode endpoint read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeOutcome {
    /// `n` frames were written into the caller's destination slice.
    Frames(usize),
    /// End of decoded PCM. Terminal, normal, not an error.
    Eof,
}

/// Why a media source could not be opened as a decode endpoint.
#[derive(Debug)]
pub struct DecodeOpenError {
    pub message: String,
}

/// Why a decode endpoint stopped producing PCM before EOF.
#[derive(Debug)]
pub struct DecodeError {
    pub message: String,
}

/// One playback-specific decode endpoint: media -> source PCM.
///
/// The endpoint owns its native decode handle for exactly one playback
/// episode and is released on drop. It is `Send` (movable to a decode
/// worker thread) but not `Sync`: calls on one endpoint must be
/// externally serialized, mirroring the native mechanism contract.
pub trait PcmSource: Send {
    /// The format of every frame this endpoint will produce. Immutable
    /// for the endpoint's lifetime (the native mechanism fails closed on
    /// mid-stream format changes rather than contradicting this value).
    fn format(&self) -> PcmFormat;

    /// Read up to `dst.len() / channels` frames into `dst` as interleaved
    /// float32. Blocking-free; decode work happens here.
    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError>;
}

/// Decode capability service: opens local media files as owned decode
/// endpoints. Long-lived mechanism provider; per-episode state (the
/// endpoint) belongs to the caller, not to the service.
pub trait PcmDecode {
    fn open_source(&self, path: &Path) -> Result<Box<dyn PcmSource>, DecodeOpenError>;
}

/// Capability key for the decode contract. Identity is this definition.
pub struct PcmDecodeCapability;

impl Capability for PcmDecodeCapability {
    const NAME: &'static str = "PcmDecode";
    type Service = dyn PcmDecode;
}
