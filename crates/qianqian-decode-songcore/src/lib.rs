//! Real Decode capability provider over the SongCore C ABI v1.
//!
//! Mechanism provider only: it owns the SongCore binding (ABI identity
//! check, host-IO callback machinery, handle RAII) and publishes the
//! `PcmDecode` capability through a kernel `ComponentSpec`. Per-episode
//! state — one opened media file, one `DecodedPcmStream` endpoint —
//! belongs to the caller (the Playback Session), never to this provider
//! (first-audible-slice design §2).

use std::fs::File;
use std::os::raw::c_void;
use std::path::Path;
use std::rc::Rc;
use std::slice;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use qianqian_audio_api::ports::{
    DecodeError, DecodeOpenError, DecodeOutcome, DecodedPcmStream, PcmDecode, PcmDecodeCapability,
    PcmFormat, ProviderSeekOutcome, SourceFacts,
};
use qianqian_composition::{ActivationError, ComponentSpec};
use qianqian_diagnostics::{Level, Record, Text, emit};
use qianqian_songcore_sys as sys;

pub mod av_log_bridge;

/// The adapter's diagnostic site tag (DG-3's consumer-facing identity).
const DIAGNOSTIC_SITE: &str = "qianqian-decode-songcore";

/// Real decode mechanism over one SongCore native library instance.
/// Long-lived and stateless across opens.
///
/// Crate-private by design (plugin-boundary hardening H1): this is the
/// concrete mechanism, not product API. Composition roots admit this
/// plugin through [`songcore_decode_plugin`]; the capability consumer
/// sees only the `PcmDecode` service trait.
struct SongcoreDecode {
    _private: (),
}

impl SongcoreDecode {
    /// Bind the mechanism fail-closed: the loaded native library must
    /// report the ABI version this crate was generated against.
    fn new() -> Result<Self, DecodeOpenError> {
        let loaded = unsafe { sys::songcore_abi_version() };
        if loaded != sys::SONGCORE_ABI_VERSION {
            return Err(DecodeOpenError {
                message: format!(
                    "SongCore ABI mismatch: native library reports v{loaded}, \
                     bindings were generated for v{}",
                    sys::SONGCORE_ABI_VERSION
                ),
            });
        }
        Ok(Self { _private: () })
    }
}

impl PcmDecode for SongcoreDecode {
    fn open_media(&self, path: &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
        SongcoreDecodeStream::open(path)
    }
}

// --- host-IO callbacks over the adapter-owned context --------------------
//
// FFI boundary discipline: every callback fails closed (-1) on a null
// userdata pointer, a null destination (regardless of the requested
// length: `from_raw_parts_mut` requires a non-null data pointer even for
// a zero-length slice), and a negative absolute offset (the ABI defines
// offsets as non-negative). The pointer preconditions for
// `from_raw_parts_mut` are checked before the slice is formed; the
// SongCore side never legitimately passes these values, so a guard hit is
// a caller bug, not a decode result.
//
// DG-4a: the callbacks are panic-free by construction (io operations plus
// a bounded slot write — no unwrap, no formatting, no allocation), and a
// failing callback records its host-io fault into the adapter-owned
// context before returning -1. The OS detail therefore survives the
// callback's `-1` return through the binding's own userdata side channel.
// Attribution is by the TOP-LEVEL operation's call-boundary id: a fault may
// be reported beside an operation's diagnostic only when it happened inside
// that operation, so no later failure can inherit an earlier transient.

/// One recorded host-io fault (bounded fields only; no owned message).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoundedIoFault {
    /// The top-level call-boundary id this fault happened in.
    boundary: u64,
    operation: &'static str,
    kind: std::io::ErrorKind,
    raw_os_error: Option<i32>,
    offset: u64,
}

/// The binding-owned context behind `song_io.userdata`: the open file, the
/// last known stream position, the top-level call-boundary id, and the
/// first-fault slot.
struct IoContext {
    file: File,
    position: AtomicU64,
    boundary: AtomicU64,
    /// First fault within the CURRENT boundary (later ones only count), plus
    /// the count of every recorded fault. Kept after a successful call as a
    /// non-attributable record (an operator may ask; nothing attaches it
    /// automatically), cleared when the next operation begins.
    fault: Mutex<Option<BoundedIoFault>>,
    fault_count: AtomicU64,
}

impl IoContext {
    fn new(file: File) -> Self {
        Self {
            file,
            position: AtomicU64::new(0),
            boundary: AtomicU64::new(0),
            fault: Mutex::new(None),
            fault_count: AtomicU64::new(0),
        }
    }

    /// Begin one top-level operation: a new call-boundary id, an empty slot.
    /// Returns the boundary id this operation's callbacks will stamp.
    fn begin_operation(&self) -> u64 {
        let boundary = self.boundary.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut slot) = self.fault.lock() {
            *slot = None;
        }
        boundary
    }

    /// Record a callback fault (first-wins inside the boundary).
    fn record_fault(&self, operation: &'static str, error: &std::io::Error, offset: u64) {
        self.fault_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut slot) = self.fault.lock()
            && slot.is_none()
        {
            *slot = Some(BoundedIoFault {
                boundary: self.boundary.load(Ordering::SeqCst),
                operation,
                kind: error.kind(),
                raw_os_error: error.raw_os_error(),
                offset,
            });
        }
    }

    /// The slot's record IF it belongs to `boundary` — the attribution gate
    /// that keeps a stale io fault from explaining a later failure.
    fn attributable_fault(&self, boundary: u64) -> Option<BoundedIoFault> {
        self.fault
            .lock()
            .ok()
            .and_then(|slot| *slot)
            .filter(|fault| fault.boundary == boundary)
    }

    /// The retained record regardless of attribution: the operator-facing
    /// inspection accessor. No automatic path calls it — a transient fault
    /// inside a call that ultimately succeeds is retained but never attached
    /// to a later failure.
    #[allow(dead_code)]
    fn retained_fault(&self) -> Option<BoundedIoFault> {
        self.fault.lock().ok().and_then(|slot| *slot)
    }

    fn fault_count(&self) -> u64 {
        self.fault_count.load(Ordering::Relaxed)
    }
}

unsafe extern "C" fn file_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    use std::io::Read;
    let context = unsafe { &mut *(ud as *mut IoContext) };
    let buf = unsafe { slice::from_raw_parts_mut(dst, size) };
    match context.file.read(buf) {
        Ok(n) => {
            context.position.fetch_add(n as u64, Ordering::Relaxed);
            n as i64
        }
        Err(error) => {
            let offset = context.position.load(Ordering::Relaxed);
            context.record_fault("read", &error, offset);
            -1
        }
    }
}

unsafe extern "C" fn file_seek(ud: *mut c_void, offset: i64) -> i64 {
    if ud.is_null() || offset < 0 {
        return -1;
    }
    use std::io::{Seek, SeekFrom};
    let context = unsafe { &mut *(ud as *mut IoContext) };
    match context.file.seek(SeekFrom::Start(offset as u64)) {
        Ok(n) => {
            context.position.store(n, Ordering::Relaxed);
            n as i64
        }
        Err(error) => {
            context.record_fault("seek", &error, offset as u64);
            -1
        }
    }
}

unsafe extern "C" fn file_size(ud: *mut c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let context = unsafe { &mut *(ud as *mut IoContext) };
    match context.file.metadata() {
        Ok(m) => m.len() as i64,
        Err(error) => {
            let offset = context.position.load(Ordering::Relaxed);
            context.record_fault("size", &error, offset);
            -1
        }
    }
}

/// The adapter-internal typed fault (DG-3 shape (a)): typed status +
/// native code + a bounded inline message, copied at the error point and
/// consumed immediately by the diagnostic outlet and the App-facing message
/// text. Never long-lived state: a `last_fault` field would be an idle
/// record with no reader obligation between errors.
#[derive(Clone, Copy)]
struct DecodeFault {
    status: u32,
    native_code: i32,
    /// Inline fixed-capacity text (no heap); the ABI caps the borrowed
    /// diagnostic at 255 bytes, so this copy cannot truncate it.
    message: Text,
}

impl DecodeFault {
    /// Render the fault for a public message string (the one place the
    /// message becomes owned text, exactly as before DG-3).
    fn message_string(&self) -> String {
        let rendered = self.message.render();
        if rendered.is_empty() {
            "SongCore decode failed".to_owned()
        } else {
            rendered.into_owned()
        }
    }
}

/// Emit one fault to the diagnostic outlet. Class α when the calling thread
/// is a registered isolation-obligated producer (the decode worker is), so
/// the call is a bounded atomics-only publish; otherwise it rides the
/// facility's bounded channel. Best-effort by contract: a logging failure
/// must never change a decode result.
fn emit_decode_fault(fault: &DecodeFault) {
    let record = Record::new(DIAGNOSTIC_SITE, Level::Error)
        .with_message(fault.message)
        .with_status(fault.status as i32)
        .with_native_code(fault.native_code)
        .stamped();
    let _ = emit(record);
}

/// Build the retained typed fault from the currently valid borrowed ABI
/// diagnostic. Must be called BEFORE any next same-handle SongCore call: the
/// ABI's diagnostics are valid only until then.
fn read_fault(handle: *mut sys::song_handle, status: u32) -> DecodeFault {
    let mut native_code = 0i32;
    let mut message = Text::empty();
    let mut err: *const sys::song_error = std::ptr::null();
    let probe = unsafe { sys::song_last_error(handle, &mut err) };
    if probe == sys::SONG_OK && !err.is_null() {
        let error = unsafe { &*err };
        native_code = error.native_code;
        if !error.message.is_null() && error.message_len > 0 {
            let bytes = unsafe {
                slice::from_raw_parts(error.message as *const u8, error.message_len as usize)
            };
            message = Text::from_bytes(bytes);
        }
    }
    if message.is_empty() {
        message = Text::from_static("SongCore decode failed");
    }
    DecodeFault {
        status,
        native_code,
        message,
    }
}

/// The container-declared duration as product evidence (ADR-PBK-002
/// D14.8). The ABI documents `duration_us == -1` as "the container
/// declares no duration"; every negative value is treated the same way,
/// fail-closed, so no sentinel can be mistaken for a real length.
///
/// A reported `0` is NOT folded into "unknown": it stays a legitimate
/// zero-length source, which the observation can therefore distinguish
/// from an absent duration. The value is relayed as mechanism evidence
/// and never corrected here — not at EOF, and not against the decoded
/// frame total (the probe measured a truncated stream still declaring
/// its full original length).
fn probe_duration(duration_us: i64) -> Option<Duration> {
    u64::try_from(duration_us).ok().map(Duration::from_micros)
}

/// One playback-specific decode endpoint: owns one native song handle and
/// the file it reads from. Released on drop (song_close), single-thread
/// serialized by contract (`DecodedPcmStream: Send` but not `Sync`).
/// Crate-private: consumers reach it only as the `DecodedPcmStream` trait
/// object returned by the capability service.
struct SongcoreDecodeStream {
    handle: *mut sys::song_handle,
    format: PcmFormat,
    /// Source duration evidence from the same `song_probe` snapshot that
    /// produced the format: the container's own declaration, `None` when
    /// it declared none (the ABI's `-1` sentinel). Truth class per
    /// ADR-PBK-002 D14.8: optional source-scoped Mechanism Evidence —
    /// never a Fact and not exact in general (a truncated stream still
    /// declares its original length).
    duration: Option<Duration>,
    /// Kept alive for the handle's lifetime; the callbacks borrow it raw.
    /// Declared after `handle` so drop runs song_close while the file is
    /// still alive. Beyond keeping the file open, this context is the
    /// adapter-owned side channel DG-4a records host-io faults into and the
    /// attribution boundary those faults are tagged with.
    #[allow(dead_code)]
    io: Box<IoContext>,
}

// The native handle is externally serialized (SongCore contract): moving
// the endpoint to another thread is safe; concurrent calls are not made.
unsafe impl Send for SongcoreDecodeStream {}

impl SongcoreDecodeStream {
    fn open(path: &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
        let file = File::open(path).map_err(|e| DecodeOpenError {
            message: format!("cannot open this file: {}: {e}", path.display()),
        })?;
        let io_context = Box::new(IoContext::new(file));
        let io = sys::song_io {
            userdata: Box::into_raw(io_context) as *mut c_void,
            read: Some(file_read),
            seek: Some(file_seek),
            size: Some(file_size),
        };
        // The open operation's call boundary: every callback fault recorded
        // from here until the next boundary belongs to THIS operation.
        let open_boundary = unsafe { (*(io.userdata as *const IoContext)).begin_operation() };
        let mut handle: *mut sys::song_handle = std::ptr::null_mut();
        let status = unsafe { sys::song_open(&io, &mut handle) };
        if status != sys::SONG_OK {
            // The open failure exposes the typed status and nothing else: the
            // native side nulls its out-handle on EVERY open failure
            // (`song_open` assigns `*out_handle = NULL` first and never
            // publishes a handle on a failure path, verified at the pinned
            // closure), so there is no handle to read a diagnostic from —
            // `song_last_error` needs one. This CONFIRMS the ABI note in
            // songcore.h:216-219 that `song_open` carries no diagnostic
            // contract; the retention window matters on the `song_probe`
            // path, which does run on a live handle.
            let context = unsafe { &*(io.userdata as *const IoContext) };
            let io_note = host_io_note(context.attributable_fault(open_boundary));
            let count = context.fault_count();
            unsafe { drop(Box::from_raw(io.userdata as *mut IoContext)) };
            return Err(DecodeOpenError {
                message: format!(
                    "cannot open this file: {} (decoder status {status}){io_note}{}",
                    path.display(),
                    host_io_count_note(count)
                ),
            });
        }
        // Fail closed: the ABI does not spell out "SONG_OK implies a
        // non-null handle", so the wrapper refuses to build on one. A null
        // here would make every later handle call undefined behavior.
        if handle.is_null() {
            unsafe {
                sys::song_close(handle);
                drop(Box::from_raw(io.userdata as *mut IoContext));
            }
            return Err(DecodeOpenError {
                message: format!(
                    "decoder error: opened {} successfully but it returned no handle",
                    path.display()
                ),
            });
        }
        let probe_boundary = unsafe { (*(io.userdata as *const IoContext)).begin_operation() };
        let mut info: sys::song_info = unsafe { std::mem::zeroed() };
        let status = unsafe { sys::song_probe(handle, &mut info) };
        if status != sys::SONG_OK {
            // DG-3/T1-2: the C side has already latched its diagnostic for
            // this failure, and the borrowed diagnostic is valid only until
            // the next same-handle call — so read it BEFORE song_close, copy
            // it into owned bounded text, and carry it in the message.
            let fault = read_fault(handle, status);
            let context = unsafe { &*(io.userdata as *const IoContext) };
            let io_note = host_io_note(context.attributable_fault(probe_boundary));
            let count = context.fault_count();
            unsafe {
                sys::song_close(handle);
                drop(Box::from_raw(io.userdata as *mut IoContext));
            }
            return Err(DecodeOpenError {
                message: format!(
                    "cannot read this file's audio: {} (decoder status {status}, native {}, diagnostic: {}){io_note}{}",
                    path.display(),
                    fault.native_code,
                    fault.message_string(),
                    host_io_count_note(count)
                ),
            });
        }
        // A successful open leaves any transient callback fault in the slot
        // as a NON-ATTRIBUTABLE record: an operator may ask for it, nothing
        // attaches it to a later failure automatically.
        Ok(Box::new(SongcoreDecodeStream {
            handle,
            format: PcmFormat {
                sample_rate: info.sample_rate as u32,
                channels: info.channels as u16,
                channel_mask: info.channel_mask,
            },
            duration: probe_duration(info.duration_us),
            io: unsafe { Box::from_raw(io.userdata as *mut IoContext) },
        }))
    }

    /// The typed fault of the current failure, copied while the borrowed ABI
    /// diagnostic is still valid.
    fn fault(&self, status: u32) -> DecodeFault {
        read_fault(self.handle, status)
    }
}

/// A host-io fault rendered as correlated evidence beside an operation's own
/// status. It never replaces the operation's classification: the C-side
/// status stays the root and this note says only what the callbacks saw.
fn host_io_note(fault: Option<BoundedIoFault>) -> String {
    match fault {
        None => String::new(),
        Some(fault) => format!(
            "; host io {} failed (kind {:?}, os error {:?}, offset {})",
            fault.operation, fault.kind, fault.raw_os_error, fault.offset
        ),
    }
}

/// The count of recorded callback faults, reported only when some happened.
fn host_io_count_note(count: u64) -> String {
    if count == 0 {
        String::new()
    } else {
        format!("; {count} host io callback failure(s) recorded")
    }
}

/// Raw `song_seek` status → the frozen semantic class (ADR-PBK-002
/// D14.5), payload-free so the mapping is pinnable without a live
/// handle. Exactly ONE status proves the pre-call decoding continuation
/// intact: `SONG_ERR_INVALID_ARGUMENT` is the ABI's pure parameter
/// check, which runs only on an OPEN handle (the not-probed guard
/// returns earlier) and mutates nothing before returning.
///
/// Everything else is destructive by the conservative rule — unprovable
/// means destructive:
///   - `SONG_ERR_NOT_OPEN` reports `!probed || !dec`: the handle is NOT
///     in an opened/probed state, so it certifies no usable old cursor.
///     A constructed endpoint is always probed, so this status is an
///     abnormal provider state, not an inert refusal — it must fail
///     closed (the F5 gate earned `INVALID_ARGUMENT` only, and never
///     measured a not-opened handle's continuation).
///   - generic `SEEK_ERROR` is returned both before `av_seek_frame` and
///     again after a successful reposition + decoder flush/reset, so
///     the status code alone proves nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeekClass {
    Applied,
    RefusedUnchanged,
    MutatedThenFailed,
}

fn classify_seek_status(status: u32) -> SeekClass {
    match status {
        sys::SONG_OK => SeekClass::Applied,
        sys::SONG_ERR_INVALID_ARGUMENT => SeekClass::RefusedUnchanged,
        _ => SeekClass::MutatedThenFailed,
    }
}

impl DecodedPcmStream for SongcoreDecodeStream {
    fn format(&self) -> PcmFormat {
        self.format
    }

    fn source_duration(&self) -> Option<Duration> {
        self.duration
    }

    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
        let channels = self.format.channels as usize;
        if channels == 0 || dst.len() < channels {
            return Err(DecodeError {
                message: format!(
                    "decode destination holds {} samples, needs at least one frame ({channels})",
                    dst.len()
                ),
            });
        }
        let capacity = (dst.len() / channels) as u64;
        let mut produced: u64 = 0;
        // One top-level call boundary per `song_read_pcm`: a callback fault
        // recorded during this call is attributable to it; one recorded by an
        // earlier call is not.
        let boundary = self.io.begin_operation();
        let status =
            unsafe { sys::song_read_pcm(self.handle, dst.as_mut_ptr(), capacity, &mut produced) };
        match status {
            sys::SONG_OK => Ok(DecodeOutcome::Frames(produced as usize)),
            sys::SONG_EOF => Ok(DecodeOutcome::Eof),
            _ => {
                // DG-3: copy the typed fault at the error point, before any
                // next same-handle call invalidates the borrowed diagnostic,
                // then hand it to the diagnostic outlet and to the
                // App-facing message text. The emit is best-effort: it can
                // never change the decode result.
                let fault = self.fault(status);
                emit_decode_fault(&fault);
                let io_note = host_io_note(self.io.attributable_fault(boundary));
                Err(DecodeError {
                    message: format!("{}{io_note}", fault.message_string(),),
                })
            }
        }
    }

    fn seek(&mut self, target: Duration) -> ProviderSeekOutcome {
        // The ABI's native unit is microseconds; clamp the (unbounded)
        // Duration into it rather than inventing a wrap. Beyond-duration
        // and invalid targets stay the PROVIDER's decision (the ABI
        // clamps against known duration; negative is unrepresentable
        // here).
        let target_us = i64::try_from(target.as_micros()).unwrap_or(i64::MAX);
        let mut landing_us: i64 = -1;
        let boundary = self.io.begin_operation();
        let status = unsafe { sys::song_seek(self.handle, target_us, &mut landing_us) };
        match classify_seek_status(status) {
            SeekClass::Applied => {
                // The ABI reports the retained-PCM start measured from
                // the first decoded frame's timestamp; -1 is the
                // explicit "landing unknown" sentinel (never
                // manufactured, never replaced by the requested target).
                // Convert to source PCM frames — the Position
                // projection's unit — rounding the µs quantization
                // (measured ±1 frame; mechanism noise, not a lie).
                let landing = u64::try_from(landing_us).ok().map(|us| {
                    let rate = u128::from(self.format.sample_rate);
                    ((u128::from(us) * rate + 500_000) / 1_000_000) as u64
                });
                ProviderSeekOutcome::Applied { landing }
            }
            SeekClass::RefusedUnchanged => ProviderSeekOutcome::RefusedUnchanged,
            SeekClass::MutatedThenFailed => {
                let fault = self.fault(status);
                emit_decode_fault(&fault);
                let io_note = host_io_note(self.io.attributable_fault(boundary));
                ProviderSeekOutcome::MutatedThenFailed {
                    diagnostic: format!("{}{io_note}", fault.message_string()),
                }
            }
        }
    }
}

impl Drop for SongcoreDecodeStream {
    fn drop(&mut self) {
        // Defensive FFI-boundary hardening: every open path above either
        // leaves a non-null handle or refuses to construct the endpoint, so
        // the guard below can only fire on a construction invariant
        // violation — it exists to keep Drop itself sound, not to excuse
        // a null handle reaching the endpoint.
        if !self.handle.is_null() {
            unsafe { sys::song_close(self.handle) };
        }
    }
}

/// The F6 source preflight query (ADR-PBK-002 D14.6, the
/// F6-AUTHORITY-PROMOTION-1 amendment): ONE public, stateless mechanism
/// query — open the media, read the container's declared facts, close.
/// It reuses the production open/probe machinery internally (`song_open`
/// + `song_probe`) and reads NO PCM frames; it owns no render stream, no
/// edge, no worker, no device session — nothing RT-visible — so it is
/// not a second live playback episode and does not trigger P1–P5.
///
/// The result is mechanism evidence for an application composition
/// decision (an Open preflight: "this source opened and declared X at
/// probe time") — advisory, never episode truth; the episode
/// activation's own open/probe publishes the authoritative
/// `source_format` / `source_duration` evidence. Failure classes are
/// exactly [`DecodeOpenError`] today.
///
/// D13 record: a plain one-shot read operation owned by its caller —
/// no Capability, no Plugin, no composition identity. This is the
/// decode provider's ONLY public surface besides the plugin
/// constructor; the boundary gate rule was updated by the same
/// amendment on purpose.
pub fn probe_media(path: &Path) -> Result<SourceFacts, DecodeOpenError> {
    // Constructing the stream performs exactly song_open + song_probe
    // (the production open path); dropping it runs song_close. No
    // `read_frames` call exists on this path. The declared facts are
    // read through the endpoint trait — the same evidence the session
    // relays at activation.
    let stream = SongcoreDecodeStream::open(path)?;
    let facts = SourceFacts {
        format: stream.format(),
        duration: stream.source_duration(),
    };
    drop(stream);
    Ok(facts)
}

/// The Decode Plugin component definition: provides the `PcmDecode`
/// capability backed by the real SongCore mechanism.
pub fn songcore_decode_plugin() -> ComponentSpec {
    ComponentSpec::new("songcore_decode_plugin")
        .provides::<PcmDecodeCapability>()
        .on_activate(|ctx| {
            let service = SongcoreDecode::new().map_err(|e| ActivationError::new(e.message))?;
            ctx.provide::<PcmDecodeCapability>(Rc::new(service))
                .map_err(|e| ActivationError::new(format!("provision refused: {e:?}")))?;
            Ok(())
        })
}
#[cfg(test)]
mod tests {
    // Callback-guard regressions from NATIVE-BOUNDARY-AUDIT-0（round record：Git 历史 / PR #134）. These run
    // against the callback machinery only — no native library, no fixture.
    // They are the executable form of the guard contract: a host-IO
    // callback must fail closed (-1) without ever forming a Rust slice
    // from a pointer the slice rules forbid.
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("qianqian-{name}-{}.tmp", std::process::id()))
    }

    /// Holds a leaked `Box<IoContext>` (the production userdata shape) for
    /// the duration of one assertion, reclaiming it on drop.
    struct RawFile(*mut c_void);
    impl RawFile {
        fn from_temp() -> Self {
            let path = std::env::temp_dir().join("qianqian-callback-guard-test");
            let context = Box::new(IoContext::new(File::create(path).unwrap()));
            Self(Box::into_raw(context) as *mut c_void)
        }
        fn as_ud(&self) -> *mut c_void {
            self.0
        }
    }
    impl Drop for RawFile {
        fn drop(&mut self) {
            if !self.0.is_null() {
                drop(unsafe { Box::from_raw(self.0 as *mut IoContext) });
            }
        }
    }

    /// S1 regression: a null destination is rejected even for a
    /// zero-length read. `from_raw_parts_mut` requires a non-null data
    /// pointer for every slice, length zero included — the guard must
    /// fire before the slice is formed, not rely on the native side
    /// never issuing this call. (Negative control: with the guard
    /// relaxed to `size > 0 &&`, Miri flags this call as UB at the
    /// `from_raw_parts_mut` site.)
    #[test]
    fn read_fails_closed_on_null_destination_even_for_zero_length() {
        let ud = RawFile::from_temp();
        let dst: *mut u8 = std::ptr::null_mut();
        unsafe {
            assert_eq!(file_read(ud.as_ud(), dst, 0), -1);
            assert_eq!(file_read(ud.as_ud(), dst, 16), -1);
        }
    }

    #[test]
    fn read_fails_closed_on_null_userdata() {
        let mut byte = 0u8;
        unsafe {
            assert_eq!(file_read(std::ptr::null_mut(), &mut byte, 1), -1);
            assert_eq!(file_seek(std::ptr::null_mut(), 0), -1);
            assert_eq!(file_size(std::ptr::null_mut()), -1);
        }
    }

    /// The ABI defines seek offsets as non-negative absolute positions.
    #[test]
    fn seek_fails_closed_on_negative_offset() {
        let ud = RawFile::from_temp();
        unsafe {
            assert_eq!(file_seek(ud.as_ud(), -1), -1);
        }
    }

    // --- DG-4a: the adapter-owned host-io fault slot -----------------------

    /// Oracle: the slot records the host-io fault with its real (kind,
    /// raw_os_error) — the OS detail that the callback's -1 return cannot
    /// carry — and it is flagged with the boundary that produced it.
    #[test]
    fn the_io_slot_retains_the_os_detail_of_a_callback_failure() {
        use std::io::Error;
        let context = IoContext::new(File::create(temp_path("dg4a-slot")).unwrap());
        let boundary = context.begin_operation();
        let error = Error::from_raw_os_error(5); // EIO
        context.record_fault("read", &error, 4096);
        let fault = context.attributable_fault(boundary).expect("attributable");
        assert_eq!(fault.operation, "read");
        // A synthesized raw-os error carries no kind classification; the
        // (kind, raw_os_error) PAIR is the retained identity, and the errno
        // is the part the -1 return cannot carry.
        assert_eq!(fault.kind, error.kind());
        assert_eq!(fault.raw_os_error, Some(5));
        assert_eq!(fault.offset, 4096);
        assert_eq!(fault.boundary, boundary);
        assert_eq!(context.fault_count(), 1);
    }

    /// Oracle (the attribution gate + the clearing discipline): a fault
    /// recorded in an EARLIER boundary is never attributed to a later
    /// operation — the counterexample the design names ("a stale error
    /// attributed to a later successful operation") — because a new
    /// top-level operation clears the slot. Before that clear, the record is
    /// retained for explicit operator inspection while remaining
    /// unattributable to any other boundary.
    #[test]
    fn an_earlier_boundarys_fault_is_never_attributed_to_a_later_operation() {
        use std::io::Error;
        let context = IoContext::new(File::create(temp_path("dg4a-stale")).unwrap());
        let first = context.begin_operation();
        context.record_fault("read", &Error::from_raw_os_error(5), 1);
        assert!(context.attributable_fault(first).is_some());
        assert!(
            context.retained_fault().is_some(),
            "an operator may still ask about it after the call returns"
        );
        assert!(
            context.attributable_fault(first + 1).is_none(),
            "it is attributed to its OWN boundary only"
        );
        let second = context.begin_operation();
        assert!(
            context.attributable_fault(second).is_none(),
            "a later operation must not inherit an earlier transient fault"
        );
        assert!(
            context.retained_fault().is_none(),
            "a new top-level operation clears the slot (the clearing discipline)"
        );
    }

    /// Oracle: first-wins within one boundary, with later faults counted —
    /// the origin is preserved and the volume is visible.
    #[test]
    fn the_first_fault_in_a_boundary_wins_and_later_ones_are_counted() {
        use std::io::Error;
        let context = IoContext::new(File::create(temp_path("dg4a-firstwins")).unwrap());
        let boundary = context.begin_operation();
        context.record_fault("read", &Error::from_raw_os_error(5), 10);
        context.record_fault("read", &Error::from_raw_os_error(9), 20);
        context.record_fault("seek", &Error::from_raw_os_error(22), 30);
        let fault = context.attributable_fault(boundary).expect("attributable");
        assert_eq!(fault.raw_os_error, Some(5), "the origin is first-wins");
        assert_eq!(fault.offset, 10);
        assert_eq!(context.fault_count(), 3, "every failure is counted");
    }

    /// Oracle: the PRODUCTION callback records a real errno through the real
    /// code path (`file_read` over a directory descriptor returns EISDIR on
    /// Linux) and still fails closed with -1 — a bounded slot write, no
    /// panic, no allocation, no formatting.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_production_callback_records_a_real_errno_and_fails_closed() {
        let directory = File::open("/").expect("a directory descriptor opens read-only");
        let context = Box::new(IoContext::new(directory));
        let raw = Box::into_raw(context);
        let boundary = unsafe { (*raw).begin_operation() };
        let mut byte = 0u8;
        let result = unsafe { file_read(raw as *mut c_void, &mut byte, 1) };
        assert_eq!(result, -1, "the callback fails closed");
        let fault = unsafe { (*raw).attributable_fault(boundary) };
        let fault = fault.expect("the real errno is retained");
        assert_eq!(fault.operation, "read");
        assert_eq!(
            fault.raw_os_error,
            Some(21),
            "EISDIR is the real OS detail the -1 return cannot carry"
        );
        drop(unsafe { Box::from_raw(raw) });
    }

    /// The raw-status → semantic-class map (ADR-PBK-002 D14.5) is the one
    /// place the F5 fail-closed rule can be lost silently: the session
    /// resumes its old cursor ONLY on `RefusedUnchanged`. Exactly one
    /// status earns that class — the ABI's pure parameter check, which
    /// runs on an OPEN handle and mutates nothing.
    ///
    /// `SONG_ERR_NOT_OPEN` in particular is NOT a refusal: it reports
    /// that the handle is not in an opened/probed state, hence certifies
    /// no usable old cursor. Promoting it would let a broken decode
    /// continuation look inert (F5-SEEK-IMPLEMENTATION-CORRECTIVE-2).
    #[test]
    fn only_invalid_argument_earns_a_pre_mutation_refusal() {
        assert_eq!(classify_seek_status(sys::SONG_OK), SeekClass::Applied);
        assert_eq!(
            classify_seek_status(sys::SONG_ERR_INVALID_ARGUMENT),
            SeekClass::RefusedUnchanged
        );

        // The whole declared ABI surface besides that one status — plus
        // EOF — is unprovable, therefore destructive.
        let destructive = [
            ("SONG_EOF", sys::SONG_EOF),
            ("SONG_ERR_NOT_OPEN", sys::SONG_ERR_NOT_OPEN),
            ("SONG_ERR_STATE", sys::SONG_ERR_STATE),
            ("SONG_ERR_IO", sys::SONG_ERR_IO),
            (
                "SONG_ERR_UNSUPPORTED_CONTAINER",
                sys::SONG_ERR_UNSUPPORTED_CONTAINER,
            ),
            ("SONG_ERR_NO_AUDIO_STREAM", sys::SONG_ERR_NO_AUDIO_STREAM),
            (
                "SONG_ERR_UNSUPPORTED_CODEC",
                sys::SONG_ERR_UNSUPPORTED_CODEC,
            ),
            ("SONG_ERR_CORRUPT_DATA", sys::SONG_ERR_CORRUPT_DATA),
            ("SONG_ERR_DECODE_ERROR", sys::SONG_ERR_DECODE_ERROR),
            ("SONG_ERR_SEEK_UNSUPPORTED", sys::SONG_ERR_SEEK_UNSUPPORTED),
            ("SONG_ERR_SEEK_ERROR", sys::SONG_ERR_SEEK_ERROR),
            ("SONG_ERR_STREAM_CHANGE", sys::SONG_ERR_STREAM_CHANGE),
            ("SONG_ERR_OUT_OF_MEMORY", sys::SONG_ERR_OUT_OF_MEMORY),
            ("SONG_ERR_INTERNAL_ERROR", sys::SONG_ERR_INTERNAL_ERROR),
        ];
        for (name, status) in destructive {
            assert_eq!(
                classify_seek_status(status),
                SeekClass::MutatedThenFailed,
                "{name} must never let the session resume old-cursor production"
            );
        }
    }
}

// White-box tests, moved inside the crate boundary by the plugin-boundary
// hardening (H1): they exercise the concrete mechanism and the real
// native corpus, so the mechanism stays crate-private and the tests
// reach it through the crate path. The shared fixture helper stays in
// tests/common/ (not auto-discovered) and is included once here.
#[cfg(test)]
mod decode_endpoint_tests;
#[cfg(test)]
mod e1_abi_tests;
#[cfg(test)]
mod plugin_tax_tests;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;
