//! Product-facing rendering of the session's truthful read-side
//! observation (F2). This module is presentation only: it formats one
//! [`SessionObservation`] handed over by the Playback Session's
//! read-side seam and adds no semantics of its own — in particular it
//! never labels pending as anything more than "no terminal outcome
//! committed yet", and it exposes no mechanism internals.

use qianqian_playback::{SessionObservation, SessionOutcome};

/// Render one status observation as stable, scriptable lines. The exact
/// pending/completed/stopped blocks are pinned by
/// `tests/status_format.rs`; the conditional failure/activation lines
/// are pinned by containment there. One observation never spans
/// multiple lines except for its own block structure, so line-based
/// parsing stays reliable:
///
/// ```text
/// outcome: pending | completed | stopped | failed
/// failure: {stage}              (failed only; class diagnostic)
/// format: {rate} Hz, {channels} channels, mask 0x… | unavailable
/// stop_requested: true | false
/// activation_error: {message}   (only when present)
/// ```
///
/// Truth classes travel with the fields (see [`SessionObservation`]):
/// `outcome` is the session authority's committed semantic fact;
/// `format` is a write-once mechanism readback, not source identity;
/// `stop_requested` is command state, not a Stopping fact;
/// `activation_error` is an activation-attempt diagnostic, not an
/// episode outcome.
pub fn format_status(observation: &SessionObservation) -> String {
    let mut lines = Vec::new();
    match &observation.outcome {
        None => lines.push("outcome: pending".to_owned()),
        Some(SessionOutcome::Completed) => lines.push("outcome: completed".to_owned()),
        Some(SessionOutcome::Stopped) => lines.push("outcome: stopped".to_owned()),
        Some(SessionOutcome::Failed { stage }) => {
            lines.push("outcome: failed".to_owned());
            // A stage diagnostic is representation, not frozen truth
            // (ADR-PBK-002 §17/D11); flattening embedded newlines keeps
            // the status block line-parseable.
            let flat = stage.replace('\n', " ");
            lines.push(format!("failure: {flat}"));
        }
    }
    match observation.source_format {
        Some(format) => lines.push(format!(
            "format: {} Hz, {} channels, mask {:#x}",
            format.sample_rate, format.channels, format.channel_mask
        )),
        None => lines.push("format: unavailable".to_owned()),
    }
    lines.push(format!("stop_requested: {}", observation.stop_requested));
    if let Some(message) = &observation.activation_error {
        lines.push(format!("activation_error: {message}"));
    }
    lines.join("\n")
}
