//! ConPTY driver for the reference-player TUI (Stage A dogfood
//! evidence). Spawns the REAL `qianqian-headless.exe play …` under a
//! Windows pseudoconsole, feeds a scripted key sequence, and checks
//! each scenario step against what the real crossterm/ratatui stack
//! actually rendered. One JSON verdict per scenario; process exit 0 iff
//! GREEN. A wedged step hits the scenario watchdog (the child is
//! terminated and the run reports RED).
//!
//! Capture model: the driver emulates the terminal CELL GRID (cursor
//! positioning, erase, alt-screen), and every time the rendered screen
//! changes it appends the full frame's text to an append-only frame
//! history. Step oracles match against that history — a plain VT-stream
//! string strip is NOT sufficient, because ratatui's diff renderer
//! never re-emits unchanged cells (spaces included), so label text is
//! fragmented across cursor moves in the raw stream.
//!
//! The driver is presentation-adjacent tooling only: it asserts on the
//! labels the shell is contractually allowed to render (truth classes
//! are pinned by the shell's own unit tests) and on process exit
//! codes. It imports no production crate.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
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

// ------------------------------------------------- VT grid emulation

/// One terminal cell grid plus the append-only frame history built
/// from it. `history` only ever grows: a frame is appended when the
/// rendered screen differs from the last appended frame.
pub struct Capture {
    grid: Grid,
    last_frame: String,
    pub history: String,
    pub raw: Vec<u8>,
    parser: VtParser,
}

impl Capture {
    fn new(w: usize, h: usize) -> Capture {
        Capture {
            grid: Grid::new(w, h),
            last_frame: String::new(),
            history: String::new(),
            raw: Vec::new(),
            parser: VtParser::default(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.raw.extend_from_slice(bytes);
        self.parser.feed(bytes, &mut self.grid);
        let frame = self.grid.text();
        if frame != self.last_frame {
            self.history.push_str(&frame);
            self.history.push('\n');
            self.last_frame = frame;
        }
    }
}

struct Grid {
    w: usize,
    h: usize,
    cells: Vec<char>,
    cx: usize,
    cy: usize,
}

/// Zero-dependency wide-glyph width. The campaign's wide fixtures are
/// CJK; the ranges below are the common East Asian Wide/Fullwidth
/// blocks. Harness approximation, documented in the README (H-11):
/// ConPTY advances the real cursor by two columns for these glyphs and
/// the grid must mirror that or every later diff frame of the screen
/// corrupts.
fn char_width(c: char) -> usize {
    let cp = c as u32;
    if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x20000..=0x2FFFD).contains(&cp)
        || (0x30000..=0x3FFFD).contains(&cp)
    {
        2
    } else {
        1
    }
}

impl Grid {
    fn new(w: usize, h: usize) -> Grid {
        Grid {
            w,
            h,
            cells: vec![' '; w * h],
            cx: 0,
            cy: 0,
        }
    }

    fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = ' ');
        self.cx = 0;
        self.cy = 0;
    }

    fn put(&mut self, c: char) {
        if self.cx >= self.w {
            self.cx = 0;
            self.advance_row();
        }
        let idx = self.cy * self.w + self.cx;
        self.cells[idx] = c;
        let width = char_width(c);
        // A wide glyph occupies TWO columns: the skip cell stays a
        // blank and the cursor advances by the glyph width, exactly
        // like the real terminal. Without this the parser's column
        // model falls behind ConPTY's on every wide char and ALL
        // subsequent diff frames of that screen corrupt (run-PROBE
        // lesson, H-11).
        if width == 2 && self.cx + 1 < self.w {
            self.cells[idx + 1] = ' ';
        }
        self.cx += width;
    }

    fn advance_row(&mut self) {
        self.cy += 1;
        if self.cy >= self.h {
            // Scroll: drop the first row, add a blank last row.
            self.cells.drain(..self.w);
            self.cells.resize(self.w * self.h, ' ');
            self.cy = self.h - 1;
        }
    }

    /// The rendered screen: rows with trailing whitespace stripped,
    /// joined with newlines.
    fn text(&self) -> String {
        let mut rows = Vec::with_capacity(self.h);
        for row in 0..self.h {
            let line: String = self.cells[row * self.w..(row + 1) * self.w]
                .iter()
                .collect();
            rows.push(line.trim_end().to_owned());
        }
        rows.join("\n")
    }

    fn erase_in_display(&mut self, mode: u32) {
        match mode {
            0 => {
                let start = self.cy * self.w + self.cx.min(self.w - 1);
                self.cells[start..].iter_mut().for_each(|c| *c = ' ');
            }
            1 => {
                let end = (self.cy * self.w + self.cx.min(self.w - 1)).min(self.cells.len());
                self.cells[..=end].iter_mut().for_each(|c| *c = ' ');
            }
            _ => self.clear(),
        }
    }

    fn erase_in_line(&mut self, mode: u32) {
        let row = self.cy * self.w;
        match mode {
            0 => {
                let start = row + self.cx.min(self.w);
                self.cells[start..row + self.w]
                    .iter_mut()
                    .for_each(|c| *c = ' ');
            }
            1 => {
                let end = row + self.cx.min(self.w - 1);
                self.cells[row..=end].iter_mut().for_each(|c| *c = ' ');
            }
            _ => self.cells[row..row + self.w]
                .iter_mut()
                .for_each(|c| *c = ' '),
        }
    }
}

/// Minimal VT input parser: just enough of ECMA-48/DEC to keep the cell
/// grid honest for a ratatui/crossterm application (CUP, CUU/D/F/B,
/// ED, EL, CHA, VPA, SGR-ignored, OSC-ignored, alt-screen enter).
#[derive(Default)]
struct VtParser {
    state: VtState,
    params: String,
    utf8: Vec<u8>,
}

#[derive(Default, PartialEq)]
enum VtState {
    #[default]
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

impl VtParser {
    fn param1(&self, index: usize, default: u32) -> u32 {
        self.params
            .split(|c| c == ';' || c == ':')
            .nth(index)
            .and_then(|p| {
                if p.is_empty() {
                    None
                } else {
                    p.parse::<u32>().ok()
                }
            })
            .filter(|v| *v > 0)
            .unwrap_or(default)
    }

    fn apply_csi(&mut self, grid: &mut Grid, final_byte: u8) {
        let private = self.params.starts_with('?');
        let n = |i: usize| self.param1(i, 1) as usize;
        match final_byte {
            b'H' | b'f' if !private => {
                grid.cy = (n(0) - 1).min(grid.h - 1);
                grid.cx = (n(1) - 1).min(grid.w - 1);
            }
            b'A' => grid.cy = grid.cy.saturating_sub(n(0)),
            b'B' => grid.cy = (grid.cy + n(0)).min(grid.h - 1),
            b'C' => grid.cx = (grid.cx + n(0)).min(grid.w - 1),
            b'D' => grid.cx = grid.cx.saturating_sub(n(0)),
            b'G' => grid.cx = (n(0) - 1).min(grid.w - 1),
            b'd' => grid.cy = (n(0) - 1).min(grid.h - 1),
            b'J' if !private => {
                let mode = self.param1(0, 0);
                grid.erase_in_display(mode);
            }
            b'K' if !private => {
                let mode = self.param1(0, 0);
                grid.erase_in_line(mode);
            }
            b'h' | b'l' if private => {
                // Alt-screen enter clears our grid; leave is ignored (the
                // post-restore transcript stays in history anyway).
                if final_byte == b'h'
                    && (self.params.contains("1049") || self.params.contains("1047"))
                {
                    grid.clear();
                }
            }
            _ => {}
        }
    }

    fn feed(&mut self, bytes: &[u8], grid: &mut Grid) {
        for &b in bytes {
            match self.state {
                VtState::Ground => match b {
                    0x1b => {
                        self.state = VtState::Esc;
                        self.params.clear();
                    }
                    b'\r' => grid.cx = 0,
                    b'\n' => grid.advance_row(),
                    0x08 => grid.cx = grid.cx.saturating_sub(1),
                    0x20..=0x7e => grid.put(b as char),
                    0x00..=0x1f | 0x7f => {}
                    _ => {
                        // UTF-8: assemble multibyte sequences so CJK /
                        // Unicode path text lands as real chars.
                        self.utf8.push(b);
                        let expected = utf8_len(self.utf8[0]);
                        if expected == 0 {
                            self.utf8.clear();
                        } else if self.utf8.len() == expected {
                            if let Ok(s) = std::str::from_utf8(&self.utf8) {
                                for c in s.chars() {
                                    grid.put(c);
                                }
                            }
                            self.utf8.clear();
                        }
                    }
                },
                VtState::Esc => match b {
                    b'[' => self.state = VtState::Csi,
                    b']' => self.state = VtState::Osc,
                    0x1b => self.state = VtState::Esc,
                    _ => self.state = VtState::Ground,
                },
                VtState::Csi => match b {
                    b'0'..=b'9' | b';' | b':' | b'?' | b' ' => self.params.push(b as char),
                    0x40..=0x7e => {
                        let final_byte = b;
                        let params = std::mem::take(&mut self.params);
                        self.params = params;
                        self.apply_csi(grid, final_byte);
                        self.params.clear();
                        self.state = VtState::Ground;
                    }
                    _ => {
                        self.state = VtState::Ground;
                        self.params.clear();
                    }
                },
                VtState::Osc => match b {
                    0x07 => self.state = VtState::Ground,
                    0x1b => self.state = VtState::OscEsc,
                    _ => {}
                },
                VtState::OscEsc => match b {
                    b'\\' => self.state = VtState::Ground,
                    _ => self.state = VtState::Osc,
                },
            }
        }
    }
}

fn utf8_len(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 0,
    }
}

// ------------------------------------------------------------- session

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
    capture: Arc<Mutex<Capture>>,
    chrono: ChronoHandle,
    cols: i16,
    rows: i16,
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
        unsafe { CreatePipe(&mut in_read, &mut in_write, Some(&sa), 0) }
            .map_err(|e| format!("CreatePipe(in): {e}"))?;
        unsafe { CreatePipe(&mut out_read, &mut out_write, Some(&sa), 0) }
            .map_err(|e| format!("CreatePipe(out): {e}"))?;

        let hpc = unsafe { CreatePseudoConsole(COORD { X: cols, Y: rows }, in_read, out_write, 0) }
            .map_err(|e| format!("CreatePseudoConsole: {e}"))?;

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

        // The child's working directory is pinned to the program's own
        // directory: relative O-dialog candidates must resolve against
        // the staging dir regardless of how the detached driver itself
        // was launched (Start-Process from a Linux-cwd shell otherwise
        // lands the whole chain in C:\Windows\System32, and every open
        // honestly fails with os error 2).
        let child_cwd = program
            .rsplit_once('\\')
            .map(|(dir, _)| dir.to_owned())
            .unwrap_or_else(|| ".".to_owned());
        let cwd_wide = wide(&child_cwd);

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
                PCWSTR(cwd_wide.as_ptr()),
                &si as *const STARTUPINFOEXW as *const STARTUPINFOW,
                &mut pi,
            )
        }
        .map_err(|e| format!("CreateProcessW: {e}"))?;

        // The ConPTY holds its ends; ours may go now.
        unsafe {
            let _ = CloseHandle(in_read);
            let _ = CloseHandle(out_write);
        }
        unsafe { DeleteProcThreadAttributeList(attr_list) };

        let capture = Arc::new(Mutex::new(Capture::new(cols as usize, rows as usize)));
        let chrono: ChronoHandle = Arc::new(Mutex::new(Chronology::new()));
        chrono
            .lock()
            .expect("chrono lock")
            .log("spawn", &format!("cwd={child_cwd} cmd={cmdline}"));
        {
            let capture = Arc::clone(&capture);
            let chrono = Arc::clone(&chrono);
            // HANDLE is a raw pointer (not Send); move the raw address.
            let read_end_addr = out_read.0 as usize;
            std::thread::spawn(move || {
                let read_end = HANDLE(read_end_addr as *mut core::ffi::c_void);
                let mut buf = [0u8; 8192];
                loop {
                    let mut n = 0u32;
                    let ok = unsafe { ReadFile(read_end, Some(&mut buf), Some(&mut n), None) };
                    if ok.is_err() || n == 0 {
                        chrono
                            .lock()
                            .expect("chrono lock")
                            .log("recv-eof", &format!("ok={:?} n={n}", ok.is_ok()));
                        break;
                    }
                    capture
                        .lock()
                        .expect("capture lock")
                        .feed(&buf[..n as usize]);
                    chrono
                        .lock()
                        .expect("chrono lock")
                        .log("recv", &format!("{} bytes", n));
                }
            });
        }

        let mut session = Session {
            hpc,
            process: pi.hProcess,
            stdin_write: in_write,
            capture,
            chrono,
            cols,
            rows,
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

    /// Force the app's next draws to be full repaints: the diff
    /// renderer resends only cells it believes changed, so the
    /// reconstructed grid can carry stale characters wherever new text
    /// aligns over old text (a `[1C` cursor jump over an unchanged
    /// cell) — substring oracles over such a grid are unsafe at diff
    /// boundaries. A pseudoconsole resize makes ratatui repaint every
    /// cell, restoring a coherent grid. Narrower-first so frames never
    /// exceed the emulated grid width.
    fn full_repaint(&self) {
        unsafe {
            let _ = ResizePseudoConsole(
                self.hpc,
                COORD {
                    X: self.cols - 1,
                    Y: self.rows,
                },
            );
            std::thread::sleep(Duration::from_millis(80));
            let _ = ResizePseudoConsole(
                self.hpc,
                COORD {
                    X: self.cols,
                    Y: self.rows,
                },
            );
        }
    }

    fn send_keys(&mut self, keys: &str) -> Result<(), String> {
        let bytes = keys.as_bytes();
        let mut written = 0usize;
        while written < bytes.len() {
            let mut n = 0u32;
            unsafe {
                WriteFile(
                    self.stdin_write,
                    Some(&bytes[written..]),
                    Some(&mut n),
                    None,
                )
            }
            .map_err(|e| format!("WriteFile(keys): {e}"))?;
            if n == 0 {
                self.chrono
                    .lock()
                    .expect("chrono lock")
                    .log("send-FAIL", "wrote nothing");
                return Err("WriteFile(keys) wrote nothing".to_owned());
            }
            written += n as usize;
        }
        self.chrono
            .lock()
            .expect("chrono lock")
            .log("send", &format!("{} bytes: {keys:?}", bytes.len()));
        Ok(())
    }

    /// True while the child has not exited. Non-blocking.
    fn is_alive(&self) -> bool {
        matches!(
            unsafe { WaitForSingleObject(self.process, 0) },
            WAIT_TIMEOUT
        )
    }

    /// The child's exit code; meaningful only after death (a live
    /// process reports STILL_ACTIVE).
    fn exit_code_now(&self) -> Option<u32> {
        let mut ec = 0u32;
        unsafe { GetExitCodeProcess(self.process, &mut ec) }
            .ok()
            .map(|_| ec)
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

    fn history(&self) -> String {
        self.capture.lock().expect("capture lock").history.clone()
    }

    fn wait_for(&self, needle: &str, since: usize, within: Duration) -> Result<(), String> {
        self.chrono
            .lock()
            .expect("chrono lock")
            .log("wait-begin", &format!("{needle:?} within {within:?}"));
        let deadline = Instant::now() + within;
        loop {
            let history = self.history();
            if history[since.min(history.len())..].contains(needle) {
                self.chrono
                    .lock()
                    .expect("chrono lock")
                    .log("wait-ok", needle);
                return Ok(());
            }
            // A dead child can never satisfy the wait — fail fast with
            // its exit code instead of burning the window on a corpse.
            if !self.is_alive() {
                let code = self.exit_code_now();
                self.chrono.lock().expect("chrono lock").log(
                    "wait-DEAD",
                    &format!("child exited {code:?} awaiting {needle:?}"),
                );
                return Err(format!(
                    "child exited (code {code:?}) while waiting for {needle:?}"
                ));
            }
            if Instant::now() > deadline {
                // Char-boundary-safe tail: the history contains
                // multi-byte glyphs (box drawing, CJK paths).
                let mut start = history.len().saturating_sub(1500);
                while start < history.len() && !history.is_char_boundary(start) {
                    start += 1;
                }
                let tail = history[start..].to_owned();
                self.chrono
                    .lock()
                    .expect("chrono lock")
                    .log("wait-TIMEOUT", &format!("{needle:?} after offset {since}"));
                return Err(format!(
                    "timeout waiting for {needle:?} after offset {since}; history tail:\n{tail}"
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Every `Position: MM:SS` label in the frame history after
    /// `since`, as (minutes, seconds) pairs.
    fn positions_after(&self, since: usize) -> Vec<(u64, u64)> {
        let history = self.history();
        let hay = &history[since.min(history.len())..];
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
        self.chrono
            .lock()
            .expect("chrono lock")
            .log("wait-begin", &format!("new-position != {previous:?}"));
        let deadline = Instant::now() + within;
        loop {
            for pos in self.positions_after(since) {
                if Some(pos) != previous {
                    self.chrono
                        .lock()
                        .expect("chrono lock")
                        .log("wait-ok", &format!("new-position {pos:?}"));
                    return Ok(pos);
                }
            }
            if !self.is_alive() {
                let code = self.exit_code_now();
                self.chrono.lock().expect("chrono lock").log(
                    "wait-DEAD",
                    &format!("child exited {code:?} awaiting position"),
                );
                return Err(format!(
                    "child exited (code {code:?}) while waiting for a position sample"
                ));
            }
            if Instant::now() > deadline {
                let seen = self.positions_after(since);
                self.chrono
                    .lock()
                    .expect("chrono lock")
                    .log("wait-TIMEOUT", &format!("new-position != {previous:?}"));
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

// ----------------------------------------------------------- execution

/// Wall-clock chronology of one scenario: spawn, key writes, output
/// chunks, wait outcomes, child death. This is the primary wedge
/// classifier — a silent app with successful key writes and a live
/// process is a different failure class than a dead child or a failed
/// input write.
struct Chronology {
    t0: Instant,
    lines: Vec<String>,
}

type ChronoHandle = Arc<Mutex<Chronology>>;

impl Chronology {
    fn new() -> Self {
        Self {
            t0: Instant::now(),
            lines: Vec::new(),
        }
    }

    fn log(&mut self, kind: &str, detail: &str) {
        self.lines.push(format!(
            "T+{:8.3}s {:>10}  {}",
            self.t0.elapsed().as_secs_f64(),
            kind,
            detail
        ));
    }
}

struct ExecReport {
    verdict: &'static str,
    exit_code: Option<u32>,
    steps: Vec<String>,
    resources: Vec<ResourceSample>,
    failure: Option<String>,
    /// Child state at RED, captured BEFORE the cleanup TerminateProcess
    /// (alive+threads, or the observed exit code).
    child_status: Option<String>,
    chronology: String,
    transcript: String,
    raw: Vec<u8>,
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
    // U1 (Issue #166): an EMPTY file list means the no-argument launch
    // itself — the child is spawned with zero arguments. Any non-empty
    // list keeps the frozen `play` grammar.
    let args: Vec<String> = if files.is_empty() {
        Vec::new()
    } else {
        std::iter::once("play".to_owned())
            .chain(files.iter().map(|f| format!("{media_dir}\\{f}")))
            .collect()
    };
    let mut session = match Session::spawn(headless, &args, 120, 40) {
        Ok(s) => s,
        Err(e) => {
            return ExecReport {
                verdict: "RED",
                exit_code: None,
                steps: vec![format!("spawn: {e}")],
                resources: Vec::new(),
                failure: Some(e),
                child_status: None,
                chronology: String::new(),
                transcript: String::new(),
                raw: Vec::new(),
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
                mark = session.capture.lock().expect("capture lock").history.len();
                step_log.push("mark".to_owned());
            }
            Step::Keys(keys) => {
                if let Err(e) = session.send_keys(keys) {
                    outcome = Err(format!("keys: {e}"));
                    break 'steps;
                }
                session.full_repaint();
                step_log.push(format!("keys {:?}", keys));
            }
            Step::Typed(text) => {
                if let Err(e) = session.send_keys(text) {
                    outcome = Err(format!("keys: {e}"));
                    break 'steps;
                }
                session.full_repaint();
                step_log.push(format!("keys {text:?}"));
            }
            Step::KeysEach(keys, times, gap_ms) => {
                for _ in 0..*times {
                    if let Err(e) = session.send_keys(keys) {
                        outcome = Err(format!("keys: {e}"));
                        break 'steps;
                    }
                    std::thread::sleep(Duration::from_millis(*gap_ms));
                }
                session.full_repaint();
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
            Step::ExpectEither { a, b, within_ms } => {
                let wait = Duration::from_millis(*within_ms);
                match session
                    .wait_for(a, 0, wait)
                    .or_else(|_| session.wait_for(b, 0, wait))
                {
                    Ok(()) => step_log.push(format!("expect-either {a:?} | {b:?} OK")),
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
                let history = session.history();
                if history[mark.min(history.len())..].contains(text) {
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
            Step::Resize { cols, rows } => {
                // One shot to the target size (never larger than the
                // captured grid, which the scenario discipline keeps
                // true by only shrinking from the spawn size), then a
                // settle sleep: the child's crossterm delivers the
                // resize event and the NEXT draw lays out at the new
                // size. The following key press triggers the harness's
                // full-repaint jiggle, so subsequent expects see a
                // coherent grid at the new width.
                unsafe {
                    let _ = ResizePseudoConsole(session.hpc, COORD { X: *cols, Y: *rows });
                }
                session.cols = *cols;
                session.rows = *rows;
                session
                    .chrono
                    .lock()
                    .expect("chrono lock")
                    .log("resize", &format!("pseudoconsole -> {cols}x{rows}"));
                std::thread::sleep(Duration::from_millis(250));
                step_log.push(format!("resize {}x{}", cols, rows));
            }
        }
    }

    // The child must not outlive the evidence run on a RED path — but
    // its state AT failure is evidence, captured before the cleanup.
    let child_status = if outcome.is_err() && exit_code.is_none() {
        if session.is_alive() {
            let threads = session.measure().map(|(t, _, _)| t).ok();
            Some(format!("alive=true threads={threads:?}"))
        } else {
            Some(format!(
                "alive=false exit_code={:?}",
                session.exit_code_now()
            ))
        }
    } else {
        None
    };
    if outcome.is_err() && exit_code.is_none() {
        unsafe {
            let _ = TerminateProcess(session.process, 42);
            let _ = WaitForSingleObject(session.process, 5000);
        }
    }

    let (transcript, raw) = {
        let cap = session.capture.lock().expect("capture lock");
        (cap.history.clone(), cap.raw.clone())
    };
    let chronology = session.chrono.lock().expect("chrono lock").lines.join("\n");
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
        child_status,
        chronology,
        transcript,
        raw,
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
        .map(|s| format!("    \"{}\"", json_escape(s)))
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
        .map(|f| format!("\"{}\"", json_escape(f)))
        .unwrap_or_else(|| "null".to_owned());
    let child_status = report
        .child_status
        .as_deref()
        .map(|c| format!("\"{}\"", json_escape(c)))
        .unwrap_or_else(|| "null".to_owned());
    let json = format!(
        "{{\"scenario\":\"{name}\",\"verdict\":\"{}\",\"exit_code\":{},\n \
         \"failure\":{failure},\n \"child_status_at_failure\":{child_status},\n \
         \"steps\":[\n{steps}\n],\n \"resources\":[\n{resources}\n]}}\n",
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
    std::fs::write(format!("{out_dir}/{name}.raw.txt"), &report.raw)
        .map_err(|e| format!("write raw: {e}"))?;
    std::fs::write(format!("{out_dir}/{name}.chrono.txt"), &report.chronology)
        .map_err(|e| format!("write chronology: {e}"))?;
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

    // Detached launches lose the hidden console's stderr; any panic
    // must leave its evidence in the out directory.
    let panic_path = format!("{out_dir}/panic.txt");
    std::fs::create_dir_all(&out_dir).ok();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(
            &panic_path,
            format!("panic: {info}\nbacktrace disabled (release)\n"),
        );
    }));

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
            let cap = s.capture.lock().unwrap();
            let report = format!(
                "probe: history {} bytes\nprobe: raw {} bytes {:?}\n",
                cap.history.len(),
                cap.raw.len(),
                String::from_utf8_lossy(&cap.raw[..cap.raw.len().min(400)])
            );
            eprintln!("{report}");
            let _ = std::fs::write(
                "C:\\Users\\Public\\qianqian-dogfood\\probe-report.txt",
                &report,
            );
        }
        unsafe {
            let _ = TerminateProcess(s.process, 0);
        }
        s.shutdown();
        std::process::exit(0);
    }

    let mut any_red = false;
    let mut summary = String::new();
    for name in &names {
        let started = Instant::now();
        let (files, steps, watchdog) = scenario(name, &media_dir);
        let report = run_scenario(&headless, &media_dir, &files, steps, watchdog);
        if let Err(e) = write_evidence(&out_dir, name, &report) {
            eprintln!("{name}: evidence write failed: {e}");
        }
        let line = format!(
            "{}: {} ({}ms, exit {:?})",
            name,
            report.verdict,
            started.elapsed().as_millis(),
            report.exit_code
        );
        println!("{line}");
        summary.push_str(&line);
        summary.push('\n');
        for step in &report.steps {
            println!("    {step}");
            summary.push_str("    ");
            summary.push_str(step);
            summary.push('\n');
        }
        if let Some(failure) = &report.failure {
            println!("    FAILURE: {failure}");
            summary.push_str(&format!("    FAILURE: {failure}\n"));
        }
        if let Some(child) = &report.child_status {
            println!("    CHILD: {child}");
            summary.push_str(&format!("    CHILD: {child}\n"));
        }
        any_red |= report.verdict == "RED";
    }
    // The runner starts this process detached (a fresh console); its
    // stdout is not captured, so the per-invocation summary also lands
    // in the evidence directory for the runner to read.
    let _ = std::fs::write(format!("{out_dir}/summary.txt"), &summary);
    std::process::exit(if any_red { 1 } else { 0 });
}
