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

static double cpu_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts);
    return (double)ts.tv_sec * 1e6 + (double)ts.tv_nsec / 1e3;
}

static int open_song(const char *path, song_io *io, song_handle **out) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    io->userdata = f;
    io->read = host_read;
    io->seek = host_seek;
    io->size = host_size;
    if (song_open(io, out) != SONG_OK) { fclose(f); return -1; }
    return 0;
}

static void close_with_file(song_handle *h, song_io *io) {
    song_close(h);
    fclose((FILE *)io->userdata);
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return x < y ? -1 : x > y ? 1 : 0;
}

static double median_of(double *v, int n) {
    qsort(v, (size_t)n, sizeof(double), cmp_double);
    return v[n / 2];
}

static double percentile_nearest_rank(double *sorted, int n, double p) {
    int idx = (int)(((p / 100.0) * (double)n) + 0.999999);
    if (idx < 1) idx = 1;
    if (idx > n) idx = n;
    return sorted[idx - 1];
}

static int run_correct(const char *path) {
    song_io io;
    song_handle *h = NULL;
    if (open_song(path, &io, &h) != 0) {
        printf("{\"mode\":\"correct\",\"status\":\"open_failed\",\"file\":\"%s\"}\n", path);
        return 1;
    }
    song_info info;
    song_status st = song_probe(h, &info);
    if (st != SONG_OK) {
        printf("{\"mode\":\"correct\",\"status\":\"probe_failed\",\"code\":%d,\"file\":\"%s\"}\n", st, path);
        close_with_file(h, &io);
        return 1;
    }

    uint64_t block = 4096;
    float *dst = (float *)malloc(sizeof(float) * block * (uint64_t)info.channels);
    sha256_ctx c;
    sha256_init(&c);
    uint64_t frames = 0;
    const char *terminal = "unknown";
    for (;;) {
        uint64_t got = 0;
        st = song_read_pcm(h, dst, block, &got);
        if (st == SONG_EOF) {
            if (got == 0) { terminal = "SONG_EOF"; break; }
            sha256_update(&c, dst, sizeof(float) * got * (uint64_t)info.channels);
            frames += got;
            continue;
        }
        if (st != SONG_OK) {
            printf("{\"mode\":\"correct\",\"status\":\"read_failed\",\"code\":%d,\"frames\":%" PRIu64 ",\"file\":\"%s\"}\n",
                   st, frames, path);
            free(dst); close_with_file(h, &io);
            return 1;
        }
        if (got == 0) { terminal = "SONG_OK_ZERO"; break; }
        sha256_update(&c, dst, sizeof(float) * got * (uint64_t)info.channels);
        frames += got;
    }
    char hex[65];
    sha256_hex(&c, hex);
    printf("{\"mode\":\"correct\",\"status\":\"ok\",\"file\":\"%s\",\"codec\":\"%s\",\"container\":\"%s\","
           "\"sample_rate\":%d,\"channels\":%d,\"duration_us\":%" PRId64 ",\"frames\":%" PRIu64
           ",\"pcm_sha256\":\"%s\",\"terminal\":\"%s\"}\n",
           path, info.codec, info.container, info.sample_rate, info.channels,
           info.duration_us, frames, hex, terminal);
    free(dst);
    close_with_file(h, &io);
    return 0;
}

static int run_bench(const char *path, int warmup, int iters) {
    song_io io;
    song_handle *h = NULL;
    if (open_song(path, &io, &h) != 0) {
        printf("{\"mode\":\"bench\",\"status\":\"open_failed\",\"file\":\"%s\"}\n", path);
        return 1;
    }
    song_info info;
    if (song_probe(h, &info) != SONG_OK) {
        printf("{\"mode\":\"bench\",\"status\":\"probe_failed\",\"file\":\"%s\"}\n", path);
        close_with_file(h, &io);
        return 1;
    }
    uint64_t block = 4096;
    float *dst = (float *)malloc(sizeof(float) * block * (uint64_t)info.channels);
    uint64_t frames_seen = 0;
    double *wall = (double *)malloc(sizeof(double) * (size_t)iters);
    double *cpu = (double *)malloc(sizeof(double) * (size_t)iters);
    const char *terminal = "unknown";

    for (int it = -warmup; it < iters; it++) {
        song_io io2;
        song_handle *h2 = NULL;
        if (open_song(path, &io2, &h2) != 0) {
            printf("{\"mode\":\"bench\",\"status\":\"open_failed\",\"iteration\":%d,\"file\":\"%s\"}\n", it, path);
            free(dst); free(wall); free(cpu);
            song_close(h);
            fclose((FILE *)io.userdata);
            return 1;
        }
        song_info info2;
        if (song_probe(h2, &info2) != SONG_OK) {
            printf("{\"mode\":\"bench\",\"status\":\"probe_failed\",\"iteration\":%d,\"file\":\"%s\"}\n", it, path);
            close_with_file(h2, &io2);
            free(dst); free(wall); free(cpu);
            song_close(h);
            fclose((FILE *)io.userdata);
            return 1;
        }
        uint64_t got = 0, frames = 0;
        double t0 = now_us(), c0 = cpu_us();
        for (;;) {
            song_status st = song_read_pcm(h2, dst, block, &got);
            if (st == SONG_EOF && got == 0) { terminal = "SONG_EOF"; break; }
            if (st != SONG_OK) { terminal = "ERROR"; break; }
            if (got == 0) { terminal = "OK_ZERO"; break; }
            frames += got;
        }
        double w = now_us() - t0, cp = cpu_us() - c0;
        if (it >= 0) {
            wall[it] = w;
            cpu[it] = cp;
            frames_seen = frames;
        }
        close_with_file(h2, &io2);
    }
    double wall_med = median_of(wall, iters);
    double cpu_med = median_of(cpu, iters);
    double audio_s = info.duration_us > 0 ? (double)info.duration_us / 1e6 : 0.0;
    double xrt = (wall_med > 0 && audio_s > 0) ? audio_s / (wall_med / 1e6) : 0.0;
    printf("{\"mode\":\"bench\",\"status\":\"ok\",\"file\":\"%s\",\"warmup\":%d,\"iterations\":%d,"
           "\"block_frames\":%" PRIu64 ",\"frames\":%" PRIu64 ",\"duration_us\":%" PRId64
           ",\"terminal\":\"%s\",\"wall_us_median\":%.3f,\"cpu_us_median\":%.3f,"
           "\"wall_us_all\":[",
           path, warmup, iters, block, frames_seen, info.duration_us,
           terminal, wall_med, cpu_med);
    for (int i = 0; i < iters; i++)
        printf("%s%.3f", i ? "," : "", wall[i]);
    printf("],\"x_realtime\":%.2f}\n", xrt);
    free(dst); free(wall); free(cpu);
    song_close(h);
    fclose((FILE *)io.userdata);
    return 0;
}

static int run_startup(const char *path, int iters) {
    double *open_us = (double *)malloc(sizeof(double) * (size_t)iters);
    double *probe_us = (double *)malloc(sizeof(double) * (size_t)iters);
    double *first_us = (double *)malloc(sizeof(double) * (size_t)iters);
    float dst[4096 * 8];
    for (int i = 0; i < iters; i++) {
        FILE *f = fopen(path, "rb");
        song_io io = {f, host_read, host_seek, host_size};
        song_handle *h = NULL;
        double t0 = now_us();
        if (song_open(&io, &h) != SONG_OK) {
            printf("{\"mode\":\"startup\",\"status\":\"open_failed\",\"iteration\":%d,\"file\":\"%s\"}\n", i, path);
            fclose(f);
            free(open_us); free(probe_us); free(first_us);
            return 1;
        }
        double t1 = now_us();
        song_info info;
        song_status st = song_probe(h, &info);
        double t2 = now_us();
        uint64_t got = 0;
        if (st == SONG_OK) song_read_pcm(h, dst, 4096, &got);
        double t3 = now_us();
        open_us[i] = t1 - t0;
        probe_us[i] = t2 - t1;
        first_us[i] = t3 - t2;
        song_close(h);
        fclose(f);
    }
    printf("{\"mode\":\"startup\",\"status\":\"ok\",\"file\":\"%s\",\"iterations\":%d,"
           "\"open_us_median\":%.3f,\"probe_us_median\":%.3f,\"first_read_us_median\":%.3f,"
           "\"ttfp_us_median\":%.3f}\n",
           path, iters, median_of(open_us, iters), median_of(probe_us, iters),
           median_of(first_us, iters),
           median_of(open_us, iters) + median_of(probe_us, iters) + median_of(first_us, iters));
    free(open_us); free(probe_us); free(first_us);
    return 0;
}

static int run_latency(const char *path, int block, int warmup, int passes) {
    int cap = 1 << 20;
    double *us = (double *)malloc(sizeof(double) * (size_t)cap);
    int n = 0;
    float *dst = (float *)malloc(sizeof(float) * (size_t)block * 8);
    const char *terminal = "unknown";
    for (int p = -warmup; p < passes; p++) {
        song_io io;
        song_handle *h = NULL;
        if (open_song(path, &io, &h) != 0) {
            printf("{\"mode\":\"latency\",\"status\":\"open_failed\",\"pass\":%d,\"file\":\"%s\"}\n", p, path);
            free(us); free(dst);
            return 1;
        }
        song_info info;
        if (song_probe(h, &info) != SONG_OK) {
            printf("{\"mode\":\"latency\",\"status\":\"probe_failed\",\"pass\":%d,\"file\":\"%s\"}\n", p, path);
            close_with_file(h, &io);
            free(us); free(dst);
            return 1;
        }
        for (;;) {
            uint64_t got = 0;
            double t0 = now_us();
            song_status st = song_read_pcm(h, dst, (uint64_t)block, &got);
            double t1 = now_us();
            if (st == SONG_EOF) { terminal = "SONG_EOF"; break; }
            if (st != SONG_OK) { terminal = "ERROR"; break; }
            if (got == 0) { terminal = "OK_ZERO"; break; }
            if (p >= 0 && n < cap) us[n++] = t1 - t0;
        }
        close_with_file(h, &io);
    }
    qsort(us, (size_t)n, sizeof(double), cmp_double);
    double sum = 0;
    for (int i = 0; i < n; i++) sum += us[i];
    printf("{\"mode\":\"latency\",\"status\":\"ok\",\"file\":\"%s\",\"block_frames\":%d,"
           "\"passes\":%d,\"count\":%d,\"terminal\":\"%s\","
           "\"p50_us\":%.3f,\"p90_us\":%.3f,\"p95_us\":%.3f,\"p99_us\":%.3f,"
           "\"max_us\":%.3f,\"mean_us\":%.3f}\n",
           path, block, passes, n, terminal,
           percentile_nearest_rank(us, n, 50), percentile_nearest_rank(us, n, 90),
           percentile_nearest_rank(us, n, 95), percentile_nearest_rank(us, n, 99),
           n ? us[n-1] : 0.0, n ? sum / (double)n : 0.0);
    free(us); free(dst);
    return 0;
}

static int run_timer(int n) {
    double *v = (double *)malloc(sizeof(double) * (size_t)n);
    for (int i = 0; i < n; i++) {
        double t0 = now_us();
        double t1 = now_us();
        v[i] = t1 - t0;
    }
    printf("{\"mode\":\"timer\",\"status\":\"ok\",\"clock_pair_us_median\":%.4f}\n", median_of(v, n));
    free(v);
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr,
                "usage: %s correct <file>\n"
                "       %s bench <file> [warmup=3] [iterations=20]\n"
                "       %s startup <file> [iterations=50]\n"
                "       %s latency <file> [block=1024] [warmup=2] [passes=3]\n"
                "       %s timer [n=1000]\n",
                argv[0], argv[0], argv[0], argv[0], argv[0]);
        return 2;
    }
    if (strcmp(argv[1], "correct") == 0) return run_correct(argv[2]);
    if (strcmp(argv[1], "bench") == 0)
        return run_bench(argv[2], argc > 3 ? atoi(argv[3]) : 3, argc > 4 ? atoi(argv[4]) : 20);
    if (strcmp(argv[1], "startup") == 0)
        return run_startup(argv[2], argc > 3 ? atoi(argv[3]) : 50);
    if (strcmp(argv[1], "latency") == 0)
        return run_latency(argv[2], argc > 3 ? atoi(argv[3]) : 1024,
                           argc > 4 ? atoi(argv[4]) : 2, argc > 5 ? atoi(argv[5]) : 3);
    if (strcmp(argv[1], "timer") == 0) return run_timer(argc > 2 ? atoi(argv[2]) : 1000);
    fprintf(stderr, "unknown mode %s\n", argv[1]);
    return 2;
}
