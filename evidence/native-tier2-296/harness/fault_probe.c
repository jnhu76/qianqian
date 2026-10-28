/*
 * fault_probe.c — closeout2 native-decode-faults probe (evidence harness).
 *
 * Drives the REAL native adapter boundary (libsongcore.a built by the
 * ffmpeg worktree, linked statically) over genuinely corrupted copies of
 * the committed FLAC fixture, and prints the exact native trail:
 * song_status per call, song_last_error native_code/message, and — with
 * QIANQIAN_FFMPEG_LOG=1 — FFmpeg's own av_log lines from the failing
 * demuxer/decoder on stderr.
 *
 * Modes:
 *   flip   <in> <out> <offset> <xor>   copy <in> to <out> XOR one byte, then run
 *   gateprobe                          QIANQIAN_FFMPEG_LOG=0 predicate witness
 *   corrupt-read   <file>              sequential read to failure
 *   corrupt-seek   <file> <target_us>  probe -> read a little -> seek
 *
 * Build: cc -O2 -std=c11 -I <native>/include fault_probe.c \
 *            -o fault_probe <native>/build/artifacts/libsongcore.a -lm -lpthread
 */
#define _POSIX_C_SOURCE 200112L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "songcore.h"

static FILE *g_f;

static int64_t io_read(void *ud, uint8_t *dst, size_t size) {
    (void)ud;
    if (size == 0) return 0;
    size_t n = fread(dst, 1, size, g_f);
    return (n > 0) ? (int64_t)n : (feof(g_f) ? 0 : -1);
}
static int64_t io_seek(void *ud, int64_t off) {
    (void)ud;
    return fseeko(g_f, (off_t)off, SEEK_SET) == 0 ? off : -1;
}
static int64_t io_size(void *ud) {
    (void)ud;
    off_t cur = ftello(g_f);
    fseeko(g_f, 0, SEEK_END);
    off_t end = ftello(g_f);
    fseeko(g_f, cur, SEEK_SET);
    return (int64_t)end;
}

static const char *status_name(song_status s) {
    switch (s) {
    case SONG_OK: return "SONG_OK";
    case SONG_EOF: return "SONG_EOF";
    case SONG_ERR_INVALID_ARGUMENT: return "INVALID_ARGUMENT";
    case SONG_ERR_STATE: return "STATE";
    case SONG_ERR_NOT_OPEN: return "NOT_OPEN";
    case SONG_ERR_IO: return "IO";
    case SONG_ERR_UNSUPPORTED_CONTAINER: return "UNSUPPORTED_CONTAINER";
    case SONG_ERR_NO_AUDIO_STREAM: return "NO_AUDIO_STREAM";
    case SONG_ERR_UNSUPPORTED_CODEC: return "UNSUPPORTED_CODEC";
    case SONG_ERR_CORRUPT_DATA: return "CORRUPT_DATA";
    case SONG_ERR_DECODE_ERROR: return "DECODE_ERROR";
    case SONG_ERR_SEEK_UNSUPPORTED: return "SEEK_UNSUPPORTED";
    case SONG_ERR_SEEK_ERROR: return "SEEK_ERROR";
    case SONG_ERR_STREAM_CHANGE: return "STREAM_CHANGE";
    case SONG_ERR_OUT_OF_MEMORY: return "OUT_OF_MEMORY";
    case SONG_ERR_INTERNAL_ERROR: return "INTERNAL_ERROR";
    default: return "?";
    }
}

static void print_err(song_handle *h) {
    const song_error *e = NULL;
    if (h && song_last_error(h, &e) == SONG_OK && e && e->message) {
        printf("    last_error: native=%d msg=\"%.*s\"\n", e->native_code,
               (int)e->message_len, e->message);
    } else {
        printf("    last_error: (none)\n");
    }
}

static song_handle *open_file(const char *path, song_info *info) {
    g_f = fopen(path, "rb");
    if (!g_f) { perror("fopen"); exit(2); }
    song_io io = { NULL, io_read, io_seek, io_size };
    song_handle *h = NULL;
    song_status s = song_open(&io, &h);
    if (s != SONG_OK) {
        printf("  song_open: %s (%d)\n", status_name(s), (int)s);
        exit(3);
    }
    s = song_probe(h, info);
    printf("  song_probe: %s (%d) rate=%d ch=%d dur_us=%lld codec=%s\n",
           status_name(s), (int)s, info->sample_rate, info->channels,
           (long long)info->duration_us, info->codec);
    print_err(h);
    if (s != SONG_OK) exit(3);
    return h;
}

static void run_read(const char *path) {
    song_info info; memset(&info, 0, sizeof info);
    song_handle *h = open_file(path, &info);
    enum { CAP = 8192 };
    static float buf[CAP * 8];
    uint64_t total = 0;
    int calls = 0;
    for (;;) {
        uint64_t got = 0;
        song_status r = song_read_pcm(h, buf, CAP, &got);
        calls++;
        if (r == SONG_EOF) {
            printf("  read #%d: SONG_EOF total=%llu\n", calls,
                   (unsigned long long)total);
            break;
        }
        if (r == SONG_OK) { total += got; continue; }
        printf("  read #%d: %s (%d) produced=%llu total_before=%llu\n",
               calls, status_name(r), (int)r, (unsigned long long)got,
               (unsigned long long)total);
        print_err(h);
        /* terminal classification: pending fatal error persists */
        uint64_t got2 = 0;
        song_status r2 = song_read_pcm(h, buf, CAP, &got2);
        printf("  read #%d (after fault): %s (%d) produced=%llu\n", calls + 1,
               status_name(r2), (int)r2, (unsigned long long)got2);
        print_err(h);
        break;
    }
    song_close(h);
    fclose(g_f);
    printf("  CLOSED\n");
}

static void run_seek(const char *path, long long target_us) {
    song_info info; memset(&info, 0, sizeof info);
    song_handle *h = open_file(path, &info);
    enum { CAP = 8192 };
    static float buf[CAP * 8];
    /* decode a little first, like a real episode */
    uint64_t got = 0, pre = 0;
    int calls = 0;
    while (pre < (uint64_t)info.sample_rate && calls < 8) {
        song_status r = song_read_pcm(h, buf, CAP, &got);
        if (r != SONG_OK) { printf("  pre-seek read: %s\n", status_name(r)); break; }
        pre += got; calls++;
    }
    printf("  pre-seek: %llu frames in %d calls\n", (unsigned long long)pre, calls);
    int64_t landed = -1;
    song_status s = song_seek(h, target_us, &landed);
    printf("  song_seek(%lldus): %s (%d) landed_us=%lld\n", target_us,
           status_name(s), (int)s, (long long)landed);
    print_err(h);
    uint64_t got2 = 0;
    song_status r2 = song_read_pcm(h, buf, CAP, &got2);
    printf("  read-after-seek: %s (%d) produced=%llu\n", status_name(r2),
           (int)r2, (unsigned long long)got2);
    print_err(h);
    song_close(h);
    fclose(g_f);
    printf("  CLOSED\n");
}

/* Witness for which gate predicate the CURRENT artifact carries: with
 * QIANQIAN_FFMPEG_LOG=0, the new predicate installs the discard callback
 * (probe swallowed); the old predicate leaves FFmpeg's default logger
 * alive (probe printed to stderr). The probe symbol resolves from the
 * merged FFmpeg closure inside libsongcore.a. */
extern void av_log(void *avcl, int level, const char *fmt, ...);

static void run_gateprobe(const char *fixture) {
    song_info info; memset(&info, 0, sizeof info);
    song_handle *h = open_file(fixture, &info);
    av_log(NULL, 16, "QIANQIAN_GATE_PROBE_MARKER");
    fflush(stderr);
    song_close(h);
    fclose(g_f);
    printf("  gateprobe: probe fired; fd2 capture carries the verdict\n");
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr,
                "usage: %s flip <in> <out> <off> <xor>\n"
                "       %s read <file>\n"
                "       %s seek <file> <target_us>\n"
                "       %s gateprobe <fixture>\n",
                argv[0], argv[0], argv[0], argv[0]);
        return 2;
    }
    if (!strcmp(argv[1], "flip") && argc == 6) {
        FILE *in = fopen(argv[2], "rb");
        if (!in) { perror("in"); return 2; }
        fseeko(in, 0, SEEK_END);
        off_t len = ftello(in);
        fseeko(in, 0, SEEK_SET);
        unsigned char *b = malloc((size_t)len);
        if (!b || fread(b, 1, (size_t)len, in) != (size_t)len) {
            fprintf(stderr, "read failed\n"); return 2;
        }
        fclose(in);
        long off = atol(argv[4]);
        unsigned xorv = (unsigned)atol(argv[5]);
        if (off < 0 || (size_t)off >= (size_t)len) {
            fprintf(stderr, "offset out of range\n"); return 2;
        }
        b[off] ^= (unsigned char)xorv;
        FILE *out = fopen(argv[3], "wb");
        if (!out) { perror("out"); return 2; }
        fwrite(b, 1, (size_t)len, out);
        fclose(out);
        free(b);
        printf("FLIP_OK %s off=%ld xor=0x%x\n", argv[3], off, xorv);
        return 0;
    }
    if (!strcmp(argv[1], "read") && argc == 3) { run_read(argv[2]); return 0; }
    if (!strcmp(argv[1], "seek") && argc == 4) { run_seek(argv[2], atoll(argv[3])); return 0; }
    if (!strcmp(argv[1], "gateprobe") && argc == 3) { run_gateprobe(argv[2]); return 0; }
    fprintf(stderr, "bad args\n");
    return 2;
}
