// pcm_ring.hpp — bounded, preallocated, frame-based PCM queue (native).
//
// The accounting unit is the FRAME: one frame = one sample for every
// channel. Capacity is fixed at construction; the ring never grows, never
// overwrites unread frames, never reads unwritten frames, and never
// allocates in write/read/flush.
//
// SPSC by design: producer = decode worker (write), consumer = the backend
// submission side (read). flush() empties the queue at commit boundaries
// (seek / stop / open); in the engine every ring operation happens under the
// engine state mutex, so the atomics below are the standalone lock-free
// correctness mechanism (proven by pcm_ring_test without external locking)
// and stay correct for a future lock-free render path.
//
// Ring-level lifetime accounting (docs/architecture/player-runtime.md):
//     produced_total == consumed_total + buffered + discarded_total
//
// Memory ordering: seq_cst everywhere. Correctness is preferred over
// memory-order cleverness; the acquire/release pairs actually required are
// a strict subset.
#ifndef QIANQIAN_PLAYER_PCM_RING_HPP
#define QIANQIAN_PLAYER_PCM_RING_HPP

#include <atomic>
#include <cstdint>
#include <vector>

namespace qn {

class PcmRing {
public:
    // slot_stride is the fixed per-frame slot size in floats (the engine
    // allocates max_channels per slot so the buffer never grows across
    // songs); capacity_frames > 0.
    PcmRing(std::uint64_t capacity_frames, std::int32_t slot_stride);

    // Set the ACTUAL per-frame data width of the current song (<= slot
    // stride). Call only while the ring is empty (song open after a flush);
    // frames are then copied data_stride floats per slot, leaving the rest
    // of each slot untouched (never read back).
    void set_data_stride(std::int32_t data_stride);

    // -- geometry -----------------------------------------------------------
    std::uint64_t capacity() const { return capacity_; }
    std::int32_t channels() const { return data_stride_; }

    // Producer-side writable space (capacity - buffered).
    std::uint64_t writable() const;
    // Consumer-side buffered frames.
    std::uint64_t readable() const;

    // -- producer side ------------------------------------------------------
    // Store up to `frames` from `src` (interleaved, frames*channels floats)
    // in FIFO order. Never overwrites unread frames; returns the count
    // actually stored. A well-behaved producer checks writable() first and
    // treats a short write as backpressure.
    std::uint64_t write(const float* src, std::uint64_t frames);

    // -- consumer side ------------------------------------------------------
    // Consume up to `frames` into `dst`, oldest first. Never over-reads.
    std::uint64_t read(float* dst, std::uint64_t frames);

    // -- flush ---------------------------------------------------------------
    // Drop every unread frame (seek / stop / open). Returns the count.
    std::uint64_t flush();

    // -- diagnostics ----------------------------------------------------------
    std::uint64_t produced_total() const { return produced_; }
    std::uint64_t consumed_total() const { return consumed_; }
    std::uint64_t discarded_total() const { return discarded_; }
    std::uint64_t buffered_total() const;  // buffered, safe from any thread

    // Ring invariants (0 <= buffered <= capacity, readable + writable ==
    // capacity, lifetime accounting). Exposed for the engine's after-every-
    // operation checks. Returns false + fills `why` on violation.
    bool check_invariants(char* why, std::size_t why_len) const;

private:
    std::uint64_t capacity_;
    std::int32_t stride_;       // slot width in floats (fixed)
    std::int32_t data_stride_;  // active per-frame data width (<= stride_)
    std::vector<float> slots_;  // capacity_ * stride_, preallocated

    // Monotonic frame counters; slot index = counter % capacity_. Owned by
    // their single side (head_: producer store / consumer load; tail_ the
    // mirror), which is what makes plain % indexing race-free under SPSC.
    std::atomic<std::uint64_t> head_{0};  // total frames ever written
    std::atomic<std::uint64_t> tail_{0};  // total frames ever consumed

    // Lifetime diagnostics. produced_ moves only on the producer side,
    // consumed_ only on the consumer side; flush() (control) folds buffered
    // into discarded_. All seq_cst.
    std::atomic<std::uint64_t> produced_{0};
    std::atomic<std::uint64_t> consumed_{0};
    std::atomic<std::uint64_t> discarded_{0};
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_PCM_RING_HPP
