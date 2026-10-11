//! The shell's presentation palette (Issue #233): a small, fixed ANSI
//! 16-color vocabulary that separates the shell's visual families the
//! way the glyph shapes already do — chrome from content, the live row
//! from its neighbours, one meter zone from the next (the cava /
//! level-meter gradient idiom; cmus stays shape-only, which is exactly
//! the degrade this palette must survive).
//!
//! Monochrome safety (#166 §20) is structural, not stylistic: every
//! distinction below rides ON TOP of the shape or modifier that already
//! carries it (glyphs, BOLD, REVERSED), and a terminal without color
//! degrades these to nothing while keeping every distinction readable.
//! Colour is redundancy — never the only carrier of a meaning. The
//! palette uses only the named 16 ANSI colors so it inherits whatever
//! theme the host terminal (Windows Terminal included) already wears.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Interactive chrome: the active route/mode signal, panel titles and
/// the waveform trace — the shell's "look here" color.
pub(super) const ACCENT: Color = Color::Cyan;
/// Secondary chrome: borders, hints, empty-meter cells — structure that
/// should group content without competing with it.
pub(super) const CHROME: Color = Color::DarkGray;
/// The live signal: the committed (playing) playlist row.
pub(super) const LIVE: Color = Color::Green;
/// The meter's normal zone: the spectrum body, the RMS fill.
pub(super) const METER: Color = Color::Green;
/// The meter's hot zone: high spectrum rows, the peak marker.
pub(super) const HOT: Color = Color::Yellow;
/// The meter's overload zone: the topmost spectrum rows, above-unity
/// peaks — the "too loud" color, matching the '!' overload glyph.
pub(super) const OVERLOAD: Color = Color::Red;
/// The waveform trace (an oscilloscope's phosphor, distinct from the
/// accent only in name).
pub(super) const TRACE: Color = Color::Cyan;

/// A panel title: BOLD first (the monochrome carrier), the accent color
/// as redundancy — the successor of the old bold-only titles.
pub(super) fn title(text: &'static str) -> Span<'static> {
    Span::styled(
        text,
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )
}

/// A bordered panel's dim frame: the panel's content carries the
/// information; the border only groups it.
pub(super) fn border() -> Style {
    Style::default().fg(CHROME)
}

/// The spectrum bar color for one bar row by its height fraction (0 at
/// the panel's floor, 1 at the ceiling): the cava/level-meter gradient
/// — green body, yellow hot zone, red overload zone. Loud passages
/// shift color before they clip; the glyph keeps the bar itself
/// readable where color is absent.
pub(super) fn meter_zone(height_fraction: f32) -> Color {
    if height_fraction > 0.85 {
        OVERLOAD
    } else if height_fraction > 0.6 {
        HOT
    } else {
        METER
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::*;

    /// The gradient's zone boundaries: strictly above 0.85 is the
    /// overload zone, strictly above 0.6 the hot zone, everything below
    /// the normal zone (the exact boundary values belong to the zone
    /// below them).
    #[test]
    fn the_meter_gradient_has_three_zones_with_fixed_boundaries() {
        assert_eq!(meter_zone(0.0), METER);
        assert_eq!(meter_zone(0.5), METER);
        assert_eq!(meter_zone(0.6), METER, "0.6 exactly is still the body");
        assert_eq!(meter_zone(0.61), HOT);
        assert_eq!(meter_zone(0.85), HOT, "0.85 exactly is still hot");
        assert_eq!(meter_zone(0.86), OVERLOAD);
        assert_eq!(meter_zone(1.0), OVERLOAD);
    }

    /// The composed styles as they land in a real buffer (the fresh
    /// review's P2: the constants were pinned, the composition was
    /// not): a title cell is ACCENT over BOLD — BOLD is the §20
    /// monochrome carrier every panel title rides — and the frame is
    /// CHROME. Dropping the modifier from `title()` or recoloring
    /// `border()` fails here, at the source all nine call sites share.
    #[test]
    fn a_rendered_title_is_accent_over_bold_and_the_frame_is_chrome() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::widgets::Block;

        let mut terminal = Terminal::new(TestBackend::new(20, 6)).expect("virtual terminal");
        terminal
            .draw(|frame| {
                let block = Block::bordered().title(title("T")).border_style(border());
                frame.render_widget(block, frame.area());
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();

        // The default top-left title renders after the corner glyph.
        let title_cell = buffer.cell((1, 0)).expect("the title cell");
        assert_eq!(title_cell.symbol(), "T");
        assert_eq!(title_cell.fg, ACCENT, "the title is accent");
        assert!(
            title_cell.modifier.contains(Modifier::BOLD),
            "the title keeps its BOLD carrier"
        );

        let frame_cell = buffer.cell((0, 5)).expect("the frame cell");
        assert_eq!(frame_cell.symbol(), "└");
        assert_eq!(frame_cell.fg, CHROME, "the border is dim chrome");
    }

    /// The palette stays inside the named ANSI 16 colors — the whole
    /// point is inheriting the host terminal's theme instead of
    /// hard-coding RGB that fights it.
    #[test]
    fn the_palette_uses_only_the_named_ansi_colors() {
        for color in [ACCENT, CHROME, LIVE, METER, HOT, OVERLOAD, TRACE] {
            assert!(
                matches!(
                    color,
                    Color::Black
                        | Color::Red
                        | Color::Green
                        | Color::Yellow
                        | Color::Blue
                        | Color::Magenta
                        | Color::Cyan
                        | Color::Gray
                        | Color::DarkGray
                        | Color::LightRed
                        | Color::LightGreen
                        | Color::LightYellow
                        | Color::LightBlue
                        | Color::LightMagenta
                        | Color::LightCyan
                        | Color::White
                ),
                "{color:?} must be a named ANSI 16 color"
            );
        }
    }
}
