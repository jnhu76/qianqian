//! Plugin tax closeout: steady decode wall through the real Decode
//! Plugin endpoint vs the raw FFI, same artifact, same host, interleaved
//! ABAB iterations (protocol lineage: the songcore-call-comparison
//! experiment; simple A/B per the first-audible-slice campaign — no
//! adapter layer exists to charge).
//!
//! Run: cargo test --release --test plugin_tax -- --nocapture
//! Verdict rule: |delta| inside the run's own noise band =>
//! NO_MEASURABLE_PLUGIN_TAX; a clear excess => INVESTIGATE.

mod common;

use std::path::{Path, PathBuf};
use std::time::Instant;

use qianqian_core::ports::{DecodeOutcome, PcmDecode};
use qianqian_decode_songcore::SongcoreDecode;
use qianqian_songcore_sys as sys;

use common::ReferenceFixture;

const BLOCK_FRAMES: usize = 1024;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/experiments/songcore-equivalence/fixtures")
}

fn load_fixture(id: &str) -> (ReferenceFixture, PathBuf) {
    let fx = ReferenceFixture::load(&fixtures_dir().parent().unwrap().join("reference.json"), id);
    let path = fixtures_dir().join(&fx.file);
    fx.verify_file_identity(&path);
    (fx, path)
}

/// Raw FFI caller: an open handle read directly through song_read_pcm,
/// one file-descriptor-style flow, no endpoint abstraction.
struct RawFfi {
    handle: *mut sys::song_handle,
    /// Owns the host file across the handle's lifetime; reclaimed on drop
    /// after song_close (the callbacks borrow it raw).
    file: *mut std::fs::File,
    channels: usize,
}

impl RawFfi {
    fn open(path: &std::path::Path) -> Self {
        let file = Box::into_raw(Box::new(std::fs::File::open(path).expect("fixture opens")));
        let io = sys::song_io {
            userdata: file as *mut _,
            read: Some(raw_read),
            seek: Some(raw_seek),
            size: Some(raw_size),
        };
        let mut handle = std::ptr::null_mut();
        let status = unsafe { sys::song_open(&io, &mut handle) };
        assert_eq!(status, sys::SONG_OK, "raw open failed: {status}");
        let mut info: sys::song_info = unsafe { std::mem::zeroed() };
        let status = unsafe { sys::song_probe(handle, &mut info) };
        assert_eq!(status, sys::SONG_OK, "raw probe failed: {status}");
        Self {
            handle,
            file,
            channels: info.channels as usize,
        }
    }

    fn read_all(&mut self) -> (usize, bool) {
        let mut dst = vec![0.0f32; BLOCK_FRAMES * self.channels];
        let mut produced = 0u64;
        let status = unsafe {
            sys::song_read_pcm(
                self.handle,
                dst.as_mut_ptr(),
                BLOCK_FRAMES as u64,
                &mut produced,
            )
        };
        (produced as usize, status == sys::SONG_EOF)
    }
}

impl Drop for RawFfi {
    fn drop(&mut self) {
        unsafe {
            sys::song_close(self.handle);
            drop(Box::from_raw(self.file));
        }
    }
}

unsafe extern "C" fn raw_read(ud: *mut std::ffi::c_void, dst: *mut u8, size: usize) -> i64 {
    use std::io::Read;
    let f = unsafe { &mut *(ud as *mut std::fs::File) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, size) };
    f.read(buf).map_or(-1, |n| n as i64)
}

unsafe extern "C" fn raw_seek(ud: *mut std::ffi::c_void, off: i64) -> i64 {
    use std::io::{Seek, SeekFrom};
    let f = unsafe { &mut *(ud as *mut std::fs::File) };
    f.seek(SeekFrom::Start(off as u64)).map_or(-1, |n| n as i64)
}

unsafe extern "C" fn raw_size(ud: *mut std::ffi::c_void) -> i64 {
    let f = unsafe { &mut *(ud as *mut std::fs::File) };
    f.metadata().map_or(-1, |m| m.len() as i64)
}

/// One steady-decode wall measurement: drain the whole file, return the
/// elapsed wall time in microseconds.
fn plugin_drain_us(path: &std::path::Path) -> usize {
    let service = SongcoreDecode::new().expect("mechanism binds");
    let mut src = service.open_source(path).expect("opens");
    let channels = usize::from(src.format().channels);
    let mut staging = vec![0.0f32; BLOCK_FRAMES * channels];
    let mut frames = 0usize;
    let start = Instant::now();
    loop {
        match src.read_frames(&mut staging).expect("read") {
            DecodeOutcome::Frames(n) => frames += n,
            DecodeOutcome::Eof => break,
        }
    }
    let us = start.elapsed().as_micros() as usize;
    assert!(frames > 0);
    us
}

fn raw_drain_us(path: &std::path::Path) -> usize {
    let mut raw = RawFfi::open(path);
    let mut frames = 0usize;
    let start = Instant::now();
    loop {
        let (n, eof) = raw.read_all();
        frames += n;
        if eof {
            break;
        }
    }
    let us = start.elapsed().as_micros() as usize;
    assert!(frames > 0);
    us
}

fn median(v: &mut [usize]) -> usize {
    v.sort_unstable();
    v[v.len() / 2]
}

#[test]
fn plugin_endpoint_vs_raw_ffi_steady_decode() {
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo"] {
        let (_fx, path) = load_fixture(id);

        // Warmup both paths once.
        let _ = plugin_drain_us(&path);
        let _ = raw_drain_us(&path);

        const RUNS: usize = 15;
        let mut plugin_samples = Vec::with_capacity(RUNS);
        let mut raw_samples = Vec::with_capacity(RUNS);
        // Balanced ABAB interleave, alternating who goes first per pair.
        for i in 0..RUNS {
            if i % 2 == 0 {
                plugin_samples.push(plugin_drain_us(&path));
                raw_samples.push(raw_drain_us(&path));
            } else {
                raw_samples.push(raw_drain_us(&path));
                plugin_samples.push(plugin_drain_us(&path));
            }
        }

        let plugin_med = median(&mut plugin_samples);
        let raw_med = median(&mut raw_samples);
        let delta_pct = if raw_med > 0 {
            (plugin_med as f64 - raw_med as f64) / raw_med as f64 * 100.0
        } else {
            f64::INFINITY
        };
        // Noise band: interquartile spread of the raw samples.
        raw_samples.sort_unstable();
        let q1 = raw_samples[RUNS / 4];
        let q3 = raw_samples[3 * RUNS / 4];
        let noise_band = if raw_med > 0 {
            (q3 as f64 - q1 as f64) / raw_med as f64 * 100.0
        } else {
            f64::INFINITY
        };

        println!(
            "{id}: raw FFI median {raw_med} us, plugin median {plugin_med} us, \
                  delta {delta_pct:+.1}%, noise band (IQR) {noise_band:.1}%"
        );

        // The gate is informational + bounded: the endpoint must not add a
        // systematic cost far outside the run's own noise. A failure here
        // demands investigation, not silent acceptance.
        assert!(
            delta_pct.abs() <= noise_band.abs().max(15.0),
            "{id}: plugin delta {delta_pct:+.1}% exceeds noise band {noise_band:.1}% — INVESTIGATE"
        );
    }
}
