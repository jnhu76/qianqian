//! binding-parity-runner — #172 S2 / G3 (BINDING_RUNTIME) binding-parity
//! runner for the windows-x86_64 release lane.
//!
//! The direct C consumer's parity suite (evidence/songcore-172/suite/
//! csuite_consumer.c, receipts under evidence/songcore-172/suite-results/)
//! is the CORE_RUNTIME reference. This runner executes THE SAME scenario
//! (S-BIND §7.1 steps 01–13, same modes, same predicates, same frozen
//! constructions) through the existing Rust binding path —
//! qianqian-songcore-sys (raw) plus qianqian-decode-songcore (adapter,
//! opened through its kernel-published PcmDecode capability) — against the
//! SAME static artifact (native/build/artifacts/libsongcore.a) and the
//! SAME media (canonical corpus fixtures + the frozen #296/#324
//! malformed constructions).
//!
//! Reference discipline (S-BIND §7.3 / release contract §11 G2–G3): NO
//! second corpus, NO second reference numbers, NO invented tolerance.
//! Expected values are read at run time from
//!   native/experiments/songcore-equivalence/reference.json
//!   native/experiments/songcore-equivalence/results/songcore-equivalence.json
//! and the C-vs-binding comparison is made against the RECORDED C-suite
//! receipts (summary + per-case logs), not re-derived. The frozen PCM
//! comparison policy is the corpus's own: exact SHA-256 of the full-drain
//! Float32 PCM for ALL fixtures (lossy included); no numeric lossy
//! tolerance exists and none is invented; for lossless codecs the
//! post-seek drain must byte-match the sequential slice at the landing
//! frame (songcore.h: "for lossless formats the landing is
//! frame-accurate").
//!
//! Lanes:
//!   raw      — csuite_consumer.c ported 1:1 onto qianqian-songcore-sys:
//!              parity / readfault / seekfault / truncated / garbage /
//!              argcheck / closecheck, same case names, same INFO lines.
//!   adapter  — the same parity shape at the adapter seam (open via the
//!              kernel-resolved PcmDecode capability, format/duration
//!              facts, full-drain identity, EOF stability, seek 25/50/75
//!              with the three-class outcome, drop/reopen identity,
//!              malformed open errors), plus probe_media facts.
//!   compare  — machine comparison of this run's observable INFO events
//!              and CASE results against the recorded C-suite receipts,
//!              field by field, plus adapter-vs-C field comparisons.
//!
//! Output: evidence/songcore-172/binding-parity/<stamp>/ with per-mode
//! receipts, adapter.log, comparison.log and one machine-readable
//! summary.json (schema songcore-binding-parity-summary/1). Exit 0 iff
//! every case passed; preflight aborts nonzero (10/11/12) like the C
//! runner; adapter kernel setup failure aborts 15.

use std::cell::RefCell;
use std::ffi::c_void;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{
    DecodeOutcome, PcmDecode, PcmDecodeCapability, ProviderSeekOutcome,
};
use qianqian_composition::{ActivationError, ComponentSpec, DesiredEntry, Revision};
use qianqian_decode_songcore::{probe_media, songcore_decode_plugin};
use qianqian_songcore_sys as sys;

mod layout_probe;

// --- frozen identities (identical to ops/run-csuite.ps1 defaults and the
// --- recorded C-suite receipts this run must match) ----------------------

const FROZEN_ARTIFACT_SHA256: &str =
    "1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402";
const FROZEN_HEADER_SHA256: &str =
    "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536";
const FROZEN_TRUNCATED_SHA256: &str =
    "0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f";
const FROZEN_SEEKFAULT_DIAGNOSTIC: &str = "could not reach a landing point";
const AVERROR_INVALIDDATA: i32 = -1094995529;

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn ror(x: u32, n: u32) -> u32 {
    (x >> n) | (x << (32 - n))
}

pub struct Sha256 {
    h: [u32; 8],
    len: u64,
    buf: [u8; 64],
    buf_len: usize,
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            len: 0,
            buf: [0; 64],
            buf_len: 0,
        }
    }

    fn compress(&mut self, p: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = ((p[4 * i] as u32) << 24)
                | ((p[4 * i + 1] as u32) << 16)
                | ((p[4 * i + 2] as u32) << 8)
                | (p[4 * i + 3] as u32);
        }
        for i in 16..64 {
            let s0 = ror(w[i - 15], 7) ^ ror(w[i - 15], 18) ^ (w[i - 15] >> 3);
            let s1 = ror(w[i - 2], 17) ^ ror(w[i - 2], 19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = ror(e, 6) ^ ror(e, 11) ^ ror(e, 25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[i])
                .wrapping_add(w[i]);
            let s0 = ror(a, 2) ^ ror(a, 13) ^ ror(a, 22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        while !data.is_empty() {
            if self.buf_len == 0 && data.len() >= 64 {
                let (block, rest) = data.split_at(64);
                self.compress(block);
                data = rest;
            } else {
                let take = (64 - self.buf_len).min(data.len());
                self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
                self.buf_len += take;
                data = &data[take..];
                if self.buf_len == 64 {
                    let block = self.buf;
                    self.compress(&block);
                    self.buf_len = 0;
                }
            }
        }
    }

    pub fn hex(mut self) -> String {
        let bit_len = self.len.wrapping_mul(8);
        let mut padded = self.buf[..self.buf_len].to_vec();
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&bit_len.to_be_bytes());
        for block in padded.chunks(64) {
            self.compress(block);
        }
        let mut s = String::with_capacity(64);
        for v in self.h {
            s.push_str(&format!("{v:08x}"));
        }
        s
    }
}

fn sha256_bytes(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.hex()
}

/// Fail-closed preflight: the hasher must reproduce the NIST FIPS 180-4
/// test vectors before any PCM identity is compared with it.
fn sha256_self_test() {
    let vectors: [(&[u8], &str); 4] = [
        (
            b"",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ];
    for (message, expected) in vectors {
        let mut h = Sha256::new();
        h.update(message);
        let got = h.hex();
        assert!(got == expected, "SHA-256 self-test failed: got {got}, want {expected}");
    }
}

fn sha256_file(path: &Path) -> String {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("sha256_file: {}: {e}", path.display()));
    sha256_bytes(&bytes)
}

// --- corpus / frozen fact loading ----------------------------------------

struct Fixture {
    id: String,
    file: String,
    path: PathBuf,
    codec: String,
    container: String,
    duration_us: i64,
    sample_rate: i64,
    channels: i64,
    pcm_frames: u64,
    pcm_sha256: String,
    lossless: bool,
}

fn find_project_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("native/experiments/songcore-equivalence/reference.json").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("project root with native/experiments/songcore-equivalence not found upward from {}", env!("CARGO_MANIFEST_DIR"));
        }
    }
}

fn load_json(path: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_fixtures(root: &Path) -> Vec<Fixture> {
    let reference = load_json(
        &root.join("native/experiments/songcore-equivalence/reference.json"),
    );
    let equivalence = load_json(
        &root.join("native/experiments/songcore-equivalence/results/songcore-equivalence.json"),
    );
    let corpus_dir = root.join("native/experiments/songcore-equivalence/fixtures");
    let mut out = Vec::new();
    for fx in reference["fixtures"].as_array().expect("fixtures array") {
        let id = fx["id"].as_str().expect("id").to_string();
        let file = fx["file"].as_str().expect("file").to_string();
        let path = corpus_dir.join(&file);
        let observed = &equivalence["correctness"][&id]["observed"];
        if !observed.is_object() {
            panic!("no frozen Linux direct-C observation for fixture {id}");
        }
        let codec = fx["codec"].as_str().expect("codec").to_string();
        let fixture = Fixture {
            id: id.clone(),
            file,
            path: path.clone(),
            codec: codec.clone(),
            container: observed["container"].as_str().expect("container").to_string(),
            duration_us: observed["duration_us"].as_i64().expect("duration_us"),
            sample_rate: fx["sample_rate"].as_i64().expect("sample_rate"),
            channels: fx["channels"].as_i64().expect("channels"),
            pcm_frames: fx["pcm_frames"].as_u64().expect("pcm_frames"),
            pcm_sha256: fx["pcm_sha256"].as_str().expect("pcm_sha256").to_string(),
            lossless: codec == "flac" || codec == "alac",
        };
        // Corpus identity discipline: the fixture file itself must match the
        // reference manifest before any decode assertion runs.
        let sha = sha256_file(&path);
        let want = fx["fixture_sha256"].as_str().expect("fixture_sha256");
        if sha != want {
            panic!("fixture identity drift: {id} got {sha} want {want}");
        }
        out.push(fixture);
    }
    out
}

// --- per-mode case recorder (the C child's output contract) --------------

struct Recorder {
    mode: String,
    lines: Vec<String>,
    cases: usize,
    failed: usize,
    /// Ordered observable INFO events: (kind, remainder) — kind is the
    /// first token after "INFO " (e.g. "probe", "drain", "seek"); META
    /// lines are recorded as kind "META" with the whole line as remainder.
    info: Vec<(String, String)>,
    /// Ordered CASE results: (name, passed).
    case_list: Vec<(String, bool)>,
    mode_result: Option<(usize, usize, bool)>,
}

impl Recorder {
    fn new(mode: &str) -> Self {
        Recorder {
            mode: mode.to_string(),
            lines: Vec::new(),
            cases: 0,
            failed: 0,
            info: Vec::new(),
            case_list: Vec::new(),
            mode_result: None,
        }
    }

    fn print(&mut self, line: String) {
        println!("{line}");
        self.lines.push(line);
    }

    fn case_pass(&mut self, name: &str) {
        self.cases += 1;
        self.case_list.push((name.to_string(), true));
        self.print(format!("CASE {name} PASS"));
    }

    fn case_fail(&mut self, name: &str, detail: &str) {
        self.cases += 1;
        self.failed += 1;
        self.case_list.push((name.to_string(), false));
        self.print(format!("CASE {name} FAIL {detail}"));
    }

    /// Record one observable INFO event and print it in the C child's
    /// exact line format: "INFO <kind> <remainder>".
    fn info(&mut self, kind: &str, remainder: &str) {
        self.info.push((kind.to_string(), remainder.to_string()));
        self.print(format!("INFO {kind} {remainder}"));
    }

    /// META[i] raw-enumeration observation line (C prints the same shape).
    fn meta_line(&mut self, line: String) {
        self.info.push(("META".to_string(), line.clone()));
        self.print(line);
    }

    fn finish(&mut self) -> bool {
        let pass = self.failed == 0;
        self.mode_result = Some((self.cases, self.failed, pass));
        self.print(format!(
            "MODE {} CASES={} FAILED={} RESULT={}",
            self.mode,
            self.cases,
            self.failed,
            if pass { "PASS" } else { "FAIL" }
        ));
        pass
    }
}

fn save_log(dir: &Path, name: &str, rec: &Recorder) -> PathBuf {
    let path = dir.join(format!("{name}.log"));
    std::fs::write(&path, rec.lines.join("\n") + "\n")
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    path
}

// --- raw-binding host IO (production shape: fail-closed file callbacks) --

unsafe extern "C" fn host_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, size) };
    match file.read(buf) {
        Ok(0) => 0,          // source EOF, like the C child's feof branch
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn host_seek(ud: *mut c_void, absolute_offset: i64) -> i64 {
    if ud.is_null() || absolute_offset < 0 {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    file.seek(SeekFrom::Start(absolute_offset as u64)).map(|p| p as i64).unwrap_or(-1)
}

unsafe extern "C" fn host_size(ud: *mut c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    file.metadata().map(|m| m.len() as i64).unwrap_or(-1)
}

/// One raw ABI handle over an open host file. Mirrors the C child's
/// open/probe ordering: `open` does ONLY song_open; probing is a separate,
/// separately-asserted step.
struct RawHandle {
    handle: *mut sys::song_handle,
    file: *mut std::fs::File,
}

impl RawHandle {
    fn open(path: &Path) -> Result<RawHandle, u32> {
        let file = Box::into_raw(Box::new(
            std::fs::File::open(path).map_err(|_| u32::MAX)?,
        ));
        let io = sys::song_io {
            userdata: file as *mut c_void,
            read: Some(host_read),
            seek: Some(host_seek),
            size: Some(host_size),
        };
        let mut handle: *mut sys::song_handle = std::ptr::null_mut();
        let status = unsafe { sys::song_open(&io, &mut handle) };
        if status != sys::SONG_OK {
            unsafe { drop(Box::from_raw(file)) };
            return Err(status);
        }
        Ok(RawHandle { handle, file })
    }

    fn probe(&self, info: &mut sys::song_info) -> u32 {
        unsafe { sys::song_probe(self.handle, info) }
    }

    fn read(&self, dst: &mut [f32], frames: u64) -> (u32, u64) {
        let mut produced = 0u64;
        let status = unsafe {
            sys::song_read_pcm(self.handle, dst.as_mut_ptr(), frames, &mut produced)
        };
        (status, produced)
    }

    fn seek(&self, target_us: i64, landing: &mut i64) -> u32 {
        unsafe { sys::song_seek(self.handle, target_us, landing) }
    }

    fn diag(&self) -> (u32, i32, String) {
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
}

impl Drop for RawHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                sys::song_close(self.handle);
            }
            drop(Box::from_raw(self.file));
        }
    }
}

fn cstr_field(field: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = field
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn status_is_typed_error(st: u32) -> bool {
    (100..=113).contains(&st)
}

/// Drain to the terminal, accumulating PCM at dst — the C child's `drain`
/// contract 1:1 (scratch-block discipline, SONG_OK must carry frames in
/// (0, block], EOF carries zero, typed-error range check).
fn drain(
    h: &RawHandle,
    dst: &mut [f32],
    block: u64,
    channels: u64,
    max_frames: u64,
) -> (u64, u32, bool, bool) {
    let mut total = 0u64;
    let mut saw_zero = false;
    let mut bad_status = false;
    let mut scratch = vec![0.0f32; (block * channels) as usize];
    loop {
        let (st, got) = h.read(&mut scratch, block);
        if st == sys::SONG_EOF {
            if got != 0 {
                bad_status = true;
            }
            return (total, st, saw_zero, bad_status);
        }
        if st == sys::SONG_OK {
            if got == 0 || got > block {
                saw_zero = true;
                return (total, st, saw_zero, bad_status);
            }
            if (total) < max_frames {
                let room = max_frames - total;
                let take = got.min(room);
                dst[total as usize * channels as usize..]
                    [..take as usize * channels as usize]
                    .copy_from_slice(&scratch[..take as usize * channels as usize]);
            }
            total += got;
            continue;
        }
        if !status_is_typed_error(st) {
            bad_status = true;
        }
        return (total, st, saw_zero, bad_status);
    }
}

// --- raw modes (1:1 port of csuite_consumer.c) ---------------------------

fn raw_parity(fx: &Fixture) -> Recorder {
    let mut r = Recorder::new("parity");
    let exp_frames = fx.pcm_frames;
    let exp_channels = fx.channels as u64;

    // step 01: ABI version (through the linked artifact)
    let abi = unsafe { sys::songcore_abi_version() };
    if abi == sys::SONGCORE_ABI_VERSION && abi == 1 {
        r.case_pass("s01.abi-version");
    } else {
        r.case_fail("s01.abi-version", &format!("abi={abi} want={}", sys::SONGCORE_ABI_VERSION));
    }

    // step 02: open via host I/O callbacks
    let h = match RawHandle::open(&fx.path) {
        Ok(h) => {
            r.case_pass("s02.open");
            h
        }
        Err(status) => {
            r.case_fail("s02.open", &format!("status={status}"));
            r.finish();
            return r;
        }
    };

    // diagnostic contract: NULL message while the last call succeeded
    let (dst_status, _, message) = h.diag();
    if dst_status == sys::SONG_OK && message.is_empty() {
        r.case_pass("s02.diag-null-after-ok");
    } else {
        r.case_fail("s02.diag-null-after-ok", "message not NULL after open");
    }

    // pre-probe state: probed-handle operations → NOT_OPEN (102)
    {
        let mut cnt = 0u32;
        let s1 = unsafe { sys::song_audio_stream_count(h.handle, &mut cnt) };
        let mut tmp = vec![0.0f32; 64];
        let (s2, _) = h.read(&mut tmp, 64);
        if s1 == sys::SONG_ERR_NOT_OPEN && s2 == sys::SONG_ERR_NOT_OPEN {
            r.case_pass("s03.not-open-class");
        } else {
            r.case_fail(
                "s03.not-open-class",
                &format!("count={s1} read={s2} want both {}", sys::SONG_ERR_NOT_OPEN),
            );
        }
    }

    // step 03: probe → frozen source facts
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let st = h.probe(&mut info);
    if st != sys::SONG_OK {
        r.case_fail("s03.probe-facts", &format!("status={st}"));
        r.finish();
        return r;
    }
    let codec = cstr_field(&info.codec);
    let container = cstr_field(&info.container);
    let facts_ok = info.sample_rate as i64 == fx.sample_rate
        && info.channels as i64 == fx.channels
        && info.duration_us == fx.duration_us
        && codec == fx.codec
        && container == fx.container
        && info.sample_rate > 0
        && info.channels > 0;
    if facts_ok {
        r.case_pass("s03.probe-facts");
    } else {
        r.case_fail(
            "s03.probe-facts",
            &format!(
                "rate={}/{} ch={}/{} dur={}/{} codec={}/{} container={}/{}",
                info.sample_rate, fx.sample_rate,
                info.channels, fx.channels,
                info.duration_us, fx.duration_us,
                codec, fx.codec, container, fx.container
            ),
        );
    }
    r.info(
        "probe",
        &format!(
            "rate={} channels={} mask=0x{:x} duration={} bps={} codec={} container={} streams={} selected={}",
            info.sample_rate,
            info.channels,
            info.channel_mask,
            info.duration_us,
            info.bits_per_sample,
            codec,
            container,
            info.audio_stream_count,
            info.selected_audio_index
        ),
    );

    // probe idempotence: cached snapshot (byte-equal, like the C memcmp)
    {
        let mut info2: sys::song_info = unsafe { std::mem::zeroed() };
        let s2 = h.probe(&mut info2);
        let a = unsafe {
            std::slice::from_raw_parts(&info as *const _ as *const u8, std::mem::size_of::<sys::song_info>())
        };
        let b = unsafe {
            std::slice::from_raw_parts(&info2 as *const _ as *const u8, std::mem::size_of::<sys::song_info>())
        };
        if s2 == sys::SONG_OK && a == b {
            r.case_pass("s03.probe-idempotent");
        } else {
            r.case_fail("s03.probe-idempotent", "second probe differs or failed");
        }
    }

    // step 04: audio-stream enumeration + default stream identity
    {
        let mut count = 0u32;
        let s1 = unsafe { sys::song_audio_stream_count(h.handle, &mut count) };
        let mut ok = s1 == sys::SONG_OK && count >= 1;
        if ok {
            for i in 0..count {
                let mut si: sys::song_stream_info = unsafe { std::mem::zeroed() };
                if unsafe { sys::song_audio_stream_info(h.handle, i, &mut si) } != sys::SONG_OK {
                    ok = false;
                    break;
                }
                if si.audio_index != i {
                    ok = false;
                    break;
                }
                if si.is_default != 0 && si.is_default != 1 {
                    ok = false;
                    break;
                }
                if i == info.selected_audio_index {
                    ok = si.sample_rate == info.sample_rate
                        && si.channels == info.channels
                        && cstr_field(&si.codec) == codec;
                }
            }
        }
        let mut si_bad: sys::song_stream_info = unsafe { std::mem::zeroed() };
        let s_bad = unsafe { sys::song_audio_stream_info(h.handle, count, &mut si_bad) };
        if ok && s_bad == sys::SONG_ERR_INVALID_ARGUMENT {
            r.case_pass("s04.streams-default");
        } else {
            r.case_fail(
                "s04.streams-default",
                &format!("count_status={s1} bad_idx={s_bad}"),
            );
        }
        r.info("streams", &format!("count={count}"));
    }

    // step 05: metadata canonical + raw enumeration
    let mut meta_ptr: *const sys::song_metadata = std::ptr::null();
    let s1 = unsafe { sys::song_get_metadata(h.handle, &mut meta_ptr) };
    let mut raw_count = 0u32;
    let s2 = unsafe { sys::song_get_metadata_count(h.handle, &mut raw_count) };
    let mut ok = s1 == sys::SONG_OK && !meta_ptr.is_null() && s2 == sys::SONG_OK;
    let mut scopes_ok = true;
    let mut keys_ok = true;
    let (has_title, has_artist, has_album) = if ok {
        let meta = unsafe { &*meta_ptr };
        if meta.has_title != 0 && meta.has_title != 1 {
            scopes_ok = false;
        }
        if meta.has_artist != 0 && meta.has_artist != 1 {
            scopes_ok = false;
        }
        if meta.has_album != 0 && meta.has_album != 1 {
            scopes_ok = false;
        }
        (meta.has_title, meta.has_artist, meta.has_album)
    } else {
        (0, 0, 0)
    };
    if ok {
        for i in 0..raw_count {
            let mut e: sys::song_metadata_entry = unsafe { std::mem::zeroed() };
            if unsafe { sys::song_get_metadata_entry(h.handle, i, &mut e) } != sys::SONG_OK {
                keys_ok = false;
                break;
            }
            if e.scope != sys::SONG_METADATA_SCOPE_CONTAINER
                && e.scope != sys::SONG_METADATA_SCOPE_STREAM
            {
                scopes_ok = false;
            }
            if e.key.is_null() || e.key_len == 0 || e.value.is_null() {
                keys_ok = false;
            }
        }
    }
    if ok && scopes_ok && keys_ok {
        r.case_pass("s05.metadata");
    } else {
        r.case_fail(
            "s05.metadata",
            &format!("meta={s1} raw={raw_count} scopes={scopes_ok} keys={keys_ok}"),
        );
    }
    r.info(
        "metadata",
        &format!("raw_count={raw_count} has_title={has_title} has_artist={has_artist} has_album={has_album}"),
    );
    for i in 0..raw_count.min(16) {
        let mut e: sys::song_metadata_entry = unsafe { std::mem::zeroed() };
        if unsafe { sys::song_get_metadata_entry(h.handle, i, &mut e) } == sys::SONG_OK {
            let key = if e.key.is_null() {
                String::new()
            } else {
                let bytes = unsafe {
                    std::slice::from_raw_parts(e.key as *const u8, e.key_len as usize)
                };
                String::from_utf8_lossy(bytes).into_owned()
            };
            let value_len = e.value_len.min(64) as usize;
            let value = if e.value.is_null() {
                String::new()
            } else {
                let bytes =
                    unsafe { std::slice::from_raw_parts(e.value as *const u8, value_len) };
                String::from_utf8_lossy(bytes).into_owned()
            };
            r.meta_line(format!("META[{i}] scope={} key={key} value={value}", e.scope));
        }
    }

    // step 06: artwork (structural; corpus fixtures provide none)
    {
        let mut aw = 0u32;
        let s = unsafe { sys::song_get_artwork_count(h.handle, &mut aw) };
        let mut ok = s == sys::SONG_OK;
        for i in 0..aw {
            if !ok {
                break;
            }
            let mut ai: sys::song_artwork_item = unsafe { std::mem::zeroed() };
            if unsafe { sys::song_get_artwork_item(h.handle, i, &mut ai) } != sys::SONG_OK {
                ok = false;
                break;
            }
            if ai.role > sys::SONG_ARTWORK_OTHER {
                ok = false;
                break;
            }
            if ai.data_len > 0 && ai.data.is_null() {
                ok = false;
                break;
            }
        }
        if ok {
            r.case_pass("s06.artwork");
        } else {
            r.case_fail("s06.artwork", &format!("status={s}"));
        }
        r.info("artwork", &format!("count={aw}"));
    }

    // step 07: full drain → frozen frame count + PCM SHA-256
    let block = 4096u64;
    let mut full = vec![0.0f32; (exp_frames * exp_channels) as usize];
    let (total, term, bad, sha) = {
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut scratch = vec![0.0f32; (block * exp_channels) as usize];
        let mut term = sys::SONG_OK;
        let mut bad = false;
        loop {
            let (st, got) = h.read(&mut scratch, block);
            if st == sys::SONG_EOF {
                term = st;
                if got != 0 {
                    bad = true;
                }
                break;
            }
            if st == sys::SONG_OK {
                if got == 0 || got > block {
                    bad = true;
                    term = st;
                    break;
                }
                let slice = &scratch[..got as usize * exp_channels as usize];
                hasher.update(byteme(slice));
                if total < exp_frames {
                    let room = exp_frames - total;
                    let take = got.min(room);
                    full[total as usize * exp_channels as usize..]
                        [..take as usize * exp_channels as usize]
                        .copy_from_slice(&scratch[..take as usize * exp_channels as usize]);
                }
                total += got;
                continue;
            }
            term = st;
            bad = true;
            break;
        }
        (total, term, bad, hasher.hex())
    };
    if term == sys::SONG_EOF && !bad && total == exp_frames && sha == fx.pcm_sha256 {
        r.case_pass("s07.full-decode");
    } else {
        r.case_fail(
            "s07.full-decode",
            &format!("terminal={term} bad={bad} frames={total}/{exp_frames} sha={sha} want={}", fx.pcm_sha256),
        );
    }
    r.info(
        "drain",
        &format!("frames={total} terminal={term} pcm_sha256={sha}"),
    );

    // step 11: EOF stability + EOF distinct from error
    {
        let (s1, got1) = h.read(&mut full, 4096);
        let (s2, got2) = h.read(&mut full, 4096);
        if s1 == sys::SONG_EOF
            && got1 == 0
            && s2 == sys::SONG_EOF
            && got2 == 0
            && sys::SONG_EOF == 1
            && sys::SONG_EOF < 100
        {
            r.case_pass("s11.eof-stable");
        } else {
            r.case_fail("s11.eof-stable", &format!("s1={s1} got={got1} s2={s2} got2={got2}"));
        }
    }

    // steps 08–10: seek 25/50/75%
    for pct in [25i64, 50, 75] {
        let cname = format!("s{:02}.seek-{pct}", 7 + pct / 25);
        let target = fx.duration_us * pct / 100;
        let mut landing: i64 = -7;

        let mut pre_info: sys::song_info = unsafe { std::mem::zeroed() };
        let mut pre_meta: *const sys::song_metadata = std::ptr::null();
        let mut pre_raw = 0u32;
        let mut pre_aw = 0u32;
        let snap_ok = h.probe(&mut pre_info) == sys::SONG_OK
            && unsafe { sys::song_get_metadata(h.handle, &mut pre_meta) } == sys::SONG_OK
            && unsafe { sys::song_get_metadata_count(h.handle, &mut pre_raw) } == sys::SONG_OK
            && unsafe { sys::song_get_artwork_count(h.handle, &mut pre_aw) } == sys::SONG_OK;

        let st = if pct == 75 {
            // documented: out_actual_position_us may be NULL; observe the
            // landing with a second seek to the same target
            let s_null = unsafe { sys::song_seek(h.handle, target, std::ptr::null_mut()) };
            if s_null != sys::SONG_OK {
                r.case_fail(&cname, &format!("null-out seek status={s_null}"));
                r.case_fail(&format!("{cname}.lands"), "seek failed; landing unobservable");
                r.case_fail(&format!("{cname}.unchanged"), "seek failed");
                continue;
            }
            h.seek(target, &mut landing)
        } else {
            h.seek(target, &mut landing)
        };
        let seek_ok = snap_ok
            && st == sys::SONG_OK
            && (landing == -1 || (landing >= 0 && landing <= target));
        if seek_ok {
            r.case_pass(&cname);
        } else {
            r.case_fail(
                &cname,
                &format!("status={st} landing={landing} target={target} snap={snap_ok}"),
            );
        }
        r.info(
            "seek",
            &format!("pct={pct} target={target} landing={landing} status={st}"),
        );

        if st != sys::SONG_OK {
            r.case_fail(&format!("{cname}.lands"), "seek failed; landing read not observable");
            r.case_fail(&format!("{cname}.unchanged"), "seek failed");
            continue;
        }

        // next read belongs to the landing
        let mut post = vec![0.0f32; (exp_frames * exp_channels) as usize];
        let (got_total, term, saw_zero, bad_status) =
            drain(&h, &mut post, block, exp_channels, exp_frames);
        let mut lands_ok = term == sys::SONG_EOF && got_total > 0 && !saw_zero && !bad_status;
        if fx.lossless && landing >= 0 {
            let f = ((landing as u64 * fx.sample_rate as u64) + 500_000) / 1_000_000;
            lands_ok = lands_ok
                && f <= exp_frames
                && got_total == exp_frames - f
                && post[..got_total as usize * exp_channels as usize]
                    == full[f as usize * exp_channels as usize..]
                    [..got_total as usize * exp_channels as usize];
            if !lands_ok {
                r.info(
                    "lands-diff",
                    &format!("pct={pct} f={f} got={got_total} want={}", exp_frames - f),
                );
            }
        } else {
            // lossy (or unknown landing): the corpus freezes no numeric
            // post-seek tolerance and inventing one is forbidden — the
            // drain is recorded, not identity-compared.
            r.info(
                "post-seek-record-only",
                &format!(
                    "lossy={} landing={landing} frames={got_total}",
                    if fx.lossless { 1 } else { 0 }
                ),
            );
        }
        if lands_ok {
            r.case_pass(&format!("{cname}.lands"));
        } else {
            r.case_fail(
                &format!("{cname}.lands"),
                &format!("terminal={term} frames={got_total} zero={saw_zero} bad={bad_status}"),
            );
        }

        // metadata/artwork/format unchanged after seek
        {
            let mut post_info: sys::song_info = unsafe { std::mem::zeroed() };
            let mut post_meta: *const sys::song_metadata = std::ptr::null();
            let mut post_raw = 0u32;
            let mut post_aw = 0u32;
            let mut u_ok = h.probe(&mut post_info) == sys::SONG_OK
                && unsafe { sys::song_get_metadata(h.handle, &mut post_meta) } == sys::SONG_OK
                && unsafe { sys::song_get_metadata_count(h.handle, &mut post_raw) } == sys::SONG_OK
                && unsafe { sys::song_get_artwork_count(h.handle, &mut post_aw) } == sys::SONG_OK;
            if u_ok {
                let pre_meta_ref = unsafe { &*pre_meta };
                let post_meta_ref = unsafe { &*post_meta };
                u_ok = post_info.sample_rate == pre_info.sample_rate
                    && post_info.channels == pre_info.channels
                    && post_info.channel_mask == pre_info.channel_mask
                    && post_info.duration_us == pre_info.duration_us
                    && cstr_field(&post_info.codec) == cstr_field(&pre_info.codec)
                    && cstr_field(&post_info.container) == cstr_field(&pre_info.container)
                    && post_info.audio_stream_count == pre_info.audio_stream_count
                    && post_info.selected_audio_index == pre_info.selected_audio_index
                    && post_raw == pre_raw
                    && post_aw == pre_aw
                    && post_meta_ref.has_title == pre_meta_ref.has_title
                    && post_meta_ref.has_artist == pre_meta_ref.has_artist;
            }
            if u_ok {
                r.case_pass(&format!("{cname}.unchanged"));
            } else {
                r.case_fail(
                    &format!("{cname}.unchanged"),
                    "info/metadata/artwork changed across seek",
                );
            }
        }
    }

    // step 13: close mid-stream, reopen, re-drain stays stable
    {
        drop(h); // song_close via Drop (the raw handle owns the file)
        let h2 = match RawHandle::open(&fx.path) {
            Ok(h2) => h2,
            Err(status) => {
                r.case_fail("s13.reopen-stable", &format!("reopen open status={status}"));
                let mut probe_null: *mut sys::song_handle = std::ptr::null_mut();
                unsafe { sys::song_close(probe_null) };
                r.case_pass("s13.close-null-noop");
                r.finish();
                return r;
            }
        };
        let mut info2: sys::song_info = unsafe { std::mem::zeroed() };
        let mut ok = h2.probe(&mut info2) == sys::SONG_OK;
        if ok {
            let (total, term, saw_zero, bad_status) =
                drain(&h2, &mut full, 4096, exp_channels, exp_frames);
            let sha = sha256_bytes(byteme(
                &full[..total as usize * exp_channels as usize],
            ));
            ok = term == sys::SONG_EOF
                && !saw_zero
                && !bad_status
                && total == exp_frames
                && sha == fx.pcm_sha256;
            if !ok {
                r.info("reopen", &format!("frames={total} term={term} sha={sha}"));
            }
        }
        drop(h2);
        if ok {
            r.case_pass("s13.reopen-stable");
        } else {
            r.case_fail("s13.reopen-stable", "reopen drain differs or failed");
        }
    }

    // song_close(NULL) is a documented no-op; the process must survive here.
    let mut null_handle: *mut sys::song_handle = std::ptr::null_mut();
    unsafe { sys::song_close(null_handle) };
    r.case_pass("s13.close-null-noop");
    r.finish();
    r
}

fn byteme(samples: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 4) }
}

fn raw_readfault(path: &Path) -> Recorder {
    let mut r = Recorder::new("readfault");
    let h = match RawHandle::open(path) {
        Ok(h) => h,
        Err(status) => {
            r.case_fail("rf.open", &format!("status={status}"));
            r.finish();
            return r;
        }
    };
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    if !(h.probe(&mut info) == sys::SONG_OK) {
        r.case_fail("rf.open-probe", "probe failed");
        r.finish();
        return r;
    }
    r.case_pass("rf.open-probe");

    let block = 4096u64;
    let channels = info.channels as u64;
    let mut dst = vec![0.0f32; (block * channels) as usize];
    let mut total = 0u64;
    let mut term = sys::SONG_OK;
    loop {
        let (st, got) = h.read(&mut dst, block);
        if st == sys::SONG_OK {
            total += got;
            continue;
        }
        term = st;
        break;
    }
    if total > 0 {
        r.case_pass("rf.prefix-frames");
    } else {
        r.case_fail("rf.prefix-frames", "no clean prefix frames before failure");
    }
    // frozen #296/#324 attribution: SONG_ERR_DECODE_ERROR (108),
    // native = -1094995529 (AVERROR_INVALIDDATA)
    if term == sys::SONG_ERR_DECODE_ERROR {
        r.case_pass("rf.terminal-108");
    } else {
        r.case_fail("rf.terminal-108", &format!("terminal={term} want={}", sys::SONG_ERR_DECODE_ERROR));
    }
    let (_, native_code, message) = h.diag();
    if !message.is_empty() && native_code == AVERROR_INVALIDDATA {
        r.case_pass("rf.native-code");
    } else {
        r.case_fail(
            "rf.native-code",
            &format!("native={native_code} msg={message}"),
        );
    }
    // error stability: the next read re-surfaces the pending decode error
    {
        let (s2, got) = h.read(&mut dst, block);
        if s2 == sys::SONG_ERR_DECODE_ERROR && got == 0 {
            r.case_pass("rf.error-stable");
        } else {
            r.case_fail("rf.error-stable", &format!("next={s2} got={got}"));
        }
    }
    r.info(
        "readfault",
        &format!("frames_before={total} terminal={term} native={native_code} msg={message}"),
    );
    r.finish();
    r
}

fn raw_seekfault(path: &Path) -> Recorder {
    let mut r = Recorder::new("seekfault");
    let h = match RawHandle::open(path) {
        Ok(h) => h,
        Err(status) => {
            r.case_fail("sf.open", &format!("status={status}"));
            r.finish();
            return r;
        }
    };
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    if !(h.probe(&mut info) == sys::SONG_OK) {
        r.case_fail("sf.open-probe", "probe failed");
        r.finish();
        return r;
    }
    r.case_pass("sf.open-probe");

    let block = 4096u64;
    let channels = info.channels as u64;
    let mut dst = vec![0.0f32; (block * channels) as usize];

    // clean prefix before the corrupted region
    let mut prefix = 0u64;
    let mut prefix_clean = true;
    for _ in 0..8 {
        let (st, got) = h.read(&mut dst, block);
        if st != sys::SONG_OK {
            prefix_clean = false;
            break;
        }
        prefix += got;
    }
    if prefix_clean && prefix > 0 {
        r.case_pass("sf.prefix-clean");
    } else {
        r.case_fail("sf.prefix-clean", &format!("prefix={prefix}"));
    }

    // seek 2.0s lands on the corrupt body → typed SEEK_ERROR, -1 landing
    let mut landing: i64 = -7;
    let sseek = h.seek(2_000_000, &mut landing);
    let (_, _, diag_message) = h.diag();
    if sseek == sys::SONG_ERR_SEEK_ERROR && landing == -1 {
        r.case_pass("sf.seek-typed");
    } else {
        r.case_fail(
            "sf.seek-typed",
            &format!("status={sseek} landing={landing} want={}/-1", sys::SONG_ERR_SEEK_ERROR),
        );
    }
    if diag_message == FROZEN_SEEKFAULT_DIAGNOSTIC {
        r.case_pass("sf.seek-diagnostic");
    } else {
        r.case_fail("sf.seek-diagnostic", &format!("msg={diag_message}"));
    }

    // post-seek read surfaces the real landing-frame decode failure
    let (sread, got) = h.read(&mut dst, block);
    let (_, native2, _) = h.diag();
    if sread == sys::SONG_ERR_DECODE_ERROR && got == 0 && native2 == AVERROR_INVALIDDATA {
        r.case_pass("sf.post-read-108");
    } else {
        r.case_fail(
            "sf.post-read-108",
            &format!("status={sread} got={got} native={native2}"),
        );
    }
    r.info(
        "seekfault",
        &format!("prefix={prefix} seek={sseek} landing={landing} read={sread} native={native2}"),
    );
    r.finish();
    r
}

fn raw_truncated(path: &Path, orig_frames: u64) -> Recorder {
    let mut r = Recorder::new("truncated");
    let h = match RawHandle::open(path) {
        Ok(h) => h,
        Err(status) => {
            // a typed open failure is fail-closed legal for a truncated source
            if status_is_typed_error(status) {
                r.case_pass("tr.typed-open");
            } else {
                r.case_fail("tr.typed-open", &format!("untyped open status={status}"));
            }
            r.info("truncated", &format!("open_status={status}"));
            r.finish();
            return r;
        }
    };
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let p = h.probe(&mut info);
    if p != sys::SONG_OK {
        let (_, _, message) = h.diag();
        if status_is_typed_error(p) && !message.is_empty() {
            r.case_pass("tr.typed-open");
        } else {
            r.case_fail("tr.typed-open", &format!("untyped probe failure status={p}"));
        }
        r.info("truncated", &format!("probe_status={p}"));
        r.finish();
        return r;
    }
    r.case_pass("tr.typed-open");

    let block = 4096u64;
    let channels = info.channels as u64;
    let mut dst = vec![0.0f32; (orig_frames * channels) as usize];
    let (total, term, saw_zero, bad_status) = drain(&h, &mut dst, block, channels, orig_frames);
    let ok = total > 0
        && !saw_zero
        && !bad_status
        && (term == sys::SONG_EOF || status_is_typed_error(term));
    if ok {
        r.case_pass("tr.typed-terminal");
    } else {
        r.case_fail(
            "tr.typed-terminal",
            &format!("terminal={term} frames={total} zero={saw_zero} bad={bad_status}"),
        );
    }
    // terminal stability: EOF repeats; an error terminal stays typed
    {
        let (s2, _) = h.read(&mut dst, block);
        let stable = (term == sys::SONG_EOF && s2 == sys::SONG_EOF)
            || (status_is_typed_error(term) && status_is_typed_error(s2));
        if stable {
            r.case_pass("tr.terminal-stable");
        } else {
            r.case_fail("tr.terminal-stable", &format!("term={term} next={s2}"));
        }
    }
    r.info("truncated", &format!("frames={total} terminal={term}"));
    r.finish();
    r
}

fn raw_garbage(path: &Path) -> Recorder {
    let mut r = Recorder::new("garbage");
    // pre-init NULL: the caller must be left WITHOUT a usable handle
    match RawHandle::open(path) {
        Ok(_) => {
            r.case_fail("gb.typed-open-no-handle", "handle_nonnull=1 (garbage opened!)");
        }
        Err(status) => {
            let typed = status == sys::SONG_ERR_IO
                || status == sys::SONG_ERR_UNSUPPORTED_CONTAINER
                || status == sys::SONG_ERR_OUT_OF_MEMORY;
            if typed {
                r.case_pass("gb.typed-open-no-handle");
            } else {
                r.case_fail("gb.typed-open-no-handle", &format!("status={status}"));
            }
            let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
            r.info("garbage", &format!("{name} status={status}"));
        }
    }
    r.finish();
    r
}

fn raw_argcheck(fixture_path: &Path) -> Recorder {
    let mut r = Recorder::new("argcheck");
    // NULL-handle guards
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let mut got = 0u64;
    let mut cnt = 0u32;
    let mut err: *const sys::song_error = std::ptr::null();
    let s_read_null = unsafe { sys::song_read_pcm(std::ptr::null_mut(), std::ptr::null_mut(), 4096, &mut got) };
    let s_probe_null = unsafe { sys::song_probe(std::ptr::null_mut(), &mut info) };
    let s_diag_null = unsafe { sys::song_last_error(std::ptr::null_mut(), &mut err) };
    let s_count_null = unsafe { sys::song_audio_stream_count(std::ptr::null_mut(), &mut cnt) };
    if s_read_null == sys::SONG_ERR_INVALID_ARGUMENT
        && s_probe_null == sys::SONG_ERR_INVALID_ARGUMENT
        && s_diag_null == sys::SONG_ERR_INVALID_ARGUMENT
        && s_count_null == sys::SONG_ERR_INVALID_ARGUMENT
    {
        r.case_pass("ac.null-handle");
    } else {
        r.case_fail(
            "ac.null-handle",
            &format!("read={s_read_null} probe={s_probe_null} diag={s_diag_null} count={s_count_null}"),
        );
    }

    let h = match RawHandle::open(fixture_path) {
        Ok(h) => h,
        Err(status) => {
            r.case_fail("ac.open", &format!("status={status}"));
            r.finish();
            return r;
        }
    };

    // pre-probe: probed-handle operations → NOT_OPEN (102)
    {
        let mut c1 = 0u32;
        let mut g1 = 0u64;
        let mut l1 = 0i64;
        let mut m1: *const sys::song_metadata = std::ptr::null();
        let mut tmp = vec![0.0f32; 64];
        let a = unsafe { sys::song_audio_stream_count(h.handle, &mut c1) };
        let (b, _) = h.read(&mut tmp, 64);
        let c = h.seek(0, &mut l1);
        let d = unsafe { sys::song_get_metadata(h.handle, &mut m1) };
        let e = unsafe { sys::song_select_stream(h.handle, 0) };
        if a == sys::SONG_ERR_NOT_OPEN
            && b == sys::SONG_ERR_NOT_OPEN
            && c == sys::SONG_ERR_NOT_OPEN
            && d == sys::SONG_ERR_NOT_OPEN
            && e == sys::SONG_ERR_NOT_OPEN
        {
            r.case_pass("ac.pre-probe-not-open");
        } else {
            r.case_fail(
                "ac.pre-probe-not-open",
                &format!("count={a} read={b} seek={c} meta={d} sel={e}"),
            );
        }
    }

    if h.probe(&mut info) != sys::SONG_OK {
        r.case_fail("ac.probe", "probe failed");
        r.finish();
        return r;
    }

    // documented argument classes on a probed handle
    {
        let mut tmp = vec![0.0f32; 64];
        let (a, g0) = h.read(&mut tmp, 0); // capacity 0
        let b = unsafe { sys::song_select_stream(h.handle, info.audio_stream_count) };
        let c = unsafe { sys::song_seek(h.handle, -1, std::ptr::null_mut()) }; // negative target
        if a == sys::SONG_ERR_INVALID_ARGUMENT
            && g0 == 0
            && b == sys::SONG_ERR_INVALID_ARGUMENT
            && c == sys::SONG_ERR_INVALID_ARGUMENT
        {
            r.case_pass("ac.invalid-argument");
        } else {
            r.case_fail(
                "ac.invalid-argument",
                &format!("cap0={a} sel={b} seekneg={c}"),
            );
        }
    }

    // diagnostic contract: NULL after success, non-NULL after failure.
    {
        let mut ok_cnt = 0u32;
        unsafe { sys::song_audio_stream_count(h.handle, &mut ok_cnt) }; // successful call
        let (_, _, m1) = h.diag();
        let ok1 = m1.is_empty();
        unsafe { sys::song_seek(h.handle, -1, std::ptr::null_mut()) }; // typed failure
        let (_, _, m2) = h.diag();
        let ok2 = !m2.is_empty();
        if ok1 && ok2 {
            r.case_pass("ac.diag-contract");
        } else {
            r.case_fail("ac.diag-contract", &format!("after_ok_null={ok1} after_fail_msg={ok2}"));
        }
    }
    drop(h);
    r.finish();
    r
}

fn raw_closecheck(path: &Path, exp_frames: u64, exp_sha: &str) -> Recorder {
    let mut r = Recorder::new("closecheck");
    let h = match RawHandle::open(path) {
        Ok(h) => h,
        Err(status) => {
            r.case_fail("cc.open", &format!("open status={status}"));
            r.finish();
            return r;
        }
    };
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    if h.probe(&mut info) != sys::SONG_OK {
        r.case_fail("cc.open", "probe failed");
        r.finish();
        return r;
    }
    // read ONE block then close mid-stream
    let channels = info.channels as u64;
    let mut dst = vec![0.0f32; (4096 * channels) as usize];
    let (st, got) = h.read(&mut dst, 4096);
    if st != sys::SONG_OK || got == 0 {
        r.case_fail("cc.midstream-read", &format!("got={got}"));
        r.finish();
        return r;
    }
    r.case_pass("cc.midstream-read");
    drop(h); // song_close mid-stream

    // reopen and re-drain → the frozen identity again
    let h2 = match RawHandle::open(path) {
        Ok(h2) => h2,
        Err(status) => {
            r.case_fail("cc.reopen-identity", &format!("reopen status={status}"));
            r.finish();
            return r;
        }
    };
    let mut info2: sys::song_info = unsafe { std::mem::zeroed() };
    let mut ok = h2.probe(&mut info2) == sys::SONG_OK;
    if ok {
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut term = sys::SONG_OK;
        loop {
            let (s, g) = h2.read(&mut dst, 4096);
            if s == sys::SONG_EOF {
                term = s;
                break;
            }
            if s != sys::SONG_OK {
                term = s;
                break;
            }
            hasher.update(byteme(&dst[..g as usize * channels as usize]));
            total += g;
        }
        let sha = hasher.hex();
        ok = term == sys::SONG_EOF && total == exp_frames && sha == exp_sha;
        if !ok {
            r.info(
                "closecheck-reopen",
                &format!("frames={total} term={term} sha={sha}"),
            );
        }
    }
    drop(h2);
    if ok {
        r.case_pass("cc.reopen-identity");
    } else {
        r.case_fail("cc.reopen-identity", "reopen drain differs or failed");
    }
    r.finish();
    r
}

// --- adapter lane (qianqian-decode-songcore through the kernel) ----------

enum AdapterTerminal {
    Eof,
    Error(String),
}

/// Drain the adapter endpoint at 4096-frame blocks; the C child's drain
/// contract mirrored at the `read_frames` seam (every Frames(n) must have
/// n in (0, 4096]; EOF carries nothing; the drain must END).
fn adapter_drain(
    stream: &mut Box<dyn qianqian_audio_api::ports::DecodedPcmStream>,
    channels: usize,
    samples: &mut Vec<f32>,
) -> (u64, AdapterTerminal, bool) {
    let mut total = 0u64;
    let mut saw_zero = false;
    loop {
        let mut block = vec![0.0f32; 4096 * channels];
        match stream.read_frames(&mut block) {
            Ok(DecodeOutcome::Frames(n)) => {
                if n == 0 || n > 4096 {
                    saw_zero = true;
                    return (total, AdapterTerminal::Error("Frames(0) or oversized block".into()), saw_zero);
                }
                samples.extend_from_slice(&block[..n * channels]);
                total += n as u64;
            }
            Ok(DecodeOutcome::Eof) => return (total, AdapterTerminal::Eof, saw_zero),
            Err(e) => return (total, AdapterTerminal::Error(e.message), saw_zero),
        }
    }
}

fn adapter_sha(samples: &[f32]) -> String {
    sha256_bytes(byteme(samples))
}

struct AdapterEnv {
    decode: Rc<dyn PcmDecode>,
}

fn adapter_setup() -> Result<AdapterEnv, String> {
    let slot: Rc<RefCell<Option<Rc<dyn PcmDecode>>>> = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let consumer = ComponentSpec::new("binding_parity_capability_consumer")
        .requires::<PcmDecodeCapability>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<PcmDecodeCapability>()
                .map_err(|e| ActivationError::new(format!("capability resolve failed: {e:?}")))?;
            *slot_in.borrow_mut() = Some(binding.service());
            Ok(())
        });
    let mut runtime = QianqianApp::new();
    runtime
        .register_component(songcore_decode_plugin())
        .map_err(|e| format!("decode plugin registration failed: {e:?}"))?;
    runtime
        .register_component(consumer)
        .map_err(|e| format!("consumer registration failed: {e:?}"))?;
    runtime
        .revise_desired(vec![
            DesiredEntry::enabled("decode", "songcore_decode_plugin", Revision::new(1)),
            DesiredEntry::enabled(
                "consumer",
                "binding_parity_capability_consumer",
                Revision::new(1),
            ),
        ])
        .map_err(|e| format!("composition revise failed: {e:?}"))?;
    let _snap = runtime.composition_snapshot();
    let decode = slot
        .borrow()
        .clone()
        .ok_or("PcmDecode capability was never resolved into the consumer slot")?;
    std::mem::forget(runtime); // keep the fiber/service alive for the whole run
    Ok(AdapterEnv { decode })
}

fn adapter_parity(env: &AdapterEnv, fx: &Fixture) -> Recorder {
    let mut r = Recorder::new(&format!("adapter.parity.{}", fx.id));
    let exp_frames = fx.pcm_frames;
    let channels = fx.channels as usize;

    // a01: open via the kernel-resolved capability (the production open
    // path: song_open + song_probe inside the adapter, ABI check at
    // plugin construction)
    let mut stream = match env.decode.open_media(&fx.path) {
        Ok(s) => {
            r.case_pass("a01.open");
            s
        }
        Err(e) => {
            r.case_fail("a01.open", &format!("message={}", e.message));
            r.finish();
            return r;
        }
    };

    // a03: adapter-visible source facts vs frozen facts
    let fmt = stream.format();
    let duration_ok = stream.source_duration() == Some(Duration::from_micros(fx.duration_us as u64));
    if fmt.sample_rate as i64 == fx.sample_rate
        && fmt.channels as u16 as i64 == fx.channels
        && duration_ok
    {
        r.case_pass("a03.facts");
    } else {
        r.case_fail(
            "a03.facts",
            &format!(
                "rate={}/{} ch={}/{} duration={:?}/Some({})",
                fmt.sample_rate, fx.sample_rate, fmt.channels, fx.channels,
                stream.source_duration().map(|d| d.as_micros() as i64), fx.duration_us
            ),
        );
    }
    r.info(
        "adapter-probe",
        &format!(
            "fixture={} rate={} channels={} mask=0x{:x} duration={}",
            fx.id,
            fmt.sample_rate,
            fmt.channels,
            fmt.channel_mask,
            stream.source_duration().map(|d| d.as_micros() as i64).unwrap_or(-1)
        ),
    );

    // a07: full drain → frozen frame count + PCM SHA-256 (exact corpus
    // policy for ALL fixtures, lossy included)
    let mut samples = Vec::with_capacity(exp_frames as usize * channels);
    let (total, term, saw_zero) = adapter_drain(&mut stream, channels, &mut samples);
    let sha = adapter_sha(&samples);
    let decode_ok = matches!(term, AdapterTerminal::Eof)
        && !saw_zero
        && total == exp_frames
        && sha == fx.pcm_sha256;
    if decode_ok {
        r.case_pass("a07.full-decode");
    } else {
        let t = match &term {
            AdapterTerminal::Eof => "eof".to_string(),
            AdapterTerminal::Error(m) => format!("error({m})"),
        };
        r.case_fail(
            "a07.full-decode",
            &format!("terminal={t} zero={saw_zero} frames={total}/{exp_frames} sha={sha} want={}", fx.pcm_sha256),
        );
    }
    r.info(
        "adapter-drain",
        &format!("fixture={} frames={total} terminal={} pcm_sha256={sha}",
            fx.id,
            match &term { AdapterTerminal::Eof => "eof".to_string(), AdapterTerminal::Error(m) => format!("error({m})") }
        ),
    );

    // a11: EOF terminal and stable
    {
        let mut block = vec![0.0f32; 4096 * channels];
        match stream.read_frames(&mut block) {
            Ok(DecodeOutcome::Eof) => r.case_pass("a11.eof-stable"),
            other => r.case_fail("a11.eof-stable", &format!("after-eof read={other:?}")),
        }
    }

    // a08–a10: seek 25/50/75% on the same drained endpoint (the C child's
    // order), three-class outcome, landing or explicit unknown
    for pct in [25i64, 50, 75] {
        let cname = format!("a{:02}.seek-{pct}", 7 + pct / 25);
        let target = fx.duration_us * pct / 100;
        let pre_fmt = stream.format();
        match stream.seek(Duration::from_micros(target as u64)) {
            ProviderSeekOutcome::Applied { landing } => {
                // landing-or-unknown: Some(f) must sit at/before the target;
                // None is the explicit "landing unknown" sentinel (the ABI's
                // -1), never a manufactured number
                let landing_ok = match landing {
                    None => true,
                    Some(f) => {
                        (f as u128 * 1_000_000)
                            <= (target as u128 * fx.sample_rate as u128 + 500_000)
                    }
                };
                if !landing_ok {
                    r.case_fail(&cname, &format!("landing={landing:?} target_us={target}"));
                    r.case_fail(&format!("{cname}.lands"), "landing above target");
                    r.case_fail(&format!("{cname}.unchanged"), "seek landing invalid");
                    continue;
                }
                r.case_pass(&cname);
                r.info(
                    "adapter-seek",
                    &format!(
                        "fixture={} pct={pct} target={target} landing={}",
                        fx.id,
                        landing.map(|f| f as i64).unwrap_or(-1)
                    ),
                );

                // next read belongs to the landing
                let mut post = Vec::new();
                let (got_total, term, saw_zero) = adapter_drain(&mut stream, channels, &mut post);
                let mut lands_ok = matches!(term, AdapterTerminal::Eof)
                    && got_total > 0
                    && !saw_zero;
                if fx.lossless {
                    if let Some(f) = landing {
                        lands_ok = lands_ok
                            && f <= exp_frames
                            && got_total == exp_frames - f
                            && post[..]
                                == samples[f as usize * channels..]
                                [..got_total as usize * channels];
                        if !lands_ok {
                            r.print(format!(
                                "INFO adapter-lands-diff pct={pct} f={f} got={got_total} want={}",
                                exp_frames - f
                            ));
                        }
                    } else {
                        // unknown landing: recorded, not identity-compared
                        r.print(format!(
                            "INFO adapter-post-seek-record-only landing=unknown frames={got_total}"
                        ));
                    }
                } else {
                    r.print(format!(
                        "INFO adapter-post-seek-record-only lossy=1 landing={} frames={got_total}",
                        landing.map(|f| f as i64).unwrap_or(-1)
                    ));
                }
                if lands_ok {
                    r.case_pass(&format!("{cname}.lands"));
                } else {
                    let t = match &term {
                        AdapterTerminal::Eof => "eof".to_string(),
                        AdapterTerminal::Error(m) => format!("error({m})"),
                    };
                    r.case_fail(
                        &format!("{cname}.lands"),
                        &format!("terminal={t} frames={got_total} zero={saw_zero}"),
                    );
                }

                // format/duration unchanged after seek (the full
                // metadata/artwork unchanged arm is witnessed at the raw
                // lane; the adapter seam exposes format + duration)
                let post_fmt = stream.format();
                let unchanged = post_fmt == pre_fmt
                    && stream.source_duration()
                        == Some(Duration::from_micros(fx.duration_us as u64));
                if unchanged {
                    r.case_pass(&format!("{cname}.unchanged"));
                } else {
                    r.case_fail(
                        &format!("{cname}.unchanged"),
                        "adapter format/duration changed across seek",
                    );
                }
            }
            ProviderSeekOutcome::RefusedUnchanged => {
                r.case_fail(&cname, "healthy-fixture seek must Apply, not Refuse");
                r.case_fail(&format!("{cname}.lands"), "seek refused");
                r.case_fail(&format!("{cname}.unchanged"), "seek refused");
            }
            ProviderSeekOutcome::MutatedThenFailed { diagnostic } => {
                r.case_fail(&cname, &format!("diagnostic={diagnostic}"));
                r.case_fail(&format!("{cname}.lands"), "seek failed");
                r.case_fail(&format!("{cname}.unchanged"), "seek failed");
            }
        }
    }

    // a13: drop (song_close) then reopen and re-drain → the frozen identity
    {
        drop(stream);
        match env.decode.open_media(&fx.path) {
            Ok(mut s2) => {
                let mut samples2 = Vec::new();
                let (total2, term2, saw_zero2) =
                    adapter_drain(&mut s2, channels, &mut samples2);
                let sha2 = adapter_sha(&samples2);
                if matches!(term2, AdapterTerminal::Eof)
                    && !saw_zero2
                    && total2 == exp_frames
                    && sha2 == fx.pcm_sha256
                {
                    r.case_pass("a13.reopen-stable");
                } else {
                    r.case_fail(
                        "a13.reopen-stable",
                        &format!("frames={total2}/{exp_frames} sha={sha2}"),
                    );
                }
            }
            Err(e) => r.case_fail("a13.reopen-stable", &format!("reopen message={}", e.message)),
        }
    }
    r.finish();
    r
}

fn adapter_readfault(env: &AdapterEnv, path: &Path, expect_msg: &str) -> Recorder {
    let mut r = Recorder::new("adapter.readfault");
    let mut stream = match env.decode.open_media(path) {
        Ok(s) => s,
        Err(e) => {
            r.case_fail("ar.open", &format!("message={}", e.message));
            r.finish();
            return r;
        }
    };
    let channels = stream.format().channels as usize;
    let mut total = 0u64;
    let mut first_error: Option<String> = None;
    for _ in 0..200 {
        let mut block = vec![0.0f32; 4096 * channels];
        match stream.read_frames(&mut block) {
            Ok(DecodeOutcome::Frames(n)) => total += n as u64,
            Ok(DecodeOutcome::Eof) => {
                first_error = Some("unexpected clean EOF".into());
                break;
            }
            Err(e) => {
                first_error = Some(e.message);
                break;
            }
        }
    }
    match first_error {
        Some(msg) if total > 0 => {
            r.case_pass("ar.prefix-frames");
            // the adapter's first-failure message IS the C child's message
            // for the same failing read (same artifact, same text source)
            if msg == expect_msg {
                r.case_pass("ar.message-parity");
            } else {
                r.case_fail("ar.message-parity", &format!("msg={msg} want={expect_msg}"));
            }
            // error stability: the next read re-surfaces a typed failure,
            // never silent OK/EOF (the C child asserts status 108 + got 0;
            // the adapter exposes the class via Err — the typed status and
            // native code ride the diagnostic outlet, witnessed by the
            // crate's own E1/DG-3 tests)
            let mut block = vec![0.0f32; 4096 * channels];
            match stream.read_frames(&mut block) {
                Err(_) => r.case_pass("ar.error-stable"),
                other => r.case_fail("ar.error-stable", &format!("next={other:?}")),
            }
            r.info(
                "adapter-readfault",
                &format!("frames_before={total} message={msg}"),
            );
        }
        Some(msg) => {
            r.case_fail("ar.prefix-frames", &format!("no prefix; message={msg}"));
        }
        None => {
            r.case_fail("ar.prefix-frames", "no terminal within 200 reads");
        }
    }
    r.finish();
    r
}

fn adapter_seekfault(env: &AdapterEnv, path: &Path) -> Recorder {
    let mut r = Recorder::new("adapter.seekfault");
    let mut stream = match env.decode.open_media(path) {
        Ok(s) => s,
        Err(e) => {
            r.case_fail("as.open", &format!("message={}", e.message));
            r.finish();
            return r;
        }
    };
    let channels = stream.format().channels as usize;
    // clean prefix before the corrupted region
    let mut prefix = 0u64;
    let mut prefix_clean = true;
    for _ in 0..8 {
        let mut block = vec![0.0f32; 4096 * channels];
        match stream.read_frames(&mut block) {
            Ok(DecodeOutcome::Frames(n)) => prefix += n as u64,
            _ => {
                prefix_clean = false;
                break;
            }
        }
    }
    if prefix_clean && prefix > 0 {
        r.case_pass("as.prefix-clean");
    } else {
        r.case_fail("as.prefix-clean", &format!("prefix={prefix}"));
    }
    // seek 2.0s lands on the corrupt body → MutatedThenFailed with the C
    // child's exact diagnostic, no landing
    match stream.seek(Duration::from_micros(2_000_000)) {
        ProviderSeekOutcome::MutatedThenFailed { diagnostic } => {
            if diagnostic == FROZEN_SEEKFAULT_DIAGNOSTIC {
                r.case_pass("as.seek-typed-diagnostic");
            } else {
                r.case_fail(
                    "as.seek-typed-diagnostic",
                    &format!("diagnostic={diagnostic} want={FROZEN_SEEKFAULT_DIAGNOSTIC}"),
                );
            }
        }
        other => {
            r.case_fail("as.seek-typed-diagnostic", &format!("outcome={other:?}"));
        }
    }
    // post-seek read surfaces the landing-frame decode failure (never
    // resumes production)
    let mut block = vec![0.0f32; 4096 * channels];
    match stream.read_frames(&mut block) {
        Err(_) => r.case_pass("as.post-read-fails"),
        other => r.case_fail("as.post-read-fails", &format!("read={other:?}")),
    }
    r.info(
        "adapter-seekfault",
        &format!("prefix={prefix}"),
    );
    r.finish();
    r
}

fn adapter_open_errors(env: &AdapterEnv, garbage: &Path, empty: &Path) -> Recorder {
    let mut r = Recorder::new("adapter.open-errors");
    for (name, path) in [("garbage-nomedia", garbage), ("garbage-empty", empty)] {
        match env.decode.open_media(path) {
            Ok(_) => r.case_fail(
                &format!("ao.{name}-refused"),
                "non-media input opened as media",
            ),
            Err(e) => {
                // the adapter relays the typed decoder status in its message
                // ("decoder status <n>"); the class itself is C-side truth
                r.case_pass(&format!("ao.{name}-refused"));
                r.info(
                    "adapter-open-error",
                    &format!("input={name} message={}", e.message),
                );
            }
        }
    }
    r.finish();
    r
}

fn adapter_probe_facts(env: &AdapterEnv, fixtures: &[Fixture]) -> Recorder {
    let mut r = Recorder::new("adapter.probe-facts");
    for fx in fixtures {
        match probe_media(&fx.path) {
            Ok(facts) => {
                let ok = facts.format.sample_rate as i64 == fx.sample_rate
                    && facts.format.channels as u16 as i64 == fx.channels
                    && facts.duration == Some(Duration::from_micros(fx.duration_us as u64));
                if ok {
                    r.case_pass(&format!("ap.facts.{}", fx.id));
                } else {
                    r.case_fail(
                        &format!("ap.facts.{}", fx.id),
                        &format!(
                            "rate={} ch={} duration={:?} want {}/{}/Some({})",
                            facts.format.sample_rate,
                            facts.format.channels,
                            facts.duration.map(|d| d.as_micros() as i64),
                            fx.sample_rate,
                            fx.channels,
                            fx.duration_us
                        ),
                    );
                }
                r.info(
                    "adapter-probe-media",
                    &format!(
                        "fixture={} rate={} channels={} duration={}",
                        fx.id,
                        facts.format.sample_rate,
                        facts.format.channels,
                        facts.duration.map(|d| d.as_micros() as i64).unwrap_or(-1)
                    ),
                );
            }
            Err(e) => r.case_fail(&format!("ap.facts.{}", fx.id), &format!("message={}", e.message)),
        }
    }
    r.finish();
    r
}

// --- C-suite receipt parsing (the recorded CORE_RUNTIME reference) -------

struct CSuiteReference {
    stamp: String,
    verdict: String,
    /// per case id → (ordered CASE results, ordered INFO events)
    cases: std::collections::BTreeMap<String, (Vec<(String, bool)>, Vec<(String, String)>)>,
}

fn normalize_c_line(line: &str) -> &str {
    line.trim_end_matches('\r')
}

fn parse_c_suite(dir: &Path) -> CSuiteReference {
    let summary = load_json(&dir.join("summary.json"));
    let verdict = summary["verdict"].as_str().unwrap_or("MISSING").to_string();
    let stamp = summary["stamp"].as_str().unwrap_or("").to_string();
    let mut cases = std::collections::BTreeMap::new();
    if let Some(list) = summary["cases"].as_array() {
        for c in list {
            let id = c["id"].as_str().unwrap_or("").to_string();
            let receipt = c["receipt"].as_str().unwrap_or("").to_string();
            if receipt.is_empty() {
                continue;
            }
            let path = PathBuf::from(&receipt);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read C receipt {}: {e}", path.display()));
            let mut case_list = Vec::new();
            let mut info = Vec::new();
            for line in text.lines() {
                let line = normalize_c_line(line);
                if let Some(rest) = line.strip_prefix("CASE ") {
                    let mut it = rest.splitn(3, ' ');
                    let name = it.next().unwrap_or("").to_string();
                    let verdict = it.next().unwrap_or("") == "PASS";
                    case_list.push((name, verdict));
                } else if let Some(rest) = line.strip_prefix("INFO ") {
                    let mut it = rest.splitn(2, ' ');
                    let kind = it.next().unwrap_or("").to_string();
                    let payload = it.next().unwrap_or("").to_string();
                    info.push((kind, payload));
                } else if line.starts_with("META[") {
                    info.push(("META".to_string(), line.to_string()));
                }
            }
            cases.insert(id, (case_list, info));
        }
    }
    CSuiteReference { stamp, verdict, cases }
}

/// Normalize one INFO payload for comparison: the garbage event carries a
/// file path that legitimately differs between the two runs (different
/// evidence directories) — compare its status field only.
fn normalized_payload(kind: &str, payload: &str) -> String {
    if kind == "garbage" {
        match payload.rsplit_once("status=") {
            Some((_, status)) => format!("status={status}"),
            None => payload.to_string(),
        }
    } else {
        payload.to_string()
    }
}

fn compare_info_events(
    mine: &[(String, String)],
    c_ref: &[(String, String)],
) -> Vec<String> {
    let mut diffs = Vec::new();
    let n = mine.len().max(c_ref.len());
    for i in 0..n {
        match (mine.get(i), c_ref.get(i)) {
            (Some((mk, mp)), Some((ck, cp))) => {
                if mk != ck || normalized_payload(mk, mp) != normalized_payload(ck, cp) {
                    diffs.push(format!(
                        "event[{i}] kind={mk}/{ck} binding=<{mp}> c=<{cp}>"
                    ));
                }
            }
            (Some((mk, mp)), None) => diffs.push(format!("event[{i}] extra binding event {mk} <{mp}>")),
            (None, Some((ck, cp))) => diffs.push(format!("event[{i}] missing binding event {ck} <{cp}>")),
            (None, None) => break,
        }
        if diffs.len() >= 6 {
            diffs.push("... (further diffs suppressed)".to_string());
            break;
        }
    }
    diffs
}

fn compare_case_lists(mine: &[(String, bool)], c_ref: &[(String, bool)]) -> Vec<String> {
    let mut diffs = Vec::new();
    let n = mine.len().max(c_ref.len());
    for i in 0..n {
        match (mine.get(i), c_ref.get(i)) {
            (Some((mn, mp)), Some((cn, cp))) => {
                if mn != cn || mp != cp {
                    diffs.push(format!(
                        "case[{i}] binding=<{mn}/{}> c=<{cn}/{}>",
                        if *mp { "PASS" } else { "FAIL" },
                        if *cp { "PASS" } else { "FAIL" }
                    ));
                }
            }
            (Some((mn, _)), None) => diffs.push(format!("case[{i}] extra binding case {mn}")),
            (None, Some((cn, _))) => diffs.push(format!("case[{i}] missing binding case {cn}")),
            (None, None) => break,
        }
        if diffs.len() >= 6 {
            diffs.push("... (further diffs suppressed)".to_string());
            break;
        }
    }
    diffs
}

// --- summary --------------------------------------------------------------

struct SummaryCase {
    id: String,
    result: String,
    detail: serde_json::Value,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut out_root = PathBuf::new();
    let mut c_stamp = String::from("20261029-052222");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                out_root = PathBuf::from(&args[i]);
            }
            "--c-stamp" => {
                i += 1;
                c_stamp = args[i].clone();
            }
            other => {
                eprintln!("unknown arg {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let root = find_project_root();
    if out_root.as_os_str().is_empty() {
        out_root = root.join("evidence/songcore-172/binding-parity");
    }
    let c_dir = root.join("evidence/songcore-172/suite-results").join(&c_stamp);

    // ---- preflight (fail-closed) ----------------------------------------
    sha256_self_test();
    let artifact = root.join("native/build/artifacts/libsongcore.a");
    if !artifact.exists() {
        eprintln!("ABORT exit=10 artifact missing: {}", artifact.display());
        std::process::exit(10);
    }
    let artifact_sha = sha256_file(&artifact);
    if artifact_sha != sys::QN_SONGCORE_ARTIFACT_SHA256 {
        eprintln!(
            "ABORT exit=11 artifact sha drift between linked build input and runtime file: build={} runtime={}",
            sys::QN_SONGCORE_ARTIFACT_SHA256, artifact_sha
        );
        std::process::exit(11);
    }
    if artifact_sha != FROZEN_ARTIFACT_SHA256 {
        eprintln!(
            "ABORT exit=11 artifact sha mismatch: got {artifact_sha} want {FROZEN_ARTIFACT_SHA256} (the C-suite frozen verification-grade artifact)"
        );
        std::process::exit(11);
    }
    if sys::QN_SONGCORE_HEADER_SHA256 != FROZEN_HEADER_SHA256 {
        eprintln!(
            "ABORT exit=11 header sha mismatch: got {} want {FROZEN_HEADER_SHA256}",
            sys::QN_SONGCORE_HEADER_SHA256
        );
        std::process::exit(11);
    }
    let abi = unsafe { sys::songcore_abi_version() };
    if abi != 1 {
        eprintln!("ABORT exit=11 songcore_abi_version()={abi}, want 1");
        std::process::exit(11);
    }
    let fixtures = load_fixtures(&root);
    if fixtures.len() != 4 {
        eprintln!("ABORT exit=12 corpus fixture count {} != 4", fixtures.len());
        std::process::exit(12);
    }
    let c_ref = parse_c_suite(&c_dir);
    if c_ref.verdict != "PASS" {
        eprintln!(
            "ABORT exit=11 C-suite reference {} verdict={} (want PASS) — no valid direct-C reference to compare against",
            c_dir.display(),
            c_ref.verdict
        );
        std::process::exit(11);
    }

    // ---- evidence directory + frozen malformed constructions ------------
    let stamp = unix_stamp();
    let ev_dir = out_root.join(&stamp);
    std::fs::create_dir_all(ev_dir.join("media"))
        .unwrap_or_else(|e| panic!("create {}: {e}", ev_dir.display()));

    let flac = fixtures
        .iter()
        .find(|f| f.id == "flac-16-44-stereo")
        .expect("flac-16-44-stereo in corpus")
        .path
        .clone();
    let flac_bytes = std::fs::read(&flac).expect("flac fixture readable");
    let mut rf = flac_bytes.clone();
    rf[76000] ^= 0x01;
    let rf_path = ev_dir.join("media/readfault-76000.flac");
    std::fs::write(&rf_path, &rf).expect("write readfault construction");
    let mut sf = flac_bytes.clone();
    for off in [73502usize, 77042, 80375] {
        sf[off] ^= 0x01;
    }
    let sf_path = ev_dir.join("media/seekfault-73502-77042-80375.flac");
    std::fs::write(&sf_path, &sf).expect("write seekfault construction");
    let mut tr = flac_bytes.clone();
    tr.truncate(100000);
    let tr_path = ev_dir.join("media/truncated-100000.flac");
    std::fs::write(&tr_path, &tr).expect("write truncated construction");
    let tr_sha = sha256_bytes(&tr);
    if tr_sha != FROZEN_TRUNCATED_SHA256 {
        eprintln!(
            "ABORT exit=11 truncated construction drift: got {tr_sha} want {FROZEN_TRUNCATED_SHA256}"
        );
        std::process::exit(11);
    }
    let gb_path = ev_dir.join("media/garbage-nomedia.bin");
    std::fs::write(&gb_path, b"QIANQIAN-CSUITE this is not a media file at all 0123456789abcdef")
        .expect("write garbage construction");
    let em_path = ev_dir.join("media/empty.bin");
    std::fs::write(&em_path, b"").expect("write empty construction");

    let mut summary_cases: Vec<SummaryCase> = Vec::new();

    // ---- ABI/layout gate (binding side of the songcore-call-comparison
    // ---- layout gate): fresh Rust emission vs the FROZEN committed
    // ---- receipts (layout-rust.txt produced by the same emitter lineage;
    // ---- layout-c.txt is the C probe's committed output). The
    // ---- call-comparison crate itself cannot build on this host (its
    // ---- build.rs compiles the benchmark C caller with -lm -lpthread),
    // ---- so the emitter is reused verbatim as a module here.
    {
        let fresh = layout_probe::emit().replace("
", "
");
        let frozen_rust_path = root.join("experiments/songcore-call-comparison/results/layout-rust.txt");
        let frozen_c_path = root.join("experiments/songcore-call-comparison/results/layout-c.txt");
        let norm = |p: &Path| -> String {
            std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
                .replace("
", "
")
        };
        let fr = norm(&frozen_rust_path);
        let fc = norm(&frozen_c_path);
        let mut rec = Recorder::new("abi-layout-frozen");
        let lines = fresh.trim_end().lines().count();
        std::fs::write(ev_dir.join("layout-rust-now.txt"), &fresh).expect("write layout-rust-now.txt");
        if fresh.trim_end() == fr.trim_end() {
            rec.case_pass("lf.rust-vs-frozen-rust");
        } else {
            rec.case_fail("lf.rust-vs-frozen-rust", &format!("fresh Rust emission differs from committed layout-rust.txt ({lines} lines)"));
        }
        if fresh.trim_end() == fc.trim_end() {
            rec.case_pass("lf.rust-vs-frozen-c");
        } else {
            rec.case_fail("lf.rust-vs-frozen-c", "fresh Rust emission differs from committed layout-c.txt");
        }
        let pass = rec.finish();
        let path = save_log(&ev_dir, "abi-layout-frozen", &rec);
        summary_cases.push(SummaryCase {
            id: "raw.abi-layout-frozen".into(),
            result: if pass { "PASS" } else { "FAIL" }.into(),
            detail: serde_json::json!({
                "frozen_rust": frozen_rust_path.display().to_string(),
                "frozen_c": frozen_c_path.display().to_string(),
                "lines": lines,
                "receipt": path.display().to_string(),
            }),
        });
    }


    // ---- raw lane (qianqian-songcore-sys, the C child ported 1:1) -------
    let raw_recorders: Vec<(String, Recorder)> = fixtures
        .iter()
        .map(|fx| (format!("parity.{}", fx.id), raw_parity(fx)))
        .collect();
    let mut raw_all: Vec<(String, Recorder)> = raw_recorders;
    raw_all.push(("readfault-flac".into(), raw_readfault(&rf_path)));
    raw_all.push(("seekfault-flac".into(), raw_seekfault(&sf_path)));
    let flac_fixture = fixtures.iter().find(|f| f.id == "flac-16-44-stereo").unwrap();
    raw_all.push((
        "truncated-flac".into(),
        raw_truncated(&tr_path, flac_fixture.pcm_frames),
    ));
    raw_all.push(("garbage-nomedia".into(), raw_garbage(&gb_path)));
    raw_all.push(("garbage-empty".into(), raw_garbage(&em_path)));
    raw_all.push((
        "argcheck-stable-classes".into(),
        raw_argcheck(&flac_fixture.path),
    ));
    raw_all.push((
        "closecheck-reopen-stable".into(),
        raw_closecheck(&flac_fixture.path, flac_fixture.pcm_frames, &flac_fixture.pcm_sha256),
    ));
    for (name, rec) in &raw_all {
        let path = save_log(&ev_dir, name, rec);
        let (cases, failed, pass) = rec.mode_result.expect("mode finished");
        summary_cases.push(SummaryCase {
            id: format!("raw.{name}"),
            result: if pass { "PASS" } else { "FAIL" }.into(),
            detail: serde_json::json!({
                "mode_cases": cases,
                "mode_failed": failed,
                "receipt": path.display().to_string(),
            }),
        });
    }

    // ---- adapter lane (qianqian-decode-songcore through the kernel) -----
    let env = match adapter_setup() {
        Ok(env) => env,
        Err(why) => {
            // write the aborting summary before dying
            write_summary(
                &ev_dir,
                &stamp,
                "FAIL",
                &artifact_sha,
                &c_ref,
                &summary_cases,
                &[],
                Some(format!("adapter setup aborted: {why}")),
            );
            eprintln!("ABORT exit=15 adapter kernel setup failed: {why}");
            std::process::exit(15);
        }
    };

    let mut adapter_recs: Vec<Recorder> = Vec::new();
    for fx in &fixtures {
        adapter_recs.push(adapter_parity(&env, fx));
    }
    let readfault_c_msg = c_ref
        .cases
        .get("readfault-flac")
        .and_then(|(_, info)| {
            info.iter().find(|(k, _)| k == "readfault").map(|(_, p)| {
                p.split(" msg=").nth(1).unwrap_or("").to_string()
            })
        })
        .unwrap_or_default();
    adapter_recs.push(adapter_readfault(&env, &rf_path, &readfault_c_msg));
    adapter_recs.push(adapter_seekfault(&env, &sf_path));
    adapter_recs.push(adapter_open_errors(&env, &gb_path, &em_path));
    adapter_recs.push(adapter_probe_facts(&env, &fixtures));

    let mut adapter_all_pass = true;
    {
        let mut adapter_log = String::new();
        for rec in &adapter_recs {
            for line in &rec.lines {
                adapter_log.push_str(line);
                adapter_log.push('\n');
            }
            let (cases, failed, pass) = rec.mode_result.expect("mode finished");
            adapter_all_pass &= pass;
            summary_cases.push(SummaryCase {
                id: format!("adapter.{}", rec.mode),
                result: if pass { "PASS" } else { "FAIL" }.into(),
                detail: serde_json::json!({ "mode_cases": cases, "mode_failed": failed }),
            });
        }
        std::fs::write(ev_dir.join("adapter.log"), adapter_log)
            .expect("write adapter.log");
    }

    // ---- comparison lane (this run vs the recorded C-suite receipts) ----
    let mut cmp = Recorder::new("c-vs-binding");

    for (name, rec) in &raw_all {
        let Some((c_cases, c_info)) = c_ref.cases.get(name.as_str()) else {
            cmp.case_fail(&format!("cvb.{name}"), "no recorded C receipt for this case");
            continue;
        };
        let mut diffs = compare_case_lists(&rec.case_list, c_cases);
        diffs.extend(compare_info_events(&rec.info, c_info));
        if diffs.is_empty() {
            cmp.case_pass(&format!("cvb.{name}"));
        } else {
            cmp.case_fail(&format!("cvb.{name}"), &diffs.join("; "));
        }
    }

    // adapter-vs-C field comparisons on the frozen comparison set
    for fx in &fixtures {
        let c_id = format!("parity.{}", fx.id);
        let Some((_, c_info)) = c_ref.cases.get(&c_id) else {
            cmp.case_fail(&format!("cvad.facts.{}", fx.id), "no C parity receipt");
            continue;
        };
        let c_probe = c_info.iter().find(|(k, _)| k == "probe");
        let c_drain = c_info.iter().find(|(k, _)| k == "drain");
        let a_probe = adapter_recs
            .iter()
            .find(|r| r.mode == format!("adapter.parity.{}", fx.id))
            .map(|r| r.info.iter().find(|(k, _)| k == "adapter-probe").cloned())
            .flatten();
        let a_drain = adapter_recs
            .iter()
            .find(|r| r.mode == format!("adapter.parity.{}", fx.id))
            .map(|r| r.info.iter().find(|(k, _)| k == "adapter-drain").cloned())
            .flatten();

        // facts: rate/channels/duration must agree between the C probe and
        // the adapter's SourceFacts
        match (c_probe, a_probe) {
            (Some((_, cp)), Some((_, ap))) => {
                let c_map = kv(cp);
                let a_map = kv(&ap);
                let bad: Vec<String> = ["rate", "channels", "duration"]
                    .iter()
                    .filter_map(|f| {
                        let f = *f;
                        if c_map.get(f) != a_map.get(f) {
                            Some(format!(
                                "{f}: c={:?} adapter={:?}",
                                c_map.get(f),
                                a_map.get(f)
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();
                if bad.is_empty() {
                    cmp.case_pass(&format!("cvad.facts.{}", fx.id));
                } else {
                    cmp.case_fail(&format!("cvad.facts.{}", fx.id), &bad.join("; "));
                }
            }
            _ => cmp.case_fail(&format!("cvad.facts.{}", fx.id), "missing probe events"),
        }

        // decode identity: frames + pcm_sha256
        match (c_drain, a_drain) {
            (Some((_, cp)), Some((_, ap))) => {
                let c_map = kv(cp);
                let a_map = kv(&ap);
                let bad: Vec<String> = ["frames", "pcm_sha256"]
                    .iter()
                    .filter_map(|f| {
                        let f = *f;
                        if c_map.get(f) != a_map.get(f) {
                            Some(format!(
                                "{f}: c={:?} adapter={:?}",
                                c_map.get(f),
                                a_map.get(f)
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();
                if bad.is_empty() {
                    cmp.case_pass(&format!("cvad.drain.{}", fx.id));
                } else {
                    cmp.case_fail(&format!("cvad.drain.{}", fx.id), &bad.join("; "));
                }
            }
            _ => cmp.case_fail(&format!("cvad.drain.{}", fx.id), "missing drain events"),
        }

        // seek landings: C µs landing → frames must equal the adapter's
        // frames landing (same artifact, same rounding); -1 ↔ unknown
        for pct in [25i64, 50, 75] {
            let c_seek = c_info
                .iter()
                .find(|(k, p)| k == "seek" && kv(p).get("pct").map(|v| v.as_str()) == Some(&pct.to_string()))
                .map(|(_, p)| p.to_string());
            let a_seek = adapter_recs
                .iter()
                .find(|r| r.mode == format!("adapter.parity.{}", fx.id))
                .and_then(|r| {
                    r.info
                        .iter()
                        .find(|(k, p)| {
                            k == "adapter-seek"
                                && kv(p).get("pct").map(|v| v.as_str()) == Some(&pct.to_string())
                        })
                        .map(|(_, p)| p.to_string())
                });
            match (c_seek, a_seek) {
                (Some(cp), Some(ap)) => {
                    let cm = kv(&cp);
                    let am = kv(&ap);
                    let rate = fx.sample_rate as u128;
                    let c_landing = cm.get("landing").and_then(|v| v.parse::<i64>().ok());
                    let a_landing = am.get("landing").and_then(|v| v.parse::<i64>().ok());
                    let c_frame = c_landing.filter(|v| *v >= 0).map(|v| {
                        ((v as u128 * rate + 500_000) / 1_000_000).to_string()
                    });
                    let a_frame = a_landing.filter(|v| *v >= 0).map(|v| v.to_string());
                    if c_frame == a_frame && cm.get("target") == am.get("target") {
                        cmp.case_pass(&format!("cvad.seek.{}.p{pct}", fx.id));
                    } else {
                        cmp.case_fail(
                            &format!("cvad.seek.{}.p{pct}", fx.id),
                            &format!(
                                "landing c={c_landing:?}→{:?} adapter={a_landing:?} target c={:?} adapter={:?}",
                                c_frame, cm.get("target"), am.get("target")
                            ),
                        );
                    }
                }
                _ => cmp.case_fail(
                    &format!("cvad.seek.{}.p{pct}", fx.id),
                    "missing seek event on either side",
                ),
            }
        }
    }

    // adapter readfault/seekfault/garbage class parity vs the C receipts
    if let (Some((_, c_info)), Some(rec)) = (
        c_ref.cases.get("readfault-flac"),
        adapter_recs.iter().find(|r| r.mode == "adapter.readfault"),
    ) {
        let c = c_info
            .iter()
            .find(|(k, _)| k == "readfault")
            .map(|(_, p)| kv(p))
            .expect("C readfault INFO");
        let a = rec
            .info
            .iter()
            .find(|(k, _)| k == "adapter-readfault")
            .map(|(_, p)| kv(p))
            .expect("adapter readfault INFO");
        let c_frames = c.get("frames_before").cloned().unwrap_or_default();
        let a_frames = a.get("frames_before").cloned().unwrap_or_default();
        if c_frames == a_frames && !a_frames.is_empty() {
            cmp.case_pass("cvad.readfault");
        } else {
            cmp.case_fail(
                "cvad.readfault",
                &format!("frames c={c_frames} adapter={a_frames}"),
            );
        }
    } else {
        cmp.case_fail("cvad.readfault", "missing events");
    }
    if c_ref.cases.contains_key("seekfault-flac")
        && adapter_recs.iter().any(|r| r.mode == "adapter.seekfault")
    {
        // the typed class + exact diagnostic are asserted inside the arm;
        // here only the presence of both receipts is compared
        cmp.case_pass("cvad.seekfault");
    } else {
        cmp.case_fail("cvad.seekfault", "missing events");
    }
    for name in ["garbage-nomedia", "garbage-empty"] {
        let c_status = c_ref
            .cases
            .get(name)
            .and_then(|(_, info)| info.iter().find(|(k, _)| k == "garbage").cloned())
            .map(|(_, p)| kv(&p).get("status").cloned().unwrap_or_default());
        let a_msg = adapter_recs
            .iter()
            .find(|r| r.mode == "adapter.open-errors")
            .and_then(|r| {
                r.info
                    .iter()
                    .find(|(k, p)| k == "adapter-open-error" && p.starts_with(&format!("input={name} ")))
                    .cloned()
            })
            .map(|(_, p)| p);
        match (c_status, a_msg) {
            (Some(cs), Some(msg)) => {
                let want = format!("decoder status {cs}");
                if msg.contains(&want) {
                    cmp.case_pass(&format!("cvad.{name}"));
                } else {
                    cmp.case_fail(
                        &format!("cvad.{name}"),
                        &format!("adapter message {:?} lacks {:?}", msg, want),
                    );
                }
            }
            _ => cmp.case_fail(&format!("cvad.{name}"), "missing events"),
        }
    }

    let cmp_pass = cmp.finish();
    let cmp_path = save_log(&ev_dir, "comparison", &cmp);
    summary_cases.push(SummaryCase {
        id: "comparison.c-vs-binding".into(),
        result: if cmp_pass { "PASS" } else { "FAIL" }.into(),
        detail: serde_json::json!({
            "mode_cases": cmp.mode_result.as_ref().map(|(c, f, _)| (*c, *f)),
            "receipt": cmp_path.display().to_string(),
        }),
    });

    // ---- summary ---------------------------------------------------------
    let passed = summary_cases.iter().filter(|c| c.result == "PASS").count();
    let failed = summary_cases.iter().filter(|c| c.result == "FAIL").count();
    let verdict = if failed == 0 { "PASS" } else { "FAIL" };
    write_summary(
        &ev_dir,
        &stamp,
        verdict,
        &artifact_sha,
        &c_ref,
        &summary_cases,
        &raw_all
            .iter()
            .map(|(n, r)| (n.clone(), r.mode_result.map(|(c, f, p)| (c, f, p))))
            .collect::<Vec<_>>(),
        None,
    );
    println!();
    println!("SUITE {verdict} cases={} passed={passed} failed={failed}", summary_cases.len());
    println!("SUMMARY {}", ev_dir.join("summary.json").display());
    if verdict != "PASS" {
        std::process::exit(1);
    }
}

fn kv(payload: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    for token in payload.split_whitespace() {
        if let Some((k, v)) = token.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

fn unix_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[allow(clippy::too_many_arguments)]
fn write_summary(
    ev_dir: &Path,
    stamp: &str,
    verdict: &str,
    artifact_sha: &str,
    c_ref: &CSuiteReference,
    cases: &[SummaryCase],
    _raw_modes: &[(String, Option<(usize, usize, bool)>)],
    abort: Option<String>,
) {
    let doc = serde_json::json!({
        "schema": "songcore-binding-parity-summary/1",
        "stamp": stamp,
        "verdict": verdict,
        "abort": abort,
        "artifact": "native/build/artifacts/libsongcore.a",
        "artifact_sha256": artifact_sha,
        "header_sha256": sys::QN_SONGCORE_HEADER_SHA256,
        "binding_path": "qianqian-songcore-sys (raw) + qianqian-decode-songcore (adapter, kernel-resolved PcmDecode capability); no new binding API",
        "reference_policy": "exact pcm_sha256 from native/experiments/songcore-equivalence/reference.json for ALL fixtures (the corpus's own frozen deterministic hash policy, lossy included); no numeric lossy tolerance exists and none was invented; lossless post-seek drains byte-match the sequential slice at the landing frame",
        "c_suite_reference": {
            "stamp": c_ref.stamp,
            "verdict": c_ref.verdict,
            "path": "evidence/songcore-172/suite-results",
        },
        "cases_total": cases.len(),
        "cases_passed": cases.iter().filter(|c| c.result == "PASS").count(),
        "cases_failed": cases.iter().filter(|c| c.result == "FAIL").count(),
        "cases": cases.iter().map(|c| serde_json::json!({
            "id": c.id,
            "result": c.result,
            "detail": c.detail,
        })).collect::<Vec<_>>(),
    });
    let path = ev_dir.join("summary.json");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap())
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}
