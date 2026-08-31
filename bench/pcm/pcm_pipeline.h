/*
 * E10-P0 experimental PCM-processing contracts — bench-only.
 *
 * NOT production code. This header exists to make the two stage semantics
 * concrete enough to measure before any SRC/DSP implementation is chosen:
 *
 *   Rate-changing stage (SRC semantics):
 *     input frame count may differ from output frame count.
 *     prepare(in_rate, out_rate, channels, max_frames)
 *     process(in, in_frames, out, out_cap) -> consumed_frames, produced_frames
 *     drain(out, out_cap) -> produced_frames
 *     reset()   latency_frames()   required_input_for_output()
 *
 *   Rate-preserving stage (DSP semantics):
 *     frame count and sample rate remain unchanged, in-place.
 *     prepare(sample_rate, channels, max_frames)
 *     process_in_place(interleaved_pcm, frames)
 *     reset()   latency_frames()
 *
 * P0 ships only a BYPASS rate stage and an OFF DSP stage. No SRC, no DSP
 * math, no libavfilter, no SIMD. SongCore is untouched; FFmpeg types must
 * not (and do not) appear here.
 *
 * Instrumentation is deterministic: allocations go through the counting
 * allocator below (never inferred from RSS); copies are counted at the
 * exact memcpy sites. Zero-copy forwarding is verified by pointer
 * identity, not assumed.
 */
#ifndef QN_PCM_PIPELINE_H
#define QN_PCM_PIPELINE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Counting allocator — the allocation gate authority.                 */
/* Any dynamic memory owned by the experimental pipeline/harness MUST  */
/* go through these. When armed, an allocation records an RT violation */
/* instead of being silently allowed.                                  */
/* ------------------------------------------------------------------ */

typedef struct pcm_alloc_stats {
    long allocations;     /* successful pcm_malloc/pcm_calloc calls      */
    long alloc_bytes;     /* bytes requested through the allocator       */
    long frees;
    long rt_violations;   /* allocations while pcm_alloc_arm() is active */
    long rt_violation_bytes;
} pcm_alloc_stats;

void pcm_alloc_reset_stats(void);
void pcm_alloc_arm(void);     /* RT path begins: allocations = violations */
void pcm_alloc_disarm(void);
const pcm_alloc_stats *pcm_alloc_stats_get(void);

void *pcm_malloc(size_t n);
void *pcm_calloc(size_t n, size_t sz);
void pcm_free(void *p);

/* ------------------------------------------------------------------ */
/* Per-pipeline counters (the machine-readable accounting payload).    */
/* ------------------------------------------------------------------ */

typedef struct pcm_counters {
    long long input_frames;         /* frames consumed at pipeline input  */
    long long output_frames;        /* frames produced at pipeline output */
    long long process_calls;
    long long drain_calls;
    long long zero_frame_calls;     /* process calls with 0 frames        */
    long long input_bytes;          /* bytes touched at input (frames*ch*4)*/
    long long output_bytes;         /* bytes written at output            */
    long long explicit_copy_calls;  /* exact memcpy sites executed        */
    long long explicit_bytes_copied;
    long long zero_copy_frames_forwarded; /* forwarded by pointer identity*/
    long long internal_buffer_capacity_frames; /* stage-internal buffers */
    long long peak_buffered_frames; /* max frames held inside stages      */
    long long allocations_during_prepare;
    long long allocations_after_prepare;
    long long dropped_frames_on_reconfigure; /* stage-buffered input dropped
                                                by a re-prepare */
    long long state_epoch;          /* bumped on every prepare/reconfigure
                                       and on reset; stale-state detector */
} pcm_counters;

/* ------------------------------------------------------------------ */
/* Rate-changing stage (SRC semantics).                                */
/* ------------------------------------------------------------------ */

typedef struct pcm_rate_stage pcm_rate_stage;

typedef struct pcm_rate_stage_ops {
    int (*prepare)(pcm_rate_stage *st, int in_rate, int out_rate,
                   int channels, long max_frames);
    /* Consume up to in_frames input frames, produce up to out_cap output
     * frames. consumed <= in_frames, produced <= out_cap. */
    int (*process)(pcm_rate_stage *st, const float *in, long in_frames,
                   float *out, long out_cap,
                   long *consumed, long *produced);
    /* Flush internal state at end-of-stream. */
    int (*drain)(pcm_rate_stage *st, float *out, long out_cap,
                 long *produced);
    int (*reset)(pcm_rate_stage *st);
    /* Total algorithmic latency in OUTPUT frames (0 for bypass). */
    long (*latency_frames)(const pcm_rate_stage *st);
    /* Input frames needed (wor case, incl. buffered) to produce
     * out_frames output frames; -1 if not expressible. */
    long (*required_input_for_output)(const pcm_rate_stage *st,
                                      long out_frames);
} pcm_rate_stage_ops;

struct pcm_rate_stage {
    const pcm_rate_stage_ops *ops;
    int in_rate, out_rate, channels;
    long max_frames;
    long buffered_frames;   /* input frames currently held internally  */
    pcm_counters *cnt;      /* set at pipeline assembly                */
    void *impl;
};

/* BYPASS rate stage factory. Bit-transparent: 1:1 frames, no gain, no
 * clip, no rematrix. P0 selects no SRC — this is the transparency
 * reference every future SRC stage will be diffed against. */
pcm_rate_stage *pcm_rate_bypass(pcm_rate_stage *storage, pcm_counters *cnt);

/* ------------------------------------------------------------------ */
/* Rate-preserving stage (DSP semantics, in-place).                    */
/* ------------------------------------------------------------------ */

typedef struct pcm_dsp_stage pcm_dsp_stage;

typedef struct pcm_dsp_stage_ops {
    int (*prepare)(pcm_dsp_stage *st, int sample_rate, int channels,
                   long max_frames);
    /* In-place, frame-preserving. Must not allocate after prepare. */
    int (*process_in_place)(pcm_dsp_stage *st, float *interleaved_pcm,
                            long frames);
    int (*reset)(pcm_dsp_stage *st);
    long (*latency_frames)(const pcm_dsp_stage *st);
} pcm_dsp_stage_ops;

struct pcm_dsp_stage {
    const pcm_dsp_stage_ops *ops;
    int sample_rate, channels;
    long max_frames;
    long buffered_frames;
    pcm_counters *cnt;
    void *impl;
};

/* OFF DSP stage: process_in_place is a true no-op (not a multiply by 1,
 * not a flat biquad). Proves the pipeline composes >=1 DSP stage while
 * remaining bit-transparent; P0's transparent gate uses zero stages. */
pcm_dsp_stage *pcm_dsp_off(pcm_dsp_stage *storage, pcm_counters *cnt);

/* ------------------------------------------------------------------ */
/* Pipeline: input -> RateStage -> DSP stage(s) -> output.             */
/* ------------------------------------------------------------------ */

#define PCM_PIPELINE_MAX_DSP 8
#define PCM_OK               0
#define PCM_ERR_OUT_CAP     -1
#define PCM_ERR_NOT_PREPARED -2
#define PCM_ERR_ALLOC        -3

typedef struct pcm_pipeline {
    pcm_rate_stage rate_storage;
    pcm_rate_stage *rate;
    pcm_dsp_stage dsp_storage[PCM_PIPELINE_MAX_DSP];
    pcm_dsp_stage *dsp[PCM_PIPELINE_MAX_DSP];
    int ndsp;
    int allow_zero_copy;    /* bypass + 0 stages may forward by pointer */
    int channels;
    long max_frames;
    int prepared;
    pcm_counters cnt;
} pcm_pipeline;

void pcm_pipeline_init(pcm_pipeline *p, int allow_zero_copy);
/* (Re)configure. in_rate == out_rate is the P0 shape; the contract keeps
 * both rates because a real SRC stage needs them. Any reconfigure bumps
 * state_epoch and drops stage-buffered input (recorded). */
int pcm_pipeline_prepare(pcm_pipeline *p, int in_rate, int out_rate,
                         int channels, long max_frames);
int pcm_pipeline_add_dsp_stage(pcm_pipeline *p, pcm_dsp_stage *st);

/* Runs one pipeline step. On return *out_ptr points to the produced PCM:
 *   - == out          (copied path: one explicit copy counted), or
 *   - == in (alias)   (zero-copy forward; *consumed == *out_frames and
 *                      out/out_cap were not touched).
 * out_cap must be >= min(in_frames, required) — for zero-copy forward
 * out_cap is required to be >= in_frames. */
int pcm_pipeline_process(pcm_pipeline *p, const float *in, long in_frames,
                         float *out, long out_cap,
                         const float **out_ptr, long *out_frames,
                         long *consumed);
int pcm_pipeline_drain(pcm_pipeline *p, float *out, long out_cap,
                       const float **out_ptr, long *out_frames);
int pcm_pipeline_reset(pcm_pipeline *p);
long pcm_pipeline_latency_frames(const pcm_pipeline *p);
const pcm_counters *pcm_pipeline_counters(const pcm_pipeline *p);

/* ------------------------------------------------------------------ */
/* Bounded slab queue — Shape B (worker processing) buffering model.   */
/* Fixed capacity, allocated entirely in prepare; RT path only pops.   */
/* ------------------------------------------------------------------ */

typedef struct pcm_slab_entry {
    float *data;            /* slab data (or forwarded caller span)     */
    long frames;
    int forwarded;          /* 1 = pointer handoff, no worker copy      */
    long storage_index;     /* -1 for forwarded spans                   */
} pcm_slab_entry;

typedef struct pcm_slab_queue {
    float **slab_storage;   /* capacity_slabs pre-allocated buffers     */
    long *free_stack;       /* indices of free slab_storage slots       */
    long free_top;
    pcm_slab_entry *ring;   /* capacity_slabs entries                   */
    long capacity_slabs;
    long slab_frames;       /* frames per slab                          */
    int channels;
    long head, tail, count; /* ring indices                             */
    long peak_count;        /* max slabs simultaneously occupied        */
    long long enqueued_frames, dequeued_frames, dropped_frames;
    long long forward_frames; /* enqueued by pointer (zero-copy handoff)*/
    long pending_idx;       /* slab handed out by acquire(), -1 if none */
} pcm_slab_queue;

/* Allocates slab storage via the counting allocator (prepare-time only). */
int pcm_slab_queue_prepare(pcm_slab_queue *q, long capacity_slabs,
                           long slab_frames, int channels);
/* Worker, copy mode: borrow a free slab (NULL under backpressure). The
 * worker runs the pipeline into it, then MUST enqueue_filled(). */
float *pcm_slab_queue_acquire(pcm_slab_queue *q);
int pcm_slab_queue_enqueue_filled(pcm_slab_queue *q);
/* Worker, zero-copy mode: enqueue the caller's span by pointer. */
int pcm_slab_queue_push_forward(pcm_slab_queue *q, const float *span,
                                long frames);
/* Consumer: pops an entry; data stays valid until retire(). Non-forwarded
 * slabs are returned to the free list by retire(), never by pop(). */
int pcm_slab_queue_pop(pcm_slab_queue *q, pcm_slab_entry *out);
void pcm_slab_queue_retire(pcm_slab_queue *q, const pcm_slab_entry *e);
/* Drop everything (reset/flush); returns dropped frames. */
long pcm_slab_queue_flush(pcm_slab_queue *q);
void pcm_slab_queue_release(pcm_slab_queue *q);
long pcm_slab_queue_frames_buffered(const pcm_slab_queue *q);

#ifdef __cplusplus
}
#endif

#endif /* QN_PCM_PIPELINE_H */
