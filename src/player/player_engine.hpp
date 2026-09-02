// player_engine.hpp — native PlayerEngine (Phase 1.5 internal API).
//
// Behavioral mirror of the frozen Phase-1 oracle
// (tools/player_model/player_model.py + docs/player-engine.md). It
// reproduces the oracle's OBSERVABLE semantics — state machine, epoch-
// guarded seek, bounded queue lifecycle, EOF drain, the device/media
// timeline split, submitted-vs-rendered output — not its implementation
// details.
//
// This is the INTERNAL native API: not a frozen ABI. The C ABI header
// (include/player_engine.h) exposes only the product control/observation
// surface; everything here (manual submit/render, debug hooks, the
// backend seam) is internal C++.
//
// Thread model (docs §9, corrective §7–§8), mirrored from the oracle's
// call discipline:
//   control thread   open/play/pause/stop/seek — serialized, take
//                    src_mtx_ THEN state_mtx_ (epoch must rise inside the
//                    src_mtx_ section so a decode's epoch and its data can
//                    never be paired across a seek). Control ops may block:
//                    they quiesce the backend before touching ring/timeline.
//   decode worker    worker_step(): SongCore read under src_mtx_ (outside
//                    state_mtx_), publication under state_mtx_ with the
//                    epoch guard — stale results die at publish time
//   backend side     fill_output()/advance_render() are the PRODUCTION
//                    realtime seam (corrective §6): no heap allocation, no
//                    SongCore, no filesystem, no logging, and NO wait on
//                    state_mtx_. They touch only the lock-free SPSC ring,
//                    the bounded timeline, and atomics. The manual-tick
//                    submit()/backend_render() are the deterministic TEST
//                    entry points (NullAudioBackend): they hold state_mtx_
//                    and additionally drive the test backend's content log.
//
// One engine owns one song at a time. The SongCore handle is not internally
// thread-safe: every song_* call happens under src_mtx_.
#ifndef QIANQIAN_PLAYER_PLAYER_ENGINE_HPP
#define QIANQIAN_PLAYER_PLAYER_ENGINE_HPP

#include <atomic>
#include <condition_variable>
#include <cstdint>
#include <functional>
#include <limits>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <vector>

#include "null_audio_backend.hpp"
#include "pcm_ring.hpp"
#include "playback_timeline.hpp"
#include "songcore.h"

namespace qn {

enum class PlayerState : std::uint8_t {
    Empty, Ready, Playing, Paused, Ended, Error,
};

enum class LandingQuality : std::uint8_t { Confirmed, Estimated };

// Typed control result. The oracle raises EngineError/SongSeekError; the
// native API returns codes (the C ABI maps 1:1).
enum class PlayerStatus : std::int32_t {
    Ok = 0,
    ErrIllegalCall = 1,   // call not allowed in current state
    ErrOpenFailed = 2,    // *out_song_status carries the song_open status
    ErrSeekFailed = 3,    // *out_song_status carries the song_seek status
};

enum class StepOutcome : std::uint8_t {
    Idle, Begin, Working, Wrote, Backpressure, Stale,
};

struct EngineConfig {
    std::uint64_t capacity_frames;      // the ONLY queue sizing
    std::uint64_t read_chunk_frames;    // song_read_pcm capacity per chunk
    std::uint64_t max_submit_frames;    // largest submit() period supported
    std::int32_t max_channels = 8;      // ring slot stride; songs may use fewer
    bool worker_thread = false;         // spawn the decode worker thread
};

struct StepReport {
    StepOutcome outcome = StepOutcome::Idle;
    std::uint64_t frames = 0;
    std::uint64_t epoch = 0;
    std::int64_t start_frame = 0;
};

struct SubmitReport {
    std::uint64_t segment = 0;
    std::uint64_t media_frames = 0;
    std::uint64_t silence_frames = 0;
    // "audio" | "underrun" | "preroll" | "eos" | "idle"
    const char* kind = "idle";
};

struct RenderReport {
    std::uint64_t segment = 0;
    // "rendered" | "paused" | "stale"
    const char* kind = "rendered";
    std::uint64_t rendered_output_frames = 0;
    std::uint64_t rendered_media_frames = 0;
    std::uint64_t generation = 0;
};

// Result of the realtime output-fill seam (corrective §6). `dst` received
// media_frames of interleaved PCM; silence_frames are device-duration GAP
// padding (zero media duration). kind is a static literal.
struct OutputFillResult {
    std::uint64_t segment = 0;
    std::uint64_t media_frames = 0;
    std::uint64_t silence_frames = 0;
    // "audio" | "underrun" | "preroll" | "eos" | "idle"
    const char* kind = "idle";
};

// Mirrors the oracle's EngineSnapshot field-for-field (frame domain; names
// carry their domain per docs §2.6). INTERNAL diagnostics — the product C
// snapshot (include/player_engine.h) is the small time-domain view.
struct EngineSnapshot {
    PlayerState state = PlayerState::Empty;
    std::int64_t media_position_frames = 0;
    LandingQuality position_quality = LandingQuality::Confirmed;
    std::int64_t decoded_source_position = 0;  // engine-observable estimate
    std::int64_t duration_frames = 0;          // -1 = unknown, never fake 0
    bool duration_known = false;
    std::uint64_t queued_media_frames = 0;
    std::uint64_t capacity_frames = 0;
    std::uint64_t epoch = 0;
    std::uint64_t segment = 0;
    bool source_eof = false;
    std::uint64_t underrun_count = 0;
    std::uint64_t underrun_silence_output_frames = 0;
    std::uint64_t preroll_events = 0;
    std::uint64_t preroll_silence_output_frames = 0;
    std::uint64_t eos_silence_output_frames = 0;
    std::uint64_t decoded_source_frames = 0;
    std::uint64_t submitted_output_frames = 0;
    std::uint64_t rendered_output_frames = 0;
    std::uint64_t pending_output_frames = 0;
    std::uint64_t discarded_output_frames = 0;
    std::uint64_t submitted_media_frames = 0;
    std::uint64_t rendered_media_frames = 0;
    std::uint64_t rendered_gap_output_frames = 0;
    std::uint64_t pending_media_frames = 0;
    std::uint64_t discarded_output_media_frames = 0;
    std::uint64_t discarded_stale_media_frames = 0;
    std::uint64_t stale_render_events = 0;
    // Coherent decode-accounting inputs: captured in the SAME state_mtx_
    // hold as decoded_source_frames above, so the conservation equation
    // decoded == ring_produced + discarded_stale + in_flight is evaluated
    // at one instant, never across three separate lock acquisitions.
    std::uint64_t in_flight_frames = 0;
    std::uint64_t ring_produced_total = 0;
    std::uint64_t ring_consumed_total = 0;
    std::uint64_t ring_discarded_total = 0;
    char last_error[96] = {0};  // "" = none; comparator-normalized categories
};

class PlayerEngine {
public:
    explicit PlayerEngine(const EngineConfig& config);
    ~PlayerEngine();

    PlayerEngine(const PlayerEngine&) = delete;
    PlayerEngine& operator=(const PlayerEngine&) = delete;

    // -- control plane (serialized) --------------------------------------------
    // open(song): stop everything, drop the previous handle, start the new
    // song at position 0 (a fresh handle — CONFIRMED), state Ready. Never
    // autoplays. `io` is copied and reused for stop()-recovery reopens.
    PlayerStatus open(const song_io& io, std::int32_t* out_song_status = nullptr);
    PlayerStatus play();
    void pause();  // documented no-op outside Playing
    PlayerStatus stop(std::int32_t* out_song_status = nullptr);
    // seek(T) from Ready/Playing/Paused (and Ended -> Ready). On success
    // *out_landing_frames carries the landing the clock was rebased on.
    PlayerStatus seek(std::int64_t position_us, std::int64_t* out_landing_frames = nullptr,
                      std::int32_t* out_song_status = nullptr);

    // -- decode worker ------------------------------------------------------------
    // One worker quantum (oracle producer_step). Safe from the worker thread
    // or, in manual mode, the test thread.
    StepReport worker_step();

    // -- backend side ----------------------------------------------------------------
    // PRODUCTION REALTIME SEAM (corrective §6–§8): one bounded output fill.
    // Copies up to requested_output_frames from the queue into `dst`
    // (caller-preallocated, >= requested frames * channels), appends the
    // MEDIA/GAP spans to the bounded timeline, updates atomic counters.
    // Contract: no heap allocation, no vector growth, no logging, no
    // SongCore, no filesystem, no blocking mutex. Callable concurrently
    // with control commits — a commit quiesces first (waits for the fill to
    // finish) before touching the ring/timeline.
    OutputFillResult fill_output(float* dst, std::uint64_t requested_output_frames);

    // PRODUCTION REALTIME SEAM: render-clock progression. `frames` of
    // device output are proven rendered; generation == kCurrentGeneration
    // = current, any other value is a literal dead generation whose late
    // event is dropped. Same realtime contract as fill_output.
    RenderReport advance_render(std::int64_t frames,
                                std::int64_t generation = kCurrentGeneration);

    // Deterministic manual-tick entry points (TEST-ONLY; NullAudioBackend
    // drives the rendered-content log for the content-continuity oracle).
    // submit() = fill_output into an internal buffer + test backend log.
    SubmitReport submit(std::uint64_t period_frames);
    // backend_render() = advance_render + test backend log.
    RenderReport backend_render(std::int64_t frames,
                                std::int64_t generation = kCurrentGeneration);

    static constexpr std::int64_t kCurrentGeneration =
        std::numeric_limits<std::int64_t>::min();

    // -- observability ---------------------------------------------------------------
    EngineSnapshot snapshot() const;
    // device→media mapping probe (s3 gate): map an output-domain position
    // (current generation) to a media-timeline position.
    std::int64_t media_position_at_output(std::uint64_t output_pos) const;
    // Per-segment landing anchors / landing quality (content-continuity
    // oracle inputs; hidden truth stays in the fake, never here).
    std::int64_t segment_anchor(std::uint64_t segment) const;
    LandingQuality segment_landing_quality(std::uint64_t segment) const;
    // Media frames of the CURRENT segment proven rendered (the frozen clock
    // formula's second term; check_all uses it directly).
    std::int64_t segment_rendered_media() const;
    const NullAudioBackend& backend() const { return backend_; }
    const PlaybackTimeline& timeline_debug() const { return timeline_; }
    std::uint64_t in_flight_frames() const { return in_flight_frames_.load(); }
    std::int32_t source_rate() const { return source_rate_; }
    // Test-only: ring lifetime diagnostics (atomics; safe unlocked).
    const PcmRing& ring_debug() const { return ring_; }
    // Test-only: the realtime seam's end-of-playout signal (set lock-free by
    // fill/advance when the frozen ENDED condition holds; the control plane
    // performs the state transition).
    bool debug_end_pending() const { return end_pending_.load(); }

    // -- test-only hooks (never on the production ABI; docs §53) -------------------
    // Worker read sizing: the frozen backpressure rule is
    // `begin iff writable >= min(chunk, remaining)`. `remaining` is truth in
    // the oracle but unknowable through the frozen SongCore ABI; production
    // derives an estimate (CONFIRMED landings: exact). The trace runner
    // injects the fake's truth so decision sequences match the oracle 1:1.
    void debug_set_source_hint(std::int64_t remaining_frames);  // <0 = clear
    // In-flight decode latency in worker quanta (the fake's work_steps).
    void debug_set_work_steps(std::uint64_t steps);
    // Mutation gates (§74): each intentionally breaks one semantic; the
    // corresponding test asserts the suite CATCHES it.
    struct DebugMutations {
        bool skip_epoch_guard = false;      // stale decode may publish
        bool advance_on_submit = false;     // submission advances media clock
        bool gap_counts_as_media = false;   // GAP spans advance media time
        bool end_without_pending = false;   // ENDED ignores pending media
    };
    void debug_set_mutations(const DebugMutations& m);
    // Thread-stress barriers: invoked at the labeled points when set. The
    // worker copies the hook under a lock and invokes the copy with no
    // engine locks held, so assigning/clearing from the control thread can
    // never race the invocation itself.
    void debug_set_publish_hook(std::function<void()> fn);  // before epoch check
    void debug_set_read_hook(std::function<void()> fn);     // after song_read_pcm
    void debug_set_fill_hook(std::function<void()> fn);     // mid-fill (realtime seam)
    void debug_set_control_hook(std::function<void()> fn);  // under state_mtx_
    void debug_set_quiesce_hook(std::function<void()> fn);  // control waits on active fill

private:
    enum MutBits : std::uint32_t {
        kMutSkipEpochGuard = 1u << 0,
        kMutAdvanceOnSubmit = 1u << 1,
        kMutGapCountsAsMedia = 1u << 2,
        kMutEndWithoutPending = 1u << 3,
    };
    bool mut(std::uint32_t bit) const { return (mutations_.load() & bit) != 0; }

    // One decode result in flight, not yet published. Carries the epoch it
    // was decoded under — the entire stale-frame defense. (The oracle's
    // chunk.eof flag has no native counterpart: the frozen ABI reports EOF
    // as a separate 0-frame SONG_EOF read, which the worker issues next.)
    struct InFlight {
        std::uint64_t epoch;
        std::int64_t start_frame;
        std::uint64_t frames;
        std::uint64_t steps_left;
    };

    void worker_loop();
    // Landing computation: song_seek result -> (landing frames, quality).
    // nullopt = the seek failed (fail-closed; caller goes to Error).
    std::optional<std::pair<std::int64_t, LandingQuality>> landing_of(
        song_status status, std::int64_t actual_us, std::int64_t requested_us) const;
    // Commit machinery (caller holds src_mtx_ AND state_mtx_).
    void invalidate();
    void commit_landing(std::int64_t landing_frames, LandingQuality quality);
    // Wait until no realtime fill/advance is mid-flight. The realtime path
    // never blocks on state_mtx_, so the wait is bounded; called from
    // control commits (under state_mtx_) before touching ring/timeline.
    void quiesce_backend();
    // Frozen ENDED condition (docs §7): playing, source exhausted, queue
    // empty, nothing in flight. Reads atomics — safe from the realtime path.
    bool end_condition() const;
    void maybe_end();
    std::int64_t source_remaining() const;  // hint if set, else estimate
    std::int64_t position_frames_locked() const;

    // Config / collaborators.
    EngineConfig config_;
    PcmRing ring_;
    NullAudioBackend backend_;
    PlaybackTimeline timeline_;

    // SongCore source (all calls under src_mtx_).
    song_handle* handle_ = nullptr;
    song_io io_copy_{};
    bool has_io_ = false;
    std::int32_t source_rate_ = 0;
    std::int32_t channels_ = 2;
    std::int64_t duration_frames_ = 0;  // -1 = unknown
    bool duration_known_ = false;

    // Engine state. The realtime seam (fill_output/advance_render) reads the
    // ATOMIC members below lock-free; the control plane and decode worker
    // write them (usually under state_mtx_ — atomics are safe either way).
    mutable std::mutex src_mtx_;   // serializes song_* calls (outer lock)
    mutable std::mutex state_mtx_;  // engine state machine (inner lock)
    std::atomic<PlayerState> state_{PlayerState::Empty};
    std::atomic<std::uint64_t> epoch_{0};
    std::atomic<std::uint64_t> segment_{0};
    std::vector<std::int64_t> segment_anchors_{0};
    std::vector<LandingQuality> segment_qualities_{LandingQuality::Confirmed};
    std::int64_t base_frame_ = 0;          // current segment landing
    std::atomic<std::uint64_t> rendered_media_frames_{0};  // per-segment, media domain
    std::atomic<bool> submitted_media_this_segment_{false};
    std::atomic<bool> source_eof_{false};
    std::optional<InFlight> in_flight_;    // decode worker only (state_mtx_)
    std::atomic<std::uint64_t> in_flight_frames_{0};  // realtime mirror
    std::int64_t decoded_since_commit_ = 0;

    // Realtime seam signals (lock-free).
    std::atomic<std::uint64_t> active_fill_{0};   // fills/advances in flight
    std::atomic<bool> end_pending_{false};        // ENDED condition observed
    std::atomic<bool> timeline_overflow_{false};  // fail-closed span store

    // Buffers (preallocated; sized at construction).
    std::vector<float> chunk_buf_;
    std::vector<float> submit_buf_;

    // Diagnostics (atomic where the realtime seam writes them).
    std::uint64_t decoded_source_frames_ = 0;  // worker only (state_mtx_)
    std::atomic<std::uint64_t> discarded_stale_media_frames_{0};
    std::atomic<std::uint64_t> stale_render_events_{0};
    std::atomic<std::uint64_t> underrun_count_{0};
    std::atomic<std::uint64_t> underrun_silence_output_frames_{0};
    std::atomic<std::uint64_t> preroll_events_{0};
    std::atomic<std::uint64_t> preroll_silence_output_frames_{0};
    std::atomic<std::uint64_t> eos_silence_output_frames_{0};
    std::string last_error_;

    // Test hooks / mutations (see accessors).
    mutable std::mutex hook_mtx_;
    std::function<void()> publish_hook_;
    std::function<void()> read_hook_;
    std::function<void()> fill_hook_;
    std::function<void()> control_hook_;
    std::function<void()> quiesce_hook_;
    std::int64_t remaining_hint_ = -1;
    std::uint64_t work_steps_ = 1;
    std::atomic<std::uint32_t> mutations_{0};

    // Worker thread.
    std::thread worker_;
    std::condition_variable work_cv_;
    bool shutdown_ = false;
};

// Exact integer us<->frame conversions (oracle fake_decoder.py formulas).
std::int64_t us_to_frames_floor(std::int64_t us, std::int64_t rate);
std::int64_t us_to_frames_nearest(std::int64_t us, std::int64_t rate);
std::int64_t frames_to_us(std::int64_t frames, std::int64_t rate);

}  // namespace qn

#endif  // QIANQIAN_PLAYER_PLAYER_ENGINE_HPP
