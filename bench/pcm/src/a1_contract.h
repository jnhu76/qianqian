/*
 * E10-A1 SRC adapter contract — bench-only. Mirrors the repaired P0
 * RateStage semantics so the shootout measures the same contract a real
 * SRC stage must implement:
 *
 *   prepare(in_rate, out_rate, channels, max_frames)
 *   process(in, in_frames, out, out_cap) -> consumed_frames, produced_frames
 *   drain(out, out_cap) -> produced_frames
 *   reset()
 *   latency_frames()                     # output-frame accounting
 *   required_input_for_output(out_frames)
 *
 * Float32 interleaved in/out only. Same channel count in/out. No remix.
 * No dither. Rate conversion only. Adapters must report input consumed /
 * output produced / internal buffered frames / latency / allocation
 * behavior; no hidden extra queues outside accounting.
 *
 * Third-party libraries are NOT production dependencies: they are fetched
 * into the untracked experiment cache (build/e10-src) and linked only
 * into the A1 runners.
 */
#ifndef QN_A1_CONTRACT_H
#define QN_A1_CONTRACT_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct a1_src a1_src;

typedef struct a1_src_ops {
    const char *name;                     /* canonical candidate name     */
    int (*prepare)(a1_src *s, int in_rate, int out_rate, int channels,
                   long max_frames);
    int (*process)(a1_src *s, const float *in, long in_frames,
                   float *out, long out_cap,
                   long *consumed, long *produced, int is_last);
    int (*drain)(a1_src *s, float *out, long out_cap, long *produced);
    int (*reset)(a1_src *s);
    /* Total algorithmic latency in OUTPUT frames; -1 if not expressible. */
    long (*latency_frames)(const a1_src *s);
    /* Input frames needed (worst case, incl. buffered) to produce
     * out_frames output frames; -1 if not expressible. */
    long (*required_input_for_output)(const a1_src *s, long out_frames);
    void (*destroy)(a1_src *s);
} a1_src_ops;

struct a1_src {
    const a1_src_ops *ops;
    void *impl;
};

/* Instantiate the candidate named `name` (one adapter is linked per
 * runner binary). Returns 0 on success, -1 unknown name. */
int a1_make(const char *name, a1_src *out);

#ifdef __cplusplus
}
#endif

#endif /* QN_A1_CONTRACT_H */
