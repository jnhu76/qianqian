/*
 * E10-A1 FFmpeg libswresample adapter — built from the pinned Qianqian
 * FFmpeg source (bench/ffmpeg-pin.json: n9.0.1), NOT a new dependency.
 * Float32 interleaved in/out, same channel count, rate-only.
 */
#include "a1_contract.h"

#include <libswresample/swresample.h>
#include <libavutil/channel_layout.h>
#include <libavutil/samplefmt.h>
#include <libavutil/opt.h>

#include <math.h>
#include <stdlib.h>

typedef struct {
    struct SwrContext *ctx;
    int in_rate, out_rate, channels;
    long max_frames;
} swr_t;

static int swr_prepare(a1_src *s, int in_rate, int out_rate, int channels,
                       long max_frames) {
    swr_t *w = (swr_t *)s->impl;
    AVChannelLayout in_lay, out_lay;
    int ret;
    if (in_rate <= 0 || out_rate <= 0 || channels <= 0 || max_frames < 0)
        return -4;
    swr_free(&w->ctx);
    av_channel_layout_default(&in_lay, channels);
    av_channel_layout_default(&out_lay, channels);
    ret = swr_alloc_set_opts2(&w->ctx, &out_lay, AV_SAMPLE_FMT_FLT, out_rate,
                              &in_lay, AV_SAMPLE_FMT_FLT, in_rate, 0, NULL);
    if (ret < 0) return -1;
    if (swr_init(w->ctx) < 0) {
        swr_free(&w->ctx);
        return -1;
    }
    w->in_rate = in_rate;
    w->out_rate = out_rate;
    w->channels = channels;
    w->max_frames = max_frames;
    return 0;
}

static int swr_process(a1_src *s, const float *in, long in_frames,
                       float *out, long out_cap,
                       long *consumed, long *produced, int is_last) {
    swr_t *w = (swr_t *)s->impl;
    (void)is_last;
    int n = swr_convert(w->ctx, (uint8_t **)&out, (int)out_cap,
                        (const uint8_t **)&in, (int)in_frames);
    if (n < 0) return -1;
    /* swr_convert always consumes the input it is given; surplus is held
     * in the internal FIFO (counted as buffered input). */
    *consumed = in_frames;
    *produced = n;
    return 0;
}

static int swr_drain(a1_src *s, float *out, long out_cap, long *produced) {
    swr_t *w = (swr_t *)s->impl;
    int n = swr_convert(w->ctx, (uint8_t **)&out, (int)out_cap, NULL, 0);
    if (n < 0) return -1;
    *produced = n;
    return 0;
}

static int swr_reset(a1_src *s) {
    swr_t *w = (swr_t *)s->impl;
    swr_close(w->ctx);
    return swr_init(w->ctx);
}

static long swr_latency(const a1_src *s) {
    const swr_t *w = (const swr_t *)s->impl;
    if (!w->ctx) return -1;
    return (long)swr_get_delay(w->ctx, w->out_rate);
}

static long swr_required(const a1_src *s, long out_frames) {
    const swr_t *w = (const swr_t *)s->impl;
    long delay_in = (long)swr_get_delay(w->ctx, w->in_rate);
    return delay_in + (long)ceil((double)out_frames * w->in_rate /
                                 (double)w->out_rate);
}

static void swr_destroy(a1_src *s) {
    swr_t *w = (swr_t *)s->impl;
    swr_free(&w->ctx);
    free(w);
    s->impl = NULL;
}

static const a1_src_ops kOps = {
    "swr", swr_prepare, swr_process, swr_drain, swr_reset,
    swr_latency, swr_required, swr_destroy,
};

int a1_make(const char *name, a1_src *out) {
    if (strcmp(name, "swr") != 0) return -1;
    out->ops = &kOps;
    out->impl = calloc(1, sizeof(swr_t));
    return out->impl ? 0 : -1;
}
