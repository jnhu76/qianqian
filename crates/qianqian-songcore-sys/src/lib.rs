#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use std::ffi::c_char;
use std::os::raw::c_void;

pub const SONGCORE_ABI_VERSION: u32 = 1;

pub const SONG_OK: u32 = 0;
pub const SONG_EOF: u32 = 1;
pub const SONG_ERR_INVALID_ARGUMENT: u32 = 100;
pub const SONG_ERR_STATE: u32 = 101;
pub const SONG_ERR_NOT_OPEN: u32 = 102;
pub const SONG_ERR_IO: u32 = 103;
pub const SONG_ERR_UNSUPPORTED_CONTAINER: u32 = 104;
pub const SONG_ERR_NO_AUDIO_STREAM: u32 = 105;
pub const SONG_ERR_UNSUPPORTED_CODEC: u32 = 106;
pub const SONG_ERR_CORRUPT_DATA: u32 = 107;
pub const SONG_ERR_DECODE_ERROR: u32 = 108;
pub const SONG_ERR_SEEK_UNSUPPORTED: u32 = 109;
pub const SONG_ERR_SEEK_ERROR: u32 = 110;
pub const SONG_ERR_STREAM_CHANGE: u32 = 111;
pub const SONG_ERR_OUT_OF_MEMORY: u32 = 112;
pub const SONG_ERR_INTERNAL_ERROR: u32 = 113;

pub const SONG_CH_FRONT_LEFT: u64 = 1 << 0;
pub const SONG_CH_FRONT_RIGHT: u64 = 1 << 1;
pub const SONG_CH_FRONT_CENTER: u64 = 1 << 2;
pub const SONG_CH_LOW_FREQUENCY: u64 = 1 << 3;
pub const SONG_CH_BACK_LEFT: u64 = 1 << 4;
pub const SONG_CH_BACK_RIGHT: u64 = 1 << 5;
pub const SONG_CH_FRONT_LEFT_OF_CENTER: u64 = 1 << 6;
pub const SONG_CH_FRONT_RIGHT_OF_CENTER: u64 = 1 << 7;
pub const SONG_CH_BACK_CENTER: u64 = 1 << 8;
pub const SONG_CH_SIDE_LEFT: u64 = 1 << 9;
pub const SONG_CH_SIDE_RIGHT: u64 = 1 << 10;
pub const SONG_CH_TOP_CENTER: u64 = 1 << 11;
pub const SONG_CH_TOP_FRONT_LEFT: u64 = 1 << 12;
pub const SONG_CH_TOP_FRONT_CENTER: u64 = 1 << 13;
pub const SONG_CH_TOP_FRONT_RIGHT: u64 = 1 << 14;
pub const SONG_CH_TOP_BACK_LEFT: u64 = 1 << 15;
pub const SONG_CH_TOP_BACK_CENTER: u64 = 1 << 16;
pub const SONG_CH_TOP_BACK_RIGHT: u64 = 1 << 17;
pub const SONG_CH_MASK_UNKNOWN: u64 = 0;
pub const SONG_CH_MONO: u64 = SONG_CH_FRONT_CENTER;
pub const SONG_CH_STEREO: u64 = SONG_CH_FRONT_LEFT | SONG_CH_FRONT_RIGHT;

pub const SONG_METADATA_SCOPE_CONTAINER: u32 = 0;
pub const SONG_METADATA_SCOPE_STREAM: u32 = 1;

pub const SONG_ARTWORK_UNKNOWN: u32 = 0;
pub const SONG_ARTWORK_FRONT_COVER: u32 = 1;
pub const SONG_ARTWORK_BACK_COVER: u32 = 2;
pub const SONG_ARTWORK_OTHER: u32 = 3;

#[repr(C)]
pub struct song_handle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct song_error {
    pub message: *const c_char,
    pub message_len: u32,
    pub native_code: i32,
    pub reserved: u32,
}

pub type song_read_fn = unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> i64;
pub type song_seek_fn = unsafe extern "C" fn(*mut c_void, i64) -> i64;
pub type song_size_fn = unsafe extern "C" fn(*mut c_void) -> i64;

#[repr(C)]
pub struct song_io {
    pub userdata: *mut c_void,
    pub read: Option<song_read_fn>,
    pub seek: Option<song_seek_fn>,
    pub size: Option<song_size_fn>,
}

#[repr(C)]
pub struct song_info {
    pub sample_rate: i32,
    pub channels: i32,
    pub channel_mask: u64,
    pub duration_us: i64,
    pub bits_per_sample: i32,
    pub codec: [c_char; 32],
    pub container: [c_char; 32],
    pub selected_audio_index: u32,
    pub audio_stream_count: u32,
    pub flags: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
pub struct song_stream_info {
    pub audio_index: u32,
    pub stream_index: u32,
    pub sample_rate: i32,
    pub channels: i32,
    pub channel_mask: u64,
    pub duration_us: i64,
    pub bits_per_sample: i32,
    pub codec: [c_char; 32],
    pub is_default: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
pub struct song_metadata {
    pub title: *const c_char,
    pub title_len: u32,
    pub has_title: u32,
    pub artist: *const c_char,
    pub artist_len: u32,
    pub has_artist: u32,
    pub album: *const c_char,
    pub album_len: u32,
    pub has_album: u32,
    pub album_artist: *const c_char,
    pub album_artist_len: u32,
    pub has_album_artist: u32,
    pub genre: *const c_char,
    pub genre_len: u32,
    pub has_genre: u32,
    pub composer: *const c_char,
    pub composer_len: u32,
    pub has_composer: u32,
    pub date: *const c_char,
    pub date_len: u32,
    pub has_date: u32,
    pub comment: *const c_char,
    pub comment_len: u32,
    pub has_comment: u32,
    pub track_number: i32,
    pub has_track_number: u32,
    pub track_total: i32,
    pub has_track_total: u32,
    pub disc_number: i32,
    pub has_disc_number: u32,
    pub disc_total: i32,
    pub has_disc_total: u32,
    pub track_gain_mb: i32,
    pub has_track_gain: u32,
    pub track_peak: u32,
    pub has_track_peak: u32,
    pub album_gain_mb: i32,
    pub has_album_gain: u32,
    pub album_peak: u32,
    pub has_album_peak: u32,
}

#[repr(C)]
pub struct song_metadata_entry {
    pub scope: u32,
    pub key: *const c_char,
    pub key_len: u32,
    pub value: *const c_char,
    pub value_len: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct song_artwork_item {
    pub role: u32,
    pub mime: *const c_char,
    pub mime_len: u32,
    pub data: *const u8,
    pub data_len: u64,
    pub width: i32,
    pub height: i32,
    pub is_front_cover: u32,
    pub reserved: u32,
}

pub const QN_SONGCORE_HEADER_SHA256: &str = env!("QN_SONGCORE_HEADER_SHA256");
pub const QN_SONGCORE_ARTIFACT_SHA256: &str = env!("QN_SONGCORE_ARTIFACT_SHA256");

unsafe extern "C" {
    pub fn songcore_abi_version() -> u32;
    pub fn song_open(io: *const song_io, out_handle: *mut *mut song_handle) -> u32;
    pub fn song_probe(handle: *mut song_handle, out_info: *mut song_info) -> u32;
    pub fn song_audio_stream_count(handle: *mut song_handle, out_count: *mut u32) -> u32;
    pub fn song_audio_stream_info(
        handle: *mut song_handle,
        audio_index: u32,
        out_info: *mut song_stream_info,
    ) -> u32;
    pub fn song_select_stream(handle: *mut song_handle, audio_index: u32) -> u32;
    pub fn song_get_metadata(handle: *mut song_handle, out_meta: *mut *const song_metadata) -> u32;
    pub fn song_get_metadata_count(handle: *mut song_handle, out_count: *mut u32) -> u32;
    pub fn song_get_metadata_entry(
        handle: *mut song_handle,
        index: u32,
        out_entry: *mut song_metadata_entry,
    ) -> u32;
    pub fn song_get_artwork_count(handle: *mut song_handle, out_count: *mut u32) -> u32;
    pub fn song_get_artwork_item(
        handle: *mut song_handle,
        index: u32,
        out_item: *mut song_artwork_item,
    ) -> u32;
    pub fn song_last_error(handle: *mut song_handle, out_error: *mut *const song_error) -> u32;
    pub fn song_read_pcm(
        handle: *mut song_handle,
        dst: *mut f32,
        frame_capacity: u64,
        out_frames_produced: *mut u64,
    ) -> u32;
    pub fn song_seek(
        handle: *mut song_handle,
        requested_position_us: i64,
        out_actual_position_us: *mut i64,
    ) -> u32;
    pub fn song_close(handle: *mut song_handle);
}
