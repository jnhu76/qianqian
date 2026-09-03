/*
 * songcore_probe — SongCore ABI v1 machine-test instrument (test only).
 *
 * Drives the full frozen SongCore contract over one file and emits ONE JSON
 * record per invocation. The regression gates live in the Python driver
 * (tests/songcore/regression.py) which runs one record per corpus case and
 * checks the consistency invariants:
 *
 *   - probe facts == decoded PCM facts (rate/channels/mask)
 *   - metadata/artwork snapshot stable across decode, EOF and seek
 *   - selected stream identity == decoder stream identity
 *   - seek lands within the documented tolerance; EOF is not an error
 *   - stream selection rebuilds info/metadata and drops old decoder state
 *   - negative cases produce the expected typed status (no generic -1)
 *
 * Modes (exactly one JSON line on stdout):
 *   identity                       embedded test-closure identity (which
 *                                  FFmpeg manifest this binary was built
 *                                  from; regression preflight consumes it)
 *   record  <file> [--select <i>]   full happy-path lifecycle
 *   neg     <file>                  typed open/probe/read statuses
 *   states  <file>                  deterministic fuzz-like state sequences
 *   iofail  <file> <fail_after>     host-I/O fault injection (bytes before
 *                                   reads start failing; 0 = fail immediately)
 *
 * This file is test-only and never links into SongCore or a shipping graph.
 * It knows no FFmpeg types and owns no audio device.
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
#if !defined(_WIN32)
#include <sys/types.h>
#endif

/* -------------------------------------------------------------------------
 * Test-closure identity (test-only provenance, injected by xmake.lua from
 * the canonical identity fields of the replayed FFmpeg manifest; see
 * tests/songcore/README.md). A probe compiled without this plumbing reports
 * "unknown" and the regression preflight rejects it fail-closed.
 * ---------------------------------------------------------------------- */
#if !defined(QN_TEST_AV_PROFILE)
#define QN_TEST_AV_PROFILE "unknown"
#endif
#if !defined(QN_TEST_AV_PROFILE_SHA256)
#define QN_TEST_AV_PROFILE_SHA256 "unknown"
#endif
#if !defined(QN_TEST_AV_TARGET)
#define QN_TEST_AV_TARGET "unknown"
#endif
#if !defined(QN_TEST_AV_FFMPEG_SOURCE_SHA256)
#define QN_TEST_AV_FFMPEG_SOURCE_SHA256 "unknown"
#endif

/* -------------------------------------------------------------------------
 * Host IO (file-backed, with optional fault injection)
 * ---------------------------------------------------------------------- */

typedef struct file_source {
    FILE *file;
    int64_t size;
    int64_t fail_after; /* host reads fail after this many bytes; <0 = never */
    int64_t bytes_read;
    int fail_once; /* only fail one read, then recover */
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
    if (src->fail_after >= 0) {
        if (src->bytes_read >= src->fail_after) {
            if (src->fail_once) return -1;
            return -1;
        }
        if ((int64_t)size > src->fail_after - src->bytes_read)
            size = (size_t)(src->fail_after - src->bytes_read);
    }
    size_t n = fread(dst, 1, size, src->file);
    src->bytes_read += (int64_t)n;
    if (n == 0 && ferror(src->file)) return -1;
    return (int64_t)n;
}

static int64_t host_seek(void *userdata, int64_t absolute_offset) {
    file_source *src = (file_source *)userdata;
    if (absolute_offset < 0 || file_seek64(src->file, absolute_offset, SEEK_SET) != 0)
        return -1;
    src->bytes_read = absolute_offset;
    return file_tell64(src->file);
}

static int64_t host_size(void *userdata) {
    return ((file_source *)userdata)->size;
}

/* -------------------------------------------------------------------------
 * Minimal SHA-256 (test-only; FIPS 180-4)
 * ---------------------------------------------------------------------- */

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

/* Streaming SHA-256 over an open FILE (host-side canonical hashes). */
typedef struct {
    sha256 s;
} pcm_hasher;

static void pcm_hasher_init(pcm_hasher *ph) { sha256_init(&ph->s); }
static void pcm_hasher_update(pcm_hasher *ph, const void *d, size_t n) {
    sha256_update(&ph->s, d, n);
}
static void pcm_hasher_hex(pcm_hasher *ph, char hex[65]) {
    uint8_t d[32];
    sha256_done(&ph->s, d);
    for (int i = 0; i < 32; ++i) sprintf(hex + i * 2, "%02x", d[i]);
    hex[64] = 0;
}

/* -------------------------------------------------------------------------
 * JSON escape helpers
 * ---------------------------------------------------------------------- */

static void json_escape(FILE *f, const char *s, size_t n) {
    fputc('"', f);
    for (size_t i = 0; i < n; ++i) {
        unsigned char c = (unsigned char)s[i];
        switch (c) {
        case '"': fputs("\\\"", f); break;
        case '\\': fputs("\\\\", f); break;
        case '\n': fputs("\\n", f); break;
        case '\r': fputs("\\r", f); break;
        case '\t': fputs("\\t", f); break;
        default:
            if (c < 0x20)
                fprintf(f, "\\u%04x", c);
            else
                fputc(c, f);
            break;
        }
    }
    fputc('"', f);
}

/* -------------------------------------------------------------------------
 * Metadata snapshot serialization (deterministic, for stability hashes)
 * ---------------------------------------------------------------------- */

static void meta_serialize(const song_metadata *m, sha256 *s) {
    struct pair { int has; const char *v; uint32_t len; };
    struct pair fields[] = {
        {m->has_title, m->title, m->title_len},
        {m->has_artist, m->artist, m->artist_len},
        {m->has_album, m->album, m->album_len},
        {m->has_album_artist, m->album_artist, m->album_artist_len},
        {m->has_genre, m->genre, m->genre_len},
        {m->has_composer, m->composer, m->composer_len},
        {m->has_date, m->date, m->date_len},
        {m->has_comment, m->comment, m->comment_len},
    };
    for (size_t i = 0; i < sizeof(fields) / sizeof(fields[0]); ++i) {
        uint8_t b = fields[i].has ? 1 : 0;
        sha256_update(s, &b, 1);
        uint32_t l = fields[i].len;
        sha256_update(s, &l, 4);
        if (fields[i].has && fields[i].len)
            sha256_update(s, fields[i].v, fields[i].len);
    }
    int32_t nums[] = {
        m->track_number, m->track_total, m->disc_number, m->disc_total,
        m->track_gain_mb, m->album_gain_mb,
    };
    uint32_t flags[] = {
        m->has_track_number, m->has_track_total, m->has_disc_number,
        m->has_disc_total, m->has_track_gain, m->has_track_peak,
        m->has_album_gain, m->has_album_peak,
    };
    for (size_t i = 0; i < sizeof(flags) / sizeof(flags[0]); ++i) {
        uint8_t b = flags[i] ? 1 : 0;
        sha256_update(s, &b, 1);
    }
    for (size_t i = 0; i < sizeof(nums) / sizeof(nums[0]); ++i)
        sha256_update(s, &nums[i], 4);
    uint32_t peaks[] = {m->track_peak, m->album_peak};
    for (size_t i = 0; i < sizeof(peaks) / sizeof(peaks[0]); ++i)
        sha256_update(s, &peaks[i], 4);
}

static void raw_serialize(song_handle *h, sha256 *s) {
    uint32_t count = 0;
    if (song_get_metadata_count(h, &count) != SONG_OK) return;
    sha256_update(s, &count, 4);
    for (uint32_t i = 0; i < count; ++i) {
        song_metadata_entry e;
        memset(&e, 0, sizeof(e));
        if (song_get_metadata_entry(h, i, &e) != SONG_OK) break;
        sha256_update(s, &e.scope, 4);
        sha256_update(s, &e.key_len, 4);
        if (e.key_len && e.key) sha256_update(s, e.key, e.key_len);
        sha256_update(s, &e.value_len, 4);
        if (e.value_len && e.value) sha256_update(s, e.value, e.value_len);
    }
}

static void compute_meta_sha(song_handle *h, const song_metadata *m, char hex[65]) {
    sha256 s;
    sha256_init(&s);
    meta_serialize(m, &s);
    raw_serialize(h, &s);
    uint8_t d[32];
    sha256_done(&s, d);
    for (int i = 0; i < 32; ++i) sprintf(hex + i * 2, "%02x", d[i]);
    hex[64] = 0;
}

static void artwork_sha(song_handle *h, char hex[65]) {
    sha256 s;
    sha256_init(&s);
    uint32_t count = 0;
    if (song_get_artwork_count(h, &count) == SONG_OK) {
        sha256_update(&s, &count, 4);
        for (uint32_t i = 0; i < count; ++i) {
            song_artwork_item it;
            memset(&it, 0, sizeof(it));
            if (song_get_artwork_item(h, i, &it) != SONG_OK) break;
            sha256_update(&s, &it.role, 4);
            sha256_update(&s, &it.mime_len, 4);
            if (it.mime_len && it.mime) sha256_update(&s, it.mime, it.mime_len);
            sha256_update(&s, &it.data_len, 8);
            if (it.data_len && it.data) sha256_update(&s, it.data, (size_t)it.data_len);
            sha256_update(&s, &it.is_front_cover, 4);
            sha256_update(&s, &it.width, 4);
            sha256_update(&s, &it.height, 4);
        }
    }
    uint8_t d[32];
    sha256_done(&s, d);
    for (int i = 0; i < 32; ++i) sprintf(hex + i * 2, "%02x", d[i]);
    hex[64] = 0;
}

/* -------------------------------------------------------------------------
 * JSON record emission
 * ---------------------------------------------------------------------- */

static void emit_info(FILE *f, const song_info *info) {
    fprintf(f, "{\"sample_rate\":%d,\"channels\":%d,\"channel_mask\":%" PRIu64
               ",\"duration_us\":%" PRId64 ",\"bits_per_sample\":%d,"
               "\"codec\":", info->sample_rate, info->channels,
               info->channel_mask, info->duration_us, info->bits_per_sample);
    json_escape(f, info->codec, strnlen(info->codec, sizeof(info->codec)));
    fprintf(f, ",\"container\":");
    json_escape(f, info->container, strnlen(info->container, sizeof(info->container)));
    fprintf(f, ",\"selected_audio_index\":%u,\"audio_stream_count\":%u}",
            info->selected_audio_index, info->audio_stream_count);
}

static void emit_streams(FILE *f, song_handle *h) {
    uint32_t count = 0;
    fputs("[", f);
    song_audio_stream_count(h, &count);
    for (uint32_t i = 0; i < count; ++i) {
        song_stream_info si;
        memset(&si, 0, sizeof(si));
        if (song_audio_stream_info(h, i, &si) != SONG_OK) continue;
        if (i) fputc(',', f);
        fprintf(f, "{\"audio_index\":%u,\"stream_index\":%u,\"sample_rate\":%d,"
                   "\"channels\":%d,\"channel_mask\":%" PRIu64 ",\"duration_us\":%"
                   PRId64 ",\"is_default\":%u,\"codec\":",
                si.audio_index, si.stream_index, si.sample_rate, si.channels,
                si.channel_mask, si.duration_us, si.is_default);
        json_escape(f, si.codec, strnlen(si.codec, sizeof(si.codec)));
        fputc('}', f);
    }
    fputs("]", f);
}

static void emit_metadata(FILE *f, song_handle *h) {
    const song_metadata *m = NULL;
    fputs("{\"canonical\":", f);
    if (song_get_metadata(h, &m) != SONG_OK || !m) {
        fputs("null", f);
    } else {
        fputs("{\"title\":", f);
        json_escape(f, m->title ? m->title : "", m->title_len);
        fprintf(f, ",\"has_title\":%u,\"artist\":", m->has_title);
        json_escape(f, m->artist ? m->artist : "", m->artist_len);
        fprintf(f, ",\"has_artist\":%u,\"album\":", m->has_artist);
        json_escape(f, m->album ? m->album : "", m->album_len);
        fprintf(f, ",\"has_album\":%u,\"album_artist\":", m->has_album);
        json_escape(f, m->album_artist ? m->album_artist : "", m->album_artist_len);
        fprintf(f, ",\"has_album_artist\":%u,\"genre\":", m->has_album_artist);
        json_escape(f, m->genre ? m->genre : "", m->genre_len);
        fprintf(f, ",\"has_genre\":%u,\"composer\":", m->has_genre);
        json_escape(f, m->composer ? m->composer : "", m->composer_len);
        fprintf(f, ",\"has_composer\":%u,\"date\":", m->has_composer);
        json_escape(f, m->date ? m->date : "", m->date_len);
        fprintf(f, ",\"has_date\":%u,\"comment\":", m->has_date);
        json_escape(f, m->comment ? m->comment : "", m->comment_len);
        fprintf(f, ",\"has_comment\":%u,"
                   "\"track_number\":%d,\"has_track_number\":%u,"
                   "\"track_total\":%d,\"has_track_total\":%u,"
                   "\"disc_number\":%d,\"has_disc_number\":%u,"
                   "\"disc_total\":%d,\"has_disc_total\":%u,"
                   "\"track_gain_mb\":%d,\"has_track_gain\":%u,"
                   "\"track_peak\":%u,\"has_track_peak\":%u,"
                   "\"album_gain_mb\":%d,\"has_album_gain\":%u,"
                   "\"album_peak\":%u,\"has_album_peak\":%u}",
                m->has_comment,
                m->track_number, m->has_track_number, m->track_total,
                m->has_track_total, m->disc_number, m->has_disc_number,
                m->disc_total, m->has_disc_total, m->track_gain_mb,
                m->has_track_gain, m->track_peak, m->has_track_peak,
                m->album_gain_mb, m->has_album_gain, m->album_peak,
                m->has_album_peak);
    }
    fputs(",\"raw\":[", f);
    uint32_t count = 0;
    if (song_get_metadata_count(h, &count) == SONG_OK) {
        for (uint32_t i = 0; i < count; ++i) {
            song_metadata_entry e;
            memset(&e, 0, sizeof(e));
            if (song_get_metadata_entry(h, i, &e) != SONG_OK) break;
            if (i) fputc(',', f);
            fprintf(f, "{\"scope\":%u,\"key\":", e.scope);
            json_escape(f, e.key ? e.key : "", e.key_len);
            fputs(",\"value\":", f);
            json_escape(f, e.value ? e.value : "", e.value_len);
            fputc('}', f);
        }
    }
    fputs("]}", f);
}

static void emit_artwork(FILE *f, song_handle *h) {
    uint32_t count = 0;
    fputs("[", f);
    if (song_get_artwork_count(h, &count) == SONG_OK) {
        for (uint32_t i = 0; i < count; ++i) {
            song_artwork_item it;
            memset(&it, 0, sizeof(it));
            if (song_get_artwork_item(h, i, &it) != SONG_OK) break;
            if (i) fputc(',', f);
            /* hash of compressed bytes only */
            pcm_hasher ph;
            pcm_hasher_init(&ph);
            if (it.data_len && it.data)
                pcm_hasher_update(&ph, it.data, (size_t)it.data_len);
            char sha[65];
            pcm_hasher_hex(&ph, sha);
            fprintf(f, "{\"role\":%u,\"mime\":", it.role);
            json_escape(f, it.mime ? it.mime : "", it.mime_len);
            fprintf(f, ",\"size\":%" PRIu64 ",\"sha256\":\"%s\","
                       "\"width\":%d,\"height\":%d,\"is_front_cover\":%u}",
                    it.data_len, sha, it.width, it.height, it.is_front_cover);
        }
    }
    fputs("]", f);
}

static void emit_last_error(FILE *f, song_handle *h) {
    const song_error *e = NULL;
    if (song_last_error(h, &e) == SONG_OK && e) {
        fprintf(f, "{\"native\":%d,\"msg\":", e->native_code);
        json_escape(f, e->message ? e->message : "", e->message_len);
        fputs("}", f);
    } else {
        fputs("null", f);
    }
}

/* -------------------------------------------------------------------------
 * Decode to EOF through the ABI; hash PCM; return frames + final status
 * ---------------------------------------------------------------------- */

typedef struct decode_result {
    uint64_t frames;
    char sha[65];
    song_status final_status; /* SONG_EOF on clean end */
} decode_result;

static void decode_all(song_handle *h, int channels, decode_result *out) {
    memset(out, 0, sizeof(*out));
    pcm_hasher ph;
    pcm_hasher_init(&ph);
    size_t cap = 4096;
    float *buf = (float *)malloc(cap * (size_t)(channels > 0 ? channels : 2) * sizeof(float));
    if (!buf) {
        out->final_status = SONG_ERR_OUT_OF_MEMORY;
        return;
    }
    song_status st = SONG_OK;
    for (;;) {
        uint64_t n = 0;
        st = song_read_pcm(h, buf, cap, &n);
        if (st == SONG_EOF) break;
        if (st != SONG_OK) break;
        if (n == 0) break;
        pcm_hasher_update(&ph, buf, (size_t)n * (size_t)channels * sizeof(float));
        out->frames += n;
    }
    pcm_hasher_hex(&ph, out->sha);
    free(buf);
    out->final_status = (st == SONG_EOF) ? SONG_EOF : st;
}

/* -------------------------------------------------------------------------
 * record mode
 * ---------------------------------------------------------------------- */

static int open_song(const char *path, song_handle **out, file_source *src,
                     int64_t fail_after) {
    memset(src, 0, sizeof(*src));
    src->file = fopen(path, "rb");
    if (!src->file) return -1;
    if (file_seek64(src->file, 0, SEEK_END) != 0 ||
        (src->size = file_tell64(src->file)) < 0 ||
        file_seek64(src->file, 0, SEEK_SET) != 0) {
        fclose(src->file);
        src->file = NULL;
        return -1;
    }
    src->fail_after = fail_after;
    song_io io = {.userdata = src, .read = host_read, .seek = host_seek,
                  .size = host_size};
    song_handle *s = NULL;
    if (song_open(&io, &s) != SONG_OK || !s) {
        fclose(src->file);
        src->file = NULL;
        return -1;
    }
    *out = s;
    return 0;
}

static void close_song(song_handle **song, file_source *src) {
    if (*song) {
        song_close(*song);
        *song = NULL;
    }
    if (src->file) {
        fclose(src->file);
        src->file = NULL;
    }
}

static int mode_record(const char *path, int select_index) {
    FILE *f = stdout;
    song_handle *song = NULL;
    file_source src;
    if (open_song(path, &song, &src, -1) < 0) {
        printf("{\"file\":");
        json_escape(f, path, strlen(path));
        printf(",\"phase\":\"open_failed\"}\n");
        return 1;
    }

    song_info info;
    memset(&info, 0, sizeof(info));
    song_status ps = song_probe(song, &info);
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));
    fprintf(f, ",\"phase\":\"%s\",\"open_status\":0,\"probe_status\":%d,",
            ps == SONG_OK ? "ok" : "probe_failed", (int)ps);
    if (ps != SONG_OK) {
        fputs("\"diag\":", f);
        emit_last_error(f, song);
        printf("}\n");
        close_song(&song, &src);
        return 1;
    }

    /* probe snapshot */
    fputs("\"info\":", f);
    emit_info(f, &info);
    fputs(",\"streams\":", f);
    emit_streams(f, song);

    /* metadata + artwork before any decode */
    const song_metadata *meta = NULL;
    song_get_metadata(song, &meta);
    char meta_sha[65] = "";
    char art_sha[65] = "";
    if (meta) compute_meta_sha(song, meta, meta_sha);
    artwork_sha(song, art_sha);
    fputs(",\"metadata\":", f);
    emit_metadata(f, song);
    fprintf(f, ",\"metadata_sha\":\"%s\",\"artwork\":", meta_sha);
    emit_artwork(f, song);
    fprintf(f, ",\"artwork_sha\":\"%s\"", art_sha);

    /* full decode */
    decode_result dr;
    decode_all(song, info.channels, &dr);
    fprintf(f, ",\"decode\":{\"frames\":%" PRIu64 ",\"pcm_sha256\":\"%s\","
               "\"final_status\":%d}",
            dr.frames, dr.sha, (int)dr.final_status);

    /* snapshot stability after decode (EOF) */
    char meta_after_decode[65] = "";
    char art_after_decode[65] = "";
    if (meta) compute_meta_sha(song, meta, meta_after_decode);
    artwork_sha(song, art_after_decode);
    fprintf(f, ",\"metadata_after_decode_sha\":\"%s\","
               "\"artwork_after_decode_sha\":\"%s\"",
            meta_after_decode, art_after_decode);

    /* seeks: 25/50/75% of declared duration on FRESH handles */
    fputs(",\"seeks\":[", f);
    for (int i = 0; i < 3; ++i) {
        int64_t target = info.duration_us > 0
                             ? (int64_t)((double)info.duration_us * (0.25 * (i + 1)))
                             : 0;
        if (i) fputc(',', f);
        song_handle *s2 = NULL;
        file_source src2;
        int64_t actual = -1;
        song_status st = SONG_ERR_STATE;
        decode_result sdr;
        memset(&sdr, 0, sizeof(sdr));
        if (open_song(path, &s2, &src2, -1) == 0) {
            song_info info2;
            memset(&info2, 0, sizeof(info2));
            if (song_probe(s2, &info2) == SONG_OK) {
                st = song_seek(s2, target, &actual);
                if (st == SONG_OK) {
                    decode_all(s2, info2.channels, &sdr);
                }
            }
        }
        fprintf(f, "{\"target_us\":%" PRId64 ",\"status\":%d,\"actual_us\":%"
                   PRId64 ",\"frames\":%" PRIu64 ",\"pcm_sha256\":\"%s\"}",
                target, (int)st, actual, sdr.frames, sdr.sha);
        close_song(&s2, &src2);
    }
    fputs("]", f);

    /* snapshot stability after seek (from a fresh seek handle) */
    char meta_after_seek[65] = "";
    char art_after_seek[65] = "";
    song_handle *s3 = NULL;
    file_source src3;
    if (open_song(path, &s3, &src3, -1) == 0) {
        song_info info3;
        memset(&info3, 0, sizeof(info3));
        if (song_probe(s3, &info3) == SONG_OK) {
            int64_t a3 = -1;
            /* attempt the seek regardless of outcome: snapshot stability is
             * checked after the seek ATTEMPT, even for SEEK_UNSUPPORTED */
            song_seek(s3, info3.duration_us > 0 ? info3.duration_us / 2 : 0,
                      &a3);
            const song_metadata *m3 = NULL;
            song_get_metadata(s3, &m3);
            if (m3) compute_meta_sha(s3, m3, meta_after_seek);
            artwork_sha(s3, art_after_seek);
        }
        close_song(&s3, &src3);
    }
    fprintf(f, ",\"metadata_after_seek_sha\":\"%s\","
               "\"artwork_after_seek_sha\":\"%s\"",
            meta_after_seek, art_after_seek);

    /* explicit stream selection (only when the file has >1 audio stream) */
    if (select_index >= 0) {
        song_handle *s4 = NULL;
        file_source src4;
        fputs(",\"select\":{", f);
        if (open_song(path, &s4, &src4, -1) == 0) {
            song_info info4;
            memset(&info4, 0, sizeof(info4));
            if (song_probe(s4, &info4) == SONG_OK &&
                select_index < (int)info4.audio_stream_count) {
                song_status sel = song_select_stream(s4, (uint32_t)select_index);
                song_info info5;
                memset(&info5, 0, sizeof(info5));
                song_probe(s4, &info5);
                decode_result sdr;
                memset(&sdr, 0, sizeof(sdr));
                if (sel == SONG_OK) decode_all(s4, info5.channels, &sdr);
                char msha[65] = "";
                const song_metadata *m5 = NULL;
                song_get_metadata(s4, &m5);
                if (m5) compute_meta_sha(s4, m5, msha);
                char asha[65] = "";
                artwork_sha(s4, asha);
                fprintf(f, "\"status\":%d,\"selected\":%u,\"info\":", (int)sel,
                        info5.selected_audio_index);
                emit_info(f, &info5);
                fprintf(f, ",\"decode_frames\":%" PRIu64
                           ",\"pcm_sha256\":\"%s\",\"metadata_sha\":\"%s\","
                           "\"artwork_sha\":\"%s\"}",
                        sdr.frames, sdr.sha, msha, asha);
            } else {
                fputs("\"status\":-1,\"info\":null}", f);
            }
            close_song(&s4, &src4);
        } else {
            fputs("\"status\":-1,\"info\":null}", f);
        }
    }

    fputs("}", f);
    printf("\n");
    close_song(&song, &src);
    return 0;
}

/* -------------------------------------------------------------------------
 * neg mode: typed error classification
 * ---------------------------------------------------------------------- */

static int mode_neg(const char *path) {
    FILE *f = stdout;
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));

    file_source src;
    memset(&src, 0, sizeof(src));
    src.fail_after = -1; /* no fault injection in the plain negative path */
    src.file = fopen(path, "rb");
    int host_ok = src.file != NULL;
    if (host_ok) {
        if (file_seek64(src.file, 0, SEEK_END) != 0 ||
            (src.size = file_tell64(src.file)) < 0 ||
            file_seek64(src.file, 0, SEEK_SET) != 0)
            host_ok = 0;
    }
    song_handle *song = NULL;
    int open_status = -2; /* -2: host file could not be opened (not ABI) */
    if (host_ok) {
        song_io io = {.userdata = &src, .read = host_read, .seek = host_seek,
                      .size = host_size};
        open_status = (int)song_open(&io, &song);
    }

    int probe_status = -2;
    song_info info;
    memset(&info, 0, sizeof(info));
    if (host_ok && song) {
        probe_status = (int)song_probe(song, &info);
    }

    int read_status = -2;
    uint64_t read_frames = 0;
    int eof_status = -2;
    if (host_ok && song && probe_status == 0) {
        float buf[8192];
        uint64_t n = 0;
        song_status st = song_read_pcm(song, buf, 512, &n);
        read_frames = n;
        if (st == SONG_EOF) {
            eof_status = 1;
            read_status = 1;
        } else if (st == SONG_OK) {
            read_status = 0;
            eof_status = 0;
        } else {
            read_status = (int)st;
            eof_status = 0;
        }
    }

    fprintf(f, ",\"open_status\":%d,\"probe_status\":%d,\"read_status\":%d,"
               "\"read_frames\":%" PRIu64 ",\"eof\":%d",
            open_status, probe_status, read_status, read_frames, eof_status);
    if (song) {
        fputs(",\"diag\":", f);
        emit_last_error(f, song);
    } else {
        fputs(",\"diag\":null", f);
    }
    fputs("}\n", f);

    if (song) song_close(song);
    if (src.file) fclose(src.file);
    return 0;
}

/* -------------------------------------------------------------------------
 * lasterror mode: the song_last_error contract (diagnostics are promised on
 * every typed failure with a valid handle, and cleared by SONG_OK/EOF)
 * ---------------------------------------------------------------------- */

static int mode_lasterror(const char *path) {
    FILE *f = stdout;
    song_handle *song = NULL;
    file_source src;
    if (open_song(path, &song, &src, -1) < 0) {
        printf("{\"file\":");
        json_escape(f, path, strlen(path));
        printf(",\"phase\":\"open_failed\"}\n");
        return 1;
    }

    song_info info;
    memset(&info, 0, sizeof(info));
    int probe_status = (int)song_probe(song, &info);
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));
    fprintf(f, ",\"probe_status\":%d", probe_status);

    const song_error *e = NULL;

    /* 1) invalid stream index -> typed INVALID_ARGUMENT + diagnostic */
    int sel_status = (int)song_select_stream(song, info.audio_stream_count + 7);
    int sel_diag_len = -1;
    if (song_last_error(song, &e) == SONG_OK && e)
        sel_diag_len = e->message ? (int)e->message_len : -1;
    fprintf(f, ",\"select_invalid_status\":%d,\"select_invalid_diag_len\":%d",
            sel_status, sel_diag_len);

    /* 2) success clears: probe again -> SONG_OK + NULL message */
    int probe2_status = (int)song_probe(song, &info);
    int probe2_diag_len = -1;
    int probe2_msg_null = -1;
    if (song_last_error(song, &e) == SONG_OK && e) {
        probe2_msg_null = e->message == NULL;
        probe2_diag_len = (int)e->message_len;
    }
    fprintf(f, ",\"probe2_status\":%d,\"probe2_msg_null\":%s,"
               "\"probe2_diag_len\":%d",
            probe2_status, probe2_msg_null == 1 ? "true" : "false",
            probe2_diag_len);

    /* 3) zero decode capacity -> INVALID_ARGUMENT + diagnostic */
    uint64_t n = 0;
    int zero_status = (int)song_read_pcm(song, NULL, 0, &n);
    int zero_diag_len = -1;
    if (song_last_error(song, &e) == SONG_OK && e)
        zero_diag_len = e->message ? (int)e->message_len : -1;
    fprintf(f, ",\"zero_capacity_status\":%d,\"zero_capacity_diag_len\":%d",
            zero_status, zero_diag_len);

    /* 4) EOF is not an error: decode to end -> SONG_EOF + NULL message */
    float buf[8192];
    int final_status = -2;
    for (;;) {
        uint64_t got = 0;
        uint64_t cap = sizeof(buf) / (sizeof(buf[0]) *
                        (uint64_t)(info.channels > 0 ? info.channels : 2));
        song_status st = song_read_pcm(song, buf, cap, &got);
        if (st != SONG_OK) { final_status = (int)st; break; }
    }
    int eof_diag_len = -1;
    int eof_msg_null = -1;
    if (song_last_error(song, &e) == SONG_OK && e) {
        eof_msg_null = e->message == NULL;
        eof_diag_len = (int)e->message_len;
    }
    fprintf(f, ",\"decode_final_status\":%d,\"eof_msg_null\":%s,"
               "\"eof_diag_len\":%d",
            final_status, eof_msg_null == 1 ? "true" : "false",
            eof_diag_len);

    /* 5) metadata entry out of range -> INVALID_ARGUMENT + diagnostic */
    uint32_t raw_count = 0;
    song_get_metadata_count(song, &raw_count);
    song_metadata_entry ent;
    int ent_status = (int)song_get_metadata_entry(song, raw_count + 3, &ent);
    int ent_diag_len = -1;
    if (song_last_error(song, &e) == SONG_OK && e)
        ent_diag_len = e->message ? (int)e->message_len : -1;
    fprintf(f, ",\"entry_invalid_status\":%d,\"entry_invalid_diag_len\":%d",
            ent_status, ent_diag_len);

    /* 6) artwork out of range (count is 0 or item exists; +5 is always
     * invalid) -> INVALID_ARGUMENT + diagnostic */
    uint32_t art_count = 0;
    song_get_artwork_count(song, &art_count);
    song_artwork_item art;
    int art_status = (int)song_get_artwork_item(song, art_count + 5, &art);
    int art_diag_len = -1;
    if (song_last_error(song, &e) == SONG_OK && e)
        art_diag_len = e->message ? (int)e->message_len : -1;
    fprintf(f, ",\"art_invalid_status\":%d,\"art_invalid_diag_len\":%d",
            art_status, art_diag_len);

    /* 7) seek success clears again */
    int seek_status = (int)song_seek(song, 0, NULL);
    int seek_diag_len = -1;
    int seek_msg_null = -1;
    if (song_last_error(song, &e) == SONG_OK && e) {
        seek_msg_null = e->message == NULL;
        seek_diag_len = (int)e->message_len;
    }
    fprintf(f, ",\"seek_status\":%d,\"seek_msg_null\":%s,\"seek_diag_len\":%d",
            seek_status,
            seek_msg_null == 1 ? "true" : "false", seek_diag_len);

    fputs("}\n", f);
    close_song(&song, &src);
    return 0;
}

/* -------------------------------------------------------------------------
 * states mode: deterministic fuzz-like call sequences
 * ---------------------------------------------------------------------- */

static int mode_states(const char *path) {
    FILE *f = stdout;
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));

    song_handle *song = NULL;
    file_source src;
    int open_ok = open_song(path, &song, &src, -1) == 0;

    /* Sequence 1: read before probe must be a typed state error. */
    int pre_read = -2;
    if (open_ok) {
        uint64_t n = 0;
        pre_read = (int)song_read_pcm(song, NULL, 0, &n); /* invalid args */
    }

    song_info info;
    memset(&info, 0, sizeof(info));
    int ps = open_ok ? (int)song_probe(song, &info) : -2;
    int metadata_pre = -2;
    if (open_ok && ps == 0) {
        const song_metadata *m = NULL;
        metadata_pre = (int)song_get_metadata(song, &m);
    }

    /* Sequence: open -> probe -> read -> seek -> read -> EOF -> seek -> read */
    uint64_t total_frames = 0;
    int seq_ok = 1;
    int seek1 = -2, seek2 = -2, eof_seen = 0;
    if (open_ok && ps == 0) {
        float buf[8192];
        int64_t target = info.duration_us > 0 ? info.duration_us / 3 : 0;
        int64_t a1 = -1;
        seek1 = (int)song_seek(song, target, &a1);
        for (int i = 0; i < 8; ++i) {
            uint64_t n = 0;
            song_status st = song_read_pcm(song, buf, 1024, &n);
            total_frames += n;
            if (st == SONG_EOF) {
                eof_seen = 1;
                break;
            }
            if (st != SONG_OK) {
                seq_ok = 0;
                break;
            }
        }
        int64_t a2 = -1;
        seek2 = (int)song_seek(song, 0, &a2);
        uint64_t n = 0;
        song_status st = song_read_pcm(song, buf, 512, &n);
        if (st != SONG_OK && st != SONG_EOF) seq_ok = 0;
    }

    /* Sequence 2: open -> probe -> select(0) -> read (same index is a full
     * reset; decoder state from before must be gone) */
    int select_ok = -2;
    int select_read = -2;
    if (open_ok && ps == 0) {
        select_ok = (int)song_select_stream(song, info.selected_audio_index);
        uint64_t n = 0;
        float buf[4096];
        song_status st = song_read_pcm(song, buf, 256, &n);
        select_read = (st == SONG_OK || st == SONG_EOF) ? (int)st : (int)st;
    }

    /* invalid stream index must be typed */
    int bad_select = -2;
    if (open_ok && ps == 0)
        bad_select = (int)song_select_stream(song, 999999);

    fprintf(f, ",\"open_ok\":%s,\"pre_read_invalid_status\":%d,"
               "\"probe_status\":%d,\"metadata_pre_status\":%d,"
               "\"seek_mid_status\":%d,\"seek_zero_status\":%d,"
               "\"eof_seen\":%s,\"sequence_ok\":%s,"
               "\"select_same_status\":%d,\"select_read_status\":%d,"
               "\"select_invalid_status\":%d}\n",
            open_ok ? "true" : "false", pre_read, ps, metadata_pre, seek1,
            seek2, eof_seen ? "true" : "false", seq_ok ? "true" : "false",
            select_ok, select_read, bad_select);

    close_song(&song, &src);
    return 0;
}

/* -------------------------------------------------------------------------
 * iofail mode: host I/O fault injection
 * ---------------------------------------------------------------------- */

static int mode_iofail(const char *path, int64_t fail_after) {
    FILE *f = stdout;
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));

    file_source src;
    memset(&src, 0, sizeof(src));
    src.file = fopen(path, "rb");
    if (!src.file) {
        fprintf(f, ",\"phase\":\"host_open_failed\"}\n");
        return 1;
    }
    file_seek64(src.file, 0, SEEK_END);
    src.size = file_tell64(src.file);
    file_seek64(src.file, 0, SEEK_SET);
    src.fail_after = fail_after;

    song_io io = {.userdata = &src, .read = host_read, .seek = host_seek,
                  .size = host_size};
    song_handle *song = NULL;
    int open_status = (int)song_open(&io, &song);
    int probe_status = -2;
    song_info info;
    memset(&info, 0, sizeof(info));
    if (open_status == 0 && song)
        probe_status = (int)song_probe(song, &info);
    int read_status = -2;
    uint64_t frames = 0;
    if (open_status == 0 && song && probe_status == 0) {
        float buf[8192];
        uint64_t n = 0;
        song_status st = song_read_pcm(song, buf, 512, &n);
        frames = n;
        read_status = (int)st;
    }
    fprintf(f, ",\"open_status\":%d,\"probe_status\":%d,\"read_status\":%d,"
               "\"read_frames\":%" PRIu64 "}\n",
            open_status, probe_status, read_status, frames);
    if (song) song_close(song);
    if (src.file) fclose(src.file);
    return 0;
}

/* -------------------------------------------------------------------------
 * lifetime mode: borrowed view lifetime (ABI §metadata)
 *
 * Save the ORIGINAL metadata/raw views, then perform read PCM, seek, and
 * read-to-EOF, then dereference the SAVED pointers and compare their bytes
 * against freshly fetched views. A realloc-based snapshot would either
 * dangle (ASan) or drift; a stable two-pass snapshot must survive intact.
 * ---------------------------------------------------------------------- */

static int bytes_eq(const char *a, uint32_t alen, const char *b, uint32_t blen) {
    return alen == blen && (alen == 0 || memcmp(a, b, alen) == 0);
}

static int mode_lifetime(const char *path) {
    FILE *f = stdout;
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));

    song_handle *song = NULL;
    file_source src;
    if (open_song(path, &song, &src, -1) < 0) {
        fputs(",\"phase\":\"open_failed\"}\n", f);
        return 1;
    }
    song_info info;
    memset(&info, 0, sizeof(info));
    if (song_probe(song, &info) != SONG_OK) {
        fputs(",\"phase\":\"probe_failed\"}\n", f);
        close_song(&song, &src);
        return 1;
    }

    /* Save the ORIGINAL borrowed views before any operation. */
    const song_metadata *m = NULL;
    song_get_metadata(song, &m);
    const char *title = m ? m->title : NULL;
    uint32_t title_len = m ? m->title_len : 0;
    uint32_t title_has = m ? m->has_title : 0;
    const char *artist = m ? m->artist : NULL;
    uint32_t artist_len = m ? m->artist_len : 0;
    uint32_t artist_has = m ? m->has_artist : 0;

    enum { SAVE_CAP = 8 };
    struct { const char *key, *value; uint32_t key_len, value_len; } saved[SAVE_CAP];
    uint32_t n_saved = 0;
    uint32_t raw_count = 0;
    if (song_get_metadata_count(song, &raw_count) == SONG_OK) {
        for (uint32_t i = 0; i < raw_count && n_saved < SAVE_CAP; ++i) {
            song_metadata_entry e;
            memset(&e, 0, sizeof(e));
            if (song_get_metadata_entry(song, i, &e) != SONG_OK) break;
            saved[n_saved].key = e.key;
            saved[n_saved].value = e.value;
            saved[n_saved].key_len = e.key_len;
            saved[n_saved].value_len = e.value_len;
            n_saved++;
        }
    }

    /* Perform: read PCM, seek, read to EOF. */
    int seek_status = -2;
    int read_status = 0;
    int eof = 0;
    uint64_t frames = 0;
    {
        float buf[8192];
        for (int i = 0; i < 64; ++i) {
            uint64_t n = 0;
            song_status st = song_read_pcm(song, buf, 1024, &n);
            frames += n;
            if (st != SONG_OK || n == 0) break;
        }
        int64_t a = -1;
        int64_t target = info.duration_us > 0 ? info.duration_us / 2 : 0;
        seek_status = (int)song_seek(song, target, &a);
        for (;;) {
            uint64_t n = 0;
            song_status st = song_read_pcm(song, buf, 4096, &n);
            frames += n;
            if (st == SONG_EOF) { eof = 1; break; }
            if (st != SONG_OK) { read_status = (int)st; break; }
            if (n == 0) break;
        }
    }

    /* Dereference the SAVED pointers and compare with fresh views. */
    int title_ok = 1, artist_ok = 1, raw_ok = 1;
    const song_metadata *m2 = NULL;
    song_get_metadata(song, &m2);
    if (m2) {
        if (title_has != m2->has_title ||
            !bytes_eq(title, title_len, m2->title, m2->title_len))
            title_ok = 0;
        if (artist_has != m2->has_artist ||
            !bytes_eq(artist, artist_len, m2->artist, m2->artist_len))
            artist_ok = 0;
    } else {
        title_ok = 0;
        artist_ok = 0;
    }
    for (uint32_t i = 0; i < n_saved; ++i) {
        song_metadata_entry e;
        memset(&e, 0, sizeof(e));
        if (song_get_metadata_entry(song, i, &e) != SONG_OK ||
            !bytes_eq(saved[i].key, saved[i].key_len, e.key, e.key_len) ||
            !bytes_eq(saved[i].value, saved[i].value_len, e.value, e.value_len)) {
            raw_ok = 0;
            break;
        }
    }

    fprintf(f, ",\"title_saved_ok\":%s,\"artist_saved_ok\":%s,"
               "\"raw_saved_ok\":%s,\"seek_status\":%d,\"read_status\":%d,"
               "\"eof\":%s,\"frames\":%" PRIu64 "}\n",
            title_ok ? "true" : "false", artist_ok ? "true" : "false",
            raw_ok ? "true" : "false", seek_status, read_status,
            eof ? "true" : "false", frames);

    close_song(&song, &src);
    return 0;
}

/* -------------------------------------------------------------------------
 * switchcheck mode: stream switch resets demux/IO (ABI §song_select_stream)
 *
 *   handle A: probe, decode significant PCM on the default stream, select
 *             stream 1, decode the first N frames.
 *   handle B: fresh open, probe, select stream 1, decode the first N frames.
 *
 * A PCM must equal B PCM: selection must rewind the source to the start.
 * ---------------------------------------------------------------------- */

static void decode_first(song_handle *h, int channels, uint64_t max_frames,
                         uint64_t *out_frames, char sha[65]) {
    pcm_hasher ph;
    pcm_hasher_init(&ph);
    uint64_t got = 0;
    size_t cap = 4096;
    float *buf = (float *)malloc(cap * (size_t)(channels > 0 ? channels : 2) *
                                 sizeof(float));
    if (!buf) {
        *out_frames = 0;
        sha[0] = 0;
        return;
    }
    while (got < max_frames) {
        uint64_t n = 0;
        song_status st = song_read_pcm(h, buf, cap, &n);
        if (st != SONG_OK || n == 0) break;
        pcm_hasher_update(&ph, buf, (size_t)n * (size_t)channels * sizeof(float));
        got += n;
    }
    pcm_hasher_hex(&ph, sha);
    free(buf);
    *out_frames = got;
}

static int mode_switchcheck(const char *path) {
    FILE *f = stdout;
    fprintf(f, "{\"file\":");
    json_escape(f, path, strlen(path));

    song_handle *a = NULL, *b = NULL;
    file_source sa, sb;
    if (open_song(path, &a, &sa, -1) < 0 || open_song(path, &b, &sb, -1) < 0) {
        fputs(",\"phase\":\"open_failed\"}\n", f);
        close_song(&a, &sa);
        close_song(&b, &sb);
        return 1;
    }
    song_info ia, ib;
    memset(&ia, 0, sizeof(ia));
    memset(&ib, 0, sizeof(ib));
    if (song_probe(a, &ia) != SONG_OK || song_probe(b, &ib) != SONG_OK) {
        fputs(",\"phase\":\"probe_failed\"}\n", f);
        close_song(&a, &sa);
        close_song(&b, &sb);
        return 1;
    }
    if (ia.audio_stream_count < 2) {
        fputs(",\"phase\":\"single_stream\"}\n", f);
        close_song(&a, &sa);
        close_song(&b, &sb);
        return 0;
    }
    const uint32_t sel = 1;

    /* A: advance the default stream significantly, then switch. */
    {
        float buf[8192];
        for (int i = 0; i < 200; ++i) {
            uint64_t n = 0;
            if (song_read_pcm(a, buf, 4096, &n) != SONG_OK) break;
            if (n == 0) break;
        }
    }
    song_status st_a = song_select_stream(a, sel);
    song_status st_b = song_select_stream(b, sel);

    /* post-select info for the switched stream layout */
    song_info ia2, ib2;
    memset(&ia2, 0, sizeof(ia2));
    memset(&ib2, 0, sizeof(ib2));
    song_probe(a, &ia2);
    song_probe(b, &ib2);

    char sha_a[65] = "", sha_b[65] = "";
    uint64_t fa = 0, fb = 0;
    if (st_a == SONG_OK && st_b == SONG_OK && ia2.channels == ib2.channels &&
        ia2.channels > 0) {
        decode_first(a, ia2.channels, 1024, &fa, sha_a);
        decode_first(b, ib2.channels, 1024, &fb, sha_b);
    }

    int equal = st_a == SONG_OK && st_b == SONG_OK && fa == fb && fa > 0 &&
                strcmp(sha_a, sha_b) == 0;
    fprintf(f, ",\"select_a_status\":%d,\"select_b_status\":%d,"
               "\"frames_a\":%" PRIu64 ",\"frames_b\":%" PRIu64
               ",\"pcm_sha_a\":\"%s\",\"pcm_sha_b\":\"%s\","
               "\"pcm_equal\":%s}\n",
            (int)st_a, (int)st_b, fa, fb, sha_a, sha_b,
            equal ? "true" : "false");

    close_song(&a, &sa);
    close_song(&b, &sb);
    return 0;
}

/* -------------------------------------------------------------------------
 * identity mode: embedded test-closure identity (regression preflight)
 * ---------------------------------------------------------------------- */

static void identity_field(const char *name, const char *value) {
    fputs(",\"", stdout);
    fputs(name, stdout);
    fputs("\":", stdout);
    json_escape(stdout, value, strlen(value));
}

static int mode_identity(void) {
    fputs("{\"kind\":\"songcore-probe-identity\"", stdout);
    identity_field("profile", QN_TEST_AV_PROFILE);
    identity_field("profile_sha256", QN_TEST_AV_PROFILE_SHA256);
    identity_field("target", QN_TEST_AV_TARGET);
    identity_field("ffmpeg_source_sha256", QN_TEST_AV_FFMPEG_SOURCE_SHA256);
    fputs("}\n", stdout);
    return 0;
}

int main(int argc, char **argv) {
    const char *mode = argc >= 2 ? argv[1] : "";
    if (!strcmp(mode, "identity")) return mode_identity();
    if (argc < 3) {
        fprintf(stderr,
                "usage: %s identity\n"
                "       %s record <file> [--select <i>]\n"
                "       %s neg <file>\n"
                "       %s states <file>\n"
                "       %s iofail <file> <fail_after_bytes>\n"
                "       %s lifetime <file>\n"
                "       %s switchcheck <file>\n"
                "       %s lasterror <file>\n",
                argv[0], argv[0], argv[0], argv[0], argv[0], argv[0],
                argv[0], argv[0]);
        return 2;
    }
    const char *path = argv[2];

    if (!strcmp(mode, "record")) {
        int select_index = -1;
        for (int i = 3; i < argc - 1; ++i) {
            if (!strcmp(argv[i], "--select") && i + 1 < argc)
                select_index = atoi(argv[i + 1]);
        }
        return mode_record(path, select_index);
    }
    if (!strcmp(mode, "neg")) return mode_neg(path);
    if (!strcmp(mode, "states")) return mode_states(path);
    if (!strcmp(mode, "lifetime")) return mode_lifetime(path);
    if (!strcmp(mode, "switchcheck")) return mode_switchcheck(path);
    if (!strcmp(mode, "lasterror")) return mode_lasterror(path);
    if (!strcmp(mode, "iofail") && argc >= 4)
        return mode_iofail(path, atoll(argv[3]));
    fprintf(stderr, "unknown mode\n");
    return 2;
}
