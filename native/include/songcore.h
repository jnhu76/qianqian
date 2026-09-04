/*
 * songcore.h — Qianqian SongCore ABI v1.
 *
 * SongCore turns one local media file into one coherent song snapshot:
 *
 *   SongSource (host IO callbacks)
 *       ↓
 *   SongCore
 *       ↓
 *   song identity · metadata · artwork · stream information ·
 *   source-rate / source-layout Float32 interleaved PCM · seek · EOF
 *
 * ABI v1 layout is FIXED. Compatible additions use reserved fields or new
 * functions; a layout/semantic break requires ABI v2 (SONGCORE_ABI_VERSION
 * bump). No FFmpeg type ever crosses this header.
 *
 * Threading: a song_handle is NOT internally thread-safe; calls on one
 * handle must be externally serialized. Different handles may be used
 * concurrently. SongCore adds no internal mutexes.
 */

#ifndef QIANQIAN_SONGCORE_H
#define QIANQIAN_SONGCORE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 * Symbol visibility (SONGCORE_API)
 *
 * The shared library must export ONLY the frozen ABI below. Contract:
 *   - default (no macro defined): plain declarations — static linking.
 *   - SONGCORE_BUILD_SHARED: defined by the SongCore build when compiling
 *     the shared library; exports the ABI (dllexport on Windows, default
 *     visibility on ELF/Mach-O; pair with hidden default visibility).
 *   - SONGCORE_DLL: defined by consumers that link the shared library on
 *     Windows; imports the ABI (dllimport). Optional elsewhere.
 * ---------------------------------------------------------------------- */

#if defined(SONGCORE_BUILD_SHARED)
#  if defined(_WIN32)
#    define SONGCORE_API __declspec(dllexport)
#  elif defined(__GNUC__) && (__GNUC__ >= 4)
#    define SONGCORE_API __attribute__((visibility("default")))
#  else
#    define SONGCORE_API
#  endif
#elif defined(SONGCORE_DLL) && defined(_WIN32)
#  define SONGCORE_API __declspec(dllimport)
#else
#  define SONGCORE_API
#endif

/* -------------------------------------------------------------------------
 * ABI version
 * ---------------------------------------------------------------------- */

#define SONGCORE_ABI_VERSION 1u

/* Returns SONGCORE_ABI_VERSION. Callable at any time. */
SONGCORE_API uint32_t songcore_abi_version(void);

/* Opaque handle. SongCore-owned until song_close. */
typedef struct song_handle song_handle;

/* -------------------------------------------------------------------------
 * Typed status / error model
 *
 * EOF is a normal terminal condition, never an error. Callers branch on
 * these codes; song_error is diagnostics only.
 * ---------------------------------------------------------------------- */

typedef enum song_status {
    SONG_OK                    = 0,  /* success */

    SONG_EOF                   = 1,  /* end of decoded PCM, not an error */

    SONG_ERR_INVALID_ARGUMENT  = 100, /* null/illegal argument */
    SONG_ERR_STATE             = 101, /* call not allowed in current state */
    SONG_ERR_NOT_OPEN          = 102, /* operation needs a probed handle */
    SONG_ERR_IO                = 103, /* host I/O failure */
    SONG_ERR_UNSUPPORTED_CONTAINER = 104,
    SONG_ERR_NO_AUDIO_STREAM   = 105,
    SONG_ERR_UNSUPPORTED_CODEC = 106,
    SONG_ERR_CORRUPT_DATA      = 107,
    SONG_ERR_DECODE_ERROR      = 108,
    SONG_ERR_SEEK_UNSUPPORTED  = 109,
    SONG_ERR_SEEK_ERROR        = 110,
    SONG_ERR_STREAM_CHANGE     = 111, /* decoder changed rate/layout mid-stream;
                                         fail-closed for v1 */
    SONG_ERR_OUT_OF_MEMORY     = 112,
    SONG_ERR_INTERNAL_ERROR    = 113,
} song_status;

/* Diagnostic for the last failed operation. `message` is NUL-terminated
 * UTF-8; `message_len` excludes the terminator; `native_code` is the
 * backend error code or 0. For logs only. Valid until the next SongCore
 * call on the same handle. */
typedef struct song_error {
    const char *message;
    uint32_t    message_len;
    int32_t     native_code;
    uint32_t    reserved;
} song_error;

/* Diagnostic of the last error on this handle, or SONG_OK when the last
 * call succeeded (out_error is then filled with a NULL message). */
SONGCORE_API song_status song_last_error(song_handle *handle, const song_error **out_error);

/* -------------------------------------------------------------------------
 * Host I/O
 *
 * All three callbacks are required:
 *   read(dst,size)   returns bytes read (>0), 0 at EOF, <0 on host error
 *   seek(absolute)   returns resulting absolute offset, <0 on host error
 *   size()           returns total source size in bytes, <0 when unavailable
 * ---------------------------------------------------------------------- */

typedef int64_t (*song_read_fn)(void *userdata, uint8_t *dst, size_t size);
typedef int64_t (*song_seek_fn)(void *userdata, int64_t absolute_offset);
typedef int64_t (*song_size_fn)(void *userdata);

typedef struct song_io {
    void        *userdata;
    song_read_fn read;
    song_seek_fn seek;
    song_size_fn size;
} song_io;

/* -------------------------------------------------------------------------
 * Channel layout
 *
 * SongCore-owned speaker bit-mask, stable for v1; follows the conventional
 * SMPTE/FFmpeg channel numbering. A mask of 0 means UNKNOWN: callers must
 * not guess channel order from count alone.
 * ---------------------------------------------------------------------- */

enum {
    SONG_CH_FRONT_LEFT            = UINT64_C(1) << 0,
    SONG_CH_FRONT_RIGHT           = UINT64_C(1) << 1,
    SONG_CH_FRONT_CENTER          = UINT64_C(1) << 2,
    SONG_CH_LOW_FREQUENCY         = UINT64_C(1) << 3,
    SONG_CH_BACK_LEFT             = UINT64_C(1) << 4,
    SONG_CH_BACK_RIGHT            = UINT64_C(1) << 5,
    SONG_CH_FRONT_LEFT_OF_CENTER  = UINT64_C(1) << 6,
    SONG_CH_FRONT_RIGHT_OF_CENTER = UINT64_C(1) << 7,
    SONG_CH_BACK_CENTER           = UINT64_C(1) << 8,
    SONG_CH_SIDE_LEFT             = UINT64_C(1) << 9,
    SONG_CH_SIDE_RIGHT            = UINT64_C(1) << 10,
    SONG_CH_TOP_CENTER            = UINT64_C(1) << 11,
    SONG_CH_TOP_FRONT_LEFT        = UINT64_C(1) << 12,
    SONG_CH_TOP_FRONT_CENTER      = UINT64_C(1) << 13,
    SONG_CH_TOP_FRONT_RIGHT       = UINT64_C(1) << 14,
    SONG_CH_TOP_BACK_LEFT         = UINT64_C(1) << 15,
    SONG_CH_TOP_BACK_CENTER       = UINT64_C(1) << 16,
    SONG_CH_TOP_BACK_RIGHT        = UINT64_C(1) << 17,

    SONG_CH_MASK_UNKNOWN = UINT64_C(0),
    SONG_CH_MONO         = SONG_CH_FRONT_CENTER,
    SONG_CH_STEREO       = SONG_CH_FRONT_LEFT | SONG_CH_FRONT_RIGHT,
};

/* -------------------------------------------------------------------------
 * Song info (immutable snapshot after probe / stream selection)
 *
 * duration_us == -1 when the container declares no duration. codec/container
 * are stable ASCII names (e.g. "mp3", "flac", "mov").
 * ---------------------------------------------------------------------- */

typedef struct song_info {
    int32_t  sample_rate;        /* source sample rate, > 0 */
    int32_t  channels;           /* source channel count, > 0 */
    uint64_t channel_mask;       /* SONG_CH_* bits; 0 = unknown */
    int64_t  duration_us;        /* container-declared duration, -1 unknown */
    int32_t  bits_per_sample;    /* source depth where meaningful, else 0 */
    char     codec[32];          /* e.g. "flac", "mp3", "aac", "pcm_s16le" */
    char     container[32];      /* e.g. "flac", "mp3", "mov", "wav", "ogg" */
    uint32_t selected_audio_index; /* index into the decodable-audio
                                      enumeration, see song_audio_stream_* */
    uint32_t audio_stream_count; /* number of decodable audio streams */
    uint32_t flags;              /* 0 for v1 */
    uint32_t reserved[3];
} song_info;

/* Per-audio-stream info. `audio_index` is the position in the decodable
 * audio enumeration (0..audio_stream_count-1); `stream_index` is the
 * absolute container stream index (identity correlation only). */
typedef struct song_stream_info {
    uint32_t audio_index;        /* enumeration position (this stream) */
    uint32_t stream_index;       /* absolute container stream index */
    int32_t  sample_rate;
    int32_t  channels;
    uint64_t channel_mask;
    int64_t  duration_us;        /* stream-level duration, -1 unknown */
    int32_t  bits_per_sample;
    char     codec[32];
    uint32_t is_default;         /* container marks this stream as default */
    uint32_t reserved[3];
} song_stream_info;

/* -------------------------------------------------------------------------
 * Lifecycle
 *
 *   song_open  -> song_probe -> {streams, metadata, artwork, read, seek}
 *              -> song_close
 *
 * song_probe selects the default decodable audio stream and opens its
 * decoder; every derived view (song_info, metadata, artwork, PCM) then
 * refers to the SAME logical song / selected stream.
 * ---------------------------------------------------------------------- */

/* Open a handle over host IO. All three callbacks are required. Errors:
 * SONG_ERR_UNSUPPORTED_CONTAINER / SONG_ERR_IO / SONG_ERR_OUT_OF_MEMORY;
 * *out_handle is then untouched. No diagnostic is available for song_open
 * failures (no handle exists). */
SONGCORE_API song_status song_open(const song_io *io, song_handle **out_handle);

/* Parse the container, enumerate decodable audio streams, select the
 * default one (policy: decodable audio streams only; prefer
 * AV_DISPOSITION_DEFAULT; otherwise lowest stream index), open its decoder,
 * and build the song snapshot. Idempotent: later calls return the cached
 * snapshot. Errors: SONG_ERR_NOT_OPEN, SONG_ERR_IO,
 * SONG_ERR_UNSUPPORTED_CONTAINER, SONG_ERR_NO_AUDIO_STREAM,
 * SONG_ERR_UNSUPPORTED_CODEC, SONG_ERR_CORRUPT_DATA, SONG_ERR_OUT_OF_MEMORY. */
SONGCORE_API song_status song_probe(song_handle *handle, song_info *out_info);

/* Number of decodable audio streams (excludes non-audio and
 * attached-picture streams). Requires a probed handle. */
SONGCORE_API song_status song_audio_stream_count(song_handle *handle, uint32_t *out_count);

/* Info for one decodable audio stream (audio_index in 0..count-1). */
SONGCORE_API song_status song_audio_stream_info(song_handle *handle, uint32_t audio_index,
                                   song_stream_info *out_info);

/* Explicitly select another decodable audio stream (audio_index).
 * Semantics:
 *   - the decoder for the old stream is destroyed and a new one is opened;
 *   - playback position resets to the start;
 *   - the metadata snapshot is rebuilt for the new stream (its views are
 *     invalidated; see Metadata);
 *   - container artwork is NOT changed by stream selection and remains
 *     valid;
 *   - song_info (rate/layout/selected_audio_index) is updated;
 *   - PCM and decoder state from the old stream are gone.
 * Invalid audio_index -> SONG_ERR_INVALID_ARGUMENT. */
SONGCORE_API song_status song_select_stream(song_handle *handle, uint32_t audio_index);

/* -------------------------------------------------------------------------
 * Metadata
 *
 * After a successful probe/stream selection SongCore builds an immutable
 * metadata snapshot. All returned string views are (pointer, length) pairs
 * into that snapshot and remain valid until:
 *   - the next stream selection (song_select_stream), or
 *   - song_close().
 * They remain valid across song_read_pcm(), song_seek(), and EOF.
 *
 * Canonical precedence: for each canonical field, the selected stream
 * metadata overrides container metadata. Missing values are not errors;
 * presence is explicit via has_*.
 * ---------------------------------------------------------------------- */

typedef struct song_metadata {
    const char *title;       uint32_t title_len;       uint32_t has_title;
    const char *artist;      uint32_t artist_len;      uint32_t has_artist;
    const char *album;       uint32_t album_len;       uint32_t has_album;
    const char *album_artist;uint32_t album_artist_len;uint32_t has_album_artist;
    const char *genre;       uint32_t genre_len;       uint32_t has_genre;
    const char *composer;    uint32_t composer_len;    uint32_t has_composer;
    const char *date;        uint32_t date_len;        uint32_t has_date;
    const char *comment;     uint32_t comment_len;     uint32_t has_comment;

    int32_t track_number;    uint32_t has_track_number; /* 1-based, -1 absent */
    int32_t track_total;     uint32_t has_track_total;
    int32_t disc_number;     uint32_t has_disc_number;
    int32_t disc_total;      uint32_t has_disc_total;

    /* ReplayGain as parsed by the backend. Gains in microbels (1e-6 dB);
     * peaks scaled so 100000 == full scale. has_* = present in source. */
    int32_t  track_gain_mb;  uint32_t has_track_gain;
    uint32_t track_peak;     uint32_t has_track_peak;
    int32_t  album_gain_mb;  uint32_t has_album_gain;
    uint32_t album_peak;     uint32_t has_album_peak;
} song_metadata;

/* Returns a pointer to the immutable metadata snapshot (borrowed). */
SONGCORE_API song_status song_get_metadata(song_handle *handle, const song_metadata **out_meta);

/* Raw metadata enumeration — unknown/future tags never need an ABI change.
 * Entries are ordered deterministically: container/global scope first, then
 * the selected audio stream scope; within a scope, source parse order.
 * Duplicate keys are preserved. */
typedef enum song_metadata_scope {
    SONG_METADATA_SCOPE_CONTAINER = 0,
    SONG_METADATA_SCOPE_STREAM    = 1,
} song_metadata_scope;

typedef struct song_metadata_entry {
    uint32_t     scope;      /* song_metadata_scope */
    const char  *key;        /* view into the snapshot */
    uint32_t     key_len;
    const char  *value;      /* view into the snapshot */
    uint32_t     value_len;
    uint32_t     reserved;
} song_metadata_entry;

SONGCORE_API song_status song_get_metadata_count(song_handle *handle, uint32_t *out_count);
SONGCORE_API song_status song_get_metadata_entry(song_handle *handle, uint32_t index,
                                song_metadata_entry *out_entry);

/* -------------------------------------------------------------------------
 * Artwork (compressed bytes only; SongCore never decodes images)
 *
 * 0..N artwork items. Views are SongCore-owned and valid until song_close();
 * stream selection does NOT invalidate container artwork. Item order is the
 * deterministic container order (attached-picture stream order).
 *
 * role is best-effort: single-artwork files are treated as front cover;
 * otherwise role is mapped from the source picture-type label when the
 * backend exposes it (else SONG_ARTWORK_UNKNOWN).
 * ---------------------------------------------------------------------- */

typedef enum song_artwork_role {
    SONG_ARTWORK_UNKNOWN     = 0,
    SONG_ARTWORK_FRONT_COVER = 1,
    SONG_ARTWORK_BACK_COVER  = 2,
    SONG_ARTWORK_OTHER       = 3,
} song_artwork_role;

typedef struct song_artwork_item {
    uint32_t       role;      /* song_artwork_role */
    const char    *mime;      /* e.g. "image/jpeg", "image/png" */
    uint32_t       mime_len;
    const uint8_t *data;      /* compressed image bytes */
    uint64_t       data_len;
    int32_t        width;     /* -1 when unknown */
    int32_t        height;    /* -1 when unknown */
    uint32_t       is_front_cover;
    uint32_t       reserved;
} song_artwork_item;

SONGCORE_API song_status song_get_artwork_count(song_handle *handle, uint32_t *out_count);
SONGCORE_API song_status song_get_artwork_item(song_handle *handle, uint32_t index,
                              song_artwork_item *out_item);

/* -------------------------------------------------------------------------
 * PCM
 *
 * Frozen format: Float32, interleaved, source sample rate, source layout.
 *
 * song_read_pcm fills dst with up to frame_capacity frames:
 *   SONG_OK  -> frames were produced (*out_frames_produced > 0, <= capacity)
 *   SONG_EOF -> *out_frames_produced == 0 (end of stream, not an error)
 *   other    -> typed error; *out_frames_produced == 0 in that call
 *
 * Partial-success: if a decode error occurs after some frames were produced
 * in the current call, the call returns SONG_OK with those frames and the
 * error is surfaced by the NEXT call (with zero frames produced).
 *
 * Format-change guard (fail-closed): if the decoder changes sample rate /
 * channel count / channel layout mid-stream relative to song_info, SongCore
 * does NOT emit contradicting PCM; it returns SONG_ERR_STREAM_CHANGE on a
 * subsequent call.
 *
 * The caller owns dst. frame_capacity == 0 -> SONG_ERR_INVALID_ARGUMENT.
 * ---------------------------------------------------------------------- */

SONGCORE_API song_status song_read_pcm(song_handle *handle, float *dst,
                          uint64_t frame_capacity,
                          uint64_t *out_frames_produced);

/* -------------------------------------------------------------------------
 * Seek
 *
 * Playback-oriented semantics, NOT sample-perfect unless the format proves it.
 *
 * requested_position_us is clamped against the known duration (when known)
 * and converted to a container seek at/before the target; the decoder is
 * flushed and all pending packet/frame/PCM state is cleared; SongCore then
 * advances to a defined landing point and the NEXT song_read_pcm belongs to
 * that landing point.
 *
 * *out_actual_position_us reports the effective landing position measured
 * from the first decoded frame's timestamp; -1 when SongCore cannot
 * determine a landing (position unknown is explicit, never manufactured).
 * out_actual_position_us may be NULL when the caller does not need it.
 *
 * Seek does NOT promise sample-exactness. For lossy/lapped codecs the first
 * frames after seek may differ from a sequential decode (bounded codec-frame
 * tolerance); for lossless formats the landing is frame-accurate.
 *
 * After a successful seek: metadata unchanged, artwork unchanged, selected
 * stream unchanged, rate/layout/codec/container unchanged.
 *
 * Errors: SONG_ERR_SEEK_UNSUPPORTED (container has no seek),
 * SONG_ERR_SEEK_ERROR, SONG_ERR_STREAM_CHANGE / SONG_ERR_DECODE_ERROR if
 * the landing frame cannot be converted (fail-closed).
 * ---------------------------------------------------------------------- */

SONGCORE_API song_status song_seek(song_handle *handle, int64_t requested_position_us,
                      int64_t *out_actual_position_us);

/* Close the handle and free every SongCore-owned resource. All views
 * returned by the handle are invalidated. Safe on any state; NULL is a
 * no-op. */
SONGCORE_API void song_close(song_handle *handle);

#ifdef __cplusplus
}
#endif

#endif /* QIANQIAN_SONGCORE_H */
