/*
 * external-consumer-example.c — SongCore package consumer smoke (ABI v1).
 *
 * A self-contained external consumer: it uses only this package's
 * include/songcore.h and lib/ plus the C standard library, and walks the
 * minimal real path  open → probe → decode → seek → EOF → close.
 *
 * Build (static, contract §9 linux link line):
 *   cc -O2 -Iinclude external-consumer-example.c \
 *      -Llib -lsongcore -lm -lpthread -o songcore-consumer-example
 *
 * Build (Windows static, contract §9 link line — -lbcrypt is REQUIRED
 * there: the archive's Win32 entropy path imports BCrypt*
 * (BCryptOpenAlgorithmProvider/GenRandom/CloseAlgorithmProvider, audited
 * per contract §9); observed in the #325 consumer smoke and printed in
 * the package README. Evidence added by the #330 P3 disposition:
 * without it a Windows consumer following this example fails to link):
 *   gcc -O2 -Iinclude external-consumer-example.c \
 *      -Llib -lsongcore -lbcrypt -o songcore-consumer-example.exe
 *
 * Build (shared):
 *   cc -O2 -Iinclude external-consumer-example.c \
 *      -Llib/shared -lsongcore -lm -lpthread \
 *      -Wl,-rpath,$ORIGIN -o songcore-consumer-example-shared
 *   (run from lib/shared/, or adjust the rpath to the .so location)
 *
 * Run:
 *   ./songcore-consumer-example <media-file>
 *
 * Exit 0 = the smoke passed; any stage failure exits 1 with a typed status.
 * The consumer supplies its own media file; the package ships no media.
 *
 * Evidence note (2026-10-30, #330 P3): the Windows build line above was
 * added to this packaging template. The S3 verification receipts
 * evidence/songcore-172/linux/verify/sha256-manifest.txt:8 and
 * evidence/songcore-172/linux/verify/verify-copy-sha256-win.txt:8 bind the
 * pre-edit bytes (sha256
 * d14547312c12e07fdaf1e826ed2236d3694808a5ba233181c07c6dbb71ce1552) and
 * remain records of the verification performed then; the archived RC
 * container's packaged copy is likewise unchanged (frozen zip sha256
 * 7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415).
 * Future packs copy this updated template (ops/package-rc.ps1
 * $ConsumerExample default).
 */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "songcore.h"

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
    long cur = ftell(f);
    if (fseek(f, 0, SEEK_END) != 0) return -1;
    long end = ftell(f);
    fseek(f, cur, SEEK_SET);
    return (int64_t)end;
}

static int fail(const char *stage, song_status st) {
    fprintf(stderr, "external-consumer-example: FAIL at %s (status %d)\n", stage, (int)st);
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <media-file>\n", argv[0]);
        return 2;
    }
    if (songcore_abi_version() != 1u) {
        fprintf(stderr, "external-consumer-example: ABI version %u != 1\n",
                songcore_abi_version());
        return 1;
    }
    FILE *f = fopen(argv[1], "rb");
    if (!f) { fprintf(stderr, "cannot open %s\n", argv[1]); return 2; }
    song_io io = { f, host_read, host_seek, host_size };
    song_handle *h = NULL;
    song_status st = song_open(&io, &h);
    if (st != SONG_OK) return fail("open", st);

    song_info info;
    st = song_probe(h, &info);
    if (st != SONG_OK) return fail("probe", st);
    printf("probe: codec=%s container=%s rate=%d channels=%d duration_us=%" PRId64
           " streams=%u selected=%u\n",
           info.codec, info.container, info.sample_rate, info.channels,
           info.duration_us, info.audio_stream_count, info.selected_audio_index);

    uint32_t nstreams = 0;
    if (song_audio_stream_count(h, &nstreams) != SONG_OK)
        return fail("audio_stream_count", SONG_ERR_INVALID_ARGUMENT);
    for (uint32_t i = 0; i < nstreams; i++) {
        song_stream_info si;
        if (song_audio_stream_info(h, i, &si) != SONG_OK)
            return fail("audio_stream_info", SONG_ERR_INVALID_ARGUMENT);
        printf("stream[%u]: stream_index=%u rate=%d channels=%d default=%u\n",
               i, si.stream_index, si.sample_rate, si.channels, si.is_default);
    }

    const song_metadata *meta = NULL;
    if (song_get_metadata(h, &meta) != SONG_OK)
        return fail("get_metadata", SONG_ERR_INVALID_ARGUMENT);
    printf("metadata: title=%s artist=%s\n",
           meta->has_title ? meta->title : "(absent)",
           meta->has_artist ? meta->artist : "(absent)");

    float *dst = malloc(sizeof(float) * 4096u * (unsigned)info.channels);
    if (!dst) return fail("alloc", SONG_ERR_OUT_OF_MEMORY);
    uint64_t frames = 0;
    for (;;) {
        uint64_t got = 0;
        st = song_read_pcm(h, dst, 4096, &got);
        if (st == SONG_EOF) {
            if (got != 0) return fail("decode_eof_shape", st);
            break;
        }
        if (st != SONG_OK) return fail("decode", st);
        frames += got;
    }
    printf("decode: %" PRIu64 " frames to EOF\n", frames);

    if (info.duration_us > 0) {
        int64_t landing = -2;
        st = song_seek(h, info.duration_us / 2, &landing);
        if (st != SONG_OK) return fail("seek", st);
        printf("seek: landing_us=%" PRId64 " (target %" PRId64 ")\n",
               landing, info.duration_us / 2);
        uint64_t got = 0;
        st = song_read_pcm(h, dst, 4096, &got);
        if (st != SONG_OK || got == 0) return fail("post_seek_read", st);
        for (;;) {
            uint64_t g2 = 0;
            st = song_read_pcm(h, dst, 4096, &g2);
            if (st == SONG_EOF) {
                if (g2 != 0) return fail("eof_shape", st);
                break;
            }
            if (st != SONG_OK) return fail("post_seek_decode", st);
        }
        printf("eof: clean SONG_EOF terminal\n");
    }

    free(dst);
    song_close(h);
    fclose(f);
    printf("external-consumer-example: PASS\n");
    return 0;
}
