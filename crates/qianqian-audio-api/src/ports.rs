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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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
///
/// The `Seek*` variants are the F5 cutover's cut-attributed park
/// acknowledgments (ADR-PBK-002 D14.5): an internal seek park is NOT
/// pause engagement evidence and never routes pause intent. They exist
/// so the session can gate the seek protocol and its cutover commit on
/// real "leg parked / tail quiesced" evidence without misattributing
/// that park to pause; they never become product surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateEvent {
    /// The render leg reached the loop-top gate and parked: it will
    /// submit no further PCM while parked and holds no device buffer.
    /// Pause-attributed (D14.7).
    Engaged,
    /// The CURRENT pause engagement observed its output tail quiesced (no
    /// frame submitted before engagement remains queued for rendering).
    /// Published at most once per engagement.
    TailQuiesced,
    /// The pause park ended (resume or a release): the leg proceeds once
    /// more and the data plane — never the gate — decides what happens
    /// next.
    Disengaged,
    /// The render leg parked at the loop-top gate under a routed seek
    /// hold (the D14.5 cutover park, reuse of the D14.7 park invariant —
    /// no device buffer held across the park). Cut-attributed, never
    /// pause engagement evidence.
    SeekEngaged,
    /// The CURRENT seek park observed its output tail quiesced (the
    /// D14.5 commit-boundary evidence class: padding == 0 while parked).
    /// Published at most once per seek park.
    SeekTailQuiesced,
    /// The seek park ended (a release routed by the session): the leg
    /// proceeds once more and the data plane decides what happens next.
    SeekDisengaged,
}

/// Why (and with what payload) a routed seek park was released
/// (ADR-PBK-002 D14.5). The session owns the cutover commit; this value
/// is how the commit release reaches the render leg on its own execution
/// path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekParkRelease {
    /// The cutover committed: the leg must rebase its position
    /// accounting BEFORE any further submission. `landing` is the
    /// decoder's reported actual landing in source PCM frames; `None`
    /// means the landing is unknown, which withdraws the Position
    /// projection for the rest of the episode (unknown stays unknown —
    /// never zero, never the requested target).
    Committed { landing: Option<u64> },
    /// No cutover happened (refusal, destructive failure, stop, or
    /// teardown): no rebase instruction exists and the leg's position
    /// accounting is untouched.
    Aborted,
}

/// The render leg's answer to ONE tail observation (the
/// [`GateSlice::TailProbe`] slice of [`RenderGate::park_loop_top`]).
/// Three truth classes, never collapsible: a device that still holds
/// queued frames, a drained tail, and an observation that could not be
/// taken at all (F5 implementation corrective-4: a failed observation
/// is NOT "not quiesced yet" — masking it as a pending tail would park
/// the leg forever while the seek worker waits for quiescence evidence
/// a dead device can never publish).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailProbeOutcome {
    /// Frames submitted before the park remain queued: keep waiting in
    /// bounded slices.
    Pending,
    /// Nothing submitted before the park remains queued: the tail is
    /// quiescent — the D14.5/D14.7 quiescence evidence.
    Quiesced,
    /// The observation itself failed (a real output-mechanism failure,
    /// e.g. an invalidated endpoint): not quiescence evidence and not a
    /// wait-forever condition — the gate releases the leg without
    /// publishing quiescence and reports the failure back, so the
    /// mechanism's existing device-failure path decides.
    Failed,
}

/// What [`RenderGate::park_loop_top`] hands back to the render leg when
/// the gated work ends. The leg proceeds either way — the gate never
/// aborts it; a failed tail observation is the mechanism's own failure
/// evidence, returned for the mechanism's existing failure path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkOutcome {
    /// Parked work finished (or none was routed): proceed with the loop.
    Proceeded,
    /// The leg's own tail observation failed while parked: no quiescence
    /// was published for it and the gate released the leg without
    /// waiting further (F5 implementation corrective-4).
    TailProbeFailed,
}

/// One unit of loop-top work [`RenderGate::park_loop_top`] hands back to
/// the render leg while it is gated (D14.7 + D14.5). One closure sees
/// every slice, so the leg's own accounting locals stay owned by exactly
/// one execution path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateSlice {
    /// Between park slices (the D14.7/D14.5 park invariant's bounded
    /// wait): observe the output tail NOW and return its outcome —
    /// quiescent, still pending, or the observation itself failed
    /// ([`TailProbeOutcome`]; never answer a failed observation with
    /// quiescence or with an endless pending). The leg keeps its own
    /// D14.8 publication discipline on this slice.
    TailProbe,
    /// A routed seek release payload was consumed — exactly once — on
    /// this leg's path: apply the instruction (a committed rebase, or
    /// nothing for an abort) BEFORE any further submission. Delivered
    /// mid-park when the leg is parked by PAUSE, so a committed cut
    /// rebases a paused leg while it STAYS parked — the pause intent
    /// survives the seek and the rebase is bookkeeping, never a
    /// submission (the return value is ignored on this slice).
    SeekRelease(SeekParkRelease),
}

/// Session-owned render gate (ADR-PBK-002 D14.7 mechanism A + the D14.5
/// F5 seek hold): routes one episode's pause intent and seek holds into
/// a render mechanism's loop-top gate check and acknowledges
/// engagement / tail quiescence / disengagement back to the owner as
/// mechanism events — pause-attributed and cut-attributed (seek) events
/// kept structurally separate, so an internal seek park can never
/// fabricate `Paused` evidence. The separation is one-directional by
/// design: the D14.5 seek commit deliberately reads the physical
/// parked-and-quiesced conjunction under EITHER attribution, so a paused
/// episode's already-quiesced tail satisfies the output-cut
/// precondition (the frozen D14.5 pause interaction — a paused episode
/// is seekable and the seek never implicitly resumes).
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
/// a failed tail observation is neither quiescence nor a reason to keep
///     waiting: the gate publishes no quiescence for it, releases the
///     leg without further slices, and reports the failed probe back
///     (F5 implementation corrective-4) — the mechanism's existing
///     device-failure path decides; the gate itself still never aborts
///     the leg;
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
    /// The mechanism's view of the routed intent. Command truth lives
    /// with the episode owner; these fields are the routed copies the
    /// render leg observes. The pause flag, the seek-hold flag and the
    /// release payload share one lock deliberately: the render loop's
    /// ONE loop-top gate operation inspects all of them under a single
    /// acquisition, so a steady iteration of normal playback pays
    /// exactly ONE uncontended mutex acquisition and O(1) flag tests —
    /// the frozen D14.5 realtime row ("no new lock acquisition; the
    /// existing loop-top gate check gains one more seek-park flag
    /// test"), realized literally. No dispatch, no allocation.
    intent: Mutex<GateIntent>,
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

#[derive(Default)]
struct GateIntent {
    /// Routed pause intent (D14.7): `true` parks the leg at its next
    /// loop-top gate check.
    paused: bool,
    /// Routed seek hold (D14.5): `true` parks the leg at its next
    /// loop-top gate check under a cut-attributed park, structurally
    /// separate from any pause concept.
    seek_hold: bool,
    /// The release instruction for a seek park, stored by the session
    /// BEFORE it clears `seek_hold` (same lock hold), so a waking leg
    /// observes hold-clear and payload together. Consumed exactly once
    /// by the leg's seek gate — at the park's exit, or at its entry
    /// when the hold was already released before the leg arrived (one
    /// leg, one seek at a time; there is no second consumer).
    seek_release: Option<SeekParkRelease>,
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
                intent: Mutex::new(GateIntent::default()),
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
        let mut guard = self.inner.intent.lock().expect("render gate lock");
        if guard.paused == paused {
            return;
        }
        guard.paused = paused;
        drop(guard);
        self.inner.wake.notify_all();
    }

    /// Route a seek hold into the mechanism (ADR-PBK-002 D14.5): `true`
    /// parks the render leg at its next loop-top gate check under a
    /// cut-attributed park; `false` releases a seek-parked leg with no
    /// payload (an abort). Idempotent. Inert on a closed gate.
    ///
    /// Routing a NEW hold resets any release payload a previous cycle
    /// left unconsumed: one seek runs at a time (the session's one-seek
    /// policy), so a stale payload could only be debris.
    pub fn set_seek_hold(&self, held: bool) {
        if self.inner.closed.load(Ordering::Acquire) {
            return;
        }
        let mut guard = self.inner.intent.lock().expect("render gate lock");
        if guard.seek_hold == held {
            return;
        }
        guard.seek_hold = held;
        if held {
            guard.seek_release = None;
        }
        drop(guard);
        self.inner.wake.notify_all();
    }

    /// Release a routed seek hold with the session's decision. `release`
    /// is stored before the hold clears (one lock hold), so the waking
    /// leg observes hold-clear and payload together and the leg's seek
    /// gate consumes the payload exactly once (at the park exit, or at
    /// its entry if the leg had not parked). Wake is immediate (notify):
    /// the leg's park slice is the latency bound. Inert on a closed
    /// gate.
    pub fn release_seek_hold(&self, release: SeekParkRelease) {
        if self.inner.closed.load(Ordering::Acquire) {
            return;
        }
        let mut guard = self.inner.intent.lock().expect("render gate lock");
        guard.seek_release = Some(release);
        guard.seek_hold = false;
        drop(guard);
        self.inner.wake.notify_all();
    }

    /// Whether a routed seek release payload still awaits consumption.
    /// Session bookkeeping for the post-commit duty (the one-seek slot
    /// stays occupied until the leg has consumed the release, so no
    /// later hold can wipe a committed rebase) — not a hot-path call.
    pub fn seek_release_pending(&self) -> bool {
        let guard = self.inner.intent.lock().expect("render gate lock");
        guard.seek_release.is_some()
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

    /// The render mechanism's ONE loop-top gate operation (D14.7 park
    /// invariant + the D14.5 cut park, unified): check pause intent, seek
    /// hold and any routed release payload under a SINGLE intent-lock
    /// acquisition — strictly before the device-buffer acquisition, no
    /// device buffer held across a park — then run whatever is routed.
    ///
    /// Steady shape (the frozen D14.5 realtime row, realized literally
    /// since the F5 implementation corrective-1 unification): when
    /// nothing is routed, this is ONE uncontended mutex acquisition and
    /// O(1) flag tests — no second acquisition, no dispatch, no
    /// allocation. (The pre-unification spelling paid a second
    /// acquisition per steady iteration for the seek check; "no new lock
    /// acquisition" is a frozen proposition, not a representation
    /// detail.)
    ///
    /// Shapes, from that one acquisition:
    ///
    /// ```text
    /// nothing routed  → return immediately; no events, no consumption.
    /// payload only    → consume it exactly once (no park, no events)
    ///                   and hand it to the leg: the rebase instruction
    ///                   lands before any further submission even when
    ///                   the hold was already released before arrival.
    /// seek hold       → the cut-attributed park: publish only Seek*
    ///                   events, wait in bounded slices probing the tail,
    ///                   then consume the routed release (Aborted when
    ///                   none) and hand it to the leg.
    /// pause intent    → the pause-attributed park: publish only pause
    ///                   events, wait in bounded slices probing the tail.
    ///                   While parked, a routed seek release payload is
    ///                   consumed mid-park and handed to the leg — which
    ///                   STAYS PARKED: pause intent survives the seek,
    ///                   and a committed cut rebases a paused leg before
    ///                   its resume, as pure bookkeeping (the leg submits
    ///                   nothing while parked). A seek hold routed during
    ///                   a pause park therefore resolves under the pause
    ///                   attribution; no Seek* park events publish.
    /// ```
    ///
    /// After a pause park ends (resume/release), a hold routed in the
    /// meantime parks this same call — the leg cannot submit past an
    /// unresolved cut. Release never aborts the leg: every shape
    /// proceeds once more, and the data plane decides.
    ///
    /// Closed gate: returns immediately, always.
    ///
    /// A FAILED tail observation ends whichever park is running (the
    /// only bounded exit that is not an owner release): no quiescence
    /// is published for it, the park's disengagement fence still
    /// publishes, any routed release payload is still consumed on the
    /// leg's path, and [`ParkOutcome::TailProbeFailed`] is returned so
    /// the mechanism's existing device-failure path decides. After a
    /// pause park that failed its probe, a still-routed seek hold does
    /// NOT park again — more slices from a dead device can produce
    /// neither quiescence nor recovery.
    pub fn park_loop_top(
        &self,
        mut slice: impl FnMut(GateSlice) -> TailProbeOutcome,
    ) -> ParkOutcome {
        // The steady-path totality: ONE acquisition, O(1) tests.
        {
            let guard = self.inner.intent.lock().expect("render gate lock");
            if self.inner.closed.load(Ordering::Acquire)
                || (!guard.paused && !guard.seek_hold && guard.seek_release.is_none())
            {
                return ParkOutcome::Proceeded;
            }
        }
        // Routed work exists. The pause shape runs first (the loop's
        // frozen order: pause before seek); it stays parked through a
        // cut that commits while paused and consumes the payload
        // mid-park. The seek shape runs after, so a hold still routed
        // after the pause ended parks this same call. These extra
        // acquisitions exist only on the non-steady path.
        if self.routed_snapshot().0
            && let ParkOutcome::TailProbeFailed = self.pause_park(&mut slice)
        {
            return ParkOutcome::TailProbeFailed;
        }
        let (_, seek_hold, payload) = self.routed_snapshot();
        if seek_hold || payload {
            return self.seek_park(&mut slice);
        }
        ParkOutcome::Proceeded
    }

    /// One lock acquisition's view of what is routed: (pause, seek hold,
    /// pending release payload).
    fn routed_snapshot(&self) -> (bool, bool, bool) {
        let guard = self.inner.intent.lock().expect("render gate lock");
        (guard.paused, guard.seek_hold, guard.seek_release.is_some())
    }

    /// The pause-attributed park body (D14.7): parks while pause intent
    /// is routed, publishes Engaged / TailQuiesced (once) / Disengaged,
    /// and — the F5 implementation corrective-1 addition — consumes a
    /// routed seek release payload MID-PARK, handing it to the leg while
    /// it stays parked.
    fn pause_park(&self, slice: &mut impl FnMut(GateSlice) -> TailProbeOutcome) -> ParkOutcome {
        {
            let guard = self.inner.intent.lock().expect("render gate lock");
            if !guard.paused || self.inner.closed.load(Ordering::Acquire) {
                // Released or closed before the leg reached the gate:
                // nothing engaged, nothing to acknowledge.
                return ParkOutcome::Proceeded;
            }
        }
        self.emit(GateEvent::Engaged);
        let mut quiesced_published = false;
        let mut probe_failed = false;
        loop {
            // The payload joins the wait predicate: a commit routed
            // while the leg is parked wakes it immediately for the
            // mid-park consumption instead of at the slice cap.
            let (released, payload) = {
                let guard = self.inner.intent.lock().expect("render gate lock");
                let closed = &self.inner.closed;
                let (mut guard, _) = self
                    .inner
                    .wake
                    .wait_timeout_while(guard, PARK_SLICE, |intent| {
                        intent.paused
                            && !closed.load(Ordering::Acquire)
                            && intent.seek_release.is_none()
                    })
                    .expect("render gate wait poisoned");
                let payload = guard.seek_release.take();
                (!guard.paused || closed.load(Ordering::Acquire), payload)
            };
            if let Some(release) = payload {
                // Consumed on the leg's own path; the leg applies the
                // instruction and keeps parked (a rebase is bookkeeping,
                // never a submission).
                slice(GateSlice::SeekRelease(release));
            }
            if released {
                break;
            }
            if !quiesced_published {
                match slice(GateSlice::TailProbe) {
                    TailProbeOutcome::Quiesced => {
                        quiesced_published = true;
                        self.emit(GateEvent::TailQuiesced);
                    }
                    TailProbeOutcome::Pending => {}
                    TailProbeOutcome::Failed => {
                        probe_failed = true;
                        break;
                    }
                }
            }
        }
        self.emit(GateEvent::Disengaged);
        if probe_failed {
            // A routed payload may have missed the mid-park window as
            // the failure cut the wait short: deliver it on the way out
            // so the once-on-this-leg discipline holds on every exit —
            // the leg proceeds once more before its failure path runs.
            if let Some(release) = self.take_release_internal() {
                slice(GateSlice::SeekRelease(release));
            }
            return ParkOutcome::TailProbeFailed;
        }
        ParkOutcome::Proceeded
    }

    /// The seek-attributed park body (D14.5): parks while a seek hold is
    /// routed, publishes only Seek* events, and hands the routed release
    /// (Aborted when none) to the leg on its own path. A hold released
    /// before the leg arrived still has its payload consumed here — the
    /// rebase instruction belongs to this leg's next submission decision.
    fn seek_park(&self, slice: &mut impl FnMut(GateSlice) -> TailProbeOutcome) -> ParkOutcome {
        {
            let mut guard = self.inner.intent.lock().expect("render gate lock");
            if self.inner.closed.load(Ordering::Acquire) {
                return ParkOutcome::Proceeded;
            }
            if !guard.seek_hold {
                // The hold was already released before this leg reached
                // the gate: whatever release was routed still belongs to
                // THIS leg's next submission decision (the session
                // routes it for this episode's one seek). Consume it
                // here — no park happened, no park events publish, and
                // the rebase still lands before any further submission.
                let release = guard.seek_release.take();
                drop(guard);
                if let Some(release) = release {
                    slice(GateSlice::SeekRelease(release));
                }
                return ParkOutcome::Proceeded;
            }
        }
        self.emit(GateEvent::SeekEngaged);
        let mut quiesced_published = false;
        let mut probe_failed = false;
        loop {
            let released = {
                let guard = self.inner.intent.lock().expect("render gate lock");
                let closed = &self.inner.closed;
                let (guard, _) = self
                    .inner
                    .wake
                    .wait_timeout_while(guard, PARK_SLICE, |intent| {
                        intent.seek_hold && !closed.load(Ordering::Acquire)
                    })
                    .expect("render gate wait poisoned");
                !guard.seek_hold || closed.load(Ordering::Acquire)
            };
            if released {
                break;
            }
            if !quiesced_published {
                match slice(GateSlice::TailProbe) {
                    TailProbeOutcome::Quiesced => {
                        quiesced_published = true;
                        self.emit(GateEvent::SeekTailQuiesced);
                    }
                    TailProbeOutcome::Pending => {}
                    TailProbeOutcome::Failed => {
                        probe_failed = true;
                        break;
                    }
                }
            }
        }
        // ONE exit funnel — the failure exit shares it: the fence
        // publishes, and the leg's next submission decision still sees
        // the routed release (Aborted when the session routed none).
        self.emit(GateEvent::SeekDisengaged);
        let release = self
            .take_release_internal()
            .unwrap_or(SeekParkRelease::Aborted);
        slice(GateSlice::SeekRelease(release));
        if probe_failed {
            return ParkOutcome::TailProbeFailed;
        }
        ParkOutcome::Proceeded
    }

    /// Consume a pending release payload under the intent lock (the
    /// internal arm of the take-once consumption contract).
    fn take_release_internal(&self) -> Option<SeekParkRelease> {
        self.inner
            .intent
            .lock()
            .expect("render gate lock")
            .seek_release
            .take()
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

    /// Writer: rebase the cell at one committed seek cutover
    /// (ADR-PBK-002 D14.5, same-cell writer-side discontinuity). A plain
    /// store — the ONE legal backward step, exactly once per committed
    /// cutover — performed by the render leg on its own execution path
    /// BEFORE any post-cut submission or publication, so pre- and
    /// post-cutover accounting can never mix. After this store the
    /// monotone `publish_consumed` rule holds again within the new
    /// stretch (the store is the stretch boundary).
    ///
    /// `Some(landing)` stores the decoder's reported actual landing in
    /// source PCM frames as the new basis. `None` means the landing is
    /// unknown: the cell is returned to the undefined sentinel, and the
    /// writer publishes nothing for the rest of the episode — unknown
    /// stays unknown, never zero and never the requested target.
    ///
    /// There is exactly one writer per episode (the render leg), and the
    /// frozen protocol guarantees no post-cut publication can precede
    /// this store on that writer's path, so no ordering beyond relaxed
    /// coherence is owed.
    pub fn rebase(&self, landing: Option<u64>) {
        let encoded = match landing {
            Some(landing) => landing.saturating_add(1),
            // The undefined sentinel: the projection reads `None` again,
            // and a writer that knows the basis is gone never publishes.
            None => 0,
        };
        self.published.store(encoded, Ordering::Relaxed);
    }
}

/// Session-owned output-level cell (ADR-PBK-002 D14.9): carries ONE
/// episode's desired stream factor from the application's routed command
/// to the render mechanism. Truth class: application configuration in
/// transit (Command family, like the pause intent the gate routes) —
/// NOT a Fact, NOT mechanism evidence about loudness, NOT a playback
/// semantic. The cell is an owned resource of the episode (like the
/// gate and the position cell), never a Capability.
///
/// Writers: the session (routing the episode seam's idempotent command)
/// and the initial value the App configured before activation. Reader:
/// the render mechanism — once at stream open (before first meaningful
/// submission) and once per loop top when the routed value changed (one
/// relaxed load + compare; V-PROBE-grounded placement). Never read by
/// the product read side: D14.9 forbids a mechanism readback
/// (GetAllVolumes stays unexposed); the displayed value is the App's
/// own desired level, not this cell and not any acoustic truth.
#[derive(Clone, Debug)]
pub struct OutputLevel {
    /// The desired factor as f32 bits (0.0 = silent, 1.0 = unity).
    /// Relaxed coherence suffices: one writer-routed value, one
    /// mechanism reader, and the apply placement is bounded by design —
    /// a stale-by-one-iteration factor costs nothing (the next loop top
    /// re-checks).
    factor_bits: Arc<AtomicU32>,
}

impl Default for OutputLevel {
    fn default() -> Self {
        Self {
            factor_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
        }
    }
}

impl OutputLevel {
    /// The cell every episode starts with: unity (no attenuation). A
    /// fresh episode sounds at the App's routed level because the App
    /// routes BEFORE activation, not because the mechanism guesses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Route the desired factor (the session, from the episode seam's
    /// idempotent command). The mechanism applies it at its loop top.
    pub fn route(&self, factor: f32) {
        let factor = factor.clamp(0.0, 1.0);
        self.factor_bits.store(factor.to_bits(), Ordering::Relaxed);
    }

    /// One mechanism-side read (stream open + loop top).
    pub fn load(&self) -> f32 {
        f32::from_bits(self.factor_bits.load(Ordering::Relaxed))
    }
}

/// Request for one playback-specific render stream: the source format to
/// negotiate, the pre-bound PCM input, the session-owned drain signal,
/// the session-owned pause gate, the session-owned position-evidence
/// cell, and the session-owned output-level cell (D14.9). All data-plane
/// pieces bind once, here.
pub struct RenderRequest {
    pub format: PcmFormat,
    pub input: Arc<dyn RenderPcmInput>,
    pub drain: DrainSignal,
    pub gate: RenderGate,
    /// The episode's output-level cell (ADR-PBK-002 D14.9): the desired
    /// stream factor the mechanism applies — once at stream open (before
    /// first meaningful submission) and re-applied at the render loop top
    /// when the routed value changed (one relaxed load + compare per
    /// iteration; never inside the quantum). Grounded by V-PROBE
    /// (experiments/v-probe, 2026-09-19): factor isolation and
    /// independence, loop-top apply boundedness, position clock
    /// advancing and monotone under applies.
    pub level: OutputLevel,
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
    ///
    /// Ownership precondition (D14.7 teardown obligation): before this
    /// call, the owning playback/session teardown path must already
    /// have released every session-owned render-gate hold (pause
    /// intent, seek hold) the render leg needs to reach its exit — a
    /// leg parked at the gate is not inside its data-plane read, so a
    /// data-plane stop alone cannot wake it and the join would never
    /// return. The backend must not fabricate pause/seek release
    /// intent; the gate belongs to the session.
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

/// What one stateless source-probe query reports (ADR-PBK-002 D14.6, the
/// F6-AUTHORITY-PROMOTION-1 amendment): the container's declared facts at
/// probe time — format and optional duration — read WITHOUT producing a
/// single PCM frame. Truth class: mechanism evidence for an application
/// composition decision (an Open preflight); advisory, never episode
/// truth — the episode activation's own open/probe publishes the
/// authoritative source evidence. `duration == None` means the container
/// declared none (unknown stays unknown, never zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFacts {
    pub format: PcmFormat,
    pub duration: Option<Duration>,
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

/// The three-class semantic outcome of a provider seek
/// (ADR-PBK-002 D14.5, F5-GATE-CORRECTIVE-1). The classification is the
/// PROVIDER's contractual duty: a caller must not infer it from raw
/// status names, because the same status code can cover provably
/// distinct phases (the SongCore ABI returns generic SEEK_ERROR both
/// from a failed reposition and again after a successful reposition +
/// decoder flush).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderSeekOutcome {
    /// The provider repositioned its decoder. `landing` is the actual
    /// landing position in source PCM frames — the retained-PCM start
    /// the NEXT read belongs to — or `None` when the provider cannot
    /// determine it (explicitly unknown, never manufactured; the
    /// Position projection is withdrawn for the episode, never replaced
    /// by the requested target).
    Applied { landing: Option<u64> },
    /// Proven pre-mutation refusal: the provider guarantees the pre-call
    /// decoding continuation remains valid — the rejection happened in
    /// pure parameter/state validation BEFORE any decoder or demuxer
    /// state could change. The caller may finish its in-flight staging
    /// exactly and resume the old cursor with zero content loss.
    RefusedUnchanged,
    /// Any failure NOT provably pre-mutation. The decoding continuation
    /// is not guaranteed (the reposition may already have happened and
    /// been flushed); the caller must NEVER resume old-cursor
    /// production. `diagnostic` is presentation text.
    MutatedThenFailed { diagnostic: String },
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

    /// Reposition this endpoint to `target` — source-relative media
    /// time (ADR-PBK-002 D14.5; the F5 seek command path). Validity and
    /// clamping are the PROVIDER's decision (duration evidence is never
    /// consulted by the caller); after an [`ProviderSeekOutcome::Applied`]
    /// outcome the next `read_frames` belongs to the reported landing.
    ///
    /// The three-class outcome is this contract's whole point: the
    /// caller may resume its old cursor ONLY on
    /// [`ProviderSeekOutcome::RefusedUnchanged`], whose guarantee — the
    /// pre-call decoding continuation remains valid — is proven by the
    /// provider's own classification, never inferred from a status name.
    fn seek(&mut self, target: Duration) -> ProviderSeekOutcome;
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
