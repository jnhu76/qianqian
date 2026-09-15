//! The application-facing episode seam (F2, reality-gate-2 verdict B;
//! ADR-PBK-002 D14.2). One handle references exactly one playback
//! episode and gives the application — headless today, a future UI
//! adapter tomorrow — exactly three rights:
//!
//! ```text
//! request_stop()   record stop intent (Command, not a Fact)
//! observe()        one coherent pure read of the episode
//! wait_terminal()  pure blocking wait for the committed terminal Fact
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
//! terminal_outcome   Fact; None means only "no terminal Fact committed
//!                    yet" — it is not Playing/Starting/Paused or any
//!                    fourth outcome
//! stop_requested     Command state; true does not mean Stopping
//! source_format      mechanism evidence, not source identity and not a
//!                    playback semantic state
//! activation_error   activation diagnostic; never a terminal Failed
//! ```
//!
//! Fields the current architecture has not earned (Playing/Starting/
//! Paused/Stopping, position/duration, buffer health, source identity,
//! K0 FiberState) are structurally absent.

use qianqian_audio_api::ports::PcmFormat;

use crate::completion::{SessionCompletion, SessionOutcome};

/// One coherent observation of one playback episode, snapshot under a
/// single lock so every field value coexisted at one real instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackSessionObservation {
    /// The committed terminal outcome, or `None` while no terminal Fact
    /// has been committed yet (pending).
    pub terminal_outcome: Option<SessionOutcome>,
    /// Whether stop intent has been recorded. Command state, not
    /// outcome truth.
    pub stop_requested: bool,
    /// The episode's source PCM format once activation published it.
    /// Mechanism evidence.
    pub source_format: Option<PcmFormat>,
    /// Why activation raised, if it did. Diagnostic; an episode that
    /// never started has no terminal Fact and must not be forged into
    /// `Failed`.
    pub activation_error: Option<String>,
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

    /// One coherent observation of the episode. Pure read: no resolve,
    /// no commit, no lifecycle action, no edge or drain operation.
    /// Repeating it changes nothing.
    pub fn observe(&self) -> PlaybackSessionObservation {
        self.completion.observe_snapshot()
    }

    /// Block until the episode's terminal Fact is committed, then return
    /// it. Pure wait: settlement is authority-owned and is never
    /// triggered or advanced by this call.
    pub fn wait_terminal(&self) -> SessionOutcome {
        self.completion.wait_terminal()
    }
}
