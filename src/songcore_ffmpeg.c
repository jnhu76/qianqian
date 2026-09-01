/*
 * songcore_ffmpeg.c — SongCore ABI v1 implementation over pinned FFmpeg.
 *
 * SongCore owns: container detection, decodable-audio-stream discovery and
 * selection, metadata snapshot, artwork snapshot, decode, seek, EOF, and
 * typed errors. It never resamples, rematrices, or processes PCM beyond the
 * representation normalization (packed/planar -> interleaved Float32).
 *
 * No FFmpeg type crosses the songcore.h boundary.
 */

#include "songcore.h"

#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/dict.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>
#include <libavutil/replaygain.h>
#include <libavutil/samplefmt.h>

#define SONG_AVIO_BUFFER_SIZE 32768

/* -------------------------------------------------------------------------
 * Handle state
 * ---------------------------------------------------------------------- */

struct song_handle {
    song_io io;
    int64_t io_pos;

    AVFormatContext *fmt;
    AVCodecContext *dec;
    AVIOContext *avio;
    AVPacket *packet;
    AVFrame *frame;

    int probed;
    int packet_pending;
    int demux_eof;
    int drain_sent;
    int decoder_eof;
    song_status fatal_error; /* pending typed error (0 == none) */
    int last_native;         /* raw backend error code of the last failure */

    /* decodable audio stream enumeration (absolute stream indices) */
    uint32_t audio_count;
    int *audio_streams;
    uint32_t selected;

    int sample_rate;
    int channels;
    uint64_t channel_mask;

    /* metadata snapshot (SongCore-owned, immutable after build) */
    song_metadata meta;
    song_metadata_entry *raw;
    uint32_t raw_count;
    char *meta_buf;   /* canonical field strings */
    size_t meta_buf_len;
    char *raw_buf;    /* raw entry key/value strings (separate buffer so a
                         realloc of one never invalidates the other) */
    size_t raw_buf_len;

    /* artwork snapshot (SongCore-owned, immutable until close) */
    struct art_item {
        uint32_t role; /* song_artwork_role */
        const char *mime;
        uint32_t mime_len;
        const uint8_t *data;
        uint64_t data_len;
        int32_t width;
        int32_t height;
        uint32_t is_front_cover;
    } *art;
    uint32_t art_count;
    uint8_t *art_buf;
    size_t art_buf_len;

    /* last-error diagnostic */
    song_error err;
    char err_msg[256];

    float *pcm;
    size_t pcm_capacity_floats;
    size_t pcm_frames;
    size_t pcm_offset_frames;
};

/* -------------------------------------------------------------------------
 * Small helpers
 * ---------------------------------------------------------------------- */

static void clear_error(song_handle *h) {
    h->err.message = NULL;
    h->err.message_len = 0;
    h->err.native_code = 0;
    h->err.reserved = 0;
}

static song_status set_error(song_handle *h, song_status st, int native,
                             const char *msg) {
    h->err.native_code = native;
    h->err.reserved = 0;
    snprintf(h->err_msg, sizeof(h->err_msg), "%s", msg ? msg : "error");
    h->err.message = h->err_msg;
    h->err.message_len = (uint32_t)strlen(h->err_msg);
    return st;
}

/* Copy a FFmpeg channel layout mask into SongCore's own bit convention.
 * The bit numbering is SongCore-owned and stable; for native layouts it
 * coincides with the conventional FFmpeg/SMPTE numbering. 0 == unknown. */
static uint64_t channel_mask_from_layout(const AVChannelLayout *l) {
    if (l->order == AV_CHANNEL_ORDER_NATIVE && l->nb_channels > 0 &&
        l->nb_channels <= 64) {
        return (uint64_t)l->u.mask;
    }
    return 0;
}

static void copy_name(char dst[32], const char *src) {
    if (!src) src = "unknown";
    snprintf(dst, 32, "%s", src);
}

/* -------------------------------------------------------------------------
 * Host I/O
 * ---------------------------------------------------------------------- */

static int io_read(void *opaque, uint8_t *dst, int size) {
    song_handle *h = (song_handle *)opaque;
    int64_t n = h->io.read(h->io.userdata, dst, (size_t)size);
    if (n < 0) return AVERROR(EIO);
    if (n == 0) return AVERROR_EOF;
    if (n > size) return AVERROR(EINVAL);
    h->io_pos += n;
    return (int)n;
}

static int64_t io_seek(void *opaque, int64_t offset, int whence) {
    song_handle *h = (song_handle *)opaque;
    if (whence & AVSEEK_SIZE) {
        int64_t size = h->io.size(h->io.userdata);
        return size >= 0 ? size : AVERROR(EIO);
    }

    int base = whence & ~AVSEEK_FORCE;
    int64_t absolute = 0;
    if (base == SEEK_SET) {
        absolute = offset;
    } else if (base == SEEK_CUR) {
        if ((offset > 0 && h->io_pos > INT64_MAX - offset) ||
            (offset < 0 && h->io_pos < INT64_MIN - offset))
            return AVERROR(EINVAL);
        absolute = h->io_pos + offset;
    } else if (base == SEEK_END) {
        int64_t size = h->io.size(h->io.userdata);
        if (size < 0) return AVERROR(EIO);
        if ((offset > 0 && size > INT64_MAX - offset) ||
            (offset < 0 && size < INT64_MIN - offset))
            return AVERROR(EINVAL);
        absolute = size + offset;
    } else {
        return AVERROR(EINVAL);
    }
    if (absolute < 0) return AVERROR(EINVAL);

    int64_t pos = h->io.seek(h->io.userdata, absolute);
    if (pos < 0) return AVERROR(EIO);
    h->io_pos = pos;
    return pos;
}

/* -------------------------------------------------------------------------
 * Decode state machine (unchanged production semantics)
 * ---------------------------------------------------------------------- */

static void reset_decode_state(song_handle *h) {
    if (h->packet) av_packet_unref(h->packet);
    if (h->frame) av_frame_unref(h->frame);
    h->packet_pending = 0;
    h->demux_eof = 0;
    h->drain_sent = 0;
    h->decoder_eof = 0;
    h->fatal_error = SONG_OK;
    h->last_native = 0;
    h->pcm_frames = 0;
    h->pcm_offset_frames = 0;
}

/*
 * Produce one decoded AVFrame. The packet is retained across send(EAGAIN):
 * backpressure always drains receive_frame() and retries the SAME packet.
 * Returns 1 (frame ready), 0 (EOF), or <0 (error; h->last_native set).
 */
static int decode_one_frame(song_handle *h) {
    for (;;) {
        int ret = avcodec_receive_frame(h->dec, h->frame);
        if (ret >= 0) return 1;
        if (ret == AVERROR_EOF) {
            h->decoder_eof = 1;
            return 0;
        }
        if (ret != AVERROR(EAGAIN)) {
            h->last_native = ret;
            return -1;
        }

        if (h->packet_pending) {
            ret = avcodec_send_packet(h->dec, h->packet);
            if (ret == AVERROR(EAGAIN)) continue;
            if (ret < 0) {
                h->last_native = ret;
                return -1;
            }
            av_packet_unref(h->packet);
            h->packet_pending = 0;
            continue;
        }

        if (!h->demux_eof) {
            for (;;) {
                ret = av_read_frame(h->fmt, h->packet);
                if (ret < 0) {
                    av_packet_unref(h->packet);
                    if (ret == AVERROR_EOF || avio_feof(h->fmt->pb)) {
                        h->demux_eof = 1;
                        break;
                    }
                    h->last_native = ret;
                    return -1;
                }
                if (h->packet->stream_index == h->audio_streams[h->selected]) {
                    h->packet_pending = 1;
                    break;
                }
                av_packet_unref(h->packet);
            }
            if (h->packet_pending) continue;
        }

        if (!h->drain_sent) {
            ret = avcodec_send_packet(h->dec, NULL);
            if (ret == AVERROR(EAGAIN)) continue;
            if (ret == AVERROR_EOF) {
                h->decoder_eof = 1;
                return 0;
            }
            if (ret < 0) {
                h->last_native = ret;
                return -1;
            }
            h->drain_sent = 1;
            continue;
        }

        /* After a successful drain signal, receive must converge to frames
         * or EOF. */
        h->last_native = AVERROR_INVALIDDATA;
        return -1;
    }
}

static song_status decode_status(const song_handle *h) {
    switch (h->last_native) {
    case AVERROR(EIO):
        return SONG_ERR_IO;
    case AVERROR(ENOMEM):
        return SONG_ERR_OUT_OF_MEMORY;
    default:
        return SONG_ERR_DECODE_ERROR;
    }
}

/* -------------------------------------------------------------------------
 * PCM conversion (representation normalization only)
 * ---------------------------------------------------------------------- */

static int ensure_pcm(song_handle *h, size_t frames, int channels) {
    if (channels <= 0 || frames > SIZE_MAX / (size_t)channels) return -1;
    size_t floats = frames * (size_t)channels;
    if (floats <= h->pcm_capacity_floats) return 0;
    if (floats > SIZE_MAX / sizeof(float)) return -1;
    float *next = (float *)realloc(h->pcm, floats * sizeof(float));
    if (!next) return -1;
    h->pcm = next;
    h->pcm_capacity_floats = floats;
    return 0;
}

/* Returns 0 on success, -1 on conversion failure, -2 on format change
 * (fail-closed: the frame contradicts song_info). */
static int frame_to_f32(song_handle *h, const AVFrame *f) {
    const int channels = f->ch_layout.nb_channels;
    const int frames = f->nb_samples;
    if (channels != h->channels || f->sample_rate != h->sample_rate ||
        frames < 0) {
        return -2;
    }
    if (h->channel_mask != 0) {
        uint64_t fm = channel_mask_from_layout(&f->ch_layout);
        if (fm != 0 && fm != h->channel_mask) return -2;
    }
    if (ensure_pcm(h, (size_t)frames, channels) < 0) return -1;

    const size_t samples = (size_t)frames * (size_t)channels;
    float *dst = h->pcm;
    switch ((enum AVSampleFormat)f->format) {
    case AV_SAMPLE_FMT_FLT:
        memcpy(dst, f->data[0], sizeof(float) * samples);
        break;
    case AV_SAMPLE_FMT_FLTP:
        for (int c = 0; c < channels; ++c) {
            const float *src = (const float *)f->extended_data[c];
            for (int i = 0; i < frames; ++i) dst[(size_t)i * channels + c] = src[i];
        }
        break;
    case AV_SAMPLE_FMT_S16: {
        const int16_t *src = (const int16_t *)f->data[0];
        for (size_t i = 0; i < samples; ++i) dst[i] = (float)src[i] * (1.0f / 32768.0f);
        break;
    }
    case AV_SAMPLE_FMT_S16P:
        for (int c = 0; c < channels; ++c) {
            const int16_t *src = (const int16_t *)f->extended_data[c];
            for (int i = 0; i < frames; ++i)
                dst[(size_t)i * channels + c] = (float)src[i] * (1.0f / 32768.0f);
        }
        break;
    case AV_SAMPLE_FMT_S32: {
        const int32_t *src = (const int32_t *)f->data[0];
        for (size_t i = 0; i < samples; ++i) dst[i] = (float)src[i] * (1.0f / 2147483648.0f);
        break;
    }
    case AV_SAMPLE_FMT_S32P:
        for (int c = 0; c < channels; ++c) {
            const int32_t *src = (const int32_t *)f->extended_data[c];
            for (int i = 0; i < frames; ++i)
                dst[(size_t)i * channels + c] = (float)src[i] * (1.0f / 2147483648.0f);
        }
        break;
    case AV_SAMPLE_FMT_DBLP:
        for (int c = 0; c < channels; ++c) {
            const double *src = (const double *)f->extended_data[c];
            for (int i = 0; i < frames; ++i) dst[(size_t)i * channels + c] = (float)src[i];
        }
        break;
    case AV_SAMPLE_FMT_U8P:
        for (int c = 0; c < channels; ++c) {
            const uint8_t *src = f->extended_data[c];
            for (int i = 0; i < frames; ++i)
                dst[(size_t)i * channels + c] = ((float)src[i] - 128.0f) * (1.0f / 128.0f);
        }
        break;
    case AV_SAMPLE_FMT_U8: {
        const uint8_t *src = f->data[0];
        for (size_t i = 0; i < samples; ++i)
            dst[i] = ((float)src[i] - 128.0f) * (1.0f / 128.0f);
        break;
    }
    case AV_SAMPLE_FMT_DBL: {
        const double *src = (const double *)f->data[0];
        for (size_t i = 0; i < samples; ++i) dst[i] = (float)src[i];
        break;
    }
    default:
        return -1;
    }

    h->pcm_frames = (size_t)frames;
    h->pcm_offset_frames = 0;
    return 0;
}

/* -------------------------------------------------------------------------
 * Metadata snapshot
 * ---------------------------------------------------------------------- */

/* Parse "N[/M]" into number and optional total. */
static void parse_pair(const AVStream *st, const AVFormatContext *fmt,
                       const char *key, int32_t *num, uint32_t *has_num,
                       int32_t *den, uint32_t *has_den) {
    const AVDictionaryEntry *e = av_dict_get(st->metadata, key, NULL, 0);
    if (!e) e = av_dict_get(fmt->metadata, key, NULL, 0);
    if (!e || !e->value) return;
    const char *p = e->value;
    char *end = NULL;
    long n = strtol(p, &end, 10);
    if (end == p) return;
    *num = (int32_t)n;
    *has_num = 1;
    if (*end == '/') {
        const char *q = end + 1;
        long d = strtol(q, &end, 10);
        if (end != q) {
            *den = (int32_t)d;
            *has_den = 1;
        }
    }
}

static uint32_t dict_count(const AVDictionary *d) {
    uint32_t n = 0;
    const AVDictionaryEntry *e = NULL;
    while ((e = av_dict_iterate(d, e))) n++;
    return n;
}

/* Build the immutable metadata snapshot for the selected stream with a
 * two-pass strategy so every returned view is stable:
 *   pass 1: measure the exact byte totals of the canonical and raw string
 *           buffers;
 *   pass 2: allocate once and copy, constructing views into fixed buffers.
 * No realloc ever moves a view after it is published.
 * Returns 0 on success, -1 on allocation failure (the partial snapshot is
 * freed by the next build or by cleanup). */
static int metadata_build(song_handle *h) {
    free(h->meta_buf);
    h->meta_buf = NULL;
    h->meta_buf_len = 0;
    free(h->raw_buf);
    h->raw_buf = NULL;
    h->raw_buf_len = 0;
    free(h->raw);
    h->raw = NULL;
    h->raw_count = 0;

    AVStream *st = h->fmt->streams[h->audio_streams[h->selected]];
    song_metadata *m = &h->meta;
    memset(m, 0, sizeof(*m));

    /* Canonical fields: selected stream overrides container. */
    struct canon {
        const char *key;
        const char **dst;
        uint32_t *len;
        uint32_t *has;
    } canon[] = {
        {"title", &m->title, &m->title_len, &m->has_title},
        {"artist", &m->artist, &m->artist_len, &m->has_artist},
        {"album", &m->album, &m->album_len, &m->has_album},
        {"album_artist", &m->album_artist, &m->album_artist_len,
         &m->has_album_artist},
        {"genre", &m->genre, &m->genre_len, &m->has_genre},
        {"composer", &m->composer, &m->composer_len, &m->has_composer},
        {"date", &m->date, &m->date_len, &m->has_date},
        {"comment", &m->comment, &m->comment_len, &m->has_comment},
    };

    const char *canon_val[sizeof(canon) / sizeof(canon[0])] = {0};
    size_t meta_bytes = 0;
    for (size_t i = 0; i < sizeof(canon) / sizeof(canon[0]); ++i) {
        const AVDictionaryEntry *e = av_dict_get(st->metadata, canon[i].key,
                                                 NULL, 0);
        if (!e) e = av_dict_get(h->fmt->metadata, canon[i].key, NULL, 0);
        if (e && e->value) {
            canon_val[i] = e->value;
            meta_bytes += strlen(e->value) + 1;
        }
    }

    /* Raw enumeration: container scope first, then selected-stream scope,
     * each in source parse order (deterministic for a given file). */
    uint32_t cc = dict_count(h->fmt->metadata);
    uint32_t sc = dict_count(st->metadata);
    uint32_t total = cc + sc;
    size_t raw_bytes = 0;
    if (total > 0) {
        const AVDictionaryEntry *e = NULL;
        while ((e = av_dict_iterate(h->fmt->metadata, e)))
            raw_bytes += strlen(e->key) + 1 +
                         (e->value ? strlen(e->value) + 1 : 1);
        e = NULL;
        while ((e = av_dict_iterate(st->metadata, e)))
            raw_bytes += strlen(e->key) + 1 +
                         (e->value ? strlen(e->value) + 1 : 1);
    }

    /* Single allocations: after this point no buffer is ever reallocated. */
    if (meta_bytes > 0) {
        h->meta_buf = (char *)malloc(meta_bytes);
        if (!h->meta_buf) return -1;
    }
    if (raw_bytes > 0) {
        h->raw_buf = (char *)malloc(raw_bytes);
        if (!h->raw_buf) return -1;
    }
    if (total > 0) {
        h->raw = (song_metadata_entry *)calloc(total, sizeof(*h->raw));
        if (!h->raw) return -1;
    }
    h->meta_buf_len = meta_bytes;
    h->raw_buf_len = raw_bytes;

    /* Pass 2: copy strings and construct stable views. */
    char *p = h->meta_buf;
    for (size_t i = 0; i < sizeof(canon) / sizeof(canon[0]); ++i) {
        if (!canon_val[i]) continue;
        size_t n = strlen(canon_val[i]);
        memcpy(p, canon_val[i], n);
        p[n] = '\0';
        *canon[i].dst = p;
        *canon[i].len = (uint32_t)n;
        *canon[i].has = 1;
        p += n + 1;
    }

    parse_pair(st, h->fmt, "track", &m->track_number, &m->has_track_number,
               &m->track_total, &m->has_track_total);
    parse_pair(st, h->fmt, "disc", &m->disc_number, &m->has_disc_number,
               &m->disc_total, &m->has_disc_total);

    /* ReplayGain: the backend parses tag values into the AVReplayGain
     * side data on the selected stream (MP3 ID3 TXXX, FLAC Vorbis comment,
     * M4A, Ogg). */
    for (int i = 0; i < st->codecpar->nb_coded_side_data; ++i) {
        const AVPacketSideData *sd = &st->codecpar->coded_side_data[i];
        if (sd->type == AV_PKT_DATA_REPLAYGAIN &&
            sd->size == (int)sizeof(AVReplayGain)) {
            const AVReplayGain *rg = (const AVReplayGain *)sd->data;
            if (rg->track_gain != INT32_MIN) {
                m->track_gain_mb = rg->track_gain;
                m->has_track_gain = 1;
            }
            if (rg->track_peak != 0) {
                m->track_peak = rg->track_peak;
                m->has_track_peak = 1;
            }
            if (rg->album_gain != INT32_MIN) {
                m->album_gain_mb = rg->album_gain;
                m->has_album_gain = 1;
            }
            if (rg->album_peak != 0) {
                m->album_peak = rg->album_peak;
                m->has_album_peak = 1;
            }
            break;
        }
    }

    char *r = h->raw_buf;
    uint32_t idx = 0;
    for (int pass = 0; pass < 2; ++pass) {
        const AVDictionary *d = pass == 0 ? h->fmt->metadata : st->metadata;
        uint32_t scope = pass == 0 ? SONG_METADATA_SCOPE_CONTAINER
                                   : SONG_METADATA_SCOPE_STREAM;
        const AVDictionaryEntry *e = NULL;
        while ((e = av_dict_iterate(d, e))) {
            song_metadata_entry *ent = &h->raw[idx];
            size_t kn = strlen(e->key);
            size_t vn = e->value ? strlen(e->value) : 0;
            ent->scope = scope;
            memcpy(r, e->key, kn);
            r[kn] = '\0';
            ent->key = r;
            ent->key_len = (uint32_t)kn;
            r += kn + 1;
            if (vn > 0) memcpy(r, e->value, vn);
            r[vn] = '\0';
            ent->value = r;
            ent->value_len = (uint32_t)vn;
            r += vn + 1;
            idx++;
        }
    }
    h->raw_count = idx;
    return 0;
}

/* -------------------------------------------------------------------------
 * Artwork snapshot
 * ---------------------------------------------------------------------- */

static void mime_from_codec(enum AVCodecID id, const char **out_mime,
                            uint32_t *out_len) {
    const char *mime = "image/x-unknown";
    switch (id) {
    case AV_CODEC_ID_MJPEG:
        mime = "image/jpeg";
        break;
    case AV_CODEC_ID_PNG:
        mime = "image/png";
        break;
    case AV_CODEC_ID_BMP:
        mime = "image/bmp";
        break;
    case AV_CODEC_ID_GIF:
        mime = "image/gif";
        break;
    case AV_CODEC_ID_TIFF:
        mime = "image/tiff";
        break;
    case AV_CODEC_ID_WEBP:
        mime = "image/webp";
        break;
    case AV_CODEC_ID_JPEG2000:
        mime = "image/jp2";
        break;
    default:
        break;
    }
    *out_mime = mime;
    *out_len = (uint32_t)strlen(mime);
}

static void artwork_role_from_comment(const AVDictionary *metadata,
                                      uint32_t *role) {
    const AVDictionaryEntry *c = av_dict_get(metadata, "comment", NULL, 0);
    if (!c || !c->value) {
        *role = SONG_ARTWORK_UNKNOWN;
        return;
    }
    if (!strcmp(c->value, "Cover (front)"))
        *role = SONG_ARTWORK_FRONT_COVER;
    else if (!strcmp(c->value, "Cover (back)"))
        *role = SONG_ARTWORK_BACK_COVER;
    else if (!strcmp(c->value, "Other"))
        *role = SONG_ARTWORK_OTHER;
    else
        *role = SONG_ARTWORK_UNKNOWN;
}

/* Build the compressed-artwork snapshot from all attached-picture streams.
 * Returns 0 on success, -1 on allocation failure. */
static int artwork_build(song_handle *h) {
    free(h->art);
    h->art = NULL;
    h->art_count = 0;
    free(h->art_buf);
    h->art_buf = NULL;
    h->art_buf_len = 0;

    uint32_t count = 0;
    uint64_t total = 0;
    for (unsigned i = 0; i < h->fmt->nb_streams; ++i) {
        AVStream *st = h->fmt->streams[i];
        if ((st->disposition & AV_DISPOSITION_ATTACHED_PIC) &&
            st->attached_pic.size > 0) {
            count++;
            total += (uint64_t)st->attached_pic.size;
        }
    }
    if (count == 0) return 0;
    if (total > SIZE_MAX) return -1;

    h->art = (struct art_item *)calloc(count, sizeof(*h->art));
    if (!h->art) return -1;
    h->art_buf = (uint8_t *)malloc((size_t)total);
    if (!h->art_buf) {
        free(h->art);
        h->art = NULL;
        return -1;
    }
    h->art_count = count;

    size_t off = 0;
    uint32_t idx = 0;
    for (unsigned i = 0; i < h->fmt->nb_streams && idx < count; ++i) {
        AVStream *st = h->fmt->streams[i];
        if (!(st->disposition & AV_DISPOSITION_ATTACHED_PIC) ||
            st->attached_pic.size <= 0)
            continue;
        struct art_item *it = &h->art[idx];
        memcpy(h->art_buf + off, st->attached_pic.data,
               (size_t)st->attached_pic.size);
        it->data = h->art_buf + off;
        it->data_len = (uint64_t)st->attached_pic.size;
        off += (size_t)st->attached_pic.size;
        mime_from_codec(st->codecpar->codec_id, &it->mime, &it->mime_len);
        it->width = st->codecpar->width > 0 ? st->codecpar->width : -1;
        it->height = st->codecpar->height > 0 ? st->codecpar->height : -1;
        artwork_role_from_comment(st->metadata, &it->role);
        idx++;
    }

    /* Front-cover policy (frozen): a single artwork is the front cover;
     * otherwise only items explicitly labelled front cover are. */
    if (count == 1) {
        h->art[0].is_front_cover = 1;
    } else {
        for (uint32_t k = 0; k < count; ++k)
            h->art[k].is_front_cover =
                (h->art[k].role == SONG_ARTWORK_FRONT_COVER);
    }
    h->art_buf_len = (size_t)total;
    return 0;
}

/* -------------------------------------------------------------------------
 * Decoder management
 * ---------------------------------------------------------------------- */

static song_status open_decoder_for(song_handle *h, uint32_t audio_index,
                                    AVCodecContext **out_dec) {
    int sidx = h->audio_streams[audio_index];
    AVStream *st = h->fmt->streams[sidx];
    const AVCodec *codec = avcodec_find_decoder(st->codecpar->codec_id);
    if (!codec) return SONG_ERR_UNSUPPORTED_CODEC;
    AVCodecContext *dec = avcodec_alloc_context3(codec);
    if (!dec) return SONG_ERR_OUT_OF_MEMORY;
    if (avcodec_parameters_to_context(dec, st->codecpar) < 0) {
        avcodec_free_context(&dec);
        return SONG_ERR_CORRUPT_DATA;
    }
    if (avcodec_open2(dec, codec, NULL) < 0) {
        avcodec_free_context(&dec);
        return SONG_ERR_UNSUPPORTED_CODEC;
    }
    *out_dec = dec;
    return SONG_OK;
}

/* -------------------------------------------------------------------------
 * Cleanup
 * ---------------------------------------------------------------------- */

static void cleanup(song_handle *h) {
    if (!h) return;
    av_packet_free(&h->packet);
    av_frame_free(&h->frame);
    avcodec_free_context(&h->dec);
    if (h->fmt) {
        h->fmt->pb = NULL;
        h->fmt->flags &= ~(unsigned)AVFMT_FLAG_CUSTOM_IO;
        avformat_close_input(&h->fmt);
    }
    if (h->avio) {
        av_freep(&h->avio->buffer);
        avio_context_free(&h->avio);
    }
    free(h->audio_streams);
    free(h->meta_buf);
    free(h->raw_buf);
    free(h->raw);
    free(h->art);
    free(h->art_buf);
    free(h->pcm);
    free(h);
}

/* -------------------------------------------------------------------------
 * Public ABI
 * ---------------------------------------------------------------------- */

uint32_t songcore_abi_version(void) { return SONGCORE_ABI_VERSION; }

song_status song_open(const song_io *io, song_handle **out_handle) {
    if (!io || !out_handle || !io->read || !io->seek || !io->size)
        return SONG_ERR_INVALID_ARGUMENT;
    *out_handle = NULL;

    song_handle *h = (song_handle *)calloc(1, sizeof(*h));
    if (!h) return SONG_ERR_OUT_OF_MEMORY;
    h->io = *io;
    h->selected = 0;

    uint8_t *avio_buffer = (uint8_t *)av_malloc(SONG_AVIO_BUFFER_SIZE);
    if (!avio_buffer) {
        free(h);
        return SONG_ERR_OUT_OF_MEMORY;
    }
    h->avio = avio_alloc_context(avio_buffer, SONG_AVIO_BUFFER_SIZE, 0, h,
                                 io_read, NULL, io_seek);
    if (!h->avio) {
        av_free(avio_buffer);
        free(h);
        return SONG_ERR_OUT_OF_MEMORY;
    }

    h->fmt = avformat_alloc_context();
    if (!h->fmt) {
        avio_context_free(&h->avio);
        free(h);
        return SONG_ERR_OUT_OF_MEMORY;
    }
    h->fmt->pb = h->avio;
    h->fmt->flags |= AVFMT_FLAG_CUSTOM_IO;

    int ret = avformat_open_input(&h->fmt, "", NULL, NULL);
    if (ret < 0) {
        song_status st;
        if (ret == AVERROR(EIO))
            st = SONG_ERR_IO;
        else if (ret == AVERROR(ENOMEM))
            st = SONG_ERR_OUT_OF_MEMORY;
        else
            st = SONG_ERR_UNSUPPORTED_CONTAINER;
        cleanup(h);
        return st;
    }
    *out_handle = h;
    return SONG_OK;
}

song_status song_probe(song_handle *h, song_info *out_info) {
    if (!h || !out_info) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->fmt) return SONG_ERR_NOT_OPEN;

    if (!h->probed) {
        int ret = avformat_find_stream_info(h->fmt, NULL);
        if (ret < 0) {
            if (ret == AVERROR(EIO))
                return set_error(h, SONG_ERR_IO, ret, "stream info failed");
            if (ret == AVERROR(ENOMEM))
                return set_error(h, SONG_ERR_OUT_OF_MEMORY, ret,
                                 "stream info failed");
            return set_error(h, SONG_ERR_CORRUPT_DATA, ret,
                             "stream info failed");
        }

        /* Enumerate decodable audio streams (exclude non-audio and
         * attached-picture streams). */
        uint32_t count = 0;
        for (unsigned i = 0; i < h->fmt->nb_streams; ++i) {
            AVStream *st = h->fmt->streams[i];
            if (st->codecpar->codec_type == AVMEDIA_TYPE_AUDIO &&
                avcodec_find_decoder(st->codecpar->codec_id))
                count++;
        }
        if (count == 0) {
            int has_audio = 0;
            for (unsigned i = 0; i < h->fmt->nb_streams; ++i)
                if (h->fmt->streams[i]->codecpar->codec_type ==
                    AVMEDIA_TYPE_AUDIO) {
                    has_audio = 1;
                    break;
                }
            return set_error(h, has_audio ? SONG_ERR_UNSUPPORTED_CODEC
                                          : SONG_ERR_NO_AUDIO_STREAM,
                             0, has_audio ? "no decodable audio codec"
                                          : "no audio stream");
        }
        h->audio_streams = (int *)malloc(count * sizeof(int));
        if (!h->audio_streams) return SONG_ERR_OUT_OF_MEMORY;
        h->audio_count = count;
        uint32_t idx = 0;
        for (unsigned i = 0; i < h->fmt->nb_streams; ++i) {
            AVStream *st = h->fmt->streams[i];
            if (st->codecpar->codec_type == AVMEDIA_TYPE_AUDIO &&
                avcodec_find_decoder(st->codecpar->codec_id))
                h->audio_streams[idx++] = (int)i;
        }

        /* Default selection: first AV_DISPOSITION_DEFAULT among decodable
         * audio streams, else the lowest stream index. */
        uint32_t sel = 0;
        for (uint32_t k = 0; k < h->audio_count; ++k)
            if (h->fmt->streams[h->audio_streams[k]]->disposition &
                AV_DISPOSITION_DEFAULT) {
                sel = k;
                break;
            }
        h->selected = sel;

        song_status st = open_decoder_for(h, sel, &h->dec);
        if (st != SONG_OK)
            return set_error(h, st, 0, "cannot open audio decoder");

        AVStream *st_ = h->fmt->streams[h->audio_streams[sel]];
        h->sample_rate = st_->codecpar->sample_rate;
        h->channels = st_->codecpar->ch_layout.nb_channels;
        h->channel_mask = channel_mask_from_layout(&st_->codecpar->ch_layout);
        if (h->sample_rate <= 0 || h->channels <= 0)
            return set_error(h, SONG_ERR_CORRUPT_DATA, 0,
                             "invalid stream parameters");

        h->packet = av_packet_alloc();
        h->frame = av_frame_alloc();
        if (!h->packet || !h->frame) return SONG_ERR_OUT_OF_MEMORY;

        if (metadata_build(h) < 0 || artwork_build(h) < 0)
            return SONG_ERR_OUT_OF_MEMORY;

        h->probed = 1;
        clear_error(h);
    }

    AVStream *st = h->fmt->streams[h->audio_streams[h->selected]];
    memset(out_info, 0, sizeof(*out_info));
    out_info->sample_rate = h->sample_rate;
    out_info->channels = h->channels;
    out_info->channel_mask = h->channel_mask;
    out_info->duration_us =
        h->fmt->duration == AV_NOPTS_VALUE ? -1 : h->fmt->duration;
    out_info->bits_per_sample = st->codecpar->bits_per_raw_sample > 0
                                    ? st->codecpar->bits_per_raw_sample
                                    : st->codecpar->bits_per_coded_sample;
    copy_name(out_info->codec, avcodec_get_name(st->codecpar->codec_id));
    copy_name(out_info->container,
              h->fmt->iformat ? h->fmt->iformat->name : NULL);
    out_info->selected_audio_index = h->selected;
    out_info->audio_stream_count = h->audio_count;
    return SONG_OK;
}

song_status song_audio_stream_count(song_handle *h, uint32_t *out_count) {
    if (!h || !out_count) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    *out_count = h->audio_count;
    return SONG_OK;
}

song_status song_audio_stream_info(song_handle *h, uint32_t audio_index,
                                   song_stream_info *out_info) {
    if (!h || !out_info) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    if (audio_index >= h->audio_count) return SONG_ERR_INVALID_ARGUMENT;
    AVStream *st = h->fmt->streams[h->audio_streams[audio_index]];
    memset(out_info, 0, sizeof(*out_info));
    out_info->audio_index = audio_index;
    out_info->stream_index = h->audio_streams[audio_index];
    out_info->sample_rate = st->codecpar->sample_rate;
    out_info->channels = st->codecpar->ch_layout.nb_channels;
    out_info->channel_mask = channel_mask_from_layout(&st->codecpar->ch_layout);
    out_info->duration_us =
        st->duration == AV_NOPTS_VALUE
            ? -1
            : av_rescale_q(st->duration, st->time_base,
                           (AVRational){1, AV_TIME_BASE});
    out_info->bits_per_sample = st->codecpar->bits_per_raw_sample > 0
                                    ? st->codecpar->bits_per_raw_sample
                                    : st->codecpar->bits_per_coded_sample;
    copy_name(out_info->codec, avcodec_get_name(st->codecpar->codec_id));
    out_info->is_default =
        (st->disposition & AV_DISPOSITION_DEFAULT) ? 1 : 0;
    return SONG_OK;
}

/* Reposition demux/IO state to the container start so the newly selected
 * stream decodes from its beginning (ABI: a switched stream starts from
 * start). Returns 0 on success, <0 when the container cannot rewind. */
static int rewind_demux(song_handle *h) {
    int sidx = h->audio_streams[h->selected];
    int ret = av_seek_frame(h->fmt, sidx, 0, AVSEEK_FLAG_BACKWARD);
    if (ret < 0)
        ret = av_seek_frame(h->fmt, -1, 0, AVSEEK_FLAG_BACKWARD);
    return ret;
}

song_status song_select_stream(song_handle *h, uint32_t audio_index) {
    if (!h) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    if (audio_index >= h->audio_count) return SONG_ERR_INVALID_ARGUMENT;

    AVCodecContext *new_dec = NULL;
    song_status st = open_decoder_for(h, audio_index, &new_dec);
    if (st != SONG_OK)
        return set_error(h, st, 0, "cannot open selected stream decoder");

    /* Swap decoders, rewind the source, reset decode + PCM state, rebuild
     * the metadata snapshot. Artwork is container-level and stays valid. */
    avcodec_free_context(&h->dec);
    h->dec = new_dec;
    h->selected = audio_index;
    reset_decode_state(h);

    if (rewind_demux(h) < 0)
        return set_error(h, SONG_ERR_SEEK_ERROR, 0,
                         "cannot rewind source for stream switch");

    AVStream *st_ = h->fmt->streams[h->audio_streams[audio_index]];
    h->sample_rate = st_->codecpar->sample_rate;
    h->channels = st_->codecpar->ch_layout.nb_channels;
    h->channel_mask = channel_mask_from_layout(&st_->codecpar->ch_layout);

    if (metadata_build(h) < 0) return SONG_ERR_OUT_OF_MEMORY;
    clear_error(h);
    return SONG_OK;
}

song_status song_get_metadata(song_handle *h, const song_metadata **out_meta) {
    if (!h || !out_meta) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    *out_meta = &h->meta;
    return SONG_OK;
}

song_status song_get_metadata_count(song_handle *h, uint32_t *out_count) {
    if (!h || !out_count) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    *out_count = h->raw_count;
    return SONG_OK;
}

song_status song_get_metadata_entry(song_handle *h, uint32_t index,
                                song_metadata_entry *out_entry) {
    if (!h || !out_entry) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    if (index >= h->raw_count) return SONG_ERR_INVALID_ARGUMENT;
    *out_entry = h->raw[index];
    return SONG_OK;
}

song_status song_get_artwork_count(song_handle *h, uint32_t *out_count) {
    if (!h || !out_count) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    *out_count = h->art_count;
    return SONG_OK;
}

song_status song_get_artwork_item(song_handle *h, uint32_t index,
                              song_artwork_item *out_item) {
    if (!h || !out_item) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed) return SONG_ERR_NOT_OPEN;
    if (index >= h->art_count) return SONG_ERR_INVALID_ARGUMENT;
    const struct art_item *it = &h->art[index];
    memset(out_item, 0, sizeof(*out_item));
    out_item->role = it->role;
    out_item->mime = it->mime;
    out_item->mime_len = it->mime_len;
    out_item->data = it->data;
    out_item->data_len = it->data_len;
    out_item->width = it->width;
    out_item->height = it->height;
    out_item->is_front_cover = it->is_front_cover;
    return SONG_OK;
}

song_status song_read_pcm(song_handle *h, float *dst, uint64_t frame_capacity,
                          uint64_t *out_frames_produced) {
    if (!h || !dst || !out_frames_produced) return SONG_ERR_INVALID_ARGUMENT;
    *out_frames_produced = 0;
    if (frame_capacity == 0) return SONG_ERR_INVALID_ARGUMENT;
    if (!h->probed || !h->dec) return SONG_ERR_NOT_OPEN;
    if (h->fatal_error != SONG_OK)
        return set_error(h, h->fatal_error, h->last_native,
                         "pending decode error");

    uint64_t produced = 0;
    while (produced < frame_capacity) {
        if (h->pcm_offset_frames < h->pcm_frames) {
            size_t available = h->pcm_frames - h->pcm_offset_frames;
            size_t take = (size_t)(frame_capacity - produced);
            if (take > available) take = available;
            memcpy(dst + produced * (size_t)h->channels,
                   h->pcm + h->pcm_offset_frames * (size_t)h->channels,
                   take * (size_t)h->channels * sizeof(float));
            produced += take;
            h->pcm_offset_frames += take;
            continue;
        }

        h->pcm_frames = 0;
        h->pcm_offset_frames = 0;
        if (h->decoder_eof) break;

        int decoded = decode_one_frame(h);
        if (decoded == 0) break;
        if (decoded < 0) {
            song_status st = decode_status(h);
            av_frame_unref(h->frame);
            h->fatal_error = st;
            set_error(h, st, h->last_native, "decode error");
            break;
        }
        int conv = frame_to_f32(h, h->frame);
        av_frame_unref(h->frame);
        if (conv < 0) {
            song_status st =
                (conv == -2) ? SONG_ERR_STREAM_CHANGE : SONG_ERR_DECODE_ERROR;
            h->fatal_error = st;
            set_error(h, st, 0, conv == -2
                                     ? "decoder changed format mid-stream"
                                     : "sample format conversion failed");
            break;
        }
    }

    *out_frames_produced = produced;
    if (produced > 0) return SONG_OK;
    if (h->fatal_error != SONG_OK) return h->fatal_error;
    return SONG_EOF;
}

song_status song_seek(song_handle *h, int64_t requested_position_us,
                      int64_t *out_actual_position_us) {
    if (!h) return SONG_ERR_INVALID_ARGUMENT;
    if (out_actual_position_us) *out_actual_position_us = -1;
    if (!h->probed || !h->dec) return SONG_ERR_NOT_OPEN;
    if (requested_position_us < 0) return SONG_ERR_INVALID_ARGUMENT;

    int64_t target = requested_position_us;
    if (h->fmt->duration != AV_NOPTS_VALUE && h->fmt->duration > 0 &&
        target > h->fmt->duration)
        target = h->fmt->duration;

    int sidx = h->audio_streams[h->selected];
    AVStream *st = h->fmt->streams[sidx];
    int64_t ts = av_rescale_q(target, (AVRational){1, AV_TIME_BASE},
                              st->time_base);
    int ret = av_seek_frame(h->fmt, sidx, ts, AVSEEK_FLAG_BACKWARD);
    if (ret < 0) {
        /* Raw/unindexed containers (raw ADTS: frames carry no timestamps and
         * the demuxer defines no read_seek) can never land anywhere; avformat
         * reports that as a generic -1 rather than ENOSYS. No declared
         * duration and no stream start/duration is the public-side shape of
         * such sources. The ABI types this class as SEEK_UNSUPPORTED
         * ("container has no seek"); SEEK_ERROR stays for seeks that should
         * work but failed. */
        int container_has_no_seek =
            h->fmt->duration == AV_NOPTS_VALUE &&
            st->start_time == AV_NOPTS_VALUE &&
            st->duration == AV_NOPTS_VALUE;
        song_status type =
            (ret == AVERROR(ENOSYS) || container_has_no_seek)
                ? SONG_ERR_SEEK_UNSUPPORTED : SONG_ERR_SEEK_ERROR;
        return set_error(h, type, ret, "container seek failed");
    }
    avcodec_flush_buffers(h->dec);
    reset_decode_state(h);
    clear_error(h);

    /* Measure the effective landing from the first decoded frame after the
     * seek. The frame is held and returned by the next song_read_pcm. */
    int decoded = decode_one_frame(h);
    if (decoded == 1) {
        int conv = frame_to_f32(h, h->frame);
        int64_t pts = h->frame->best_effort_timestamp;
        av_frame_unref(h->frame);
        if (conv < 0) {
            /* Fail-closed: a landing frame that cannot be converted must
             * not be silently skipped behind a SONG_OK. */
            song_status st = (conv == -2) ? SONG_ERR_STREAM_CHANGE
                                          : SONG_ERR_DECODE_ERROR;
            h->fatal_error = st;
            return set_error(h, st, 0, conv == -2
                                 ? "decoder changed format after seek"
                                 : "sample format conversion failed after seek");
        }
        if (pts != AV_NOPTS_VALUE && out_actual_position_us)
            *out_actual_position_us =
                av_rescale_q(pts, st->time_base,
                             (AVRational){1, AV_TIME_BASE});
        /* else: landing unknown; actual stays -1 (explicit). */
    } else if (decoded < 0) {
        av_frame_unref(h->frame);
        return set_error(h, SONG_ERR_SEEK_ERROR, h->last_native,
                         "could not reach a landing point");
    }
    return SONG_OK;
}

song_status song_last_error(song_handle *h, const song_error **out_error) {
    if (!h || !out_error) return SONG_ERR_INVALID_ARGUMENT;
    *out_error = &h->err;
    return SONG_OK;
}

void song_close(song_handle *h) { cleanup(h); }
