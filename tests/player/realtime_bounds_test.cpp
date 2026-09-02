// realtime_bounds_test.cpp — permanent realtime-contract regressions.
//
// Proves the PRODUCTION realtime seam (docs §9–§10):
//   * fill_output()/advance_render()/timeline ops never allocate (global
//     new counter in this test binary), take NO MUTEX of any kind (a
//     test-only atomic barrier stands in for the reset-race proof), and
//     never wait on the control/state mutex (control thread provably holds
//     it while the fill completes on another thread);
//   * GAP silence is PHYSICALLY zero in the caller's PCM buffer (poison-
//     buffer gate) — an underrun/preroll/EOS fill returns real media frames
//     followed by exact Float32 zeros, never stale buffer contents;
//   * the bounded timeline stays bounded for arbitrarily long playback and
//     under pathological MEDIA/GAP alternation, and fails closed (never
//     grows, never corrupts) when the store is exhausted;
//   * a control commit closes backend admission BEFORE draining: a callback
//     that entered before the close is counted and waited out; a callback
//     that attempts after the close is never counted and returns idle
//     without touching ring/timeline;
//   * timeline overflow reaches deterministic ERROR through the PRODUCTION
//     seam (fill_output/advance_render), not the test-only submit wrapper;
//   * the product snapshot is one coherent instant: position/duration/
//     sample_rate all derive from the rate captured inside the snapshot
//     hold, while another thread opens alternating-rate sources;
//   * the realtime seam raises the ENDED signal when the frozen ENDED
//     condition holds (docs §7).
//
// The global operator new/delete override is test-only.
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
#include "fake_songcore_c.h"
#include "player_engine.h"
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
    // One decode quantum.
    void producer_step() { e->worker_step(); }
    // Decode the whole source to EOF.
    void drain_to_eof() {
        for (int i = 0; i < 100000 && !e->snapshot().source_eof; ++i) {
            e->worker_step();
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

// Fill a caller buffer with a non-zero poison value.
void poison_buf(std::vector<float>& buf, std::size_t n) {
    const std::size_t lim = n < buf.size() ? n : buf.size();
    for (std::size_t i = 0; i < lim; ++i) buf[i] = 123.0f;
}

}  // namespace

// ---------------------------------------------------------------------------
// The production realtime path never allocates, takes NO
// MUTEX, and never waits on the control mutex.
//
// Structural proof for "no mutex": fill_output()/advance_render() no longer
// contain a single mutex operation — the only mutex they ever took
// (hook_mtx_ for the debug_set_fill_hook std::function) is deleted; the
// remaining shared state is atomics + the lock-free ring/timeline. The
// dynamic checks below pin the observable part (no allocation, completes
// while the control thread provably holds both mutexes).
// ---------------------------------------------------------------------------

GATE(realtime_no_alloc_no_mutex) {
    // (a) NO ALLOCATION: fill (media + underrun GAP insertion) and
    //     render-clock advancement run under a zero-allocation delta, and
    //     the underrun GAP is PHYSICALLY zero in the caller's buffer —
    //     not merely counted.
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
        // The GAP fill's dst must be exact silence, not the previous fill's
        // media PCM (2 channels).
        for (std::size_t i = 0; i < 480 * 2; ++i) {
            if (s.buf[i] != 0.0f) {
                QN_CHECK_MSG(false, "realtime-no-alloc",
                             "underrun GAP left non-zero PCM at %zu: %f", i, s.buf[i]);
            }
        }
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
    std::printf("  fill/advance: 0 allocations, no mutex, completes while control mutex held\n");
}

// ---------------------------------------------------------------------------
// GAP silence is physically written into the caller's PCM buffer (docs
// §5). A poison-prefilled dst must end with exact 0.0f for every channel
// of the shortfall — never the poison, never stale media PCM.
// ---------------------------------------------------------------------------

GATE(gap_output_zero_fill) {
    Setup s(1024, 4, 64);  // tiny chunks: precise 4-frame media + 6-frame gap
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(1000);
    s.open(c);
    s.e->play();

    const std::int32_t ch = 2;
    auto poison = [&](std::size_t n) {
        for (std::size_t i = 0; i < n; ++i) s.buf[i] = 123.0f;
    };
    auto media_at = [&](std::int64_t f, std::int32_t chan) {
        return static_cast<float>(f) + 0.25f + 0.125f * static_cast<float>(chan);
    };

    // (a) M real + N-M gap in ONE fill: request 10 frames, only 4 available.
    publish_chunk(s, 4);
    poison(10 * static_cast<std::size_t>(ch));
    const OutputFillResult r = s.e->fill_output(s.buf.data(), 10);
    QN_CHECK(r.media_frames == 4 && r.silence_frames == 6 &&
                 std::strcmp(r.kind, "preroll") == 0,
             "gap-zero: media/gap split");
    for (std::int64_t f = 0; f < 4; ++f) {
        for (std::int32_t cc = 0; cc < ch; ++cc) {
            const float got = s.buf[static_cast<std::size_t>(f) * ch + cc];
            QN_CHECK_MSG(got == media_at(f, cc), "gap-zero",
                         "media frame %lld ch %d = %f, want %f", (long long)f, cc,
                         static_cast<double>(got), static_cast<double>(media_at(f, cc)));
        }
    }
    for (std::size_t i = 4 * static_cast<std::size_t>(ch);
         i < 10 * static_cast<std::size_t>(ch); ++i) {
        QN_CHECK_MSG(s.buf[i] == 0.0f, "gap-zero",
                     "GAP tail at %zu = %f (poison/stale survived)", i,
                     static_cast<double>(s.buf[i]));
    }

    // (b) same split under a true UNDERRUN (media already submitted this
    //     segment), with real media PCM after the split.
    publish_chunk(s, 4);        // frames 4..7
    s.e->fill_output(s.buf.data(), 4);  // consume: media submit, no gap
    publish_chunk(s, 4);        // frames 8..11
    poison(10 * static_cast<std::size_t>(ch));
    const OutputFillResult u = s.e->fill_output(s.buf.data(), 10);
    QN_CHECK(u.media_frames == 4 && u.silence_frames == 6 &&
                 std::strcmp(u.kind, "underrun") == 0,
             "gap-zero: underrun split");
    for (std::int64_t f = 8; f < 12; ++f) {
        for (std::int32_t cc = 0; cc < ch; ++cc) {
            const std::size_t off = static_cast<std::size_t>(f - 8) * ch + cc;
            QN_CHECK_MSG(s.buf[off] == media_at(f, cc), "gap-zero",
                         "underrun media frame %lld ch %d", (long long)f, cc);
        }
    }
    for (std::size_t i = 4 * static_cast<std::size_t>(ch);
         i < 10 * static_cast<std::size_t>(ch); ++i) {
        QN_CHECK_MSG(s.buf[i] == 0.0f, "gap-zero",
                     "underrun GAP tail at %zu = %f", i,
                     static_cast<double>(s.buf[i]));
    }

    // (c) full-gap fill (m == 0): the ENTIRE period is silence.
    poison(10 * static_cast<std::size_t>(ch));
    const OutputFillResult z = s.e->fill_output(s.buf.data(), 10);  // ring empty
    QN_CHECK(z.media_frames == 0 && z.silence_frames == 10 &&
                 std::strcmp(z.kind, "underrun") == 0,
             "gap-zero: full-gap");
    for (std::size_t i = 0; i < 10 * static_cast<std::size_t>(ch); ++i) {
        QN_CHECK_MSG(s.buf[i] == 0.0f, "gap-zero",
                     "full-gap at %zu = %f", i, static_cast<double>(s.buf[i]));
    }
    std::printf("  poison-buffer: media PCM kept, GAP tail exact 0.0f (preroll/underrun)\n");
}

// ---------------------------------------------------------------------------
// Timeline append/coalesce/advance/mapping never allocate.
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
// Timeline storage stays bounded for arbitrarily long playback.
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
// Pathological MEDIA/GAP alternation — bounded with render.
// ---------------------------------------------------------------------------

GATE(pathological_alternation_bounded) {
    Setup s(4800, 480, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 1000);  // long song
    s.open(c);
    s.e->play();

    // Each cycle publishes exactly one 480-frame chunk, then submits MEDIA
    // (drains the ring) and GAP (underrun), then renders both: the
    // MEDIA/GAP/MEDIA/GAP pathological pattern.
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

// ---------------------------------------------------------------------------
// Timeline overflow reaches deterministic ERROR through the PRODUCTION seam
// (fill_output), not the test-only locked submit() wrapper. The fail-closed
// transition must fire on a real WASAPI-style fill, with the fixed
// diagnostic visible in the snapshot, no allocation, and no further output
// mutation once ERROR.
// ---------------------------------------------------------------------------

GATE(production_overflow_fail_closed) {
    Setup s(4800, 480, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 1000);
    s.open(c);
    s.e->play();

    // Alternating MEDIA (publish 480 -> fill 480) and GAP (empty ring ->
    // fill 480 underrun) via fill_output ONLY, with no rendering: the
    // pending window grows two spans per cycle and must exhaust the fixed
    // store on the realtime seam itself.
    const std::uint64_t alloc_before = g_alloc_count.load(std::memory_order_relaxed);
    bool overflowed = false;
    for (int i = 0; i < 2000 && !overflowed; ++i) {
        publish_chunk(s, 480);
        const OutputFillResult m = s.e->fill_output(s.buf.data(), 480);
        if (std::strcmp(m.kind, "idle") == 0) {
            overflowed = true;
            break;
        }
        const OutputFillResult g = s.e->fill_output(s.buf.data(), 480);
        if (std::strcmp(g.kind, "idle") == 0) {
            overflowed = true;
            break;
        }
    }
    const std::uint64_t alloc_delta =
        g_alloc_count.load(std::memory_order_relaxed) - alloc_before;
    QN_CHECK(overflowed, "production-overflow: never failed closed");
    QN_CHECK_MSG(alloc_delta == 0, "production-overflow",
                 "realtime overflow path allocated %llu times",
                 static_cast<unsigned long long>(alloc_delta));

    const EngineSnapshot snap = s.e->snapshot();
    QN_CHECK(snap.state == PlayerState::Error, "production-overflow: not Error");
    QN_CHECK(std::strstr(snap.last_error, "timeline") != nullptr,
             "production-overflow: no diagnostic in snapshot");
    QN_CHECK_MSG(s.e->timeline_debug().span_count() <= PlaybackTimeline::kCapacity,
                 "production-overflow",
                 "store grew past capacity (%zu)",
                 static_cast<std::size_t>(s.e->timeline_debug().span_count()));

    // No further output mutation once ERROR: a fill returns idle, appends
    // nothing, and consumes nothing from the ring.
    const std::uint64_t spans_before = s.e->timeline_debug().span_count();
    const std::uint64_t queued_before = s.e->snapshot().queued_media_frames;
    poison_buf(s.buf, 8192 * 8);
    const OutputFillResult after = s.e->fill_output(s.buf.data(), 480);
    QN_CHECK(std::strcmp(after.kind, "idle") == 0 && after.media_frames == 0,
             "production-overflow: post-ERROR fill not inert");
    QN_CHECK(s.e->timeline_debug().span_count() == spans_before,
             "production-overflow: post-ERROR span appended");
    QN_CHECK(s.e->snapshot().queued_media_frames == queued_before,
             "production-overflow: post-ERROR ring consumed");
    std::printf("  production seam overflow: Error + snapshot diagnostic, 0 allocs, inert\n");
}

// ---------------------------------------------------------------------------
// Backend admission — close-then-drain quiescence, both sides of the race,
// deterministically.
//
// Side 2 (below): a callback that ENTERED just before admission closed is
// counted; control waits for it to exit before the reset proceeds — no
// resurrected frames survive the commit.
// Side 1 (quiesce_closes_admission_first): a callback that ATTEMPTS to enter
// after admission closed is never counted and returns idle without touching
// ring/timeline; control's drain observes an empty pool and resets.
//
// No sleeps: the control-side quiesce hook and the test-only atomic fill
// barrier (the replacement for the removed std::function fill hook) pin the
// interleavings.
// ---------------------------------------------------------------------------

GATE(quiesce_waits_for_inflight) {
    Setup s(16384, 1024, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 4);
    s.open(c);
    s.e->play();
    publish_chunk(s, 1024);

    std::atomic<bool> quiescing{false};
    std::atomic<bool> release_consumer{false};
    std::atomic<bool> release_control{false};
    std::atomic<bool> seek_done{false};
    std::atomic<int> seek_result{-99};

    // Consumer: enters the realtime seam (admitted + counted) and blocks
    // mid-fill on the atomic barrier — the no-mutex replacement for the
    // removed fill hook (ring/timeline untouched yet).
    s.e->debug_set_fill_barrier_armed(true);
    std::vector<float> buf(static_cast<std::size_t>(8192 * 8));
    std::thread consumer([&] { s.e->fill_output(buf.data(), 512); });
    while (!s.e->debug_fill_barrier_entered()) std::this_thread::yield();

    // Control commit: admission closed, then it must wait for the counted
    // consumer (quiesce hook fires after the close, before the drain).
    s.e->debug_set_quiesce_hook([&] {
        quiescing = true;
        while (!release_control.load()) std::this_thread::yield();
    });
    std::thread control([&] {
        seek_result = static_cast<int>(s.e->seek(1'000'000));
        seek_done = true;
    });
    while (!quiescing.load()) std::this_thread::yield();  // admission closed
    QN_CHECK_MSG(!seek_done.load(), "quiesce-wait",
                 "seek committed while the consumer was still mid-fill");

    // Let the consumer finish; only then may the control drain proceed.
    s.e->debug_release_fill_barrier();
    release_control = true;
    while (!seek_done.load()) std::this_thread::yield();
    consumer.join();
    control.join();
    s.e->debug_set_fill_barrier_armed(false);
    s.e->debug_set_quiesce_hook(nullptr);

    QN_CHECK(seek_result.load() == static_cast<int>(PlayerStatus::Ok), "quiesce-wait: seek");
    const EngineSnapshot snap = s.e->snapshot();
    QN_CHECK(snap.queued_media_frames == 0, "quiesce-wait: resurrected frames after flush");
    QN_CHECK(snap.pending_media_frames == 0, "quiesce-wait: old pending output survived");
    QN_CHECK(snap.segment > 1, "quiesce-wait: no new segment after commit");
    QN_CHECK(snap.media_position_frames == 48000, "quiesce-wait: clock not rebased to landing");
    QN_CHECK(s.e->backend().pending() == 0, "quiesce-wait: backend pending not discarded");
    std::printf("  callback entered before close: counted, waited out, flush clean\n");
}

GATE(quiesce_closes_admission_first) {
    Setup s(16384, 1024, 8192);
    s.e = std::make_unique<PlayerEngine>(s.cfg);
    fake::SongConfig c = song(48000 * 4);
    s.open(c);
    s.e->play();
    publish_chunk(s, 1024);

    std::atomic<bool> admission_closed{false};
    std::atomic<bool> release_control{false};
    std::atomic<bool> seek_done{false};
    std::atomic<int> seek_result{-99};

    // Control commit: close admission, then block in the quiesce hook — the
    // deterministic "admission closed BEFORE any callback" checkpoint.
    s.e->debug_set_quiesce_hook([&] {
        admission_closed = true;
        while (!release_control.load()) std::this_thread::yield();
    });
    std::thread control([&] {
        seek_result = static_cast<int>(s.e->seek(1'000'000));
        seek_done = true;
    });
    while (!admission_closed.load()) std::this_thread::yield();

    // A callback attempting to enter while admission is closed must NOT
    // become active: it returns idle immediately and is not counted.
    std::vector<float> buf(static_cast<std::size_t>(8192 * 8));
    const OutputFillResult q = s.e->fill_output(buf.data(), 512);
    QN_CHECK(std::strcmp(q.kind, "idle") == 0 && q.media_frames == 0 &&
                 q.silence_frames == 0,
             "quiesce-close-first: quiesced fill not idle");
    QN_CHECK_MSG(s.e->debug_active_backend_ops() == 0, "quiesce-close-first",
                 "quiesced callback became an active backend op");
    QN_CHECK_MSG(!seek_done.load(), "quiesce-close-first",
                 "seek finished while the consumer raced the reset");

    // The drain pool is empty; releasing control completes the reset.
    release_control = true;
    while (!seek_done.load()) std::this_thread::yield();
    control.join();
    s.e->debug_set_quiesce_hook(nullptr);

    QN_CHECK(seek_result.load() == static_cast<int>(PlayerStatus::Ok),
             "quiesce-close-first: seek");
    const EngineSnapshot snap = s.e->snapshot();
    QN_CHECK(snap.queued_media_frames == 0, "quiesce-close-first: resurrected frames");
    QN_CHECK(snap.pending_media_frames == 0, "quiesce-close-first: old pending survived");
    QN_CHECK(snap.media_position_frames == 48000, "quiesce-close-first: clock rebased");
    std::printf("  callback after close: never counted, idle return, reset proceeds\n");
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

// ---------------------------------------------------------------------------
// The product snapshot is ONE coherent instant. Thread A
// polls pe_get_snapshot while thread B opens 44.1 kHz / 48 kHz sources
// back-to-back. Every snapshot's position_us/duration_us/sample_rate must
// belong to the same captured source state — a post-lock source_rate() read
// (the old bug) mixes frames from one song with the next song's rate and
// produces an impossible duration. TSan must stay clean (the old code was a
// data race on source_rate_).
// ---------------------------------------------------------------------------

GATE(snapshot_rate_coherent) {
    pe_engine* eng = pe_create(nullptr);
    QN_CHECK(eng != nullptr, "snapshot-coherent: create");

    std::atomic<bool> stop{false};
    std::atomic<int> bad{0};
    std::atomic<int> polls{0};

    std::thread opener([&] {
        // Both fixtures are exactly 2 s, so the coherent duration_us is
        // 2,000,000 regardless of which song is open; a mismatched rate
        // yields a different value (88200 frames @ 48 kHz -> 1,837,500).
        for (int i = 0; i < 300 && !stop.load(); ++i) {
            const int rate = (i % 2 == 0) ? 44100 : 48000;
            song_io io;
            if (fake_song_make_io(&io, static_cast<std::int64_t>(rate) * 2, rate, 2) !=
                SONG_OK) {
                bad.fetch_add(1);
                stop = true;
                return;
            }
            pe_open(eng, &io, nullptr);
            pe_play(eng);
            // Brief playback so media positions advance and the conversion
            // path is exercised (the engine's worker + the locked test driver).
            qn::PlayerEngine* e = reinterpret_cast<qn::PlayerEngine*>(eng);
            e->submit(512);
            e->backend_render(512);
            pe_stop(eng, nullptr);
        }
        stop = true;
    });

    std::thread poller([&] {
        while (!stop.load()) {
            pe_snapshot sn;
            if (pe_get_snapshot(eng, &sn) != PE_OK) continue;
            polls.fetch_add(1);
            if (!sn.duration_known || sn.sample_rate <= 0) continue;
            if (sn.sample_rate != 44100 && sn.sample_rate != 48000) {
                bad.fetch_add(1);
                stop = true;
                return;
            }
            if (sn.duration_us != 2000000) {
                bad.fetch_add(1);
                stop = true;
                return;
            }
            if (sn.position_us < 0 || sn.position_us > sn.duration_us) {
                bad.fetch_add(1);
                stop = true;
                return;
            }
        }
    });

    opener.join();
    poller.join();
    pe_destroy(eng);

    QN_CHECK_MSG(polls.load() > 0, "snapshot-coherent", "no snapshots were taken");
    QN_CHECK_MSG(bad.load() == 0, "snapshot-coherent",
                 "%d incoherent snapshots (rate/frames mismatch)",
                 bad.load());
    std::printf("  concurrent open(44.1k/48k) + snapshot polls: %d polls, all coherent\n",
                polls.load());
}

}  // namespace qn::test
