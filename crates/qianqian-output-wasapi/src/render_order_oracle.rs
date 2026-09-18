//! Source-order oracle for the render leg's frame accounting (F4,
//! ADR-PBK-002 D14.8).
//!
//! The frozen D14.8 derivation pairs two values that must come from ONE
//! instant: the leg's handed-off total as it stood when the padding was
//! read, and that padding reading. Both live on the render thread, which
//! only exists on Windows, so the pairing cannot be exercised by a
//! platform-independent test — it is pinned here at the level it
//! actually has: the ORDER of the calls in the mechanism's own source.
//!
//! What this oracle checks, read against `wasapi.rs` as text:
//!
//! ```text
//! P1  the steady loop publishes before it acquires a device buffer
//!     (the shipped order; P2 carries the binding constraint)
//! P2  EVERY publication site in the steady loop precedes this
//!     iteration's credit, so no site can pair the key's padding
//!     reading with a handed-off total that already includes the block
//!     being submitted;
//! P3  the credit follows a successful ReleaseBuffer and its failure
//!     break, so a failed submission cannot be counted;
//! P4  exactly one place in the whole mechanism credits frames;
//! P5  the park-slice observation publishes;
//! P6  the EOF drain publishes and credits nothing;
//! P7  only CODE sites count: a call moved into a comment mentions the
//!     same text without executing anything, so sites are matched at
//!     statement position, never inside a line that has code before it;
//! P8  the park slice publishes UNCONDITIONALLY (at its argument's top
//!     level). Quiescence is the slice whose publication matters most —
//!     it is what walks a paused episode's sample up to the frozen
//!     total — so a publication gated behind a condition, which skips
//!     exactly that slice, must RED;
//! P9  since F5 every publication in the mechanism routes through the
//!     one basis-aware helper (`publish_consumed(position, …)`, which
//!     folds the stretch basis into the estimate), and the mechanism
//!     holds EXACTLY ONE direct cell publication — inside that helper.
//!     A second direct `position.publish_consumed(` anywhere would be a
//!     publication path the basis discipline cannot see;
//! P10 (merged into P5/P8 by the loop-top unification): the seek and
//!     pause parks share ONE tail-probe arm since the unified gate, so
//!     one slice check covers both attributions;
//! P11 exactly ONE loop-top gate call exists in the whole mechanism —
//!     the frozen D14.5 realtime row ("no new lock acquisition")
//!     realized as a single unified inspection; a second gate call in
//!     the steady path is the defect the row forbids;
//! P12 the retired per-park gate calls (`park_while_paused` /
//!     `park_while_seek_hold`) appear nowhere — a reintroduction would
//!     silently restore the two-acquisition steady iteration.
//! ```
//!
//! This is a REGRESSION PIN, not a semantic proof: it says the shipped
//! text has the frozen call order, and it makes an accidental
//! reordering (the exact defect D14.8 names: "publish after the
//! increment overstates consumption by the new block") fail loudly and
//! locally. The negative controls below run the same checker against
//! deliberately mis-ordered bodies, so a checker that stopped checking
//! would fail them.

/// One violation of the frozen call order, named by its check.
fn check_render_order(source: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let mut check = |name: &str, ok: bool| {
        if !ok {
            violations.push(name.to_owned());
        }
    };

    // P4 is file-wide and independent of body extraction.
    check(
        "P4: exactly one handed-off credit in the mechanism",
        source.matches(CREDIT).count() == 1,
    );

    // P9 is file-wide too: exactly one direct cell publication, and it
    // lives inside the basis-aware helper (whose own `if publishing`
    // gate is the F5 withdrawal discipline, not a reordering hazard).
    let direct_publish_count = source.matches(DIRECT_PUBLISH).count();
    let helper_publish = body_of(source, PUBLISH_HELPER)
        .is_some_and(|body| code_sites(body, DIRECT_PUBLISH).len() == 1);
    check(
        "P9: exactly one direct cell publication, inside the basis-aware helper",
        direct_publish_count == 1 && helper_publish,
    );

    let Some(steady) = body_of(source, STEADY_LOOP) else {
        violations.push("P1..P3, P5: steady loop body not found".to_owned());
        return violations;
    };

    // The park slice publishes from its own padding read; the loop's own
    // publication sites are what P1/P2/P7 constrain. Checking the raw
    // body would let the park closure's copy satisfy them (its call is
    // the first in the text), so the closure's argument is removed first
    // and checked separately by P5/P8.
    let unparked = without_park_argument(steady);
    let publications = code_sites(&unparked, PUBLISH);
    let acquire = unparked.find(ACQUIRE);
    let credit = unparked.find(CREDIT);
    let submit_ok = unparked.find(SUBMIT_OK);
    let submit_fail = unparked.find(SUBMIT_FAIL);

    check(
        "P1: the steady loop publishes before device-buffer acquisition (shipped order)",
        matches!(publications.first(), Some(&p) if matches!(acquire, Some(a) if p < a)),
    );
    // Every publication site in the loop, not only the first: a second
    // one added after the credit pairs the same iteration's padding with
    // the post-submission total — the overstatement D14.8 names.
    check(
        "P2: every loop publication uses the pre-submission total",
        !publications.is_empty()
            && matches!(credit, Some(c) if publications.iter().all(|&p| p < c)),
    );
    check(
        "P3: the credit follows a successful submission and its failure break",
        matches!((submit_ok, submit_fail, credit), (Some(ok), Some(fail), Some(c)) if ok < c && fail < c),
    );
    // P5/P8 are checked over the tail-probe arm of the ONE gate
    // closure — since the loop-top unification that single arm serves
    // BOTH park attributions (pause and cut), so one slice check covers
    // both. The slice must publish, at its own reading, unconditionally:
    // quiescence is the one slice whose publication matters most (it is
    // what walks a parked episode's sample to the frozen total), so a
    // publication gated behind a condition — skipping exactly that
    // slice — must RED.
    let park_argument = argument_of(steady, PARK);
    let tail_arm = park_argument.and_then(|argument| arm_body(argument, TAIL_ARM, RELEASE_ARM));
    check(
        "P5: the park-slice observation publishes",
        tail_arm.is_some_and(|arm| !code_sites(arm, PUBLISH).is_empty()),
    );
    check(
        "P8: the park slice publishes unconditionally, at its arm's top level",
        tail_arm.is_some_and(|arm| {
            code_sites(arm, PUBLISH)
                .into_iter()
                .any(|at| depth_inside_closure(arm, at) == 0)
        }),
    );

    // P11/P12 are the C4 source oracle (implementation corrective-1):
    // the frozen realtime row's "no new lock acquisition" is realized
    // as ONE unified loop-top inspection.
    check(
        "P11: exactly one loop-top gate call in the mechanism",
        code_sites(source, PARK).len() == 1,
    );
    check(
        "P12: the retired per-park gate calls are gone",
        code_sites(source, RETIRED_PARK).is_empty()
            && code_sites(source, RETIRED_SEEK_PARK).is_empty(),
    );

    let Some(drain) = body_of(source, DRAIN) else {
        violations.push("P6: drain body not found".to_owned());
        return violations;
    };
    check(
        "P6: the EOF drain publishes and credits nothing",
        !code_sites(drain, PUBLISH).is_empty() && drain.find(CREDIT).is_none(),
    );

    violations
}

/// Every occurrence of `needle` in `text` that is a CODE site rather than
/// a mention: it must stand on its own line, so a call moved into a
/// comment (which would otherwise satisfy a text match without executing
/// anything) is not counted.
fn code_sites(text: &str, needle: &str) -> Vec<usize> {
    text.match_indices(needle)
        .map(|(at, _)| at)
        .filter(|&at| {
            let line_start = text[..at].rfind('\n').map_or(0, |nl| nl + 1);
            text[line_start..at].chars().all(char::is_whitespace)
        })
        .collect()
}

/// The `{`-nesting depth of `at` relative to the closure body it sits in:
/// 0 means the site is a statement of the closure's own block, 1 means it
/// is one block deeper (inside an `if`, a loop, …). A closure written
/// without a block (nothing but a call) has no nesting to speak of, so
/// its sites are top level by definition.
fn depth_inside_closure(argument: &str, at: usize) -> usize {
    let Some(open) = argument.find('{').filter(|&open| open < at) else {
        return 0;
    };
    brace_depth_at(&argument[open + 1..], at - open - 1)
}

/// The `{`-nesting depth at `at` within `text`, so a statement wrapped in
/// a conditional can be told from a top-level one.
fn brace_depth_at(text: &str, at: usize) -> usize {
    text[..at].matches('{').count() - text[..at].matches('}').count()
}

/// The steady loop's publication, as the oracle recognizes it: since F5
/// every publication routes through the basis-aware helper, so a
/// publication site is a helper CALL (its argument begins with the
/// position cell).
const PUBLISH: &str = "publish_consumed(position,";
/// The one direct cell publication (inside the helper).
const DIRECT_PUBLISH: &str = "position.publish_consumed(";
/// The basis-aware publication helper's signature.
const PUBLISH_HELPER: &str = "fn publish_consumed(";
/// The device-buffer acquisition.
const ACQUIRE: &str = "session.render.GetBuffer(";
/// The successful-submission call (ReleaseBuffer of a real block).
const SUBMIT_OK: &str = "session.render.ReleaseBuffer(n as u32, 0)";
/// The failed-submission guard's diagnostic: reaching the credit without
/// passing this text would mean the failure path can fall through to it.
const SUBMIT_FAIL: &str = "\"ReleaseBuffer failed:";
/// The one frame-accounting credit.
const CREDIT: &str = "handed_off += n as u64;";
/// THE one loop-top gate (D14.7 + D14.5 unified; implementation
/// corrective-1).
const PARK: &str = "gate.park_loop_top(";
/// The tail-probe arm of the gate closure (both park attributions).
const TAIL_ARM: &str = "GateSlice::TailProbe";
/// The seek-release arm of the gate closure.
const RELEASE_ARM: &str = "GateSlice::SeekRelease";
/// The retired per-park spellings (P12: their reintroduction restores
/// the two-acquisition steady iteration).
const RETIRED_PARK: &str = "gate.park_while_paused(";
const RETIRED_SEEK_PARK: &str = "gate.park_while_seek_hold(";
const STEADY_LOOP: &str = "fn steady_loop(";
const DRAIN: &str = "fn drain_to_zero(";

/// The body between the first `{` after `signature` and its matching `}`.
/// `None` when the function is absent or its braces do not balance.
fn body_of<'a>(source: &'a str, signature: &str) -> Option<&'a str> {
    let after_signature = source.find(signature)? + signature.len();
    let open = source[after_signature..].find('{')? + after_signature;
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[open..open + offset + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The parenthesized argument of the first `call(` in `text`.
fn argument_of<'a>(text: &'a str, call: &str) -> Option<&'a str> {
    let (start, end) = argument_span(text, call)?;
    Some(&text[start..end])
}

/// The byte range of the first `call(`'s parenthesized argument.
fn argument_span(text: &str, call: &str) -> Option<(usize, usize)> {
    let open = text.find(call)? + call.len();
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some((open, open + offset));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The body of the `head` match arm inside a gate-closure argument: the
/// text from the end of the arm's pattern to the start of the following
/// arm (or the end of the argument).
fn arm_body<'a>(argument: &'a str, head: &str, next: &str) -> Option<&'a str> {
    let start = argument.find(head)? + head.len();
    let end = argument.find(next).unwrap_or(argument.len()).max(start);
    Some(&argument[start..end])
}

/// `steady` with the gate closure's argument removed — i.e. the loop's
/// own body, without the park slice's copy of the publication (the
/// slice is checked separately by P5/P8).
fn without_park_argument(steady: &str) -> String {
    let mut unparked = steady.to_owned();
    if let Some((start, end)) = argument_span(&unparked, PARK) {
        unparked = format!("{}{}", &unparked[..start], &unparked[end..]);
    }
    unparked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The basis-aware publication helper exactly as the mechanism
    /// ships it, for the synthetic bodies below (P9 requires it).
    const HELPER: &str = r#"
fn publish_consumed(
    position: &PositionEvidence,
    basis: u64,
    handed_off: u64,
    tail: u64,
    publishing: bool,
) {
    if publishing {
        position.publish_consumed(basis + handed_off, tail);
    }
}
"#;

    /// The production mechanism itself: the shipped render loop has the
    /// frozen order. This is the regression pin — a reordering of
    /// `wasapi.rs` (publishing after the credit, crediting before the
    /// submission succeeds, dropping the park-slice or drain
    /// publication) fails here.
    #[test]
    fn the_production_render_loop_has_the_frozen_order() {
        let violations = check_render_order(include_str!("wasapi.rs"));
        assert!(
            violations.is_empty(),
            "the render leg's accounting order drifted: {violations:?}"
        );
    }

    /// The checker is not vacuous: a loop that publishes AFTER it
    /// acquires the device buffer — the shape that pairs a fresh
    /// padding reading with a block about to be submitted — is rejected.
    #[test]
    fn publishing_after_device_buffer_acquisition_is_rejected() {
        let violations = check_render_order(&format!(
            r#"
{HELPER}
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {{
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {{
        gate.park_loop_top(|gated| match gated {{
            GateSlice::TailProbe => {{
                let tail = 0;
                publish_consumed(position, basis, handed_off, tail, publishing);
                true
            }}
            GateSlice::SeekRelease(_) => false,
        }});
        let padding = unsafe {{ session.client.GetCurrentPadding() }}.unwrap_or(0);
        let ptr = unsafe {{ session.render.GetBuffer(available as u32) }};
        publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
        let dst = unsafe {{ slice::from_raw_parts_mut(ptr as *mut f32, available) }};
        match render_input.read_frames(dst) {{
            PcmPull::Frames(n) => {{
                if let Err(e) = unsafe {{ session.render.ReleaseBuffer(n as u32, 0) }} {{
                    break abort_msg(format!("ReleaseBuffer failed: {{e}}"));
                }}
                handed_off += n as u64;
            }}
        }}
    }}
}}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {{
    publish_consumed(position, 0, handed_off, 0, true);
    LoopOutcome::Drained
}}
"#
        ));
        assert!(
            violations.iter().any(|v| v.starts_with("P1")),
            "the order check must reject a publish after GetBuffer: {violations:?}"
        );
    }

    /// The named defect of D14.8: the loop's publication paired with the
    /// post-submission total overstates consumption by the block just
    /// submitted.
    #[test]
    fn publishing_from_the_post_submission_total_is_rejected() {
        let violations = check_render_order(&format!(
            r#"
{HELPER}
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {{
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {{
        gate.park_loop_top(|gated| match gated {{
            GateSlice::TailProbe => {{
                publish_consumed(position, basis, handed_off, 0, publishing);
                true
            }}
            GateSlice::SeekRelease(_) => false,
        }});
        let padding = unsafe {{ session.client.GetCurrentPadding() }}.unwrap_or(0);
        let ptr = unsafe {{ session.render.GetBuffer(available as u32) }};
        let dst = unsafe {{ slice::from_raw_parts_mut(ptr as *mut f32, available) }};
        match render_input.read_frames(dst) {{
            PcmPull::Frames(n) => {{
                if let Err(e) = unsafe {{ session.render.ReleaseBuffer(n as u32, 0) }} {{
                    break abort_msg(format!("ReleaseBuffer failed: {{e}}"));
                }}
                handed_off += n as u64;
                publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
            }}
        }}
    }}
}}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {{
    publish_consumed(position, 0, handed_off, 0, true);
    LoopOutcome::Drained
}}
"#
        ));
        assert!(
            violations.iter().any(|v| v.starts_with("P2")),
            "the loop's publication must be checked, not the park slice's \
             copy: {violations:?}"
        );
    }

    /// The credit must not appear before the submission succeeded: a
    /// credit placed above `ReleaseBuffer` counts a block the device
    /// never accepted.
    #[test]
    fn crediting_before_a_successful_submission_is_rejected() {
        let violations = check_render_order(&format!(
            r#"
{HELPER}
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {{
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {{
        gate.park_loop_top(|gated| match gated {{
            GateSlice::TailProbe => {{
                publish_consumed(position, basis, handed_off, 0, publishing);
                true
            }}
            GateSlice::SeekRelease(_) => false,
        }});
        let padding = unsafe {{ session.client.GetCurrentPadding() }}.unwrap_or(0);
        publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
        let ptr = unsafe {{ session.render.GetBuffer(available as u32) }};
        let dst = unsafe {{ slice::from_raw_parts_mut(ptr as *mut f32, available) }};
        match render_input.read_frames(dst) {{
            PcmPull::Frames(n) => {{
                handed_off += n as u64;
                if let Err(e) = unsafe {{ session.render.ReleaseBuffer(n as u32, 0) }} {{
                    break abort_msg(format!("ReleaseBuffer failed: {{e}}"));
                }}
            }}
        }}
    }}
}}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {{
    publish_consumed(position, 0, handed_off, 0, true);
    LoopOutcome::Drained
}}
"#
        ));
        assert!(
            violations.iter().any(|v| v.starts_with("P3")),
            "a credit above the submission must be rejected: {violations:?}"
        );
    }

    /// A second credit — e.g. one added to the drain path or to an
    /// abort arm — is rejected file-wide.
    #[test]
    fn a_second_credit_site_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            "fn drain_to_zero(",
            "fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {\n    handed_off += n as u64;",
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P4")),
            "an extra credit site must be rejected: {violations:?}"
        );
    }

    /// The park slice is where a paused episode's sample walks up to the
    /// frozen handed-off total: a park closure that observes the tail
    /// without publishing breaks that.
    #[test]
    fn a_park_slice_that_does_not_publish_is_rejected() {
        let violations = check_render_order(&format!(
            r#"
{HELPER}
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {{
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {{
        gate.park_loop_top(|gated| match gated {{
            GateSlice::TailProbe => unsafe {{ session.client.GetCurrentPadding() }}
                .is_ok_and(|p| p == 0),
            GateSlice::SeekRelease(_) => false,
        }});
        let padding = unsafe {{ session.client.GetCurrentPadding() }}.unwrap_or(0);
        publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
        let ptr = unsafe {{ session.render.GetBuffer(available as u32) }};
        let dst = unsafe {{ slice::from_raw_parts_mut(ptr as *mut f32, available) }};
        match render_input.read_frames(dst) {{
            PcmPull::Frames(n) => {{
                if let Err(e) = unsafe {{ session.render.ReleaseBuffer(n as u32, 0) }} {{
                    break abort_msg(format!("ReleaseBuffer failed: {{e}}"));
                }}
                handed_off += n as u64;
            }}
        }}
    }}
}}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {{
    publish_consumed(position, 0, handed_off, 0, true);
    LoopOutcome::Drained
}}
"#
        ));
        assert!(
            violations.iter().any(|v| v.starts_with("P5")),
            "a park slice that never publishes must be rejected: {violations:?}"
        );
    }

    /// The drain path: it must keep publishing (the Sample rises to the
    /// exact handed-off total) and must never credit frames (it submits
    /// none).
    #[test]
    fn a_drain_path_that_stops_publishing_is_rejected() {
        let violations = check_render_order(&format!(
            r#"
{HELPER}
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {{
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {{
        gate.park_loop_top(|gated| match gated {{
            GateSlice::TailProbe => {{
                publish_consumed(position, basis, handed_off, 0, publishing);
                true
            }}
            GateSlice::SeekRelease(_) => false,
        }});
        let padding = unsafe {{ session.client.GetCurrentPadding() }}.unwrap_or(0);
        publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
        let ptr = unsafe {{ session.render.GetBuffer(available as u32) }};
        let dst = unsafe {{ slice::from_raw_parts_mut(ptr as *mut f32, available) }};
        match render_input.read_frames(dst) {{
            PcmPull::Frames(n) => {{
                if let Err(e) = unsafe {{ session.render.ReleaseBuffer(n as u32, 0) }} {{
                    break abort_msg(format!("ReleaseBuffer failed: {{e}}"));
                }}
                handed_off += n as u64;
            }}
        }}
    }}
}}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {{
    LoopOutcome::Drained
}}
"#
        ));
        assert!(
            violations.iter().any(|v| v.starts_with("P6")),
            "a silent drain must be rejected: {violations:?}"
        );
    }

    /// A SECOND publication added after the credit — the shape a
    /// refactor could introduce while keeping the first one intact —
    /// pairs the same iteration's padding with the post-submission
    /// total. The first-site-only version of this check let it through;
    /// every site is constrained now.
    #[test]
    fn a_second_publication_after_the_credit_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            CREDIT,
            &format!(
                "{CREDIT}\n                publish_consumed(position, basis, handed_off, u64::from(padding), publishing);"
            ),
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P2")),
            "a publication after the credit must be rejected, wherever it \
             sits relative to the first one: {violations:?}"
        );
    }

    /// A pause park slice that publishes only when the tail is NOT
    /// quiesced skips exactly the observation that matters most:
    /// quiescence is where a paused episode's sample is walked up to the
    /// frozen total. The mutation anchors on the pause park's call site
    /// (rustfmt renders both park closures at the same indent, so the
    /// indentation is not a discriminator — the call marker is) and
    /// wraps the closure's publication in the exact condition that
    /// skips the quiescent slice.
    #[test]
    fn a_park_slice_that_skips_its_quiescent_publication_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            "GateSlice::TailProbe => {\n                let Ok(padding) = (unsafe { session.client.GetCurrentPadding() }) else {\n                    return false;\n                };\n                publish_consumed(position, basis, handed_off, u64::from(padding), publishing);",
            "GateSlice::TailProbe => {\n                let Ok(padding) = (unsafe { session.client.GetCurrentPadding() }) else {\n                    return false;\n                };\n                if padding != 0 {\n                    publish_consumed(position, basis, handed_off, u64::from(padding), publishing);\n                }",
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P8")),
            "a conditional park publication must be rejected: {violations:?}"
        );
        assert!(
            !violations.iter().any(|v| v.starts_with("P5")),
            "the mutation must hide the publication, not remove it: {violations:?}"
        );
    }

    /// P11 is the C4 source oracle: the frozen realtime row's "no new
    /// lock acquisition" is realized as ONE unified loop-top inspection,
    /// so a second gate call in the steady path — the pre-unification
    /// two-call spelling, or any added check — must RED.
    #[test]
    fn a_second_loop_top_gate_call_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            "        // Period cadence; the bounded wait is also the stop-latency bound.",
            "        gate.park_loop_top(|gated| match gated {\n            GateSlice::TailProbe => true,\n            GateSlice::SeekRelease(_) => false,\n });\n        // Period cadence; the bounded wait is also the stop-latency bound.",
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P11")),
            "a second loop-top gate call must be rejected: {violations:?}"
        );
    }

    /// P12: reintroducing the retired per-park spelling — e.g. a merge
    /// that drags a pre-unification hunk back in — must RED, because it
    /// silently restores the two-acquisition steady iteration.
    #[test]
    fn a_reintroduced_retired_park_call_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            "        // Period cadence; the bounded wait is also the stop-latency bound.",
            "        // gate.park_while_paused(|| false);\n        // Period cadence; the bounded wait is also the stop-latency bound.",
        );
        let violations = check_render_order(&source);
        assert!(
            !violations.iter().any(|v| v.starts_with("P12")),
            "a commented mention is not a call (P7's rule): {violations:?}"
        );
        let source = include_str!("wasapi.rs").replace(
            "        // Period cadence; the bounded wait is also the stop-latency bound.",
            "        gate.park_while_paused(|| false);\n        // Period cadence; the bounded wait is also the stop-latency bound.",
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P12")),
            "a reintroduced retired park call must be rejected: {violations:?}"
        );
    }

    /// A comment that merely mentions the call is not a call. The
    /// text-matching version of this check counted it, so deleting the
    /// publication degraded into renaming it.
    #[test]
    fn a_publication_moved_into_a_comment_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            "pins this.\n        publish_consumed(position, basis, handed_off, u64::from(padding), publishing);",
            "pins this.\n        // publish_consumed(position, basis, handed_off, u64::from(padding), publishing);",
        );
        let violations = check_render_order(&source);
        assert!(
            violations
                .iter()
                .any(|v| v.starts_with("P1") || v.starts_with("P2")),
            "a commented-out publication is not a publication: {violations:?}"
        );
    }

    /// A site that bypasses the basis-aware helper — writing the
    /// stretch arithmetic (or a bare total) inline — reintroduces the
    /// dual-accounting F5 retired: P9 pins the helper as the single
    /// implementation of the publication.
    #[test]
    fn a_publication_that_bypasses_the_helper_is_rejected() {
        let violations = check_render_order(&format!(
            r#"
{HELPER}
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {{
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {{
        gate.park_loop_top(|gated| match gated {{
            GateSlice::TailProbe => {{
                publish_consumed(position, basis, handed_off, 0, publishing);
                true
            }}
            GateSlice::SeekRelease(_) => false,
        }});
        let padding = unsafe {{ session.client.GetCurrentPadding() }}.unwrap_or(0);
        position.publish_consumed(basis + handed_off, u64::from(padding));
        let ptr = unsafe {{ session.render.GetBuffer(available as u32) }};
        let dst = unsafe {{ slice::from_raw_parts_mut(ptr as *mut f32, available) }};
        match render_input.read_frames(dst) {{
            PcmPull::Frames(n) => {{
                if let Err(e) = unsafe {{ session.render.ReleaseBuffer(n as u32, 0) }} {{
                    break abort_msg(format!("ReleaseBuffer failed: {{e}}"));
                }}
                handed_off += n as u64;
            }}
        }}
    }}
}}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {{
    publish_consumed(position, 0, handed_off, 0, true);
    LoopOutcome::Drained
}}
"#
        ));
        assert!(
            violations.iter().any(|v| v.starts_with("P9")),
            "a publication outside the helper must be rejected: {violations:?}"
        );
    }

    /// The value-level negative control for the hazard itself: if the
    /// publication were paired with a total that already includes the
    /// new block, the published sample would claim consumed frames the
    /// device has not been given yet.
    #[test]
    fn the_mis_paired_total_overstates_the_consumed_sample_by_one_block() {
        use qianqian_audio_api::ports::PositionEvidence;

        let block = 1024u64;
        let cell = PositionEvidence::new();
        cell.publish_consumed(8 * block, 0); // eight blocks submitted, none queued
        let honest = cell.published().expect("published");

        // The wrong pairing: the same instant's padding (an empty queue)
        // read against the total as it will stand AFTER this iteration's
        // submission.
        let mis_paired_total = 9 * block;
        let tail_at_the_same_instant = 0u64;
        let mis_paired = mis_paired_total - tail_at_the_same_instant.min(mis_paired_total);
        assert_eq!(honest, 8 * block);
        assert_eq!(
            mis_paired - honest,
            block,
            "publishing against the post-submission total claims one whole \
             block the device has not been given"
        );
    }
}
