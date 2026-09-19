//! The `qianqian-headless` binary: the historical regression target.
//!
//! The physical-gate scripts and dogfood harnesses of the closed
//! transport campaigns name this executable, so it stays buildable
//! alongside the product binary. All behavior lives in
//! [`qianqian_headless::entry`]; see `src/bin/qianqian.rs` for the
//! canonical product binary (U1, Issue #166) — the two differ only in
//! the name they report.

fn main() -> std::process::ExitCode {
    qianqian_headless::entry::run(env!("CARGO_BIN_NAME"))
}
