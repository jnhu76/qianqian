/*
 * songcore_so_consumer — functional smoke for libqianqian_songcore.so.
 *
 * Experiment fixture (E07/S6), NOT a product tool. It links against the
 * version-scripted shared library and drives the full contract the way a
 * host application would: stdio-backed song_io, open, probe, decode a
 * bounded slice, seek, decode again, close. Prints one JSON line.
 */
#include "songcore.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    FILE *f;
} file_io;

static int64_t io_read(void *userdata, uint8_t *dst, size_t size) {
    file_io *io = userdata;
    size_t n = fread(dst, 1, size, io->f);
    if (n == 0) return ferror(io->f) ? -1 : 0;
    return (int64_t)n;
}

static int64_t io_seek(void *userdata, int64_t absolute_offset) {
    file_io *io = userdata;
    if (fseeko(io->f, (off_t)absolute_offset, SEEK_SET) != 0) return -1;
    return (int64_t)ftello(io->f);
}

static int64_t io_size(void *userdata) {
    file_io *io = userdata;
    long cur = ftell(io->f);
    if (cur < 0) return -1;
    if (fseek(io->f, 0, SEEK_END) != 0) return -1;
    long end = ftell(io->f);
    if (fseek(io->f, cur, SEEK_SET) != 0) return -1;
    return (int64_t)end;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <local-song>\n", argv[0]);
        return 2;
    }
    file_io io = { .f = fopen(argv[1], "rb") };
    if (!io.f) {
        printf("{\"status\":\"open_failed\"}\n");
        return 1;
    }

    song_io sio = { .userdata = &io, .read = io_read, .seek = io_seek, .size = io_size };
    song_handle *h = song_open(&sio);
    if (!h) {
        printf("{\"status\":\"song_open_failed\"}\n");
        fclose(io.f);
        return 1;
    }

    song_info info;
    memset(&info, 0, sizeof(info));
    if (song_probe(h, &info) != 0) {
        printf("{\"status\":\"probe_failed\"}\n");
        song_close(h);
        fclose(io.f);
        return 1;
    }

    /* bounded first slice: one second of frames */
    size_t cap = (size_t)(info.sample_rate > 0 ? info.sample_rate : 48000);
    float *buf = malloc(sizeof(float) * cap * (size_t)(info.channels > 0 ? info.channels : 1));
    int64_t frames_a = song_read_pcm(h, buf, cap);

    int seeked = song_seek(h, 0);
    int64_t frames_b = seeked == 0 ? song_read_pcm(h, buf, cap) : -1;

    printf("{\"status\":\"ok\",\"codec\":\"%s\",\"sample_rate\":%d,\"channels\":%d,"
           "\"first_read_frames\":%lld,\"seek_rc\":%d,\"post_seek_read_frames\":%lld}\n",
           info.codec, info.sample_rate, info.channels,
           (long long)frames_a, seeked, (long long)frames_b);

    free(buf);
    song_close(h);
    fclose(io.f);
    return (frames_a > 0 && seeked == 0 && frames_b > 0) ? 0 : 1;
}
