/*
 * robustness_probe.c — Lane C (linux) package consumer for S-BIND 7.1 step 12
 * (malformed / truncated source: typed fail-closed statuses, no fabricated
 * data). Built ONLY against the unpacked package.
 *
 * usage: robustness_probe <label> <path> [<label> <path> ...]
 *
 * Fail-closed rule applied here:
 *   - open or probe returns a typed error (>= 100), or
 *   - the decode terminal is SONG_EOF with produced == 0, or a typed error
 *     with produced == 0;
 *   never SONG_OK with zero frames, never frames after a terminal error.
 * Exit 0 when every case obeyed the rule, 1 otherwise.
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
    int bad = 0, cases = 0;
    if (argc < 3 || (argc % 2) == 0) {
        fprintf(stderr, "usage: %s <label> <path> [<label> <path> ...]\n", argv[0]);
        return 2;
    }
    for (int i = 1; i + 1 < argc; i += 2) {
        const char *label = argv[i], *path = argv[i + 1];
        FILE *f = fopen(path, "rb");
        song_io io;
        song_handle *h = NULL;
        song_info info;
        song_status open_st, probe_st = SONG_ERR_NOT_OPEN, term_st = SONG_OK;
        uint64_t frames = 0, produced_at_terminal = 0;
        const char *terminal = "none";
        int fail_closed = 1, fabricated = 0;
        cases++;

        if (!f) {
            printf("{\"label\":\"%s\",\"path\":\"%s\",\"case\":\"unreadable\"}\n", label, path);
            bad++;
            continue;
        }
        io.userdata = f; io.read = host_read; io.seek = host_seek; io.size = host_size;
        open_st = song_open(&io, &h);
        if (open_st == SONG_OK) {
            probe_st = song_probe(h, &info);
            if (probe_st == SONG_OK) {
                uint64_t blk = 4096;
                float *buf = (float *)malloc(sizeof(float) * (size_t)blk * (size_t)info.channels);
                for (;;) {
                    uint64_t got = 0;
                    song_status s = song_read_pcm(h, buf, blk, &got);
                    if (s == SONG_EOF) {
                        terminal = "SONG_EOF";
                        produced_at_terminal = got;
                        term_st = s;
                        if (got != 0) fabricated = 1;
                        break;
                    }
                    if (s != SONG_OK) {
                        terminal = "ERROR";
                        produced_at_terminal = got;
                        term_st = s;
                        if (got != 0) fabricated = 1;   /* frames after terminal error */
                        break;
                    }
                    if (got == 0) { terminal = "SONG_OK_ZERO"; term_st = s; break; }
                    frames += got;
                }
                free(buf);
            }
            song_close(h);
        }
        fclose(f);

        if (open_st >= 100 || probe_st >= 100) {
            fail_closed = 1;
        } else if (open_st == SONG_OK && probe_st == SONG_OK) {
            fail_closed = (term_st == SONG_EOF && produced_at_terminal == 0) ||
                          (term_st >= 100 && produced_at_terminal == 0);
        }
        if (fabricated) fail_closed = 0;
        if (!fail_closed) bad++;

        printf("{\"label\":\"%s\",\"open_status\":%d,\"probe_status\":%d,"
               "\"terminal\":\"%s\",\"terminal_status\":%d,\"produced_at_terminal\":%" PRIu64
               ",\"frames_before_terminal\":%" PRIu64 ",\"fail_closed\":%u}\n",
               label, (int)open_st, (int)probe_st, terminal, (int)term_st,
               produced_at_terminal, frames, fail_closed ? 1u : 0u);
    }
    printf("{\"cases\":%d,\"violations\":%d,\"verdict\":\"%s\"}\n", cases, bad,
           bad == 0 ? "PASS" : "FAIL");
    return bad == 0 ? 0 : 1;
}
