//! Pure TUI model: the truthful projection of one episode observation
//! into display labels, plus the keyboard grammar and its wiring to the
//! episode seam.
//!
//! Same truth-class discipline as [`crate::status`] (D14.2/D14.7):
//! `pending` states only "no terminal Fact committed yet" — never
//! Playing/Starting/Stopping; `stop_requested`/`pause_requested` are
//! Command state; `source_format` is mechanism evidence; the `Paused`
//! projection is derived by the seam itself from the frozen D14.7
//! establishment conjunction; the diagnostics are presentation text.
//! This module performs no I/O and holds no truth of its own; every
//! label is derived from the last observation handed to
//! [`TuiModel::update`].

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use qianqian_playback::{
    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, PlaybackSessionObservation,
};
use std::time::Duration;

/// One frame's worth of presentation state: the source path the episode
/// was opened with and the latest coherent observation.
pub struct TuiModel {
    source: String,
    observation: PlaybackSessionObservation,
}

impl TuiModel {
    /// A model for an episode whose observation has not been read yet:
    /// every label starts at the honest "unknown/pending" projection.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            observation: PlaybackSessionObservation {
                terminal_outcome: None,
                failure_diagnostic: None,
                stop_requested: false,
                pause_requested: false,
                source_format: None,
                source_duration: None,
                position: None,
                pause_engagement: PauseEngagement::Disengaged,
                activation_error: None,
            },
        }
    }

    /// Replace the projected truth with one fresh coherent read. Pure:
    /// the caller got the observation from the episode seam.
    pub fn update(&mut self, observation: PlaybackSessionObservation) {
        self.observation = observation;
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn observation(&self) -> &PlaybackSessionObservation {
        &self.observation
    }

    /// The terminal-outcome label: `pending` while no terminal Fact is
    /// committed, otherwise exactly the committed D11 outcome. No
    /// fourth state exists here.
    pub fn terminal_label(&self) -> &'static str {
        match self.observation.terminal_outcome {
            None => "pending",
            Some(EpisodeTerminalOutcome::Completed) => "Completed",
            Some(EpisodeTerminalOutcome::Stopped) => "Stopped",
            Some(EpisodeTerminalOutcome::Failed) => "Failed",
        }
    }

    /// The Paused projection (D14.7), derived by the seam itself from
    /// the frozen establishment conjunction. Display only.
    pub fn paused(&self) -> bool {
        self.observation.paused()
    }

    /// The source-format label: the published PCM format once
    /// activation reported it (mechanism evidence), `pending` before.
    pub fn format_label(&self) -> String {
        match &self.observation.source_format {
            Some(format) => format!(
                "{} Hz, {} channels, mask {:#x}",
                format.sample_rate, format.channels, format.channel_mask
            ),
            None => "pending".to_owned(),
        }
    }

    /// The F4 timeline line (D14.8): `00:42 / 03:58`, with `--:--` for a
    /// side whose evidence does not exist yet (or is unknown). Rendered
    /// by the same projection helper the scriptable status text uses, so
    /// the two read-side surfaces cannot disagree; the model keeps no
    /// position of its own (no `last_position`, no local playback truth).
    pub fn timeline_label(&self) -> String {
        crate::status::format_timeline(&self.observation)
    }

    /// Diagnostics worth showing, in stable order. Both are
    /// presentation text supplied by the seam; their presence or
    /// wording is never part of the semantic outcome.
    pub fn diagnostics(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(error) = &self.observation.activation_error {
            lines.push(format!("activation: {error}"));
        }
        if let Some(failure) = &self.observation.failure_diagnostic {
            lines.push(format!("failure: {failure}"));
        }
        lines
    }

    /// Whether the episode's terminal Fact is already committed. The
    /// shell keeps rendering it (a committed outcome is not erased by
    /// the UI); S stays a no-op of the idempotent seam, Q still quits.
    pub fn terminal_committed(&self) -> bool {
        self.observation.terminal_outcome.is_some()
    }
}

/// What one key press means to the shell. `Stop`, `PauseResume` and the
/// seek arrows are episode commands; `Quit` is loop control, not a
/// playback semantic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Stop,
    /// Space: pause when pause intent is not recorded, resume when it
    /// is. One key, two commands — never a local `paused` bool.
    PauseResume,
    /// Left arrow: seek one [`SEEK_STEP`] earlier (D14.5).
    SeekBackward,
    /// Right arrow: seek one [`SEEK_STEP`] later (D14.5).
    SeekForward,
    Quit,
}

/// The fixed seek step the arrow keys request (D14.5). One product
/// decision, one constant — deliberately not a configuration surface.
pub const SEEK_STEP: Duration = Duration::from_secs(5);

/// The seek target one arrow key requests, derived from ONE coherent
/// observation of the episode: the position Projection (D14.8, source
/// PCM frames) converted with that same observation's published sample
/// rate (the stream runs at the source format, so frames and rate are
/// one unit world — the F4 negotiation rule). `None` means the arrow is
/// inert for this episode: with no position sample (or no rate to
/// convert it) there is no target to compute, and a seek with no
/// computable target is never SENT — no fabricated zero, no seek to the
/// episode start, no command at all.
pub fn seek_target(observation: &PlaybackSessionObservation, forward: bool) -> Option<Duration> {
    let rate = u64::from(observation.source_format?.sample_rate);
    if rate == 0 {
        return None;
    }
    let position = observation.position?;
    let current = Duration::from_micros(position * 1_000_000 / rate);
    Some(if forward {
        current.saturating_add(SEEK_STEP)
    } else {
        current.saturating_sub(SEEK_STEP)
    })
}

/// One event-loop step after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// The shell's whole keyboard grammar: Left/Right seek in fixed steps,
/// Space toggles pause/resume, S stops, Q quits, Ctrl+C quits. Anything
/// else is presentation noise (including key-release events, which
/// Windows terminals emit).
pub fn action_for_key(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    // Letters and arrows act on their plain or shift-keyed form
    // (terminals disagree about reporting SHIFT); chords stay noise
    // except the conventional Ctrl+C quit — so Ctrl+S/Ctrl+Q and
    // Ctrl+arrows never act by accident.
    let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
    match key.code {
        KeyCode::Char(' ') if plain => Some(Action::PauseResume),
        KeyCode::Char('s') | KeyCode::Char('S') if plain => Some(Action::Stop),
        KeyCode::Char('q') | KeyCode::Char('Q') if plain => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Left if plain => Some(Action::SeekBackward),
        KeyCode::Right if plain => Some(Action::SeekForward),
        _ => None,
    }
}

/// The event loop's entire reaction to one key press, factored out of
/// [`super::runtime::run`] so the key → seam wiring is testable
/// without a terminal. Space routes to the pause/resume seams: which of
/// the two commands is sent comes from a FRESH authoritative observation
/// of the episode's pause-intent command state — the shell never keeps
/// a local `paused` bool. The arrows route a fixed-step seek (D14.5):
/// the target is derived from one fresh coherent observation, and an
/// episode whose position is unknown gets NO command at all. S routes
/// to `request_stop`; all of these are idempotent, valid before and
/// after the terminal Fact. Q exits the loop without touching the
/// episode.
pub fn apply_action(action: Action, handle: &PlaybackSessionHandle) -> Step {
    match action {
        Action::Stop => {
            handle.request_stop();
            Step::Continue
        }
        Action::PauseResume => {
            if handle.observe().pause_requested {
                handle.request_resume();
            } else {
                handle.request_pause();
            }
            Step::Continue
        }
        Action::SeekBackward | Action::SeekForward => {
            if let Some(target) = seek_target(&handle.observe(), action == Action::SeekForward) {
                handle.request_seek(target);
            }
            Step::Continue
        }
        Action::Quit => Step::Exit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_audio_api::ports::PcmFormat;
    use std::time::Duration;

    fn pending() -> PlaybackSessionObservation {
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
        }
    }

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

    #[test]
    fn s_maps_to_stop_and_q_maps_to_quit() {
        for key in ['s', 'S'] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(Action::Stop),
                "{key} must request stop"
            );
        }
        for key in ['q', 'Q'] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(Action::Quit),
                "{key} must quit"
            );
        }
        // Terminals disagree about reporting SHIFT with a letter.
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT)),
            Some(Action::Stop)
        );
    }

    #[test]
    fn ctrl_c_keeps_its_conventional_quit_meaning() {
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
    }

    #[test]
    fn any_other_key_is_presentation_noise() {
        for key in [
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            // Chords stay noise (except Ctrl+C) so e.g. Ctrl+S/Ctrl+Q
            // never act by accident.
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        ] {
            assert_eq!(action_for_key(key), None, "{key:?} must be ignored");
        }
        // Key-release events (Windows terminals emit them) never act.
        assert_eq!(
            action_for_key(KeyEvent::new_with_kind(
                KeyCode::Char('s'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
            None
        );
    }

    /// The F5 arrow grammar: Left and Right map to the two seek
    /// directions, in their plain and shift-keyed forms (same terminal
    /// posture as the letters), on press only — and chords (Ctrl+arrow)
    /// stay noise.
    #[test]
    fn arrows_map_to_the_fixed_step_seek_actions() {
        for (key, action) in [
            (KeyCode::Left, Action::SeekBackward),
            (KeyCode::Right, Action::SeekForward),
        ] {
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::NONE)),
                Some(action)
            );
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::SHIFT)),
                Some(action),
                "shift-keyed arrows act like plain ones"
            );
            assert_eq!(
                action_for_key(KeyEvent::new_with_kind(
                    key,
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                )),
                None,
                "release events never act"
            );
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::CONTROL)),
                None,
                "chorded arrows stay noise"
            );
        }
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
        assert_eq!(seek_target(&no_position, true), None);
        assert_eq!(seek_target(&no_position, false), None);
        // No published rate: no unit to convert with, inert.
        let no_format = observation_with(Some(100), None);
        assert_eq!(seek_target(&no_format, true), None);

        // 42 s at 44.1 kHz: the step is exactly five seconds of media
        // time, and the backward step saturates at zero (Duration is
        // non-negative by type).
        let at_42s = observation_with(Some(44_100 * 42), Some(44_100));
        assert_eq!(seek_target(&at_42s, true), Some(Duration::from_secs(47)));
        assert_eq!(seek_target(&at_42s, false), Some(Duration::from_secs(37)));
        let at_2s = observation_with(Some(2 * 44_100), Some(44_100));
        assert_eq!(
            seek_target(&at_2s, false),
            Some(Duration::from_secs(0)),
            "before zero the step saturates at the episode start"
        );

        // The conversion uses the observation's own rate: 42 s at
        // 48 kHz is the same media time from different frames.
        let at_48k = observation_with(Some(48_000 * 42), Some(48_000));
        assert_eq!(seek_target(&at_48k, true), Some(Duration::from_secs(47)));
    }

    /// An episode whose position is unknown gets NO seek command: the
    /// arrows change nothing (and panic on nothing).
    #[test]
    fn the_seek_actions_change_nothing_when_the_position_is_unknown() {
        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        assert_eq!(apply_action(Action::SeekForward, &handle), Step::Continue);
        assert_eq!(apply_action(Action::SeekBackward, &handle), Step::Continue);
        assert_eq!(handle.observe(), before);
    }

    /// The stop key maps to the EXISTING request_stop seam — the same
    /// frozen right the machine transport uses — and quit never
    /// touches the episode.
    #[test]
    fn the_stop_action_routes_through_the_request_stop_seam_only() {
        let handle = PlaybackSessionHandle::new();
        assert!(!handle.observe().stop_requested);

        assert_eq!(apply_action(Action::Stop, &handle), Step::Continue);
        assert!(
            handle.observe().stop_requested,
            "S must record stop intent through the seam"
        );
        // Idempotent: pressing S again stays a plain seam call.
        assert_eq!(apply_action(Action::Stop, &handle), Step::Continue);
        assert!(handle.observe().stop_requested);

        // Quit is loop control, not a playback command: no new state.
        let before = handle.observe();
        assert_eq!(apply_action(Action::Quit, &handle), Step::Exit);
        assert_eq!(handle.observe(), before);
    }

    #[test]
    fn space_maps_to_the_pause_resume_toggle_and_no_other_key_does() {
        // Same posture as the letters: terminals disagree about
        // reporting SHIFT, so a shift-keyed space acts too.
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(' '), modifiers)),
                Some(Action::PauseResume)
            );
        }
        assert_eq!(
            action_for_key(KeyEvent::new_with_kind(
                KeyCode::Char(' '),
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            None
        );
    }

    /// Space never keeps a local paused bool: the first press records
    /// pause intent through the seam, the next press releases it, and
    /// the choice between the two commands is read from a fresh
    /// authoritative observation each time.
    #[test]
    fn the_pause_resume_action_routes_through_the_seam_both_ways() {
        let handle = PlaybackSessionHandle::new();
        assert!(!handle.observe().pause_requested);
        assert!(!handle.observe().paused());

        assert_eq!(apply_action(Action::PauseResume, &handle), Step::Continue);
        assert!(
            handle.observe().pause_requested,
            "first Space must record pause intent through the seam"
        );
        // Idempotent command state: repeated presses while paused stay
        // recorded intent, and the second press resumes.
        assert_eq!(apply_action(Action::PauseResume, &handle), Step::Continue);
        assert!(
            !handle.observe().pause_requested,
            "second Space must release the pause through the seam"
        );
        // Resuming without a prior pause still goes through the seam
        // (inert intent history): the observation derives everything.
        assert_eq!(apply_action(Action::PauseResume, &handle), Step::Continue);
        assert!(handle.observe().pause_requested);
    }

    /// The displayed Paused projection comes from the seam's frozen
    /// establishment conjunction, never from command state alone: an
    /// episode with recorded pause intent but no engaged+quiesced
    /// render evidence must not display Paused.
    #[test]
    fn the_paused_label_follows_the_establishment_conjunction_only() {
        let mut model = TuiModel::new("song.flac");
        assert!(!model.paused(), "fresh episode is not Paused");

        // Pause intent recorded, but no render engagement yet.
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            ..pending()
        });
        assert!(!model.paused(), "intent alone is not Paused");

        // Engaged, but the output tail has not been observed quiesced —
        // including after a prior cycle's release was observed (the
        // D14.7 corrective negative oracle: stale cross-cycle evidence
        // satisfies nothing).
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::Engaged,
            ..pending()
        });
        assert!(
            !model.paused(),
            "engagement without CURRENT quiescence is not Paused"
        );

        // Full establishment.
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(model.paused(), "intent + engagement + quiescence is Paused");

        // A committed terminal outcome breaks establishment even with
        // the mechanism evidence still latched.
        model.update(PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(!model.paused(), "a settled episode is never Paused");
    }
}
