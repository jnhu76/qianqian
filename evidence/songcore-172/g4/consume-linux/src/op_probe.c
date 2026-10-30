/*
 * op_probe.c — Lane C diagnostic consumer (package-only build).
 *
 * Runs an explicit op script against ONE handle so a seek anomaly can be
 * characterized without guessing:
 *   s:<us>   song_seek(<us>)
 *   r:<n>    song_read_pcm(n frames)
 *   d        read_pcm until EOF/error (drain)
 *   m        re-read the metadata snapshot (validity across seek/EOF)
 *
 * usage: op_probe <file> <op>...
 * Prints one JSON line per op. Exit 0 unless a call is made on a closed handle.
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
    if (argc < 3) {
        fprintf(stderr, "usage: %s <file> <op>...   ops: s:<us> r:<n> d m\n", argv[0]);
        return 2;
    }
    const char *path = argv[1];
    FILE *f = fopen(path, "rb");
    if (!f) return 2;
    song_io io = { f, host_read, host_seek, host_size };
    song_handle *h = NULL;
    song_info info;
    if (song_open(&io, &h) != SONG_OK) { fclose(f); return 1; }
    if (song_probe(h, &info) != SONG_OK) { song_close(h); fclose(f); return 1; }
    printf("{\"file\":\"%s\",\"codec\":\"%s\",\"rate\":%d,\"channels\":%d,\"duration_us\":%" PRId64 "}\n",
           path, info.codec, info.sample_rate, info.channels, info.duration_us);

    float *buf = (float *)malloc(sizeof(float) * 65536u * (unsigned)info.channels);
    if (!buf) return 1;

    for (int i = 2; i < argc; i++) {
        const char *op = argv[i];
        if (op[0] == 's' && op[1] == ':') {
            int64_t target = (int64_t)atoll(op + 2), landing = -2;
            song_status st = song_seek(h, target, &landing);
            printf("{\"op\":\"seek\",\"target_us\":%" PRId64 ",\"status\":%d,\"landing_us\":%" PRId64 "}\n",
                   target, (int)st, landing);
        } else if (op[0] == 'r' && op[1] == ':') {
            uint64_t n = (uint64_t)atoll(op + 2), got = 0;
            song_status st = song_read_pcm(h, buf, n, &got);
            printf("{\"op\":\"read\",\"capacity\":%" PRIu64 ",\"status\":%d,\"produced\":%" PRIu64 "}\n",
                   n, (int)st, got);
        } else if (op[0] == 'd') {
            uint64_t total = 0, calls = 0;
            const char *terminal = "unknown";
            song_status st = SONG_OK;
            for (;;) {
                uint64_t got = 0;
                st = song_read_pcm(h, buf, 4096, &got);
                calls++;
                if (st == SONG_EOF) { terminal = (got == 0) ? "SONG_EOF" : "SONG_EOF_WITH_FRAMES"; break; }
                if (st != SONG_OK) { terminal = "ERROR"; break; }
                if (got == 0) { terminal = "OK_ZERO"; break; }
                total += got;
            }
            printf("{\"op\":\"drain\",\"terminal\":\"%s\",\"status\":%d,\"frames\":%" PRIu64
                   ",\"calls\":%" PRIu64 "}\n", terminal, (int)st, total, calls);
        } else if (op[0] == 'm') {
            const song_metadata *meta = NULL;
            song_info i2;
            uint32_t n = 0;
            song_status st = song_get_metadata(h, &meta);
            song_status ps = song_probe(h, &i2);
            song_get_metadata_count(h, &n);
            printf("{\"op\":\"metadata\",\"status\":%d,\"has_title\":%u,\"title\":\"%.*s\",\"count\":%u,"
                   "\"probe_status\":%d,\"rate\":%d,\"channels\":%d,\"codec\":\"%s\",\"container\":\"%s\","
                   "\"selected_audio_index\":%u,\"audio_stream_count\":%u,\"duration_us\":%" PRId64 "}\n",
                   (int)st, meta ? meta->has_title : 0u,
                   (meta && meta->has_title) ? (int)meta->title_len : 0,
                   (meta && meta->has_title) ? meta->title : "", n,
                   (int)ps, i2.sample_rate, i2.channels, i2.codec, i2.container,
                   i2.selected_audio_index, i2.audio_stream_count, i2.duration_us);
        } else if (op[0] == 'e') {
            const song_error *err = NULL;
            song_status st = song_last_error(h, &err);
            printf("{\"op\":\"last_error\",\"status\":%d,\"native_code\":%d,\"message\":\"%.*s\"}\n",
                   (int)st, err ? err->native_code : 0,
                   (err && err->message) ? (int)err->message_len : 0,
                   (err && err->message) ? err->message : "");
        } else {
            printf("{\"op\":\"unknown\",\"arg\":\"%s\"}\n", op);
        }
    }
    free(buf);
    song_close(h);
    fclose(f);
    printf("{\"op\":\"close\"}\n");
    return 0;
}
