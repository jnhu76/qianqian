/*
 * E10-A1 libsamplerate adapter (0.2.2, SRC_SINC_BEST_QUALITY).
 * Float32 interleaved, same channel count, rate-only.
 */
#include "a1_contract.h"

#include <samplerate.h>

#include <math.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    SRC_STATE *st;
    int in_rate, out_rate, channels;
    long max_frames;
} lsr_t;

static int lsr_prepare(a1_src *s, int in_rate, int out_rate, int channels,
                       long max_frames) {
    lsr_t *l = (lsr_t *)s->impl;
    int err = 0;
    if (in_rate <= 0 || out_rate <= 0 || channels <= 0 || max_frames < 0)
        return -4;
    if (l->st) src_delete(l->st);
    l->st = src_new(SRC_SINC_BEST_QUALITY, channels, &err);
    if (!l->st || err) return -1;
    if (src_set_ratio(l->st, (double)out_rate / (double)in_rate) != 0)
        return -1;
    l->in_rate = in_rate;
    l->out_rate = out_rate;
    l->channels = channels;
    l->max_frames = max_frames;
    return 0;
}

static int lsr_process(a1_src *s, const float *in, long in_frames,
                       float *out, long out_cap,
                       long *consumed, long *produced, int is_last) {
    lsr_t *l = (lsr_t *)s->impl;
    SRC_DATA d;
    long in_use = in_frames;
    if (in_use > 0 && (in_frames * l->channels > 0x7fffffffL))
        in_use = 0x7fffffffL / l->channels;
    memset(&d, 0, sizeof(d));
    d.data_in = (float *)in;
    d.input_frames = (long)in_use;
    d.data_out = out;
    d.output_frames = (long)out_cap;
    /* libsamplerate flushes its filter tail only when end_of_input is set
     * on the final data call (its drain is not a "flush to silence"). */
    d.end_of_input = is_last;
    d.src_ratio = (double)l->out_rate / (double)l->in_rate;
    if (src_process(l->st, &d) != 0) return -1;
    *consumed = d.input_frames_used;
    *produced = d.output_frames_gen;
    return 0;
}

static int lsr_drain(a1_src *s, float *out, long out_cap, long *produced) {
    lsr_t *l = (lsr_t *)s->impl;
    SRC_DATA d;
    memset(&d, 0, sizeof(d));
    d.data_in = NULL;
    d.input_frames = 0;
    d.data_out = out;
    d.output_frames = (long)out_cap;
    d.end_of_input = 1; /* safety; flush already requested via is_last */
    d.src_ratio = (double)l->out_rate / (double)l->in_rate;
    if (src_process(l->st, &d) != 0) return -1;
    *produced = d.output_frames_gen;
    return 0;
}

static int lsr_reset(a1_src *s) {
    lsr_t *l = (lsr_t *)s->impl;
    return src_reset(l->st) == 0 ? 0 : -1;
}

static long lsr_latency(const a1_src *s) {
    (void)s;
    /* libsamplerate does not expose a latency query; the measured delay
     * is reported from the impulse analysis instead. -1 = not expressible
     * by the library itself. */
    return -1;
}

static long lsr_required(const a1_src *s, long out_frames) {
    const lsr_t *l = (const lsr_t *)s->impl;
    return (long)ceil((double)out_frames * l->in_rate / (double)l->out_rate) +
           (long)l->max_frames; /* conservative: worst-case internal buffering */
}

static void lsr_destroy(a1_src *s) {
    lsr_t *l = (lsr_t *)s->impl;
    if (l->st) src_delete(l->st);
    free(l);
    s->impl = NULL;
}

static const a1_src_ops kOps = {
    "lsr", lsr_prepare, lsr_process, lsr_drain, lsr_reset,
    lsr_latency, lsr_required, lsr_destroy,
};

int a1_make(const char *name, a1_src *out) {
    if (strcmp(name, "lsr") != 0) return -1;
    out->ops = &kOps;
    out->impl = calloc(1, sizeof(lsr_t));
    return out->impl ? 0 : -1;
}
