/*
 * E10-A1 runner — one candidate per binary. Feeds a deterministic raw
 * Float32 interleaved stream through the adapter, records per-call
 * consumed/produced, drains, times the stream, counts post-prepare
 * allocations (via --wrap malloc/calloc/realloc/free), writes the
 * output raw PCM and a machine JSON.
 *
 * Usage: a1_run_<cand> <in_rate> <out_rate> <channels> <block>
 *                     <in.raw> <out.raw|-> <out.json>
 *
 * The candidate name is compiled in via -DCANDIDATE=\"name\".
 */
#define _POSIX_C_SOURCE 200809L
#include "a1_contract.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static volatile uint64_t g_sink;

/* ---------------------------------------------------------------- */
/* allocation counting (linker --wrap)                               */
/* ---------------------------------------------------------------- */

static long long g_alloc_calls;
static long long g_alloc_bytes;
static long long g_free_calls;
static long long g_armed_calls;   /* allocations while armed (post-prepare) */
static long long g_armed_bytes;
static int g_armed;

static void count_alloc(const void *p, size_t n, int is_alloc) {
    if (!p) return;
    if (is_alloc) {
        g_alloc_calls++;
        g_alloc_bytes += (long long)n;
        if (g_armed) {
            g_armed_calls++;
            g_armed_bytes += (long long)n;
        }
    } else {
        g_free_calls++;
    }
}

void *__real_malloc(size_t n);
void *__real_calloc(size_t n, size_t s);
void *__real_realloc(void *p, size_t n);
void __real_free(void *p);
void *__real_aligned_alloc(size_t a, size_t n);

void *__wrap_malloc(size_t n) {
    void *p = __real_malloc(n);
    count_alloc(p, n, 1);
    return p;
}
void *__wrap_calloc(size_t n, size_t s) {
    void *p = __real_calloc(n, s);
    count_alloc(p, n * s, 1);
    return p;
}
void *__wrap_realloc(void *p, size_t n) {
    void *q = __real_realloc(p, n);
    count_alloc(q, n, 1);
    return q;
}
void __wrap_free(void *p) {
    if (p) count_alloc(p, 0, 0);
    __real_free(p);
}
void *__wrap_aligned_alloc(size_t a, size_t n) {
    void *p = __real_aligned_alloc(a, n);
    count_alloc(p, n, 1);
    return p;
}

/* ---------------------------------------------------------------- */

static double now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e9 + (double)ts.tv_nsec;
}

static uint64_t f32_bits(const float *p) {
    uint32_t b;
    memcpy(&b, p, sizeof(b));
    return (uint64_t)b;
}

int main(int argc, char **argv) {
    const char *cand = CANDIDATE;
    if (argc < 8) {
        fprintf(stderr, "usage: a1_run_<cand> in_rate out_rate channels "
                        "block in.raw out.raw out.json\n");
        return 2;
    }
    int in_rate = atoi(argv[1]);
    int out_rate = atoi(argv[2]);
    int channels = atoi(argv[3]);
    long block = atol(argv[4]);
    const char *in_path = argv[5];
    const char *out_path = argv[6];
    const char *json_path = argv[7];

    /* read input */
    FILE *f = fopen(in_path, "rb");
    if (!f) { perror("open in"); return 2; }
    fseek(f, 0, SEEK_END);
    long bytes = ftell(f);
    fseek(f, 0, SEEK_SET);
    long in_frames = bytes / (channels * (long)sizeof(float));
    float *in = malloc((size_t)(in_frames * channels) * sizeof(float));
    if (!in || fread(in, sizeof(float), (size_t)in_frames * channels, f)
            != (size_t)in_frames * channels) {
        fprintf(stderr, "read in failed\n");
        return 2;
    }
    fclose(f);

    long max_calls = in_frames / (block > 0 ? block : 1) + 16;
    long *cons_trace = malloc((size_t)max_calls * sizeof(long));
    long *prod_trace = malloc((size_t)max_calls * sizeof(long));
    float *out = malloc((size_t)(block * 2 + 4096) * channels * sizeof(float));

    a1_src src;
    if (a1_make(cand, &src) != 0) {
        fprintf(stderr, "unknown candidate %s\n", cand);
        return 2;
    }

    /* reset alloc counters to exclude startup/prepare */
    g_alloc_calls = g_alloc_bytes = g_free_calls = 0;
    g_armed_calls = g_armed_bytes = 0;

    if (src.ops->prepare(&src, in_rate, out_rate, channels, block * 8) != 0) {
        fprintf(stderr, "prepare failed\n");
        return 2;
    }
    long latency = src.ops->latency_frames(&src);
    long req1024 = src.ops->required_input_for_output(&src, 1024);

    /* armed post-prepare region: process + drain + reset must not allocate */
    g_armed = 1;
    long pos = 0, ncalls = 0;
    long long produced_total = 0, consumed_total = 0;
    long first_output_pos = -1;      /* input pos when first output appeared */
    double t0 = now_ns();
    FILE *fo = (strcmp(out_path, "-") != 0) ? fopen(out_path, "wb") : NULL;
    while (pos < in_frames) {
        long blk = block;
        if (blk > in_frames - pos) blk = in_frames - pos;
        long consumed = 0, produced = 0;
        if (src.ops->process(&src, in + pos * channels, blk, out,
                             (long)(block * 2 + 4096),
                             &consumed, &produced,
                             (pos + blk >= in_frames) ? 1 : 0) != 0) {
            fprintf(stderr, "process failed\n");
            return 2;
        }
        if (produced > 0 && first_output_pos < 0)
            first_output_pos = pos;
        if (fo && produced > 0)
            fwrite(out, sizeof(float), (size_t)produced * channels, fo);
        g_sink ^= f32_bits(out);
        if (ncalls < max_calls) {
            cons_trace[ncalls] = consumed;
            prod_trace[ncalls] = produced;
        }
        ncalls++;
        produced_total += produced;
        consumed_total += consumed;
        pos += consumed;
        if (consumed == 0) break; /* safety against infinite loop */
    }
    /* drain */
    long drained = 0;
    if (src.ops->drain(&src, out, (long)(block * 2 + 4096), &drained) != 0) {
        fprintf(stderr, "drain failed\n");
        return 2;
    }
    if (fo && drained > 0)
        fwrite(out, sizeof(float), (size_t)drained * channels, fo);
    if (fo) fclose(fo);
    g_sink ^= f32_bits(out);
    produced_total += drained;
    double t1 = now_ns();
    /* reset must be allocation-free too */
    src.ops->reset(&src);
    g_armed = 0;

    long buffered_before_drain = (long)in_frames - consumed_total;
    if (buffered_before_drain < 0) buffered_before_drain = 0;

    FILE *j = fopen(json_path, "w");
    if (!j) return 2;
    fprintf(j, "{\n");
    fprintf(j, "  \"candidate\": \"%s\",\n", cand);
    fprintf(j, "  \"in_rate\": %d, \"out_rate\": %d, \"channels\": %d,\n",
            in_rate, out_rate, channels);
    fprintf(j, "  \"block_frames\": %ld,\n", block);
    fprintf(j, "  \"input_frames\": %ld,\n", in_frames);
    fprintf(j, "  \"consumed_frames\": %lld, \"produced_frames\": %lld,\n",
            (long long)consumed_total, (long long)produced_total);
    fprintf(j, "  \"drain_produced_frames\": %ld,\n", drained);
    fprintf(j, "  \"buffered_before_drain_frames\": %ld,\n",
            buffered_before_drain);
    fprintf(j, "  \"process_calls\": %ld,\n", ncalls);
    fprintf(j, "  \"first_output_input_pos\": %ld,\n", first_output_pos);
    fprintf(j, "  \"latency_frames_reported\": %ld,\n", latency);
    fprintf(j, "  \"required_input_for_output_1024\": %ld,\n", req1024);
    fprintf(j, "  \"ns_per_stream\": %.1f,\n", t1 - t0);
    fprintf(j, "  \"ns_per_input_frame\": %.3f,\n", (t1 - t0) / in_frames);
    fprintf(j, "  \"xrt\": %.4f,\n",
            ((double)in_frames / in_rate) / ((t1 - t0) / 1e9));
    fprintf(j, "  \"alloc\": {\n");
    fprintf(j, "    \"total_calls\": %lld, \"total_bytes\": %lld, "
               "\"frees\": %lld,\n", g_alloc_calls, g_alloc_bytes, g_free_calls);
    fprintf(j, "    \"post_prepare_calls\": %lld, "
               "\"post_prepare_bytes\": %lld\n", g_armed_calls, g_armed_bytes);
    fprintf(j, "  },\n");
    fprintf(j, "  \"trace_calls\": %ld,\n",
            ncalls < max_calls ? ncalls : max_calls);
    fprintf(j, "  \"consumed_trace\": [");
    {
        long n = ncalls < max_calls ? ncalls : max_calls;
        long i;
        for (i = 0; i < n; i++)
            fprintf(j, "%s%ld", i ? "," : "", cons_trace[i]);
    }
    fprintf(j, "],\n");
    fprintf(j, "  \"produced_trace\": [");
    {
        long n = ncalls < max_calls ? ncalls : max_calls;
        long i;
        for (i = 0; i < n; i++)
            fprintf(j, "%s%ld", i ? "," : "", prod_trace[i]);
    }
    fprintf(j, "]\n");
    fprintf(j, "}\n");
    fclose(j);

    src.ops->destroy(&src);
    free(in);
    free(out);
    free(cons_trace);
    free(prod_trace);
    return 0;
}
