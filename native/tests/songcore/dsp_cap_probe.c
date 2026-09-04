/*
 * DSP capability probe (test-only; never ships).
 *
 * Exists to establish, for one frozen FFmpeg closure (the Xmake-replayed
 * libqianqian_av.a this binary links): actual link reachability of the
 * enabled filters, negotiated audio formats (incl. graph-inserted
 * conversion), registration presence/absence, small correctness smokes,
 * and graph lifecycle timings. It decodes nothing; it is not production
 * code and exposes no API.
 *
 * Input: a key=value scenario file, one check per line ('#' comments):
 *   kind=present name=<filter>
 *   kind=absent  name=<filter>
 *   kind=graph id=<id> chain=<f1=opts;f2=opts> signal=<sine|impulse|noise>:<freq>:<amp>
 *            rate=<Hz> ch=<n> frames=<n> block=<n> [sink_fmt=flt] [lifecycle=1]
 *            [want_fmt=<fmt>] [want_rate=<Hz>] [want_channels=<n>]
 *            [expect=<spec>]
 *   expect specs (colon-separated k=v after the kind):
 *     amplitude_db:db=<x>:tol=<t>:in_amp=<a>   peak_out ~= a*10^(x/20)
 *     rms_ratio_vs_anull_db:ch=<c>:min=<a>:max=<b>
 *     bounded:max=<m>                          max|x| <= m
 *     cross_channel:src=<c>:dst=<d>:min_rms=<r>
 *     fir:taps=<a,b,c>:tol=<t>                 impulse response head
 *     duration_ratio:ratio=<r>:tol=<t>         out_frames/in_frames
 *     frames_equal                             out_frames == in_frames
 *
 * Chains must not contain spaces or ';' inside option values. Two-input
 * filters (afir, acrossfade, headphone, amix) are outside this linear
 * probe's scope; their evidence is registration + closure cost only.
 *
 * Usage: dsp_cap_probe <scenario.kv> <out.json>
 */
#define _POSIX_C_SOURCE 200809L
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/samplefmt.h>

#include "songcore.h"

/*
 * Product-shape contract pin: a Qianqian-shaped link always has the codec
 * closure live (SongCore decode contract). The volatile table below forces
 * the five song_* entry points to stay reachable so the FFmpeg codec closure
 * contributes its real live bytes at every ladder point; the filter
 * capabilities then stack on top of that baseline. Mirrors the
 * the SongCore public contract (ABI v1).
 *
 * Summing the addresses (not comparing them to NULL) matters: GCC folds
 * "&f != 0" to true and silently drops the symbol reference, which would
 * leave the whole closure unpulled and make the closure look smaller than it is.
 */
static void (*const qn_contract_pins[])(void) = {
    (void (*)(void))song_open,
    (void (*)(void))song_probe,
    (void (*)(void))song_read_pcm,
    (void (*)(void))song_seek,
    (void (*)(void))song_close,
};
static volatile size_t qn_contract_pin_sink;

static void qn_touch_contract(void) {
    size_t s = 0;
    for (size_t i = 0; i < sizeof(qn_contract_pins) / sizeof(qn_contract_pins[0]); i++) {
        s += (size_t)qn_contract_pins[i];
    }
    qn_contract_pin_sink = s;
}

#ifdef QN_PROBE_NO_AVFILTER
/* Codec-only baseline (avf-c0): the closure has no libavfilter, so the
 * probe keeps its product shape (same scenario file, same JSON) but every
 * graph run fails closed and every registration probe reports absent. The
 * size delta between this and the real backend lands honestly in F0. */
static const AVFilter *qn_no_filter(const char *name) { (void)name; return NULL; }
#define avfilter_get_by_name qn_no_filter
#endif

#define MAX_TOKS 32
#define MAX_TAPS 8
#define FIRST_KEEP 8
#define MAX_CHAIN 32

/* ---------------- tiny utils ---------------- */

static uint64_t fnv1a64(const uint8_t *p, size_t n, uint64_t h) {
    for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ULL; }
    return h;
}

static uint64_t splitmix64(uint64_t *s) {
    uint64_t z = (*s += 0x9E3779B97F4A7C15ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}

static double now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e9 + (double)ts.tv_nsec;
}

/* ---------------- json emit helpers ---------------- */

static FILE *g_out;
static int g_json_first = 1;

static void jsep(void) { if (!g_json_first) fputs(",", g_out); g_json_first = 0; }
static void jraw(const char *k, const char *v) {
    jsep(); fprintf(g_out, "\"%s\":\"", k);
    for (const char *p = v; *p; p++) {
        if (*p == '"' || *p == '\\') fputc('\\', g_out);
        if (*p == '\n') { fputs("\\n", g_out); continue; }
        fputc(*p, g_out);
    }
    fputc('"', g_out);
}
static void jbool(const char *k, int v) { jsep(); fprintf(g_out, "\"%s\":%s", k, v ? "true" : "false"); }
static void jint(const char *k, long long v) { jsep(); fprintf(g_out, "\"%s\":%lld", k, v); }
static void jdbl(const char *k, double v) {
    jsep();
    if (isnan(v) || isinf(v)) fprintf(g_out, "\"%s\":null", k);
    else fprintf(g_out, "\"%s\":%.9g", k, v);
}
static void jkey(const char *k) { jsep(); fprintf(g_out, "\"%s\":{", k); g_json_first = 1; }
static void jarr(const char *k) { jsep(); fprintf(g_out, "\"%s\":[", k); g_json_first = 1; }
static void jend(void) { fputs("}", g_out); g_json_first = 0; }
static void jarr_end(void) { fputs("]", g_out); g_json_first = 0; }
static void jnum(double v) {
    if (isnan(v) || isinf(v)) fputs("null", g_out);
    else fprintf(g_out, "%.9g", v);
}

/* ---------------- scenario parsing ---------------- */

typedef struct { char *k, *v; } Tok;

static int parse_kv_line(char *line, Tok *toks, int max) {
    int n = 0;
    for (char *p = strtok(line, " \t\r\n"); p && n < max; p = strtok(NULL, " \t\r\n")) {
        char *eq = strchr(p, '=');
        if (!eq) return -1;
        *eq = 0;
        toks[n].k = p;
        toks[n].v = eq + 1;
        n++;
    }
    return n;
}

static int tok_get(Tok *t, int n, const char *k, const char **out) {
    for (int i = 0; i < n; i++)
        if (!strcmp(t[i].k, k)) { *out = t[i].v; return 1; }
    return 0;
}

/* ---------------- signal + stats ---------------- */

typedef struct { int type; /* 0 sine 1 impulse 2 noise */ double freq, amp; } Signal;

typedef struct {
    long long in_frames_pushed, out_frames;
    double peak[8], sumsq[8];
    long long n[8];
    double first[FIRST_KEEP * 8];
    int first_kept;
    uint64_t fnv;
    int all_finite;
    int channels;
    int planar;       /* negotiated sink format is planar: data[c] are planes */
    int sample_bytes; /* 4 = float32, 8 = float64 (dynamics are dbl/dblp) */
} Stats;

static void stats_init(Stats *s, int ch) {
    memset(s, 0, sizeof(*s));
    s->channels = ch;
    s->all_finite = 1;
    for (int c = 0; c < ch; c++) s->peak[c] = -1.0;
}

static inline double stats_sample(const Stats *s, uint8_t **data, int c, int i) {
    int bps = s->sample_bytes;
    const uint8_t *q = s->planar ? data[c] + (size_t)i * bps
                                 : data[0] + ((size_t)i * s->channels + c) * bps;
    if (bps == 8) {
        double v;
        memcpy(&v, q, 8);
        return v;
    }
    float v;
    memcpy(&v, q, 4);
    return (double)v;
}

static void stats_feed(Stats *s, uint8_t **data, int frames) {
    const int ch = s->channels;
    for (int i = 0; i < frames; i++) {
        for (int c = 0; c < ch; c++) {
            double v = stats_sample(s, data, c, i);
            uint64_t h = s->fnv ^ (uint64_t)(0x9E3779B9u + (uint32_t)s->sample_bytes);
            s->fnv = fnv1a64((uint8_t *)&v, 8, h);
            if (!isfinite(v)) { s->all_finite = 0; continue; }
            double a = fabs(v);
            if (a > s->peak[c]) s->peak[c] = a;
            s->sumsq[c] += v * v;
            s->n[c]++;
            if (s->first_kept < FIRST_KEEP * ch) s->first[s->first_kept++] = v;
        }
    }
    s->out_frames += frames;
}

static void gen_block(const Signal *sig, double rate, int ch, long long start,
                      int frames, float *buf) {
    uint64_t seed = 0x243F6A8885A308D3ULL;
    for (int i = 0; i < frames; i++) {
        for (int c = 0; c < ch; c++) {
            long long gi = start + i;
            double v = 0.0;
            if (sig->type == 0) {
                v = sig->amp * sin(2.0 * M_PI * sig->freq * (double)gi / rate);
            } else if (sig->type == 1) {
                v = (gi == 0 && c == 0) ? sig->amp : 0.0;
            } else {
                v = sig->amp * (((double)(int64_t)splitmix64(&seed) /
                                 9223372036854775808.0) - 1.0);
            }
            buf[i * ch + c] = (float)v;
        }
    }
}

/* ---------------- graph runner ---------------- */

typedef struct {
    int ok, config_failed, drained_eof, rebuild_ok;
    char err[512];
    char neg_fmt[16], neg_layout[64];
    char b_layout[32];
    int neg_rate, neg_channels;
    char filters[MAX_CHAIN + 2][64];
    int n_filters;
    char auto_ins[8][64];
    int n_auto;
    Stats stats;
    double create_config_ns, first_out_ns, process_ns, destroy_ns;
    long long first_out_in_frames;
    uint64_t rebuild_fnv;
} GraphRun;

#ifdef QN_PROBE_NO_AVFILTER
static int run_graph(const char *chain, int rate, int ch, const Signal *sig,
                     long long frames, int block, int constrain_flt,
                     int do_lifecycle, GraphRun *r) {
    (void)chain; (void)rate; (void)ch; (void)sig; (void)frames; (void)block;
    (void)constrain_flt; (void)do_lifecycle;
    memset(r, 0, sizeof(*r));
    r->ok = 0;
    snprintf(r->err, sizeof(r->err), "backend none: closure has no libavfilter");
    return 1;
}
#else
static int run_graph(const char *chain, int rate, int ch, const Signal *sig,
                     long long frames, int block, int constrain_flt,
                     int do_lifecycle, GraphRun *r);

/* lifecycle rebuild: same graph again, output must be bit-identical */
static int run_graph_rebuild(const char *chain, int rate, int ch, const Signal *sig,
                             long long frames, int block, int constrain_flt,
                             uint64_t *fnv_out) {
    GraphRun r2;
    int rc = run_graph(chain, rate, ch, sig, frames, block, constrain_flt, 0, &r2);
    *fnv_out = r2.stats.fnv;
    return rc == 0 && r2.ok;
}

static int run_graph(const char *chain, int rate, int ch, const Signal *sig,
                     long long frames, int block, int constrain_flt,
                     int do_lifecycle, GraphRun *r) {
    memset(r, 0, sizeof(*r));
    r->ok = 1;
    stats_init(&r->stats, ch);

    char chainbuf[1024];
    snprintf(chainbuf, sizeof(chainbuf), "%s", chain);

    double t0 = now_ns();
    AVFilterGraph *g = avfilter_graph_alloc();
    if (!g) { r->ok = 0; snprintf(r->err, sizeof(r->err), "graph_alloc failed"); goto fail_nograph; }

    char asrc_args[128];
    snprintf(asrc_args, sizeof(asrc_args),
             "time_base=1/%d:sample_rate=%d:sample_fmt=flt:channel_layout=%s",
             rate, rate, ch == 1 ? "mono" : "stereo");
    AVFilterContext *src = NULL;
    int ret = avfilter_graph_create_filter(&src, avfilter_get_by_name("abuffer"),
                                           "in", asrc_args, NULL, g);
    if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "abuffer: %s", av_err2str(ret)); goto fail; }

    AVFilterContext *sink = avfilter_graph_alloc_filter(
        g, avfilter_get_by_name("abuffersink"), "out");
    if (!sink) { r->ok = 0; snprintf(r->err, sizeof(r->err), "abuffersink missing"); goto fail; }
    if (constrain_flt) {
        /* n9 abuffersink constraints are ARRAY options renamed from the old
         * av_opt_set_bin-era names: "sample_formats" here */
        enum AVSampleFormat flt = AV_SAMPLE_FMT_FLT;
        ret = av_opt_set_array(sink, "sample_formats", AV_OPT_SEARCH_CHILDREN,
                               0, 1, AV_OPT_TYPE_SAMPLE_FMT, &flt);
        if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "sink fmt opt: %s", av_err2str(ret)); goto fail; }
    }
    ret = avfilter_init_dict(sink, NULL);
    if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "sink init: %s", av_err2str(ret)); goto fail; }

    AVFilterContext *prev = src;
    char *save = NULL;
    int n_links = 0;
    for (char *seg = strtok_r(chainbuf, ";", &save); seg;
         seg = strtok_r(NULL, ";", &save)) {
        char name[64], opts[512], inst[72];
        char *eq = strchr(seg, '=');
        if (eq) {
            size_t nl = (size_t)(eq - seg);
            if (nl >= sizeof(name)) nl = sizeof(name) - 1;
            memcpy(name, seg, nl); name[nl] = 0;
            snprintf(opts, sizeof(opts), "%s", eq + 1);
        } else {
            snprintf(name, sizeof(name), "%s", seg);
            opts[0] = 0;
        }
        snprintf(inst, sizeof(inst), "n%d", n_links);
        const AVFilter *f = avfilter_get_by_name(name);
        if (!f) {
            r->ok = 0;
            snprintf(r->err, sizeof(r->err), "filter not registered: %s", name);
            goto fail;
        }
        AVFilterContext *ctx = NULL;
        ret = avfilter_graph_create_filter(&ctx, f, inst, opts, NULL, g);
        if (ret < 0) {
            r->ok = 0;
            snprintf(r->err, sizeof(r->err), "init %s: %s", name, av_err2str(ret));
            goto fail;
        }
        ret = avfilter_link(prev, 0, ctx, 0);
        if (ret < 0) {
            r->ok = 0;
            snprintf(r->err, sizeof(r->err), "link %s: %s", name, av_err2str(ret));
            goto fail;
        }
        prev = ctx;
        n_links++;
    }
    ret = avfilter_link(prev, 0, sink, 0);
    if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "link sink: %s", av_err2str(ret)); goto fail; }

    ret = avfilter_graph_config(g, NULL);
    if (ret < 0) {
        r->ok = 0;
        r->config_failed = 1;
        snprintf(r->err, sizeof(r->err), "graph_config: %s", av_err2str(ret));
        goto fail;
    }
    r->create_config_ns = now_ns() - t0;

    for (unsigned i = 0; i < g->nb_filters; i++) {
        AVFilterContext *fc = g->filters[i];
        if (r->n_filters < MAX_CHAIN + 2)
            snprintf(r->filters[r->n_filters++], 64, "%s", fc->name);
        if (!strncmp(fc->name, "auto_", 5) && r->n_auto < 8)
            snprintf(r->auto_ins[r->n_auto++], 64, "%s", fc->name);
    }
    {
        AVChannelLayout lay = {0};
        if (av_buffersink_get_ch_layout(sink, &lay) == 0) {
            av_channel_layout_describe(&lay, r->neg_layout, sizeof(r->neg_layout));
            r->neg_channels = lay.nb_channels;
            av_channel_layout_uninit(&lay);
        }
        r->neg_rate = av_buffersink_get_sample_rate(sink);
        int fmt = av_buffersink_get_format(sink);
        r->stats.planar = av_sample_fmt_is_planar(fmt);
        r->stats.sample_bytes = av_get_bytes_per_sample(fmt);
        const char *fn = av_get_sample_fmt_name(fmt);
        snprintf(r->neg_fmt, sizeof(r->neg_fmt), "%s", fn ? fn : "?");
    }

    AVFrame *inf = av_frame_alloc();
    AVFrame *outf = av_frame_alloc();
    float *gen = malloc(sizeof(float) * (size_t)block * (size_t)ch);
    if (!inf || !outf || !gen) {
        r->ok = 0; snprintf(r->err, sizeof(r->err), "oom");
        av_frame_free(&inf); av_frame_free(&outf); free(gen);
        goto fail;
    }
    inf->format = AV_SAMPLE_FMT_FLT;
    inf->sample_rate = rate;
    av_channel_layout_default(&inf->ch_layout, ch);

    long long done = 0;
    int got_first = 0;
    double tp = now_ns();
    while (done < frames) {
        int n = (frames - done > block) ? block : (int)(frames - done);
        gen_block(sig, rate, ch, done, n, gen);
        /* av_frame_unref below resets these; they must be re-set each block */
        inf->format = AV_SAMPLE_FMT_FLT;
        inf->sample_rate = rate;
        av_channel_layout_uninit(&inf->ch_layout);
        av_channel_layout_default(&inf->ch_layout, ch);
        inf->nb_samples = n;
        inf->pts = done;
        if (av_frame_get_buffer(inf, 0) < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "frame_get_buffer"); break; }
        memcpy(inf->data[0], gen, sizeof(float) * (size_t)n * (size_t)ch);
        r->stats.in_frames_pushed += n;
        ret = av_buffersrc_add_frame(src, inf);
        av_frame_unref(inf);
        if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "add_frame: %s", av_err2str(ret)); break; }
        while ((ret = av_buffersink_get_frame(sink, outf)) >= 0) {
            if (!got_first) { got_first = 1; r->first_out_ns = now_ns() - tp;
                              r->first_out_in_frames = r->stats.in_frames_pushed; }
            stats_feed(&r->stats, outf->data, outf->nb_samples);
            av_frame_unref(outf);
        }
        if (ret != AVERROR(EAGAIN)) { r->ok = 0; snprintf(r->err, sizeof(r->err), "sink pull: %s", av_err2str(ret)); break; }
        done += n;
    }
    if (r->ok) {
        ret = av_buffersrc_add_frame(src, NULL);
        if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "eof push: %s", av_err2str(ret)); }
        else {
            while ((ret = av_buffersink_get_frame(sink, outf)) >= 0) {
                stats_feed(&r->stats, outf->data, outf->nb_samples);
                av_frame_unref(outf);
            }
            r->drained_eof = (ret == AVERROR_EOF);
            if (!r->drained_eof) { r->ok = 0; snprintf(r->err, sizeof(r->err), "drain: %s", av_err2str(ret)); }
        }
    }
    r->process_ns = now_ns() - tp;
    r->stats.fnv = fnv1a64((const uint8_t *)&r->stats.out_frames, sizeof(long long),
                           r->stats.fnv);
    av_frame_free(&inf);
    av_frame_free(&outf);
    free(gen);

    {
        double td = now_ns();
        if (do_lifecycle && r->ok) {
            avfilter_graph_free(&g);
            g = NULL;
            r->rebuild_ok = run_graph_rebuild(chain, rate, ch, sig, frames, block,
                                              constrain_flt, &r->rebuild_fnv);
            if (!r->rebuild_ok) { r->ok = 0; snprintf(r->err, sizeof(r->err), "rebuild diverged"); }
        }
        avfilter_graph_free(&g);
        r->destroy_ns = now_ns() - td;
    }
    return r->ok ? 0 : 1;

fail:
    avfilter_graph_free(&g);
    r->destroy_ns = 0;
fail_nograph:
    return r->ok ? 0 : 1;
}
#endif /* !QN_PROBE_NO_AVFILTER */

/* ---------------- two-input graph runner  ----------------
 *
 * Minimal functional harness for the multi-input filters the linear probe
 * cannot reach (afir: signal+IR, acrossfade: A+B, headphone: signal+HRIR).
 * Only proves: graph config succeeds, process succeeds, drain reaches EOF,
 * output finite and non-empty. No audio-quality competition.
 *
 * filter_spec = exactly ONE filter (name[=opts]); input A feeds in-pad 0,
 * input B feeds in-pad 1. */
#ifdef QN_PROBE_NO_AVFILTER
static int run_graph2(const char *filter_spec, int rate, int ch,
                      const Signal *sig_a, long long frames_a,
                      int ch_b, const Signal *sig_b, long long frames_b,
                      int block, GraphRun *r) {
    (void)filter_spec; (void)rate; (void)ch; (void)sig_a; (void)frames_a;
    (void)ch_b; (void)sig_b; (void)frames_b; (void)block;
    memset(r, 0, sizeof(*r));
    r->ok = 0;
    snprintf(r->err, sizeof(r->err), "backend none: closure has no libavfilter");
    return 1;
}
#else
static int run_graph2(const char *filter_spec, int rate, int ch,
                      const Signal *sig_a, long long frames_a,
                      int ch_b, const Signal *sig_b, long long frames_b,
                      int block, GraphRun *r) {
    memset(r, 0, sizeof(*r));
    r->ok = 1;
    stats_init(&r->stats, ch);

    AVFilterGraph *g = avfilter_graph_alloc();
    if (!g) { r->ok = 0; snprintf(r->err, sizeof(r->err), "graph_alloc failed"); return 1; }

    char asrc_args[128];
    snprintf(asrc_args, sizeof(asrc_args),
             "time_base=1/%d:sample_rate=%d:sample_fmt=flt:channel_layout=%s",
             rate, rate, ch == 1 ? "mono" : "stereo");
    AVFilterContext *src_a = NULL, *src_b = NULL;
    int ret = avfilter_graph_create_filter(&src_a, avfilter_get_by_name("abuffer"),
                                           "inA", asrc_args, NULL, g);
    if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "abufferA: %s", av_err2str(ret)); goto fail; }
    {
        char bsrc_args[128];
        const char *lay_b = ch_b == 1 ? "mono" : ch_b == 2 ? "stereo"
                          : ch_b == 4 ? "quad" : ch_b == 6 ? "5.1" : "stereo";
        snprintf(r->b_layout, sizeof(r->b_layout), "%s", lay_b);
        snprintf(bsrc_args, sizeof(bsrc_args),
                 "time_base=1/%d:sample_rate=%d:sample_fmt=flt:channel_layout=%s",
                 rate, rate, lay_b);
        ret = avfilter_graph_create_filter(&src_b, avfilter_get_by_name("abuffer"),
                                          "inB", bsrc_args, NULL, g);
        if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "abufferB: %s", av_err2str(ret)); goto fail; }
    }

    AVFilterContext *sink = avfilter_graph_alloc_filter(
        g, avfilter_get_by_name("abuffersink"), "out");
    if (!sink) { r->ok = 0; snprintf(r->err, sizeof(r->err), "abuffersink missing"); goto fail; }
    ret = avfilter_init_dict(sink, NULL);
    if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "sink init: %s", av_err2str(ret)); goto fail; }

    /* the single multi-input filter */
    {
        char specbuf[512], name[64], opts[384];
        snprintf(specbuf, sizeof(specbuf), "%s", filter_spec);
        char *eq = strchr(specbuf, '=');
        if (eq) {
            size_t nl = (size_t)(eq - specbuf);
            if (nl >= sizeof(name)) nl = sizeof(name) - 1;
            memcpy(name, specbuf, nl); name[nl] = 0;
            snprintf(opts, sizeof(opts), "%s", eq + 1);
        } else {
            snprintf(name, sizeof(name), "%s", specbuf);
            opts[0] = 0;
        }
        const AVFilter *f = avfilter_get_by_name(name);
        if (!f) {
            r->ok = 0;
            snprintf(r->err, sizeof(r->err), "filter not registered: %s", name);
            goto fail;
        }
        AVFilterContext *ctx = NULL;
        ret = avfilter_graph_create_filter(&ctx, f, "m", opts, NULL, g);
        if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "init %s: %s", name, av_err2str(ret)); goto fail; }
        if (ret = avfilter_link(src_a, 0, ctx, 0), ret < 0) {
            r->ok = 0; snprintf(r->err, sizeof(r->err), "linkA %s: %s", name, av_err2str(ret)); goto fail;
        }
        if (ret = avfilter_link(src_b, 0, ctx, 1), ret < 0) {
            r->ok = 0; snprintf(r->err, sizeof(r->err), "linkB %s: %s", name, av_err2str(ret)); goto fail;
        }
        if (ret = avfilter_link(ctx, 0, sink, 0), ret < 0) {
            r->ok = 0; snprintf(r->err, sizeof(r->err), "link sink: %s", av_err2str(ret)); goto fail;
        }
    }

    ret = avfilter_graph_config(g, NULL);
    if (ret < 0) {
        r->ok = 0;
        r->config_failed = 1;
        snprintf(r->err, sizeof(r->err), "graph_config: %s", av_err2str(ret));
        goto fail;
    }

    for (unsigned i = 0; i < g->nb_filters; i++) {
        AVFilterContext *fc = g->filters[i];
        if (r->n_filters < MAX_CHAIN + 2)
            snprintf(r->filters[r->n_filters++], 64, "%s", fc->name);
        if (!strncmp(fc->name, "auto_", 5) && r->n_auto < 8)
            snprintf(r->auto_ins[r->n_auto++], 64, "%s", fc->name);
    }
    {
        AVChannelLayout lay = {0};
        if (av_buffersink_get_ch_layout(sink, &lay) == 0) {
            av_channel_layout_describe(&lay, r->neg_layout, sizeof(r->neg_layout));
            r->neg_channels = lay.nb_channels;
            av_channel_layout_uninit(&lay);
        }
        r->neg_rate = av_buffersink_get_sample_rate(sink);
        int fmt = av_buffersink_get_format(sink);
        r->stats.planar = av_sample_fmt_is_planar(fmt);
        r->stats.sample_bytes = av_get_bytes_per_sample(fmt);
        const char *fn = av_get_sample_fmt_name(fmt);
        snprintf(r->neg_fmt, sizeof(r->neg_fmt), "%s", fn ? fn : "?");
    }

    AVFrame *inf = av_frame_alloc();
    AVFrame *outf = av_frame_alloc();
    float *gen = malloc(sizeof(float) * (size_t)block * (size_t)(ch > ch_b ? ch : ch_b));
    if (!inf || !outf || !gen) {
        r->ok = 0; snprintf(r->err, sizeof(r->err), "oom");
        av_frame_free(&inf); av_frame_free(&outf); free(gen);
        goto fail;
    }

    long long done_a = 0, done_b = 0;
    int eof_b = 0, got_first = 0;
    while (done_a < frames_a || !eof_b) {
        if (done_a < frames_a) {
            long long n = (frames_a - done_a > block) ? block : (frames_a - done_a);
            gen_block(sig_a, rate, ch, done_a, (int)n, gen);
            inf->format = AV_SAMPLE_FMT_FLT;
            inf->sample_rate = rate;
            av_channel_layout_uninit(&inf->ch_layout);
            av_channel_layout_default(&inf->ch_layout, ch);
            inf->nb_samples = (int)n;
            inf->pts = done_a;
            if (av_frame_get_buffer(inf, 0) < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "frame_get_buffer"); break; }
            memcpy(inf->data[0], gen, sizeof(float) * (size_t)n * (size_t)ch);
            r->stats.in_frames_pushed += n;
            ret = av_buffersrc_add_frame(src_a, inf);
            av_frame_unref(inf);
            if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "addA: %s", av_err2str(ret)); break; }
            done_a += n;
        }
        if (done_b < frames_b) {
            long long n = (frames_b - done_b > block) ? block : (frames_b - done_b);
            gen_block(sig_b, rate, ch_b, done_b, (int)n, gen);
            inf->format = AV_SAMPLE_FMT_FLT;
            inf->sample_rate = rate;
            av_channel_layout_uninit(&inf->ch_layout);
            /* use the SAME layout string the abuffer was configured with;
             * default(N) masks need not match named layouts (quad vs 4.0) */
            if (av_channel_layout_from_string(&inf->ch_layout, r->b_layout) < 0)
                av_channel_layout_default(&inf->ch_layout, ch_b);
            inf->nb_samples = (int)n;
            inf->pts = done_b;
            if (av_frame_get_buffer(inf, 0) < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "frameB_get_buffer"); break; }
            memcpy(inf->data[0], gen, sizeof(float) * (size_t)n * (size_t)ch_b);
            ret = av_buffersrc_add_frame(src_b, inf);
            av_frame_unref(inf);
            if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "addB: %s", av_err2str(ret)); break; }
            done_b += n;
            if (done_b >= frames_b) {
                ret = av_buffersrc_add_frame(src_b, NULL); /* B reached EOF */
                if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "eofB: %s", av_err2str(ret)); break; }
                eof_b = 1;
            }
        }
        while ((ret = av_buffersink_get_frame(sink, outf)) >= 0) {
            if (!got_first) got_first = 1;
            stats_feed(&r->stats, outf->data, outf->nb_samples);
            av_frame_unref(outf);
        }
        if (ret != AVERROR(EAGAIN)) { r->ok = 0; snprintf(r->err, sizeof(r->err), "sink pull: %s", av_err2str(ret)); break; }
    }
    if (r->ok) {
        ret = av_buffersrc_add_frame(src_a, NULL);
        if (ret < 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "eof push A: %s", av_err2str(ret)); }
        else {
            while ((ret = av_buffersink_get_frame(sink, outf)) >= 0) {
                stats_feed(&r->stats, outf->data, outf->nb_samples);
                av_frame_unref(outf);
            }
            r->drained_eof = (ret == AVERROR_EOF);
            if (!r->drained_eof) { r->ok = 0; snprintf(r->err, sizeof(r->err), "drain: %s", av_err2str(ret)); }
        }
    }
    /* functional smoke success: configured, processed, drained, finite,
     * non-empty output */
    if (r->ok) {
        if (r->stats.out_frames <= 0) { r->ok = 0; snprintf(r->err, sizeof(r->err), "empty output"); }
        else if (!r->stats.all_finite) { r->ok = 0; snprintf(r->err, sizeof(r->err), "non-finite output"); }
    }
    r->stats.fnv = fnv1a64((const uint8_t *)&r->stats.out_frames, sizeof(long long),
                           r->stats.fnv);
    av_frame_free(&inf);
    av_frame_free(&outf);
    free(gen);

fail:
    avfilter_graph_free(&g);
    return r->ok ? 0 : 1;
}
#endif /* !QN_PROBE_NO_AVFILTER */

/* ---------------- expectations ---------------- */

/* scan remaining "k=v" tokens of an expect spec */
static double spec_val(char **save, const char *k, double def) {
    char *t;
    while ((t = strtok_r(NULL, ":", save))) {
        char *eq = strchr(t, '=');
        if (!eq) continue;
        *eq = 0;
        if (!strcmp(t, k)) return atof(eq + 1);
    }
    return def;
}

static int eval_expect(const char *spec, const GraphRun *r, const GraphRun *base,
                       double *measured, char *detail, size_t dlen) {
    char specbuf[256];
    snprintf(specbuf, sizeof(specbuf), "%s", spec);
    char *save = NULL;
    char *kind = strtok_r(specbuf, ":", &save);
    if (!kind) { snprintf(detail, dlen, "empty expect"); return 0; }

    if (!strcmp(kind, "amplitude_db")) {
        double db = spec_val(&save, "db", 0.0);
        double tol = spec_val(&save, "tol", 0.5);
        double in_pk = spec_val(&save, "in_amp", NAN);
        if (isnan(in_pk) || in_pk <= 0) { snprintf(detail, dlen, "missing in_amp"); return 0; }
        double m = 20.0 * log10(r->stats.peak[0] / in_pk);
        *measured = m;
        snprintf(detail, dlen, "measured %.3f dB want %.3f+-%.3f", m, db, tol);
        return fabs(m - db) <= tol;
    }
    if (!strcmp(kind, "rms_ratio_vs_anull_db")) {
        int c = (int)spec_val(&save, "ch", 0);
        double mn = spec_val(&save, "min", 0.0), mx = spec_val(&save, "max", 999);
        if (!base->ok || base->stats.n[c] == 0 || r->stats.n[c] == 0) {
            snprintf(detail, dlen, "baseline unavailable"); return 0;
        }
        double ra = sqrt(r->stats.sumsq[c] / (double)r->stats.n[c]);
        double rb = sqrt(base->stats.sumsq[c] / (double)base->stats.n[c]);
        double m = rb > 0 ? 20.0 * log10(ra / rb) : -999;
        *measured = m;
        snprintf(detail, dlen, "ratio %.3f dB want [%.3f,%.3f]", m, mn, mx);
        return m >= mn && m <= mx;
    }
    if (!strcmp(kind, "bounded")) {
        double mx = spec_val(&save, "max", 1.0);
        double p = -1;
        for (int c = 0; c < r->stats.channels; c++) if (r->stats.peak[c] > p) p = r->stats.peak[c];
        *measured = p;
        snprintf(detail, dlen, "peak %.4f <= %.4f", p, mx);
        return p >= 0 && p <= mx;
    }
    if (!strcmp(kind, "cross_channel")) {
        int sp = (int)spec_val(&save, "src", 0), d = (int)spec_val(&save, "dst", 1);
        double mr = spec_val(&save, "min_rms", 1e-5);
        int src_free = (int)spec_val(&save, "src_free", 0);
        double rs = r->stats.n[sp] ? sqrt(r->stats.sumsq[sp] / (double)r->stats.n[sp]) : 0;
        double rd = r->stats.n[d] ? sqrt(r->stats.sumsq[d] / (double)r->stats.n[d]) : 0;
        *measured = rd;
        snprintf(detail, dlen, "rms src %.6f dst %.6f (min %.1e)", rs, rd, mr);
        /* src_free: routes that MOVE the signal off the source channel
         * (e.g. channelmap swap) legitimately leave the src silent */
        return rd >= mr && (src_free || rs > rd * 0.01);
    }
    if (!strcmp(kind, "fir")) {
        double tol = 0.02;
        char rest[256] = "";
        char *t;
        while ((t = strtok_r(NULL, ":", &save))) {
            if (!strncmp(t, "taps=", 5)) snprintf(rest, sizeof(rest), "%s", t + 5);
            else if (!strncmp(t, "tol=", 4)) tol = atof(t + 4);
        }
        char tapslog[128] = "";
        int ok = 1, i = 0;
        char *save2 = NULL;
        for (char *tp = strtok_r(rest, ",", &save2); tp;
             tp = strtok_r(NULL, ",", &save2), i++) {
            double want = atof(tp);
            double got = (i < r->stats.first_kept) ? r->stats.first[i] : 0.0;
            if (fabs(got - want) > tol) ok = 0;
            char one[32];
            snprintf(one, sizeof(one), "%s[%.4f]", i ? "," : "", got);
            strncat(tapslog, one, sizeof(tapslog) - strlen(tapslog) - 1);
        }
        *measured = i;
        snprintf(detail, dlen, "taps%s tol %.3f", tapslog, tol);
        return ok && i > 0;
    }
    if (!strcmp(kind, "duration_ratio")) {
        double want = spec_val(&save, "ratio", 1.0), tol = spec_val(&save, "tol", 0.02);
        double m = r->stats.in_frames_pushed > 0
                       ? (double)r->stats.out_frames / (double)r->stats.in_frames_pushed
                       : 0;
        *measured = m;
        snprintf(detail, dlen, "dur ratio %.5f want %.5f+-%.3f", m, want, tol);
        return fabs(m - want) <= tol;
    }
    if (!strcmp(kind, "frames_equal")) {
        *measured = (double)r->stats.out_frames;
        snprintf(detail, dlen, "in %lld out %lld", r->stats.in_frames_pushed,
                 r->stats.out_frames);
        return r->stats.out_frames == r->stats.in_frames_pushed;
    }
    snprintf(detail, dlen, "unknown expect kind %s", kind);
    return 0;
}

/* ---------------- main ---------------- */

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s <scenario.kv> <out.json>\n", argv[0]);
        return 2;
    }
    FILE *sf = fopen(argv[1], "r");
    if (!sf) { perror("scenario"); return 2; }
    g_out = fopen(argv[2], "w");
    if (!g_out) { perror("out"); return 2; }

    char line[2048];
    int total = 0, pass = 0;
    qn_touch_contract();
    fprintf(g_out, "{\"contract_pin_addr_sum_lowbits\":%d,\"results\":[",
            (int)(qn_contract_pin_sink & 0xFF));
    g_json_first = 1;

    while (fgets(line, sizeof(line), sf)) {
        if (line[0] == '#' || line[0] == '\n' || line[0] == '\r') continue;
        Tok toks[MAX_TOKS];
        int n = parse_kv_line(line, toks, MAX_TOKS);
        if (n <= 0) continue;
        const char *kind = NULL;
        tok_get(toks, n, "kind", &kind);
        if (!kind) continue;
        total++;

        if (!strcmp(kind, "present") || !strcmp(kind, "absent")) {
            const char *name = NULL;
            tok_get(toks, n, "name", &name);
            if (!name) { total--; continue; }
            int found = avfilter_get_by_name(name) != NULL;
            int want = !strcmp(kind, "present");
            int ok = found == want;
            if (ok) pass++;
            jsep();
            fprintf(g_out, "{\"kind\":\"registration\",\"want\":\"%s\",\"name\":\"%s\","
                           "\"found\":%s,\"ok\":%s}",
                    kind, name, found ? "true" : "false", ok ? "true" : "false");
            continue;
        }
        if (!strcmp(kind, "graph2")) {
            const char *id = NULL, *chain = NULL, *sigstr = NULL, *s = NULL;
            const char *sigbstr = NULL;
            long long rate = 48000, ch = 2, frames = 48000, block = 1024;
            long long frames_b = 64, ch_b = 0;
            tok_get(toks, n, "id", &id);
            tok_get(toks, n, "chain", &chain);
            tok_get(toks, n, "signal", &sigstr);
            tok_get(toks, n, "signal_b", &sigbstr);
            if (tok_get(toks, n, "rate", &s)) rate = atoll(s);
            if (tok_get(toks, n, "ch", &s)) ch = atoll(s);
            if (tok_get(toks, n, "ch_b", &s)) ch_b = atoll(s);
            if (!ch_b) ch_b = ch;
            if (tok_get(toks, n, "frames", &s)) frames = atoll(s);
            if (tok_get(toks, n, "frames_b", &s)) frames_b = atoll(s);
            if (tok_get(toks, n, "block", &s)) block = atoll(s);
            if (!chain || !sigstr || !sigbstr) { total--; continue; }

            Signal sa = {0, 1000.0, 0.5}, sb = {0, 1000.0, 0.5};
            {
                char sb2[128];
                const char *specs[2] = { sigstr, sigbstr };
                Signal *dst[2] = { &sa, &sb };
                for (int k = 0; k < 2; k++) {
                    snprintf(sb2, sizeof(sb2), "%s", specs[k]);
                    char *c1 = strchr(sb2, ':');
                    if (c1) {
                        char *c2 = strchr(c1 + 1, ':');
                        if (c2) { dst[k]->amp = atof(c2 + 1); *c2 = 0; }
                        dst[k]->freq = atof(c1 + 1);
                        *c1 = 0;
                    }
                    dst[k]->type = !strcmp(sb2, "sine") ? 0
                                 : !strcmp(sb2, "impulse") ? 1 : 2;
                }
            }

            GraphRun r;
            run_graph2(chain, (int)rate, (int)ch, &sa, frames, (int)ch_b,
                       &sb, frames_b, (int)block, &r);
            int ok = r.ok;   /* config + process + drain + finite + non-empty */
            if (ok) pass++;

            jsep();
            fprintf(g_out, "{\"kind\":\"graph2\",\"multi_input\":true,"
                           "\"id\":\"%s\",\"ok\":%s,\"chain\":\"%s\"",
                    id ? id : "?", ok ? "true" : "false", chain ? chain : "");
            g_json_first = 0;
            if (!r.ok) jraw("error", r.err);
            jkey("negotiated");
            jraw("fmt", r.neg_fmt); jint("rate", r.neg_rate);
            jint("channels", r.neg_channels); jraw("layout", r.neg_layout);
            jend();
            jarr("auto_inserted");
            for (int i = 0; i < r.n_auto; i++) {
                if (i) fputc(',', g_out);
                fprintf(g_out, "\"%s\"", r.auto_ins[i]);
            }
            jarr_end();
            jbool("all_finite", r.stats.all_finite);
            jbool("drained_eof", r.drained_eof);
            jint("in_frames", r.stats.in_frames_pushed);
            jint("out_frames", r.stats.out_frames);
            fputs("}", g_out);
            g_json_first = 0;
            continue;
        }
        if (strcmp(kind, "graph")) continue;

        const char *id = NULL, *chain = NULL, *sigstr = NULL, *expect = NULL;
        const char *want_fmt = NULL, *s = NULL;
        long long rate = 48000, ch = 2, frames = 48000, block = 1024;
        long long want_channels = -1, want_rate = -1;
        int constrain_flt = 0, lifecycle = 0;
        tok_get(toks, n, "id", &id);
        tok_get(toks, n, "chain", &chain);
        tok_get(toks, n, "signal", &sigstr);
        tok_get(toks, n, "expect", &expect);
        tok_get(toks, n, "want_fmt", &want_fmt);
        if (tok_get(toks, n, "sink_fmt", &s) && !strcmp(s, "flt")) constrain_flt = 1;
        if (tok_get(toks, n, "lifecycle", &s)) lifecycle = atoi(s);
        if (tok_get(toks, n, "rate", &s)) rate = atoll(s);
        if (tok_get(toks, n, "ch", &s)) ch = atoll(s);
        if (tok_get(toks, n, "frames", &s)) frames = atoll(s);
        if (tok_get(toks, n, "block", &s)) block = atoll(s);
        if (tok_get(toks, n, "want_channels", &s)) want_channels = atoll(s);
        if (tok_get(toks, n, "want_rate", &s)) want_rate = atoll(s);
        if (!chain || !sigstr) { total--; continue; }

        Signal sig = {0, 1000.0, 0.5};
        {
            char sb[128];
            snprintf(sb, sizeof(sb), "%s", sigstr);
            char *c1 = strchr(sb, ':');
            if (c1) {
                char *c2 = strchr(c1 + 1, ':');
                if (c2) { sig.amp = atof(c2 + 1); *c2 = 0; }
                sig.freq = atof(c1 + 1);
                *c1 = 0;
            }
            sig.type = !strcmp(sb, "sine") ? 0 : !strcmp(sb, "impulse") ? 1 : 2;
        }

        GraphRun base;
        int has_base = 0;
        if (expect && !strncmp(expect, "rms_ratio_vs_anull_db", 21)) {
            run_graph("anull", (int)rate, (int)ch, &sig, frames, (int)block, 0, 0, &base);
            has_base = 1;
        }

        GraphRun r;
        run_graph(chain, (int)rate, (int)ch, &sig, frames, (int)block,
                  constrain_flt, lifecycle, &r);
        (void)has_base;

        int ok = r.ok;
        if (ok && want_fmt) ok &= !strcmp(r.neg_fmt, want_fmt);
        if (ok && want_rate > 0) ok &= r.neg_rate == (int)want_rate;
        if (ok && want_channels > 0) ok &= r.neg_channels == (int)want_channels;
        char detail[256] = "";
        double measured = 0;
        int expect_ran = 0;
        if (ok && expect) {
            expect_ran = 1;
            ok &= eval_expect(expect, &r, &base, &measured, detail, sizeof(detail));
        }
        if (ok) pass++;

        jsep();
        fprintf(g_out, "{\"kind\":\"graph\",\"id\":\"%s\",\"ok\":%s,\"chain\":\"%s\"",
                id ? id : "?", ok ? "true" : "false", chain ? chain : "");
        g_json_first = 0;
        if (!r.ok) jraw("error", r.err);
        jkey("negotiated");
        jraw("fmt", r.neg_fmt); jint("rate", r.neg_rate);
        jint("channels", r.neg_channels); jraw("layout", r.neg_layout);
        jend();
        jarr("graph_filters");
        for (int i = 0; i < r.n_filters; i++) {
            if (i) fputc(',', g_out);
            fprintf(g_out, "\"%s\"", r.filters[i]);
        }
        jarr_end();
        jarr("auto_inserted");
        for (int i = 0; i < r.n_auto; i++) {
            if (i) fputc(',', g_out);
            fprintf(g_out, "\"%s\"", r.auto_ins[i]);
        }
        jarr_end();
        jbool("all_finite", r.stats.all_finite);
        jbool("drained_eof", r.drained_eof);
        jint("in_frames", r.stats.in_frames_pushed);
        jint("out_frames", r.stats.out_frames);
        jarr("peak");
        for (int c = 0; c < r.stats.channels; c++) {
            if (c) fputc(',', g_out);
            jnum(r.stats.peak[c]);
        }
        jarr_end();
        jarr("rms");
        for (int c = 0; c < r.stats.channels; c++) {
            if (c) fputc(',', g_out);
            jnum(r.stats.n[c] ? sqrt(r.stats.sumsq[c] / (double)r.stats.n[c]) : 0);
        }
        jarr_end();
        if (expect_ran) { jraw("expect_detail", detail); jdbl("expect_measured", measured); }
        jkey("timing");
        jdbl("create_config_ns", r.create_config_ns);
        jdbl("first_out_ns", r.first_out_ns);
        jint("first_out_in_frames", r.first_out_in_frames);
        jdbl("process_ns", r.process_ns);
        jdbl("ns_per_out_frame",
             r.stats.out_frames > 0 ? r.process_ns / (double)r.stats.out_frames : 0);
        jdbl("destroy_ns", r.destroy_ns);
        jbool("rebuild_ok", lifecycle ? r.rebuild_ok : 1);
        jend();
        fputs("}", g_out);
        g_json_first = 0;
    }
    fprintf(g_out, "],\"summary\":{\"total\":%d,\"pass\":%d,\"fail\":%d,"
                   "\"verdict\":\"%s\"}}\n",
            total, pass, total - pass, pass == total ? "PASS" : "FAIL");
    fclose(sf);
    fclose(g_out);
    printf("%s: %d/%d\n", pass == total ? "PASS" : "FAIL", pass, total);
    return pass == total ? 0 : 1;
}
