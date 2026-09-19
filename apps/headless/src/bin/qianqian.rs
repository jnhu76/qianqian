//! The canonical `qianqian` product binary (U1, Issue #166).
//!
//! On Windows this builds `qianqian.exe`: a normal console-subsystem
//! application. A bare launch (Explorer double-click, or a shell line
//! with no arguments) enters the interactive TUI with no music loaded;
//! `play <file-or-folder> ...` expands its input into a temporary
//! track list and opens the first candidate. All behavior lives in
//! [`qianqian_headless::entry`]; see `src/main.rs` for the historical
//! `qianqian-headless` regression target — the two differ only in the
//! name they report.

fn main() -> std::process::ExitCode {
    qianqian_headless::entry::run(env!("CARGO_BIN_NAME"))
}
