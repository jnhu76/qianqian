/*
 * pe-consumer.c — Windows PE external consumer verification (Lane C, windows).
 *
 * Target: songcore-v0.0.0-preflight-windows-x86_64 package
 *         (include/songcore.h + lib/libsongcore.a), ABI v1.
 *
 * Path exercised: open -> probe -> streams -> metadata -> artwork
 *                 -> full decode (PCM sha256) -> seek -> post-seek decode
 *                 -> EOF -> close.
 *
 * PCM hash convention is copied verbatim from
 * native/experiments/songcore-equivalence/harness.c run_correct():
 *   4096-frame blocks; sha256 over sizeof(float)*frames_produced*channels
 *   bytes of interleaved Float32, in read order, nothing else. That makes the
 *   printed pcm_sha256 directly comparable with
 *   native/experiments/songcore-equivalence/reference.json.
 *
 * Runtime dependency instrumentation (contract section 9): this file makes no
 * static BCrypt* call, so every bcrypt.dll entry in the produced PE's import
 * table comes from the shipped archive. Two independent runtime observations:
 *   1. bcrypt_runtime_probe(): LoadLibraryA("bcrypt.dll") + GetProcAddress of
 *      the three contract-named exports, then a real
 *      open -> genrandom -> close chain (resolved by pointer, so it does not
 *      go through this PE's IAT).
 *   2. bcrypt IAT counter: the BCryptGenRandom import slot of this module is
 *      patched to a counting thunk before song_open. A non-zero count is
 *      therefore a call made by the linked SongCore/FFmpeg code, i.e. the
 *      av_get_random_seed() entropy path actually executed.
 *
 * Exit codes: 0 = all checks passed, 1 = a stage check failed, 2 = usage/IO.
 */

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <bcrypt.h>          /* types + constants only; no BCrypt* call site */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "songcore.h"

/* ============================ SHA-256 ================================= */

typedef struct {
    uint32_t h[8];
    uint64_t total;
    uint8_t  buf[64];
    size_t   buf_len;
} sha256_ctx;

static const uint32_t K256[64] = {
    0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,
    0x923f82a4u,0xab1c5ed5u,0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,
    0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,0xe49b69c1u,0xefbe4786u,
    0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
    0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,
    0x06ca6351u,0x14292967u,0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,
    0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,0xa2bfe8a1u,0xa81a664bu,
    0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
    0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,
    0x5b9cca4fu,0x682e6ff3u,0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,
    0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u
};

static uint32_t ror32(uint32_t x, int n) { return (x >> n) | (x << (32 - n)); }

static void sha256_compress(sha256_ctx *c, const uint8_t *p) {
    uint32_t w[64], a, b, c2, d, e, f, g, h, t1, t2;
    int i;
    for (i = 0; i < 16; i++)
        w[i] = ((uint32_t)p[i*4] << 24) | ((uint32_t)p[i*4+1] << 16) |
               ((uint32_t)p[i*4+2] << 8) | (uint32_t)p[i*4+3];
    for (i = 16; i < 64; i++) {
        uint32_t s0 = ror32(w[i-15], 7) ^ ror32(w[i-15], 18) ^ (w[i-15] >> 3);
        uint32_t s1 = ror32(w[i-2], 17) ^ ror32(w[i-2], 19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a = c->h[0]; b = c->h[1]; c2 = c->h[2]; d = c->h[3];
    e = c->h[4]; f = c->h[5]; g = c->h[6]; h = c->h[7];
    for (i = 0; i < 64; i++) {
        uint32_t S1 = ror32(e, 6) ^ ror32(e, 11) ^ ror32(e, 25);
        uint32_t ch = (e & f) ^ ((~e) & g);
        uint32_t S0, mj;
        t1 = h + S1 + ch + K256[i] + w[i];
        S0 = ror32(a, 2) ^ ror32(a, 13) ^ ror32(a, 22);
        mj = (a & b) ^ (a & c2) ^ (b & c2);
        t2 = S0 + mj;
        h = g; g = f; f = e; e = d + t1;
        d = c2; c2 = b; b = a; a = t1 + t2;
    }
    c->h[0] += a; c->h[1] += b; c->h[2] += c2; c->h[3] += d;
    c->h[4] += e; c->h[5] += f; c->h[6] += g; c->h[7] += h;
}

static void sha256_init(sha256_ctx *c) {
    c->h[0] = 0x6a09e667u; c->h[1] = 0xbb67ae85u; c->h[2] = 0x3c6ef372u;
    c->h[3] = 0xa54ff53au; c->h[4] = 0x510e527fu; c->h[5] = 0x9b05688cu;
    c->h[6] = 0x1f83d9abu; c->h[7] = 0x5be0cd19u;
    c->total = 0; c->buf_len = 0;
}

static void sha256_update(sha256_ctx *c, const void *data, size_t n) {
    const uint8_t *p = (const uint8_t *)data;
    c->total += (uint64_t)n;
    while (n > 0) {
        size_t take = 64 - c->buf_len;
        if (take > n) take = n;
        memcpy(c->buf + c->buf_len, p, take);
        c->buf_len += take; p += take; n -= take;
        if (c->buf_len == 64) { sha256_compress(c, c->buf); c->buf_len = 0; }
    }
}

static void sha256_hex(sha256_ctx *c, char out[65]) {
    static const char *hexd = "0123456789abcdef";
    uint8_t pad = 0x80, zero = 0, tail[8];
    uint64_t bits = c->total * 8;
    int i, j;
    sha256_update(c, &pad, 1);
    while (c->buf_len != 56) sha256_update(c, &zero, 1);
    for (i = 0; i < 8; i++) tail[i] = (uint8_t)(bits >> (56 - 8*i));
    sha256_update(c, tail, 8);
    for (i = 0; i < 8; i++)
        for (j = 0; j < 4; j++) {
            uint32_t v = (c->h[i] >> (24 - 8*j)) & 0xffu;
            out[i*8 + j*2]     = hexd[v >> 4];
            out[i*8 + j*2 + 1] = hexd[v & 15u];
        }
    out[64] = 0;
}

static void sha256_hexof(const void *data, size_t n, char out[65]) {
    sha256_ctx c;
    sha256_init(&c);
    sha256_update(&c, data, n);
    sha256_hex(&c, out);
}

/* ==================== bcrypt IAT call counter ========================= */

typedef NTSTATUS (WINAPI *PFN_BCryptOpenAlgorithmProvider)(BCRYPT_ALG_HANDLE *, LPCWSTR, LPCWSTR, ULONG);
typedef NTSTATUS (WINAPI *PFN_BCryptGenRandom)(BCRYPT_ALG_HANDLE, PUCHAR, ULONG, ULONG);
typedef NTSTATUS (WINAPI *PFN_BCryptCloseAlgorithmProvider)(BCRYPT_ALG_HANDLE, ULONG);

static volatile LONG g_genrandom_calls = 0;
static PFN_BCryptGenRandom g_genrandom_orig = NULL;
static PFN_BCryptGenRandom *g_genrandom_slot = NULL;   /* &IAT entry */

/* resolved once by bcrypt_runtime_probe(), reused by the positive control */
static HMODULE g_bcrypt_mod = NULL;
static PFN_BCryptOpenAlgorithmProvider g_open_fn = NULL;
static PFN_BCryptGenRandom g_gen_fn = NULL;
static PFN_BCryptCloseAlgorithmProvider g_close_fn = NULL;

static NTSTATUS WINAPI counted_genrandom(BCRYPT_ALG_HANDLE hAlg, PUCHAR pbOut,
                                        ULONG cbOut, ULONG dwFlags) {
    InterlockedIncrement(&g_genrandom_calls);
    return g_genrandom_orig(hAlg, pbOut, cbOut, dwFlags);
}

static int ascii_ieq(const char *a, const char *b) {
    while (*a && *b) {
        char x = *a++, y = *b++;
        if (x >= 'A' && x <= 'Z') x = (char)(x - 'A' + 'a');
        if (y >= 'A' && y <= 'Z') y = (char)(y - 'A' + 'a');
        if (x != y) return 0;
    }
    return *a == 0 && *b == 0;
}

/* Patch this module's import-table slot for BCryptGenRandom so calls made by
 * the LINKED archive (FFmpeg av_get_random_seed) are counted. Returns 1 when
 * installed, negative on a specific failure. */
static int install_bcrypt_iat_counter(void) {
    HMODULE base = GetModuleHandleA(NULL);
    IMAGE_DOS_HEADER *dos;
    IMAGE_NT_HEADERS *nt;
    IMAGE_DATA_DIRECTORY dd;
    IMAGE_IMPORT_DESCRIPTOR *imp;

    if (!base) return -1;
    dos = (IMAGE_DOS_HEADER *)base;
    if (dos->e_magic != IMAGE_DOS_SIGNATURE) return -2;
    nt = (IMAGE_NT_HEADERS *)((BYTE *)base + dos->e_lfanew);
    if (nt->Signature != IMAGE_NT_SIGNATURE) return -3;
    dd = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (dd.VirtualAddress == 0 || dd.Size == 0) return -4;
    imp = (IMAGE_IMPORT_DESCRIPTOR *)((BYTE *)base + dd.VirtualAddress);
    for (; imp->Name != 0; imp++) {
        const char *dll = (const char *)((BYTE *)base + imp->Name);
        IMAGE_THUNK_DATA *oft, *ft;
        if (!ascii_ieq(dll, "bcrypt.dll")) continue;
        if (imp->OriginalFirstThunk == 0) return -5;   /* names needed */
        oft = (IMAGE_THUNK_DATA *)((BYTE *)base + imp->OriginalFirstThunk);
        ft  = (IMAGE_THUNK_DATA *)((BYTE *)base + imp->FirstThunk);
        for (; oft->u1.AddressOfData != 0; oft++, ft++) {
            IMAGE_IMPORT_BY_NAME *ibn;
            void **slot;
            DWORD old_prot = 0;
            if (IMAGE_SNAP_BY_ORDINAL64(oft->u1.Ordinal)) continue;
            ibn = (IMAGE_IMPORT_BY_NAME *)((BYTE *)base + oft->u1.AddressOfData);
            if (strcmp((const char *)ibn->Name, "BCryptGenRandom") != 0) continue;
            slot = (void **)&ft->u1.Function;
            if (!VirtualProtect(slot, sizeof(void *), PAGE_READWRITE, &old_prot)) return -6;
            g_genrandom_orig = (PFN_BCryptGenRandom)(*slot);
            g_genrandom_slot = (PFN_BCryptGenRandom *)slot;
            *slot = (void *)&counted_genrandom;
            VirtualProtect(slot, sizeof(void *), old_prot, &old_prot);
            return 1;
        }
    }
    return -7;   /* bcrypt.dll not imported at all */
}

/* ==================== bcrypt runtime resolution probe ================= */

static int bcrypt_runtime_probe(void) {
    HMODULE mod;
    PFN_BCryptOpenAlgorithmProvider open_fn;
    PFN_BCryptGenRandom gen_fn;
    PFN_BCryptCloseAlgorithmProvider close_fn;
    BCRYPT_ALG_HANDLE alg = NULL;
    NTSTATUS s_open, s_gen, s_close;
    unsigned char rnd[32];
    char hex[65];

    mod = LoadLibraryA("bcrypt.dll");
    if (!mod) {
        printf("bcrypt_runtime: LoadLibraryA=FAILED err=%lu\n", (unsigned long)GetLastError());
        return -1;
    }
    open_fn = (PFN_BCryptOpenAlgorithmProvider)GetProcAddress(mod, "BCryptOpenAlgorithmProvider");
    gen_fn   = (PFN_BCryptGenRandom)GetProcAddress(mod, "BCryptGenRandom");
    close_fn = (PFN_BCryptCloseAlgorithmProvider)GetProcAddress(mod, "BCryptCloseAlgorithmProvider");
    g_bcrypt_mod = mod; g_open_fn = open_fn; g_gen_fn = gen_fn; g_close_fn = close_fn;
    printf("bcrypt_runtime: loadlibrary=ok module=%p open_provider=%p gen_random=%p close_provider=%p\n",
           (void *)mod, (void *)open_fn, (void *)gen_fn, (void *)close_fn);
    if (!open_fn || !gen_fn || !close_fn) return -2;

    /* Call shape copied from the pinned source the archive was built from:
     * libavutil/random_seed.c:165-169 (ffmpeg n9.0.1,
     * BCryptOpenAlgorithmProvider(&h, BCRYPT_RNG_ALGORITHM,
     * MS_PRIMITIVE_PROVIDER, 0) -> BCryptGenRandom(h, buf, len, 0) ->
     * BCryptCloseAlgorithmProvider(h, 0)). That is the exact sequence the
     * shipped archive performs when av_get_random_seed() runs. */
    s_open  = open_fn(&alg, BCRYPT_RNG_ALGORITHM, MS_PRIMITIVE_PROVIDER, 0);
    memset(rnd, 0, sizeof rnd);
    s_gen   = (s_open == 0) ? gen_fn(alg, rnd, (ULONG)sizeof rnd, 0)
                            : (NTSTATUS)0x80000000;
    s_close = (s_open == 0) ? close_fn(alg, 0) : (NTSTATUS)0x80000000;
    sha256_hexof(rnd, sizeof rnd, hex);
    printf("bcrypt_runtime: open_rng=0x%08lx gen_random=0x%08lx close=0x%08lx sample32_sha256=%s\n",
           (unsigned long)s_open, (unsigned long)s_gen, (unsigned long)s_close, hex);
    return (s_open == 0 && s_gen == 0 && s_close == 0) ? 0 : -3;
}

/* Positive control for the IAT instrument: call the patched import slot once
 * and require the counter to move by exactly 1. Without this, a zero count
 * during decode could equally mean "the hook is broken". */
static int bcrypt_iat_positive_control(void) {
    BCRYPT_ALG_HANDLE alg = NULL;
    NTSTATUS s_open, s_call, s_close;
    unsigned char buf[8];
    LONG before, after;
    PFN_BCryptGenRandom through_slot;

    if (!g_genrandom_slot || !g_open_fn || !g_close_fn) return -1;
    through_slot = *g_genrandom_slot;                 /* the patched entry */
    before = InterlockedCompareExchange(&g_genrandom_calls, 0, 0);
    s_open  = g_open_fn(&alg, BCRYPT_RNG_ALGORITHM, MS_PRIMITIVE_PROVIDER, 0);
    s_call  = (s_open == 0) ? through_slot(alg, buf, (ULONG)sizeof buf, 0)
                            : (NTSTATUS)0x80000000;
    s_close = (s_open == 0) ? g_close_fn(alg, 0) : (NTSTATUS)0x80000000;
    after = InterlockedCompareExchange(&g_genrandom_calls, 0, 0);
    printf("bcrypt_iat_positive_control: slot=%p entry=%p open=0x%08lx call_via_slot=0x%08lx "
           "close=0x%08lx calls_before=%ld calls_after=%ld delta=%ld expect=1 %s\n",
           (void *)g_genrandom_slot, (void *)through_slot,
           (unsigned long)s_open, (unsigned long)s_call, (unsigned long)s_close,
           (long)before, (long)after, (long)(after - before),
           (after - before) == 1 ? "PASS" : "FAIL");
    return (after - before) == 1 ? 0 : -2;
}

/* ==================== host IO callbacks =============================== */

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

/* ==================== helper modes ==================================== */

static int selftest(void) {
    /* RFC 6234 / FIPS 180-4 known-answer vectors */
    static const struct { const char *in; const char *want; } v[] = {
        { "", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" },
        { "abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" },
        { "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
          "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1" },
    };
    size_t i; int bad = 0;
    for (i = 0; i < sizeof v / sizeof v[0]; i++) {
        char got[65];
        sha256_hexof(v[i].in, strlen(v[i].in), got);
        printf("sha256_selftest[%zu] %s len=%zu got=%s want=%s\n",
               i, strcmp(got, v[i].want) == 0 ? "ok" : "MISMATCH",
               strlen(v[i].in), got, v[i].want);
        if (strcmp(got, v[i].want) != 0) bad = 1;
    }
    printf("SHA256_SELFTEST=%s\n", bad ? "FAIL" : "PASS");
    return bad;
}

static int sha256_file_mode(const char *path) {
    FILE *f = fopen(path, "rb");
    unsigned char buf[65536];
    sha256_ctx c;
    char hex[65];
    uint64_t total = 0;
    size_t n;
    if (!f) { fprintf(stderr, "cannot open %s\n", path); return 2; }
    sha256_init(&c);
    while ((n = fread(buf, 1, sizeof buf, f)) > 0) {
        sha256_update(&c, buf, n);
        total += n;
    }
    fclose(f);
    sha256_hex(&c, hex);
    printf("file=%s bytes=%" PRIu64 " sha256=%s\n", path, total, hex);
    return 0;
}

/* ==================== main walk ======================================= */

#define BLOCK_FRAMES 4096u

int main(int argc, char **argv) {
    const char *path;
    FILE *f;
    song_io io;
    song_handle *h = NULL;
    song_status st;
    song_info info;
    uint32_t nstreams = 0, i, meta_count = 0, art_count = 0;
    const song_metadata *meta = NULL;
    const song_error *err = NULL;
    float *dst;
    sha256_ctx full, post;
    uint64_t frames = 0, frames_post = 0;
    char hex[65], hex_post[65];
    int64_t landing = -2;
    const char *terminal = "unknown";
    long calls_decode;
    int hook_state;

    for (i = 1; i < (uint32_t)argc; i++) printf("consumer_argv%d=%s\n", i, argv[i]);
    if (argc == 2 && strcmp(argv[1], "--selftest") == 0) return selftest();
    if (argc == 3 && strcmp(argv[1], "--sha256-file") == 0) return sha256_file_mode(argv[2]);
    if (argc != 2) {
        fprintf(stderr, "usage: %s <media-file> | --selftest | --sha256-file <path>\n", argv[0]);
        return 2;
    }
    path = argv[1];

    printf("host_os=windows_pe\n");
    printf("abi_version=%lu\n", (unsigned long)songcore_abi_version());

    /* Runtime dependency observation #1 */
    printf("bcrypt_runtime_probe_rc=%d\n", bcrypt_runtime_probe());

    /* Runtime dependency observation #2 */
    hook_state = install_bcrypt_iat_counter();
    printf("bcrypt_iat_counter: state=%d slot=%p orig=%p\n",
           hook_state, (void *)g_genrandom_slot, (void *)g_genrandom_orig);
    InterlockedExchange(&g_genrandom_calls, 0);
    printf("bcrypt_iat_positive_control_rc=%d\n", bcrypt_iat_positive_control());
    InterlockedExchange(&g_genrandom_calls, 0);

    f = fopen(path, "rb");
    if (!f) { printf("RESULT=FAIL stage=open_media\n"); return 2; }
    io.userdata = f; io.read = host_read; io.seek = host_seek; io.size = host_size;
    printf("media_path=%s media_size=%" PRId64 "\n", path, host_size(f));

    /* ---- open ---- */
    st = song_open(&io, &h);
    printf("stage=open status=%d\n", (int)st);
    if (st != SONG_OK) { printf("RESULT=FAIL stage=open status=%d\n", (int)st); fclose(f); return 1; }

    /* ---- probe ---- */
    memset(&info, 0, sizeof info);
    st = song_probe(h, &info);
    printf("stage=probe status=%d codec=%s container=%s rate=%d channels=%d "
           "channel_mask=0x%016" PRIx64 " bits=%d duration_us=%" PRId64
           " streams=%u selected=%u\n",
           (int)st, info.codec, info.container, info.sample_rate, info.channels,
           info.channel_mask, info.bits_per_sample, info.duration_us,
           info.audio_stream_count, info.selected_audio_index);
    if (st != SONG_OK) { printf("RESULT=FAIL stage=probe status=%d\n", (int)st);
                         song_close(h); fclose(f); return 1; }

    /* ---- streams ---- */
    st = song_audio_stream_count(h, &nstreams);
    printf("stage=audio_stream_count status=%d count=%u\n", (int)st, nstreams);
    if (st != SONG_OK) { printf("RESULT=FAIL stage=audio_stream_count status=%d\n", (int)st);
                         song_close(h); fclose(f); return 1; }
    for (i = 0; i < nstreams; i++) {
        song_stream_info si;
        memset(&si, 0, sizeof si);
        st = song_audio_stream_info(h, i, &si);
        printf("stream[%u] status=%d stream_index=%u rate=%d channels=%d mask=0x%016" PRIx64
               " bits=%d duration_us=%" PRId64 " codec=%s default=%u\n",
               i, (int)st, si.stream_index, si.sample_rate, si.channels, si.channel_mask,
               si.bits_per_sample, si.duration_us, si.codec, si.is_default);
        if (st != SONG_OK) { printf("RESULT=FAIL stage=audio_stream_info status=%d\n", (int)st);
                             song_close(h); fclose(f); return 1; }
    }

    /* ---- metadata ---- */
    st = song_get_metadata(h, &meta);
    if (st != SONG_OK || !meta) { printf("RESULT=FAIL stage=get_metadata status=%d\n", (int)st);
                                  song_close(h); fclose(f); return 1; }
    printf("stage=metadata status=%d title=%s artist=%s album=%s genre=%s date=%s "
           "track_number=%d has_track=%u\n",
           (int)st,
           meta->has_title ? meta->title : "(absent)",
           meta->has_artist ? meta->artist : "(absent)",
           meta->has_album ? meta->album : "(absent)",
           meta->has_genre ? meta->genre : "(absent)",
           meta->has_date ? meta->date : "(absent)",
           (int)meta->track_number, meta->has_track_number);
    st = song_get_metadata_count(h, &meta_count);
    printf("stage=metadata_count status=%d count=%u\n", (int)st, meta_count);
    if (st != SONG_OK) { printf("RESULT=FAIL stage=metadata_count status=%d\n", (int)st);
                         song_close(h); fclose(f); return 1; }
    for (i = 0; i < meta_count && i < 12; i++) {
        song_metadata_entry e;
        memset(&e, 0, sizeof e);
        st = song_get_metadata_entry(h, i, &e);
        printf("meta[%u] status=%d scope=%u key=%.*s value=%.*s\n", i, (int)st, e.scope,
               (int)(e.key ? e.key_len : 0), e.key ? e.key : "",
               (int)(e.value ? e.value_len : 0), e.value ? e.value : "");
        if (st != SONG_OK) break;
    }

    /* ---- artwork ---- */
    st = song_get_artwork_count(h, &art_count);
    printf("stage=artwork_count status=%d count=%u\n", (int)st, art_count);
    if (st != SONG_OK) { printf("RESULT=FAIL stage=artwork_count status=%d\n", (int)st);
                         song_close(h); fclose(f); return 1; }
    for (i = 0; i < art_count && i < 4; i++) {
        song_artwork_item a;
        memset(&a, 0, sizeof a);
        st = song_get_artwork_item(h, i, &a);
        printf("artwork[%u] status=%d role=%u mime=%.*s bytes=%" PRIu64 " w=%d h=%d front=%u\n",
               i, (int)st, a.role, (int)(a.mime ? a.mime_len : 0), a.mime ? a.mime : "",
               a.data_len, a.width, a.height, a.is_front_cover);
        if (st != SONG_OK) break;
    }

    /* ---- full decode, PCM sha256 ---- */
    InterlockedExchange(&g_genrandom_calls, 0);
    dst = (float *)malloc(sizeof(float) * BLOCK_FRAMES * (uint64_t)info.channels);
    if (!dst) { printf("RESULT=FAIL stage=alloc\n"); song_close(h); fclose(f); return 1; }
    sha256_init(&full);
    for (;;) {
        uint64_t got = 0;
        st = song_read_pcm(h, dst, BLOCK_FRAMES, &got);
        if (st == SONG_EOF) {
            if (got == 0) { terminal = "SONG_EOF"; break; }
            sha256_update(&full, dst, sizeof(float) * got * (uint64_t)info.channels);
            frames += got;
            continue;
        }
        if (st != SONG_OK) {
            printf("stage=decode status=%d frames=%" PRIu64 "\n", (int)st, frames);
            free(dst); song_close(h); fclose(f);
            printf("RESULT=FAIL stage=decode status=%d\n", (int)st);
            return 1;
        }
        if (got == 0) { terminal = "SONG_OK_ZERO"; break; }
        sha256_update(&full, dst, sizeof(float) * got * (uint64_t)info.channels);
        frames += got;
    }
    sha256_hex(&full, hex);
    calls_decode = InterlockedCompareExchange(&g_genrandom_calls, 0, 0);
    printf("stage=decode status=0 frames=%" PRIu64 " pcm_sha256=%s terminal=%s "
           "bytes_hashed=%" PRIu64 " block_frames=4096\n",
           frames, hex, terminal,
           (uint64_t)sizeof(float) * frames * (uint64_t)info.channels);
    printf("bcrypt_genrandom_iat_calls_open_to_decode_eof=%ld\n", calls_decode);

    /* ---- seek + post-seek decode to EOF ---- */
    if (info.duration_us > 0) {
        int64_t target = info.duration_us / 2;
        sha256_init(&post);
        st = song_seek(h, target, &landing);
        printf("stage=seek status=%d target_us=%" PRId64 " landing_us=%" PRId64 "\n",
               (int)st, target, landing);
        if (st != SONG_OK) { free(dst); song_close(h); fclose(f);
                             printf("RESULT=FAIL stage=seek status=%d\n", (int)st); return 1; }
        for (;;) {
            uint64_t g2 = 0;
            st = song_read_pcm(h, dst, BLOCK_FRAMES, &g2);
            if (st == SONG_EOF) {
                if (g2 != 0) {
                    sha256_update(&post, dst, sizeof(float) * g2 * (uint64_t)info.channels);
                    frames_post += g2;
                    continue;
                }
                break;
            }
            if (st != SONG_OK) {
                printf("stage=post_seek_decode status=%d\n", (int)st);
                free(dst); song_close(h); fclose(f);
                printf("RESULT=FAIL stage=post_seek_decode status=%d\n", (int)st);
                return 1;
            }
            if (g2 == 0) break;
            sha256_update(&post, dst, sizeof(float) * g2 * (uint64_t)info.channels);
            frames_post += g2;
        }
        sha256_hex(&post, hex_post);
        printf("stage=post_seek_decode status=0 frames=%" PRIu64 " pcm_sha256=%s terminal=SONG_EOF\n",
               frames_post, hex_post);
        if (frames_post == 0) { free(dst); song_close(h); fclose(f);
                                printf("RESULT=FAIL stage=post_seek_empty\n"); return 1; }
    } else {
        printf("stage=seek status=skipped reason=duration_unknown\n");
    }

    /* ---- last_error on a healthy handle ---- */
    st = song_last_error(h, &err);
    printf("stage=last_error status=%d has_message=%d\n", (int)st, (err && err->message) ? 1 : 0);

    /* ---- close ---- */
    free(dst);
    song_close(h);
    fclose(f);
    printf("stage=close status=ok\n");
    printf("bcrypt_genrandom_iat_calls_total=%ld\n", (long)InterlockedCompareExchange(&g_genrandom_calls, 0, 0));
    printf("RESULT=PASS\n");
    return 0;
}
