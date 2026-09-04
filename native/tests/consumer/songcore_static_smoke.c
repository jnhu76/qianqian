/*
 * songcore_static_smoke.c — external static-archive consumer proof.
 *
 * This file is written the way an outside integrator would write it: it
 * sees ONLY include/songcore.h and the link line below. It knows nothing
 * about qianqian_av, the FFmpeg closure, or any Qianqian test binary.
 *
 * Documented external link line (whole point of the merged archive):
 *
 *   cc -Iinclude tests/consumer/songcore_static_smoke.c \
 *       -Lbuild/artifacts -lsongcore -lm -lpthread \
 *       -o build/consumer/songcore_static_smoke
 *
 * (Windows/mingw: swap -lm -lpthread for -lbcrypt.)
 *
 * Exit 0 = PASS. Any unexpected status aborts with a diagnostic.
 */
#include "songcore.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    FILE *fh;
    long size;
} file_source;

static int64_t fs_read(void *ud, uint8_t *dst, size_t size) {
    file_source *fs = ud;
    size_t got = fread(dst, 1, size, fs->fh);
    if (got == 0 && ferror(fs->fh)) return -1;
    return (int64_t)got;
}

static int64_t fs_seek(void *ud, int64_t absolute) {
    file_source *fs = ud;
    if (fseek(fs->fh, (long)absolute, SEEK_SET) != 0) return -1;
    return (int64_t)ftell(fs->fh);
}

static int64_t fs_size(void *ud) { return ((file_source *)ud)->size; }

#define REQUIRE(cond, msg)                                          \
    do {                                                            \
        if (!(cond)) {                                              \
            fprintf(stderr, "static_smoke: FAIL %s (line %d)\n",    \
                    msg, __LINE__);                                 \
            return 1;                                               \
        }                                                           \
    } while (0)

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: songcore_static_smoke <audio-file>\n");
        return 2;
    }

    REQUIRE(songcore_abi_version() == SONGCORE_ABI_VERSION, "abi version");

    FILE *fh = fopen(argv[1], "rb");
    REQUIRE(fh != NULL, "fopen");
    fseek(fh, 0, SEEK_END);
    long size = ftell(fh);
    fseek(fh, 0, SEEK_SET);
    file_source fs = { fh, size };

    song_io io;
    memset(&io, 0, sizeof(io));
    io.userdata = &fs;
    io.read = fs_read;
    io.seek = fs_seek;
    io.size = fs_size;

    song_handle *song = NULL;
    song_status st = song_open(&io, &song);
    REQUIRE(st == SONG_OK && song != NULL, "song_open");

    song_info info;
    st = song_probe(song, &info);
    REQUIRE(st == SONG_OK, "song_probe");
    REQUIRE(info.sample_rate > 0 && info.channels > 0, "probe sanity");

    /* one second of PCM at the source rate */
    uint64_t cap = (uint64_t)info.sample_rate;
    float *pcm = malloc(cap * (uint64_t)info.channels * sizeof(float));
    REQUIRE(pcm != NULL, "malloc");
    uint64_t produced = 0, total = 0;
    while (total < cap) {
        st = song_read_pcm(song, pcm, cap - total, &produced);
        REQUIRE(st == SONG_OK || st == SONG_EOF, "read_pcm typed");
        if (st == SONG_EOF) break;
        total += produced;
    }
    REQUIRE(total > 0, "decoded PCM");

    /* seek to mid-file; a typed refusal is acceptable for raw containers */
    int64_t actual = -1;
    st = song_seek(song, info.duration_us > 0 ? info.duration_us / 2 : 0,
                   &actual);
    REQUIRE(st == SONG_OK || st == SONG_ERR_SEEK_UNSUPPORTED, "seek typed");
    if (st == SONG_OK) {
        st = song_read_pcm(song, pcm, cap, &produced);
        REQUIRE(st == SONG_OK && produced > 0, "post-seek PCM");
    }

    free(pcm);
    song_close(song);
    fclose(fh);

    printf("static_smoke: PASS %s (%s/%s %d Hz %d ch, %llu frames decoded)\n",
           argv[1], info.container, info.codec, info.sample_rate,
           info.channels, (unsigned long long)total);
    return 0;
}
