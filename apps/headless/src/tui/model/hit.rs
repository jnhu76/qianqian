//! Hit regions: what THIS frame actually drew, the semantic target of
//! each rectangle, and the armed-click interaction between Left Down
//! and Left Up.

use ratatui::layout::Rect;

use super::actions::TuiRoute;
use super::controls::{PreferenceButton, SeekButton, TransportButton};
use super::modal::ModalButton;

/// The semantic target of one hit region: a rendered, currently valid
/// control (§13). No widget tree, no DOM, no retained component graph —
/// only what THIS frame actually drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    RouteTab(TuiRoute),
    /// One visible seek button (G1 F07) — present only while the
    /// episode's position/rate evidence makes the step computable.
    Seek(SeekButton),
    Transport(TransportButton),
    /// One of the preference-row controls.
    Preference(PreferenceButton),
    /// A playlist row, by absolute traversal index.
    PlaylistRow(usize),
    /// The playlist list's content area (the wheel-scroll target).
    PlaylistPane,
    /// The position bar WITH duration evidence: a click-to-position
    /// seek affordance (G1 §9). Without duration evidence the bar is a
    /// display and publishes no region at all.
    SeekBar,
    /// One visible row of the Open picker's listing, by list index
    /// (index 0 is the synthesized `..` parent row when shown). The
    /// parent row is navigation chrome, not a selectable target: its
    /// activation is the parent step ([`ModalInput::ListParent`]).
    PickerRow(usize),
    /// The Open picker's path-field row (a click focuses the field).
    ModalField,
    /// One of the Open picker's visible buttons.
    ModalButton(ModalButton),
}

/// One frame's hit region: a rectangle plus the semantic target drawn
/// into it (§13). Regions belong to ONE rendered presentation state and
/// are republished (or dropped) by every draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitRegion {
    pub area: Rect,
    pub target: HitTarget,
}

/// An armed pointer interaction: the control under the pointer AND the
/// cell the press landed on (§17, G1 F06/F15). The frozen validity rule
/// is ONE rule, not a pile of per-event patches:
///
/// > An armed interaction activates only when Left Up lands on the SAME
/// > cell it was armed on AND the hit test still answers that cell with
/// > the SAME semantic target. Everything else is inert.
///
/// Semantic invalidation is carried by the model's own mutators: a
/// route/modal change, a refreshed listing, an EPISODE REPLACEMENT, a
/// playlist REVISION change or a resize each call
/// [`TuiModel::invalidate_frame`], which drops the arm together with
/// the frame's regions — so an armed press can never activate into a
/// context it was not armed in, even when the new context draws the
/// same control at the same coordinates. An ordinary redraw that
/// changes none of those (the per-tick refresh) keeps the arm: holding
/// a button across a position update is not a context change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmedClick {
    pub target: HitTarget,
    pub column: u16,
    pub row: u16,
}
