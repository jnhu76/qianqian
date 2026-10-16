//! DG-4c — the opt-in FFmpeg `av_log` bridge (T5-9 posture, tier β).
//!
//! Baseline is unchanged and stays the default: SongCore installs its discard
//! callback when `QIANQIAN_FFMPEG_LOG` is unset. When the variable IS set (the
//! host's existing opt-in for FFmpeg diagnostics), the host — not SongCore —
//! owns the process-wide `av_log` callback from startup, and ranges it onto the
//! diagnostic outlet as class-α records.
//!
//! **β shape, and why it sends no interpolation.** The callback receives a
//! format string plus a C `va_list` whose referents are valid only for this
//! invocation. Rust cannot portably consume a `va_list`, and no copy can keep
//! the varargs' pointees alive past the callback, so the bridge records what
//! needs no varargs at all: the level, a bounded copy of the `fmt` string
//! itself, and the `av_class` name when one is attached. Interpolated values
//! (filenames, error numbers) are therefore honestly ABSENT from these records.
//! Recovering them needs a native `vsnprintf` shim (tier α), which is admitted
//! only when a named consumer demonstrates the need — no native change and no
//! ABI change is made here.
//!
//! **Discipline inside the callback (all of it load-bearing):** no FFmpeg API
//! re-entry, no allocation, no lock, no `write(2)`, no formatting, no panic
//! (a panic must not unwind across the C boundary), and no record publication
//! at all on a thread that registered no class-α pool: FFmpeg-calling threads
//! are decode-side and isolation-obligated, so the shared class-β channel is
//! deliberately NOT admitted here — such a record is dropped and counted. No
//! assumption is made about which FFmpeg-internal lock is held at callback
//! time; the discipline above is chosen so that the answer cannot change it.
//!
//! **Install ordering and surface.** The bridge installs once, from the decode
//! plugin's activation — the host-facing startup moment that precedes every
//! `song_open` that this provider can serve. SongCore's per-open reinstall only
//! fires when the variable is unset, so a bridged host is never clobbered. The
//! module is crate-private and adds NO public item: the decode chain's public
//! surface is frozen by the plugin-boundary gate to the plugin constructor and
//! the D14.6 probe query, so the host owns this decision through the
//! environment variable rather than through a new exported function. Concurrent
//! handles share the one callback and records carry no handle identity: this is
//! a process-level diagnostic, and per-handle attribution is explicitly not
//! provided.

use std::ffi::c_char;
use std::os::raw::c_void;
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};

use qianqian_diagnostics::{Level, MSG_CAP, Record, Text, emit, is_alpha_producer};

/// The environment switch that opts a host into FFmpeg diagnostics. Its meaning
/// for a bridged host extends from "keep FFmpeg's default logger" to "the host
/// owns the av_log callback" (a documentation change in the binding
/// architecture, never an ABI change).
const FFMPEG_LOG_ENV: &str = "QIANQIAN_FFMPEG_LOG";

/// The bridge records' site tag.
const BRIDGE_SITE: &str = "ffmpeg";

// FFmpeg's `AV_LOG_*` levels (libavutil/log.h, pinned release), the ones the
// mapping actually names:
//
//   PANIC 0   FATAL 8   ERROR 16   WARNING 24   INFO 32   VERBOSE 40
//   DEBUG 48   TRACE 56
//
// The mapping only needs the three boundaries it branches on; the severities
// between them collapse onto the record's three levels, and naming every
// constant would publish a table nobody reads.
const AV_LOG_PANIC: i32 = 0;
const AV_LOG_WARNING: i32 = 24;
const AV_LOG_INFO: i32 = 32;
const AV_LOG_DEBUG: i32 = 48;

/// FFmpeg's callback signature. `va_list` is passed as an opaque pointer: the
/// β shape never reads it (see the module docs).
type AvLogCallback = unsafe extern "C" fn(*mut c_void, i32, *const c_char, *mut c_void);

/// The first member of FFmpeg's `AVClass`. FFmpeg's own logging implementation
/// reaches an item's class the same way — `*(AVClass **)avcl` on the object
/// pointer — so this is that convention, with only the member the bridge reads:
/// declaring the whole struct would depend on far more of FFmpeg's layout than
/// the class name does.
#[repr(C)]
struct AvClass {
    class_name: *const c_char,
}

unsafe extern "C" {
    /// `void av_log_set_callback(void (*callback)(void*, int, const char*, va_list))`.
    fn av_log_set_callback(callback: Option<AvLogCallback>);

    /// The witness path only: driving a real `av_log` through the real callback
    /// is what proves the installation reached FFmpeg's global state.
    #[cfg(test)]
    fn av_log(avcl: *mut c_void, level: i32, fmt: *const c_char, ...);
}

/// Records the callback could not publish because the calling thread registered
/// no class-α pool. Process-level and relaxed: it orders nothing.
static BRIDGE_DROPPED: AtomicU64 = AtomicU64::new(0);

/// What one host-startup bridge attempt did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AvLogBridgeOutcome {
    /// `QIANQIAN_FFMPEG_LOG` is unset (or "0"): the bridge was NOT installed and
    /// SongCore's discard callback stays the process posture (T5-9 PRESERVE).
    NotRequested,
    /// The bridge is installed (this call, or an earlier one in this process).
    Installed,
}

/// Reads the bridge's drop count. The count itself is maintained in production
/// (the design's "drops+counts" rule for an unregistered calling thread); the
/// reader is the witness, because no operator-facing consumer exists today — a
/// host that wants bridge loss in its shutdown report would have to name one.
#[cfg(test)]
fn bridge_dropped() -> u64 {
    BRIDGE_DROPPED.load(Ordering::Relaxed)
}

/// The opt-in test as a pure function of the variable's value, so the gate has
/// a deterministic witness that does not mutate process-global state.
fn bridge_requested(value: Option<&std::ffi::OsStr>) -> bool {
    match value {
        Some(value) => value != "0",
        // Unset, or explicitly "0": the discard baseline stands.
        None => false,
    }
}

/// Install the bridge when the host opted in. Idempotent: the callback is
/// process-global, so a second call is a no-op reporting the same state.
///
/// No error is possible on this path: a host that opted into FFmpeg diagnostics
/// either owns the callback or the opt-in was not requested. Nothing else in the
/// program may depend on the outcome — this is diagnostics.
pub(crate) fn install_av_log_bridge() {
    let _ = install_av_log_bridge_decision();
}

/// The same call, returning the decision — the module-level witness's entry, kept
/// separate so the activation call site does not name a type it has no use for.
fn install_av_log_bridge_decision() -> AvLogBridgeOutcome {
    if bridge_requested(std::env::var_os(FFMPEG_LOG_ENV).as_deref()) {
        install_bridge_callback()
    } else {
        AvLogBridgeOutcome::NotRequested
    }
}

fn install_bridge_callback() -> AvLogBridgeOutcome {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| unsafe { av_log_set_callback(Some(bridge)) });
    AvLogBridgeOutcome::Installed
}

/// The host's `av_log` callback: bounded, allocation-free, panic-free, and
/// atomics-only when it publishes.
unsafe extern "C" fn bridge(avcl: *mut c_void, level: i32, fmt: *const c_char, _vl: *mut c_void) {
    let Some(level) = bridge_level(level) else {
        // Below the record floor (debug/trace): dropped by the level filter,
        // not counted as loss — a filter is not a failure.
        return;
    };
    if fmt.is_null() {
        return;
    }
    // A bounded copy with a NUL stop that never scans the whole string: a
    // broken `fmt` must not turn this callback into an unbounded walk.
    let mut bytes = [0u8; MSG_CAP];
    let mut len = 0usize;
    while len < bytes.len() {
        let byte = unsafe { *fmt.add(len) };
        if byte == 0 {
            break;
        }
        bytes[len] = byte as u8;
        len += 1;
    }
    // Admission (module docs): decode-side threads are variant-α; a record from
    // an unregistered thread is dropped and counted rather than queued.
    if !is_alpha_producer() {
        BRIDGE_DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let mut text = Text::empty();
    if !avcl.is_null() {
        // FFmpeg's own convention: the object's first word is its `AVClass *`.
        let class = unsafe { *(avcl as *const *const AvClass) };
        if !class.is_null() {
            let name = unsafe { (*class).class_name };
            if !name.is_null() {
                let mut name_len = 0usize;
                let mut name_bytes = [0u8; 64];
                while name_len < name_bytes.len() {
                    let byte = unsafe { *name.add(name_len) };
                    if byte == 0 {
                        break;
                    }
                    name_bytes[name_len] = byte as u8;
                    name_len += 1;
                }
                text.push_bytes(&name_bytes[..name_len]);
                text.push_str(": ");
            }
        }
    }
    text.push_bytes(&bytes[..len]);
    let _ = emit(Record::new(BRIDGE_SITE, level).with_message(text).stamped());
}

/// The record floor and level mapping: FFmpeg's severity vocabulary onto the
/// outlet's three levels. Anything below `AV_LOG_INFO` is filtered out.
fn bridge_level(level: i32) -> Option<Level> {
    if level >= AV_LOG_DEBUG {
        None
    } else if level >= AV_LOG_INFO {
        // INFO and VERBOSE.
        Some(Level::Info)
    } else if level >= AV_LOG_WARNING {
        Some(Level::Warn)
    } else {
        // ERROR, FATAL and PANIC: FFmpeg's error severities.
        debug_assert!(level >= AV_LOG_PANIC, "negative av_log level");
        Some(Level::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_diagnostics::{AlphaRegistration, Disposition};

    /// The floor/mapping table, asserted over FFmpeg's own constants.
    #[test]
    fn dg4c_level_mapping_and_floor() {
        assert_eq!(bridge_level(AV_LOG_PANIC), Some(Level::Error), "PANIC 0");
        assert_eq!(bridge_level(8), Some(Level::Error), "FATAL 8");
        assert_eq!(bridge_level(16), Some(Level::Error), "ERROR 16");
        assert_eq!(
            bridge_level(AV_LOG_WARNING),
            Some(Level::Warn),
            "WARNING 24"
        );
        assert_eq!(bridge_level(AV_LOG_INFO), Some(Level::Info), "INFO 32");
        assert_eq!(bridge_level(40), Some(Level::Info), "VERBOSE 40");
        assert_eq!(bridge_level(AV_LOG_DEBUG), None, "DEBUG 48");
        assert_eq!(bridge_level(56), None, "TRACE 56 is below the floor");
    }

    /// The byte copy is bounded and never runs past the cap — the property that
    /// keeps a broken `fmt` from becoming an unbounded walk.
    #[test]
    fn dg4c_the_format_copy_is_bounded() {
        let registration = AlphaRegistration::register();
        let long = vec![b'x'; MSG_CAP * 4];
        let mut terminated = long.clone();
        terminated.push(0);
        let before = registration.pool().outstanding();
        unsafe {
            bridge(
                std::ptr::null_mut(),
                16,
                terminated.as_ptr() as *const c_char,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(
            registration.pool().outstanding(),
            before + 1,
            "one record, however long the format string"
        );
        let mut last = 0u32;
        let mut last_record = None;
        registration.pool().drain(&mut last, &mut |record| {
            last_record = Some(*record);
        });
        let record = last_record.expect("copied");
        assert_eq!(
            record.message.len(),
            MSG_CAP,
            "the copy stops at the record bound"
        );
    }

    /// The unregistered-thread arm: no pool means no publication (the β channel
    /// is not admitted on this path) — the drop is counted instead.
    #[test]
    fn dg4c_an_unregistered_thread_drops_and_counts() {
        let before = bridge_dropped();
        let out_of_pool = std::thread::spawn(|| {
            // No AlphaRegistration on this thread.
            assert!(!is_alpha_producer());
            unsafe {
                bridge(
                    std::ptr::null_mut(),
                    AV_LOG_WARNING,
                    c"dg4c unregistered".as_ptr(),
                    std::ptr::null_mut(),
                )
            };
            bridge_dropped()
        })
        .join()
        .expect("witness thread");
        assert_eq!(
            out_of_pool,
            before + 1,
            "the unregistered-thread record is dropped and counted"
        );
    }

    /// The real end-to-end witness: after the bridge installs, an actual
    /// `av_log` call from a real FFmpeg entry point arrives as one record whose
    /// text is the (uninterpolated) format string and whose level came from the
    /// FFmpeg level. This is what proves the installation reached FFmpeg's
    /// process-global callback rather than a private copy of it.
    ///
    /// Process-global side effect: installing the callback affects every test in
    /// this binary. It is deliberate — the witness must drive the REAL callback —
    /// and harmless here, because no other test in this crate asserts on FFmpeg's
    /// own logging output.
    #[test]
    fn dg4c_a_real_av_log_call_arrives_as_one_record() {
        let registration = AlphaRegistration::register();
        assert_eq!(
            install_bridge_callback(),
            AvLogBridgeOutcome::Installed,
            "the witness forces installation; the env gate has its own witness"
        );
        let before = registration.pool().outstanding();
        unsafe { av_log(std::ptr::null_mut(), 16, c"dg4c witness fmt %d".as_ptr()) };
        assert_eq!(
            registration.pool().outstanding(),
            before + 1,
            "one av_log call, one record"
        );
        let mut last = 0u32;
        let mut site_and_message = None;
        registration.pool().drain(&mut last, &mut |record| {
            site_and_message = Some((
                record.site,
                record.level,
                record.message.as_str().to_owned(),
            ));
        });
        let (site, level, message) = site_and_message.expect("copied");
        assert_eq!(site, BRIDGE_SITE);
        assert_eq!(level, Level::Error);
        assert_eq!(
            message, "dg4c witness fmt %d",
            "the β shape copies the format string verbatim: interpolated values are honestly absent"
        );
        assert_eq!(
            emit(Record::new("dg4c-probe", Level::Info)),
            Disposition::AlphaRecorded,
            "the witness published through the class-α pool, not the class-β channel"
        );
    }

    /// The class-name read, witnessed with an object that follows FFmpeg's own
    /// convention (its first word is its `AVClass *`): the name is carried as the
    /// record's prefix.
    #[test]
    fn dg4c_the_av_class_name_prefixes_the_record() {
        let registration = AlphaRegistration::register();
        let named = AvClass {
            class_name: c"DG4CClass".as_ptr(),
        };
        let object_with_class: *const AvClass = &named;
        let before = registration.pool().outstanding();
        unsafe {
            bridge(
                &object_with_class as *const *const AvClass as *mut c_void,
                AV_LOG_INFO,
                c"named item".as_ptr(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(registration.pool().outstanding(), before + 1);
        let mut last = 0u32;
        let mut copied = Vec::new();
        registration.pool().drain(&mut last, &mut |record| {
            copied.push(record.message.as_str().to_owned())
        });
        assert!(
            copied.iter().any(|line| line == "DG4CClass: named item"),
            "the class name prefixes the record: {copied:?}"
        );
    }

    /// The env gate: without the opt-in the bridge is not installed and the
    /// discard baseline stands. This witnesses the DECISION (a pure function of
    /// the variable's value — never a mutated process environment), not FFmpeg's
    /// runtime callback state: the discard path is C's, unchanged, and the
    /// pinned-FFmpeg runtime observation stays on its own evidence boundary.
    #[test]
    fn dg4c_without_the_opt_in_the_bridge_is_not_installed() {
        use std::ffi::OsStr;
        assert!(!bridge_requested(None), "unset: discard stays");
        assert!(
            !bridge_requested(Some(OsStr::new("0"))),
            "explicit zero: discard stays"
        );
        assert!(bridge_requested(Some(OsStr::new("1"))));
        assert!(bridge_requested(Some(OsStr::new("warn"))));
    }
}
