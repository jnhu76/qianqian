/*
 * E10-B0 thin DSP reference — bench-only. Mirrors the P0 FixedRateDSP
 * contract: in-place, frame-preserving, sample-rate-preserving. All
 * coefficient/parameter computation happens in prepare/set_* (outside
 * the RT loop); process() is allocation-free.
 *
 * NaN/Inf policy (B0, applies ONLY while DSP is ACTIVE): sanitize —
 * an offending input sample is replaced with 0 for the duration of the
 * active pass, so stateful filters cannot be poisoned permanently.
 * DSP OFF remains the P0 bit-transparent bypass.
 *
 * Biquad authority: Audio EQ Cookbook (RBJ) peaking coefficients.
 * Implementation form: Direct Form II Transposed (good float32
 * numerical behavior, minimal state). Float32 vs double reference is
 * compared by the harness.
 */
#ifndef QN_B0_DSP_H
#define QN_B0_DSP_H

typedef struct b0_dsp b0_dsp;

typedef struct b0_dsp_ops {
    const char *name;
    int (*prepare)(b0_dsp *d, int sample_rate, int channels, long max_frames);
    int (*process)(b0_dsp *d, float *pcm, long frames);
    int (*reset)(b0_dsp *d);
    long (*latency_frames)(const b0_dsp *d);
    void (*destroy)(b0_dsp *d);
} b0_dsp_ops;

struct b0_dsp {
    const b0_dsp_ops *ops;
    void *impl;
};

/* Gain (linear, fused single multiply). */
int b0_gain_new(b0_dsp *d, float gain_linear);

/* RBJ peaking biquad. f0 and gain_db are set at creation; coefficient
 * computation is outside the RT loop. */
int b0_peaking_new(b0_dsp *d, int sample_rate, float f0, float q,
                   float gain_db);

/* Classic 10-band graphic EQ (31.25..16000 Hz, octave-ish spacing).
 * gains_db[10]. Bands at or above 0.95 * Nyquist are skipped (never
 * unstable coefficients). */
int b0_eq10_new(b0_dsp *d, int sample_rate, const float gains_db[10]);

/* Simple peak limiter, EXPLICITLY enabled by the caller; not in the
 * default playback path. No lookahead: latency_frames = 0. */
int b0_limiter_new(b0_dsp *d, int sample_rate, float threshold_db,
                   float attack_s, float release_s);

/* Adversarial / instrumented accessors used only by the bench harness. */
void b0_dsp_set_nan_policy(b0_dsp *d, int sanitize_active);
int b0_dsp_nan_seen(const b0_dsp *d);
void b0_dsp_set_gain(b0_dsp *d, float gain_linear);
void b0_dsp_set_peaking(b0_dsp *d, int sample_rate, float f0, float q,
                        float gain_db);
int b0_dsp_biquad_state_is_zero(const b0_dsp *d);

#endif /* QN_B0_DSP_H */
