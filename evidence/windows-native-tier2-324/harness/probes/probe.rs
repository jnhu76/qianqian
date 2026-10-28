use std::ffi::c_void;
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

type Handle = *mut c_void;
const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
const FILE_TYPE_PIPE: u32 = 3;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(id: u32) -> Handle;
    fn SetStdHandle(id: u32, h: Handle) -> i32;
    fn CreatePipe(r: *mut Handle, w: *mut Handle, sa: *mut c_void, size: u32) -> i32;
    fn CloseHandle(h: Handle) -> i32;
    fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, ov: *mut c_void) -> i32;
    fn GetFileType(h: Handle) -> u32;
    fn PeekNamedPipe(h: Handle, buf: *mut u8, blen: u32, bread: *mut u32, avail: *mut u32, left: *mut u32) -> i32;
}

fn main() {
    println!("PROBE pid={}", std::process::id());
    let orig = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    println!("ORIG_HANDLE={:p} TYPE={}", orig, unsafe { GetFileType(orig) });

    // --- A: CreatePipe, close read end, SetStdHandle, std write must fail.
    let (mut r, mut w): (Handle, Handle) = (std::ptr::null_mut(), std::ptr::null_mut());
    let ok = unsafe { CreatePipe(&mut r, &mut w, std::ptr::null_mut(), 0) };
    println!("CREATEPIPE ok={ok} r={r:p} w={w:p}");
    println!("PIPE_TYPE r={} w={}", unsafe { GetFileType(r) }, unsafe { GetFileType(w) });
    // fill the pipe with a single raw write of 4096
    let chunk = vec![b'F'; 4096];
    let mut written: u32 = 0;
    let ok = unsafe { WriteFile(w, chunk.as_ptr(), 4096, &mut written, std::ptr::null_mut()) };
    let mut avail: u32 = 0;
    let peek = unsafe { PeekNamedPipe(r, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut avail, std::ptr::null_mut()) };
    println!("FILL4096 ok={ok} written={written} peek={peek} avail={avail}");
    // 1-byte blocking write with watchdog
    let w2 = w as usize;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let one = [b'x'; 1];
        let mut n: u32 = 0;
        let ok = unsafe { WriteFile(w2 as Handle, one.as_ptr(), 1, &mut n, std::ptr::null_mut()) };
        let _ = tx.send((ok, n, unsafe { GetLastError() }));
    });
    match rx.recv_timeout(Duration::from_millis(400)) {
        Ok(v) => println!("BLOCK1 returned {v:?}"),
        Err(_) => println!("BLOCK1 still-blocked-after-400ms=true"),
    }
    // drain then the blocked write should complete
    let mut drain = vec![0u8; 8192];
    let mut got: u32 = 0;
    let okr = unsafe { ReadFile(r, drain.as_mut_ptr(), 8192, &mut got, std::ptr::null_mut()) };
    println!("DRAIN ok={okr} got={got} (unblocking writer)");
    if let Ok(v) = rx.recv_timeout(Duration::from_secs(2)) {
        println!("BLOCK1 completed-after-drain {v:?}");
    }
    // now close read end and set as stderr
    unsafe { CloseHandle(r); SetStdHandle(STD_ERROR_HANDLE, w); }
    let mut err = std::io::stderr();
    let res = err.write_all(b"canary\n").and_then(|_| err.flush());
    println!("BROKEN_STD_WRITE result={res:?} raw={:?}", res.as_ref().err().and_then(|e| e.raw_os_error()));
    // --- B: stderr lock is process-global?
    unsafe { SetStdHandle(STD_ERROR_HANDLE, orig); }
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let s = std::io::stderr();
        let g = s.lock();
        ready_tx.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(3));
        drop(g);
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut s = std::io::stderr();
        let r = s.write_all(b"locked-canary\n").and_then(|_| s.flush());
        let _ = done_tx.send(r.is_ok());
    });
    match done_rx.recv_timeout(Duration::from_millis(500)) {
        Ok(ok) => println!("LOCK_BLOCKING completed-in-500ms={ok} (lock NOT global)"),
        Err(_) => println!("LOCK_BLOCKING still-blocked-after-500ms=true (lock is global)"),
    }
    println!("PROBE-DONE");
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, ov: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}
