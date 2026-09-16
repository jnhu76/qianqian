//! Corrected D14.7 establishment shape (F3-GATE-CORRECTIVE-1).
//!
//! The pre-corrective D14.7 proposal equated render-gate engagement
//! with product-level `Paused`, but mechanism A's own physical evidence
//! proves that already-submitted device audio keeps playing after
//! engagement until the WASAPI shared-mode padding drains. The
//! corrected establishment therefore adds a second mechanism-evidence
//! factor — output-tail quiescence — and demarcates `Paused` /
//! `Resumed` as application-facing derived Projections (PBK-001 §2.3),
//! never semantic Facts and never a correctness basis for control or
//! lifetime decisions.
//!
//! Truth classes (corrected D14.7):
//!
//! ```text
//! pause intent      Command state
//! engagement        Mechanism Evidence (render leg parked at the gate)
//! tail quiescence   Mechanism Evidence (no pre-engagement frame remains
//!                   queued to play in this stream's endpoint buffer;
//!                   observed as GetCurrentPadding() == 0 after
//!                   engagement, with no submission in between)
//! Paused/Resumed    derived Projection over the above — NOT a Fact
//! ```

/// The establishment inputs, exactly as the corrected D14.7 defines
/// them. Evidence-shape only: this mirrors the observation contract,
/// it is not a proposed product type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EstablishmentInputs {
    /// A D11 terminal outcome has committed on the episode (settled
    /// episodes are never Paused, regardless of latched engagement).
    pub settled: bool,
    /// Pause intent recorded (Command state).
    pub pause_intent: bool,
    /// Render engagement evidence: the render leg reached the
    /// pre-GetBuffer gate and parked.
    pub engaged: bool,
    /// Output-tail quiescence evidence: a post-engagement observation
    /// that no previously submitted frame remains queued to play
    /// (WASAPI shared-mode `GetCurrentPadding() == 0`), with no frame
    /// submission between engagement and the observation. Under the
    /// frozen mechanism-A shape the parked leg submits nothing, so one
    /// zero observation holds for the remainder of the park.
    pub tail_quiesced: bool,
}

/// The corrected D14.7 `Paused` projection: application-facing derived
/// visibility over command state + both mechanism-evidence factors.
/// NOT a semantic Fact, NOT control/lifetime correctness authority.
pub fn paused_projection(i: EstablishmentInputs) -> bool {
    !i.settled && i.pause_intent && i.engaged && i.tail_quiesced
}

/// The corrected D14.7 `Resumed` projection: pause control is no longer
/// established and render submission is re-enabled. It does NOT claim
/// new audio is already audible (refilled frames still traverse the
/// device buffer; position semantics remain F4/D14.8 territory).
pub fn resumed_projection(i: EstablishmentInputs) -> bool {
    !i.settled && !i.pause_intent && !i.engaged
}

/// The PRE-CORRECTIVE conjunction (engagement alone). Kept ONLY as the
/// negative control's mutation subject: the scenario suite proves this
/// definition goes RED in the exact state human review identified
/// (engaged, intent, unsettled, padding still > 0). Not a definition
/// anyone should adopt.
pub fn paused_projection_mutated_engagement_only(i: EstablishmentInputs) -> bool {
    !i.settled && i.pause_intent && i.engaged
}
