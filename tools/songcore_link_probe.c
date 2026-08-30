/*
 * songcore_link_probe — link-reachability fixture, NOT a product tool.
 *
 * It exists so the S1 minimization audit can force the linker to resolve the
 * ENTIRE SongCore public contract (open/probe/read_pcm/seek/close) against
 * libqianqian_av.a, independent of whatever subset qn_pcm_dump happens to
 * call on its current code path. Taking the address of every entry point
 * through a volatile table prevents the compiler from folding them away.
 *
 * It must stay free of FFmpeg types: it only pins the boundary contract.
 */
#include "songcore.h"

#include <stdio.h>

typedef song_handle *(*song_open_fn)(const song_io *);
typedef int (*song_probe_fn)(song_handle *, song_info *);
typedef int64_t (*song_read_pcm_fn)(song_handle *, float *, size_t);
typedef int (*song_seek_api_fn)(song_handle *, int64_t);
typedef void (*song_close_fn)(song_handle *);

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <local-song>\n", argv[0]);
        return 2;
    }

    /* Contract pin: every public entry point must be resolvable at link
     * time, whether or not this host exercises each one at runtime. */
    static void *const contract[] = {
        (void *)&song_open,
        (void *)&song_probe,
        (void *)&song_read_pcm,
        (void *)&song_seek,
        (void *)&song_close,
    };
    void *const *pin = contract;
    if (pin[0] == NULL) return 3;
    volatile const void *const pinned = pin[0];
    (void)pinned;

    FILE *f = fopen(argv[1], "rb");
    if (!f) return 1;
    /* The decode path is intentionally NOT run here; reachability only needs
     * symbol-level resolution against the full contract. Decode / seek / EOF
     * behavior is owned by the real gates (corpus, PCM, seek, real songs). */
    static song_io io;
    song_handle *h = ((song_open_fn)contract[0])(&io);
    if (h) ((song_close_fn)contract[4])(h);
    fclose(f);
    return 0;
}
