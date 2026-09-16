//! ratatui rendering of one [`TuiModel`]: three panels, only earned
//! information.
//!
//! This function is presentation only. It reads the model and draws;
//! it performs no I/O, resolves no capability, and observes nothing —
//! the runtime owns the single `observe()` read per refresh. Rendering
//! is exercised on ratatui's `TestBackend`, so the layout and the exact
//! vocabulary are pinned without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::model::TuiModel;

/// Shown once the episode's terminal Fact is committed: the shell
/// stays up so the outcome can be inspected; only the quit command
/// remains meaningful.
pub const COMMITTED_HINT: &str = "terminal outcome committed; press Q to quit";

/// Render one frame of the reference player.
pub fn draw(frame: &mut Frame, model: &TuiModel) {
    let [main, diagnostics, controls] = Layout::vertical([
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    frame.render_widget(main_panel(model), main);
    frame.render_widget(diagnostics_panel(model), diagnostics);
    frame.render_widget(controls_panel(), controls);
}

fn bold(title: &'static str) -> Span<'static> {
    Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
}

fn main_panel(model: &TuiModel) -> Paragraph<'_> {
    let mut lines = vec![
        Line::from(""),
        Line::from(format!("Source: {}", model.source())),
        Line::from(format!("Format: {}", model.format_label())),
        Line::from(""),
        Line::from(format!("Terminal: {}", model.terminal_label())),
        Line::from(format!(
            "Stop requested: {}",
            model.observation().stop_requested
        )),
    ];
    if model.terminal_committed() {
        lines.push(Line::from(""));
        lines.push(Line::from(COMMITTED_HINT));
    }
    Paragraph::new(lines).block(
        Block::bordered()
            .title(bold(" Qianqian Reference Player "))
            .title_style(Style::default()),
    )
}

fn diagnostics_panel(model: &TuiModel) -> Paragraph<'_> {
    let mut lines: Vec<Line> = model.diagnostics().into_iter().map(Line::from).collect();
    if lines.is_empty() {
        lines.push(Line::from("(none)"));
    }
    Paragraph::new(lines).block(Block::bordered().title(bold(" Diagnostics ")))
}

fn controls_panel() -> Paragraph<'static> {
    Paragraph::new(vec![Line::from(" S  Stop    Q  Quit    Ctrl+C  Quit")])
        .block(Block::bordered().title(bold(" Controls ")))
}
