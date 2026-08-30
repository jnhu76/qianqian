/*
 * songcore_windows_host — Windows-native host adapter for the Common Formats
 * Windows gate (test-only; never ships).
 *
 * Proves the SongCore IO ownership contract on real Windows:
 *
 *   <file>       full contract (open/probe/head-decode/decode-to-EOF/
 *                seek 25/50/75 + decode + clean EOF) through a wide-char
 *                host: CreateFileW + ReadFile + SetFilePointerEx +
 *                GetFileSizeEx. <file> is widened from UTF-8 via
 *                MultiByteToWideChar, so the narrow argv never reaches
 *                SongCore or a CRT narrow open.
 *   --unicode    copies a fixture to a Chinese+Unicode+spaces path under
 *                the user's temp dir and runs the same contract entirely
 *                through wide APIs (wide literals in this source).
 *   --largefile  synthetic virtual IO: declares a 3 GiB WAV, asserts the
 *                >2 GiB seek offset actually reaches the host and that
 *                decode still works after it.
 *
 * The adapter owns ALL file IO; SongCore/FFmpeg only ever see callbacks.
 * Prints one JSON line. Exit code 0 = PASS.
 */
#include <windows.h>

#include "songcore.h"

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ------------------------------------------------------------------ */
/* Win32-file-backed host IO                                           */
/* ------------------------------------------------------------------ */

typedef struct {
    HANDLE file;
    int64_t size;
} win_source;

static int64_t host_read(void *userdata, uint8_t *dst, size_t size) {
    win_source *src = (win_source *)userdata;
    if (size > 0x7FFFFFFF) size = 0x7FFFFFFF;
    DWORD got = 0;
    if (!ReadFile(src->file, dst, (DWORD)size, &got, NULL)) return -1;
    return (int64_t)got;
}

static int64_t host_seek(void *userdata, int64_t absolute_offset) {
    win_source *src = (win_source *)userdata;
    LARGE_INTEGER li, out;
    li.QuadPart = absolute_offset;
    if (!SetFilePointerEx(src->file, li, &out, FILE_BEGIN)) return -1;
    return out.QuadPart;
}

static int64_t host_size(void *userdata) {
    return ((win_source *)userdata)->size;
}

/* ------------------------------------------------------------------ */
/* virtual host IO (large-file gate; no real file exists)              */
/* ------------------------------------------------------------------ */

#define VIRTUAL_SIZE ((int64_t)3 * 1024 * 1024 * 1024) /* 3 GiB */

typedef struct {
    int64_t pos;
    int64_t max_seek_abs;
    int64_t max_read_end;
    int negative_seek;
} virtual_source;

static int64_t virt_read(void *userdata, uint8_t *dst, size_t size) {
    virtual_source *v = (virtual_source *)userdata;
    if (v->pos >= VIRTUAL_SIZE) return 0;
    int64_t n = (int64_t)size;
    if (v->pos + n > VIRTUAL_SIZE) n = VIRTUAL_SIZE - v->pos;
    memset(dst, 0, (size_t)n);
    v->pos += n;
    if (v->pos > v->max_read_end) v->max_read_end = v->pos;
    return n;
}

static int64_t virt_seek(void *userdata, int64_t absolute_offset) {
    virtual_source *v = (virtual_source *)userdata;
    if (absolute_offset < 0) { v->negative_seek = 1; return -1; }
    v->pos = absolute_offset;
    if (absolute_offset > v->max_seek_abs) v->max_seek_abs = absolute_offset;
    return absolute_offset;
}

static int64_t virt_size(void *userdata) {
    (void)userdata;
    return VIRTUAL_SIZE;
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

/* ------------------------------------------------------------------ */
/* contract runner                                                     */
/* ------------------------------------------------------------------ */

typedef struct {
    float *data;
    size_t frames;
    size_t capacity_floats;
    int channels;
} pcm_buf;

static int buf_reserve(pcm_buf *b, size_t extra_frames) {
    if ((size_t)b->channels == 0 ||
        extra_frames > SIZE_MAX / sizeof(float) / (size_t)b->channels - b->frames)
        return -1;
    size_t need = (b->frames + extra_frames) * (size_t)b->channels;
    if (need <= b->capacity_floats) return 0;
    float *next = (float *)realloc(b->data, need * sizeof(float));
    if (!next) return -1;
    b->data = next;
    b->capacity_floats = need;
    return 0;
}

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

static int win_open(const wchar_t *wpath, song_handle **out, win_source *src) {
    memset(src, 0, sizeof(*src));
    src->file = CreateFileW(wpath, GENERIC_READ, FILE_SHARE_READ, NULL,
                            OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (src->file == INVALID_HANDLE_VALUE) return -1;
    LARGE_INTEGER li;
    if (!GetFileSizeEx(src->file, &li)) { CloseHandle(src->file); return -1; }
    src->size = li.QuadPart;
    song_io io = { .userdata = src, .read = host_read, .seek = host_seek, .size = host_size };
    *out = song_open(&io);
    if (!*out) { CloseHandle(src->file); return -1; }
    return 0;
}

static void win_close(song_handle **song, win_source *src) {
    if (*song) { song_close(*song); *song = NULL; }
    if (src->file && src->file != INVALID_HANDLE_VALUE) {
        CloseHandle(src->file);
        src->file = INVALID_HANDLE_VALUE;
    }
}

static wchar_t *utf8_to_wide(const char *s) {
    int n = MultiByteToWideChar(CP_UTF8, 0, s, -1, NULL, 0);
    if (n <= 0) return NULL;
    wchar_t *w = (wchar_t *)malloc((size_t)n * sizeof(wchar_t));
    if (!w) return NULL;
    MultiByteToWideChar(CP_UTF8, 0, s, -1, w, n);
    return w;
}

static int run_contract(song_handle *song, const char *label) {
    song_info info;
    if (song_probe(song, &info) < 0) {
        fprintf(stderr, "probe failed for %s\n", label);
        return 1;
    }
    printf("{\"label\":\"%s\",\"container\":\"%s\",\"codec\":\"%s\","
           "\"sample_rate\":%d,\"channels\":%d",
           label, info.container, info.codec, info.sample_rate, info.channels);

    /* bounded head decode */
    size_t head_frames = (size_t)info.sample_rate;
    float *head = (float *)malloc(head_frames * (size_t)info.channels * sizeof(float));
    size_t head_got = 0;
    while (head_got < head_frames) {
        int64_t n = song_read_pcm(song, head + head_got * (size_t)info.channels,
                                  head_frames - head_got);
        if (n < 0) { free(head); return 1; }
        if (n == 0) break;
        head_got += (size_t)n;
    }
    char head_sha[65] = "";
    if (head_got) sha256_hex(head, head_got * (size_t)info.channels * sizeof(float), head_sha);
    free(head);
    printf(",\"head_frames\":%zu,\"head_sha256\":\"%s\"", head_got, head_sha);

    /* sequential decode to EOF */
    song_info info2;
    pcm_buf seq = { .channels = info.channels };
    if (song_probe(song, &info2) < 0) return 1;
    int seq_ok = decode_all(song, &seq) == 0;
    char seq_sha[65] = "";
    if (seq_ok && seq.frames)
        sha256_hex(seq.data, seq.frames * (size_t)seq.channels * sizeof(float), seq_sha);
    printf(",\"sequential_frames\":%zu,\"sequential_ok\":%s,\"sequential_sha256\":\"%s\"",
           seq.frames, seq_ok ? "true" : "false", seq_sha);

    /* seeks */
    printf(",\"seeks\":[");
    for (int i = 0; i < 3; ++i) {
        int64_t target = info.duration_us > 0
            ? (int64_t)((double)info.duration_us * (0.25 * (i + 1))) : 0;
        printf("%s{", i ? "," : "");
        song_handle *s2 = NULL;
        win_source src2;
        if (song_seek(song, target) != 0) {
            printf("\"status\":\"seek_failed\",\"target_us\":%" PRId64 "}", target);
            continue;
        }
        (void)s2; (void)src2;
        pcm_buf post = { .channels = info.channels };
        int ok = decode_all(song, &post) == 0;
        int64_t resume = -1;
        int suffix_exact = 0;
        char post_sha[65] = "";
        /* locate the 64-frame probe prefix inside the sequential stream */
        const size_t probe_frames = 64;
        if (ok && post.frames >= probe_frames && seq.frames >= probe_frames) {
            const float *probe = post.data;
            size_t span = probe_frames * (size_t)seq.channels;
            size_t limit = (seq.frames - probe_frames) * (size_t)seq.channels;
            uint32_t first = ((const uint32_t *)probe)[0];
            for (size_t off = 0; off <= limit; off += (size_t)seq.channels) {
                if (((const uint32_t *)seq.data)[off] != first) continue;
                if (memcmp(seq.data + off, probe, span * sizeof(float)) == 0) {
                    resume = (int64_t)(off / (size_t)seq.channels);
                    break;
                }
            }
            if (resume >= 0) {
                size_t remaining = seq.frames - (size_t)resume;
                suffix_exact = remaining == post.frames &&
                    memcmp(seq.data + (size_t)resume * (size_t)seq.channels,
                           post.data, post.frames * (size_t)seq.channels * sizeof(float)) == 0;
            }
        }
        if (post.frames)
            sha256_hex(post.data, post.frames * (size_t)post.channels * sizeof(float), post_sha);
        printf("\"status\":\"done\",\"target_us\":%" PRId64 ",\"frames\":%zu,"
               "\"resume_frame\":%" PRId64 ",\"suffix_exact\":%s,\"clean_eof\":%s,"
               "\"sha256\":\"%s\"}",
               target, post.frames, resume, suffix_exact ? "true" : "false",
               ok ? "true" : "false", post_sha);
        free(post.data);
    }
    printf("]}\n");
    free(seq.data);
    return seq_ok ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* modes                                                               */
/* ------------------------------------------------------------------ */

static int mode_file(const char *path) {
    wchar_t *wpath = utf8_to_wide(path);
    if (!wpath) { fprintf(stderr, "utf8 conversion failed\n"); return 2; }
    song_handle *song = NULL;
    win_source src;
    if (win_open(wpath, &song, &src) < 0) {
        fprintf(stderr, "open failed: %ls\n", wpath);
        free(wpath);
        return 1;
    }
    int rc = run_contract(song, path);
    win_close(&song, &src);
    free(wpath);
    return rc;
}

#ifndef UNICODE_TEST_DIR
#define UNICODE_TEST_DIR L"qianqian-unicode-test"
#endif

static int mode_unicode(const wchar_t *fixture_w) {
    /* <temp>\qianqian-unicode-test\测试音乐\歌曲-你好世界.m4a */
    wchar_t base[MAX_PATH];
    DWORD n = GetTempPathW(MAX_PATH, base);
    if (!n) return 2;
    wchar_t dir[MAX_PATH], file[MAX_PATH];
    _snwprintf(dir, MAX_PATH, L"%sqianqian-unicode-test\\测试音乐", base);
    _snwprintf(file, MAX_PATH, L"%s\\歌曲-你好世界.m4a", dir);
    dir[MAX_PATH - 1] = 0; file[MAX_PATH - 1] = 0;
    if (!CreateDirectoryW(dir, NULL) && GetLastError() != ERROR_ALREADY_EXISTS)
        return 2;
    if (!CopyFileW(fixture_w, file, FALSE)) {
        fprintf(stderr, "CopyFileW failed: %lu\n", GetLastError());
        return 2;
    }
    song_handle *song = NULL;
    win_source src;
    if (win_open(file, &song, &src) < 0) {
        fprintf(stderr, "wide open failed: %ls\n", file);
        return 1;
    }
    int rc = run_contract(song, "unicode-path");
    win_close(&song, &src);
    DeleteFileW(file);
    /* leave the directory; it is in the temp tree */
    return rc;
}

static int mode_largefile(void) {
    /* A RIFF/WAVE header declaring 3 GiB of PCM data, served by the virtual
     * IO: probing derives duration from the size callback, seeking to 75%
     * must ask the host for an offset > 2 GiB (proving 64-bit IO survives
     * the whole stack), and decode must produce frames afterwards. */
    static const uint8_t wav_header[44] = {
        'R','I','F','F', 0xFF,0xFF,0xFF,0x7F, 'W','A','V','E',
        'f','m','t',' ', 16,0,0,0, 1,0, 2,0, 0x44,0xAC,0,0,
        0x10,0xB1,2,0, 4,0, 16,0,
        'd','a','t','a', 0x00,0x00,0x00,0x70 /* 0x70000000 = 1.75 GiB */
    };
    /* data chunk size 0x70000000 (~1.75 GiB) keeps total RIFF < 2 GiB of
     * declared payload; the HOST size (3 GiB) and the seek arithmetic are
     * what push offsets past 2 GiB via sample seek inside a padded stream.
     * To force >2 GiB offsets deterministically we instead declare the full
     * 3 GiB below. */
    uint8_t hdr[44];
    memcpy(hdr, wav_header, 44);
    uint32_t data_size = 0xB0000000u; /* 2.75 GiB of s16le stereo @44.1k */
    uint32_t riff_size = 36 + data_size;
    hdr[4] = (uint8_t)(riff_size); hdr[5] = (uint8_t)(riff_size >> 8);
    hdr[6] = (uint8_t)(riff_size >> 16); hdr[7] = (uint8_t)(riff_size >> 24);
    hdr[40] = (uint8_t)(data_size); hdr[41] = (uint8_t)(data_size >> 8);
    hdr[42] = (uint8_t)(data_size >> 16); hdr[43] = (uint8_t)(data_size >> 24);

    virtual_source virt = {0};
    /* serve the header, then zeros, through a read callback wrapper */
    /* simplest: materialize a 44-byte prefix buffer inside the source */
    /* (the virtual read returns zeros; patch the prefix via a static) */
    static uint8_t prefix[44];
    memcpy(prefix, hdr, 44);

    song_io io = {
        .userdata = &virt,
        .read = NULL, .seek = virt_seek, .size = virt_size,
    };
    /* wrap: first read serves prefix, later reads zeros */
    /* (implemented with a tiny trampoline below) */
    extern int64_t virt_read_prefix(void *userdata, uint8_t *dst, size_t size);
    io.read = virt_read_prefix;
    /* stash the prefix pointer in a file-scope variable */
    g_prefix = prefix;
    g_prefix_consumed = 0;
    virt.pos = 0;
    virt.max_seek_abs = 0;
    virt.max_read_end = 0;

    song_handle *song = song_open(&io);
    if (!song) { fprintf(stderr, "virtual open failed\n"); return 1; }
    song_info info;
    if (song_probe(song, &info) < 0) { fprintf(stderr, "virtual probe failed\n"); return 1; }
    printf("{\"label\":\"largefile\",\"sample_rate\":%d,\"channels\":%d,"
           "\"duration_us\":%" PRId64 "}", info.sample_rate, info.channels, info.duration_us);
    int rc = 0;
    int64_t target = (int64_t)((double)info.duration_us * 0.75);
    if (song_seek(song, target) != 0) { printf(",\"seek_failed\":true}\n"); return 1; }
    pcm_buf post = { .channels = info.channels };
    int ok = decode_all(song, &post) == 0;
    printf(",\"target_us\":%" PRId64 ",\"max_seek_offset\":%" PRId64
           ",\"max_read_end\":%" PRId64 ",\"post_seek_frames\":%zu,"
           "\"clean_eof\":%s,\"negative_seek\":%s,"
           "\"gt_2gib_seek\":%s}",
           target, virt.max_seek_abs, virt.max_read_end, post.frames,
           ok ? "true" : "false",
           virt.negative_seek ? "true" : "false",
           virt.max_seek_abs > ((int64_t)2 * 1024 * 1024 * 1024) ? "true" : "false");
    if (!ok || virt.max_seek_abs <= (int64_t)2 * 1024 * 1024 * 1024 || virt.negative_seek)
        rc = 1;
    free(post.data);
    song_close(song);
    return rc;
}

/* trampoline state + read wrapper (file scope) */
static uint8_t *g_prefix = NULL;
static size_t g_prefix_consumed = 0;
static int64_t virt_read_prefix(void *userdata, uint8_t *dst, size_t size) {
    virtual_source *v = (virtual_source *)userdata;
    size_t served = 0;
    if (g_prefix_consumed < 44 && v->pos < 44) {
        size_t off = (size_t)v->pos;
        served = 44 - off < size ? 44 - off : size;
        memcpy(dst, g_prefix + off, served);
        g_prefix_consumed = off + served;
        v->pos += (int64_t)served;
        if (v->pos > v->max_read_end) v->max_read_end = v->pos;
        if (served == size) return (int64_t)served;
    }
    int64_t n = virt_read(userdata, dst + served, size - served);
    if (n < 0) return -1;
    return (int64_t)served + n;
}

int wmain(int argc, wchar_t **argv) {
    if (argc >= 3 && wcscmp(argv[1], L"--unicode") == 0)
        return mode_unicode(argv[2]);
    if (argc >= 2 && wcscmp(argv[1], L"--largefile") == 0)
        return mode_largefile();
    if (argc >= 2) {
        int n = WideCharToMultiByte(CP_UTF8, 0, argv[1], -1, NULL, 0, NULL, NULL);
        char *p = (char *)malloc((size_t)n);
        WideCharToMultiByte(CP_UTF8, 0, argv[1], -1, p, n, NULL, NULL);
        int rc = mode_file(p);
        free(p);
        return rc;
    }
    fprintf(stderr, "usage: songcore_windows_host <file>\n"
                    "       songcore_windows_host --unicode <fixture>\n"
                    "       songcore_windows_host --largefile\n");
    return 2;
}
