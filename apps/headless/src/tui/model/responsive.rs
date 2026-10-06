//! The responsive shell classes and the supported minimum (§27/§28).
//!
//! T0 freezes FOUR operable classes — `WIDE | NORMAL | COMPACT |
//! MINIMUM` — with `MINIMUM` the smallest FULLY OPERABLE layout, and a
//! distinct below-minimum too-small state underneath them. They are
//! one taxonomy: every class keeps focus, hit regions and the
//! persistent controls; only the layout/spelling tuning differs.

/// The responsive shell class (§27). Exact thresholds are presentation
/// tuning, not authority; they exist so controls do not overlap, focus
/// targets stay visible, and resize produces fresh geometry.
///
/// All four classes are operable. `Minimum` is the smallest: the same
/// interactive shell at its tightest tuning (two-row navigation, short
/// control spellings). Below [`MIN_WIDTH`]/[`MIN_HEIGHT`] there is no
/// class at all — [`ShellFit::TooSmall`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsiveClass {
    Wide,
    Normal,
    Compact,
    /// The smallest fully operable layout: everything still focuses,
    /// answers the mouse and carries the persistent Help/Quit pair —
    /// only the tuning is tighter (§28 names this the supported
    /// minimum, not an error page).
    Minimum,
}

/// The shell's classified fit for one terminal size: one of the four
/// operable classes, or the distinct below-minimum state (§28). This —
/// not a fifth class — is what a terminal smaller than the supported
/// minimum gets: one truthful message, no interactive layout, no hit
/// regions, no focus. Playback is untouched; resizing back restores
/// the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFit {
    /// Below the supported minimum geometry (§28).
    TooSmall,
    /// An operable responsive class.
    Operable(ResponsiveClass),
}

/// Below this width (or [`MIN_HEIGHT`] height) the shell refuses to
/// render the interactive layout (§28). Playback continues under the
/// product's own semantics; resizing back restores the UI.
pub const MIN_WIDTH: u16 = 40;
/// See [`MIN_WIDTH`].
pub const MIN_HEIGHT: u16 = 14;

/// Derive the shell fit for one terminal size (§27/§28). The bands
/// preserve the shell's rendering history end to end: the smallest
/// operable band keeps the tightest tuning, and every wider band keeps
/// the layout it has always drawn at that width.
pub fn shell_fit(width: u16, height: u16) -> ShellFit {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        ShellFit::TooSmall
    } else if width < 60 || height < 18 {
        ShellFit::Operable(ResponsiveClass::Minimum)
    } else if width < 100 {
        ShellFit::Operable(ResponsiveClass::Compact)
    } else if width < 120 {
        ShellFit::Operable(ResponsiveClass::Normal)
    } else {
        ShellFit::Operable(ResponsiveClass::Wide)
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use crate::tui::model::*;

    #[test]
    fn the_shell_fit_follows_the_terminal_size() {
        assert_eq!(shell_fit(20, 8), ShellFit::TooSmall);
        assert_eq!(shell_fit(MIN_WIDTH - 1, 30), ShellFit::TooSmall);
        assert_eq!(shell_fit(100, MIN_HEIGHT - 1), ShellFit::TooSmall);
        assert_eq!(
            shell_fit(MIN_WIDTH, MIN_HEIGHT),
            ShellFit::Operable(ResponsiveClass::Minimum),
            "the supported minimum itself is operable, not an error page"
        );
        assert_eq!(
            shell_fit(50, 16),
            ShellFit::Operable(ResponsiveClass::Minimum)
        );
        assert_eq!(
            shell_fit(80, 24),
            ShellFit::Operable(ResponsiveClass::Compact)
        );
        assert_eq!(
            shell_fit(110, 30),
            ShellFit::Operable(ResponsiveClass::Normal)
        );
        assert_eq!(
            shell_fit(120, 40),
            ShellFit::Operable(ResponsiveClass::Wide)
        );
    }

    // ------------------------------------------------------------------
    // G1: the Open picker grammar, the seek bar, the DSP summary.
    // ------------------------------------------------------------------
}
