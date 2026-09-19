//! Binary-seam smoke tests for the product-facing CLI grammar.
//!
//! These run the real binary (no playback feature involved) and pin the
//! scriptable contract: which invocations are recognized and which exit
//! code / stream each grammar class produces. Playback episodes are not
//! exercised here; they need the physical slice (first-audible-slice
//! evidence path).

use std::process::Command;

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn invoke(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_qianqian-headless"))
        .args(args)
        .output()
        .expect("the headless binary is built alongside its tests");
    Run {
        code: output.status.code().expect("terminated by a signal"),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn help_flag_prints_usage_to_stdout_and_exits_zero() {
    for flag in ["--help", "-h"] {
        let run = invoke(&[flag]);
        assert_eq!(run.code, 0, "help must succeed ({flag})");
        assert!(
            run.stdout.contains("qianqian play <file-or-folder>"),
            "usage must document the play grammar ({flag}): {}",
            run.stdout
        );
        assert!(run.stderr.is_empty(), "help is not an error ({flag})");
    }
}

#[test]
fn version_flag_prints_the_binary_identity_and_exits_zero() {
    for flag in ["--version", "-V"] {
        let run = invoke(&[flag]);
        assert_eq!(run.code, 0, "version must succeed ({flag})");
        assert!(
            run.stdout.starts_with("qianqian-headless "),
            "version prints the binary identity ({flag}): {}",
            run.stdout
        );
        assert!(run.stderr.is_empty(), "version is not an error ({flag})");
    }
}

/// U1 (Issue #166): the canonical PRODUCT binary target compiles the
/// same source under the name `qianqian` and reports its own identity.
#[test]
fn the_product_binary_reports_its_own_identity() {
    let run = Command::new(env!("CARGO_BIN_EXE_qianqian"))
        .arg("--version")
        .output()
        .expect("the product binary is built alongside its tests");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert_eq!(run.status.code(), Some(0));
    assert!(
        stdout.starts_with("qianqian "),
        "the product binary names itself 'qianqian': {stdout}"
    );
    assert!(!stdout.contains("headless"), "{stdout}");
}

/// U1 (Issue #166 §4/§15): the empty argv IS the interactive product
/// launch. It routes to the reference-player transport — NOT to a usage
/// error — and the typed-token negative cases keep failing (see the cli
/// unit tests for the parse-level pins, including the fact that a stray
/// first token never widens into interactive mode). This suite builds
/// WITHOUT the playback feature, so the transport refuses at the same
/// honest feature gate as `play`; the physical interactive launch (a
/// real terminal, W1 of the Windows gate) is evidence a test harness
/// must not fabricate — launching the real idle TUI here would open a
/// real terminal session and corrupt or hang the runner.
#[cfg(not(feature = "playback"))]
#[test]
fn empty_argv_routes_to_the_interactive_transport_not_a_usage_error() {
    let run = invoke(&[]);
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("built without the playback slice"),
        "the interactive launch must reach the transport layer, not the usage error: {}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("unknown command"),
        "no arguments is a legal invocation, not a malformed one: {}",
        run.stderr
    );
}

#[test]
fn unknown_command_is_rejected_with_the_offending_token() {
    let run = invoke(&["frobnicate", "x"]);
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("unknown command 'frobnicate'"),
        "the rejection names the token: {}",
        run.stderr
    );
}

/// Negative control: the retired pre-F0 positional grammar must not be
/// silently accepted as a file argument — `song.flac` is a token where a
/// command belongs, and the parser must say so.
#[test]
fn the_retired_positional_grammar_is_rejected_as_an_unknown_command() {
    let run = invoke(&["song.flac"]);
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("unknown command 'song.flac'"),
        "old positional grammar must fail loudly, not play by accident: {}",
        run.stderr
    );
}

#[test]
fn play_with_missing_file_is_a_usage_error() {
    let missing = invoke(&["play"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing
            .stderr
            .contains("'play' received the wrong number of arguments"),
        "{}",
        missing.stderr
    );
}

/// Stage D (D14.6 navigation): `play` takes ONE OR MORE files — the
/// first opens, all seed the startup playlist (open representation).
/// The multi-file form therefore parses and RUNS; each feature
/// configuration pins its own honest observable for that run.
#[cfg(not(feature = "playback"))]
#[test]
fn play_with_extra_files_hits_the_feature_gate_without_playback() {
    let extra = invoke(&["play", "a.flac", "b.flac"]);
    assert_eq!(extra.code, 2);
    assert!(
        extra.stderr.contains("built without the playback slice"),
        "the multi-file grammar parses; the no-playback build refuses at the feature gate: {}",
        extra.stderr
    );
}

/// The playback-configuration twin: the multi-file invocation parses,
/// the first (missing) file is REFUSED by the probe before any
/// destructive step, the shell-less run reports the refusal and exits
/// 1 through the no-episode exit contract (no terminal Fact exists).
#[cfg(feature = "playback")]
#[test]
fn play_with_extra_files_parses_and_reports_the_probe_refusal() {
    let extra = invoke(&["play", "a.flac", "b.flac"]);
    assert_eq!(extra.code, 1);
    assert!(
        extra.stderr.contains("open refused:"),
        "the startup Open's refusal must be reported: {}",
        extra.stderr
    );
}

#[cfg(not(feature = "playback"))]
#[test]
fn play_without_the_playback_feature_reports_the_rebuild_hint() {
    let run = invoke(&["play", "song.flac"]);
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("built without the playback slice"),
        "the no-mechanism build refuses an episode honestly: {}",
        run.stderr
    );
}

#[test]
fn usage_documents_both_transports() {
    let run = invoke(&["--help"]);
    assert!(
        run.stdout.contains("qianqian play <file-or-folder>"),
        "the interactive grammar stays documented: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("--machine play <file>"),
        "the scriptable transport stays documented: {}",
        run.stdout
    );
    // Since F3 the shell documents pause control; since U1 the help
    // leads with the normal-user surface and keeps automation in an
    // advanced section.
    assert!(
        run.stdout.contains("pause / resume"),
        "the shell pause key stays documented: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("Automation (advanced)"),
        "the automation transport is documented under its own section: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("qianqian"),
        "the usage names the product binary: {}",
        run.stdout
    );
    // U1 negative control: unshipped U2/U3 affordances must not be
    // advertised before they exist.
    for unearned in ["shuffle", "auto-next", "exact seek", "Up/Down"] {
        assert!(
            !run.stdout.contains(unearned),
            "{unearned:?} is not shipped and must not be documented: {}",
            run.stdout
        );
    }
}

/// Negative control: `--machine` alone (or without a well-formed
/// `play <file>` tail) is a usage error, not a silent fallback.
#[test]
fn machine_flag_without_a_well_formed_play_tail_is_a_usage_error() {
    for args in [
        &["--machine"][..],
        &["--machine", "play"][..],
        &["--machine", "play", "a.flac", "b.flac"][..],
        &["--machine", "frobnicate", "x"][..],
    ] {
        let run = invoke(args);
        assert_eq!(run.code, 2, "{args:?}");
        assert!(
            run.stderr.contains("unknown command") || run.stderr.contains("wrong number"),
            "the error names what was wrong: {args:?} -> {}",
            run.stderr
        );
        assert!(
            run.stderr.contains("Usage:"),
            "usage errors carry the usage text: {args:?} -> {}",
            run.stderr
        );
    }
}

/// Machine-transport regression: the scriptable invocation reaches the
/// same playback slice gate as `play` — refused honestly without the
/// feature, never silently falling back to the terminal shell (which
/// would fail later in a piped stdin/stdout harness).
#[cfg(not(feature = "playback"))]
#[test]
fn machine_play_without_the_playback_feature_reports_the_rebuild_hint() {
    let run = invoke(&["--machine", "play", "song.flac"]);
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("built without the playback slice"),
        "the no-mechanism build refuses an episode honestly: {}",
        run.stderr
    );
}
