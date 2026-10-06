//! ratatui rendering of the shell: the persistent top navigation, the
//! active route's body, and the bottom status line — plus the frame's
//! hit regions, published from the SAME layout decisions that drew the
//! controls (one geometry calculation, two readers).
//!
//! This module is presentation only: it reads the model and draws,
//! performs no I/O, and observes nothing. It is also the one place
//! geometry exists: every control is rendered from the same `Rect`
//! that is published as its [`HitRegion`], so a hit test can never
//! disagree with what is on screen. Rendering is exercised on ratatui's
//! `TestBackend`, so the layout and the exact vocabulary are pinned
//! without a real terminal.
//!
//! The route bodies live in the sibling modules ([`now_playing`],
//! [`playlist`], the Audio workbench [`audio`], and the Observation
//! Plane displays of [`visualizer`]); the popups live in [`modal`].

mod audio;
mod modal;
mod now_playing;
mod playlist;
mod visualizer;

#[cfg(test)]
mod testutil;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::model::{
    FocusId, HitRegion, HitTarget, MAX_STATUS_ROWS, NAV_BUTTONS, NavBarButton, ResponsiveClass,
    TuiModel, TuiRoute, responsive_class, status_shape,
};

use audio::draw_audio;
use modal::{draw_modal, modal_regions};
use now_playing::draw_now_playing;
use playlist::draw_playlist;
use visualizer::draw_visualizer;

/// Shown once the episode's terminal Fact is committed: the shell
/// stays up so the outcome can be inspected; the player keys stay
/// meaningful (the App's EOF policy may already have moved on to the
/// next entry).
pub const COMMITTED_HINT: &str = "terminal outcome committed";

/// The idle page's one honest claim: no episode exists (a fresh
/// interactive launch, or a clean-failed Open), so nothing is playing
/// and nothing is loaded. The panel fabricates no label beyond it.
pub const NO_MUSIC_LINE: &str = "No music loaded.";

/// The below-minimum page (§28): no interactive layout, no hit
/// regions, no focus — one truthful line, and playback continues under
/// the product's own semantics.
pub const TOO_SMALL_LINE: &str = "Terminal too small.";

/// The play marker: the committed (playing) row. Deliberately a shape,
/// not a colour — the shell must stay readable in a monochrome
/// terminal (Issue #166 §20).
pub(super) const PLAYING_MARKER: &str = "▶";

/// The selection marker: the UI cursor row. Independent of the play
/// marker; one row may carry both.
const SELECTED_MARKER: &str = ">";
/// Render one frame of the shell, and publish the frame's hit regions
/// into the model from the same layout this draw used.
pub fn draw(frame: &mut Frame, model: &mut TuiModel) {
    let area = frame.area();
    let class = responsive_class(area.width, area.height);
    model.set_class(class);
    // The frame starts with no geometry; everything published below is
    // drawn THIS frame (§15: stale coordinates never survive).
    let mut regions: Vec<HitRegion> = Vec::new();
    // Focus is validated against the visible enabled controls of THIS
    // frame's class (§12/§29).
    model.validate_focus();

    if class == ResponsiveClass::Minimum {
        render_too_small(frame, area);
        model.publish_regions(regions);
        return;
    }

    // The modal's regions are PUBLISHED first, so the first-match hit
    // test answers a click inside the popup with the modal's own
    // control and never with the background under it (§25). The modal
    // is still PAINTED last, over everything (§23) — publication order
    // and paint order are deliberately independent decisions.
    if let Some(modal) = model.modal() {
        modal_regions(
            modal,
            area,
            model.class() == ResponsiveClass::Compact,
            &mut regions,
        );
    }

    let status_rows = status_line_count(model);
    let [tabs, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(status_rows as u16),
    ])
    .areas(area);

    draw_tabs(frame, model, tabs, &mut regions);
    match model.route() {
        TuiRoute::NowPlaying => draw_now_playing(frame, model, body, &mut regions),
        TuiRoute::Playlist => draw_playlist(frame, model, body, &mut regions),
        TuiRoute::Audio => draw_audio(frame, model, body, &mut regions),
        TuiRoute::Visualizer => draw_visualizer(frame, model, body, &mut regions),
    }
    draw_status(frame, model, status);

    // The modal paints last, over everything (§23).
    if let Some(modal) = model.modal() {
        draw_modal(frame, modal, area, model);
    }

    model.publish_regions(regions);
}

/// The below-minimum page (§28): truthful, non-interactive, and with
/// NO hit regions published — the shell fabricates neither controls
/// nor silence about playback.
fn render_too_small(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(TOO_SMALL_LINE),
        Line::from("Enlarge the window; playback continues."),
    ];
    let height = 4;
    let width = area.width.min(60);
    let box_area = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height.min(area.height),
    );
    frame.render_widget(Clear, box_area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(bold(" Qianqian "))),
        box_area,
    );
}

/// The persistent top navigation (§6, G5): the four frozen route tabs,
/// then the two persistent application controls — Help and Quit —
/// pinned to the bar's right end on every route and in every class
/// above the minimum. The active route is bold, the focused control is
/// inverted — two shapes a monochrome terminal can tell apart. Every
/// control's cell is published as its hit region from the very rect
/// it was drawn into.
fn draw_tabs(frame: &mut Frame, model: &TuiModel, area: Rect, regions: &mut Vec<HitRegion>) {
    let compact = model.class() == ResponsiveClass::Compact;
    let label = |button: NavBarButton| {
        if compact {
            button.compact_label()
        } else {
            button.label()
        }
    };
    // The bar's right end: one measured cell per persistent control,
    // one spare column between and after them, over the route tabs'
    // remaining width. Plain arithmetic — the cells must never
    // overlap the tabs, whatever the terminal width.
    let nav_widths: Vec<u16> = NAV_BUTTONS
        .iter()
        .map(|button| label(*button).chars().count() as u16)
        .collect();
    let nav_total: u16 = nav_widths
        .iter()
        .sum::<u16>()
        .saturating_add(NAV_BUTTONS.len() as u16);
    let tabs_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(nav_total),
        area.height,
    );

    let cells: [Rect; 4] = Layout::horizontal([Constraint::Ratio(1, 4); 4]).areas(tabs_area);
    for (route, cell) in TuiRoute::ALL.iter().zip(cells.iter()) {
        let active = *route == model.route();
        let focused = model.focus() == Some(FocusId::RouteTab(*route));
        let label = if compact {
            route.compact_label()
        } else {
            route.label()
        };
        let mut style = Style::default();
        if active {
            style = style.add_modifier(Modifier::BOLD);
        }
        if focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        frame.render_widget(Paragraph::new(Line::styled(label, style)), *cell);
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::RouteTab(*route),
        });
    }
    let mut cell_x = area.x + area.width.saturating_sub(nav_total);
    for (index, button) in NAV_BUTTONS.iter().enumerate() {
        let cell = Rect::new(cell_x, area.y, nav_widths[index], area.height);
        cell_x += nav_widths[index] + 1;
        let focused = model.focus() == Some(FocusId::NavBar(*button));
        let mut style = Style::default();
        if focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        frame.render_widget(Paragraph::new(Line::styled(label(*button), style)), cell);
        regions.push(HitRegion {
            area: cell,
            target: HitTarget::NavBar(*button),
        });
    }
}
/// The bottom status/context line (§6): the latest operation feedback
/// (application composition feedback, never a playback semantic) and
/// one fixed keymap hint.
fn draw_status(frame: &mut Frame, model: &TuiModel, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(status) = model.status() {
        for line in status.lines().take(MAX_STATUS_ROWS) {
            lines.push(Line::from(line.to_owned()));
        }
    }
    lines.push(Line::from(HINT_LINE));
    frame.render_widget(Paragraph::new(lines), area);
}

/// The one-line keymap hint under the status feedback (§6's
/// "status / latest feedback" line). The FULL keymap lives in the Help
/// modal; this line only names the foundation keys.
const HINT_LINE: &str = "Tab=focus  Enter=activate  O=open  ?=help  Q=quit";

/// How many rows the bottom bar needs: the status block (capped) plus
/// the hint line — always at least one. The cap and the shape come
/// from the model (the same shape its `set_status` arm invalidation
/// compares).
fn status_line_count(model: &TuiModel) -> usize {
    status_shape(model.status()) + 1
}
/// The picker listing's first visible row: a STATELESS scroll that
/// keeps the cursor inside the listing window (Issue #166 §21). No
/// scroll offset is kept for the picker — the rule is a pure function
/// of the list length, the cursor and the window height, so no
/// presentation state can drift out of sync with what is on screen.
/// (The playlist pane is different: G2's owner ruling keeps an
/// explicit TUI-local viewport hint on the model, which the wheel
/// moves and the selection reveals.)
fn viewport_offset(len: usize, selected: Option<usize>, height: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let Some(selected) = selected else {
        return 0;
    };
    if selected < height {
        0
    } else {
        (selected + 1 - height).min(len - height)
    }
}
fn bold(title: &'static str) -> Span<'static> {
    Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use qianqian_audio_api::ports::PcmFormat;
    use qianqian_playback::PlaybackSessionObservation;
    use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement};

    use super::testutil::*;
    use super::*;
    use crate::tui::model::*;

    /// The persistent shell shows the four tabs on every route, and the
    /// route bodies differ (§6).
    #[test]
    fn the_shell_shows_the_four_frozen_tabs() {
        let mut model = plain_model();
        for route in TuiRoute::ALL {
            model.set_route(route);
            let text = rendered(&mut model, 100, 30);
            for tab in TuiRoute::ALL {
                assert!(
                    text.contains(tab.label()),
                    "{:?}: tab {:?} missing in:\n{text}",
                    route.label(),
                    tab.label()
                );
            }
        }
    }

    /// The active tab is bold; a focused tab renders (the REVERSED
    /// style does not change the text). The tab row carries all four
    /// labels in order.
    #[test]
    fn the_tab_row_carries_the_tabs_in_order() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Playlist);
        model.set_focus(Some(FocusId::RouteTab(TuiRoute::Playlist)));
        let text = rendered(&mut model, 100, 30);
        let now = text.find("Now Playing").expect("Now Playing tab");
        let playlist = text.find("Playlist").expect("Playlist tab");
        let audio = text.find("Audio").expect("Audio tab");
        let visualizer = text.find("Visualizer").expect("Visualizer tab");
        assert!(
            now < playlist && playlist < audio && audio < visualizer,
            "{text}"
        );
    }

    /// The Visualizer route without telemetry renders the honest
    /// unavailable panel (G4): no fabricated bars, no playback claim.
    /// The Audio route body is exercised in `view::audio`.
    #[test]
    fn the_visualizer_route_renders_honest_unavailable_without_telemetry() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Visualizer);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Visualization unavailable"), "{text}");
        assert!(text.contains("[Spectrum]"), "{text}");
        assert!(text.contains("[Peak-RMS]"), "{text}");
        assert!(text.contains("[Waveform]"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    // ------------------------------------------------------------------
    // The bottom status line.
    // ------------------------------------------------------------------

    /// The status feedback renders under the body, and the hint line is
    /// always present.
    #[test]
    fn the_status_line_renders_feedback_and_the_hint() {
        let mut model = plain_model();
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Tab=focus"), "{text}");

        model.set_status(Some("opened D:\\media (6 candidates)".to_owned()));
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("opened D:\\media (6 candidates)"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    // ------------------------------------------------------------------
    // The modal popups.
    // ------------------------------------------------------------------

    #[test]
    fn a_fresh_episode_renders_pending_without_inventing_state() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Source: song.flac"), "{text}");
        assert!(text.contains("Format: pending"), "{text}");
        assert!(
            text.contains("Position: --:-- / --:--"),
            "no timeline evidence yet — and no fabricated zero:\n{text}"
        );
        assert!(text.contains("Terminal: pending"), "{text}");
        assert!(text.contains("Stop requested: false"), "{text}");
        assert!(text.contains("Paused: false"), "{text}");
        // No unearned playback semantic may appear anywhere in the frame
        // (`Paused` is earned since D14.7 and pinned to the frozen
        // establishment conjunction by the model tests).
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The Paused line follows the seam's derived projection: recorded
    /// intent without the mechanism evidence must not display Paused;
    /// full establishment must.
    #[test]
    fn the_paused_line_follows_the_frozen_establishment_conjunction() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Pause requested: true"), "{text}");
        assert!(text.contains("Paused: false"), "intent alone: {text}");
        assert_eq!(scan(&text), None, "{text}");

        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("Paused: true"),
            "intent + engagement + tail quiescence: {text}"
        );
        assert!(
            !text.contains("Terminal: Paused"),
            "the projection is never a fourth terminal state: {text}"
        );
    }

    #[test]
    fn each_terminal_fact_renders_its_own_label() {
        for (outcome, label) in [
            (EpisodeTerminalOutcome::Completed, "Terminal: Completed"),
            (EpisodeTerminalOutcome::Stopped, "Terminal: Stopped"),
            (EpisodeTerminalOutcome::Failed, "Terminal: Failed"),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.update(PlaybackSessionObservation {
                terminal_outcome: Some(outcome),
                source_format: Some(PcmFormat {
                    sample_rate: 44100,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                ..pending()
            });
            let text = rendered(&mut model, 100, 30);
            assert!(text.contains(label), "{label:?} missing in:\n{text}");
            assert!(
                text.contains(COMMITTED_HINT),
                "committed hint missing in:\n{text}"
            );
            assert_eq!(scan(&text), None, "{text}");
        }
    }

    /// An activation failure and a published failure diagnostic both
    /// render (C13) — as presentation text, never as forged facts.
    #[test]
    fn diagnostics_render_as_diagnostics() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            activation_error: Some("render stream open failed: no device".to_owned()),
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("activation: render stream open failed: no device"),
            "{text}"
        );
        assert!(text.contains("failure: decode: corrupt frame"), "{text}");
        assert!(
            text.contains("Terminal: pending"),
            "no terminal Fact: {text}"
        );
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The pane shows no playing marker when no episode is live, and
    /// the idle page keeps the honest navigation line (labeled a
    /// cursor, never a playback claim).
    #[test]
    fn the_pane_shows_no_playing_marker_when_no_episode_is_live() {
        let mut model = TuiModel::new("song.flac");
        model.set_playlist(1, || {
            vec![
                PlaylistRow {
                    label: "flac4.flac".to_owned(),
                    playing: true,
                    selected: true,
                },
                PlaylistRow {
                    label: "alac4.m4a".to_owned(),
                    playing: false,
                    selected: false,
                },
            ]
        });
        model.set_navigation(Some((1, 2)));
        // The pane renders on the Playlist route; a live episode: the
        // committed row carries the marker.
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("▶ >   1  flac4.flac"), "{text}");

        // The same navigation state with no live episode (a clean-failed
        // Open): the cursor is still shown, the play marker is not.
        model.set_episode(None);
        let text = rendered(&mut model, 100, 30);
        assert!(!text.contains(PLAYING_MARKER), "{text}");
        assert!(text.contains("  >   1  flac4.flac"), "{text}");

        // The idle page on Now Playing keeps the honest navigation line.
        model.set_route(TuiRoute::NowPlaying);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(text.contains("Track: 1/2 (navigation cursor)"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The no-episode frame (F6; the U1 idle page) fabricates nothing —
    /// no Source/Format/Position/Terminal labels, and no operation
    /// feedback for an Open that was never attempted. The visible Open
    /// control is the discoverable entry (G1), so the idle page names
    /// it instead of a shortcut.
    #[test]
    fn a_no_episode_frame_says_so_and_fabricates_nothing() {
        let mut model = TuiModel::new("song.flac");
        model.set_episode(None);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(
            text.contains("Open a file or folder with the Open button below"),
            "{text}"
        );
        for fabricated in ["Source:", "Format:", "Position:", "Terminal:", "Paused:"] {
            assert!(
                !text.contains(fabricated),
                "{fabricated} must not render without an episode:\n{text}"
            );
        }
        assert_eq!(scan(&text), None, "{text}");
    }

    /// Open operation feedback renders as composition feedback — a
    /// refusal, a clean failure, or the fail-stop banner — and never
    /// as a playback claim. A multi-line status block renders every
    /// line (up to the bar's cap).
    #[test]
    fn operation_feedback_renders_as_composition_feedback() {
        for status in [
            "opened /media/b.flac".to_owned(),
            "open refused: unsupported container: /media/x.txt".to_owned(),
            "open failed (clean): decode open failed".to_owned(),
            "FAIL-STOP: old episode teardown violated (§G.6 latch; no exit)".to_owned(),
            "opened /media (2 candidates, 1 skipped; 1 scan warning)\n\
             scan warning: cannot read /media/album-c: access denied"
                .to_owned(),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.update(pending());
            model.set_status(Some(status.clone()));
            let text = rendered(&mut model, 100, 32);
            for line in status.lines().take(MAX_STATUS_ROWS) {
                assert!(text.contains(line), "{line:?} missing in:\n{text}");
            }
            assert_eq!(scan(&text), None, "{text}");
        }
    }

    /// The preference row renders the App's desired stream factor
    /// between its steppers (D14.9 read side: exactly the configured
    /// value, never an acoustic or mechanism claim).
    #[test]
    fn the_volume_line_renders_the_desired_factor() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        assert!(
            !rendered(&mut model, 100, 30).contains("(desired)"),
            "before the first refresh the model holds no level and renders none"
        );
        model.set_volume(Some(80));
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("80/100 (desired)"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    /// Below the minimum the shell renders the truthful message and
    /// publishes NO interactive geometry — no stale hit regions, no
    /// focus (§28).
    #[test]
    fn a_below_minimum_frame_has_no_interactive_geometry() {
        let mut model = plain_model();
        model.set_focus(Some(FocusId::Transport(TransportButton::PlayPause)));
        let text = rendered(&mut model, 30, 10);
        assert!(text.contains(TOO_SMALL_LINE), "{text}");
        assert!(
            !text.contains("Play/Pause"),
            "no interactive layout below the minimum:\n{text}"
        );
        assert!(
            model.regions().is_empty(),
            "below the minimum no hit regions survive: {:?}",
            model.regions()
        );
        assert_eq!(model.focus(), None, "no invisible focus below the minimum");
        assert_eq!(model.class(), ResponsiveClass::Minimum);
        assert_eq!(responsive_class(30, 10), ResponsiveClass::Minimum);
    }

    /// G1 re-review (B1): the exact-minimum Now Playing shell is
    /// pinned by SEMANTIC evidence, not just nonpanic — every
    /// interactive label renders whole at `MIN_WIDTH×MIN_HEIGHT` and
    /// every published region sits inside the frame. The picker's
    /// minimum has its own test above.
    #[test]
    fn the_minimum_shell_renders_every_interactive_label_whole() {
        let mut model = plain_model();
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH,
            crate::tui::model::MIN_HEIGHT,
        );
        assert_eq!(model.class(), ResponsiveClass::Compact);
        for label in [
            // Navigation (compact spellings).
            "Now",
            "List",
            "Viz",
            // Transport.
            "Open",
            "Prev",
            "Pause",
            "Stop",
            "Next",
            // The visible relative seek (evidence exists here).
            "[ Back 5s ]",
            "[ Forward 5s ]",
            // Preferences (compact spellings + glyphs).
            "Ord:Seq",
            "Rep:Off",
            "100/100",
            "-",
            "+",
        ] {
            assert!(
                text.contains(label),
                "{label:?} is clipped at the supported minimum:\n{text}"
            );
        }
        // No region leaks outside the frame: every published hit target
        // is exactly what the minimum shell drew.
        let (width, height) = (crate::tui::model::MIN_WIDTH, crate::tui::model::MIN_HEIGHT);
        assert!(
            !model.regions().is_empty(),
            "the minimum shell publishes regions"
        );
        for region in model.regions() {
            let area = region.area;
            assert!(
                area.x < width
                    && area.y < height
                    && area.x + area.width <= width
                    && area.y + area.height <= height,
                "region {area:?} for {:?} escapes the minimum frame",
                region.target
            );
        }
    }

    /// The compact class shortens the tab labels — the mechanics exist
    /// so controls do not overlap on a narrow terminal (§27).
    #[test]
    fn the_compact_class_shortens_the_tab_labels() {
        let mut model = plain_model();
        let text = rendered(&mut model, 50, 16);
        assert!(text.contains("Now"), "{text}");
        assert!(text.contains("List"), "{text}");
        assert!(text.contains("Viz"), "{text}");
        assert_eq!(model.class(), ResponsiveClass::Compact);
    }

    /// A large → small → large resize sequence keeps the shell coherent
    /// at every step: rendering, fresh geometry, and a validated focus
    /// (§29's draw half; the event half lives in the runtime tests).
    #[test]
    fn a_resize_sequence_keeps_the_shell_coherent() {
        let mut model = plain_model();
        model.set_focus(Some(FocusId::Transport(TransportButton::PlayPause)));
        let large = rendered(&mut model, 100, 30);
        assert!(large.contains("Pause"), "{large}");
        assert_eq!(model.class(), ResponsiveClass::Wide);

        let small = rendered(&mut model, 30, 10);
        assert!(small.contains(TOO_SMALL_LINE), "{small}");
        assert_eq!(model.focus(), None, "below the minimum nothing is focused");

        let large_again = rendered(&mut model, 100, 30);
        assert!(
            large_again.contains("Pause"),
            "the shell is back after growing:\n{large_again}"
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Seek(crate::tui::model::SeekButton::Back)),
            "focus revalidates to a visible enabled control (§12: the route's \
             first local control — the seek row, whose evidence exists)"
        );
    }

    /// A full draw at a series of shrinking sizes — down to a 4×3
    /// window — never panics and never renders an unearned semantic
    /// (C10). Layout degrades by clipping; the truth classes live in
    /// the model, not in the geometry.
    #[test]
    fn tiny_terminals_degrade_without_panicking_or_fabricating() {
        let mut model = plain_model();
        model.update(stopped());
        model.set_status(Some("volume 75/100 (desired)".to_owned()));
        model.open_modal(ModalKind::Open);
        for c in "synth30.flac".chars() {
            model.modal_push(c);
        }
        for (width, height) in [(100u16, 24u16), (40, 12), (20, 8), (10, 6), (4, 3), (1, 1)] {
            let text = rendered(&mut model, width, height);
            assert_eq!(
                scan(&text),
                None,
                "unearned semantic at {width}x{height}:\n{text}"
            );
        }
    }

    /// The responsive pass over every route (§27): at the compact
    /// floor, the Normal boundary and the Wide boundary, each route
    /// renders its key controls WHOLE (the class's own spellings), the
    /// persistent Help/Quit buttons render whole everywhere, the frame
    /// claims nothing unearned, and the hit geometry is published.
    #[test]
    fn every_route_renders_whole_at_the_class_boundaries() {
        // (route, labels for the COMPACT floor, labels for full width).
        let routes: [(TuiRoute, &[&str], &[&str]); 4] = [
            (
                TuiRoute::NowPlaying,
                &["Open", "Pause", "Ord:Seq", "100/100", "Help", "Quit"],
                &["Open", "Pause", "100/100 (desired)", "Help", "Quit"],
            ),
            (
                TuiRoute::Playlist,
                &["[+File]", "[+Folder]", "[Play]", "[Remove]", "[Clear]"],
                &[
                    "[Add File...]",
                    "[Add Folder...]",
                    "[Play selected]",
                    "[Clear...]",
                ],
            ),
            (
                TuiRoute::Audio,
                &[
                    "DSP",
                    "[Pre−]",
                    "[Pre+]",
                    "[Presets]",
                    "[Apply]",
                    "[Cancel]",
                ],
                &[
                    "[Preamp −]",
                    "[Preamp +]",
                    "[EQ preset...]",
                    "[Apply]",
                    "[Cancel]",
                ],
            ),
            (
                TuiRoute::Visualizer,
                &[
                    "[Spectrum]",
                    "[Levels]",
                    "[Wave]",
                    "Visualization unavailable",
                ],
                &[
                    "[Spectrum]",
                    "[Peak-RMS]",
                    "[Waveform]",
                    "Visualization unavailable",
                ],
            ),
        ];
        for (route, compact_labels, full_labels) in routes {
            // The compact floor: the class's own short spellings.
            let mut model = plain_model();
            model.set_route(route);
            let text = rendered(
                &mut model,
                crate::tui::model::MIN_WIDTH,
                crate::tui::model::MIN_HEIGHT,
            );
            assert_eq!(scan(&text), None, "{route:?} at 40x14:\n{text}");
            for label in compact_labels {
                assert!(
                    text.contains(label),
                    "{label:?} for {route:?} clipped at the compact floor:\n{text}"
                );
            }
            assert!(!model.regions().is_empty(), "{route:?} at 40x14");

            // The Normal boundary and the Wide boundary: full
            // spellings — including at 60 columns, where the playlist
            // toolbar's full row would not fit and wraps instead.
            for (width, height) in [(60u16, 18u16), (100u16, 30u16)] {
                let mut model = plain_model();
                model.set_route(route);
                let text = rendered(&mut model, width, height);
                assert_eq!(scan(&text), None, "{route:?} at {width}x{height}:\n{text}");
                for label in full_labels {
                    assert!(
                        text.contains(label),
                        "{label:?} for {route:?} clipped at {width}x{height}:\n{text}"
                    );
                }
                assert!(
                    text.contains("Help") && text.contains("Quit"),
                    "the persistent buttons clipped at {width}x{height}:\n{text}"
                );
                assert!(!model.regions().is_empty(), "{route:?} at {width}x{height}");
            }
        }
    }

    /// A very long source path clips at the panel edge without
    /// corrupting the rows below it (C11): the display truncates; the
    /// model keeps the full internal identity.
    #[test]
    fn a_long_source_path_clips_without_corrupting_other_rows() {
        let long_path = format!(
            "C:\\very\\long\\prefix\\{}\\season.takes.flac",
            "directory_component_".repeat(10)
        );
        let mut model = TuiModel::new(long_path.clone());
        model.update(pending());
        assert_eq!(model.source(), Some(long_path.as_str()), "identity intact");
        let text = rendered(&mut model, 100, 30);
        // The clipped Source line still names the beginning of the
        // path, and the panel below it is uncorrupted.
        assert!(text.contains("Source: C:\\very\\long\\prefix\\"), "{text}");
        assert!(text.contains("Format: pending"), "{text}");
        assert!(text.contains("◀ Prev"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    /// CJK filenames render structurally (C12): the Source line shows
    /// the characters, the Open modal accepts CJK characters and
    /// backspace pops ONE character, and the frame stays free of
    /// unearned semantics. The fixtures are SIMPLIFIED Chinese (the
    /// product's audience; QUICKSTART's examples are 夜曲/七里香/晴天)
    /// — wide glyphs are what the structural assertions need, and the
    /// spelling stays in the users' own script.
    #[test]
    fn cjk_filenames_render_and_edit_structurally() {
        let mut model = TuiModel::new("千曲测试曲.flac");
        model.update(pending());
        let text = rendered(&mut model, 100, 30);
        // Wide CJK glyphs occupy two cells; the skipped cells surface as
        // blanks in this test's per-cell text reconstruction (a real
        // terminal renders them as one glyph). Structural assertion is
        // on the space-normalized text.
        let compact = text.replace(' ', "");
        assert!(compact.contains("Source:千曲测试曲.flac"), "{text}");
        assert_eq!(scan(&text), None, "{text}");

        model.open_modal(ModalKind::Open);
        for c in "音乐/歌曲.flac".chars() {
            model.modal_push(c);
        }
        model.modal_backspace();
        let text = rendered(&mut model, 100, 30);
        let compact = text.replace(' ', "");
        assert!(
            compact.contains("音乐/歌曲.fla▏"),
            "backspace popped exactly one character:\n{text}"
        );
        assert!(text.contains("↑↓ select"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }
    // ------------------------------------------------------------------
    // G1: the seek bar affordance, the DSP summary line, the picker.
    // ------------------------------------------------------------------
}
