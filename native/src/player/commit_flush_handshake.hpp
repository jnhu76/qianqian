// commit_flush_handshake.hpp — the #40 commit-flush protocol, as a type.
//
// The request/claim/complete/cancel state machine between the engine's
// control thread (the commit boundary inside invalidate()) and the
// backend's render thread (the physical device flush). Platform-neutral on
// purpose: the transition rules below ARE the correctness argument of PR
// #42 v3, so they live in a header the deterministic gates can drive
// without threads, a scheduler, or a Windows runtime. Synchronization
// objects (the render thread's sleep cv, the device event) stay with their
// owners; this class is atomics only.
//
// One request is in flight at a time (the engine serializes commits under
// src_mtx_ + state_mtx_). Each request id moves through:
//
//   REQUESTED(id)  control published it; the renderer may claim it, or
//                  control may cancel it before the claim
//   CLAIMED(id)    the renderer owns the physical flush; control can no
//                  longer cancel — the outcome is definitive and MUST be
//                  awaited (I4), however long the device takes
//   COMPLETED(id)  the physical flush finished; outcome() is its verdict
//   CANCELLED(id)  control cancelled before the claim: the operation never
//                  began and the renderer can never execute it (I3)
//
// Invariants (v3 review):
//   I3  a cancelled request can never reach CLAIMED — claim()'s CAS fails
//       against CANCELLED, so no ghost flush can execute after the commit
//       it belonged to was aborted;
//   I4  a claimed request can never be cancelled — try_cancel()'s CAS
//       fails against CLAIMED, so control can never fake a rollback after
//       an irreversible physical action may have started;
//   I5  a completion is bound to its request id (the id rides in the slot
//       the CAS matches on), so a stale/late ACK can never satisfy another
//       request — no cross-request ACK, no ABA.
#ifndef QIANQIAN_PLAYER_COMMIT_FLUSH_HANDSHAKE_HPP
#define QIANQIAN_PLAYER_COMMIT_FLUSH_HANDSHAKE_HPP

#include <atomic>
#include <cstdint>
#include <utility>

namespace qn {

class CommitFlushHandshake {
public:
    enum Phase : std::uint64_t {
        kIdle = 0,
        kRequested = 1,
        kClaimed = 2,
        kCompleted = 3,
        kCancelled = 4,
    };

    // Control: publish a new request and return its id. Single flight —
    // the previous request must be resolved (completed/cancelled) first.
    std::uint64_t request() {
        const std::uint64_t id =
            next_id_.fetch_add(1, std::memory_order_seq_cst) + 1;
        state_.store(pack(id, kRequested), std::memory_order_seq_cst);
        return id;
    }

    // Renderer: atomically claim the request. false = it was cancelled (or
    // superseded by a newer request) — the physical flush must NOT execute.
    bool claim(std::uint64_t id) {
        std::uint64_t expected = pack(id, kRequested);
        return state_.compare_exchange_strong(expected, pack(id, kClaimed),
                                              std::memory_order_seq_cst);
    }

    // Renderer: publish the physical outcome. The verdict is stored BEFORE
    // the phase, so a control thread that observes COMPLETED also observes
    // the verdict (seq_cst).
    void complete(std::uint64_t id, bool ok) {
        ok_.store(ok, std::memory_order_seq_cst);
        std::uint64_t expected = pack(id, kClaimed);
        state_.compare_exchange_strong(expected, pack(id, kCompleted),
                                       std::memory_order_seq_cst);
    }

    // Control: cancel before the claim. true = cancelled (safe rollback;
    // the commit aborts with the old generation intact). false = the
    // renderer already claimed — never fake a rollback past this point.
    bool try_cancel(std::uint64_t id) {
        std::uint64_t expected = pack(id, kRequested);
        return state_.compare_exchange_strong(expected, pack(id, kCancelled),
                                              std::memory_order_seq_cst);
    }

    Phase phase() const {
        return unpack(state_.load(std::memory_order_seq_cst)).second;
    }

    // The id of the request a renderer should service, or 0.
    std::uint64_t pending_request() const {
        const auto p = unpack(state_.load(std::memory_order_seq_cst));
        return p.second == kRequested ? p.first : 0;
    }

    bool completed(std::uint64_t id) const { return phase_is(id, kCompleted); }
    bool cancelled(std::uint64_t id) const { return phase_is(id, kCancelled); }
    bool claimed(std::uint64_t id) const { return phase_is(id, kClaimed); }

    // Verdict of the completed request (valid once completed(id) holds).
    bool outcome() const { return ok_.load(std::memory_order_seq_cst); }

private:
    static std::uint64_t pack(std::uint64_t id, Phase ph) {
        return (id << 3) | static_cast<std::uint64_t>(ph);
    }
    static std::pair<std::uint64_t, Phase> unpack(std::uint64_t v) {
        return {v >> 3, static_cast<Phase>(v & 7u)};
    }
    bool phase_is(std::uint64_t id, Phase ph) const {
        const auto p = unpack(state_.load(std::memory_order_seq_cst));
        return p.first == id && p.second == ph;
    }

    // Packed current slot: (id << 3) | phase. The id rides in the high
    // bits so an observer of a superseded request can never match the
    // slot — that packing IS the ABA defense.
    std::atomic<std::uint64_t> state_{0};  // pack(0, kIdle)
    std::atomic<bool> ok_{false};
    std::atomic<std::uint64_t> next_id_{0};
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_COMMIT_FLUSH_HANDSHAKE_HPP
