use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use qianqian_songcore_sys::*;

use crate::sha256::Sha256;

struct Song {
    handle: *mut song_handle,
    file: Box<File>,
}

unsafe extern "C" fn host_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    let file = unsafe { &mut *(ud as *mut File) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, size) };
    match file.read(buf) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn host_seek(ud: *mut c_void, offset: i64) -> i64 {
    let file = unsafe { &mut *(ud as *mut File) };
    match file.seek(SeekFrom::Start(offset as u64)) {
        Ok(pos) => pos as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn host_size(ud: *mut c_void) -> i64 {
    let file = unsafe { &mut *(ud as *mut File) };
    let cur = file.stream_position().ok();
    let end = file.seek(SeekFrom::End(0)).ok();
    match (cur, end) {
        (Some(cur), Some(end)) => {
            let _ = file.seek(SeekFrom::Start(cur));
            end as i64
        }
        _ => -1,
    }
}

fn open_song(path: &Path) -> Result<Song, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?;
    let mut song = Song {
        handle: std::ptr::null_mut(),
        file: Box::new(file),
    };
    let io = song_io {
        userdata: &mut *song.file as *mut File as *mut c_void,
        read: Some(host_read),
        seek: Some(host_seek),
        size: Some(host_size),
    };
    let mut handle: *mut song_handle = std::ptr::null_mut();
    let st = unsafe { song_open(&io, &mut handle) };
    if st != SONG_OK {
        return Err(format!("song_open status {}", st));
    }
    song.handle = handle;
    Ok(song)
}

impl Song {
    fn close(&mut self) {
        unsafe { song_close(self.handle) };
    }
}

fn enumerate_snapshot(handle: *mut song_handle) {
    let mut stream_count: u32 = 0;
    if unsafe { song_audio_stream_count(handle, &mut stream_count) } == SONG_OK && stream_count > 0
    {
        let mut sinfo: song_stream_info = unsafe { std::mem::zeroed() };
        let _ = unsafe { song_audio_stream_info(handle, 0, &mut sinfo) };
    }
    let mut meta_count: u32 = 0;
    if unsafe { song_get_metadata_count(handle, &mut meta_count) } == SONG_OK && meta_count > 0 {
        let mut e: song_metadata_entry = unsafe { std::mem::zeroed() };
        let _ = unsafe { song_get_metadata_entry(handle, 0, &mut e) };
    }
    let mut meta: *const song_metadata = std::ptr::null();
    let _ = unsafe { song_get_metadata(handle, &mut meta) };
    let mut art_count: u32 = 0;
    if unsafe { song_get_artwork_count(handle, &mut art_count) } == SONG_OK && art_count > 0 {
        let mut item: song_artwork_item = unsafe { std::mem::zeroed() };
        let _ = unsafe { song_get_artwork_item(handle, 0, &mut item) };
    }
    let mut err: *const song_error = std::ptr::null();
    let _ = unsafe { song_last_error(handle, &mut err) };
}

pub struct CorrectOutcome {
    pub frames: u64,
    pub terminal: i32,
    pub sha_hex: String,
}

pub fn correct(path: &Path, block: u64) -> Result<CorrectOutcome, String> {
    let mut song = open_song(path)?;
    let mut info: song_info = unsafe { std::mem::zeroed() };
    let st = unsafe { song_probe(song.handle, &mut info) };
    if st != SONG_OK {
        song.close();
        return Err(format!("song_probe status {}", st));
    }
    enumerate_snapshot(song.handle);
    let mut buf: Vec<f32> = vec![0.0; block as usize * info.channels as usize];
    let mut sha = Sha256::new();
    let mut frames: u64 = 0;
    let terminal: i32;
    loop {
        let mut got: u64 = 0;
        let st = unsafe { song_read_pcm(song.handle, buf.as_mut_ptr(), block, &mut got) };
        if st == SONG_EOF && got == 0 {
            terminal = SONG_EOF as i32;
            break;
        }
        if st != SONG_OK {
            terminal = st as i32;
            break;
        }
        if got == 0 {
            terminal = SONG_OK as i32;
            break;
        }
        let bytes = unsafe {
            std::slice::from_raw_parts(
                buf.as_ptr() as *const u8,
                got as usize * info.channels as usize * 4,
            )
        };
        sha.update(bytes);
        frames += got;
    }
    let sha_hex = sha.finish_hex();
    song.close();
    Ok(CorrectOutcome {
        frames,
        terminal,
        sha_hex,
    })
}

pub struct SurfaceOutcome {
    pub select_status: i32,
    pub seek_status: i32,
    pub seek_actual_us: i64,
    pub first_frames: u64,
}

pub fn surface(path: &Path) -> Result<SurfaceOutcome, String> {
    let mut song = open_song(path)?;
    let mut info: song_info = unsafe { std::mem::zeroed() };
    let st = unsafe { song_probe(song.handle, &mut info) };
    if st != SONG_OK {
        song.close();
        return Err(format!("song_probe status {}", st));
    }
    let mut select_status: i32 = -1;
    let mut stream_count: u32 = 0;
    if unsafe { song_audio_stream_count(song.handle, &mut stream_count) } == SONG_OK
        && stream_count > 0
    {
        let mut sinfo: song_stream_info = unsafe { std::mem::zeroed() };
        let _ = unsafe { song_audio_stream_info(song.handle, 0, &mut sinfo) };
        select_status = unsafe { song_select_stream(song.handle, 0) } as i32;
        let _ = unsafe { song_probe(song.handle, &mut info) };
    }
    enumerate_snapshot(song.handle);
    let mut actual: i64 = -2;
    let st = unsafe { song_seek(song.handle, 500000, &mut actual) };
    let mut got: u64 = 0;
    if st == SONG_OK {
        let mut buf: Vec<f32> = vec![0.0; 1024 * info.channels as usize];
        let _ = unsafe { song_read_pcm(song.handle, buf.as_mut_ptr(), 1024, &mut got) };
    }
    song.close();
    Ok(SurfaceOutcome {
        select_status,
        seek_status: st as i32,
        seek_actual_us: actual,
        first_frames: got,
    })
}

pub struct SteadyOutcome {
    pub wall_us: f64,
    pub frames: u64,
    pub terminal: i32,
}

pub fn steady(path: &Path, block: u64) -> Result<SteadyOutcome, String> {
    let mut song = open_song(path)?;
    let mut info: song_info = unsafe { std::mem::zeroed() };
    let st = unsafe { song_probe(song.handle, &mut info) };
    if st != SONG_OK {
        song.close();
        return Err(format!("song_probe status {}", st));
    }
    let mut buf: Vec<f32> = vec![0.0; block as usize * info.channels as usize];
    let mut frames: u64 = 0;
    let terminal: i32;
    let t0 = Instant::now();
    loop {
        let mut got: u64 = 0;
        let st = unsafe { song_read_pcm(song.handle, buf.as_mut_ptr(), block, &mut got) };
        if st == SONG_EOF && got == 0 {
            terminal = SONG_EOF as i32;
            break;
        }
        if st != SONG_OK {
            terminal = st as i32;
            break;
        }
        if got == 0 {
            terminal = SONG_OK as i32;
            break;
        }
        frames += got;
    }
    let wall_us = t0.elapsed().as_secs_f64() * 1e6;
    song.close();
    Ok(SteadyOutcome {
        wall_us,
        frames,
        terminal,
    })
}

pub fn latency(path: &Path, block: u64) -> Result<(i32, Vec<f64>), String> {
    let mut song = open_song(path)?;
    let mut info: song_info = unsafe { std::mem::zeroed() };
    let st = unsafe { song_probe(song.handle, &mut info) };
    if st != SONG_OK {
        song.close();
        return Err(format!("song_probe status {}", st));
    }
    let mut buf: Vec<f32> = vec![0.0; block as usize * info.channels as usize];
    let mut call_us: Vec<f64> = Vec::new();
    let terminal: i32;
    loop {
        let mut got: u64 = 0;
        let t0 = Instant::now();
        let st = unsafe { song_read_pcm(song.handle, buf.as_mut_ptr(), block, &mut got) };
        let t1 = Instant::now();
        if st == SONG_EOF && got == 0 {
            terminal = SONG_EOF as i32;
            break;
        }
        if st != SONG_OK {
            terminal = st as i32;
            break;
        }
        if got == 0 {
            terminal = SONG_OK as i32;
            break;
        }
        call_us.push(t1.duration_since(t0).as_secs_f64() * 1e6);
    }
    song.close();
    Ok((terminal, call_us))
}

pub struct TtfpOutcome {
    pub open_us: f64,
    pub probe_us: f64,
    pub first_us: f64,
}

pub fn ttfp(path: &Path, first_block: u64) -> Result<TtfpOutcome, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?;
    let mut buf: Vec<f32> = vec![0.0; first_block as usize * 8];
    let mut song = Song {
        handle: std::ptr::null_mut(),
        file: Box::new(file),
    };
    let io = song_io {
        userdata: &mut *song.file as *mut File as *mut c_void,
        read: Some(host_read),
        seek: Some(host_seek),
        size: Some(host_size),
    };
    let t0 = Instant::now();
    let mut handle: *mut song_handle = std::ptr::null_mut();
    let st = unsafe { song_open(&io, &mut handle) };
    let t1 = Instant::now();
    if st != SONG_OK {
        return Err(format!("song_open status {}", st));
    }
    song.handle = handle;
    let mut info: song_info = unsafe { std::mem::zeroed() };
    let st = unsafe { song_probe(song.handle, &mut info) };
    let t2 = Instant::now();
    let mut got: u64 = 0;
    if st == SONG_OK {
        let _ = unsafe { song_read_pcm(song.handle, buf.as_mut_ptr(), first_block, &mut got) };
    }
    let t3 = Instant::now();
    song.close();
    Ok(TtfpOutcome {
        open_us: t1.duration_since(t0).as_secs_f64() * 1e6,
        probe_us: t2.duration_since(t1).as_secs_f64() * 1e6,
        first_us: t3.duration_since(t2).as_secs_f64() * 1e6,
    })
}

pub fn floor(iterations: u64) -> f64 {
    let t0 = Instant::now();
    let mut sink: u32 = 0;
    for _ in 0..iterations {
        sink = sink.wrapping_add(unsafe { songcore_abi_version() });
    }
    let wall_ns = t0.elapsed().as_secs_f64() * 1e9;
    std::hint::black_box(sink);
    wall_ns / iterations as f64
}
