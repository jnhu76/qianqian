/*
 * player_engine.h — Qianqian PlayerEngine C ABI v1.
 *
 * The narrow control surface over the native PlayerEngine (docs/
 * player-engine.md §11): lifecycle control, the backend-side manual
 * submit/render ticks, and a polled snapshot struct for diagnostics.
 * Semantics are the frozen ones — this header only transcribes the
 * internal C++ API (src/player/player_engine.hpp) 1:1.
 *
 *   control thread    pe_open / pe_play / pe_pause / pe_stop / pe_seek
 *   decode worker     internal (pe_config.worker_thread) or pe-free
 *                     manual stepping through the tests only
 *   backend side      pe_submit (callback refill) and pe_render (device
 *                     progression) stay separate entry points
 *
 * ABI v1 layout is FIXED. Compatible additions use reserved fields or new
 * functions; a layout/semantic break requires ABI v2
 * (PLAYER_ENGINE_ABI_VERSION bump). No FFmpeg type and no C++ type ever
 * crosses this header. Reuses songcore.h's song_io / song_status: the
 * engine owns a SongCore handle, and open() travels through both.
 *
 * Threading: control calls may be made from any single control thread
 * (serialized by the engine); pe_submit / pe_render / pe_get_snapshot are
 * safe concurrently with the decode worker, mirroring the internal lock
 * model. One engine owns one song at a time.
 */

#ifndef QIANQIAN_PLAYER_ENGINE_H
#define QIANQIAN_PLAYER_ENGINE_H

#include <stdint.h>

#include "songcore.h"

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 * Symbol visibility (PE_API) — same contract as songcore.h.
 * ---------------------------------------------------------------------- */

#if defined(PLAYER_ENGINE_BUILD_SHARED)
#  if defined(_WIN32)
#    define PE_API __declspec(dllexport)
#  elif defined(__GNUC__) && (__GNUC__ >= 4)
#    define PE_API __attribute__((visibility("default")))
#  else
#    define PE_API
#  endif
#elif defined(PLAYER_ENGINE_DLL) && defined(_WIN32)
#  define PE_API __declspec(dllimport)
#else
#  define PE_API
#endif

/* -------------------------------------------------------------------------
 * ABI version
 * ---------------------------------------------------------------------- */

#define PLAYER_ENGINE_ABI_VERSION 1u

/* Returns PLAYER_ENGINE_ABI_VERSION. Callable at any time. */
PE_API uint32_t player_engine_abi_version(void);

/* Opaque engine. */
typedef struct pe_engine pe_engine;

/* Sentinel for pe_render's `generation`: "the current one". Any other
 * value (including negatives) is a literal dead generation id whose late
 * render events are dropped (stale-output guard). */
#define PE_CURRENT_GENERATION ((int64_t)INT64_MIN)

/* -------------------------------------------------------------------------
 * Typed status / state model — numeric values match the internal
 * PlayerStatus / PlayerState enums (asserted in the implementation).
 * ---------------------------------------------------------------------- */

typedef enum pe_status {
    PE_OK               = 0, /* success */
    PE_ERR_ILLEGAL_CALL = 1, /* call not allowed in current state */
    PE_ERR_OPEN_FAILED  = 2, /* *out_song_status carries the song_open status */
    PE_ERR_SEEK_FAILED  = 3, /* *out_song_status carries the song_seek status */
} pe_status;

typedef enum pe_state {
    PE_STATE_EMPTY   = 0,
    PE_STATE_READY   = 1,
    PE_STATE_PLAYING = 2,
    PE_STATE_PAUSED  = 3,
    PE_STATE_ENDED   = 4,
    PE_STATE_ERROR   = 5,
} pe_state;

/* Landing quality of the current segment's clock anchor (snapshot only). */
#define PE_QUALITY_CONFIRMED 0 /* rebased on SongCore's returned landing */
#define PE_QUALITY_ESTIMATED 1 /* landing unknown; clamped requested target */

/* -------------------------------------------------------------------------
 * Configuration (fail-closed: pe_create returns NULL on a zero field or
 * read_chunk_frames > capacity_frames).
 * ---------------------------------------------------------------------- */

typedef struct pe_config {
    uint64_t capacity_frames;   /* the ONLY queue sizing */
    uint64_t read_chunk_frames; /* song_read_pcm capacity per chunk */
    uint64_t max_submit_frames; /* largest pe_submit() period supported */
    int32_t max_channels;       /* ring slot stride; songs may use fewer */
    int32_t worker_thread;      /* 0 = manual stepping (tests), 1 = spawn */
} pe_config;

/* -------------------------------------------------------------------------
 * Backend-side reports (kind strings are static literals, valid forever).
 * ---------------------------------------------------------------------- */

typedef struct pe_submit_report {
    uint64_t    segment;       /* segment the PCM was tagged with */
    uint64_t    media_frames;  /* real frames moved queue -> backend */
    uint64_t    silence_frames;/* GAP frames injected (device time only) */
    const char *kind;          /* "audio"|"underrun"|"preroll"|"eos"|"idle" */
} pe_submit_report;

typedef struct pe_render_report {
    uint64_t    segment;                /* segment at render time */
    const char *kind;   /* "rendered" | "paused" | "stale" */
    uint64_t    rendered_output_frames; /* device frames proven rendered */
    uint64_t    rendered_media_frames;  /* media subset of the above */
    int64_t     generation;             /* generation the event ran under */
} pe_render_report;

/* -------------------------------------------------------------------------
 * Polled snapshot (diagnostics; UI cadence 5-10 Hz). One call = one
 * coherent instant: every counter is captured under the same internal
 * lock hold, so the conservation laws below hold across the struct.
 * ---------------------------------------------------------------------- */

typedef struct pe_snapshot {
    pe_state  state;
    int64_t   media_position_frames;
    uint8_t   position_quality;      /* PE_QUALITY_* */
    int64_t   decoded_source_position; /* engine-observable estimate */
    int64_t   duration_frames;         /* -1 = unknown, never fake 0 */
    uint32_t  duration_known;
    uint64_t  queued_media_frames;
    uint64_t  capacity_frames;
    uint64_t  epoch;
    uint64_t  segment;
    uint32_t  source_eof;
    uint64_t  underrun_count;
    uint64_t  underrun_silence_output_frames;
    uint64_t  preroll_events;
    uint64_t  preroll_silence_output_frames;
    uint64_t  eos_silence_output_frames;
    uint64_t  decoded_source_frames;
    uint64_t  submitted_output_frames;
    uint64_t  rendered_output_frames;
    uint64_t  pending_output_frames;
    uint64_t  discarded_output_frames;
    uint64_t  submitted_media_frames;
    uint64_t  rendered_media_frames;
    uint64_t  rendered_gap_output_frames;
    uint64_t  pending_media_frames;
    uint64_t  discarded_output_media_frames;
    uint64_t  discarded_stale_media_frames;
    uint64_t  stale_render_events;
    /* Coherent decode-accounting inputs (same lock hold as above):
       decoded_source_frames == ring_produced_total +
       discarded_stale_media_frames + in_flight_frames. */
    uint64_t  in_flight_frames;
    uint64_t  ring_produced_total;
    uint64_t  ring_consumed_total;
    uint64_t  ring_discarded_total;
    char      last_error[96]; /* "" = none; normalized category prefixes */
} pe_snapshot;

/* -------------------------------------------------------------------------
 * Lifecycle
 * ---------------------------------------------------------------------- */

/* Creates an engine. Returns NULL on an invalid config (null pointer,
 * zero field, or read_chunk_frames > capacity_frames) — fail-closed,
 * never a partially-configured engine. */
PE_API pe_engine *pe_create(const pe_config *config);

/* Destroys the engine: stops publications, joins the decode worker (if
 * any), closes the SongCore handle. NULL is a no-op. */
PE_API void pe_destroy(pe_engine *engine);

/* open(song): stop everything, drop the previous handle, start the new
 * song at position 0 (a fresh handle — CONFIRMED), state READY. Never
 * autoplays. `io` is copied and reused for stop()-recovery reopens.
 * *out_song_status (when non-NULL) always receives the SongCore status:
 * SONG_OK on success, the failing song_open/song_probe status otherwise. */
PE_API pe_status pe_open(pe_engine *engine, const song_io *io,
                         int32_t *out_song_status);

/* play(): READY/PAUSED -> PLAYING; after ENDED replays from the beginning
 * (a failed restart seek lands in ERROR); idempotent while PLAYING. */
PE_API pe_status pe_play(pe_engine *engine);

/* pause(): freezes audible progression, retains the buffer. Documented
 * no-op outside PLAYING; always returns PE_OK. */
PE_API pe_status pe_pause(pe_engine *engine);

/* stop(): deterministic rebuild to READY @0. From a healthy state the
 * handle is rewound in place; from ERROR (or a failed rewind) the source
 * is dropped and reopened; if even the reopen fails, ERROR persists and
 * only pe_open recovers. *out_song_status (when non-NULL) always receives
 * the SongCore status (SONG_OK on success). */
PE_API pe_status pe_stop(pe_engine *engine, int32_t *out_song_status);

/* seek(T) from READY/PLAYING/PAUSED (and ENDED -> READY). On success
 * *out_landing_frames carries the landing the clock was rebased on.
 * *out_song_status (when non-NULL) always receives the SongCore status
 * (SONG_OK on success, the failing song_seek status otherwise). */
PE_API pe_status pe_seek(pe_engine *engine, int64_t position_us,
                         int64_t *out_landing_frames,
                         int32_t *out_song_status);

/* -------------------------------------------------------------------------
 * Backend side — the manual-tick device (NullAudioBackend today; WASAPI
 * later maps its callback/refill onto these same two entry points).
 * ---------------------------------------------------------------------- */

/* One backend callback: move up to `period_frames` of real PCM from the
 * queue into the backend, padding shortfalls with classified GAP silence.
 * Never blocks, never decodes. Submitting is NOT audible. */
PE_API pe_submit_report pe_submit(pe_engine *engine, uint64_t period_frames);

/* Device/render progression: the device consumed `frames` output frames.
 * generation == PE_CURRENT_GENERATION means the current one; any other
 * value is a literal dead generation whose late event is dropped. */
PE_API pe_render_report pe_render(pe_engine *engine, int64_t frames,
                                  int64_t generation);

/* One coherent instant of all diagnostics (see pe_snapshot). */
PE_API pe_status pe_get_snapshot(pe_engine *engine, pe_snapshot *out);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* QIANQIAN_PLAYER_ENGINE_H */
