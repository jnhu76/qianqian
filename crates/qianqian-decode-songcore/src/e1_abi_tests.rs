//! E1-ABI — real SongCore latch calibration at the ABI boundary
//! (migration §7.4).
//!
//! Property authority: the ABI's partial-success / pending-fatal freeze
//! (`native/include/songcore.h`; reconciliation §2.1) — NOT a D11 claim.
//! This harness witnesses what the C core actually does across the real
//! ABI; the worker/session-level consequences are E1-WORKER's.
//!
//! Harness shape: the crate's test target links the real native library and
//! constructs `song_io` directly with test-owned callbacks that serve a
//! crafted byte vector from memory — the io-callback boundary the ABI
//! already defines, with no filesystem and no new C symbol. The healthy leg
//! additionally re-derives a committed corpus fixture's referenced PCM
//! identity (`reference.json`), so "the fixture and harness are sound" is
//! anchored to the same reference the production corpus tests use.
//!
//! Calibration discipline (migration §7.4): a healthy control proves only
//! the healthy path; the partial-success shape is established by the FAULT
//! configuration, never asserted beside it. Identification is by injection
//! record + behavior: the injection offsets are the harness's own record
//! and the diagnostic TEXT is corroboration only — never the identity.
//!
//! Reachability honesty: the `INVALID_ARGUMENT` refusal row below is a
//! harness-constructed refusal. It is NOT evidence that the production
//! adapter reaches `RefusedUnchanged` (that would forge reachability); the
//! production path only ever passes user-target durations through
//! `classify_seek_status`.

use std::os::raw::c_void;

use qianqian_songcore_sys as sys;

use crate::test_common as common;

/// The in-memory byte source behind the `song_io` callbacks, plus the
/// harness's own injection record.
struct MemFile {
    bytes: Vec<u8>,
    /// Byte ranges the harness deliberately disturbed (the injection
    /// record: the only legitimate fault identity).
    disturbed: Vec<(usize, usize)>,
    pos: u64,
    reads: u32,
    seeks: u32,
    /// E-3 family fault injection: fail this read/seek index outright.
    fail_read_at: Option<u32>,
    fail_seek_at: Option<u32>,
}

impl MemFile {
    fn new(bytes: Vec<u8>, disturbed: Vec<(usize, usize)>) -> Self {
        Self {
            bytes,
            disturbed,
            pos: 0,
            reads: 0,
            seeks: 0,
            fail_read_at: None,
            fail_seek_at: None,
        }
    }

}

unsafe extern "C" fn mem_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    let src = unsafe { &mut *(ud as *mut MemFile) };
    src.reads += 1;
    if src.fail_read_at == Some(src.reads) {
        return -1;
    }
    let remaining = src.bytes.len().saturating_sub(src.pos as usize);
    let take = remaining.min(size);
    if take == 0 {
        // Zero bytes is the ABI's source EOF (`songcore_ffmpeg.c` io_read
        // maps 0 to AVERROR_EOF) — deliberately NOT a stub failure.
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(src.bytes.as_ptr().add(src.pos as usize), dst, take);
    }
    src.pos += take as u64;
    take as i64
}

unsafe extern "C" fn mem_seek(ud: *mut c_void, offset: i64) -> i64 {
    if ud.is_null() || offset < 0 {
        return -1;
    }
    let src = unsafe { &mut *(ud as *mut MemFile) };
    src.seeks += 1;
    if src.fail_seek_at == Some(src.seeks) {
        return -1;
    }
    if offset as u64 > src.bytes.len() as u64 {
        return -1;
    }
    src.pos = offset as u64;
    src.pos as i64
}

unsafe extern "C" fn mem_size(ud: *mut c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let src = unsafe { &*(ud as *const MemFile) };
    src.bytes.len() as i64
}

/// One live ABI handle over an in-memory source.
struct AbiHandle {
    handle: *mut sys::song_handle,
    source: *mut MemFile,
    info: sys::song_info,
}

impl AbiHandle {
    /// Construct exactly the production open sequence: `song_open` over the
    /// test's `song_io` callbacks, then `song_probe`. `Err(status)` carries
    /// the raw status so a caller can distinguish open from probe refusal.
    fn open(bytes: Vec<u8>, disturbed: Vec<(usize, usize)>) -> Result<Self, (u32, String)> {
        let source = Box::into_raw(Box::new(MemFile::new(bytes, disturbed)));
        let io = sys::song_io {
            userdata: source as *mut c_void,
            read: Some(mem_read),
            seek: Some(mem_seek),
            size: Some(mem_size),
        };
        let mut handle: *mut sys::song_handle = std::ptr::null_mut();
        let status = unsafe { sys::song_open(&io, &mut handle) };
        if status != sys::SONG_OK {
            let me = unsafe { &*source };
            let text = format!("song_open status {status}; {} reads", me.reads);
            unsafe { drop(Box::from_raw(source)) };
            return Err((status, text));
        }
        let mut info: sys::song_info = unsafe { std::mem::zeroed() };
        let status = unsafe { sys::song_probe(handle, &mut info) };
        if status != sys::SONG_OK {
            unsafe {
                sys::song_close(handle);
                drop(Box::from_raw(source));
            }
            return Err((status, "song_probe refused".to_owned()));
        }
        Ok(Self {
            handle,
            source,
            info,
        })
    }

    fn channels(&self) -> usize {
        self.info.channels as usize
    }

    /// One `song_read_pcm` of `frames` capacity → (status, produced).
    fn read(&mut self, frames: u64) -> (u32, u64) {
        let channels = self.channels();
        let mut dst = vec![0.0f32; frames as usize * channels];
        let mut produced = 0u64;
        let status = unsafe {
            sys::song_read_pcm(self.handle, dst.as_mut_ptr(), frames, &mut produced)
        };
        (status, produced)
    }

    /// One `song_read_pcm` returning the produced samples too.
    fn read_samples(&mut self, frames: u64) -> (u32, Vec<f32>) {
        let channels = self.channels();
        let mut dst = vec![0.0f32; frames as usize * channels];
        let mut produced = 0u64;
        let status = unsafe {
            sys::song_read_pcm(self.handle, dst.as_mut_ptr(), frames, &mut produced)
        };
        dst.truncate(produced as usize * channels);
        (status, dst)
    }

    fn seek_us(&mut self, target_us: i64) -> (u32, i64) {
        let mut landing: i64 = -1;
        let status = unsafe { sys::song_seek(self.handle, target_us, &mut landing) };
        (status, landing)
    }

    /// `song_last_error` → (status, native_code, message).
    fn last_error(&mut self) -> (u32, i32, String) {
        let mut err: *const sys::song_error = std::ptr::null();
        let status = unsafe { sys::song_last_error(self.handle, &mut err) };
        if status != sys::SONG_OK || err.is_null() {
            return (status, 0, String::new());
        }
        let e = unsafe { &*err };
        let message = if e.message.is_null() || e.message_len == 0 {
            String::new()
        } else {
            let bytes =
                unsafe { std::slice::from_raw_parts(e.message as *const u8, e.message_len as usize) };
            String::from_utf8_lossy(bytes).into_owned()
        };
        (status, e.native_code, message)
    }

    fn source(&self) -> &MemFile {
        unsafe { &*self.source }
    }
}

impl Drop for AbiHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                sys::song_close(self.handle);
            }
            drop(Box::from_raw(self.source));
        }
    }
}

/// Disturb a byte range: the injection record of the fault configuration.
fn disturb(bytes: &[u8], offset: usize, len: usize) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let end = (offset + len).min(out.len());
    for byte in out[offset..end].iter_mut() {
        *byte = 0x00;
    }
    out
}

/// The pinned fault configuration (calibrated 2026-10-16 against the Linux
/// build of the pinned closure; see the E1-ABI evidence record): the ALAC
/// reference fixture with a 16-byte zeroed disturbance at 20% of its
/// length. Calibration observed 32 capacity-1024 reads of partial success
/// (32768 frames, SONG_OK throughout) followed by one fatal read.
const FAULT_FIXTURE: &str = "alac-16-44-stereo";
const FAULT_PERCENT: usize = 20;
const FAULT_LEN: usize = 16;

/// The calibration legs' read capacity (frames per `song_read_pcm`).
const CALIB_BLOCK: u64 = 1024;

struct FaultConfig {
    healthy: Vec<u8>,
    corrupted: Vec<u8>,
    disturbed: Vec<(usize, usize)>,
    /// The reference manifest entry of the healthy fixture.
    fx: common::ReferenceFixture,
}

fn fault_config() -> FaultConfig {
    let (fx, path) = common::load_fixture(FAULT_FIXTURE);
    let healthy = std::fs::read(&path).expect("fixture bytes");
    let offset = healthy.len() * FAULT_PERCENT / 100;
    let disturbed = vec![(offset, FAULT_LEN)];
    let corrupted = disturb(&healthy, offset, FAULT_LEN);
    FaultConfig {
        healthy,
        corrupted,
        disturbed,
        fx,
    }
}

/// Read until the first non-`SONG_OK` status (bounded), returning the
/// frames delivered beforehand and the terminal observation.
struct ReadUntilFatal {
    frames: u64,
    reads: u32,
    status: u32,
    native_code: i32,
    message: String,
}

fn read_until_fatal(handle: &mut AbiHandle, limit: u32) -> ReadUntilFatal {
    let mut frames = 0u64;
    let mut reads = 0u32;
    loop {
        reads += 1;
        assert!(reads <= limit, "no fatal within {limit} reads");
        let (status, produced) = handle.read(CALIB_BLOCK);
        if status != sys::SONG_OK {
            let (_, native_code, message) = handle.last_error();
            return ReadUntilFatal {
                frames,
                reads,
                status,
                native_code,
                message,
            };
        }
        frames += produced;
    }
}

/// Calibration leg (i) — the HEALTHY control. The same container/codec with
/// no disturbed region decodes to clean EOF with `SONG_OK` throughout, and
/// the decoded PCM reproduces the committed reference identity. This proves
/// the fixture and the callback harness are sound; it proves nothing about
/// the fault path.
#[test]
fn healthy_control_reproduces_the_reference_pcm_identity() {
    let config = fault_config();
    let mut handle = AbiHandle::open(config.healthy.clone(), Vec::new()).expect("healthy open");
    assert_eq!(handle.channels(), 2, "the fixture is stereo");
    let mut hasher = common::Sha256::new();
    let mut total = 0u64;
    loop {
        let (status, samples) = handle.read_samples(CALIB_BLOCK);
        if status == sys::SONG_EOF {
            assert!(samples.is_empty(), "EOF cannot carry frames");
            break;
        }
        assert_eq!(status, sys::SONG_OK, "healthy reads stay SONG_OK");
        assert!(!samples.is_empty(), "SONG_OK either carries frames or is EOF");
        for sample in &samples {
            hasher.update(&sample.to_le_bytes());
        }
        total += (samples.len() / 2) as u64;
    }
    assert_eq!(total, config.fx.pcm_frames as u64, "reference frame count");
    assert_eq!(
        hasher.hex(),
        config.fx.pcm_sha256,
        "healthy control PCM identity"
    );
    // The healthy terminal is EOF, and it is stable, not one-shot.
    let (status, _) = handle.read(CALIB_BLOCK);
    assert_eq!(status, sys::SONG_EOF, "clean EOF after the drain");
}

/// Calibration leg (ii) — the FAULT configuration, which is what actually
/// establishes the partial-success shape. Three independent observations of
/// the latch, none of them message-text identification:
///
///   1. the disturbance is the harness's own injection record;
///   2. reads BEFORE it deliver frames with `SONG_OK` (partial success
///      happened);
///   3. the read that consumes the disturbed region fails with a typed
///      status + native code, and a SECOND read with no newly consumed
///      bytes fails again with the SAME typed status + native code (the
///      fatal is latched in the handle, not re-derived from the stream).
///
/// The observed text is printed as corroboration only.
#[test]
fn fault_configuration_yields_partial_success_then_a_latched_fatal() {
    let config = fault_config();
    let mut handle =
        AbiHandle::open(config.corrupted.clone(), config.disturbed.clone()).expect("faulted open");
    let fatal = read_until_fatal(&mut handle, 200);
    println!(
        "E1-ABI calibration: fixture={} bytes={} disturbed={:?} frames_before_fatal={} reads={} \
         fatal_status={} native_code={} message={:?}",
        FAULT_FIXTURE,
        config.corrupted.len(),
        config.disturbed,
        fatal.frames,
        fatal.reads,
        fatal.status,
        fatal.native_code,
        fatal.message
    );
    assert!(
        fatal.frames > 0,
        "the fault shape is PARTIAL success: frames must be delivered before the fatal"
    );
    assert_ne!(fatal.status, sys::SONG_EOF, "a fatal is not a source EOF");
    let (_, native_first, text_first) = handle.last_error();
    // Second observation, with no new disturbed bytes consumed between the
    // two reads: the same typed evidence must come back.
    let (status_again, produced_again) = handle.read(CALIB_BLOCK);
    let (_, native_second, text_second) = handle.last_error();
    assert_eq!(produced_again, 0, "a latched handle produces nothing");
    assert_eq!(
        status_again, fatal.status,
        "the latched fatal re-publishes its typed status"
    );
    assert_eq!(
        native_second, native_first,
        "the latched fatal re-publishes the original native code"
    );
    // Corroboration only — never the fault identity.
    println!(
        "E1-ABI latch re-read: status={} native={} first_text={:?} second_text={:?}",
        status_again, native_second, text_first, text_second
    );
}

/// The Applied row: a seek PAST the disturbed region lands, and its success
/// ERASES the latched fatal — subsequent reads deliver healthy frames whose
/// bytes match the healthy twin's continuation bit-exactly.
#[test]
fn applied_seek_erases_the_latch_and_matches_the_healthy_twin() {
    let config = fault_config();
    let mut faulted =
        AbiHandle::open(config.corrupted.clone(), config.disturbed.clone()).expect("faulted open");
    let fatal = read_until_fatal(&mut faulted, 200);
    assert!(fatal.frames > 0, "partial success before the fatal");
    // Seek past the region (the healthy twin's own declared duration is the
    // container's; 1500 ms sits well past the disturbed 20%).
    let target_us: i64 = 1_500_000;
    let (status, landing) = faulted.seek_us(target_us);
    assert_eq!(status, sys::SONG_OK, "the Applied seek succeeds");
    assert!(landing >= 0, "the landing is reported");
    // The twin: the same bytes minus the disturbance, drained and seeked
    // identically (the pre-cut reads are identical because the disturbance
    // is downstream of them).
    // The twin IS the healthy byte vector: restoring the disturbed range
    // from the reference fixture reproduces it exactly.
    let mut twin = AbiHandle::open(config.healthy.clone(), Vec::new()).expect("twin open");
    let mut drained = 0u64;
    while drained < fatal.frames {
        let (status, produced) = twin.read(CALIB_BLOCK);
        assert_eq!(status, sys::SONG_OK);
        drained += produced;
    }
    let (twin_status, twin_landing) = twin.seek_us(target_us);
    assert_eq!(twin_status, sys::SONG_OK, "the twin seek succeeds");
    // Bit-exact continuation over a bounded budget.
    let mut compared_frames = 0u64;
    for _ in 0..8 {
        let (fault_status, fault_samples) = faulted.read_samples(CALIB_BLOCK);
        let (twin_status, twin_samples) = twin.read_samples(CALIB_BLOCK);
        assert_eq!(fault_status, sys::SONG_OK, "post-cut reads are healthy");
        assert_eq!(twin_status, sys::SONG_OK);
        assert_eq!(
            fault_samples, twin_samples,
            "post-cut continuation must be bit-exact against the healthy twin"
        );
        assert!(!fault_samples.is_empty());
        compared_frames += (fault_samples.len() / 2) as u64;
    }
    println!(
        "E1-ABI applied-seek row: fatal_before={} frames, landing={landing} (twin {twin_landing}), bit-exact frames compared={compared_frames}",
        fatal.frames
    );
}

/// The refusal row: a seek that is refused BEFORE any mutation
/// (`INVALID_ARGUMENT`, the ABI's pure parameter check) retains the latch —
/// the next read re-publishes the same typed evidence.
///
/// Reachability honesty: this row is constructed by the harness. It is NOT
/// evidence that the production adapter reaches `RefusedUnchanged`; the
/// production path only ever forwards user-target durations.
#[test]
fn refused_seek_retains_the_latch() {
    let config = fault_config();
    let mut handle =
        AbiHandle::open(config.corrupted.clone(), config.disturbed.clone()).expect("faulted open");
    let fatal = read_until_fatal(&mut handle, 200);
    assert!(fatal.frames > 0);
    let (refusal, _) = handle.seek_us(-1);
    assert_eq!(
        refusal,
        sys::SONG_ERR_INVALID_ARGUMENT,
        "a negative target is the pure parameter refusal"
    );
    let (status, produced) = handle.read(CALIB_BLOCK);
    assert_eq!(produced, 0);
    assert_eq!(
        status, fatal.status,
        "the pre-mutation refusal must not disturb the latched fatal"
    );
}

/// The destructive row: a seek whose LANDING decode meets the disturbed
/// region mutates first (`av_seek_frame` + flush + `reset_decode_state`)
/// and then fails, so the old continuation is unprovable and the seek
/// reports the post-mutation failure class rather than a refusal.
///
/// Pinned configuration (calibrated): targets in [745 ms, 800 ms] fail;
/// 750 ms is pinned inside that plateau. Recorded limitation: the fresh
/// failure the following read derives carries the SAME typed status and
/// native code as the pre-seek latch (both are the same disturbed region),
/// so "the old latch was cleared" is not distinguishable from "the same
/// latch continued" by codes alone; what IS witnessed is the destructive
/// seek's own failure class and the absence of resumed production.
#[test]
fn destructive_seek_failure_is_a_post_mutation_failure() {
    let config = fault_config();
    let mut handle =
        AbiHandle::open(config.corrupted.clone(), config.disturbed.clone()).expect("faulted open");
    let fatal = read_until_fatal(&mut handle, 200);
    assert!(fatal.frames > 0, "partial success before the fatal");
    let target_us: i64 = 750_000;
    let (status, landing) = handle.seek_us(target_us);
    println!(
        "E1-ABI destructive-seek row: target_us={target_us} status={status} landing={landing}"
    );
    assert_eq!(
        status,
        sys::SONG_ERR_SEEK_ERROR,
        "the seek's landing decode failed: a destructive failure, not a refusal"
    );
    assert_ne!(
        status,
        sys::SONG_ERR_INVALID_ARGUMENT,
        "a post-mutation failure must never be reported as the pre-mutation refusal"
    );
    assert_eq!(landing, -1, "no landing was reached");
    let (after_status, after_produced) = handle.read(CALIB_BLOCK);
    println!(
        "E1-ABI after destructive seek: status={after_status} produced={after_produced} native={}",
        handle.last_error().1
    );
    assert_eq!(
        after_produced, 0,
        "a destructive seek failure never resumes the old continuation"
    );
    assert_ne!(
        after_status,
        sys::SONG_OK,
        "the handle stays failed after the destructive seek"
    );
}

/// Teardown: `song_close` after a latched fatal is a clean release (no
/// panic, no use-after-free — the adapter's Drop path).
#[test]
fn close_after_a_latched_fatal_is_a_clean_teardown() {
    let config = fault_config();
    let mut handle =
        AbiHandle::open(config.corrupted.clone(), config.disturbed.clone()).expect("faulted open");
    let fatal = read_until_fatal(&mut handle, 200);
    assert!(fatal.frames > 0);
    drop(handle); // song_close
}
