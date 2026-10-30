/*
 * consume_corpus.c — Lane C (linux) external SongCore package consumer.
 *
 * Compiled and linked ONLY against the unpacked release package:
 *   -I<package>/include                (songcore.h)
 *   -L<package>/lib              -lsongcore   (static)
 *   -L<package>/lib/shared       -lsongcore   (shared)
 *
 * Walks the frozen G2 scenario for one fixture:
 *   open -> probe -> metadata -> full decode PCM sha256 -> seek 25/50/75 %
 *        -> EOF/drain -> close
 *
 * PCM sha256 protocol is byte-identical to
 * native/experiments/songcore-equivalence/harness.c: SHA-256 over the raw
 * Float32 interleaved little-endian bytes, read block = 4096 frames.
 *
 * Output: one JSON object per line on stdout. Exit 0 only when every gating
 * check passed; 1 on a gated failure; 2 on usage.
 */
#define _POSIX_C_SOURCE 200809L

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#include "songcore.h"

/* ------------------------------------------------------------------ */
/* SHA-256 (same implementation/protocol as the equivalence harness)  */
/* ------------------------------------------------------------------ */

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
    int i, t;
    for (i = 0; i < 16; i++)
        w[i] = ((uint32_t)p[4*i] << 24) | ((uint32_t)p[4*i+1] << 16) |
               ((uint32_t)p[4*i+2] << 8) | (uint32_t)p[4*i+3];
    for (i = 16; i < 64; i++) {
        uint32_t s0 = ror(w[i-15], 7) ^ ror(w[i-15], 18) ^ (w[i-15] >> 3);
        uint32_t s1 = ror(w[i-2], 17) ^ ror(w[i-2], 19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    uint32_t a=c->h[0],b=c->h[1],d=c->h[2],e=c->h[3];
    uint32_t f=c->h[4],g=c->h[5],hh=c->h[6],ii=c->h[7];
    for (t = 0; t < 64; t++) {
        uint32_t S1 = ror(f,6) ^ ror(f,11) ^ ror(f,25);
        uint32_t ch = (f & g) ^ (~f & hh);
        uint32_t t1 = ii + S1 + ch + K256[t] + w[t];
        uint32_t S0 = ror(a,2) ^ ror(a,13) ^ ror(a,22);
        uint32_t mj = (a & b) ^ (a & d) ^ (b & d);
        uint32_t t2 = S0 + mj;
        ii = hh; hh = g; g = f; f = e + t1;
        e = d; d = b; b = a; a = t1 + t2;
    }
    c->h[0]+=a; c->h[1]+=b; c->h[2]+=d; c->h[3]+=e;
    c->h[4]+=f; c->h[5]+=g; c->h[6]+=hh; c->h[7]+=ii;
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
    uint8_t pad = 0x80, zero = 0, tail[8];
    int i, j;
    sha256_update(c, &pad, 1);
    while (c->buf_len != 56) sha256_update(c, &zero, 1);
    for (i = 0; i < 8; i++) tail[i] = (uint8_t)(bits >> (56 - 8*i));
    c->buf_len = 56;
    sha256_update(c, tail, 8);
    {
        static const char hexd[] = "0123456789abcdef";
        for (i = 0; i < 8; i++)
            for (j = 0; j < 4; j++) {
                uint32_t v = (c->h[i] >> (24 - 8*j)) & 0xff;
                out[i*8 + j*2] = hexd[v >> 4];
                out[i*8 + j*2 + 1] = hexd[v & 15];
            }
    }
    out[64] = 0;
}

/* ------------------------------------------------------------------ */
/* Host IO over stdio (the consumer's own source-side callbacks)      */
/* ------------------------------------------------------------------ */

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

/* ------------------------------------------------------------------ */
/* Helpers                                                            */
/* ------------------------------------------------------------------ */

static int failures = 0;

static void gate(const char *what, int ok) {
    printf("{\"check\":\"%s\",\"ok\":%u}\n", what, ok ? 1u : 0u);
    if (!ok) failures++;
}

/* Print every /proc/self/maps line that names a songcore shared object:
 * runtime proof of *which* library this process actually loaded. */
static void print_loaded_songcore(void) {
    FILE *m = fopen("/proc/self/maps", "r");
    char line[4096];
    int n = 0;
    if (!m) { printf("{\"loaded_songcore_paths\":\"proc_maps_unavailable\"}\n"); return; }
    printf("{\"loaded_songcore_paths\":[");
    while (fgets(line, sizeof line, m)) {
        if (strstr(line, "songcore")) {
            char *p = strchr(line, '/');
            size_t L;
            if (!p) continue;
            L = strlen(p);
            while (L && (p[L-1] == '\n' || p[L-1] == ' ')) p[--L] = 0;
            printf("%s\"%s\"", n ? "," : "", p);
            n++;
        }
    }
    printf("]}\n");
    fclose(m);
}

static void print_meta_view(const char *name, const char *s, uint32_t len, uint32_t has) {
    printf("{\"meta\":\"%s\",\"has\":%u,\"len\":%u,\"value\":", name, has, len);
    if (has && s) {
        uint32_t i;
        printf("\"");
        for (i = 0; i < len; i++) {
            unsigned char ch = (unsigned char)s[i];
            if (ch == '"' || ch == '\\') printf("\\%c", ch);
            else if (ch >= 0x20 && ch < 0x7f) putchar(ch);
            else printf("\\u%04x", ch);
        }
        printf("\"");
    } else {
        printf("null");
    }
    printf("}\n");
}

int main(int argc, char **argv) {
    const char *path;
    FILE *f;
    song_io io;
    song_handle *h = NULL;
    song_info info;
    song_status st;
    uint32_t abi;
    float *block = NULL;
    float *seq = NULL;
    uint64_t seq_frames = 0, seq_cap = 0;
    uint64_t frames = 0;
    char hex[65];
    sha256_ctx c;
    uint64_t blk = 4096;
    int lossless = 0;
    int pct;

    if (argc != 2) {
        fprintf(stderr, "usage: %s <media-file>\n", argv[0]);
        return 2;
    }
    path = argv[1];

    abi = songcore_abi_version();
    printf("{\"stage\":\"abi\",\"songcore_abi_version\":%u,\"ok\":%u}\n", abi, abi == SONGCORE_ABI_VERSION ? 1u : 0u);
    if (abi != SONGCORE_ABI_VERSION) { fprintf(stderr, "ABI mismatch\n"); return 1; }
    print_loaded_songcore();

    f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "cannot open %s\n", path); return 2; }
    io.userdata = f;
    io.read = host_read;
    io.seek = host_seek;
    io.size = host_size;

    st = song_open(&io, &h);
    printf("{\"stage\":\"open\",\"status\":%d,\"ok\":%u}\n", (int)st, st == SONG_OK ? 1u : 0u);
    if (st != SONG_OK) { fprintf(stderr, "song_open failed: %d\n", (int)st); fclose(f); return 1; }

    st = song_probe(h, &info);
    printf("{\"stage\":\"probe\",\"status\":%d,\"codec\":\"%s\",\"container\":\"%s\","
           "\"sample_rate\":%d,\"channels\":%d,\"channel_mask\":\"0x%016" PRIx64 "\","
           "\"duration_us\":%" PRId64 ",\"bits_per_sample\":%d,"
           "\"audio_stream_count\":%u,\"selected_audio_index\":%u,\"ok\":%u}\n",
           (int)st, info.codec, info.container, info.sample_rate, info.channels,
           info.channel_mask, info.duration_us, info.bits_per_sample,
           info.audio_stream_count, info.selected_audio_index, st == SONG_OK ? 1u : 0u);
    gate("probe", st == SONG_OK);
    if (st != SONG_OK) { song_close(h); fclose(f); return 1; }

    lossless = (strcmp(info.codec, "flac") == 0 || strcmp(info.codec, "alac") == 0 ||
                strncmp(info.codec, "pcm_", 4) == 0);
    printf("{\"stage\":\"lossless_class\",\"lossless\":%d,\"codec\":\"%s\"}\n", lossless, info.codec);

    /* ---- audio streams ---- */
    {
        uint32_t nstreams = 0;
        st = song_audio_stream_count(h, &nstreams);
        printf("{\"stage\":\"audio_stream_count\",\"status\":%d,\"count\":%u}\n", (int)st, nstreams);
        gate("audio_stream_count", st == SONG_OK && nstreams == info.audio_stream_count);
        for (uint32_t i = 0; i < nstreams; i++) {
            song_stream_info si;
            st = song_audio_stream_info(h, i, &si);
            printf("{\"stage\":\"audio_stream_info\",\"status\":%d,\"audio_index\":%u,"
                   "\"stream_index\":%u,\"rate\":%d,\"channels\":%d,\"default\":%u,\"codec\":\"%s\"}\n",
                   (int)st, si.audio_index, si.stream_index, si.sample_rate, si.channels,
                   si.is_default, si.codec);
        }
    }

    /* ---- metadata snapshot ---- */
    {
        const song_metadata *meta = NULL;
        st = song_get_metadata(h, &meta);
        printf("{\"stage\":\"get_metadata\",\"status\":%d,\"ok\":%u}\n", (int)st,
               (st == SONG_OK && meta != NULL) ? 1u : 0u);
        gate("get_metadata", st == SONG_OK && meta != NULL);
        if (st == SONG_OK && meta) {
            print_meta_view("title", meta->title, meta->title_len, meta->has_title);
            print_meta_view("artist", meta->artist, meta->artist_len, meta->has_artist);
            print_meta_view("album", meta->album, meta->album_len, meta->has_album);
            print_meta_view("album_artist", meta->album_artist, meta->album_artist_len, meta->has_album_artist);
            print_meta_view("genre", meta->genre, meta->genre_len, meta->has_genre);
            print_meta_view("composer", meta->composer, meta->composer_len, meta->has_composer);
            print_meta_view("date", meta->date, meta->date_len, meta->has_date);
            print_meta_view("comment", meta->comment, meta->comment_len, meta->has_comment);
            printf("{\"stage\":\"metadata_numbers\",\"track_number\":%d,\"has_track_number\":%u,"
                   "\"track_total\":%d,\"has_track_total\":%u,\"disc_number\":%d,\"has_disc_number\":%u,"
                   "\"disc_total\":%d,\"has_disc_total\":%u,"
                   "\"track_gain_mb\":%d,\"has_track_gain\":%u,\"track_peak\":%u,\"has_track_peak\":%u,"
                   "\"album_gain_mb\":%d,\"has_album_gain\":%u,\"album_peak\":%u,\"has_album_peak\":%u}\n",
                   meta->track_number, meta->has_track_number, meta->track_total, meta->has_track_total,
                   meta->disc_number, meta->has_disc_number, meta->disc_total, meta->has_disc_total,
                   meta->track_gain_mb, meta->has_track_gain, meta->track_peak, meta->has_track_peak,
                   meta->album_gain_mb, meta->has_album_gain, meta->album_peak, meta->has_album_peak);
        }
        {
            uint32_t mcount = 0;
            st = song_get_metadata_count(h, &mcount);
            printf("{\"stage\":\"metadata_count\",\"status\":%d,\"count\":%u}\n", (int)st, mcount);
            gate("metadata_count", st == SONG_OK);
            for (uint32_t i = 0; i < mcount; i++) {
                song_metadata_entry e;
                st = song_get_metadata_entry(h, i, &e);
                printf("{\"stage\":\"metadata_entry\",\"status\":%d,\"index\":%u,\"scope\":%u,"
                       "\"key\":\"%.*s\",\"value\":\"%.*s\"}\n",
                       (int)st, i, e.scope,
                       (int)e.key_len, e.key ? e.key : "",
                       (int)e.value_len, e.value ? e.value : "");
            }
        }
        {
            uint32_t acount = 0;
            st = song_get_artwork_count(h, &acount);
            printf("{\"stage\":\"artwork_count\",\"status\":%d,\"count\":%u}\n", (int)st, acount);
            for (uint32_t i = 0; i < acount; i++) {
                song_artwork_item it;
                st = song_get_artwork_item(h, i, &it);
                printf("{\"stage\":\"artwork_item\",\"status\":%d,\"index\":%u,\"role\":%u,"
                       "\"mime\":\"%.*s\",\"data_len\":%" PRIu64 ",\"w\":%d,\"h\":%d}\n",
                       (int)st, i, it.role, (int)it.mime_len, it.mime ? it.mime : "",
                       it.data_len, it.width, it.height);
            }
        }
    }

    /* ---- full sequential decode -> PCM sha256 (harness protocol) ---- */
    block = (float *)malloc(sizeof(float) * (size_t)blk * (size_t)info.channels);
    if (!block) return 1;
    sha256_init(&c);
    for (;;) {
        uint64_t got = 0;
        st = song_read_pcm(h, block, blk, &got);
        if (st == SONG_EOF) {
            gate("full_decode_eof_produced_zero", got == 0);
            break;
        }
        if (st != SONG_OK) {
            printf("{\"stage\":\"full_decode\",\"status\":%d,\"frames_before\":%" PRIu64 "}\n", (int)st, frames);
            failures++;
            break;
        }
        if (got == 0) { gate("full_decode_zero_frames_with_ok", 0); break; }
        sha256_update(&c, block, sizeof(float) * (size_t)got * (size_t)info.channels);
        if (seq_frames + got > seq_cap) {
            uint64_t ncap = (seq_cap ? seq_cap * 2 : (uint64_t)blk * 16);
            while (ncap < seq_frames + got) ncap *= 2;
            float *ns = (float *)realloc(seq, sizeof(float) * (size_t)ncap * (size_t)info.channels);
            if (!ns) { free(block); return 1; }
            seq = ns; seq_cap = ncap;
        }
        memcpy(seq + seq_frames * (uint64_t)info.channels, block,
               sizeof(float) * (size_t)got * (size_t)info.channels);
        seq_frames += got;
        frames += got;
    }
    sha256_hex(&c, hex);
    printf("{\"stage\":\"full_decode\",\"status\":\"done\",\"frames\":%" PRIu64
           ",\"pcm_sha256\":\"%s\",\"terminal\":\"SONG_EOF\"}\n", frames, hex);

    /* after clean EOF, a further read must stay SONG_EOF with 0 frames and
     * must be distinguishable from any error code (errors are >= 100). */
    {
        uint64_t got = 12345;
        st = song_read_pcm(h, block, blk, &got);
        printf("{\"stage\":\"eof_repeat\",\"status\":%d,\"produced\":%" PRIu64
               ",\"is_error\":%u,\"ok\":%u}\n",
               (int)st, got, (st >= 100) ? 1u : 0u,
               (st == SONG_EOF && got == 0) ? 1u : 0u);
        gate("eof_repeat_song_eof_zero", st == SONG_EOF && got == 0);
        gate("eof_not_an_error_code", (st >= 100) ? 0 : 1);
    }

    /* ---- seek 25 / 50 / 75 % ---- */
    if (info.duration_us > 0) {
        static const int pcts[3] = { 25, 50, 75 };
        const song_metadata *meta0 = NULL;
        uint32_t meta_n0 = 0, art_n0 = 0;
        song_get_metadata(h, &meta0);
        song_get_metadata_count(h, &meta_n0);
        song_get_artwork_count(h, &art_n0);
        for (int k = 0; k < 3; k++) {
            int64_t target, landing = -2;
            uint64_t got = 0;
            int status_class;
            pct = pcts[k];
            target = info.duration_us / 100 * pct;
            st = song_seek(h, target, &landing);
            status_class = (st == SONG_OK) ? 0 : (st == SONG_EOF ? 1 : 2);
            printf("{\"stage\":\"seek\",\"pct\":%d,\"target_us\":%" PRId64 ",\"status\":%d,"
                   "\"status_class\":%d,\"landing_us\":%" PRId64 ",\"landing_not_after_target\":%u}\n",
                   pct, target, (int)st, status_class, landing,
                   (landing >= 0 && landing <= target) ? 1u : 0u);
            gate("seek_status_ok", st == SONG_OK);
            gate("seek_landing_known", landing >= 0);
            gate("seek_landing_at_or_before_target", landing >= 0 && landing <= target);
            if (st == SONG_OK) {
                st = song_read_pcm(h, block, blk, &got);
                printf("{\"stage\":\"seek_next_read\",\"pct\":%d,\"status\":%d,\"produced\":%" PRIu64
                       ",\"at_landing\":%u}\n", pct, (int)st, got,
                       (st == SONG_OK && got > 0) ? 1u : 0u);
                gate("seek_next_read_frames", st == SONG_OK && got > 0);
                /* S-BIND 7.1 step 10: metadata / artwork / format unchanged */
                {
                    song_info i2;
                    const song_metadata *m2 = NULL;
                    uint32_t mn2 = 0, an2 = 0;
                    song_status ps = song_probe(h, &i2);
                    song_get_metadata(h, &m2);
                    song_get_metadata_count(h, &mn2);
                    song_get_artwork_count(h, &an2);
                    int same_fmt = ps == SONG_OK &&
                                   i2.sample_rate == info.sample_rate &&
                                   i2.channels == info.channels &&
                                   i2.channel_mask == info.channel_mask &&
                                   strcmp(i2.codec, info.codec) == 0 &&
                                   strcmp(i2.container, info.container) == 0 &&
                                   i2.audio_stream_count == info.audio_stream_count &&
                                   i2.selected_audio_index == info.selected_audio_index;
                    int same_meta = m2 != NULL && meta0 != NULL &&
                                    m2->has_title == meta0->has_title &&
                                    m2->title_len == meta0->title_len &&
                                    (m2->has_title == 0 ||
                                     memcmp(m2->title, meta0->title, m2->title_len) == 0) &&
                                    mn2 == meta_n0;
                    printf("{\"stage\":\"seek_views_unchanged\",\"pct\":%d,\"format_unchanged\":%u,"
                           "\"metadata_unchanged\":%u,\"artwork_count\":%u,\"artwork_unchanged\":%u}\n",
                           pct, same_fmt ? 1u : 0u, same_meta ? 1u : 0u, an2,
                           (an2 == art_n0) ? 1u : 0u);
                    gate("seek_format_unchanged", same_fmt);
                    gate("seek_metadata_unchanged", same_meta);
                    gate("seek_artwork_unchanged", an2 == art_n0);
                }
                if (lossless && seq && got > 0 && landing >= 0) {
                    uint64_t idx = (uint64_t)((landing * (int64_t)info.sample_rate + 500000) / 1000000);
                    uint64_t n = got, i, mism = 0;
                    int64_t best_delta = INT64_MIN;
                    uint64_t best_mism = UINT64_MAX;
                    if (idx + n > seq_frames) n = (seq_frames > idx) ? (seq_frames - idx) : 0;
                    for (i = 0; i < n; i++)
                        if (memcmp(block + i * (uint64_t)info.channels,
                                   seq + (idx + i) * (uint64_t)info.channels,
                                   sizeof(float) * (size_t)info.channels) != 0) mism++;
                    if (mism) {
                        /* locate the true landing offset around the reported one */
                        int64_t d;
                        for (d = -8192; d <= 8192; d++) {
                            int64_t sidx = (int64_t)idx + d;
                            uint64_t m2 = 0, j;
                            if (sidx < 0 || (uint64_t)sidx + 8 > seq_frames) continue;
                            for (j = 0; j < 8; j++)
                                if (memcmp(block + j * (uint64_t)info.channels,
                                           seq + ((uint64_t)sidx + j) * (uint64_t)info.channels,
                                           sizeof(float) * (size_t)info.channels) != 0) { m2 = 1; break; }
                            if (m2 == 0) { best_delta = d; best_mism = 0; break; }
                        }
                    }
                    printf("{\"stage\":\"seek_lossless_landing_check\",\"pct\":%d,"
                           "\"reported_frame_idx\":%" PRIu64 ",\"compared_frames\":%" PRIu64
                           ",\"mismatched_frames\":%" PRIu64 ",\"exact_match\":%u,"
                           "\"true_landing_delta_frames\":%" PRId64 "}\n",
                           pct, idx, n, mism, mism == 0 ? 1u : 0u, best_delta);
                    gate("seek_landing_frame_exact_lossless", mism == 0);
                }
            }
        }
    } else {
        printf("{\"stage\":\"seek\",\"skipped\":\"duration_unknown\"}\n");
    }

    /* ---- drain to EOF ----
     * Preferred: a near-end seek (duration - 0.2 s). If that seek is refused
     * (non-OK status), fall back to the 50 % landing so that the drain/EOF
     * terminal can still be observed. The near-end seek result itself is
     * reported as a diagnostic and is NOT a gate. */
    if (info.duration_us > 0) {
        int64_t targets[2];
        int64_t landing = -2;
        uint64_t total = 0;
        int used = -1;
        targets[0] = info.duration_us - 200000; /* last 0.2 s */
        targets[1] = info.duration_us / 2;
        if (targets[0] < 0) targets[0] = 0;
        for (int t = 0; t < 2; t++) {
            landing = -2;
            st = song_seek(h, targets[t], &landing);
            printf("{\"stage\":\"drain_seek\",\"attempt\":%d,\"target_us\":%" PRId64
                   ",\"status\":%d,\"landing_us\":%" PRId64 ",\"gated\":%u}\n",
                   t, targets[t], (int)st, landing, t == 0 ? 0u : 0u);
            if (st == SONG_OK && landing >= 0) { used = t; break; }
        }
        if (used >= 0) {
            for (;;) {
                uint64_t got = 0;
                st = song_read_pcm(h, block, blk, &got);
                if (st == SONG_EOF) {
                    printf("{\"stage\":\"drain\",\"terminal\":\"SONG_EOF\",\"status\":%d,"
                           "\"produced_at_eof\":%" PRIu64 ",\"frames_after_seek\":%" PRIu64
                           ",\"is_error\":0,\"terminal_distinguishable_from_error\":1}\n",
                           (int)st, got, total);
                    gate("drain_eof_zero", got == 0);
                    gate("drain_terminal_is_song_eof", st == SONG_EOF);
                    break;
                }
                if (st != SONG_OK) {
                    printf("{\"stage\":\"drain\",\"terminal\":\"ERROR\",\"status\":%d,"
                           "\"frames_after_seek\":%" PRIu64 "}\n", (int)st, total);
                    gate("drain_terminal_is_song_eof", 0);
                    break;
                }
                if (got == 0) { gate("drain_zero_frames_with_ok", 0); break; }
                total += got;
            }
        } else {
            gate("drain_seek_ok", 0);
        }
    }

    /* diagnostic: non-fatal observations */
    {
        uint64_t got = 999;
        song_status s0 = song_read_pcm(h, block, 0, &got);
        printf("{\"stage\":\"read_pcm_zero_capacity\",\"status\":%d,\"note\":\"header line 369 expects SONG_ERR_INVALID_ARGUMENT\"}\n", (int)s0);
    }

    free(block);
    free(seq);
    song_close(h);
    fclose(f);
    printf("{\"stage\":\"close\",\"ok\":1}\n");

    /* ---- S-BIND 7.1 step 13: drop mid-stream, then reopen and re-derive ---- */
    {
        FILE *f2;
        song_io io2;
        song_handle *h2 = NULL;
        song_info i2;
        float *b2;
        sha256_ctx c2;
        uint64_t fr2 = 0, k;
        char hex2[65];
        int mid_ok = 0;

        f2 = fopen(path, "rb");
        if (!f2) { gate("reopen_after_midstream_close_stable", 0); goto done; }
        io2.userdata = f2; io2.read = host_read; io2.seek = host_seek; io2.size = host_size;
        if (song_open(&io2, &h2) != SONG_OK || song_probe(h2, &i2) != SONG_OK) {
            if (h2) song_close(h2);
            fclose(f2);
            gate("reopen_after_midstream_close_stable", 0);
            goto done;
        }
        b2 = (float *)malloc(sizeof(float) * (size_t)blk * (size_t)i2.channels);
        if (!b2) { song_close(h2); fclose(f2); return 1; }
        for (k = 0; k < 3; k++) {          /* drop mid-stream: 3 blocks, no drain */
            uint64_t g = 0;
            song_status s = song_read_pcm(h2, b2, blk, &g);
            if (s != SONG_OK || g == 0) break;
        }
        song_close(h2);                    /* no views touched after close */
        fclose(f2);

        f2 = fopen(path, "rb");
        if (!f2) { free(b2); gate("reopen_after_midstream_close_stable", 0); goto done; }
        io2.userdata = f2; io2.read = host_read; io2.seek = host_seek; io2.size = host_size;
        h2 = NULL;
        if (song_open(&io2, &h2) != SONG_OK || song_probe(h2, &i2) != SONG_OK) {
            if (h2) song_close(h2);
            fclose(f2); free(b2);
            gate("reopen_after_midstream_close_stable", 0);
            goto done;
        }
        sha256_init(&c2);
        for (;;) {
            uint64_t g = 0;
            song_status s = song_read_pcm(h2, b2, blk, &g);
            if (s == SONG_EOF) { mid_ok = (g == 0); break; }
            if (s != SONG_OK) { mid_ok = 0; break; }
            if (g == 0) { mid_ok = 0; break; }
            sha256_update(&c2, b2, sizeof(float) * (size_t)g * (size_t)i2.channels);
            fr2 += g;
        }
        song_close(h2);
        fclose(f2);
        free(b2);
        sha256_hex(&c2, hex2);
        printf("{\"stage\":\"reopen_after_midstream_close\",\"eof_clean\":%u,\"frames\":%" PRIu64
               ",\"frames_equal_pass1\":%u,\"pcm_sha256\":\"%s\",\"pcm_equal_pass1\":%u}\n",
               mid_ok ? 1u : 0u, fr2, (fr2 == frames) ? 1u : 0u, hex2,
               (strcmp(hex2, hex) == 0) ? 1u : 0u);
        gate("reopen_after_midstream_close_stable", mid_ok && fr2 == frames && strcmp(hex2, hex) == 0);
    }
done:
    printf("{\"stage\":\"verdict\",\"ok\":%u,\"gated_failures\":%d}\n", failures == 0 ? 1u : 0u, failures);
    return failures == 0 ? 0 : 1;
}
