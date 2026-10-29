#include "songcore.h"
#include <stdio.h>
/* ABI-complete reference probe: taking the address of every song* ABI
 * function forces the linker to pull every defining member of the merged
 * archive, approximating the real consumer's transitive closure. */
static void *const song_abi_refs[] = {
  (void *)&song_audio_stream_count,
  (void *)&song_audio_stream_info,
  (void *)&song_close,
  (void *)&song_get_artwork_count,
  (void *)&song_get_artwork_item,
  (void *)&song_get_metadata,
  (void *)&song_get_metadata_count,
  (void *)&song_get_metadata_entry,
  (void *)&song_last_error,
  (void *)&song_open,
  (void *)&song_probe,
  (void *)&song_read_pcm,
  (void *)&song_seek,
  (void *)&song_select_stream,
  (void *)&songcore_abi_version,
};
int main(void) {
    printf("%zu\n", sizeof(song_abi_refs) / sizeof(song_abi_refs[0]));
    return song_abi_refs[0] == 0;
}
