// realtime_bounds_test.cpp — corrective gates: bounded realtime path.
//
// Proves the PRODUCTION realtime seam (corrective §6–§19, §47–§50):
//   * fill_output()/advance_render()/timeline ops never allocate
//     (global new counter in this test binary) and never wait on the
//     control/state mutex (control thread provably holds it while the
//     fill completes on another thread);
//   * the bounded timeline stays bounded for arbitrarily long playback
//     (§48) and under pathological MEDIA/GAP alternation (§49), and fails
//     closed (never grows, never corrupts) when the store is exhausted;
//   * a control commit quiesces the backend before touching ring/timeline
//     (§11/§50): a consumer mid-fill is deterministically waited out, and
//     no resurrected frames survive the commit;
//   * the realtime seam raises the ENDED signal when the frozen ENDED
//     condition holds (§7).
//
// The global operator new/delete override is test-only (corrective §18:
// "Do not require allocator tricks in production").
#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <new>
#include <string>
#include <thread>
#include <vector>

#include "fake_songcore.hpp"
#include "player_engine.hpp"
#include "playback_timeline.hpp"
#include "test_support.hpp"

namespace qn::test {

using qn::EngineSnapshot;
using qn::LandingQuality;
using qn::OutputFillResult;
using qn::PlayerEngine;
using qn::PlayerState;
using qn::PlayerStatus;
using qn::RenderReport;
using qn::SpanKind;
using qn::StepOutcome;

namespace {

// --- test-binary allocation counter ----------------------------------------
std::atomic<std::uint64_t> g_alloc_count{0};

}  // namespace

}  // namespace qn::test

// Global operator new/delete override (test binary only).
void* operator new(std::size_t n) {
    qn::test::g_alloc_count.fetch_add(1, std::memory_order_relaxed);
    if (void* p = std::malloc(n)) return p;
    throw std::bad_alloc();
}
void* operator new[](std::size_t n) {
    qn::test::g_alloc_count.fetch_add(1, std::memory_order_relaxed);
    if (void* p = std::malloc(n)) return p;
    throw std::bad_alloc();
}
void operator delete(void* p) noexcept { std::free(p); }
void operator delete[](void* p) noexcept { std::free(p); }
void operator delete(void* p, std::size_t) noexcept { std::free(p); }
void operator delete[](void* p, std::size_t) noexcept { std::free(p); }

namespace qn::test {
namespace {

struct Setup {
    qn::EngineConfig cfg;
    std::unique_ptr<PlayerEngine> e;
    fake::SongConfig* song = nullptr;
    std::vector<float> buf;  // caller-preallocated fill buffer (8 ch)

    Setup(std::uint64_t cap, std::uint64_t chunk, std::uint64_t max_submit)
        : buf(static_cast<std::size_t>(max_submit * 8)) {
        cfg.capacity_frames = cap;
        cfg.read_chunk_frames = chunk;
        cfg.max_submit_frames = max_submit;
        cfg.max_channels = 8;
        cfg.worker_thread = false;  // manual decode stepping
    }
    void open(fake::SongConfig& c) {
        song = &c;
        c.live.reset();
        song_io io{};
        io.userdata = &c;
        io.read = [](void*, std::uint8_t*, std::size_t) -> std::int64_t { return 0; };
        io.seek = [](void*, std::int64_t) -> std::int64_t { return 0; };
        io.size = [](void*) -> std::int64_t { return 0; };
        QN_CHECK(e->open(io) == PlayerStatus::Ok, "setup-open");
    }
    // One decode quantum with the remaining-hint injection + EOF fold.
    void producer_step() {
        if (song && song->live) {
            const std::int64_t rem = song->total_frames - song->live->position;
            e->debug_set_source_hint(rem >= 0 ? rem : 0);
        }
        e->worker_step();
    }
    // Decode the whole source to EOF (with the harness EOF fold).
    void drain_to_eof() {
        bool fold = false;
        for (int i = 0; i < 100000 && !e->snapshot().source_eof; ++i) {
            const StepReport rep = e->worker_step();
            if (rep.outcome == StepOutcome::Begin) {
                if (song->live && song->live->position >= song->total_frames) fold = true;
            } else if (rep.outcome == StepOutcome::Wrote) {
                if (fold) {
                    e->debug_set_source_hint(0);
                    e->worker_step();  // SONG_EOF confirmation
                    fold = false;
                }
            } else if (rep.outcome == StepOutcome::Stale) {
                fold = false;
            }
        }
    }
};

fake::SongConfig song(std::int64_t total = 48000 * 8) {
    fake::SongConfig c;
    c.total_frames = total;
    c.sample_rate = 48000;
    c.channels = 2;
    return c;
}

// Publish one full chunk into the ring (Begin then Wrote with work_steps=1).
void publish_chunk(Setup& s, std::uint64_t chunk) {
    s.producer_step();
    s.producer_step();
    QN_CHECK(s.e->snapshot().queued_media_frames == chunk, "publish-chunk");
}

}  // namespace

// ---------------------------------------------------------------------------
// §18/§47: the production realtime path never allocates and never waits on
// the control mutex.
// ---------------------------------------------------------------------------

GATE(realtime_no_alloc_no_control_lock) {
    // (a) NO ALLOCATION: fill (media + underrun GAP insertion) and
    //     render-clock advancement run under a zero-allocation delta.
    {
        Setup s(16384, 1024, 8192);
        s.e = std::make_unique<PlayerEngine>(s.cfg);
        fake::SongConfig c = song(48000 * 4);
        s.open(c);
        s.e->play();
        publish_chunk(s, 1024);  // ring has exactly one chunk

        const std::uint64_t before = g_alloc_count.load(std::memory_order_relaxed);
        const OutputFillResult m1 = s.e->fill_output(s.buf.data(), 1024);  // media
        const OutputFillResult m2 = s.e->fill_output(s.buf.data(), 480);   // underrun GAP
        const RenderReport rr = s.e->advance_render(1504);                 // render clock
        const std::uint64_t after = g_alloc_count.load(std::memory_order_relaxed);

        QN_CHECK(m1.media_frames == 1024 && m1.silence_frames == 0 &&
                     std::strcmp(m1.kind, "audio") == 0,
                 "realtime-no-alloc: media fill");
        QN_CHECK(m2.media_frames == 0 && m2.silence_frames == 480 &&
                     std::strcmp(m2.kind, "underrun") == 0,
                 "realtime-no-alloc: underrun GAP fill");
        QN_CHECK(rr.rendered_output_frames == 1504 && rr.rendered_media_frames == 1024,
                 "realtime-no-alloc: render clock");
        QN_CHECK_MSG(after == before, "realtime-no-alloc",
                     "fill/advance performed %llu heap allocations",
                     static_cast<unsigned long long>(after - before));
    }

    // (b) NO CONTROL LOCK: the control thread provably holds state_mtx_ (and
    //     src_mtx_) — blocked inside a commit hook — while fill_output runs
    //     to completion on another thread. If the realtime path waited on the
    //     control mutex, the fill could never finish.
    {
        Setup s(16384, 1024, 8192);
        s.e = std::make_unique<PlayerEngine>(s.cfg);
        fake::SongConfig c = song(48000 * 4);
        s.open(c);
        s.e->play();
        publish_chunk(s, 1024);

        std::atomic<bool> hook_fired{false};
        std::atomic<bool> release{false};
        std::atomic<bool> fill_done{false};
        s.e->debug_set_control_hook([&] {
            hook_fired = true;
            while (!release.load()) std::this_thread::yield();
        });
        std::thread control([&] { s.e->seek(1'000'000); });
        while (!hook_fired.load()) std::this_thread::yield();  // control holds both locks

        std::thread fill([&] {
            s.e->fill_output(s.buf.data(), 480);
            fill_done = true;
        });
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
        while (!fill_done.load() && std::chrono::steady_clock::now() < deadline) {
            std::this_thread::yield();
        }
        QN_CHECK_MSG(fill_done.load(), "realtime-no-control-lock",
                     "fill_output blocked while the control thread held the mutex");

        release = true;
        control.join();
        fill.join();
        s.e->debug_set_control_hook(nullptr);
        QN_CHECK(s.e->snapshot().state != PlayerState::Error, "realtime-no-control-lock");
    }
    std::printf("  fill/advance: 0 allocations, completes while control mutex held\n");
}

// ---------------------------------------------------------------------------
// §18: timeline append/coalesce/advance/mapping never allocate.
// ---------------------------------------------------------------------------

GATE(timeline_no_alloc_ops) {
    PlaybackTimeline tl;
    const std::uint64_t before = g_alloc_count.load(std::memory_order_relaxed);
    for (int i = 0; i < 1000; ++i) {
        tl.append(SpanKind::Media, 480);
        tl.append(SpanKind::Gap, 480);
        tl.advance(480);
        tl.media_at_output(static_cast<std::uint64_t>(i));
        tl.pending_output();
        tl.pending_media();
    }
    const std::uint64_t after = g_alloc_count.load(std::memory_order_relaxed);
    QN_CHECK_MSG(after == before, "timeline-no-alloc",
                 "timeline ops performed %llu heap allocations",
                 static_cast<unsigned long long>(after - before));
}

// ---------------------------------------------------------------------------
// §48: timeline storage stays bounded for arbitrarily long playback.
// ---------------------------------------------------------------------------

GATE(timeline_long_run_memory) {
    PlaybackTimeline tl;
    const std::uint64_t period = 480;                // 10 ms @ 48 kHz
    const std::uint64_t cycles = 6ULL * 3600 * 100;  // 6 hours at 100 Hz
    std::uint64_t max_spans = 0;

    // Realistic pacing: media runs with an underrun GAP replacing every 8th
    // period (the device requests one period; on an underrun the ring is
    // empty so the period delivers 0 media + 1 period of silence — docs
    // §5). The device keeps up, so submission == rendering every period.
    // Coalescing + lazy trim must keep the store tiny.
    for (std::uint64_t i = 0; i < cycles; ++i) {
        const bool gap_period = ((i % 8) == 7);
        if (!tl.append(gap_period ? SpanKind::Gap : SpanKind::Media, period))
            QN_CHECK(false, "long-run: overflow");
        tl.advance(period);
        max_spans = std::max(max_spans, tl.span_count());
    }
    QN_CHECK_MSG(max_spans <= 8, "long-run",
                 "media-run coalescing failed: peak %zu live spans",
                 static_cast<std::size_t>(max_spans));
    QN_CHECK(tl.pending_output() == 0 && tl.pending_media() == 0, "long-run: drain");

    // Stall phase: no rendering for 100 alternating periods (a 200-span
    // pending window, still representable), then a catch-up render that must
    // trim the rendered history back down. Memory must never exceed the
    // fixed capacity, and accounting must stay exact.
    PlaybackTimeline t2;
    std::uint64_t peak = 0;
    for (int i = 0; i < 100; ++i) {
        QN_CHECK(t2.append(i % 2 == 0 ? SpanKind::Media : SpanKind::Gap, period),
                 "long-run-stall: overflow before capacity");
        peak = std::max(peak, t2.span_count());
    }
    QN_CHECK_MSG(peak <= PlaybackTimeline::kCapacity, "long-run-stall",
                 "pending window exceeded the fixed store (%zu)",
                 static_cast<std::size_t>(peak));
    const PlaybackTimeline::RenderSplit catch_up = t2.advance(100 * period);
    QN_CHECK_MSG(catch_up.media == 50 * period && catch_up.gap == 50 * period,
                 "long-run-stall", "catch-up accounting broken");
    QN_CHECK_MSG(t2.span_count() <= 4, "long-run-stall",
                 "trim did not collapse the rendered history (%zu live)",
                 static_cast<std::size_t>(t2.span_count()));
    QN_CHECK(t2.pending_output() == 0 && t2.pending_media() == 0, "long-run-stall");
    QN_CHECK(t2.rendered_media_total() == 50 * period, "long-run-stall");
    QN_CHECK(t2.rendered_gap_total() == 50 * period, "long-run-stall");
    std::printf("  6h realistic: peak %llu spans; stall window peak %llu -> trim\n",
                static_cast<unsigned long long>(max_spans),
                static_cast<unsigned long long>(peak));
}

// ---------------------------------------------------------------------------
// §49: pathological MEDIA/GAP alternation — bounded with render, fail-closed
// without it.
// ---------------------------------------------------------------------------

GATE(pathological_alternation_bounded) {
    Setup s(4800, 480, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 1000);  // long song
    s.open(c);
    s.e->play();

    // Each cycle publishes exactly one 480-frame chunk, then submits MEDIA
    // (drains the ring) and GAP (underrun), then renders both: the
    // MEDIA/GAP/MEDIA/GAP pattern the corrective calls out.
    std::uint64_t max_spans = 0;
    const std::uint64_t cycles = 100000;
    for (std::uint64_t i = 0; i < cycles; ++i) {
        publish_chunk(s, 480);
        const OutputFillResult m = s.e->fill_output(s.buf.data(), 480);
        const OutputFillResult g = s.e->fill_output(s.buf.data(), 480);
        const RenderReport r = s.e->advance_render(960);
        QN_CHECK(m.media_frames == 480 && std::strcmp(m.kind, "audio") == 0,
                 "alternation: media submit");
        QN_CHECK(g.silence_frames == 480 && std::strcmp(g.kind, "underrun") == 0,
                 "alternation: gap submit");
        QN_CHECK(r.rendered_output_frames == 960 && r.rendered_media_frames == 480,
                 "alternation: render");
        max_spans = std::max(max_spans, s.e->timeline_debug().span_count());
    }
    QN_CHECK_MSG(max_spans <= 8, "alternation",
                 "live spans grew to %zu under alternation + render",
                 static_cast<std::size_t>(max_spans));

    // Mapping correctness at the render cursor (the only production query).
    const EngineSnapshot snap = s.e->snapshot();
    const std::int64_t mapped =
        s.e->media_position_at_output(static_cast<std::uint64_t>(snap.rendered_output_frames));
    QN_CHECK_MSG(mapped == static_cast<std::int64_t>(snap.rendered_media_frames),
                 "alternation",
                 "device->media mapping off: %lld != %lld", (long long)mapped,
                 (long long)snap.rendered_media_frames);
    QN_CHECK(s.e->timeline_debug().span_count() <= 8, "alternation: end state bounded");
    std::printf("  100k alternating periods: peak %llu live spans, mapping exact\n",
                static_cast<unsigned long long>(max_spans));
}

GATE(pathological_alternation_overflow_fail_closed) {
    Setup s(4800, 480, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 1000);
    s.open(c);
    s.e->play();

    // Same alternation but WITHOUT rendering: the pending window grows two
    // spans per cycle and must exhaust the fixed store. The engine must
    // FAIL CLOSED (stop submitting, land in Error with a diagnostic) —
    // never grow, never corrupt, never crash.
    bool overflowed = false;
    for (int i = 0; i < 2000 && !overflowed; ++i) {
        publish_chunk(s, 480);
        const SubmitReport m = s.e->submit(480);
        const SubmitReport g = s.e->submit(480);
        if (std::strcmp(m.kind, "idle") == 0 || std::strcmp(g.kind, "idle") == 0) {
            overflowed = true;
        }
    }
    QN_CHECK(overflowed, "alternation-overflow: never failed closed");
    const EngineSnapshot snap = s.e->snapshot();
    QN_CHECK(snap.state == PlayerState::Error, "alternation-overflow: not Error");
    QN_CHECK(std::strstr(snap.last_error, "timeline") != nullptr,
             "alternation-overflow: no diagnostic");
    QN_CHECK_MSG(s.e->timeline_debug().span_count() <= PlaybackTimeline::kCapacity,
                 "alternation-overflow",
                 "store grew past capacity (%zu)",
                 static_cast<std::size_t>(s.e->timeline_debug().span_count()));
    std::printf("  no-render alternation: fail-closed Error, store held at capacity\n");
}

// ---------------------------------------------------------------------------
// §11/§50: reset race — control commit quiesces a mid-fill consumer; no
// resurrected frames survive the commit.
// ---------------------------------------------------------------------------

GATE(reset_race_quiesce) {
    Setup s(16384, 1024, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 4);
    s.open(c);
    s.e->play();
    publish_chunk(s, 1024);

    std::atomic<bool> in_fill{false};
    std::atomic<bool> release_fill{false};
    std::atomic<bool> quiescing{false};
    std::atomic<bool> release_control{false};
    std::atomic<bool> seek_done{false};
    std::atomic<int> seek_result{-99};

    // Consumer thread: enters the realtime seam and blocks mid-fill
    // (active_fill_ incremented, ring/timeline untouched yet).
    s.e->debug_set_fill_hook([&] {
        in_fill = true;
        while (!release_fill.load()) std::this_thread::yield();
    });
    // Control commit: when it actually has to wait for the consumer, it
    // signals here (both control locks held).
    s.e->debug_set_quiesce_hook([&] {
        quiescing = true;
        while (!release_control.load()) std::this_thread::yield();
    });

    std::vector<float> buf(static_cast<std::size_t>(8192 * 8));
    std::thread consumer([&] { s.e->fill_output(buf.data(), 512); });
    while (!in_fill.load()) std::this_thread::yield();  // consumer mid-fill

    std::thread control([&] {
        seek_result = static_cast<int>(s.e->seek(1'000'000));
        seek_done = true;
    });
    while (!quiescing.load()) std::this_thread::yield();  // control in quiesce wait
    QN_CHECK_MSG(!seek_done.load(), "reset-race",
                 "seek committed while the consumer was still mid-fill");

    // Let the consumer finish; only then may the control commit proceed.
    release_fill = true;
    release_control = true;
    while (!seek_done.load()) std::this_thread::yield();
    consumer.join();
    control.join();
    s.e->debug_set_fill_hook(nullptr);
    s.e->debug_set_quiesce_hook(nullptr);

    QN_CHECK(seek_result.load() == static_cast<int>(PlayerStatus::Ok), "reset-race: seek");
    const EngineSnapshot snap = s.e->snapshot();
    QN_CHECK(snap.queued_media_frames == 0, "reset-race: resurrected frames after flush");
    QN_CHECK(snap.pending_media_frames == 0, "reset-race: old pending output survived");
    QN_CHECK(snap.segment > 1, "reset-race: no new segment after commit");
    QN_CHECK(snap.media_position_frames == 48000, "reset-race: clock not rebased to landing");
    QN_CHECK(s.e->backend().pending() == 0, "reset-race: backend pending not discarded");
    std::printf("  consumer mid-fill + seek: quiesce waits, flush clean, no resurrection\n");
}

// ---------------------------------------------------------------------------
// §7: the realtime seam raises the ENDED signal once the frozen ENDED
// condition holds (control plane performs the state transition).
// ---------------------------------------------------------------------------

GATE(realtime_seam_end_signal) {
    Setup s(16384, 1024, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(3000);
    s.open(c);
    s.e->play();
    s.drain_to_eof();
    QN_CHECK(s.e->snapshot().source_eof, "seam-end: source not exhausted");

    // Drive the realtime seam (not the locked test API) to full playout.
    int guard = 0;
    while (!s.e->debug_end_pending()) {
        s.e->fill_output(s.buf.data(), 512);
        s.e->advance_render(512);
        QN_CHECK(++guard < 10000, "seam-end: ENDED signal never raised");
    }
    QN_CHECK(s.e->snapshot().state == PlayerState::Playing,
             "seam-end: realtime seam must not transition state itself");
    // The control plane (here: the manual-tick submit) performs the
    // transition once the signal is observed.
    s.e->submit(512);
    QN_CHECK(s.e->snapshot().state == PlayerState::Ended, "seam-end: no transition");
    QN_CHECK(s.e->snapshot().media_position_frames == 3000, "seam-end: ENDED @duration");
    std::printf("  realtime seam raises ENDED signal; control plane transitions\n");
}

}  // namespace qn::test
