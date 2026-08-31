/*
 * qn_runner_common.c — shared host-side helpers for the E09 runtime ladder.
 * See qn_runner_common.h for the contract.
 */

#define _FILE_OFFSET_BITS 64
#define _POSIX_C_SOURCE 200809L

#include "qn_runner_common.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <sys/resource.h>

/* ------------------------------------------------------------------ */
/* fixture (host owns the source; guest sees only the door)            */
/* ------------------------------------------------------------------ */

int qn_fixture_load(qn_fixture *fx, const char *path) {
    memset(fx, 0, sizeof(*fx));
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return -1; }
    long n = ftell(f);
    if (n < 0 || fseek(f, 0, SEEK_SET) != 0) { fclose(f); return -1; }
    fx->buf = malloc(n > 0 ? (size_t)n : 1);
    if (!fx->buf) { fclose(f); return -1; }
    if (fread(fx->buf, 1, (size_t)n, f) != (size_t)n) {
        fclose(f); qn_fixture_free(fx); return -1;
    }
    fclose(f);
    fx->len = n;
    fx->pos = 0;
    return 0;
}

void qn_fixture_free(qn_fixture *fx) {
    free(fx->buf);
    memset(fx, 0, sizeof(*fx));
}

/* ------------------------------------------------------------------ */
/* the door                                                            */
/* ------------------------------------------------------------------ */

int64_t qn_door_read(int64_t handle, uint8_t *dst, int32_t len) {
    qn_fixture *fx = (qn_fixture *)(intptr_t)handle;
    if (len < 0) return -1;
    if (fx->pos >= fx->len) return 0;
    int64_t n = fx->len - fx->pos;
    if (n > len) n = len;
    memcpy(dst, fx->buf + fx->pos, (size_t)n);
    fx->pos += n;
    return n;
}

int64_t qn_door_seek(int64_t handle, int64_t absolute_offset) {
    qn_fixture *fx = (qn_fixture *)(intptr_t)handle;
    if (absolute_offset < 0 || absolute_offset > fx->len) return -1;
    fx->pos = absolute_offset;
    return fx->pos;
}

int64_t qn_door_size(int64_t handle) {
    return ((qn_fixture *)(intptr_t)handle)->len;
}

/* ------------------------------------------------------------------ */
/* timing / rss                                                        */
/* ------------------------------------------------------------------ */

double qn_now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1e6;
}

double qn_cpu_time_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1e6;
}

long qn_peak_rss_kb(void) {
    struct rusage ru;
    getrusage(RUSAGE_SELF, &ru);
    return ru.ru_maxrss;
}

/* ------------------------------------------------------------------ */
/* sha256 (host-side; Mode B)                                          */
/* ------------------------------------------------------------------ */

static const uint32_t K256[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2

};

#define ROTR(x,n) (((x) >> (n)) | ((x) << (32 - (n))))

typedef struct {
    uint32_t h[8];
    uint64_t total_len;
    uint8_t buf[64];
    size_t buf_len;
} qn_sha;

static void sha_transform(qn_sha *c, const uint8_t *p) {
    uint32_t w[64], a, b, cc, d, e, f, g, h;
    for (int i = 0; i < 16; i++)
        w[i] = ((uint32_t)p[i*4]<<24)|((uint32_t)p[i*4+1]<<16)|((uint32_t)p[i*4+2]<<8)|p[i*4+3];
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = ROTR(w[i-15],7) ^ ROTR(w[i-15],18) ^ (w[i-15] >> 3);
        uint32_t s1 = ROTR(w[i-2],17) ^ ROTR(w[i-2],19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a=c->h[0];b=c->h[1];cc=c->h[2];d=c->h[3];e=c->h[4];f=c->h[5];g=c->h[6];h=c->h[7];
    for (int i = 0; i < 64; i++) {
        uint32_t S1 = ROTR(e,6)^ROTR(e,11)^ROTR(e,25);
        uint32_t ch = (e & f) ^ (~e & g);
        uint32_t t1 = h + S1 + ch + K256[i] + w[i];
        uint32_t S0 = ROTR(a,2)^ROTR(a,13)^ROTR(a,22);
        uint32_t maj = (a & b) ^ (a & cc) ^ (b & cc);
        uint32_t t2 = S0 + maj;
        h=g; g=f; f=e; e=d+t1; d=cc; cc=b; b=a; a=t1+t2;
    }
    c->h[0]+=a;c->h[1]+=b;c->h[2]+=cc;c->h[3]+=d;
    c->h[4]+=e;c->h[5]+=f;c->h[6]+=g;c->h[7]+=h;
}

static void sha_update(qn_sha *c, const uint8_t *p, size_t n) {
    c->total_len += n;
    while (n > 0) {
        size_t take = 64 - c->buf_len;
        if (take > n) take = n;
        memcpy(c->buf + c->buf_len, p, take);
        c->buf_len += take; p += take; n -= take;
        if (c->buf_len == 64) { sha_transform(c, c->buf); c->buf_len = 0; }
    }
}

void qn_sha256(const void *data, size_t n, char hex_out[65]) {
    void *st = qn_sha_new();
    qn_sha_feed(st, data, n);
    qn_sha_finish(&st, hex_out);
}

void *qn_sha_new(void) {
    qn_sha *c = malloc(sizeof(qn_sha));
    if (!c) return NULL;
    static const uint32_t H0[8] = {0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
                                   0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    memcpy(c->h, H0, sizeof(H0));
    c->total_len = 0; c->buf_len = 0;
    return c;
}

void qn_sha_feed(void *state, const void *data, size_t n) {
    qn_sha *c = state;
    const uint8_t *p = data;
    c->total_len += n;
    while (n > 0) {
        size_t take = 64 - c->buf_len;
        if (take > n) take = n;
        memcpy(c->buf + c->buf_len, p, take);
        c->buf_len += take; p += take; n -= take;
        if (c->buf_len == 64) { sha_transform(c, c->buf); c->buf_len = 0; }
    }
}

void qn_sha_finish(void **state, char hex_out[65]) {
    qn_sha *c = *state;
    uint64_t bits = c->total_len * 8;
    uint8_t pad = 0x80;
    sha_update(c, &pad, 1);
    uint8_t zero = 0;
    while (c->buf_len != 56) sha_update(c, &zero, 1);
    uint8_t lenb[8];
    for (int i = 0; i < 8; i++) lenb[i] = (uint8_t)(bits >> (56 - i*8));
    sha_update(c, lenb, 8);
    uint8_t d[32];
    for (int i = 0; i < 8; i++) {
        d[i*4]   = (uint8_t)(c->h[i] >> 24);
        d[i*4+1] = (uint8_t)(c->h[i] >> 16);
        d[i*4+2] = (uint8_t)(c->h[i] >> 8);
        d[i*4+3] = (uint8_t)(c->h[i]);
    }
    for (int i = 0; i < 32; i++) sprintf(hex_out + i*2, "%02x", d[i]);
    hex_out[64] = 0;
    free(c);
    *state = NULL;
}
