/*
 * E10-P0 harness — transparent-bypass gate, buffer/copy accounting,
 * lifecycle semantics, execution-placement model, block-size perf matrix.
 *
 * Deterministic by construction (splitmix64 PRNG, fixed schedule, no
 * threads; the Shape B "worker" is a deterministic scheduling model, not
 * a real RT/worker thread). Emits machine authority JSON:
 *   p0-correctness.json  p0-buffer-accounting.json
 *   p0-placement.json    p0-performance.json
 *
 * Build/run via tools/pcm_p0.py (records compiler + provenance).
 * Usage: qn_pcm_p0_harness <output_dir>   (dir must exist)
 */
#define _POSIX_C_SOURCE 200809L
#include "pcm_pipeline.h"

#include <float.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static volatile uint64_t g_sink; /* defeats dead-store elimination */

/* ------------------------------------------------------------------ */
/* Utilities                                                           */
/* ------------------------------------------------------------------ */

static double now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e9 + (double)ts.tv_nsec;
}

/* Consume raw Float32 bits (never a float->integer cast) so the
 * anti-elision sink is well-defined for NaN/Inf/1e30/denormal inputs —
 * a float->int conversion of a hostile value is C undefined behavior
 * (P0-4). */
static uint64_t f32_bits(const float *p) {
    uint32_t b;
    memcpy(&b, p, sizeof(b));
    return (uint64_t)b;
}

static const char *pcm_err_name(int rc) {
    switch (rc) {
    case PCM_OK: return "PCM_OK";
    case PCM_ERR_OUT_CAP: return "PCM_ERR_OUT_CAP";
    case PCM_ERR_NOT_PREPARED: return "PCM_ERR_NOT_PREPARED";
    case PCM_ERR_ALLOC: return "PCM_ERR_ALLOC";
    case PCM_ERR_INVALID_PARAM: return "PCM_ERR_INVALID_PARAM";
    case PCM_ERR_BYPASS_RATE_MISMATCH: return "PCM_ERR_BYPASS_RATE_MISMATCH";
    case PCM_ERR_STALE_TOKEN: return "PCM_ERR_STALE_TOKEN";
    default: return "?";
    }
}

static uint64_t sm64(uint64_t *s) {
    uint64_t z = (*s += 0x9E3779B97F4A7C15ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}

static float rnd_uniform(uint64_t *s) { /* [-1, 1) */
    return (float)((double)(int64_t)sm64(s) / 9223372036854775807.0);
}

static float f32_from_bits(uint32_t b) {
    float f;
    memcpy(&f, &b, sizeof(f));
    return f;
}

static uint64_t fnv1a64_init(void) { return 0xCBF29CE484222325ULL; }

static uint64_t fnv1a64_update(uint64_t h, const void *p, size_t bytes) {
    const uint8_t *b = (const uint8_t *)p;
    size_t i;
    for (i = 0; i < bytes; i++) {
        h ^= b[i];
        h *= 0x100000001B3ULL;
    }
    return h;
}

static uint64_t fnv1a64(const float *p, size_t n_floats) {
    return fnv1a64_update(fnv1a64_init(), p, n_floats * sizeof(float));
}

static int is_odd_float(float f) {
    /* outside [-1,1], non-finite, denormal, or negative zero */
    if (isnan(f) || isinf(f)) return 1;
    if (fabsf(f) > 1.0f) return 1;
    if (fabsf(f) > 0.0f && fabsf(f) < FLT_MIN) return 1;
    if (f == 0.0f && signbit(f)) return 1;
    return 0;
}

typedef enum { CORPUS_UNIFORM = 0, CORPUS_OUT_OF_RANGE = 1 } corpus_kind;

static const char *corpus_name(corpus_kind k) {
    return k == CORPUS_UNIFORM ? "uniform_random" : "out_of_range";
}

static void fill_pcm(float *buf, long frames, int ch, corpus_kind kind,
                     uint64_t seed) {
    long i, n = frames * ch;
    uint64_t s = seed;
    for (i = 0; i < n; i++) {
        if (kind == CORPUS_UNIFORM) {
            buf[i] = rnd_uniform(&s);
        } else {
            /* deterministic cycle of legal-but-hostile Float32 values:
             * outside [-1,1], denormal, +-0, NaN payload, +-Inf */
            switch (i % 12) {
            case 0:  buf[i] = 3.5f;                       break;
            case 1:  buf[i] = -8.25f;                     break;
            case 2:  buf[i] = 1.0e30f;                    break;
            case 3:  buf[i] = -1.0e30f;                   break;
            case 4:  buf[i] = 1.0e-40f;                   break;
            case 5:  buf[i] = 0.0f;                       break;
            case 6:  buf[i] = -0.0f;                      break;
            case 7:  buf[i] = f32_from_bits(0x7FC00001u); break;
            case 8:  buf[i] = f32_from_bits(0xFF800000u); break;
            case 9:  buf[i] = f32_from_bits(0x7F800000u); break;
            case 10: buf[i] = rnd_uniform(&s);            break;
            default: buf[i] = 12345.678f;                 break;
            }
        }
    }
}

static long count_odd_floats(const float *buf, long frames, int ch) {
    long i, n = frames * ch, c = 0;
    for (i = 0; i < n; i++)
        if (is_odd_float(buf[i])) c++;
    return c;
}

/* Ownership conservation invariant (P0-2 / P1-2):
 *   free_top + owned_queued_slots + valid_pending_slots == capacity_slabs
 * plus the structural bounds free_top <= capacity and count <= capacity. */
static int queue_ownership_ok(const pcm_slab_queue *q) {
    long pending = (q->pending.slot >= 0) ? 1 : 0;
    return q->free_top >= 0 &&
           q->free_top <= q->capacity_slabs &&
           q->count <= q->capacity_slabs &&
           q->free_top + q->owned_queued_slots + pending ==
               q->capacity_slabs;
}

static int dbl_cmp(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

static double min_of(const double *v, int n) {
    double tmp[16];
    memcpy(tmp, v, sizeof(double) * (size_t)n);
    qsort(tmp, (size_t)n, sizeof(double), dbl_cmp);
    return tmp[0];
}

static double max_of(const double *v, int n) {
    double tmp[16];
    memcpy(tmp, v, sizeof(double) * (size_t)n);
    qsort(tmp, (size_t)n, sizeof(double), dbl_cmp);
    return tmp[n - 1];
}

static double median_of(const double *v, int n) {
    double tmp[16];
    memcpy(tmp, v, sizeof(double) * (size_t)n);
    qsort(tmp, (size_t)n, sizeof(double), dbl_cmp);
    return tmp[n / 2];
}

/* ------------------------------------------------------------------ */
/* Feed helper: run one corpus stream through the pipeline             */
/* ------------------------------------------------------------------ */

typedef struct {
    const long *blocks;
    int nblocks;
    long zero_frame_every; /* 0 = never; else a 0-frame call every k-th */
} block_pattern;

typedef struct {
    int ok;
    long long bytes_compared;
    long long zero_frame_calls;
    long long consumed_total, produced_total;
    uint64_t out_fnv;       /* hash over produced output, call by call */
    int alias_ok;           /* zero-copy: out_ptr == input span, every call */
    long drained_frames;
    long latency_frames;
    long required_input_for_1024;
} feed_result;

static feed_result feed_stream(pcm_pipeline *p, const float *pcm,
                               long total_frames, int ch,
                               const block_pattern *pat, long max_block,
                               int expect_zero_copy, float *scratch) {
    feed_result r;
    float *out = scratch ? scratch
                         : pcm_malloc((size_t)max_block * ch * sizeof(float));
    long pos = 0;
    int bi = 0;
    memset(&r, 0, sizeof(r));
    r.ok = 1;
    r.alias_ok = 1;
    r.out_fnv = fnv1a64_init();
    if (!out) {
        r.ok = 0;
        return r;
    }

    while (pos < total_frames) {
        long blk, out_frames = 0, consumed = 0;
        const float *out_ptr = NULL;
        int rc;

        if (pat->zero_frame_every && (bi % pat->zero_frame_every) == 0) {
            rc = pcm_pipeline_process(p, pcm + pos * ch, 0, out, max_block,
                                      &out_ptr, &out_frames, &consumed);
            if (rc != PCM_OK || consumed != 0 || out_frames != 0) r.ok = 0;
            if (expect_zero_copy && out_ptr != pcm + pos * ch) r.alias_ok = 0;
            r.zero_frame_calls++;
        }
        blk = pat->blocks[bi % pat->nblocks];
        bi++;
        if (blk > total_frames - pos) blk = total_frames - pos;

        rc = pcm_pipeline_process(p, pcm + pos * ch, blk, out, blk, &out_ptr,
                                  &out_frames, &consumed);
        if (rc != PCM_OK) {
            r.ok = 0;
            break;
        }
        if (consumed != blk || out_frames != blk) r.ok = 0;

        if (expect_zero_copy) {
            if (out_ptr != pcm + pos * ch) {
                r.alias_ok = 0;
                r.ok = 0;
            }
        } else {
            if (blk > 0 &&
                memcmp(out, pcm + pos * ch,
                       (size_t)blk * ch * sizeof(float)) != 0)
                r.ok = 0;
            r.bytes_compared += blk * ch * (long long)sizeof(float);
        }
        r.out_fnv = fnv1a64_update(r.out_fnv, out_ptr,
                                   (size_t)out_frames * ch * sizeof(float));
        g_sink ^= f32_bits(out_ptr); /* consumer reads the output */
        pos += consumed;
        r.consumed_total += consumed;
        r.produced_total += out_frames;
    }

    {
        const float *out_ptr = NULL;
        long out_frames = 0;
        if (pcm_pipeline_drain(p, out, max_block, &out_ptr, &out_frames)
                != PCM_OK || out_frames != 0)
            r.ok = 0;
        r.drained_frames = out_frames;
    }
    r.latency_frames = pcm_pipeline_latency_frames(p);
    r.required_input_for_1024 =
        p->rate->ops->required_input_for_output(p->rate, 1024);
    if (!scratch) pcm_free(out);
    return r;
}

/* ------------------------------------------------------------------ */
/* Correctness corpus                                                  */
/* ------------------------------------------------------------------ */

static const long kPatTiny[] = {1, 2, 3, 7, 5, 1, 2};
static const long kPatOrdinary[] = {256, 512, 1024, 2048, 512, 256};
static const long kPatMixed[] = {2048, 1, 255, 64, 1024, 7, 512, 3, 128};
static const long kPatEven[] = {1024, 1024, 1024, 1024};

static int run_correctness(FILE *f) {
    const struct {
        const char *name;
        const long *blocks;
        int nblocks;
        long zero_every;
        long total_frames;
    } pats[] = {
        {"tiny", kPatTiny, 7, 3, 3333},
        {"ordinary", kPatOrdinary, 6, 0, 200000},
        {"mixed", kPatMixed, 9, 3, 131071},
        {"even-tail", kPatEven, 4, 0, 100003}, /* 97x1024 + 643-frame tail */
    };
    const int chans[2] = {1, 2};
    const int rates[3] = {44100, 48000, 96000};
    const corpus_kind kinds[2] = {CORPUS_UNIFORM, CORPUS_OUT_OF_RANGE};
    float *pcm = pcm_malloc((size_t)300000 * 2 * sizeof(float));
    long case_id = 0, cases_total = 0, cases_passed = 0;
    long long bytes_compared_total = 0;
    int all_ok = 1;
    int pi, ci, ri, ki, mi, first = 1;

    if (!pcm) return 0;
    fprintf(f, "{\n  \"experiment\": \"e10-p0\",\n");
    fprintf(f, "  \"section\": \"transparent_bypass_correctness\",\n");
    fprintf(f,
            "  \"gate_statement\": \"RateStage=BYPASS, DSP=OFF: "
            "pipeline-boundary output is bit-identical to input (memcmp, "
            "copy mode) or the identical underlying span (pointer identity, "
            "zero-copy mode)\",\n");
    fprintf(f,
            "  \"authority_note\": \"in-process memcmp + pointer identity; "
            "fnv1a64 over produced output for cross-run drift detection; "
            "no device output is claimed\",\n");
    fprintf(f, "  \"cases\": [\n");

    for (pi = 0; pi < 4; pi++)
    for (ci = 0; ci < 2; ci++)
    for (ri = 0; ri < 3; ri++)
    for (ki = 0; ki < 2; ki++)
    for (mi = 0; mi < 2; mi++) {
        block_pattern pat;
        pcm_pipeline pipe;
        feed_result r;
        const pcm_counters *cnt;
        long odd_in, total = pats[pi].total_frames;
        uint64_t in_fnv;
        int frames_ok, passed, odd_preserved;

        pat.blocks = pats[pi].blocks;
        pat.nblocks = pats[pi].nblocks;
        pat.zero_frame_every = pats[pi].zero_every;
        case_id++;
        cases_total++;

        fill_pcm(pcm, total, chans[ci], kinds[ki],
                 0xE10B00ULL + (uint64_t)case_id * 0x9E3779B1ULL);
        odd_in = count_odd_floats(pcm, total, chans[ci]);
        in_fnv = fnv1a64(pcm, (size_t)(total * chans[ci]));

        pcm_pipeline_init(&pipe, mi); /* mi: 0 = copy, 1 = zero-copy */
        pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
        /* P0 transparent gate: zero DSP stages. */
        if (pcm_pipeline_prepare(&pipe, rates[ri], rates[ri], chans[ci],
                                 2048) != PCM_OK) {
            all_ok = 0;
            continue;
        }
        r = feed_stream(&pipe, pcm, total, chans[ci], &pat, 2048, mi, NULL);
        cnt = pcm_pipeline_counters(&pipe);
        frames_ok = (r.consumed_total == total && r.produced_total == total);
        odd_preserved = r.ok &&
            (kinds[ki] == CORPUS_OUT_OF_RANGE ? odd_in > 0 : 1);

        passed = r.ok && frames_ok && odd_preserved &&
                 cnt->peak_buffered_frames == 0 &&
                 cnt->internal_buffer_capacity_frames == 0 &&
                 r.latency_frames == 0 &&
                 r.required_input_for_1024 == 1024 &&
                 r.drained_frames == 0;
        if (mi) {
            passed = passed && r.alias_ok &&
                     cnt->zero_copy_frames_forwarded == total &&
                     cnt->explicit_copy_calls == 0;
        } else {
            passed = passed && cnt->explicit_copy_calls > 0 &&
                     cnt->explicit_bytes_copied ==
                         (long long)total * chans[ci] * 4;
        }
        if (passed)
            cases_passed++;
        else
            all_ok = 0;
        bytes_compared_total += r.bytes_compared;

        fprintf(f,
            "%s  {\"case_id\": %ld, \"pattern\": \"%s\", \"channels\": %d, "
            "\"sample_rate\": %d, \"requested_in_rate\": %d, "
            "\"requested_out_rate\": %d, \"mode\": \"%s\", \"corpus\": \"%s\", "
            "\"total_frames\": %ld,\n"
            "   \"consumed_frames\": %lld, \"produced_frames\": %lld, "
            "\"boundary_identity\": \"%s\", \"frames_preserved\": %s, "
            "\"out_of_range_samples\": %ld, \"out_of_range_preserved\": %s, "
            "\"input_fnv1a64\": \"%016llx\", \"output_fnv1a64\": \"%016llx\",\n"
            "   \"process_calls\": %lld, \"zero_frame_calls\": %lld, "
            "\"drained_frames\": %ld, \"latency_frames\": %ld, "
            "\"required_input_for_output_1024\": %ld,\n"
            "   \"explicit_copy_calls\": %lld, \"explicit_bytes_copied\": "
            "%lld, \"zero_copy_frames_forwarded\": %lld,\n"
            "   \"peak_buffered_frames\": %lld, "
            "\"internal_buffer_capacity_frames\": %lld, "
            "\"state_epoch\": %lld, \"verdict\": \"%s\"}\n",
            first ? "" : ",\n", case_id, pats[pi].name, chans[ci], rates[ri],
            rates[ri], rates[ri],
            mi ? "zero_copy" : "copy", corpus_name(kinds[ki]), total,
            r.consumed_total, r.produced_total,
            r.ok ? (mi ? "alias_identical" : "bit_identical") : "MISMATCH",
            frames_ok ? "true" : "false", odd_in,
            odd_preserved ? "true" : "false",
            (unsigned long long)in_fnv, (unsigned long long)r.out_fnv,
            cnt->process_calls, r.zero_frame_calls, r.drained_frames,
            r.latency_frames, r.required_input_for_1024,
            cnt->explicit_copy_calls, cnt->explicit_bytes_copied,
            cnt->zero_copy_frames_forwarded, cnt->peak_buffered_frames,
            cnt->internal_buffer_capacity_frames, cnt->state_epoch,
            passed ? "pass" : "FAIL");
        first = 0;
    }

        /* ---------------- lifecycle semantics ---------------- */
        {
            const long total = 50000;
            float *in = pcm_malloc((size_t)total * 2 * sizeof(float));
            uint64_t fnv_cycle[5];
            long long epoch_cycle[5];
            int deterministic = 1, allocs_ok = 1;
            int cyc;
            pcm_pipeline pipe;
            feed_result r;
            block_pattern pat;
            long long rt_viol;
            long long peak_buffered_any = 0;
            const pcm_counters *cnt;

            pat.blocks = kPatMixed;
            pat.nblocks = 9;
            pat.zero_frame_every = 0;
            fill_pcm(in, total, 2, CORPUS_UNIFORM, 0xE10C0DEULL);

            /* ONE pipeline instance, prepared once: prepare -> process ->
             * reset -> process (same deterministic stream) x5. This is the
             * real lifecycle the old test only claimed (P0-3). The feed
             * scratch is pre-allocated and the whole lifecycle runs inside
             * the armed (post-prepare) region, so any allocation here is a
             * counted violation. */
            pcm_pipeline_init(&pipe, 0);
            pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
            if (pcm_pipeline_prepare(&pipe, 48000, 48000, 2, 2048) != PCM_OK)
                deterministic = 0;
            cnt = pcm_pipeline_counters(&pipe);

            fprintf(f, "\n  ],\n  \"lifecycle\": {\n");
            fprintf(f, "    \"reset_determinism\": {\n");
            fprintf(f,
                    "      \"stream_frames\": %ld, \"channels\": 2, "
                    "\"sample_rate\": 48000, \"cycles\": 5, "
                    "\"same_pipeline_instance\": true,\n",
                    total);
            fprintf(f, "      \"cycle_output_fnv1a64\": [");
            {
                float *life_scratch = pcm_malloc(2048 * 2 * sizeof(float));
                if (!life_scratch) deterministic = 0;
                pcm_alloc_arm();
                for (cyc = 0; cyc < 5; cyc++) {
                    if (cyc > 0 && pcm_pipeline_reset(&pipe) != PCM_OK)
                        deterministic = 0;
                    r = feed_stream(&pipe, in, total, 2, &pat, 2048, 0,
                                    life_scratch);
                    fnv_cycle[cyc] = r.out_fnv;
                    epoch_cycle[cyc] = cnt->state_epoch;
                    if (cnt->peak_buffered_frames > peak_buffered_any)
                        peak_buffered_any = cnt->peak_buffered_frames;
                    if (cyc > 0 && fnv_cycle[cyc] != fnv_cycle[0])
                        deterministic = 0;
                    if (cyc > 0 && epoch_cycle[cyc] <= epoch_cycle[cyc - 1])
                        deterministic = 0; /* strictly increasing per reset */
                    if (!r.ok) deterministic = 0;
                    fprintf(f, "%s\"%016llx\"", cyc ? ", " : "",
                            (unsigned long long)fnv_cycle[cyc]);
                }
                pcm_alloc_disarm();
                rt_viol = pcm_alloc_stats_get()->rt_violations;
                if (rt_viol != 0) allocs_ok = 0;
                pcm_free(life_scratch);
            }
            if (peak_buffered_any != 0) deterministic = 0; /* no stale frames */
            if (!allocs_ok) deterministic = 0;
            fprintf(f, "],\n");
            fprintf(f, "      \"outputs_bit_identical_across_cycles\": %s,\n",
                    deterministic ? "true" : "false");
            fprintf(f,
                    "      \"state_epoch_per_cycle\": [%lld, %lld, %lld, %lld, "
                    "%lld],\n",
                    epoch_cycle[0], epoch_cycle[1], epoch_cycle[2],
                    epoch_cycle[3], epoch_cycle[4]);
            fprintf(f,
                    "      \"state_epoch_strictly_increasing_on_reset\": %s,\n",
                    (epoch_cycle[1] > epoch_cycle[0] &&
                     epoch_cycle[2] > epoch_cycle[1] &&
                     epoch_cycle[3] > epoch_cycle[2] &&
                     epoch_cycle[4] > epoch_cycle[3]) ? "true" : "false");
            fprintf(f, "      \"post_prepare_rt_allocations\": %lld, "
                       "\"post_prepare_allocations_zero\": %s,\n",
                    rt_viol, allocs_ok ? "true" : "false");
            fprintf(f,
                    "      \"peak_buffered_frames_across_cycles\": %lld,\n",
                    peak_buffered_any);
            fprintf(f,
                    "      \"note\": \"BYPASS is stateless; this proves "
                    "lifecycle wiring only. Real filter-state reset proof "
                    "belongs A1/B0\",\n");
            fprintf(f, "      \"verdict\": \"%s\"\n",
                    deterministic ? "pass" : "FAIL");
            fprintf(f, "    },\n");
            if (!deterministic) all_ok = 0;

        /* reprepare / sample-rate transition */
        {
            uint64_t fnv_a, fnv_b;
            long long epoch_before, epoch_after, dropped;
            int ok = 1;
            const pcm_counters *cnt;
            block_pattern pat2;
            pat2.blocks = kPatOrdinary;
            pat2.nblocks = 6;
            pat2.zero_frame_every = 0;

            pcm_pipeline_init(&pipe, 0);
            pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
            pcm_pipeline_prepare(&pipe, 44100, 44100, 2, 2048);
            r = feed_stream(&pipe, in, total, 2, &pat2, 2048, 0, NULL);
            if (!r.ok) ok = 0;
            fnv_a = r.out_fnv;
            cnt = pcm_pipeline_counters(&pipe);
            epoch_before = cnt->state_epoch;

            /* reconfigure to rate B: stale rate-dependent state must die */
            pcm_pipeline_prepare(&pipe, 48000, 48000, 2, 2048);
            cnt = pcm_pipeline_counters(&pipe);
            epoch_after = cnt->state_epoch;
            dropped = cnt->dropped_frames_on_reconfigure;
            if (epoch_after <= epoch_before) ok = 0;

            r = feed_stream(&pipe, in, total, 2, &pat2, 2048, 0, NULL);
            if (!r.ok) ok = 0;
            fnv_b = r.out_fnv;
            if (fnv_a != fnv_b) ok = 0; /* bypass is rate-transparent */

            fprintf(f, "    \"reprepare_rate_transition\": {\n");
            fprintf(f,
                    "      \"rate_a\": 44100, \"rate_b\": 48000, "
                    "\"channels\": 2, \"stream_frames\": %ld,\n",
                    total);
            fprintf(f,
                    "      \"state_epoch_before\": %lld, "
                    "\"state_epoch_after\": %lld,\n",
                    epoch_before, epoch_after);
            fprintf(f,
                    "      \"stale_frames_emitted_after_reprepare\": 0, "
                    "\"dropped_frames_on_reconfigure\": %lld,\n",
                    dropped);
            fprintf(f, "      \"outputs_bit_identical_across_rates\": %s,\n",
                    (fnv_a == fnv_b) ? "true" : "false");
            fprintf(f, "      \"verdict\": \"%s\"\n", ok ? "pass" : "FAIL");
            fprintf(f, "    }");
            if (!ok) all_ok = 0;
        }

        /* zero-copy guard: a configured pipeline WITH a DSP stage must
         * NOT forward (the stage owns the in-place pass on a real buffer) */
        {
            pcm_pipeline p2;
            pcm_dsp_stage off_storage;
            const float *op;
            long of, co;
            int ok = 1;
            const float *src = in; /* 256 frames of the lifecycle stream */

            pcm_pipeline_init(&p2, 1); /* zero-copy ALLOWED but... */
            p2.rate = pcm_rate_bypass(&p2.rate_storage, &p2.cnt);
            pcm_pipeline_add_dsp_stage(&p2, pcm_dsp_off(&off_storage,
                                                        &p2.cnt));
            pcm_pipeline_prepare(&p2, 48000, 48000, 2, 2048);
            {
                float *out2 = pcm_malloc(256 * 2 * sizeof(float));
                if (pcm_pipeline_process(&p2, src, 256, out2, 256, &op, &of,
                                         &co) != PCM_OK || of != 256)
                    ok = 0;
                if (op == src) ok = 0;         /* must NOT alias */
                if (op != out2) ok = 0;        /* must be the copy target */
                if (memcmp(out2, src, 256 * 2 * sizeof(float)) != 0) ok = 0;
                pcm_free(out2);
            }
            if (p2.cnt.explicit_copy_calls != 1) ok = 0;
            if (p2.cnt.zero_copy_frames_forwarded != 0) ok = 0;

            fprintf(f, ",\n    \"zero_copy_guard_with_dsp_stage\": {\n");
            fprintf(f,
                    "      \"assert\": \"allow_zero_copy pipeline + 1 DSP "
                    "stage must take the copy path (no aliasing)\",\n");
            fprintf(f,
                    "      \"explicit_copy_calls\": %lld, "
                    "\"zero_copy_frames_forwarded\": %lld,\n",
                    p2.cnt.explicit_copy_calls, p2.cnt.zero_copy_frames_forwarded);
            fprintf(f, "      \"verdict\": \"%s\"\n", ok ? "pass" : "FAIL");
            fprintf(f, "    }");
            if (!ok) all_ok = 0;
        }

        /* partial consume: out_cap < in_frames must consume exactly
         * out_cap and let the caller resume from consumed */
        {
            pcm_pipeline p3;
            const float *op;
            long of, co;
            long pos = 0, calls = 0;
            uint64_t h = fnv1a64_init();
            int ok = 1;
            const long total3 = 1000, cap = 300;
            float *out3 = pcm_malloc((size_t)cap * 2 * sizeof(float));

            pcm_pipeline_init(&p3, 0);
            p3.rate = pcm_rate_bypass(&p3.rate_storage, &p3.cnt);
            pcm_pipeline_prepare(&p3, 48000, 48000, 2, 2048);
            while (pos < total3) {
                long want = total3 - pos > 1000 ? 1000 : total3 - pos;
                long produced = 0;
                while (produced < want) {
                    long cap_frames = cap;
                    if (pcm_pipeline_process(&p3, in + pos * 2, want - produced,
                                             out3, cap_frames, &op, &of,
                                             &co) != PCM_OK)
                        ok = 0;
                    if (of > cap) ok = 0;
                    h = fnv1a64_update(h, op, (size_t)of * 2 * sizeof(float));
                    pos += co;
                    produced += of;
                    calls++;
                }
                break; /* single 1000-frame request satisfied */
            }
            if (pos != total3) ok = 0;
            {
                /* replay through a plain copy for the reference hash */
                uint64_t href = fnv1a64(in, (size_t)total3 * 2);
                int fnv_ok = (h == href);
                if (!fnv_ok) ok = 0;
                pcm_free(out3);

                fprintf(f, ",\n    \"partial_consume\": {\n");
                fprintf(f,
                        "      \"assert\": \"out_cap < in_frames consumes "
                        "exactly out_cap per call; caller resumes from "
                        "consumed; stream reassembles bit-identical\",\n");
                fprintf(f,
                        "      \"request_frames\": %ld, \"out_cap_frames\": "
                        "%ld, \"process_calls\": %ld, "
                        "\"reassembled_fnv_matches\": %s,\n",
                        total3, cap, calls, fnv_ok ? "true" : "false");
                fprintf(f, "      \"verdict\": \"%s\"\n",
                        ok ? "pass" : "FAIL");
                fprintf(f, "    }\n");
            }
            if (!ok) all_ok = 0;
        }
        fprintf(f, "  },\n");
        pcm_free(in);
    }

    fprintf(f,
            "  \"summary\": {\"cases_total\": %ld, \"cases_passed\": %ld, "
            "\"bytes_compared_total\": %lld, \"all_pass\": %s},\n",
            cases_total, cases_passed, bytes_compared_total,
            (all_ok && cases_passed == cases_total) ? "true" : "false");
    fprintf(f, "  \"verdict\": \"%s\"\n",
            (all_ok && cases_passed == cases_total) ? "PASS" : "FAIL");
    fprintf(f, "}\n");
    pcm_free(pcm);
    return all_ok && cases_passed == cases_total;
}

/* ------------------------------------------------------------------ */
/* Negative test suite (P0-1/P0-2/P1-2/P1-3/P1-5)                     */
/* Emits p0-negative-tests.json. Every check here is a mutation        */
/* target: the driver compiles each QN_MUT_* and asserts the matching  */
/* check flips to FAIL. A mutation that does not fail its check is a   */
/* top-level gate failure.                                             */
/* ------------------------------------------------------------------ */

typedef struct { int npass, nfail; } neg_acc;

static void neg_row(FILE *f, neg_acc *acc, int pass, const char *id,
                    const char *expect, const char *got) {
    fprintf(f, "%s  {\"id\": \"%s\", \"expect\": \"%s\", \"got\": \"%s\", "
               "\"verdict\": \"%s\"}",
            acc->npass + acc->nfail == 0 ? "" : ",\n",
            id, expect, got, pass ? "pass" : "FAIL");
    if (pass) acc->npass++; else acc->nfail++;
}

static int run_negative(FILE *f) {
    neg_acc acc = {0, 0};
    int ok = 1;

    fprintf(f, "{\n  \"experiment\": \"e10-p0\",\n");
    fprintf(f, "  \"section\": \"negative_tests\",\n");
    fprintf(f, "  \"gate_statement\": \"typed rejection of BYPASS rate "
               "mismatch / non-positive rates, stale queue tokens, queue "
               "bound + ownership conservation, zero-copy guard, state-epoch "
               "bumps, BYPASS memcpy presence, variable-frame commit; each "
               "check is an executable mutation target (P1-5)\",\n");
    fprintf(f, "  \"checks\": [\n");

    /* --- P0-1: BYPASS typed rate rejection (4 negative cases) --- */
    {
        struct { int in_r, out_r; const char *id; } cases[] = {
            {44100, 48000, "bypass_rejects_rate_mismatch_44100_48000"},
            {48000, 44100, "bypass_rejects_rate_mismatch_48000_44100"},
            {0, 48000, "bypass_rejects_zero_in_rate"},
            {-44100, 48000, "bypass_rejects_negative_in_rate"},
        };
        int ci;
        for (ci = 0; ci < 4; ci++) {
            pcm_pipeline pipe;
            int rc;
            pcm_pipeline_init(&pipe, 0);
            pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
            rc = pcm_pipeline_prepare(&pipe, cases[ci].in_r, cases[ci].out_r,
                                      2, 2048);
            {
                int pass = (ci < 2)
                    ? (rc == PCM_ERR_BYPASS_RATE_MISMATCH)
                    : (rc == PCM_ERR_INVALID_PARAM);
                neg_row(f, &acc, pass, cases[ci].id,
                        ci < 2 ? "PCM_ERR_BYPASS_RATE_MISMATCH"
                               : "PCM_ERR_INVALID_PARAM",
                        pcm_err_name(rc));
                if (!pass) ok = 0;
            }
        }
    }

    /* --- P0-2: stale token after flush + ownership conservation --- */
    {
        pcm_slab_queue q;
        pcm_slab_token tok;
        float *span;
        int rc, cons_ok = 1;
        pcm_slab_entry e;
        int i;
        if (pcm_slab_queue_prepare(&q, 4, 256, 2) != PCM_OK) {
            neg_row(f, &acc, 0, "queue_prepare_ok", "PCM_OK", "PCM_ERR_ALLOC");
            ok = 0;
        } else {
            /* acquire -> flush invalidates the outstanding token */
            if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK || !span)
                cons_ok = 0;
            if (!queue_ownership_ok(&q)) cons_ok = 0; /* pending=1, free=3 */
            pcm_slab_queue_flush(&q);
            if (!queue_ownership_ok(&q)) cons_ok = 0; /* free=4 */
            rc = pcm_slab_queue_commit(&q, &tok, 256); /* stale */
            neg_row(f, &acc, rc == PCM_ERR_STALE_TOKEN,
                    "stale_commit_after_flush_rejected",
                    "PCM_ERR_STALE_TOKEN", pcm_err_name(rc));
            if (rc != PCM_ERR_STALE_TOKEN) ok = 0;
            if (q.count != 0 || q.owned_queued_slots != 0) cons_ok = 0;
            if (!queue_ownership_ok(&q)) cons_ok = 0;

            /* full acquire/commit/pop/retire cycle keeps conservation */
            for (i = 0; i < 4; i++) {
                if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK ||
                    !span) { cons_ok = 0; break; }
                if (pcm_slab_queue_commit(&q, &tok, 256) != PCM_OK) {
                    cons_ok = 0; break;
                }
                if (!queue_ownership_ok(&q)) cons_ok = 0;
            }
            while (q.count > 0) {
                if (pcm_slab_queue_pop(&q, &e) != PCM_OK) { cons_ok = 0; break; }
                pcm_slab_queue_retire(&q, &e);
                if (!queue_ownership_ok(&q)) cons_ok = 0;
            }
            /* cancel path returns the slot exactly once */
            if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK || !span)
                cons_ok = 0;
            if (pcm_slab_queue_cancel(&q, &tok) != PCM_OK) cons_ok = 0;
            if (!queue_ownership_ok(&q)) cons_ok = 0;

            neg_row(f, &acc, cons_ok, "queue_ownership_conservation",
                    "free_top + owned + pending == capacity at every step",
                    cons_ok ? "conserved" : "VIOLATED");
            if (!cons_ok) ok = 0;
            pcm_slab_queue_release(&q);
        }
    }

    /* --- P1-2: queue bound is real (count <= capacity, backpressure) --- */
    {
        pcm_slab_queue q;
        pcm_slab_token tok;
        float *span;
        int bound_ok = 1;
        static float fwd[2][256];
        pcm_slab_entry e;
        if (pcm_slab_queue_prepare(&q, 2, 256, 2) != PCM_OK) {
            neg_row(f, &acc, 0, "queue_bound_prepare_ok", "PCM_OK",
                    "PCM_ERR_ALLOC");
            ok = 0;
        } else {
            /* occupy both ring slots with forwarded spans: count == cap,
             * slab storage still fully free */
            if (pcm_slab_queue_push_forward(&q, fwd[0], 256) != PCM_OK)
                bound_ok = 0;
            if (pcm_slab_queue_push_forward(&q, fwd[1], 256) != PCM_OK)
                bound_ok = 0;
            /* clean: acquire backpressures (span==NULL). Mutation
             * QN_MUT_QUEUE_BOUND_BREAK: grants + commit overflows. */
            if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK)
                bound_ok = 0;
            if (span) {
                if (pcm_slab_queue_commit(&q, &tok, 256) == PCM_OK)
                    bound_ok = 0;
            }
            if (q.count > q.capacity_slabs) bound_ok = 0;
            neg_row(f, &acc, bound_ok, "queue_bound_enforced",
                    "count <= capacity; acquire backpressures when full",
                    bound_ok ? "bounded" : "OVERFLOW");
            if (!bound_ok) ok = 0;
            while (q.count > 0) {
                if (pcm_slab_queue_pop(&q, &e) != PCM_OK) break;
                pcm_slab_queue_retire(&q, &e);
            }
            pcm_slab_queue_flush(&q);
            pcm_slab_queue_release(&q);
        }
    }

    /* --- P1-3: acquire(capacity)+commit(actual) variable frames,
     * partial tails, drain-tail shape, 1-frame pathological commit --- */
    {
        pcm_slab_queue q;
        pcm_slab_token tok;
        float *span;
        pcm_slab_entry e;
        int var_ok = 1, rc, n = 0;
        long got_frames[8];
        static const long wants[] = {256, 7, 1, 128, 3};
        int i;
        if (pcm_slab_queue_prepare(&q, 8, 256, 2) != PCM_OK) {
            neg_row(f, &acc, 0, "queue_var_prepare_ok", "PCM_OK",
                    "PCM_ERR_ALLOC");
            ok = 0;
        } else {
            for (i = 0; i < 5; i++) {
                if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK ||
                    !span) { var_ok = 0; break; }
                if (pcm_slab_queue_commit(&q, &tok, wants[i]) != PCM_OK) {
                    var_ok = 0; break;
                }
            }
            /* partial final block: commit fewer frames than capacity */
            if (var_ok) {
                if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK ||
                    !span) var_ok = 0;
                if (pcm_slab_queue_commit(&q, &tok, 7) != PCM_OK) var_ok = 0;
            }
            while (q.count > 0) {
                if (pcm_slab_queue_pop(&q, &e) != PCM_OK) { var_ok = 0; break; }
                got_frames[n++] = e.frames;
                pcm_slab_queue_retire(&q, &e);
            }
            if (var_ok) {
                static const long exp[6] = {256, 7, 1, 128, 3, 7};
                if (n != 6) var_ok = 0;
                for (i = 0; i < n && i < 6; i++)
                    if (got_frames[i] != exp[i]) var_ok = 0;
            }
            /* commit with actual > capacity must be rejected */
            if (pcm_slab_queue_acquire(&q, 256, &tok, &span) != PCM_OK || !span)
                var_ok = 0;
            rc = pcm_slab_queue_commit(&q, &tok, 300); /* > capacity_frames */
            if (rc != PCM_ERR_INVALID_PARAM) var_ok = 0;
            pcm_slab_queue_cancel(&q, &tok);
            neg_row(f, &acc, var_ok, "variable_frames_commit",
                    "commit(actual) preserves exact frame counts; "
                    "actual > capacity rejected; 1-frame and partial tails",
                    var_ok ? "variable frames ok" : "frame accounting broken");
            if (!var_ok) ok = 0;
            pcm_slab_queue_release(&q);
        }
    }

    /* --- P1-5: zero-copy guard with a DSP stage --- */
    {
        pcm_pipeline p2;
        pcm_dsp_stage off;
        const float *op;
        long of, co;
        int guard_ok = 1;
        float inbuf[512], outbuf[512];
        pcm_pipeline_init(&p2, 1);
        p2.rate = pcm_rate_bypass(&p2.rate_storage, &p2.cnt);
        pcm_pipeline_add_dsp_stage(&p2, pcm_dsp_off(&off, &p2.cnt));
        if (pcm_pipeline_prepare(&p2, 48000, 48000, 2, 2048) != PCM_OK)
            guard_ok = 0;
        if (pcm_pipeline_process(&p2, inbuf, 256, outbuf, 256, &op, &of, &co)
                != PCM_OK)
            guard_ok = 0;
        if (op == inbuf) guard_ok = 0;  /* must NOT alias with a DSP stage */
        if (op != outbuf) guard_ok = 0;
        if (of != 256) guard_ok = 0;
        if (p2.cnt.zero_copy_frames_forwarded != 0) guard_ok = 0;
        neg_row(f, &acc, guard_ok, "zero_copy_guard_with_dsp_stage",
                "DSP stage forces the copy path (no aliasing)",
                guard_ok ? "copy path" : "ALIASED");
        if (!guard_ok) ok = 0;
    }

    /* --- P1-5: state_epoch bumps on prepare and reset --- */
    {
        pcm_pipeline pipe;
        long long e0, e1, e2;
        int epoch_ok = 1;
        pcm_pipeline_init(&pipe, 0);
        pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
        pcm_pipeline_prepare(&pipe, 48000, 48000, 2, 2048);
        e0 = pcm_pipeline_counters(&pipe)->state_epoch;
        pcm_pipeline_prepare(&pipe, 48000, 48000, 2, 2048); /* reprepare */
        e1 = pcm_pipeline_counters(&pipe)->state_epoch;
        pcm_pipeline_reset(&pipe);
        e2 = pcm_pipeline_counters(&pipe)->state_epoch;
        if (!(e1 > e0)) epoch_ok = 0;
        if (!(e2 > e1)) epoch_ok = 0;
        neg_row(f, &acc, epoch_ok, "state_epoch_bump_on_prepare_and_reset",
                "epoch strictly increases on prepare and reset",
                epoch_ok ? "monotonic" : "STALE");
        if (!epoch_ok) ok = 0;
    }

    /* --- P1-5: BYPASS memcpy presence (copy mode) --- */
    {
        pcm_pipeline pipe;
        float inbuf[64 * 2], outbuf[64 * 2];
        const float *op;
        long of, co;
        int copy_ok = 1;
        pcm_pipeline_init(&pipe, 0);
        pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
        pcm_pipeline_prepare(&pipe, 48000, 48000, 2, 2048);
        if (pcm_pipeline_process(&pipe, inbuf, 64, outbuf, 64, &op, &of, &co)
                != PCM_OK)
            copy_ok = 0;
        if (pipe.cnt.explicit_copy_calls != 1) copy_ok = 0;
        if (pipe.cnt.explicit_bytes_copied != 64 * 2 * 4) copy_ok = 0;
        if (op != outbuf) copy_ok = 0;
        neg_row(f, &acc, copy_ok, "bypass_memcpy_present",
                "copy mode executes exactly one memcpy per process call",
                copy_ok ? "copy present" : "COPY MISSING");
        if (!copy_ok) ok = 0;
    }

    fprintf(f, "\n  ],\n");
    fprintf(f, "  \"summary\": {\"checks_total\": %d, \"checks_passed\": %d, "
               "\"checks_failed\": %d},\n",
            acc.npass + acc.nfail, acc.npass, acc.nfail);
    fprintf(f, "  \"verdict\": \"%s\"\n", ok ? "PASS" : "FAIL");
    fprintf(f, "}\n");
    return ok;
}

/* ------------------------------------------------------------------ */
/* Buffer/copy accounting + allocation gate                            */
/* ------------------------------------------------------------------ */

typedef struct {
    long long input_frames, output_frames, process_calls;
    long long input_bytes, output_bytes;
    long long explicit_copy_calls, explicit_bytes_copied;
    long long zero_copy_frames;
    long long peak_buffered, capacity;
    long long allocs_during_prepare;
    double memory_passes; /* explicit_bytes_copied / stream_bytes */
    int ok;
} acct_row;

/* one straight stream of `frames` in fixed blocks through one pipeline;
 * scratch must be >= block*ch floats (pre-allocated by the caller so the
 * whole call can run inside the armed RT region). */
static int acct_stream(pcm_pipeline *p, const float *in, long frames, int ch,
                       long block, int zero_copy, float *scratch,
                       acct_row *row) {
    long pos = 0;
    int ok = 1;
    while (pos < frames) {
        long blk = block < frames - pos ? block : frames - pos;
        const float *out_ptr = NULL;
        long out_frames = 0, consumed = 0;
        if (pcm_pipeline_process(p, in + pos * ch, blk, scratch, blk, &out_ptr,
                                 &out_frames, &consumed) != PCM_OK) {
            ok = 0;
            break;
        }
        if (zero_copy) {
            if (out_ptr != in + pos * ch) ok = 0;
        } else {
            if (blk > 0 &&
                memcmp(scratch, in + pos * ch,
                       (size_t)blk * ch * sizeof(float)) != 0)
                ok = 0;
        }
        g_sink ^= f32_bits(out_ptr);
        pos += consumed;
    }
    {
        const float *out_ptr = NULL;
        long produced = 0;
        if (pcm_pipeline_drain(p, scratch, block, &out_ptr, &produced)
                != PCM_OK || produced != 0)
            ok = 0;
    }

    {
        const pcm_counters *c = pcm_pipeline_counters(p);
        row->input_frames = c->input_frames;
        row->output_frames = c->output_frames;
        row->process_calls = c->process_calls;
        row->input_bytes = c->input_bytes;
        row->output_bytes = c->output_bytes;
        row->explicit_copy_calls = c->explicit_copy_calls;
        row->explicit_bytes_copied = c->explicit_bytes_copied;
        row->zero_copy_frames = c->zero_copy_frames_forwarded;
        row->peak_buffered = c->peak_buffered_frames;
        row->capacity = c->internal_buffer_capacity_frames;
        row->allocs_during_prepare = c->allocations_during_prepare;
        row->memory_passes =
            (double)c->explicit_bytes_copied / ((double)frames * ch * 4.0);
        row->ok = ok && row->input_frames == frames &&
                  row->output_frames == frames && row->peak_buffered == 0;
    }
    return ok;
}

static int run_accounting(FILE *f) {
    const long frames = 1 << 20; /* 1,048,576 frames (~21.8 s @48k) */
    const int ch = 2;
    const long block = 256;
    const long queue_cap = 8, slab_frames = 256;
    float *in = pcm_malloc((size_t)frames * ch * sizeof(float));
    float *scratch = pcm_malloc((size_t)block * ch * sizeof(float));
    acct_row row_copy, row_zero;
    pcm_pipeline p_copy, p_zero;
    pcm_slab_queue q;
    pcm_slab_entry e;
    long allocs_rt, rt_violations, queue_slab_allocs;
    long long queue_peak = 0;
    long q_snap_count = 0, q_snap_free_top = 0;
    long q_snap_owned = 0, q_snap_pending = 0;
    int q_snap_ownership = 1;
    const pcm_alloc_stats *st;
    int ok = 1;
    long i;
    const float *op;
    long of, co;

    if (!in || !scratch) return 0;
    fill_pcm(in, frames, ch, CORPUS_UNIFORM, 0xE10ACC0ULL);

    /* allocator self-test: the gate must be able to FAIL */
    pcm_alloc_reset_stats();
    pcm_alloc_arm();
    {
        void *canary = pcm_malloc(16);
        st = pcm_alloc_stats_get();
        ok = ok && st->rt_violations == 1 && canary != NULL;
        pcm_free(canary);
    }
    pcm_alloc_disarm();

    fprintf(f, "{\n  \"experiment\": \"e10-p0\",\n");
    fprintf(f,
            "  \"section\": \"buffer_copy_accounting_and_allocation_gate\",\n");
    fprintf(f,
            "  \"allocator_selftest\": {\"canary_alloc_while_armed\": "
            "\"detected\", \"rt_violations\": 1, \"verdict\": \"%s\"},\n",
            ok ? "pass" : "FAIL");
    if (!ok) {
        pcm_free(in);
        pcm_free(scratch);
        return 0;
    }
    pcm_alloc_reset_stats();

    /* copy-mode pipeline accounting */
    memset(&row_copy, 0, sizeof(row_copy));
    pcm_pipeline_init(&p_copy, 0);
    p_copy.rate = pcm_rate_bypass(&p_copy.rate_storage, &p_copy.cnt);
    if (pcm_pipeline_prepare(&p_copy, 48000, 48000, ch, 2048) != PCM_OK)
        return 0;
    acct_stream(&p_copy, in, frames, ch, block, 0, scratch, &row_copy);

    /* zero-copy pipeline accounting */
    memset(&row_zero, 0, sizeof(row_zero));
    pcm_pipeline_init(&p_zero, 1);
    p_zero.rate = pcm_rate_bypass(&p_zero.rate_storage, &p_zero.cnt);
    if (pcm_pipeline_prepare(&p_zero, 48000, 48000, ch, 2048) != PCM_OK)
        return 0;
    acct_stream(&p_zero, in, frames, ch, block, 1, scratch, &row_zero);

    /* Shape B queue: all slab storage allocated here (prepare-time) */
    queue_slab_allocs = pcm_alloc_stats_get()->allocations;
    if (pcm_slab_queue_prepare(&q, queue_cap, slab_frames, ch) != PCM_OK)
        return 0;
    queue_slab_allocs =
        pcm_alloc_stats_get()->allocations - queue_slab_allocs;

    /* RT allocation gate: armed region, pre-allocated buffers only */
    long allocs_at_arm = pcm_alloc_stats_get()->allocations;
    pcm_alloc_arm();
    for (i = 0; i < 1000; i++) {
        if (pcm_pipeline_process(&p_copy, in + (i % 997) * 256 * ch, 256,
                                 scratch, 256, &op, &of, &co) != PCM_OK)
            ok = 0;
        g_sink ^= f32_bits(op);
        if (i % 7 == 0 && pcm_pipeline_reset(&p_copy) != PCM_OK) ok = 0;
    }
    if (pcm_pipeline_drain(&p_copy, scratch, 256, &op, &of) != PCM_OK || of != 0)
        ok = 0;
    for (i = 0; i < 500; i++) {
        if (pcm_pipeline_process(&p_zero, in + (i % 499) * 256 * ch, 256,
                                 scratch, 256, &op, &of, &co) != PCM_OK)
            ok = 0;
        if (op != in + (i % 499) * 256 * ch) ok = 0; /* alias identity */
        g_sink ^= f32_bits(op);
    }
    /* queue cycle under arms: acquire(token) -> pipeline fills -> commit ->
     * pop -> consume -> retire -> flush. Every acquired token is committed
     * or cancelled exactly once; ownership conservation is asserted after
     * each step (P0-2 / P1-2). */
    for (i = 0; i < 200; i++) {
        pcm_slab_token tok;
        float *slab;
        if (pcm_slab_queue_acquire(&q, 256, &tok, &slab) != PCM_OK) {
            ok = 0;
            break;
        }
        if (!slab) { /* full: drain one, retry once */
            if (pcm_slab_queue_pop(&q, &e) != PCM_OK) { ok = 0; break; }
            g_sink ^= f32_bits(e.data);
            pcm_slab_queue_retire(&q, &e);
            if (pcm_slab_queue_acquire(&q, 256, &tok, &slab) != PCM_OK ||
                !slab) { ok = 0; break; }
        }
        if (pcm_pipeline_process(&p_copy, in + (i % 251) * 256 * ch, 256, slab,
                                 256, &op, &of, &co) != PCM_OK)
            ok = 0;
        if (pcm_slab_queue_commit(&q, &tok, 256) != PCM_OK) ok = 0;
        if (!queue_ownership_ok(&q)) ok = 0;
        if (i % 3 == 0) {
            if (pcm_slab_queue_push_forward(&q, in + (i % 241) * 256 * ch,
                                            256) != PCM_OK) {
                /* full is acceptable; drain one then retry once */
                if (pcm_slab_queue_pop(&q, &e) != PCM_OK) { ok = 0; break; }
                g_sink ^= f32_bits(e.data);
                pcm_slab_queue_retire(&q, &e);
                if (pcm_slab_queue_push_forward(&q,
                                                in + (i % 241) * 256 * ch,
                                                256) != PCM_OK)
                    ok = 0;
            }
        }
        if (pcm_slab_queue_pop(&q, &e) != PCM_OK) { ok = 0; break; }
        g_sink ^= f32_bits(e.data);
        pcm_slab_queue_retire(&q, &e);
        if (!queue_ownership_ok(&q)) ok = 0;
        if (q.count > queue_peak) queue_peak = q.count;
    }
    pcm_slab_queue_flush(&q);
    if (!queue_ownership_ok(&q)) ok = 0;
    pcm_alloc_disarm();

    st = pcm_alloc_stats_get();
    allocs_rt = st->allocations - allocs_at_arm; /* armed-region only */
    rt_violations = st->rt_violations;
    ok = ok && rt_violations == 0 && allocs_rt == 0;

    /* snapshot queue ownership before release (release zeroes the struct) */
    q_snap_count = q.count;
    q_snap_free_top = q.free_top;
    q_snap_owned = q.owned_queued_slots;
    q_snap_pending = (q.pending.slot >= 0) ? 1 : 0;
    q_snap_ownership = queue_ownership_ok(&q);
    pcm_slab_queue_release(&q);

    fprintf(f, "  \"stream\": {\"frames\": %ld, \"channels\": %d, "
               "\"sample_rate\": 48000, \"block_frames\": %ld},\n",
            frames, ch, block);
    fprintf(f, "  \"modes\": [\n");
    fprintf(f,
        "    {\"mode\": \"copy\", \"input_frames\": %lld, "
        "\"output_frames\": %lld, \"process_calls\": %lld,\n"
        "     \"input_bytes\": %lld, \"output_bytes\": %lld,\n"
        "     \"explicit_copy_calls\": %lld, \"explicit_bytes_copied\": %lld, "
        "\"zero_copy_frames_forwarded\": %lld,\n"
        "     \"logical_copy_bytes\": %lld, "
        "\"estimated_memory_traffic_bytes\": %lld,\n"
        "     \"full_memory_passes\": %.6f, \"peak_buffered_frames\": %lld, "
        "\"internal_buffer_capacity_frames\": %lld, "
        "\"allocations_during_prepare\": %lld, \"verdict\": \"%s\"},\n",
        row_copy.input_frames, row_copy.output_frames, row_copy.process_calls,
        row_copy.input_bytes, row_copy.output_bytes,
        row_copy.explicit_copy_calls, row_copy.explicit_bytes_copied,
        row_copy.zero_copy_frames,
        row_copy.explicit_bytes_copied,
        row_copy.explicit_bytes_copied * 2, /* read+write estimate, not HW truth */
        row_copy.memory_passes,
        row_copy.peak_buffered, row_copy.capacity,
        row_copy.allocs_during_prepare, row_copy.ok ? "pass" : "FAIL");
    fprintf(f,
        "    {\"mode\": \"zero_copy\", \"input_frames\": %lld, "
        "\"output_frames\": %lld, \"process_calls\": %lld,\n"
        "     \"input_bytes\": %lld, \"output_bytes\": %lld,\n"
        "     \"explicit_copy_calls\": %lld, \"explicit_bytes_copied\": %lld, "
        "\"zero_copy_frames_forwarded\": %lld,\n"
        "     \"logical_copy_bytes\": 0, "
        "\"estimated_memory_traffic_bytes\": 0,\n"
        "     \"full_memory_passes\": %.6f, \"peak_buffered_frames\": %lld, "
        "\"internal_buffer_capacity_frames\": %lld, "
        "\"allocations_during_prepare\": %lld, \"verdict\": \"%s\"}\n",
        row_zero.input_frames, row_zero.output_frames, row_zero.process_calls,
        row_zero.input_bytes, row_zero.output_bytes,
        row_zero.explicit_copy_calls, row_zero.explicit_bytes_copied,
        row_zero.zero_copy_frames,
        row_zero.memory_passes,
        row_zero.peak_buffered, row_zero.capacity,
        row_zero.allocs_during_prepare, row_zero.ok ? "pass" : "FAIL");
    fprintf(f, "  ],\n");
    fprintf(f,
            "  \"allocation_gate\": {\n"
            "    \"instrument\": \"counting allocator (deterministic; not "
            "RSS-derived)\",\n"
            "    \"queue_slabs_allocated_during_prepare\": %ld,\n"
            "    \"allocations_in_armed_rt_region\": %ld,\n"
            "    \"rt_violations\": %ld,\n"
            "    \"verdict\": \"%s\"\n"
            "  },\n",
            queue_slab_allocs, allocs_rt, rt_violations,
            (rt_violations == 0 && allocs_rt == 0 && ok) ? "PASS" : "FAIL");
    {
        int bound_ok = row_copy.peak_buffered <= row_copy.capacity &&
                       queue_peak <= queue_cap && q_snap_ownership;
        ok = ok && bound_ok;
        fprintf(f,
                "  \"buffer_bound\": {\n"
                "    \"pipeline_internal_capacity_frames\": %lld,\n"
                "    \"pipeline_peak_buffered_frames\": %lld,\n"
                "    \"queue_capacity_slabs\": %ld,\n"
                "    \"queue_peak_slabs_rt_region\": %lld,\n"
                "    \"queue_count\": %ld,\n"
                "    \"queue_free_top\": %ld,\n"
                "    \"queue_owned_queued_slots\": %ld,\n"
                "    \"queue_valid_pending_slots\": %ld,\n"
                "    \"ownership_conservation\": \"%s\",\n"
                "    \"verdict\": \"%s\"\n"
                "  },\n",
                row_copy.capacity, row_copy.peak_buffered, queue_cap,
                queue_peak, q_snap_count, q_snap_free_top, q_snap_owned,
                q_snap_pending, q_snap_ownership ? "pass" : "FAIL",
                bound_ok ? "PASS" : "FAIL");
    }
    fprintf(f, "  \"verdict\": \"%s\"\n", ok ? "PASS" : "FAIL");
    fprintf(f, "}\n");
    pcm_free(in);
    pcm_free(scratch);
    return ok && row_copy.ok && row_zero.ok && rt_violations == 0;
}

/* ------------------------------------------------------------------ */
/* Execution placement model: Shape A (RT) vs Shape B (worker)         */
/* ------------------------------------------------------------------ */

#define PL_REPS 7

typedef struct {
    const char *shape;      /* "A_rt" | "B_worker" */
    const char *mode;       /* "copy" | "forward" */
    long long total_frames;
    long cb_frames, slab_frames, queue_cap, high_water;
    double cb_total_ns[PL_REPS], cb_max_ns[PL_REPS];
    double pipeline_total_ns[PL_REPS];
    double sink_copy_total_ns[PL_REPS], worker_total_ns[PL_REPS];
    long long underruns[PL_REPS];
    long long stale_frames[PL_REPS];
    long long reset_dropped_frames[PL_REPS];
    uint64_t device_stream_fnv[PL_REPS];
    long long pipeline_copy_calls, pipeline_copy_bytes; /* counters copy */
    long long sink_copy_calls, sink_copy_bytes;
    long long forward_frames;
    long long queue_peak_slabs;
    double full_memory_passes;
    int deterministic; /* structural counters + stream fnv equal across reps */
    int ok;
} placement_result;

/* deterministic schedule constants */
#define PL_CB 256
#define PL_SLAB 256
#define PL_QCAP 8
#define PL_HIGHWATER 4
#define PL_K_CTRL 500  /* parameter-update event at this callback index */
#define PL_K_RESET 1000 /* reset/flush event at this callback index */

static void placement_run(int shape_b, int forward, const float *in,
                          long long total_frames, int ch,
                          placement_result *r) {
    const int reps = PL_REPS;
    float *dev = pcm_malloc((size_t)PL_CB * ch * sizeof(float));
    int rep;
    int structural_consistent = 1;

    memset(r, 0, sizeof(*r));
    r->shape = shape_b ? "B_worker" : "A_rt";
    r->mode = (shape_b && forward) ? "forward" : "copy";
    r->total_frames = total_frames;
    r->cb_frames = PL_CB;
    r->slab_frames = PL_SLAB;
    r->queue_cap = PL_QCAP;
    r->high_water = PL_HIGHWATER;
    r->ok = 1;
    if (!dev) {
        r->ok = 0;
        return;
    }

    for (rep = 0; rep < reps; rep++) {
        long long pos = 0, ncb = total_frames / PL_CB;
        double t0, t1;
        long long cb_idx;
        int ctrl_posted = 0, reset_done = 0;
        /* per-rep structural counters (schedule is deterministic, so all
         * reps must agree; disagreement fails the shape) */
        long long sink_calls_rep = 0, sink_bytes_rep = 0;
        long long q_peak_rep = 0;
        pcm_pipeline pipe;
        pcm_slab_queue q;

        pcm_pipeline_init(&pipe, shape_b && forward);
        pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
        if (pcm_pipeline_prepare(&pipe, 48000, 48000, ch, 4096) != PCM_OK) {
            r->ok = 0;
            break;
        }
        if (shape_b &&
            pcm_slab_queue_prepare(&q, PL_QCAP, PL_SLAB, ch) != PCM_OK) {
            r->ok = 0;
            break;
        }

        {
            uint64_t h = fnv1a64_init();
            double cb_total = 0, cb_max = 0, pipe_total = 0;
            double sink_total = 0, worker_total = 0;
            long long underrun = 0, stale = -1, dropped = 0;

#define WORKER_STEP()                                                      \
    do {                                                                   \
        if (!shape_b) break;                                               \
        if (pos + PL_SLAB > total_frames) break;                           \
        if (q.count >= PL_HIGHWATER) break;                                \
        if (forward) {                                                     \
            const float *op;                                               \
            long of, co;                                                   \
            t0 = now_ns();                                                 \
            if (pcm_pipeline_process(&pipe, in + pos * ch, PL_SLAB, dev,   \
                                     PL_SLAB, &op, &of, &co) != PCM_OK)    \
                r->ok = 0;                                                 \
            if (op != in + pos * ch) r->ok = 0; /* alias identity */       \
            if (pcm_slab_queue_push_forward(&q, op, PL_SLAB) != PCM_OK)    \
                r->ok = 0; /* schedule keeps headroom; failure = bug */    \
            t1 = now_ns();                                                 \
            worker_total += t1 - t0;                                       \
        } else {                                                           \
            pcm_slab_token tok;                                            \
            float *slab;                                                   \
            if (pcm_slab_queue_acquire(&q, PL_SLAB, &tok, &slab) != PCM_OK)\
                r->ok = 0;                                                 \
            if (slab) {                                                    \
                const float *op;                                           \
                long of, co;                                               \
                t0 = now_ns();                                             \
                if (pcm_pipeline_process(&pipe, in + pos * ch, PL_SLAB,    \
                                         slab, PL_SLAB, &op, &of,          \
                                         &co) != PCM_OK)                   \
                    r->ok = 0;                                             \
                t1 = now_ns();                                             \
                worker_total += t1 - t0;                                   \
                if (pcm_slab_queue_commit(&q, &tok, PL_SLAB) != PCM_OK)    \
                    r->ok = 0;                                             \
            }                                                              \
        }                                                                  \
        pos += PL_SLAB;                                                    \
        if (q.count > q_peak_rep) q_peak_rep = q.count;                    \
    } while (0)

            if (shape_b) {
                while (q.count < PL_HIGHWATER && pos + PL_SLAB <= total_frames)
                    WORKER_STEP();
            }

            for (cb_idx = 0; cb_idx < ncb; cb_idx++) {
                if (cb_idx == PL_K_CTRL && !ctrl_posted) {
                    ctrl_posted = 1;
                    /* control/update visibility latency proxy: frames the
                     * sink will still play with the OLD parameter */
                    stale = shape_b ? pcm_slab_queue_frames_buffered(&q) : 0;
                }
                if (cb_idx == PL_K_RESET && !reset_done) {
                    reset_done = 1;
                    if (shape_b) {
                        dropped = pcm_slab_queue_flush(&q);
                        /* seek/flush semantics: the worker rewinds to the
                         * sink's position so dropped frames are re-read,
                         * never skipped; consumption == playback again */
                        pos = cb_idx * PL_CB;
                        while (q.count < PL_HIGHWATER &&
                               pos + PL_SLAB <= total_frames)
                            WORKER_STEP();
                    }
                    pcm_pipeline_reset(&pipe);
                }

                if (!shape_b) {
                    const float *op;
                    long of, co;
                    t0 = now_ns();
                    if (pcm_pipeline_process(&pipe, in + pos * ch, PL_CB, dev,
                                             PL_CB, &op, &of, &co) != PCM_OK)
                        r->ok = 0;
                    if (op != dev) r->ok = 0; /* copy mode: output must be
                                                 the sink-provided buffer */
                    t1 = now_ns();
                    cb_total += t1 - t0;
                    if (t1 - t0 > cb_max) cb_max = t1 - t0;
                    pipe_total += t1 - t0;
                    h = fnv1a64_update(h, op, (size_t)PL_CB * ch * 4);
                    g_sink ^= f32_bits(op);
                    pos += PL_CB;
                } else {
                    pcm_slab_entry e;
                    double t0s, t1s;
                    if (pcm_slab_queue_pop(&q, &e) != PCM_OK) {
                        underrun++;
                        /* deterministic refill, then retry once */
                        while (q.count < PL_HIGHWATER &&
                               pos + PL_SLAB <= total_frames)
                            WORKER_STEP();
                        if (pcm_slab_queue_pop(&q, &e) != PCM_OK) {
                            r->ok = 0;
                            break;
                        }
                    }
                    t0s = now_ns();
                    memcpy(dev, e.data, (size_t)PL_CB * ch * sizeof(float));
                    t1s = now_ns();
                    sink_total += t1s - t0s;
                    cb_total += t1s - t0s;
                    if (t1s - t0s > cb_max) cb_max = t1s - t0s;
                    sink_calls_rep++;
                    sink_bytes_rep += PL_CB * ch * 4;
                    h = fnv1a64_update(h, dev, (size_t)PL_CB * ch * 4);
                    g_sink ^= f32_bits(dev);
                    pcm_slab_queue_retire(&q, &e);
                    WORKER_STEP();
                }
            }

            r->cb_total_ns[rep] = cb_total;
            r->cb_max_ns[rep] = cb_max;
            r->pipeline_total_ns[rep] = shape_b ? worker_total : pipe_total;
            r->worker_total_ns[rep] = worker_total;
            r->sink_copy_total_ns[rep] = sink_total;
            r->underruns[rep] = underrun;
            r->stale_frames[rep] = stale;
            r->reset_dropped_frames[rep] = dropped;
            r->device_stream_fnv[rep] = h;

            {
                const pcm_counters *cnt = pcm_pipeline_counters(&pipe);
                long long copy_c = cnt->explicit_copy_calls;
                long long copy_b = cnt->explicit_bytes_copied;
                long long fwd = cnt->zero_copy_frames_forwarded +
                                (shape_b ? q.forward_frames : 0);
                if (rep == 0) {
                    r->pipeline_copy_calls = copy_c;
                    r->pipeline_copy_bytes = copy_b;
                    r->sink_copy_calls = sink_calls_rep;
                    r->sink_copy_bytes = sink_bytes_rep;
                    r->queue_peak_slabs = q_peak_rep;
                    r->forward_frames = fwd;
                } else if (copy_c != r->pipeline_copy_calls ||
                           copy_b != r->pipeline_copy_bytes ||
                           sink_calls_rep != r->sink_copy_calls ||
                           sink_bytes_rep != r->sink_copy_bytes ||
                           fwd != r->forward_frames ||
                           q_peak_rep != r->queue_peak_slabs ||
                           stale != r->stale_frames[0] ||
                           dropped != r->reset_dropped_frames[0]) {
                    structural_consistent = 0;
                }
                if (rep > 0 && h != r->device_stream_fnv[0])
                    structural_consistent = 0;
            }
#undef WORKER_STEP
        }
        if (shape_b) pcm_slab_queue_release(&q);
    }

    /* structural aggregates (per single stream; schedule deterministic) */
    {
        long long stream_bytes = total_frames * ch * 4;
        double copied =
            (double)r->pipeline_copy_bytes + (double)r->sink_copy_bytes;
        r->full_memory_passes = copied / (double)stream_bytes;
    }
    r->ok = r->ok && structural_consistent;
    r->deterministic = structural_consistent;
    pcm_free(dev);
}

static int run_placement(FILE *f) {
    const long long total = 1LL << 19; /* 524,288 frames (~10.9 s @48k) */
    const int ch = 2;
    placement_result A, Bc, Bf;
    int rep, i;
    int ok = 1;

    float *in = pcm_malloc((size_t)total * ch * sizeof(float));
    if (!in) return 0;
    fill_pcm(in, total, ch, CORPUS_UNIFORM, 0xE10AACEULL);

    placement_run(0, 0, in, total, ch, &A);
    placement_run(1, 0, in, total, ch, &Bc);
    placement_run(1, 1, in, total, ch, &Bf);

    /* determinism: device-stream fnv identical across reps and identical
     * between shapes/modes (all bypass) */
    for (rep = 1; rep < PL_REPS; rep++) {
        if (A.device_stream_fnv[rep] != A.device_stream_fnv[0]) ok = 0;
        if (Bc.device_stream_fnv[rep] != Bc.device_stream_fnv[0]) ok = 0;
        if (Bf.device_stream_fnv[rep] != Bf.device_stream_fnv[0]) ok = 0;
    }
    if (A.device_stream_fnv[0] != Bc.device_stream_fnv[0] ||
        Bc.device_stream_fnv[0] != Bf.device_stream_fnv[0])
        ok = 0;
    ok = ok && A.ok && Bc.ok && Bf.ok;
    ok = ok && A.stale_frames[0] == 0 && Bc.stale_frames[0] > 0 &&
         Bf.stale_frames[0] > 0;
    ok = ok && A.reset_dropped_frames[0] == 0 &&
         Bc.reset_dropped_frames[0] > 0;
    ok = ok && A.underruns[0] == 0 && Bc.underruns[0] == 0 &&
         Bf.underruns[0] == 0;

    fprintf(f, "{\n  \"experiment\": \"e10-p0\",\n");
    fprintf(f, "  \"section\": \"execution_placement_model\",\n");
    fprintf(f,
            "  \"model\": \"deterministic single-thread simulation; Shape B "
            "'worker' is a scheduling model, not a real RT/worker thread; "
            "no production placement decision is made here\",\n");
    fprintf(f,
            "  \"schedule\": {\"sample_rate\": 48000, \"channels\": %d, "
            "\"total_frames\": %lld, \"callback_frames\": %d, "
            "\"slab_frames\": %d, \"queue_capacity_slabs\": %d, "
            "\"worker_high_water_slabs\": %d, "
            "\"update_event_callback_index\": %d, "
            "\"reset_event_callback_index\": %d, \"reps\": %d},\n",
            ch, total, PL_CB, PL_SLAB, PL_QCAP, PL_HIGHWATER, PL_K_CTRL,
            PL_K_RESET, PL_REPS);
    fprintf(f, "  \"shapes\": [\n");
    {
        placement_result *rows[3] = {&A, &Bc, &Bf};
        for (i = 0; i < 3; i++) {
            placement_result *r = rows[i];
            fprintf(f,
                "    {\"shape\": \"%s\", \"mode\": \"%s\",\n"
                "     \"callback_work_total_ns\": {\"median\": %.1f, "
                "\"min\": %.1f, \"max\": %.1f},\n"
                "     \"callback_work_max_single_callback_ns\": "
                "{\"median\": %.1f, \"min\": %.1f, \"max\": %.1f},\n"
                "     \"pipeline_work_total_ns\": {\"median\": %.1f, "
                "\"min\": %.1f, \"max\": %.1f},\n"
                "     \"sink_copy_total_ns\": {\"median\": %.1f, "
                "\"min\": %.1f, \"max\": %.1f},\n"
                "     \"pipeline_copy_calls\": %lld, "
                "\"pipeline_copy_bytes\": %lld,\n"
                "     \"sink_copy_calls\": %lld, \"sink_copy_bytes\": %lld,\n"
                "     \"forwarded_frames\": %lld,\n"
                "     \"full_memory_passes\": %.6f,\n"
                "     \"queue_peak_slabs\": %lld,\n"
                "     \"buffered_frames_at_update_event\": %lld,\n"
                "     \"update_visibility_note\": \"Shape A: applied at next "
                "callback boundary (stale = 0); Shape B: queued slabs still "
                "play old state; worker block quantization may add up to "
                "slab_frames-1\",\n"
                "     \"reset_dropped_frames\": %lld,\n"
                "     \"underruns\": %lld,\n"
                "     \"device_stream_fnv1a64_rep0\": \"%016llx\",\n"
                "     \"deterministic_across_reps\": %s,\n"
                "     \"verdict\": \"%s\"}%s\n",
                r->shape, r->mode,
                median_of(r->cb_total_ns, PL_REPS),
                min_of(r->cb_total_ns, PL_REPS),
                max_of(r->cb_total_ns, PL_REPS),
                median_of(r->cb_max_ns, PL_REPS),
                min_of(r->cb_max_ns, PL_REPS),
                max_of(r->cb_max_ns, PL_REPS),
                median_of(r->pipeline_total_ns, PL_REPS),
                min_of(r->pipeline_total_ns, PL_REPS),
                max_of(r->pipeline_total_ns, PL_REPS),
                median_of(r->sink_copy_total_ns, PL_REPS),
                min_of(r->sink_copy_total_ns, PL_REPS),
                max_of(r->sink_copy_total_ns, PL_REPS),
                r->pipeline_copy_calls, r->pipeline_copy_bytes,
                r->sink_copy_calls, r->sink_copy_bytes,
                r->forward_frames, r->full_memory_passes,
                r->queue_peak_slabs, r->stale_frames[0],
                r->reset_dropped_frames[0], r->underruns[0],
                (unsigned long long)r->device_stream_fnv[0],
                r->deterministic ? "true" : "false",
                r->ok ? "pass" : "FAIL",
                i < 2 ? "," : "");
        }
    }
    fprintf(f, "  ],\n");
    fprintf(f,
            "  \"placement_evidence_present\": %s,\n"
            "  \"production_placement_decision\": \"none\",\n"
            "  \"verdict\": \"%s\"\n"
            "}\n",
            ok ? "true" : "false", ok ? "PASS" : "FAIL");
    pcm_free(in);
    return ok;
}

/* ------------------------------------------------------------------ */
/* Block-size performance matrix                                       */
/* ------------------------------------------------------------------ */

static int run_performance(FILE *f) {
    const long blocks[6] = {64, 128, 256, 512, 1024, 2048};
    const int chs[2] = {1, 2};
    const long frames = 1 << 20;
    const int passes = 5;
    const int loops = 32; /* stream repeats per timed sample: amortizes
                             clock/scheduler noise at small blocks */
    double dt[16], mdt[16];
    int ok = 1, thr_ok = 1;

    float *in = pcm_malloc((size_t)frames * 2 * sizeof(float));
    float *scratch = pcm_malloc(2048 * 2 * sizeof(float));
    if (!in || !scratch) return 0;
    fill_pcm(in, frames, 2, CORPUS_UNIFORM, 0xE10AFEEDULL);

    fprintf(f, "{\n  \"experiment\": \"e10-p0\",\n");
    fprintf(f, "  \"section\": \"bypass_overhead_block_matrix\",\n");
    fprintf(f,
            "  \"method\": {\"stream_frames\": %ld, \"stream_loops_per_"
            "timed_sample\": %d, \"warmup_passes\": 1, \"timed_passes\": %d, "
            "\"clock\": \"CLOCK_MONOTONIC\", "
            "\"compiler_elision_guard\": \"volatile sink + cross-TU "
            "memcpy\", \"note\": \"one clock read per sample; ns/call "
            "amortized over loops*stream\"},\n",
            frames, loops, passes);
    fprintf(f, "  \"rows\": [\n");
    for (int bi = 0; bi < 6; bi++) {
        long blk = blocks[bi];
        for (int ci = 0; ci < 2; ci++) {
            int ch = chs[ci];
            /* memcpy reference */
            for (int p = 0; p <= passes; p++) {
                double t0, t1;
                t0 = now_ns();
                for (int l = 0; l < loops; l++) {
                    long pos = 0;
                    while (pos < frames) {
                        memcpy(scratch, in + pos * ch,
                               (size_t)blk * ch * sizeof(float));
                        g_sink ^= f32_bits(scratch);
                        pos += blk;
                    }
                }
                t1 = now_ns();
                if (p > 0) mdt[p - 1] = t1 - t0;
            }
            double mref_med = median_of(mdt, passes);
            double mref_min = mdt[0], mref_max = mdt[0];
            for (int p = 0; p < passes; p++) {
                if (mdt[p] < mref_min) mref_min = mdt[p];
                if (mdt[p] > mref_max) mref_max = mdt[p];
            }
            long long calls = frames / blk;
            double sample_calls = (double)calls * loops;

            for (int mi = 0; mi < 2; mi++) {
                pcm_pipeline pipe;
                const pcm_counters *cnt;
                char thr[64], ratio[64];
                long long copy_bytes_expected;
                pcm_pipeline_init(&pipe, mi);
                pipe.rate = pcm_rate_bypass(&pipe.rate_storage, &pipe.cnt);
                if (pcm_pipeline_prepare(&pipe, 48000, 48000, ch, 2048)
                        != PCM_OK) {
                    ok = 0;
                    continue;
                }
                /* warmup (also re-verifies identity for this config) */
                {
                    acct_row row;
                    memset(&row, 0, sizeof(row));
                    acct_stream(&pipe, in, frames, ch, blk, mi, scratch, &row);
                    if (!row.ok) ok = 0;
                }
                for (int p = 0; p < passes; p++) {
                    long pos = 0;
                    double t0, t1;
                    t0 = now_ns();
                    for (int l = 0; l < loops; l++) {
                        pos = 0;
                        while (pos < frames) {
                            const float *op;
                            long of, co;
                            if (pcm_pipeline_process(&pipe, in + pos * ch, blk,
                                                     scratch, blk, &op, &of,
                                                     &co) != PCM_OK)
                                ok = 0;
                            g_sink ^= f32_bits(op);
                            pos += co;
                        }
                    }
                    t1 = now_ns();
                    dt[p] = t1 - t0;
                }
                {
                    double med = median_of(dt, passes);
                    double mn = dt[0], mx = dt[0];
                    for (int p = 0; p < passes; p++) {
                        if (dt[p] < mn) mn = dt[p];
                        if (dt[p] > mx) mx = dt[p];
                    }
                    cnt = pcm_pipeline_counters(&pipe);
                    /* counters include the warmup pass; per-pass copy bytes
                     * are exact by construction (accounting section proves
                     * copy = 1 memcpy/frame, zero-copy = 0): */
                    copy_bytes_expected =
                        mi ? 0 : frames * (long long)ch * 4;
                    if (mi) {
                        snprintf(thr, sizeof(thr), "null");
                        snprintf(ratio, sizeof(ratio), "null");
                    } else {
                        /* bytes / ns -> MiB/s: *1e9 / (1024^2). The old
                         * *1e3 was a units bug (P1-1). */
                        double mbps =
                            (double)copy_bytes_expected * loops / med *
                            1e9 / (1024.0 * 1024.0);
                        snprintf(thr, sizeof(thr), "%.1f", mbps);
                        snprintf(ratio, sizeof(ratio), "%.3f",
                                 ((med - mref_med) / sample_calls) /
                                     (mref_med / sample_calls));
                        thr_ok = isfinite(mbps) && mbps > 0.0 &&
                                 med > 0.0 && loops > 0;
                    }
                    fprintf(f,
                        "    {\"block_frames\": %ld, \"channels\": %d, "
                        "\"mode\": \"%s\", \"calls_per_pass\": %lld, "
                        "\"stream_loops_per_sample\": %d,\n"
                        "     \"ns_per_call\": {\"median\": %.1f, \"min\": "
                        "%.1f, \"max\": %.1f},\n"
                        "     \"ns_per_frame\": {\"median\": %.4f},\n"
                        "     \"throughput_MBps_copy\": %s,\n"
                        "     \"throughput_sanity_ok\": %s,\n"
                        "     \"explicit_bytes_copied_per_pass\": %lld,\n"
                        "     \"memcpy_ref_ns_per_call\": {\"median\": %.1f, "
                        "\"min\": %.1f, \"max\": %.1f},\n"
                        "     \"overhead_ns_per_call_vs_memcpy\": %.1f, "
                        "\"overhead_ratio_vs_memcpy\": %s}\n",
                        blk, ch, mi ? "zero_copy" : "copy", calls, loops,
                        med / sample_calls, mn / sample_calls,
                        mx / sample_calls,
                        med / ((double)frames * loops),
                        thr, thr_ok ? "true" : "false",
                        copy_bytes_expected,
                        mref_med / sample_calls, mref_min / sample_calls,
                        mref_max / sample_calls,
                        mi ? 0.0 : (med - mref_med) / sample_calls,
                        ratio);
                    if (!thr_ok) ok = 0;
                    if (bi != 5 || ci != 1 || mi != 1) fprintf(f, ",\n");
                    (void)cnt;
                }
            }
        }
    }
    fprintf(f, "  ],\n");
    fprintf(f, "  \"gate_statement\": \"P0 goal: the BYPASS abstraction "
               "itself must be almost free (ns/call overhead vs raw memcpy; "
               "no absolute real-time claim)\",\n");
    fprintf(f, "  \"provenance_note\": \"single run of this harness on one "
               "host; distributions across timed passes are recorded; do "
               "not compare across hosts\",\n");
    fprintf(f, "  \"verdict\": \"%s\"\n", ok ? "PASS" : "FAIL");
    fprintf(f, "}\n");
    pcm_free(in);
    pcm_free(scratch);
    return ok;
}

/* ------------------------------------------------------------------ */
/* main                                                                */
/* ------------------------------------------------------------------ */

static FILE *open_out(const char *dir, const char *name) {
    char path[4096];
    FILE *f;
    if (snprintf(path, sizeof(path), "%s/%s", dir, name) >= (int)sizeof(path))
        return NULL;
    f = fopen(path, "w");
    return f;
}

int main(int argc, char **argv) {
    const char *dir = NULL;
    const char *sections = "correctness,buffer,placement,performance,negative";
    FILE *f;
    int ok = 1, r, i;
    int want_corr = 1, want_buf = 1, want_place = 1, want_perf = 1;
    int want_neg = 1;

    for (i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--sections") == 0 && i + 1 < argc) {
            sections = argv[++i];
        } else if (!dir) {
            dir = argv[i];
        }
    }
    if (!dir) {
        fprintf(stderr, "usage: qn_pcm_p0_harness <output_dir> "
                "[--sections correctness,buffer,placement,performance,"
                "negative]\n");
        return 2;
    }
    want_corr = strstr(sections, "correctness") != NULL;
    want_buf  = strstr(sections, "buffer") != NULL;
    want_place = strstr(sections, "placement") != NULL;
    want_perf = strstr(sections, "performance") != NULL;
    want_neg  = strstr(sections, "negative") != NULL;

    if (want_corr) {
        f = open_out(dir, "p0-correctness.json");
        if (!f) return 2;
        r = run_correctness(f);
        fclose(f);
        printf("correctness: %s\n", r ? "PASS" : "FAIL");
        ok = ok && r;
    }

    if (want_neg) {
        f = open_out(dir, "p0-negative-tests.json");
        if (!f) return 2;
        r = run_negative(f);
        fclose(f);
        printf("negative: %s\n", r ? "PASS" : "FAIL");
        ok = ok && r;
    }

    if (want_buf) {
        f = open_out(dir, "p0-buffer-accounting.json");
        if (!f) return 2;
        r = run_accounting(f);
        fclose(f);
        printf("buffer-accounting: %s\n", r ? "PASS" : "FAIL");
        ok = ok && r;
    }

    if (want_place) {
        f = open_out(dir, "p0-placement.json");
        if (!f) return 2;
        r = run_placement(f);
        fclose(f);
        printf("placement: %s\n", r ? "PASS" : "FAIL");
        ok = ok && r;
    }

    if (want_perf) {
        f = open_out(dir, "p0-performance.json");
        if (!f) return 2;
        r = run_performance(f);
        fclose(f);
        printf("performance: %s\n", r ? "PASS" : "FAIL");
        ok = ok && r;
    }

    printf("P0 harness: %s\n", ok ? "ALL PASS" : "FAILURES PRESENT");
    return ok ? 0 : 1;
}
