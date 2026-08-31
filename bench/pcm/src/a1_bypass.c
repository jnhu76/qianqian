/*
 * E10-A1 BYPASS adapter — the transparent same-rate reference. Rate
 * conversion must be rejected the same way P0 BYPASS rejects a mismatch.
 */
#include "a1_contract.h"

#include <stdlib.h>
#include <string.h>

typedef struct {
    int in_rate, out_rate, channels;
    long max_frames;
    long buffered_frames;
} bypass_t;

static int bypass_prepare(a1_src *s, int in_rate, int out_rate, int channels,
                          long max_frames) {
    bypass_t *b = (bypass_t *)s->impl;
    if (in_rate <= 0 || out_rate <= 0 || channels <= 0 || max_frames < 0)
        return -4; /* PCM_ERR_INVALID_PARAM */
    if (in_rate != out_rate) return -5; /* PCM_ERR_BYPASS_RATE_MISMATCH */
    b->in_rate = in_rate;
    b->out_rate = out_rate;
    b->channels = channels;
    b->max_frames = max_frames;
    b->buffered_frames = 0;
    return 0;
}

static int bypass_process(a1_src *s, const float *in, long in_frames,
                          float *out, long out_cap,
                          long *consumed, long *produced, int is_last) {
    bypass_t *b = (bypass_t *)s->impl;
    (void)is_last;
    long n = in_frames < out_cap ? in_frames : out_cap;
    if (n < 0) n = 0;
    if (n > 0) memcpy(out, in, (size_t)n * b->channels * sizeof(float));
    *consumed = n;
    *produced = n;
    return 0;
}

static int bypass_drain(a1_src *s, float *out, long out_cap, long *produced) {
    (void)s; (void)out; (void)out_cap;
    *produced = 0;
    return 0;
}

static int bypass_reset(a1_src *s) {
    ((bypass_t *)s->impl)->buffered_frames = 0;
    return 0;
}

static long bypass_latency(const a1_src *s) {
    (void)s;
    return 0;
}

static long bypass_required(const a1_src *s, long out_frames) {
    (void)s;
    return out_frames;
}

static void bypass_destroy(a1_src *s) {
    free(s->impl);
    s->impl = NULL;
}

static const a1_src_ops kOps = {
    "bypass", bypass_prepare, bypass_process, bypass_drain, bypass_reset,
    bypass_latency, bypass_required, bypass_destroy,
};

int a1_make(const char *name, a1_src *out) {
    if (strcmp(name, "bypass") != 0) return -1;
    out->ops = &kOps;
    out->impl = calloc(1, sizeof(bypass_t));
    return out->impl ? 0 : -1;
}
