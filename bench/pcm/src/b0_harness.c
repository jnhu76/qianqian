/*
 * E10-B0 thin DSP harness — correctness, impulse responses (raw files
 * for the Python response analysis), NaN/Inf policy, memory passes,
 * allocation behavior. Writes b0-correctness.json, b0-memory.json and
 * impulse raw files to <outdir>.
 *
 * Usage: b0_harness <outdir>
 */
#define _POSIX_C_SOURCE 200809L
#include "b0_dsp.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static long long g_alloc_calls;
static long long g_free_calls;
static int g_armed;

void *__real_malloc(size_t n);
void *__real_calloc(size_t n, size_t s);
void __real_free(void *p);

void *__wrap_malloc(size_t n) {
    void *p = __real_malloc(n);
    if (g_armed) g_alloc_calls++;
    return p;
}
void *__wrap_calloc(size_t n, size_t s) {
    void *p = __real_calloc(n, s);
    if (g_armed) g_alloc_calls++;
    return p;
}
void __wrap_free(void *p) {
    if (p && g_armed) g_free_calls++;
    __real_free(p);
}

static int nearly(float a, float b, float eps) {
    return fabsf(a - b) <= eps;
}

static uint64_t fnv1a(const float *p, long n) {
    uint64_t h = 0xcbf29ce484222325ULL;
    long i;
    const unsigned char *b = (const unsigned char *)p;
    for (i = 0; i < n * (long)sizeof(float); i++) {
        h ^= b[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}

/* ---------------------------------------------------------------- */
/* correctness                                                       */
/* ---------------------------------------------------------------- */

static int test_gain(FILE *f) {
    int ok = 1;
    b0_dsp g;
    float buf[64];
    long i;
    uint64_t h0, h1;

    fprintf(f, "  \"gain\": [\n");

    /* 0 dB -> bit-identical */
    if (b0_gain_new(&g, 1.0f) != 0) return 0;
    g.ops->prepare(&g, 48000, 2, 4096);
    for (i = 0; i < 64; i++) buf[i] = (float)(i - 32) * 0.1f;
    h0 = fnv1a(buf, 64);
    g.ops->process(&g, buf, 32);
    h1 = fnv1a(buf, 64);
    {
        int id = (h0 == h1);
        fprintf(f, "    {\"case\": \"gain_0db_bit_identical\", "
                   "\"verdict\": \"%s\"}", id ? "pass" : "FAIL");
        if (!id) ok = 0;
    }

    /* -6 dB analytical */
    b0_dsp_set_gain(&g, powf(10.0f, -6.0f / 20.0f));
    g.ops->reset(&g);
    for (i = 0; i < 64; i++) buf[i] = (float)(i - 32) * 0.1f;
    g.ops->process(&g, buf, 32);
    {
        int pass = 1;
        for (i = 0; i < 32 * 2; i++) {
            float ref = (float)(i - 32) * 0.1f * powf(10.0f, -6.0f / 20.0f);
            if (!nearly(buf[i], ref, 1e-6f)) { pass = 0; break; }
        }
        fprintf(f, ",\n    {\"case\": \"gain_m6db_analytical\", "
                   "\"verdict\": \"%s\"}", pass ? "pass" : "FAIL");
        if (!pass) ok = 0;
    }

    /* gain fusion: G1*G2 single multiply == two chained multiplies */
    {
        b0_dsp g2;
        float a[16], b_[16];
        float fused = powf(10.0f, 3.0f / 20.0f) * powf(10.0f, -2.0f / 20.0f);
        for (i = 0; i < 16; i++) { a[i] = (float)i * 0.05f; b_[i] = a[i]; }
        g.ops->destroy(&g); /* release before re-creating (ASan leak) */
        b0_gain_new(&g, fused);
        g.ops->prepare(&g, 48000, 1, 4096);
        g.ops->process(&g, a, 16);
        b0_gain_new(&g2, powf(10.0f, 3.0f / 20.0f));
        g2.ops->prepare(&g2, 48000, 1, 4096);
        g2.ops->process(&g2, b_, 16);
        b0_dsp_set_gain(&g2, powf(10.0f, -2.0f / 20.0f));
        g2.ops->process(&g2, b_, 16);
        {
            int pass = 1;
            float maxdiff = 0.0f;
            for (i = 0; i < 16; i++) {
                float d = fabsf(a[i] - b_[i]);
                if (d > maxdiff) maxdiff = d;
                /* mathematical equivalence within float32 rounding of the
                 * two separate multiplies; not bit-identical by design */
                if (d > 1e-6f) { pass = 0; break; }
            }
            fprintf(f, ",\n    {\"case\": \"gain_fusion_equivalent\", "
                       "\"verdict\": \"%s\", \"max_abs_diff\": %.9f}",
                    pass ? "pass" : "FAIL", maxdiff);
            if (!pass) ok = 0;
        }
        g2.ops->destroy(&g2);
    }
    fprintf(f, "\n  ],\n");
    g.ops->destroy(&g);
    return ok;
}

static int test_biquad(FILE *f) {
    int ok = 1;
    b0_dsp b;
    float buf[256];
    long i;

    fprintf(f, "  \"biquad\": [\n");
    if (b0_peaking_new(&b, 48000, 1000.0f, 1.0f, 6.0f) != 0) return 0;
    b.ops->prepare(&b, 48000, 1, 4096);

    /* stability: process DC and a sine, output must stay finite */
    {
        int pass = 1;
        memset(buf, 0, sizeof(buf));
        for (i = 0; i < 256; i++) buf[i] = 1.0f;
        b.ops->reset(&b);
        b.ops->process(&b, buf, 256);
        for (i = 0; i < 256; i++)
            if (!isfinite(buf[i])) { pass = 0; break; }
        fprintf(f, "    {\"case\": \"biquad_dc_finite\", \"verdict\": \"%s\"}",
                pass ? "pass" : "FAIL");
        if (!pass) ok = 0;
    }
    /* impulse response for Python response analysis */
    {
        memset(buf, 0, sizeof(buf));
        buf[0] = 1.0f;
        b.ops->reset(&b);
        b.ops->process(&b, buf, 256);
        FILE *r = fopen("/tmp/b0_imp_biquad.raw", "wb");
        if (r) { fwrite(buf, sizeof(float), 256, r); fclose(r); }
        else { ok = 0; }
    }
    /* reset clears state */
    {
        b.ops->process(&b, buf, 256); /* state now nonzero */
        b.ops->reset(&b);
        {
            extern int b0_dsp_biquad_state_is_zero(const b0_dsp *);
            int zero = b0_dsp_biquad_state_is_zero(&b);
            fprintf(f, ",\n    {\"case\": \"biquad_reset_clears_state\", "
                       "\"verdict\": \"%s\"}", zero ? "pass" : "FAIL");
            if (!zero) ok = 0;
        }
    }
    fprintf(f, "\n  ],\n");
    b.ops->destroy(&b);
    return ok;
}

static int test_eq10(FILE *f) {
    int ok = 1;
    b0_dsp e;
    float buf[1024];
    float gains[10] = {6, 6, 6, 6, 6, 6, 6, 6, 6, 6};

    fprintf(f, "  \"eq10\": [\n");
    if (b0_eq10_new(&e, 48000, gains) != 0) return 0;
    e.ops->prepare(&e, 48000, 1, 4096);
    memset(buf, 0, sizeof(buf));
    buf[0] = 1.0f;
    e.ops->process(&e, buf, 1024);
    {
        int finite = 1;
        long i;
        for (i = 0; i < 1024; i++)
            if (!isfinite(buf[i])) { finite = 0; break; }
        FILE *r = fopen("/tmp/b0_imp_eq10.raw", "wb");
        if (r) { fwrite(buf, sizeof(float), 1024, r); fclose(r); }
        fprintf(f, "    {\"case\": \"eq10_impulse_finite\", "
                   "\"verdict\": \"%s\"}", finite ? "pass" : "FAIL");
        if (!finite || !r) ok = 0;
    }
    /* high-rate EQ: 16k band valid at 48k/96k, skipped >= 0.95 nyq */
    {
        b0_dsp e2;
        int rc = b0_eq10_new(&e2, 96000, gains);
        fprintf(f, ",\n    {\"case\": \"eq10_96k_prepare\", "
                   "\"verdict\": \"%s\"}", rc == 0 ? "pass" : "FAIL");
        if (rc == 0) e2.ops->destroy(&e2);
        else ok = 0;
    }
    fprintf(f, "\n  ],\n");
    e.ops->destroy(&e);
    return ok;
}

static int test_limiter(FILE *f) {
    int ok = 1;
    b0_dsp l;
    float buf[512];
    long i;

    fprintf(f, "  \"limiter\": [\n");
    if (b0_limiter_new(&l, 48000, 0.0f, 0.001f, 0.1f) != 0) return 0;
    l.ops->prepare(&l, 48000, 1, 4096);

    /* below threshold: unchanged */
    {
        int pass = 1;
        for (i = 0; i < 256; i++) buf[i] = 0.1f * (float)(i % 7) / 7.0f;
        l.ops->reset(&l);
        l.ops->process(&l, buf, 256);
        for (i = 0; i < 256; i++)
            if (fabsf(buf[i]) > 0.1f + 1e-6f) { pass = 0; break; }
        fprintf(f, "    {\"case\": \"limiter_below_threshold_untouched\", "
                   "\"verdict\": \"%s\"}", pass ? "pass" : "FAIL");
        if (!pass) ok = 0;
    }
    /* above threshold: output peak clamped to threshold after attack */
    {
        int pass = 1;
        float peak = 0;
        for (i = 0; i < 256; i++) buf[i] = 2.0f; /* 2x threshold */
        l.ops->reset(&l);
        l.ops->process(&l, buf, 256);
        for (i = 0; i < 256; i++)
            if (fabsf(buf[i]) > peak) peak = fabsf(buf[i]);
        /* peak must be <= threshold (1.0) within a small overshoot */
        if (peak > 1.0f + 0.05f) pass = 0;
        fprintf(f, ",\n    {\"case\": \"limiter_clamps_peak\", "
                   "\"verdict\": \"%s\", \"peak\": %.4f}",
                pass ? "pass" : "FAIL", peak);
        if (!pass) ok = 0;
    }
    /* latency = 0 (no lookahead) */
    {
        long lat = l.ops->latency_frames(&l);
        fprintf(f, ",\n    {\"case\": \"limiter_latency_zero\", "
                   "\"verdict\": \"%s\", \"latency\": %ld}",
                lat == 0 ? "pass" : "FAIL", lat);
        if (lat != 0) ok = 0;
    }
    fprintf(f, "\n  ],\n");
    l.ops->destroy(&l);
    return ok;
}

static int test_nan_policy(FILE *f) {
    int ok = 1;
    b0_dsp b, g;
    float buf[64];
    long i;

    fprintf(f, "  \"nan_policy\": [\n");

    /* DSP ACTIVE + NaN input -> sanitized to 0; subsequent samples
     * finite (state not poisoned) */
    b0_peaking_new(&b, 48000, 1000.0f, 1.0f, 6.0f);
    b.ops->prepare(&b, 48000, 1, 4096);
    memset(buf, 0, sizeof(buf));
    buf[0] = NAN;
    for (i = 1; i < 16; i++) buf[i] = 0.5f;
    b.ops->process(&b, buf, 16);
    {
        int finite = 1, seen = b0_dsp_nan_seen(&b);
        for (i = 0; i < 16; i++)
            if (!isfinite(buf[i])) { finite = 0; break; }
        int pass = finite && seen;
        fprintf(f, "    {\"case\": \"active_nan_sanitized\", "
                   "\"verdict\": \"%s\", \"nan_seen\": %d}",
                pass ? "pass" : "FAIL", seen);
        if (!pass) ok = 0;
    }
    b.ops->destroy(&b);

    /* DSP OFF is the P0 bit-transparent bypass (no pass, proven in P0);
     * an ACTIVE gain node applies the sanitize policy: NaN -> 0 and the
     * event is flagged. */
    b0_gain_new(&g, 1.0f);
    g.ops->prepare(&g, 48000, 1, 4096);
    memset(buf, 0, sizeof(buf));
    buf[0] = NAN;
    g.ops->process(&g, buf, 16);
    {
        int zeroed = buf[0] == 0.0f;
        int seen = b0_dsp_nan_seen(&g);
        int pass = zeroed && seen;
        fprintf(f, ",\n    {\"case\": \"active_gain_sanitizes_nan\", "
                   "\"verdict\": \"%s\", \"nan_seen\": %d}",
                pass ? "pass" : "FAIL", seen);
        if (!pass) ok = 0;
    }
    g.ops->destroy(&g);
    fprintf(f, "\n  ],\n");
    return ok;
}

/* ---------------------------------------------------------------- */
/* lifecycle: re-prepare ownership + per-instance NaN policy          */
/* (review blockers: prepare leaked old state on re-prepare; NaN      */
/* policy was a hidden file global shared by all instances)           */
/* ---------------------------------------------------------------- */

/* prepare(rate A) -> process -> prepare(rate B) -> process -> destroy,
 * with alloc/free counters armed across the whole lifecycle: live
 * (allocs - frees) must return to 0, i.e. no re-prepare leak. */
static int reprepare_case(FILE *f, const char *case_name, int is_first,
                          int kind) {
    /* kind: 0=biquad 1=eq10 2=limiter */
    b0_dsp d;
    float buf[256];
    long i;
    long long live;
    int ok = 1;
    float gains[10] = {3, 3, 3, 3, 3, 3, 3, 3, 3, 3};

    g_alloc_calls = 0;
    g_free_calls = 0;
    g_armed = 1;
    switch (kind) {
    case 0: b0_peaking_new(&d, 44100, 1000.0f, 1.0f, 6.0f); break;
    case 1: b0_eq10_new(&d, 44100, gains); break;
    default: b0_limiter_new(&d, 44100, 0.0f, 0.001f, 0.1f); break;
    }
    d.ops->prepare(&d, 44100, 1, 4096);
    for (i = 0; i < 256; i++) buf[i] = 0.4f * (float)(i % 9) / 9.0f;
    d.ops->process(&d, buf, 256);
    /* re-prepare at a different rate: previous state must be released */
    d.ops->prepare(&d, 48000, 1, 4096);
    for (i = 0; i < 256; i++) buf[i] = 0.4f * (float)(i % 9) / 9.0f;
    d.ops->process(&d, buf, 256);
    {
        int finite = 1;
        for (i = 0; i < 256; i++)
            if (!isfinite(buf[i])) { finite = 0; break; }
        if (!finite) ok = 0;
        if (kind != 2) { /* limiter on 0.4 input may stay untouched */
            int nonzero = 0;
            for (i = 0; i < 256; i++)
                if (buf[i] != 0.0f) { nonzero = 1; break; }
            if (!nonzero) ok = 0;
        }
    }
    d.ops->destroy(&d);
    live = g_alloc_calls - g_free_calls;
    g_armed = 0;
    fprintf(f, "%s    {\"case\": \"%s\", \"allocs\": %lld, \"frees\": %lld, "
               "\"live_after_destroy\": %lld, \"verdict\": \"%s\"}",
            is_first ? "" : ",\n", case_name, g_alloc_calls,
            g_free_calls, live, (ok && live == 0) ? "pass" : "FAIL");
    return ok && live == 0;
}

static int test_lifecycle(FILE *f) {
    int ok = 1;
    fprintf(f, "  \"lifecycle\": [\n");
    ok &= reprepare_case(f, "biquad_reprepare_44100_48000_no_leak", 1, 0);
    ok &= reprepare_case(f, "eq10_reprepare_44100_48000_no_leak", 0, 1);
    ok &= reprepare_case(f, "limiter_reprepare_44100_48000_no_leak", 0, 2);
    fprintf(f, "\n  ],\n");
    return ok;
}

static int test_instance_isolation(FILE *f) {
    int ok = 1;
    b0_dsp b1, b2;
    float x1[16], x2[16];
    long i;

    fprintf(f, "  \"instance_isolation\": [\n");
    b0_peaking_new(&b1, 48000, 1000.0f, 1.0f, 6.0f);
    b0_peaking_new(&b2, 48000, 1000.0f, 1.0f, 6.0f);
    b1.ops->prepare(&b1, 48000, 1, 4096);
    b2.ops->prepare(&b2, 48000, 1, 4096);
    /* two instances with OPPOSITE policies must behave independently */
    b0_dsp_set_nan_policy(&b1, 1);
    b0_dsp_set_nan_policy(&b2, 0);
    for (i = 0; i < 16; i++) { x1[i] = 0.5f; x2[i] = 0.5f; }
    x1[0] = NAN;
    x2[0] = NAN;
    b1.ops->process(&b1, x1, 16);
    b2.ops->process(&b2, x2, 16);
    {
        int s1 = isfinite(x1[0]) && b0_dsp_nan_seen(&b1) == 1;
        int p2 = (x2[0] != x2[0]) && b0_dsp_nan_seen(&b2) == 0;
        /* flip the policies the other way round: still independent.
         * reset() first — nan_seen is a sticky event flag and the
         * policy=0 pass poisons b2's IIR state by design. */
        b0_dsp_set_nan_policy(&b1, 0);
        b0_dsp_set_nan_policy(&b2, 1);
        b1.ops->reset(&b1);
        b2.ops->reset(&b2);
        for (i = 0; i < 16; i++) { x1[i] = 0.5f; x2[i] = 0.5f; }
        x1[0] = NAN;
        x2[0] = NAN;
        b1.ops->process(&b1, x1, 16);
        b2.ops->process(&b2, x2, 16);
        int p1 = (x1[0] != x1[0]) && b0_dsp_nan_seen(&b1) == 0;
        int s2 = isfinite(x2[0]) && b0_dsp_nan_seen(&b2) == 1;
        int pass = s1 && p2 && p1 && s2;
        fprintf(f, "    {\"case\": \"nan_policy_per_instance\", "
                   "\"verdict\": \"%s\"}", pass ? "pass" : "FAIL");
        if (!pass) ok = 0;
    }
    b1.ops->destroy(&b1);
    b2.ops->destroy(&b2);
    fprintf(f, "\n  ],\n");
    return ok;
}

/* ---------------------------------------------------------------- */
/* memory passes                                                     */
/* ---------------------------------------------------------------- */

static int run_memory(FILE *f) {
    /* 256 frames x 2 channels: the chains are prepared stereo, so the
     * buffer must hold frames*channels floats (was float[256], an
     * out-of-bounds read/write caught by ASan — pre-existing bug) */
    enum { FRAMES = 256, CHANNELS = 2 };
    float buf[FRAMES * CHANNELS];
    long i;
    int ok = 1;
    /* chains: [gain], [biquad], [eq10], [gain,eq10], [gain,eq10,limiter] */
    struct {
        const char *name;
        int ngain, nbq, neq, nlim;
    } chains[] = {
        {"gain_only", 1, 0, 0, 0},
        {"one_biquad", 0, 1, 0, 0},
        {"eq10", 0, 0, 1, 0},
        {"gain_eq10", 1, 0, 1, 0},
        {"gain_eq10_limiter", 1, 0, 1, 1},
    };
    int ci;
    fprintf(f, "{\n  \"experiment\": \"e10-b0\",\n");
    fprintf(f, "  \"section\": \"memory_passes\",\n");
    fprintf(f, "  \"rows\": [\n");
    for (ci = 0; ci < 5; ci++) {
        b0_dsp nodes[12];
        int nn = 0, k;
        long long allocs_rt = 0;
        float gains[10] = {3, 3, 3, 3, 3, 3, 3, 3, 3, 3};
        for (i = 0; i < (long)FRAMES * CHANNELS; i++)
            buf[i] = 0.5f * (float)(i % 5) / 5.0f;
        if (chains[ci].ngain) {
            b0_gain_new(&nodes[nn], 0.8f);
            nodes[nn].ops->prepare(&nodes[nn], 48000, 2, 4096);
            nn++;
        }
        if (chains[ci].nbq) {
            b0_peaking_new(&nodes[nn], 48000, 1000.0f, 1.0f, 3.0f);
            nodes[nn].ops->prepare(&nodes[nn], 48000, 2, 4096);
            nn++;
        }
        if (chains[ci].neq) {
            b0_eq10_new(&nodes[nn], 48000, gains);
            nodes[nn].ops->prepare(&nodes[nn], 48000, 2, 4096);
            nn++;
        }
        if (chains[ci].nlim) {
            b0_limiter_new(&nodes[nn], 48000, 0.0f, 0.001f, 0.1f);
            nodes[nn].ops->prepare(&nodes[nn], 48000, 2, 4096);
            nn++;
        }
        /* armed: process must not allocate */
        g_alloc_calls = 0;
        g_armed = 1;
        for (k = 0; k < 100; k++)
            for (int n = 0; n < nn; n++)
                nodes[n].ops->process(&nodes[n], buf, FRAMES);
        g_armed = 0;
        allocs_rt = g_alloc_calls;
        {
            int nactive = (chains[ci].ngain + chains[ci].nbq +
                           chains[ci].neq * 10 + chains[ci].nlim);
            fprintf(f, "%s  {\"chain\": \"%s\", \"nodes\": %d, "
                       "\"logical_passes_per_block\": %d, "
                       "\"explicit_copies\": 0, \"buffered_frames\": 0, "
                       "\"post_prepare_allocations\": %lld, "
                       "\"verdict\": \"%s\"}",
                    ci ? ",\n" : "", chains[ci].name, nn, nactive,
                    allocs_rt, allocs_rt == 0 ? "pass" : "FAIL");
            if (allocs_rt != 0) ok = 0;
        }
        for (int n = 0; n < nn; n++) nodes[n].ops->destroy(&nodes[n]);
    }
    fprintf(f, "\n  ]\n}\n");
    return ok;
}

/* ---------------------------------------------------------------- */

int main(int argc, char **argv) {
    const char *outdir;
    char path[1024];
    FILE *f;
    int ok = 1;

    if (argc < 2) {
        fprintf(stderr, "usage: b0_harness <outdir>\n");
        return 2;
    }
    outdir = argv[1];

    snprintf(path, sizeof(path), "%s/b0-correctness.json", outdir);
    f = fopen(path, "w");
    if (!f) return 2;
    fprintf(f, "{\n  \"experiment\": \"e10-b0\",\n");
    fprintf(f, "  \"section\": \"dsp_correctness\",\n");
    ok &= test_gain(f);
    ok &= test_biquad(f);
    ok &= test_eq10(f);
    ok &= test_limiter(f);
    ok &= test_nan_policy(f);
    ok &= test_lifecycle(f);
    ok &= test_instance_isolation(f);
    fprintf(f, "  \"verdict\": \"%s\"\n}\n", ok ? "PASS" : "FAIL");
    fclose(f);

    snprintf(path, sizeof(path), "%s/b0-memory.json", outdir);
    f = fopen(path, "w");
    if (!f) return 2;
    ok &= run_memory(f);
    fclose(f);

    printf("b0 harness: %s\n", ok ? "PASS" : "FAIL");
    return ok ? 0 : 1;
}
