/*
 * E10-B0 thin DSP reference implementations. Bench-only.
 * Biquad = RBJ peaking, Direct Form II Transposed. NaN/Inf policy:
 * sanitize-while-active (offending sample -> 0, no state poisoning).
 */
#include "b0_dsp.h"

#define _GNU_SOURCE
#include <math.h>
#include <stdlib.h>
#include <string.h>

static int g_nan_policy_sanitize = 1;

void b0_dsp_set_nan_policy(b0_dsp *d, int sanitize_active) {
    (void)d;
    g_nan_policy_sanitize = sanitize_active;
}

/* ---------------------------------------------------------------- */
/* shared: sanitize helper (applies to every active DSP pass)        */
/* ---------------------------------------------------------------- */

/* Returns 1 if any non-finite sample was found (and, with the policy
 * active, zeroed). */
static int sanitize_block(float *pcm, long frames, int channels) {
    long n = frames * channels;
    long i;
    int found = 0;
    if (!g_nan_policy_sanitize)
        return 0;
    for (i = 0; i < n; i++) {
        float v = pcm[i];
        if (v != v || v > 3.402823466e+38f || v < -3.402823466e+38f) {
            pcm[i] = 0.0f;
            found = 1;
        }
    }
    return found;
}

/* ---------------------------------------------------------------- */
/* Gain — one fused multiply pass                                    */
/* ---------------------------------------------------------------- */

typedef struct {
    float g;
    int channels;
    int nan_seen;
} gain_t;

static int gain_prepare(b0_dsp *d, int sample_rate, int channels,
                        long max_frames) {
    gain_t *g = (gain_t *)d->impl;
    (void)sample_rate;
    g->channels = channels;
    g->nan_seen = 0;
    (void)max_frames;
    return 0;
}

static int gain_process(b0_dsp *d, float *pcm, long frames) {
    gain_t *g = (gain_t *)d->impl;
    long n = frames * g->channels, i;
    if (sanitize_block(pcm, frames, g->channels)) g->nan_seen = 1;
    for (i = 0; i < n; i++) pcm[i] *= g->g;
    return 0;
}

static int gain_reset(b0_dsp *d) {
    gain_t *g = (gain_t *)d->impl;
    g->nan_seen = 0;
    return 0;
}

static long gain_latency(const b0_dsp *d) {
    (void)d;
    return 0;
}

static void gain_destroy(b0_dsp *d) {
    free(d->impl);
    d->impl = NULL;
}

static const b0_dsp_ops kGainOps = {
    "gain", gain_prepare, gain_process, gain_reset, gain_latency,
    gain_destroy,
};

int b0_gain_new(b0_dsp *d, float gain_linear) {
    gain_t *g = (gain_t *)calloc(1, sizeof(gain_t));
    if (!g) return -1;
    d->ops = &kGainOps;
    d->impl = g;
    g->g = gain_linear;
    return 0;
}

void b0_dsp_set_gain(b0_dsp *d, float gain_linear) {
    ((gain_t *)d->impl)->g = gain_linear;
}

/* ---------------------------------------------------------------- */
/* Biquad (RBJ peaking, DF2T)                                        */
/* ---------------------------------------------------------------- */

typedef struct {
    int channels;
    int nan_seen;
    /* normalized coefficients (b0..b2, a1..a2; a0 folded in) */
    float b0, b1, b2, a1, a2;
    /* DF2T state per channel: s1, s2 */
    float *s1, *s2;
} biquad_t;

static int biquad_set_coeffs(biquad_t *b, double f0, double q,
                             double gain_db, int sample_rate) {
    double A, w0, alpha, a0;
    if (f0 <= 0 || f0 >= 0.95 * sample_rate / 2.0 || q <= 0)
        return -1;
    A = pow(10.0, gain_db / 40.0);
    w0 = 2.0 * M_PI * f0 / sample_rate;
    alpha = sin(w0) / (2.0 * q);
    a0 = 1.0 + alpha / A;
    b->b0 = (float)((1.0 + alpha * A) / a0);
    b->b1 = (float)((-2.0 * cos(w0)) / a0);
    b->b2 = (float)((1.0 - alpha * A) / a0);
    b->a1 = (float)((-2.0 * cos(w0)) / a0);
    b->a2 = (float)((1.0 - alpha / A) / a0);
    return 0;
}

static int biquad_prepare(b0_dsp *d, int sample_rate, int channels,
                          long max_frames) {
    biquad_t *b = (biquad_t *)d->impl;
    b->channels = channels;
    b->nan_seen = 0;
    b->s1 = (float *)calloc((size_t)channels, sizeof(float));
    b->s2 = (float *)calloc((size_t)channels, sizeof(float));
    if (!b->s1 || !b->s2) return -1;
    (void)max_frames;
    return 0;
}

static int biquad_process(b0_dsp *d, float *pcm, long frames) {
    biquad_t *b = (biquad_t *)d->impl;
    long i;
    if (sanitize_block(pcm, frames, b->channels)) b->nan_seen = 1;
    for (i = 0; i < frames; i++) {
        int c;
        for (c = 0; c < b->channels; c++) {
            float *x = &pcm[i * b->channels + c];
            float y = b->b0 * (*x) + b->s1[c];
            b->s1[c] = b->b1 * (*x) - b->a1 * y + b->s2[c];
            b->s2[c] = b->b2 * (*x) - b->a2 * y;
            *x = y;
        }
    }
    return 0;
}

static int biquad_reset(b0_dsp *d) {
    biquad_t *b = (biquad_t *)d->impl;
    if (b->s1) memset(b->s1, 0, (size_t)b->channels * sizeof(float));
    if (b->s2) memset(b->s2, 0, (size_t)b->channels * sizeof(float));
    b->nan_seen = 0;
    return 0;
}

static long biquad_latency(const b0_dsp *d) {
    (void)d;
    return 0;
}

static void biquad_destroy(b0_dsp *d) {
    biquad_t *b = (biquad_t *)d->impl;
    free(b->s1);
    free(b->s2);
    free(b);
    d->impl = NULL;
}

static const b0_dsp_ops kBiquadOps = {
    "biquad", biquad_prepare, biquad_process, biquad_reset,
    biquad_latency, biquad_destroy,
};

int b0_peaking_new(b0_dsp *d, int sample_rate, float f0, float q,
                   float gain_db) {
    biquad_t *b = (biquad_t *)calloc(1, sizeof(biquad_t));
    if (!b) return -1;
    if (biquad_set_coeffs(b, f0, q, gain_db, sample_rate) < 0) {
        free(b);
        return -1;
    }
    d->ops = &kBiquadOps;
    d->impl = b;
    return 0;
}

void b0_dsp_set_peaking(b0_dsp *d, int sample_rate, float f0, float q,
                        float gain_db) {
    biquad_t *b = (biquad_t *)d->impl;
    biquad_set_coeffs(b, f0, q, gain_db, sample_rate);
}

int b0_dsp_biquad_state_is_zero(const b0_dsp *d) {
    const biquad_t *b = (const biquad_t *)d->impl;
    int c;
    for (c = 0; c < b->channels; c++)
        if (b->s1[c] != 0.0f || b->s2[c] != 0.0f) return 0;
    return 1;
}

/* ---------------------------------------------------------------- */
/* 10-band EQ — cascade of 10 RBJ peaking biquads                    */
/* ---------------------------------------------------------------- */

#define EQ10_BANDS 10

typedef struct {
    biquad_t bands[EQ10_BANDS];
    int nactive;
    int channels;
    int nan_seen;
    float centers[EQ10_BANDS];
    int active[EQ10_BANDS];
} eq10_t;

static const float kEq10Centers[EQ10_BANDS] = {
    31.25f, 62.5f, 125.0f, 250.0f, 500.0f, 1000.0f,
    2000.0f, 4000.0f, 8000.0f, 16000.0f,
};

static int eq10_prepare(b0_dsp *d, int sample_rate, int channels,
                        long max_frames) {
    eq10_t *e = (eq10_t *)d->impl;
    int i;
    e->channels = channels;
    e->nan_seen = 0;
    e->nactive = 0;
    for (i = 0; i < EQ10_BANDS; i++) {
        biquad_t *b = &e->bands[i];
        if (e->centers[i] <= 0) {
            e->active[i] = 0;
            continue;
        }
        /* skip bands at/above 0.95 * Nyquist: never unstable coeffs */
        if (e->centers[i] >= 0.95 * sample_rate / 2.0) {
            e->active[i] = 0;
            continue;
        }
        e->active[i] = 1;
        e->nactive++;
        b->channels = channels;
        b->nan_seen = 0;
        b->s1 = (float *)calloc((size_t)channels, sizeof(float));
        b->s2 = (float *)calloc((size_t)channels, sizeof(float));
        if (!b->s1 || !b->s2) return -1;
    }
    (void)max_frames;
    return 0;
}

static int eq10_process(b0_dsp *d, float *pcm, long frames) {
    eq10_t *e = (eq10_t *)d->impl;
    int i;
    if (sanitize_block(pcm, frames, e->channels)) e->nan_seen = 1;
    for (i = 0; i < EQ10_BANDS; i++) {
        biquad_t *b;
        long k;
        if (!e->active[i]) continue;
        b = &e->bands[i];
        for (k = 0; k < frames; k++) {
            int c;
            for (c = 0; c < b->channels; c++) {
                float *x = &pcm[k * b->channels + c];
                float y = b->b0 * (*x) + b->s1[c];
                b->s1[c] = b->b1 * (*x) - b->a1 * y + b->s2[c];
                b->s2[c] = b->b2 * (*x) - b->a2 * y;
                *x = y;
            }
        }
    }
    return 0;
}

static int eq10_reset(b0_dsp *d) {
    eq10_t *e = (eq10_t *)d->impl;
    int i;
    for (i = 0; i < EQ10_BANDS; i++) {
        biquad_t *b = &e->bands[i];
        if (!e->active[i]) continue;
        if (b->s1) memset(b->s1, 0, (size_t)b->channels * sizeof(float));
        if (b->s2) memset(b->s2, 0, (size_t)b->channels * sizeof(float));
    }
    e->nan_seen = 0;
    return 0;
}

static long eq10_latency(const b0_dsp *d) {
    (void)d;
    return 0;
}

static void eq10_destroy(b0_dsp *d) {
    eq10_t *e = (eq10_t *)d->impl;
    int i;
    for (i = 0; i < EQ10_BANDS; i++) {
        free(e->bands[i].s1);
        free(e->bands[i].s2);
    }
    free(e);
    d->impl = NULL;
}

static const b0_dsp_ops kEq10Ops = {
    "eq10", eq10_prepare, eq10_process, eq10_reset, eq10_latency,
    eq10_destroy,
};

int b0_eq10_new(b0_dsp *d, int sample_rate, const float gains_db[10]) {
    eq10_t *e = (eq10_t *)calloc(1, sizeof(eq10_t));
    int i;
    if (!e) return -1;
    for (i = 0; i < EQ10_BANDS; i++) {
        e->centers[i] = kEq10Centers[i];
        if (biquad_set_coeffs(&e->bands[i], kEq10Centers[i], 1.0,
                              gains_db[i], sample_rate) < 0)
            e->centers[i] = 0; /* mark inactive */
    }
    d->ops = &kEq10Ops;
    d->impl = e;
    (void)sample_rate;
    return 0;
}

/* ---------------------------------------------------------------- */
/* Limiter — simple peak limiter, explicitly enabled                 */
/* ---------------------------------------------------------------- */

typedef struct {
    int channels;
    int nan_seen;
    float threshold;      /* linear (e.g. 10^(0/20)=1.0) */
    float env;            /* per-channel peak envelope */
    float *envs;
    float attack_coef;    /* 1 - exp(-1/(attack_s * sr)) */
    float release_coef;
    int sample_rate;
    long calls;
} limiter_t;

static int limiter_prepare(b0_dsp *d, int sample_rate, int channels,
                           long max_frames) {
    limiter_t *l = (limiter_t *)d->impl;
    l->channels = channels;
    l->sample_rate = sample_rate;
    l->nan_seen = 0;
    l->env = 0.0f;
    l->envs = (float *)calloc((size_t)channels, sizeof(float));
    if (!l->envs) return -1;
    (void)max_frames;
    return 0;
}

static int limiter_process(b0_dsp *d, float *pcm, long frames) {
    limiter_t *l = (limiter_t *)d->impl;
    long i;
    /* instant attack via peak-hold; smooth release */
    double rel = 1.0 - exp(-1.0 / ((double)l->release_coef * l->sample_rate));
    if (sanitize_block(pcm, frames, l->channels)) l->nan_seen = 1;
    for (i = 0; i < frames; i++) {
        int c;
        for (c = 0; c < l->channels; c++) {
            float *x = &pcm[i * l->channels + c];
            float a = fabsf(*x);
            float *e = &l->envs[c];
            /* peak-hold: attack instant (no overshoot), release at rate */
            if (a > *e)
                *e = a;
            else
                *e *= (float)(1.0 - rel);
            if (*e > l->threshold && *e > 1e-6f)
                *x *= l->threshold / *e;
        }
    }
    l->calls++;
    return 0;
}

static int limiter_reset(b0_dsp *d) {
    limiter_t *l = (limiter_t *)d->impl;
    if (l->envs)
        memset(l->envs, 0, (size_t)l->channels * sizeof(float));
    l->env = 0.0f;
    l->nan_seen = 0;
    return 0;
}

static long limiter_latency(const b0_dsp *d) {
    (void)d;
    return 0; /* no lookahead */
}

static void limiter_destroy(b0_dsp *d) {
    limiter_t *l = (limiter_t *)d->impl;
    free(l->envs);
    free(l);
    d->impl = NULL;
}

static const b0_dsp_ops kLimiterOps = {
    "limiter", limiter_prepare, limiter_process, limiter_reset,
    limiter_latency, limiter_destroy,
};

int b0_limiter_new(b0_dsp *d, int sample_rate, float threshold_db,
                   float attack_s, float release_s) {
    limiter_t *l = (limiter_t *)calloc(1, sizeof(limiter_t));
    if (!l) return -1;
    l->threshold = powf(10.0f, threshold_db / 20.0f);
    l->attack_coef = attack_s > 0 ? attack_s : 0.001f;
    l->release_coef = release_s > 0 ? release_s : 0.1f;
    d->ops = &kLimiterOps;
    d->impl = l;
    return 0;
}

int b0_dsp_nan_seen(const b0_dsp *d) {
    const char *n = d->ops->name;
    if (strcmp(n, "gain") == 0) return ((gain_t *)d->impl)->nan_seen;
    if (strcmp(n, "biquad") == 0) return ((biquad_t *)d->impl)->nan_seen;
    if (strcmp(n, "eq10") == 0) return ((eq10_t *)d->impl)->nan_seen;
    if (strcmp(n, "limiter") == 0) return ((limiter_t *)d->impl)->nan_seen;
    return 0;
}
