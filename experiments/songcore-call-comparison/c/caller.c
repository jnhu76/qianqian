#define _POSIX_C_SOURCE 200809L

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "songcore.h"

typedef struct {
    uint32_t h[8];
    uint64_t len;
    uint8_t buf[64];
    size_t buf_len;
} sha256_ctx;

static const uint32_t K256[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,
    0x923f82a4,0xab1c5ed5,0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,
    0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,0xe49b69c1,0xefbe4786,
    0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,
    0x06ca6351,0x14292967,0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,
    0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,0xa2bfe8a1,0xa81a664b,
    0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,
    0x5b9cca4f,0x682e6ff3,0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,
    0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

static uint32_t ror(uint32_t x, int n) { return (x >> n) | (x << (32 - n)); }

static void sha256_compress(sha256_ctx *c, const uint8_t *p) {
    uint32_t w[64];
    for (int i = 0; i < 16; i++)
        w[i] = ((uint32_t)p[4*i] << 24) | ((uint32_t)p[4*i+1] << 16) |
               ((uint32_t)p[4*i+2] << 8) | (uint32_t)p[4*i+3];
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = ror(w[i-15], 7) ^ ror(w[i-15], 18) ^ (w[i-15] >> 3);
        uint32_t s1 = ror(w[i-2], 17) ^ ror(w[i-2], 19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    uint32_t a=c->h[0],b=c->h[1],d=c->h[2],e=c->h[3];
    uint32_t f=c->h[4],g=c->h[5],hh=c->h[6],i=c->h[7];
    for (int t = 0; t < 64; t++) {
        uint32_t S1 = ror(f,6) ^ ror(f,11) ^ ror(f,25);
        uint32_t ch = (f & g) ^ (~f & hh);
        uint32_t t1 = i + S1 + ch + K256[t] + w[t];
        uint32_t S0 = ror(a,2) ^ ror(a,13) ^ ror(a,22);
        uint32_t mj = (a & b) ^ (a & d) ^ (b & d);
        uint32_t t2 = S0 + mj;
        i = hh; hh = g; g = f; f = e + t1;
        e = d; d = b; b = a; a = t1 + t2;
    }
    c->h[0]+=a; c->h[1]+=b; c->h[2]+=d; c->h[3]+=e;
    c->h[4]+=f; c->h[5]+=g; c->h[6]+=hh; c->h[7]+=i;
}

static void sha256_init(sha256_ctx *c) {
    c->h[0]=0x6a09e667; c->h[1]=0xbb67ae85; c->h[2]=0x3c6ef372; c->h[3]=0xa54ff53a;
    c->h[4]=0x510e527f; c->h[5]=0x9b05688c; c->h[6]=0x1f83d9ab; c->h[7]=0x5be0cd19;
    c->len = 0; c->buf_len = 0;
}

static void sha256_update(sha256_ctx *c, const void *data, size_t n) {
    const uint8_t *p = (const uint8_t *)data;
    c->len += n;
    while (n > 0) {
        size_t take = 64 - c->buf_len;
        if (take > n) take = n;
        memcpy(c->buf + c->buf_len, p, take);
        c->buf_len += take; p += take; n -= take;
        if (c->buf_len == 64) { sha256_compress(c, c->buf); c->buf_len = 0; }
    }
}

static void sha256_hex(sha256_ctx *c, char out[65]) {
    uint64_t bits = c->len * 8;
    uint8_t pad = 0x80;
    sha256_update(c, &pad, 1);
    uint8_t zero = 0;
    while (c->buf_len != 56) sha256_update(c, &zero, 1);
    uint8_t tail[8];
    for (int i = 0; i < 8; i++) tail[i] = (uint8_t)(bits >> (56 - 8*i));
    c->buf_len = 56;
    sha256_update(c, tail, 8);
    static const char hexd[] = "0123456789abcdef";
    for (int i = 0; i < 8; i++)
        for (int j = 0; j < 4; j++) {
            uint32_t v = (c->h[i] >> (24 - 8*j)) & 0xff;
            out[i*8 + j*2] = hexd[v >> 4];
            out[i*8 + j*2 + 1] = hexd[v & 15];
        }
    out[64] = 0;
}

static int64_t host_read(void *ud, uint8_t *dst, size_t size) {
    FILE *f = (FILE *)ud;
    size_t n = fread(dst, 1, size, f);
    if (n > 0) return (int64_t)n;
    return feof(f) ? 0 : -1;
}

static int64_t host_seek(void *ud, int64_t absolute_offset) {
    FILE *f = (FILE *)ud;
    if (fseek(f, (long)absolute_offset, SEEK_SET) != 0) return -1;
    return (int64_t)ftell(f);
}

static int64_t host_size(void *ud) {
    FILE *f = (FILE *)ud;
    long cur = ftell(f);
    if (fseek(f, 0, SEEK_END) != 0) return -1;
    long end = ftell(f);
    fseek(f, cur, SEEK_SET);
    return (int64_t)end;
}

static double now_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e6 + (double)ts.tv_nsec / 1e3;
}

typedef struct {
    FILE *file;
    song_handle *handle;
} caller_song;

static int caller_open(const char *path, caller_song *s) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    song_io io;
    io.userdata = f;
    io.read = host_read;
    io.seek = host_seek;
    io.size = host_size;
    if (song_open(&io, &s->handle) != SONG_OK) {
        fclose(f);
        return -1;
    }
    s->file = f;
    return 0;
}

static void caller_close(caller_song *s) {
    song_close(s->handle);
    fclose(s->file);
}

static void enumerate_snapshot(song_handle *h) {
    uint32_t stream_count = 0;
    if (song_audio_stream_count(h, &stream_count) == SONG_OK && stream_count > 0) {
        song_stream_info sinfo;
        song_audio_stream_info(h, 0, &sinfo);
    }
    uint32_t meta_count = 0;
    if (song_get_metadata_count(h, &meta_count) == SONG_OK && meta_count > 0) {
        song_metadata_entry e;
        song_get_metadata_entry(h, 0, &e);
    }
    const song_metadata *meta = NULL;
    song_get_metadata(h, &meta);
    uint32_t art_count = 0;
    if (song_get_artwork_count(h, &art_count) == SONG_OK && art_count > 0) {
        song_artwork_item item;
        song_get_artwork_item(h, 0, &item);
    }
    const song_error *err = NULL;
    song_last_error(h, &err);
}

int caller_correct(const char *path, uint64_t block, uint64_t *out_frames,
                   int32_t *out_terminal, char *out_sha_hex) {
    caller_song s;
    if (caller_open(path, &s) != 0) return -1;
    song_info info;
    if (song_probe(s.handle, &info) != SONG_OK) {
        caller_close(&s);
        return -1;
    }
    enumerate_snapshot(s.handle);
    float *dst = (float *)malloc(sizeof(float) * block * (size_t)info.channels);
    if (!dst) {
        caller_close(&s);
        return -1;
    }
    sha256_ctx c;
    sha256_init(&c);
    uint64_t frames = 0;
    int32_t terminal = 0;
    for (;;) {
        uint64_t got = 0;
        song_status st = song_read_pcm(s.handle, dst, block, &got);
        if (st == SONG_EOF && got == 0) { terminal = (int32_t)SONG_EOF; break; }
        if (st != SONG_OK) { terminal = (int32_t)st; break; }
        if (got == 0) { terminal = (int32_t)SONG_OK; break; }
        sha256_update(&c, dst, sizeof(float) * got * (size_t)info.channels);
        frames += got;
    }
    char hex[65];
    sha256_hex(&c, hex);
    *out_frames = frames;
    *out_terminal = terminal;
    memcpy(out_sha_hex, hex, 65);
    free(dst);
    caller_close(&s);
    return 0;
}

int caller_surface(const char *path, int32_t *out_select_status,
                   int32_t *out_seek_status, int64_t *out_seek_actual_us,
                   uint64_t *out_first_frames) {
    caller_song s;
    if (caller_open(path, &s) != 0) return -1;
    song_info info;
    if (song_probe(s.handle, &info) != SONG_OK) {
        caller_close(&s);
        return -1;
    }
    int32_t select_status = -1;
    uint32_t stream_count = 0;
    if (song_audio_stream_count(s.handle, &stream_count) == SONG_OK && stream_count > 0) {
        song_stream_info sinfo;
        song_audio_stream_info(s.handle, 0, &sinfo);
        select_status = (int32_t)song_select_stream(s.handle, 0);
        song_probe(s.handle, &info);
    }
    enumerate_snapshot(s.handle);
    int64_t actual = -2;
    song_status st = song_seek(s.handle, 500000, &actual);
    uint64_t got = 0;
    float dst[4096 * 8];
    if (st == SONG_OK) song_read_pcm(s.handle, dst, 1024, &got);
    *out_select_status = select_status;
    *out_seek_status = (int32_t)st;
    *out_seek_actual_us = actual;
    *out_first_frames = got;
    caller_close(&s);
    return 0;
}

int caller_steady(const char *path, uint64_t block, int64_t *out_wall_us,
                  uint64_t *out_frames, int32_t *out_terminal) {
    caller_song s;
    if (caller_open(path, &s) != 0) return -1;
    song_info info;
    if (song_probe(s.handle, &info) != SONG_OK) {
        caller_close(&s);
        return -1;
    }
    float *dst = (float *)malloc(sizeof(float) * block * (size_t)info.channels);
    if (!dst) {
        caller_close(&s);
        return -1;
    }
    uint64_t frames = 0;
    int32_t terminal = 0;
    double t0 = now_us();
    for (;;) {
        uint64_t got = 0;
        song_status st = song_read_pcm(s.handle, dst, block, &got);
        if (st == SONG_EOF && got == 0) { terminal = (int32_t)SONG_EOF; break; }
        if (st != SONG_OK) { terminal = (int32_t)st; break; }
        if (got == 0) { terminal = (int32_t)SONG_OK; break; }
        frames += got;
    }
    *out_wall_us = (int64_t)(now_us() - t0);
    *out_frames = frames;
    *out_terminal = terminal;
    free(dst);
    caller_close(&s);
    return 0;
}

int caller_latency(const char *path, uint64_t block, double *call_us,
                   int32_t call_cap, int32_t *out_call_count,
                   int32_t *out_terminal) {
    caller_song s;
    if (caller_open(path, &s) != 0) return -1;
    song_info info;
    if (song_probe(s.handle, &info) != SONG_OK) {
        caller_close(&s);
        return -1;
    }
    float *dst = (float *)malloc(sizeof(float) * block * (size_t)info.channels);
    if (!dst) {
        caller_close(&s);
        return -1;
    }
    int32_t n = 0;
    int32_t terminal = 0;
    for (;;) {
        uint64_t got = 0;
        double t0 = now_us();
        song_status st = song_read_pcm(s.handle, dst, block, &got);
        double t1 = now_us();
        if (st == SONG_EOF && got == 0) { terminal = (int32_t)SONG_EOF; break; }
        if (st != SONG_OK) { terminal = (int32_t)st; break; }
        if (got == 0) { terminal = (int32_t)SONG_OK; break; }
        if (n < call_cap) call_us[n++] = t1 - t0;
    }
    *out_call_count = n;
    *out_terminal = terminal;
    free(dst);
    caller_close(&s);
    return 0;
}

int caller_ttfp(const char *path, uint64_t first_block, int64_t *out_open_us,
                int64_t *out_probe_us, int64_t *out_first_us) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    song_io io;
    io.userdata = f;
    io.read = host_read;
    io.seek = host_seek;
    io.size = host_size;
    song_handle *h = NULL;
    double t0 = now_us();
    song_status st = song_open(&io, &h);
    double t1 = now_us();
    if (st != SONG_OK) {
        fclose(f);
        return -1;
    }
    song_info info;
    st = song_probe(h, &info);
    double t2 = now_us();
    uint64_t got = 0;
    float dst[4096 * 8];
    if (st == SONG_OK) song_read_pcm(h, dst, first_block, &got);
    double t3 = now_us();
    *out_open_us = (int64_t)(t1 - t0);
    *out_probe_us = (int64_t)(t2 - t1);
    *out_first_us = (int64_t)(t3 - t2);
    song_close(h);
    fclose(f);
    return 0;
}

int caller_floor(int64_t iterations, int64_t *out_wall_ns) {
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    volatile uint32_t sink = 0;
    for (int64_t i = 0; i < iterations; i++) sink = songcore_abi_version();
    clock_gettime(CLOCK_MONOTONIC, &t1);
    *out_wall_ns = (int64_t)((double)(t1.tv_sec - t0.tv_sec) * 1e9 +
                             (double)(t1.tv_nsec - t0.tv_nsec));
    return 0;
}

#ifndef CALLER_NO_MAIN
int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr,
                "usage: %s correct <file> [block]\n"
                "       %s surface <file>\n"
                "       %s steady <file> [block]\n"
                "       %s latency <file> [block]\n"
                "       %s ttfp <file> [first_block]\n"
                "       %s floor <iterations>\n",
                argv[0], argv[0], argv[0], argv[0], argv[0], argv[0]);
        return 2;
    }
    if (strcmp(argv[1], "correct") == 0) {
        uint64_t block = argc > 3 ? strtoull(argv[3], NULL, 10) : 1024;
        uint64_t frames = 0;
        int32_t terminal = 0;
        char hex[65];
        if (caller_correct(argv[2], block, &frames, &terminal, hex) != 0) {
            printf("{\"mode\":\"correct\",\"status\":\"failed\",\"file\":\"%s\"}\n", argv[2]);
            return 1;
        }
        printf("{\"mode\":\"correct\",\"status\":\"ok\",\"file\":\"%s\",\"frames\":%" PRIu64
               ",\"terminal\":%d,\"pcm_sha256\":\"%s\"}\n",
               argv[2], frames, terminal, hex);
        return 0;
    }
    if (strcmp(argv[1], "surface") == 0) {
        int32_t select_status = 0, seek_status = 0;
        int64_t actual = 0;
        uint64_t got = 0;
        if (caller_surface(argv[2], &select_status, &seek_status, &actual, &got) != 0) {
            printf("{\"mode\":\"surface\",\"status\":\"failed\",\"file\":\"%s\"}\n", argv[2]);
            return 1;
        }
        printf("{\"mode\":\"surface\",\"status\":\"ok\",\"file\":\"%s\",\"select_status\":%d,"
               "\"seek_status\":%d,\"seek_actual_us\":%" PRId64 ",\"first_frames\":%" PRIu64 "}\n",
               argv[2], select_status, seek_status, actual, got);
        return 0;
    }
    if (strcmp(argv[1], "steady") == 0) {
        uint64_t block = argc > 3 ? strtoull(argv[3], NULL, 10) : 1024;
        int64_t wall = 0;
        uint64_t frames = 0;
        int32_t terminal = 0;
        if (caller_steady(argv[2], block, &wall, &frames, &terminal) != 0) {
            printf("{\"mode\":\"steady\",\"status\":\"failed\",\"file\":\"%s\"}\n", argv[2]);
            return 1;
        }
        printf("{\"mode\":\"steady\",\"status\":\"ok\",\"file\":\"%s\",\"block\":%" PRIu64
               ",\"wall_us\":%" PRId64 ",\"frames\":%" PRIu64 ",\"terminal\":%d}\n",
               argv[2], block, wall, frames, terminal);
        return 0;
    }
    if (strcmp(argv[1], "latency") == 0) {
        uint64_t block = argc > 3 ? strtoull(argv[3], NULL, 10) : 1024;
        double *call_us = (double *)malloc(sizeof(double) * (size_t)(1 << 20));
        if (!call_us) {
            printf("{\"mode\":\"latency\",\"status\":\"oom\"}\n");
            return 1;
        }
        int32_t count = 0, terminal = 0;
        if (caller_latency(argv[2], block, call_us, 1 << 20, &count, &terminal) != 0) {
            printf("{\"mode\":\"latency\",\"status\":\"failed\",\"file\":\"%s\"}\n", argv[2]);
            free(call_us);
            return 1;
        }
        printf("{\"mode\":\"latency\",\"status\":\"ok\",\"file\":\"%s\",\"block\":%" PRIu64
               ",\"call_count\":%d,\"terminal\":%d}\n",
               argv[2], block, count, terminal);
        free(call_us);
        return 0;
    }
    if (strcmp(argv[1], "ttfp") == 0) {
        uint64_t first_block = argc > 3 ? strtoull(argv[3], NULL, 10) : 1024;
        int64_t open_us = 0, probe_us = 0, first_us = 0;
        if (caller_ttfp(argv[2], first_block, &open_us, &probe_us, &first_us) != 0) {
            printf("{\"mode\":\"ttfp\",\"status\":\"failed\",\"file\":\"%s\"}\n", argv[2]);
            return 1;
        }
        printf("{\"mode\":\"ttfp\",\"status\":\"ok\",\"file\":\"%s\",\"open_us\":%" PRId64
               ",\"probe_us\":%" PRId64 ",\"first_us\":%" PRId64 "}\n",
               argv[2], open_us, probe_us, first_us);
        return 0;
    }
    if (strcmp(argv[1], "floor") == 0) {
        int64_t iterations = argc > 2 ? strtoll(argv[2], NULL, 10) : 2000000;
        int64_t wall_ns = 0;
        if (caller_floor(iterations, &wall_ns) != 0) {
            printf("{\"mode\":\"floor\",\"status\":\"failed\"}\n");
            return 1;
        }
        printf("{\"mode\":\"floor\",\"status\":\"ok\",\"iterations\":%" PRId64
               ",\"wall_ns\":%" PRId64 ",\"ns_per_call\":%.3f}\n",
               iterations, wall_ns, (double)wall_ns / (double)iterations);
        return 0;
    }
    fprintf(stderr, "unknown mode %s\n", argv[1]);
    return 2;
}
#endif
