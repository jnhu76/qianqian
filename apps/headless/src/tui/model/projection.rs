//! The read-side projections: labels derived from one coherent
//! observation handed to [`TuiModel::update`](super::state::TuiModel::update),
//! the shared seek-target math, the status shape, and the desired-DSP
//! summary.

use std::path::Path;
use std::time::Duration;

use qianqian_playback::{AudioProcessingConfig, EqConfig, EqPreset, PlaybackSessionObservation};

/// One playlist row as the shell presents it: the display label and the
/// two INDEPENDENT markers. A projection of the App's navigation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRow {
    pub label: String,
    /// This row is the committed (playing) position.
    pub playing: bool,
    /// This row is the UI selection.
    pub selected: bool,
}

/// The row label for one source (Issue #166 §22): the file name when the
/// path has one, the whole path otherwise. The filename IS the title for
/// this campaign — no metadata is read, and no path is invented.
pub fn row_label(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => path.to_string_lossy().into_owned(),
    }
}
/// The small seek step the plain arrow keys request (D14.5): five
/// seconds of media time.
pub const SEEK_STEP_SECS: i64 = 5;
/// The large seek step Shift+arrow requests (Issue #166 §26): thirty
/// seconds of the same media time.
pub const LARGE_SEEK_STEP_SECS: i64 = 30;

/// How many status lines the bottom bar renders at most (a multi-line
/// status BLOCK clips honestly past this, like every other region).
/// The MODEL owns the cap because the status block's shape feeds the
/// shell layout — a shape change moves every target below it and must
/// invalidate the arm (§22); the view renders from the same constant.
pub const MAX_STATUS_ROWS: usize = 3;

/// How many rows the status BLOCK occupies under the bounded cap: the
/// shape `set_status` compares (and the view's bottom-bar budget is
/// built from).
pub(crate) fn status_shape(status: Option<&str>) -> usize {
    status
        .map(|status| status.lines().count())
        .unwrap_or(0)
        .min(MAX_STATUS_ROWS)
}

/// The read-only progress bar's width in cells (Issue #166 §33).
pub const BAR_WIDTH: usize = 24;

/// The seek target one seek action requests, derived from ONE coherent
/// observation of the episode: the position Projection (D14.8, source
/// PCM frames) converted with that same observation's published sample
/// rate. `None` means the seek is inert for this episode: with no
/// position sample (or no rate to convert it) there is no target to
/// compute, and a seek with no computable target is never SENT — no
/// fabricated zero, no seek to the episode start, no command at all.
pub fn seek_target(
    observation: &PlaybackSessionObservation,
    step: Duration,
    forward: bool,
) -> Option<Duration> {
    let rate = u64::from(observation.source_format?.sample_rate);
    if rate == 0 {
        return None;
    }
    let position = observation.position?;
    let current = Duration::from_micros(position * 1_000_000 / rate);
    Some(if forward {
        current.saturating_add(step)
    } else {
        current.saturating_sub(step)
    })
}

/// The click-to-position seek target (G1 §9): `per_mille` of the
/// episode's PUBLISHED duration evidence, clamped into it. Like
/// [`seek_target`], it is `None` — no command at all — whenever the
/// duration evidence does not exist: an unknown timeline is never
/// seekable by fraction, and nothing is fabricated in its place.
pub fn seek_fraction_target(
    observation: &PlaybackSessionObservation,
    per_mille: u16,
) -> Option<Duration> {
    let duration = observation.source_duration?;
    if duration.is_zero() {
        return None;
    }
    let per_mille = u128::from(per_mille.min(1000));
    let micros = duration.as_micros() * per_mille / 1000;
    Some(Duration::from_micros(u64::try_from(micros).ok()?))
}

/// The EQ stage's name by EXACT public data equality (T0): a factory
/// preset whose trims and Q match verbatim, else `custom EQ` — no new
/// product variant is invented for modified configurations.
pub fn eq_stage_summary(eq: &EqConfig) -> String {
    let preset = EqPreset::all()
        .into_iter()
        .find(|preset| preset.to_config().eq.as_ref() == Some(eq));
    match preset {
        Some(preset) => format!("preset {}", preset.name()),
        None => "custom EQ".to_owned(),
    }
}

/// The one-line summary of the App's DESIRED DSP configuration (G1:
/// the Now Playing route names it; the Audio route will edit it). A
/// desired-state statement only — the word is part of the line — and
/// the parts come from the T1A product data: bypass vs enabled, the
/// EQ stage as a matched factory preset or `custom EQ`, and the
/// preamp in dB. Nothing here is an applied-DSP claim.
pub fn dsp_summary(config: &AudioProcessingConfig) -> String {
    if !config.enabled {
        return "DSP (desired): off (bypass)".to_owned();
    }
    let mut parts = Vec::new();
    if let Some(eq) = &config.eq {
        parts.push(eq_stage_summary(eq));
    }
    let preamp_db = if config.gain > 0.0 {
        format!("{:+.1} dB", 20.0 * config.gain.log10())
    } else {
        "-inf dB".to_owned()
    };
    parts.push(format!("preamp {preamp_db}"));
    format!("DSP (desired): on — {}", parts.join(", "))
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use std::path::Path;

    use qianqian_playback::{AudioProcessingConfig, EqPreset};

    use crate::tui::model::testutil::pending;
    use crate::tui::model::*;

    use crate::playlist::{PlaybackOrder, RepeatMode};
    use qianqian_audio_api::ports::PcmFormat;
    use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};
    use std::time::Duration;

    #[test]
    fn the_terminal_label_names_only_pending_and_the_three_facts() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.terminal_label(), "pending");
        assert!(!model.terminal_committed());
        for (outcome, label) in [
            (EpisodeTerminalOutcome::Completed, "Completed"),
            (EpisodeTerminalOutcome::Stopped, "Stopped"),
            (EpisodeTerminalOutcome::Failed, "Failed"),
        ] {
            model.update(PlaybackSessionObservation {
                terminal_outcome: Some(outcome),
                ..pending()
            });
            assert_eq!(model.terminal_label(), label);
            assert!(model.terminal_committed());
        }
    }

    #[test]
    fn the_format_stays_pending_until_activation_publishes_one() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.format_label(), "pending");
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            ..pending()
        });
        assert_eq!(model.format_label(), "44100 Hz, 2 channels, mask 0x3");
    }

    /// The timeline line is the D14.8 projection rendered by the shared
    /// read-side helper, and the model keeps no position of its own: the
    /// label is exactly what the current observation says, including
    /// `--:--` for a side whose evidence is absent.
    #[test]
    fn the_timeline_label_follows_the_observation_only() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(
            model.timeline_label(),
            "--:-- / --:--",
            "no evidence yet, and never a fabricated zero"
        );

        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            ..pending()
        });
        assert_eq!(
            model.timeline_label(),
            "00:42 / --:--",
            "an unknown duration must not hide a known position"
        );

        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(model.timeline_label(), "00:42 / 03:58");

        // The live pre-first-sample window (owner re-review P1): format
        // and duration have arrived, the position projection has NOT.
        // The window where a fabricated start was observable — and the
        // unknown position must still not be rewritten into a zero.
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: None,
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(
            model.timeline_label(),
            "--:-- / 03:58",
            "a live episode without a position sample keeps the dashes"
        );
        assert!(
            model.position_bar_label().is_none(),
            "the bar does not render a fabricated start fill"
        );

        // A settled episode (the seam withdraws the position) keeps the
        // duration evidence and shows no position — no final-position
        // latch is invented here either.
        model.update(PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Completed),
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(model.timeline_label(), "--:-- / 03:58");
    }

    /// The progress bar renders only from BOTH sides' evidence: the
    /// fraction is the published position over the reported duration,
    /// clamped when the position runs past the duration, and no state
    /// short of both sides draws anything.
    #[test]
    fn the_position_bar_renders_only_from_both_sides_evidence() {
        let mut model = TuiModel::new("song.flac");

        // Nothing at all: no bar.
        assert_eq!(model.position_bar_label(), None);

        // Duration alone (position unknown — even LIVE): still no bar.
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(model.position_bar_label(), None);

        // Both sides: 00:42 of 03:58 fills 42/238 of the width.
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        let bar = model.position_bar_label().expect("both sides known");
        let filled = bar.chars().filter(|&c| c == '━').count();
        assert_eq!(filled, 42 * BAR_WIDTH / 238, "{bar}");
        assert!(
            bar.starts_with("00:42 ") && bar.ends_with(" 03:58"),
            "{bar}"
        );

        // A position past the reported duration clamps to a full bar.
        model.update(PlaybackSessionObservation {
            position: Some(44_100 * 300),
            ..model.observation().clone()
        });
        let bar = model.position_bar_label().expect("still both sides");
        assert_eq!(
            bar.chars().filter(|&c| c == '━').count(),
            BAR_WIDTH,
            "{bar}"
        );
    }

    #[test]
    fn diagnostics_stay_ordered_and_absent_without_content() {
        let mut model = TuiModel::new("song.flac");
        assert!(model.diagnostics().is_empty());
        model.update(PlaybackSessionObservation {
            activation_error: Some("no device".to_owned()),
            failure_diagnostic: Some("corrupt frame".to_owned()),
            ..pending()
        });
        assert_eq!(
            model.diagnostics(),
            vec![
                "activation: no device".to_owned(),
                "failure: corrupt frame".to_owned(),
            ]
        );
    }

    /// The seek target is a fixed step around the observed position,
    /// converted with the SAME observation's published rate, saturating
    /// at zero on the backward side — and it is `None` (no command at
    /// all) whenever either side of the evidence is missing: no
    /// fabricated zero, no seek to the episode start.
    #[test]
    fn the_seek_target_is_a_fixed_step_of_the_coherent_observation_or_inert() {
        let observation_with =
            |position: Option<u64>, sample_rate: Option<u32>| PlaybackSessionObservation {
                position,
                source_format: sample_rate.map(|sample_rate| PcmFormat {
                    sample_rate,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                ..pending()
            };
        // Unknown position: inert in both directions.
        let no_position = observation_with(None, Some(44_100));
        assert_eq!(
            seek_target(&no_position, Duration::from_secs(5), true),
            None
        );
        assert_eq!(
            seek_target(&no_position, Duration::from_secs(5), false),
            None
        );
        // No published rate: no unit to convert with, inert.
        let no_format = observation_with(Some(100), None);
        assert_eq!(seek_target(&no_format, Duration::from_secs(5), true), None);

        // 42 s at 44.1 kHz: the step is exactly five seconds of media
        // time, and the backward step saturates at zero (Duration is
        // non-negative by type).
        let at_42s = observation_with(Some(44_100 * 42), Some(44_100));
        assert_eq!(
            seek_target(&at_42s, Duration::from_secs(5), true),
            Some(Duration::from_secs(47))
        );
        assert_eq!(
            seek_target(&at_42s, Duration::from_secs(5), false),
            Some(Duration::from_secs(37))
        );
        let at_2s = observation_with(Some(2 * 44_100), Some(44_100));
        assert_eq!(
            seek_target(&at_2s, Duration::from_secs(5), false),
            Some(Duration::from_secs(0)),
            "before zero the step saturates at the episode start"
        );

        // The conversion uses the observation's own rate: 42 s at
        // 48 kHz is the same media time from different frames.
        let at_48k = observation_with(Some(48_000 * 42), Some(48_000));
        assert_eq!(
            seek_target(&at_48k, Duration::from_secs(5), true),
            Some(Duration::from_secs(47))
        );
    }

    #[test]
    fn row_labels_use_the_file_name_and_keep_unicode() {
        for (path, expected) in [
            ("/media/01 Intro.flac", "01 Intro.flac"),
            ("/media/夜曲 七里香.flac", "夜曲 七里香.flac"),
            ("/media/a/b/c.mp3", "c.mp3"),
            ("/", "/"),
            ("..", ".."),
        ] {
            assert_eq!(row_label(Path::new(path)), expected, "{path}");
        }
    }

    /// The order/repeat labels come from the App's own policy state and
    /// are absent until the shell has been told them.
    #[test]
    fn the_order_and_repeat_labels_follow_the_app_state() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.order_label(), None);
        assert_eq!(model.repeat_label(), None);
        model.set_order(PlaybackOrder::Shuffle);
        model.set_repeat(RepeatMode::One);
        assert_eq!(model.order_label(), Some("Shuffle"));
        assert_eq!(model.repeat_label(), Some("One"));
    }

    /// The status line is plain presentation: recorded, read, replaced.
    #[test]
    fn the_status_line_records_operation_feedback() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.status(), None);
        model.set_status(Some("open refused: unsupported container".to_owned()));
        assert_eq!(model.status(), Some("open refused: unsupported container"));
        model.set_status(None);
        assert_eq!(model.status(), None);
    }

    /// The displayed Paused projection comes from the seam's frozen
    /// establishment conjunction, never from command state alone.
    #[test]
    fn the_paused_label_follows_the_establishment_conjunction_only() {
        let mut model = TuiModel::new("song.flac");
        assert!(!model.paused(), "fresh episode is not Paused");
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            ..pending()
        });
        assert!(!model.paused(), "intent alone is not Paused");
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::Engaged,
            ..pending()
        });
        assert!(
            !model.paused(),
            "engagement without quiescence is not Paused"
        );
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(model.paused(), "intent + engagement + quiescence is Paused");
        model.update(PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(!model.paused(), "a settled episode is never Paused");
    }

    // ------------------------------------------------------------------
    // Route / focus / modal model tests.
    // ------------------------------------------------------------------

    /// The fraction seek target is a per-mille of the PUBLISHED
    /// duration — and `None` (no command at all) without duration
    /// evidence: an unknown timeline is never clickable into a
    /// fabricated target.
    #[test]
    fn the_seek_fraction_target_needs_published_duration_evidence() {
        let with_duration = PlaybackSessionObservation {
            source_duration: Some(Duration::from_secs(200)),
            ..pending()
        };
        assert_eq!(
            seek_fraction_target(&with_duration, 500),
            Some(Duration::from_secs(100))
        );
        assert_eq!(
            seek_fraction_target(&with_duration, 1000),
            Some(Duration::from_secs(200))
        );
        assert_eq!(
            seek_fraction_target(&with_duration, 1500),
            Some(Duration::from_secs(200)),
            "the fraction clamps into the duration"
        );
        let without_duration = PlaybackSessionObservation {
            source_duration: None,
            ..pending()
        };
        assert_eq!(seek_fraction_target(&without_duration, 500), None);
        let zero_duration = PlaybackSessionObservation {
            source_duration: Some(Duration::ZERO),
            ..pending()
        };
        assert_eq!(seek_fraction_target(&zero_duration, 500), None);
    }

    /// The DSP summary line (G1): a DESIRED-state statement only —
    /// bypass says off, a preset configuration names the preset, a
    /// custom EQ says custom, the preamp renders in dB — and nothing
    /// here claims an applied state.
    #[test]
    fn the_dsp_summary_names_the_desired_configuration_only() {
        assert_eq!(
            dsp_summary(&AudioProcessingConfig::BYPASS),
            "DSP (desired): off (bypass)"
        );
        assert_eq!(
            dsp_summary(&EqPreset::Rock.to_config()),
            "DSP (desired): on — preset rock, preamp +0.0 dB"
        );
        let mut custom = EqPreset::Bass.to_config();
        custom.eq = Some(qianqian_playback::EqConfig::new(
            qianqian_playback::EqConfig::FLAT.band_gain_db,
            1.7,
        ));
        assert_eq!(
            dsp_summary(&custom),
            "DSP (desired): on — custom EQ, preamp +0.0 dB"
        );
        assert_eq!(
            dsp_summary(&AudioProcessingConfig::gain(2.0)),
            "DSP (desired): on — preamp +6.0 dB"
        );
    }
}
