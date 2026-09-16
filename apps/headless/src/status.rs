//! Truthful text projection of one playback-episode observation (F2).
//!
//! This module is presentation only: it renders a
//! `PlaybackSessionObservation` — the coherent pure read the playback
//! seam hands the application — into stable, scriptable lines. It owns
//! no truth of its own and reads no mechanism state (no K0 snapshot, no
//! logs, no PcmEdge/DrainSignal/RenderGate/worker/WASAPI/SongCore
//! internals).
//!
//! Truth-class discipline (D14.2, D14.7): `pending` states only "no
//! terminal Fact committed yet" — never Playing/Starting/Stopping;
//! `stop_requested`/`pause_requested` are Command state; `format` is
//! mechanism evidence; the `paused` line is the D14.7 establishment
//! projection derived by the seam itself; `activation_error` is a
//! diagnostic. The forbidden-vocabulary oracle below pins that
//! boundary.

use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionObservation};

/// Render one observation as the `status` output: stable,
/// scriptable, truth-class correct.
pub fn format_status(observation: &PlaybackSessionObservation) -> String {
    let mut out = String::new();
    // The semantic line names ONLY the stable terminal vocabulary
    // (D14.2): completed / stopped / failed. No diagnostic text and no
    // failure subclass may appear here — callers cannot infer
    // DecodeFailed/DeviceFailed from the projection's first line.
    match observation.terminal_outcome {
        None => out.push_str("outcome: pending\n"),
        Some(EpisodeTerminalOutcome::Completed) => out.push_str("outcome: completed\n"),
        Some(EpisodeTerminalOutcome::Stopped) => out.push_str("outcome: stopped\n"),
        Some(EpisodeTerminalOutcome::Failed) => out.push_str("outcome: failed\n"),
    }
    // The failure diagnostic is a separate presentation line, printed
    // only when one was published. Its presence, absence or wording is
    // never part of the semantic outcome: a Failed fact is Failed with
    // or without it.
    if let Some(failure) = &observation.failure_diagnostic {
        out.push_str("failure: ");
        out.push_str(failure);
        out.push('\n');
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
    out.push_str(&format!(
        "pause_requested: {}\n",
        observation.pause_requested
    ));
    // The D14.7 establishment projection, derived by the seam (unsettled
    // ∧ pause intent ∧ engagement ∧ current output-tail quiescence).
    out.push_str(&format!("paused: {}\n", observation.paused()));
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
///
/// `paused` left this list when D14.7 froze the pause establishment
/// (F3): the `paused:` line is now an earned, seam-derived projection,
/// pinned to the frozen establishment conjunction by the status and
/// TUI model tests. The remaining words name states no current
/// authority has earned.
const FORBIDDEN_STATUS_WORDS: [&str; 6] = [
    "playing",
    "starting",
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
