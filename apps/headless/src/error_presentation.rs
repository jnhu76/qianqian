//! App-owned, read-only projection from established error truth to user copy.
//!
//! This module does not establish failures or choose recovery. It consumes the
//! existing D14.6 Open result or a D11 terminal observation, and it keeps
//! diagnostic text separate from the stable presentation kind. In particular,
//! diagnostic prefixes such as `decode:` and `device:` are never dispatch
//! inputs.

use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionObservation};

use crate::player::OpenOutcome;

/// The four error distinctions that current product authority can prove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorPresentationKind {
    OpenRefused,
    ActivationFailedClean,
    EpisodeFailed,
    FailStop,
}

/// A user-facing explanation plus the diagnostic text that supported it.
///
/// `technical_detail` retains the diagnostic verbatim for inspection and
/// copying. Renderers must use [`Self::technical_detail_for_display`] when
/// placing untrusted text in a terminal or plain-text view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPresentation {
    kind: ErrorPresentationKind,
    title: &'static str,
    explanation: &'static str,
    technical_detail: Option<String>,
}

impl ErrorPresentation {
    pub fn kind(&self) -> ErrorPresentationKind {
        self.kind
    }

    pub fn title(&self) -> &'static str {
        self.title
    }

    pub fn explanation(&self) -> &'static str {
        self.explanation
    }

    pub fn technical_detail(&self) -> Option<&str> {
        self.technical_detail.as_deref()
    }

    /// Return the full diagnostic with control characters escaped.
    ///
    /// Printable Unicode, paths and length are preserved. Terminal controls,
    /// line separators and bidi direction controls are escaped. This function
    /// does not parse, classify, truncate or execute any part of the diagnostic.
    pub fn technical_detail_for_display(&self) -> Option<String> {
        self.technical_detail.as_deref().map(diagnostic_for_display)
    }
}

/// The display-safe form of one diagnostic string: the raw bytes are
/// untouched at the source; only the DISPLAY mapping escapes the
/// characters a terminal would act on or render invisibly. Every in-crate
/// surface that places a diagnostic in front of the user — the B1
/// details modal, the Now Playing panel's diagnostic lines, operation
/// feedback, picker notes — must go through this one mapping, so no
/// second, weaker escaping can drift next to it.
pub(crate) fn diagnostic_for_display(text: &str) -> String {
    escape_control_characters(text)
}

/// Project one D14.6 Open operation result. Successful Open has no error.
pub fn presentation_for_open(outcome: &OpenOutcome) -> Option<ErrorPresentation> {
    let (kind, title, explanation, diagnostic) = match outcome {
        OpenOutcome::Opened => return None,
        OpenOutcome::Refused { diagnostic } => (
            ErrorPresentationKind::OpenRefused,
            "无法打开所选音频",
            "所选音频未能打开，当前播放内容没有被替换。",
            diagnostic,
        ),
        OpenOutcome::ActivationFailedClean { diagnostic } => (
            ErrorPresentationKind::ActivationFailedClean,
            "无法开始播放",
            "新的播放没有成功建立；本次启动占用的资源已经清理，当前没有运行中的播放。",
            diagnostic,
        ),
        OpenOutcome::FailStop { diagnostic } => {
            let base = fail_stop_presentation();
            return Some(ErrorPresentation {
                technical_detail: Some(diagnostic.clone()),
                ..base
            });
        }
    };

    Some(ErrorPresentation {
        kind,
        title,
        explanation,
        technical_detail: Some(diagnostic.clone()),
    })
}

/// The FailStop presentation's user copy, available without an
/// `OpenOutcome` in hand: the §G.6 latch is visible to surfaces that
/// never ran the Open that latched it (the TUI's persistent banner and
/// its action gate), and they must render exactly the copy the latched
/// Open would project — the same static strings, one source.
pub fn fail_stop_presentation() -> ErrorPresentation {
    ErrorPresentation {
        kind: ErrorPresentationKind::FailStop,
        title: "播放器遇到严重的资源清理问题",
        explanation: "资源清理未能安全完成，当前进程不能继续打开音频。",
        technical_detail: None,
    }
}

/// The bounded status block for one operation error, shared by every
/// shell surface that reports an `OpenOutcome` failure (the TUI
/// dispatch and the startup entry): the operation's own tag, then the
/// presentation's user copy — title and explanation. The raw
/// diagnostic deliberately does NOT ride the status line (it is
/// display-unsafe text; its display-safe form is the details modal's,
/// one `E` away), and no recovery advice is invented here.
pub fn operation_error_feedback(operation: &str, presentation: &ErrorPresentation) -> String {
    let tag = match presentation.kind() {
        ErrorPresentationKind::OpenRefused => "refused",
        ErrorPresentationKind::ActivationFailedClean => "failed (clean)",
        // EpisodeFailed is never an operation outcome (B1 projects it
        // only from a D11 observation); the arm exists so a future
        // misuse fails a test, not the user's trust.
        ErrorPresentationKind::EpisodeFailed => "failed",
        ErrorPresentationKind::FailStop => "FAIL-STOP",
    };
    format!(
        "{operation} {tag}: {}\n{}",
        presentation.title(),
        presentation.explanation()
    )
}

/// Project a D11 terminal observation. Only `Failed` is an error.
pub fn presentation_for_observation(
    observation: &PlaybackSessionObservation,
) -> Option<ErrorPresentation> {
    if observation.terminal_outcome != Some(EpisodeTerminalOutcome::Failed) {
        return None;
    }

    Some(ErrorPresentation {
        kind: ErrorPresentationKind::EpisodeFailed,
        title: "这首歌曲播放失败",
        explanation: "播放已失败，暂时无法确定具体原因。",
        technical_detail: observation.failure_diagnostic.clone(),
    })
}

fn escape_control_characters(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() || is_display_format_control(character) {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    escaped
}

fn is_display_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{034f}'
            | '\u{200b}'
            | '\u{200c}'
            | '\u{200d}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
    )
}

#[cfg(test)]
mod tests {
    use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};

    use super::{ErrorPresentationKind, presentation_for_observation, presentation_for_open};
    use crate::player::OpenOutcome;

    fn observation(
        terminal_outcome: Option<EpisodeTerminalOutcome>,
        failure_diagnostic: Option<&str>,
    ) -> PlaybackSessionObservation {
        PlaybackSessionObservation {
            terminal_outcome,
            failure_diagnostic: failure_diagnostic.map(str::to_owned),
            stop_requested: false,
            pause_requested: false,
            source_format: None,
            source_duration: None,
            position: None,
            pause_engagement: PauseEngagement::Disengaged,
            activation_error: None,
            last_processing_refusal: None,
        }
    }

    /// Test-quality record (M1 / high semantic risk):
    /// PROPERTY: #23 B1 maps only D14.6 Open outcomes and D11 Failed;
    /// diagnostics never select a kind. PRECONDITION: each row directly
    /// constructs one real production input. LEGAL OUTCOMES: the four error
    /// kinds below, or None for success/non-failure terminal states.
    /// OBSERVATION: the pure App-owned projection return. FALSIFIER: parsing
    /// `decode:`/`device:` or conflating Open with D11 changes a row and fails;
    /// the prefix mutation is executed separately. EXECUTION: ordinary host,
    /// no timing or platform assumptions; cargo runs every row non-empty.
    #[test]
    fn open_outcomes_map_without_forging_episode_failure() {
        let refused = presentation_for_open(&OpenOutcome::Refused {
            diagnostic: "unsupported source".to_owned(),
        })
        .expect("refusal is user-presentable");
        assert_eq!(refused.kind(), ErrorPresentationKind::OpenRefused);
        assert_eq!(refused.title(), "无法打开所选音频");
        assert_eq!(refused.technical_detail(), Some("unsupported source"));

        let activation = presentation_for_open(&OpenOutcome::ActivationFailedClean {
            diagnostic: "output activation failed".to_owned(),
        })
        .expect("clean activation failure is user-presentable");
        assert_eq!(
            activation.kind(),
            ErrorPresentationKind::ActivationFailedClean
        );
        assert!(activation.explanation().contains("新的播放没有成功建立"));
        assert!(activation.explanation().contains("当前没有运行中的播放"));
        assert!(!activation.explanation().contains("之前的播放"));
        assert!(!activation.explanation().contains("仍在播放"));

        let fail_stop = presentation_for_open(&OpenOutcome::FailStop {
            diagnostic: "cleanup contract violated".to_owned(),
        })
        .expect("fail-stop is user-presentable");
        assert_eq!(fail_stop.kind(), ErrorPresentationKind::FailStop);
        assert_eq!(fail_stop.title(), "播放器遇到严重的资源清理问题");
        assert!(fail_stop.explanation().contains("不能继续打开音频"));
        assert!(!fail_stop.explanation().contains("重试"));

        assert_eq!(presentation_for_open(&OpenOutcome::Opened), None);
    }

    #[test]
    fn only_failed_terminal_facts_create_episode_presentations() {
        let failed = presentation_for_observation(&observation(
            Some(EpisodeTerminalOutcome::Failed),
            Some("decode: pending decode error"),
        ))
        .expect("D11 Failed is user-presentable");
        assert_eq!(failed.kind(), ErrorPresentationKind::EpisodeFailed);
        assert_eq!(failed.title(), "这首歌曲播放失败");
        assert_eq!(
            failed.technical_detail(),
            Some("decode: pending decode error")
        );

        let unknown =
            presentation_for_observation(&observation(Some(EpisodeTerminalOutcome::Failed), None))
                .expect("Failed remains presentable without a diagnostic");
        assert!(unknown.explanation().contains("暂时无法确定具体原因"));
        assert_eq!(unknown.technical_detail(), None);

        for terminal in [
            None,
            Some(EpisodeTerminalOutcome::Completed),
            Some(EpisodeTerminalOutcome::Stopped),
        ] {
            assert_eq!(
                presentation_for_observation(&observation(terminal, Some("device: spoof"))),
                None
            );
        }
    }

    #[test]
    fn diagnostic_prefixes_never_dispatch_the_presentation_kind() {
        for diagnostic in [
            "device: endpoint unavailable",
            "decode: corrupt file (unverified)",
            "processing: bad coefficient",
            "device: decode: processing: all spoofed",
        ] {
            let presentation = presentation_for_observation(&observation(
                Some(EpisodeTerminalOutcome::Failed),
                Some(diagnostic),
            ))
            .expect("the terminal fact, not the diagnostic, creates the presentation");
            assert_eq!(presentation.kind(), ErrorPresentationKind::EpisodeFailed);
            assert_eq!(presentation.technical_detail(), Some(diagnostic));
        }
    }

    #[test]
    fn technical_detail_preserves_raw_unicode_paths_and_length_but_escapes_controls_for_display() {
        let long_tail = "错".repeat(8_192);
        let diagnostic = format!(
            "decode: C:\\音乐\\张韶涵\\safe\u{202e}gpj.exe.flac\n\r\t{long_tail}\u{1b}[31m\0\u{7f}\u{85}\u{2066}x\u{2069}"
        );
        let presentation = presentation_for_observation(&observation(
            Some(EpisodeTerminalOutcome::Failed),
            Some(&diagnostic),
        ))
        .expect("Failed is presentable");

        assert_eq!(presentation.technical_detail(), Some(diagnostic.as_str()));
        let display = presentation
            .technical_detail_for_display()
            .expect("diagnostic has display text");
        assert!(display.contains("C:\\音乐\\张韶涵\\safe\\u{202e}gpj.exe.flac"));
        assert!(display.contains(&long_tail));
        assert!(display.contains("\\n\\r\\t"));
        assert!(display.contains("\\u{1b}[31m\\u{0}\\u{7f}\\u{85}"));
        assert!(display.contains("\\u{2066}x\\u{2069}"));
        for control in [
            '\n', '\r', '\t', '\u{1b}', '\0', '\u{7f}', '\u{85}', '\u{202e}', '\u{2066}',
            '\u{2069}',
        ] {
            assert!(!display.contains(control));
        }
    }

    /// B2 display hardening (Issue #23 §4.3): the invisible-Unicode
    /// vocabulary a terminal cannot show — zero-width, word-joining and
    /// obsolete-directional controls — escapes in the display mapping
    /// while the raw diagnostic keeps its bytes, and ordinary CJK /
    /// long no-space / non-ASCII text survives untouched.
    #[test]
    fn invisible_unicode_escapes_for_display_while_normal_text_survives() {
        let invisible = [
            '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}', '\u{034f}', '\u{206a}',
            '\u{206b}', '\u{206c}', '\u{206d}', '\u{206e}', '\u{206f}',
        ];
        let diagnostic = format!("decode: a{}z", invisible.iter().collect::<String>());
        let presentation = presentation_for_observation(&observation(
            Some(EpisodeTerminalOutcome::Failed),
            Some(&diagnostic),
        ))
        .expect("Failed is presentable");
        // The raw fact keeps every byte.
        assert_eq!(presentation.technical_detail(), Some(diagnostic.as_str()));
        let display = presentation
            .technical_detail_for_display()
            .expect("display text");
        for character in invisible {
            let code = character as u32;
            assert!(
                !display.contains(character),
                "U+{code:04X} must not survive into the display: {display:?}"
            );
            assert!(
                display.contains(&format!("\\u{{{:x}}}", code)),
                "U+{code:04X} must be VISIBLE as its escape: {display:?}"
            );
        }

        // Normal text is not mangled: CJK, accents, spaces and a long
        // no-space run all survive verbatim.
        let normal = "音乐/夜曲.flac — café, §12, ﬁ; 0123456789";
        let long = "Z".repeat(4_096);
        let safe = format!("{normal}{long}");
        let presentation = presentation_for_observation(&observation(
            Some(EpisodeTerminalOutcome::Failed),
            Some(&safe),
        ))
        .expect("presentable");
        let display = presentation
            .technical_detail_for_display()
            .expect("display text");
        assert_eq!(display, safe, "ordinary text passes through unchanged");
    }

    #[test]
    fn a_new_episode_observation_cannot_inherit_the_old_episode_error() {
        let old = presentation_for_observation(&observation(
            Some(EpisodeTerminalOutcome::Failed),
            Some("old episode failure"),
        ));
        assert!(old.is_some());

        let replacement = observation(None, None);
        assert_eq!(presentation_for_observation(&replacement), None);
    }

    /// The shared feedback formatter switches on the presentation's
    /// KIND, never on any diagnostic text — the EpisodeFailed arm
    /// included, so a future misuse (feeding a D11 presentation into
    /// an operation slot) produces a visible, test-pinned tag instead
    /// of silently passing.
    #[test]
    fn the_feedback_tags_come_from_the_kind_not_the_text() {
        use super::operation_error_feedback;
        use crate::player::OpenOutcome;

        let failed = presentation_for_observation(&observation(
            Some(EpisodeTerminalOutcome::Failed),
            Some("decode: whatever"),
        ))
        .expect("Failed presents");
        assert_eq!(
            operation_error_feedback("replay", &failed),
            "replay failed: 这首歌曲播放失败\n播放已失败，暂时无法确定具体原因。",
            "EpisodeFailed carries its own tag — the diagnostic text selects nothing"
        );

        let stop = presentation_for_open(&OpenOutcome::FailStop {
            diagnostic: "old episode teardown violated".to_owned(),
        })
        .expect("FailStop presents");
        assert!(operation_error_feedback("open", &stop).starts_with("open FAIL-STOP: "));
    }
}
