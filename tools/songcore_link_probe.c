/*
 * songcore_link_probe — link-reachability fixture, NOT a product tool.
 *
 * It exists so the S1 minimization audit can force the linker to resolve the
 * ENTIRE SongCore public contract (SongCore ABI v1 entry points) against
 * libqianqian_av.a, independent of whatever subset a host happens to call on
 * its current code path.
 *
 * The machine guarantee is external linkage: `songcore_contract` is visible
 * outside this translation unit, so the compiler must emit ALL address
 * relocations (it may not elide single elements of an array whose address
 * escapes). The volatile read in main only preserves the runtime claim;
 * tools/link_audit.py additionally asserts with `nm -u` that probe.o's
 * undefined-symbol set is EXACTLY the contract entry points.
 *
 * It must stay free of FFmpeg types and of libc calls: any extra undefined
 * reference would weaken the exact-set assertion.
 */
#include "songcore.h"

#define SONG_CONTRACT_N 15

void *const songcore_contract[SONG_CONTRACT_N] = {
    (void *)&songcore_abi_version,
    (void *)&song_open,
    (void *)&song_probe,
    (void *)&song_audio_stream_count,
    (void *)&song_audio_stream_info,
    (void *)&song_select_stream,
    (void *)&song_get_metadata,
    (void *)&song_get_metadata_count,
    (void *)&song_get_metadata_entry,
    (void *)&song_get_artwork_count,
    (void *)&song_get_artwork_item,
    (void *)&song_read_pcm,
    (void *)&song_seek,
    (void *)&song_last_error,
    (void *)&song_close,
};

int main(void) {
    void *const volatile *pin = songcore_contract;
    for (int i = 0; i < SONG_CONTRACT_N; i++) {
        if (pin[i] == (void *)0) return 3;
    }
    return 0;
}
