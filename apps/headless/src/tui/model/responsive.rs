//! The responsive shell classes and the supported minimum (§27/§28).

/// The responsive shell class (§27). Exact thresholds are presentation
/// tuning, not authority; they exist so controls do not overlap, focus
/// targets stay visible, and resize produces fresh geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsiveClass {
    Wide,
    Normal,
    Compact,
    /// Below the implementation's minimum: no interactive layout, no
    /// hit regions, no focus — one truthful message instead (§28).
    Minimum,
}

/// Below this width (or [`MIN_HEIGHT`] height) the shell refuses to
/// render the interactive layout (§28). Playback continues under the
/// product's own semantics; resizing back restores the UI.
pub const MIN_WIDTH: u16 = 40;
/// See [`MIN_WIDTH`].
pub const MIN_HEIGHT: u16 = 14;

/// Derive the responsive class for one terminal size (§27).
pub fn responsive_class(width: u16, height: u16) -> ResponsiveClass {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        ResponsiveClass::Minimum
    } else if width < 60 || height < 18 {
        ResponsiveClass::Compact
    } else if width < 100 {
        ResponsiveClass::Normal
    } else {
        ResponsiveClass::Wide
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use crate::tui::model::*;

    #[test]
    fn the_responsive_class_follows_the_terminal_size() {
        assert_eq!(responsive_class(20, 8), ResponsiveClass::Minimum);
        assert_eq!(
            responsive_class(MIN_WIDTH - 1, 30),
            ResponsiveClass::Minimum
        );
        assert_eq!(
            responsive_class(100, MIN_HEIGHT - 1),
            ResponsiveClass::Minimum
        );
        assert_eq!(responsive_class(50, 16), ResponsiveClass::Compact);
        assert_eq!(responsive_class(80, 24), ResponsiveClass::Normal);
        assert_eq!(responsive_class(120, 40), ResponsiveClass::Wide);
    }

    // ------------------------------------------------------------------
    // G1: the Open picker grammar, the seek bar, the DSP summary.
    // ------------------------------------------------------------------
}
