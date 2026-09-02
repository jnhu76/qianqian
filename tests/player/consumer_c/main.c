/*
 * main.c — tiny external consumer of the PlayerEngine product C ABI.
 *
 * A pure-C99 translation unit that includes ONLY the C ABI headers
 * (player_engine.h + the stand-in filesystem factory) and links against
 * libplayer_core: proof that the product surface is self-contained,
 * C++-leak-free, and ABI-linkable from a foreign TU. It uses ONLY the
 * product control/observation API — create/open/play/pause/seek/stop/
 * snapshot/destroy — and never touches the audio backend: an internal
 * test driver (player_test_driver.h, tests-only) manually ticks the
 * NullAudioBackend behind the scenes, standing in for WASAPI's callback.
 *
 * Drives one full lifecycle with the engine-owned decode worker:
 * open -> play -> ENDED with time-domain snapshot -> seek -> stop, plus
 * no-crash boundary probes.
 */
#include <stdio.h>
#include <string.h>

#include "fake_songcore_c.h"
#include "player_engine.h"
#include "player_test_driver.h"

static int failures = 0;

#define CHECK(cond)                                                        \
    do {                                                                   \
        if (!(cond)) {                                                     \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);         \
            ++failures;                                                    \
        }                                                                  \
    } while (0)

#define CHECK_RUNAWAY(guard)                                               \
    do {                                                                   \
        if (++(guard) > 200000) {                                          \
            printf("FAIL %s:%d: runaway loop\n", __FILE__, __LINE__);      \
            ++failures;                                                    \
            break;                                                         \
        }                                                                  \
    } while (0)

int main(void) {
    CHECK(player_engine_abi_version() == PLAYER_ENGINE_ABI_VERSION);

    /* --- fail-closed creation probes: NULL config and zero capacity both
     * select the internal default; never a dead process. ---------------- */
    pe_engine *eng = pe_create(NULL);
    CHECK(eng != NULL);
    if (eng == NULL) return 1;

    pe_config cfg;
    memset(&cfg, 0, sizeof cfg);
    pe_engine *eng2 = pe_create(&cfg); /* capacity 0 -> default */
    CHECK(eng2 != NULL);
    if (eng2 != NULL) pe_destroy(eng2);

    /* --- product snapshot is time-domain, EMPTY at creation -------------- */
    pe_snapshot sn;
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_EMPTY);
    CHECK(sn.position_us == 0);
    CHECK(sn.duration_known == 0u);

    /* --- boundary probes: invalid caller input is a typed result, never
     * an abort / assert / uncaught exception. ---------------------------- */
    CHECK(pe_get_snapshot(NULL, &sn) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_get_snapshot(eng, NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_open(NULL, NULL, NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_open(eng, NULL, NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_play(NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_pause(NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_stop(NULL, NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_seek(NULL, 0, NULL, NULL) == PE_ERR_INVALID_ARGUMENT);
    CHECK(pe_seek(eng, 0, NULL, NULL) == PE_ERR_ILLEGAL_CALL); /* EMPTY */
    CHECK(pe_play(eng) == PE_ERR_ILLEGAL_CALL);                /* EMPTY */
    CHECK(pe_pause(eng) == PE_OK);                             /* no-op */
    CHECK(pe_seek(eng, -1000000, NULL, NULL) == PE_ERR_ILLEGAL_CALL); /* EMPTY */

    /* --- open a 4 s / 48 kHz synthetic song ------------------------------ */
    song_io io;
    CHECK(fake_song_make_io(&io, 48000 * 4, 48000, 2) == SONG_OK);
    CHECK(fake_song_make_io(NULL, 1, 1, 1) == SONG_ERR_INVALID_ARGUMENT);

    int32_t song_status = -1;
    CHECK(pe_open(eng, &io, &song_status) == PE_OK && song_status == SONG_OK);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK && sn.state == PE_STATE_READY);
    CHECK(sn.position_quality == PE_QUALITY_CONFIRMED);
    CHECK(sn.duration_known == 1u && sn.duration_us == 4000000);
    CHECK(sn.position_us == 0);
    CHECK(sn.sample_rate == 48000);

    /* --- play to ENDED via the test driver (backend stand-in) ------------ */
    CHECK(pe_play(eng) == PE_OK);
    int guard = 0;
    for (;;) {
        /* The drive may return idle once ENDED has landed: the ENDED
         * transition is performed by whichever side wins the final-drain
         * race — the drive's own submit, or the worker's EOF poll between
         * drives. Both are the same frozen transition; only an idle drive
         * that is NOT followed by ENDED is a fault. */
        const int drive_idle = pe_test_drive(eng, 512);
        CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
        if (sn.state == PE_STATE_ENDED) break;
        CHECK(drive_idle == 0); /* idle before ENDED = left PLAYING wrongly */
        CHECK_RUNAWAY(guard);
    }
    CHECK(sn.position_us == 4000000);      /* ENDED = full media duration */
    CHECK(sn.duration_us == 4000000);
    CHECK(sn.buffered_frames == 0);
    CHECK(sn.position_quality == PE_QUALITY_CONFIRMED);
    CHECK(strlen(sn.last_error) == 0);

    /* --- seek to midpoint: landing reported in media microseconds -------- */
    int64_t landing_us = -1;
    CHECK(pe_seek(eng, 2000000, &landing_us, &song_status) == PE_OK &&
          song_status == SONG_OK);
    CHECK(landing_us == 2000000);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_READY);
    CHECK(sn.position_us == 2000000);
    CHECK(sn.position_quality == PE_QUALITY_CONFIRMED);

    /* --- negative / huge seeks are typed results, never crashes ---------- */
    CHECK(pe_seek(eng, -1000000, &landing_us, NULL) == PE_OK); /* clamps to 0 */
    CHECK(landing_us == 0);
    CHECK(pe_seek(eng, (int64_t)1 << 40, &landing_us, NULL) == PE_OK); /* clamps */
    CHECK(landing_us == 4000000);

    /* --- re-home to mid-song so the play/pause test below is
     * deterministic: the end-boundary landing above leaves the source
     * exhausted, and playing an already-exhausted source races the
     * worker's EOF poll against the first drive — the engine correctly
     * ends (drain of zero media), but WHICH tick observes ENDED is a
     * timing artifact. From mid-song the play/pause outcome is fixed. --- */
    CHECK(pe_seek(eng, 1000000, &landing_us, NULL) == PE_OK);
    CHECK(landing_us == 1000000);

    /* --- play a little, pause, resume, then stop ------------------------- */
    CHECK(pe_play(eng) == PE_OK);
    CHECK(pe_test_drive(eng, 512) == 0);
    CHECK(pe_pause(eng) == PE_OK);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK && sn.state == PE_STATE_PAUSED);
    CHECK(pe_play(eng) == PE_OK);
    CHECK(pe_test_drive(eng, 512) == 0);

    /* --- stop() rebuilds to READY @0 ------------------------------------- */
    CHECK(pe_stop(eng, &song_status) == PE_OK && song_status == SONG_OK);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_READY && sn.position_us == 0);

    pe_destroy(eng);
    pe_destroy(NULL); /* documented no-op */

    if (failures != 0) {
        printf("CONSUMER FAIL: %d check(s) failed\n", failures);
        return 1;
    }
    printf("CONSUMER OK: product-only ABI lifecycle, time-domain snapshot, "
           "no-crash probes\n");
    return 0;
}
