// thread_stress_test.cpp — real-thread adversarial stress.
//
// Native concurrency introduces races a single-threaded model cannot
// express. These gates race the real decode worker thread against control
// commits (seek / stop / open) and race render progression against
// everything, using deterministic barriers where the interesting
// interleaving must be forced, then assert the frozen conservation laws at
// quiescence. The sanitizers (ASan/UBSan/TSan via --player_san) are the
// second half of the evidence.
//
// Determinism note: stress workers run WITHOUT any decode hints — production
// behavior (duration-derived estimate) is what races here.
#include <atomic>
#include <cmath>
#include <cstring>
#include <condition_variable>
#include <cstdio>
#include <map>
#include <memory>
#include <mutex>
#include <thread>
#include <vector>

#include "fake_songcore.hpp"
#include "player_engine.hpp"
#include "test_support.hpp"

namespace qn::test {

using qn::EngineSnapshot;
using qn::PlayerEngine;
using qn::PlayerState;
using qn::PlayerStatus;

namespace {

// Deterministic two-phase barrier used as a test hook target.
struct Barrier {
    std::mutex m;
    std::condition_variable cv;
    bool arrived = false;
    bool release = false;

    // Hook side: announce arrival, block until released.
    void arrive_and_wait() {
        std::unique_lock<std::mutex> lk(m);
        arrived = true;
        cv.notify_all();
        cv.wait(lk, [this] { return release; });
    }
    // Test side: wait for the hook to fire.
    void wait_arrived() {
        std::unique_lock<std::mutex> lk(m);
        cv.wait(lk, [this] { return arrived; });
    }
    // Test side: let the hook continue.
    void let_go() {
        std::lock_guard<std::mutex> lk(m);
        release = true;
        cv.notify_all();
    }
};

struct StressCfg {
    std::uint64_t capacity = 4096;
    std::uint64_t chunk = 1024;
    std::uint64_t max_submit = 8192;
};

std::unique_ptr<PlayerEngine> make_engine(bool threaded, const StressCfg& c = {}) {
    qn::EngineConfig cfg;
    cfg.capacity_frames = c.capacity;
    cfg.read_chunk_frames = c.chunk;
    cfg.max_submit_frames = c.max_submit;
    cfg.max_channels = 8;
    cfg.worker_thread = threaded;
    auto e = std::make_unique<PlayerEngine>(cfg);
    return e;
}

fake::SongConfig stress_song(std::int64_t total = 48000 * 4) {
    fake::SongConfig c;
    c.total_frames = total;
    c.sample_rate = 48000;
    c.channels = 2;
    return c;
}

// Song configs must outlive the engine (io.userdata points at them): all
// stress songs live in a per-round keepalive vector.
using SongKeep = std::vector<std::unique_ptr<fake::SongConfig>>;

fake::SongConfig& add_song(SongKeep& keep, std::int64_t total = 48000 * 4) {
    keep.push_back(std::make_unique<fake::SongConfig>(stress_song(total)));
    return *keep.back();
}

PlayerStatus open_into(PlayerEngine& e, fake::SongConfig& c) {
    c.live.reset();
    song_io io{};
    io.userdata = &c;
    io.read = [](void*, std::uint8_t*, std::size_t) -> std::int64_t { return 0; };
    io.seek = [](void*, std::int64_t) -> std::int64_t { return 0; };
    io.size = [](void*) -> std::int64_t { return 0; };
    return e.open(io);
}

// Quiescent invariant block: conservation laws hold after any interleaving.
// Every term comes from ONE snapshot() hold — all of these counters move
// under state_mtx_, so the equation is evaluated at a single instant. (The
// earlier multi-call form — snapshot, then ring totals, then in-flight —
// was a real UBSan catch: the worker's stale discard could land between
// the reads and tear the equation.)
void check_conservation(PlayerEngine& e, const char* ctx) {
    const EngineSnapshot s = e.snapshot();
    QN_CHECK_MSG(s.ring_produced_total == s.ring_consumed_total +
                                                s.queued_media_frames +
                                                s.ring_discarded_total,
                 ctx, "ring conservation");
    QN_CHECK_MSG(s.decoded_source_frames == s.ring_produced_total +
                                                s.discarded_stale_media_frames +
                                                s.in_flight_frames,
                 ctx, "decode conservation");
    QN_CHECK_MSG(s.submitted_output_frames == s.pending_output_frames +
                                                    s.rendered_output_frames +
                                                    s.discarded_output_frames,
                 ctx, "output conservation");
    QN_CHECK_MSG(s.submitted_media_frames == s.pending_media_frames +
                                                    s.rendered_media_frames +
                                                    s.discarded_output_media_frames,
                 ctx, "media conservation");
    QN_CHECK_MSG(s.rendered_output_frames ==
                     s.rendered_media_frames + s.rendered_gap_output_frames,
                 ctx, "rendered split");
}

// Verify the FINAL segment's rendered content is contiguous from its anchor
// (CONFIRMED segments only — the stress corpus uses plain landings).
void check_final_segment_continuity(PlayerEngine& e, const char* ctx) {
    const EngineSnapshot s = e.snapshot();
    if (s.rendered_media_frames == 0) return;
    if (e.segment_landing_quality(s.segment) != qn::LandingQuality::Confirmed) return;
    const std::int64_t anchor = e.segment_anchor(s.segment);
    std::int64_t expect = anchor;
    for (const auto& entry : e.backend().rendered_log()) {
        if (entry.segment != s.segment) continue;
        for (std::size_t i = 0; i + 1 < entry.pcm.size(); i += 2) {
            const std::int64_t tag = static_cast<std::int64_t>(llroundf(entry.pcm[i] - 0.25f));
            QN_CHECK_MSG(tag == expect, ctx, "segment %llu tag %lld != %lld",
                         (unsigned long long)s.segment, (long long)tag,
                         (long long)expect);
            ++expect;
        }
    }
}

}  // namespace

// Repeated create/open/play/destroy with in-flight work — the
// destruction order (stop publications, join worker, close source) must be
// race-free. Evidence = sanitizers + no hang; loop count keeps runtime sane.
GATE(stress_destruction) {
    for (int i = 0; i < 120; ++i) {
        SongKeep keep;
        auto e = make_engine(true);
        QN_CHECK(open_into(*e, add_song(keep)) == PlayerStatus::Ok, "stress-destruction");
        e->play();
        // burst of backend activity, then destroy mid-everything
        e->submit(512);
        e->backend_render(256);
        if (i % 3 == 0) {
            e->seek(1'000'000);
        } else if (i % 3 == 1) {
            e->pause();
        }
        e->submit(256);
        // destructor runs here with the worker possibly mid-decode
    }
    std::printf("  120x create/open/play/<commit>/destroy under worker thread OK\n");
}

// Decode completion vs seek — forced interleaving via the
// before-publish barrier: the seek commits while the chunk is decoded but
// unpublished; publication must see the dead epoch and discard.
GATE(stress_seek_vs_decode) {
    for (int round = 0; round < 40; ++round) {
        SongKeep keep;
        auto e = make_engine(true);
        QN_CHECK(open_into(*e, add_song(keep, 48000 * 20)) == PlayerStatus::Ok,
                 "stress-seek-decode");
        // Long in-flight latency makes the BEGIN..publish window observable
        // and parkable (the trace runner gets this from the fake's
        // work_steps; production just however long a decode takes).
        e->debug_set_work_steps(64);
        e->play();
        // Wait for a chunk to be in flight, pumping the backend so the
        // bounded queue can never wedge the worker asleep.
        int guard = 0;
        while (e->in_flight_frames() == 0) {
            e->submit(512);
            e->backend_render(512);
            QN_CHECK(++guard < 100000, "stress-seek-decode: no decode began");
        }
        Barrier bar;
        e->debug_set_publish_hook([&bar] { bar.arrive_and_wait(); });
        bar.wait_arrived();  // worker parked before the epoch check
        QN_CHECK(e->seek(2'000'000) == PlayerStatus::Ok, "stress-seek-decode");
        const std::uint64_t stale_before = e->snapshot().discarded_stale_media_frames;
        bar.let_go();        // publication fires under the dead epoch
        e->debug_set_publish_hook(nullptr);
        guard = 0;
        while (e->snapshot().discarded_stale_media_frames <= stale_before) {
            e->submit(256);
            e->backend_render(256);
            QN_CHECK(++guard < 200000, "stress-seek-decode: stale discard never landed");
        }
        e->pause();  // quiesce: in-flight completes, worker idles
        guard = 0;
        while (e->in_flight_frames() != 0) {
            e->submit(256);
            e->backend_render(256);
            QN_CHECK(++guard < 200000, "stress-seek-decode: in-flight never completed");
        }
        check_conservation(*e, "stress-seek-decode");
    }
    std::printf("  40x barrier-forced seek-during-decode: stale discard observed\n");
}

// Decode completion vs stop / vs open.
GATE(stress_stop_open_vs_decode) {
    for (int round = 0; round < 30; ++round) {
        SongKeep keep;
        auto e = make_engine(true);
        QN_CHECK(open_into(*e, add_song(keep, 48000 * 20)) == PlayerStatus::Ok,
                 "stress-stop-decode");
        e->debug_set_work_steps(64);
        e->play();
        int guard = 0;
        while (e->in_flight_frames() == 0) {
            e->submit(512);
            e->backend_render(512);
            QN_CHECK(++guard < 100000, "stress-stop-decode: no decode began");
        }
        Barrier bar;
        e->debug_set_publish_hook([&bar] { bar.arrive_and_wait(); });
        bar.wait_arrived();
        if (round % 2 == 0) {
            e->stop();
        } else {
            QN_CHECK(open_into(*e, add_song(keep, 48000 * 10)) == PlayerStatus::Ok,
                     "stress-open-decode");
        }
        const std::uint64_t stale_before = e->snapshot().discarded_stale_media_frames;
        bar.let_go();
        e->debug_set_publish_hook(nullptr);
        guard = 0;
        while (e->snapshot().discarded_stale_media_frames <= stale_before) {
            e->submit(256);
            e->backend_render(256);
            QN_CHECK(++guard < 200000, "stress-stop-decode: stale discard never landed");
        }
        const EngineSnapshot s = e->snapshot();
        if (round % 2 == 0) {
            QN_CHECK(s.state == PlayerState::Ready, "stress-stop-decode: stop->READY");
        } else {
            QN_CHECK(s.state == PlayerState::Ready, "stress-open-decode: open->READY");
        }
        check_conservation(*e, "stress-stop-decode");
    }
    std::printf("  30x barrier-forced stop/open-during-decode: stale discard observed\n");
}

// Submit/render vs pause, render vs seek, render vs open — a render
// thread hammers backend_render while the main thread commits.
GATE(stress_render_vs_control) {
    for (int round = 0; round < 20; ++round) {
        SongKeep keep;
        auto e = make_engine(true);
        QN_CHECK(open_into(*e, add_song(keep, 48000 * 30)) == PlayerStatus::Ok,
                 "stress-render-control");
        e->play();
        std::atomic<bool> stop_render{false};
        std::atomic<bool> bad{false};
        std::thread render_thread([&] {
            Rng rng(static_cast<std::uint64_t>(round) + 1);
            while (!stop_render.load()) {
                const RenderReport rep = e->backend_render(
                    static_cast<std::int64_t>(rng.uniform(1, 8) * 128));
                if (std::strcmp(rep.kind, "rendered") == 0 &&
                    rep.rendered_output_frames < rep.rendered_media_frames) {
                    bad = true;  // media rendered may never exceed output
                }
            }
        });
        Rng rng(99 + static_cast<std::uint64_t>(round));
        for (int k = 0; k < 60; ++k) {
            const int what = rng.uniform(0, 5);
            if (what == 0) {
                e->pause();
            } else if (what == 1) {
                e->play();
            } else if (what == 2) {
                e->seek(static_cast<std::int64_t>(rng.below(20'000'000)));
            } else if (what == 3) {
                e->stop();
                e->play();
            } else if (what == 4) {
                open_into(*e, add_song(keep, 48000 * 8));
                e->play();
            } else {
                e->submit(static_cast<std::uint64_t>(rng.uniform(1, 4) * 256));
            }
        }
        stop_render = true;
        render_thread.join();
        QN_CHECK(!bad.load(), "stress-render-control: media rendered > output rendered");
        e->stop();  // quiesce
        check_conservation(*e, "stress-render-control");
        check_final_segment_continuity(*e, "stress-render-control");
    }
    std::printf("  20x render-thread vs pause/seek/stop/open: guards + conservation hold\n");
}

// EOF vs pause / EOF vs stop — a short song finishing while control
// commits race the drain; ENDED must only ever appear after full playout.
GATE(stress_eof_vs_control) {
    for (int round = 0; round < 30; ++round) {
        SongKeep keep;
        auto e = make_engine(true);
        QN_CHECK(open_into(*e, add_song(keep, 2400)) == PlayerStatus::Ok, "stress-eof");
        e->play();
        Rng rng(500 + static_cast<std::uint64_t>(round));
        for (int k = 0; k < 40; ++k) {
            e->submit(256);
            e->backend_render(256);
            const int what = rng.uniform(0, 4);
            if (what == 0) e->pause();
            else if (what == 1) e->play();
            else if (what == 2) e->stop();
            else if (what == 3) e->seek(0);
            const EngineSnapshot s = e->snapshot();
            if (s.state == PlayerState::Ended) {
                // ENDED demands the complete drain: nothing pending, no
                // queue, source exhausted.
                QN_CHECK_MSG(s.pending_media_frames == 0 && s.queued_media_frames == 0 &&
                                 s.source_eof,
                             "stress-eof", "ENDED without full playout");
            }
        }
        e->stop();
        check_conservation(*e, "stress-eof");
    }
    std::printf("  30x short-song EOF drain vs pause/stop/seek: ENDED only after playout\n");
}

// Soak: full pipeline to ENDED under the threaded worker with random
// pacing; the final state must be a clean, complete playout.
GATE(stress_play_to_end) {
    for (int round = 0; round < 15; ++round) {
        SongKeep keep;
        auto e = make_engine(true);
        QN_CHECK(open_into(*e, add_song(keep, 48000)) == PlayerStatus::Ok, "stress-e2e");
        e->play();
        Rng rng(7000 + static_cast<std::uint64_t>(round));
        int guard = 0;
        while (e->snapshot().state != PlayerState::Ended) {
            QN_CHECK(++guard < 200000, "stress-e2e: runaway");
            e->submit(512);
            e->backend_render(static_cast<std::int64_t>(rng.uniform(128, 1024)));
            if (rng.uniform(0, 16) == 0) e->pause();
            if (rng.uniform(0, 16) == 0) e->play();
        }
        const EngineSnapshot s = e->snapshot();
        QN_CHECK(s.media_position_frames == s.duration_frames, "stress-e2e: ENDED @duration");
        QN_CHECK(s.pending_media_frames == 0, "stress-e2e");
        check_conservation(*e, "stress-e2e");
        check_final_segment_continuity(*e, "stress-e2e");
    }
    std::printf("  15x threaded play-to-ENDED: complete playout, conservation holds\n");
}

}  // namespace qn::test
