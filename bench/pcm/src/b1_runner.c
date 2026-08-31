/*
 * E10-B1 runner — capability-equivalent comparison of the trimmed
 * libavfilter graph (volume + 10x equalizer + alimiter) against the B0
 * thin-DSP chain (gain + eq10 + limiter). Bench-only.
 *
 * Usage: b1_runner <backend: avf|thin> <block> <in.raw> <out.json>
 * The stream is 48 kHz stereo Float32 interleaved. Measures wall time,
 * first-output latency, post-init allocations (--wrap) and the number
 * of sample-scanning filter passes.
 */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#ifndef B1_THIN_ONLY

#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavutil/channel_layout.h>
#include <libavutil/frame.h>
#include <libavutil/mem.h>
#include <libavutil/samplefmt.h>
#include <libavutil/opt.h>
#endif /* !B1_THIN_ONLY */

#include "b0_dsp.h"

static volatile uint64_t g_sink;
static long long g_alloc_calls;
static long long g_alloc_bytes;
static int g_armed;

void *__real_malloc(size_t n);
void *__real_calloc(size_t n, size_t s);
void *__real_realloc(void *p, size_t n);
void __real_free(void *p);

void *__wrap_malloc(size_t n) {
    void *p = __real_malloc(n);
    if (p && g_armed) { g_alloc_calls++; g_alloc_bytes += (long long)n; }
    return p;
}
void *__wrap_calloc(size_t n, size_t s) {
    void *p = __real_calloc(n, s);
    if (p && g_armed) { g_alloc_calls++; g_alloc_bytes += (long long)n * s; }
    return p;
}
void *__wrap_realloc(void *p, size_t n) {
    void *q = __real_realloc(p, n);
    if (q && g_armed) { g_alloc_calls++; g_alloc_bytes += (long long)n; }
    return q;
}
void __wrap_free(void *p) { __real_free(p); }

static double now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e9 + (double)ts.tv_nsec;
}

#ifndef B1_THIN_ONLY
/* ---------------------------------------------------------------- */
/* libavfilter backend                                               */
/* ---------------------------------------------------------------- */

#ifndef B1_THIN_ONLY
static int run_avf(const float *in, long in_frames, long block,
                   double *ns_total, long *first_out, long long *allocs,
                   long long *allocs_bytes, int *passes) {
    const AVFilter *buffersrc = avfilter_get_by_name("abuffer");
    const AVFilter *buffersink = avfilter_get_by_name("abuffersink");
    AVFilterGraph *graph = NULL;
    AVFilterContext *src = NULL, *sink = NULL;
    AVFrame *frame = NULL, *oframe = NULL;
    float *plane[2];
    char desc[1024];
    long pos = 0;
    double t0, t1;
    long first = -1;
    int rc = -1;
    plane[0] = plane[1] = NULL;

    graph = avfilter_graph_alloc();
    if (!graph) return -1;
    snprintf(desc, sizeof(desc),
             "abuffer=sample_rate=48000:sample_fmt=fltp:channel_layout=stereo,"
             "volume=volume=0.8,"
             "equalizer=f=31.25:t=q:w=1:g=3,equalizer=f=62.5:t=q:w=1:g=3,"
             "equalizer=f=125:t=q:w=1:g=3,equalizer=f=250:t=q:w=1:g=3,"
             "equalizer=f=500:t=q:w=1:g=3,equalizer=f=1000:t=q:w=1:g=3,"
             "equalizer=f=2000:t=q:w=1:g=3,equalizer=f=4000:t=q:w=1:g=3,"
             "equalizer=f=8000:t=q:w=1:g=3,equalizer=f=16000:t=q:w=1:g=3,"
             "alimiter=limit=1:attack=1:release=100,"
             "abuffersink");
    if (avfilter_graph_parse_ptr(graph, desc, NULL, NULL, NULL) < 0) {
        fprintf(stderr, "graph parse failed\n");
        goto out;
    }
    if (avfilter_graph_config(graph, NULL) < 0) {
        fprintf(stderr, "graph config failed\n");
        goto out;
    }
    src = avfilter_graph_get_filter(graph, "Parsed_abuffer_0");
    {
        int i2;
        for (i2 = 0; i2 < graph->nb_filters; i2++) {
            AVFilterContext *fc = graph->filters[i2];
            if (!sink && fc && fc->filter &&
                strcmp(fc->filter->name, "abuffersink") == 0)
                sink = fc;
        }
    }
    if (!src || !sink) {
        fprintf(stderr, "cannot find in/out filters\n");
        goto out;
    }
    *passes = 12; /* volume + 10 equalizer + alimiter sample-scanning */

    frame = av_frame_alloc();
    oframe = av_frame_alloc();
    plane[0] = (float *)av_malloc((size_t)block * 4);
    plane[1] = (float *)av_malloc((size_t)block * 4);
    if (!frame || !oframe || !plane[0] || !plane[1]) goto out;

    /* reset alloc counters AFTER graph config (post-init region) */
    g_alloc_calls = 0;
    g_alloc_bytes = 0;
    g_armed = 1;

    t0 = now_ns();
    while (pos < in_frames) {
        long blk = block;
        long i;
        if (blk > in_frames - pos) blk = in_frames - pos;
        /* packed -> planar adapter glue (explicit) */
        for (i = 0; i < blk; i++) {
            plane[0][i] = in[(pos + i) * 2];
            plane[1][i] = in[(pos + i) * 2 + 1];
        }
        frame->format = AV_SAMPLE_FMT_FLTP;
        frame->sample_rate = 48000;
        av_channel_layout_default(&frame->ch_layout, 2);
        frame->nb_samples = (int)blk;
        frame->data[0] = (uint8_t *)plane[0];
        frame->data[1] = (uint8_t *)plane[1];
        frame->linesize[0] = (int)blk * 4;
        frame->linesize[1] = (int)blk * 4;
        if (av_buffersrc_add_frame_flags(src, frame,
                                         AV_BUFFERSRC_FLAG_PUSH) < 0) {
            fprintf(stderr, "buffersrc add failed at %ld\n", pos);
            goto out;
        }
        while (av_buffersink_get_frame(sink, oframe) == 0) {
            if (first < 0) first = (long)pos;
            g_sink ^= (uint64_t)(oframe->data[0][0]);
            av_frame_unref(oframe);
        }
        av_frame_unref(frame);
        pos += blk;
    }
    av_buffersrc_add_frame_flags(src, NULL, 0); /* EOF */
    while (av_buffersink_get_frame(sink, oframe) == 0) {
        g_sink ^= (uint64_t)(oframe->data[0][0]);
        av_frame_unref(oframe);
    }
    t1 = now_ns();
    g_armed = 0;
    *ns_total = t1 - t0;
    *first_out = first;
    *allocs = g_alloc_calls;
    *allocs_bytes = g_alloc_bytes;
    rc = 0;
out:
    if (frame) av_frame_free(&frame);
    if (oframe) av_frame_free(&oframe);
    av_free(plane[0]);
    av_free(plane[1]);
    avfilter_graph_free(&graph);
    return rc;
}

#endif /* !B1_THIN_ONLY */
#endif /* !B1_THIN_ONLY */

/* ---------------------------------------------------------------- */
/* thin DSP backend (B0 chain: gain + eq10 + limiter)                */
/* ---------------------------------------------------------------- */

static int run_thin(const float *in, long in_frames, long block,
                    double *ns_total, long *first_out, long long *allocs,
                    long long *allocs_bytes, int *passes) {
    b0_dsp g, e, l;
    float *buf = (float *)malloc((size_t)block * 2 * sizeof(float));
    float gains[10] = {3, 3, 3, 3, 3, 3, 3, 3, 3, 3};
    long pos = 0;
    double t0, t1;
    long first = -1;
    if (!buf) return -1;
    b0_gain_new(&g, 0.8f);
    g.ops->prepare(&g, 48000, 2, 4096);
    b0_eq10_new(&e, 48000, gains);
    e.ops->prepare(&e, 48000, 2, 4096);
    b0_limiter_new(&l, 48000, 0.0f, 0.001f, 0.1f);
    l.ops->prepare(&l, 48000, 2, 4096);
    *passes = 12; /* gain + 10 eq bands + limiter */

    g_alloc_calls = 0;
    g_alloc_bytes = 0;
    g_armed = 1;

    t0 = now_ns();
    while (pos < in_frames) {
        long blk = block;
        if (blk > in_frames - pos) blk = in_frames - pos;
        memcpy(buf, in + pos * 2, (size_t)blk * 2 * sizeof(float));
        if (first < 0) first = pos;
        g.ops->process(&g, buf, blk);
        e.ops->process(&e, buf, blk);
        l.ops->process(&l, buf, blk);
        g_sink ^= (uint64_t)(buf[0]);
        pos += blk;
    }
    t1 = now_ns();
    g_armed = 0;
    *ns_total = t1 - t0;
    *first_out = first;
    *allocs = g_alloc_calls;
    *allocs_bytes = g_alloc_bytes;
    g.ops->destroy(&g);
    e.ops->destroy(&e);
    l.ops->destroy(&l);
    free(buf);
    return 0;
}

/* ---------------------------------------------------------------- */

int main(int argc, char **argv) {
    const char *backend, *in_path, *json_path;
    long block, in_frames;
    FILE *f;
    long long n = 0;
    float *in;
    double ns_total;
    long first_out;
    long long allocs, allocs_bytes;
    int passes = 0;
    int rc;

    if (argc < 5) {
        fprintf(stderr, "usage: b1_runner <avf|thin> <block> <in.raw> "
                        "<out.json>\n");
        return 2;
    }
    backend = argv[1];
    block = atol(argv[2]);
    in_path = argv[3];
    json_path = argv[4];

    f = fopen(in_path, "rb");
    if (!f) return 2;
    fseek(f, 0, SEEK_END);
    n = ftell(f) / (2 * 4);
    fseek(f, 0, SEEK_SET);
    in = (float *)malloc((size_t)n * 2 * 4);
    if (fread(in, 4, (size_t)n * 2, f) != (size_t)n * 2) return 2;
    fclose(f);
    in_frames = n;

    if (strcmp(backend, "avf") == 0) {
#ifndef B1_THIN_ONLY
        rc = run_avf(in, in_frames, block, &ns_total, &first_out, &allocs,
                     &allocs_bytes, &passes);
#else
        (void)passes; (void)allocs_bytes; rc = -1;
#endif
    } else {
        rc = run_thin(in, in_frames, block, &ns_total, &first_out, &allocs,
                      &allocs_bytes, &passes);
    }
    if (rc != 0) {
        fprintf(stderr, "backend run failed\n");
        return 1;
    }

    f = fopen(json_path, "w");
    fprintf(f, "{\n");
    fprintf(f, "  \"backend\": \"%s\",\n", backend);
    fprintf(f, "  \"block_frames\": %ld, \"input_frames\": %ld,\n",
            block, in_frames);
    fprintf(f, "  \"ns_per_stream\": %.1f,\n", ns_total);
    fprintf(f, "  \"ns_per_input_frame\": %.3f,\n", ns_total / in_frames);
    fprintf(f, "  \"xrt\": %.4f,\n",
            ((double)in_frames / 48000.0) / (ns_total / 1e9));
    fprintf(f, "  \"first_output_input_pos\": %ld,\n", first_out);
    fprintf(f, "  \"post_init_alloc_calls\": %lld, "
               "\"post_init_alloc_bytes\": %lld,\n", allocs, allocs_bytes);
    fprintf(f, "  \"sample_scanning_passes\": %d\n", passes);
    fprintf(f, "}\n");
    fclose(f);
    free(in);
    return 0;
}
