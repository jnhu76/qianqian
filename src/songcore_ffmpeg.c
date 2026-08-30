#include "songcore.h"

#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>
#include <libavutil/samplefmt.h>

#define SONG_AVIO_BUFFER_SIZE 32768

struct song_handle {
    song_io io;
    int64_t io_pos;

    AVFormatContext *fmt;
    AVCodecContext *dec;
    AVIOContext *avio;
    AVPacket *packet;
    AVFrame *frame;

    int audio_index;
    int probed;
    int packet_pending;
    int demux_eof;
    int drain_sent;
    int decoder_eof;
    int fatal_error;

    int sample_rate;
    int channels;

    float *pcm;
    size_t pcm_capacity_floats;
    size_t pcm_frames;
    size_t pcm_offset_frames;
};

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

static void reset_decode_state(song_handle *h) {
    if (h->packet) av_packet_unref(h->packet);
    if (h->frame) av_frame_unref(h->frame);
    h->packet_pending = 0;
    h->demux_eof = 0;
    h->drain_sent = 0;
    h->decoder_eof = 0;
    h->fatal_error = 0;
    h->pcm_frames = 0;
    h->pcm_offset_frames = 0;
}

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
    free(h->pcm);
    free(h);
}

song_handle *song_open(const song_io *io) {
    if (!io || !io->read || !io->seek || !io->size) return NULL;

    song_handle *h = (song_handle *)calloc(1, sizeof(*h));
    if (!h) return NULL;
    h->io = *io;
    h->audio_index = -1;

    uint8_t *avio_buffer = (uint8_t *)av_malloc(SONG_AVIO_BUFFER_SIZE);
    if (!avio_buffer) {
        cleanup(h);
        return NULL;
    }
    h->avio = avio_alloc_context(avio_buffer, SONG_AVIO_BUFFER_SIZE, 0, h,
                                 io_read, NULL, io_seek);
    if (!h->avio) {
        av_free(avio_buffer);
        cleanup(h);
        return NULL;
    }

    h->fmt = avformat_alloc_context();
    if (!h->fmt) {
        cleanup(h);
        return NULL;
    }
    h->fmt->pb = h->avio;
    h->fmt->flags |= AVFMT_FLAG_CUSTOM_IO;

    if (avformat_open_input(&h->fmt, "", NULL, NULL) < 0) {
        cleanup(h);
        return NULL;
    }
    return h;
}

static void copy_name(char dst[32], const char *src) {
    if (!src) src = "unknown";
    snprintf(dst, 32, "%s", src);
}

int song_probe(song_handle *h, song_info *out_info) {
    if (!h || !out_info) return -1;

    if (!h->probed) {
        if (avformat_find_stream_info(h->fmt, NULL) < 0) return -1;

        int audio_index = -1;
        for (unsigned i = 0; i < h->fmt->nb_streams; ++i) {
            if (h->fmt->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
                audio_index = (int)i;
                break;
            }
        }
        if (audio_index < 0) return -1;

        AVStream *st = h->fmt->streams[audio_index];
        const AVCodec *codec = avcodec_find_decoder(st->codecpar->codec_id);
        if (!codec) return -1;

        AVCodecContext *dec = avcodec_alloc_context3(codec);
        AVPacket *packet = NULL;
        AVFrame *frame = NULL;
        if (!dec) return -1;
        if (avcodec_parameters_to_context(dec, st->codecpar) < 0 ||
            avcodec_open2(dec, codec, NULL) < 0) {
            avcodec_free_context(&dec);
            return -1;
        }
        packet = av_packet_alloc();
        frame = av_frame_alloc();
        if (!packet || !frame) {
            av_packet_free(&packet);
            av_frame_free(&frame);
            avcodec_free_context(&dec);
            return -1;
        }

        int sample_rate = st->codecpar->sample_rate;
        int channels = st->codecpar->ch_layout.nb_channels;
        if (sample_rate <= 0 || channels <= 0) {
            av_packet_free(&packet);
            av_frame_free(&frame);
            avcodec_free_context(&dec);
            return -1;
        }

        h->audio_index = audio_index;
        h->dec = dec;
        h->packet = packet;
        h->frame = frame;
        h->sample_rate = sample_rate;
        h->channels = channels;
        h->probed = 1;
    }

    AVStream *st = h->fmt->streams[h->audio_index];
    memset(out_info, 0, sizeof(*out_info));
    out_info->sample_rate = h->sample_rate;
    out_info->channels = h->channels;
    out_info->duration_us = h->fmt->duration;
    out_info->bits_per_sample = st->codecpar->bits_per_raw_sample > 0
        ? st->codecpar->bits_per_raw_sample
        : st->codecpar->bits_per_coded_sample;
    copy_name(out_info->codec, avcodec_get_name(st->codecpar->codec_id));
    copy_name(out_info->container, h->fmt->iformat ? h->fmt->iformat->name : NULL);
    return 0;
}

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

static int frame_to_f32(song_handle *h, const AVFrame *f) {
    const int channels = f->ch_layout.nb_channels;
    const int frames = f->nb_samples;
    if (channels != h->channels || f->sample_rate != h->sample_rate || frames < 0)
        return -1;
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

/*
 * Produce one decoded AVFrame. The packet is retained across send(EAGAIN):
 * backpressure always drains receive_frame() and retries the SAME packet.
 * This is the production state machine; no packet is unref'd until accepted.
 */
static int decode_one_frame(song_handle *h) {
    for (;;) {
        int ret = avcodec_receive_frame(h->dec, h->frame);
        if (ret >= 0) return 1;
        if (ret == AVERROR_EOF) {
            h->decoder_eof = 1;
            return 0;
        }
        if (ret != AVERROR(EAGAIN)) return -1;

        if (h->packet_pending) {
            ret = avcodec_send_packet(h->dec, h->packet);
            if (ret == AVERROR(EAGAIN)) continue;
            if (ret < 0) return -1;
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
                    return -1;
                }
                if (h->packet->stream_index == h->audio_index) {
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
            if (ret < 0) return -1;
            h->drain_sent = 1;
            continue;
        }

        /* After a successful drain signal, receive must converge to frames or EOF. */
        return -1;
    }
}

int64_t song_read_pcm(song_handle *h, float *output, size_t frame_capacity) {
    if (!h || !output || frame_capacity == 0 || !h->probed || !h->dec) return -1;
    if (h->fatal_error) return -1;

    size_t produced = 0;
    while (produced < frame_capacity) {
        if (h->pcm_offset_frames < h->pcm_frames) {
            size_t available = h->pcm_frames - h->pcm_offset_frames;
            size_t take = frame_capacity - produced;
            if (take > available) take = available;
            memcpy(output + produced * (size_t)h->channels,
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
        if (decoded < 0 || frame_to_f32(h, h->frame) < 0) {
            av_frame_unref(h->frame);
            if (produced > 0) {
                h->fatal_error = 1;
                break;
            }
            return -1;
        }
        av_frame_unref(h->frame);
    }
    return (int64_t)produced;
}

int song_seek(song_handle *h, int64_t position_us) {
    if (!h || !h->probed || !h->dec || position_us < 0) return -1;
    AVStream *st = h->fmt->streams[h->audio_index];
    int64_t ts = av_rescale_q(position_us, (AVRational){1, AV_TIME_BASE}, st->time_base);
    if (av_seek_frame(h->fmt, h->audio_index, ts, AVSEEK_FLAG_BACKWARD) < 0) return -1;
    avcodec_flush_buffers(h->dec);
    reset_decode_state(h);
    return 0;
}

void song_close(song_handle *h) {
    cleanup(h);
}
