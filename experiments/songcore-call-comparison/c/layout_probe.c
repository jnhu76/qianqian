#define _POSIX_C_SOURCE 200809L

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include "songcore.h"

#define TYPE(t) printf("type %s size %zu align %zu\n", #t, sizeof(t), _Alignof(t))
#define FIELD(t, f) printf("field %s.%s %zu\n", #t, #f, offsetof(t, f)); \
                    printf("fieldsize %s.%s %zu\n", #t, #f, sizeof(((t *)0)->f))

int main(void) {
    printf("abi_version %u\n", (unsigned)SONGCORE_ABI_VERSION);

    printf("status SONG_OK %d\n", (int)SONG_OK);
    printf("status SONG_EOF %d\n", (int)SONG_EOF);
    printf("status SONG_ERR_INVALID_ARGUMENT %d\n", (int)SONG_ERR_INVALID_ARGUMENT);
    printf("status SONG_ERR_STATE %d\n", (int)SONG_ERR_STATE);
    printf("status SONG_ERR_NOT_OPEN %d\n", (int)SONG_ERR_NOT_OPEN);
    printf("status SONG_ERR_IO %d\n", (int)SONG_ERR_IO);
    printf("status SONG_ERR_UNSUPPORTED_CONTAINER %d\n", (int)SONG_ERR_UNSUPPORTED_CONTAINER);
    printf("status SONG_ERR_NO_AUDIO_STREAM %d\n", (int)SONG_ERR_NO_AUDIO_STREAM);
    printf("status SONG_ERR_UNSUPPORTED_CODEC %d\n", (int)SONG_ERR_UNSUPPORTED_CODEC);
    printf("status SONG_ERR_CORRUPT_DATA %d\n", (int)SONG_ERR_CORRUPT_DATA);
    printf("status SONG_ERR_DECODE_ERROR %d\n", (int)SONG_ERR_DECODE_ERROR);
    printf("status SONG_ERR_SEEK_UNSUPPORTED %d\n", (int)SONG_ERR_SEEK_UNSUPPORTED);
    printf("status SONG_ERR_SEEK_ERROR %d\n", (int)SONG_ERR_SEEK_ERROR);
    printf("status SONG_ERR_STREAM_CHANGE %d\n", (int)SONG_ERR_STREAM_CHANGE);
    printf("status SONG_ERR_OUT_OF_MEMORY %d\n", (int)SONG_ERR_OUT_OF_MEMORY);
    printf("status SONG_ERR_INTERNAL_ERROR %d\n", (int)SONG_ERR_INTERNAL_ERROR);

    printf("ch SONG_CH_FRONT_LEFT %llu\n", (unsigned long long)SONG_CH_FRONT_LEFT);
    printf("ch SONG_CH_FRONT_RIGHT %llu\n", (unsigned long long)SONG_CH_FRONT_RIGHT);
    printf("ch SONG_CH_FRONT_CENTER %llu\n", (unsigned long long)SONG_CH_FRONT_CENTER);
    printf("ch SONG_CH_LOW_FREQUENCY %llu\n", (unsigned long long)SONG_CH_LOW_FREQUENCY);
    printf("ch SONG_CH_BACK_LEFT %llu\n", (unsigned long long)SONG_CH_BACK_LEFT);
    printf("ch SONG_CH_BACK_RIGHT %llu\n", (unsigned long long)SONG_CH_BACK_RIGHT);
    printf("ch SONG_CH_FRONT_LEFT_OF_CENTER %llu\n", (unsigned long long)SONG_CH_FRONT_LEFT_OF_CENTER);
    printf("ch SONG_CH_FRONT_RIGHT_OF_CENTER %llu\n", (unsigned long long)SONG_CH_FRONT_RIGHT_OF_CENTER);
    printf("ch SONG_CH_BACK_CENTER %llu\n", (unsigned long long)SONG_CH_BACK_CENTER);
    printf("ch SONG_CH_SIDE_LEFT %llu\n", (unsigned long long)SONG_CH_SIDE_LEFT);
    printf("ch SONG_CH_SIDE_RIGHT %llu\n", (unsigned long long)SONG_CH_SIDE_RIGHT);
    printf("ch SONG_CH_TOP_CENTER %llu\n", (unsigned long long)SONG_CH_TOP_CENTER);
    printf("ch SONG_CH_TOP_FRONT_LEFT %llu\n", (unsigned long long)SONG_CH_TOP_FRONT_LEFT);
    printf("ch SONG_CH_TOP_FRONT_CENTER %llu\n", (unsigned long long)SONG_CH_TOP_FRONT_CENTER);
    printf("ch SONG_CH_TOP_FRONT_RIGHT %llu\n", (unsigned long long)SONG_CH_TOP_FRONT_RIGHT);
    printf("ch SONG_CH_TOP_BACK_LEFT %llu\n", (unsigned long long)SONG_CH_TOP_BACK_LEFT);
    printf("ch SONG_CH_TOP_BACK_CENTER %llu\n", (unsigned long long)SONG_CH_TOP_BACK_CENTER);
    printf("ch SONG_CH_TOP_BACK_RIGHT %llu\n", (unsigned long long)SONG_CH_TOP_BACK_RIGHT);
    printf("ch SONG_CH_MASK_UNKNOWN %llu\n", (unsigned long long)SONG_CH_MASK_UNKNOWN);
    printf("ch SONG_CH_MONO %llu\n", (unsigned long long)SONG_CH_MONO);
    printf("ch SONG_CH_STEREO %llu\n", (unsigned long long)SONG_CH_STEREO);

    printf("scope SONG_METADATA_SCOPE_CONTAINER %d\n", (int)SONG_METADATA_SCOPE_CONTAINER);
    printf("scope SONG_METADATA_SCOPE_STREAM %d\n", (int)SONG_METADATA_SCOPE_STREAM);

    printf("role SONG_ARTWORK_UNKNOWN %d\n", (int)SONG_ARTWORK_UNKNOWN);
    printf("role SONG_ARTWORK_FRONT_COVER %d\n", (int)SONG_ARTWORK_FRONT_COVER);
    printf("role SONG_ARTWORK_BACK_COVER %d\n", (int)SONG_ARTWORK_BACK_COVER);
    printf("role SONG_ARTWORK_OTHER %d\n", (int)SONG_ARTWORK_OTHER);

    printf("type song_handle_ptr size %zu\n", sizeof(song_handle *));
    printf("type song_read_fn size %zu\n", sizeof(song_read_fn));
    printf("type song_seek_fn size %zu\n", sizeof(song_seek_fn));
    printf("type song_size_fn size %zu\n", sizeof(song_size_fn));

    TYPE(song_error);
    FIELD(song_error, message);
    FIELD(song_error, message_len);
    FIELD(song_error, native_code);
    FIELD(song_error, reserved);

    TYPE(song_io);
    FIELD(song_io, userdata);
    FIELD(song_io, read);
    FIELD(song_io, seek);
    FIELD(song_io, size);

    TYPE(song_info);
    FIELD(song_info, sample_rate);
    FIELD(song_info, channels);
    FIELD(song_info, channel_mask);
    FIELD(song_info, duration_us);
    FIELD(song_info, bits_per_sample);
    FIELD(song_info, codec);
    FIELD(song_info, container);
    FIELD(song_info, selected_audio_index);
    FIELD(song_info, audio_stream_count);
    FIELD(song_info, flags);
    FIELD(song_info, reserved);

    TYPE(song_stream_info);
    FIELD(song_stream_info, audio_index);
    FIELD(song_stream_info, stream_index);
    FIELD(song_stream_info, sample_rate);
    FIELD(song_stream_info, channels);
    FIELD(song_stream_info, channel_mask);
    FIELD(song_stream_info, duration_us);
    FIELD(song_stream_info, bits_per_sample);
    FIELD(song_stream_info, codec);
    FIELD(song_stream_info, is_default);
    FIELD(song_stream_info, reserved);

    TYPE(song_metadata);
    FIELD(song_metadata, title);
    FIELD(song_metadata, title_len);
    FIELD(song_metadata, has_title);
    FIELD(song_metadata, artist);
    FIELD(song_metadata, artist_len);
    FIELD(song_metadata, has_artist);
    FIELD(song_metadata, album);
    FIELD(song_metadata, album_len);
    FIELD(song_metadata, has_album);
    FIELD(song_metadata, album_artist);
    FIELD(song_metadata, album_artist_len);
    FIELD(song_metadata, has_album_artist);
    FIELD(song_metadata, genre);
    FIELD(song_metadata, genre_len);
    FIELD(song_metadata, has_genre);
    FIELD(song_metadata, composer);
    FIELD(song_metadata, composer_len);
    FIELD(song_metadata, has_composer);
    FIELD(song_metadata, date);
    FIELD(song_metadata, date_len);
    FIELD(song_metadata, has_date);
    FIELD(song_metadata, comment);
    FIELD(song_metadata, comment_len);
    FIELD(song_metadata, has_comment);
    FIELD(song_metadata, track_number);
    FIELD(song_metadata, has_track_number);
    FIELD(song_metadata, track_total);
    FIELD(song_metadata, has_track_total);
    FIELD(song_metadata, disc_number);
    FIELD(song_metadata, has_disc_number);
    FIELD(song_metadata, disc_total);
    FIELD(song_metadata, has_disc_total);
    FIELD(song_metadata, track_gain_mb);
    FIELD(song_metadata, has_track_gain);
    FIELD(song_metadata, track_peak);
    FIELD(song_metadata, has_track_peak);
    FIELD(song_metadata, album_gain_mb);
    FIELD(song_metadata, has_album_gain);
    FIELD(song_metadata, album_peak);
    FIELD(song_metadata, has_album_peak);

    TYPE(song_metadata_entry);
    FIELD(song_metadata_entry, scope);
    FIELD(song_metadata_entry, key);
    FIELD(song_metadata_entry, key_len);
    FIELD(song_metadata_entry, value);
    FIELD(song_metadata_entry, value_len);
    FIELD(song_metadata_entry, reserved);

    TYPE(song_artwork_item);
    FIELD(song_artwork_item, role);
    FIELD(song_artwork_item, mime);
    FIELD(song_artwork_item, mime_len);
    FIELD(song_artwork_item, data);
    FIELD(song_artwork_item, data_len);
    FIELD(song_artwork_item, width);
    FIELD(song_artwork_item, height);
    FIELD(song_artwork_item, is_front_cover);
    FIELD(song_artwork_item, reserved);

    return 0;
}
