//! Product-facing CLI grammar.
//!
//! The parser is a pure vocabulary layer: argv (or one interactive line)
//! in, a typed intent out. It performs no I/O, touches no composition or
//! audio types, and attaches no playback semantics — wiring a parsed
//! intent to a control seam is later Phase-F work.

use std::path::PathBuf;

/// One product-facing invocation of the headless binary (argv without
/// the program path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// `play <file>`: one playback episode over one local media file,
    /// presented in the interactive reference-player shell (TUI).
    Play { file: PathBuf },
    /// `--machine play <file>`: the same episode through the scriptable
    /// stdin/stdout transport. This is the automation contract; the
    /// flag is recognized in command position only, like every flag.
    MachinePlay { file: PathBuf },
    /// `--help` / `-h`: print the usage text.
    Help,
    /// `--version` / `-V`: print the binary version.
    Version,
}

/// Why an argv did not parse as a product-facing invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationError {
    /// No command was given at all.
    MissingCommand,
    /// The first argument is neither a known subcommand nor a known flag.
    UnknownCommand(String),
    /// A recognized command received the wrong number of arguments.
    WrongArity { command: &'static str },
}

/// Parse argv (excluding the program path) into one typed invocation.
///
/// Flags are recognized only in command position: `play --help` denotes
/// a file literally named `--help`, not a help request.
pub fn parse_invocation(args: &[String]) -> Result<Invocation, InvocationError> {
    let Some(command) = args.first() else {
        return Err(InvocationError::MissingCommand);
    };
    match command.as_str() {
        "play" => match args.get(1) {
            Some(file) if args.len() == 2 => Ok(Invocation::Play {
                file: PathBuf::from(file),
            }),
            _ => Err(InvocationError::WrongArity { command: "play" }),
        },
        "--machine" => match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("play"), Some(file)) if args.len() == 3 => Ok(Invocation::MachinePlay {
                file: PathBuf::from(file),
            }),
            (Some("play"), _) => Err(InvocationError::WrongArity {
                command: "--machine",
            }),
            (Some(other), _) => Err(InvocationError::UnknownCommand(other.to_string())),
            (None, _) => Err(InvocationError::WrongArity {
                command: "--machine",
            }),
        },
        "--help" | "-h" => match args.len() {
            1 => Ok(Invocation::Help),
            _ => Err(InvocationError::WrongArity { command: "--help" }),
        },
        "--version" | "-V" => match args.len() {
            1 => Ok(Invocation::Version),
            _ => Err(InvocationError::WrongArity {
                command: "--version",
            }),
        },
        other => Err(InvocationError::UnknownCommand(other.to_string())),
    }
}

/// The product-facing usage text, printed for `--help` (stdout, exit 0)
/// and for every usage error (stderr, exit 2).
pub fn usage() -> &'static str {
    "usage: qianqian-headless <command> [args]

commands:
  play <file>            play one local media file in the interactive
                         reference-player terminal shell
  --machine play <file>  the same episode through the scriptable
                         stdin/stdout transport (automation)
  --help | -h            print this usage
  --version | -V         print the version

in the machine transport, `stop` stops the episode, `pause` and
`resume` pause and resume it, and `status` prints its truthful state
(other interactive commands are recognized but not wired yet); in the
terminal shell, Space pauses/resumes, S stops and Q or Ctrl+C quits
"
}

impl std::fmt::Display for InvocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvocationError::MissingCommand => write!(f, "missing command"),
            InvocationError::UnknownCommand(token) => write!(f, "unknown command '{token}'"),
            InvocationError::WrongArity { command } => {
                write!(f, "'{command}' received the wrong number of arguments")
            }
        }
    }
}

/// One interactive shell command, parsed from a single line.
///
/// Phase F0 freezes spelling and arity only. The vocabulary matches the
/// Issue #119 target shape; each command's playback semantics are earned
/// by a later Phase-F slice before the binary wires a control loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractiveCommand {
    Status,
    Stop,
    Pause,
    Resume,
    /// `seek <time>`: the time token stays opaque until seek semantics
    /// (and the time grammar they imply) are frozen in a later phase.
    Seek {
        time: String,
    },
    /// `open <file>`: one local media file path.
    Open {
        file: PathBuf,
    },
    Next,
    Previous,
    /// `volume <value>`: the value token stays opaque until the volume
    /// authority (OS session volume vs PCM gain vs DSP) is decided.
    Volume {
        value: String,
    },
    Devices,
    Device {
        id: String,
    },
    Quit,
}

/// Why one interactive line did not parse as a shell command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractiveParseError {
    /// The line was empty or only whitespace.
    EmptyLine,
    /// The first token is not part of the shell vocabulary.
    UnknownCommand(String),
    /// A recognized command received the wrong number of arguments.
    WrongArity {
        command: &'static str,
        expected: usize,
        got: usize,
    },
}

impl std::fmt::Display for InteractiveParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InteractiveParseError::EmptyLine => write!(f, "empty line"),
            InteractiveParseError::UnknownCommand(token) => {
                write!(f, "unknown command '{token}'")
            }
            InteractiveParseError::WrongArity {
                command,
                expected,
                got,
            } => write!(f, "'{command}' expects {expected} argument(s), got {got}"),
        }
    }
}

/// Parse one interactive shell line into a typed command.
///
/// Tokens are whitespace-separated with no quoting grammar; a path that
/// contains spaces cannot be expressed in F0 and waits for a real
/// requirement to earn one.
pub fn parse_interactive_line(line: &str) -> Result<InteractiveCommand, InteractiveParseError> {
    let mut tokens = line.split_whitespace();
    let Some(command) = tokens.next() else {
        return Err(InteractiveParseError::EmptyLine);
    };
    let args: Vec<&str> = tokens.collect();
    match command {
        "status" => zero_arg("status", &args, InteractiveCommand::Status),
        "stop" => zero_arg("stop", &args, InteractiveCommand::Stop),
        "pause" => zero_arg("pause", &args, InteractiveCommand::Pause),
        "resume" => zero_arg("resume", &args, InteractiveCommand::Resume),
        "next" => zero_arg("next", &args, InteractiveCommand::Next),
        "previous" => zero_arg("previous", &args, InteractiveCommand::Previous),
        "devices" => zero_arg("devices", &args, InteractiveCommand::Devices),
        "quit" => zero_arg("quit", &args, InteractiveCommand::Quit),
        "seek" => one_arg("seek", &args, |token| InteractiveCommand::Seek {
            time: token.to_string(),
        }),
        "open" => one_arg("open", &args, |token| InteractiveCommand::Open {
            file: PathBuf::from(token),
        }),
        "volume" => one_arg("volume", &args, |token| InteractiveCommand::Volume {
            value: token.to_string(),
        }),
        "device" => one_arg("device", &args, |token| InteractiveCommand::Device {
            id: token.to_string(),
        }),
        other => Err(InteractiveParseError::UnknownCommand(other.to_string())),
    }
}

fn zero_arg(
    command: &'static str,
    args: &[&str],
    parsed: InteractiveCommand,
) -> Result<InteractiveCommand, InteractiveParseError> {
    match args.len() {
        0 => Ok(parsed),
        got => Err(InteractiveParseError::WrongArity {
            command,
            expected: 0,
            got,
        }),
    }
}

fn one_arg(
    command: &'static str,
    args: &[&str],
    make: impl FnOnce(&str) -> InteractiveCommand,
) -> Result<InteractiveCommand, InteractiveParseError> {
    match args.len() {
        1 => Ok(make(args[0])),
        got => Err(InteractiveParseError::WrongArity {
            command,
            expected: 1,
            got,
        }),
    }
}

/// Read a `seek <time>` token as source-relative media time (D14.5).
/// Accepted grammar: `SS`, `SS.s`, `MM:SS` and `MM:SS.s` — seconds are
/// the episode's media time; the token is a REQUEST, and everything the
/// frozen protocol says about acceptance, refusal and the actual
/// landing applies downstream. `None` = the shell cannot read the token
/// as a non-negative time, so no command is sent (inert input, never a
/// fabricated target).
pub fn parse_seek_time(token: &str) -> Option<std::time::Duration> {
    let seconds = match token.split_once(':') {
        Some((m, s)) => {
            let minutes: u64 = m.parse().ok()?;
            let seconds: f64 = s.parse().ok()?;
            if !(0.0..60.0).contains(&seconds) {
                return None;
            }
            minutes as f64 * 60.0 + seconds
        }
        None => {
            let seconds: f64 = token.parse().ok()?;
            if seconds < 0.0 {
                return None;
            }
            seconds
        }
    };
    Some(std::time::Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn play_with_one_file_parses_as_a_playback_invocation() {
        let parsed = parse_invocation(&argv(&["play", "song.flac"]))
            .expect("play <file> is the one-file grammar");
        assert_eq!(
            parsed,
            Invocation::Play {
                file: PathBuf::from("song.flac")
            }
        );
    }

    #[test]
    fn play_without_a_file_is_an_arity_error() {
        let err = parse_invocation(&argv(&["play"])).expect_err("play needs a file");
        assert_eq!(err, InvocationError::WrongArity { command: "play" });
    }

    #[test]
    fn play_with_extra_arguments_is_an_arity_error() {
        let err = parse_invocation(&argv(&["play", "a.flac", "b.flac"]))
            .expect_err("one file per episode");
        assert_eq!(err, InvocationError::WrongArity { command: "play" });
    }

    #[test]
    fn help_and_version_flags_parse_alone() {
        for tokens in [["--help"], ["-h"]] {
            assert_eq!(
                parse_invocation(&argv(&tokens)).expect("help flag"),
                Invocation::Help
            );
        }
        for tokens in [["--version"], ["-V"]] {
            assert_eq!(
                parse_invocation(&argv(&tokens)).expect("version flag"),
                Invocation::Version
            );
        }
    }

    #[test]
    fn help_or_version_with_trailing_arguments_is_an_arity_error() {
        // Short flags canonicalize to their long form in the error.
        let cases = [
            (&["--help", "play"][..], "--help"),
            (&["-h", "x"][..], "--help"),
            (&["--version", "x"][..], "--version"),
            (&["-V", "x"][..], "--version"),
        ];
        for (tokens, command) in cases {
            let err = parse_invocation(&argv(tokens)).expect_err("flags take no arguments");
            assert_eq!(err, InvocationError::WrongArity { command });
        }
    }

    #[test]
    fn empty_argv_reports_a_missing_command() {
        assert_eq!(
            parse_invocation(&argv(&[])).expect_err("no command at all"),
            InvocationError::MissingCommand
        );
    }

    #[test]
    fn an_unrecognized_first_token_reports_the_offending_token() {
        // The pre-F0 positional grammar (`qianqian-headless <file>`) now
        // lands here: the file token is not a known command.
        for tokens in [&["frobnicate", "x"][..], &["song.flac"][..]] {
            let err = parse_invocation(&argv(tokens)).expect_err("unknown command");
            assert_eq!(err, InvocationError::UnknownCommand(tokens[0].to_string()));
        }
    }

    #[test]
    fn machine_flag_selects_the_scriptable_transport() {
        let parsed = parse_invocation(&argv(&["--machine", "play", "song.flac"]))
            .expect("the machine transport keeps the play grammar");
        assert_eq!(
            parsed,
            Invocation::MachinePlay {
                file: PathBuf::from("song.flac")
            }
        );
    }

    #[test]
    fn machine_flag_rejects_wrong_arity_and_unknown_subcommands() {
        let err = parse_invocation(&argv(&["--machine"])).expect_err("--machine alone");
        assert_eq!(
            err,
            InvocationError::WrongArity {
                command: "--machine"
            }
        );
        let err = parse_invocation(&argv(&["--machine", "play"])).expect_err("play needs a file");
        assert_eq!(
            err,
            InvocationError::WrongArity {
                command: "--machine"
            }
        );
        let err = parse_invocation(&argv(&["--machine", "play", "a.flac", "b.flac"]))
            .expect_err("one file per episode");
        assert_eq!(
            err,
            InvocationError::WrongArity {
                command: "--machine"
            }
        );
        let err = parse_invocation(&argv(&["--machine", "frobnicate", "x"]))
            .expect_err("only play follows --machine");
        assert_eq!(
            err,
            InvocationError::UnknownCommand("frobnicate".to_string())
        );
    }

    #[test]
    fn plain_play_is_not_the_machine_transport() {
        let parsed =
            parse_invocation(&argv(&["play", "song.flac"])).expect("interactive play grammar");
        assert_eq!(
            parsed,
            Invocation::Play {
                file: PathBuf::from("song.flac")
            }
        );
    }

    #[test]
    fn interactive_zero_argument_commands_parse_from_one_line() {
        let cases = [
            ("status", InteractiveCommand::Status),
            ("stop", InteractiveCommand::Stop),
            ("pause", InteractiveCommand::Pause),
            ("resume", InteractiveCommand::Resume),
            ("next", InteractiveCommand::Next),
            ("previous", InteractiveCommand::Previous),
            ("devices", InteractiveCommand::Devices),
            ("quit", InteractiveCommand::Quit),
        ];
        for (line, expected) in cases {
            assert_eq!(
                parse_interactive_line(line).expect("known zero-arg command"),
                expected
            );
        }
    }

    #[test]
    fn interactive_one_argument_commands_keep_their_token_opaque() {
        let cases = [
            (
                "seek 90",
                InteractiveCommand::Seek {
                    time: "90".to_string(),
                },
            ),
            (
                "open /music/a.flac",
                InteractiveCommand::Open {
                    file: PathBuf::from("/music/a.flac"),
                },
            ),
            (
                "volume 0.5",
                InteractiveCommand::Volume {
                    value: "0.5".to_string(),
                },
            ),
            (
                "device {0.0.0.00000000}.{abcd1234}",
                InteractiveCommand::Device {
                    id: "{0.0.0.00000000}.{abcd1234}".to_string(),
                },
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(
                parse_interactive_line(line).expect("known one-arg command"),
                expected
            );
        }
    }

    /// The seek time reader (D14.5): plain seconds, fractional seconds
    /// and mm:ss are media time; unreadable or negative tokens are
    /// `None` — the shell sends NO command rather than fabricating a
    /// target. Time zero is a legal request (the provider owns
    /// validity); the grammar only decides readability.
    #[test]
    fn seek_time_tokens_read_as_media_time_or_none() {
        use std::time::Duration;
        assert_eq!(parse_seek_time("90"), Some(Duration::from_secs(90)));
        assert_eq!(parse_seek_time("1.5"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_seek_time("2:05"), Some(Duration::from_secs(125)));
        assert_eq!(
            parse_seek_time("0:00"),
            Some(Duration::ZERO),
            "time zero is a legal request, not an unreadable token"
        );
        assert_eq!(parse_seek_time("75:00"), Some(Duration::from_secs(4500)));
        for bad in ["", "-30", "1:99", "abc", "1:2:3", ":", "-0:01"] {
            assert_eq!(parse_seek_time(bad), None, "{bad:?} is unreadable");
        }
    }

    #[test]
    fn interactive_blank_lines_are_empty_not_unknown() {
        for line in ["", "   ", "\t"] {
            assert_eq!(
                parse_interactive_line(line).expect_err("nothing to parse"),
                InteractiveParseError::EmptyLine
            );
        }
    }

    #[test]
    fn interactive_unknown_tokens_report_the_offending_token() {
        assert_eq!(
            parse_interactive_line("frobnicate now").expect_err("not vocabulary"),
            InteractiveParseError::UnknownCommand("frobnicate".to_string())
        );
    }

    #[test]
    fn interactive_wrong_arity_reports_command_expected_and_got() {
        let cases = [
            (
                "stop now",
                InteractiveParseError::WrongArity {
                    command: "stop",
                    expected: 0,
                    got: 1,
                },
            ),
            (
                "seek",
                InteractiveParseError::WrongArity {
                    command: "seek",
                    expected: 1,
                    got: 0,
                },
            ),
            (
                "seek 1 2",
                InteractiveParseError::WrongArity {
                    command: "seek",
                    expected: 1,
                    got: 2,
                },
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(
                parse_interactive_line(line).expect_err("arity violation"),
                expected
            );
        }
    }
}
