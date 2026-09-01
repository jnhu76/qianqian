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
 * the SongCore ABI v1 contract with an explicit int64 handle — the 15
 * song_wasm_* functions are a 1:1 delegation to the native contract; struct
 * outputs are written into caller-provided WASM memory. Alongside them, a
 * small bridge-infrastructure surface exists for external hosts ONLY:
 *   song_wasm_alloc / song_wasm_free      guest-memory ownership
 *   song_wasm_layout                      compiler-emitted struct layout table
 * These are implementation details of the WASM door, never SongCore ABI
 * functions, and have no native counterpart.
 */

#include "qn_host_imports.h"
#include "songcore.h"

#include <stddef.h>
#include <stdlib.h>

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

/* Bridge infrastructure (NOT part of the 15-symbol SongCore ABI):
 * guest-memory allocation and the machine-generated struct layout table
 * the external host needs to consume the ABI output structs. These exports
 * exist only for WASM embedders; native consumers read songcore.h and use
 * their platform's own allocator. */

QN_EXPORT("song_wasm_alloc")
int32_t song_wasm_alloc(uint32_t bytes) {
    /* wasm32 pointers fit int32_t; 0 signals allocation failure (the host
     * must never dereference it). malloc guarantees the alignment the ABI
     * structs require. */
    void *p = malloc(bytes ? (size_t)bytes : 1);
    return p ? (int32_t)(uintptr_t)p : 0;
}

QN_EXPORT("song_wasm_free")
void song_wasm_free(int32_t ptr) {
    free((void *)(uintptr_t)ptr);
}

/* Machine-validated layout mirror: word-indexed table of sizeof/offsetof
 * constants compiled INTO the guest from the real songcore.h definitions.
 * The host calls once with out_words=0 to learn the length, allocates, and
 * calls again; every struct interpretation it does afterwards is keyed to
 * these compiler-emitted numbers, never to hand-maintained guesses. */
enum {
    QN_LAYOUT_VERSION = 0,   /* table format version, currently 1 */
    QN_LAYOUT_SIZEOF_INFO,
    QN_LAYOUT_OFF_INFO_SAMPLE_RATE,
    QN_LAYOUT_OFF_INFO_CHANNELS,
    QN_LAYOUT_OFF_INFO_CHANNEL_MASK,
    QN_LAYOUT_OFF_INFO_DURATION_US,
    QN_LAYOUT_OFF_INFO_BITS_PER_SAMPLE,
    QN_LAYOUT_OFF_INFO_CODEC,
    QN_LAYOUT_OFF_INFO_CONTAINER,
    QN_LAYOUT_OFF_INFO_SELECTED_INDEX,
    QN_LAYOUT_OFF_INFO_STREAM_COUNT,
    QN_LAYOUT_SIZEOF_STREAM_INFO,
    QN_LAYOUT_OFF_STREAM_SAMPLE_RATE,
    QN_LAYOUT_OFF_STREAM_CHANNELS,
    QN_LAYOUT_SIZEOF_METADATA,
    QN_LAYOUT_OFF_META_FIRST_STRING, /* title; 8 strings at a fixed stride */
    QN_LAYOUT_META_STRING_STRIDE,
    QN_LAYOUT_OFF_META_TRACK_NUMBER,
    QN_LAYOUT_SIZEOF_METADATA_ENTRY,
    QN_LAYOUT_OFF_ENTRY_SCOPE,
    QN_LAYOUT_OFF_ENTRY_KEY,
    QN_LAYOUT_OFF_ENTRY_KEY_LEN,
    QN_LAYOUT_OFF_ENTRY_VALUE,
    QN_LAYOUT_OFF_ENTRY_VALUE_LEN,
    QN_LAYOUT_SIZEOF_ARTWORK_ITEM,
    QN_LAYOUT_OFF_ART_ROLE,
    QN_LAYOUT_OFF_ART_MIME,
    QN_LAYOUT_OFF_ART_MIME_LEN,
    QN_LAYOUT_OFF_ART_DATA,
    QN_LAYOUT_OFF_ART_DATA_LEN,
    QN_LAYOUT_OFF_ART_WIDTH,
    QN_LAYOUT_OFF_ART_HEIGHT,
    QN_LAYOUT_OFF_ART_FRONT,
    QN_LAYOUT_SIZEOF_ERROR,
    QN_LAYOUT_OFF_ERR_MESSAGE,
    QN_LAYOUT_OFF_ERR_MESSAGE_LEN,
    QN_LAYOUT_OFF_ERR_NATIVE,
    QN_LAYOUT_WORD_COUNT
};

QN_EXPORT("song_wasm_layout")
uint32_t song_wasm_layout(uint32_t *out, uint32_t out_words) {
    static const uint32_t words[QN_LAYOUT_WORD_COUNT] = {
        [QN_LAYOUT_VERSION]                 = 1,
        [QN_LAYOUT_SIZEOF_INFO]             = (uint32_t)sizeof(song_info),
        [QN_LAYOUT_OFF_INFO_SAMPLE_RATE]    = (uint32_t)offsetof(song_info, sample_rate),
        [QN_LAYOUT_OFF_INFO_CHANNELS]       = (uint32_t)offsetof(song_info, channels),
        [QN_LAYOUT_OFF_INFO_CHANNEL_MASK]   = (uint32_t)offsetof(song_info, channel_mask),
        [QN_LAYOUT_OFF_INFO_DURATION_US]    = (uint32_t)offsetof(song_info, duration_us),
        [QN_LAYOUT_OFF_INFO_BITS_PER_SAMPLE]= (uint32_t)offsetof(song_info, bits_per_sample),
        [QN_LAYOUT_OFF_INFO_CODEC]          = (uint32_t)offsetof(song_info, codec),
        [QN_LAYOUT_OFF_INFO_CONTAINER]      = (uint32_t)offsetof(song_info, container),
        [QN_LAYOUT_OFF_INFO_SELECTED_INDEX] = (uint32_t)offsetof(song_info, selected_audio_index),
        [QN_LAYOUT_OFF_INFO_STREAM_COUNT]   = (uint32_t)offsetof(song_info, audio_stream_count),
        [QN_LAYOUT_SIZEOF_STREAM_INFO]      = (uint32_t)sizeof(song_stream_info),
        [QN_LAYOUT_OFF_STREAM_SAMPLE_RATE]  = (uint32_t)offsetof(song_stream_info, sample_rate),
        [QN_LAYOUT_OFF_STREAM_CHANNELS]     = (uint32_t)offsetof(song_stream_info, channels),
        [QN_LAYOUT_SIZEOF_METADATA]         = (uint32_t)sizeof(song_metadata),
        [QN_LAYOUT_OFF_META_FIRST_STRING]   = (uint32_t)offsetof(song_metadata, title),
        [QN_LAYOUT_META_STRING_STRIDE]      = (uint32_t)(offsetof(song_metadata, artist) - offsetof(song_metadata, title)),
        [QN_LAYOUT_OFF_META_TRACK_NUMBER]   = (uint32_t)offsetof(song_metadata, track_number),
        [QN_LAYOUT_SIZEOF_METADATA_ENTRY]   = (uint32_t)sizeof(song_metadata_entry),
        [QN_LAYOUT_OFF_ENTRY_SCOPE]         = (uint32_t)offsetof(song_metadata_entry, scope),
        [QN_LAYOUT_OFF_ENTRY_KEY]           = (uint32_t)offsetof(song_metadata_entry, key),
        [QN_LAYOUT_OFF_ENTRY_KEY_LEN]       = (uint32_t)offsetof(song_metadata_entry, key_len),
        [QN_LAYOUT_OFF_ENTRY_VALUE]         = (uint32_t)offsetof(song_metadata_entry, value),
        [QN_LAYOUT_OFF_ENTRY_VALUE_LEN]     = (uint32_t)offsetof(song_metadata_entry, value_len),
        [QN_LAYOUT_SIZEOF_ARTWORK_ITEM]     = (uint32_t)sizeof(song_artwork_item),
        [QN_LAYOUT_OFF_ART_ROLE]            = (uint32_t)offsetof(song_artwork_item, role),
        [QN_LAYOUT_OFF_ART_MIME]            = (uint32_t)offsetof(song_artwork_item, mime),
        [QN_LAYOUT_OFF_ART_MIME_LEN]        = (uint32_t)offsetof(song_artwork_item, mime_len),
        [QN_LAYOUT_OFF_ART_DATA]            = (uint32_t)offsetof(song_artwork_item, data),
        [QN_LAYOUT_OFF_ART_DATA_LEN]        = (uint32_t)offsetof(song_artwork_item, data_len),
        [QN_LAYOUT_OFF_ART_WIDTH]           = (uint32_t)offsetof(song_artwork_item, width),
        [QN_LAYOUT_OFF_ART_HEIGHT]          = (uint32_t)offsetof(song_artwork_item, height),
        [QN_LAYOUT_OFF_ART_FRONT]           = (uint32_t)offsetof(song_artwork_item, is_front_cover),
        [QN_LAYOUT_SIZEOF_ERROR]            = (uint32_t)sizeof(song_error),
        [QN_LAYOUT_OFF_ERR_MESSAGE]         = (uint32_t)offsetof(song_error, message),
        [QN_LAYOUT_OFF_ERR_MESSAGE_LEN]     = (uint32_t)offsetof(song_error, message_len),
        [QN_LAYOUT_OFF_ERR_NATIVE]          = (uint32_t)offsetof(song_error, native_code),
    };
    if (out_words < QN_LAYOUT_WORD_COUNT) return (uint32_t)QN_LAYOUT_WORD_COUNT;
    for (int i = 0; i < QN_LAYOUT_WORD_COUNT; ++i) out[i] = words[i];
    return (uint32_t)QN_LAYOUT_WORD_COUNT;
}

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
