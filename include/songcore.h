#ifndef QIANQIAN_SONGCORE_H
#define QIANQIAN_SONGCORE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct song_handle song_handle;

typedef int64_t (*song_read_fn)(
    void* userdata,
    uint8_t* dst,
    size_t size
);

typedef int64_t (*song_seek_fn)(
    void* userdata,
    int64_t absolute_offset
);

typedef int64_t (*song_size_fn)(
    void* userdata
);

typedef struct song_io {
    void* userdata;
    song_read_fn read;
    song_seek_fn seek;
    song_size_fn size;
} song_io;

typedef struct song_info {
    int32_t sample_rate;
    int32_t channels;
    int64_t duration_us;
    int32_t bits_per_sample;
    char codec[32];
    char container[32];
} song_info;

/*
 * Experimental ABI sketch.
 * This header is intentionally tiny and is NOT frozen yet.
 */

song_handle* song_open(const song_io* io);
int song_probe(song_handle* handle, song_info* out_info);

/* Interleaved Float32 PCM. Returns frames produced, 0 for EOF, <0 for error. */
int64_t song_read_pcm(
    song_handle* handle,
    float* output,
    size_t frame_capacity
);

int song_seek(song_handle* handle, int64_t position_us);
void song_close(song_handle* handle);

#ifdef __cplusplus
}
#endif

#endif
