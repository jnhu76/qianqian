#include "player_engine.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string_view>
#include <thread>

namespace qn {

// AudioEngine boundary (docs §10): v1 is BYPASS only. process/reset/drain
// exist as the lifecycle seams the engine must call; SRC stays frozen out.
namespace {
struct AudioEngineBypass {
    void reset() {}
    void drain() {}
};
}  // namespace

std::int64_t us_to_frames_floor(std::int64_t us, std::int64_t rate) {
    return (us * rate) / 1000000;
}

std::int64_t us_to_frames_nearest(std::int64_t us, std::int64_t rate) {
    // Nearest-frame clock rebase: the ABI reports landings in microseconds;
    // converting back must not bias the clock a frame early every seek.
    return (us * rate + 500000) / 1000000;
}

std::int64_t frames_to_us(std::int64_t frames, std::int64_t rate) {
    return frames * 1000000 / rate;
}

PlayerEngine::PlayerEngine(const EngineConfig& config)
    : config_(config),
      ring_(config.capacity_frames, static_cast<std::int32_t>(config.max_channels)) {
    if (config_.capacity_frames == 0 || config_.read_chunk_frames == 0 ||
        config_.max_submit_frames == 0 || config_.max_channels == 0) {
        std::fprintf(stderr, "PlayerEngine: capacity/chunk/submit/channels must be > 0\n");
        std::abort();
    }
    if (config_.read_chunk_frames > config_.capacity_frames) {
        std::fprintf(stderr, "PlayerEngine: read_chunk_frames must fit the queue capacity\n");
        std::abort();
    }
    chunk_buf_.resize(static_cast<std::size_t>(config_.read_chunk_frames * config_.max_channels));
    submit_buf_.resize(static_cast<std::size_t>(config_.max_submit_frames * config_.max_channels));
    if (config_.worker_thread) {
        worker_ = std::thread([this] { worker_loop(); });
    }
}

PlayerEngine::~PlayerEngine() {
    {
        std::lock_guard<std::mutex> g(state_mtx_);
        shutdown_ = true;
    }
    work_cv_.notify_all();
    if (worker_.joinable()) worker_.join();
    // No realtime fill/advance may be mid-flight while the engine tears
    // down. The realtime path never blocks on state_mtx_, so the wait is
    // bounded; production callers stop the device before destroy.
    quiesce_backend();
    {
        std::lock_guard<std::mutex> g(src_mtx_);
        if (handle_) {
            song_close(handle_);
            handle_ = nullptr;
        }
    }
}

// ---------------------------------------------------------------------------
// control plane — serialized; lock order src_mtx_ -> state_mtx_
// ---------------------------------------------------------------------------

PlayerStatus PlayerEngine::open(const song_io& io, std::int32_t* out_song_status) {
    std::lock_guard<std::mutex> sg(src_mtx_);
    std::lock_guard<std::mutex> g(state_mtx_);
    invalidate();

    if (handle_) {
        song_close(handle_);
        handle_ = nullptr;
    }
    io_copy_ = io;
    has_io_ = true;

    // out_song_status always receives the SongCore status (SONG_OK on
    // success) — a C consumer must never see an untouched output.
    song_status st = song_open(&io_copy_, &handle_);
    if (st != SONG_OK || handle_ == nullptr) {
        if (out_song_status) *out_song_status = st;
        handle_ = nullptr;
        has_io_ = false;
        state_.store(PlayerState::Empty);
        last_error_ = "open failed: status=" + std::to_string(st);
        return PlayerStatus::ErrOpenFailed;
    }
    song_info info{};
    st = song_probe(handle_, &info);
    if (st != SONG_OK) {
        if (out_song_status) *out_song_status = st;
        song_close(handle_);
        handle_ = nullptr;
        has_io_ = false;
        state_.store(PlayerState::Empty);
        last_error_ = "open failed: status=" + std::to_string(st);
        return PlayerStatus::ErrOpenFailed;
    }
    source_rate_ = info.sample_rate;
    channels_ = info.channels;
    if (channels_ > config_.max_channels) {
        // Fail-closed config mismatch (unreachable for the test corpus).
        if (out_song_status) *out_song_status = SONG_ERR_INVALID_ARGUMENT;
        song_close(handle_);
        handle_ = nullptr;
        has_io_ = false;
        state_.store(PlayerState::Empty);
        last_error_ = "open failed: status=" + std::to_string(SONG_ERR_INVALID_ARGUMENT);
        return PlayerStatus::ErrOpenFailed;
    }
    duration_known_ = info.duration_us >= 0;
    duration_frames_ =
        duration_known_ ? us_to_frames_nearest(info.duration_us, source_rate_) : -1;
    backend_.configure(channels_);
    ring_.set_data_stride(channels_);  // ring is empty (invalidate() flushed)

    commit_landing(0, LandingQuality::Confirmed);  // fresh handle: CONFIRMED 0
    state_.store(PlayerState::Ready);
    last_error_.clear();
    if (out_song_status) *out_song_status = SONG_OK;
    work_cv_.notify_all();
    return PlayerStatus::Ok;
}

PlayerStatus PlayerEngine::play() {
    std::lock_guard<std::mutex> sg(src_mtx_);
    std::lock_guard<std::mutex> g(state_mtx_);
    const PlayerState st = state_.load();
    if (st == PlayerState::Playing) return PlayerStatus::Ok;  // idempotent
    if (st == PlayerState::Ready || st == PlayerState::Paused) {
        state_.store(PlayerState::Playing);
        work_cv_.notify_all();
        return PlayerStatus::Ok;
    }
    if (st == PlayerState::Ended) {
        // Frozen restart policy: play after ENDED replays from the beginning;
        // a failed restart seek lands in ERROR (fail-closed, like any seek).
        invalidate();
        std::int64_t actual = 0;
        song_status sst = song_seek(handle_, 0, &actual);
        auto landing = landing_of(sst, actual, 0);
        if (!landing) {
            state_.store(PlayerState::Error);
            last_error_ = "restart seek failure, status=" + std::to_string(sst);
            return PlayerStatus::ErrSeekFailed;
        }
        commit_landing(landing->first, landing->second);
        state_.store(PlayerState::Playing);
        work_cv_.notify_all();
        return PlayerStatus::Ok;
    }
    return PlayerStatus::ErrIllegalCall;  // Empty / Error
}

void PlayerEngine::pause() {
    std::lock_guard<std::mutex> sg(src_mtx_);
    std::lock_guard<std::mutex> g(state_mtx_);
    // Frozen: audible progression stops, position freezes, buffer retained.
    // READY / PAUSED / ENDED / EMPTY are documented no-ops.
    if (state_.load() == PlayerState::Playing) state_.store(PlayerState::Paused);
}

PlayerStatus PlayerEngine::stop(std::int32_t* out_song_status) {
    std::lock_guard<std::mutex> sg(src_mtx_);
    std::lock_guard<std::mutex> g(state_mtx_);
    if (state_.load() == PlayerState::Empty) {
        if (out_song_status) *out_song_status = SONG_OK;
        return PlayerStatus::Ok;  // no-op
    }

    // Deterministic rebuild to READY @0 (docs §3.2): from a healthy state the
    // handle is trusted — rewind in place via seek(0). From ERROR (or when
    // the rewind seek fails) the handle is not trusted — drop and reopen. If
    // even the reopen fails, ERROR persists and only open() recovers.
    invalidate();
    std::optional<std::pair<std::int64_t, LandingQuality>> landing;
    if (state_.load() != PlayerState::Error) {
        std::int64_t actual = 0;
        song_status st = song_seek(handle_, 0, &actual);
        if (out_song_status) *out_song_status = st;
        landing = landing_of(st, actual, 0);
    }
    if (!landing) {
        // Reopen to a TEMP handle first: a failed reopen must leave the old
        // handle alive (the oracle keeps its decoder; the frozen ABI's
        // song_close is irreversible).
        song_handle* fresh = nullptr;
        song_status st = song_open(&io_copy_, &fresh);
        song_info info{};
        if (st == SONG_OK && fresh != nullptr) st = song_probe(fresh, &info);
        if (st != SONG_OK) {
            if (out_song_status) *out_song_status = st;
            if (fresh) song_close(fresh);
            state_.store(PlayerState::Error);
            last_error_ = "stop recovery failed: status=" + std::to_string(st);
            return PlayerStatus::ErrOpenFailed;
        }
        song_close(handle_);
        handle_ = fresh;
        source_rate_ = info.sample_rate;
        channels_ = info.channels;
        duration_known_ = info.duration_us >= 0;
        duration_frames_ =
            duration_known_ ? us_to_frames_nearest(info.duration_us, source_rate_) : -1;
        backend_.configure(channels_);
        ring_.set_data_stride(channels_);
        landing = std::make_pair(std::int64_t{0}, LandingQuality::Confirmed);
    }
    commit_landing(landing->first, landing->second);
    state_.store(PlayerState::Ready);
    last_error_.clear();
    if (out_song_status) *out_song_status = SONG_OK;
    work_cv_.notify_all();
    return PlayerStatus::Ok;
}

PlayerStatus PlayerEngine::seek(std::int64_t position_us, std::int64_t* out_landing_frames,
                                std::int32_t* out_song_status) {
    std::lock_guard<std::mutex> sg(src_mtx_);
    std::lock_guard<std::mutex> g(state_mtx_);
    const PlayerState st = state_.load();
    if (st != PlayerState::Ready && st != PlayerState::Playing &&
        st != PlayerState::Paused && st != PlayerState::Ended) {
        return PlayerStatus::ErrIllegalCall;
    }
    const bool was_ended = st == PlayerState::Ended;
    // Fail-closed commit order: invalidate the generation and flush FIRST,
    // then attempt SongCore — a failed seek must never resume the old
    // timeline (docs §3.1).
    invalidate();
    std::int64_t actual = 0;
    song_status sst = song_seek(handle_, position_us, &actual);
    if (out_song_status) *out_song_status = sst;
    auto landing = landing_of(sst, actual, position_us);
    if (!landing) {
        state_.store(PlayerState::Error);
        last_error_ = "seek failure, status=" + std::to_string(sst);
        return PlayerStatus::ErrSeekFailed;
    }
    commit_landing(landing->first, landing->second);
    if (was_ended) state_.store(PlayerState::Ready);
    if (out_landing_frames) *out_landing_frames = landing->first;
    if (out_song_status) *out_song_status = SONG_OK;
    work_cv_.notify_all();
    return PlayerStatus::Ok;
}

// ---------------------------------------------------------------------------
// decode worker
// ---------------------------------------------------------------------------

StepReport PlayerEngine::worker_step() {
    // An in-flight chunk always completes — that is what makes the epoch
    // guard exercisable — then publishes or dies by epoch.
    if (in_flight_) {
        {
            std::lock_guard<std::mutex> g(state_mtx_);
            if (in_flight_->steps_left > 1) {
                --in_flight_->steps_left;
                return StepReport{StepOutcome::Working, in_flight_->frames,
                                  in_flight_->epoch, in_flight_->start_frame};
            }
        }
        std::function<void()> hook;
        {
            std::lock_guard<std::mutex> hk(hook_mtx_);
            hook = publish_hook_;
        }
        if (hook) hook();  // test barrier, no locks held
        std::lock_guard<std::mutex> g(state_mtx_);
        if (in_flight_->epoch != epoch_.load() && !mut(kMutSkipEpochGuard)) {
            // Late publication is where stale data dies (docs §6).
            discarded_stale_media_frames_.fetch_add(in_flight_->frames);
            StepReport rep{StepOutcome::Stale, in_flight_->frames, in_flight_->epoch,
                           in_flight_->start_frame};
            in_flight_.reset();
            in_flight_frames_.store(0);
            return rep;
        }
        if (ring_.writable() < in_flight_->frames) {
            // Only reachable for poll-origin chunks (their fit could not be
            // pre-checked); defer publication by one quantum.
            in_flight_->steps_left = 1;
            return StepReport{StepOutcome::Backpressure, 0, in_flight_->epoch,
                              in_flight_->start_frame};
        }
        ring_.write(chunk_buf_.data(), in_flight_->frames);
        StepReport rep{StepOutcome::Wrote, in_flight_->frames, in_flight_->epoch,
                       in_flight_->start_frame};
        in_flight_.reset();
        in_flight_frames_.store(0);
        return rep;
    }

    // New decode work starts only while PLAYING (docs: PAUSED retains the
    // buffer). The whole begin section runs under src_mtx_ so the captured
    // epoch and the read data can never be paired across a seek/stop commit.
    std::lock_guard<std::mutex> sg(src_mtx_);
    std::uint64_t request = 0;
    std::uint64_t flight_epoch = 0;
    {
        std::lock_guard<std::mutex> g(state_mtx_);
        const PlayerState st = state_.load();
        const bool can_touch_source =
            (st == PlayerState::Playing || st == PlayerState::Paused);
        if (!can_touch_source || source_eof_.load() || handle_ == nullptr) {
            return StepReport{StepOutcome::Idle};
        }
        const std::int64_t remaining = source_remaining();
        std::uint64_t want = config_.read_chunk_frames;
        if (remaining >= 0 && static_cast<std::uint64_t>(remaining) < want) {
            want = remaining > 0 ? static_cast<std::uint64_t>(remaining) : 0;
        }
        if (want == 0) {
            // EOF poll: the source is (believed) exhausted but SONG_EOF has
            // not been observed yet. Confirm with a >=1-frame read; a
            // 0-frame result needs no ring space. The poll is a source-side
            // read, not new media work — it also runs while PAUSED, because
            // the oracle's eof flag rides with publication regardless of
            // the playback state.
            const std::uint64_t writable = ring_.writable();
            request = (writable > 0 && writable < config_.read_chunk_frames)
                          ? writable
                          : config_.read_chunk_frames;
            if (request == 0) request = 1;
        } else {
            if (st != PlayerState::Playing) {
                return StepReport{StepOutcome::Idle};
            }
            // Frozen backpressure rule (docs §4): begin a chunk only when it
            // fits; never overwrite, never drop, never grow.
            if (ring_.writable() < want) {
                return StepReport{StepOutcome::Backpressure};
            }
            request = want;
        }
        flight_epoch = epoch_.load();
    }

    // Decode under src_mtx_ (serialized with seek/stop on the same handle),
    // outside state_mtx_ (submits/renders never block on a decode).
    std::uint64_t produced = 0;
    const song_status st =
        song_read_pcm(handle_, chunk_buf_.data(), request, &produced);

    std::function<void()> hook;
    {
        std::lock_guard<std::mutex> hk(hook_mtx_);
        hook = read_hook_;
    }
    if (hook) hook();  // test barrier, src_mtx_ held

    {
        std::lock_guard<std::mutex> g(state_mtx_);
        if (st == SONG_EOF) {
            // SONG_EOF is the frozen way the engine learns exhaustion: the
            // oracle's chunk.eof flag arrives with the final data chunk, the
            // real ABI reports EOF on the next read.
            source_eof_.store(true);
            maybe_end();
            return StepReport{StepOutcome::Idle};
        }
        if (st != SONG_OK || produced == 0) {
            state_.store(PlayerState::Error);
            last_error_ = "decode failure, status=" + std::to_string(st);
            return StepReport{StepOutcome::Idle};
        }
        // Partial-success framing is the source's contract (frames before a
        // fault were already delivered as SONG_OK; the fault surfaces on the
        // next call) — nothing extra to do here.
        decoded_source_frames_ += produced;
        decoded_since_commit_ += produced;
        in_flight_ = InFlight{flight_epoch, /*start_frame=*/0, produced, work_steps_};
        in_flight_frames_.store(produced);
        return StepReport{StepOutcome::Begin, produced, flight_epoch, 0};
    }
}

void PlayerEngine::worker_loop() {
    for (;;) {
        const StepReport rep = worker_step();
        std::unique_lock<std::mutex> g(state_mtx_);
        if (shutdown_) return;
        if (rep.outcome == StepOutcome::Idle || rep.outcome == StepOutcome::Backpressure) {
            // Wake when work may have become available: state change, source
            // reset, or ring space freed by a submit.
            work_cv_.wait(g, [this] {
                if (shutdown_) return true;
                if (state_.load() != PlayerState::Playing || source_eof_.load())
                    return false;
                if (in_flight_) return true;
                const std::int64_t remaining = source_remaining();
                std::uint64_t want = config_.read_chunk_frames;
                if (remaining >= 0 && static_cast<std::uint64_t>(remaining) < want) {
                    want = remaining > 0 ? static_cast<std::uint64_t>(remaining) : 1;
                }
                return ring_.writable() >= want;
            });
            if (shutdown_) return;
        }
    }
}

// ---------------------------------------------------------------------------
// backend side — realtime seam + manual-tick wrappers
// ---------------------------------------------------------------------------

OutputFillResult PlayerEngine::fill_output(float* dst,
                                           std::uint64_t requested_output_frames) {
    OutputFillResult r;
    active_fill_.fetch_add(1, std::memory_order_acq_rel);
    std::function<void()> hook;
    {
        std::lock_guard<std::mutex> hk(hook_mtx_);
        hook = fill_hook_;
    }
    if (hook) hook();  // test barrier, no engine locks held

    r.segment = segment_.load();
    if (state_.load() != PlayerState::Playing || timeline_overflow_.load()) {
        r.kind = "idle";  // not playing, or fail-closed span-store overflow
        active_fill_.fetch_sub(1, std::memory_order_acq_rel);
        return r;
    }
    const std::uint64_t period = requested_output_frames;
    const std::uint64_t readable = ring_.readable();
    const std::uint64_t m =
        ring_.read(dst, period < readable ? period : readable);
    const std::uint64_t shortfall = period - m;
    const bool drained = source_eof_.load() && in_flight_frames_.load() == 0 &&
                         ring_.readable() == 0;

    const char* kind = "audio";
    const char* gap_kind = nullptr;
    if (drained) {
        if (shortfall > 0) gap_kind = kind = "eos";
    } else if (shortfall > 0) {
        if (!submitted_media_this_segment_.load()) {
            // Startup preroll: the device ran before the first real frames.
            // Not an underrun (docs §5).
            gap_kind = kind = "preroll";
        } else {
            gap_kind = kind = "underrun";
        }
    }
    if (m > 0) {
        if (!timeline_.append(SpanKind::Media, m)) {
            timeline_overflow_.store(true);
        }
        submitted_media_this_segment_.store(true);
    }
    if (gap_kind != nullptr) {
        // GAP silence: device duration, ZERO media duration (docs §5).
        const SpanKind gk =
            mut(kMutGapCountsAsMedia) ? SpanKind::Media : SpanKind::Gap;
        if (!timeline_.append(gk, shortfall)) {
            timeline_overflow_.store(true);
        }
        if (gap_kind == std::string_view("preroll")) {
            preroll_events_.fetch_add(1);
            preroll_silence_output_frames_.fetch_add(shortfall);
        } else if (gap_kind == std::string_view("underrun")) {
            underrun_count_.fetch_add(1);
            underrun_silence_output_frames_.fetch_add(shortfall);
        } else {
            eos_silence_output_frames_.fetch_add(shortfall);
        }
    }
    // Frozen ENDED condition (docs §7), computed lock-free for the realtime
    // seam. The control plane performs the state transition on its side.
    if (end_condition() && (mut(kMutEndWithoutPending) || timeline_.pending_media() == 0)) {
        end_pending_.store(true);
    }
    r.kind = kind;
    r.media_frames = m;
    r.silence_frames = gap_kind != nullptr ? shortfall : 0;
    active_fill_.fetch_sub(1, std::memory_order_acq_rel);
    return r;
}

RenderReport PlayerEngine::advance_render(std::int64_t frames,
                                          std::int64_t generation) {
    RenderReport rep;
    active_fill_.fetch_add(1, std::memory_order_acq_rel);
    rep.segment = segment_.load();
    const std::uint64_t epoch = epoch_.load();
    rep.generation = generation == kCurrentGeneration
                         ? epoch
                         : static_cast<std::uint64_t>(generation);
    if (rep.generation != epoch) {
        // A late device event from a dead generation is dropped: stale
        // output accounting can never advance the current media timeline.
        stale_render_events_.fetch_add(1);
        rep.kind = "stale";
        active_fill_.fetch_sub(1, std::memory_order_acq_rel);
        return rep;
    }
    if (state_.load() == PlayerState::Paused) {
        // Pause freezes render advancement; pending output stays pending.
        rep.kind = "paused";
        active_fill_.fetch_sub(1, std::memory_order_acq_rel);
        return rep;
    }
    const std::uint64_t pending = timeline_.pending_output();
    const std::uint64_t wanted = frames > 0 ? static_cast<std::uint64_t>(frames) : 0;
    const std::uint64_t advance = wanted < pending ? wanted : pending;
    const PlaybackTimeline::RenderSplit split = timeline_.advance(advance);
    rendered_media_frames_.fetch_add(split.media);
    if (end_condition() && (mut(kMutEndWithoutPending) || timeline_.pending_media() == 0)) {
        end_pending_.store(true);
    }
    rep.kind = "rendered";
    rep.rendered_output_frames = advance;
    rep.rendered_media_frames = split.media;
    active_fill_.fetch_sub(1, std::memory_order_acq_rel);
    return rep;
}

SubmitReport PlayerEngine::submit(std::uint64_t period_frames) {
    std::lock_guard<std::mutex> g(state_mtx_);
    SubmitReport rep;
    // Fail-closed (never abort) for an oversized period: clamp to the
    // preallocated internal buffer. Only reachable through the internal
    // test API — the product ABI has no submit surface.
    const std::uint64_t bounded =
        std::min(period_frames, config_.max_submit_frames);
    const OutputFillResult r = fill_output(submit_buf_.data(), bounded);
    rep.segment = r.segment;
    rep.kind = r.kind;
    rep.media_frames = r.media_frames;
    rep.silence_frames = r.silence_frames;
    if (r.media_frames > 0) {
        // Test backend log (rendered-content oracle); production backends
        // receive the PCM from fill_output's dst directly.
        backend_.submit_media(submit_buf_.data(), r.media_frames, r.segment);
    }
    if (r.silence_frames > 0) {
        backend_.submit_silence(r.silence_frames, r.segment);
    }
    if (mut(kMutAdvanceOnSubmit) && r.media_frames > 0) {
        // Mutation gate: submission treated as audibility — the
        // submitted-vs-rendered gates must catch this.
        rendered_media_frames_.fetch_add(r.media_frames);
    }
    if (timeline_overflow_.load()) {
        // Fail-closed diagnostic stop (corrective §17): the bounded span
        // store can no longer represent output. Never silent corruption.
        last_error_ = "timeline capacity exhausted";
        state_.store(PlayerState::Error);
    }
    maybe_end();
    if (r.media_frames > 0) work_cv_.notify_all();  // ring space freed
    return rep;
}

RenderReport PlayerEngine::backend_render(std::int64_t frames,
                                          std::int64_t generation) {
    std::lock_guard<std::mutex> g(state_mtx_);
    const RenderReport rep = advance_render(frames, generation);
    if (std::strcmp(rep.kind, "rendered") == 0) {
        backend_.render(rep.rendered_output_frames);  // test content log
    }
    if (timeline_overflow_.load()) {
        last_error_ = "timeline capacity exhausted";
        state_.store(PlayerState::Error);
    }
    maybe_end();
    return rep;
}

// ---------------------------------------------------------------------------
// observability
// ---------------------------------------------------------------------------

EngineSnapshot PlayerEngine::snapshot() const {
    std::lock_guard<std::mutex> g(state_mtx_);
    EngineSnapshot s;
    s.state = state_.load();
    s.media_position_frames = position_frames_locked();
    s.position_quality = segment_qualities_[segment_.load()];
    // Engine-observable decode-position estimate: landing + frames taken
    // since the commit. The frozen ABI has no "tell"; the comparator
    // normalizes the oracle's truth against the same formula.
    s.decoded_source_position =
        handle_ != nullptr ? base_frame_ + decoded_since_commit_ : 0;
    s.duration_frames = handle_ != nullptr ? (duration_known_ ? duration_frames_ : -1) : 0;
    s.duration_known = handle_ != nullptr && duration_known_;
    s.queued_media_frames = ring_.readable();
    s.capacity_frames = ring_.capacity();
    s.epoch = epoch_.load();
    s.segment = segment_.load();
    s.source_eof = source_eof_.load();
    s.underrun_count = underrun_count_.load();
    s.underrun_silence_output_frames = underrun_silence_output_frames_.load();
    s.preroll_events = preroll_events_.load();
    s.preroll_silence_output_frames = preroll_silence_output_frames_.load();
    s.eos_silence_output_frames = eos_silence_output_frames_.load();
    s.decoded_source_frames = decoded_source_frames_;
    s.submitted_output_frames = timeline_.submitted_output_total();
    s.rendered_output_frames = timeline_.rendered_output_total();
    s.pending_output_frames = timeline_.pending_output();
    s.discarded_output_frames = timeline_.discarded_output_total();
    s.submitted_media_frames = timeline_.submitted_media_total();
    s.rendered_media_frames = timeline_.rendered_media_total();
    s.rendered_gap_output_frames = timeline_.rendered_gap_total();
    s.pending_media_frames = timeline_.pending_media();
    s.discarded_output_media_frames = timeline_.discarded_media_total();
    s.discarded_stale_media_frames = discarded_stale_media_frames_.load();
    s.stale_render_events = stale_render_events_.load();
    s.in_flight_frames = in_flight_frames_.load();
    s.ring_produced_total = ring_.produced_total();
    s.ring_consumed_total = ring_.consumed_total();
    s.ring_discarded_total = ring_.discarded_total();
    std::snprintf(s.last_error, sizeof s.last_error, "%s", last_error_.c_str());
    return s;
}

std::int64_t PlayerEngine::media_position_at_output(std::uint64_t output_pos) const {
    std::lock_guard<std::mutex> g(state_mtx_);
    return base_frame_ + static_cast<std::int64_t>(timeline_.media_at_output(output_pos));
}

std::int64_t PlayerEngine::segment_anchor(std::uint64_t segment) const {
    std::lock_guard<std::mutex> g(state_mtx_);
    return segment < segment_anchors_.size() ? segment_anchors_[segment] : 0;
}

LandingQuality PlayerEngine::segment_landing_quality(std::uint64_t segment) const {
    std::lock_guard<std::mutex> g(state_mtx_);
    return segment < segment_qualities_.size() ? segment_qualities_[segment]
                                               : LandingQuality::Confirmed;
}

std::int64_t PlayerEngine::segment_rendered_media() const {
    std::lock_guard<std::mutex> g(state_mtx_);
    return static_cast<std::int64_t>(rendered_media_frames_.load());
}

void PlayerEngine::debug_set_source_hint(std::int64_t remaining_frames) {
    std::lock_guard<std::mutex> g(state_mtx_);
    remaining_hint_ = remaining_frames;
}

void PlayerEngine::debug_set_work_steps(std::uint64_t steps) {
    std::lock_guard<std::mutex> g(state_mtx_);
    work_steps_ = steps;
}

void PlayerEngine::debug_set_mutations(const DebugMutations& m) {
    std::lock_guard<std::mutex> g(state_mtx_);
    std::uint32_t bits = 0;
    if (m.skip_epoch_guard) bits |= kMutSkipEpochGuard;
    if (m.advance_on_submit) bits |= kMutAdvanceOnSubmit;
    if (m.gap_counts_as_media) bits |= kMutGapCountsAsMedia;
    if (m.end_without_pending) bits |= kMutEndWithoutPending;
    mutations_.store(bits);
}

void PlayerEngine::debug_set_publish_hook(std::function<void()> fn) {
    std::lock_guard<std::mutex> g(hook_mtx_);
    publish_hook_ = std::move(fn);
}

void PlayerEngine::debug_set_read_hook(std::function<void()> fn) {
    std::lock_guard<std::mutex> g(hook_mtx_);
    read_hook_ = std::move(fn);
}

void PlayerEngine::debug_set_fill_hook(std::function<void()> fn) {
    std::lock_guard<std::mutex> g(hook_mtx_);
    fill_hook_ = std::move(fn);
}

void PlayerEngine::debug_set_control_hook(std::function<void()> fn) {
    std::lock_guard<std::mutex> g(hook_mtx_);
    control_hook_ = std::move(fn);
}

void PlayerEngine::debug_set_quiesce_hook(std::function<void()> fn) {
    std::lock_guard<std::mutex> g(hook_mtx_);
    quiesce_hook_ = std::move(fn);
}

// ---------------------------------------------------------------------------
// internals — all callers hold src_mtx_ AND state_mtx_ unless noted
// ---------------------------------------------------------------------------

std::optional<std::pair<std::int64_t, LandingQuality>> PlayerEngine::landing_of(
    song_status status, std::int64_t actual_us, std::int64_t requested_us) const {
    if (status != SONG_OK) return std::nullopt;
    if (actual_us >= 0) {
        // CONFIRMED: the engine rebases on the RETURNED landing, never on
        // the request (docs §6.1).
        return std::make_pair(us_to_frames_nearest(actual_us, source_rate_),
                              LandingQuality::Confirmed);
    }
    // ESTIMATED: SONG_OK with a genuinely unknown landing — the clamped
    // requested target, never described as an actual landing.
    std::int64_t target = us_to_frames_floor(requested_us, source_rate_);
    if (target < 0) target = 0;
    if (duration_known_ && target > duration_frames_) target = duration_frames_;
    return std::make_pair(target, LandingQuality::Estimated);
}

void PlayerEngine::quiesce_backend() {
    // Realtime fill/advance never block on state_mtx_, so this wait is
    // bounded. In the manual-tick (NullBackend) test model the only
    // concurrent caller is a stress thread driving the realtime seam;
    // control commits wait for it to finish before touching ring/timeline.
    if (active_fill_.load(std::memory_order_acquire) == 0) return;
    std::function<void()> hook;
    {
        std::lock_guard<std::mutex> hk(hook_mtx_);
        hook = quiesce_hook_;
    }
    // Test barrier: fires (with both control locks held) only when a real
    // quiesce wait is actually required. The hook must not call back into
    // the engine — state_mtx_ is held.
    if (hook) hook();
    while (active_fill_.load(std::memory_order_acquire) != 0) {
        std::this_thread::yield();
    }
}

void PlayerEngine::invalidate() {
    // Kill the current generation: epoch bump FIRST (invalidates producer
    // output), discard pending output (submitted-but-unrendered spans can
    // never advance the next segment), flush the queue. The media clock is
    // intentionally NOT touched: position stays at the last audible endpoint
    // until a new landing commits (or freezes forever in ERROR).
    // Backend quiescence (P1-A): no realtime fill/advance may be mid-flight
    // while the ring/timeline are reset. Callers hold state_mtx_, so the
    // manual-tick path is already serialized; the wait covers the production
    // lock-free seam.
    std::function<void()> hook;
    {
        std::lock_guard<std::mutex> hk(hook_mtx_);
        hook = control_hook_;
    }
    if (hook) hook();  // test barrier, both locks held
    quiesce_backend();
    ++epoch_;
    timeline_.invalidate();
    backend_.reset();
    ring_.flush();
    source_eof_.store(false);
    end_pending_.store(false);
    timeline_overflow_.store(false);
    AudioEngineBypass aes;
    aes.reset();
}

void PlayerEngine::commit_landing(std::int64_t landing_frames, LandingQuality quality) {
    base_frame_ = landing_frames;
    rendered_media_frames_.store(0);
    submitted_media_this_segment_.store(false);
    ++segment_;
    segment_anchors_.push_back(landing_frames);
    segment_qualities_.push_back(quality);
    decoded_since_commit_ = 0;
}

bool PlayerEngine::end_condition() const {
    // Reads only atomics + the lock-free ring — safe from the realtime path.
    return state_.load() == PlayerState::Playing && source_eof_.load() &&
           in_flight_frames_.load() == 0 && ring_.readable() == 0;
}

void PlayerEngine::maybe_end() {
    // ENDED requires the complete audible drain: source exhausted, queue
    // empty, nothing in flight, and no pending MEDIA output. Trailing EOS
    // padding must not postpone it (docs §7).
    if (!end_condition()) return;
    if (!mut(kMutEndWithoutPending) && timeline_.pending_media() != 0) return;
    state_.store(PlayerState::Ended);
    end_pending_.store(false);
    AudioEngineBypass aes;
    aes.drain();
}

std::int64_t PlayerEngine::source_remaining() const {
    // state_mtx_ held. Test hint (fake truth) first; production derives the
    // estimate from the known duration and the decode-position estimate —
    // exact for CONFIRMED landings, off by the unknown segment offset for
    // ESTIMATED ones, unbounded when the duration is unknown.
    if (remaining_hint_ >= 0) return remaining_hint_;
    if (!duration_known_) return -1;
    return duration_frames_ - (base_frame_ + decoded_since_commit_);
}

std::int64_t PlayerEngine::position_frames_locked() const {
    if (state_.load() == PlayerState::Ended && duration_known_) return duration_frames_;
    std::int64_t pos = base_frame_ + static_cast<std::int64_t>(rendered_media_frames_.load());
    if (duration_known_ && pos > duration_frames_) pos = duration_frames_;
    return pos;
}

}  // namespace qn
