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
            run.stdout.contains("play <file>"),
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

#[test]
fn empty_argv_prints_usage_to_stderr_and_exits_two() {
    let run = invoke(&[]);
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("usage: qianqian-headless"),
        "usage errors carry the usage text: {}",
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
fn play_with_missing_or_extra_file_is_a_usage_error() {
    let missing = invoke(&["play"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing
            .stderr
            .contains("'play' received the wrong number of arguments"),
        "{}",
        missing.stderr
    );

    let extra = invoke(&["play", "a.flac", "b.flac"]);
    assert_eq!(extra.code, 2);
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
