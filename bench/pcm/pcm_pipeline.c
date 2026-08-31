/*
 * E10-P0 experimental PCM pipeline — BYPASS implementation + instrumentation.
 * See pcm_pipeline.h for the contract rationale. Bench-only code.
 */
#include "pcm_pipeline.h"

#include <stdlib.h>
#include <string.h>

/* ------------------------------------------------------------------ */
/* Counting allocator                                                  */
/* ------------------------------------------------------------------ */

static pcm_alloc_stats g_alloc;
static int g_alloc_armed;

void pcm_alloc_reset_stats(void) {
    memset(&g_alloc, 0, sizeof(g_alloc));
    g_alloc_armed = 0;
}
void pcm_alloc_arm(void)    { g_alloc_armed = 1; }
void pcm_alloc_disarm(void) { g_alloc_armed = 0; }
const pcm_alloc_stats *pcm_alloc_stats_get(void) { return &g_alloc; }

static void *pcm_alloc_record(size_t n, void *p) {
    if (!p) return NULL;
    g_alloc.allocations++;
    g_alloc.alloc_bytes += (long)n;
    if (g_alloc_armed) {
        g_alloc.rt_violations++;
        g_alloc.rt_violation_bytes += (long)n;
    }
    return p;
}

void *pcm_malloc(size_t n) {
    return pcm_alloc_record((long)n, malloc(n));
}

void *pcm_calloc(size_t n, size_t sz) {
    return pcm_alloc_record(n * sz, calloc(n, sz));
}

void pcm_free(void *p) {
    if (p) { g_alloc.frees++; free(p); }
}

/* ------------------------------------------------------------------ */
/* BYPASS rate stage (SRC semantics, P0 implementation)                */
/* ------------------------------------------------------------------ */

static int bypass_prepare(pcm_rate_stage *st, int in_rate, int out_rate,
                          int channels, long max_frames) {
    if (channels <= 0 || max_frames < 0) return PCM_ERR_NOT_PREPARED;
    st->in_rate = in_rate;
    st->out_rate = out_rate;
    st->channels = channels;
    st->max_frames = max_frames;
    st->buffered_frames = 0;
    st->cnt->state_epoch++;   /* rate-dependent state must not survive */
    st->cnt->internal_buffer_capacity_frames = 0; /* bypass buffers nothing */
    return PCM_OK;
}

static int bypass_process(pcm_rate_stage *st, const float *in, long in_frames,
                          float *out, long out_cap,
                          long *consumed, long *produced) {
    long n, bytes;
    if (!st->channels) return PCM_ERR_NOT_PREPARED;
    n = in_frames < out_cap ? in_frames : out_cap;
    if (n < 0) n = 0;
    bytes = n * (long)st->channels * (long)sizeof(float);
    if (n > 0) {
        memcpy(out, in, (size_t)bytes);   /* the one explicit copy */
        st->cnt->explicit_copy_calls++;
        st->cnt->explicit_bytes_copied += bytes;
    }
    *consumed = n;
    *produced = n;
    st->buffered_frames = 0;  /* bypass never retains input */
    if (st->cnt->peak_buffered_frames < 0) st->cnt->peak_buffered_frames = 0;
    return PCM_OK;
}

static int bypass_drain(pcm_rate_stage *st, float *out, long out_cap,
                        long *produced) {
    (void)out; (void)out_cap;
    *produced = 0;            /* nothing buffered */
    if (st->cnt) st->cnt->drain_calls++;
    return PCM_OK;
}

static int bypass_reset(pcm_rate_stage *st) {
    st->buffered_frames = 0;
    if (st->cnt) st->cnt->state_epoch++;
    return PCM_OK;
}

static long bypass_latency(const pcm_rate_stage *st) {
    (void)st;
    return 0;
}

static long bypass_required_input(const pcm_rate_stage *st, long out_frames) {
    (void)st;
    return out_frames;        /* strictly 1:1 */
}

static const pcm_rate_stage_ops kBypassOps = {
    bypass_prepare, bypass_process, bypass_drain, bypass_reset,
    bypass_latency, bypass_required_input,
};

pcm_rate_stage *pcm_rate_bypass(pcm_rate_stage *storage, pcm_counters *cnt) {
    storage->ops = &kBypassOps;
    storage->in_rate = storage->out_rate = storage->channels = 0;
    storage->max_frames = 0;
    storage->buffered_frames = 0;
    storage->cnt = cnt;
    storage->impl = NULL;
    return storage;
}

/* ------------------------------------------------------------------ */
/* OFF DSP stage (rate-preserving semantics, P0)                       */
/* ------------------------------------------------------------------ */

static int dspoff_prepare(pcm_dsp_stage *st, int sample_rate, int channels,
                          long max_frames) {
    if (channels <= 0 || max_frames < 0) return PCM_ERR_NOT_PREPARED;
    st->sample_rate = sample_rate;
    st->channels = channels;
    st->max_frames = max_frames;
    st->buffered_frames = 0;
    st->cnt->state_epoch++;   /* stale coefficients must not survive */
    return PCM_OK;
}

static int dspoff_process(pcm_dsp_stage *st, float *pcm, long frames) {
    (void)st; (void)pcm; (void)frames;
    return PCM_OK;            /* true no-op: no read, no write, no copy */
}

static int dspoff_reset(pcm_dsp_stage *st) {
    st->buffered_frames = 0;
    if (st->cnt) st->cnt->state_epoch++;
    return PCM_OK;
}

static long dspoff_latency(const pcm_dsp_stage *st) {
    (void)st;
    return 0;
}

static const pcm_dsp_stage_ops kDspOffOps = {
    dspoff_prepare, dspoff_process, dspoff_reset, dspoff_latency,
};

pcm_dsp_stage *pcm_dsp_off(pcm_dsp_stage *storage, pcm_counters *cnt) {
    storage->ops = &kDspOffOps;
    storage->sample_rate = storage->channels = 0;
    storage->max_frames = 0;
    storage->buffered_frames = 0;
    storage->cnt = cnt;
    storage->impl = NULL;
    return storage;
}

/* ------------------------------------------------------------------ */
/* Pipeline                                                            */
/* ------------------------------------------------------------------ */

void pcm_pipeline_init(pcm_pipeline *p, int allow_zero_copy) {
    memset(p, 0, sizeof(*p));
    p->allow_zero_copy = allow_zero_copy;
}

int pcm_pipeline_prepare(pcm_pipeline *p, int in_rate, int out_rate,
                         int channels, long max_frames) {
    long alloc_before = pcm_alloc_stats_get()->allocations;
    int rc, i;
    long dropped = 0;

    if (!p->rate || p->ndsp > PCM_PIPELINE_MAX_DSP) return PCM_ERR_NOT_PREPARED;
    p->channels = channels;
    p->max_frames = max_frames;

    /* Reconfigure semantics: stage-buffered input must not survive. */
    if (p->prepared) {
        dropped += p->rate->buffered_frames;
        p->rate->buffered_frames = 0;
        for (i = 0; i < p->ndsp; i++) dropped += p->dsp[i]->buffered_frames;
        p->cnt.dropped_frames_on_reconfigure += dropped;
    }

    rc = p->rate->ops->prepare(p->rate, in_rate, out_rate, channels, max_frames);
    if (rc != PCM_OK) return rc;
    for (i = 0; i < p->ndsp; i++) {
        rc = p->dsp[i]->ops->prepare(p->dsp[i], out_rate, channels, max_frames);
        if (rc != PCM_OK) return rc;
    }
    p->prepared = 1;
    p->cnt.allocations_during_prepare =
        pcm_alloc_stats_get()->allocations - alloc_before;
    return PCM_OK;
}

int pcm_pipeline_add_dsp_stage(pcm_pipeline *p, pcm_dsp_stage *st) {
    if (p->ndsp >= PCM_PIPELINE_MAX_DSP) return PCM_ERR_NOT_PREPARED;
    st->cnt = &p->cnt;
    p->dsp[p->ndsp++] = st;
    return PCM_OK;
}

int pcm_pipeline_process(pcm_pipeline *p, const float *in, long in_frames,
                         float *out, long out_cap,
                         const float **out_ptr, long *out_frames,
                         long *consumed) {
    long produced = 0, cons = 0, bytes;
    int rc, i;

    if (!p->prepared) return PCM_ERR_NOT_PREPARED;
    p->cnt.process_calls++;
    if (in_frames <= 0) { p->cnt.zero_frame_calls++; }
    p->cnt.input_frames += in_frames > 0 ? in_frames : 0;
    p->cnt.input_bytes += in_frames > 0
        ? in_frames * (long)p->channels * (long)sizeof(float) : 0;

    /* Zero-copy forward: only when configured AND the shape permits it
     * (bypass rate stage, zero DSP stages). Verified by pointer identity
     * at the call site; nothing is written here. */
    if (p->allow_zero_copy && p->ndsp == 0 &&
        p->rate->ops->process == bypass_process) {
        if (out_cap < in_frames) return PCM_ERR_OUT_CAP;
        if (in_frames > 0) p->cnt.zero_copy_frames_forwarded += in_frames;
        *out_ptr = in;
        *out_frames = in_frames;
        *consumed = in_frames;
        p->cnt.output_frames += in_frames;
        p->cnt.output_bytes += in_frames > 0
            ? in_frames * (long)p->channels * (long)sizeof(float) : 0;
        return PCM_OK;
    }

    rc = p->rate->ops->process(p->rate, in, in_frames, out, out_cap,
                               &cons, &produced);
    if (rc != PCM_OK) return rc;
    bytes = produced * (long)p->channels * (long)sizeof(float);

    for (i = 0; i < p->ndsp; i++) {
        rc = p->dsp[i]->ops->process_in_place(p->dsp[i], out, produced);
        if (rc != PCM_OK) return rc;
    }

    p->cnt.output_frames += produced;
    p->cnt.output_bytes += bytes;
    if (p->cnt.peak_buffered_frames < p->rate->buffered_frames)
        p->cnt.peak_buffered_frames = p->rate->buffered_frames;
    *out_ptr = out;
    *out_frames = produced;
    *consumed = cons;
    return PCM_OK;
}

int pcm_pipeline_drain(pcm_pipeline *p, float *out, long out_cap,
                       const float **out_ptr, long *out_frames) {
    long produced = 0, total = 0, bytes;
    int rc, i;
    if (!p->prepared) return PCM_ERR_NOT_PREPARED;
    rc = p->rate->ops->drain(p->rate, out, out_cap, &produced);
    if (rc != PCM_OK) return rc;
    total = produced;
    bytes = produced * (long)p->channels * (long)sizeof(float);
    for (i = 0; i < p->ndsp; i++) {
        rc = p->dsp[i]->ops->process_in_place(p->dsp[i], out, total);
        if (rc != PCM_OK) return rc;
    }
    p->cnt.output_frames += total;
    p->cnt.output_bytes += bytes;
    *out_ptr = out;
    *out_frames = total;
    return PCM_OK;
}

int pcm_pipeline_reset(pcm_pipeline *p) {
    int rc, i;
    if (!p->prepared) return PCM_ERR_NOT_PREPARED;
    rc = p->rate->ops->reset(p->rate);
    if (rc != PCM_OK) return rc;
    for (i = 0; i < p->ndsp; i++) {
        rc = p->dsp[i]->ops->reset(p->dsp[i]);
        if (rc != PCM_OK) return rc;
    }
    return PCM_OK;
}

long pcm_pipeline_latency_frames(const pcm_pipeline *p) {
    long lat = 0;
    int i;
    if (!p->rate) return -1;
    lat += p->rate->ops->latency_frames(p->rate);
    for (i = 0; i < p->ndsp; i++)
        lat += p->dsp[i]->ops->latency_frames(p->dsp[i]);
    return lat;
}

const pcm_counters *pcm_pipeline_counters(const pcm_pipeline *p) {
    return &p->cnt;
}

/* ------------------------------------------------------------------ */
/* Bounded slab queue                                                  */
/* ------------------------------------------------------------------ */

int pcm_slab_queue_prepare(pcm_slab_queue *q, long capacity_slabs,
                           long slab_frames, int channels) {
    long i;
    memset(q, 0, sizeof(*q));
    q->pending_idx = -1;
    q->slab_storage = pcm_calloc((size_t)capacity_slabs, sizeof(float *));
    q->free_stack = pcm_calloc((size_t)capacity_slabs, sizeof(long));
    q->ring = pcm_calloc((size_t)capacity_slabs, sizeof(pcm_slab_entry));
    if (!q->slab_storage || !q->free_stack || !q->ring)
        return PCM_ERR_ALLOC;
    for (i = 0; i < capacity_slabs; i++) {
        q->slab_storage[i] =
            pcm_malloc((size_t)slab_frames * channels * sizeof(float));
        if (!q->slab_storage[i]) return PCM_ERR_ALLOC;
        q->free_stack[i] = capacity_slabs - 1 - i; /* LIFO free list */
    }
    q->free_top = capacity_slabs;
    q->capacity_slabs = capacity_slabs;
    q->slab_frames = slab_frames;
    q->channels = channels;
    return PCM_OK;
}

float *pcm_slab_queue_acquire(pcm_slab_queue *q) {
    if (q->count >= q->capacity_slabs || q->free_top == 0)
        return NULL; /* backpressure: worker must wait */
    q->pending_idx = q->free_stack[--q->free_top];
    return q->slab_storage[q->pending_idx];
}

int pcm_slab_queue_enqueue_filled(pcm_slab_queue *q) {
    if (q->pending_idx < 0) return PCM_ERR_OUT_CAP;
    q->ring[q->tail].data = q->slab_storage[q->pending_idx];
    q->ring[q->tail].frames = q->slab_frames;
    q->ring[q->tail].forwarded = 0;
    q->ring[q->tail].storage_index = q->pending_idx;
    q->pending_idx = -1;
    q->tail = (q->tail + 1) % q->capacity_slabs;
    q->count++;
    if (q->count > q->peak_count) q->peak_count = q->count;
    q->enqueued_frames += q->slab_frames;
    return PCM_OK;
}

int pcm_slab_queue_push_forward(pcm_slab_queue *q, const float *span,
                                long frames) {
    if (frames != q->slab_frames) return PCM_ERR_OUT_CAP;
    if (q->count >= q->capacity_slabs) return PCM_ERR_OUT_CAP;
    q->ring[q->tail].data = (float *)span; /* borrowed, not copied */
    q->ring[q->tail].frames = frames;
    q->ring[q->tail].forwarded = 1;
    q->ring[q->tail].storage_index = -1;
    q->tail = (q->tail + 1) % q->capacity_slabs;
    q->count++;
    if (q->count > q->peak_count) q->peak_count = q->count;
    q->enqueued_frames += frames;
    q->forward_frames += frames;
    return PCM_OK;
}

int pcm_slab_queue_pop(pcm_slab_queue *q, pcm_slab_entry *out) {
    if (q->count == 0) return PCM_ERR_OUT_CAP;
    *out = q->ring[q->head];
    q->head = (q->head + 1) % q->capacity_slabs;
    q->count--;
    q->dequeued_frames += out->frames;
    return PCM_OK;
}

void pcm_slab_queue_retire(pcm_slab_queue *q, const pcm_slab_entry *e) {
    if (!e->forwarded && e->storage_index >= 0)
        q->free_stack[q->free_top++] = e->storage_index;
}

long pcm_slab_queue_flush(pcm_slab_queue *q) {
    long frames = q->count * q->slab_frames;
    long i;
    q->dropped_frames += frames;
    q->head = q->tail = q->count = 0;
    /* All storage returns to the free list (forwarded spans own none). */
    for (i = 0; i < q->capacity_slabs; i++)
        q->free_stack[i] = q->capacity_slabs - 1 - i;
    q->free_top = q->capacity_slabs;
    return frames;
}

void pcm_slab_queue_release(pcm_slab_queue *q) {
    long i;
    if (q->slab_storage) {
        for (i = 0; i < q->capacity_slabs; i++)
            if (q->slab_storage[i]) pcm_free(q->slab_storage[i]);
        pcm_free(q->slab_storage);
    }
    if (q->free_stack) pcm_free(q->free_stack);
    if (q->ring) pcm_free(q->ring);
    memset(q, 0, sizeof(*q));
}

long pcm_slab_queue_frames_buffered(const pcm_slab_queue *q) {
    return q->count * q->slab_frames;
}
