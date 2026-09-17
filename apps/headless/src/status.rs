//! Truthful text projection of one playback-episode observation (F2).
//!
//! This module is presentation only: it renders a
//! `PlaybackSessionObservation` — the coherent pure read the playback
//! seam hands the application — into stable, scriptable lines. It owns
//! no truth of its own and reads no mechanism state (no K0 snapshot, no
//! logs, no PcmEdge/DrainSignal/RenderGate/worker/WASAPI/SongCore
//! internals).
//!
//! Truth-class discipline (D14.2, D14.7, D14.8): `pending` states only
//! "no terminal Fact committed yet" — never Playing/Starting/Stopping;
//! `stop_requested`/`pause_requested` are Command state; `format` and
//! `duration` are mechanism evidence; `position` is the seam's derived
//! Projection; the `paused` line is the D14.7 establishment projection
//! derived by the seam itself; `activation_error` is a diagnostic. The
//! forbidden-vocabulary oracle below pins that boundary.

use std::time::Duration;

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
    // The F4 timeline (D14.8) on one line: the position Projection over
    // the source duration evidence, each side independently absent when
    // its evidence is — never a fabricated zero.
    out.push_str("position: ");
    out.push_str(&format_timeline(observation));
    out.push('\n');
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

/// The F4 timeline projection (D14.8) as one display string:
/// `00:42 / 03:58`, with `--:--` for a side whose evidence does not
/// exist.
///
/// Presentation only, and deliberately thin:
///
/// ```text
/// left   the Position Projection — source frames divided by the
///        published source sample rate. Unknown (never guessed) when
///        either the sample or the format is absent.
/// right  the source duration Mechanism Evidence as reported. It is NOT
///        exact and is never adjusted to agree with the left side.
/// ```
///
/// The two sides are independent evidence, so all four combinations are
/// reachable: a position with no known duration, a duration with no
/// sample yet, and both absent. Nothing here is a playback semantic, and
/// nothing here claims the position is audible — it is the
/// device-consumed presentation location.
///
/// The position conversion is `frames / source sample rate`, truncated
/// to whole seconds (the display never rounds a position up past the
/// evidence). It yields `--:--` when there is no published sample (the
/// projection is absent — unknown, not zero), when no source format was
/// published, or in the structurally impossible zero-rate case, which
/// fails closed rather than dividing by a guessed rate.
pub fn format_timeline(observation: &PlaybackSessionObservation) -> String {
    let position_time = observation
        .position
        .zip(observation.source_format.map(|format| format.sample_rate))
        .and_then(|(frames, sample_rate)| match sample_rate {
            0 => None,
            rate => Some(Duration::from_secs(frames / u64::from(rate))),
        });
    let position = position_time
        .map(format_clock)
        .unwrap_or_else(|| "--:--".to_owned());
    let duration = observation
        .source_duration
        .map(format_clock)
        .unwrap_or_else(|| "--:--".to_owned());
    format!("{position} / {duration}")
}

/// `mm:ss` (minutes not zero-padded beyond two digits, so an hour-long
/// track reads `63:20` rather than wrapping). Negative values cannot
/// occur: both inputs are unsigned durations.
fn format_clock(seconds: Duration) -> String {
    let seconds = seconds.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// Playback semantics the status projection must never claim (F2
/// negative-control vocabulary). Matches are word-ish to avoid tripping
/// on substrings of unrelated diagnostics.
///
/// `paused` left this list when D14.7 froze the pause establishment
/// (F3): the `paused:` line is now an earned, seam-derived projection,
/// pinned to the frozen establishment conjunction by the status and
/// TUI model tests. `resumed` entered it when the D14.7
/// AUTHORITY-CORRECTIVE removed the Resumed product projection:
/// disengagement evidence is never a user-facing transport claim, so a
/// `resumed` status line is unearned vocabulary again. The remaining
/// words name states no current
/// authority has earned.
const FORBIDDEN_STATUS_WORDS: [&str; 7] = [
    "playing",
    "starting",
    "pausing",
    "stopping",
    "resumed",
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
