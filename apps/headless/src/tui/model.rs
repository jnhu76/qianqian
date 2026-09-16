//! Pure TUI model: the truthful projection of one episode observation
//! into display labels, plus the keyboard grammar and its wiring to the
//! episode seam.
//!
//! Same truth-class discipline as [`crate::status`] (D14.2): `pending`
//! states only "no terminal Fact committed yet" — never
//! Playing/Starting/Paused/Stopping; `stop_requested` is Command state;
//! `source_format` is mechanism evidence; the diagnostics are
//! presentation text. This module performs no I/O and holds no truth of
//! its own; every label is derived from the last observation handed to
//! [`TuiModel::update`].

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use qianqian_playback::{
    EpisodeTerminalOutcome, PlaybackSessionHandle, PlaybackSessionObservation,
};

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
                source_format: None,
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

/// What one key press means to the shell. `Stop` is an episode
/// command; `Quit` is loop control, not a playback semantic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Stop,
    Quit,
}

/// One event-loop step after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// The shell's whole keyboard grammar: S stops, Q quits, Ctrl+C quits.
/// Anything else is presentation noise (including key-release events,
/// which Windows terminals emit).
pub fn action_for_key(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    // Letters act on their plain or shift-keyed form (terminals
    // disagree about reporting SHIFT); chords stay noise except the
    // conventional Ctrl+C quit — so Ctrl+S/Ctrl+Q never act by accident.
    let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
    match key.code {
        KeyCode::Char('s') | KeyCode::Char('S') if plain => Some(Action::Stop),
        KeyCode::Char('q') | KeyCode::Char('Q') if plain => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        _ => None,
    }
}

/// The event loop's entire reaction to one key press, factored out of
/// [`super::runtime::run`] so the key → seam wiring is testable
/// without a terminal: S routes to the handle's `request_stop` seam —
/// idempotent and monotone, valid before and after the terminal Fact —
/// and Q exits the loop without touching the episode.
pub fn apply_action(action: Action, handle: &PlaybackSessionHandle) -> Step {
    match action {
        Action::Stop => {
            handle.request_stop();
            Step::Continue
        }
        Action::Quit => Step::Exit,
    }
}
