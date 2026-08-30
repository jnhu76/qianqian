/*
 * songcore_seek_probe — SongCore-level seek/EOF acceptance fixture (test only).
 *
 * qn_pcm_dump never calls song_seek, so a minimization candidate could pass
 * plain-decode gates while the seek path rots. This tool exercises the
 * production SongCore contract end to end on one file:
 *
 *   open -> probe -> bounded head decode ->
 *   sequential decode to EOF (reference PCM) ->
 *   for 25%/50%/75% of duration: reopen -> song_seek -> decode to EOF ->
 *       exact frame-aligned suffix match against the reference -> clean EOF
 *
 * It owns file IO like qn_pcm_dump, knows no FFmpeg types, and prints one
 * JSON line so gate scripts can diff candidate vs baseline byte-for-byte.
 */
#if !defined(_WIN32)
#define _FILE_OFFSET_BITS 64
#define _POSIX_C_SOURCE 200809L
#endif

#include "songcore.h"

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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
    size_t n = fread(dst, 1, size, src->file);
    if (n == 0 && ferror(src->file)) return -1;
    return (int64_t)n;
}

static int64_t host_seek(void *userdata, int64_t absolute_offset) {
    file_source *src = (file_source *)userdata;
    if (absolute_offset < 0 || file_seek64(src->file, absolute_offset, SEEK_SET) != 0) return -1;
    return file_tell64(src->file);
}

static int64_t host_size(void *userdata) {
    return ((file_source *)userdata)->size;
}

typedef struct pcm_buf {
    float *data;        /* interleaved f32 */
    size_t frames;
    size_t capacity_floats;
    int channels;
} pcm_buf;

static int buf_reserve(pcm_buf *b, size_t extra_frames) {
    if ((size_t)b->channels == 0 || extra_frames > SIZE_MAX / sizeof(float) / (size_t)b->channels - b->frames)
        return -1;
    size_t need_floats = (b->frames + extra_frames) * (size_t)b->channels;
    if (need_floats <= b->capacity_floats) return 0;
    float *next = (float *)realloc(b->data, need_floats * sizeof(float));
    if (!next) return -1;
    b->data = next;
    b->capacity_floats = need_floats;
    return 0;
}

/* Decode to EOF through the production pipe. Returns 0 clean, -1 error. */
static int decode_all(song_handle *song, pcm_buf *out) {
    const size_t chunk = 4096;
    for (;;) {
        if (buf_reserve(out, chunk) < 0) return -1;
        int64_t n = song_read_pcm(song, out->data + out->frames * (size_t)out->channels, chunk);
        if (n < 0) return -1;
        if (n == 0) return 0;
        out->frames += (size_t)n;
    }
}

/* --- minimal SHA-256 (test-only; FIPS 180-4) --- */
typedef struct {
    uint32_t h[8];
    uint64_t bits;
    uint8_t block[64];
    size_t fill;
} sha256;

static uint32_t ror(uint32_t x, int r) { return (x >> r) | (x << (32 - r)); }

static void sha256_block(sha256 *s, const uint8_t *p) {
    static const uint32_t K[64] = {
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2 };
    uint32_t w[64], a, b, c, d, e, f, g, h;
    for (int i = 0; i < 16; ++i)
        w[i] = (uint32_t)p[i*4] << 24 | (uint32_t)p[i*4+1] << 16 | (uint32_t)p[i*4+2] << 8 | p[i*4+3];
    for (int i = 16; i < 64; ++i) {
        uint32_t s0 = ror(w[i-15],7) ^ ror(w[i-15],18) ^ (w[i-15] >> 3);
        uint32_t s1 = ror(w[i-2],17) ^ ror(w[i-2],19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a=s->h[0]; b=s->h[1]; c=s->h[2]; d=s->h[3]; e=s->h[4]; f=s->h[5]; g=s->h[6]; h=s->h[7];
    for (int i = 0; i < 64; ++i) {
        uint32_t S1 = ror(e,6) ^ ror(e,11) ^ ror(e,25);
        uint32_t ch = (e & f) ^ (~e & g);
        uint32_t t1 = h + S1 + ch + K[i] + w[i];
        uint32_t S0 = ror(a,2) ^ ror(a,13) ^ ror(a,22);
        uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
        uint32_t t2 = S0 + maj;
        h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;
    }
    s->h[0]+=a; s->h[1]+=b; s->h[2]+=c; s->h[3]+=d; s->h[4]+=e; s->h[5]+=f; s->h[6]+=g; s->h[7]+=h;
}

static void sha256_init(sha256 *s) {
    static const uint32_t H0[8] = {0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
                                   0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    memcpy(s->h, H0, sizeof(H0));
    s->bits = 0; s->fill = 0;
}
static void sha256_update(sha256 *s, const void *data, size_t len) {
    const uint8_t *p = data;
    s->bits += (uint64_t)len * 8;
    while (len) {
        size_t take = 64 - s->fill < len ? 64 - s->fill : len;
        memcpy(s->block + s->fill, p, take);
        s->fill += take; p += take; len -= take;
        if (s->fill == 64) { sha256_block(s, s->block); s->fill = 0; }
    }
}
static void sha256_done(sha256 *s, uint8_t out[32]) {
    uint64_t bits = s->bits;
    uint8_t pad = 0x80;
    sha256_update(s, &pad, 1);
    uint8_t z = 0;
    while (s->fill != 56) sha256_update(s, &z, 1);
    uint8_t lenb[8];
    for (int i = 0; i < 8; ++i) lenb[i] = (uint8_t)(bits >> (56 - i * 8));
    memcpy(s->block + 56, lenb, 8);
    sha256_block(s, s->block);
    s->fill = 0;
    for (int i = 0; i < 8; ++i) {
        out[i*4]   = (uint8_t)(s->h[i] >> 24);
        out[i*4+1] = (uint8_t)(s->h[i] >> 16);
        out[i*4+2] = (uint8_t)(s->h[i] >> 8);
        out[i*4+3] = (uint8_t)(s->h[i]);
    }
}
static void sha256_hex(const void *data, size_t len, char hex[65]) {
    sha256 s; sha256_init(&s); sha256_update(&s, data, len);
    uint8_t d[32]; sha256_done(&s, d);
    for (int i = 0; i < 32; ++i) sprintf(hex + i * 2, "%02x", d[i]);
    hex[64] = 0;
}

static int open_song(const char *path, song_handle **out, file_source *src) {
    memset(src, 0, sizeof(*src));
    src->file = fopen(path, "rb");
    if (!src->file) return -1;
    if (file_seek64(src->file, 0, SEEK_END) != 0 || (src->size = file_tell64(src->file)) < 0 ||
        file_seek64(src->file, 0, SEEK_SET) != 0) {
        fclose(src->file);
        return -1;
    }
    song_io io = { .userdata = src, .read = host_read, .seek = host_seek, .size = host_size };
    *out = song_open(&io);
    if (!*out) { fclose(src->file); return -1; }
    return 0;
}

static void close_song(song_handle **song, file_source *src) {
    if (*song) { song_close(*song); *song = NULL; }
    if (src->file) { fclose(src->file); src->file = NULL; }
}

/*
 * Locate the frame-aligned offset in the sequential stream where the seeked
 * decode (post, post_frames) begins. The 64-frame prefix locates candidates;
 * EACH candidate is verified against the FULL post stream before acceptance
 * (short periodic content — mono sines — can alias a short probe prefix).
 * Returns the verified resume frame or -1.
 */
static int64_t find_resume_frame(const pcm_buf *seq, const pcm_buf *post) {
    const size_t probe_frames = 64;
    if (probe_frames == 0 || post->frames < probe_frames || seq->frames < probe_frames)
        return -1;
    size_t span = probe_frames * (size_t)seq->channels;
    size_t limit = (seq->frames - probe_frames) * (size_t)seq->channels;
    uint32_t first = ((const uint32_t *)post->data)[0];
    for (size_t off = 0; off <= limit; off += (size_t)seq->channels) {
        if (((const uint32_t *)seq->data)[off] != first) continue;
        if (memcmp(seq->data + off, post->data, span * sizeof(float)) != 0) continue;
        size_t candidate = off / (size_t)seq->channels;
        size_t remaining = seq->frames - candidate;
        if (remaining == post->frames &&
            memcmp(seq->data + off, post->data,
                   post->frames * (size_t)seq->channels * sizeof(float)) == 0)
            return (int64_t)candidate;
    }
    return -1;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <local-song>\n", argv[0]);
        return 2;
    }
    const char *path = argv[1];

    song_handle *song = NULL;
    file_source src;
    if (open_song(path, &song, &src) < 0) {
        fprintf(stderr, "songcore_seek_probe: open failed\n");
        return 1;
    }
    song_info info;
    if (song_probe(song, &info) < 0) {
        fprintf(stderr, "songcore_seek_probe: probe failed\n");
        close_song(&song, &src);
        return 1;
    }

    /* bounded head decode (~5 s) before any seek */
    const size_t head_frames = (size_t)info.sample_rate * 5;
    float *head = (float *)malloc(head_frames * (size_t)info.channels * sizeof(float));
    if (!head) return 1;
    size_t head_got = 0;
    char head_sha[65] = "";
    int head_ok = 1;
    while (head_got < head_frames) {
        int64_t n = song_read_pcm(song, head + head_got * (size_t)info.channels,
                                  head_frames - head_got);
        if (n < 0) { head_ok = 0; break; }
        if (n == 0) break;
        head_got += (size_t)n;
    }
    if (head_ok && head_got)
        sha256_hex(head, head_got * (size_t)info.channels * sizeof(float), head_sha);
    free(head);
    close_song(&song, &src);

    /* reference: full sequential decode on a fresh handle */
    if (open_song(path, &song, &src) < 0) return 1;
    if (song_probe(song, &info) < 0) return 1;
    pcm_buf seq = { .channels = info.channels };
    int seq_ok = decode_all(song, &seq) == 0;
    close_song(&song, &src);
    char seq_sha[65] = "";
    if (seq_ok && seq.frames)
        sha256_hex(seq.data, seq.frames * (size_t)seq.channels * sizeof(float), seq_sha);

    printf("{\"file\":\"%s\",\"container\":\"%s\",\"codec\":\"%s\","
           "\"sample_rate\":%d,\"channels\":%d,\"duration_us\":%" PRId64 ","
           "\"head_frames\":%zu,\"head_sha256\":\"%s\","
           "\"sequential_frames\":%zu,\"sequential_ok\":%s,\"sequential_sha256\":\"%s\","
           "\"seeks\":[",
           path, info.container, info.codec, info.sample_rate, info.channels,
           info.duration_us, head_got, head_sha,
           seq.frames, seq_ok ? "true" : "false", seq_sha);

    const int64_t dur = info.duration_us;
    const size_t probe_frames = 64;
    float *probe = (float *)malloc(probe_frames * (size_t)info.channels * sizeof(float));
    for (int i = 0; i < 3; ++i) {
        int64_t target = dur > 0 ? (int64_t)((double)dur * (0.25 * (i + 1))) : 0;
        printf("%s{", i ? "," : "");
        song_handle *s2 = NULL;
        file_source src2;
        pcm_buf post = { .channels = info.channels };
        int opened = open_song(path, &s2, &src2) == 0;
        int probed = opened && song_probe(s2, &info) == 0;
        int seeked = probed && song_seek(s2, target) == 0;
        if (!seeked) {
            printf("\"status\":\"%s\",\"target_us\":%" PRId64 "}",
                   opened ? (probed ? "seek_failed" : "probe_failed") : "reopen_failed", target);
        } else {
            int decode_ok = decode_all(s2, &post) == 0;
            int clean_eof = decode_ok; /* decode_all returns only on clean EOF */
            int64_t resume = -1;
            int suffix_exact = 0;
            char post_sha[65] = "";
            if (decode_ok && post.frames >= probe_frames && seq.frames) {
                memcpy(probe, post.data, probe_frames * (size_t)info.channels * sizeof(float));
                resume = find_resume_frame(&seq, &post);
                suffix_exact = resume >= 0;
            } else if (decode_ok && post.frames == 0) {
                resume = -1;
                suffix_exact = seq.frames == 0;
            }
            if (post.frames)
                sha256_hex(post.data, post.frames * (size_t)post.channels * sizeof(float), post_sha);
            printf("\"status\":\"done\",\"target_us\":%" PRId64 ",\"frames\":%zu,"
                   "\"resume_frame\":%" PRId64 ",\"suffix_exact\":%s,\"clean_eof\":%s,"
                   "\"sha256\":\"%s\"}",
                   target, post.frames, resume, suffix_exact ? "true" : "false",
                   clean_eof ? "true" : "false", post_sha);
        }
        free(post.data);
        if (s2) close_song(&s2, &src2);
    }
    printf("]}\n");

    free(probe);
    free(seq.data);
    return (seq_ok && head_ok) ? 0 : 1;
}
