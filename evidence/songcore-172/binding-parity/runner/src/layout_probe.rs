use std::mem::{align_of, offset_of, size_of, size_of_val};

use qianqian_songcore_sys::*;

fn emit_struct(
    s: &mut String,
    name: &str,
    size: usize,
    align: usize,
    fields: &[(&str, usize, usize)],
) {
    s.push_str(&format!("type {} size {} align {}\n", name, size, align));
    for (f, off, fsz) in fields {
        s.push_str(&format!("field {}.{} {}\n", name, f, off));
        s.push_str(&format!("fieldsize {}.{} {}\n", name, f, fsz));
    }
}

pub fn emit() -> String {
    let mut s = String::new();

    s.push_str(&format!("abi_version {}\n", SONGCORE_ABI_VERSION));

    let statuses = [
        ("SONG_OK", SONG_OK),
        ("SONG_EOF", SONG_EOF),
        ("SONG_ERR_INVALID_ARGUMENT", SONG_ERR_INVALID_ARGUMENT),
        ("SONG_ERR_STATE", SONG_ERR_STATE),
        ("SONG_ERR_NOT_OPEN", SONG_ERR_NOT_OPEN),
        ("SONG_ERR_IO", SONG_ERR_IO),
        (
            "SONG_ERR_UNSUPPORTED_CONTAINER",
            SONG_ERR_UNSUPPORTED_CONTAINER,
        ),
        ("SONG_ERR_NO_AUDIO_STREAM", SONG_ERR_NO_AUDIO_STREAM),
        ("SONG_ERR_UNSUPPORTED_CODEC", SONG_ERR_UNSUPPORTED_CODEC),
        ("SONG_ERR_CORRUPT_DATA", SONG_ERR_CORRUPT_DATA),
        ("SONG_ERR_DECODE_ERROR", SONG_ERR_DECODE_ERROR),
        ("SONG_ERR_SEEK_UNSUPPORTED", SONG_ERR_SEEK_UNSUPPORTED),
        ("SONG_ERR_SEEK_ERROR", SONG_ERR_SEEK_ERROR),
        ("SONG_ERR_STREAM_CHANGE", SONG_ERR_STREAM_CHANGE),
        ("SONG_ERR_OUT_OF_MEMORY", SONG_ERR_OUT_OF_MEMORY),
        ("SONG_ERR_INTERNAL_ERROR", SONG_ERR_INTERNAL_ERROR),
    ];
    for (n, v) in statuses {
        s.push_str(&format!("status {} {}\n", n, v));
    }

    let channels = [
        ("SONG_CH_FRONT_LEFT", SONG_CH_FRONT_LEFT),
        ("SONG_CH_FRONT_RIGHT", SONG_CH_FRONT_RIGHT),
        ("SONG_CH_FRONT_CENTER", SONG_CH_FRONT_CENTER),
        ("SONG_CH_LOW_FREQUENCY", SONG_CH_LOW_FREQUENCY),
        ("SONG_CH_BACK_LEFT", SONG_CH_BACK_LEFT),
        ("SONG_CH_BACK_RIGHT", SONG_CH_BACK_RIGHT),
        ("SONG_CH_FRONT_LEFT_OF_CENTER", SONG_CH_FRONT_LEFT_OF_CENTER),
        (
            "SONG_CH_FRONT_RIGHT_OF_CENTER",
            SONG_CH_FRONT_RIGHT_OF_CENTER,
        ),
        ("SONG_CH_BACK_CENTER", SONG_CH_BACK_CENTER),
        ("SONG_CH_SIDE_LEFT", SONG_CH_SIDE_LEFT),
        ("SONG_CH_SIDE_RIGHT", SONG_CH_SIDE_RIGHT),
        ("SONG_CH_TOP_CENTER", SONG_CH_TOP_CENTER),
        ("SONG_CH_TOP_FRONT_LEFT", SONG_CH_TOP_FRONT_LEFT),
        ("SONG_CH_TOP_FRONT_CENTER", SONG_CH_TOP_FRONT_CENTER),
        ("SONG_CH_TOP_FRONT_RIGHT", SONG_CH_TOP_FRONT_RIGHT),
        ("SONG_CH_TOP_BACK_LEFT", SONG_CH_TOP_BACK_LEFT),
        ("SONG_CH_TOP_BACK_CENTER", SONG_CH_TOP_BACK_CENTER),
        ("SONG_CH_TOP_BACK_RIGHT", SONG_CH_TOP_BACK_RIGHT),
        ("SONG_CH_MASK_UNKNOWN", SONG_CH_MASK_UNKNOWN),
        ("SONG_CH_MONO", SONG_CH_MONO),
        ("SONG_CH_STEREO", SONG_CH_STEREO),
    ];
    for (n, v) in channels {
        s.push_str(&format!("ch {} {}\n", n, v));
    }

    s.push_str(&format!(
        "scope SONG_METADATA_SCOPE_CONTAINER {}\n",
        SONG_METADATA_SCOPE_CONTAINER
    ));
    s.push_str(&format!(
        "scope SONG_METADATA_SCOPE_STREAM {}\n",
        SONG_METADATA_SCOPE_STREAM
    ));

    s.push_str(&format!(
        "role SONG_ARTWORK_UNKNOWN {}\n",
        SONG_ARTWORK_UNKNOWN
    ));
    s.push_str(&format!(
        "role SONG_ARTWORK_FRONT_COVER {}\n",
        SONG_ARTWORK_FRONT_COVER
    ));
    s.push_str(&format!(
        "role SONG_ARTWORK_BACK_COVER {}\n",
        SONG_ARTWORK_BACK_COVER
    ));
    s.push_str(&format!("role SONG_ARTWORK_OTHER {}\n", SONG_ARTWORK_OTHER));

    s.push_str(&format!(
        "type song_handle_ptr size {}\n",
        size_of::<*mut song_handle>()
    ));
    s.push_str(&format!(
        "type song_read_fn size {}\n",
        size_of::<song_read_fn>()
    ));
    s.push_str(&format!(
        "type song_seek_fn size {}\n",
        size_of::<song_seek_fn>()
    ));
    s.push_str(&format!(
        "type song_size_fn size {}\n",
        size_of::<song_size_fn>()
    ));

    let error_probe: song_error = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_error",
        size_of::<song_error>(),
        align_of::<song_error>(),
        &[
            (
                "message",
                offset_of!(song_error, message),
                size_of_val(&error_probe.message),
            ),
            (
                "message_len",
                offset_of!(song_error, message_len),
                size_of_val(&error_probe.message_len),
            ),
            (
                "native_code",
                offset_of!(song_error, native_code),
                size_of_val(&error_probe.native_code),
            ),
            (
                "reserved",
                offset_of!(song_error, reserved),
                size_of_val(&error_probe.reserved),
            ),
        ],
    );

    let io_probe: song_io = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_io",
        size_of::<song_io>(),
        align_of::<song_io>(),
        &[
            (
                "userdata",
                offset_of!(song_io, userdata),
                size_of_val(&io_probe.userdata),
            ),
            (
                "read",
                offset_of!(song_io, read),
                size_of_val(&io_probe.read),
            ),
            (
                "seek",
                offset_of!(song_io, seek),
                size_of_val(&io_probe.seek),
            ),
            (
                "size",
                offset_of!(song_io, size),
                size_of_val(&io_probe.size),
            ),
        ],
    );

    let info_probe: song_info = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_info",
        size_of::<song_info>(),
        align_of::<song_info>(),
        &[
            (
                "sample_rate",
                offset_of!(song_info, sample_rate),
                size_of_val(&info_probe.sample_rate),
            ),
            (
                "channels",
                offset_of!(song_info, channels),
                size_of_val(&info_probe.channels),
            ),
            (
                "channel_mask",
                offset_of!(song_info, channel_mask),
                size_of_val(&info_probe.channel_mask),
            ),
            (
                "duration_us",
                offset_of!(song_info, duration_us),
                size_of_val(&info_probe.duration_us),
            ),
            (
                "bits_per_sample",
                offset_of!(song_info, bits_per_sample),
                size_of_val(&info_probe.bits_per_sample),
            ),
            (
                "codec",
                offset_of!(song_info, codec),
                size_of_val(&info_probe.codec),
            ),
            (
                "container",
                offset_of!(song_info, container),
                size_of_val(&info_probe.container),
            ),
            (
                "selected_audio_index",
                offset_of!(song_info, selected_audio_index),
                size_of_val(&info_probe.selected_audio_index),
            ),
            (
                "audio_stream_count",
                offset_of!(song_info, audio_stream_count),
                size_of_val(&info_probe.audio_stream_count),
            ),
            (
                "flags",
                offset_of!(song_info, flags),
                size_of_val(&info_probe.flags),
            ),
            (
                "reserved",
                offset_of!(song_info, reserved),
                size_of_val(&info_probe.reserved),
            ),
        ],
    );

    let stream_probe: song_stream_info = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_stream_info",
        size_of::<song_stream_info>(),
        align_of::<song_stream_info>(),
        &[
            (
                "audio_index",
                offset_of!(song_stream_info, audio_index),
                size_of_val(&stream_probe.audio_index),
            ),
            (
                "stream_index",
                offset_of!(song_stream_info, stream_index),
                size_of_val(&stream_probe.stream_index),
            ),
            (
                "sample_rate",
                offset_of!(song_stream_info, sample_rate),
                size_of_val(&stream_probe.sample_rate),
            ),
            (
                "channels",
                offset_of!(song_stream_info, channels),
                size_of_val(&stream_probe.channels),
            ),
            (
                "channel_mask",
                offset_of!(song_stream_info, channel_mask),
                size_of_val(&stream_probe.channel_mask),
            ),
            (
                "duration_us",
                offset_of!(song_stream_info, duration_us),
                size_of_val(&stream_probe.duration_us),
            ),
            (
                "bits_per_sample",
                offset_of!(song_stream_info, bits_per_sample),
                size_of_val(&stream_probe.bits_per_sample),
            ),
            (
                "codec",
                offset_of!(song_stream_info, codec),
                size_of_val(&stream_probe.codec),
            ),
            (
                "is_default",
                offset_of!(song_stream_info, is_default),
                size_of_val(&stream_probe.is_default),
            ),
            (
                "reserved",
                offset_of!(song_stream_info, reserved),
                size_of_val(&stream_probe.reserved),
            ),
        ],
    );

    let meta_probe: song_metadata = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_metadata",
        size_of::<song_metadata>(),
        align_of::<song_metadata>(),
        &[
            (
                "title",
                offset_of!(song_metadata, title),
                size_of_val(&meta_probe.title),
            ),
            (
                "title_len",
                offset_of!(song_metadata, title_len),
                size_of_val(&meta_probe.title_len),
            ),
            (
                "has_title",
                offset_of!(song_metadata, has_title),
                size_of_val(&meta_probe.has_title),
            ),
            (
                "artist",
                offset_of!(song_metadata, artist),
                size_of_val(&meta_probe.artist),
            ),
            (
                "artist_len",
                offset_of!(song_metadata, artist_len),
                size_of_val(&meta_probe.artist_len),
            ),
            (
                "has_artist",
                offset_of!(song_metadata, has_artist),
                size_of_val(&meta_probe.has_artist),
            ),
            (
                "album",
                offset_of!(song_metadata, album),
                size_of_val(&meta_probe.album),
            ),
            (
                "album_len",
                offset_of!(song_metadata, album_len),
                size_of_val(&meta_probe.album_len),
            ),
            (
                "has_album",
                offset_of!(song_metadata, has_album),
                size_of_val(&meta_probe.has_album),
            ),
            (
                "album_artist",
                offset_of!(song_metadata, album_artist),
                size_of_val(&meta_probe.album_artist),
            ),
            (
                "album_artist_len",
                offset_of!(song_metadata, album_artist_len),
                size_of_val(&meta_probe.album_artist_len),
            ),
            (
                "has_album_artist",
                offset_of!(song_metadata, has_album_artist),
                size_of_val(&meta_probe.has_album_artist),
            ),
            (
                "genre",
                offset_of!(song_metadata, genre),
                size_of_val(&meta_probe.genre),
            ),
            (
                "genre_len",
                offset_of!(song_metadata, genre_len),
                size_of_val(&meta_probe.genre_len),
            ),
            (
                "has_genre",
                offset_of!(song_metadata, has_genre),
                size_of_val(&meta_probe.has_genre),
            ),
            (
                "composer",
                offset_of!(song_metadata, composer),
                size_of_val(&meta_probe.composer),
            ),
            (
                "composer_len",
                offset_of!(song_metadata, composer_len),
                size_of_val(&meta_probe.composer_len),
            ),
            (
                "has_composer",
                offset_of!(song_metadata, has_composer),
                size_of_val(&meta_probe.has_composer),
            ),
            (
                "date",
                offset_of!(song_metadata, date),
                size_of_val(&meta_probe.date),
            ),
            (
                "date_len",
                offset_of!(song_metadata, date_len),
                size_of_val(&meta_probe.date_len),
            ),
            (
                "has_date",
                offset_of!(song_metadata, has_date),
                size_of_val(&meta_probe.has_date),
            ),
            (
                "comment",
                offset_of!(song_metadata, comment),
                size_of_val(&meta_probe.comment),
            ),
            (
                "comment_len",
                offset_of!(song_metadata, comment_len),
                size_of_val(&meta_probe.comment_len),
            ),
            (
                "has_comment",
                offset_of!(song_metadata, has_comment),
                size_of_val(&meta_probe.has_comment),
            ),
            (
                "track_number",
                offset_of!(song_metadata, track_number),
                size_of_val(&meta_probe.track_number),
            ),
            (
                "has_track_number",
                offset_of!(song_metadata, has_track_number),
                size_of_val(&meta_probe.has_track_number),
            ),
            (
                "track_total",
                offset_of!(song_metadata, track_total),
                size_of_val(&meta_probe.track_total),
            ),
            (
                "has_track_total",
                offset_of!(song_metadata, has_track_total),
                size_of_val(&meta_probe.has_track_total),
            ),
            (
                "disc_number",
                offset_of!(song_metadata, disc_number),
                size_of_val(&meta_probe.disc_number),
            ),
            (
                "has_disc_number",
                offset_of!(song_metadata, has_disc_number),
                size_of_val(&meta_probe.has_disc_number),
            ),
            (
                "disc_total",
                offset_of!(song_metadata, disc_total),
                size_of_val(&meta_probe.disc_total),
            ),
            (
                "has_disc_total",
                offset_of!(song_metadata, has_disc_total),
                size_of_val(&meta_probe.has_disc_total),
            ),
            (
                "track_gain_mb",
                offset_of!(song_metadata, track_gain_mb),
                size_of_val(&meta_probe.track_gain_mb),
            ),
            (
                "has_track_gain",
                offset_of!(song_metadata, has_track_gain),
                size_of_val(&meta_probe.has_track_gain),
            ),
            (
                "track_peak",
                offset_of!(song_metadata, track_peak),
                size_of_val(&meta_probe.track_peak),
            ),
            (
                "has_track_peak",
                offset_of!(song_metadata, has_track_peak),
                size_of_val(&meta_probe.has_track_peak),
            ),
            (
                "album_gain_mb",
                offset_of!(song_metadata, album_gain_mb),
                size_of_val(&meta_probe.album_gain_mb),
            ),
            (
                "has_album_gain",
                offset_of!(song_metadata, has_album_gain),
                size_of_val(&meta_probe.has_album_gain),
            ),
            (
                "album_peak",
                offset_of!(song_metadata, album_peak),
                size_of_val(&meta_probe.album_peak),
            ),
            (
                "has_album_peak",
                offset_of!(song_metadata, has_album_peak),
                size_of_val(&meta_probe.has_album_peak),
            ),
        ],
    );

    let entry_probe: song_metadata_entry = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_metadata_entry",
        size_of::<song_metadata_entry>(),
        align_of::<song_metadata_entry>(),
        &[
            (
                "scope",
                offset_of!(song_metadata_entry, scope),
                size_of_val(&entry_probe.scope),
            ),
            (
                "key",
                offset_of!(song_metadata_entry, key),
                size_of_val(&entry_probe.key),
            ),
            (
                "key_len",
                offset_of!(song_metadata_entry, key_len),
                size_of_val(&entry_probe.key_len),
            ),
            (
                "value",
                offset_of!(song_metadata_entry, value),
                size_of_val(&entry_probe.value),
            ),
            (
                "value_len",
                offset_of!(song_metadata_entry, value_len),
                size_of_val(&entry_probe.value_len),
            ),
            (
                "reserved",
                offset_of!(song_metadata_entry, reserved),
                size_of_val(&entry_probe.reserved),
            ),
        ],
    );

    let art_probe: song_artwork_item = unsafe { std::mem::zeroed() };
    emit_struct(
        &mut s,
        "song_artwork_item",
        size_of::<song_artwork_item>(),
        align_of::<song_artwork_item>(),
        &[
            (
                "role",
                offset_of!(song_artwork_item, role),
                size_of_val(&art_probe.role),
            ),
            (
                "mime",
                offset_of!(song_artwork_item, mime),
                size_of_val(&art_probe.mime),
            ),
            (
                "mime_len",
                offset_of!(song_artwork_item, mime_len),
                size_of_val(&art_probe.mime_len),
            ),
            (
                "data",
                offset_of!(song_artwork_item, data),
                size_of_val(&art_probe.data),
            ),
            (
                "data_len",
                offset_of!(song_artwork_item, data_len),
                size_of_val(&art_probe.data_len),
            ),
            (
                "width",
                offset_of!(song_artwork_item, width),
                size_of_val(&art_probe.width),
            ),
            (
                "height",
                offset_of!(song_artwork_item, height),
                size_of_val(&art_probe.height),
            ),
            (
                "is_front_cover",
                offset_of!(song_artwork_item, is_front_cover),
                size_of_val(&art_probe.is_front_cover),
            ),
            (
                "reserved",
                offset_of!(song_artwork_item, reserved),
                size_of_val(&art_probe.reserved),
            ),
        ],
    );

    s
}
