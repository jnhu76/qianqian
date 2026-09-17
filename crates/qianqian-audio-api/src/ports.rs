//! Capability/data-plane mechanism seams consumed by product code.
//!
//! These are the production seams earned by the first audible slice
//! (`docs/architecture/first-audible-slice.md`): `AudioOutput` is the
//! output seam, `PcmDecode` the decode seam. Logical boundary != crate
//! boundary: a trait implies nothing about physical packaging or dynamic
//! loading. Contracts speak PCM, never decoder/vendor vocabulary.
//!
//! Two PCM endpoints sit at different points of the one data path and are
//! named for it: [`DecodedPcmStream`] is the decode leg (encoded media in,
//! source-format PCM out), [`RenderPcmInput`] is the render leg's already
//! pre-bound input (the renderer knows nothing about decoders or media).
//!
//! Capability identity is the key-type definition site in this module —
//! not any concrete provider implementation. Consumers depend on the
//! definition across the plugin seam; providers own mechanisms. No
//! provider or consumer is wired here.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use qianqian_composition::Capability;

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
/// never the kernel, the filesystem or a decoder. It is the renderer's
/// PCM input: ready PCM only, no knowledge of where it came from.
///
/// Terminals are reachable from both ends: `stop` unblocks a blocked
/// reader (and, symmetrically, the producer the edge carries) so stop,
/// failure and EOF can never wedge the data plane.
pub trait RenderPcmInput: Send + Sync {
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
/// aborted), optionally notifying one owner-installed observer.
///
/// The observer is a small one-shot publication seam, not an event
/// system: at most one observer may be installed (at construction, so it
/// is in place before the signal can reach any publishing leg), and the
/// first successful [`DrainSignal::complete`] invokes it synchronously,
/// before that call returns and with no signal lock held.
#[derive(Clone, Debug, Default)]
pub struct DrainSignal {
    inner: Arc<DrainInner>,
}

/// The one-shot terminal-evidence observer type (see
/// [`DrainSignal::with_on_complete`]). Deliberately minimal: one
/// function, invoked once — not an event framework.
type OnComplete = Arc<dyn Fn(DrainVerdict) + Send + Sync>;

#[derive(Default)]
struct DrainInner {
    verdict: std::sync::Mutex<Option<DrainVerdict>>,
    on_complete: std::sync::Mutex<Option<OnComplete>>,
}

impl std::fmt::Debug for DrainInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Lock-free rendering: Debug may run concurrently with complete.
        f.debug_struct("DrainInner").finish_non_exhaustive()
    }
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

    /// Create a signal whose first successful [`DrainSignal::complete`]
    /// invokes `observer` once, synchronously, before `complete` returns.
    /// This is the generic terminal-evidence publication seam for the
    /// stream's owner; the mechanism layer knows nothing about what the
    /// observer publishes. The observer runs synchronously on the
    /// terminal publication path: it must not perform unbounded work or
    /// I/O, and must not call back into this signal (a re-entrant
    /// `complete` is a no-op first-wins anyway). Brief owner-side
    /// synchronization — such as acquiring the owner's state lock — is
    /// permitted and expected.
    pub fn with_on_complete(observer: impl Fn(DrainVerdict) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(DrainInner {
                verdict: std::sync::Mutex::new(None),
                on_complete: std::sync::Mutex::new(Some(Arc::new(observer))),
            }),
        }
    }

    /// Publish the terminal verdict (exactly once; later calls are
    /// no-ops). On the first successful publication the installed
    /// observer — if any — has finished running before this call
    /// returns.
    pub fn complete(&self, verdict: DrainVerdict) {
        let observer = {
            let mut guard = self.inner.verdict.lock().expect("drain verdict lock");
            if guard.is_some() {
                return;
            }
            *guard = Some(verdict);
            // Invoke outside the verdict lock: the observer may acquire
            // other locks, and no path may hold a drain lock while doing
            // so (lock-order safety).
            drop(guard);
            self.inner
                .on_complete
                .lock()
                .expect("drain observer lock")
                .clone()
        };
        if let Some(observer) = observer {
            observer(verdict);
        }
    }
}

/// What one render-stream gate event acknowledges back to its owner
/// (ADR-PBK-002 D14.7). Mechanism evidence only: these events record
/// where the render leg physically is; they are never semantic truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateEvent {
    /// The render leg reached the loop-top gate and parked: it will
    /// submit no further PCM while parked and holds no device buffer.
    Engaged,
    /// The CURRENT engagement observed its output tail quiesced (no
    /// frame submitted before engagement remains queued for rendering).
    /// Published at most once per engagement.
    TailQuiesced,
    /// The park ended (resume or a release): the leg proceeds once more
    /// and the data plane — never the gate — decides what happens next.
    Disengaged,
}

/// Session-owned render pause gate (ADR-PBK-002 D14.7, mechanism A):
/// routes one episode's pause intent into a render mechanism's loop-top
/// gate check and acknowledges engagement / tail quiescence /
/// disengagement back to the owner as mechanism events.
///
/// Ownership mirrors [`DrainSignal`]: the session creates the gate (with
/// its observer, before any mechanism can see it) and hands it to the
/// render stream in [`RenderRequest`]; the mechanism never holds intent
/// truth of its own. The mechanism contract is frozen:
///
/// ```text
/// check the gate at the render loop top, strictly BEFORE the
///     device-buffer acquisition (GetBuffer), never holding a device
///     buffer across a park;
/// while parked, submit nothing, hold no device buffer, and wait in
///     bounded slices (notify + cap) so release and stop wake the leg
///     with bounded latency;
/// never abort the leg from the gate — after release the loop proceeds
///     once more and the data plane decides;
/// the device stream stays open — a park replaces no resource.
/// ```
///
/// One gate serves exactly one render leg: the engagement evidence, the
/// once-per-engagement tail-quiescence discipline, and the
/// disengagement fence that keeps the owner's evidence attributable to
/// the current engagement are all sound only under that premise.
#[derive(Clone, Debug, Default)]
pub struct RenderGate {
    inner: Arc<GateInner>,
}

#[derive(Default)]
struct GateInner {
    /// The mechanism's view of the routed pause intent. Command truth
    /// lives with the episode owner; this flag is the routed copy the
    /// render leg observes.
    paused: Mutex<bool>,
    /// Once true, this gate can never park a leg again: the open-abort
    /// lifetime (a stream being torn down without ever becoming a
    /// session episode), so a later `set_paused(true)` — e.g. pause
    /// intent routed for an episode that never opened — must be inert.
    /// Ordinary mechanism-lifetime state of an owned resource, not a
    /// new lifecycle noun. There is no un-close: a closed gate is
    /// finished.
    closed: AtomicBool,
    wake: Condvar,
    on_event: Mutex<Option<OnGateEvent>>,
}

/// The one-shot gate-evidence observer type (see
/// [`RenderGate::with_observer`]). Deliberately minimal, like
/// [`DrainSignal`]'s: one function invoked per event — not an event
/// framework.
type OnGateEvent = Arc<dyn Fn(GateEvent) + Send + Sync>;

impl std::fmt::Debug for GateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Lock-free rendering: Debug may run concurrently with the leg.
        f.debug_struct("GateInner").finish_non_exhaustive()
    }
}

/// Bounded park slice: the notify is the wake path, this cap is the
/// backstop so a missed wakeup costs latency (one slice), never
/// correctness.
const PARK_SLICE: std::time::Duration = std::time::Duration::from_millis(10);

impl RenderGate {
    /// A gate with no observer: intent routing works, but every
    /// engagement / quiescence / disengagement event is silently
    /// discarded. For mechanism tests only — a production episode's
    /// gate is built with [`RenderGate::with_observer`] by its owner,
    /// because D14.7 requires engagement evidence to reach the session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a gate whose first observer receives every engagement /
    /// tail-quiescence / disengagement event, synchronously, outside the
    /// gate's own lock. This is the owner-side evidence publication seam;
    /// the mechanism knows nothing about what the observer publishes.
    /// The observer must not perform unbounded work or I/O and must not
    /// call back into this gate (owner-side synchronization such as
    /// acquiring the owner's state lock is permitted, as for
    /// [`DrainSignal::with_on_complete`]).
    pub fn with_observer(observer: impl Fn(GateEvent) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(GateInner {
                paused: Mutex::new(false),
                closed: AtomicBool::new(false),
                wake: Condvar::new(),
                on_event: Mutex::new(Some(Arc::new(observer))),
            }),
        }
    }
    /// Route the pause intent into the mechanism: `true` parks the render
    /// leg at its next loop-top gate check; `false` releases a parked leg
    /// (bounded-slice latency via notify). Idempotent. Inert on a closed
    /// gate: a closed gate never parks again (see
    /// [`RenderGate::close_and_release`]).
    pub fn set_paused(&self, paused: bool) {
        if self.inner.closed.load(Ordering::Acquire) {
            return;
        }
        let mut guard = self.inner.paused.lock().expect("render gate lock");
        if *guard == paused {
            return;
        }
        *guard = paused;
        drop(guard);
        self.inner.wake.notify_all();
    }

    /// Close the gate permanently: the episode's render leg must never
    /// park here again. This is the open-abort release — a stream whose
    /// open failed or timed out is being torn down without ever
    /// becoming a session episode, and the abort join that follows must
    /// be safe against ANY later pause intent on this gate, routed or
    /// hostile: once closed, `set_paused` routes nothing and a leg
    /// between two park calls finds the gate shut at its next loop-top
    /// check. A parked leg is woken with bounded latency (notify + the
    /// park-slice cap) and proceeds once more; the data plane — here the
    /// stop issued by the abort itself — decides how it ends. Idempotent;
    /// there is no un-close.
    pub fn close_and_release(&self) {
        self.inner.closed.store(true, Ordering::Release);
        self.inner.wake.notify_all();
    }

    /// The render mechanism's loop-top gate: park while pause intent is
    /// routed here, then return (the caller proceeds once more; the data
    /// plane decides what its next read sees). Returns immediately when
    /// no intent is routed.
    ///
    /// While parked the leg submits nothing and holds no device buffer.
    /// Between bounded slices it calls `tail_observed_quiescent` — the
    /// mechanism's own observation of its output tail — and publishes
    /// [`GateEvent::TailQuiesced`] on the first `true` of the current
    /// engagement. [`GateEvent::Engaged`] is published on park entry and
    /// [`GateEvent::Disengaged`] on park exit. Returns immediately (no
    /// events) on a closed gate.
    pub fn park_while_paused(&self, mut tail_observed_quiescent: impl FnMut() -> bool) {
        {
            let guard = self.inner.paused.lock().expect("render gate lock");
            if !*guard || self.inner.closed.load(Ordering::Acquire) {
                // Released or closed before the leg reached the gate:
                // nothing engaged, nothing to acknowledge.
                return;
            }
        }
        self.emit(GateEvent::Engaged);
        let mut quiesced_published = false;
        loop {
            let released = {
                let guard = self.inner.paused.lock().expect("render gate lock");
                let closed = &self.inner.closed;
                let (guard, _) = self
                    .inner
                    .wake
                    .wait_timeout_while(guard, PARK_SLICE, |paused| {
                        *paused && !closed.load(Ordering::Acquire)
                    })
                    .expect("render gate wait poisoned");
                !*guard || closed.load(Ordering::Acquire)
            };
            if released {
                break;
            }
            if !quiesced_published && tail_observed_quiescent() {
                quiesced_published = true;
                self.emit(GateEvent::TailQuiesced);
            }
        }
        self.emit(GateEvent::Disengaged);
    }

    fn emit(&self, event: GateEvent) {
        // Clone outside the event publication: the observer may acquire
        // other locks, and no path may hold the gate lock while it runs
        // (same lock-order discipline as DrainSignal::complete).
        let observer = self
            .inner
            .on_event
            .lock()
            .expect("gate observer lock")
            .clone();
        if let Some(observer) = observer {
            observer(event);
        }
    }
}

/// Session-owned, episode-scoped position-evidence cell (ADR-PBK-002
/// D14.8): the render leg publishes one monotone sample of the
/// device-consumed presentation position into it, and the observation
/// path reads that sample as one pure load.
///
/// Truth class: **Mechanism Evidence**. Never a Fact, never a transport
/// state, and never a correctness basis for control, lifetime,
/// settlement, resource lifetime, mechanism wakeup or K0 lifecycle.
/// The sample is exact only for the instant its writer took the tail
/// reading; a reader is promised no bound on how old its sample is
/// (freshness is a scheduling property of the reader, not a concurrency
/// invariant) — only that the published sample never goes backward,
/// never exceeds the writer's own handed-off accounting, and is never
/// fabricated.
///
/// Writer contract (D14.8): exactly ONE writer — the episode's render
/// leg — which owns both derivation inputs on its own execution path:
///
/// ```text
/// handed_off   the frames this episode's leg has submitted into the
///              device buffer, its own plain local accounting
/// tail         that leg's own queued-to-play reading
///              (GetCurrentPadding), taken on the same execution path
/// publish      published = max(published, handed_off - min(tail, handed_off))
/// ```
///
/// The subtraction therefore happens on the render leg's path, and the
/// monotonicity is owned by the publication: no reader composes a
/// position from two cells, keeps a previous value, or clamps anything —
/// which is what keeps the D14.2 observation a pure read. The update is
/// one relaxed monotone RMW: no lock, no allocation, no blocking, no
/// device call of its own.
///
/// Encoding: the zero-initialized cell means **undefined** (no sample
/// published yet — "unknown is not zero"); a sample of `N` source frames
/// is stored as `N + 1`, saturating at [`PositionEvidence::MAX_POSITION`].
/// The encoding is total: it neither wraps nor panics at the domain
/// boundary, and the boundary itself is unreachable for a live episode
/// (see [`PositionEvidence::MAX_POSITION`]).
#[derive(Clone, Debug, Default)]
pub struct PositionEvidence {
    published: Arc<AtomicU64>,
}

impl PositionEvidence {
    /// The largest representable sample, in source PCM frames.
    ///
    /// The cell stores `sample + 1`, so this is the top of the encoding's
    /// legal domain: a sample above it (including the u64::MAX one) is
    /// stored — and later read back — as this value. The one value below
    /// the domain, zero, is the undefined sentinel instead.
    ///
    /// Reaching the top would take a single live episode submitting
    /// `u64::MAX - 1` source frames into its device buffer, each one
    /// consumed at the source rate by the output engine — a frame count
    /// bounded by real elapsed time (~1.8e19 frames is ≈ 1.3e7 years at
    /// 44.1 kHz). The saturated value is never published in practice, and
    /// a saturated publication would still be monotone, still ≤ the
    /// writer's accounting, and still never fabricated.
    pub const MAX_POSITION: u64 = u64::MAX - 1;

    /// A cell with no published sample (undefined — not zero).
    pub fn new() -> Self {
        Self::default()
    }

    /// Writer: publish the consumed estimate this leg derives from its
    /// own `handed_off` accounting and its own `tail` reading, taken at
    /// the same instant. `tail` is capped at `handed_off` (a tail reading
    /// above it is a legal transient, never a wrap), and the publication
    /// keeps the running maximum.
    ///
    /// Mechanism path: called from the render thread's tail observations.
    /// One relaxed RMW, no device call, no lock — and the caller must
    /// already have the reading it is passing in (F4 adds no device call).
    pub fn publish_consumed(&self, handed_off: u64, tail: u64) {
        let estimate = handed_off - tail.min(handed_off);
        // The encode saturates so it stays total: the unreachable
        // u64::MAX sample maps to the top of the legal domain instead of
        // wrapping into 0, which would have read as "undefined".
        let encoded = estimate.saturating_add(1);
        // The single writer is monotone, so the max is a guard against a
        // regressing tail reading (the queue growing again) rather than a
        // repair of a torn read: both inputs come from one execution path.
        self.published.fetch_max(encoded, Ordering::Relaxed);
    }

    /// Reader: one pure load of the published sample, in source PCM
    /// frames. `None` while no sample has been published (the episode's
    /// render leg has not reached its first tail observation, or the
    /// caller is gating the projection away — e.g. after a terminal
    /// Fact, where the D14.8 projection is withdrawn). Repeating it
    /// changes nothing.
    pub fn published(&self) -> Option<u64> {
        match self.published.load(Ordering::Relaxed) {
            // The zero-initialized cell is the undefined sentinel, so a
            // published 0-frame sample is encoded as 1 and never
            // collapses into "unknown".
            0 => None,
            encoded => Some(encoded - 1),
        }
    }
}

/// Request for one playback-specific render stream: the source format to
/// negotiate, the pre-bound PCM input, the session-owned drain signal,
/// the session-owned pause gate, and the session-owned position-evidence
/// cell. All data-plane pieces bind once, here.
pub struct RenderRequest {
    pub format: PcmFormat,
    pub input: Arc<dyn RenderPcmInput>,
    pub drain: DrainSignal,
    pub gate: RenderGate,
    /// The episode's position-evidence cell (D14.8). The render leg's F4
    /// obligation is exactly this: from the tail readings it already
    /// takes, publish its own consumed estimate into this cell — one
    /// monotone relaxed update per observation, from the same execution
    /// path that owns the handed-off accounting. The cell is an owned
    /// resource of the episode (like the gate), not a Capability; the
    /// leg neither reads it nor creates one.
    pub position: PositionEvidence,
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
/// (first-audible-slice design §1.1). `channels` is always `> 0`;
/// `channel_mask == 0` means the channel layout is unspecified/unknown
/// and is never a valid "no channels selected" format — consumers must
/// not guess channel order from the count alone.
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

/// One playback-specific decode endpoint: from encoded media to
/// source-format decoded PCM.
///
/// The endpoint owns its native decode handle for exactly one playback
/// episode and is released on drop. It is `Send` (movable to a decode
/// worker thread) but not `Sync`: calls on one endpoint must be
/// externally serialized, mirroring the native mechanism contract.
pub trait DecodedPcmStream: Send {
    /// The format of every frame this endpoint will produce. Immutable
    /// for the endpoint's lifetime (the native mechanism fails closed on
    /// mid-stream format changes rather than contradicting this value).
    fn format(&self) -> PcmFormat;

    /// The source duration this mechanism reported at probe/open time,
    /// or `None` when it reported none.
    ///
    /// Truth class (ADR-PBK-002 D14.8): optional **source-scoped
    /// Mechanism Evidence**, relayed once by the session as episode
    /// evidence. Never a Fact, and NOT exact in general — it is the
    /// container's own declaration, so it may over- or under-claim what
    /// the decodable audio actually contains (a truncated stream still
    /// reports its declared length). Only the actual decoded total at
    /// decode EOF is exact, and that is terminal consumption truth, not
    /// this value.
    ///
    /// Unknown stays unknown: a provider whose probe reported no duration
    /// (a sentinel such as a negative value) returns `None` — it must
    /// never convert that into zero or into an estimate. A legitimate
    /// zero-length source reports `Some(ZERO)`, which is therefore
    /// distinguishable from "unknown".
    fn source_duration(&self) -> Option<Duration>;

    /// Read up to `dst.len() / channels` frames into `dst` as interleaved
    /// float32. Blocking-free; decode work happens here.
    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError>;
}

/// Decode capability service: opens local media files as owned decode
/// endpoints. Long-lived mechanism provider; per-episode state (the
/// endpoint) belongs to the caller, not to the service.
pub trait PcmDecode {
    fn open_media(&self, path: &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError>;
}

/// Capability key for the decode contract. Identity is this definition.
pub struct PcmDecodeCapability;

impl Capability for PcmDecodeCapability {
    const NAME: &'static str = "PcmDecode";
    type Service = dyn PcmDecode;
}
