/*
 * songcore_wasm_bridge.c — the thin WASM door into SongCore.
 *
 * Host-owned IO, unchanged from the native architecture:
 *
 *   Host file / memory / browser File
 *       ↓ host read / seek64 / size64        (qianqian_host imports)
 *   songcore_wasm_bridge
 *       ↓ song_io (SongCore contract)
 *   SongCore (songcore_ffmpeg.c)
 *       ↓ FFmpeg custom AVIO
 *
 * Rules enforced here:
 *   - FFmpeg types never appear above this file;
 *   - runtime types never appear in the SongCore contract;
 *   - the source handle stays opaque (int64_t); 64-bit offsets preserved;
 *   - this file exists only for WASM targets; native SongCore consumers use
 *     song_open/song_io directly and never link this TU.
 *
 * Build as part of the SongCore.wasm reactor module. The exported surface is
 * the SongCore ABI v1 contract with an explicit int64 handle. Every export
 * is a 1:1 delegation to the native contract; struct outputs are written
 * into caller-provided WASM memory.
 */

#include "qn_host_imports.h"
#include "songcore.h"

/* --- SongCore contract adapters over the host door ------------------- */

static int64_t bridge_read(void *userdata, uint8_t *dst, size_t size) {
    /* one chunk stays within the wasm32 address/size space; FFmpeg AVIO
     * reads arrive in <= buffer-size chunks (32 KiB here) */
    if (size > (size_t)INT32_MAX) size = (size_t)INT32_MAX;
    return qn_host_read((int64_t)(intptr_t)userdata, dst, (int32_t)size);
}

static int64_t bridge_seek(void *userdata, int64_t absolute_offset) {
    return qn_host_seek((int64_t)(intptr_t)userdata, absolute_offset);
}

static int64_t bridge_size(void *userdata) {
    return qn_host_size((int64_t)(intptr_t)userdata);
}

static song_handle *as_handle(int64_t song) {
    return (song_handle *)(intptr_t)song;
}

/* --- exported contract surface (SongCore ABI v1) --------------------- */

QN_EXPORT("song_wasm_abi_version")
uint32_t song_wasm_abi_version(void) {
    return songcore_abi_version();
}

QN_EXPORT("song_wasm_open")
int32_t song_wasm_open(int64_t host_handle, int64_t *out_song) {
    song_io io = {
        .userdata = (void *)(intptr_t)host_handle,
        .read = bridge_read,
        .seek = bridge_seek,
        .size = bridge_size,
    };
    song_handle *song = NULL;
    song_status st = song_open(&io, &song);
    if (out_song) *out_song = (int64_t)(intptr_t)song;
    return (int32_t)st;
}

QN_EXPORT("song_wasm_probe")
int32_t song_wasm_probe(int64_t song, song_info *out_info) {
    return (int32_t)song_probe(as_handle(song), out_info);
}

QN_EXPORT("song_wasm_audio_stream_count")
int32_t song_wasm_audio_stream_count(int64_t song, uint32_t *out_count) {
    return (int32_t)song_audio_stream_count(as_handle(song), out_count);
}

QN_EXPORT("song_wasm_audio_stream_info")
int32_t song_wasm_audio_stream_info(int64_t song, uint32_t audio_index,
                                    song_stream_info *out_info) {
    return (int32_t)song_audio_stream_info(as_handle(song), audio_index,
                                           out_info);
}

QN_EXPORT("song_wasm_select_stream")
int32_t song_wasm_select_stream(int64_t song, uint32_t audio_index) {
    return (int32_t)song_select_stream(as_handle(song), audio_index);
}

QN_EXPORT("song_wasm_get_metadata")
int32_t song_wasm_get_metadata(int64_t song, const song_metadata **out_meta) {
    return (int32_t)song_get_metadata(as_handle(song), out_meta);
}

QN_EXPORT("song_wasm_get_metadata_count")
int32_t song_wasm_get_metadata_count(int64_t song, uint32_t *out_count) {
    return (int32_t)song_get_metadata_count(as_handle(song), out_count);
}

QN_EXPORT("song_wasm_get_metadata_entry")
int32_t song_wasm_get_metadata_entry(int64_t song, uint32_t index,
                                     song_metadata_entry *out_entry) {
    return (int32_t)song_get_metadata_entry(as_handle(song), index, out_entry);
}

QN_EXPORT("song_wasm_get_artwork_count")
int32_t song_wasm_get_artwork_count(int64_t song, uint32_t *out_count) {
    return (int32_t)song_get_artwork_count(as_handle(song), out_count);
}

QN_EXPORT("song_wasm_get_artwork_item")
int32_t song_wasm_get_artwork_item(int64_t song, uint32_t index,
                                   song_artwork_item *out_item) {
    return (int32_t)song_get_artwork_item(as_handle(song), index, out_item);
}

QN_EXPORT("song_wasm_read_pcm")
int32_t song_wasm_read_pcm(int64_t song, float *dst, int32_t frame_capacity,
                           int64_t *out_frames_produced) {
    uint64_t produced = 0;
    song_status st = song_read_pcm(as_handle(song), dst,
                                   (uint64_t)frame_capacity, &produced);
    if (out_frames_produced) *out_frames_produced = (int64_t)produced;
    return (int32_t)st;
}

QN_EXPORT("song_wasm_seek")
int32_t song_wasm_seek(int64_t song, int64_t position_us,
                       int64_t *out_actual_position_us) {
    return (int32_t)song_seek(as_handle(song), position_us,
                              out_actual_position_us);
}

QN_EXPORT("song_wasm_last_error")
int32_t song_wasm_last_error(int64_t song, const song_error **out_error) {
    return (int32_t)song_last_error(as_handle(song), out_error);
}

QN_EXPORT("song_wasm_close")
void song_wasm_close(int64_t song) {
    song_close(as_handle(song));
}
