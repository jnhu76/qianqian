/* D3 probe: does song_read_pcm honour songcore.h:358
 * ("other -> typed error; *out_frames_produced == 0 in that call")
 * on EVERY error path?
 *
 * Method: poison *out_frames_produced with UINT64_MAX before each call and
 * read it back. A compliant implementation leaves 0; a violating path
 * leaves the poison visible. A live-handle control call runs first so that
 * the dst==NULL / capacity==0 results cannot be explained by a dead handle.
 *
 * Read-only: this file calls the real shared library and makes no
 * assumption about its internals. */

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "songcore.h"

typedef struct {
    FILE *f;
} file_ctx;

static int64_t io_read(void *ud, uint8_t *dst, size_t size) {
    file_ctx *c = (file_ctx *)ud;
    size_t n = fread(dst, 1, size, c->f);
    if (n == 0 && ferror(c->f)) return -1;
    return (int64_t)n;
}

static int64_t io_seek(void *ud, int64_t absolute_offset) {
    file_ctx *c = (file_ctx *)ud;
    if (fseek(c->f, (long)absolute_offset, SEEK_SET) != 0) return -1;
    return (int64_t)ftell(c->f);
}

static int64_t io_size(void *ud) {
    file_ctx *c = (file_ctx *)ud;
    long cur = ftell(c->f);
    if (fseek(c->f, 0, SEEK_END) != 0) return -1;
    long end = ftell(c->f);
    if (cur >= 0) fseek(c->f, cur, SEEK_SET);
    return (int64_t)end;
}

#define POISON UINT64_MAX

static int report(const char *name, song_status st, uint64_t produced,
                  uint64_t capacity) {
    /* Header-derived invariant: any status that is neither SONG_OK nor
     * SONG_EOF is an error and owes produced == 0. */
    int is_error = (st != SONG_OK && st != SONG_EOF);
    int violated = is_error && produced != 0;
    printf("PROBE %-28s status=%3d produced=0x%016" PRIx64
           " (dec %" PRIu64 ") capacity=%" PRIu64 " -> %s\n",
           name, (int)st, produced, produced, capacity,
           is_error ? (violated ? "VIOLATION" : "zeroed") : "not-an-error");
    fflush(stdout);
    return violated;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <media-file>\n", argv[0]);
        return 2;
    }

    printf("ABI_VERSION %u\n", (unsigned)songcore_abi_version());

    file_ctx ctx;
    ctx.f = fopen(argv[1], "rb");
    if (!ctx.f) {
        fprintf(stderr, "cannot open %s\n", argv[1]);
        return 2;
    }

    song_io io;
    io.userdata = &ctx;
    io.read = io_read;
    io.seek = io_seek;
    io.size = io_size;

    song_handle *h = NULL;
    song_status ost = song_open(&io, &h);
    printf("song_open status=%d handle=%p\n", (int)ost, (void *)h);
    if (ost != SONG_OK || !h) return 2;

    song_info info;
    memset(&info, 0, sizeof info);
    song_status pst = song_probe(h, &info);
    printf("song_probe status=%d rate=%d channels=%d codec=%s container=%s\n",
           (int)pst, info.sample_rate, info.channels, info.codec,
           info.container);
    if (pst != SONG_OK) { song_close(h); return 2; }

    float *dst = (float *)malloc(4096 * sizeof(float));
    if (!dst) { song_close(h); return 2; }

    int violations = 0;

    /* Control: a genuinely live handle must produce frames, which proves the
     * handle used by the probes below is valid and probed. */
    uint64_t produced = POISON;
    song_status cst = song_read_pcm(h, dst, 256, &produced);
    printf("CONTROL valid-call status=%d produced=%" PRIu64 "\n", (int)cst,
           produced);
    if (cst != SONG_OK || produced == 0) {
        fprintf(stderr, "control call did not produce frames; probe aborted\n");
        song_close(h);
        return 2;
    }

    /* (1) NULL handle, valid out pointer. */
    produced = POISON;
    song_status s1 = song_read_pcm(NULL, dst, 1, &produced);
    violations += report("NULL handle", s1, produced, 1);

    /* (2) valid handle, NULL dst, valid out pointer. */
    produced = POISON;
    song_status s2 = song_read_pcm(h, NULL, 1, &produced);
    violations += report("NULL dst", s2, produced, 1);

    /* (3) valid handle, valid dst, frame_capacity == 0. */
    produced = POISON;
    song_status s3 = song_read_pcm(h, dst, 0, &produced);
    violations += report("capacity==0", s3, produced, 0);

    /* (4) dual-invalid: NULL dst AND capacity==0. */
    produced = POISON;
    song_status s4 = song_read_pcm(h, NULL, 0, &produced);
    violations += report("NULL dst + capacity==0", s4, produced, 0);

    /* (5) reference compliant error path, for contrast: unprobed handle-free
     *     path is not reachable here (our handle is probed), so instead use
     *     a NULL out pointer which cannot be observed at all. */

    free(dst);
    song_close(h);
    fclose(ctx.f);

    printf("VIOLATIONS %d\n", violations);
    return violations == 0 ? 0 : 1;
}
