#include <inttypes.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <sys/resource.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>

#include "songcore.h"

#define IO_BUFFER_BYTES 32768

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

static double wall_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e6 + (double)ts.tv_nsec / 1e3;
}

static double cpu_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts);
    return (double)ts.tv_sec * 1e6 + (double)ts.tv_nsec / 1e3;
}

static long peak_rss_kb(void) {
    struct rusage ru;
    getrusage(RUSAGE_SELF, &ru);
    return ru.ru_maxrss;
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

static void print_path(const char *path) {
    char e[1024];
    json_escape(path, e, sizeof(e));
    printf("\"%s\"", e);
}

typedef struct {
    FILE *f;
    int64_t size;
    int error_count;
    char first_error[256];
} host_io;

static int host_read_cb(void *opaque, uint8_t *buf, int buf_size) {
    host_io *io = opaque;
    size_t n = fread(buf, 1, (size_t)buf_size, io->f);
    if (n == 0) {
        if (ferror(io->f)) io->error_count++;
        return AVERROR_EOF;
    }
    return (int)n;
}

static int64_t host_seek_cb(void *opaque, int64_t offset, int whence) {
    host_io *io = opaque;
    if (whence & AVSEEK_SIZE) return io->size;
    int w = whence & ~AVSEEK_FORCE;
    if (fseek(io->f, (long)offset, w) != 0) return -1;
    return ftell(io->f);
}

static int64_t song_read_cb(void *userdata, uint8_t *dst, size_t size) {
    FILE *f = (FILE *)userdata;
    size_t n = fread(dst, 1, size, f);
    if (n == 0) return feof(f) ? 0 : -1;
    return (int64_t)n;
}

static int64_t song_seek_cb(void *userdata, int64_t offset) {
    FILE *f = (FILE *)userdata;
    if (fseek(f, (long)offset, SEEK_SET) != 0) return -1;
    return (int64_t)ftell(f);
}

static int64_t song_size_cb(void *userdata) {
    FILE *f = (FILE *)userdata;
    long cur = ftell(f);
    if (cur < 0) return -1;
    fseek(f, 0, SEEK_END);
    long end = ftell(f);
    fseek(f, cur, SEEK_SET);
    return end;
}

typedef struct {
    AVFormatContext *fmt;
    AVCodecContext *dec;
    AVIOContext *avio;
    uint8_t *avio_buf;
    host_io io;
    int audio_index;
    int demux_eof;
    int decoder_eof;
    char first_error[256];
} core_session;

static void core_close(core_session *s) {
    if (s->dec) avcodec_free_context(&s->dec);
    if (s->fmt) {
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

static int core_open(core_session *s, const char *path) {
    memset(s, 0, sizeof(*s));
    s->audio_index = -1;
    s->io.f = fopen(path, "rb");
    if (!s->io.f) {
        snprintf(s->first_error, sizeof(s->first_error), "host open failed");
        return -1;
    }
    fseek(s->io.f, 0, SEEK_END);
    long fsz = ftell(s->io.f);
    fseek(s->io.f, 0, SEEK_SET);
    s->io.size = fsz;
    s->avio_buf = av_malloc(IO_BUFFER_BYTES);
    s->avio = avio_alloc_context(s->avio_buf, IO_BUFFER_BYTES, 0, &s->io,
                                 host_read_cb, NULL, host_seek_cb);
    if (!s->avio) {
        snprintf(s->first_error, sizeof(s->first_error), "avio_alloc_context failed");
        return -1;
    }
    s->fmt = avformat_alloc_context();
    if (!s->fmt) {
        snprintf(s->first_error, sizeof(s->first_error), "avformat_alloc failed");
        return -1;
    }
    s->fmt->pb = s->avio;
    s->fmt->flags |= AVFMT_FLAG_CUSTOM_IO;
    int ret = avformat_open_input(&s->fmt, "", NULL, NULL);
    if (ret < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "open_input: %d", ret);
        return ret;
    }
    return 0;
}

static int core_probe(core_session *s) {
    int ret = avformat_find_stream_info(s->fmt, NULL);
    if (ret < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "find_stream_info: %d", ret);
        return ret;
    }
    for (unsigned i = 0; i < s->fmt->nb_streams; i++) {
        if (s->fmt->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            s->audio_index = (int)i;
            break;
        }
    }
    if (s->audio_index < 0) {
        snprintf(s->first_error, sizeof(s->first_error), "no audio stream");
        return -1;
    }
    return 0;
}

static int core_dec_open(core_session *s) {
    AVStream *st = s->fmt->streams[s->audio_index];
    const AVCodec *codec = avcodec_find_decoder(st->codecpar->codec_id);
    if (!codec) {
        snprintf(s->first_error, sizeof(s->first_error), "no decoder");
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
        snprintf(s->first_error, sizeof(s->first_error), "avcodec_open2: %d", ret);
        return ret;
    }
    return 0;
}

static int core_decode_all(core_session *s, int64_t *out_samples, int64_t *out_frames) {
    AVPacket *pkt = av_packet_alloc();
    AVFrame *frm = av_frame_alloc();
    int64_t total_samples = 0, total_frames = 0;
    int stopped = 0;
    while (!stopped) {
        int ret = av_read_frame(s->fmt, pkt);
        if (ret < 0) {
            if (ret == AVERROR_EOF || avio_feof(s->fmt->pb)) s->demux_eof = 1;
            else s->io.error_count++;
            stopped = 1;
            break;
        }
        if (pkt->stream_index == s->audio_index && s->dec) {
            ret = avcodec_send_packet(s->dec, pkt);
            if (ret < 0 && ret != AVERROR(EAGAIN)) s->io.error_count++;
            while (ret >= 0) {
                ret = avcodec_receive_frame(s->dec, frm);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                if (ret < 0) { s->io.error_count++; break; }
                total_samples += frm->nb_samples;
                total_frames += 1;
                av_frame_unref(frm);
            }
        }
        av_packet_unref(pkt);
    }
    if (s->dec) {
        int ret = avcodec_send_packet(s->dec, NULL);
        if (ret >= 0) {
            while ((ret = avcodec_receive_frame(s->dec, frm)) >= 0) {
                total_samples += frm->nb_samples;
                total_frames += 1;
                av_frame_unref(frm);
            }
            if (ret == AVERROR_EOF) s->decoder_eof = 1;
        }
    }
    av_packet_free(&pkt);
    av_frame_free(&frm);
    if (out_samples) *out_samples = total_samples;
    if (out_frames) *out_frames = total_frames;
    return 0;
}

typedef struct {
    song_handle *handle;
    song_info info;
    FILE *f;
} abi_session;

static void abi_close(abi_session *s) {
    if (s->handle) song_close(s->handle);
    s->handle = NULL;
    if (s->f) fclose(s->f);
    s->f = NULL;
}

static int abi_open(abi_session *s, const char *path) {
    memset(s, 0, sizeof(*s));
    s->f = fopen(path, "rb");
    if (!s->f) return -1;
    song_io io;
    io.userdata = s->f;
    io.read = song_read_cb;
    io.seek = song_seek_cb;
    io.size = song_size_cb;
    song_status st = song_open(&io, &s->handle);
    if (st != SONG_OK) {
        fclose(s->f);
        s->f = NULL;
        return (int)st;
    }
    st = song_probe(s->handle, &s->info);
    if (st != SONG_OK) {
        song_close(s->handle);
        s->handle = NULL;
        fclose(s->f);
        s->f = NULL;
        return (int)st;
    }
    return 0;
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return x < y ? -1 : x > y ? 1 : 0;
}

static double pct_sorted(const double *a, size_t n, double q) {
    if (n == 0) return 0.0;
    double rank = ceil((q / 100.0) * (double)n);
    size_t idx = (size_t)rank;
    if (idx < 1) idx = 1;
    if (idx > n) idx = n;
    return a[idx - 1];
}

typedef struct {
    size_t n;
    double mean, min, max, stddev;
    double p50, p90, p95, p99, p999;
    int p999_ok;
} lat_stats;

static void lat_stats_compute(lat_stats *st, double *a, size_t n) {
    memset(st, 0, sizeof(*st));
    st->n = n;
    if (n == 0) return;
    double sum = 0.0;
    for (size_t i = 0; i < n; i++) sum += a[i];
    st->mean = sum / (double)n;
    qsort(a, n, sizeof(double), cmp_double);
    st->min = a[0];
    st->max = a[n - 1];
    st->p50 = pct_sorted(a, n, 50.0);
    st->p90 = pct_sorted(a, n, 90.0);
    st->p95 = pct_sorted(a, n, 95.0);
    st->p99 = pct_sorted(a, n, 99.0);
    st->p999_ok = n >= 1000;
    st->p999 = st->p999_ok ? pct_sorted(a, n, 99.9) : 0.0;
    if (n > 1) {
        double var = 0.0;
        for (size_t i = 0; i < n; i++) {
            double d = a[i] - st->mean;
            var += d * d;
        }
        st->stddev = sqrt(var / (double)(n - 1));
    }
}

static void lat_stats_print(const char *key, const lat_stats *st) {
    printf(",\"%s\":{\"count\":%zu,\"mean_us\":%.3f,\"min_us\":%.3f,\"p50_us\":%.3f,\"p90_us\":%.3f,\"p95_us\":%.3f,\"p99_us\":%.3f,",
           key, st->n, st->mean, st->min, st->p50, st->p90, st->p95, st->p99);
    if (st->p999_ok) printf("\"p999_us\":%.3f", st->p999);
    else printf("\"p999_us\":null,\"p999_flag\":\"INSUFFICIENT_SAMPLE_COUNT\"");
    printf(",\"max_us\":%.3f,\"stddev_us\":%.3f}", st->max, st->stddev);
}

static void print_info_fields(const song_info *info, int64_t pcm_frames) {
    char ce[64], ct[64];
    json_escape(info->codec, ce, sizeof(ce));
    json_escape(info->container, ct, sizeof(ct));
    printf(",\"sample_rate\":%d,\"channels\":%d,\"channel_mask\":%" PRIu64,
           info->sample_rate, info->channels, info->channel_mask);
    printf(",\"duration_us\":%" PRId64 ",\"codec\":\"%s\",\"container\":\"%s\"",
           info->duration_us, ce, ct);
    if (pcm_frames > 0)
        printf(",\"pcm_frames\":%" PRId64, pcm_frames);
}

static int mode_timer_baseline(int n) {
    if (n <= 0) n = 100000;
    double *clock_pairs = (double *)malloc(sizeof(double) * (size_t)n);
    double *abicalls = (double *)malloc(sizeof(double) * (size_t)n);
    if (!clock_pairs || !abicalls) return 1;
    for (int i = 0; i < n; i++) {
        double t0 = wall_us();
        double t1 = wall_us();
        clock_pairs[i] = t1 - t0;
    }
    for (int i = 0; i < n; i++) {
        double t0 = wall_us();
        (void)songcore_abi_version();
        double t1 = wall_us();
        abicalls[i] = t1 - t0;
    }
    lat_stats st1, st2;
    lat_stats_compute(&st1, clock_pairs, (size_t)n);
    lat_stats_compute(&st2, abicalls, (size_t)n);
    free(clock_pairs);
    free(abicalls);
    printf("{\"mode\":\"timer_baseline\",\"n\":%d", n);
    lat_stats_print("clock_pair", &st1);
    lat_stats_print("abi_version_call", &st2);
    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());
    return 0;
}

static int mode_verify(const char *path) {
    core_session cs;
    printf("{\"mode\":\"verify\",\"file\":");
    print_path(path);
    int rc = core_open(&cs, path);
    if (rc < 0 || core_probe(&cs) < 0 || core_dec_open(&cs) < 0) {
        char e[512]; json_escape(cs.first_error, e, sizeof(e));
        printf(",\"status\":\"failed\",\"layer\":\"core\",\"error\":\"%s\"}\n", e);
        core_close(&cs);
        return 1;
    }
    int64_t core_samples = 0, core_frames = 0;
    core_decode_all(&cs, &core_samples, &core_frames);
    int core_ok = (cs.demux_eof && cs.decoder_eof && cs.io.error_count == 0 &&
                   core_samples > 0);
    printf(",\"core\":{\"status\":\"%s\",\"samples\":%" PRId64
           ",\"frames\":%" PRId64 ",\"errors\":%d,\"demux_eof\":%s,\"decoder_eof\":%s}",
           core_ok ? "ok" : "failed", core_samples, core_frames, cs.io.error_count,
           cs.demux_eof ? "true" : "false", cs.decoder_eof ? "true" : "false");
    core_close(&cs);

    abi_session as;
    int st = abi_open(&as, path);
    if (st != 0) {
        printf(",\"status\":\"failed\",\"layer\":\"abi\",\"error_code\":%d}\n", st);
        return 1;
    }
    song_info info = as.info;
    uint64_t ch = (uint64_t)info.channels;
    size_t cap_frames = 8192;
    float *dst = (float *)malloc(sizeof(float) * cap_frames * ch);
    if (!dst) {
        printf(",\"status\":\"failed\",\"layer\":\"abi\",\"error\":\"oom\"}\n");
        abi_close(&as);
        return 1;
    }
    sha256_ctx sha;
    sha256_init(&sha);
    int64_t total_frames = 0, calls = 0;
    int terminal = 0;
    for (;;) {
        uint64_t got = 0;
        song_status rs = song_read_pcm(as.handle, dst, cap_frames, &got);
        calls++;
        if (rs == SONG_EOF) { terminal = 1; break; }
        if (rs != SONG_OK) { terminal = (int)rs; break; }
        sha256_update(&sha, dst, (size_t)got * ch * sizeof(float));
        total_frames += (int64_t)got;
    }
    char hex[65];
    sha256_hex(&sha, hex);
    free(dst);
    abi_close(&as);
    printf(",\"abi\":{\"status\":\"%s\",\"terminal\":%d,\"frames\":%" PRId64
           ",\"calls\":%" PRId64 ",\"pcm_sha256\":\"%s\"",
           terminal == 1 ? "ok" : "failed", terminal, total_frames, calls, hex);
    print_info_fields(&info, total_frames);
    printf(",\"abi_frames_eq_core_samples\":%s}",
           total_frames == core_samples ? "true" : "false");
    printf(",\"status\":\"%s\",\"peak_rss_kb\":%ld}\n",
           (terminal == 1 && core_ok && total_frames == core_samples) ? "ok" : "failed",
           peak_rss_kb());
    return (terminal == 1 && core_ok && total_frames == core_samples) ? 0 : 1;
}

static int mode_startup_abi(const char *path, int iters) {
    if (iters <= 0) iters = 20;
    double *open_us = malloc(sizeof(double) * (size_t)iters);
    double *probe_us = malloc(sizeof(double) * (size_t)iters);
    double *first_read_us = malloc(sizeof(double) * (size_t)iters);
    double *ttfp_us = malloc(sizeof(double) * (size_t)iters);
    if (!open_us || !probe_us || !first_read_us || !ttfp_us) return 1;
    int silent_hits = 0;
    int failures = 0;
    song_info info;
    memset(&info, 0, sizeof(info));
    for (int it = 0; it < iters && failures == 0; it++) {
        abi_session s;
        s.handle = NULL;
        s.f = NULL;
        double t0 = wall_us();
        s.f = fopen(path, "rb");
        if (!s.f) { failures++; break; }
        song_io io;
        io.userdata = s.f;
        io.read = song_read_cb;
        io.seek = song_seek_cb;
        io.size = song_size_cb;
        song_status st = song_open(&io, &s.handle);
        double t1 = wall_us();
        if (st != SONG_OK) { failures++; abi_close(&s); break; }
        st = song_probe(s.handle, &s.info);
        double t2 = wall_us();
        if (st != SONG_OK) { failures++; abi_close(&s); break; }
        info = s.info;
        uint64_t ch = (uint64_t)s.info.channels;
        size_t cap = 4096;
        float *dst = (float *)malloc(sizeof(float) * cap * ch);
        if (!dst) { failures++; abi_close(&s); break; }
        uint64_t got = 0;
        st = song_read_pcm(s.handle, dst, cap, &got);
        double t3 = wall_us();
        int silent = 1;
        for (uint64_t i = 0; i < got * ch; i++) {
            if (dst[i] != 0.0f) { silent = 0; break; }
        }
        if (!silent) silent_hits++;
        free(dst);
        open_us[it] = t1 - t0;
        probe_us[it] = t2 - t1;
        first_read_us[it] = t3 - t2;
        ttfp_us[it] = t3 - t0;
        abi_close(&s);
    }
    printf("{\"mode\":\"startup_abi\",\"file\":");
    print_path(path);
    if (failures > 0) {
        printf(",\"status\":\"failed\",\"failures\":%d}\n", failures);
        free(open_us); free(probe_us); free(first_read_us); free(ttfp_us);
        return 1;
    }
    lat_stats s1, s2, s3, s4;
    lat_stats_compute(&s1, open_us, (size_t)iters);
    lat_stats_compute(&s2, probe_us, (size_t)iters);
    lat_stats_compute(&s3, first_read_us, (size_t)iters);
    lat_stats_compute(&s4, ttfp_us, (size_t)iters);
    free(open_us); free(probe_us); free(first_read_us); free(ttfp_us);
    printf(",\"status\":\"ok\",\"iterations\":%d", iters);
    print_info_fields(&info, -1);
    printf(",\"first_read_cap_frames\":4096,\"first_buffer_silent_count\":%d",
           iters - silent_hits);
    lat_stats_print("open", &s1);
    lat_stats_print("probe", &s2);
    lat_stats_print("first_read", &s3);
    lat_stats_print("ttfp", &s4);
    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());
    return 0;
}

static int mode_startup_core(const char *path, int iters) {
    if (iters <= 0) iters = 20;
    double *open_us = malloc(sizeof(double) * (size_t)iters);
    double *probe_us = malloc(sizeof(double) * (size_t)iters);
    double *dec_open_us = malloc(sizeof(double) * (size_t)iters);
    double *first_frame_us = malloc(sizeof(double) * (size_t)iters);
    if (!open_us || !probe_us || !dec_open_us || !first_frame_us) return 1;
    int failures = 0;
    int sample_rate = 0, channels = 0;
    char codec_name[64] = "";
    for (int it = 0; it < iters && failures == 0; it++) {
        core_session s;
        double t0 = wall_us();
        if (core_open(&s, path) < 0) { failures++; core_close(&s); break; }
        double t1 = wall_us();
        if (core_probe(&s) < 0) { failures++; core_close(&s); break; }
        double t2 = wall_us();
        if (core_dec_open(&s) < 0) { failures++; core_close(&s); break; }
        double t3 = wall_us();
        AVPacket *pkt = av_packet_alloc();
        AVFrame *frm = av_frame_alloc();
        int64_t first_samples = -1;
        for (;;) {
            int ret = av_read_frame(s.fmt, pkt);
            if (ret < 0) break;
            if (pkt->stream_index != s.audio_index) { av_packet_unref(pkt); continue; }
            ret = avcodec_send_packet(s.dec, pkt);
            av_packet_unref(pkt);
            if (ret < 0 && ret != AVERROR(EAGAIN)) break;
            ret = avcodec_receive_frame(s.dec, frm);
            if (ret >= 0) {
                first_samples = frm->nb_samples;
                break;
            }
            if (ret != AVERROR(EAGAIN)) break;
        }
        av_packet_free(&pkt);
        av_frame_free(&frm);
        double t4 = wall_us();
        if (first_samples < 0) { failures++; core_close(&s); break; }
        AVStream *st = s.fmt->streams[s.audio_index];
        sample_rate = st->codecpar->sample_rate;
        channels = st->codecpar->ch_layout.nb_channels;
        snprintf(codec_name, sizeof(codec_name), "%s",
                 avcodec_get_name(st->codecpar->codec_id));
        open_us[it] = t1 - t0;
        probe_us[it] = t2 - t1;
        dec_open_us[it] = t3 - t2;
        first_frame_us[it] = t4 - t3;
        core_close(&s);
    }
    printf("{\"mode\":\"startup_core\",\"file\":");
    print_path(path);
    if (failures > 0) {
        printf(",\"status\":\"failed\",\"failures\":%d}\n", failures);
        free(open_us); free(probe_us); free(dec_open_us); free(first_frame_us);
        return 1;
    }
    lat_stats s1, s2, s3, s4;
    lat_stats_compute(&s1, open_us, (size_t)iters);
    lat_stats_compute(&s2, probe_us, (size_t)iters);
    lat_stats_compute(&s3, dec_open_us, (size_t)iters);
    lat_stats_compute(&s4, first_frame_us, (size_t)iters);
    free(open_us); free(probe_us); free(dec_open_us); free(first_frame_us);
    char ce[64];
    json_escape(codec_name, ce, sizeof(ce));
    printf(",\"status\":\"ok\",\"iterations\":%d", iters);
    printf(",\"sample_rate\":%d,\"channels\":%d,\"codec\":\"%s\"", sample_rate, channels, ce);
    lat_stats_print("open", &s1);
    lat_stats_print("probe", &s2);
    lat_stats_print("decoder_open", &s3);
    lat_stats_print("first_frame", &s4);
    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());
    return 0;
}

static int core_run_once(const char *path, double *wall, double *cpu,
                         int64_t *samples, int64_t *frames, int *errors,
                         int *demux_eof, int *decoder_eof) {
    core_session s;
    if (core_open(&s, path) < 0 || core_probe(&s) < 0 || core_dec_open(&s) < 0) {
        fprintf(stderr, "core session failed: %s\n", s.first_error);
        core_close(&s);
        return -1;
    }
    double t0 = wall_us();
    double c0 = cpu_us();
    core_decode_all(&s, samples, frames);
    double c1 = cpu_us();
    double t1 = wall_us();
    *wall = t1 - t0;
    *cpu = c1 - c0;
    *errors = s.io.error_count;
    *demux_eof = s.demux_eof;
    *decoder_eof = s.decoder_eof;
    core_close(&s);
    return 0;
}

static int abi_run_once(abi_session *s, float *dst, size_t cap_frames,
                        double *wall, double *cpu, int64_t *frames,
                        int64_t *calls, int *terminal) {
    double t0 = wall_us();
    double c0 = cpu_us();
    int64_t total = 0;
    int64_t ncalls = 0;
    int term = 0;
    for (;;) {
        uint64_t got = 0;
        song_status rs = song_read_pcm(s->handle, dst, cap_frames, &got);
        ncalls++;
        if (rs == SONG_EOF) { term = 1; break; }
        if (rs != SONG_OK) { term = (int)rs; break; }
        total += (int64_t)got;
    }
    double c1 = cpu_us();
    double t1 = wall_us();
    *wall = t1 - t0;
    *cpu = c1 - c0;
    *frames = total;
    *calls = ncalls;
    *terminal = term;
    return 0;
}

typedef struct {
    double wall;
    double cpu;
    int64_t samples;
    int64_t frames;
    int errors;
    int demux_eof;
    int decoder_eof;
} core_iter;

typedef struct {
    double wall;
    double cpu;
    int64_t frames;
    int64_t calls;
} abi_iter;

static int abi_iter_once(const char *path, float *dst, size_t block, abi_iter *out) {
    abi_session s;
    if (abi_open(&s, path) != 0) return -1;
    int term;
    abi_run_once(&s, dst, block, &out->wall, &out->cpu, &out->frames, &out->calls, &term);
    abi_close(&s);
    if (term != 1) return -1;
    return 0;
}

static int mode_throughput(const char *path, int warmup, int iters, size_t block) {
    if (warmup <= 0) warmup = 3;
    if (iters <= 0) iters = 20;
    if (block == 0) block = 4096;
    abi_session as;
    if (abi_open(&as, path) != 0) {
        printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"abi open failed\"}\n");
        return 1;
    }
    song_info info = as.info;
    uint64_t ch = (uint64_t)info.channels;
    abi_close(&as);
    float *dst = (float *)malloc(sizeof(float) * block * ch);
    if (!dst) {
        printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"oom\"}\n");
        return 1;
    }
    for (int i = 0; i < warmup; i++) {
        core_session cs;
        if (core_open(&cs, path) < 0 || core_probe(&cs) < 0 || core_dec_open(&cs) < 0) {
            fprintf(stderr, "core warmup failed: %s\n", cs.first_error);
            core_close(&cs);
            free(dst);
            printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"core warmup\"}\n");
            return 1;
        }
        int64_t sm, fr;
        core_decode_all(&cs, &sm, &fr);
        core_close(&cs);
        abi_session w;
        if (abi_open(&w, path) != 0) {
            free(dst);
            printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"abi warmup\"}\n");
            return 1;
        }
        double cw, ww;
        int64_t f, ca;
        int term;
        abi_run_once(&w, dst, block, &ww, &cw, &f, &ca, &term);
        abi_close(&w);
        if (term != 1) {
            free(dst);
            printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"abi warmup terminal\"}\n");
            return 1;
        }
    }
    core_iter *cit = (core_iter *)malloc(sizeof(core_iter) * iters);
    abi_iter *ait = (abi_iter *)malloc(sizeof(abi_iter) * iters);
    if (!cit || !ait) {
        free(dst);
        free(cit);
        free(ait);
        printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"oom\"}\n");
        return 1;
    }
    for (int i = 0; i < iters; i++) {
        int rc = 0;
        if ((i & 1) == 0) {
            if (core_run_once(path, &cit[i].wall, &cit[i].cpu, &cit[i].samples,
                              &cit[i].frames, &cit[i].errors, &cit[i].demux_eof,
                              &cit[i].decoder_eof) < 0)
                rc = 1;
            else if (abi_iter_once(path, dst, block, &ait[i]) < 0)
                rc = 2;
        } else {
            if (abi_iter_once(path, dst, block, &ait[i]) < 0)
                rc = 2;
            else if (core_run_once(path, &cit[i].wall, &cit[i].cpu, &cit[i].samples,
                                   &cit[i].frames, &cit[i].errors, &cit[i].demux_eof,
                                   &cit[i].decoder_eof) < 0)
                rc = 1;
        }
        if (rc) {
            free(dst);
            free(cit);
            free(ait);
            printf("{\"mode\":\"throughput\",\"status\":\"failed\",\"error\":\"%s\"}\n",
                   rc == 1 ? "core iter" : "abi iter");
            return 1;
        }
    }
    printf("{\"mode\":\"throughput\",\"file\":");
    print_path(path);
    printf(",\"status\":\"ok\",\"warmup\":%d,\"iterations\":%d,\"block_frames\":%zu"
           ",\"interleave\":\"abab\"",
           warmup, iters, block);
    print_info_fields(&info, -1);
    printf(",\"core\":{\"iters\":[");
    for (int i = 0; i < iters; i++) {
        printf("%s{\"wall_us\":%.3f,\"cpu_us\":%.3f,\"samples\":%" PRId64
               ",\"frames\":%" PRId64 ",\"errors\":%d,\"demux_eof\":%s,\"decoder_eof\":%s}",
               i ? "," : "", cit[i].wall, cit[i].cpu, cit[i].samples, cit[i].frames,
               cit[i].errors, cit[i].demux_eof ? "true" : "false",
               cit[i].decoder_eof ? "true" : "false");
    }
    printf("]},\"abi\":{\"iters\":[");
    for (int i = 0; i < iters; i++) {
        printf("%s{\"wall_us\":%.3f,\"cpu_us\":%.3f,\"frames\":%" PRId64
               ",\"calls\":%" PRId64 "}", i ? "," : "", ait[i].wall, ait[i].cpu,
               ait[i].frames, ait[i].calls);
    }
    printf("]}");
    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());
    free(dst);
    free(cit);
    free(ait);
    return 0;
}

static int read_latency_pass(abi_session *s, float *dst, size_t block,
                             double **samples_io, size_t *n_io,
                             double *wall_out, int64_t *frames_out) {
    size_t cap = 65536;
    size_t n = 0;
    double *lat = (double *)malloc(sizeof(double) * cap);
    if (!lat) return -1;
    int64_t total = 0;
    double t0 = wall_us();
    for (;;) {
        uint64_t got = 0;
        double c0 = wall_us();
        song_status rs = song_read_pcm(s->handle, dst, block, &got);
        double c1 = wall_us();
        if (n == cap) {
            cap *= 2;
            double *nl = (double *)realloc(lat, sizeof(double) * cap);
            if (!nl) { free(lat); return -1; }
            lat = nl;
        }
        lat[n++] = c1 - c0;
        if (rs == SONG_EOF) break;
        if (rs != SONG_OK) { free(lat); return (int)rs; }
        total += (int64_t)got;
    }
    double t1 = wall_us();
    *samples_io = lat;
    *n_io = n;
    *wall_out = t1 - t0;
    *frames_out = total;
    return 0;
}

static int mode_read_latency(const char *path, int warmup, int passes,
                             const char *blocks_arg, const char *raw_dir) {
    if (warmup <= 0) warmup = 2;
    if (passes <= 0) passes = 3;
    abi_session as;
    if (abi_open(&as, path) != 0) {
        printf("{\"mode\":\"read_latency\",\"status\":\"failed\",\"error\":\"abi open failed\"}\n");
        return 1;
    }
    song_info info = as.info;
    uint64_t ch = (uint64_t)info.channels;
    abi_close(&as);

    unsigned blocks[16];
    size_t nblocks = 0;
    const char *p = blocks_arg;
    while (*p && nblocks < 16) {
        unsigned v = 0;
        while (*p >= '0' && *p <= '9') { v = v * 10u + (unsigned)(*p - '0'); p++; }
        blocks[nblocks++] = v;
        if (*p == ',') p++;
        else break;
    }
    size_t max_block = 0;
    for (size_t i = 0; i < nblocks; i++)
        if (blocks[i] > max_block) max_block = blocks[i];
    if (max_block == 0) {
        printf("{\"mode\":\"read_latency\",\"status\":\"failed\",\"error\":\"bad block list\"}\n");
        return 1;
    }
    float *dst = (float *)malloc(sizeof(float) * max_block * ch);
    if (!dst) {
        printf("{\"mode\":\"read_latency\",\"status\":\"failed\",\"error\":\"oom\"}\n");
        return 1;
    }

    for (int i = 0; i < warmup; i++) {
        abi_session w;
        if (abi_open(&w, path) != 0) {
            printf("{\"mode\":\"read_latency\",\"status\":\"failed\",\"error\":\"warmup open failed\"}\n");
            free(dst);
            return 1;
        }
        double wall, cw;
        int64_t frames, calls;
        int term;
        abi_run_once(&w, dst, max_block, &wall, &cw, &calls, &frames, &term);
        abi_close(&w);
        if (term != 1) {
            free(dst);
            printf("{\"mode\":\"read_latency\",\"status\":\"failed\",\"error\":\"warmup terminal\"}\n");
            return 1;
        }
    }

    printf("{\"mode\":\"read_latency\",\"file\":");
    print_path(path);
    printf(",\"status\":\"ok\",\"warmup\":%d,\"passes\":%d,\"blocks\":[", warmup, passes);
    for (size_t i = 0; i < nblocks; i++)
        printf("%s%u", i ? "," : "", blocks[i]);
    printf("]");
    print_info_fields(&info, -1);
    printf(",\"per_block\":[");
    for (size_t bi = 0; bi < nblocks; bi++) {
        unsigned block = blocks[bi];
        double *all = NULL;
        size_t nall = 0;
        size_t calls_per_pass = 0;
        double pass_wall_sum = 0.0;
        int64_t pass_frames_sum = 0;
        for (int pi = 0; pi < passes; pi++) {
            abi_session s2;
            if (abi_open(&s2, path) != 0) {
                free(all);
                printf("]}\n");
                free(dst);
                return 1;
            }
            double *lat = NULL;
            size_t n = 0;
            double wall = 0.0;
            int64_t frames = 0;
            int rc = read_latency_pass(&s2, dst, block, &lat, &n, &wall, &frames);
            abi_close(&s2);
            if (rc != 0) {
                free(lat);
                free(all);
                printf("]}\n");
                free(dst);
                return 1;
            }
            double *na = (double *)realloc(all, sizeof(double) * (nall + n));
            if (!na) { free(lat); free(all); printf("]}\n"); free(dst); return 1; }
            all = na;
            memcpy(all + nall, lat, sizeof(double) * n);
            nall += n;
            calls_per_pass = n;
            pass_wall_sum += wall;
            pass_frames_sum += frames;
            free(lat);
        }
        lat_stats st;
        lat_stats_compute(&st, all, nall);
        free(all);
        double batch_wall = 0.0, batch_cpu = 0.0;
        int64_t batch_frames = 0, batch_calls = 0;
        int batch_term = 0;
        abi_session s3;
        if (abi_open(&s3, path) == 0) {
            abi_run_once(&s3, dst, block, &batch_wall, &batch_cpu,
                         &batch_frames, &batch_calls, &batch_term);
            abi_close(&s3);
        }
        if (bi) printf(",");
        printf("{\"block_frames\":%u,\"pool_count\":%zu,\"calls_per_pass\":%zu"
               ",\"frames_per_pass\":%" PRId64,
               block, st.n, calls_per_pass, pass_frames_sum / passes);
        lat_stats_print("read_call", &st);
        printf(",\"latency_run_wall_us\":%.3f,\"batch_wall_us\":%.3f"
               ",\"batch_cpu_us\":%.3f,\"batch_mean_call_us\":%.3f"
               ",\"batch_frames\":%" PRId64 ",\"batch_calls\":%" PRId64
               ",\"batch_terminal\":%d}",
               pass_wall_sum / (double)passes,
               batch_wall,
               batch_cpu,
               batch_calls > 0 ? batch_wall / (double)batch_calls : 0.0,
               batch_frames, batch_calls, batch_term);
        if (raw_dir) {
            abi_session s4;
            if (abi_open(&s4, path) == 0) {
                char rawpath[1024];
                snprintf(rawpath, sizeof(rawpath), "%s/block%u.latency.raw",
                         raw_dir, block);
                FILE *rf = fopen(rawpath, "w");
                if (rf) {
                    double *lat = NULL;
                    size_t n = 0;
                    double wall = 0.0;
                    int64_t frames = 0;
                    if (read_latency_pass(&s4, dst, block, &lat, &n, &wall, &frames) == 0) {
                        for (size_t i = 0; i < n; i++)
                            fprintf(rf, "%.1f\n", lat[i]);
                    }
                    free(lat);
                    fclose(rf);
                }
                abi_close(&s4);
            }
        }
    }
    printf("]");
    printf(",\"peak_rss_kb\":%ld}\n", peak_rss_kb());
    free(dst);
    return 0;
}

int main(int argc, char **argv) {
    av_log_set_level(AV_LOG_ERROR);
    if (argc < 2) {
        fprintf(stderr,
                "usage: %s timer-baseline [n]\n"
                "       %s verify <file>\n"
                "       %s startup-abi <file> <iters>\n"
                "       %s startup-core <file> <iters>\n"
                "       %s throughput <file> <warmup> <iters> <block>\n"
                "       %s read-latency <file> <warmup> <passes> <blocks> [raw_dir]\n",
                argv[0], argv[0], argv[0], argv[0], argv[0], argv[0]);
        return 2;
    }
    if (strcmp(argv[1], "timer-baseline") == 0)
        return mode_timer_baseline(argc > 2 ? atoi(argv[2]) : 100000);
    if (strcmp(argv[1], "verify") == 0 && argc > 2)
        return mode_verify(argv[2]);
    if (strcmp(argv[1], "startup-abi") == 0 && argc > 3)
        return mode_startup_abi(argv[2], atoi(argv[3]));
    if (strcmp(argv[1], "startup-core") == 0 && argc > 3)
        return mode_startup_core(argv[2], atoi(argv[3]));
    if (strcmp(argv[1], "throughput") == 0 && argc > 5)
        return mode_throughput(argv[2], atoi(argv[3]), atoi(argv[4]),
                               (size_t)strtoul(argv[5], NULL, 10));
    if (strcmp(argv[1], "read-latency") == 0 && argc > 5)
        return mode_read_latency(argv[2], atoi(argv[3]), atoi(argv[4]), argv[5],
                                 argc > 6 ? argv[6] : NULL);
    fprintf(stderr, "unknown mode or missing arguments\n");
    return 2;
}
