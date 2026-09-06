// wasapi_submit_accounting.hpp — the period loop's submit-accounting
// decision, extracted verbatim so the I6 rule is deterministically testable
// on every platform (gate t30). The Windows call site installs exactly this
// result; nothing else about the period accounting moved.
//
// I6 (v3 review): accounting may move only after the physical API
// operation that establishes it succeeds. ReleaseBuffer failing means the
// device did NOT accept the frames — written_total must not advance, and
// the period must escalate to teardown instead of reconciling or
// fabricating advance_render evidence.
#ifndef QIANQIAN_PLAYER_WASAPI_SUBMIT_ACCOUNTING_HPP
#define QIANQIAN_PLAYER_WASAPI_SUBMIT_ACCOUNTING_HPP

#include <cstdint>

namespace qn {

struct SubmitAccounting {
    std::uint64_t written_total;  // the value to install
    bool teardown;                // true: the session is unproven — tear down
};

inline SubmitAccounting apply_release_result(bool released,
                                             std::uint32_t submitted,
                                             std::uint64_t written_total) {
    if (!released) return SubmitAccounting{written_total, true};
    return SubmitAccounting{written_total + submitted, false};
}

}  // namespace qn

#endif  // QIANQIAN_PLAYER_WASAPI_SUBMIT_ACCOUNTING_HPP
