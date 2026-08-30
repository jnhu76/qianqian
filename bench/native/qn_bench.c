/*
 * qn_bench.c — Phase 0 Step 1 native benchmark binary.
 *
 * SongCore-shaped flow over a trimmed FFmpeg, used ONLY by the bench
 * harness. This file is experiment/benchmark code; it must never become
 * a player-level dependency, and it never links more FFmpeg libraries
 * than the profile under test provides.
 *
 * Flow per corpus case (correct mode):
 *   open(host io) -> probe -> metadata -> artwork -> sequential decode
 *   -> seek(25%/50%/75%) -> decode again -> EOF
 *
 * Host IO callbacks mirror the SongCore contract (read/seek/size), so
 * FFmpeg runs without any enabled protocol, filesystem or network
 * permission of its own: it only ever sees bytes.
 *
 * Output: one JSON object on stdout. The Python harness owns all
 * expectation checking; this binary only reports observed facts.
 *
 * Canonical PCM: Float32 interleaved at source rate/layout (no resample,
 * no rematrix — sample format conversion and planar->interleaved only).
 * Built with QN_HAVE_SWR the conversion runs through libswresample;
 * without it the bench performs the same conversion with its own code
 * (what SongCore would have to own if swr is dropped). Both paths must
 * produce byte-identical output; the harness enforces that across
 * profiles.
 */

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <sys/resource.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/opt.h>

#ifdef QN_HAVE_SWR
#include <libswresample/swresample.h>
#endif

/* ------------------------------------------------------------------ */
/* sha256                                                              */
/* ------------------------------------------------------------------ */

typedef struct {
    uint32_t h[8];
    uint64_t total_len;
    uint8_t buf[64];
    size_t buf_len;
} sha256_ctx;

static const uint32_t K256[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

#define ROTR(x,n) (((x) >> (n)) | ((x) << (32 - (n))))

static void sha256_transform(sha256_ctx *c, const uint8_t *p) {
    uint32_t w[64], a, b, cc, d, e, f, g, h;
    for (int i = 0; i < 16; i++)
        w[i] = ((uint32_t)p[i*4]<<24)|((uint32_t)p[i*4+1]<<16)|((uint32_t)p[i*4+2]<<8)|p[i*4+3];
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = ROTR(w[i-15],7) ^ ROTR(w[i-15],18) ^ (w[i-15] >> 3);
        uint32_t s1 = ROTR(w[i-2],17) ^ ROTR(w[i-2],19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a=c->h[0];b=c->h[1];cc=c->h[2];d=c->h[3];e=c->h[4];f=c->h[5];g=c->h[6];h=c->h[7];
    for (int i = 0; i < 64; i++) {
        uint32_t S1 = ROTR(e,6)^ROTR(e,11)^ROTR(e,25);
        uint32_t ch = (e & f) ^ (~e & g);
        uint32_t t1 = h + S1 + ch + K256[i] + w[i];
        uint32_t S0 = ROTR(a,2)^ROTR(a,13)^ROTR(a,22);
        uint32_t maj = (a & b) ^ (a & cc) ^ (b & cc);
        uint32_t t2 = S0 + maj;
        h=g; g=f; f=e; e=d+t1; d=cc; cc=b; b=a; a=t1+t2;
    }
    c->h[0]+=a;c->h[1]+=b;c->h[2]+=cc;c->h[3]+=d;
    c->h[4]+=e;c->h[5]+=f;c->h[6]+=g;c->h[7]+=h;
}

static void sha256_init(sha256_ctx *c) {
    static const uint32_t H0[8] = {0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
                                   0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    memcpy(c->h, H0, sizeof(H0));
    c->total_len = 0; c->buf_len = 0;
}

static void sha256_update(sha256_ctx *c, const void *data, size_t n) {
    const uint8_t *p = data;
    c->total_len += n;
    while (n > 0) {
        size_t take = 64 - c->buf_len;
        if (take > n) take = n;
        memcpy(c->buf + c->buf_len, p, take);
        c->buf_len += take; p += take; n -= take;
        if (c->buf_len == 64) { sha256_transform(c, c->buf); c->buf_len = 0; }
    }
}

static void sha256_final(sha256_ctx *c, uint8_t out[32]) {
    uint64_t bits = c->total_len * 8;
    uint8_t pad = 0x80;
    sha256_update(c, &pad, 1);
    uint8_t zero = 0;
    while (c->buf_len != 56) sha256_update(c, &zero, 1);
    uint8_t lenb[8];
    for (int i = 0; i < 8; i++) lenb[i] = (uint8_t)(bits >> (56 - i*8));
    sha256_update(c, lenb, 8);
    for (int i = 0; i < 8; i++) {
        out[i*4]   = (uint8_t)(c->h[i] >> 24);
        out[i*4+1] = (uint8_t)(c->h[i] >> 16);
        out[i*4+2] = (uint8_t)(c->h[i] >> 8);
        out[i*4+3] = (uint8_t)(c->h[i]);
    }
}

static void sha256_hex(sha256_ctx *c, char out[65]) {
    uint8_t d[32];
    sha256_final(c, d);
    for (int i = 0; i < 32; i++) sprintf(out + i*2, "%02x", d[i]);
    out[64] = 0;
}

static void json_escape(const char *s, char *out, size_t cap) {
    size_t o = 0;
    for (const unsigned char *p = (const unsigned char *)s; *p && o + 8 < cap; p++) {
        if (*p == '"' || *p == '\\') { out[o++] = '\\'; out[o++] = (char)*p; }
        else if (*p < 0x20) { o += (size_t)snprintf(out + o, cap - o, "\\u%04x", *p); }
        else out[o++] = (char)*p;
    }
    out[o] = 0;
}

/* ------------------------------------------------------------------ */
/* host IO (SongCore-shaped: read / seek / size, nothing else)         */
/* ------------------------------------------------------------------ */

typedef struct {
    FILE *f;
    int64_t size;
} host_io;

static int io_read_cb(void *opaque, uint8_t *buf, int buf_size) {
    host_io *io = opaque;
    size_t n = fread(buf, 1, (size_t)buf_size, io->f);
    if (n == 0) return AVERROR_EOF;
    return (int)n;
}

static int64_t io_seek_cb(void *opaque, int64_t offset, int whence) {
    host_io *io = opaque;
    if (whence & AVSEEK_SIZE) return io->size;
    int w = whence & ~AVSEEK_FORCE;
    if (w == SEEK_END) {
        if (fseek(io->f, (long)offset, SEEK_END) != 0) return -1;
        return ftell(io->f);
    }
    if (fseek(io->f, (long)offset, w) != 0) return -1;
    return ftell(io->f);
}

/* ------------------------------------------------------------------ */
/* timing / rss                                                        */
/* ------------------------------------------------------------------ */

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1e6;
}

static long peak_rss_kb(void) {
    struct rusage ru;
    getrusage(RUSAGE_SELF, &ru);
    return ru.ru_maxrss; /* KB on Linux */
}

/* ------------------------------------------------------------------ */
/* canonical Float32 conversion                                        */
/* ------------------------------------------------------------------ */

/*
 * Convert one decoded frame to interleaved Float32 at the frame's own
 * rate and layout. Format conversion + planar->interleaved only.
 * Returns frames written or <0. Must stay byte-identical to the swr
 * path below (harness cross-check).
 */
static int frame_to_f32(const AVFrame *f, float *dst, int dst_capacity_frames) {
    int ch = f->ch_layout.nb_channels;
    int ns = f->nb_samples;
    if (ns <= 0) return 0;
    if (ns * ch > dst_capacity_frames) return -1;
    switch (f->format) {
    case AV_SAMPLE_FMT_FLT:
        memcpy(dst, f->data[0], sizeof(float) * (size_t)ns * ch);
        return ns;
    case AV_SAMPLE_FMT_FLTP:
        for (int c = 0; c < ch; c++) {
            const float *src = (const float *)f->extended_data[c];
            for (int i = 0; i < ns; i++) dst[(size_t)i*ch + c] = src[i];
        }
        return ns;
    case AV_SAMPLE_FMT_S16P:
        for (int c = 0; c < ch; c++) {
            const int16_t *src = (const int16_t *)f->extended_data[c];
            for (int i = 0; i < ns; i++) dst[(size_t)i*ch + c] = (float)src[i] * (1.0f/32768.0f);
        }
        return ns;
    case AV_SAMPLE_FMT_S32P:
        for (int c = 0; c < ch; c++) {
            const int32_t *src = (const int32_t *)f->extended_data[c];
            for (int i = 0; i < ns; i++) dst[(size_t)i*ch + c] = (float)src[i] * (1.0f/2147483648.0f);
        }
        return ns;
    case AV_SAMPLE_FMT_S16: {
        const int16_t *src = (const int16_t *)f->data[0];
        for (int i = 0; i < ns * ch; i++) dst[i] = (float)src[i] * (1.0f/32768.0f);
        return ns;
    }
    case AV_SAMPLE_FMT_S32: {
        const int32_t *src = (const int32_t *)f->data[0];
        for (int i = 0; i < ns * ch; i++) dst[i] = (float)src[i] * (1.0f/2147483648.0f);
        return ns;
    }
    case AV_SAMPLE_FMT_DBLP:
        for (int c = 0; c < ch; c++) {
            const double *src = (const double *)f->extended_data[c];
            for (int i = 0; i < ns; i++) dst[(size_t)i*ch + c] = (float)src[i];
        }
        return ns;
    case AV_SAMPLE_FMT_U8: {
        const uint8_t *src = f->data[0];
        for (int i = 0; i < ns * ch; i++) dst[i] = ((float)src[i] - 128.0f) * (1.0f/128.0f);
        return ns;
    }
    case AV_SAMPLE_FMT_DBL: {
        const double *src = (const double *)f->data[0];
        for (int i = 0; i < ns * ch; i++) dst[i] = (float)src[i];
        return ns;
    }
    case AV_SAMPLE_FMT_U8P:
        for (int c = 0; c < ch; c++) {
            const uint8_t *src = f->extended_data[c];
            for (int i = 0; i < ns; i++) dst[(size_t)i*ch + c] = ((float)src[i] - 128.0f) * (1.0f/128.0f);
        }
        return ns;
    default:
        return -1;
    }
}

#ifdef QN_HAVE_SWR
/* swresample path: same contract, format conversion + interleave only. */
static int frame_to_f32_swr(struct SwrContext **swr, const AVFrame *f,
                            float *dst, int cap_frames) {
    if (!*swr) {
        *swr = swr_alloc();
        if (!*swr) return -1;
        av_opt_set_chlayout(*swr, "in_chlayout",  &f->ch_layout, 0);
        av_opt_set_chlayout(*swr, "out_chlayout", &f->ch_layout, 0);
        av_opt_set_int(*swr, "in_sample_rate", f->sample_rate, 0);
        av_opt_set_int(*swr, "out_sample_rate", f->sample_rate, 0);
        av_opt_set_sample_fmt(*swr, "in_sample_fmt", f->format, 0);
        av_opt_set_sample_fmt(*swr, "out_sample_fmt", AV_SAMPLE_FMT_FLT, 0);
        if (swr_init(*swr) < 0) return -1;
    }
    return (int)swr_convert(*swr, (uint8_t **)&dst, f->nb_samples,
                            (const uint8_t **)f->extended_data, f->nb_samples);
}
#endif

/* ------------------------------------------------------------------ */
/* sample store (correct mode: full canonical stream + frame index)    */
/* ------------------------------------------------------------------ */

typedef struct {
    float *data;
    size_t len, cap;            /* interleaved sample count */
    int channels;
    int sample_rate;
    int64_t *frame_first;        /* first sample index per decoded frame */
    size_t frame_count, frame_cap;
} sample_store;

static void store_init(sample_store *s) { memset(s, 0, sizeof(*s)); }
static void store_free(sample_store *s) {
    free(s->data); free(s->frame_first);
    memset(s, 0, sizeof(*s));
}
static int store_push(sample_store *s, const float *v, size_t n, int64_t first_index) {
    if (s->len + n > s->cap) {
        size_t ncap = s->cap ? s->cap * 2 : (1u << 16);
        while (ncap < s->len + n) ncap *= 2;
        float *nd = realloc(s->data, ncap * sizeof(float));
        if (!nd) return -1;
        s->data = nd; s->cap = ncap;
    }
    memcpy(s->data + s->len, v, n * sizeof(float));
    s->len += n;
    if (s->frame_count == s->frame_cap) {
        s->frame_cap = s->frame_cap ? s->frame_cap * 2 : 256;
        int64_t *nf = realloc(s->frame_first, s->frame_cap * sizeof(int64_t));
        if (!nf) return -1;
        s->frame_first = nf;
    }
    s->frame_first[s->frame_count++] = first_index;
    return 0;
}

/* ------------------------------------------------------------------ */
/* session                                                             */
/* ------------------------------------------------------------------ */

typedef struct {
    AVFormatContext *fmt;
    AVCodecContext *dec;
    AVIOContext *avio;
    uint8_t *avio_buf;
    host_io io;
    int audio_index;
    int artwork_index;
#ifdef QN_HAVE_SWR
    struct SwrContext *swr;
#endif
    char first_error[256];
    int error_count;
    int demux_eof;
    int decoder_eof;
} session;

static void sess_close(session *s) {
#ifdef QN_HAVE_SWR
    if (s->swr) swr_free(&s->swr);
#endif
    if (s->dec) avcodec_free_context(&s->dec);
    if (s->fmt) {
        /* detach custom pb before close so ffmpeg does not free our avio */
        s->fmt->pb = NULL;
        s->fmt->flags &= ~(unsigned)AVFMT_FLAG_CUSTOM_IO;
        avformat_close_input(&s->fmt);
    }
    if (s->avio) {
        av_freep(&s->avio->buffer);
        avio_context_free(&s->avio);
    }
    if (s->io.f) fclose(s->io.f);
    memset(s, 0, sizeof(*s));
}

static int sess_open(session *s, const char *path) {
    memset(s, 0, sizeof(*s));
    s->audio_index = -1;
    s->artwork_index = -1;

    s->io.f = fopen(path, "rb");
    if (!s->io.f) { snprintf(s->first_error, sizeof(s->first_error), "host open failed: %s", path); return -1; }
    fseek(s->io.f, 0, SEEK_END);
    long fsz = ftell(s->io.f);
    fseek(s->io.f, 0, SEEK_SET);
    s->io.size = fsz;

    s->avio_buf = av_malloc(32768);
    s->avio = avio_alloc_context(s->avio_buf, 32768, 0, &s->io,
                                 io_read_cb, NULL, io_seek_cb);
    if (!s->avio) { snprintf(s->first_error, sizeof(s->first_error), "avio_alloc_context failed"); return -1; }

    s->fmt = avformat_alloc_context();
    if (!s->fmt) { snprintf(s->first_error, sizeof(s->first_error), "avformat_alloc failed"); return -1; }
    s->fmt->pb = s->avio;
    s->fmt->flags |= AVFMT_FLAG_CUSTOM_IO;

    int ret = avformat_open_input(&s->fmt, "", NULL, NULL);
    if (ret < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "open_input: %d (%s)", ret, av_err2str(ret));
        return ret;
    }
    return 0;
}

static int sess_probe(session *s) {
    int ret = avformat_find_stream_info(s->fmt, NULL);
    if (ret < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "find_stream_info: %d (%s)", ret, av_err2str(ret));
        return ret;
    }
    for (unsigned i = 0; i < s->fmt->nb_streams; i++) {
        AVStream *st = s->fmt->streams[i];
        if (st->codecpar->codec_type == AVMEDIA_TYPE_AUDIO && s->audio_index < 0)
            s->audio_index = (int)i;
        if ((st->disposition & AV_DISPOSITION_ATTACHED_PIC) && s->artwork_index < 0)
            s->artwork_index = (int)i;
    }
    if (s->audio_index < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "no audio stream");
        return -1;
    }
    return 0;
}

static int sess_open_decoder(session *s) {
    AVStream *st = s->fmt->streams[s->audio_index];
    const AVCodec *codec = avcodec_find_decoder(st->codecpar->codec_id);
    if (!codec) {
        snprintf(s->first_error, sizeof(s->first_error), "no decoder for codec_id %d", st->codecpar->codec_id);
        return -1;
    }
    s->dec = avcodec_alloc_context3(codec);
    if (!s->dec) return -1;
    int ret = avcodec_parameters_to_context(s->dec, st->codecpar);
    if (ret < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "params_to_context: %d", ret);
        return ret;
    }
    ret = avcodec_open2(s->dec, codec, NULL);
    if (ret < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "avcodec_open2: %d (%s)", ret, av_err2str(ret));
        return ret;
    }
    return 0;
}

static void sess_note_error(session *s, const char *what, int err) {
    if (!s->error_count)
        snprintf(s->first_error, sizeof(s->first_error), "%s: %d (%s)", what, err, av_err2str(err));
    s->error_count++;
}

/* decode all packets until demux EOF; canonical samples go to `store`
 * (if non-NULL); stops cleanly at demux EOF + decoder drain. */
static int sess_decode_all(session *s, sample_store *store,
                           int64_t *out_samples, int64_t *out_frames) {
    AVPacket *pkt = av_packet_alloc();
    AVFrame *frm = av_frame_alloc();
    int64_t total_samples = 0, total_frames = 0;
    int ch = 0, max_frame_floats = 0;

    if (s->dec) {
        ch = s->dec->ch_layout.nb_channels;
        if (ch <= 0) {
            AVStream *st = s->fmt->streams[s->audio_index];
            ch = st->codecpar->ch_layout.nb_channels;
        }
        max_frame_floats = ch * 65536;
    }
    float *conv = malloc(sizeof(float) * (size_t)(max_frame_floats > 0 ? max_frame_floats : 1));

    int stopped = 0;
    while (!stopped) {
        int ret = av_read_frame(s->fmt, pkt);
        if (ret < 0) {
            if (ret == AVERROR_EOF || avio_feof(s->fmt->pb)) { s->demux_eof = 1; }
            else sess_note_error(s, "read_frame", ret);
            stopped = 1;
            break;
        }
        if (pkt->stream_index == s->audio_index && s->dec) {
            ret = avcodec_send_packet(s->dec, pkt);
            if (ret < 0 && ret != AVERROR(EAGAIN)) sess_note_error(s, "send_packet", ret);
            while (ret >= 0) {
                ret = avcodec_receive_frame(s->dec, frm);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                if (ret < 0) { sess_note_error(s, "receive_frame", ret); break; }
                AVStream *st = s->fmt->streams[s->audio_index];
                int64_t first_index = frm->pts != AV_NOPTS_VALUE
                    ? av_rescale_q(frm->pts, st->time_base, (AVRational){1, frm->sample_rate})
                    : (int64_t)(store ? store->len / (store->channels ? (size_t)store->channels : 1) : 0);
                /* Codec-delay trimming advances pts past the container start
                 * (opus preskip): the presentation timeline is shifted
                 * relative to the canonical output stream, which starts
                 * at 0; a negative index can never be stored as size_t. */
                if (first_index < 0) first_index = 0;
                if (store) {
                    int n = -1;
#ifdef QN_HAVE_SWR
                    n = frame_to_f32_swr(&s->swr, frm, conv, max_frame_floats);
#else
                    n = frame_to_f32(frm, conv, max_frame_floats);
#endif
                    if (n < 0) {
                        sess_note_error(s, "canonical conversion unsupported fmt", frm->format);
                    } else if (store_push(store, conv, (size_t)n * frm->ch_layout.nb_channels, first_index) < 0) {
                        sess_note_error(s, "store_push", -ENOMEM);
                    }
                }
                total_samples += frm->nb_samples;
                total_frames += 1;
                av_frame_unref(frm);
            }
        }
        av_packet_unref(pkt);
    }

    /* drain decoder */
    if (s->dec) {
        int ret = avcodec_send_packet(s->dec, NULL);
        if (ret >= 0) {
            while ((ret = avcodec_receive_frame(s->dec, frm)) >= 0) {
                AVStream *st = s->fmt->streams[s->audio_index];
                int64_t first_index = frm->pts != AV_NOPTS_VALUE
                    ? av_rescale_q(frm->pts, st->time_base, (AVRational){1, frm->sample_rate})
                    : (int64_t)(store && store->channels ? store->len / (size_t)store->channels : 0);
                if (first_index < 0) first_index = 0;
                if (store) {
                    int n = -1;
#ifdef QN_HAVE_SWR
                    n = frame_to_f32_swr(&s->swr, frm, conv, max_frame_floats);
#else
                    n = frame_to_f32(frm, conv, max_frame_floats);
#endif
                    if (n >= 0 && store_push(store, conv, (size_t)n * frm->ch_layout.nb_channels, first_index) < 0)
                        sess_note_error(s, "store_push", -ENOMEM);
                }
                total_samples += frm->nb_samples;
                total_frames += 1;
                av_frame_unref(frm);
            }
            if (ret == AVERROR_EOF) s->decoder_eof = 1;
        }
    }

    free(conv);
    av_packet_free(&pkt);
    av_frame_free(&frm);
    if (out_samples) *out_samples = total_samples;
    if (out_frames) *out_frames = total_frames;
    return 0;
}

/* ------------------------------------------------------------------ */
/* correctness mode                                                    */
/* ------------------------------------------------------------------ */

static int run_correct(const char *path) {
    session s;
    double t0 = now_ms();
    int rc = sess_open(&s, path);
    double open_ms = now_ms() - t0;
    if (rc < 0) {
        printf("{\"mode\":\"correct\",\"status\":\"open_failed\",\"error\":\"");
        char e[512]; json_escape(s.first_error, e, sizeof(e));
        printf("%s\"}\n", e);
        sess_close(&s);
        return 1;
    }

    t0 = now_ms();
    rc = sess_probe(&s);
    double probe_ms = now_ms() - t0;
    if (rc < 0) {
        printf("{\"mode\":\"correct\",\"status\":\"probe_failed\",\"error\":\"");
        char e[512]; json_escape(s.first_error, e, sizeof(e));
        printf("%s\"}\n", e);
        sess_close(&s);
        return 1;
    }

    AVStream *ast = s.fmt->streams[s.audio_index];
    printf("{\"mode\":\"correct\",\"status\":\"ok\"");
    printf(",\"container\":\"%s\"", s.fmt->iformat->name);
    printf(",\"codec\":\"%s\"", avcodec_get_name(ast->codecpar->codec_id));
    printf(",\"sample_rate\":%d", ast->codecpar->sample_rate);
    printf(",\"channels\":%d", ast->codecpar->ch_layout.nb_channels);
    printf(",\"bits_per_raw_sample\":%d", ast->codecpar->bits_per_raw_sample);
    printf(",\"duration_us\":%" PRId64, s.fmt->duration);
    printf(",\"nb_streams\":%u", s.fmt->nb_streams);
    printf(",\"open_ms\":%.3f", open_ms);
    printf(",\"probe_ms\":%.3f", probe_ms);

    /* metadata */
    printf(",\"metadata\":[");
    int first = 1;
    AVDictionary *md[] = { s.fmt->metadata, ast->metadata };
    for (int d = 0; d < 2; d++) {
        const AVDictionaryEntry *tag = NULL;
        int count = 0;
        while ((tag = av_dict_iterate(md[d], tag)) && count < 32) {
            char k[256], v[512];
            json_escape(tag->key, k, sizeof(k));
            json_escape(tag->value, v, sizeof(v));
            printf("%s{\"key\":\"%s\",\"value\":\"%s\",\"scope\":\"%s\"}",
                   first ? "" : ",", k, v, d == 0 ? "format" : "stream");
            first = 0; count++;
        }
    }
    printf("]");

    /* artwork: compressed bytes only (SongCore contract; no image decode) */
    if (s.artwork_index >= 0) {
        AVStream *vs = s.fmt->streams[s.artwork_index];
        AVPacket *pic = &vs->attached_pic;
        if (pic && pic->size > 0) {
            sha256_ctx c; sha256_init(&c);
            sha256_update(&c, pic->data, (size_t)pic->size);
            char hex[65]; sha256_hex(&c, hex);
            printf(",\"artwork\":{\"found\":true,\"size\":%d,\"sha256\":\"%s\",\"codec\":\"%s\"}",
                   pic->size, hex, avcodec_get_name(vs->codecpar->codec_id));
        }
    }
    if (s.artwork_index < 0)
        printf(",\"artwork\":{\"found\":false}");

    /* sequential decode */
    sample_store store;
    store_init(&store);
    store.channels = ast->codecpar->ch_layout.nb_channels;
    store.sample_rate = ast->codecpar->sample_rate;

    if (sess_open_decoder(&s) < 0) {
        printf(",\"decode\":{\"status\":\"decoder_open_failed\"}");
        char e[512]; json_escape(s.first_error, e, sizeof(e));
        printf(",\"error\":\"%s\"}\n", e);
        store_free(&store);
        sess_close(&s);
        return 1;
    }

    int64_t samples = 0, frames = 0;
    t0 = now_ms();
    sess_decode_all(&s, &store, &samples, &frames);
    double decode_ms = now_ms() - t0;

    char pcm_hex[65] = "";
    if (store.len) {
        sha256_ctx c; sha256_init(&c);
        sha256_update(&c, store.data, store.len * sizeof(float));
        sha256_hex(&c, pcm_hex);
    }
    printf(",\"decode\":{\"status\":\"done\",\"frames\":%" PRId64 ",\"samples\":%" PRId64,
           frames, samples);
    printf(",\"canonical_f32_sha256\":\"%s\",\"canonical_len\":%zu", pcm_hex, store.len);
    printf(",\"decode_ms\":%.3f}", decode_ms);
    printf(",\"eof\":{\"demux\":%s,\"decoder\":%s}",
           s.demux_eof ? "true" : "false", s.decoder_eof ? "true" : "false");
    printf(",\"error_count\":%d", s.error_count);
    {
        char e[512]; json_escape(s.first_error, e, sizeof(e));
        printf(",\"first_error\":\"%s\"", e);
    }

    /* suffix checksum map at every frame boundary (for seek verification) */
    printf(",\"suffix\":[");
    if (store.len) {
        int ch = store.channels ? store.channels : 1;
        int emitted = 0;
        for (size_t fi = 0; fi < store.frame_count; fi++) {
            int64_t idx = store.frame_first[fi];
            if (idx < 0) continue;
            size_t off = (size_t)idx * ch;
            if (off > store.len) continue;
            sha256_ctx c; sha256_init(&c);
            sha256_update(&c, store.data + off, (store.len - off) * sizeof(float));
            char hex[65]; sha256_hex(&c, hex);
            printf("%s[%" PRId64 ",\"%s\"]", emitted ? "," : "", idx, hex);
            emitted++;
        }
    }
    printf("]");

    /* seek: 25% / 50% / 75% */
    printf(",\"seeks\":[");
    int64_t dur = s.fmt->duration;
    int have_dur = dur > 0;
    for (int i = 0; i < 3; i++) {
        if (!have_dur) break;
        int64_t target = (int64_t)((double)dur * (0.25 * (i + 1)));
        /* fresh session per seek for isolation */
        session s2;
        printf("%s{", i ? "," : "");
        if (sess_open(&s2, path) < 0 || sess_probe(&s2) < 0 || sess_open_decoder(&s2) < 0) {
            char e[512]; json_escape(s2.first_error, e, sizeof(e));
            printf("\"status\":\"reopen_failed\",\"error\":\"%s\"}", e);
            sess_close(&s2);
            continue;
        }
        AVStream *ast2 = s2.fmt->streams[s2.audio_index];
        int64_t target_samples = (int64_t)av_rescale_q(target, (AVRational){1, AV_TIME_BASE},
                                                       (AVRational){1, ast2->codecpar->sample_rate});
        /* target is in AV_TIME_BASE us; seek wants stream time_base units */
        int64_t target_ts = av_rescale_q(target, (AVRational){1, AV_TIME_BASE},
                                         ast2->time_base);
        double t_s = now_ms();
        int ret = av_seek_frame(s2.fmt, s2.audio_index, target_ts, AVSEEK_FLAG_BACKWARD);
        double seek_ms = now_ms() - t_s;
        if (ret < 0) {
            printf("\"status\":\"seek_failed\",\"target_us\":%" PRId64 ",\"ret\":%d}", target, ret);
            sess_close(&s2);
            continue;
        }
        avcodec_flush_buffers(s2.dec);
        sample_store st2;
        store_init(&st2);
        st2.channels = store.channels;
        st2.sample_rate = store.sample_rate;
        int64_t smp = 0, fr = 0;
        sess_decode_all(&s2, &st2, &smp, &fr);
        int64_t resume_index = -1;
        if (st2.frame_count > 0) resume_index = st2.frame_first[0];
        /* suffix match vs sequential stream */
        int match = 0;
        if (resume_index >= 0 && st2.len > 0) {
            size_t off = (size_t)resume_index * (store.channels ? store.channels : 1);
            if (off <= store.len && store.len - off == st2.len) {
                match = memcmp(store.data + off, st2.data, st2.len * sizeof(float)) == 0;
            }
        }
        char hex[65] = "";
        if (st2.len) {
            sha256_ctx c; sha256_init(&c);
            sha256_update(&c, st2.data, st2.len * sizeof(float));
            sha256_hex(&c, hex);
        }
        printf("\"status\":\"done\",\"target_us\":%" PRId64 ",\"target_sample\":%" PRId64
               ",\"resume_sample\":%" PRId64 ",\"frames\":%" PRId64 ",\"samples\":%" PRId64
               ",\"suffix_sha256\":\"%s\",\"suffix_match_sequential\":%s"
               ",\"seek_ms\":%.3f}",
               target, target_samples, resume_index, fr, smp, hex,
               match ? "true" : "false", seek_ms);
        store_free(&st2);
        sess_close(&s2);
    }
    printf("]");

    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());

    store_free(&store);
    sess_close(&s);
    return 0;
}

/* ------------------------------------------------------------------ */
/* throughput mode                                                     */
/* ------------------------------------------------------------------ */

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return x < y ? -1 : x > y ? 1 : 0;
}

static int run_bench(const char *path, int iterations) {
    int iters = iterations > 0 ? iterations : 5;
    double *songcore_ms = malloc(sizeof(double) * iters);
    double *decodecore_ms = malloc(sizeof(double) * iters);
    double open_first_ms = 0;
    char pcm_hex[65] = "";
    int64_t samples = 0;
    int sample_rate = 0, channels = 0;
    int64_t duration_us = 0;

    for (int it = -1; it < iters; it++) {
        /* songcore-output: open -> decode -> canonical f32 */
        session s;
        double t_open = now_ms();
        if (sess_open(&s, path) < 0 || sess_probe(&s) < 0 || sess_open_decoder(&s) < 0) {
            printf("{\"mode\":\"bench\",\"status\":\"failed\",\"error\":\"");
            char e[512]; json_escape(s.first_error, e, sizeof(e));
            printf("%s\"}\n", e);
            sess_close(&s);
            free(songcore_ms); free(decodecore_ms);
            return 1;
        }
        double t_open_end = now_ms();
        if (it == 0) open_first_ms = t_open_end - t_open; /* after one warmup */

        sample_store store;
        store_init(&store);
        store.channels = s.fmt->streams[s.audio_index]->codecpar->ch_layout.nb_channels;
        store.sample_rate = s.fmt->streams[s.audio_index]->codecpar->sample_rate;
        int64_t smp = 0, fr = 0;
        double t_d = now_ms();
        sess_decode_all(&s, &store, &smp, &fr);
        double t_d_end = now_ms();
        if (it == 0) {
            samples = smp;
            sample_rate = store.sample_rate;
            channels = store.channels;
            duration_us = s.fmt->duration;
            if (store.len) {
                sha256_ctx c; sha256_init(&c);
                sha256_update(&c, store.data, store.len * sizeof(float));
                sha256_hex(&c, pcm_hex);
            }
        }
        if (it >= 0) songcore_ms[it] = t_d_end - t_d;
        store_free(&store);
        sess_close(&s);

        /* decode-core: open -> demux+decode only (no canonical conversion). */
        if (it >= 0) {
            session s2;
            if (sess_open(&s2, path) < 0 || sess_probe(&s2) < 0 || sess_open_decoder(&s2) < 0) {
                sess_close(&s2);
                continue;
            }
            int64_t smp2 = 0, fr2 = 0;
            double t_d2 = now_ms();
            sess_decode_all(&s2, NULL, &smp2, &fr2);
            decodecore_ms[it] = now_ms() - t_d2;
            sess_close(&s2);
        }
    }

    qsort(songcore_ms, iters, sizeof(double), cmp_double);
    qsort(decodecore_ms, iters, sizeof(double), cmp_double);
    double median_songcore = songcore_ms[iters / 2];
    double median_decodecore = decodecore_ms[iters / 2];
    double min_songcore = songcore_ms[0], max_songcore = songcore_ms[iters - 1];
    double audio_s = duration_us > 0 ? duration_us / 1e6 : (samples && sample_rate ? (double)samples / sample_rate : 0);

    printf("{\"mode\":\"bench\",\"status\":\"ok\"");
    printf(",\"sample_rate\":%d,\"channels\":%d,\"samples\":%" PRId64, sample_rate, channels, samples);
    printf(",\"duration_us\":%" PRId64, duration_us);
    printf(",\"canonical_f32_sha256\":\"%s\"", pcm_hex);
    printf(",\"open_first_ms\":%.3f", open_first_ms);
    printf(",\"iterations\":%d", iters);
    printf(",\"warmup\":1");
    printf(",\"songcore_output_ms\":{\"median\":%.3f,\"min\":%.3f,\"max\":%.3f}",
           median_songcore, min_songcore, max_songcore);
    printf(",\"decode_core_ms\":{\"median\":%.3f,\"min\":%.3f,\"max\":%.3f}",
           median_decodecore, decodecore_ms[0], decodecore_ms[iters - 1]);
    if (audio_s > 0) {
        printf(",\"xrt_songcore_output\":%.2f", audio_s / (median_songcore / 1000.0));
        printf(",\"xrt_decode_core\":%.2f", audio_s / (median_decodecore / 1000.0));
    }
    printf(",\"audio_seconds\":%.3f", audio_s);
    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());
    free(songcore_ms); free(decodecore_ms);
    return 0;
}

/* ------------------------------------------------------------------ */

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr,
                "usage: %s correct <file>\n"
                "       %s bench <file> [iterations=%d default 5]\n",
                argv[0], argv[0], 5);
        return 2;
    }
    av_log_set_level(AV_LOG_ERROR);
    if (strcmp(argv[1], "correct") == 0) return run_correct(argv[2]);
    if (strcmp(argv[1], "bench") == 0)
        return run_bench(argv[2], argc > 3 ? atoi(argv[3]) : 5);
    fprintf(stderr, "unknown mode %s\n", argv[1]);
    return 2;
}
