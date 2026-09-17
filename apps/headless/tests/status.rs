//! F2 status-projection tests: the truthful text rendering of one
//! episode observation, plus its negative-control oracle (T13).
//!
//! The oracle proves two directions:
//!   - every rendered observation is free of unearned playback
//!     semantics (no playing/starting/stopping/buffering — `paused`
//!     left the forbidden set when D14.7 earned the projection);
//!   - the scan itself is not vacuous — text that DOES claim a
//!     forbidden semantic is caught.

use qianqian_audio_api::ports::PcmFormat;
use qianqian_headless::status::{forbidden_status_claim, format_status};
use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};

fn pending_observation() -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: None,
        failure_diagnostic: None,
        stop_requested: false,
        pause_requested: false,
        source_format: None,
        pause_engagement: PauseEngagement::Disengaged,
        activation_error: None,
    }
}

#[test]
fn a_fresh_episode_projects_pending_without_inventing_state() {
    let text = format_status(&pending_observation());
    assert_eq!(
        text,
        "outcome: pending\nformat: unavailable\nstop_requested: false\n\
         pause_requested: false\npaused: false\n"
    );
    assert_eq!(forbidden_status_claim(&text), None);
}

#[test]
fn stop_intent_projects_as_command_state_not_as_a_fact() {
    let observation = PlaybackSessionObservation {
        stop_requested: true,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("outcome: pending\n"), "still pending: {text}");
    assert!(text.contains("stop_requested: true\n"));
    assert_eq!(forbidden_status_claim(&text), None, "no 'stopping' claim");
}

/// The `paused:` line is the D14.7 establishment projection: recorded
/// intent alone must NOT project paused, and the full frozen
/// conjunction (unsettled ∧ intent ∧ engagement ∧ current tail
/// quiescence) must.
#[test]
fn pause_projects_through_the_frozen_establishment_conjunction() {
    let observation = PlaybackSessionObservation {
        pause_requested: true,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("pause_requested: true\n"), "{text}");
    assert!(
        text.contains("paused: false\n"),
        "intent without mechanism evidence is not Paused: {text}"
    );

    let observation = PlaybackSessionObservation {
        pause_requested: true,
        pause_engagement: PauseEngagement::Engaged,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(
        text.contains("paused: false\n"),
        "engagement without tail quiescence is not Paused: {text}"
    );

    let observation = PlaybackSessionObservation {
        pause_requested: true,
        pause_engagement: PauseEngagement::TailQuiesced,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("paused: true\n"), "{text}");
    assert_eq!(forbidden_status_claim(&text), None);
}

/// A settled episode is never Paused, whatever the mechanism evidence
/// still shows latched (D14.7): the establishment conjunction guards on
/// the unsettled state.
#[test]
fn a_settled_episode_never_projects_paused() {
    let observation = PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
        stop_requested: true,
        pause_requested: true,
        pause_engagement: PauseEngagement::TailQuiesced,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("outcome: stopped\n"), "{text}");
    assert!(text.contains("paused: false\n"), "{text}");
}

#[test]
fn each_terminal_fact_projects_its_own_line() {
    for (outcome, expected) in [
        (EpisodeTerminalOutcome::Completed, "outcome: completed\n"),
        (EpisodeTerminalOutcome::Stopped, "outcome: stopped\n"),
        (EpisodeTerminalOutcome::Failed, "outcome: failed\n"),
    ] {
        let observation = PlaybackSessionObservation {
            terminal_outcome: Some(outcome),
            stop_requested: false,
            pause_requested: false,
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            pause_engagement: PauseEngagement::Disengaged,
            activation_error: None,
            failure_diagnostic: None,
        };
        let text = format_status(&observation);
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
        assert!(text.contains("format: 44100 Hz, 2 channels, mask 0x3\n"));
        assert!(text.contains("stop_requested: false\n"));
        assert!(text.contains("paused: false\n"));
        assert_eq!(forbidden_status_claim(&text), None);
    }
}

/// The truth-class separation the projection must keep (CORRECTIVE-1
/// MAJOR-1): the semantic line names only the stable terminal
/// vocabulary; the diagnostic is a separate presentation line.
#[test]
fn a_failed_fact_projects_semantics_first_and_diagnostic_separately() {
    let observation = PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Failed),
        failure_diagnostic: Some("decode: corrupt frame".to_owned()),
        ..pending_observation()
    };
    let text = format_status(&observation);
    // Semantic line first, no diagnostic vocabulary in it.
    assert!(
        text.starts_with("outcome: failed\n"),
        "semantic line first: {text:?}"
    );
    // Diagnostic line separately, when one was published.
    assert!(
        text.contains("failure: decode: corrupt frame\n"),
        "diagnostic on its own line: {text:?}"
    );
    // The semantic line must not smuggle a failure subclass: callers
    // cannot infer DecodeFailed/DeviceFailed from the projection.
    assert!(
        !text.contains("failed:"),
        "no failure subclass in the semantic line: {text:?}"
    );
    assert_eq!(forbidden_status_claim(&text), None);
}

/// A Failed fact MAY lack a diagnostic; the semantic line must not
/// depend on it.
#[test]
fn a_failed_fact_without_a_diagnostic_still_projects_its_semantic_line() {
    let observation = PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Failed),
        failure_diagnostic: None,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("outcome: failed\n"), "{text:?}");
    assert!(
        !text.contains("failure:"),
        "no diagnostic line without a diagnostic: {text:?}"
    );
    assert_eq!(forbidden_status_claim(&text), None);
}

#[test]
fn an_activation_failure_projects_the_diagnostic_not_a_forged_fact() {
    let observation = PlaybackSessionObservation {
        activation_error: Some("render stream open failed: no device".to_owned()),
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("outcome: pending\n"), "no terminal Fact yet");
    assert!(text.contains("activation_error: render stream open failed: no device\n"));
    assert_eq!(forbidden_status_claim(&text), None);
}

// --- T13: the negative-control oracle, proven non-vacuous both ways ------

#[test]
fn the_forbidden_vocabulary_scan_catches_a_relabeled_pending() {
    // If someone rewrites pending as "playing", the oracle must fire.
    assert_eq!(
        forbidden_status_claim("outcome: playing\n"),
        Some("playing")
    );
}

#[test]
fn the_forbidden_vocabulary_scan_catches_smuggled_buffer_health() {
    // If someone smuggles buffer health into the projection, the
    // oracle must fire.
    assert_eq!(
        forbidden_status_claim("outcome: pending\nbuffered: 4096 frames\n"),
        Some("buffered")
    );
    assert_eq!(
        forbidden_status_claim("outcome: pending\nbuffering\n"),
        Some("buffering")
    );
}

#[test]
fn the_scan_covers_every_forbidden_word_and_passes_clean_text() {
    // `paused` is no longer forbidden: D14.7 earned the projection, and
    // the establishment conjunction is pinned by the positive tests
    // above.
    for word in [
        "playing",
        "starting",
        "pausing",
        "stopping",
        "buffering",
        "buffered",
    ] {
        assert_eq!(
            forbidden_status_claim(&format!("outcome: {word}\n")),
            Some(word),
            "the scan must catch '{word}'"
        );
    }
    assert_eq!(forbidden_status_claim("outcome: stopped\n"), None);
    assert_eq!(forbidden_status_claim("stop_requested: false\n"), None);
    assert_eq!(
        forbidden_status_claim("pause_requested: true\npaused: true\n"),
        None,
        "the earned D14.7 projection must pass the scan"
    );
}
