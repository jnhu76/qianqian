//! [`TuiModel`]: one frame's worth of presentation state and the
//! projection mutators the runtime calls on every refresh. The
//! route/focus/modal/viewport behavior lives in [`super::interaction`]
//! (a second `impl TuiModel` over the same fields).

use std::time::Duration;

use super::actions::{PlayPauseOffer, TuiRoute};
use super::focus::FocusId;
use super::hit::{ArmedClick, HitRegion};
use super::modal::Modal;
use super::projection::{BAR_WIDTH, PlaylistRow, status_shape};
use super::responsive::{ResponsiveClass, responsive_class};
use crate::playlist::{PlaybackOrder, RepeatMode};
use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};

/// One frame's worth of presentation state: the episode the player has
/// committed (source path + latest coherent observation), the playlist
/// rows, and the T1B interaction state (route, focus, modal, armed
/// click, this frame's hit regions, responsive class). All of it is
/// presentation: the shell keeps no playback truth of its own.
pub struct TuiModel {
    /// The committed episode's source path. `None` is a real state
    /// (F6): no episode is live — a clean-failed Open leaves no
    /// runtime, and the honest panel says so instead of fabricating
    /// labels.
    source: Option<String>,
    observation: PlaybackSessionObservation,
    /// The last operation's feedback — application composition feedback
    /// (D14.6), never a playback semantic.
    status: Option<String>,
    /// The player's navigation projection (1-based cursor, playlist
    /// length), refreshed with the episode.
    navigation_position: Option<(usize, usize)>,
    /// The playlist pane's rows, rebuilt only when the App's playlist
    /// revision moves (so a 5 000-row list costs nothing per frame).
    pub(super) playlist: Vec<PlaylistRow>,
    playlist_revision: Option<u64>,
    /// The App's traversal order / repeat preferences (labels only).
    order: Option<PlaybackOrder>,
    repeat: Option<RepeatMode>,
    /// The App's desired stream factor (D14.9 read side: exactly the
    /// configured value — never an acoustic level or mechanism
    /// readback).
    volume: Option<u8>,
    /// The App's desired DSP configuration, as the one-line summary
    /// label built by [`dsp_summary`] (T1A seam read side). Always a
    /// DESIRED-state statement: nothing here is an applied-DSP claim
    /// (G3 §20 discipline applies to the summary line too).
    desired_dsp: Option<String>,

    /// The active route (§5). Presentation-only: default Now Playing.
    pub(super) route: TuiRoute,
    /// The one active focus target, if any (§10).
    pub(super) focus: Option<FocusId>,
    /// The route-local focus a modal interrupted, restored on close
    /// (§24: close restores a valid prior focus) when it still exists.
    pub(super) focus_before_modal: Option<FocusId>,
    /// The one active modal, if any (§23).
    pub(super) modal: Option<Modal>,
    /// The armed-click interaction between Left Down and Left Up (§17):
    /// target AND origin cell, invalidated as one with the frame.
    pub(super) armed: Option<ArmedClick>,
    /// The CURRENT frame's hit regions (§13/§15). Empty before the
    /// first draw and after every invalidation.
    pub(super) regions: Vec<HitRegion>,
    /// The current responsive class, set by every draw (§27).
    pub(super) class: ResponsiveClass,
}

impl TuiModel {
    /// A model for an episode whose observation has not been read yet:
    /// every label starts at the honest "unknown/pending" projection.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: Some(source.into()),
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
                last_processing_refusal: None,
            },
            status: None,
            navigation_position: None,
            playlist: Vec::new(),
            playlist_revision: None,
            order: None,
            repeat: None,
            volume: None,
            desired_dsp: None,
            route: TuiRoute::NowPlaying,
            focus: None,
            focus_before_modal: None,
            modal: None,
            armed: None,
            regions: Vec::new(),
            class: responsive_class(u16::MAX, u16::MAX),
        }
    }

    /// Replace the projected truth with one fresh coherent read. Pure:
    /// the caller got the observation from the episode seam.
    pub fn update(&mut self, observation: PlaybackSessionObservation) {
        self.observation = observation;
    }

    /// Record the player's navigation projection (D14.6).
    pub fn set_navigation(&mut self, position: Option<(usize, usize)>) {
        self.navigation_position = position;
    }

    /// Record the playlist pane's rows, keyed by the App's playlist
    /// revision. A revision the model already holds does not even build
    /// the rows: the caller may call this on every refresh, and an
    /// unchanged (or huge) playlist costs nothing per frame. A MOVING
    /// revision is a list edit — the armed click dies with it (§17:
    /// a list revision change cancels the arm, even where a same-named
    /// row reappears at the same coordinates).
    pub fn set_playlist(&mut self, revision: u64, rows: impl FnOnce() -> Vec<PlaylistRow>) {
        if self.playlist_revision == Some(revision) {
            return;
        }
        self.playlist_revision = Some(revision);
        self.playlist = rows();
        self.invalidate_frame();
    }

    /// The playlist pane's rows, in the App's traversal order.
    pub fn playlist(&self) -> &[PlaylistRow] {
        &self.playlist
    }

    /// Record the App's traversal order preference.
    pub fn set_order(&mut self, order: PlaybackOrder) {
        self.order = Some(order);
    }

    /// Record the App's repeat preference.
    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = Some(repeat);
    }

    /// The order label (`Sequential` / `Shuffle`), once known.
    pub fn order_label(&self) -> Option<&'static str> {
        self.order.map(PlaybackOrder::label)
    }

    /// The repeat label (`Off` / `All` / `One`), once known.
    pub fn repeat_label(&self) -> Option<&'static str> {
        self.repeat.map(RepeatMode::label)
    }

    /// Record the player's desired stream factor (D14.9 read side).
    pub fn set_volume(&mut self, volume: Option<u8>) {
        self.volume = volume;
    }

    /// The desired stream factor label: the App's configured value.
    pub fn volume_label(&self) -> Option<String> {
        self.volume.map(|v| format!("{v}/100"))
    }

    /// Record the App's desired DSP summary line (T1A seam read side).
    pub fn set_desired_dsp(&mut self, summary: String) {
        self.desired_dsp = Some(summary);
    }

    /// The desired DSP summary line, once refreshed from the App.
    pub fn desired_dsp_label(&self) -> Option<&str> {
        self.desired_dsp.as_deref()
    }

    /// Follow the player's committed episode: `Some(path)` after a
    /// committed replacement, `None` after a clean-failed one. `None`
    /// also drops the last observation — the model holds no episode
    /// truth at all then, so the retired episode's diagnostics must
    /// not leak into the frame as if they described anything current.
    ///
    /// Every COMMITTED CHANGE of the episode is an episode replacement
    /// (§17): the armed click dies with it, so a press armed on one
    /// episode can never activate into the next one — even where the
    /// new episode draws the same control at the same coordinates
    /// (G1 F06). An unchanged refresh between two ticks keeps the arm.
    ///
    /// `set_episode` alone cannot see every replacement: its guard
    /// compares the DISPLAY string, and pathname equality is not
    /// episode identity — a same-file replay/Repeat One replaces the
    /// episode under an unchanged display, and two distinct native
    /// paths can render to one lossy string. The RUNTIME owns
    /// replacement knowledge (its seams perform the replacements), so
    /// it reports each committed replacement through
    /// [`Self::note_episode_replacement`]; the string guard below
    /// stays as the refresh-path safety net.
    pub fn set_episode(&mut self, source: Option<String>) {
        if self.source == source {
            return;
        }
        let episode_gone = source.is_none();
        self.source = source;
        self.invalidate_frame();
        if episode_gone {
            self.observation = PlaybackSessionObservation {
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
            };
        }
    }

    /// The committed episode's source path, if one is live.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// The runtime's report that a replacement seam (open, add-commit,
    /// navigation, play-selected, play-current, the EOF policy's
    /// advance) committed a NEW episode. The display string cannot be
    /// the replacement witness — a same-file replay reuses it — so the
    /// seam owner reports the event and the arm dies with the context
    /// it was armed in (§17, G1 F06; the model holds no episode
    /// identity of its own).
    pub fn note_episode_replacement(&mut self) {
        self.invalidate_frame();
    }

    /// Record one operation's feedback line (composition feedback,
    /// never a playback semantic). The status block's SHAPE feeds the
    /// shell layout — a status that grows or shrinks across the
    /// bounded line cap moves every target below it — so a shape
    /// change invalidates the frame exactly like a resize (§22: "the
    /// same applies after content changes that move targets"). A
    /// same-shape feedback keeps the arm.
    pub fn set_status(&mut self, status: Option<String>) {
        let shape = status_shape(status.as_deref());
        if shape != status_shape(self.status.as_deref()) {
            self.invalidate_frame();
        }
        self.status = status;
    }

    /// The navigation projection (D14.6): the 1-based cursor position
    /// and the playlist length, straight from the player's committed
    /// navigation state — presentation of application navigation state,
    /// never playback truth.
    pub fn navigation_position(&self) -> Option<(usize, usize)> {
        self.navigation_position
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
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

    /// What the central transport control OFFERS right now (T0
    /// transport freeze, G1 F03) — the context-labeled button:
    ///
    /// ```text
    /// no episode                 -> Play        (establish selection;
    ///                                               inert with a reason
    ///                                               when there is none)
    /// episode, terminal Fact     -> Play        (replay through the
    ///                                               existing replacement)
    /// episode, pause requested   -> Resume      (the command state)
    /// episode, otherwise         -> Pause       (the command state)
    /// ```
    ///
    /// Derived from the current observation every frame — never from a
    /// local paused/playing bool. `Pause`/`Resume` are the command
    /// state's own vocabulary; no unsettled episode is ever labeled
    /// Playing/Starting/Stopping.
    pub fn play_pause_offer(&self) -> PlayPauseOffer {
        if self.source.is_none() || self.observation.terminal_outcome.is_some() {
            return PlayPauseOffer::Play;
        }
        if self.observation.pause_requested {
            PlayPauseOffer::Resume
        } else {
            PlayPauseOffer::Pause
        }
    }

    /// Whether a relative seek step is COMPUTABLE right now (T0:
    /// visible relative-seek controls require published position/rate;
    /// G1 F07): exactly the evidence [`seek_target`] consumes. Without
    /// it the seek buttons render nothing, publish no regions and stay
    /// out of the focus cycle.
    pub fn relative_seek_available(&self) -> bool {
        let Some(format) = &self.observation.source_format else {
            return false;
        };
        format.sample_rate > 0 && self.observation.position.is_some()
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
    /// position of its own (no `last_position`, no local playback
    /// truth). The live pre-first-sample window keeps the dashes too
    /// (T0 unknown-position): an unknown position is not a zero, and the
    /// display must not promote a start-of-track shortcut into position
    /// evidence.
    pub fn timeline_label(&self) -> String {
        crate::status::format_timeline(&self.observation)
    }

    /// The progress bar (Issue #166 §33):
    /// `00:42 ━━━━━╸────────── 05:47`. `None` unless BOTH sides have
    /// evidence: an unknown duration has no percentage to draw and an
    /// unknown position is not a zero, so the bar simply does not
    /// appear — it is never fabricated, not even in the live
    /// pre-first-sample window after an Open replacement (T0
    /// unknown-position; see [`Self::timeline_label`]). Since G1 the
    /// drawn glyph is ALSO a click-to-position affordance: a seek-bar
    /// hit region over exactly its cells, and only while duration
    /// evidence exists (see the view's `seek_bar_glyph_area`).
    ///
    /// The fill is the position's
    /// fraction of the reported duration, clamped into the bar. The
    /// duration is mechanism evidence and the position an independent
    /// projection, so a position beyond the reported duration is
    /// representable; it clamps to a full bar rather than overflowing,
    /// which is the honest degradation of a display that cannot show
    /// "more than all of it".
    pub fn position_bar_label(&self) -> Option<String> {
        let rate = u64::from(self.observation.source_format?.sample_rate);
        if rate == 0 {
            return None;
        }
        let position_secs = self.observation.position? / rate;
        let duration_secs = self.observation.source_duration?.as_secs();

        let filled = if duration_secs > 0 {
            let width = BAR_WIDTH as u128;
            let filled = u128::from(position_secs) * width / u128::from(duration_secs);
            usize::try_from(filled.min(width)).unwrap_or(BAR_WIDTH)
        } else {
            0
        };
        let mut bar = String::with_capacity(BAR_WIDTH);
        for cell in 0..BAR_WIDTH {
            bar.push(match cell.cmp(&filled) {
                std::cmp::Ordering::Less => '━',
                std::cmp::Ordering::Equal => '╸',
                std::cmp::Ordering::Greater => '─',
            });
        }
        let total = crate::status::format_clock(Duration::from_secs(duration_secs));
        Some(format!(
            "{} {bar} {}",
            crate::status::format_clock(Duration::from_secs(position_secs)),
            total
        ))
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
#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use crate::tui::model::testutil::pending;
    use crate::tui::model::*;

    use qianqian_playback::PlaybackSessionObservation;

    /// The playlist rows are revision-gated: a revision the model
    /// already holds does not even build them, which is what keeps a
    /// huge playlist off the per-frame path (Issue #166 §21).
    #[test]
    fn the_playlist_rows_rebuild_only_when_the_revision_moves() {
        let mut model = TuiModel::new("song.flac");
        assert!(model.playlist().is_empty());

        model.set_playlist(7, || {
            vec![PlaylistRow {
                label: "first.flac".to_owned(),
                playing: true,
                selected: true,
            }]
        });
        assert_eq!(model.playlist().len(), 1);

        // The same revision: the closure must not even run.
        model.set_playlist(7, || {
            panic!("an unchanged revision must not rebuild the rows")
        });

        model.set_playlist(8, Vec::new);
        assert!(model.playlist().is_empty());
    }

    /// The model follows the player's committed episode; a no-episode
    /// model drops the retired episode's diagnostics with it.
    #[test]
    fn the_model_follows_the_player_committed_episode() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.source(), Some("song.flac"));
        model.set_episode(Some("/media/b.flac".to_owned()));
        assert_eq!(model.source(), Some("/media/b.flac"));
        model.update(PlaybackSessionObservation {
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        model.set_episode(None);
        assert_eq!(model.source(), None, "no episode is a real F6 state");
        assert!(
            model.diagnostics().is_empty(),
            "no episode, no episode diagnostics"
        );
    }
}
