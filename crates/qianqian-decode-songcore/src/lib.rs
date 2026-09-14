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

use qianqian_audio_api::ports::{
    DecodeError, DecodeOpenError, DecodeOutcome, DecodedPcmStream, PcmDecode, PcmDecodeCapability,
    PcmFormat,
};
use qianqian_composition::{ActivationError, ComponentSpec};
use qianqian_songcore_sys as sys;

/// Real decode mechanism over one SongCore native library instance.
/// Long-lived and stateless across opens.
pub struct SongcoreDecode {
    _private: (),
}

impl SongcoreDecode {
    /// Bind the mechanism fail-closed: the loaded native library must
    /// report the ABI version this crate was generated against.
    pub fn new() -> Result<Self, DecodeOpenError> {
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

// --- host-IO callbacks over std::fs::File --------------------------------
//
// FFI boundary discipline: every callback fails closed (-1) on a null
// userdata pointer, a null destination (regardless of the requested
// length: `from_raw_parts_mut` requires a non-null data pointer even for
// a zero-length slice), and a negative absolute offset (the ABI defines
// offsets as non-negative). The pointer preconditions for
// `from_raw_parts_mut` are checked before the slice is formed; the
// SongCore side never legitimately passes these values, so a guard hit is
// a caller bug, not a decode result.

unsafe extern "C" fn file_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    use std::io::Read;
    let file = unsafe { &mut *(ud as *mut File) };
    let buf = unsafe { slice::from_raw_parts_mut(dst, size) };
    match file.read(buf) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn file_seek(ud: *mut c_void, offset: i64) -> i64 {
    if ud.is_null() || offset < 0 {
        return -1;
    }
    use std::io::{Seek, SeekFrom};
    let file = unsafe { &mut *(ud as *mut File) };
    match file.seek(SeekFrom::Start(offset as u64)) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn file_size(ud: *mut c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut File) };
    match file.metadata() {
        Ok(m) => m.len() as i64,
        Err(_) => -1,
    }
}

fn status_open_error(path: &Path, status: u32) -> DecodeOpenError {
    DecodeOpenError {
        message: format!("SongCore refused '{}': status {status}", path.display()),
    }
}

/// One playback-specific decode endpoint: owns one native song handle and
/// the file it reads from. Released on drop (song_close), single-thread
/// serialized by contract (`DecodedPcmStream: Send` but not `Sync`).
pub struct SongcoreDecodeStream {
    handle: *mut sys::song_handle,
    format: PcmFormat,
    /// Kept alive for the handle's lifetime; the callbacks borrow it raw.
    /// Declared after `handle` so drop runs song_close while the file is
    /// still alive. Never read: its whole job is being alive, then dropped.
    #[allow(dead_code)]
    file: Box<File>,
}

// The native handle is externally serialized (SongCore contract): moving
// the endpoint to another thread is safe; concurrent calls are not made.
unsafe impl Send for SongcoreDecodeStream {}

impl SongcoreDecodeStream {
    fn open(path: &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
        let file = File::open(path)
            .map_err(|e| DecodeOpenError {
                message: format!("cannot open '{}': {e}", path.display()),
            })?
            .into();
        let io = sys::song_io {
            userdata: Box::into_raw(file) as *mut c_void,
            read: Some(file_read),
            seek: Some(file_seek),
            size: Some(file_size),
        };
        let mut handle: *mut sys::song_handle = std::ptr::null_mut();
        let status = unsafe { sys::song_open(&io, &mut handle) };
        if status != sys::SONG_OK {
            unsafe { drop(Box::from_raw(io.userdata as *mut File)) };
            return Err(status_open_error(path, status));
        }
        // Fail closed: the ABI does not spell out "SONG_OK implies a
        // non-null handle", so the wrapper refuses to build on one. A null
        // here would make every later handle call undefined behavior.
        if handle.is_null() {
            unsafe { drop(Box::from_raw(io.userdata as *mut File)) };
            return Err(DecodeOpenError {
                message: format!(
                    "SongCore opened '{}' with SONG_OK but a null handle",
                    path.display()
                ),
            });
        }
        let mut info: sys::song_info = unsafe { std::mem::zeroed() };
        let status = unsafe { sys::song_probe(handle, &mut info) };
        if status != sys::SONG_OK {
            unsafe {
                sys::song_close(handle);
                drop(Box::from_raw(io.userdata as *mut File));
            }
            return Err(DecodeOpenError {
                message: format!(
                    "SongCore probe failed for '{}': status {status}",
                    path.display()
                ),
            });
        }
        Ok(Box::new(SongcoreDecodeStream {
            handle,
            format: PcmFormat {
                sample_rate: info.sample_rate as u32,
                channels: info.channels as u16,
                channel_mask: info.channel_mask,
            },
            file: unsafe { Box::from_raw(io.userdata as *mut File) },
        }))
    }

    fn last_error(&self) -> String {
        let mut err: *const sys::song_error = std::ptr::null();
        let status = unsafe { sys::song_last_error(self.handle, &mut err) };
        if status == sys::SONG_OK && !err.is_null() {
            let e = unsafe { &*err };
            if !e.message.is_null() && e.message_len > 0 {
                let bytes = unsafe {
                    slice::from_raw_parts(e.message as *const u8, e.message_len as usize)
                };
                return String::from_utf8_lossy(bytes).into_owned();
            }
        }
        "SongCore decode failed".to_owned()
    }
}

impl DecodedPcmStream for SongcoreDecodeStream {
    fn format(&self) -> PcmFormat {
        self.format
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
        let status =
            unsafe { sys::song_read_pcm(self.handle, dst.as_mut_ptr(), capacity, &mut produced) };
        match status {
            sys::SONG_OK => Ok(DecodeOutcome::Frames(produced as usize)),
            sys::SONG_EOF => Ok(DecodeOutcome::Eof),
            _ => Err(DecodeError {
                message: self.last_error(),
            }),
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
    // Callback-guard regressions from NATIVE-BOUNDARY-AUDIT-0. These run
    // against the callback machinery only — no native library, no fixture.
    // They are the executable form of the guard contract: a host-IO
    // callback must fail closed (-1) without ever forming a Rust slice
    // from a pointer the slice rules forbid.
    use super::*;

    /// Holds a `Box<File>` leaked to a raw userdata pointer for the
    /// duration of one assertion, reclaiming it on drop.
    struct RawFile(*mut c_void);
    impl RawFile {
        fn from_temp() -> Self {
            let path = std::env::temp_dir().join("qianqian-callback-guard-test");
            let file: Box<File> = Box::new(File::create(path).unwrap());
            Self(Box::into_raw(file) as *mut c_void)
        }
        fn as_ud(&self) -> *mut c_void {
            self.0
        }
    }
    impl Drop for RawFile {
        fn drop(&mut self) {
            if !self.0.is_null() {
                drop(unsafe { Box::from_raw(self.0 as *mut File) });
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
}
