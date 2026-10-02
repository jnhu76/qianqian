//! F2 status-projection tests: the truthful text rendering of one
//! episode observation, plus its negative-control oracle (T13).
//! Extended by F4 with the timeline line (D14.8).
//!
//! The oracle proves two directions:
//!   - every rendered observation is free of unearned playback
//!     semantics (no playing/starting/stopping/buffering — `paused`
//!     left the forbidden set when D14.7 earned the projection, and the
//!     F4 `position:` line is the D14.8 Projection);
//!   - the scan itself is not vacuous — text that DOES claim a
//!     forbidden semantic is caught.

use std::time::Duration;

use qianqian_audio_api::ports::PcmFormat;
use qianqian_headless::status::{forbidden_status_claim, format_status, format_timeline};
use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};

const TEST_FORMAT: PcmFormat = PcmFormat {
    sample_rate: 44100,
    channels: 2,
    channel_mask: 0x3,
};

fn pending_observation() -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: None,
        failure_diagnostic: None,
        stop_requested: false,
        pause_requested: false,
        source_format: None,
        source_duration: None,
        position: None,
        pause_engagement: PauseEngagement::Disengaged,
        activation_error: None,
        last_processing_refusal: None,
    }
}

#[test]
fn a_fresh_episode_projects_pending_without_inventing_state() {
    let text = format_status(&pending_observation());
    assert_eq!(
        text,
        "outcome: pending\nformat: unavailable\nposition: --:-- / --:--\n\
         stop_requested: false\npause_requested: false\npaused: false\n"
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
            source_format: Some(TEST_FORMAT),
            source_duration: None,
            position: None,
            pause_engagement: PauseEngagement::Disengaged,
            activation_error: None,
            last_processing_refusal: None,
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

// --- F4: the timeline projection (D14.8) --------------------------------

/// The four combinations of the two independent evidence sides, none of
/// which may fabricate a zero.
#[test]
fn the_timeline_projects_each_side_independently() {
    // 42 s of 44.1 kHz frames is 1_852_200 frames.
    let known_position = Some(1_852_200u64);
    let known_duration = Some(Duration::from_secs(238)); // 03:58

    let both = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        position: known_position,
        source_duration: known_duration,
        ..pending_observation()
    };
    assert_eq!(format_timeline(&both), "00:42 / 03:58");
    assert!(
        format_status(&both).contains("position: 00:42 / 03:58\n"),
        "{}",
        format_status(&both)
    );

    let unknown_duration = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        position: known_position,
        ..pending_observation()
    };
    assert_eq!(format_timeline(&unknown_duration), "00:42 / --:--");

    let unknown_position = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        source_duration: known_duration,
        ..pending_observation()
    };
    assert_eq!(format_timeline(&unknown_position), "--:-- / 03:58");

    let neither = pending_observation();
    assert_eq!(format_timeline(&neither), "--:-- / --:--");
}

/// A published position sample with no published source format has no
/// honest time: the display must not divide by a guessed sample rate.
#[test]
fn a_position_without_a_published_format_projects_as_unknown() {
    let observation = PlaybackSessionObservation {
        position: Some(44_100),
        source_duration: Some(Duration::from_secs(1)),
        ..pending_observation()
    };
    assert_eq!(format_timeline(&observation), "--:-- / 00:01");
}

/// The display never rounds up past the evidence, and it stays a clock
/// rather than wrapping at an hour.
#[test]
fn the_clock_truncates_and_does_not_wrap_at_an_hour() {
    // One frame short of 43 s.
    let just_under = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        position: Some(44_100 * 43 - 1),
        ..pending_observation()
    };
    assert_eq!(format_timeline(&just_under), "00:42 / --:--");

    let long = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        position: Some(44_100 * 3_800), // 63:20
        source_duration: Some(Duration::from_secs(3_800)),
        ..pending_observation()
    };
    assert_eq!(format_timeline(&long), "63:20 / 63:20");
}

/// A reported duration is evidence, never an exact truth, and never a
/// promise the position must catch up to: a duration with no position
/// sample projects exactly that, and the two sides are never reconciled.
#[test]
fn the_duration_side_is_independent_evidence() {
    let observation = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        position: Some(44_100),                          // 00:01 consumed
        source_duration: Some(Duration::from_secs(240)), // 04:00 declared
        ..pending_observation()
    };
    assert_eq!(format_timeline(&observation), "00:01 / 04:00");

    // A zero-length declaration is evidence too, not "unknown" — and it
    // stays distinct from the absent case above.
    let empty = PlaybackSessionObservation {
        source_format: Some(TEST_FORMAT),
        position: Some(0),
        source_duration: Some(Duration::ZERO),
        ..pending_observation()
    };
    assert_eq!(format_timeline(&empty), "00:00 / 00:00");
}

/// The position sample is frames of the source, so the conversion uses
/// the published source rate — and a rate of zero can never divide.
#[test]
fn the_conversion_uses_the_published_source_rate() {
    let at_48k = PlaybackSessionObservation {
        source_format: Some(PcmFormat {
            sample_rate: 48_000,
            channels: 2,
            channel_mask: 0x3,
        }),
        position: Some(48_000 * 42),
        ..pending_observation()
    };
    assert_eq!(format_timeline(&at_48k), "00:42 / --:--");

    // Structurally impossible (a format with zero channels/rate cannot
    // be negotiated), and it fails closed rather than dividing.
    let zero_rate = PlaybackSessionObservation {
        source_format: Some(PcmFormat {
            sample_rate: 0,
            channels: 2,
            channel_mask: 0x3,
        }),
        position: Some(48_000 * 42),
        ..pending_observation()
    };
    assert_eq!(format_timeline(&zero_rate), "--:-- / --:--");
}

/// A settled episode withdraws the position projection (the seam does),
/// while the duration stays observable evidence — the status text must
/// render exactly that, and must not invent a final position.
#[test]
fn a_settled_episode_projects_no_position_but_keeps_the_duration() {
    let observation = PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Completed),
        source_format: Some(TEST_FORMAT),
        source_duration: Some(Duration::from_secs(238)),
        position: None,
        ..pending_observation()
    };
    let text = format_status(&observation);
    assert!(text.contains("position: --:-- / 03:58\n"), "{text}");
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
    // above. `resumed` re-entered the forbidden set when the D14.7
    // AUTHORITY-CORRECTIVE removed the Resumed product projection.
    for word in [
        "playing",
        "starting",
        "pausing",
        "stopping",
        "resumed",
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
