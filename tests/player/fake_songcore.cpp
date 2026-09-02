// fake_songcore.cpp — SongCore C-ABI fake for PlayerEngine tests.
//
// Contract mirror of song_open/probe/read_pcm/seek/close with the exact
// injection surface of the Python oracle's FakeSongCore: typed decode
// failure with partial-success framing, seek failure statuses, genuinely
// unknown landings (-1, never manufactured), unknown duration (-1), landing
// offsets, and reopen failure. Not thread-safe by itself — the engine
// serializes every call under its source mutex, matching the real handle's
// "not internally thread-safe" contract.
#include "fake_songcore.hpp"

#include <cstdlib>
#include <cstring>
#include <memory>
#include <vector>

#include "songcore.h"

namespace {

struct FakeSong {
    qn::fake::SongConfig* cfg;
    std::shared_ptr<qn::fake::SongState> state;
};

std::int64_t us_to_frames_floor(std::int64_t us, std::int64_t rate) {
    return (us * rate) / 1000000;
}

std::int64_t frames_to_us(std::int64_t frames, std::int64_t rate) {
    return frames * 1000000 / rate;
}

// Registry keeping every fake_song_make_io config alive for the process
// lifetime (handles may reference them until song_close, and the io the
// engine copied must survive stop()-recovery reopens).
std::vector<std::unique_ptr<qn::fake::SongConfig>>& io_registry() {
    static std::vector<std::unique_ptr<qn::fake::SongConfig>> reg;
    return reg;
}

}  // namespace

namespace qn::fake {

std::int64_t remaining(const SongConfig& cfg) {
    if (!cfg.live) return cfg.total_frames;
    return cfg.total_frames - cfg.live->position;
}

}  // namespace qn::fake

// ---------------------------------------------------------------------------
// Frozen C ABI, faked
// ---------------------------------------------------------------------------

extern "C" {

std::uint32_t songcore_abi_version(void) { return SONGCORE_ABI_VERSION; }

song_status song_open(const song_io* io, song_handle** out_handle) {
    if (io == nullptr || out_handle == nullptr) return SONG_ERR_INVALID_ARGUMENT;
    auto* cfg = static_cast<qn::fake::SongConfig*>(io->userdata);
    if (cfg->open_fails) return SONG_ERR_IO;
    if (cfg->total_frames <= 0 || cfg->sample_rate <= 0 || cfg->channels <= 0) {
        return SONG_ERR_INVALID_ARGUMENT;
    }
    auto* song = new FakeSong;
    song->cfg = cfg;
    if (cfg->live) {
        // Reopen (stop() recovery): a fresh handle at position 0, lifetime
        // diagnostics survive — mirrors FakeSongCore.reopen().
        if (cfg->reopen_fails) {
            delete song;
            return SONG_ERR_IO;
        }
        cfg->live->position = 0;
        cfg->live->exhausted = false;
        cfg->live->reopen_count += 1;
    } else {
        cfg->live = std::make_shared<qn::fake::SongState>();
    }
    song->state = cfg->live;
    *out_handle = reinterpret_cast<song_handle*>(song);
    return SONG_OK;
}

song_status song_probe(song_handle* handle, song_info* out_info) {
    if (handle == nullptr || out_info == nullptr) return SONG_ERR_NOT_OPEN;
    auto* song = reinterpret_cast<FakeSong*>(handle);
    std::memset(out_info, 0, sizeof(*out_info));
    out_info->sample_rate = song->cfg->sample_rate;
    out_info->channels = song->cfg->channels;
    out_info->channel_mask = SONG_CH_STEREO;
    out_info->duration_us = song->cfg->unknown_duration
                                ? std::int64_t{-1}
                                : frames_to_us(song->cfg->total_frames,
                                               song->cfg->sample_rate);
    std::memcpy(out_info->codec, "fake", 4);
    std::memcpy(out_info->container, "fake", 4);
    return SONG_OK;
}

song_status song_read_pcm(song_handle* handle, float* dst, std::uint64_t frame_capacity,
                          std::uint64_t* out_frames_produced) {
    if (handle == nullptr || out_frames_produced == nullptr) return SONG_ERR_NOT_OPEN;
    if (frame_capacity == 0) return SONG_ERR_INVALID_ARGUMENT;
    auto* song = reinterpret_cast<FakeSong*>(handle);
    auto& st = *song->state;
    if (st.exhausted) {
        *out_frames_produced = 0;
        return SONG_EOF;
    }
    const std::int64_t total = song->cfg->total_frames;
    std::int64_t n = static_cast<std::int64_t>(frame_capacity);
    if (n > total - st.position) n = total - st.position;
    if (song->cfg->fail_at_frame >= 0 && st.position >= song->cfg->fail_at_frame) {
        return SONG_ERR_DECODE_ERROR;  // fault surfaces with zero frames
    }
    if (song->cfg->fail_at_frame >= 0) {
        // Partial-success framing: emit up to the fault as SONG_OK; the
        // fault itself surfaces on the NEXT call.
        const std::int64_t room = song->cfg->fail_at_frame - st.position;
        if (n > room) n = room;
    }
    const std::int32_t channels = song->cfg->channels;
    for (std::int64_t i = 0; i < n; ++i) {
        const float base = static_cast<float>(st.position + i) + 0.25f;
        for (std::int32_t c = 0; c < channels; ++c) {
            dst[static_cast<std::size_t>(i) * channels + c] = base + 0.125f * c;
        }
    }
    st.position += n;
    st.emitted_total += n;
    st.exhausted = st.position >= total;
    *out_frames_produced = static_cast<std::uint64_t>(n);
    return n > 0 ? SONG_OK : SONG_EOF;
}

song_status song_seek(song_handle* handle, std::int64_t requested_position_us,
                      std::int64_t* out_actual_position_us) {
    if (handle == nullptr) return SONG_ERR_NOT_OPEN;
    auto* song = reinterpret_cast<FakeSong*>(handle);
    auto& st = *song->state;
    st.seek_count += 1;
    if (song->cfg->seek_status != SONG_OK) {
        if (out_actual_position_us) *out_actual_position_us = -1;
        return static_cast<song_status>(song->cfg->seek_status);
    }
    const std::int64_t rate = song->cfg->sample_rate;
    const std::int64_t total = song->cfg->total_frames;
    std::int64_t target = us_to_frames_floor(requested_position_us, rate);
    if (target < 0) target = 0;
    if (!song->cfg->unknown_duration && target > total) target = total;
    std::int64_t landing = target + song->cfg->seek_landing_offset;
    if (landing < 0) landing = 0;
    if (!song->cfg->unknown_duration && landing > total) landing = total;
    st.position = landing;
    st.exhausted = landing >= total;
    if (out_actual_position_us) {
        *out_actual_position_us =
            song->cfg->seek_landing_unknown ? std::int64_t{-1}
                                            : frames_to_us(landing, rate);
    }
    return SONG_OK;
}

song_status song_last_error(song_handle*, const song_error**) { return SONG_OK; }

song_status fake_song_make_io(song_io* out_io, std::int64_t total_frames,
                              std::int32_t sample_rate, std::int32_t channels) {
    if (out_io == nullptr || total_frames <= 0 || sample_rate <= 0 ||
        channels <= 0) {
        return SONG_ERR_INVALID_ARGUMENT;
    }
    io_registry().push_back(std::make_unique<qn::fake::SongConfig>());
    qn::fake::SongConfig* cfg = io_registry().back().get();
    cfg->total_frames = total_frames;
    cfg->sample_rate = sample_rate;
    cfg->channels = channels;
    std::memset(out_io, 0, sizeof *out_io);
    out_io->userdata = cfg;
    // The fake is synthetic: host IO callbacks are never invoked.
    out_io->read = [](void*, std::uint8_t*, std::size_t) -> std::int64_t { return 0; };
    out_io->seek = [](void*, std::int64_t) -> std::int64_t { return 0; };
    out_io->size = [](void*) -> std::int64_t { return 0; };
    return SONG_OK;
}

void song_close(song_handle* handle) {
    // The config/state outlive handles (the runner owns them); only the
    // handle shell dies.
    delete reinterpret_cast<FakeSong*>(handle);
}

}  // extern "C"
