/*
 * seek_probe.c — Lane C diagnostic consumer (package-only build).
 *
 * Characterizes song_seek over a list of target positions, either on a fresh
 * handle per target ("fresh") or on one handle reused across targets ("same").
 * This exists only to explain observed seek results; it is diagnostic, not a
 * G2 gate.
 *
 * usage: seek_probe <mode:fresh|same> <file> <target_us> ...
 */
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "songcore.h"

static int64_t host_read(void *ud, uint8_t *dst, size_t size) {
    size_t n = fread(dst, 1, size, (FILE *)ud);
    if (n > 0) return (int64_t)n;
    return feof((FILE *)ud) ? 0 : -1;
}
static int64_t host_seek(void *ud, int64_t absolute_offset) {
    if (fseek((FILE *)ud, (long)absolute_offset, SEEK_SET) != 0) return -1;
    return (int64_t)ftell((FILE *)ud);
}
static int64_t host_size(void *ud) {
    FILE *f = (FILE *)ud;
    long cur = ftell(f), end;
    if (fseek(f, 0, SEEK_END) != 0) return -1;
    end = ftell(f);
    fseek(f, cur, SEEK_SET);
    return (int64_t)end;
}

int main(int argc, char **argv) {
    const char *mode, *path;
    int same;
    if (argc < 4) {
        fprintf(stderr, "usage: %s <fresh|same> <file> <target_us> ...\n", argv[0]);
        return 2;
    }
    mode = argv[1];
    path = argv[2];
    same = (strcmp(mode, "same") == 0);

    FILE *f = NULL;
    song_io io;
    song_handle *h = NULL;
    song_info info;
    float *buf = NULL;

    if (same) {
        f = fopen(path, "rb");
        if (!f) return 2;
        io.userdata = f; io.read = host_read; io.seek = host_seek; io.size = host_size;
        if (song_open(&io, &h) != SONG_OK || song_probe(h, &info) != SONG_OK) return 1;
        buf = (float *)malloc(sizeof(float) * 4096 * (size_t)info.channels);
        printf("{\"same_handle\":1,\"codec\":\"%s\",\"rate\":%d,\"channels\":%d,\"duration_us\":%" PRId64 "}\n",
               info.codec, info.sample_rate, info.channels, info.duration_us);
    }

    for (int i = 3; i < argc; i++) {
        int64_t target = (int64_t)atoll(argv[i]);
        int64_t landing = -2;
        song_status st;
        if (!same) {
            f = fopen(path, "rb");
            if (!f) return 2;
            io.userdata = f; io.read = host_read; io.seek = host_seek; io.size = host_size;
            if (song_open(&io, &h) != SONG_OK) { fclose(f); continue; }
            if (song_probe(h, &info) != SONG_OK) { song_close(h); fclose(f); continue; }
            buf = (float *)malloc(sizeof(float) * 4096 * (size_t)info.channels);
        }
        st = song_seek(h, target, &landing);
        printf("{\"mode\":\"%s\",\"target_us\":%" PRId64 ",\"seek_status\":%d,\"landing_us\":%" PRId64,
               mode, target, (int)st, landing);
        if (st == SONG_OK) {
            uint64_t got = 0;
            song_status rs = song_read_pcm(h, buf, 4096, &got);
            int64_t drain_frames = 0;
            const char *terminal = "none";
            printf(",\"read_status\":%d,\"read_produced\":%" PRIu64, (int)rs, got);
            if (rs == SONG_OK) {
                for (;;) {
                    uint64_t g2 = 0;
                    song_status s2 = song_read_pcm(h, buf, 4096, &g2);
                    if (s2 == SONG_EOF) { terminal = "SONG_EOF"; if (g2 != 0) terminal = "EOF_WITH_FRAMES"; break; }
                    if (s2 != SONG_OK) { terminal = "ERROR"; break; }
                    if (g2 == 0) { terminal = "OK_ZERO"; break; }
                    drain_frames += (int64_t)g2;
                }
            }
            printf(",\"drain_terminal\":\"%s\",\"frames_after_landing\":%" PRId64, terminal, drain_frames);
        }
        printf("}\n");
        free(buf); buf = NULL;
        if (!same) { song_close(h); fclose(f); }
    }
    if (same) { free(buf); song_close(h); fclose(f); }
    return 0;
}
