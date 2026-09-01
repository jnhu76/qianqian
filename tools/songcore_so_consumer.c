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
    song_handle *h = NULL;
    if (song_open(&sio, &h) != SONG_OK || !h) {
        printf("{\"status\":\"song_open_failed\"}\n");
        fclose(io.f);
        return 1;
    }

    song_info info;
    memset(&info, 0, sizeof(info));
    if (song_probe(h, &info) != SONG_OK) {
        printf("{\"status\":\"probe_failed\"}\n");
        song_close(h);
        fclose(io.f);
        return 1;
    }

    /* bounded first slice: one second of frames */
    size_t cap = (size_t)(info.sample_rate > 0 ? info.sample_rate : 48000);
    float *buf = malloc(sizeof(float) * cap * (size_t)(info.channels > 0 ? info.channels : 1));
    uint64_t frames_a = 0;
    song_status st_a = song_read_pcm(h, buf, cap, &frames_a);

    int64_t actual = -1;
    song_status seeked = song_seek(h, 0, &actual);
    uint64_t frames_b = 0;
    song_status st_b = seeked == SONG_OK ? song_read_pcm(h, buf, cap, &frames_b) : SONG_ERR_STATE;

    printf("{\"status\":\"ok\",\"codec\":\"%s\",\"sample_rate\":%d,\"channels\":%d,"
           "\"first_read_frames\":%llu,\"seek_rc\":%d,\"post_seek_read_frames\":%llu}\n",
           info.codec, info.sample_rate, info.channels,
           (unsigned long long)frames_a, (int)seeked, (unsigned long long)frames_b);

    free(buf);
    song_close(h);
    fclose(io.f);
    return (st_a == SONG_OK && frames_a > 0 && seeked == SONG_OK && st_b == SONG_OK &&
            frames_b > 0)
               ? 0
               : 1;
}
