//! Product-facing CLI grammar.
//!
//! The parser is a pure vocabulary layer: argv (or one interactive line)
//! in, a typed intent out. It performs no I/O, attaches no playback
//! semantics, and wires nothing to a control seam. Since I4 (Issue
//! #177), the parser carries ONE piece of the playback crate's product
//! vocabulary as DATA — `EqPreset`, resolved by name so an unknown
//! preset is refused at the grammar, not at composition — but it
//! touches no composition, session, or audio mechanism; wiring a parsed
//! intent to a control seam stays downstream ([`crate::entry`]).

use qianqian_playback::EqPreset;

use std::path::PathBuf;

/// One product-facing invocation of the headless binary (argv without
/// the program path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// No arguments at all: the interactive reference-player TUI with
    /// NO active episode (U1 product launch, Issue #166). This is how
    /// the canonical `qianqian` binary starts from Explorer or a bare
    /// shell line. The shell begins truthfully idle — no media path is
    /// faked, no episode is constructed, and the Open input line (O)
    /// is how music gets loaded.
    Interactive,
    /// `play [--shuffle] [--eq <name>] <file-or-folder> [more…]`: local
    /// media files and/or folders for the interactive reference-player
    /// shell (TUI). The input expansion ([`crate::input`]) turns files
    /// and folders into ONE ordered candidate list; the FIRST accepted
    /// candidate is opened as the startup episode and the WHOLE list
    /// establishes the temporary playlist on commit (the AMENDED D14.6
    /// playlist authority).
    ///
    /// `--shuffle` selects the Shuffle traversal order from the start
    /// (Issue #166 §24). It is the SAME order policy the `R` key
    /// toggles, so the startup discipline is unchanged: the first
    /// committed candidate is still the canonical first, and the shuffle
    /// cycle is anchored on it — shuffle governs the SUBSEQUENT order,
    /// never the safe first Open.
    Play {
        files: Vec<PathBuf>,
        shuffle: bool,
        /// The startup EQ preset selection (`--eq <name>`), `None` when
        /// the flag is absent — the transparent bypass (I4). Data, not a
        /// processor: the resolved configuration rides into episode
        /// establishment like any other desired configuration.
        eq: Option<EqPreset>,
    },
    /// `--machine play <file>`: one FILE through the scriptable
    /// stdin/stdout transport. This is the automation contract; the
    /// flag is recognized in command position only, like every flag,
    /// and the machine grammar is NOT extended by U1 (no folder
    /// expansion, no interactive startup).
    MachinePlay { file: PathBuf },
    /// `--help` / `-h`: print the usage text.
    Help,
    /// `--version` / `-V`: print the binary version.
    Version,
}

/// Why an argv did not parse as a product-facing invocation.
///
/// There is no "missing command" error: an empty argv IS the
/// interactive invocation. Every remaining case is a token the user
/// actually typed, and each one must keep failing truthfully — the
/// no-argument change must never widen into "anything unparseable
/// starts the player" (Issue #166 U1 §15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationError {
    /// The first argument is neither a known subcommand nor a known flag.
    UnknownCommand(String),
    /// A recognized command received the wrong number of arguments.
    WrongArity { command: &'static str },
    /// `--eq` was given a name that is not a product preset.
    UnknownEqPreset { name: String },
}

/// Parse argv (excluding the program path) into one typed invocation.
///
/// Flags are recognized only in command position: `play --help` denotes
/// a file literally named `--help`, not a help request.
pub fn parse_invocation(args: &[String]) -> Result<Invocation, InvocationError> {
    let Some(command) = args.first() else {
        return Ok(Invocation::Interactive);
    };
    match command.as_str() {
        "play" => parse_play(&args[1..]),
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
        other if args.len() == 1 && !other.starts_with('-') => Ok(Invocation::Play {
            files: vec![PathBuf::from(other)],
            shuffle: false,
            eq: None,
        }),
        other => Err(InvocationError::UnknownCommand(other.to_string())),
    }
}

/// The `play` grammar:
/// `play [--shuffle] [--eq <name>] <file-or-folder> [more…]`.
///
/// Flag-position discipline: BOTH flags are recognized only in the
/// LEADING flag block — order-free among themselves and repeatable
/// (the last `--eq` wins; a repeated `--shuffle` is idempotent). After
/// the FIRST non-flag token, everything is an ordinary path, so a path
/// token can never be silently swallowed by a flag, and a file literally
/// named `--shuffle` is still reachable as `play ./--shuffle`.
///
/// `--eq` value grammar: the next token is ALWAYS the preset name
/// (consumed even if it looks like a path — an unknown name is refused
/// with the shipped vocabulary, never silently treated as a source);
/// a missing name is an arity error naming `--eq`.
fn parse_play(rest: &[String]) -> Result<Invocation, InvocationError> {
    // Flag-position discipline (see above): flags live only in the
    // leading flag block.
    let mut rest_iter = rest;
    let mut shuffle = false;
    let mut eq: Option<EqPreset> = None;
    loop {
        match rest_iter.first().map(String::as_str) {
            Some("--shuffle") => {
                shuffle = true;
                rest_iter = &rest_iter[1..];
            }
            Some("--eq") => {
                let Some(name) = rest_iter.get(1) else {
                    return Err(InvocationError::WrongArity { command: "--eq" });
                };
                let Some(preset) = EqPreset::from_name(name) else {
                    return Err(InvocationError::UnknownEqPreset { name: name.clone() });
                };
                eq = Some(preset);
                rest_iter = &rest_iter[2..];
            }
            _ => break,
        }
    }
    let paths = rest_iter;
    if paths.is_empty() {
        return Err(InvocationError::WrongArity { command: "play" });
    }
    Ok(Invocation::Play {
        files: paths.iter().map(PathBuf::from).collect(),
        shuffle,
        eq,
    })
}

/// The product-facing usage text, printed for `--help` (stdout, exit 0)
/// and for every usage error (stderr, exit 2).
///
/// Written for a normal user first (U1, Issue #166 §12): launch, play,
/// and the player keys lead; the automation transport stays available
/// under an advanced section. It documents ONLY shipped behavior — the
/// temporary list, `--shuffle`, row selection, the order/repeat keys and
/// the fixed/exact seek keys are all shipped (U2, Issue #166 §43) and
/// the usage text must match the shipped keymap and nothing beyond it
/// (the negative-vocabulary test below pins the boundary).
pub fn usage() -> &'static str {
    "Qianqian — a lightweight local music player

Usage:
  qianqian                                   listen (opens the player)
  qianqian <file-or-folder>                  play one file or folder
  qianqian play <file-or-folder> [more...]   play files or folders
  qianqian play --shuffle <paths...>         start in shuffle order
  qianqian play --eq <preset> <paths...>     start with an EQ preset
  qianqian --help                            show this help
  qianqian --version                         show the version

A folder is expanded recursively into a temporary track list. The first
track starts; the list plays on through the whole folder, and you can
move around it or make it repeat.

Examples:
  qianqian
  qianqian song.flac
  qianqian play song.flac
  qianqian play \"D:\\Music\"
  qianqian play --shuffle \"D:\\Music\" \"E:\\More Music\"
  qianqian play --eq rock \"D:\\Music\"

Keys in the player:
  Tab / Shift+Tab  move keyboard focus
  Mouse            click a tab, a transport button or a playlist row
  Up / Down    select a row in the list (does not change what plays)
  Enter        play the selected row, or activate the focused control
  N / P        next / previous track
  R            order: sequential / shuffle
  L            repeat: off / all / one
  Space        pause / resume
  Left / Right seek 5 seconds back / forward
  Shift+Left / Shift+Right                    seek 30 seconds
  G            go to a time you type (e.g. 1:35)
  + / -        volume up / down
  S            stop
  O            open a file or folder
  ?            keyboard help
  Esc          cancel the current input / close help
  Q or Ctrl+C  quit

--shuffle starts the LIST in shuffle order: the first track still starts
the same way, and the rest of the folder follows shuffled.

--eq picks a tonal balance for this listening session: flat, jazz, vocal,
blues, rock, classical, bass, or treble. It applies to everything this
launch plays and lasts until you quit. Bands at or above source Nyquist
are inert for that source; their desired trims are retained.

Automation (advanced):
  qianqian --machine play <file>   scriptable single-episode transport
                                   over stdin/stdout; `stop`, `pause`,
                                   `resume`, `seek <time>`, `status`
                                   control it, and the exit code and
                                   report lines are a pinned contract
"
}

impl std::fmt::Display for InvocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvocationError::UnknownCommand(token) => write!(f, "unknown command '{token}'"),
            InvocationError::WrongArity { command } => {
                write!(f, "'{command}' received the wrong number of arguments")
            }
            InvocationError::UnknownEqPreset { name } => {
                let vocabulary: Vec<&str> = EqPreset::all().iter().map(|p| p.name()).collect();
                write!(
                    f,
                    "unknown EQ preset '{name}' (presets: {})",
                    vocabulary.join(", ")
                )
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
    /// `seek <time>`: the time token is READ by the shell into a
    /// source-relative request ([`parse_seek_time`]: a decimal seconds
    /// field, or `MINUTES:SECONDS` with a decimal seconds part);
    /// whether the episode accepts it, refuses it or lands elsewhere is
    /// the frozen D14.5 protocol's business, never the shell's.
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
/// The token is read as a decimal seconds field; a first `:` splits it
/// into a minutes field and a seconds field that must be in `[0, 60)`.
/// Being a Rust float parse, every spelling that parses is accepted
/// (`90`, `1.5`, `.5`, `5e3`, `+5`) — the shell owns readability, not
/// taste. Seconds are the episode's media time; the token is a REQUEST,
/// and everything the frozen protocol says about acceptance, refusal
/// and the actual landing applies downstream. `None` = the shell cannot
/// read the token as a non-negative, representable time, so no command
/// is sent (inert input, never a fabricated target) — the shell has no
/// failure channel into the episode, so it fails closed here instead.
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
    // `try_from_secs_f64` is the failing twin of `from_secs_f64`, which
    // PANICS on NaN, infinities and values outside the Duration range —
    // reachable from plain tokens (`nan`, `inf`, `1e400`, a minutes
    // field near u64::MAX). Readability is a shell-side property: an
    // unreadable token is inert input, never an abort.
    std::time::Duration::try_from_secs_f64(seconds).ok()
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
                files: vec![PathBuf::from("song.flac")],
                shuffle: false,
                eq: None,
            }
        );
    }

    #[test]
    fn play_without_a_file_is_an_arity_error() {
        let err = parse_invocation(&argv(&["play"])).expect_err("play needs a file");
        assert_eq!(err, InvocationError::WrongArity { command: "play" });
    }

    /// Stage D (D14.6 navigation): `play` takes ONE OR MORE files — the
    /// first is the startup episode, all of them seed the startup
    /// playlist (open representation). The OLD one-file-per-episode
    /// arity rule is deliberately retired for `play` (it stays frozen
    /// for `--machine play`).
    #[test]
    fn play_takes_one_or_more_files_for_the_startup_playlist() {
        assert_eq!(
            parse_invocation(&argv(&["play", "a.flac", "b.flac", "c.flac"]))
                .expect("multi-file play is the Stage D grammar"),
            Invocation::Play {
                files: vec![
                    PathBuf::from("a.flac"),
                    PathBuf::from("b.flac"),
                    PathBuf::from("c.flac")
                ],
                shuffle: false,
                eq: None,
            }
        );
    }

    /// `play --shuffle <paths…>` is the ONE shuffle grammar (Issue #166
    /// §24): the flag is recognized immediately after the subcommand, it
    /// is part of `play`, and it never appears anywhere else in argv.
    #[test]
    fn play_accepts_shuffle_immediately_after_the_subcommand() {
        assert_eq!(
            parse_invocation(&argv(&["play", "--shuffle", "D:\\Music"]))
                .expect("the shuffle grammar"),
            Invocation::Play {
                files: vec![PathBuf::from("D:\\Music")],
                shuffle: true,
                eq: None,
            }
        );
        assert_eq!(
            parse_invocation(&argv(&["play", "--shuffle", "a.flac", "b.flac", "c.flac"]))
                .expect("shuffle with several roots"),
            Invocation::Play {
                files: vec![
                    PathBuf::from("a.flac"),
                    PathBuf::from("b.flac"),
                    PathBuf::from("c.flac")
                ],
                shuffle: true,
                eq: None,
            }
        );
        // Without the flag the default is sequential.
        assert_eq!(
            parse_invocation(&argv(&["play", "a.flac"]))
                .expect("the plain grammar")
                .clone(),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac")],
                shuffle: false,
                eq: None,
            }
        );
    }

    /// The shuffle flag keeps `play`'s arity contract: it selects an
    /// order, it is not a source, so `play --shuffle` alone still fails
    /// truthfully, and the flag is recognized ONLY in the position the
    /// grammar names — a LATER `--shuffle` is an ordinary path token.
    #[test]
    fn the_shuffle_flag_never_widens_the_play_grammar() {
        assert_eq!(
            parse_invocation(&argv(&["play", "--shuffle"])).expect_err("no source given"),
            InvocationError::WrongArity { command: "play" }
        );
        assert_eq!(
            parse_invocation(&argv(&["play", "a.flac", "--shuffle"]))
                .expect("a later --shuffle is a path, not a flag"),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac"), PathBuf::from("--shuffle")],
                shuffle: false,
                eq: None,
            }
        );
        // The machine transport is NOT extended by it (Issue #166 §44).
        assert_eq!(
            parse_invocation(&argv(&["--machine", "play", "--shuffle", "a.flac"]))
                .expect_err("--machine play takes exactly one file"),
            InvocationError::WrongArity {
                command: "--machine"
            }
        );
        assert_eq!(
            parse_invocation(&argv(&["--shuffle", "play", "a.flac"]))
                .expect_err("--shuffle is not a command-position flag"),
            InvocationError::UnknownCommand("--shuffle".to_string())
        );
    }

    /// The usage text documents the shipped surface and ONLY it (Issue
    /// #166 §43): the folder list, `--shuffle`, the playlist/repeat/seek
    /// keys — and nothing about persistence, libraries or devices.
    #[test]
    fn the_usage_text_documents_the_shipped_surface_only() {
        let usage = usage();
        for earned in [
            "qianqian play --shuffle",
            "qianqian play --eq <preset>",
            // The low-rate refusal disclosure (I4 review): the usage
            // text must tell the user the EQ's sample-rate condition.
            "their desired trims are retained",
            "Up / Down",
            "Enter        play the selected row",
            "R            order: sequential / shuffle",
            "L            repeat: off / all / one",
            "seek 30 seconds",
            "G            go to a time you type",
            "temporary track list",
        ] {
            assert!(usage.contains(earned), "{earned:?} missing in:\n{usage}");
        }
        for unearned in [
            "library",
            "favorites",
            "playlist file",
            "M3U",
            "history",
            "database",
        ] {
            assert!(!usage.contains(unearned), "{unearned:?} in:\n{usage}");
        }
    }

    /// `play --eq <preset> <paths…>` is the startup EQ grammar (D14.11
    /// option B): the App owns the desired configuration, the flag is
    /// recognized only in command position (next to `--shuffle`), and a
    /// selection applies to everything this launch plays.
    #[test]
    fn play_accepts_an_eq_preset_in_command_position() {
        assert_eq!(
            parse_invocation(&argv(&["play", "--eq", "rock", "D:\\Music"]))
                .expect("the --eq grammar"),
            Invocation::Play {
                files: vec![PathBuf::from("D:\\Music")],
                shuffle: false,
                eq: Some(EqPreset::Rock),
            }
        );
        // Flags compose, order-free among themselves.
        assert_eq!(
            parse_invocation(&argv(&["play", "--eq", "bass", "--shuffle", "a.flac"]))
                .expect("both flags"),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac")],
                shuffle: true,
                eq: Some(EqPreset::Bass),
            }
        );
        assert_eq!(
            parse_invocation(&argv(&["play", "--shuffle", "--eq", "flat", "a.flac"]))
                .expect("both flags, other order"),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac")],
                shuffle: true,
                eq: Some(EqPreset::Flat),
            }
        );
        // Every shipped preset name parses.
        for preset in EqPreset::all() {
            let name = preset.name();
            let parsed = parse_invocation(&argv(&["play", "--eq", name, "a.flac"]))
                .unwrap_or_else(|e| panic!("{name} must parse: {e}"));
            assert_eq!(
                parsed,
                Invocation::Play {
                    files: vec![PathBuf::from("a.flac")],
                    shuffle: false,
                    eq: Some(preset),
                },
                "preset {name} parses to its own variant"
            );
        }
    }

    /// The EQ flag keeps `play`'s flag-position discipline: a later
    /// `--eq` is an ordinary path token, `--eq` without a name is an
    /// arity error, and an unknown name is refused with the shipped
    /// vocabulary — never silently accepted as a path.
    #[test]
    fn the_eq_flag_never_widens_the_play_grammar() {
        // A later --eq (out of command position) is an ordinary path.
        assert_eq!(
            parse_invocation(&argv(&["play", "a.flac", "--eq"]))
                .expect("a later --eq is a path, not a flag"),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac"), PathBuf::from("--eq")],
                shuffle: false,
                eq: None,
            }
        );
        // Missing name: arity error naming --eq.
        assert_eq!(
            parse_invocation(&argv(&["play", "--eq"])).expect_err("--eq needs a name"),
            InvocationError::WrongArity { command: "--eq" }
        );
        assert_eq!(
            parse_invocation(&argv(&["play", "--eq", "a.flac"]))
                .expect_err("--eq consumes the next token as the preset name"),
            InvocationError::UnknownEqPreset {
                name: "a.flac".to_string()
            }
        );
        // Unknown name: refused with the shipped vocabulary.
        let err = parse_invocation(&argv(&["play", "--eq", "loudness", "a.flac"]))
            .expect_err("loudness is not a shipped preset");
        assert_eq!(
            err,
            InvocationError::UnknownEqPreset {
                name: "loudness".to_string()
            }
        );
        let message = err.to_string();
        for preset in EqPreset::all() {
            assert!(
                message.contains(preset.name()),
                "refusal must name the shipped preset {:?}: {message}",
                preset.name()
            );
        }
        // `play --eq` alone is still an arity error (no source).
        assert_eq!(
            parse_invocation(&argv(&["play", "--eq", "rock"])).expect_err("no source given"),
            InvocationError::WrongArity { command: "play" }
        );
    }

    /// The leading flag block is repeatable and last-wins (pinned,
    /// deliberately: `--eq a --eq b` resolves to b rather than smuggling
    /// a second source; a repeated `--shuffle` is idempotent). The
    /// escape hatch is unchanged: after the first path token, a flag
    /// spelling is an ordinary path.
    #[test]
    fn repeated_flags_in_the_leading_block_are_last_wins() {
        assert_eq!(
            parse_invocation(&argv(&["play", "--eq", "jazz", "--eq", "rock", "a.flac"]))
                .expect("last --eq wins"),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac")],
                shuffle: false,
                eq: Some(EqPreset::Rock),
            }
        );
        assert_eq!(
            parse_invocation(&argv(&["play", "--shuffle", "--shuffle", "a.flac"]))
                .expect("repeated --shuffle is idempotent"),
            Invocation::Play {
                files: vec![PathBuf::from("a.flac")],
                shuffle: true,
                eq: None,
            }
        );
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

    /// U1 (Issue #166 §4/§15): an empty argv IS the interactive product
    /// launch — the parser maps it to the explicit `Interactive`
    /// invocation and to nothing else. This is the parse-level pin of
    /// the no-argument TUI startup; the binary itself cannot be
    /// smoke-launched argument-less from a test harness (it would open
    /// a real terminal session), so the physical launch evidence lives
    /// in the Windows ConPTY/physical gate.
    #[test]
    fn one_positional_source_is_the_existing_play_intent() {
        for source in [
            "song.flac",
            "~/Music/Album/",
            "./play",
            "Q:\\Music\\千千.flac",
        ] {
            assert_eq!(
                parse_invocation(&argv(&[source])),
                Ok(Invocation::Play {
                    files: vec![PathBuf::from(source)],
                    shuffle: false,
                    eq: None
                })
            );
        }
        assert!(parse_invocation(&argv(&["song.flac", "--shuffle"])).is_err());
        assert!(parse_invocation(&argv(&["--shuffle"])).is_err());
    }

    #[test]
    fn empty_argv_is_the_interactive_invocation() {
        assert_eq!(
            parse_invocation(&argv(&[])).expect("no arguments is a legal invocation"),
            Invocation::Interactive
        );
    }

    /// The negative side of the no-argument change: the interactive
    /// widening covers EXACTLY the empty argv, never a token the user
    /// typed. A stray first token still fails as an unknown command.
    #[test]
    fn the_interactive_invocation_never_swallows_a_typed_token() {
        for tokens in [
            &["--unknown"][..],
            &["song.flac", "second.flac"][..],
            &["--machine"][..],
            &["--machine", "play"][..],
            &["--machine", "play", "a.flac", "b.flac"][..],
            &["play"][..],
        ] {
            assert!(
                parse_invocation(&argv(tokens)).is_err(),
                "{tokens:?} must keep failing, not start the player"
            );
        }
    }

    #[test]
    fn an_unrecognized_first_token_reports_the_offending_token() {
        // The pre-F0 positional grammar (`qianqian-headless <file>`) now
        // lands here: the file token is not a known command.
        for tokens in [&["frobnicate", "x"][..], &["--unknown"][..]] {
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
                files: vec![PathBuf::from("song.flac")],
                shuffle: false,
                eq: None,
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
        // The reader is a plain Rust float parse, so spellings beyond the
        // obvious decimal ones read too. Pinned so the doc contract
        // ("readability is the shell's, not taste") stays true.
        assert_eq!(parse_seek_time("5e3"), Some(Duration::from_secs(5000)));
        assert_eq!(parse_seek_time(".5"), Some(Duration::from_millis(500)));
        assert_eq!(parse_seek_time("+5"), Some(Duration::from_secs(5)));
        // Unreadable tokens include the ones a float parser ACCEPTS but
        // no Duration can represent: NaN, infinities (including the
        // `1e400` overflow spelling) and a seconds value beyond the
        // Duration range. They are inert input exactly like `abc` —
        // never a panic in the command reader, and never a fabricated
        // target.
        for bad in [
            "",
            "-30",
            "1:99",
            "abc",
            "1:2:3",
            ":",
            "-0:01",
            "nan",
            "NaN",
            "inf",
            "-inf",
            "1e400",
            "1e20",
            "18446744073709551615:00",
        ] {
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
