/* D1 differential probe — what a caller actually observes in *out_handle.
 *
 * Read-only evidence harness for Issue #172 D1. It links the real
 * libsongcore.a built from source SHA 69ba1ce and records, per failure
 * path, the value left in the caller's out_handle slot.
 *
 * Path taxonomy:
 *   (a) failures BEFORE the argument guard writes anything
 *       -> songcore_ffmpeg.c:823-824 return SONG_ERR_INVALID_ARGUMENT
 *   (b) failures after the guard and BEFORE the NULL deposit
 *       -> structurally empty: 824 is a return, 825 is the deposit
 *   (c) failures AFTER the NULL deposit
 *       -> songcore_ffmpeg.c:825 *out_handle = NULL, then every failure
 *
 * The probe asserts only what the implementation is claimed to do; a
 * mismatch is a failed claim, not a failed contract.
 */
#include <songcore.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define SENTINEL ((song_handle *)(uintptr_t)0x5E27E1CAFEULL)
#define SENTINEL_NAME "SENTINEL(0x5E27E1CAFE)"

static int g_fail;

/* Link-time injection of the FIRST allocation inside song_open
 * (songcore_ffmpeg.c:829 calloc). Built with -Wl,--wrap=calloc, so the
 * archive's calloc reference resolves here while the rest of the program
 * keeps the real allocator. */
static int g_fail_calloc;
void *__real_calloc(size_t n, size_t sz);
void *__wrap_calloc(size_t n, size_t sz) {
    if (g_fail_calloc) return NULL;
    return __real_calloc(n, sz);
}

static const char *name_of(song_handle *h) {
    if (h == SENTINEL) return SENTINEL_NAME;
    if (h == NULL) return "NULL";
    return "OTHER";
}

static void report(const char *case_id, const char *path_class,
                   song_status st, song_handle *h, const char *expect_desc,
                   int ok) {
    printf("%-28s path=%s status=%-3d out_handle=%-22s expect=%s -> %s\n",
           case_id, path_class, (int)st, name_of(h), expect_desc,
           ok ? "MATCH" : "MISMATCH");
    if (!ok) g_fail++;
}

/* ---- host IO over a memory buffer ------------------------------------- */

typedef struct {
    const unsigned char *data;
    size_t len;
    size_t pos;
    int fail_read; /* when set, every read reports a host error (<0) */
} mem_src;

static int64_t m_read(void *ud, uint8_t *dst, size_t size) {
    mem_src *s = (mem_src *)ud;
    if (s->fail_read) return -1;
    size_t n = s->len - s->pos;
    if (n > size) n = size;
    if (n == 0) return 0;
    memcpy(dst, s->data + s->pos, n);
    s->pos += n;
    return (int64_t)n;
}

static int64_t m_seek(void *ud, int64_t off) {
    mem_src *s = (mem_src *)ud;
    if (off < 0 || (uint64_t)off > s->len) return -1;
    s->pos = (size_t)off;
    return off;
}

static int64_t m_size(void *ud) { return (int64_t)((mem_src *)ud)->len; }

int main(int argc, char **argv) {
    const char *fixture = NULL;
    for (int i = 1; i < argc; ++i)
        if (strcmp(argv[i], "--fixture") == 0 && i + 1 < argc)
            fixture = argv[++i];

    printf("abi_version=%u\n", songcore_abi_version());

    /* 4 KiB of non-media bytes: the container probe must refuse it. */
    static unsigned char garbage[4096];
    for (size_t i = 0; i < sizeof(garbage); ++i)
        garbage[i] = (unsigned char)((i * 131u + 17u) & 0xffu);

    mem_src junk = {garbage, sizeof(garbage), 0, 0};
    mem_src badio = {garbage, sizeof(garbage), 0, 1}; /* read() -> -1 */
    song_io io_junk = {&junk, m_read, m_seek, m_size};
    song_io io_badio = {&badio, m_read, m_seek, m_size};
    song_io io_no_read = {&junk, NULL, m_seek, m_size};
    song_io io_no_seek = {&junk, m_read, NULL, m_size};
    song_io io_no_size = {&junk, m_read, m_seek, NULL};

    /* ================= (a) pre-guard: nothing written ================= */
    {
        song_handle *h = SENTINEL;
        song_status st = song_open(NULL, &h);
        report("a1-null-io", "a", st, h, "100 + untouched",
               st == SONG_ERR_INVALID_ARGUMENT && h == SENTINEL);
    }
    {
        song_status st = song_open(&io_junk, NULL); /* must not crash */
        report("a2-null-out-handle", "a", st, NULL, "100 + no crash",
               st == SONG_ERR_INVALID_ARGUMENT);
    }
    {
        song_handle *h = SENTINEL;
        song_status st = song_open(&io_no_read, &h);
        report("a3-missing-read", "a", st, h, "100 + untouched",
               st == SONG_ERR_INVALID_ARGUMENT && h == SENTINEL);
    }
    {
        song_handle *h = SENTINEL;
        song_status st = song_open(&io_no_seek, &h);
        report("a4-missing-seek", "a", st, h, "100 + untouched",
               st == SONG_ERR_INVALID_ARGUMENT && h == SENTINEL);
    }
    {
        song_handle *h = SENTINEL;
        song_status st = song_open(&io_no_size, &h);
        report("a5-missing-size", "a", st, h, "100 + untouched",
               st == SONG_ERR_INVALID_ARGUMENT && h == SENTINEL);
    }

    /* ================= (c) post-guard: NULL deposited ================= */
    {
        song_handle *h = SENTINEL;
        song_status st = song_open(&io_junk, &h);
        report("c1-garbage-container", "c", st, h, "104 + NULL (clobbered)",
               st == SONG_ERR_UNSUPPORTED_CONTAINER && h == NULL);
    }
    {
        song_handle *h = SENTINEL;
        song_status st = song_open(&io_badio, &h);
        /* status recorded verbatim; the D1 observable is the slot value */
        report("c2-host-io-error", "c", st, h, "any typed failure + NULL",
               st != SONG_OK && h == NULL);
    }
    {
        /* Make the first allocation (songcore_ffmpeg.c:829) fail. */
        song_handle *h = SENTINEL;
        g_fail_calloc = 1;
        song_status st = song_open(&io_junk, &h);
        g_fail_calloc = 0; /* before any stdio */
        report("c3-out-of-memory", "c", st, h, "112 + NULL (clobbered)",
               st == SONG_ERR_OUT_OF_MEMORY && h == NULL);
    }

    /* ===== counterfactual: a live prior handle lost by a failed re-open ===== */
    if (!fixture) {
        printf("%-28s SKIPPED (no --fixture)\n", "d1-prior-handle-reopen");
    } else {
        FILE *f = fopen(fixture, "rb");
        if (!f) {
            printf("%-28s SKIPPED (fixture not readable: %s)\n",
                   "d1-prior-handle-reopen", fixture);
        } else {
            fseek(f, 0, SEEK_END);
            long n = ftell(f);
            fseek(f, 0, SEEK_SET);
            unsigned char *buf = (unsigned char *)malloc((size_t)(n > 0 ? n : 0));
            size_t got = buf ? fread(buf, 1, (size_t)n, f) : 0;
            fclose(f);
            mem_src src = {buf, got, 0, 0};
            song_io io_fix = {&src, m_read, m_seek, m_size};

            song_handle *h = NULL;
            song_status st_open = song_open(&io_fix, &h);
            printf("%-28s status=%-3d out_handle=%s\n",
                   "d1-open-real-fixture", (int)st_open, name_of(h));

            if (st_open == SONG_OK && h != NULL) {
                song_handle *prior = h; /* the caller's only copy of the live handle */
                junk.pos = 0;
                song_status st_re = song_open(&io_junk, &h);
                printf("%-28s status=%-3d slot_after=%s prior_copy=%s\n",
                       "d1-failed-reopen", (int)st_re, name_of(h),
                       prior == NULL ? "NULL" : "live");
                report("d1-slot-clobbered", "c", st_re, h,
                       "failed re-open leaves NULL, not prior",
                       st_re != SONG_OK && h == NULL);
                /* Is the prior handle still alive after its slot was lost? */
                song_info info;
                memset(&info, 0, sizeof(info));
                song_status st_prior = song_probe(prior, &info);
                printf("%-28s prior_handle_probe_status=%d (alive=%d)"
                       " sr=%d ch=%d\n",
                       "d1-prior-still-valid", (int)st_prior,
                       st_prior == SONG_OK, (int)info.sample_rate,
                       (int)info.channels);
                report("d1-prior-alive-but-lost", "c", st_re, h,
                       "prior handle alive yet unreachable",
                       st_prior == SONG_OK);
                song_close(prior);
            } else {
                printf("%-28s SKIPPED (fixture did not open: %d)\n",
                       "d1-prior-handle-reopen", (int)st_open);
            }
            free(buf);
        }
    }

    printf("D1_PROBE_FAILURES=%d\n", g_fail);
    return g_fail == 0 ? 0 : 1;
}
