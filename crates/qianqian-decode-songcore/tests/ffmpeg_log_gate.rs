// POSIX-only witness. The observable under test (fd 2) is read by redirecting it
// with `dup`/`dup2` and `AsRawFd`; those APIs do not exist on Windows. The
// production C gate and the Rust predicate are platform-neutral, and the
// Windows test target must still COMPILE, so the whole witness is compiled out
// there via this file-level cfg. On Windows this binary therefore contains zero
// tests: absence of the witness is not a pass. Windows runtime evidence for the
// gate remains a separate #172 obligation and stays NOT_RUN here.
#![cfg(unix)]

//! closeout2 反事实测试 — `QIANQIAN_FFMPEG_LOG=0` 下两层 av_log gate 的一致性。
//!
//! 冻结契约：`out/closeout2/ffmpeg-contract.md` §3 — 取值 `"0"` 与 unset 同类：
//! 桥不安装，**SongCore 的 discard callback 维持为 process posture**。
//!
//! 证据分界（对齐 `src/av_log_bridge.rs:376-380` 的自述）：
//! - Rust 层谓词（`bridge_requested`，`src/av_log_bridge.rs:120-126`）已由单元
//!   测试 `dg4c_without_the_opt_in_the_bridge_is_not_installed` 钉定（`"0"` →
//!   桥不安装）。本文件不复制该谓词，只通过真实激活路径（lib.rs:644 →
//!   `install_av_log_bridge`）运行它，并以运行时行为锚定其结果。
//! - 本文件钉定 **C 层 per-song_open gate**（`native/src/songcore_ffmpeg.c`
//!   的 `songcore_quiet_ffmpeg_logging`，由真实 `song_open` 触发）在同一取值
//!   下必须把 discard 安装为 process posture。
//!
//! 旧谓词（仅 `getenv("QIANQIAN_FFMPEG_LOG") == NULL` 才安装 discard）下，
//! 取值 `"0"` 时两层叠加：Rust 不装桥 + C 不装 discard ⇒ 无人安装 discard，
//! FFmpeg 默认同步 stderr logger 存活。C 层断言在该状态下精确 RED。
//!
//! 观察机制（全运行时行为，不引用任何源码常量或谓词副本）：FFmpeg 9.0 的
//! callback 槽是 `log.c` 内部的 `static atomic_uintptr_t`，初始值为
//! `av_log_default_callback`，无导出的读取器 —— 所以每个场景用**一次真实
//! `av_log` 探针**读出当前 callback 是谁：默认 logger 把消息写到 fd 2（探针
//! 窗口捕获非空）；discard 吞掉（空）；桥向诊断 outlet 发布 class-α 记录
//! （`wait_for_line` 可见）。三个判据互斥地覆盖本进程可能安装的全部 callback。
//!
//! 本测试单函数内固定顺序推进：env 与 av_log callback 都是进程全局，无法在
//! 并行测试间共享。

// The shared helpers serve every witness in this crate; from this integration
// binary's narrower view some of their fields are simply not read yet.
#[allow(dead_code)]
mod common;
#[path = "common/outlet.rs"]
#[allow(dead_code)]
mod test_outlet;

use std::ffi::{c_char, c_void};
use std::os::fd::AsRawFd;

use qianqian_composition::{CompositionKernel, DesiredEntry, FiberState, Revision};
use qianqian_decode_songcore::probe_media;
use qianqian_diagnostics::AlphaRegistration;

// The pinned FFmpeg's `av_log` (the process-global logger this whole contract
// is about) plus the libc fd helpers for the probe window. Declarations only;
// this file adds no ABI and no export — the symbols resolve from the merged
// FFmpeg closure inside libsongcore.a, the same code production runs.
unsafe extern "C" {
    fn av_log(avcl: *mut c_void, level: i32, fmt: *const c_char, ...);
    fn dup(fd: i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn fflush(stream: *mut c_void);
}

/// FFmpeg's `AV_LOG_ERROR` (level table pinned at `src/av_log_bridge.rs:57-69`).
/// Below the default logger's threshold (INFO, 32), so FFmpeg's default logger
/// prints it, while the discard callback swallows it and the bridge publishes it.
const AV_LOG_ERROR: i32 = 16;

/// NUL-terminated on purpose: the probe format string is handed to C, and a
/// non-terminated `&str` pointer would let `fprintf` walk past it into the
/// binary's rodata.
const PROBE_MARKER: &std::ffi::CStr = c"QIANQIAN_GATE_PROBE_MARKER";

/// The real host-startup moment: mount the decode plugin through the real
/// composition kernel, so `on_activate` (`src/lib.rs:644` →
/// `install_av_log_bridge`) runs the real Rust-layer decision for the current
/// environment value.
fn activate_decode_plugin() {
    let mut k = CompositionKernel::new();
    k.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .expect("decode plugin registers");
    k.set_desired(vec![DesiredEntry::enabled(
        "songcore_decode_plugin",
        "songcore_decode_plugin",
        Revision::fresh(),
    )])
    .expect("decode plugin desired");
    k.settle();
    let snap = k.snapshot();
    let fiber = snap
        .fibers
        .get("songcore_decode_plugin")
        .expect("decode fiber present");
    assert_eq!(
        fiber.state,
        FiberState::Active,
        "the decode plugin must activate for the layer witness to mean anything"
    );
}

/// The real production open path (`probe_media` → `song_open` + `song_probe`),
/// whose C entry runs the per-open gate (`native/src/songcore_ffmpeg.c:822`).
fn probe_fixture() {
    let (_fx, path) = common::load_fixture("flac-16-44-stereo");
    probe_media(&path)
        .expect("the committed reference fixture opens through the production song_open path");
}

/// Drive one REAL `av_log` call with fd 2 redirected into a capture file and
/// return the bytes the process logger wrote during that window. Under FFmpeg's
/// default logger the marker reaches stderr; under the discard callback nothing
/// is written; under the bridge the record goes to the diagnostic outlet
/// instead of fd 2 (and an unregistered thread's record is dropped+counted).
fn probe_stderr(level: i32) -> Vec<u8> {
    let path = std::env::temp_dir().join(format!(
        "qianqian-ffmpeg-gate-probe-{}.log",
        std::process::id()
    ));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .expect("probe capture file");
    let saved = unsafe { dup(2) };
    assert!(saved >= 0, "dup(2) failed for the probe window");
    unsafe { dup2(file.as_raw_fd(), 2) };
    unsafe { av_log(std::ptr::null_mut(), level, PROBE_MARKER.as_ptr()) };
    unsafe { fflush(std::ptr::null_mut()) };
    unsafe { dup2(saved, 2) };
    unsafe { close(saved) };
    let captured = std::fs::read(&path).expect("probe capture readable");
    let _ = std::fs::remove_file(&path);
    captured
}

#[test]
fn ffmpeg_log_gate_layers_agree_on_explicit_zero() {
    // ---- Scenario A: the counterfactual core, `QIANQIAN_FFMPEG_LOG=0`. ----
    // SAFETY: single-threaded witness; the variable is consumed below only by
    // this test's own sequential activation/open calls (edition 2024 unsafe API).
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "0") };

    // Rust layer, the real activation path: "0" → bridge_requested == false →
    // the bridge is NOT installed (av_log_bridge.rs:120-126). Runtime anchor:
    // the probe must still reach stderr through FFmpeg's default logger — if
    // something swallowed it, a callback other than the default was installed
    // and this file's frozen premise (contract §3) is stale.
    activate_decode_plugin();
    let rust_layer_probe = probe_stderr(AV_LOG_ERROR);
    assert!(
        !rust_layer_probe.is_empty(),
        "Rust-layer premise: with \"0\" the bridge is NOT installed (av_log_bridge.rs:120-126), \
         so right after activation the process logger is still FFmpeg's default and the probe \
         reaches stderr. Captured instead: {rust_layer_probe:?}. If this fails, the Rust \
         predicate moved and this witness must be re-derived against the new semantics."
    );

    // C layer, the real per-open gate (songcore_ffmpeg.c:822): with "0" the
    // discard callback must become the process posture (contract §3, "0" row).
    probe_fixture();
    let c_layer_probe = probe_stderr(AV_LOG_ERROR);
    assert!(
        c_layer_probe.is_empty(),
        "C layer: after a real song_open with QIANQIAN_FFMPEG_LOG=0, SongCore's discard \
         callback must be installed and swallow the probe. Instead {:?} reached stderr \
         through FFmpeg's default synchronous logger — nobody installed the discard \
         callback: exactly the two-layer contradiction the frozen contract §5 records \
         for the OLD predicate (install only when the variable is unset).",
        String::from_utf8_lossy(&c_layer_probe)
    );

    // ---- Scenario B: unset — the agreed baseline, witnessed at runtime. ----
    // SAFETY: as above; sequential, single consumer.
    unsafe { std::env::remove_var("QIANQIAN_FFMPEG_LOG") };
    probe_fixture();
    assert!(
        probe_stderr(AV_LOG_ERROR).is_empty(),
        "unset: the discard baseline is (re-)installed per song_open — both layers already \
         agree on this row; this also gives the C discard path its first runtime \
         observation (recorded NOT_RUN in the closeout contract §4.3)."
    );

    // ---- Scenario C: "1" — the repair must not break the existing opt-in. ----
    // SAFETY: as above.
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };
    let registration = AlphaRegistration::register();
    let outlet = test_outlet::facility();
    activate_decode_plugin(); // "1" → bridge_requested == true → the bridge installs (Once).
    unsafe { av_log(std::ptr::null_mut(), AV_LOG_ERROR, PROBE_MARKER.as_ptr()) };
    let bridged_line = format!("ffmpeg|error|{}", PROBE_MARKER.to_string_lossy());
    assert_eq!(
        outlet.wait_for_line(|line| line == bridged_line, "the bridged probe record"),
        bridged_line,
        "with \"1\" the bridge owns the av_log callback: the probe arrives as one \
         class-α record (β shape: the format string verbatim, no interpolation)"
    );
    probe_fixture(); // C gate: set → the install branch must not fire.
    unsafe { av_log(std::ptr::null_mut(), AV_LOG_ERROR, PROBE_MARKER.as_ptr()) };
    assert_eq!(
        outlet.wait_for_line(
            |line| line == bridged_line,
            "the post-open bridged probe record"
        ),
        bridged_line,
        "with \"1\" the per-open C gate must leave the bridged host's callback alone \
         (\"never clobbered\", av_log_bridge.rs:29-32): a second probe still arrives \
         through the bridge, not through a reinstalled discarder"
    );
    let _ = registration; // hold the α registration to the end of the witness

    // Leave the opt-in removed for any later witness in this binary; the
    // process posture it leaves behind is the bridge's, by FFmpeg's own
    // process-global lifetime (av_log_bridge.rs:36-39).
    // SAFETY: as above.
    unsafe { std::env::remove_var("QIANQIAN_FFMPEG_LOG") };
}
