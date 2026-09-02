/*
 * main.c — tiny external consumer of the PlayerEngine C ABI (§64-69).
 *
 * A pure-C99 translation unit that includes ONLY the C ABI headers
 * (player_engine.h + the stand-in filesystem factory), compiled by a C
 * compiler and linked against libplayer_core: proof that the frozen C
 * surface is self-contained, C++-leak-free, and ABI-linkable from a
 * foreign TU. Drives one full lifecycle with the threaded decode worker:
 * open -> play -> ENDED with snapshot conservation -> seek -> stop.
 */
#include <stdio.h>
#include <string.h>

#include "fake_songcore_c.h"
#include "player_engine.h"

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

static void check_conservation(const pe_snapshot *sn) {
    CHECK(sn->ring_produced_total ==
          sn->ring_consumed_total + sn->queued_media_frames +
              sn->ring_discarded_total);
    CHECK(sn->decoded_source_frames ==
          sn->ring_produced_total + sn->discarded_stale_media_frames +
              sn->in_flight_frames);
    CHECK(sn->submitted_output_frames ==
          sn->pending_output_frames + sn->rendered_output_frames +
              sn->discarded_output_frames);
    CHECK(sn->submitted_media_frames ==
          sn->pending_media_frames + sn->rendered_media_frames +
              sn->discarded_output_media_frames);
    CHECK(sn->rendered_output_frames ==
          sn->rendered_media_frames + sn->rendered_gap_output_frames);
}

int main(void) {
    CHECK(player_engine_abi_version() == PLAYER_ENGINE_ABI_VERSION);

    pe_config cfg;
    memset(&cfg, 0, sizeof cfg);
    cfg.capacity_frames = 4096;
    cfg.read_chunk_frames = 1024;
    cfg.max_submit_frames = 8192;
    cfg.max_channels = 2;
    cfg.worker_thread = 1;

    /* fail-closed creation probes */
    pe_config bad = cfg;
    bad.capacity_frames = 0;
    CHECK(pe_create(&bad) == NULL);
    bad = cfg;
    bad.read_chunk_frames = cfg.capacity_frames * 2;
    CHECK(pe_create(&bad) == NULL);
    CHECK(pe_create(NULL) == NULL);

    pe_engine *eng = pe_create(&cfg);
    CHECK(eng != NULL);
    if (eng == NULL) return 1; /* cannot continue without an engine */

    pe_snapshot sn;
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_EMPTY);
    CHECK(pe_get_snapshot(NULL, &sn) == PE_ERR_ILLEGAL_CALL);
    CHECK(pe_open(NULL, NULL, NULL) == PE_ERR_ILLEGAL_CALL);

    song_io io;
    CHECK(fake_song_make_io(&io, 48000 * 4, 48000, 2) == SONG_OK);
    CHECK(fake_song_make_io(NULL, 1, 1, 1) == SONG_ERR_INVALID_ARGUMENT);

    int32_t song_status = -1;
    CHECK(pe_open(eng, &io, &song_status) == PE_OK && song_status == SONG_OK);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK && sn.state == PE_STATE_READY);
    CHECK(sn.position_quality == PE_QUALITY_CONFIRMED);
    CHECK(sn.duration_known == 1u && sn.duration_frames == 48000 * 4);
    CHECK(sn.media_position_frames == 0 && sn.segment == 1);
    check_conservation(&sn);

    CHECK(pe_play(eng) == PE_OK);

    /* lockstep device: drive a complete playout to ENDED */
    int guard = 0;
    for (;;) {
        pe_submit(eng, 512);
        pe_render(eng, 512, PE_CURRENT_GENERATION);
        CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
        if (sn.state == PE_STATE_ENDED) break;
        CHECK_RUNAWAY(guard);
    }
    CHECK(sn.media_position_frames == sn.duration_frames);
    CHECK(sn.pending_media_frames == 0);
    CHECK(sn.queued_media_frames == 0);
    CHECK(sn.source_eof == 1u);
    CHECK(sn.rendered_media_frames > 0);
    check_conservation(&sn);

    /* seek reopens a segment; the landing is CONFIRMED and nonzero */
    int64_t landing = -1;
    CHECK(pe_seek(eng, 1000000, &landing, &song_status) == PE_OK &&
          song_status == SONG_OK);
    CHECK(landing >= 0);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_READY && sn.segment == 2);
    CHECK(sn.position_quality == PE_QUALITY_CONFIRMED);
    CHECK(sn.media_position_frames == landing);
    check_conservation(&sn);

    /* stop() rebuilds to READY @0 */
    CHECK(pe_stop(eng, &song_status) == PE_OK && song_status == SONG_OK);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_READY && sn.media_position_frames == 0);
    check_conservation(&sn);

    pe_destroy(eng);
    pe_destroy(NULL); /* documented no-op */

    if (failures != 0) {
        printf("CONSUMER FAIL: %d check(s) failed\n", failures);
        return 1;
    }
    printf("CONSUMER OK: C ABI lifecycle, snapshot conservation, fail-closed probes\n");
    return 0;
}
