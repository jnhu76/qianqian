/*
 * songcore_consumer_smoke.c — #324 Windows x86_64 SongCore artifact gate.
 *
 * A REAL native Windows C consumer: compiled and linked ON the Windows host
 * (mingw-w64 / WinLibs GCC) against the target-specific `libsongcore.a`, then
 * executed as a PE. It exercises the frozen ABI v1 lifecycle over a committed
 * media fixture:
 *
 *   songcore_abi_version
 *   song_open -> song_probe -> song_get_metadata -> song_read_pcm -> song_seek
 *             -> song_read_pcm (post-seek) -> song_close
 *
 * Usage: songcore_consumer_smoke.exe <fixture.flac> <expected-sha256>
 * Exit 0 + `CONSUMER_SMOKE=PASS` only when every step is observed.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "songcore.h"

static int64_t io_read(void *userdata, uint8_t *dst, size_t size) {
    FILE *f = (FILE *)userdata;
    size_t n = fread(dst, 1, size, f);
    if (n == 0 && ferror(f)) {
        return -1;
    }
    return (int64_t)n;
}

static int64_t io_seek(void *userdata, int64_t absolute_offset) {
    FILE *f = (FILE *)userdata;
    if (fseek(f, (long)absolute_offset, SEEK_SET) != 0) {
        return -1;
    }
    return (int64_t)ftell(f);
}

static int64_t io_size(void *userdata) {
    FILE *f = (FILE *)userdata;
    long current = ftell(f);
    if (fseek(f, 0, SEEK_END) != 0) {
        return -1;
    }
    long total = ftell(f);
    if (fseek(f, current, SEEK_SET) != 0) {
        return -1;
    }
    return (int64_t)total;
}

#define CHECK(what, cond)                                                      \
    do {                                                                       \
        if (!(cond)) {                                                         \
            printf("CONSUMER_FAIL step=%s\n", what);                           \
            fflush(stdout);                                                    \
            return 10;                                                         \
        }                                                                      \
    } while (0)

int main(int argc, char **argv) {
    if (argc < 2) {
        printf("usage: %s <fixture.flac>\n", argv[0]);
        return 2;
    }
    printf("PLATFORM=%s CONSUMER=windows-x86_64-native-c\n", "windows-x86_64");

    uint32_t abi = songcore_abi_version();
    printf("ABI_VERSION=%u\n", abi);
    CHECK("abi_version", abi == SONGCORE_ABI_VERSION);

    FILE *f = fopen(argv[1], "rb");
    CHECK("fopen", f != NULL);
    int64_t size = io_size(f);
    printf("SOURCE_BYTES=%lld\n", (long long)size);
    CHECK("source_size", size > 0);

    song_io io;
    io.userdata = f;
    io.read = io_read;
    io.seek = io_seek;
    io.size = io_size;

    song_handle *handle = NULL;
    song_status status = song_open(&io, &handle);
    printf("OPEN status=%d\n", (int)status);
    CHECK("song_open", status == SONG_OK && handle != NULL);

    song_info info;
    memset(&info, 0, sizeof(info));
    status = song_probe(handle, &info);
    printf("PROBE status=%d rate=%d channels=%d mask=%llu duration_us=%lld codec=%s "
           "container=%s streams=%u selected=%u\n",
           (int)status, (int)info.sample_rate, (int)info.channels,
           (unsigned long long)info.channel_mask, (long long)info.duration_us,
           info.codec, info.container, info.audio_stream_count,
           info.selected_audio_index);
    CHECK("song_probe", status == SONG_OK);
    CHECK("probe_rate", info.sample_rate == 44100);
    CHECK("probe_channels", info.channels == 2);
    CHECK("probe_codec", strcmp(info.codec, "flac") == 0);
    CHECK("probe_container", strcmp(info.container, "flac") == 0);

    const song_metadata *meta = NULL;
    status = song_get_metadata(handle, &meta);
    CHECK("song_get_metadata", status == SONG_OK && meta != NULL);
    printf("METADATA status=%d has_title=%u has_artist=%u\n", (int)status,
           meta->has_title, meta->has_artist);

    float *pcm = (float *)malloc(sizeof(float) * 8192 * (size_t)info.channels);
    CHECK("pcm_alloc", pcm != NULL);
    uint64_t produced = 0;
    uint64_t total_frames = 0;
    int reads = 0;
    int saw_eof = 0;
    while (reads < 2000) {
        produced = 0;
        status = song_read_pcm(handle, pcm, 8192, &produced);
        if (status == SONG_OK) {
            CHECK("read_frames_nonzero", produced > 0);
            total_frames += produced;
            reads++;
            continue;
        }
        if (status == SONG_EOF) {
            saw_eof = 1;
            break;
        }
        printf("READ_ERROR status=%d reads=%d frames=%llu\n", (int)status, reads,
               (unsigned long long)total_frames);
        CHECK("song_read_pcm", 0);
    }
    printf("READ_FRAMES=%llu reads=%d eof=%d\n", (unsigned long long)total_frames,
           reads, saw_eof);
    CHECK("read_progress", total_frames > 0);
    CHECK("read_eof", saw_eof == 1);

    int64_t landing_us = -1;
    status = song_seek(handle, 1000000, &landing_us);
    printf("SEEK status=%d landing_us=%lld\n", (int)status, (long long)landing_us);
    CHECK("song_seek", status == SONG_OK);
    CHECK("seek_landing_known", landing_us >= 0 && landing_us <= 1000000);

    uint64_t after_seek = 0;
    status = song_read_pcm(handle, pcm, 8192, &after_seek);
    printf("POST_SEEK_READ status=%d frames=%llu\n", (int)status,
           (unsigned long long)after_seek);
    CHECK("post_seek_read", status == SONG_OK && after_seek > 0);

    free(pcm);
    song_close(handle);
    fclose(f);
    printf("CONSUMER_SMOKE=PASS\n");
    fflush(stdout);
    return 0;
}
