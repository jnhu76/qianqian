/*
 * qn_pcm_dump — the thinnest native SongCore host used by audible smoke tests.
 *
 * It owns file IO, feeds SongCore, and writes one tiny self-describing stream:
 *   "QPCM" | u32le sample_rate | u16le channels | u16le format(1=f32le)
 *   followed by interleaved Float32 PCM.
 *
 * It does NOT know FFmpeg types and it does NOT own an audio device.
 */
#if !defined(_WIN32)
#define _FILE_OFFSET_BITS 64
#define _POSIX_C_SOURCE 200809L
#endif

#include "songcore.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if !defined(_WIN32)
#include <sys/types.h>
#endif

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#endif

typedef struct file_source {
    FILE *file;
    int64_t size;
} file_source;

#if defined(_WIN32)
static int file_seek64(FILE *f, int64_t off, int whence) { return _fseeki64(f, off, whence); }
static int64_t file_tell64(FILE *f) { return _ftelli64(f); }
#else
static int file_seek64(FILE *f, int64_t off, int whence) { return fseeko(f, (off_t)off, whence); }
static int64_t file_tell64(FILE *f) { return (int64_t)ftello(f); }
#endif

static int64_t host_read(void *userdata, uint8_t *dst, size_t size) {
    file_source *src = (file_source *)userdata;
    size_t n = fread(dst, 1, size, src->file);
    if (n == 0 && ferror(src->file)) return -1;
    return (int64_t)n;
}

static int64_t host_seek(void *userdata, int64_t absolute_offset) {
    file_source *src = (file_source *)userdata;
    if (absolute_offset < 0 || file_seek64(src->file, absolute_offset, SEEK_SET) != 0) return -1;
    return file_tell64(src->file);
}

static int64_t host_size(void *userdata) {
    return ((file_source *)userdata)->size;
}

static int write_header(const song_info *info) {
    if (info->sample_rate <= 0 || info->channels <= 0 || info->channels > UINT16_MAX) return -1;
    uint8_t h[12] = {
        'Q', 'P', 'C', 'M',
        (uint8_t)(info->sample_rate),
        (uint8_t)(info->sample_rate >> 8),
        (uint8_t)(info->sample_rate >> 16),
        (uint8_t)(info->sample_rate >> 24),
        (uint8_t)(info->channels),
        (uint8_t)(info->channels >> 8),
        1, 0 /* format 1 = Float32 little-endian */
    };
    return fwrite(h, 1, sizeof(h), stdout) == sizeof(h) ? 0 : -1;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <local-song.mp3|flac>\n", argv[0]);
        return 2;
    }

    const uint16_t endian_probe = 1;
    if (*(const uint8_t *)&endian_probe != 1) {
        fprintf(stderr, "qn_pcm_dump: f32le smoke transport requires a little-endian host\n");
        return 2;
    }
#if defined(_WIN32)
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    file_source src = {0};
    src.file = fopen(argv[1], "rb");
    if (!src.file) {
        fprintf(stderr, "qn_pcm_dump: cannot open %s\n", argv[1]);
        return 1;
    }
    if (file_seek64(src.file, 0, SEEK_END) != 0 || (src.size = file_tell64(src.file)) < 0 ||
        file_seek64(src.file, 0, SEEK_SET) != 0) {
        fprintf(stderr, "qn_pcm_dump: cannot size input\n");
        fclose(src.file);
        return 1;
    }

    song_io io = {
        .userdata = &src,
        .read = host_read,
        .seek = host_seek,
        .size = host_size,
    };
    song_handle *song = song_open(&io);
    if (!song) {
        fprintf(stderr, "qn_pcm_dump: song_open failed\n");
        fclose(src.file);
        return 1;
    }

    song_info info;
    if (song_probe(song, &info) < 0) {
        fprintf(stderr, "qn_pcm_dump: song_probe failed\n");
        song_close(song);
        fclose(src.file);
        return 1;
    }
    fprintf(stderr, "qn_pcm_dump: %s/%s %d Hz %d ch\n",
            info.container, info.codec, info.sample_rate, info.channels);

    if (write_header(&info) < 0) {
        song_close(song);
        fclose(src.file);
        return 1;
    }

    const size_t frames_per_chunk = 4096;
    if ((size_t)info.channels > SIZE_MAX / frames_per_chunk / sizeof(float)) {
        song_close(song);
        fclose(src.file);
        return 1;
    }
    float *pcm = (float *)malloc(frames_per_chunk * (size_t)info.channels * sizeof(float));
    if (!pcm) {
        song_close(song);
        fclose(src.file);
        return 1;
    }

    int rc = 0;
    for (;;) {
        int64_t frames = song_read_pcm(song, pcm, frames_per_chunk);
        if (frames < 0) {
            fprintf(stderr, "qn_pcm_dump: decode failed\n");
            rc = 1;
            break;
        }
        if (frames == 0) break;
        size_t samples = (size_t)frames * (size_t)info.channels;
        if (fwrite(pcm, sizeof(float), samples, stdout) != samples) {
            rc = 1;
            break;
        }
    }

    free(pcm);
    song_close(song);
    fclose(src.file);
    if (fflush(stdout) != 0) rc = 1;
    return rc;
}
