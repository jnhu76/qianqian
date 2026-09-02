// fake_songcore.hpp — deterministic SongCore stand-in (test-only).
//
// Native mirror of tools/player_model/fake_decoder.py. Implements the frozen
// C ABI (include/songcore.h) exactly as the contract allows, backed by a
// synthetic frame-indexed source. Test binaries link this INSTEAD of the
// real songcore target: the engine calls the production ABI, and the fake
// replaces its symbols at link time — no abstraction layer is added to the
// engine.
//
// Frame identity encoding (test-only oracle truth, docs §Content
// continuity): media frame f is written as sample[c] = f + 0.25f +
// 0.125f*c, so the trace runner can decode rendered PCM back to frame tags;
// GAP silence is all-zero and stays distinguishable (L >= 0.25f iff media).
//
// The state block survives engine stop()-recovery reopens (mirroring the
// oracle's decoder object); the runner resets it when a trace OP open
// starts a fresh song.
#ifndef QIANQIAN_TESTS_PLAYER_FAKE_SONGCORE_HPP
#define QIANQIAN_TESTS_PLAYER_FAKE_SONGCORE_HPP

#include <cstdint>
#include <memory>

namespace qn::fake {

struct SongState {
    std::int64_t position = 0;       // next source frame read_pcm will emit
    bool exhausted = false;
    std::int64_t emitted_total = 0;  // frames handed out (global accounting)
    int seek_count = 0;
    int reopen_count = 0;
};

struct SongConfig {
    std::int64_t total_frames = 48000 * 8;
    std::int32_t sample_rate = 48000;
    std::int32_t channels = 2;
    std::int64_t seek_landing_offset = 0;
    std::int64_t fail_at_frame = -1;  // -1 = no injected fault
    bool open_fails = false;
    std::int32_t seek_status = 0;     // injected song_seek status
    bool seek_landing_unknown = false;
    bool unknown_duration = false;
    bool reopen_fails = false;

    std::shared_ptr<SongState> live;  // non-null after song_open
};

// Remaining frames per the fake's truth (what the oracle's decoder knows).
std::int64_t remaining(const SongConfig& cfg);

}  // namespace qn::fake

#endif  // QIANQIAN_TESTS_PLAYER_FAKE_SONGCORE_HPP
