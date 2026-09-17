//! The application-facing episode seam (F2, reality-gate-2 verdict B;
//! ADR-PBK-002 D14.2). One handle references exactly one playback
//! episode and gives the application — headless today, a future UI
//! adapter tomorrow — exactly these rights:
//!
//! ```text
//! request_stop()     record stop intent (Command, not a Fact)
//! request_pause()    record pause intent (Command, not a Fact; D14.7)
//! request_resume()   release a recorded pause intent (Command)
//! observe()          one coherent pure read of the episode
//! wait_terminal()    pure blocking wait for the committed terminal Fact
//! ```
//!
//! The handle is NOT a Plugin, Capability, K0 primitive, global store or
//! second lifecycle owner, and it carries no mechanism rights: terminal
//! evidence publication and D11 settlement stay on session-owned paths
//! behind the crate boundary ([`crate::completion`]). Nothing the
//! application can call here can create or relabel the terminal Fact.
//!
//! `PlaybackSessionObservation` keeps the truth classes of its fields
//! explicit and separate:
//!
//! ```text
//! terminal_outcome    Fact; None means only "no terminal Fact committed
//!                     yet" — it is not Playing/Starting/Paused or any
//!                     fourth outcome
//! failure_diagnostic  presentation text for a Failed outcome; never part
//!                     of the semantic contract
//! stop_requested      Command state; true does not mean Stopping
//! pause_requested     Command state (D14.7); true does not mean Paused
//! source_format       mechanism evidence, not source identity and not a
//!                     playback semantic state
//! pause_engagement    mechanism evidence: the CURRENT pause
//!                     engagement's render-gate / output-tail state
//! pause_disengaged_observed  mechanism evidence latch: the render
//!                     leg's most recent engagement has disengaged
//! activation_error    activation diagnostic; never a terminal Failed
//! ```
//!
//! `paused()` and `resumed()` are the D14.7 derived Projections over
//! those fields. They are derived visibility only: never correctness
//! bases for resume/stop legality, teardown, terminal settlement,
//! resource lifetime, mechanism wakeup, or K0 lifecycle transitions.
//!
//! Fields the current architecture has not earned (Playing/Starting/
//! Stopping, position/duration, buffer health, source identity, K0
//! FiberState) are structurally absent.

use qianqian_audio_api::ports::PcmFormat;

use crate::completion::{SessionCompletion, SessionOutcome};

/// The stable semantic terminal outcome of one playback episode
/// (ADR-PBK-002 D14.2): exactly these three variants and nothing else.
///
/// Diagnostics — which leg failed and why — are deliberately NOT part of
/// this contract. They are carried separately as
/// [`PlaybackSessionObservation::failure_diagnostic`] and may change
/// freely; no semantic subclass (decode-failed, device-failed, …) is
/// representable in or derivable from this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeTerminalOutcome {
    /// EOF was produced, drained and played out.
    Completed,
    /// Stop intent was recorded at the decisive evidence boundary and no
    /// higher-precedence failure classification won.
    Stopped,
    /// The episode failed before completion. The reason is diagnostic
    /// only: read `failure_diagnostic` for presentation, never for
    /// semantic dispatch.
    Failed,
}

/// The mechanism-evidence state of an episode's CURRENT pause
/// engagement (ADR-PBK-002 D14.7). This is the observation spelling of
/// the engagement / output-tail-quiescence evidence latches — mechanism
/// evidence, not a semantic transport state, and never a correctness
/// basis for anything. The derived Paused projection requires
/// [`PauseEngagement::TailQuiesced`]; a previous pause cycle's
/// quiescence never satisfies a later pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseEngagement {
    /// No engagement is latched: the episode never paused, or its most
    /// recent pause engagement was released.
    Disengaged,
    /// The render leg is parked at the pre-GetBuffer pause gate, but
    /// this engagement's output tail has not been observed quiesced yet
    /// (frames submitted before engagement may still be queued to play).
    Engaged,
    /// Engaged, and the CURRENT engagement's output tail was observed
    /// quiesced: no frame submitted before engagement remains queued
    /// for rendering by this output session.
    TailQuiesced,
}

/// One coherent observation of one playback episode, snapshot under a
/// single lock so every field value coexisted at one real instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackSessionObservation {
    /// The committed terminal outcome, or `None` while no terminal Fact
    /// has been committed yet (pending).
    pub terminal_outcome: Option<EpisodeTerminalOutcome>,
    /// Why the episode failed, when it failed and a diagnostic was
    /// published. Presentation text, NOT part of the semantic contract:
    /// `Completed`/`Stopped` observations never carry one, and a `Failed`
    /// observation may or may not. Its presence, absence or spelling may
    /// change without a semantic change.
    pub failure_diagnostic: Option<String>,
    /// Whether stop intent has been recorded. Command state, not
    /// outcome truth.
    pub stop_requested: bool,
    /// Whether pause intent has been recorded (D14.7). Command state,
    /// not playback truth: true does not mean Paused.
    pub pause_requested: bool,
    /// The episode's source PCM format once activation published it.
    /// Mechanism evidence.
    pub source_format: Option<PcmFormat>,
    /// The current pause engagement's mechanism-evidence state
    /// (D14.7). Evidence, not a semantic transport state. Meaningful
    /// only while `terminal_outcome` is `None`: publication is
    /// first-wins and closes at settlement, so after a terminal Fact
    /// the latched spelling may outlive the leg — the `paused`/
    /// `resumed` projections guard on the unsettled state, and any
    /// other consumer of this field must too.
    pub pause_engagement: PauseEngagement,
    /// Whether the render leg's most recent engagement has disengaged
    /// (mechanism-evidence latch for the Resumed projection only).
    /// Attribution is exact at engagement granularity — the leg's
    /// `Engaged` event is the current-engagement fence, so a previous
    /// pause cycle's disengagement can never establish `Resumed` for a
    /// later cycle. Symmetric to the current-engagement tail-quiescence
    /// discipline. The frozen claim: `resumed()` is true only while the
    /// gate is released and render submission is re-enabled.
    pub pause_disengaged_observed: bool,
    /// Why activation raised, if it did. Diagnostic; an episode that
    /// never started has no terminal Fact and must not be forged into
    /// `Failed`.
    pub activation_error: Option<String>,
}

impl PlaybackSessionObservation {
    /// The Paused projection (D14.7): the episode is unsettled, pause
    /// intent is recorded, the render leg is engaged at the gate, AND the
    /// current engagement's output tail was observed quiesced. A derived
    /// projection only — never a correctness basis for control,
    /// lifetime, settlement or K0 lifecycle.
    pub fn paused(&self) -> bool {
        self.terminal_outcome.is_none()
            && self.pause_requested
            && self.pause_engagement == PauseEngagement::TailQuiesced
    }

    /// The Resumed projection (D14.7): the episode is unsettled, pause
    /// intent is released, and the leg's most recent engagement has
    /// disengaged. Claims exactly that pause control is no longer
    /// established and render submission is re-enabled — NOT that new
    /// audio is already audible.
    pub fn resumed(&self) -> bool {
        self.terminal_outcome.is_none() && !self.pause_requested && self.pause_disengaged_observed
    }
}

/// The public application handle to one playback episode: the reference
/// the App / headless / a future UI adapter holds while the episode is
/// live. Clones reference the same episode; one handle serves exactly
/// one episode (do not re-use across a retried or restarted episode).
#[derive(Clone)]
pub struct PlaybackSessionHandle {
    pub(crate) completion: SessionCompletion,
}

impl Default for PlaybackSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackSessionHandle {
    /// A handle for one not-yet-activated episode. Pass it to
    /// [`crate::playback_session_spec`] when installing the session.
    pub fn new() -> Self {
        Self {
            completion: SessionCompletion::new(),
        }
    }

    /// Request the episode to stop. Command only: records stop intent
    /// and routes it to the session-owned data-plane stop. Idempotent
    /// and monotone; it never relabels an already-committed outcome.
    pub fn request_stop(&self) {
        self.completion.request_stop();
    }

    /// Request the episode to pause (ADR-PBK-002 D14.7). Command only:
    /// records pause intent and routes it to the episode's render gate;
    /// the mechanism parks its render leg before any device buffer is
    /// held. Same-episode, non-terminal: this never settles an outcome.
    /// Whether the episode actually establishes `paused()` is mechanism
    /// evidence the observation derives. Idempotent; inert history after
    /// settlement.
    pub fn request_pause(&self) {
        self.completion.request_pause();
    }

    /// Release a recorded pause (D14.7). Command only: clears pause
    /// intent and releases the gate; the render leg proceeds and the
    /// data plane decides what its next read sees. Idempotent; inert
    /// history after settlement.
    pub fn request_resume(&self) {
        self.completion.request_resume();
    }

    /// One coherent observation of the episode. Pure read: no resolve,
    /// no commit, no lifecycle action, no edge or drain operation.
    /// Repeating it changes nothing.
    pub fn observe(&self) -> PlaybackSessionObservation {
        self.completion.observe_snapshot()
    }

    /// Block until the episode's terminal Fact is committed, then return
    /// its stable semantic outcome. Pure wait: settlement is
    /// authority-owned and is never triggered or advanced by this call.
    /// A failure diagnostic, if any, is read separately through
    /// [`PlaybackSessionHandle::observe`]; it is presentation, not
    /// semantics.
    pub fn wait_terminal(&self) -> EpisodeTerminalOutcome {
        let outcome: SessionOutcome = self.completion.wait_terminal();
        outcome.split().0
    }
}
