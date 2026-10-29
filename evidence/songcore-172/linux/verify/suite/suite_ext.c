/*
 * suite_ext.c — S3 Linux lane extension suite for the SongCore C consumer
 * scenario (#172 S3-verify). Covers the S-BIND §7.1 steps 04-13 and the
 * §8 status/sentinel/lifetime contracts that the committed canonical gates
 * (native/experiments/songcore-equivalence/run.py + harness.c "correct"
 * mode, experiments/songcore-call-comparison surface/layout gates) do not
 * already execute:
 *   - seek at 25/50/75% of duration (§7.1 steps 08-10) with the §8.9 seek
 *     contract: status class, landing at/before target or explicit -1,
 *     next-read-belongs-to-landing, lossless byte-accurate continuation
 *     against the sequential-decode PCM captured by the scenario mode,
 *     post-seek metadata/artwork/info invariance (§8.9/§8.11);
 *   - metadata canonical presence rules + raw enumeration (§7.1 step 05,
 *     §8.4/§8.5/§8.6 presence and units rules);
 *   - artwork enumeration invariants (§7.1 step 06);
 *   - EOF stability (§8.10: EOF repeated, produced == 0, distinct from error);
 *   - read contract negatives: frame_capacity == 0 -> SONG_ERR_INVALID_ARGUMENT
 *     with produced == 0 (§8.8, header-documented);
 *   - stream enumeration + default-stream identity + invalid-index negatives
 *     (§7.1 step 04, header-documented SONG_ERR_INVALID_ARGUMENT);
 *   - malformed/truncated/corrupted sources: typed fail-closed statuses and
 *     the no-fabrication prefix property (§7.1 step 12; truncation technique,
 *     corruption-window technique from the committed Rust fault harness
 *     crates/qianqian-decode-songcore/src/e1_abi_tests.rs "disturb", applied
 *     to the same committed corpus — no second corpus, no invented fixtures);
 *   - lifetime: mid-stream close -> reopen -> byte-identical decode;
 *     song_close(NULL) is a documented no-op (§7.1 step 13, §8.6).
 *
 * Every check emits one JSON line: {"check":ID,"pass":0|1,...observed fields}.
 * The process exits 0 only when every emitted check passed; the driver judges
 * frozen-field equality against reference.json. Open-failure status codes are
 * RECORDED, not over-specified: song_open documents
 * {UNSUPPORTED_CONTAINER, IO, OUT_OF_MEMORY} (songcore.h:216-219) and §8.2
 * adds INVALID_ARGUMENT for null/illegal arguments — the asserted invariant is
 * "typed status, no handle published", with the observed code in the receipt.
 */
#define _POSIX_C_SOURCE 200809L

#include <inttypes.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "songcore.h"

static int g_checks = 0, g_fails = 0;

static void json_escape(const char *s, uint32_t n) {
    for (uint32_t i = 0; i < n && s[i]; i++) {
        unsigned char c = (unsigned char)s[i];
        if (c == '"' || c == '\\') printf("\\%c", c);
        else if (c < 0x20) printf("\\u%04x", c);
        else putchar(c);
    }
}

static void emit(const char *check, int pass, const char *tail_fmt, ...) {
    g_checks++;
    if (!pass) g_fails++;
    printf("{\"check\":\"%s\",\"pass\":%d", check, pass ? 1 : 0);
    if (tail_fmt && *tail_fmt) {
        printf(",");
        va_list ap;
        va_start(ap, tail_fmt);
        vprintf(tail_fmt, ap);
        va_end(ap);
    }
    printf("}\n");
    fflush(stdout);
}

/* ---- SHA-256 (constants verbatim from the canonical
 * native/experiments/songcore-equivalence/harness.c:19-97) ---- */
typedef struct { uint32_t h[8]; uint64_t len; uint8_t buf[64]; size_t buf_len; } sha256_ctx;
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
        uint32_t s0 = ror(w[i-15],7) ^ ror(w[i-15],18) ^ (w[i-15] >> 3);
        uint32_t s1 = ror(w[i-2],17) ^ ror(w[i-2],19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + w[i-7] + s0 + s1;
    }
    uint32_t a=c->h[0],b=c->h[1],d=c->h[2],e=c->h[3];
    uint32_t f=c->h[4],g=c->h[5],hh=c->h[6],i=c->h[7];
    for (int t = 0; t < 64; t++) {
        uint32_t S1 = ror(f,6)^ror(f,11)^ror(f,25), ch = (f&g)^(~f&hh);
        uint32_t t1 = i + S1 + ch + K256[t] + w[t];
        uint32_t S0 = ror(a,2)^ror(a,13)^ror(a,22), mj = (a&b)^(a&d)^(b&d);
        uint32_t t2 = S0 + mj;
        i = hh; hh = g; g = f; f = e + t1; e = d; d = b; b = a; a = t1 + t2;
    }
    c->h[0]+=a; c->h[1]+=b; c->h[2]+=d; c->h[3]+=e;
    c->h[4]+=f; c->h[5]+=g; c->h[6]+=hh; c->h[7]+=i;
}
static void sha256_init(sha256_ctx *c) {
    static const uint32_t iv[8] = {0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
                                   0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    for (int i = 0; i < 8; i++) c->h[i] = iv[i];
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
    uint8_t pad = 0x80, zero = 0;
    sha256_update(c, &pad, 1);
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

/* ---- host I/O (same callbacks as the canonical harness.c:99-119) ---- */
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

#define BLOCK 4096

/* read the whole stream. Returns the terminal status (SONG_EOF expected).
 * Sets *violation=1 when a read violates the §8.8 shape (EOF with produced>0,
 * OK with produced==0 or produced>capacity). When full_pcm is given, any
 * output that is not the exact sequential-decode prefix aborts with
 * SONG_ERR_INTERNAL_ERROR as a sentinel (no-fabrication property, §7.1/12). */
static song_status drain(song_handle *h, uint32_t channels, FILE *capture,
                         const uint8_t *full_pcm, size_t full_pcm_bytes,
                         uint64_t *out_frames, char out_sha[65],
                         size_t *out_pcm_bytes, int *violation) {
    float *dst = malloc(sizeof(float) * BLOCK * channels);
    sha256_ctx c; sha256_init(&c);
    uint64_t frames = 0;
    size_t bytes = 0;
    song_status st = SONG_OK;
    *out_frames = 0; out_sha[0] = 0; if (out_pcm_bytes) *out_pcm_bytes = 0;
    if (violation) *violation = 0;
    for (;;) {
        uint64_t got = 0;
        st = song_read_pcm(h, dst, BLOCK, &got);
        if (st == SONG_EOF) {
            if (got != 0 && violation) *violation = 1;
            break;
        }
        if (st != SONG_OK) break;
        if (got == 0 || got > BLOCK) {
            if (violation) *violation = 1;
            break;
        }
        size_t nb = sizeof(float) * (size_t)got * channels;
        if (full_pcm &&
            (bytes + nb > full_pcm_bytes ||
             memcmp((const uint8_t *)dst, full_pcm + bytes, nb) != 0)) {
            free(dst);
            return SONG_ERR_INTERNAL_ERROR; /* sentinel: prefix violated */
        }
        sha256_update(&c, dst, nb);
        bytes += nb;
        frames += got;
        if (capture) fwrite(dst, 1, nb, capture);
    }
    if (st == SONG_EOF && out_sha) sha256_hex(&c, out_sha);
    if (out_frames) *out_frames = frames;
    if (out_pcm_bytes) *out_pcm_bytes = bytes;
    free(dst);
    return st;
}

/* ---------------- scenario mode: §7.1 steps 01-11 per fixture ---------------- */
static int run_scenario(const char *path, const char *scratch) {
    char pcm_path[1024];
    snprintf(pcm_path, sizeof pcm_path, "%s.pcm", scratch);

    /* 01 */
    emit("abi_version", songcore_abi_version() == 1, "\"value\":%u", songcore_abi_version());

    /* 02/03 */
    song_io io; song_handle *h = NULL;
    if (open_song(path, &io, &h) != 0) {
        emit("open", 0, "\"error\":\"open_failed\""); return 1;
    }
    emit("open", 1, NULL);
    song_info info;
    song_status st = song_probe(h, &info);
    int probe_ok = (st == SONG_OK);
    emit("probe", probe_ok, "\"status\":%d,\"rate\":%d,\"channels\":%d,\"mask\":%" PRIu64
         ",\"duration_us\":%" PRId64 ",\"bps\":%d,\"codec\":\"%s\",\"container\":\"%s\""
         ",\"selected\":%u,\"stream_count\":%u,\"flags\":%u",
         st, info.sample_rate, info.channels, info.channel_mask, info.duration_us,
         info.bits_per_sample, info.codec, info.container,
         info.selected_audio_index, info.audio_stream_count, info.flags);
    if (!probe_ok) { close_with_file(h, &io); return 1; }
    song_info info2;
    int idem = (song_probe(h, &info2) == SONG_OK &&
                memcmp(&info, &info2, sizeof info) == 0);
    emit("probe_idempotent", idem, NULL);

    /* 04 stream enumeration */
    uint32_t nstreams = 0;
    int streams_ok = (song_audio_stream_count(h, &nstreams) == SONG_OK && nstreams >= 1);
    emit("stream_count", streams_ok, "\"count\":%u", nstreams);
    if (streams_ok) {
        for (uint32_t i = 0; i < nstreams; i++) {
            song_stream_info si;
            song_status sst = song_audio_stream_info(h, i, &si);
            int ok = (sst == SONG_OK && si.audio_index == i);
            emit("stream_info", ok, "\"audio_index\":%u,\"stream_index\":%u,\"status\":%d",
                 si.audio_index, si.stream_index, sst);
            if (!ok) { streams_ok = 0; break; }
            if (i == info.selected_audio_index) {
                emit("selected_stream_identity",
                     si.sample_rate == info.sample_rate && si.channels == info.channels,
                     "\"rate\":%d,\"channels\":%d,\"is_default\":%u",
                     si.sample_rate, si.channels, si.is_default);
            }
        }
    }
    song_stream_info dummy;
    emit("stream_info_invalid_index",
         song_audio_stream_info(h, nstreams, &dummy) == SONG_ERR_INVALID_ARGUMENT, NULL);
    emit("select_invalid_index",
         song_select_stream(h, nstreams) == SONG_ERR_INVALID_ARGUMENT, NULL);

    /* 05 metadata */
    const song_metadata *meta = NULL;
    int meta_ok = (song_get_metadata(h, &meta) == SONG_OK && meta != NULL);
    emit("metadata_snapshot", meta_ok, NULL);
    if (meta_ok) {
        int present_ok = 1;
        const char *vals[8] = {meta->title, meta->artist, meta->album,
                               meta->album_artist, meta->genre, meta->composer,
                               meta->date, meta->comment};
        uint32_t lens[8] = {meta->title_len, meta->artist_len, meta->album_len,
                            meta->album_artist_len, meta->genre_len,
                            meta->composer_len, meta->date_len, meta->comment_len};
        uint32_t hass[8] = {meta->has_title, meta->has_artist, meta->has_album,
                            meta->has_album_artist, meta->has_genre,
                            meta->has_composer, meta->has_date, meta->has_comment};
        for (int i = 0; i < 8; i++)
            if (hass[i] && (vals[i] == NULL || lens[i] == 0)) present_ok = 0;
        emit("metadata_presence_consistency", present_ok, NULL);
        printf("{\"check\":\"metadata_canonical\",\"pass\":1,\"title\":\"");
        if (meta->has_title) json_escape(meta->title, meta->title_len);
        printf("\",\"artist\":\"");
        if (meta->has_artist) json_escape(meta->artist, meta->artist_len);
        printf("\",\"album\":\"");
        if (meta->has_album) json_escape(meta->album, meta->album_len);
        printf("\",\"track\":%" PRId32 ",\"has_track\":%u,\"disc\":%" PRId32
               ",\"has_disc\":%u,\"has_track_gain\":%u,\"has_album_gain\":%u}\n",
               meta->track_number, meta->has_track_number, meta->disc_number,
               meta->has_disc_number, meta->has_track_gain, meta->has_album_gain);
        g_checks++;
        uint32_t mcount = 0;
        int raw_ok = (song_get_metadata_count(h, &mcount) == SONG_OK);
        emit("metadata_raw_count", raw_ok, "\"count\":%u", mcount);
        for (uint32_t i = 0; i < mcount; i++) {
            song_metadata_entry e;
            if (song_get_metadata_entry(h, i, &e) != SONG_OK ||
                (e.scope != SONG_METADATA_SCOPE_CONTAINER &&
                 e.scope != SONG_METADATA_SCOPE_STREAM) || e.key == NULL) {
                emit("metadata_raw_entry", 0, "\"index\":%u", i);
                continue;
            }
            printf("{\"check\":\"metadata_raw_entry\",\"pass\":1,\"index\":%u,\"scope\":%u,\"key\":\"",
                   i, e.scope);
            json_escape(e.key, e.key_len);
            printf("\",\"value\":\"");
            if (e.value) json_escape(e.value, e.value_len);
            printf("\"}\n");
            g_checks++;
        }
    }

    /* 06 artwork */
    uint32_t acount = 0;
    int art_ok = (song_get_artwork_count(h, &acount) == SONG_OK);
    emit("artwork_count", art_ok, "\"count\":%u", acount);
    for (uint32_t i = 0; art_ok && i < acount; i++) {
        song_artwork_item ai;
        if (song_get_artwork_item(h, i, &ai) != SONG_OK ||
            ai.role > SONG_ARTWORK_OTHER ||
            (ai.data_len > 0 && ai.data == NULL) ||
            !(ai.width == -1 || ai.width >= 0) ||
            !(ai.height == -1 || ai.height >= 0)) {
            emit("artwork_item", 0, "\"index\":%u", i);
            continue;
        }
        char ahex[65] = "null";
        if (ai.data_len > 0) {
            sha256_ctx ac; sha256_init(&ac);
            sha256_update(&ac, ai.data, (size_t)ai.data_len);
            sha256_hex(&ac, ahex);
        }
        printf("{\"check\":\"artwork_item\",\"pass\":1,\"index\":%u,\"role\":%u,\"mime\":\"",
               i, ai.role);
        if (ai.mime) json_escape(ai.mime, ai.mime_len);
        printf("\",\"bytes\":%" PRIu64 ",\"w\":%" PRId32 ",\"h\":%" PRId32
               ",\"sha256\":\"%s\"}\n", ai.data_len, ai.width, ai.height, ahex);
        g_checks++;
    }

    /* capacity-0 negative (§8.8, header-documented: valid dst, capacity 0) */
    static float zerobuf[BLOCK];
    uint64_t zgot = 1;
    song_status zst = song_read_pcm(h, zerobuf, 0, &zgot);
    emit("read_capacity_zero",
         zst == SONG_ERR_INVALID_ARGUMENT && zgot == 0,
         "\"status\":%d,\"produced\":%" PRIu64, zst, zgot);
    /* observation (not a gate): with an ALSO-illegal NULL dst, SongCore
       returns INVALID_ARGUMENT before writing *out_frames_produced — the
       out value is left untouched. Header text says produced == 0 "for that
       call" on any non-OK/EOF status; recorded for authority review. */
    uint64_t ngot = 1;
    song_status nst = song_read_pcm(h, NULL, 0, &ngot);
    emit("observation_nulldst_produced_untouched", 1,
         "\"status\":%d,\"produced_was\":%" PRIu64
         ",\"note\":\"out not zeroed on null-dst invalid-arg path\"", nst, ngot);

    /* 07 full decode + capture + 11 EOF stability */
    FILE *cap = fopen(pcm_path, "wb");
    uint64_t frames = 0; char hex[65]; size_t pcm_bytes = 0; int violation = 0;
    st = drain(h, info.channels, cap, NULL, 0, &frames, hex, &pcm_bytes, &violation);
    if (cap) fclose(cap);
    emit("full_decode", st == SONG_EOF && !violation,
         "\"status\":%d,\"violation\":%d,\"frames\":%" PRIu64
         ",\"pcm_sha256\":\"%s\",\"pcm_bytes\":%zu",
         st, violation, frames, hex, pcm_bytes);
    uint64_t egot = 1;
    song_status est = song_read_pcm(h, zerobuf, BLOCK, &egot);
    emit("eof_stable", est == SONG_EOF && egot == 0,
         "\"status\":%d,\"produced\":%" PRIu64, est, egot);

    close_with_file(h, &io);

    /* 08-10 seek 25/50/75 (§7.1 steps 08-10, §8.9 contract) */
    for (int pct = 25; pct <= 75; pct += 25) {
        song_io sio; song_handle *sh = NULL;
        if (open_song(path, &sio, &sh) != 0) {
            emit("seek_open", 0, "\"pct\":%d", pct); continue;
        }
        song_info sinfo;
        if (song_probe(sh, &sinfo) != SONG_OK) {
            emit("seek_probe", 0, "\"pct\":%d", pct);
            close_with_file(sh, &sio); continue;
        }
        if (sinfo.duration_us <= 0) {
            emit("seek", 0, "\"pct\":%d,\"error\":\"no_duration\"", pct);
            close_with_file(sh, &sio); continue;
        }
        const song_metadata *smeta = NULL;
        int have_meta = (song_get_metadata(sh, &smeta) == SONG_OK && smeta != NULL);
        uint32_t sacount = 0;
        song_get_artwork_count(sh, &sacount);

        int64_t target = sinfo.duration_us * pct / 100;
        int64_t landing = -2;
        song_status sst = song_seek(sh, target, &landing);
        int land_ok = (sst == SONG_OK) &&
                      (landing == -1 || (landing >= 0 && landing <= target));
        emit("seek", land_ok,
             "\"pct\":%d,\"status\":%d,\"target_us\":%" PRId64 ",\"landing_us\":%" PRId64,
             pct, sst, target, landing);
        if (!land_ok) { close_with_file(sh, &sio); continue; }

        float *dst = malloc(sizeof(float) * BLOCK * sinfo.channels);
        uint64_t got = 0;
        song_status rst = song_read_pcm(sh, dst, BLOCK, &got);
        emit("seek_next_read", (rst == SONG_OK && got > 0),
             "\"pct\":%d,\"status\":%d,\"produced\":%" PRIu64, pct, rst, got);

        /* invariance after seek (§8.9/§8.11) */
        song_info pinfo;
        const song_metadata *pmeta = NULL;
        uint32_t pacount = 0;
        int inv = (song_probe(sh, &pinfo) == SONG_OK &&
                   memcmp(&sinfo, &pinfo, sizeof sinfo) == 0 &&
                   song_get_metadata(sh, &pmeta) == SONG_OK && pmeta != NULL &&
                   song_get_artwork_count(sh, &pacount) == SONG_OK &&
                   pacount == sacount);
        if (inv && have_meta) {
            inv = (smeta->has_title == pmeta->has_title) &&
                  (smeta->title_len == pmeta->title_len) &&
                  (smeta->has_title == 0 ||
                   memcmp(smeta->title, pmeta->title, pmeta->title_len) == 0);
        }
        emit("seek_invariance", inv, "\"pct\":%d", pct);

        if (strcmp(sinfo.codec, "mp3") != 0) {
            /* lossless: drain post-seek and compare byte-exact against the
               sequential-decode PCM at the inferred landing frame index */
            uint64_t post_frames = got;
            size_t post_bytes = sizeof(float) * (size_t)got * sinfo.channels;
            sha256_ctx pc; sha256_init(&pc);
            sha256_update(&pc, dst, sizeof(float) * (size_t)got * sinfo.channels);
            int prefix_bad = 0, clean_eof = 0;
            for (;;) {
                uint64_t g2 = 0;
                song_status st2 = song_read_pcm(sh, dst, BLOCK, &g2);
                if (st2 == SONG_EOF) { clean_eof = (g2 == 0); if (g2) prefix_bad = 1; break; }
                if (st2 != SONG_OK) { prefix_bad = 2; break; }
                if (g2 == 0 || g2 > BLOCK) { prefix_bad = 3; break; }
                size_t nb = sizeof(float) * (size_t)g2 * sinfo.channels;
                sha256_update(&pc, dst, nb);
                post_bytes += nb;
                post_frames += g2;
            }
            FILE *pf = fopen(pcm_path, "rb");
            int tail_ok = 0, land_frame_ok = 0, acct_ok = 0;
            uint64_t total_frames = 0, K = 0;
            if (pf) {
                fseek(pf, 0, SEEK_END);
                long sz = ftell(pf);
                total_frames = (uint64_t)sz / (sizeof(float) * sinfo.channels);
                K = total_frames - post_frames;
                size_t Kbytes = (size_t)K * sizeof(float) * sinfo.channels;
                if (sz >= 0 && Kbytes + post_bytes == (size_t)sz) {
                    uint8_t *full = malloc((size_t)sz);
                    fseek(pf, 0, SEEK_SET);
                    if (fread(full, 1, (size_t)sz, pf) == (size_t)sz) {
                        sha256_ctx fc; sha256_init(&fc);
                        sha256_update(&fc, full + Kbytes, post_bytes);
                        char fhex[65], phex[65];
                        sha256_hex(&fc, fhex);
                        sha256_hex(&pc, phex);
                        tail_ok = (strcmp(fhex, phex) == 0);
                        int64_t tol = (int64_t)2 * 1000000 / sinfo.sample_rate + 1;
                        int64_t k_us = (int64_t)((K * 1000000) / (uint64_t)sinfo.sample_rate);
                        land_frame_ok = (landing >= 0) &&
                                        (k_us - landing <= tol) &&
                                        (landing - k_us <= tol);
                    }
                    free(full);
                }
                fclose(pf);
            }
            acct_ok = (clean_eof && post_frames > 0 && post_frames <= total_frames);
            emit("seek_lossless_byte_accurate", tail_ok && !prefix_bad,
                 "\"pct\":%d,\"prefix_bad\":%d", pct, prefix_bad);
            emit("seek_landing_frame_index", land_frame_ok,
                 "\"pct\":%d,\"landing_us\":%" PRId64 ",\"inferred_frame\":%" PRIu64,
                 pct, landing, K);
            emit("seek_frame_accounting", acct_ok,
                 "\"pct\":%d,\"clean_eof\":%d,\"post_frames\":%" PRIu64
                 ",\"total_frames\":%" PRIu64, pct, clean_eof, post_frames, total_frames);
        } else {
            /* lossy: contract-class only (§8.9 bounded codec-frame tolerance);
               drain to EOF and record the tail sha for the parity receipt */
            char phex[65] = ""; uint64_t post_frames = got; size_t pb2 = 0; int vio2 = 0;
            song_status tst = drain(sh, sinfo.channels, NULL, NULL, 0,
                                    &post_frames, phex, &pb2, &vio2);
            emit("seek_lossy_contract_only",
                 tst == SONG_EOF && !vio2,
                 "\"pct\":%d,\"tail_sha256\":\"%s\",\"post_frames\":%" PRIu64,
                 pct, phex, post_frames);
        }
        free(dst);
        close_with_file(sh, &sio);
    }

    /* status class anchors (§8.2): EOF==1 normal terminal; ERR_* from 100 */
    emit("status_classes",
         SONG_OK == 0 && SONG_EOF == 1 && SONG_ERR_INVALID_ARGUMENT == 100 &&
         SONG_ERR_INTERNAL_ERROR == 113, NULL);
    return g_fails ? 1 : 0;
}

/* ---------------- trunc mode: §7.1 step 12 (typed fail-closed, no fabrication) ---- */
static int run_trunc(const char *variant, const char *full_pcm_path) {
    song_io io; song_handle *h = NULL;
    if (open_song(variant, &io, &h) != 0) {
        emit("trunc_outcome", 1,
             "\"stage\":\"open\",\"note\":\"typed open failure allowed\"");
        return g_fails ? 1 : 0;
    }
    song_info info;
    song_status pst = song_probe(h, &info);
    if (pst != SONG_OK) {
        int typed = (pst >= 100 && pst <= 113);
        emit("trunc_outcome", typed, "\"stage\":\"probe\",\"status\":%d", pst);
        close_with_file(h, &io);
        return g_fails ? 1 : 0;
    }
    FILE *pf = fopen(full_pcm_path, "rb");
    uint8_t *full = NULL; size_t full_bytes = 0;
    if (pf) {
        fseek(pf, 0, SEEK_END); long sz = ftell(pf);
        fseek(pf, 0, SEEK_SET);
        full = malloc((size_t)sz);
        if (fread(full, 1, (size_t)sz, pf) == (size_t)sz) full_bytes = (size_t)sz;
        fclose(pf);
    }
    float *dst = malloc(sizeof(float) * BLOCK * info.channels);
    uint64_t frames = 0; size_t bytes = 0;
    int prefix_ok = 1, terminal_ok = 1;
    song_status st = SONG_OK;
    for (;;) {
        uint64_t got = 0;
        st = song_read_pcm(h, dst, BLOCK, &got);
        if (st == SONG_EOF) { terminal_ok = (got == 0); break; }
        if (st != SONG_OK) {
            terminal_ok = (st >= 100 && st <= 113 && got == 0);
            break;
        }
        if (got == 0 || got > BLOCK) { terminal_ok = 0; break; }
        size_t nb = sizeof(float) * (size_t)got * info.channels;
        if (bytes + nb > full_bytes ||
            memcmp((const uint8_t *)dst, full + bytes, nb) != 0) {
            prefix_ok = 0;
            break;
        }
        bytes += nb; frames += got;
    }
    emit("trunc_prefix_property", prefix_ok,
         "\"frames\":%" PRIu64 ",\"bytes\":%zu", frames, bytes);
    emit("trunc_typed_terminal", terminal_ok, "\"status\":%d", st);
    free(dst); free(full);
    close_with_file(h, &io);
    return g_fails ? 1 : 0;
}

/* ---------------- lifetime mode: §7.1 step 13 ---------------- */
static int run_lifetime(const char *path) {
    song_io io; song_handle *h = NULL;
    if (open_song(path, &io, &h) != 0) {
        emit("lifetime_open", 0, NULL); return 1;
    }
    song_info info;
    if (song_probe(h, &info) != SONG_OK) {
        emit("lifetime_probe", 0, NULL); close_with_file(h, &io); return 1;
    }
    int64_t landing = -2;
    song_status sst = song_seek(h, info.duration_us / 4, &landing);
    if (sst != SONG_OK) {
        emit("lifetime_seek", 0, "\"status\":%d", sst);
        close_with_file(h, &io); return 1;
    }
    float *dst = malloc(sizeof(float) * BLOCK * info.channels);
    uint64_t got = 0;
    for (int i = 0; i < 3; i++) {
        if (song_read_pcm(h, dst, BLOCK, &got) != SONG_OK) break;
    }
    song_close(h);           /* mid-stream close */
    fclose((FILE *)io.userdata);
    emit("lifetime_midstream_close", 1, NULL);

    song_close(NULL);        /* documented no-op (songcore.h:407-410) */
    emit("close_null_noop", 1, NULL);

    song_io io2; song_handle *h2 = NULL;
    if (open_song(path, &io2, &h2) != 0) {
        emit("lifetime_reopen", 0, NULL); free(dst); return 1;
    }
    song_info info2;
    if (song_probe(h2, &info2) != SONG_OK ||
        memcmp(&info, &info2, sizeof info) != 0) {
        emit("lifetime_reopen", 0, NULL);
        close_with_file(h2, &io2); free(dst); return 1;
    }
    uint64_t frames = 0; char hex[65]; size_t pb = 0; int vio = 0;
    song_status st = drain(h2, info2.channels, NULL, NULL, 0, &frames, hex, &pb, &vio);
    emit("lifetime_reopen_decode", st == SONG_EOF && !vio,
         "\"status\":%d,\"frames\":%" PRIu64 ",\"pcm_sha256\":\"%s\"", st, frames, hex);
    free(dst);
    close_with_file(h2, &io2);
    return g_fails ? 1 : 0;
}

/* ---------------- misc mode: open-failure status classes ---------------- */
static int64_t fail_read(void *ud, uint8_t *dst, size_t size) {
    (void)ud; (void)dst; (void)size; return -1;
}
static int open_typed_ok(song_status st) {
    return st == SONG_ERR_INVALID_ARGUMENT || st == SONG_ERR_UNSUPPORTED_CONTAINER ||
           st == SONG_ERR_IO || st == SONG_ERR_OUT_OF_MEMORY;
}
static int run_misc(void) {
    song_handle *h = NULL;
    song_status st = song_open(NULL, &h);
    emit("open_null_io", open_typed_ok(st) && h == NULL, "\"status\":%d", st);
    song_io io = {NULL, NULL, host_seek, host_size};
    h = NULL;
    st = song_open(&io, &h);
    emit("open_null_read_cb", open_typed_ok(st) && h == NULL, "\"status\":%d", st);
    song_io io2 = {NULL, fail_read, host_seek, host_size};
    h = NULL;
    st = song_open(&io2, &h);
    emit("open_host_io_failure", open_typed_ok(st) && h == NULL, "\"status\":%d", st);
    return g_fails ? 1 : 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s scenario <file> <scratch>\n"
                        "       %s trunc <variant> <full_pcm>\n"
                        "       %s lifetime <file>\n"
                        "       %s misc\n", argv[0], argv[0], argv[0], argv[0]);
        return 2;
    }
    if (!strcmp(argv[1], "scenario") && argc == 4) return run_scenario(argv[2], argv[3]);
    if (!strcmp(argv[1], "trunc") && argc == 4) return run_trunc(argv[2], argv[3]);
    if (!strcmp(argv[1], "lifetime") && argc == 3) return run_lifetime(argv[2]);
    if (!strcmp(argv[1], "misc")) return run_misc();
    fprintf(stderr, "bad args\n");
    return 2;
}
