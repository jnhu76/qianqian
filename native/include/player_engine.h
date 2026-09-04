/*
 * player_engine.h — Qianqian PlayerEngine product C ABI.
 *
 * The narrow control and observation surface over the native PlayerEngine
 * (docs/contracts/player-api.md). This is the ONLY public PlayerEngine surface:
 * lifecycle control (create/open/play/pause/stop/seek/destroy), a polled
 * time-domain snapshot, and typed error reporting. It exposes no audio
 * backend, no manual render ticks, no worker mode, no queue internals —
 * a managed caller (KMP) needs to know nothing about how audio is driven.
 *
 * Threading contract:
 *   - Control calls (pe_open/pe_play/pe_pause/pe_stop/pe_seek) must be
 *     serialized by the caller (the engine also serializes them internally);
 *     they may block (seek/stop wait for the audio path to quiesce).
 *   - pe_get_snapshot may be polled concurrently from another thread.
 *   - The engine owns its decode worker thread; the caller never drives it.
 *
 * Semantics are the frozen ones (docs/contracts/player-api.md): submitted !=
 * rendered, device time != media time, GAP has zero media duration, seek
 * fail-closed, CONFIRMED/ESTIMATED landing, pause freezes audible
 * progression, EOF waits for real media playout. The snapshot reports the
 * MEDIA timeline in MICROSECONDS directly (position_us / duration_us) so
 * a UI renders progress without knowing any sample rate.
 *
 * Error contract: every function returns a typed pe_status (or NULL for
 * pe_create); invalid caller input never aborts, asserts, or terminates the
 * process, and no C++ exception can cross this boundary. Out parameters are
 * written on every return path where their parent pointer is valid: each is
 * initialized to a deterministic value before validation (corrective §19),
 * then overwritten on success (out_landing_us = -1 and out_song_status =
 * SONG_ERR_INVALID_ARGUMENT before any failure path; SONG_OK on success).
 *
 * No FFmpeg type and no C++ type crosses this header. Reuses songcore.h's
 * song_io / song_status: the engine owns a SongCore handle, and open()
 * travels through both.
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

/* -------------------------------------------------------------------------
 * Typed status. Numeric values match the internal PlayerStatus where they
 * overlap (asserted in the implementation); the shim adds its own
 * validation statuses.
 * ---------------------------------------------------------------------- */

typedef enum pe_status {
    PE_OK                  = 0, /* success */
    PE_ERR_ILLEGAL_CALL    = 1, /* call not allowed in current state */
    PE_ERR_OPEN_FAILED     = 2, /* *out_song_status carries the song_open status */
    PE_ERR_SEEK_FAILED     = 3, /* *out_song_status carries the song_seek status */
    PE_ERR_INVALID_ARGUMENT = 4, /* NULL/invalid argument */
    PE_ERR_NO_MEMORY       = 5, /* allocation failed */
    PE_ERR_INTERNAL        = 6, /* unexpected internal failure */
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
 * Configuration. The only product-relevant knob is the queue size (buffer
 * latency); 0 selects the internal default. A NULL config is equivalent to
 * the default. No test or backend implementation knobs are exposed.
 * ---------------------------------------------------------------------- */

typedef struct pe_config {
    uint64_t capacity_frames; /* queue sizing in frames; 0 = internal default */
} pe_config;

/* -------------------------------------------------------------------------
 * Polled snapshot (UI cadence 5-10 Hz). One call = one coherent instant.
 * position_us / duration_us are the MEDIA timeline in microseconds — the
 * product authority. duration_us == -1 means the duration is unknown
 * (duration_known == 0); it is never a fake 0. Frame-domain fields are
 * diagnostics only.
 * ---------------------------------------------------------------------- */

typedef struct pe_snapshot {
    pe_state  state;
    int64_t   position_us;       /* audible media position, microseconds */
    int64_t   duration_us;       /* media duration, microseconds; -1 = unknown */
    uint32_t  duration_known;
    uint8_t   position_quality;  /* PE_QUALITY_*; ESTIMATED is not an error */
    uint64_t  buffered_frames;   /* queued media frames (diagnostic) */
    uint64_t  underrun_count;    /* cumulative underruns (diagnostic) */
    int32_t   sample_rate;       /* source rate of THIS captured instant
                                    (position_us/duration_us convert with it) */
    char      last_error[96];    /* "" = none; normalized category prefixes */
} pe_snapshot;

/* -------------------------------------------------------------------------
 * Lifecycle
 * ---------------------------------------------------------------------- */

/* Creates an engine with the default decode worker thread. Returns NULL on
 * an allocation failure. A NULL config, or capacity_frames == 0, selects
 * the internal default queue size. */
PE_API pe_engine *pe_create(const pe_config *config);

/* Destroys the engine: stops publications, joins the decode worker,
 * quiesces the audio path, closes the SongCore handle. NULL is a no-op. */
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
 * *out_landing_us carries the landing the clock was rebased on, in media
 * microseconds; on any failure it is the deterministic default -1.
 * *out_song_status (when non-NULL) always receives the SongCore status
 * (SONG_OK on success, the failing song_seek status otherwise). A failed
 * seek lands the engine in ERROR (fail-closed). */
PE_API pe_status pe_seek(pe_engine *engine, int64_t position_us,
                         int64_t *out_landing_us, int32_t *out_song_status);

/* One coherent instant of the product snapshot. Every field is written on
 * every return path where `out` is valid. */
PE_API pe_status pe_get_snapshot(pe_engine *engine, pe_snapshot *out);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* QIANQIAN_PLAYER_ENGINE_H */
