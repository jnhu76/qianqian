/*
 * fake_songcore_c.h — C entry points of the test-only SongCore stand-in.
 *
 * External-consumer gates link this stand-in INSTEAD of real SongCore
 * (link-time C ABI substitution, exactly like every other player test).
 * The stand-in "filesystem" factory below is the only non-frozen symbol a
 * consumer may touch: everything above the engine goes through
 * player_engine.h alone. Not shipped; never part of any production graph.
 */

#ifndef QIANQIAN_TESTS_PLAYER_FAKE_SONGCORE_C_H
#define QIANQIAN_TESTS_PLAYER_FAKE_SONGCORE_C_H

#include <stdint.h>

#include "songcore.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Fills *out_io with host IO bound to an internally-owned synthetic song
 * of the given shape (each call creates a NEW song). Returns SONG_OK, or
 * SONG_ERR_INVALID_ARGUMENT on a null pointer / non-positive shape. */
song_status fake_song_make_io(song_io *out_io, int64_t total_frames,
                              int32_t sample_rate, int32_t channels);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* QIANQIAN_TESTS_PLAYER_FAKE_SONGCORE_C_H */
