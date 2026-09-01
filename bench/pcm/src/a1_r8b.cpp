/*
 * E10-A1 r8brain-free-src adapter (6.5) — C++ bridge exposing a C
 * contract. Double-precision internal, linear-phase; per-channel
 * processing. The f32 interleaved <-> deinterleaved double conversion
 * is counted as adapter glue (explicit copies reported separately).
 */
#include "a1_contract.h"

#include "CDSPResampler.h"

#include <cmath>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace {

struct r8b_t {
    std::vector<r8b::CDSPResampler *> res;
    std::vector<std::vector<double>> in_buf, out_buf;
    int in_rate = 0, out_rate = 0, channels = 0;
    long max_frames = 0;
    long latency = -1;
    long long total_input_fed = 0;
    long long total_output_produced = 0;
};

} // namespace

using namespace std;

extern "C" {

static int r8b_prepare(a1_src *s, int in_rate, int out_rate, int channels,
                       long max_frames) {
    r8b_t *r = (r8b_t *)s->impl;
    if (in_rate <= 0 || out_rate <= 0 || channels <= 0 || max_frames < 0)
        return -4;
    for (auto *p : r->res) delete p;
    r->res.clear();
    r->in_buf.clear();
    r->out_buf.clear();
    r->in_rate = in_rate;
    r->out_rate = out_rate;
    r->channels = channels;
    r->max_frames = max_frames;
    r->latency = -1;
    for (int c = 0; c < channels; c++) {
        r8b::CDSPResampler *p = new r8b::CDSPResampler(
            (double)in_rate, (double)out_rate, (int)max_frames);
        r->res.push_back(p);
        r->in_buf.emplace_back((size_t)max_frames);
        /* output buffer must hold the worst-case output of one drain
         * round (max_frames input at the upsampling ratio) */
        double ratio = (double)out_rate / (double)in_rate;
        if (ratio < 1.0) ratio = 1.0;
        r->out_buf.emplace_back((size_t)(max_frames * ratio) + 1024);
    }
    r->latency = r->res[0]->getLatency();
    return 0;
}

static int r8b_process(a1_src *s, const float *in, long in_frames,
                       float *out, long out_cap,
                       long *consumed, long *produced, int is_last) {
    r8b_t *r = (r8b_t *)s->impl;
    (void)is_last; /* r8b drains its tail in drain() by feeding silence */
    if (in_frames > r->max_frames) in_frames = r->max_frames;
    r->total_input_fed += in_frames;
    /* interleave -> per-channel double (adapter glue, explicit) */
    for (int c = 0; c < r->channels; c++)
        for (long i = 0; i < in_frames; i++)
            r->in_buf[c][i] = (double)in[i * r->channels + c];
    /* process each channel; all channels produce the same count. NOTE:
     * r8b returns a POINTER to its internal output buffer via the `op0`
     * reference — it does not write into a caller-supplied buffer. */
    int outn = 0;
    for (int c = 0; c < r->channels; c++) {
        double *op = nullptr;
        int n = r->res[c]->process(r->in_buf[c].data(), (int)in_frames, op);
        if (c == 0) outn = n;
        else if (n != outn) return -1;
        if (n > (int)r->out_buf[c].size()) return -1;
        for (long i = 0; i < n; i++) r->out_buf[c][i] = op[i];
    }
    if (outn > out_cap) return -1;
    /* per-channel double -> interleaved f32 (adapter glue, explicit) */
    for (int c = 0; c < r->channels; c++)
        for (long i = 0; i < outn; i++)
            out[i * r->channels + c] = (float)r->out_buf[c][i];
    *consumed = in_frames;
    *produced = outn;
    r->total_output_produced += outn;
    return 0;
}

static int r8b_drain(a1_src *s, float *out, long out_cap, long *produced) {
    r8b_t *r = (r8b_t *)s->impl;
    long total = 0;
    const long chunk = r->max_frames > 0 ? r->max_frames : 1024;
    /* r8brain releases its filter tail only when fed more input; the
     * caller is expected to know the ideal output count (its example
     * feeds zeros until the expected count is reached). The adapter
     * trims to exactly total_input * out_rate / in_rate (r8b advertises
     * ~0 algorithmic latency, linear phase). */
    long long ideal = (long long)llround(
        r->total_input_fed * (double)r->out_rate / (double)r->in_rate);
    long long remaining = ideal - r->total_output_produced;
    for (int round = 0; round < 16384 && total < out_cap &&
                            remaining > 0; round++) {
        for (int c = 0; c < r->channels; c++)
            memset(r->in_buf[c].data(), 0, (size_t)chunk * sizeof(double));
        int outn = 0;
        for (int c = 0; c < r->channels; c++) {
            double *op = nullptr;
            int n = r->res[c]->process(r->in_buf[c].data(), (int)chunk, op);
            if (c == 0) outn = n;
            else if (n != outn) return -1;
            for (long i = 0; i < n; i++) r->out_buf[c][i] = op[i];
        }
        if (outn <= 0) break;
        if (outn > remaining) outn = (int)remaining;
        if (outn > out_cap - total) outn = (int)(out_cap - total);
        if (outn <= 0) break;
        for (int c = 0; c < r->channels; c++)
            for (long i = 0; i < outn; i++)
                out[(total + i) * r->channels + c] = (float)r->out_buf[c][i];
        total += outn;
        remaining -= outn;
    }
    /* keep the cumulative output accounting consistent: drain feeds
     * zeros directly into the resamplers (bypassing process()), and the
     * produced frames must be reflected here or repeated drain() calls
     * would re-emit the same tail forever (found by the lifecycle run) */
    r->total_output_produced += total;
    *produced = total;
    return 0;
}

static int r8b_reset(a1_src *s) {
    r8b_t *r = (r8b_t *)s->impl;
    for (auto *p : r->res) p->clear();
    /* drain() trims to the cumulative ideal output count computed from
     * total_input_fed/total_output_produced; leaving them stale after a
     * reset made post-reset drain mis-trim (review blocker). */
    r->total_input_fed = 0;
    r->total_output_produced = 0;
    return 0;
}

static long r8b_latency(const a1_src *s) {
    return ((r8b_t *)s->impl)->latency;
}

static long r8b_required(const a1_src *s, long out_frames) {
    r8b_t *r = (r8b_t *)s->impl;
    return (long)ceil((double)out_frames * r->in_rate / (double)r->out_rate) +
           r->latency;
}

static void r8b_destroy(a1_src *s) {
    r8b_t *r = (r8b_t *)s->impl;
    for (auto *p : r->res) delete p;
    delete r;
    s->impl = NULL;
}

static const a1_src_ops kOps = {
    "r8b", r8b_prepare, r8b_process, r8b_drain, r8b_reset,
    r8b_latency, r8b_required, r8b_destroy,
};

int a1_make(const char *name, a1_src *out) {
    if (strcmp(name, "r8b") != 0) return -1;
    out->ops = &kOps;
    out->impl = new r8b_t();
    return 0;
}

} // extern "C"
