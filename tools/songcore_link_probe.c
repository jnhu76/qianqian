/*
 * songcore_link_probe — link-reachability fixture, NOT a product tool.
 *
 * It exists so the S1 minimization audit can force the linker to resolve the
 * ENTIRE SongCore public contract (open/probe/read_pcm/seek/close) against
 * libqianqian_av.a, independent of whatever subset qn_pcm_dump happens to
 * call on its current code path.
 *
 * The machine guarantee is external linkage: `songcore_contract` is visible
 * outside this translation unit, so the compiler must emit ALL five address
 * relocations (it may not elide single elements of an array whose address
 * escapes). The volatile read in main only preserves the runtime claim;
 * tools/link_audit.py additionally asserts with `nm -u` that probe.o's
 * undefined-symbol set is EXACTLY the five contract entry points.
 *
 * It must stay free of FFmpeg types and of libc calls: any extra undefined
 * reference would weaken the exact-set assertion.
 */
#include "songcore.h"

#define SONG_CONTRACT_N 5

void *const songcore_contract[SONG_CONTRACT_N] = {
    (void *)&song_open,
    (void *)&song_probe,
    (void *)&song_read_pcm,
    (void *)&song_seek,
    (void *)&song_close,
};

int main(void) {
    void *const volatile *pin = songcore_contract;
    for (int i = 0; i < SONG_CONTRACT_N; i++) {
        if (pin[i] == (void *)0) return 3;
    }
    return 0;
}
