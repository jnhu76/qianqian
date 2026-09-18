//! Observable contract of the scriptable `--machine play` transport —
//! the automation interface consumed by pipes and gate scripts.
//!
//! Every report line and exit code a script can observe from the
//! transport is pinned here as pure data→presentation functions, in
//! the same spirit as [`crate::status`]: this module owns no truth and
//! performs no I/O. The binary renders THROUGH these functions, so its
//! behavior and the pinned contract cannot drift apart; the TUI
//! transport settles episodes through the same settlement contract
//! (outcome report, disposal warnings, exit code), which is what makes
//! the two adapters interchangeable at the automation boundary.
//!
//! Truth-class discipline (D14.2): an episode that never activated has
//! NO terminal Fact and none may be forged — the activation report is
//! a diagnostic, and the outcome report names only the three stable
//! terminal outcomes with the failure diagnostic on a separate line.

use std::process::ExitCode;

use qianqian_composition::{CompositionSnapshot, DisposeVerdict};
use qianqian_playback::EpisodeTerminalOutcome;

/// Why an episode never reached a running session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartFailure {
    /// Component registration refused.
    Registration {
        /// The full report line (names the component and the error).
        message: String,
    },
    /// Composition refused the desired revision.
    CompositionRefused {
        /// The composition errors as formatted by the kernel.
        errors: String,
    },
}

impl StartFailure {
    /// The full stderr report line for this failure.
    pub fn report(&self) -> String {
        match self {
            Self::Registration { message } => message.clone(),
            Self::CompositionRefused { errors } => format!("composition refused: {errors}"),
        }
    }

    /// The transport exit code for this failure: registration failure
    /// fails (1), a refused composition is a usage-class refusal (2).
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Registration { .. } => ExitCode::from(1),
            Self::CompositionRefused { .. } => ExitCode::from(2),
        }
    }
}

/// The activation-failure report: the session never activated, so the
/// transport reports the activation diagnostic the seam published — or,
/// when none was published (a required provider failed or is missing),
/// exactly that fact. Never upgraded into a terminal outcome.
pub fn activation_failure_report(diagnostic: Option<&str>) -> String {
    match diagnostic {
        Some(message) => format!("playback session failed to activate: {message}"),
        None => "playback session did not activate (a required capability provider \
                 failed or is missing on this platform)"
            .to_owned(),
    }
}

/// Which stream a report line belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportStream {
    Stdout,
    Stderr,
}

/// The outcome summary for a committed terminal Fact: the semantic line
/// exactly as the F1 physical gate has always matched it; a published
/// failure diagnostic rides on a separate stderr line — presentation
/// text, never part of the semantic outcome (D14.2).
pub fn outcome_report(
    outcome: EpisodeTerminalOutcome,
    failure_diagnostic: Option<&str>,
) -> Vec<(ReportStream, String)> {
    match outcome {
        EpisodeTerminalOutcome::Completed => {
            vec![(
                ReportStream::Stdout,
                "EOF: played out completely".to_owned(),
            )]
        }
        EpisodeTerminalOutcome::Stopped => {
            vec![(ReportStream::Stdout, "stopped before completion".to_owned())]
        }
        EpisodeTerminalOutcome::Failed => vec![match failure_diagnostic {
            Some(failure) => (ReportStream::Stderr, format!("playback failed: {failure}")),
            None => (ReportStream::Stderr, "playback failed".to_owned()),
        }],
    }
}

/// Teardown warnings a script must see from a disposal snapshot
/// (empty when the disposal was quiet). Printed on stderr.
/// The pinned verdict line for a root disposal that did NOT end
/// Discharged (F6): a latched §G.6 teardown violation has no exit, and
/// the report says so in exactly this spelling. `None` for a clean
/// discharge (the disposal report stays silent about success —
/// quietness is the success signal).
pub fn disposal_verdict_warning(verdict: &DisposeVerdict) -> Option<String> {
    match verdict {
        DisposeVerdict::Discharged => None,
        DisposeVerdict::TeardownViolated => {
            Some("fail-stop: disposal reported a latched teardown violation (no exit)".to_owned())
        }
    }
}

pub fn disposal_warnings(snapshot: &CompositionSnapshot) -> Vec<String> {
    if snapshot.quiet {
        return Vec::new();
    }
    let mut warnings = vec!["warning: disposal reported a latched teardown violation".to_owned()];
    for (name, fiber) in &snapshot.fibers {
        if fiber.teardown_violated {
            warnings.push(format!(
                "  fiber '{name}': teardown violated (state {:?})",
                fiber.state
            ));
        }
    }
    warnings
}

/// The transport exit code for a settled episode: Completed/Stopped
/// succeed only with a quiet disposal; a latched teardown violation or
/// a Failed outcome fails; an episode that never activated (no terminal
/// Fact to wait for) fails.
pub fn episode_exit_code(
    outcome: Option<EpisodeTerminalOutcome>,
    disposal_quiet: bool,
) -> ExitCode {
    match outcome {
        None => ExitCode::from(1),
        Some(EpisodeTerminalOutcome::Completed | EpisodeTerminalOutcome::Stopped) => {
            if disposal_quiet {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Some(EpisodeTerminalOutcome::Failed) => ExitCode::from(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_composition::{FiberDiagnostic, FiberState};
    use std::collections::{BTreeMap, BTreeSet};

    fn quiet_snapshot() -> CompositionSnapshot {
        CompositionSnapshot {
            fibers: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            provisions: BTreeMap::new(),
            relations: BTreeSet::new(),
            committed: BTreeMap::new(),
            quiet: true,
        }
    }

    #[test]
    fn start_failures_report_on_their_contracted_exit_codes() {
        let registration = StartFailure::Registration {
            message: "decode plugin registration failed: E".to_owned(),
        };
        assert_eq!(
            registration.report(),
            "decode plugin registration failed: E"
        );
        assert_eq!(registration.exit_code(), ExitCode::from(1));

        let refused = StartFailure::CompositionRefused {
            errors: "E1, E2".to_owned(),
        };
        assert_eq!(refused.report(), "composition refused: E1, E2");
        assert_eq!(refused.exit_code(), ExitCode::from(2));
    }

    #[test]
    fn the_activation_report_states_the_diagnostic_or_its_absence() {
        assert_eq!(
            activation_failure_report(Some("render stream open failed: no device")),
            "playback session failed to activate: render stream open failed: no device"
        );
        assert_eq!(
            activation_failure_report(None),
            "playback session did not activate (a required capability provider \
             failed or is missing on this platform)"
        );
    }

    #[test]
    fn each_terminal_fact_reports_its_contracted_line_and_stream() {
        assert_eq!(
            outcome_report(EpisodeTerminalOutcome::Completed, None),
            vec![(
                ReportStream::Stdout,
                "EOF: played out completely".to_owned()
            )]
        );
        assert_eq!(
            outcome_report(EpisodeTerminalOutcome::Stopped, None),
            vec![(ReportStream::Stdout, "stopped before completion".to_owned())]
        );
        assert_eq!(
            outcome_report(
                EpisodeTerminalOutcome::Failed,
                Some("decode: corrupt frame")
            ),
            vec![(
                ReportStream::Stderr,
                "playback failed: decode: corrupt frame".to_owned()
            )]
        );
        // A Failed fact MAY lack a diagnostic; the semantic line must
        // not depend on it.
        assert_eq!(
            outcome_report(EpisodeTerminalOutcome::Failed, None),
            vec![(ReportStream::Stderr, "playback failed".to_owned())]
        );
    }

    #[test]
    fn disposal_warnings_are_exactly_the_latched_violations() {
        assert!(disposal_warnings(&quiet_snapshot()).is_empty());

        let mut snapshot = quiet_snapshot();
        snapshot.quiet = false;
        snapshot.fibers.insert(
            "session".to_owned(),
            FiberDiagnostic {
                state: FiberState::Active,
                failed_outcome: false,
                teardown_violated: true,
            },
        );
        snapshot.fibers.insert(
            "decode".to_owned(),
            FiberDiagnostic {
                state: FiberState::Unloading,
                failed_outcome: false,
                teardown_violated: false,
            },
        );
        assert_eq!(
            disposal_warnings(&snapshot),
            vec![
                "warning: disposal reported a latched teardown violation".to_owned(),
                "  fiber 'session': teardown violated (state Active)".to_owned(),
            ]
        );
    }

    #[test]
    fn exit_codes_are_quiet_gated_exactly_for_completed_and_stopped() {
        for outcome in [
            EpisodeTerminalOutcome::Completed,
            EpisodeTerminalOutcome::Stopped,
        ] {
            assert_eq!(
                episode_exit_code(Some(outcome), true),
                ExitCode::SUCCESS,
                "{outcome:?} succeeds on quiet disposal"
            );
            assert_eq!(
                episode_exit_code(Some(outcome), false),
                ExitCode::from(1),
                "{outcome:?} fails on a latched violation"
            );
        }
        assert_eq!(
            episode_exit_code(Some(EpisodeTerminalOutcome::Failed), true),
            ExitCode::from(1)
        );
        // No terminal Fact (never activated): no successful exit.
        assert_eq!(episode_exit_code(None, true), ExitCode::from(1));
    }
}
