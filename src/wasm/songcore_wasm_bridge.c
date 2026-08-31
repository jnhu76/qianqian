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
 * Build as part of the SongCore.wasm reactor module; the exported surface is
 * the SongCore contract with an explicit handle: song_wasm_open /
 * song_wasm_probe / song_wasm_read_pcm / song_wasm_seek / song_wasm_close.
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

/* --- exported contract surface --------------------------------------- */

QN_EXPORT("song_wasm_open")
song_handle *song_wasm_open(int64_t handle) {
    song_io io = {
        .userdata = (void *)(intptr_t)handle,
        .read = bridge_read,
        .seek = bridge_seek,
        .size = bridge_size,
    };
    return song_open(&io);
}

QN_EXPORT("song_wasm_probe")
int song_wasm_probe(song_handle *handle, song_info *out_info) {
    return song_probe(handle, out_info);
}

QN_EXPORT("song_wasm_read_pcm")
int64_t song_wasm_read_pcm(song_handle *handle, float *output, int32_t frame_capacity) {
    if (frame_capacity < 0) return -1;
    return song_read_pcm(handle, output, (size_t)frame_capacity);
}

QN_EXPORT("song_wasm_seek")
int song_wasm_seek(song_handle *handle, int64_t position_us) {
    return song_seek(handle, position_us);
}

QN_EXPORT("song_wasm_close")
void song_wasm_close(song_handle *handle) {
    song_close(handle);
}
