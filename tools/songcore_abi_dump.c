/*
 * songcore_abi_dump.c — ABI v1 layout snapshot generator (release gate).
 *
 * Compiles against include/songcore.h ONLY and prints a deterministic text
 * dump of the ABI-relevant surface: public function list, enum numeric
 * values, public struct sizes and field offsets. The output is compared
 * against the frozen snapshot (bench/results/songcore-v1/abi-layout-<target>-v1.txt)
 * by tools/songcore_release.py; any drift means the ABI changed and must be
 * an explicit SONGCORE_ABI_VERSION bump (docs/development/release.md).
 *
 * No SongCore symbol is called, so no library is needed to link:
 *
 *   cc -Iinclude tools/songcore_abi_dump.c -o abi_dump && ./abi_dump
 */
#include "songcore.h"

#include <stdio.h>
#include <stddef.h>

#define DUMP_ENUM(Enum, name) printf("enum %s %s %lld\n", #Enum, #name, (long long)(name))
#define DUMP_SIZE(sname) printf("struct %s size %zu\n", #sname, sizeof(sname))
#define DUMP_OFF(sname, field) printf("offset %s %s %zu\n", #sname, #field, offsetof(sname, field))
#define DUMP_FUNC(name) printf("func %s\n", #name)

int main(void) {
    printf("abi_version %u\n", SONGCORE_ABI_VERSION);

    /* song_status */
    DUMP_ENUM(song_status, SONG_OK);
    DUMP_ENUM(song_status, SONG_EOF);
    DUMP_ENUM(song_status, SONG_ERR_INVALID_ARGUMENT);
    DUMP_ENUM(song_status, SONG_ERR_STATE);
    DUMP_ENUM(song_status, SONG_ERR_NOT_OPEN);
    DUMP_ENUM(song_status, SONG_ERR_IO);
    DUMP_ENUM(song_status, SONG_ERR_UNSUPPORTED_CONTAINER);
    DUMP_ENUM(song_status, SONG_ERR_NO_AUDIO_STREAM);
    DUMP_ENUM(song_status, SONG_ERR_UNSUPPORTED_CODEC);
    DUMP_ENUM(song_status, SONG_ERR_CORRUPT_DATA);
    DUMP_ENUM(song_status, SONG_ERR_DECODE_ERROR);
    DUMP_ENUM(song_status, SONG_ERR_SEEK_UNSUPPORTED);
    DUMP_ENUM(song_status, SONG_ERR_SEEK_ERROR);
    DUMP_ENUM(song_status, SONG_ERR_STREAM_CHANGE);
    DUMP_ENUM(song_status, SONG_ERR_OUT_OF_MEMORY);
    DUMP_ENUM(song_status, SONG_ERR_INTERNAL_ERROR);

    /* song_metadata_scope */
    DUMP_ENUM(song_metadata_scope, SONG_METADATA_SCOPE_CONTAINER);
    DUMP_ENUM(song_metadata_scope, SONG_METADATA_SCOPE_STREAM);

    /* song_artwork_role */
    DUMP_ENUM(song_artwork_role, SONG_ARTWORK_UNKNOWN);
    DUMP_ENUM(song_artwork_role, SONG_ARTWORK_FRONT_COVER);
    DUMP_ENUM(song_artwork_role, SONG_ARTWORK_BACK_COVER);
    DUMP_ENUM(song_artwork_role, SONG_ARTWORK_OTHER);

    /* channel layout bits (the meaningful bases + masks) */
    DUMP_ENUM(channel, SONG_CH_FRONT_LEFT);
    DUMP_ENUM(channel, SONG_CH_FRONT_RIGHT);
    DUMP_ENUM(channel, SONG_CH_FRONT_CENTER);
    DUMP_ENUM(channel, SONG_CH_LOW_FREQUENCY);
    DUMP_ENUM(channel, SONG_CH_BACK_LEFT);
    DUMP_ENUM(channel, SONG_CH_BACK_RIGHT);
    DUMP_ENUM(channel, SONG_CH_SIDE_LEFT);
    DUMP_ENUM(channel, SONG_CH_SIDE_RIGHT);
    DUMP_ENUM(channel, SONG_CH_MASK_UNKNOWN);
    DUMP_ENUM(channel, SONG_CH_MONO);
    DUMP_ENUM(channel, SONG_CH_STEREO);

    /* struct layouts */
    DUMP_SIZE(song_io);
    DUMP_OFF(song_io, userdata);
    DUMP_OFF(song_io, read);
    DUMP_OFF(song_io, seek);
    DUMP_OFF(song_io, size);

    DUMP_SIZE(song_error);
    DUMP_OFF(song_error, message);
    DUMP_OFF(song_error, message_len);
    DUMP_OFF(song_error, native_code);
    DUMP_OFF(song_error, reserved);

    DUMP_SIZE(song_info);
    DUMP_OFF(song_info, sample_rate);
    DUMP_OFF(song_info, channels);
    DUMP_OFF(song_info, channel_mask);
    DUMP_OFF(song_info, duration_us);
    DUMP_OFF(song_info, bits_per_sample);
    DUMP_OFF(song_info, codec);
    DUMP_OFF(song_info, container);
    DUMP_OFF(song_info, selected_audio_index);
    DUMP_OFF(song_info, audio_stream_count);
    DUMP_OFF(song_info, flags);
    DUMP_OFF(song_info, reserved);

    DUMP_SIZE(song_stream_info);
    DUMP_OFF(song_stream_info, audio_index);
    DUMP_OFF(song_stream_info, stream_index);
    DUMP_OFF(song_stream_info, sample_rate);
    DUMP_OFF(song_stream_info, channels);
    DUMP_OFF(song_stream_info, channel_mask);
    DUMP_OFF(song_stream_info, duration_us);
    DUMP_OFF(song_stream_info, bits_per_sample);
    DUMP_OFF(song_stream_info, codec);
    DUMP_OFF(song_stream_info, is_default);
    DUMP_OFF(song_stream_info, reserved);

    DUMP_SIZE(song_metadata);
    DUMP_OFF(song_metadata, title);
    DUMP_OFF(song_metadata, title_len);
    DUMP_OFF(song_metadata, has_title);
    DUMP_OFF(song_metadata, artist);
    DUMP_OFF(song_metadata, album);
    DUMP_OFF(song_metadata, album_artist);
    DUMP_OFF(song_metadata, genre);
    DUMP_OFF(song_metadata, composer);
    DUMP_OFF(song_metadata, date);
    DUMP_OFF(song_metadata, comment);
    DUMP_OFF(song_metadata, track_number);
    DUMP_OFF(song_metadata, track_total);
    DUMP_OFF(song_metadata, disc_number);
    DUMP_OFF(song_metadata, disc_total);
    DUMP_OFF(song_metadata, track_gain_mb);
    DUMP_OFF(song_metadata, track_peak);
    DUMP_OFF(song_metadata, album_gain_mb);
    DUMP_OFF(song_metadata, album_peak);

    DUMP_SIZE(song_metadata_entry);
    DUMP_OFF(song_metadata_entry, scope);
    DUMP_OFF(song_metadata_entry, key);
    DUMP_OFF(song_metadata_entry, key_len);
    DUMP_OFF(song_metadata_entry, value);
    DUMP_OFF(song_metadata_entry, value_len);
    DUMP_OFF(song_metadata_entry, reserved);

    DUMP_SIZE(song_artwork_item);
    DUMP_OFF(song_artwork_item, role);
    DUMP_OFF(song_artwork_item, mime);
    DUMP_OFF(song_artwork_item, mime_len);
    DUMP_OFF(song_artwork_item, data);
    DUMP_OFF(song_artwork_item, data_len);
    DUMP_OFF(song_artwork_item, width);
    DUMP_OFF(song_artwork_item, height);
    DUMP_OFF(song_artwork_item, is_front_cover);
    DUMP_OFF(song_artwork_item, reserved);

    /* public function list — the frozen 15 + version */
    DUMP_FUNC(songcore_abi_version);
    DUMP_FUNC(song_open);
    DUMP_FUNC(song_probe);
    DUMP_FUNC(song_audio_stream_count);
    DUMP_FUNC(song_audio_stream_info);
    DUMP_FUNC(song_select_stream);
    DUMP_FUNC(song_get_metadata);
    DUMP_FUNC(song_get_metadata_count);
    DUMP_FUNC(song_get_metadata_entry);
    DUMP_FUNC(song_get_artwork_count);
    DUMP_FUNC(song_get_artwork_item);
    DUMP_FUNC(song_read_pcm);
    DUMP_FUNC(song_seek);
    DUMP_FUNC(song_last_error);
    DUMP_FUNC(song_close);

    return 0;
}
