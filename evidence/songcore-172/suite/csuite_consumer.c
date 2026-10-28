/*
 * csuite_consumer.c — #172 G2 / CORE_RUNTIME Common Formats parity suite.
 *
 * A direct native (C) consumer implementing the S-BIND §7.1 binding-parity
 * scenario (docs/architecture/songcore-binding-architecture.md §7) against
 * ONE target-specific SongCore static artifact, on the target's real runtime.
 * This file plus ops/run-csuite.ps1 are the G2 executor for the
 * windows-x86_64 release lane; expected values are passed in by the runner
 * FROM the canonical frozen corpus (native/experiments/songcore-equivalence/
 * reference.json + results/songcore-equivalence.json) — no second corpus,
 * no second reference numbers, no locally invented tolerance.
 *
 * Modes:
 *   parity <file> <codec> <container> <duration_us> <rate> <channels>
 *          <frames> <pcm_sha256> <lossless>
 *       S-BIND §7.1 steps 01–11,13 on one fixture:
 *       abi version, open, probe source facts, audio-stream enumeration +
 *       default identity, metadata canonical + raw enumeration, artwork
 *       (structural; the corpus fixtures provide none), full-drain PCM vs
 *       the frozen frame count + PCM SHA-256, EOF/drain stability, seek
 *       25/50/75% (status class, landing-or-sentinel, next-read-lands — for
 *       lossless codecs the post-seek drain must byte-match the sequential
 *       reference slice at the landing frame, per the header's frozen
 *       "for lossless formats the landing is frame-accurate" contract),
 *       metadata/artwork/format unchanged, close + reopen stays stable.
 *       For lossy codecs no post-seek PCM identity is asserted: the corpus
 *       freezes no numeric tolerance and inventing one is forbidden; the
 *       observed drain is recorded instead.
 *   readfault <file>
 *       #296/#324 decode-read construction: FLAC fixture with byte 76000
 *       flipped (^= 0x01) inside a frame body. Frozen observation: the
 *       sequential read surfaces SONG_ERR_DECODE_ERROR (108) with native
 *       code -1094995529 (AVERROR_INVALIDDATA) after a clean prefix.
 *   seekfault <file>
 *       #296/#324 decode-seek construction: FLAC fixture with frame-body
 *       bits flipped at 73502/77042/80375 (^= 0x01), headers intact; seek
 *       2.0 s lands on the corrupt body. Frozen observation: typed seek
 *       failure SONG_ERR_SEEK_ERROR (110) with the "could not reach a
 *       landing point" diagnostic and -1 landing, then the post-seek read
 *       surfaces 108 / -1094995529 with zero frames. No fabricated landing,
 *       no fabricated PCM.
 *   truncated <file> <orig_frames>
 *       #296/#324 ffmpeg-bridge construction: FLAC fixture truncated to
 *       100000 bytes (frozen sha256 0aa66168…, verified by the runner).
 *       Contract oracle only: typed fail-closed terminal (EOF or a 100..113
 *       class), no SONG_OK-with-zero fabrication, stable terminal, no
 *       crash. The exact terminal class is RECORDED, not asserted — the
 *       corpus freezes no consumer-level expectation for this construction.
 *       <orig_frames> is the original fixture's frozen frame count and only
 *       bounds the drain accumulator.
 *   garbage <file>
 *       Non-media input: song_open must fail with a documented open-error
 *       class (IO 103 / UNSUPPORTED_CONTAINER 104 / OUT_OF_MEMORY 112) and
 *       leave *out_handle untouched.
 *   argcheck <fixture>
 *       Stable documented status classes on a real handle: pre-probe
 *       operations → NOT_OPEN (102); capacity-0 read, invalid stream index,
 *       negative seek, NULL handle/dst → INVALID_ARGUMENT (100); diagnostic
 *       contract (song_last_error: NULL message after OK/EOF, non-NULL
 *       after a typed failure).
 *   closecheck <file> <frames> <pcm_sha256>
 *       S-BIND step 13 lifetime arm: close mid-stream, reopen, re-drain →
 *       the frozen frame count + PCM SHA-256 again; song_close(NULL) is a
 *       no-op.
 *
 * Output contract: one `CASE <name> PASS|FAIL [detail]` line per sub-case,
 * then `MODE <mode> CASES=<n> FAILED=<m> RESULT=PASS|FAIL`. Exit 0 iff the
 * mode passed. The runner is a witness only; every predicate is asserted
 * here, inside the child (same discipline as the #324 cell runner).
 *
 * Link contract (evidence/songcore-172/link-contract.md §2, windows-x86_64
 * static): gcc ... csuite_consumer.c <libsongcore.a> -lbcrypt — nothing else.
 */

#include <inttypes.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "songcore.h"

/* ------------------------------------------------------------------ */
/* Minimal SHA-256 (FIPS 180-4) — same implementation lineage as the   */
/* equivalence harness and the decode-songcore test common module.     */
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

static uint32_t ror32(uint32_t x, int n) { return (x >> n) | (x << (32 - n)); }

static void sha256_compress(sha256_ctx *c, const uint8_t *p) {
    uint32_t w[64];
    for (int i = 0; i < 16; i++)
        w[i] = ((uint32_t)p[4*i] << 24) | ((uint32_t)p[4*i+1] << 16) |
               ((uint32_t)p[4*i+2] << 8) | (uint32_t)p[4*i+3];
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = ror32(w[i-15], 7) ^ ror32(w[i-15], 18) ^ (w[i-15] >> 3);
        uint32_t s1 = ror32(w[i-2], 17) ^ ror32(w[i-2], 19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    uint32_t a=c->h[0],b=c->h[1],d=c->h[2],e=c->h[3];
    uint32_t f=c->h[4],g=c->h[5],hh=c->h[6],i=c->h[7];
    for (int t = 0; t < 64; t++) {
        uint32_t S1 = ror32(f,6) ^ ror32(f,11) ^ ror32(f,25);
        uint32_t ch = (f & g) ^ (~f & hh);
        uint32_t t1 = i + S1 + ch + K256[t] + w[t];
        uint32_t S0 = ror32(a,2) ^ ror32(a,13) ^ ror32(a,22);
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

static void sha256_of_bytes(const uint8_t *p, size_t n, char out[65]) {
    sha256_ctx c;
    sha256_init(&c);
    sha256_update(&c, p, n);
    sha256_hex(&c, out);
}

/* ------------------------------------------------------------------ */
/* Host I/O + case reporting                                           */
/* ------------------------------------------------------------------ */

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
    if (fseek(f, cur, SEEK_SET) != 0) return -1;
    return (int64_t)end;
}

static int g_cases = 0;
static int g_failed = 0;

static void case_pass(const char *name) {
    g_cases++;
    printf("CASE %s PASS\n", name);
}

static void case_fail(const char *name, const char *fmt, ...) {
    g_cases++;
    g_failed++;
    printf("CASE %s FAIL ", name);
    va_list ap;
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    printf("\n");
}

static void mode_result(const char *mode) {
    printf("MODE %s CASES=%d FAILED=%d RESULT=%s\n", mode, g_cases, g_failed,
           g_failed == 0 ? "PASS" : "FAIL");
    fflush(stdout);
}

static int open_file_io(const char *path, FILE **out_f, song_io *out_io) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    out_io->userdata = f;
    out_io->read = host_read;
    out_io->seek = host_seek;
    out_io->size = host_size;
    *out_f = f;
    return 0;
}

static void close_file_io(FILE *f) {
    if (f) fclose(f);
}

static int fetch_diag(song_handle *h, const song_error **err) {
    return song_last_error(h, err) == SONG_OK ? 0 : -1;
}

static int status_is_typed_error(int st) {
    return st >= 100 && st <= 113;
}

/* Drain to the terminal, accumulating PCM at dst. Every read lands in a
 * block-sized scratch buffer first; at most max_frames frames are copied
 * into dst, so a defecting artifact that overproduces cannot overflow the
 * caller's buffer (total still counts every produced frame). dst must be
 * sized for max_frames frames. Returns the terminal status. SONG_OK blocks
 * must produce frames in (0, block]. */
static song_status drain(song_handle *h, float *dst, uint64_t block,
                         uint64_t channels, uint64_t max_frames,
                         uint64_t *out_frames, int *out_saw_zero,
                         int *out_bad_status) {
    uint64_t total = 0;
    *out_saw_zero = 0;
    *out_bad_status = 0;
    float *scratch = (float *)malloc((size_t)block * (size_t)channels *
                                     sizeof(float));
    if (!scratch) {
        *out_bad_status = 1;
        *out_frames = total;
        return SONG_ERR_OUT_OF_MEMORY;
    }
    for (;;) {
        uint64_t got = 0;
        song_status st = song_read_pcm(h, scratch, block, &got);
        if (st == SONG_EOF) {
            if (got != 0) *out_bad_status = 1;
            *out_frames = total;
            free(scratch);
            return SONG_EOF;
        }
        if (st == SONG_OK) {
            if (got == 0 || got > block) {
                *out_saw_zero = 1;
                *out_frames = total;
                free(scratch);
                return st;
            }
            if (dst && total < max_frames) {
                uint64_t room = max_frames - total;
                uint64_t take = got < room ? got : room;
                memcpy(dst + total * channels, scratch,
                       (size_t)take * (size_t)channels * sizeof(float));
            }
            total += got;
            continue;
        }
        if (!status_is_typed_error((int)st)) *out_bad_status = 1;
        *out_frames = total;
        free(scratch);
        return st;
    }
}

/* ------------------------------------------------------------------ */
/* parity mode                                                         */
/* ------------------------------------------------------------------ */

static int run_parity(int argc, char **argv) {
    if (argc < 11) {
        printf("usage: parity <file> <codec> <container> <duration_us> "
               "<rate> <channels> <frames> <pcm_sha256> <lossless>\n");
        return 2;
    }
    const char *path = argv[2];
    const char *exp_codec = argv[3];
    const char *exp_container = argv[4];
    int64_t exp_duration = strtoll(argv[5], NULL, 10);
    int64_t exp_rate = strtoll(argv[6], NULL, 10);
    int64_t exp_channels = strtoll(argv[7], NULL, 10);
    uint64_t exp_frames = strtoull(argv[8], NULL, 10);
    const char *exp_sha = argv[9];
    int lossless = (int)strtoll(argv[10], NULL, 10);

    /* step 01: ABI version */
    uint32_t abi = songcore_abi_version();
    if (abi == SONGCORE_ABI_VERSION && abi == 1u) {
        case_pass("s01.abi-version");
    } else {
        char d[96];
        snprintf(d, sizeof(d), "abi=%u want=%u", abi, SONGCORE_ABI_VERSION);
        case_fail("s01.abi-version", "%s", d);
    }

    /* step 02: open via host I/O callbacks */
    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    song_handle *h = NULL;
    if (open_file_io(path, &f, &io) != 0) {
        case_fail("s02.open", "fopen failed for %s", path);
        mode_result("parity");
        return 1;
    }
    song_status st = song_open(&io, &h);
    if (st == SONG_OK && h != NULL) {
        case_pass("s02.open");
    } else {
        char d[96];
        snprintf(d, sizeof(d), "status=%d handle=%p", (int)st, (void *)h);
        case_fail("s02.open", "%s", d);
        mode_result("parity");
        return 1;
    }

    /* diagnostic contract: NULL message while the last call succeeded */
    const song_error *err = NULL;
    if (fetch_diag(h, &err) == 0 && err != NULL && err->message == NULL) {
        case_pass("s02.diag-null-after-ok");
    } else {
        case_fail("s02.diag-null-after-ok", "message not NULL after open");
    }

    /* pre-probe state: probed-handle operations → NOT_OPEN (102) */
    {
        uint32_t cnt = 0;
        float tmp[64];
        uint64_t got = 0;
        song_status s1 = song_audio_stream_count(h, &cnt);
        song_status s2 = song_read_pcm(h, tmp, 64, &got);
        if (s1 == SONG_ERR_NOT_OPEN && s2 == SONG_ERR_NOT_OPEN) {
            case_pass("s03.not-open-class");
        } else {
            char d[96];
            snprintf(d, sizeof(d), "count=%d read=%d want both %d",
                     (int)s1, (int)s2, (int)SONG_ERR_NOT_OPEN);
            case_fail("s03.not-open-class", "%s", d);
        }
    }

    /* step 03: probe → frozen source facts */
    song_info info;
    memset(&info, 0, sizeof(info));
    st = song_probe(h, &info);
    if (st != SONG_OK) {
        case_fail("s03.probe-facts", "status=%d", (int)st);
        song_close(h);
        close_file_io(f);
        mode_result("parity");
        return 1;
    }
    int facts_ok =
        info.sample_rate == exp_rate &&
        info.channels == exp_channels &&
        info.duration_us == exp_duration &&
        strcmp(info.codec, exp_codec) == 0 &&
        strcmp(info.container, exp_container) == 0 &&
        info.sample_rate > 0 && info.channels > 0;
    if (facts_ok) {
        case_pass("s03.probe-facts");
    } else {
        case_fail("s03.probe-facts",
                  "rate=%d/%lld ch=%d/%lld dur=%lld/%lld codec=%s/%s container=%s/%s",
                  info.sample_rate, (long long)exp_rate, info.channels,
                  (long long)exp_channels, (long long)info.duration_us,
                  (long long)exp_duration, info.codec, exp_codec,
                  info.container, exp_container);
    }
    printf("INFO probe rate=%d channels=%d mask=0x%llx duration=%lld bps=%d "
           "codec=%s container=%s streams=%u selected=%u\n",
           info.sample_rate, info.channels,
           (unsigned long long)info.channel_mask,
           (long long)info.duration_us, info.bits_per_sample,
           info.codec, info.container, info.audio_stream_count,
           info.selected_audio_index);

    /* probe idempotence: cached snapshot */
    {
        song_info info2;
        memset(&info2, 0, sizeof(info2));
        song_status s2 = song_probe(h, &info2);
        if (s2 == SONG_OK && memcmp(&info2, &info, sizeof(song_info)) == 0) {
            case_pass("s03.probe-idempotent");
        } else {
            case_fail("s03.probe-idempotent", "second probe differs or failed");
        }
    }

    /* step 04: audio-stream enumeration + default stream identity */
    {
        uint32_t count = 0;
        song_status s1 = song_audio_stream_count(h, &count);
        int ok = s1 == SONG_OK && count >= 1;
        if (ok) {
            for (uint32_t i = 0; i < count && ok; i++) {
                song_stream_info si;
                memset(&si, 0, sizeof(si));
                if (song_audio_stream_info(h, i, &si) != SONG_OK) { ok = 0; break; }
                if (si.audio_index != i) { ok = 0; break; }
                if (si.is_default != 0 && si.is_default != 1) { ok = 0; break; }
                if (i == info.selected_audio_index) {
                    ok = si.sample_rate == info.sample_rate &&
                         si.channels == info.channels &&
                         strcmp(si.codec, info.codec) == 0;
                }
            }
        }
        song_stream_info si_bad;
        memset(&si_bad, 0, sizeof(si_bad));
        song_status s_bad = song_audio_stream_info(h, count, &si_bad);
        if (ok && s_bad == SONG_ERR_INVALID_ARGUMENT) {
            case_pass("s04.streams-default");
        } else {
            char d[96];
            snprintf(d, sizeof(d), "count_status=%d bad_idx=%d", (int)s1,
                     (int)s_bad);
            case_fail("s04.streams-default", "%s", d);
        }
        printf("INFO streams count=%u\n", count);
    }

    /* step 05: metadata canonical + raw enumeration */
    {
        const song_metadata *meta = NULL;
        song_status s1 = song_get_metadata(h, &meta);
        uint32_t raw_count = 0;
        song_status s2 = song_get_metadata_count(h, &raw_count);
        int ok = s1 == SONG_OK && meta != NULL && s2 == SONG_OK;
        int scopes_ok = 1, keys_ok = 1;
        if (ok) {
            if (meta->has_title != 0 && meta->has_title != 1) scopes_ok = 0;
            if (meta->has_artist != 0 && meta->has_artist != 1) scopes_ok = 0;
            if (meta->has_album != 0 && meta->has_album != 1) scopes_ok = 0;
            for (uint32_t i = 0; i < raw_count; i++) {
                song_metadata_entry e;
                memset(&e, 0, sizeof(e));
                if (song_get_metadata_entry(h, i, &e) != SONG_OK) {
                    keys_ok = 0;
                    break;
                }
                if (e.scope != SONG_METADATA_SCOPE_CONTAINER &&
                    e.scope != SONG_METADATA_SCOPE_STREAM) {
                    scopes_ok = 0;
                }
                if (e.key == NULL || e.key_len == 0 || e.value == NULL) {
                    keys_ok = 0;
                }
            }
        }
        if (ok && scopes_ok && keys_ok) {
            case_pass("s05.metadata");
        } else {
            char d[128];
            snprintf(d, sizeof(d), "meta=%d raw=%u scopes=%d keys=%d",
                     (int)s1, raw_count, scopes_ok, keys_ok);
            case_fail("s05.metadata", "%s", d);
        }
        printf("INFO metadata raw_count=%u has_title=%u has_artist=%u "
               "has_album=%u\n", raw_count, meta ? meta->has_title : 0,
               meta ? meta->has_artist : 0, meta ? meta->has_album : 0);
        for (uint32_t i = 0; i < raw_count && i < 16; i++) {
            song_metadata_entry e;
            memset(&e, 0, sizeof(e));
            if (song_get_metadata_entry(h, i, &e) == SONG_OK) {
                printf("META[%u] scope=%u key=%.*s value=%.*s\n", i, e.scope,
                       (int)e.key_len, e.key,
                       (int)(e.value_len > 64 ? 64 : e.value_len), e.value);
            }
        }
    }

    /* step 06: artwork (structural; corpus fixtures provide none) */
    {
        uint32_t aw = 0;
        song_status s = song_get_artwork_count(h, &aw);
        int ok = s == SONG_OK;
        for (uint32_t i = 0; ok && i < aw; i++) {
            song_artwork_item ai;
            memset(&ai, 0, sizeof(ai));
            if (song_get_artwork_item(h, i, &ai) != SONG_OK) { ok = 0; break; }
            if (ai.role > SONG_ARTWORK_OTHER) { ok = 0; break; }
            if (ai.data_len > 0 && ai.data == NULL) { ok = 0; break; }
        }
        if (ok) {
            case_pass("s06.artwork");
        } else {
            char d[64];
            snprintf(d, sizeof(d), "status=%d", (int)s);
            case_fail("s06.artwork", "%s", d);
        }
        printf("INFO artwork count=%u\n", aw);
    }

    /* step 07: full drain → frozen frame count + PCM SHA-256 */
    uint64_t block = 4096;
    size_t pcm_bytes = (size_t)exp_frames * (size_t)exp_channels * sizeof(float);
    float *full = (float *)malloc(pcm_bytes);
    if (!full) {
        case_fail("s07.full-decode", "alloc %zu failed", pcm_bytes);
        song_close(h);
        close_file_io(f);
        mode_result("parity");
        return 1;
    }
    {
        sha256_ctx c;
        sha256_init(&c);
        uint64_t total = 0;
        int bad = 0;
        song_status term = SONG_OK;
        float *scratch = (float *)malloc((size_t)block *
                                         (size_t)exp_channels * sizeof(float));
        if (!scratch) bad = 1;
        while (!bad) {
            uint64_t got = 0;
            song_status s = song_read_pcm(h, scratch, block, &got);
            if (s == SONG_EOF) {
                term = s;
                if (got != 0) bad = 1;
                break;
            }
            if (s == SONG_OK) {
                if (got == 0 || got > block) { bad = 1; term = s; break; }
                sha256_update(&c, scratch,
                              (size_t)got * (size_t)exp_channels * sizeof(float));
                if (total < exp_frames) {
                    uint64_t room = exp_frames - total;
                    uint64_t take = got < room ? got : room;
                    memcpy(full + total * (uint64_t)exp_channels, scratch,
                           (size_t)take * (size_t)exp_channels * sizeof(float));
                }
                total += got;
                continue;
            }
            term = s;
            bad = 1;
            break;
        }
        free(scratch);
        char hex[65];
        sha256_hex(&c, hex);
        if (term == SONG_EOF && !bad && total == exp_frames &&
            strcmp(hex, exp_sha) == 0) {
            case_pass("s07.full-decode");
        } else {
            case_fail("s07.full-decode",
                      "terminal=%d bad=%d frames=%llu/%llu sha=%s want=%s",
                      (int)term, bad, (unsigned long long)total,
                      (unsigned long long)exp_frames, hex, exp_sha);
        }
        printf("INFO drain frames=%llu terminal=%d pcm_sha256=%s\n",
               (unsigned long long)total, (int)term, hex);
    }

    /* step 11: EOF stability + EOF distinct from error */
    {
        uint64_t got = 0xdeadbeef;
        song_status s1 = song_read_pcm(h, full, block, &got);
        song_status s2 = song_read_pcm(h, full, block, &got);
        int ok = s1 == SONG_EOF && got == 0 && s2 == SONG_EOF && got == 0 &&
                 SONG_EOF == 1 && SONG_EOF < 100;
        if (ok) {
            case_pass("s11.eof-stable");
        } else {
            char d[96];
            snprintf(d, sizeof(d), "s1=%d got=%llu s2=%d", (int)s1,
                     (unsigned long long)got, (int)s2);
            case_fail("s11.eof-stable", "%s", d);
        }
    }

    /* steps 08–10: seek 25/50/75% */
    for (int pct = 25; pct <= 75; pct += 25) {
        char cname[48];
        snprintf(cname, sizeof(cname), "s%02d.seek-%d", 7 + pct / 25, pct);
        int64_t target = exp_duration * pct / 100;
        int64_t landing = -7;

        song_info pre_info;
        memset(&pre_info, 0, sizeof(pre_info));
        const song_metadata *pre_meta = NULL;
        uint32_t pre_raw = 0, pre_aw = 0;
        int snap_ok = song_probe(h, &pre_info) == SONG_OK &&
                      song_get_metadata(h, &pre_meta) == SONG_OK &&
                      song_get_metadata_count(h, &pre_raw) == SONG_OK &&
                      song_get_artwork_count(h, &pre_aw) == SONG_OK;

        if (pct == 75) {
            /* documented: out_actual_position_us may be NULL; observe the
             * landing with a second seek to the same target */
            song_status s_null = song_seek(h, target, NULL);
            st = song_seek(h, target, &landing);
            if (s_null != SONG_OK) {
                case_fail(cname, "null-out seek status=%d", (int)s_null);
                case_fail("s10.lands", "seek failed; landing unobservable");
                case_fail("s10.unchanged", "seek failed");
                continue;
            }
        } else {
            st = song_seek(h, target, &landing);
        }
        int seek_ok = snap_ok && st == SONG_OK &&
                      (landing == -1 || (landing >= 0 && landing <= target));
        if (seek_ok) {
            case_pass(cname);
        } else {
            char d[128];
            snprintf(d, sizeof(d), "status=%d landing=%lld target=%lld snap=%d",
                     (int)st, (long long)landing, (long long)target, snap_ok);
            case_fail(cname, "%s", d);
        }
        printf("INFO seek pct=%d target=%lld landing=%lld status=%d\n",
               pct, (long long)target, (long long)landing, (int)st);

        /* next read belongs to the landing */
        char cname2[64];
        snprintf(cname2, sizeof(cname2), "%s.lands", cname);
        char cname3[64];
        snprintf(cname3, sizeof(cname3), "%s.unchanged", cname);
        if (st != SONG_OK) {
            case_fail(cname2, "seek failed; landing read not observable");
            case_fail(cname3, "seek failed");
            continue;
        }
        float *post = (float *)malloc(pcm_bytes);
        if (!post) {
            case_fail(cname2, "alloc failed");
            case_fail(cname3, "alloc failed");
            continue;
        }
        uint64_t got_total = 0;
        int saw_zero = 0, bad_status = 0;
        song_status term = drain(h, post, block, (uint64_t)exp_channels,
                                 exp_frames, &got_total, &saw_zero,
                                 &bad_status);
        int lands_ok = term == SONG_EOF && got_total > 0 && !saw_zero &&
                       !bad_status;
        if (lossless && landing >= 0) {
            uint64_t f = ((uint64_t)landing * (uint64_t)exp_rate + 500000) /
                         1000000;
            lands_ok = lands_ok && f <= exp_frames &&
                       got_total == exp_frames - f &&
                       memcmp(post, full + f * (uint64_t)exp_channels,
                              (size_t)got_total * (size_t)exp_channels *
                                  sizeof(float)) == 0;
            if (!lands_ok) {
                printf("INFO lands-diff pct=%d f=%llu got=%llu want=%llu\n",
                       pct, (unsigned long long)f,
                       (unsigned long long)got_total,
                       (unsigned long long)(exp_frames - f));
            }
        } else {
            /* lossy (or unknown landing): the corpus freezes no numeric
             * post-seek tolerance and inventing one is forbidden — the
             * drain is recorded, not identity-compared. */
            printf("INFO post-seek-record-only lossy=%d landing=%lld "
                   "frames=%llu\n", lossless, (long long)landing,
                   (unsigned long long)got_total);
        }
        if (lands_ok) {
            case_pass(cname2);
        } else {
            char d[128];
            snprintf(d, sizeof(d), "terminal=%d frames=%llu zero=%d bad=%d",
                     (int)term, (unsigned long long)got_total, saw_zero,
                     bad_status);
            case_fail(cname2, "%s", d);
        }

        /* metadata/artwork/format unchanged after seek */
        {
            song_info post_info;
            memset(&post_info, 0, sizeof(post_info));
            const song_metadata *post_meta = NULL;
            uint32_t post_raw = 0, post_aw = 0;
            int u_ok = song_probe(h, &post_info) == SONG_OK &&
                       song_get_metadata(h, &post_meta) == SONG_OK &&
                       song_get_metadata_count(h, &post_raw) == SONG_OK &&
                       song_get_artwork_count(h, &post_aw) == SONG_OK;
            if (u_ok) {
                u_ok = post_info.sample_rate == pre_info.sample_rate &&
                       post_info.channels == pre_info.channels &&
                       post_info.channel_mask == pre_info.channel_mask &&
                       post_info.duration_us == pre_info.duration_us &&
                       strcmp(post_info.codec, pre_info.codec) == 0 &&
                       strcmp(post_info.container, pre_info.container) == 0 &&
                       post_info.audio_stream_count ==
                           pre_info.audio_stream_count &&
                       post_info.selected_audio_index ==
                           pre_info.selected_audio_index &&
                       post_raw == pre_raw && post_aw == pre_aw &&
                       post_meta->has_title == pre_meta->has_title &&
                       post_meta->has_artist == pre_meta->has_artist;
            }
            if (u_ok) {
                case_pass(cname3);
            } else {
                case_fail(cname3, "info/metadata/artwork changed across seek");
            }
        }
        free(post);
    }

    /* step 13: close mid-stream, reopen, re-drain stays stable */
    {
        song_close(h);
        close_file_io(f);
        FILE *f2 = NULL;
        song_io io2;
        memset(&io2, 0, sizeof(io2));
        song_handle *h2 = NULL;
        int ok = open_file_io(path, &f2, &io2) == 0;
        if (ok) ok = song_open(&io2, &h2) == SONG_OK && h2 != NULL;
        if (ok) {
            song_info info2;
            memset(&info2, 0, sizeof(info2));
            ok = song_probe(h2, &info2) == SONG_OK;
            if (ok) {
                uint64_t total = 0;
                int saw_zero = 0, bad_status = 0;
                song_status term = drain(h2, full, block,
                                         (uint64_t)exp_channels, exp_frames,
                                         &total, &saw_zero, &bad_status);
                char hex[65];
                sha256_of_bytes((const uint8_t *)full,
                                (size_t)total * (size_t)exp_channels *
                                    sizeof(float),
                                hex);
                ok = term == SONG_EOF && !saw_zero && !bad_status &&
                     total == exp_frames && strcmp(hex, exp_sha) == 0;
                if (!ok) {
                    printf("INFO reopen frames=%llu term=%d sha=%s\n",
                           (unsigned long long)total, (int)term, hex);
                }
            }
            song_close(h2);
        }
        close_file_io(f2);
        if (ok) {
            case_pass("s13.reopen-stable");
        } else {
            case_fail("s13.reopen-stable", "reopen drain differs or failed");
        }
    }

    free(full);
    song_close(NULL); /* documented no-op; the process must survive here */
    case_pass("s13.close-null-noop");
    mode_result("parity");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* readfault mode (#296/#324 decode-read construction)                 */
/* ------------------------------------------------------------------ */

static int run_readfault(int argc, char **argv) {
    if (argc < 3) {
        printf("usage: readfault <file>\n");
        return 2;
    }
    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    song_handle *h = NULL;
    if (open_file_io(argv[2], &f, &io) != 0) {
        case_fail("rf.open", "fopen failed");
        mode_result("readfault");
        return 1;
    }
    song_status st = song_open(&io, &h);
    song_info info;
    memset(&info, 0, sizeof(info));
    if (!(st == SONG_OK && h != NULL && song_probe(h, &info) == SONG_OK)) {
        case_fail("rf.open-probe", "status=%d", (int)st);
        mode_result("readfault");
        return 1;
    }
    case_pass("rf.open-probe");

    uint64_t block = 4096;
    float *dst = (float *)malloc((size_t)block * (size_t)info.channels *
                                 sizeof(float));
    if (!dst) {
        case_fail("rf.alloc", "alloc failed");
        song_close(h);
        close_file_io(f);
        mode_result("readfault");
        return 1;
    }
    uint64_t total = 0;
    song_status term = SONG_OK;
    for (;;) {
        uint64_t got = 0;
        song_status s = song_read_pcm(h, dst, block, &got);
        if (s == SONG_OK) { total += got; continue; }
        term = s;
        break;
    }
    if (total > 0) {
        case_pass("rf.prefix-frames");
    } else {
        case_fail("rf.prefix-frames", "no clean prefix frames before failure");
    }
    /* frozen #296/#324 attribution: SONG_ERR_DECODE_ERROR (108),
     * native = -1094995529 (AVERROR_INVALIDDATA) */
    if (term == SONG_ERR_DECODE_ERROR) {
        case_pass("rf.terminal-108");
    } else {
        case_fail("rf.terminal-108", "terminal=%d want=%d", (int)term,
                  (int)SONG_ERR_DECODE_ERROR);
    }
    const song_error *err = NULL;
    int diag_ok = fetch_diag(h, &err) == 0 && err != NULL &&
                  err->message != NULL && err->message_len > 0 &&
                  err->native_code == -1094995529;
    if (diag_ok) {
        case_pass("rf.native-code");
    } else {
        char d[160];
        snprintf(d, sizeof(d), "native=%d msg=%s", err ? err->native_code : 0,
                 err && err->message ? err->message : "(null)");
        case_fail("rf.native-code", "%s", d);
    }
    /* error stability: the next read re-surfaces the pending decode error */
    {
        uint64_t got = 77;
        song_status s2 = song_read_pcm(h, dst, block, &got);
        if (s2 == SONG_ERR_DECODE_ERROR && got == 0) {
            case_pass("rf.error-stable");
        } else {
            char d[96];
            snprintf(d, sizeof(d), "next=%d got=%llu", (int)s2,
                     (unsigned long long)got);
            case_fail("rf.error-stable", "%s", d);
        }
    }
    printf("INFO readfault frames_before=%llu terminal=%d native=%d msg=%.*s\n",
           (unsigned long long)total, (int)term, err ? err->native_code : 0,
           err && err->message ? (int)err->message_len : 0,
           err && err->message ? err->message : "");
    free(dst);
    song_close(h);
    close_file_io(f);
    mode_result("readfault");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* seekfault mode (#296/#324 decode-seek construction)                 */
/* ------------------------------------------------------------------ */

static int run_seekfault(int argc, char **argv) {
    if (argc < 3) {
        printf("usage: seekfault <file>\n");
        return 2;
    }
    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    song_handle *h = NULL;
    if (open_file_io(argv[2], &f, &io) != 0) {
        case_fail("sf.open", "fopen failed");
        mode_result("seekfault");
        return 1;
    }
    song_status st = song_open(&io, &h);
    song_info info;
    memset(&info, 0, sizeof(info));
    if (!(st == SONG_OK && h != NULL && song_probe(h, &info) == SONG_OK)) {
        case_fail("sf.open-probe", "status=%d", (int)st);
        mode_result("seekfault");
        return 1;
    }
    case_pass("sf.open-probe");

    uint64_t block = 4096;
    float *dst = (float *)malloc((size_t)block * (size_t)info.channels *
                                 sizeof(float));
    if (!dst) {
        case_fail("sf.alloc", "alloc failed");
        song_close(h);
        close_file_io(f);
        mode_result("seekfault");
        return 1;
    }

    /* clean prefix before the corrupted region */
    uint64_t prefix = 0;
    int prefix_clean = 1;
    for (int i = 0; i < 8; i++) {
        uint64_t got = 0;
        song_status s = song_read_pcm(h, dst, block, &got);
        if (s != SONG_OK) { prefix_clean = 0; break; }
        prefix += got;
    }
    if (prefix_clean && prefix > 0) {
        case_pass("sf.prefix-clean");
    } else {
        case_fail("sf.prefix-clean", "prefix=%llu",
                  (unsigned long long)prefix);
    }

    /* seek 2.0s lands on the corrupt body → typed SEEK_ERROR, -1 landing */
    int64_t landing = -7;
    song_status sseek = song_seek(h, 2000000, &landing);
    const song_error *err = NULL;
    int diag_ok = fetch_diag(h, &err) == 0 && err != NULL &&
                  err->message != NULL &&
                  strcmp(err->message, "could not reach a landing point") == 0;
    if (sseek == SONG_ERR_SEEK_ERROR && landing == -1) {
        case_pass("sf.seek-typed");
    } else {
        case_fail("sf.seek-typed", "status=%d landing=%lld want=%d/-1",
                  (int)sseek, (long long)landing, (int)SONG_ERR_SEEK_ERROR);
    }
    if (diag_ok) {
        case_pass("sf.seek-diagnostic");
    } else {
        char d[160];
        snprintf(d, sizeof(d), "msg=%s",
                 err && err->message ? err->message : "(null)");
        case_fail("sf.seek-diagnostic", "%s", d);
    }

    /* post-seek read surfaces the real landing-frame decode failure */
    uint64_t got = 12345;
    song_status sread = song_read_pcm(h, dst, block, &got);
    const song_error *err2 = NULL;
    (void)fetch_diag(h, &err2);
    if (sread == SONG_ERR_DECODE_ERROR && got == 0 && err2 != NULL &&
        err2->native_code == -1094995529) {
        case_pass("sf.post-read-108");
    } else {
        char d[160];
        snprintf(d, sizeof(d), "status=%d got=%llu native=%d", (int)sread,
                 (unsigned long long)got, err2 ? err2->native_code : 0);
        case_fail("sf.post-read-108", "%s", d);
    }
    printf("INFO seekfault prefix=%llu seek=%d landing=%lld read=%d native=%d\n",
           (unsigned long long)prefix, (int)sseek, (long long)landing,
           (int)sread, err2 ? err2->native_code : 0);
    free(dst);
    song_close(h);
    close_file_io(f);
    mode_result("seekfault");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* truncated mode (typed fail-closed, no fabricated data)              */
/* ------------------------------------------------------------------ */

static int run_truncated(int argc, char **argv) {
    if (argc < 4) {
        printf("usage: truncated <file> <orig_frames>\n");
        return 2;
    }
    uint64_t orig_frames = strtoull(argv[3], NULL, 10);
    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    song_handle *h = NULL;
    if (open_file_io(argv[2], &f, &io) != 0) {
        case_fail("tr.open", "fopen failed");
        mode_result("truncated");
        return 1;
    }
    song_status st = song_open(&io, &h);
    if (st != SONG_OK) {
        /* a typed open failure is fail-closed legal for a truncated source */
        if (status_is_typed_error((int)st)) {
            case_pass("tr.typed-open");
        } else {
            case_fail("tr.typed-open", "untyped open status=%d", (int)st);
        }
        printf("INFO truncated open_status=%d\n", (int)st);
        close_file_io(f);
        mode_result("truncated");
        return g_failed == 0 ? 0 : 1;
    }
    song_info info;
    memset(&info, 0, sizeof(info));
    song_status p = song_probe(h, &info);
    if (p != SONG_OK) {
        const song_error *err = NULL;
        (void)fetch_diag(h, &err);
        if (status_is_typed_error((int)p) && err != NULL &&
            err->message != NULL) {
            case_pass("tr.typed-open");
        } else {
            case_fail("tr.typed-open", "untyped probe failure status=%d",
                      (int)p);
        }
        printf("INFO truncated probe_status=%d\n", (int)p);
        song_close(h);
        close_file_io(f);
        mode_result("truncated");
        return g_failed == 0 ? 0 : 1;
    }
    case_pass("tr.typed-open");

    uint64_t block = 4096;
    /* the truncated source can decode at most the original fixture's frames;
     * the drain accumulator is bounded by that frozen count */
    float *dst = (float *)malloc((size_t)orig_frames * (size_t)info.channels *
                                 sizeof(float));
    if (!dst) {
        case_fail("tr.alloc", "alloc failed");
        song_close(h);
        close_file_io(f);
        mode_result("truncated");
        return 1;
    }
    uint64_t total = 0;
    int saw_zero = 0, bad_status = 0;
    song_status term = drain(h, dst, block, (uint64_t)info.channels,
                             orig_frames, &total, &saw_zero, &bad_status);
    int ok = total > 0 && !saw_zero && !bad_status &&
             (term == SONG_EOF || status_is_typed_error((int)term));
    if (ok) {
        case_pass("tr.typed-terminal");
    } else {
        case_fail("tr.typed-terminal",
                  "terminal=%d frames=%llu zero=%d bad=%d", (int)term,
                  (unsigned long long)total, saw_zero, bad_status);
    }
    /* terminal stability: EOF repeats; an error terminal stays typed */
    {
        uint64_t got = 77;
        song_status s2 = song_read_pcm(h, dst, block, &got);
        int stable = (term == SONG_EOF && s2 == SONG_EOF && got == 0) ||
                     (status_is_typed_error((int)term) &&
                      status_is_typed_error((int)s2));
        if (stable) {
            case_pass("tr.terminal-stable");
        } else {
            char d[96];
            snprintf(d, sizeof(d), "term=%d next=%d got=%llu", (int)term,
                     (int)s2, (unsigned long long)got);
            case_fail("tr.terminal-stable", "%s", d);
        }
    }
    printf("INFO truncated frames=%llu terminal=%d\n",
           (unsigned long long)total, (int)term);
    free(dst);
    song_close(h);
    close_file_io(f);
    mode_result("truncated");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* garbage mode (documented open-error classes, handle untouched)      */
/* ------------------------------------------------------------------ */

static int run_garbage(int argc, char **argv) {
    if (argc < 3) {
        printf("usage: garbage <file>\n");
        return 2;
    }
    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    /* pre-init NULL: both the header's "untouched" letter and the native's
     * fail-path NULL write must leave the caller WITHOUT a usable handle.
     * Differential note: songcore.h says "*out_handle is then untouched",
     * while native/src/songcore_ffmpeg.c song_open() NULLs the out-param on
     * every post-allocation failure path — recorded for authority review;
     * the semantic invariant (no valid handle after a failed open) holds in
     * both readings and is what is asserted here. */
    song_handle *h = NULL;
    if (open_file_io(argv[2], &f, &io) != 0) {
        case_fail("gb.typed-open-no-handle", "fopen failed");
        mode_result("garbage");
        return 1;
    }
    song_status st = song_open(&io, &h);
    int typed = st == SONG_ERR_IO || st == SONG_ERR_UNSUPPORTED_CONTAINER ||
                st == SONG_ERR_OUT_OF_MEMORY;
    if (typed && h == NULL) {
        case_pass("gb.typed-open-no-handle");
    } else {
        char d[96];
        snprintf(d, sizeof(d), "status=%d handle_nonnull=%d", (int)st,
                 h != NULL);
        case_fail("gb.typed-open-no-handle", "%s", d);
    }
    printf("INFO garbage %s status=%d\n", argv[2], (int)st);
    close_file_io(f);
    mode_result("garbage");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* argcheck mode (documented stable status classes)                    */
/* ------------------------------------------------------------------ */

static int run_argcheck(int argc, char **argv) {
    if (argc < 3) {
        printf("usage: argcheck <fixture>\n");
        return 2;
    }
    /* NULL-handle guards */
    song_info info;
    memset(&info, 0, sizeof(info));
    uint64_t got = 0;
    uint32_t cnt = 0;
    const song_error *err = NULL;
    song_status s_read_null = song_read_pcm(NULL, NULL, 4096, &got);
    song_status s_probe_null = song_probe(NULL, &info);
    song_status s_diag_null = song_last_error(NULL, &err);
    song_status s_count_null = song_audio_stream_count(NULL, &cnt);
    if (s_read_null == SONG_ERR_INVALID_ARGUMENT &&
        s_probe_null == SONG_ERR_INVALID_ARGUMENT &&
        s_diag_null == SONG_ERR_INVALID_ARGUMENT &&
        s_count_null == SONG_ERR_INVALID_ARGUMENT) {
        case_pass("ac.null-handle");
    } else {
        char d[128];
        snprintf(d, sizeof(d), "read=%d probe=%d diag=%d count=%d",
                 (int)s_read_null, (int)s_probe_null, (int)s_diag_null,
                 (int)s_count_null);
        case_fail("ac.null-handle", "%s", d);
    }

    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    song_handle *h = NULL;
    if (open_file_io(argv[2], &f, &io) != 0) {
        case_fail("ac.open", "fopen failed");
        mode_result("argcheck");
        return 1;
    }
    song_status so = song_open(&io, &h);
    if (so != SONG_OK || h == NULL) {
        case_fail("ac.open", "status=%d", (int)so);
        close_file_io(f);
        mode_result("argcheck");
        return 1;
    }

    /* pre-probe: probed-handle operations → NOT_OPEN (102) */
    {
        uint32_t c1 = 0;
        uint64_t g1 = 0;
        int64_t l1 = 0;
        const song_metadata *m1 = NULL;
        float tmp[64];
        song_status a = song_audio_stream_count(h, &c1);
        song_status b = song_read_pcm(h, tmp, 64, &g1);
        song_status c = song_seek(h, 0, &l1);
        song_status d = song_get_metadata(h, &m1);
        song_status e = song_select_stream(h, 0);
        if (a == SONG_ERR_NOT_OPEN && b == SONG_ERR_NOT_OPEN &&
            c == SONG_ERR_NOT_OPEN && d == SONG_ERR_NOT_OPEN &&
            e == SONG_ERR_NOT_OPEN) {
            case_pass("ac.pre-probe-not-open");
        } else {
            char d2[160];
            snprintf(d2, sizeof(d2),
                     "count=%d read=%d seek=%d meta=%d sel=%d", (int)a,
                     (int)b, (int)c, (int)d, (int)e);
            case_fail("ac.pre-probe-not-open", "%s", d2);
        }
    }

    if (song_probe(h, &info) != SONG_OK) {
        case_fail("ac.probe", "probe failed");
        song_close(h);
        close_file_io(f);
        mode_result("argcheck");
        return 1;
    }

    /* documented argument classes on a probed handle */
    {
        float tmp[64];
        uint64_t g0 = 99;
        song_status a = song_read_pcm(h, tmp, 0, &g0); /* capacity 0 */
        song_status b = song_select_stream(h, info.audio_stream_count);
        song_status c = song_seek(h, -1, NULL);        /* negative target */
        if (a == SONG_ERR_INVALID_ARGUMENT && g0 == 0 &&
            b == SONG_ERR_INVALID_ARGUMENT &&
            c == SONG_ERR_INVALID_ARGUMENT) {
            case_pass("ac.invalid-argument");
        } else {
            char d[128];
            snprintf(d, sizeof(d), "cap0=%d sel=%d seekneg=%d", (int)a,
                     (int)b, (int)c);
            case_fail("ac.invalid-argument", "%s", d);
        }
    }

    /* diagnostic contract: NULL after success, non-NULL after failure.
     * A successful call must come last before the NULL-side check (the
     * invalid-argument probes above deliberately left an error set). */
    {
        uint32_t ok_cnt = 0;
        (void)song_audio_stream_count(h, &ok_cnt); /* successful call */
        const song_error *e1 = NULL;
        int ok1 = fetch_diag(h, &e1) == 0 && e1 != NULL && e1->message == NULL;
        (void)song_seek(h, -1, NULL); /* typed failure */
        const song_error *e2 = NULL;
        int ok2 = fetch_diag(h, &e2) == 0 && e2 != NULL &&
                  e2->message != NULL && e2->message_len > 0;
        if (ok1 && ok2) {
            case_pass("ac.diag-contract");
        } else {
            char d[128];
            snprintf(d, sizeof(d), "after_ok_null=%d after_fail_msg=%d", ok1,
                     ok2);
            case_fail("ac.diag-contract", "%s", d);
        }
    }

    song_close(h);
    close_file_io(f);
    mode_result("argcheck");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */
/* closecheck mode (S-BIND step 13 lifetime arm)                       */
/* ------------------------------------------------------------------ */

static int run_closecheck(int argc, char **argv) {
    if (argc < 5) {
        printf("usage: closecheck <file> <frames> <pcm_sha256>\n");
        return 2;
    }
    const char *path = argv[2];
    uint64_t exp_frames = strtoull(argv[3], NULL, 10);
    const char *exp_sha = argv[4];

    FILE *f = NULL;
    song_io io;
    memset(&io, 0, sizeof(io));
    song_handle *h = NULL;
    if (open_file_io(path, &f, &io) != 0 || song_open(&io, &h) != SONG_OK ||
        h == NULL) {
        case_fail("cc.open", "open failed");
        mode_result("closecheck");
        return 1;
    }
    song_info info;
    memset(&info, 0, sizeof(info));
    if (song_probe(h, &info) != SONG_OK) {
        case_fail("cc.open", "probe failed");
        song_close(h);
        close_file_io(f);
        mode_result("closecheck");
        return 1;
    }
    /* read ONE block then close mid-stream */
    uint64_t got = 0;
    float *dst = (float *)malloc((size_t)4096 * (size_t)info.channels *
                                 sizeof(float));
    if (!dst || song_read_pcm(h, dst, 4096, &got) != SONG_OK || got == 0) {
        case_fail("cc.midstream-read", "got=%llu", (unsigned long long)got);
        free(dst);
        song_close(h);
        close_file_io(f);
        mode_result("closecheck");
        return 1;
    }
    case_pass("cc.midstream-read");
    song_close(h);
    close_file_io(f);
    h = NULL;

    /* reopen and re-drain → the frozen identity again */
    FILE *f2 = NULL;
    song_io io2;
    memset(&io2, 0, sizeof(io2));
    song_handle *h2 = NULL;
    int ok = open_file_io(path, &f2, &io2) == 0 &&
             song_open(&io2, &h2) == SONG_OK && h2 != NULL;
    if (ok) {
        song_info info2;
        memset(&info2, 0, sizeof(info2));
        ok = song_probe(h2, &info2) == SONG_OK;
        if (ok) {
            sha256_ctx c;
            sha256_init(&c);
            uint64_t total = 0;
            song_status term = SONG_OK;
            for (;;) {
                uint64_t g = 0;
                song_status s = song_read_pcm(h2, dst, 4096, &g);
                if (s == SONG_EOF) { term = s; break; }
                if (s != SONG_OK) { term = s; break; }
                sha256_update(&c, dst,
                              (size_t)g * (size_t)info2.channels * sizeof(float));
                total += g;
            }
            char hex[65];
            sha256_hex(&c, hex);
            ok = term == SONG_EOF && total == exp_frames &&
                 strcmp(hex, exp_sha) == 0;
            if (!ok) {
                printf("INFO closecheck reopen frames=%llu term=%d sha=%s\n",
                       (unsigned long long)total, (int)term, hex);
            }
        }
        song_close(h2);
    }
    close_file_io(f2);
    free(dst);
    if (ok) {
        case_pass("cc.reopen-identity");
    } else {
        case_fail("cc.reopen-identity", "reopen drain differs or failed");
    }
    mode_result("closecheck");
    return g_failed == 0 ? 0 : 1;
}

/* ------------------------------------------------------------------ */

int main(int argc, char **argv) {
    /* unbuffered: a hard crash must not lose the case trail (the runner
     * reads stdout as the receipt) */
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc < 2) {
        fprintf(stderr,
                "usage: %s <parity|readfault|seekfault|truncated|garbage|"
                "argcheck|closecheck> ...\n",
                argv[0]);
        return 2;
    }
    if (strcmp(argv[1], "parity") == 0) return run_parity(argc, argv);
    if (strcmp(argv[1], "readfault") == 0) return run_readfault(argc, argv);
    if (strcmp(argv[1], "seekfault") == 0) return run_seekfault(argc, argv);
    if (strcmp(argv[1], "truncated") == 0) return run_truncated(argc, argv);
    if (strcmp(argv[1], "garbage") == 0) return run_garbage(argc, argv);
    if (strcmp(argv[1], "argcheck") == 0) return run_argcheck(argc, argv);
    if (strcmp(argv[1], "closecheck") == 0) return run_closecheck(argc, argv);
    fprintf(stderr, "unknown mode %s\n", argv[1]);
    return 2;
}
