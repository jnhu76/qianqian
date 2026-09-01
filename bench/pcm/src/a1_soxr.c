/*
 * E10-A1 SoXR adapter (soxr 0.1.3, HQ quality). Float32 interleaved,
 * same channel count, rate-only.
 */
#include "a1_contract.h"

#include <soxr.h>

#include <math.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    soxr_t soxr;
    int in_rate, out_rate, channels;
    long max_frames;
} soxr_t2;

static int soxr_prepare(a1_src *s, int in_rate, int out_rate, int channels,
                        long max_frames) {
    soxr_t2 *x = (soxr_t2 *)s->impl;
    soxr_io_spec_t io;
    soxr_quality_spec_t q;
    soxr_error_t err;
    if (in_rate <= 0 || out_rate <= 0 || channels <= 0 || max_frames < 0)
        return -4;
    if (x->soxr) soxr_delete(x->soxr);
    x->soxr = NULL;
    io = soxr_io_spec(SOXR_FLOAT32, SOXR_FLOAT32);
    q = soxr_quality_spec(SOXR_HQ, 0);
    x->soxr = soxr_create((double)in_rate, (double)out_rate, channels,
                          &err, &io, &q, NULL);
    if (err || !x->soxr) return -1;
    x->in_rate = in_rate;
    x->out_rate = out_rate;
    x->channels = channels;
    x->max_frames = max_frames;
    return 0;
}

static int soxr_adapter_process(a1_src *s, const float *in, long in_frames,
                                float *out, long out_cap,
                                long *consumed, long *produced, int is_last) {
    soxr_t2 *x = (soxr_t2 *)s->impl;
    (void)is_last;
    size_t done = 0, used = 0;
    soxr_error_t err = soxr_process(x->soxr, in, (size_t)in_frames,
                                    &used, out, (size_t)out_cap, &done);
    if (err) return -1;
    *consumed = (long)used;
    *produced = (long)done;
    return 0;
}

static int soxr_adapter_drain(a1_src *s, float *out, long out_cap,
                              long *produced) {
    soxr_t2 *x = (soxr_t2 *)s->impl;
    size_t done = 0;
    soxr_error_t err = soxr_process(x->soxr, NULL, 0, NULL, out,
                                    (size_t)out_cap, &done);
    if (err) return -1;
    *produced = (long)done;
    return 0;
}

static int soxr_reset(a1_src *s) {
    soxr_t2 *x = (soxr_t2 *)s->impl;
    soxr_clear(x->soxr);
    return 0;
}

static long soxr_latency(const a1_src *s) {
    const soxr_t2 *x = (const soxr_t2 *)s->impl;
    if (!x->soxr) return -1;
    /* soxr_delay returns current delay in output samples (including the
     * fixed algorithmic latency). */
    double d = soxr_delay(x->soxr);
    return d < 0 ? -1 : (long)lrint(d);
}

static long soxr_required(const a1_src *s, long out_frames) {
    const soxr_t2 *x = (const soxr_t2 *)s->impl;
    return (long)ceil((double)out_frames * x->in_rate / (double)x->out_rate) +
           soxr_latency(s);
}

static void soxr_destroy(a1_src *s) {
    soxr_t2 *x = (soxr_t2 *)s->impl;
    if (x->soxr) soxr_delete(x->soxr);
    free(x);
    s->impl = NULL;
}

static const a1_src_ops kOps = {
    "soxr", soxr_prepare, soxr_adapter_process, soxr_adapter_drain,
    soxr_reset, soxr_latency, soxr_required, soxr_destroy,
};

int a1_make(const char *name, a1_src *out) {
    if (strcmp(name, "soxr") != 0) return -1;
    out->ops = &kOps;
    out->impl = calloc(1, sizeof(soxr_t2));
    return out->impl ? 0 : -1;
}
