//! Truthful text projection of one playback-episode observation (F2).
//!
//! This module is presentation only: it renders a
//! `PlaybackSessionObservation` — the coherent pure read the playback
//! seam hands the application — into stable, scriptable lines. It owns
//! no truth of its own and reads no mechanism state (no K0 snapshot, no
//! logs, no PcmEdge/DrainSignal/worker/WASAPI/SongCore internals).
//!
//! Truth-class discipline (D14.2):
//! `pending` states only "no terminal Fact committed yet" — never
//! Playing/Starting/Paused/Stopping; `stop_requested` is Command state;
//! `format` is mechanism evidence; `activation_error` is a diagnostic.
//! The forbidden-vocabulary oracle below pins that boundary.

use qianqian_playback::{PlaybackSessionObservation, SessionOutcome};

/// Render one observation as the `status` output: stable,
/// scriptable, truth-class correct.
pub fn format_status(observation: &PlaybackSessionObservation) -> String {
    let mut out = String::new();
    match &observation.terminal_outcome {
        None => out.push_str("outcome: pending\n"),
        Some(SessionOutcome::Completed) => out.push_str("outcome: completed\n"),
        Some(SessionOutcome::Stopped) => out.push_str("outcome: stopped\n"),
        Some(SessionOutcome::Failed { stage }) => {
            out.push_str("outcome: failed\n");
            // Stage text is a diagnostic (D14.2), printed as such and
            // never frozen into the semantic outcome vocabulary.
            out.push_str("failure: ");
            out.push_str(stage);
            out.push('\n');
        }
    }
    match &observation.source_format {
        Some(format) => {
            out.push_str(&format!(
                "format: {} Hz, {} channels, mask {:#x}\n",
                format.sample_rate, format.channels, format.channel_mask
            ));
        }
        None => out.push_str("format: unavailable\n"),
    }
    out.push_str(&format!("stop_requested: {}\n", observation.stop_requested));
    if let Some(error) = &observation.activation_error {
        out.push_str("activation_error: ");
        out.push_str(error);
        out.push('\n');
    }
    out
}

/// Playback semantics the status projection must never claim (F2
/// negative-control vocabulary). Matches are word-ish to avoid tripping
/// on substrings of unrelated diagnostics.
const FORBIDDEN_STATUS_WORDS: [&str; 7] = [
    "playing",
    "starting",
    "paused",
    "pausing",
    "stopping",
    "buffering",
    "buffered",
];

/// Negative-control scan (T13): the first forbidden playback-semantics
/// word appearing in a status projection, if any. `None` = clean.
pub fn forbidden_status_claim(text: &str) -> Option<&'static str> {
    let lowered = text.to_ascii_lowercase();
    FORBIDDEN_STATUS_WORDS
        .into_iter()
        .find(|word| lowered.contains(word))
}
