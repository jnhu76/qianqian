//! ConPTY driver for the reference-player TUI (Stage A dogfood
//! evidence). Spawns the REAL `qianqian-headless.exe play …` under a
//! Windows pseudoconsole, feeds a scripted key sequence, and checks
//! each scenario step against the terminal output the real crossterm/
//! ratatui stack produced. One JSON verdict per scenario; process exit
//! 0 iff GREEN. A wedged step hits the scenario watchdog (the child is
//! terminated and the run reports RED).
//!
//! The driver is presentation-adjacent tooling only: it asserts on the
//! labels the shell is contractually allowed to render (truth classes
//! are pinned by the shell's own unit tests) and on process exit
//! codes. It imports no production crate.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{ClosePseudoConsole, CreatePseudoConsole, COORD, HPCON};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, GetProcessHandleCount,
    GetProcessId, InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTUPINFOEXW, STARTUPINFOW,
};

#[path = "../scenarios.rs"]
mod scenarios;
use scenarios::{scenario, Step};

// ---------------------------------------------------------------- keys

/// VT input encodings for the keys the shell grammar listens for. The
/// pseudoconsole translates these into console input records — exactly
/// what crossterm reads.
pub const ENTER: &str = "\r";
pub const ESC: &str = "\u{1b}";
pub const BACKSPACE: &str = "\u{7f}";
pub const LEFT: &str = "\u{1b}[D";
pub const RIGHT: &str = "\u{1b}[C";

// ------------------------------------------------------------- session

struct Screen {
    text: String,
    raw: Vec<u8>,
}

#[derive(Clone)]
struct ResourceSample {
    label: String,
    threads: u32,
    handles: u32,
    working_set_bytes: u64,
}

struct Session {
    hpc: HPCON,
    process: HANDLE,
    stdin_write: HANDLE,
    screen: Arc<Mutex<Screen>>,
    base_threads: u32,
    base_handles: u32,
    base_ws: u64,
    samples: Vec<ResourceSample>,
}

fn wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

impl Session {
    fn spawn(program: &str, args: &[String], cols: i16, rows: i16) -> Result<Session, String> {
        let mut in_read = HANDLE::default();
        let mut in_write = HANDLE::default();
        let mut out_read = HANDLE::default();
        let mut out_write = HANDLE::default();

        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: true.into(),
        };
        // Input pipe: ConPTY holds the READ end (it consumes our keys);
        // we keep the write end.
        unsafe { CreatePipe(&mut in_read, &mut in_write, Some(&sa), 0) }
            .map_err(|e| format!("CreatePipe(in): {e}"))?;
        // Output pipe: ConPTY holds the WRITE end; we keep the read end.
        unsafe { CreatePipe(&mut out_read, &mut out_write, Some(&sa), 0) }
            .map_err(|e| format!("CreatePipe(out): {e}"))?;

        let hpc = unsafe {
            CreatePseudoConsole(COORD { X: cols, Y: rows }, in_read, out_write, 0)
        }
        .map_err(|e| format!("CreatePseudoConsole: {e}"))?;
        // (The ConPTY ends are closed after CreateProcessW, matching the
        // documented sample order.)

        // Attribute list carrying the pseudoconsole into the child.
        let mut attr_size = 0usize;
        unsafe {
            // Expected to fail with a required-size answer; result ignored.
            let _ = InitializeProcThreadAttributeList(None, 1, None, &mut attr_size);
        }
        let mut attr_buf = vec![0u8; attr_size];
        let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr().cast());
        unsafe { InitializeProcThreadAttributeList(Some(attr_list), 1, None, &mut attr_size) }
            .map_err(|e| format!("InitializeProcThreadAttributeList: {e}"))?;
        unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(hpc.0 as *const std::ffi::c_void),
                std::mem::size_of::<HPCON>(),
                None,
                None,
            )
        }
        .map_err(|e| format!("UpdateProcThreadAttribute: {e}"))?;

        let mut cmdline = String::from(program);
        for a in args {
            cmdline.push_str(" \"");
            cmdline.push_str(a);
            cmdline.push('"');
        }
        let mut cmdline_wide = wide(&cmdline);

        let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        // The EXW struct itself must carry the attribute list — without
        // this assignment the child silently gets no pseudoconsole.
        si.lpAttributeList = attr_list;
        let mut pi = PROCESS_INFORMATION::default();

        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(cmdline_wide.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                None,
                PCWSTR::null(),
                &si as *const STARTUPINFOEXW as *const STARTUPINFOW,
                &mut pi,
            )
        }
        .map_err(|e| format!("CreateProcessW: {e}"))?;

        // The ConPTY holds duplicated ends now; drop ours.
        unsafe {
            let _ = CloseHandle(in_read);
            let _ = CloseHandle(out_write);
        }

        unsafe { DeleteProcThreadAttributeList(attr_list) };

        let screen = Arc::new(Mutex::new(Screen {
            text: String::new(),
            raw: Vec::new(),
        }));
        {
            let screen = Arc::clone(&screen);
            // HANDLE is a raw pointer (not Send); move the raw address.
            let read_end_addr = out_read.0 as usize;
            std::thread::spawn(move || {
                let read_end = HANDLE(read_end_addr as *mut core::ffi::c_void);
                let mut stripper = Stripper::default();
                let mut buf = [0u8; 8192];
                loop {
                    let mut n = 0u32;
                    let ok = unsafe { ReadFile(read_end, Some(&mut buf), Some(&mut n), None) };
                    if ok.is_err() || n == 0 {
                        break;
                    }
                    let mut s = screen.lock().expect("screen lock");
                    s.raw.extend_from_slice(&buf[..n as usize]);
                    stripper.feed(&buf[..n as usize], &mut s.text);
                }
            });
        }

        let mut session = Session {
            hpc,
            process: pi.hProcess,
            stdin_write: in_write,
            screen,
            base_threads: 0,
            base_handles: 0,
            base_ws: 0,
            samples: Vec::new(),
        };
        // Baseline resources: give the child a moment, then measure.
        std::thread::sleep(Duration::from_millis(400));
        let (t, h, w) = session.measure()?;
        session.base_threads = t;
        session.base_handles = h;
        session.base_ws = w;
        session.samples.push(ResourceSample {
            label: "baseline".to_owned(),
            threads: t,
            handles: h,
            working_set_bytes: w,
        });
        Ok(session)
    }

    fn send_keys(&mut self, keys: &str) -> Result<(), String> {
        let bytes = keys.as_bytes();
        let mut written = 0usize;
        while written < bytes.len() {
            let mut n = 0u32;
            unsafe { WriteFile(self.stdin_write, Some(&bytes[written..]), Some(&mut n), None) }
                .map_err(|e| format!("WriteFile(keys): {e}"))?;
            if n == 0 {
                return Err("WriteFile(keys) wrote nothing".to_owned());
            }
            written += n as usize;
        }
        Ok(())
    }

    /// Child resource measurement: threads via the Toolhelp snapshot,
    /// handle count and working set via the child process handle we
    /// own. Bounded, explicit measurements per the campaign — never
    /// "it did not crash, therefore no leak".
    fn measure(&self) -> Result<(u32, u32, u64), String> {
        let pid = unsafe { GetProcessId(self.process) };

        let mut threads = 0u32;
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
                .map_err(|e| format!("CreateToolhelp32Snapshot: {e}"))?;
            let mut entry = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            if Thread32First(snap, &mut entry).is_ok() {
                loop {
                    if entry.th32OwnerProcessID == pid {
                        threads += 1;
                    }
                    if Thread32Next(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }

        let mut handles = 0u32;
        unsafe { GetProcessHandleCount(self.process, &mut handles) }
            .map_err(|e| format!("GetProcessHandleCount: {e}"))?;
        let mut pmc = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let ws_ok = unsafe { K32GetProcessMemoryInfo(self.process, &mut pmc, pmc.cb) };
        if !ws_ok.as_bool() {
            return Err("K32GetProcessMemoryInfo failed".to_owned());
        }
        Ok((threads, handles, pmc.WorkingSetSize as u64))
    }

    fn wait_for(&self, needle: &str, since: usize, within: Duration) -> Result<(), String> {
        let deadline = Instant::now() + within;
        loop {
            let text = self.screen.lock().expect("screen lock").text.clone();
            if text[since.min(text.len())..].contains(needle) {
                return Ok(());
            }
            if Instant::now() > deadline {
                let tail_len = text.len().min(1500);
                let tail = text[text.len() - tail_len..].to_owned();
                return Err(format!(
                    "timeout waiting for {needle:?} after offset {since}; text tail:\n{tail}"
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Every `Position: MM:SS` label in the captured text after
    /// `since`, as (minutes, seconds) pairs.
    fn positions_after(&self, since: usize) -> Vec<(u64, u64)> {
        let text = self.screen.lock().expect("screen lock").text.clone();
        let hay = &text[since.min(text.len())..];
        let needle = "Position: ";
        let mut out = Vec::new();
        let mut rest = hay;
        while let Some(p) = rest.find(needle) {
            let tail = &rest[p + needle.len()..];
            if let Some(colon) = tail.find(':') {
                let mm_text = &tail[..colon];
                let mm: Option<u64> = mm_text.trim().parse().ok();
                let ss: Option<u64> = tail.get(colon + 1..colon + 3).and_then(|s| s.parse().ok());
                if let (Some(mm), Some(ss)) = (mm, ss) {
                    if mm_text.len() <= 3 {
                        out.push((mm, ss));
                    }
                }
            }
            rest = &tail[1.min(tail.len())..];
        }
        out
    }

    fn expect_new_position(
        &self,
        since: usize,
        previous: Option<(u64, u64)>,
        within: Duration,
    ) -> Result<(u64, u64), String> {
        let deadline = Instant::now() + within;
        loop {
            for pos in self.positions_after(since) {
                if Some(pos) != previous {
                    return Ok(pos);
                }
            }
            if Instant::now() > deadline {
                let seen = self.positions_after(since);
                return Err(format!(
                    "timeout waiting for a position sample different from {previous:?}; \
                     post-mark samples: {seen:?}"
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn shutdown(&mut self) {
        unsafe {
            let _ = CloseHandle(self.stdin_write);
            let _ = ClosePseudoConsole(self.hpc);
        }
    }
}

// ------------------------------------------------------- ANSI stripping

/// Incremental stripper keeping printable text and dropping VT/CSI/OSC
/// sequences, so step matching works on a stable plain-text projection.
#[derive(Default)]
struct Stripper {
    state: StripState,
}

#[derive(Default, PartialEq)]
enum StripState {
    #[default]
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

impl Stripper {
    fn feed(&mut self, bytes: &[u8], out: &mut String) {
        for &b in bytes {
            match self.state {
                StripState::Ground => match b {
                    0x1b => self.state = StripState::Esc,
                    0x00..=0x09 | 0x0b | 0x0c => {}
                    0x0a | 0x0d => {}
                    0x20..=0x7e => out.push(b as char),
                    // Other bytes (UTF-8 lead/continuation, box-drawing
                    // runes) pass through so CJK/Unicode path evidence
                    // (§21) stays in the transcript.
                    _ => out.push(b as char),
                },
                StripState::Esc => match b {
                    b'[' => self.state = StripState::Csi,
                    b']' => self.state = StripState::Osc,
                    0x1b => self.state = StripState::Esc,
                    _ => self.state = StripState::Ground,
                },
                StripState::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        self.state = StripState::Ground;
                    }
                }
                StripState::Osc => match b {
                    0x07 => self.state = StripState::Ground,
                    0x1b => self.state = StripState::OscEsc,
                    _ => {}
                },
                StripState::OscEsc => match b {
                    b'\\' => self.state = StripState::Ground,
                    _ => self.state = StripState::Osc,
                },
            }
        }
    }
}

// ----------------------------------------------------------- execution

struct ExecReport {
    verdict: &'static str,
    exit_code: Option<u32>,
    steps: Vec<String>,
    resources: Vec<ResourceSample>,
    failure: Option<String>,
    transcript: String,
}

fn run_scenario(
    headless: &str,
    media_dir: &str,
    files: &[&str],
    steps: Vec<Step>,
    watchdog: Duration,
) -> ExecReport {
    let mut step_log: Vec<String> = Vec::new();
    let deadline = Instant::now() + watchdog;
    let args: Vec<String> = std::iter::once("play".to_owned())
        .chain(
            files
                .iter()
                .map(|f| format!("{media_dir}\\{f}")),
        )
        .collect();
    let mut session = match Session::spawn(headless, &args, 120, 40) {
        Ok(s) => s,
        Err(e) => {
            return ExecReport {
                verdict: "RED",
                exit_code: None,
                steps: vec![format!("spawn: {e}")],
                resources: Vec::new(),
                failure: Some(e),
                transcript: String::new(),
            }
        }
    };

    let mut mark = 0usize;
    let mut last_position: Option<(u64, u64)> = None;
    let mut outcome: Result<Option<u32>, String> = Ok(None);
    let mut exit_code: Option<u32> = None;

    'steps: for step in &steps {
        if Instant::now() > deadline {
            outcome = Err(format!("watchdog ({watchdog:?}) hit at step {step:?}"));
            break 'steps;
        }
        match step {
            Step::Mark => {
                mark = session.screen.lock().expect("screen lock").text.len();
                step_log.push("mark".to_owned());
            }
            Step::Keys(keys) => {
                if let Err(e) = session.send_keys(keys) {
                    outcome = Err(format!("keys: {e}"));
                    break 'steps;
                }
                step_log.push(format!("keys {:?}", keys));
            }
            Step::KeysEach(keys, times, gap_ms) => {
                for _ in 0..*times {
                    if let Err(e) = session.send_keys(keys) {
                        outcome = Err(format!("keys: {e}"));
                        break 'steps;
                    }
                    std::thread::sleep(Duration::from_millis(*gap_ms));
                }
                step_log.push(format!("keys {keys:?} x{times}"));
            }
            Step::SleepMs(ms) => {
                std::thread::sleep(Duration::from_millis(*ms));
                step_log.push(format!("sleep {ms}ms"));
            }
            Step::Expect { text, within_ms } => {
                match session.wait_for(text, 0, Duration::from_millis(*within_ms)) {
                    Ok(()) => step_log.push(format!("expect {text:?} OK")),
                    Err(e) => {
                        outcome = Err(e);
                        break 'steps;
                    }
                }
            }
            Step::ExpectAfterMark { text, within_ms } => {
                match session.wait_for(text, mark, Duration::from_millis(*within_ms)) {
                    Ok(()) => step_log.push(format!("expect-after-mark {text:?} OK")),
                    Err(e) => {
                        outcome = Err(e);
                        break 'steps;
                    }
                }
            }
            Step::ExpectNewPosition { within_ms } => {
                match session.expect_new_position(
                    mark,
                    last_position,
                    Duration::from_millis(*within_ms),
                ) {
                    Ok(pos) => {
                        step_log.push(format!("new-position {pos:?} OK"));
                        last_position = Some(pos);
                    }
                    Err(e) => {
                        outcome = Err(e);
                        break 'steps;
                    }
                }
            }
            Step::RecordPosition => {
                if let Some(p) = session.positions_after(mark).last() {
                    last_position = Some(*p);
                }
                step_log.push(format!("record-position {last_position:?}"));
            }
            Step::AbsentAfterMark(text) => {
                let t = session.screen.lock().expect("screen lock").text.clone();
                if t[mark.min(t.len())..].contains(text) {
                    outcome = Err(format!("forbidden text {text:?} appeared after mark"));
                    break 'steps;
                }
                step_log.push(format!("absent-after-mark {text:?} OK"));
            }
            Step::Resources {
                max_thread_delta,
                max_ws_mb,
            } => match session.measure() {
                Ok((t, h, w)) => {
                    step_log.push(format!(
                        "resources threads={t} (base {} + {max_thread_delta}), \
                         handles={h}, ws={}MB (bound {max_ws_mb}MB)",
                        session.base_threads,
                        w / 1024 / 1024
                    ));
                    if t > session.base_threads + max_thread_delta {
                        outcome = Err(format!(
                            "thread growth: {t} > baseline {} + {max_thread_delta}",
                            session.base_threads
                        ));
                        break 'steps;
                    }
                    if w > u64::from(*max_ws_mb) * 1024 * 1024 {
                        outcome = Err(format!(
                            "working set {}MB > bound {max_ws_mb}MB",
                            w / 1024 / 1024
                        ));
                        break 'steps;
                    }
                    session.samples.push(ResourceSample {
                        label: format!("step#{}", step_log.len()),
                        threads: t,
                        handles: h,
                        working_set_bytes: w,
                    });
                }
                Err(e) => {
                    outcome = Err(format!("resources: {e}"));
                    break 'steps;
                }
            },
            Step::ExpectExit { code, within_ms } => {
                let exit_deadline = Instant::now() + Duration::from_millis(*within_ms);
                let mut got: Option<u32> = None;
                while Instant::now() < exit_deadline {
                    if unsafe { WaitForSingleObject(session.process, 100) } == WAIT_OBJECT_0 {
                        let mut ec = 0u32;
                        if unsafe { GetExitCodeProcess(session.process, &mut ec) }.is_ok() {
                            got = Some(ec);
                            break;
                        }
                    }
                }
                match got {
                    Some(ec) if ec == *code => {
                        step_log.push(format!("exit {ec} OK"));
                        exit_code = Some(ec);
                        // Drain trailing output: the quit report lands on
                        // stdout AFTER the terminal is restored.
                        std::thread::sleep(Duration::from_millis(400));
                    }
                    Some(ec) => {
                        outcome = Err(format!("exit code {ec}, expected {code}"));
                        break 'steps;
                    }
                    None => {
                        outcome = Err(format!("process did not exit within {within_ms}ms"));
                        break 'steps;
                    }
                }
            }
        }
    }

    // The child must not outlive the evidence run on a RED path.
    if outcome.is_err() && exit_code.is_none() {
        unsafe {
            let _ = TerminateProcess(session.process, 42);
            let _ = WaitForSingleObject(session.process, 5000);
        }
    }

    let transcript = session.screen.lock().expect("screen lock").text.clone();
    let resources = std::mem::take(&mut session.samples);
    session.shutdown();

    let (verdict, failure) = match &outcome {
        Ok(_) => ("GREEN", None),
        Err(e) => ("RED", Some(e.clone())),
    };
    ExecReport {
        verdict,
        exit_code,
        steps: step_log,
        resources,
        failure,
        transcript,
    }
}

// ----------------------------------------------------------------- CLI

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn write_evidence(out_dir: &str, name: &str, report: &ExecReport) -> Result<(), String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {out_dir}: {e}"))?;
    let steps = report
        .steps
        .iter()
        .map(|s| format!("    \"{s}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let resources = report
        .resources
        .iter()
        .map(|r| {
            format!(
                "    {{\"label\":\"{}\",\"threads\":{},\"handles\":{},\"working_set_bytes\":{}}}",
                json_escape(&r.label),
                r.threads,
                r.handles,
                r.working_set_bytes
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let failure = report
        .failure
        .as_deref()
        .map(json_escape)
        .unwrap_or_else(|| "null".to_owned());
    let json = format!(
        "{{\"scenario\":\"{name}\",\"verdict\":\"{}\",\"exit_code\":{},\n \
         \"failure\":{failure},\n \"steps\":[\n{steps}\n],\n \"resources\":[\n{resources}\n]}}\n",
        report.verdict,
        report
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "null".to_owned()),
    );
    std::fs::write(format!("{out_dir}/{name}.json"), json)
        .map_err(|e| format!("write json: {e}"))?;
    std::fs::write(format!("{out_dir}/{name}.txt"), &report.transcript)
        .map_err(|e| format!("write transcript: {e}"))?;
    Ok(())
}

fn main() {
    let mut headless = String::from("qianqian-headless.exe");
    let mut media_dir = String::from(".");
    let mut out_dir = String::from("evidence/logs");
    let mut names: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exe" => headless = args.next().expect("--exe value"),
            "--media" => media_dir = args.next().expect("--media value"),
            "--out" => out_dir = args.next().expect("--out value"),
            other => names.push(other.to_owned()),
        }
    }
    if names.is_empty() {
        eprintln!("usage: tuidriver --exe <headless.exe> --media <dir> --out <dir> SCENARIO...");
        std::process::exit(2);
    }

    if names.iter().any(|n| n == "--probe") {
        // Minimal attachment probe: spawn `cmd /c echo PROBE-OK` under
        // the ConPTY and dump whatever the pipe yields.
        let probe_args = vec!["/c echo PROBE-OK && ver".to_owned()];
        let mut s = match Session::spawn("cmd.exe", &probe_args, 80, 25) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("probe spawn failed: {e}");
                std::process::exit(3);
            }
        };
        std::thread::sleep(Duration::from_secs(3));
        {
            let scr = s.screen.lock().unwrap();
            let report = format!(
                "probe: text {} bytes {:?}\nprobe: raw {} bytes {:?}\n",
                scr.text.len(),
                scr.text,
                scr.raw.len(),
                String::from_utf8_lossy(&scr.raw[..scr.raw.len().min(400)])
            );
            eprintln!("{report}");
            let _ = std::fs::write(
                concat!(
                    "C:\\Users\\Public\\qianqian-dogfood\\",
                    "probe-report.txt"
                ),
                &report,
            );
        }
        let (t, h, w) = s.measure().unwrap_or((0, 0, 0));
        eprintln!("probe child resources: threads={t} handles={h} ws={w}");
        unsafe {
            let _ = TerminateProcess(s.process, 0);
        }
        s.shutdown();
        std::process::exit(0);
    }

    let mut any_red = false;
    for name in &names {
        let started = Instant::now();
        let (files, steps, watchdog) = scenario(name);
        let report = run_scenario(&headless, &media_dir, &files, steps, watchdog);
        if let Err(e) = write_evidence(&out_dir, name, &report) {
            eprintln!("{name}: evidence write failed: {e}");
        }
        println!(
            "{}: {} ({}ms, exit {:?})",
            name,
            report.verdict,
            started.elapsed().as_millis(),
            report.exit_code
        );
        for step in &report.steps {
            println!("    {step}");
        }
        if let Some(failure) = &report.failure {
            println!("    FAILURE: {failure}");
        }
        any_red |= report.verdict == "RED";
    }
    std::process::exit(if any_red { 1 } else { 0 });
}
