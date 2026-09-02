/*
 * real_songcore_smoke.c — PlayerEngine over
 * REAL SongCore + REAL corpus fixtures + host file I/O.
 *
 * Every other player gate links the SongCore stand-in (link-time C ABI
 * substitution). This one is the P1-B "real SongCore integration" proof:
 * it links libplayer_core against the REAL SongCore archive, opens actual
 * corpus FLAC/MP3 fixtures through host FILE* I/O, and drives the frozen
 * product lifecycle end to end with the NullAudioBackend behind the scenes:
 *
 *   open -> play -> ENDED at the real media duration (time-domain
 *   snapshot: position_us == duration_us == the fixture's true duration)
 *   -> seek to the real midpoint with a CONFIRMED landing -> play a bit
 *   (media position provably advances) -> stop -> READY @0.
 *
 * Error path: a missing file fails pe_open with a typed status
 * (PE_ERR_OPEN_FAILED + non-SONG_OK song status) — never a crash, assert,
 * or abort. No FFmpeg type appears anywhere; only songcore.h + the product
 * C ABI (include/player_engine.h) cross this TU.
 *
 * Test-only: links a test-only backend driver (player_test_driver.h) in
 * place of WASAPI's callback and never ships. Host I/O is the same
 * FILE*-backed song_io pattern as tests/songcore/songcore_probe.c.
 */
#if !defined(_WIN32)
#define _FILE_OFFSET_BITS 64
#define _POSIX_C_SOURCE 200809L
#endif

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#if defined(_WIN32)
#include <windows.h>
#else
#include <time.h>
#endif

#include "player_engine.h"
#include "player_test_driver.h"

/* -------------------------------------------------------------------------
 * Host I/O (FILE*-backed song_io; pattern from songcore_probe.c)
 * ---------------------------------------------------------------------- */

typedef struct file_source {
    FILE *file;
    int64_t size;
} file_source;

#if defined(_WIN32)
static int file_seek64(FILE *f, int64_t off, int whence) { return _fseeki64(f, off, whence); }
static int64_t file_tell64(FILE *f) { return _ftelli64(f); }
#else
static int file_seek64(FILE *f, int64_t off, int whence) { return fseeko(f, (off_t)off, whence); }
static int64_t file_tell64(FILE *f) { return (int64_t)ftello(f); }
#endif

static int64_t host_read(void *userdata, uint8_t *dst, size_t size) {
    file_source *src = (file_source *)userdata;
    if (!src->file) return -1;
    size_t n = fread(dst, 1, size, src->file);
    if (n == 0 && ferror(src->file)) return -1;
    return (int64_t)n;
}

static int64_t host_seek(void *userdata, int64_t absolute_offset) {
    file_source *src = (file_source *)userdata;
    if (!src->file || absolute_offset < 0 ||
        file_seek64(src->file, absolute_offset, SEEK_SET) != 0)
        return -1;
    return file_tell64(src->file);
}

static int64_t host_size(void *userdata) {
    file_source *src = (file_source *)userdata;
    return src->file ? src->size : 0;
}

/* Opens `path`; returns 0 and fills *src/*io on success, -1 otherwise. */
static int open_file_io(const char *path, file_source *src, song_io *io) {
    memset(src, 0, sizeof(*src));
    src->file = fopen(path, "rb");
    if (!src->file) return -1;
    if (file_seek64(src->file, 0, SEEK_END) != 0 ||
        (src->size = file_tell64(src->file)) < 0 ||
        file_seek64(src->file, 0, SEEK_SET) != 0) {
        fclose(src->file);
        src->file = NULL;
        return -1;
    }
    memset(io, 0, sizeof(*io));
    io->userdata = src;
    io->read = host_read;
    io->seek = host_seek;
    io->size = host_size;
    return 0;
}

static void close_file_io(file_source *src) {
    if (src->file) {
        fclose(src->file);
        src->file = NULL;
    }
}

/* -------------------------------------------------------------------------
 * Realistic drive pacing.
 *
 * The test driver (player_test_driver.h) stands in for WASAPI's callback by
 * calling the engine's LOCKED submit/render wrappers. A zero-pause tight
 * loop hammers the engine control mutex and starves the decode worker —
 * an artifact of the stand-in, not of production: a real audio callback
 * fires at ~10 ms periods on the lock-free realtime seam (never the control
 * mutex), so the worker always gets CPU. Pace the stand-in like a real
 * device (~1 ms between periods) and the worker keeps up (verified: 0
 * underruns, ENDED at the true duration).
 * ---------------------------------------------------------------------- */

static void pace_ms(long ms) {
#if defined(_WIN32)
    Sleep((DWORD)ms);
#else
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (long)(ms % 1000) * 1000000L;
    nanosleep(&ts, NULL);
#endif
}

/* -------------------------------------------------------------------------
 * Checks
 * ---------------------------------------------------------------------- */

static int failures = 0;

#define CHECK(cond)                                                        \
    do {                                                                   \
        if (!(cond)) {                                                     \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);         \
            ++failures;                                                    \
        }                                                                  \
    } while (0)

#define CHECK_MSG(cond, ...)                                               \
    do {                                                                   \
        if (!(cond)) {                                                     \
            printf("FAIL %s:%d: ", __FILE__, __LINE__);                    \
            printf(__VA_ARGS__);                                           \
            printf("\n");                                                  \
            ++failures;                                                    \
        }                                                                  \
    } while (0)

/* One full lifecycle over a real fixture. */
static void run_lifecycle(const char *path, const char *label,
                          int64_t expected_duration_us) {
    printf("[%s] %s\n", label, path);

    pe_engine *eng = pe_create(NULL);
    CHECK_MSG(eng != NULL, "%s: pe_create", label);
    if (!eng) return;

    file_source src;
    song_io io;
    CHECK_MSG(open_file_io(path, &src, &io) == 0, "%s: host open", label);

    int32_t song_status = -1;
    pe_status st = pe_open(eng, &io, &song_status);
    CHECK_MSG(st == PE_OK && song_status == SONG_OK,
              "%s: pe_open -> %d / song %d", label, (int)st, (int)song_status);

    pe_snapshot sn;
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK_MSG(sn.state == PE_STATE_READY, "%s: state after open", label);
    CHECK_MSG(sn.duration_known == 1u && sn.duration_us == expected_duration_us,
              "%s: duration %lld us (want %lld)", label,
              (long long)sn.duration_us, (long long)expected_duration_us);
    CHECK_MSG(sn.position_us == 0, "%s: starts at 0", label);
    CHECK_MSG(sn.position_quality == PE_QUALITY_CONFIRMED,
              "%s: fresh-handle landing is CONFIRMED", label);
    CHECK_MSG(sn.sample_rate == 44100, "%s: sample_rate %d", label,
              (int)sn.sample_rate);

    /* play -> ENDED at the REAL media duration. */
    CHECK(pe_play(eng) == PE_OK);
    int guard = 0;
    for (;;) {
        /* The drive may return idle once ENDED has landed: the ENDED
         * transition is performed by whichever side wins the final-drain
         * race — the drive's own submit, or the worker's EOF poll between
         * drives. Both are the same frozen transition; only an idle drive
         * that is NOT followed by ENDED is a fault. */
        const int drive_idle = pe_test_drive(eng, 1024);
        pace_ms(1); /* let the decode worker run, like a real audio callback */
        CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
        if (sn.state == PE_STATE_ENDED) break;
        CHECK_MSG(drive_idle == 0, "%s: drive idle before ENDED", label);
        CHECK_MSG(++guard < 5000, "%s: ENDED never reached", label);
        if (guard >= 5000) break;
    }
    CHECK_MSG(sn.state == PE_STATE_ENDED, "%s: final state", label);
    CHECK_MSG(sn.position_us == expected_duration_us,
              "%s: ENDED @ %lld us (want %lld)", label,
              (long long)sn.position_us, (long long)expected_duration_us);
    CHECK_MSG(sn.duration_us == expected_duration_us, "%s: duration stable", label);
    CHECK_MSG(strlen(sn.last_error) == 0, "%s: no error at ENDED", label);

    /* seek to the real midpoint. SongCore does NOT promise sample-exact
     * landings (songcore.h): lossless lands at/before the target, lossy
     * within the bounded codec-frame tolerance (65536 samples, the
     * regression authority). The engine reports SongCore's ACTUAL landing
     * and rebases the clock on it. */
    const int64_t mid_us = expected_duration_us / 2;
    const int64_t tol_us =
        (int64_t)(65536 * 1000000LL / (sn.sample_rate > 0 ? sn.sample_rate : 44100));
    int64_t landing_us = -1;
    song_status = -1;
    CHECK(pe_seek(eng, mid_us, &landing_us, &song_status) == PE_OK);
    CHECK_MSG(song_status == SONG_OK, "%s: song_seek status", label);
    CHECK_MSG(landing_us != -1, "%s: landing known", label);
    CHECK_MSG(landing_us <= mid_us + tol_us && landing_us >= mid_us - tol_us,
              "%s: landing %lld outside tolerance of %lld (target %lld)", label,
              (long long)landing_us, (long long)tol_us, (long long)mid_us);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK_MSG(sn.state == PE_STATE_READY, "%s: state after seek", label);
    CHECK_MSG(sn.position_us == landing_us,
              "%s: position %lld rebased on landing %lld", label,
              (long long)sn.position_us, (long long)landing_us);
    CHECK_MSG(sn.position_quality == PE_QUALITY_CONFIRMED,
              "%s: seek landing CONFIRMED", label);

    /* play a bit: media position must provably advance past the landing. */
    CHECK(pe_play(eng) == PE_OK);
    for (int i = 0; i < 8; ++i) {
        CHECK(pe_test_drive(eng, 1024) == 0);
        pace_ms(1);
    }
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK_MSG(sn.state == PE_STATE_PLAYING, "%s: still playing", label);
    CHECK_MSG(sn.position_us > landing_us,
              "%s: position advanced to %lld (landing %lld)", label,
              (long long)sn.position_us, (long long)landing_us);

    /* stop -> deterministic READY @0. */
    song_status = -1;
    CHECK(pe_stop(eng, &song_status) == PE_OK);
    CHECK_MSG(song_status == SONG_OK, "%s: stop song status", label);
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK_MSG(sn.state == PE_STATE_READY && sn.position_us == 0,
              "%s: READY @0 after stop", label);

    close_file_io(&src);
    pe_destroy(eng);
}

/* Missing file: typed failure, never a crash. */
static void run_missing_file(void) {
    file_source src;
    song_io io;
    /* A song_io whose host file does not exist. */
    memset(&src, 0, sizeof(src));
    src.file = NULL; /* never opened */
    memset(&io, 0, sizeof(io));
    io.userdata = &src;
    io.read = host_read;
    io.seek = host_seek;
    io.size = host_size;

    pe_engine *eng = pe_create(NULL);
    CHECK_MSG(eng != NULL, "missing-file: pe_create");
    if (!eng) return;

    int32_t song_status = SONG_OK;
    pe_status st = pe_open(eng, &io, &song_status);
    CHECK_MSG(st == PE_ERR_OPEN_FAILED, "missing-file: status %d", (int)st);
    CHECK_MSG(song_status != SONG_OK, "missing-file: song status %d",
              (int)song_status);

    /* Engine remains usable: snapshot still readable, no dead state. */
    pe_snapshot sn;
    CHECK(pe_get_snapshot(eng, &sn) == PE_OK);
    CHECK_MSG(sn.state == PE_STATE_READY || sn.state == PE_STATE_EMPTY,
              "missing-file: sane state after failed open");
    CHECK(strlen(sn.last_error) > 0); /* a diagnostic is required */

    pe_destroy(eng);
}

int main(int argc, char **argv) {
    /* Optional argv overrides; defaults are repo-relative corpus fixtures. */
    const char *flac = argc > 1 ? argv[1]
                                : "corpus/fixtures/flac-16-44-stereo.flac";
    const char *mp3 = argc > 2 ? argv[2] : "corpus/fixtures/mp3-short.mp3";

    run_lifecycle(flac, "flac-16-44-stereo", 4000000);
    run_lifecycle(mp3, "mp3-short", 300000);
    run_missing_file();

    if (failures != 0) {
        printf("REAL-SONGCORE SMOKE FAIL: %d check(s) failed\n", failures);
        return 1;
    }
    printf("REAL-SONGCORE SMOKE OK: real SongCore + real fixtures + host I/O, "
           "ENDED @ duration, seek landing, error path\n");
    return 0;
}
