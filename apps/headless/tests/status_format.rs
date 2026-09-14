//! F2 status output contract tests: the headless `status` command
//! renders exactly one session observation as stable, scriptable lines.
//! These tests freeze the rendering contract (goal §8: tests must
//! freeze the chosen contract) and pin the truthful-read-side rule: the
//! output never claims an unearned semantic.

use qianqian_headless::status::format_status;
use qianqian_playback::{SessionObservation, SessionOutcome};

const FORMAT_44K: qianqian_playback::PcmFormat = qianqian_playback::PcmFormat {
    sample_rate: 44100,
    channels: 2,
    channel_mask: 0x3,
};

fn pending_observation() -> SessionObservation {
    SessionObservation {
        outcome: None,
        source_format: None,
        activation_error: None,
        stop_requested: false,
    }
}

/// The exact pending render: pending means only "no terminal outcome
/// committed yet" — it names no transport state.
#[test]
fn pending_renders_exactly_the_pending_block() {
    let text = format_status(&pending_observation());
    assert_eq!(
        text,
        "outcome: pending\n\
         format: unavailable\n\
         stop_requested: false"
    );
}

/// The format line is the write-once mechanism readback rendered
/// verbatim; it carries no source identity.
#[test]
fn the_source_format_readback_renders_as_mechanism_evidence() {
    let observation = SessionObservation {
        source_format: Some(FORMAT_44K),
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(
        text.contains("format: 44100 Hz, 2 channels, mask 0x3"),
        "format readback line missing: {text}"
    );
    assert!(
        !text.contains("source:"),
        "format must not be presented as source identity: {text}"
    );
}

/// Terminal outcomes render their committed truth verbatim.
#[test]
fn committed_outcomes_render_their_committed_truth() {
    let completed = SessionObservation {
        outcome: Some(SessionOutcome::Completed),
        source_format: Some(FORMAT_44K),
        activation_error: None,
        stop_requested: false,
    };
    assert_eq!(
        format_status(&completed),
        "outcome: completed\n\
         format: 44100 Hz, 2 channels, mask 0x3\n\
         stop_requested: false"
    );

    let stopped = SessionObservation {
        outcome: Some(SessionOutcome::Stopped),
        source_format: Some(FORMAT_44K),
        activation_error: None,
        stop_requested: true,
    };
    assert_eq!(
        format_status(&stopped),
        "outcome: stopped\n\
         format: 44100 Hz, 2 channels, mask 0x3\n\
         stop_requested: true"
    );
}

/// A failed outcome renders its class diagnostic on its own line.
#[test]
fn a_failed_outcome_renders_its_class_diagnostic() {
    let failed = SessionObservation {
        outcome: Some(SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned(),
        }),
        source_format: None,
        activation_error: None,
        stop_requested: false,
    };
    let text = format_status(&failed);
    assert!(
        text.contains("outcome: failed\nfailure: decode: test decode failure"),
        "failed block missing its diagnostic line: {text}"
    );
}

/// A multi-line stage diagnostic is flattened so one status block stays
/// line-parseable (the stage is representation, not frozen truth).
#[test]
fn a_multiline_stage_diagnostic_is_flattened_to_one_line() {
    let failed = SessionObservation {
        outcome: Some(SessionOutcome::Failed {
            stage: "decode: broken\nheader".to_owned(),
        }),
        ..pending_observation()
    };
    let text = format_status(&failed);
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("failure:"))
            .count(),
        1,
        "the diagnostic must stay on one line: {text}"
    );
    assert!(
        text.contains("failure: decode: broken header"),
        "flattened diagnostic missing: {text}"
    );
}

/// The activation diagnostic renders only when present, and stays
/// labeled as the activation attempt's diagnostic.
#[test]
fn the_activation_diagnostic_renders_only_when_present() {
    let clean = format_status(&pending_observation());
    assert!(
        !clean.contains("activation"),
        "no activation line without a diagnostic: {clean}"
    );

    let failed_activation = SessionObservation {
        activation_error: Some("render stream open failed: no device".to_owned()),
        ..pending_observation()
    };
    let text = format_status(&failed_activation);
    assert!(
        text.contains("activation_error: render stream open failed: no device"),
        "activation diagnostic line missing: {text}"
    );
    assert!(
        !text.contains("outcome: failed"),
        "an activation diagnostic must not fabricate an episode outcome: {text}"
    );
}

/// Truthful-read-side oracle: no rendering of any observation may claim
/// an unearned semantic (Playing/Starting/Paused/Stopping, position,
/// buffer health, source identity).
#[test]
fn no_rendering_claims_an_unearned_semantic() {
    let observations = [
        pending_observation(),
        SessionObservation {
            outcome: Some(SessionOutcome::Completed),
            source_format: Some(FORMAT_44K),
            activation_error: None,
            stop_requested: false,
        },
        SessionObservation {
            outcome: Some(SessionOutcome::Stopped),
            source_format: None,
            activation_error: None,
            stop_requested: true,
        },
        SessionObservation {
            outcome: Some(SessionOutcome::Failed {
                stage: "device".to_owned(),
            }),
            source_format: Some(FORMAT_44K),
            activation_error: Some("activation attempt diagnostic".to_owned()),
            stop_requested: true,
        },
    ];
    for observation in &observations {
        assert_never_claims_unearned_semantics(&format_status(observation));
    }
}

/// Assert the status text stays inside the earned truth boundary.
/// Case-insensitive on the state-like words that must not appear at
/// all; exact on the mechanism-ish labels that must not appear as
/// product claims.
fn assert_never_claims_unearned_semantics(text: &str) {
    let lowered = text.to_lowercase();
    for forbidden in [
        "playing", "starting", "paused", "stopping", "position", "elapsed", "buffered", "duration",
    ] {
        assert!(
            !lowered.contains(forbidden),
            "unearned semantic '{forbidden}' in status output: {text}"
        );
    }
    assert!(
        !text.contains("source:"),
        "no source-identity claim in status output: {text}"
    );
}

/// Negative control (T10): the oracle above is load-bearing — feeding
/// it the forbidden mapping (pending relabeled Playing) must fail the
/// test, proving the truthful-read-side assertions are not vacuous.
#[test]
#[should_panic(expected = "unearned semantic 'playing'")]
fn negative_control_the_unearned_semantics_oracle_is_not_vacuous() {
    let honest = format_status(&pending_observation());
    let lying = honest.replace("pending", "Playing");
    assert_never_claims_unearned_semantics(&lying);
}

/// Negative control (T10, second axis): an output that smuggles buffer
/// health into the status block must fail the oracle too.
#[test]
#[should_panic(expected = "unearned semantic 'buffered'")]
fn negative_control_buffer_health_is_rejected() {
    let honest = format_status(&pending_observation());
    let lying = format!("{honest}\nbuffered: 4096 frames");
    assert_never_claims_unearned_semantics(&lying);
}
