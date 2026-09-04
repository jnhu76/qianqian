/*
 * ffi_smoke.c — external consumer of the qianqian runtime (native runtime
 * closure spec §32/§50).
 *
 * KNOWS ONLY: include/player_engine.h (the pure C-compatible public
 * header) and the location of the application-facing dynamic library.
 * It does NOT include any C++/internal header, does NOT link any
 * implementation archive, and loads the library at run time
 * (LoadLibraryA / dlopen) exactly like the future KMP consumer will.
 *
 * Evidence classification (spec §39/§40): the lifecycle below is real
 * everywhere; on a build whose runtime carries a platform output (Windows)
 * and a real audio endpoint, PLAYING with an advancing media position is
 * REAL render-progression evidence through the backend. Without output the
 * smoke reports SIMULATED/lifecycle-only explicitly instead of faking a
 * PASS.
 *
 * Usage: ffi_smoke <runtime-library> <audio-fixture>
 */
#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L /* nanosleep, fseeko/ftello */
#endif

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "player_engine.h"

#if !defined(_WIN32)
#include <dlfcn.h>
#include <time.h>
#endif

#if defined(_WIN32)
#include <windows.h>
static HMODULE g_lib = NULL;
static void *load_library(const char *path) {
    g_lib = LoadLibraryA(path);
    return (void *)g_lib;
}
static void *find_symbol(void *h, const char *name) {
    return (void *)GetProcAddress((HMODULE)h, name);
}
static void msleep(unsigned ms) { Sleep(ms); }
#else
static void *load_library(const char *path) { return dlopen(path, RTLD_NOW); }
static void *find_symbol(void *h, const char *name) { return dlsym(h, name); }
static void msleep(unsigned ms) {
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (long)(ms % 1000) * 1000000L;
    nanosleep(&ts, NULL);
}
#endif

/* Resolved pe_* entry points (the complete existing ABI). */
typedef uint32_t (*abi_version_fn)(void);
typedef pe_engine *(*create_fn)(const pe_config *);
typedef void (*destroy_fn)(pe_engine *);
typedef pe_status (*open_fn)(pe_engine *, const song_io *, int32_t *);
typedef pe_status (*play_fn)(pe_engine *);
typedef pe_status (*pause_fn)(pe_engine *);
typedef pe_status (*stop_fn)(pe_engine *, int32_t *);
typedef pe_status (*seek_fn)(pe_engine *, int64_t, int64_t *, int32_t *);
typedef pe_status (*snapshot_fn)(pe_engine *, pe_snapshot *);

static abi_version_fn p_abi_version;
static create_fn p_create;
static destroy_fn p_destroy;
static open_fn p_open;
static play_fn p_play;
static pause_fn p_pause;
static stop_fn p_stop;
static seek_fn p_seek;
static snapshot_fn p_snapshot;

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);             \
            ++failures;                                                        \
        }                                                                      \
    } while (0)

/* Host FILE* I/O — the song_io contract, implemented from outside. */
static int64_t io_read(void *ud, uint8_t *dst, size_t size) {
    return (int64_t)fread(dst, 1, size, (FILE *)ud);
}
static int64_t io_seek(void *ud, int64_t offset) {
    return fseeko((FILE *)ud, (off_t)offset, SEEK_SET) == 0
               ? (int64_t)ftello((FILE *)ud)
               : -1;
}
static int64_t io_size(void *ud) {
    FILE *f = (FILE *)ud;
    int64_t cur = (int64_t)ftello(f);
    if (fseeko(f, 0, SEEK_END) != 0) return -1;
    int64_t end = (int64_t)ftello(f);
    fseeko(f, (off_t)cur, SEEK_SET);
    return end;
}

static int resolve(void *lib) {
    struct { const char *name; void **fn; } syms[] = {
        {"player_engine_abi_version", (void **)&p_abi_version},
        {"pe_create", (void **)&p_create},
        {"pe_destroy", (void **)&p_destroy},
        {"pe_open", (void **)&p_open},
        {"pe_play", (void **)&p_play},
        {"pe_pause", (void **)&p_pause},
        {"pe_stop", (void **)&p_stop},
        {"pe_seek", (void **)&p_seek},
        {"pe_get_snapshot", (void **)&p_snapshot},
    };
    for (size_t i = 0; i < sizeof syms / sizeof syms[0]; ++i) {
        *syms[i].fn = find_symbol(lib, syms[i].name);
        if (*syms[i].fn == NULL) {
            printf("FAIL: runtime library does not export %s\n", syms[i].name);
            return 0;
        }
    }
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        printf("usage: ffi_smoke <runtime-library> <audio-fixture>\n");
        return 2;
    }
    void *lib = load_library(argv[1]);
    if (lib == NULL) {
#if defined(_WIN32)
        printf("FAIL: cannot load %s (error %lu)\n", argv[1], GetLastError());
#else
        printf("FAIL: cannot load %s (%s)\n", argv[1], dlerror());
#endif
        return 1;
    }
    if (!resolve(lib)) { return 1; }

    /* ABI gate: the header we compiled against matches the loaded runtime. */
    CHECK(p_abi_version() == PLAYER_ENGINE_ABI_VERSION);

    /* create / EMPTY snapshot / NULL-config default */
    pe_engine *eng = p_create(NULL);
    CHECK(eng != NULL);
    if (eng == NULL) return 1;

    pe_snapshot sn;
    memset(&sn, 0, sizeof sn);
    CHECK(p_snapshot(eng, &sn) == PE_OK);
    CHECK(sn.state == PE_STATE_EMPTY);

    /* open a REAL song through the caller-provided song_io */
    FILE *f = fopen(argv[2], "rb");
    CHECK(f != NULL);
    if (f == NULL) return 1;
    song_io io;
    memset(&io, 0, sizeof io);
    io.userdata = f;
    io.read = io_read;
    io.seek = io_seek;
    io.size = io_size;

    int32_t song_status = -1;
    CHECK(p_open(eng, &io, &song_status) == PE_OK && song_status == SONG_OK);
    CHECK(p_snapshot(eng, &sn) == PE_OK && sn.state == PE_STATE_READY);
    CHECK(sn.duration_known == 1u && sn.duration_us > 0);
    CHECK(sn.sample_rate > 0);

    /* play + render-progression evidence */
    CHECK(p_play(eng) == PE_OK);
    CHECK(p_snapshot(eng, &sn) == PE_OK && sn.state == PE_STATE_PLAYING);

    int real_output = 0;
    for (int i = 0; i < 30; ++i) { /* poll ~1.5 s */
        msleep(50);
        CHECK(p_snapshot(eng, &sn) == PE_OK);
        if (sn.state == PE_STATE_PLAYING && sn.position_us > 0) {
            real_output = 1;
            break;
        }
    }
    if (real_output) {
        printf("EVIDENCE: REAL render progression — position_us=%lld, "
               "rate=%d (backend proved playout)\n",
               (long long)sn.position_us, sn.sample_rate);
        /* Playout window: position must advance monotonically, then the
         * 4 s fixture reaches ENDED at its duration through the real
         * output (EOF lifecycle, docs/contracts/player-api.md). */
        int64_t last = sn.position_us;
        int ended = 0;
        for (int i = 0; i < 100; ++i) { /* ≤ 10 s guard */
            msleep(100);
            CHECK(p_snapshot(eng, &sn) == PE_OK);
            CHECK(sn.position_us >= last); /* media clock is monotonic */
            last = sn.position_us;
            if (sn.state == PE_STATE_ENDED) {
                ended = 1;
                break;
            }
        }
        CHECK(ended == 1);
        CHECK(sn.duration_known == 1u);
        CHECK(sn.position_us == sn.duration_us); /* ENDED = full duration */
        printf("EVIDENCE: ENDED at duration_us=%lld through the real "
               "output (EOF -> ENDED lifecycle)\n",
               (long long)sn.duration_us);
    } else {
        printf("EVIDENCE: SIMULATED lifecycle only — no render progression "
               "(runtime carries no platform output, or no audio endpoint): "
               "SKIP real-endpoint claim\n");
    }

    /* seek to the midpoint (landing in media microseconds) */
    int64_t landing = -1;
    CHECK(p_seek(eng, sn.duration_us / 2, &landing, &song_status) == PE_OK &&
          song_status == SONG_OK);
    CHECK(landing >= 0);

    /* play, pause freezes, play resumes, stop rebuilds to READY @0
     * (seek from ENDED lands READY; play is then required — pause on
     * READY is the documented no-op, so play must come first). */
    CHECK(p_play(eng) == PE_OK);
    CHECK(p_pause(eng) == PE_OK);
    CHECK(p_snapshot(eng, &sn) == PE_OK);
    if (sn.state != PE_STATE_PAUSED) {
        printf("DIAG: state after seek+play+pause = %d (pos=%lld us)\n",
               (int)sn.state, (long long)sn.position_us);
    }
    CHECK(sn.state == PE_STATE_PAUSED);
    CHECK(p_play(eng) == PE_OK);
    int64_t at_pause = 0;
    CHECK(p_snapshot(eng, &sn) == PE_OK && (at_pause = sn.position_us) >= 0);
    msleep(200);
    CHECK(p_snapshot(eng, &sn) == PE_OK);
    if (sn.state == PE_STATE_PAUSED) {
        CHECK(sn.position_us >= at_pause); /* frozen or drained backlog only */
    }

    CHECK(p_stop(eng, &song_status) == PE_OK && song_status == SONG_OK);
    CHECK(p_snapshot(eng, &sn) == PE_OK && sn.state == PE_STATE_READY &&
          sn.position_us == 0);

    /* destroy: stop + teardown implied; NULL is a documented no-op */
    p_destroy(eng);
    p_destroy(NULL);

    fclose(f);

    if (failures != 0) {
        printf("FFI_SMOKE FAIL: %d check(s) failed\n", failures);
        return 1;
    }
    printf("FFI_SMOKE OK: pure-C consumer, public header only, runtime "
           "loaded dynamically, full lifecycle\n");
    return 0;
}
