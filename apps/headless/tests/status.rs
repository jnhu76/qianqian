//! F2 status-projection tests: the truthful text rendering of one
//! episode observation, plus its negative-control oracle (T13).
//!
//! The oracle proves two directions:
//!   - every rendered observation is free of unearned playback
//!     semantics (no playing/starting/paused/stopping/buffering);
//!   - the scan itself is not vacuous — text that DOES claim a
//!     forbidden semantic is caught.

use qianqian_audio_api::ports::PcmFormat;
use qianqian_headless::status::{forbidden_status_claim, format_status};
use qianqian_playback::{PlaybackSessionObservation, SessionOutcome};

fn pending_observation() -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: None,
        stop_requested: false,
        source_format: None,
        activation_error: None,
    }
}

#[test]
fn a_fresh_episode_projects_pending_without_inventing_state() {
    let text = format_status(&pending_observation());
    assert_eq!(
        text,
        "outcome: pending\nformat: unavailable\nstop_requested: false\n"
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

#[test]
fn each_terminal_fact_projects_its_own_line() {
    for (outcome, expected) in [
        (SessionOutcome::Completed, "outcome: completed\n"),
        (SessionOutcome::Stopped, "outcome: stopped\n"),
        (
            SessionOutcome::Failed {
                stage: "decode: corrupt frame".to_owned(),
            },
            "outcome: failed\nfailure: decode: corrupt frame\n",
        ),
    ] {
        let observation = PlaybackSessionObservation {
            terminal_outcome: Some(outcome),
            stop_requested: false,
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            activation_error: None,
        };
        let text = format_status(&observation);
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
        assert!(text.contains("format: 44100 Hz, 2 channels, mask 0x3\n"));
        assert!(text.contains("stop_requested: false\n"));
        assert_eq!(forbidden_status_claim(&text), None);
    }
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
    for word in [
        "playing",
        "starting",
        "paused",
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
}
