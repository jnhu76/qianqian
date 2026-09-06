// engine_gates_test.cpp — native semantic gates for the PlayerEngine.
//
// These gates own the frozen PlayerEngine semantics (docs/contracts/player-api.md)
// as permanent regressions: lifecycle (T1–T14), state-illegal probes, clock
// model, submitted-vs-rendered and MEDIA/GAP mapping (S1–S10), the
// ESTIMATED-segment offset invariance, clock-corrective gates (T16–T20),
// and the capacity sweep. Every gate drives the engine through its manual
// API (worker quantum / backend callback / render progression) and checks
// the conservation laws, clock continuity, and content continuity after
// every operation.
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

#include "commit_flush_handshake.hpp"
#include "fake_songcore.hpp"
#include "player_engine.hpp"
#include "test_support.hpp"
#include "wasapi_submit_accounting.hpp"

namespace qn::test {

using qn::EngineSnapshot;
using qn::LandingQuality;
using qn::PlayerEngine;
using qn::PlayerState;
using qn::PlayerStatus;
using qn::StepOutcome;
using qn::StepReport;
using qn::SubmitReport;
using qn::RenderReport;

[[maybe_unused]] static const char* sname(PlayerState s) {
    switch (s) {
        case PlayerState::Empty: return "EMPTY";
        case PlayerState::Ready: return "READY";
        case PlayerState::Playing: return "PLAYING";
        case PlayerState::Paused: return "PAUSED";
        case PlayerState::Ended: return "ENDED";
        case PlayerState::Error: return "ERROR";
    }
    return "?";
}

static bool is_idle_kind(const char* k) { return std::strcmp(k, "idle") == 0; }

// ---------------------------------------------------------------------------
// harness: pair (engine + sink), song builder, manual stepping
// ---------------------------------------------------------------------------

struct Sink {
    PlayerEngine& e;
    std::uint64_t period;
    fake::SongConfig* cfg = nullptr;

    // per-tick accounting (FakeSink mirror)
    std::uint64_t total_requested = 0, total_tick_media = 0, total_tick_silence = 0;
    std::uint64_t total_tick_silence_all = 0;
    std::uint64_t total_submitted_tags = 0;
    SubmitReport last_submit{};
    RenderReport last_render{};

    // rendered content per segment (hidden frame identities, decoded from
    // the fake's sample encoding — test-only truth)
    std::map<std::uint64_t, std::vector<std::int64_t>> segments;
    std::map<std::uint64_t, std::size_t> collected;  // floats already decoded

    explicit Sink(PlayerEngine& engine, std::uint64_t period_frames)
        : e(engine), period(period_frames) {}

    StepReport producer_step() { return e.worker_step(); }

    void tick_submit(std::uint64_t n = 1) {
        for (std::uint64_t i = 0; i < n; ++i) {
            const SubmitReport rep = e.submit(period);
            last_submit = rep;
            total_tick_silence_all += rep.silence_frames;
            total_submitted_tags += rep.media_frames;
            if (!is_idle_kind(rep.kind)) {
                if (rep.media_frames + rep.silence_frames != period) {
                    std::fprintf(stderr, "tick accounting broken\n");
                    std::abort();
                }
                total_requested += period;
                total_tick_media += rep.media_frames;
                total_tick_silence += rep.silence_frames;
            }
        }
    }

    void collect_rendered() {
        for (const auto& entry : e.backend().rendered_log()) {
            std::size_t& done = collected[entry.segment];
            if (done >= entry.pcm.size()) continue;
            auto& tags = segments[entry.segment];
            for (std::size_t i = done; i + 1 < entry.pcm.size(); i += 2) {
                tags.push_back(static_cast<std::int64_t>(llroundf(entry.pcm[i] - 0.25f)));
            }
            done = entry.pcm.size();
        }
    }

    void tick_render(std::int64_t frames) {
        last_render = e.backend_render(frames);
        collect_rendered();
    }

    void tick(std::uint64_t n = 1) {
        for (std::uint64_t i = 0; i < n; ++i) {
            tick_submit(1);
            tick_render(static_cast<std::int64_t>(period));
        }
    }

    void check_accounting(const char* ctx) {
        QN_CHECK(total_requested == total_tick_media + total_tick_silence, ctx);
    }

    std::uint64_t total_output() const { return total_tick_media + total_tick_silence; }
};

struct Pair {
    PlayerEngine engine;
    Sink sink;
    Pair(std::uint64_t cap, std::uint64_t chunk, std::uint64_t period)
        : engine(MakeConfig(cap, chunk)), sink(engine, period) {}

    static qn::EngineConfig MakeConfig(std::uint64_t cap, std::uint64_t chunk) {
        qn::EngineConfig c;
        c.capacity_frames = cap;
        c.read_chunk_frames = chunk;
        c.max_submit_frames = 8192;
        c.max_channels = 8;
        c.worker_thread = false;
        return c;
    }
};

static fake::SongConfig song(std::int64_t total_frames = 48000 * 8) {
    fake::SongConfig c;
    c.total_frames = total_frames;
    c.sample_rate = 48000;
    c.channels = 2;
    return c;
}

static PlayerStatus open_song(PlayerEngine& e, Sink& s, fake::SongConfig& c,
                              std::uint64_t work_steps = 1) {
    c.live.reset();
    song_io io{};
    io.userdata = &c;
    io.read = [](void*, std::uint8_t*, std::size_t) -> std::int64_t { return 0; };
    io.seek = [](void*, std::int64_t) -> std::int64_t { return 0; };
    io.size = [](void*) -> std::int64_t { return 0; };
    s.cfg = &c;
    const PlayerStatus st = e.open(io);
    if (st == PlayerStatus::Ok) e.debug_set_work_steps(work_steps);
    return st;
}

// check_all mirror: everything that must hold after EVERY operation.
struct Seen {
    std::map<std::uint64_t, std::int64_t> verified;  // content frames per segment
};

static void check_segments(PlayerEngine& e, Sink& s, Seen& seen, const char* ctx) {
    for (auto& [seg, tags] : s.segments) {
        std::int64_t prev = seen.verified.count(seg) ? seen.verified[seg] : 0;
        if (tags.empty()) continue;
        std::int64_t anchor;
        if (e.segment_landing_quality(seg) == LandingQuality::Confirmed) {
            anchor = e.segment_anchor(seg);
            QN_CHECK_MSG(tags[0] == anchor, ctx,
                         "segment %llu starts at %lld, committed landing %lld",
                         (unsigned long long)seg, (long long)tags[0],
                         (long long)anchor);
        } else {
            anchor = tags[0];  // hidden truth only; never a production input
        }
        for (std::int64_t i = prev; i < static_cast<std::int64_t>(tags.size()); ++i) {
            QN_CHECK_MSG(tags[i] == anchor + i, ctx, "segment %llu pos %lld: %lld != %lld",
                         (unsigned long long)seg, (long long)i, (long long)tags[i],
                         (long long)(anchor + i));
        }
        seen.verified[seg] = static_cast<std::int64_t>(tags.size());
    }
}

static void check_all(PlayerEngine& e, Sink& s, Seen& seen, const char* ctx) {
    char why[256];
    const EngineSnapshot snap = e.snapshot();
    QN_CHECK(e.ring_debug().check_invariants(why, sizeof why), ctx);
    const std::uint64_t inflight = e.in_flight_frames();
    QN_CHECK_MSG(snap.decoded_source_frames ==
                     e.ring_debug().produced_total() + snap.discarded_stale_media_frames +
                         inflight,
                 ctx, "decode conservation broken");
    QN_CHECK_MSG(snap.submitted_output_frames == snap.pending_output_frames +
                                                    snap.rendered_output_frames +
                                                    snap.discarded_output_frames,
                 ctx, "output conservation broken");
    QN_CHECK_MSG(snap.submitted_media_frames == snap.pending_media_frames +
                                                    snap.rendered_media_frames +
                                                    snap.discarded_output_media_frames,
                 ctx, "media conservation broken");
    QN_CHECK_MSG(snap.rendered_output_frames ==
                     snap.rendered_media_frames + snap.rendered_gap_output_frames,
                 ctx, "rendered output split broken");
    s.check_accounting(ctx);
    check_segments(e, s, seen, ctx);
    if (snap.state == PlayerState::Ended && snap.duration_known) {
        QN_CHECK_MSG(snap.media_position_frames == snap.duration_frames, ctx,
                     "ENDED position != duration");
    } else {
        const std::int64_t base = e.segment_anchor(snap.segment);
        const std::int64_t seg_rendered = e.segment_rendered_media();
        QN_CHECK_MSG(snap.media_position_frames == base + seg_rendered, ctx,
                     "clock continuity: %lld != base %lld + rendered %lld",
                     (long long)snap.media_position_frames, (long long)base,
                     (long long)seg_rendered);
    }
    if (snap.state != PlayerState::Empty) {
        if (snap.duration_known) {
            QN_CHECK_MSG(snap.media_position_frames >= 0 &&
                             snap.media_position_frames <= snap.duration_frames,
                         ctx, "position outside song");
        } else {
            QN_CHECK_MSG(snap.duration_frames == -1, ctx,
                         "unknown duration must stay -1");
        }
    }
}

static void drain_producer(PlayerEngine&, Sink& s, std::uint64_t limit = 4096) {
    for (std::uint64_t i = 0; i < limit; ++i) {
        if (s.producer_step().outcome == StepOutcome::Idle) break;
    }
}

static std::int64_t us(std::int64_t frames, std::int64_t rate = 48000) {
    return qn::frames_to_us(frames, rate);
}

// ---------------------------------------------------------------------------
// T1..T15
// ---------------------------------------------------------------------------

GATE(t1_sequential) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c1 = song();
    QN_CHECK(open_song(p.engine, p.sink, c1) == PlayerStatus::Ok, "t1");
    QN_CHECK(p.engine.play() == PlayerStatus::Ok, "t1");
    for (int i = 0; i < 600; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Playing, "t1");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t1");
    auto& tags = p.sink.segments[snap.segment];
    QN_CHECK(tags.size() == p.sink.total_tick_media, "t1");
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t1");
    }
    QN_CHECK(p.sink.total_tick_media == p.engine.ring_debug().consumed_total(), "t1");
    QN_CHECK(snap.underrun_count == 0, "t1");
    std::printf("  600 lockstep rounds, %llu frames contiguous, 0 underruns\n",
                (unsigned long long)p.sink.total_tick_media);
}

GATE(t2_wraparound) {
    Pair p(8, 8, 3);
    fake::SongConfig c = song(20000);
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t2");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 6000; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t2");
    }
    QN_CHECK(p.sink.total_tick_media >= 5000, "t2");
    std::printf("  cap=8/period=3 x 6000 rounds, %llu frames, wraps verified\n",
                (unsigned long long)p.sink.total_tick_media);
}

GATE(t3_producer_faster) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c3 = song();
    QN_CHECK(open_song(p.engine, p.sink, c3) == PlayerStatus::Ok, "t3");
    p.engine.play();
    Seen seen;
    bool saw_backpressure = false;
    for (int i = 0; i < 600; ++i) {
        for (int k = 0; k < 4; ++k) {
            if (p.sink.producer_step().outcome == StepOutcome::Backpressure) {
                saw_backpressure = true;
            }
        }
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t3");
    }
    QN_CHECK(saw_backpressure, "t3: producer never hit the bounded-queue wall");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.queued_media_frames <= snap.capacity_frames, "t3");
    QN_CHECK(snap.underrun_count == 0, "t3");
    QN_CHECK(p.sink.total_tick_media == p.engine.ring_debug().consumed_total(), "t3");
    std::printf("  producer 4x faster: backpressure seen, buffered<=cap, 0 underruns\n");
}

GATE(t4_consumer_faster) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c, /*work_steps=*/3) == PlayerStatus::Ok, "t4");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 900; ++i) {
        p.sink.producer_step();
        p.sink.tick(3);
        check_all(p.engine, p.sink, seen, "t4");
    }
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_count > 0, "t4: expected starvation underruns");
    QN_CHECK(snap.underrun_silence_output_frames > 0, "t4");
    QN_CHECK(snap.preroll_events >= 1, "t4");
    auto& tags = p.sink.segments[snap.segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t4: order intact");
    }
    QN_CHECK(p.sink.total_tick_media == p.engine.ring_debug().consumed_total(), "t4");

    // long producer stall in the middle of playback
    Pair q(16384, 1024, 512);
    fake::SongConfig cs = song();
    QN_CHECK(open_song(q.engine, q.sink, cs) == PlayerStatus::Ok, "t4-stall");
    q.engine.play();
    Seen seen2;
    for (int i = 0; i < 12; ++i) q.sink.producer_step();
    q.sink.tick(4);
    const std::int64_t pos_stall = q.engine.snapshot().media_position_frames;
    const std::uint64_t buffered = q.engine.snapshot().queued_media_frames;
    const std::uint64_t out_before = q.engine.snapshot().rendered_output_frames;
    q.sink.tick(40);  // producer frozen: pure starvation
    EngineSnapshot s2 = q.engine.snapshot();
    QN_CHECK(s2.underrun_count > 0, "t4-stall");
    QN_CHECK(s2.rendered_output_frames == out_before + 40 * q.sink.period, "t4-stall");
    QN_CHECK(s2.media_position_frames == pos_stall + static_cast<std::int64_t>(buffered),
             "t4-stall: silence must advance device time only");
    for (int i = 0; i < 12; ++i) q.sink.producer_step();
    q.sink.tick(4);
    auto& t2 = q.sink.segments[q.engine.snapshot().segment];
    for (std::size_t i = 0; i < t2.size(); ++i) {
        QN_CHECK(t2[i] == static_cast<std::int64_t>(i), "t4-stall: frames intact");
    }
    check_all(q.engine, q.sink, seen2, "t4-stall");
    std::printf("  consumer 3x faster: %llu underruns; stall: device-only advance OK\n",
                (unsigned long long)s2.underrun_count);
}

GATE(t5_pause_resume) {
    // pause mid-buffer
    Pair p(16384, 1024, 512);
    fake::SongConfig c5 = song();
    QN_CHECK(open_song(p.engine, p.sink, c5) == PlayerStatus::Ok, "t5");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 12; ++i) p.sink.producer_step();
    p.sink.tick(5);
    p.engine.pause();
    QN_CHECK(p.engine.snapshot().state == PlayerState::Paused, "t5");
    const std::int64_t pos = p.engine.snapshot().media_position_frames;
    const std::int64_t emitted = p.sink.cfg->live->emitted_total;
    const std::uint64_t buffered = p.engine.snapshot().queued_media_frames;
    p.sink.tick(5);  // device must not progress while paused
    QN_CHECK(is_idle_kind(p.sink.last_submit.kind), "t5");
    QN_CHECK(std::strcmp(p.sink.last_render.kind, "paused") == 0, "t5");
    QN_CHECK(p.engine.snapshot().media_position_frames == pos, "t5");
    QN_CHECK(p.engine.snapshot().queued_media_frames == buffered, "t5");
    p.engine.play();
    for (int i = 0; i < 30; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    check_all(p.engine, p.sink, seen, "t5");
    QN_CHECK(p.sink.cfg->live->emitted_total >= emitted, "t5: retained, not re-decoded");
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t5: pause/resume broke stream");
    }
    QN_CHECK(p.engine.snapshot().underrun_count == 0, "t5");

    // pause at the buffer boundary (queue completely full)
    Pair q(4096, 1024, 512);
    fake::SongConfig c5b = song();
    QN_CHECK(open_song(q.engine, q.sink, c5b) == PlayerStatus::Ok, "t5-boundary");
    q.engine.play();
    while (q.sink.producer_step().outcome != StepOutcome::Backpressure) {}
    QN_CHECK(q.engine.snapshot().queued_media_frames == q.engine.snapshot().capacity_frames,
             "t5-boundary");
    q.engine.pause();
    q.sink.tick(3);
    QN_CHECK(is_idle_kind(q.sink.last_submit.kind), "t5-boundary");
    q.engine.play();
    q.sink.tick();
    QN_CHECK(q.sink.segments[q.engine.snapshot().segment][0] == 0, "t5-boundary");
    Seen seen2;
    check_all(q.engine, q.sink, seen2, "t5-boundary");
    std::printf("  pause mid-buffer and at full boundary: frozen clock, buffer retained\n");
}

GATE(t6_stop_restart) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c6 = song();
    QN_CHECK(open_song(p.engine, p.sink, c6, 2) == PlayerStatus::Ok, "t6");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 10; ++i) p.sink.producer_step();
    p.sink.tick(4);
    p.engine.stop();
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ready, "t6");
    QN_CHECK(snap.media_position_frames == 0, "t6");
    QN_CHECK(snap.queued_media_frames == 0, "t6");
    // a chunk in flight at stop() belongs to the dead epoch and dies at
    // publish time
    p.sink.producer_step();  // decays the stale chunk (WORKING)
    p.sink.producer_step();  // publishes -> STALE
    snap = p.engine.snapshot();
    QN_CHECK(snap.discarded_stale_media_frames > 0, "t6");
    QN_CHECK(snap.decoded_source_frames == p.engine.ring_debug().produced_total() +
                                              snap.discarded_stale_media_frames,
             "t6");
    p.engine.play();
    for (int i = 0; i < 40; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t6");
    }
    snap = p.engine.snapshot();
    auto& tags = p.sink.segments[snap.segment];
    QN_CHECK(!tags.empty() && tags[0] == 0, "t6: restart must begin at 0");
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t6");
    }
    std::printf("  stop->READY@0, buffered=0, stale=%llu, restart contiguous\n",
                (unsigned long long)snap.discarded_stale_media_frames);
}

GATE(t7_seek_playing) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    c.seek_landing_offset = -37;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t7");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 10; ++i) p.sink.producer_step();
    p.sink.tick(4);
    std::int64_t landing = 0;
    QN_CHECK(p.engine.seek(us(48000 * 4), &landing) == PlayerStatus::Ok, "t7");
    QN_CHECK(landing == 48000 * 4 - 37, "t7: engine must rebase on the RETURNED landing");
    QN_CHECK(p.engine.snapshot().media_position_frames == landing, "t7");
    QN_CHECK(p.engine.snapshot().position_quality == LandingQuality::Confirmed, "t7");
    QN_CHECK(p.engine.snapshot().queued_media_frames == 0, "t7");
    QN_CHECK(p.engine.snapshot().pending_output_frames == 0, "t7");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Playing, "t7");
    for (int i = 0; i < 60; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t7");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == landing, "t7: first post-seek frame = landing");
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == landing + static_cast<std::int64_t>(i), "t7");
    }
    std::printf("  seek@4s landing=%lld (offset -37): post-seek stream starts there\n",
                (long long)landing);
}

GATE(t8_seek_paused) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c8 = song();
    QN_CHECK(open_song(p.engine, p.sink, c8) == PlayerStatus::Ok, "t8");
    p.engine.play();
    for (int i = 0; i < 8; ++i) p.sink.producer_step();
    p.sink.tick(3);
    p.engine.pause();
    std::int64_t landing = 0;
    QN_CHECK(p.engine.seek(us(48000 * 2), &landing) == PlayerStatus::Ok, "t8");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Paused, "t8");
    QN_CHECK(p.engine.snapshot().media_position_frames == landing, "t8");
    QN_CHECK(p.engine.snapshot().queued_media_frames == 0, "t8");
    Seen seen;
    p.engine.play();
    for (int i = 0; i < 50; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t8");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == landing + static_cast<std::int64_t>(i), "t8");
    }
    std::printf("  seek while paused: resume plays from landing %lld, contiguous\n",
                (long long)landing);
}

GATE(t9_repeated_seeks) {
    Pair p(512, 256, 128);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c, 2) == PlayerStatus::Ok, "t9");
    p.engine.play();
    Seen seen;
    // deterministic prelude: one in-flight chunk killed by a seek (guarantees
    // stale evidence independent of rng)
    p.sink.producer_step();  // BEGIN (work_steps=2)
    p.engine.seek(us(1000));
    p.sink.producer_step();  // WORKING
    p.sink.producer_step();  // STALE
    Rng rng(7);
    for (int i = 0; i < 30; ++i) {
        const std::int64_t total = p.engine.snapshot().duration_frames;
        const std::int64_t target = rng.below(static_cast<std::uint64_t>(total + 1));
        p.engine.seek(us(target));
        const int steps = rng.uniform(0, 3);
        for (int k = 0; k < steps; ++k) p.sink.producer_step();
        p.sink.tick(static_cast<std::uint64_t>(rng.uniform(0, 2)));
        check_all(p.engine, p.sink, seen, "t9");
    }
    for (int i = 0; i < 40; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    check_all(p.engine, p.sink, seen, "t9-final");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.discarded_stale_media_frames > 0, "t9: rapid seeks must kill chunks");
    auto& tags = p.sink.segments[snap.segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == tags[0] + static_cast<std::int64_t>(i), "t9");
    }
    std::printf("  30 rapid seeks (cap 512): stale=%llu, final segment clean\n",
                (unsigned long long)snap.discarded_stale_media_frames);
}

GATE(t10_stale_after_seek) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c, 3) == PlayerStatus::Ok, "t10");
    p.engine.play();
    StepReport rep = p.sink.producer_step();
    QN_CHECK(rep.outcome == StepOutcome::Begin, "t10");
    const std::uint64_t flight_len = rep.frames;
    p.engine.seek(us(48000));
    p.sink.tick_submit(2);  // callbacks during in-flight decay: silence only
    QN_CHECK(p.sink.segments[p.engine.snapshot().segment].empty(), "t10");
    rep = p.sink.producer_step();
    QN_CHECK(rep.outcome == StepOutcome::Working, "t10");
    rep = p.sink.producer_step();
    QN_CHECK(rep.outcome == StepOutcome::Working, "t10");
    rep = p.sink.producer_step();
    QN_CHECK(rep.outcome == StepOutcome::Stale, "t10");
    QN_CHECK(rep.frames == flight_len, "t10");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.discarded_stale_media_frames == flight_len, "t10");
    QN_CHECK(snap.queued_media_frames == 0, "t10");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t10");
    for (int i = 0; i < 30; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == 48000 + static_cast<std::int64_t>(i), "t10: stale PCM escaped");
    }
    std::printf("  in-flight chunk (%llu f) discarded after seek; stream clean\n",
                (unsigned long long)flight_len);
}

GATE(t11_eof_drain) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song(3000);
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t11");
    p.engine.play();
    drain_producer(p.engine, p.sink);
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.source_eof, "t11");
    QN_CHECK(snap.queued_media_frames > 0, "t11");
    QN_CHECK(snap.state == PlayerState::Playing, "t11: EOF must not shortcut to ENDED");
    int ticks = 0;
    while (p.engine.snapshot().state != PlayerState::Ended) {
        p.sink.tick();
        QN_CHECK(++ticks < 100, "t11: drain did not finish");
    }
    snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == snap.duration_frames, "t11");
    QN_CHECK(std::strcmp(p.sink.last_submit.kind, "eos") == 0, "t11");
    QN_CHECK(snap.underrun_count == 0, "t11");
    auto& tags = p.sink.segments[snap.segment];
    QN_CHECK(tags.size() == 3000, "t11");
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t11");
    }
    std::printf("  EOF->drain(%d ticks)->ENDED @duration, eos-silence=%llu\n", ticks,
                (unsigned long long)snap.eos_silence_output_frames);

    // very short song: shorter than one device period
    Pair q(16384, 1024, 512);
    fake::SongConfig cshort = song(100);
    QN_CHECK(open_song(q.engine, q.sink, cshort) == PlayerStatus::Ok, "t11-short");
    q.engine.play();
    drain_producer(q.engine, q.sink);
    q.sink.tick();
    EngineSnapshot s2 = q.engine.snapshot();
    QN_CHECK(s2.state == PlayerState::Ended, "t11-short");
    QN_CHECK(s2.media_position_frames == 100, "t11-short");
    QN_CHECK(s2.preroll_events == 0 && s2.underrun_count == 0, "t11-short");

    // EOF with exactly one frame still buffered
    Pair r(16384, 1024, 256);
    fake::SongConfig ct513 = song(513);
    QN_CHECK(open_song(r.engine, r.sink, ct513) == PlayerStatus::Ok, "t11-tail");
    r.engine.play();
    drain_producer(r.engine, r.sink);
    while (r.engine.snapshot().state != PlayerState::Ended) r.sink.tick();
    auto& t3 = r.sink.segments[r.engine.snapshot().segment];
    QN_CHECK(t3.back() == 512, "t11-tail");
    QN_CHECK(r.engine.snapshot().media_position_frames == 513, "t11-tail");

    // pause near EOF: ENDED must wait until playback resumes and drains
    Pair w(16384, 1024, 256);
    fake::SongConfig cw600 = song(600);
    QN_CHECK(open_song(w.engine, w.sink, cw600) == PlayerStatus::Ok, "t11-pause");
    w.engine.play();
    drain_producer(w.engine, w.sink);
    w.sink.tick(1);
    w.engine.pause();
    QN_CHECK(w.engine.snapshot().state == PlayerState::Paused, "t11-pause");
    w.sink.tick(3);
    QN_CHECK(w.engine.snapshot().state == PlayerState::Paused,
             "t11-pause: drain must not complete while paused");
    w.engine.play();
    while (w.engine.snapshot().state != PlayerState::Ended) w.sink.tick();
    QN_CHECK(w.engine.snapshot().media_position_frames == 600, "t11-pause");
    std::printf("  short-song / one-frame-tail / pause-near-EOF variants OK\n");
}

GATE(t12_after_eof) {
    Pair p(16384, 1024, 256);
    fake::SongConfig c12 = song(4000);
    QN_CHECK(open_song(p.engine, p.sink, c12) == PlayerStatus::Ok, "t12");
    p.engine.play();
    drain_producer(p.engine, p.sink);
    while (p.engine.snapshot().state != PlayerState::Ended) p.sink.tick();
    std::int64_t landing = 0;
    QN_CHECK(p.engine.seek(us(1000), &landing) == PlayerStatus::Ok, "t12");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Ready, "t12");
    QN_CHECK(!p.engine.snapshot().source_eof, "t12");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 40; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == landing + static_cast<std::int64_t>(i), "t12");
    }
    // play after ENDED restarts from 0 (frozen policy)
    drain_producer(p.engine, p.sink);
    while (p.engine.snapshot().state != PlayerState::Ended) p.sink.tick();
    p.engine.play();
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Playing, "t12");
    QN_CHECK(snap.media_position_frames == 0, "t12");
    QN_CHECK(snap.queued_media_frames == 0, "t12");
    for (int i = 0; i < 10; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    auto& t2 = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!t2.empty() && t2[0] == 0, "t12");
    for (std::size_t i = 0; i < t2.size(); ++i) {
        QN_CHECK(t2[i] == static_cast<std::int64_t>(i), "t12");
    }
    check_all(p.engine, p.sink, seen, "t12");
    p.engine.stop();
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ready && snap.media_position_frames == 0, "t12");
    std::printf("  seek/play/stop after EOF: restart-from-0 policy verified\n");
}

GATE(t13_open_while_playing) {
    Pair p(16384, 1024, 512);
    fake::SongConfig a = song(2000);
    QN_CHECK(open_song(p.engine, p.sink, a, 3) == PlayerStatus::Ok, "t13");
    p.engine.play();
    StepReport rep = p.sink.producer_step();
    QN_CHECK(rep.outcome == StepOutcome::Begin, "t13");
    p.sink.tick_submit(2);
    QN_CHECK(p.engine.snapshot().pending_output_frames > 0, "t13");
    fake::SongConfig b = song(5000);
    QN_CHECK(open_song(p.engine, p.sink, b) == PlayerStatus::Ok, "t13");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ready, "t13");
    QN_CHECK(snap.queued_media_frames == 0, "t13");
    QN_CHECK(snap.pending_output_frames == 0, "t13: open must drop old pending output");
    QN_CHECK(snap.media_position_frames == 0, "t13");
    for (int i = 0; i < 3; ++i) rep = p.sink.producer_step();
    QN_CHECK(rep.outcome == StepOutcome::Stale, "t13: old song's in-flight chunk must die");
    Seen seen;
    p.engine.play();
    for (int i = 0; i < 80; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t13");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t13");
    }
    QN_CHECK(tags.size() > 2000, "t13: new song must play past the old length");
    std::printf("  open-during-play: old chunk stale, new song contiguous past 2000\n");
}

GATE(t14_error_injection) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song(5000);
    c.fail_at_frame = 1500;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t14");
    p.engine.play();
    drain_producer(p.engine, p.sink);
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Error, "t14");
    QN_CHECK(std::strstr(snap.last_error, "108") != nullptr, "t14");
    // partial-success framing: frames before the fault were emitted
    QN_CHECK(p.sink.cfg->live->emitted_total == 1500, "t14");
    p.sink.tick(3);
    QN_CHECK(is_idle_kind(p.sink.last_submit.kind), "t14");
    QN_CHECK(p.engine.play() == PlayerStatus::ErrIllegalCall, "t14");
    QN_CHECK(p.engine.seek(0) == PlayerStatus::ErrIllegalCall, "t14");
    p.engine.stop();  // universal recovery (reopen path from ERROR)
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ready, "t14");
    QN_CHECK(snap.media_position_frames == 0, "t14");
    fake::SongConfig d = song();
    QN_CHECK(open_song(p.engine, p.sink, d) == PlayerStatus::Ok, "t14");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 30; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "t14");
    }
    std::printf("  decode fault -> ERROR (partial success honored), stop/open recover\n");
}

GATE(state_illegal_probes) {
    Pair p(16384, 1024, 512);
    QN_CHECK(p.engine.play() == PlayerStatus::ErrIllegalCall, "probes: EMPTY play");
    QN_CHECK(p.engine.seek(0) == PlayerStatus::ErrIllegalCall, "probes: EMPTY seek");
    p.engine.stop();   // stop on EMPTY is a documented no-op
    p.engine.pause();  // documented no-op
    QN_CHECK(p.engine.snapshot().state == PlayerState::Empty, "probes");
    fake::SongConfig bad = song();
    bad.open_fails = true;
    QN_CHECK(open_song(p.engine, p.sink, bad) == PlayerStatus::ErrOpenFailed, "probes");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Empty, "probes");
    QN_CHECK(p.sink.cfg->live == nullptr, "probes: no fake state leaked");
    fake::SongConfig good = song();
    QN_CHECK(open_song(p.engine, p.sink, good) == PlayerStatus::Ok, "probes");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Ready, "probes");
    std::printf("  EMPTY probes + failed open stays EMPTY, then recovers\n");
}

GATE(clock_model) {
    const std::uint64_t period = 512;
    Pair p(16384, 1024, period);
    fake::SongConfig cc = song();
    QN_CHECK(open_song(p.engine, p.sink, cc) == PlayerStatus::Ok, "clock");
    p.engine.play();
    for (int i = 0; i < 12; ++i) p.sink.producer_step();  // decode far ahead
    QN_CHECK(p.engine.snapshot().media_position_frames == 0,
             "clock: buffering must not move the clock");
    p.sink.tick(4);
    QN_CHECK(p.engine.snapshot().media_position_frames ==
                 static_cast<std::int64_t>(4 * period),
             "clock");
    QN_CHECK(p.engine.snapshot().preroll_events == 0, "clock");
    p.engine.pause();
    p.sink.tick(3);
    QN_CHECK(p.engine.snapshot().media_position_frames ==
                 static_cast<std::int64_t>(4 * period),
             "clock: pause must freeze");
    p.engine.play();
    p.sink.tick();
    QN_CHECK(p.engine.snapshot().media_position_frames ==
                 static_cast<std::int64_t>(5 * period),
             "clock: resume must continue");
    std::int64_t landing = 0;
    p.engine.seek(us(96000), &landing);
    QN_CHECK(p.engine.snapshot().media_position_frames == landing,
             "clock: seek must rebase");
    Seen seen;
    check_all(p.engine, p.sink, seen, "clock");

    // preroll vs underrun accounting, device time vs media time
    Pair q(16384, 1024, period);
    fake::SongConfig cc2 = song();
    QN_CHECK(open_song(q.engine, q.sink, cc2, 4) == PlayerStatus::Ok, "clock2");
    q.engine.play();
    q.sink.tick();  // nothing decoded yet: preroll, clock stays at 0
    QN_CHECK(std::strcmp(q.sink.last_submit.kind, "preroll") == 0, "clock2");
    QN_CHECK(q.engine.snapshot().media_position_frames == 0, "clock2");
    QN_CHECK(q.engine.snapshot().preroll_events == 1, "clock2");
    for (int i = 0; i < 5; ++i) q.sink.producer_step();  // work_steps=4
    q.sink.tick();
    QN_CHECK(q.engine.snapshot().media_position_frames ==
                 static_cast<std::int64_t>(period),
             "clock2: clock starts at first real frame");
    q.sink.tick(3);  // producer starved: underrun GAP
    QN_CHECK(q.engine.snapshot().underrun_count >= 1, "clock2");
    EngineSnapshot s2 = q.engine.snapshot();
    QN_CHECK(s2.rendered_output_frames == 5 * period, "clock2: device advanced");
    QN_CHECK(s2.media_position_frames == static_cast<std::int64_t>(2 * period),
             "clock2: underrun GAP advances device time but not media position");
    Seen seen2;
    check_all(q.engine, q.sink, seen2, "clock2");
    std::printf("  decode-ahead/pause/resume/seek-rebase/preroll/underrun clock rules OK\n");
}

// ---------------------------------------------------------------------------
// S1..S10
// ---------------------------------------------------------------------------

static void expect_seek_error(PlayerEngine& e, Sink&, Seen&, std::int64_t position_us,
                              std::int32_t status, const char* ctx) {
    const EngineSnapshot pre = e.snapshot();
    std::int32_t song_status = 0;
    QN_CHECK_MSG(e.seek(position_us, nullptr, &song_status) == PlayerStatus::ErrSeekFailed,
                 ctx, "seek must fail");
    QN_CHECK(song_status == status, ctx);
    const EngineSnapshot post = e.snapshot();
    QN_CHECK(post.state == PlayerState::Error, ctx);
    QN_CHECK(std::strstr(post.last_error, std::to_string(status).c_str()) != nullptr,
             ctx);
    QN_CHECK(post.epoch == pre.epoch + 1, ctx);
    QN_CHECK(post.queued_media_frames == 0, ctx);
    QN_CHECK(post.pending_output_frames == 0, ctx);
    QN_CHECK(post.media_position_frames == pre.media_position_frames, ctx);
}

GATE(s1_submitted_not_audible) {
    Pair p(1000, 100, 100);
    fake::SongConfig c = song(10000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s1");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();
    const SubmitReport sub = p.engine.submit(50);
    QN_CHECK(std::strcmp(sub.kind, "audio") == 0 && sub.media_frames == 50, "s1");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.submitted_output_frames == 50, "s1");
    QN_CHECK(snap.rendered_output_frames == 0, "s1");
    QN_CHECK(snap.pending_output_frames == 50, "s1");
    QN_CHECK(snap.media_position_frames == 0, "s1: submitted-only must not advance");
    p.engine.backend_render(0);
    QN_CHECK(p.engine.snapshot().media_position_frames == 0, "s1");
    p.engine.backend_render(50);
    QN_CHECK(p.engine.snapshot().media_position_frames == 50, "s1");
    Seen seen;
    check_all(p.engine, p.sink, seen, "s1");
    std::printf("  submit 50 / render 0 -> position 0; render 50 -> position 50\n");
}

GATE(s2_partial_render_only_audible) {
    Pair p(1000, 100, 100);
    fake::SongConfig c = song(10000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s2");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();
    p.engine.submit(50);
    const RenderReport rep = p.engine.backend_render(20);
    QN_CHECK(rep.rendered_output_frames == 20 && rep.rendered_media_frames == 20, "s2");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == 20, "s2");
    QN_CHECK(snap.pending_output_frames == 30, "s2");
    p.engine.submit(50);
    p.engine.pause();
    const RenderReport paused = p.engine.backend_render(10);
    QN_CHECK(std::strcmp(paused.kind, "paused") == 0, "s2");
    snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == 20 && snap.pending_output_frames == 80, "s2");
    QN_CHECK(snap.rendered_output_frames == 20, "s2");
    p.engine.play();
    p.engine.backend_render(80);
    QN_CHECK(p.engine.snapshot().media_position_frames == 100, "s2");
    Seen seen;
    check_all(p.engine, p.sink, seen, "s2");
    std::printf("  partial render advances only audible media; pause holds pending\n");
}

GATE(s3_media_gap_output_mapping) {
    Pair p(1000, 10, 10);
    fake::SongConfig c = song(1000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s3");
    p.engine.play();
    p.sink.producer_step();
    p.sink.producer_step();  // 10 frames published
    QN_CHECK(std::strcmp(p.engine.submit(10).kind, "audio") == 0, "s3");
    const SubmitReport sub = p.engine.submit(5);
    QN_CHECK(std::strcmp(sub.kind, "underrun") == 0 && sub.silence_frames == 5, "s3");
    p.sink.producer_step();
    p.sink.producer_step();
    QN_CHECK(std::strcmp(p.engine.submit(10).kind, "audio") == 0, "s3");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.submitted_output_frames == 25, "s3");
    p.engine.backend_render(25);
    const std::int64_t base = p.engine.segment_anchor(p.engine.snapshot().segment);
    const std::pair<std::int64_t, std::int64_t> cases[] = {
        {5, 5}, {12, 10}, {15, 10}, {20, 15}, {25, 20}};
    for (auto [out_pos, want_media] : cases) {
        QN_CHECK(p.engine.media_position_at_output(static_cast<std::uint64_t>(out_pos)) ==
                     base + want_media,
                 "s3: device->media mapping");
    }
    snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == 20, "s3");
    QN_CHECK(snap.rendered_output_frames == 25, "s3");
    QN_CHECK(snap.rendered_media_frames == 20, "s3");
    Seen seen;
    check_all(p.engine, p.sink, seen, "s3");
    std::printf("  10 MEDIA + 5 GAP + 10 MEDIA: mapping exact at 5/12/15/20/25\n");
}

GATE(s4_seek_failure_deterministic_error) {
    // READY
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song(2000);
    c.seek_status = 109;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s4-ready");
    Seen seen;
    expect_seek_error(p.engine, p.sink, seen, us(1000), 109, "s4-ready");
    p.sink.tick(2);
    QN_CHECK(is_idle_kind(p.sink.last_submit.kind), "s4-ready: no stale PCM consumable");
    p.engine.stop();
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ready && snap.media_position_frames == 0, "s4");
    p.engine.play();
    for (int i = 0; i < 20; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == 0, "s4");
    check_all(p.engine, p.sink, seen, "s4-ready-recovered");

    // PLAYING
    Pair q(16384, 1024, 512);
    fake::SongConfig c2 = song(2000);
    c2.seek_status = 110;
    QN_CHECK(open_song(q.engine, q.sink, c2) == PlayerStatus::Ok, "s4-playing");
    q.engine.play();
    for (int i = 0; i < 6; ++i) q.sink.producer_step();
    q.sink.tick(2);
    Seen seen2;
    expect_seek_error(q.engine, q.sink, seen2, us(1500), 110, "s4-playing");
    q.engine.stop();
    QN_CHECK(q.engine.snapshot().state == PlayerState::Ready, "s4-playing");

    // PAUSED
    Pair r(16384, 1024, 512);
    QN_CHECK(open_song(r.engine, r.sink, c2) == PlayerStatus::Ok, "s4-paused");
    r.engine.play();
    for (int i = 0; i < 6; ++i) r.sink.producer_step();
    r.sink.tick(2);
    r.engine.pause();
    Seen seen3;
    expect_seek_error(r.engine, r.sink, seen3, us(1500), 110, "s4-paused");

    // ENDED (failure wins over the -> READY transition)
    Pair w(16384, 1024, 256);
    fake::SongConfig c4 = song(500);
    c4.seek_status = 110;
    QN_CHECK(open_song(w.engine, w.sink, c4) == PlayerStatus::Ok, "s4-ended");
    w.engine.play();
    drain_producer(w.engine, w.sink);
    while (w.engine.snapshot().state != PlayerState::Ended) w.sink.tick();
    Seen seen4;
    expect_seek_error(w.engine, w.sink, seen4, 250, 110, "s4-ended");
    std::printf("  READY/PLAYING/PAUSED/ENDED + failed seek -> ERROR, stop recovers\n");
}

GATE(s5_seek_failure_after_underrun) {
    Pair p(16384, 1024, 256);
    fake::SongConfig c = song(4000);
    c.seek_status = 110;
    QN_CHECK(open_song(p.engine, p.sink, c, 4) == PlayerStatus::Ok, "s5");
    p.engine.play();
    p.sink.tick();  // preroll
    for (int i = 0; i < 5; ++i) p.sink.producer_step();
    p.sink.tick();
    p.sink.tick(5);  // starve past the buffered media
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_count >= 1, "s5");
    QN_CHECK(snap.media_position_frames > 0, "s5");
    Seen seen;
    expect_seek_error(p.engine, p.sink, seen, us(1000), 110, "s5-underrun");
    p.engine.stop();  // from ERROR: drop + reopen the source
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ready, "s5");
    QN_CHECK(snap.media_position_frames == 0, "s5");
    QN_CHECK(p.sink.cfg->live->reopen_count == 1, "s5");
    p.engine.play();
    for (int i = 0; i < 40; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "s5");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == 0, "s5");

    // negative: even the reopen fails -> ERROR persists, open() recovers
    Pair q(16384, 1024, 256);
    fake::SongConfig c2 = song(1000);
    c2.seek_status = 109;
    c2.reopen_fails = true;
    QN_CHECK(open_song(q.engine, q.sink, c2) == PlayerStatus::Ok, "s5-reopen-fails");
    q.engine.play();
    for (int i = 0; i < 6; ++i) q.sink.producer_step();
    q.sink.tick(2);
    Seen seen2;
    expect_seek_error(q.engine, q.sink, seen2, us(500), 109, "s5-reopen-fails");
    q.engine.stop();
    EngineSnapshot s2 = q.engine.snapshot();
    QN_CHECK(s2.state == PlayerState::Error, "s5: failed reopen must not fake READY");
    QN_CHECK(std::strstr(s2.last_error, "stop") != nullptr, "s5");
    fake::SongConfig c3 = song(2000);
    QN_CHECK(open_song(q.engine, q.sink, c3) == PlayerStatus::Ok, "s5-recover");
    QN_CHECK(q.engine.snapshot().state == PlayerState::Ready, "s5");
    std::printf("  underrun+failed seek -> ERROR; stop reopens; reopen-fail keeps ERROR\n");
}

GATE(s6_unknown_duration_eof_endpoint) {
    Pair p(200, 10, 10);
    fake::SongConfig c = song(100);
    c.sample_rate = 100;
    c.unknown_duration = true;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s6");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(!snap.duration_known, "s6");
    QN_CHECK(snap.duration_frames == -1, "s6");
    p.engine.play();
    p.sink.producer_step();
    p.sink.producer_step();  // 10 frames
    p.sink.tick();
    p.sink.tick(2);  // 20 cumulative underrun GAP frames
    snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_silence_output_frames == 20, "s6");
    QN_CHECK(snap.media_position_frames == 10, "s6: GAP must not advance media position");
    drain_producer(p.engine, p.sink);  // 90 more frames, source EOF
    while (p.engine.snapshot().state != PlayerState::Ended) p.sink.tick();
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ended, "s6");
    QN_CHECK(snap.media_position_frames == 100, "s6: ENDED = final rendered endpoint");
    QN_CHECK(snap.rendered_output_frames == 120, "s6: device elapsed 1.2 s");
    QN_CHECK(snap.duration_frames == -1, "s6");
    Seen seen;
    check_all(p.engine, p.sink, seen, "s6");
    std::printf("  100 media + 20 underrun silence: ENDED @1.0 s media / 1.2 s device\n");
}

GATE(s7_confirmed_landing) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song(48000 * 8);
    c.seek_landing_offset = -3840;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s7");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    std::int64_t landing = 0;
    p.engine.seek(us(48000 * 5), &landing);
    QN_CHECK(landing == 48000 * 5 - 3840, "s7");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == landing, "s7");
    QN_CHECK(snap.position_quality == LandingQuality::Confirmed, "s7");
    QN_CHECK(snap.media_position_frames != 48000 * 5, "s7: request must not stay authority");
    Seen seen;
    for (int i = 0; i < 20; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == landing, "s7");
    check_all(p.engine, p.sink, seen, "s7");
    std::printf("  requested 5.000 s -> landing %lld (4.920 s), CONFIRMED\n",
                (long long)landing);
}

GATE(s8_estimated_landing) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song(48000 * 8);
    c.seek_landing_unknown = true;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s8");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const std::int64_t requested = 48000 * 5;
    std::int64_t landing = 0;
    p.engine.seek(us(requested), &landing);
    QN_CHECK(landing == requested, "s8: unknown landing -> base = requested clamp");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.position_quality == LandingQuality::Estimated, "s8");
    QN_CHECK(snap.state == PlayerState::Playing, "s8");
    Seen seen;
    for (int i = 0; i < 20; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "s8");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == requested, "s8: media decodes from the clamp");
    snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == requested + static_cast<std::int64_t>(tags.size()),
             "s8");
    std::printf("  OK + landing -1 -> ESTIMATED @5.000 s, playback progresses\n");
}

GATE(s9_pending_old_output_cannot_advance_new_song) {
    Pair p(1000, 100, 100);
    fake::SongConfig a = song(10000);
    a.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, a) == PlayerStatus::Ok, "s9");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();
    p.engine.submit(50);
    p.engine.backend_render(20);
    QN_CHECK(p.engine.snapshot().media_position_frames == 20, "s9");
    const std::int64_t old_gen = static_cast<std::int64_t>(p.engine.snapshot().epoch);
    fake::SongConfig b = song(5000);
    b.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, b) == PlayerStatus::Ok, "s9");
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.pending_output_frames == 0, "s9: open must drop pending output");
    QN_CHECK(snap.discarded_output_media_frames == 30, "s9");
    const RenderReport rep = p.engine.backend_render(30, old_gen);
    QN_CHECK(std::strcmp(rep.kind, "stale") == 0, "s9");
    snap = p.engine.snapshot();
    QN_CHECK(snap.stale_render_events == 1, "s9");
    QN_CHECK(snap.media_position_frames == 0, "s9: old generation must not move Song B");
    QN_CHECK(snap.rendered_output_frames == 20, "s9");
    p.engine.play();
    Seen seen;
    for (int i = 0; i < 10; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "s9");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == 0, "s9");
    std::printf("  30 pending Song-A frames dropped at open; late render discarded\n");
}

GATE(s10_eof_waits_for_submitted_playout) {
    Pair p(1000, 60, 50);
    fake::SongConfig c = song(60);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "s10");
    p.engine.play();
    drain_producer(p.engine, p.sink);
    p.engine.submit(50);
    p.engine.submit(10);  // final partial period: 10 media + 40 EOS GAP
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Playing,
             "s10: submitted-but-unrendered media must NOT be ENDED");
    QN_CHECK(snap.pending_media_frames == 60, "s10");
    p.engine.backend_render(10);
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Playing && snap.media_position_frames == 10,
             "s10");
    p.engine.backend_render(50);
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ended, "s10");
    QN_CHECK(snap.media_position_frames == 60 && snap.duration_frames == 60, "s10");

    // trailing backend padding must not postpone ENDED
    Pair q(1000, 60, 60);
    fake::SongConfig c2 = song(55);
    c2.sample_rate = 100;
    QN_CHECK(open_song(q.engine, q.sink, c2) == PlayerStatus::Ok, "s10-pad");
    q.engine.play();
    drain_producer(q.engine, q.sink);
    const SubmitReport sub = q.engine.submit(60);
    QN_CHECK(std::strcmp(sub.kind, "eos") == 0 && sub.silence_frames == 5, "s10-pad");
    q.engine.backend_render(55);
    EngineSnapshot s2 = q.engine.snapshot();
    QN_CHECK(s2.state == PlayerState::Ended, "s10-pad: EOS GAP must not postpone");
    QN_CHECK(s2.pending_output_frames == 5, "s10-pad");
    QN_CHECK(s2.media_position_frames == 55, "s10-pad");
    std::printf("  submitted media: ENDED only after render; EOS padding never postpones\n");
}

GATE(estimated_segment_offset_invariance) {
    const std::int64_t rate = 48000;
    const std::int64_t offset = 1776;  // +37 ms of true landing divergence
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song(rate * 8);
    c.seek_landing_unknown = true;
    c.seek_landing_offset = offset;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "inv");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const std::int64_t requested = rate * 5;
    std::int64_t landing = 0;
    p.engine.seek(us(requested), &landing);
    QN_CHECK(landing == requested, "inv");
    QN_CHECK(p.engine.snapshot().position_quality == LandingQuality::Estimated, "inv");
    Seen seen;
    auto true_minus_reported = [&]() -> std::int64_t {
        auto& tags = p.sink.segments[p.engine.snapshot().segment];
        return (tags.back() + 1) - p.engine.snapshot().media_position_frames;
    };
    for (int i = 0; i < 10; ++i) {
        p.sink.producer_step();
        p.sink.tick();
        check_all(p.engine, p.sink, seen, "inv");
    }
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == requested + offset,
             "inv: hidden truth = clamp + offset");
    QN_CHECK(true_minus_reported() == offset, "inv");
    for (int i = 0; i < 20; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    QN_CHECK(true_minus_reported() == offset, "inv: constant across playback");
    p.sink.tick(6);  // starved periods
    QN_CHECK(p.engine.snapshot().underrun_count >= 1, "inv");
    QN_CHECK(true_minus_reported() == offset, "inv: constant across underruns");
    p.engine.pause();
    p.sink.tick(2);
    QN_CHECK(std::strcmp(p.sink.last_render.kind, "paused") == 0, "inv");
    p.engine.play();
    for (int i = 0; i < 6; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    QN_CHECK(true_minus_reported() == offset, "inv: constant across pause/resume");
    std::printf("  ESTIMATED segment: true-vs-reported offset stays %lld (+37 ms)\n",
                (long long)offset);
}

// ---------------------------------------------------------------------------
// T16..T20 (clock semantics corrective)
// ---------------------------------------------------------------------------

GATE(t16_decode_ahead_clock_separation) {
    Pair p(1000, 25, 10);
    fake::SongConfig c = song(10000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t16");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();  // 2 x 25 = 500 ms
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.decoded_source_position == 50, "t16: 0.5 s decoded ahead");
    QN_CHECK(snap.media_position_frames == 0, "t16: decode must not move media position");
    QN_CHECK(snap.rendered_output_frames == 0, "t16");
    QN_CHECK(p.sink.total_output() == 0, "t16");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t16");
    std::printf("  500 ms decode-ahead: decoded=0.5 s, media=0, device=0\n");
}

GATE(t17_underrun_device_vs_media_clock) {
    Pair p(1000, 10, 10);
    fake::SongConfig c = song(10000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t17");
    p.engine.play();
    p.sink.producer_step();
    p.sink.producer_step();  // frames 0..9
    p.engine.submit(6);
    p.sink.tick_render(6);   // 6 media audible, 4 left queued
    p.sink.tick();           // requests 10: 4 media + 6 silence
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_count == 1, "t17");
    QN_CHECK(snap.media_position_frames == 10, "t17: media +40 ms this period");
    QN_CHECK(snap.rendered_output_frames == 16, "t17: device +100 ms this period");
    p.sink.producer_step();
    p.sink.producer_step();  // frames 10..19
    p.sink.tick();
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(tags.size() == 20, "t17: next media continues exactly after the 4");
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t17");
    }
    Seen seen;
    check_all(p.engine, p.sink, seen, "t17");
    std::printf("  starved period: 4 media + 6 silence -> device +100 ms, media +40 ms\n");
}

GATE(t18_repeated_underrun_media_continuity) {
    Pair p(1000, 10, 10);
    fake::SongConfig c = song(10000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t18");
    p.engine.play();
    for (int i = 0; i < 3; ++i) {
        p.sink.producer_step();
        p.sink.producer_step();  // 10 media frames
        p.sink.tick();           // real period
        p.sink.tick();           // starved period: underrun GAP
    }
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_count == 3, "t18");
    QN_CHECK(snap.underrun_silence_output_frames == 30, "t18");
    auto& tags = p.sink.segments[snap.segment];
    QN_CHECK(tags.size() == 30, "t18: every media frame exactly once, in order");
    for (std::size_t i = 0; i < tags.size(); ++i) {
        QN_CHECK(tags[i] == static_cast<std::int64_t>(i), "t18");
    }
    QN_CHECK(snap.rendered_output_frames == 60, "t18: device 600 ms");
    QN_CHECK(snap.media_position_frames == 30, "t18: media 300 ms");
    QN_CHECK(snap.rendered_output_frames > snap.rendered_media_frames, "t18");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t18");
    std::printf("  real/silence x3: 300 ms media exactly-once vs 600 ms device elapsed\n");
}

GATE(t19_seek_after_underrun_rebase) {
    Pair p(2000, 100, 10);
    fake::SongConfig c = song(4000);
    c.sample_rate = 100;
    c.seek_landing_offset = -6;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t19");
    p.engine.play();
    for (int i = 0; i < 16; ++i) p.sink.producer_step();  // 8 chunks x 100
    for (int i = 0; i < 80; ++i) p.sink.tick();           // 8.0 s clean media
    QN_CHECK(p.engine.snapshot().media_position_frames == 800, "t19");
    for (int i = 0; i < 3; ++i) p.sink.tick();            // 0.3 s underrun silence
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_silence_output_frames == 30, "t19");
    QN_CHECK(snap.rendered_output_frames == 830, "t19: device elapsed 8.3 s");
    QN_CHECK(snap.media_position_frames == 800, "t19: media position 8.0 s");
    std::int64_t landing = 0;
    p.engine.seek(qn::frames_to_us(3000, 100), &landing);  // request 30 s
    QN_CHECK(landing == 2994, "t19: actual landing 29.94 s");
    snap = p.engine.snapshot();
    QN_CHECK(snap.media_position_frames == 2994, "t19: media base = actual landing");
    QN_CHECK(snap.rendered_output_frames == 830, "t19: seek never mutates device history");
    QN_CHECK(snap.pending_output_frames == 0, "t19");
    p.sink.producer_step();
    p.sink.producer_step();
    p.sink.tick();
    auto& tags = p.sink.segments[p.engine.snapshot().segment];
    QN_CHECK(!tags.empty() && tags[0] == 2994, "t19: post-seek media starts at landing");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t19");
    std::printf("  device 8.3 s / media 8.0 s -> seek lands 29.94 s; media rebases\n");
}

GATE(t20_eof_after_underrun_duration) {
    Pair p(2000, 100, 10);
    fake::SongConfig c = song(1000);
    c.sample_rate = 100;
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t20");
    p.engine.play();
    for (int i = 0; i < 2; ++i) p.sink.producer_step();  // 100 frames = 1 s
    for (int i = 0; i < 10; ++i) p.sink.tick();          // 1.0 s clean media
    for (int i = 0; i < 5; ++i) p.sink.tick();           // 0.5 s underrun silence
    EngineSnapshot snap = p.engine.snapshot();
    QN_CHECK(snap.underrun_silence_output_frames == 50, "t20");
    drain_producer(p.engine, p.sink);
    while (p.engine.snapshot().state != PlayerState::Ended) p.sink.tick();
    snap = p.engine.snapshot();
    QN_CHECK(snap.state == PlayerState::Ended, "t20");
    QN_CHECK(snap.media_position_frames == 1000, "t20: ENDED at media duration (10 s)");
    QN_CHECK(snap.duration_frames == 1000, "t20");
    QN_CHECK(snap.rendered_output_frames == 1050, "t20: device session 10.5 s");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t20");
    std::printf("  10 s song + 0.5 s underrun: ENDED @10.0 s media / 10.5 s device\n");
}

// ---------------------------------------------------------------------------
// #40 corrective v2/v3/v4: the commit-flush protocol.
//
// invalidate() invokes the registered backend hook between the admission
// drain and the generation reset; the Windows renderer claims the request,
// executes the physical Stop+Reset on its own thread there, and completes
// it with a definitive verdict. Ordering — not renderer-side detection —
// now enforces the audible segment invariant: once segment N+1 has
// committed, the device buffer was already proven empty and no fill can
// cross the closed admission window, so no PCM of segment N can become
// audible. The gates below pin that ordering, the two race windows a
// periodic probe could never close (review findings on the first
// corrective), and the protocol's own timeout/cancellation/ABA semantics:
// cancel before claim is a safe never-began abort (t26), a claim can never
// be cancelled and its outcome is definitive (t27), completions are
// id-bound (t28), and the failure classification is INTERNAL, never a fake
// SongCore verdict (t29). v4 splits the abort outcomes by physical truth:
// kCancelled keeps the old generation intact (t26/t29), while kFailed —
// the session is torn down, pending output can never playout — poisons the
// generation to Error on every caller (t25, t31). t30 pins the extracted
// submit-accounting rule the WASAPI period loop installs.
// ---------------------------------------------------------------------------

GATE(t22_commit_flush_orders_before_landing) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t22");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const EngineSnapshot before = p.engine.snapshot();
    QN_CHECK(before.state == PlayerState::Playing, "t22");
    const std::uint64_t seg0 = before.segment;
    const std::uint64_t readable0 = p.engine.ring_debug().readable();

    // Hook-side observations must be lock-free (the hook runs under
    // src_mtx_ + state_mtx_): segment still OLD, seam fully drained, queue
    // not yet flushed — the physical flush strictly precedes the reset and
    // the landing.
    int calls = 0;
    std::uint64_t seg_at_flush = 99, active_at_flush = 99;
    std::uint64_t readable_at_flush = 99;
    p.engine.set_commit_flush_hook([&] {
        ++calls;
        seg_at_flush = p.engine.debug_segment();
        active_at_flush = p.engine.debug_active_backend_ops();
        readable_at_flush = p.engine.ring_debug().readable();
        return qn::CommitFlushResult::kPerformed;
    });
    QN_CHECK(p.engine.seek(us(48000 * 2)) == PlayerStatus::Ok, "t22");
    QN_CHECK(calls == 1, "t22: exactly one flush per commit");
    QN_CHECK(seg_at_flush == seg0, "t22: flush precedes the landing (old segment)");
    QN_CHECK(active_at_flush == 0, "t22: flush runs with the seam drained");
    QN_CHECK(readable_at_flush == readable0, "t22: flush precedes the queue flush");
    QN_CHECK(p.engine.snapshot().segment == seg0 + 1,
             "t22: segment landed only after the ACK");
    p.engine.set_commit_flush_hook({});

    // Post-commit the queue holds only the new generation: the first fill
    // is preroll silence (the queue was flushed after the ACK), and the
    // next media carries segment N+1.
    p.sink.tick_submit(1);
    QN_CHECK(p.sink.last_submit.segment == seg0 + 1, "t22");
    QN_CHECK(p.sink.last_submit.kind != nullptr &&
                 std::strcmp(p.sink.last_submit.kind, "preroll") == 0 &&
                 p.sink.last_submit.media_frames == 0,
             "t22: no old-generation media survived the commit");
    p.sink.tick_render(static_cast<std::int64_t>(p.sink.period));
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(4);
    Seen seen;
    check_all(p.engine, p.sink, seen, "t22");
    std::printf("  handshake: flush(ACK) -> reset -> landing, exactly once per commit\n");
}

GATE(t23_race_a_fill_cannot_cross_commit) {
    // Race A (review): fill/probe segment N, commit N+1, fill again. Under
    // the first corrective's periodic probe, the second fill could consume
    // segment-N media that then had to be "reconciled" as rendered without
    // ever being audible. Under the handshake, once seek() returns the
    // queue holds no N media, so the next fill can only serve preroll
    // silence or N+1 — nothing to reconcile, no phantom advance.
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t23");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();
    p.sink.tick(1);
    const std::uint64_t seg0 = p.engine.snapshot().segment;
    QN_CHECK(seg0 == 1, "t23");
    const qn::OutputFillResult probe = p.engine.fill_output(nullptr, 0);
    QN_CHECK(probe.segment == seg0, "t23: pre-commit fill sees segment N");

    p.engine.set_commit_flush_hook([] { return qn::CommitFlushResult::kPerformed; });
    QN_CHECK(p.engine.seek(us(48000 * 2)) == PlayerStatus::Ok, "t23");
    p.engine.set_commit_flush_hook({});

    p.sink.tick_submit(1);
    QN_CHECK(p.sink.last_submit.segment == seg0 + 1, "t23");
    QN_CHECK(p.sink.last_submit.media_frames == 0,
             "t23: post-commit fill must not consume segment-N media");
    p.sink.tick_render(static_cast<std::int64_t>(p.sink.period));
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(4);
    Seen seen;
    check_all(p.engine, p.sink, seen, "t23");
    std::printf("  race A: post-commit fills serve N+1 only, nothing fake-rendered\n");
}

GATE(t24_race_b_submit_never_crosses_commit) {
    // Race B (review): a fill of segment N returns, the commit lands, the
    // backend submits. Under the periodic probe, segment-N PCM crossed the
    // physical ReleaseBuffer AFTER segment N+1 had committed. Under the
    // handshake the commit blocks until the render thread ACKs the flush,
    // and the ACK is ordered after every submission that thread made — so,
    // deterministically on the engine side: every submit after seek()
    // returned carries the new segment, and the rendered log gains no
    // segment-N content past the commit.
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t24");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(3);
    const std::uint64_t seg0 = p.engine.snapshot().segment;

    p.engine.set_commit_flush_hook([] { return qn::CommitFlushResult::kPerformed; });
    QN_CHECK(p.engine.seek(us(48000 * 2)) == PlayerStatus::Ok, "t24");
    p.engine.set_commit_flush_hook({});

    const std::size_t log0 = p.engine.backend().rendered_log().size();
    for (int i = 0; i < 12; ++i) {
        p.sink.producer_step();
        p.sink.tick();
    }
    const auto& log = p.engine.backend().rendered_log();
    for (std::size_t i = log0; i < log.size(); ++i) {
        QN_CHECK_MSG(log[i].segment == seg0 + 1, "t24",
                     "segment %llu media crossed the landed commit",
                     (unsigned long long)log[i].segment);
    }
    Seen seen;
    check_all(p.engine, p.sink, seen, "t24");
    std::printf("  race B: no post-commit submit attributed to the old segment\n");
}

GATE(t25_flush_failure_poisons_generation) {
    // A claimed flush whose definitive outcome is NOT a proven flush
    // (kFailed — the backend escalates to teardown) must NEVER let playback
    // resume as if the commit had merely been rejected: the physical
    // session is gone, so the old segment's pending output can never
    // playout (ADR-0005: ring/timeline/device-pending share one segment
    // lifetime). The engine therefore invalidates the generation (epoch
    // bump kills in-flight producer results; pending timeline and ring
    // die), stores Error, and keeps the realtime seam closed; the control
    // op still reports the INTERNAL failure — never a fake SongCore seek
    // failure, with the out params untouched (SongCore was never
    // consulted). A fresh open with a proven flush recovers.
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t25");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const EngineSnapshot before = p.engine.snapshot();
    const std::uint64_t seg0 = before.segment;
    const std::uint64_t epoch0 = before.epoch;
    const int seek0 = c.live->seek_count;

    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kFailed; });
    std::int64_t landing = -5;
    std::int32_t sst = 7777;
    QN_CHECK(p.engine.seek(us(48000), &landing, &sst) ==
                 PlayerStatus::ErrInternal, "t25");
    QN_CHECK(landing == -5 && sst == 7777,
             "t25: out params untouched (no SongCore verdict to report)");
    QN_CHECK(c.live->seek_count == seek0,
             "t25: song_seek was never consulted");
    const EngineSnapshot after = p.engine.snapshot();
    QN_CHECK(after.state == PlayerState::Error,
             "t25: a dead physical generation must not resume as Playing");
    QN_CHECK(after.epoch == epoch0 + 1,
             "t25: generation invalidated (epoch bumped) — unlike cancel");
    QN_CHECK(after.segment == seg0,
             "t25: no landing was committed, so no segment bump");
    QN_CHECK(std::strcmp(after.last_error,
                         "commit flush failed after renderer claim") == 0,
             "t25: failure after claim is its own category");
    const qn::OutputFillResult r = p.engine.fill_output(nullptr, 0);
    QN_CHECK(std::strcmp(r.kind, "idle") == 0,
             "t25: the dead generation serves nothing (fail-closed seam)");

    // Recovery: a fresh open with a PROVEN flush rebuilds from Error to
    // Ready and commits normally afterwards.
    fake::SongConfig c2 = song();
    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kPerformed; });
    QN_CHECK(open_song(p.engine, p.sink, c2) == PlayerStatus::Ok,
             "t25: open recovers from the poisoned generation");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Ready, "t25");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();
    p.sink.tick(1);
    QN_CHECK(p.engine.snapshot().state == PlayerState::Playing,
             "t25: playback resumes in the new segment");
    p.engine.set_commit_flush_hook({});
    Seen seen;
    check_all(p.engine, p.sink, seen, "t25");
    std::printf("  flush failure after claim: generation poisoned to Error, open recovers\n");
}

GATE(t26_timeout_before_claim_cancels) {
    // Timeout BEFORE the renderer claims (v3 review, T26): the request is
    // cancelled atomically — the operation never began — and the protocol
    // itself refuses every late renderer action: a cancelled request can
    // never be claimed or completed (I3). Engine-side, the cancelled
    // flush aborts the commit fail-closed with the old generation intact
    // and reports the INTERNAL failure; the next commit (request B) then
    // commits normally. (The Windows control side derives kCancelled from
    // its bounded REQUESTED-phase wait; these are the transition rules
    // that timeout policy stands on.)
    qn::CommitFlushHandshake hs;
    const std::uint64_t a = hs.request();
    QN_CHECK(hs.phase() == qn::CommitFlushHandshake::kRequested, "t26");
    QN_CHECK(hs.try_cancel(a), "t26: cancel before claim succeeds");
    QN_CHECK(!hs.claim(a),
             "t26: a cancelled request can never be claimed (I3)");
    hs.complete(a, true);  // a late renderer "ACK" for A
    QN_CHECK(hs.cancelled(a) && !hs.completed(a),
             "t26: a late completion cannot revive a cancelled request");

    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t26");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const EngineSnapshot before = p.engine.snapshot();
    const std::uint64_t seg0 = before.segment;
    const std::uint64_t epoch0 = before.epoch;

    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kCancelled; });
    std::int64_t landing = -5;
    std::int32_t sst = 7777;
    QN_CHECK(p.engine.seek(us(48000), &landing, &sst) ==
                 PlayerStatus::ErrInternal, "t26");
    QN_CHECK(landing == -5 && sst == 7777,
             "t26: out params untouched (SongCore never consulted)");
    p.engine.set_commit_flush_hook({});
    const EngineSnapshot after = p.engine.snapshot();
    QN_CHECK(after.segment == seg0 && after.epoch == epoch0,
             "t26: commit abandoned without any mutation");
    QN_CHECK(after.state == PlayerState::Playing,
             "t26: old generation logically intact");
    QN_CHECK(std::strcmp(after.last_error,
                         "commit flush cancelled before claim") == 0,
             "t26: cancelled-before-claim is its own category");
    const qn::OutputFillResult r = p.engine.fill_output(nullptr, 0);
    QN_CHECK(std::strcmp(r.kind, "audio") == 0 && r.segment == seg0,
             "t26: admission re-opened against the untouched generation");

    // Request B: the protocol slot is reusable and the commit lands.
    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kPerformed; });
    QN_CHECK(p.engine.seek(us(48000 * 2)) == PlayerStatus::Ok, "t26");
    p.engine.set_commit_flush_hook({});
    QN_CHECK(p.engine.snapshot().segment == seg0 + 1,
             "t26: the next commit commits normally");
    Seen seen;
    check_all(p.engine, p.sink, seen, "t26");
    std::printf("  timeout before claim: cancel + late-action refusal, engine aborts, retry OK\n");
}

GATE(t27_claimed_flush_cannot_be_cancelled) {
    // Timeout AFTER the renderer claimed (v3 review, T27): the claim is
    // the protocol's point of no return — control cannot cancel (I4) and
    // must await the definitive outcome, whichever it is. (The Windows
    // control side waits without a bound once claimed; these are the
    // transition rules that policy stands on. The engine-side mapping of
    // a definitive claimed failure is gate t25.)
    qn::CommitFlushHandshake ok;
    const std::uint64_t a = ok.request();
    QN_CHECK(ok.claim(a), "t27: renderer claims");
    QN_CHECK(ok.phase() == qn::CommitFlushHandshake::kClaimed, "t27");
    QN_CHECK(!ok.try_cancel(a),
             "t27: a claimed request can never be cancelled (I4)");
    ok.complete(a, true);
    QN_CHECK(ok.completed(a) && ok.outcome(), "t27: definitive success");

    qn::CommitFlushHandshake bad;
    const std::uint64_t b = bad.request();
    QN_CHECK(bad.claim(b), "t27");
    QN_CHECK(!bad.try_cancel(b), "t27: cancel refused past the claim (I4)");
    bad.complete(b, false);
    QN_CHECK(bad.completed(b) && !bad.outcome(),
             "t27: definitive failure");
    std::printf("  claimed flush: cancel refused, definitive outcome awaited (both verdicts)\n");
}

GATE(t28_stale_ack_never_satisfies_new_request) {
    // Stale ACK / ABA (v3 review, T28): request A cancelled before the
    // claim, request B published into the slot — a late completion for A
    // can never resolve B (I5); only B's own claim/complete sequence
    // resolves B.
    qn::CommitFlushHandshake hs;
    const std::uint64_t a = hs.request();
    QN_CHECK(hs.try_cancel(a), "t28: A cancelled before the claim");
    QN_CHECK(hs.cancelled(a) && !hs.claimed(a) && !hs.completed(a),
             "t28: A's terminal state is CANCELLED while observable");
    const std::uint64_t b = hs.request();
    QN_CHECK(b == a + 1, "t28: request ids are monotonic");
    QN_CHECK(hs.pending_request() == b, "t28: B is the serviceable request");
    QN_CHECK(!hs.cancelled(b) && !hs.completed(b), "t28");
    hs.complete(a, true);  // the stale, late "ACK" for A
    QN_CHECK(!hs.completed(b) && hs.pending_request() == b,
             "t28: ACK(A) cannot complete B (I5)");
    QN_CHECK(hs.claim(b), "t28: B remains claimable");
    hs.complete(b, true);
    QN_CHECK(hs.completed(b) && hs.outcome(), "t28: ACK(B) resolves B");
    // The slot is single flight by design: once B supersedes A, A's phase
    // is no longer addressable — which is exactly the ABA protection (a
    // stale actor cannot even find A in the slot to act on it).
    std::printf("  stale ACK: id-bound completions, no ABA\n");
}

GATE(t29_internal_error_classification_all_callers) {
    // A commit-flush protocol failure is an ENGINE-INTERNAL failure for
    // every commit caller (v3 review, T29): it surfaces as ErrInternal —
    // never as a fake SongCore open/seek failure — with *out_song_status
    // untouched and the SongCore callback counts unchanged (the abort
    // happens before SongCore is consulted). Logical mutation matches the
    // before-claim policy: none (epoch/segment/state unchanged; the
    // per-caller failures below plus t25/t26 pin it).
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t29");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const EngineSnapshot baseline = p.engine.snapshot();
    const int seek0 = c.live->seek_count;
    const int reopen0 = c.live->reopen_count;

    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kCancelled; });

    // seek
    std::int64_t landing = -5;
    std::int32_t sst = 7777;
    QN_CHECK(p.engine.seek(us(48000), &landing, &sst) ==
                 PlayerStatus::ErrInternal, "t29: seek");
    QN_CHECK(landing == -5 && sst == 7777, "t29: seek out params untouched");

    // open (a second open while the first song lives: the abort must
    // happen before song_open, so no reopen is recorded either)
    song_io io{};
    io.userdata = &c;
    io.read = [](void*, std::uint8_t*, std::size_t) -> std::int64_t { return 0; };
    io.seek = [](void*, std::int64_t) -> std::int64_t { return 0; };
    io.size = [](void*) -> std::int64_t { return 0; };
    sst = 7777;
    QN_CHECK(p.engine.open(io, &sst) == PlayerStatus::ErrInternal,
             "t29: open");
    QN_CHECK(sst == 7777, "t29: open out param untouched");

    // stop
    sst = 7777;
    QN_CHECK(p.engine.stop(&sst) == PlayerStatus::ErrInternal, "t29: stop");
    QN_CHECK(sst == 7777, "t29: stop out param untouched");
    p.engine.set_commit_flush_hook({});
    QN_CHECK(c.live->seek_count == seek0 && c.live->reopen_count == reopen0,
             "t29: SongCore was never consulted on the abort paths");
    const EngineSnapshot mid = p.engine.snapshot();
    QN_CHECK(mid.state == PlayerState::Playing &&
                 mid.segment == baseline.segment &&
                 mid.epoch == baseline.epoch,
             "t29: aborted commits left the generation untouched");

    // play after ENDED
    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kCancelled; });
    drain_producer(p.engine, p.sink);
    int guard = 0;
    while (p.engine.snapshot().state != PlayerState::Ended) {
        QN_CHECK(++guard < 100000, "t29: runaway");
        // The 8 s song dwarfs the queue: keep producing while draining or
        // the source never reaches EOF (the sweep-gate idiom).
        for (int i = 0; i < 4; ++i) p.sink.producer_step();
        p.sink.tick();
    }
    const EngineSnapshot ended0 = p.engine.snapshot();
    QN_CHECK(p.engine.play() == PlayerStatus::ErrInternal,
             "t29: play-after-ENDED");
    const EngineSnapshot ended1 = p.engine.snapshot();
    QN_CHECK(ended1.state == PlayerState::Ended &&
                 ended1.segment == ended0.segment &&
                 ended1.epoch == ended0.epoch,
             "t29: ENDED restart aborted without mutation");
    QN_CHECK(c.live->seek_count == seek0, "t29: restart seek never ran");
    p.engine.set_commit_flush_hook({});
    Seen seen;
    check_all(p.engine, p.sink, seen, "t29");
    std::printf("  seek/open/stop/play-ENDED: INTERNAL classification, SongCore untouched\n");
}

GATE(t30_release_failure_never_advances_accounting) {
    // I6 (v3 review), pinned at the extracted decision the Windows period
    // loop installs verbatim: ReleaseBuffer SUCCESS is what advances
    // written_total; a failure must leave the accounting untouched, flag
    // the teardown, and never fabricate advance_render evidence. (The
    // WASAPI call itself cannot run on Linux; the mingw codegen build
    // proves the call site compiles against this helper.)
    const qn::SubmitAccounting released =
        qn::apply_release_result(true, 512, 1000);
    QN_CHECK(released.written_total == 1512 && !released.teardown,
             "t30: accepted frames advance written_total");
    const qn::SubmitAccounting failed =
        qn::apply_release_result(false, 512, 1000);
    QN_CHECK(failed.written_total == 1000 && failed.teardown,
             "t30: a failed release moves nothing and fails closed");
    const qn::SubmitAccounting empty =
        qn::apply_release_result(false, 0, 1000);
    QN_CHECK(empty.written_total == 1000 && empty.teardown,
             "t30: even a zero-frame release failure fails closed");
    std::printf("  release accounting: success advances, failure tears down, no reconciliation\n");
}

GATE(t31_flush_failure_poisons_open_and_stop) {
    // The v4 generation-poisoning applies to EVERY commit caller: open and
    // stop under kFailed also leave the engine in Error — never silently
    // READY/Playing — with SongCore untouched (the abort precedes song_open
    // / song_stop) and out params untouched. A later open with a proven
    // flush recovers to READY@0.
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t31");
    p.engine.play();
    for (int i = 0; i < 4; ++i) p.sink.producer_step();
    const int reopen0 = c.live->reopen_count;

    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kFailed; });

    // stop under kFailed
    std::int32_t sst = 7777;
    QN_CHECK(p.engine.stop(&sst) == PlayerStatus::ErrInternal, "t31: stop");
    QN_CHECK(sst == 7777, "t31: stop out param untouched");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Error,
             "t31: stop failure poisons the generation too");

    // open under kFailed
    song_io io{};
    io.userdata = &c;
    io.read = [](void*, std::uint8_t*, std::size_t) -> std::int64_t { return 0; };
    io.seek = [](void*, std::int64_t) -> std::int64_t { return 0; };
    io.size = [](void*) -> std::int64_t { return 0; };
    sst = 7777;
    QN_CHECK(p.engine.open(io, &sst) == PlayerStatus::ErrInternal,
             "t31: open");
    QN_CHECK(sst == 7777, "t31: open out param untouched");
    QN_CHECK(c.live->reopen_count == reopen0,
             "t31: song_open was never consulted");
    QN_CHECK(p.engine.snapshot().state == PlayerState::Error,
             "t31: still Error after the aborted open");

    // Recovery with a proven flush
    p.engine.set_commit_flush_hook(
        [] { return qn::CommitFlushResult::kPerformed; });
    fake::SongConfig c2 = song();
    QN_CHECK(open_song(p.engine, p.sink, c2) == PlayerStatus::Ok,
             "t31: open recovers with a proven flush");
    const EngineSnapshot rec = p.engine.snapshot();
    QN_CHECK(rec.state == PlayerState::Ready && rec.media_position_frames == 0,
             "t31: clean rebuild to READY@0");
    p.engine.set_commit_flush_hook({});
    std::printf("  open/stop under kFailed: generation poisoned to Error, recoverable\n");
}

GATE(t21_zero_frame_probe_commit_segment) {
    Pair p(16384, 1024, 512);
    fake::SongConfig c = song();
    QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "t21");
    p.engine.play();
    for (int i = 0; i < 6; ++i) p.sink.producer_step();
    p.sink.tick(2);
    const EngineSnapshot before = p.engine.snapshot();
    QN_CHECK(before.state == PlayerState::Playing, "t21");
    QN_CHECK(before.segment == 1, "t21");
    QN_CHECK(before.underrun_count == 0 && before.preroll_events == 0, "t21");

    // Zero-frame probe: current segment, nothing moved, NO side effects —
    // the admission-safe activation check the renderer's idle loop runs.
    const qn::OutputFillResult probe = p.engine.fill_output(nullptr, 0);
    QN_CHECK(std::strcmp(probe.kind, "audio") == 0, "t21");
    QN_CHECK(probe.segment == 1, "t21");
    QN_CHECK(probe.media_frames == 0 && probe.silence_frames == 0, "t21");
    QN_CHECK(probe.source_rate == 48000 && probe.channels == 2, "t21");
    const EngineSnapshot after = p.engine.snapshot();
    QN_CHECK(after.state == PlayerState::Playing, "t21");
    QN_CHECK(after.underrun_count == 0 && after.preroll_events == 0 &&
                 after.eos_silence_output_frames == 0,
             "t21: probe must not fake GAP events");
    QN_CHECK(after.pending_output_frames == before.pending_output_frames &&
                 after.submitted_output_frames == before.submitted_output_frames &&
                 after.submitted_media_frames == before.submitted_media_frames &&
                 after.queued_media_frames == before.queued_media_frames,
             "t21: probe must not touch ring/timeline accounting");

    // Playing seek: the very next probe carries only the new segment —
    // the commit-flush protocol already cleared the old segment's device
    // PCM before this segment landed.
    QN_CHECK(p.engine.seek(us(48000 * 2)) == PlayerStatus::Ok, "t21");
    const qn::OutputFillResult post = p.engine.fill_output(nullptr, 0);
    QN_CHECK(std::strcmp(post.kind, "audio") == 0, "t21");
    QN_CHECK(post.segment == 2, "t21");
    QN_CHECK(post.media_frames == 0 && post.silence_frames == 0, "t21");

    // Paused: idle with the current segment — the renderer keeps its
    // session; the commit flush already cleared its pending buffer before
    // the new segment landed, so the resume re-Starts clean.
    p.engine.pause();
    const qn::OutputFillResult paused = p.engine.fill_output(nullptr, 0);
    QN_CHECK(std::strcmp(paused.kind, "idle") == 0, "t21");
    QN_CHECK(paused.segment == 2, "t21");
    std::printf("  zero-frame probe: side-effect-free, sees the commit segment bump\n");
}

// ---------------------------------------------------------------------------
// capacity sweep (reduced native mirror of the structural gate)
// ---------------------------------------------------------------------------

GATE(capacity_sweep) {
    struct Row {
        const char* pattern;
        int rate, ms, cap;
        std::uint64_t underruns, underrun_f;
    };
    std::vector<Row> rows;
    for (const char* pattern : {"deficit", "surplus"}) {
        const int lo_p = std::strcmp(pattern, "deficit") == 0 ? 3 : 5;
        const int hi_p = std::strcmp(pattern, "deficit") == 0 ? 6 : 8;
        const int lo_c = std::strcmp(pattern, "deficit") == 0 ? 6 : 4;
        const int hi_c = std::strcmp(pattern, "deficit") == 0 ? 14 : 8;
        for (int rate : {44100, 48000, 96000}) {
            for (int ms : {20, 50, 100, 250, 500}) {
                const int cap = rate * ms / 1000;
                Pair p(static_cast<std::uint64_t>(cap),
                       static_cast<std::uint64_t>(std::min(512, cap)),
                       static_cast<std::uint64_t>(std::min(256, cap)));
                fake::SongConfig c = song(rate * 8);
                c.sample_rate = rate;
                QN_CHECK(open_song(p.engine, p.sink, c) == PlayerStatus::Ok, "sweep");
                p.engine.play();
                Rng rng(static_cast<std::uint64_t>(
                    (std::strcmp(pattern, "deficit") == 0 ? 1 : 2) * rate));
                Seen seen;
                        int guard = 0;
                while (p.engine.snapshot().state != PlayerState::Ended) {
                    QN_CHECK(++guard < 100000, "sweep: runaway");
                    for (int i = 0, n = rng.uniform(lo_p, hi_p); i < n; ++i) {
                        p.sink.producer_step();
                    }
                    for (int i = 0, n = rng.uniform(lo_c, hi_c); i < n; ++i) {
                        p.sink.tick();
                    }
                }
                EngineSnapshot snap = p.engine.snapshot();
                QN_CHECK(snap.media_position_frames == snap.duration_frames, "sweep");
                QN_CHECK(snap.state == PlayerState::Ended, "sweep");
                rows.push_back(Row{pattern, rate, ms, cap, snap.underrun_count,
                                   snap.underrun_silence_output_frames});
            }
        }
    }
    for (int rate : {44100, 48000, 96000}) {
        std::uint64_t prev = ~0ULL;
        for (const Row& r : rows) {
            if (std::strcmp(r.pattern, "deficit") != 0 || r.rate != rate) continue;
            QN_CHECK(prev == ~0ULL || r.underruns <= prev,
                     "sweep: deficit should starve small queues sooner");
            prev = r.underruns;
        }
    }
    std::printf("  2 patterns x 3 rates x 5 capacities: all ENDED @duration, deficit ordered\n");
}

}  // namespace qn::test
