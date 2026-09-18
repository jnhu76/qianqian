//! Host-side sanity check for the S-PROBE second-handle machinery:
//! probe each path given on argv (open → probe → close, no PCM reads)
//! and print one JSON line per file. No playback, no Windows — this
//! validates the FFI sequence and the fixture expectations on the host
//! before the physical runs. Exit 0 iff every probe matched
//! `--expect valid|invalid` (default: valid).

use std::path::PathBuf;

fn main() {
    let mut expect_valid = true;
    let mut ok_all = true;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--expect-invalid" => expect_valid = false,
            "--expect-valid" => expect_valid = true,
            path => {
                let record = f6_source_probe_check(PathBuf::from(path));
                // Reuse the harness's probe via a minimal local copy to
                // keep this bin standalone.
                let opened = std::panic::catch_unwind(|| record);
                match opened {
                    Ok(r) => {
                        let matched = r.success == expect_valid;
                        ok_all &= matched;
                        println!(
                            "{}",
                            serde_json::json!({
                                "path": path,
                                "success": r.success,
                                "matched": matched,
                                "facts": r.facts,
                                "error": r.error,
                            })
                        );
                    }
                    Err(_) => {
                        ok_all = false;
                        println!("{}", serde_json::json!({"path": path, "panic": true}));
                    }
                }
            }
        }
    }
    std::process::exit(if ok_all { 0 } else { 1 });
}

struct CheckRecord {
    success: bool,
    facts: Option<serde_json::Value>,
    error: Option<String>,
}

fn f6_source_probe_check(path: PathBuf) -> CheckRecord {
    match sprobe_probe(&path) {
        Ok(facts) => CheckRecord {
            success: true,
            facts: Some(serde_json::json!({
                "sample_rate": facts.sample_rate,
                "channels": facts.channels,
                "duration_us": facts.duration_us,
                "bits_per_sample": facts.bits_per_sample,
            })),
            error: None,
        },
        Err(e) => CheckRecord {
            success: false,
            facts: None,
            error: Some(e),
        },
    }
}

// The same open→probe→close sequence as the harness bin (kept literally
// in sync; the evidence contract lives in sprobe.rs).
mod ffi {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::raw::c_void;
    use std::slice;

    pub unsafe extern "C" fn file_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
        if ud.is_null() || dst.is_null() {
            return -1;
        }
        let file = unsafe { &mut *(ud as *mut File) };
        let buf = unsafe { slice::from_raw_parts_mut(dst, size) };
        match file.read(buf) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    pub unsafe extern "C" fn file_seek(ud: *mut c_void, offset: i64) -> i64 {
        if ud.is_null() || offset < 0 {
            return -1;
        }
        let file = unsafe { &mut *(ud as *mut File) };
        match file.seek(SeekFrom::Start(offset as u64)) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    pub unsafe extern "C" fn file_size(ud: *mut c_void) -> i64 {
        if ud.is_null() {
            return -1;
        }
        let file = unsafe { &mut *(ud as *mut File) };
        match file.metadata() {
            Ok(m) => m.len() as i64,
            Err(_) => -1,
        }
    }
}

fn sprobe_probe(path: &PathBuf) -> Result<ProbeFactsLite, String> {
    use qianqian_songcore_sys as sys;
    use std::os::raw::c_void;

    let loaded = unsafe { sys::songcore_abi_version() };
    if loaded != sys::SONGCORE_ABI_VERSION {
        return Err(format!("ABI mismatch: v{loaded}"));
    }
    let file: Box<std::fs::File> =
        Box::new(std::fs::File::open(path).map_err(|e| format!("open: {e}"))?);
    let io = sys::song_io {
        userdata: Box::into_raw(file) as *mut c_void,
        read: Some(ffi::file_read),
        seek: Some(ffi::file_seek),
        size: Some(ffi::file_size),
    };
    let mut handle: *mut sys::song_handle = std::ptr::null_mut();
    let status = unsafe { sys::song_open(&io, &mut handle) };
    if status != sys::SONG_OK {
        unsafe { drop(Box::from_raw(io.userdata as *mut std::fs::File)) };
        return Err(format!("song_open status {status}"));
    }
    if handle.is_null() {
        unsafe { drop(Box::from_raw(io.userdata as *mut std::fs::File)) };
        return Err("song_open null handle".into());
    }
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let status = unsafe { sys::song_probe(handle, &mut info) };
    let probe_status = status;
    // Close BEFORE reclaiming the file box: the callbacks borrow it.
    unsafe { sys::song_close(handle) };
    unsafe { drop(Box::from_raw(io.userdata as *mut std::fs::File)) };
    if probe_status != sys::SONG_OK {
        return Err(format!("song_probe status {probe_status}"));
    }
    Ok(ProbeFactsLite {
        sample_rate: info.sample_rate,
        channels: info.channels,
        duration_us: info.duration_us,
        bits_per_sample: info.bits_per_sample,
    })
}

struct ProbeFactsLite {
    sample_rate: i32,
    channels: i32,
    duration_us: i64,
    bits_per_sample: i32,
}
