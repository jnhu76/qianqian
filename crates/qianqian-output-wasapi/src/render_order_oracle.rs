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
//!     exactly that slice, must RED.
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
    // P8 is checked over the park argument as a whole: its own sites,
    // its own nesting.
    let park_argument = argument_of(steady, PARK);
    check(
        "P5: the park-slice observation publishes",
        park_argument.is_some_and(|argument| !code_sites(argument, PUBLISH).is_empty()),
    );
    // The park slice must publish at ITS OWN reading, unconditionally:
    // quiescence is the one slice whose publication matters most (it is
    // what walks a paused episode's sample to the frozen total), so a
    // publication gated behind a condition — skipping exactly that
    // slice — must RED.
    check(
        "P8: the park slice publishes unconditionally, at its closure's top level",
        park_argument.is_some_and(|argument| {
            code_sites(argument, PUBLISH)
                .into_iter()
                .any(|at| depth_inside_closure(argument, at) == 0)
        }),
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

/// The steady loop's publication, as the oracle recognizes it.
const PUBLISH: &str = "position.publish_consumed(";
/// The device-buffer acquisition.
const ACQUIRE: &str = "session.render.GetBuffer(";
/// The successful-submission call (ReleaseBuffer of a real block).
const SUBMIT_OK: &str = "session.render.ReleaseBuffer(n as u32, 0)";
/// The failed-submission guard's diagnostic: reaching the credit without
/// passing this text would mean the failure path can fall through to it.
const SUBMIT_FAIL: &str = "\"ReleaseBuffer failed:";
/// The one frame-accounting credit.
const CREDIT: &str = "handed_off += n as u64;";
/// The loop-top pause gate.
const PARK: &str = "park_while_paused(";
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

/// `steady` with the pause gate's argument removed — i.e. the loop's own
/// body, without the park slice's copy of the publication.
fn without_park_argument(steady: &str) -> String {
    match argument_span(steady, PARK) {
        Some((start, end)) => format!("{}{}", &steady[..start], &steady[end..]),
        None => steady.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let violations = check_render_order(
            r#"
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {
    let mut handed_off: u64 = 0;
    loop {
        let padding = unsafe { session.client.GetCurrentPadding() }.unwrap_or(0);
        let ptr = unsafe { session.render.GetBuffer(available as u32) };
        position.publish_consumed(handed_off, u64::from(padding));
        let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available) };
        match render_input.read_frames(dst) {
            PcmPull::Frames(n) => {
                if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                    break abort_msg(format!("ReleaseBuffer failed: {e}"));
                }
                handed_off += n as u64;
            }
        }
    }
}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {
    position.publish_consumed(handed_off, 0);
    LoopOutcome::Drained
}
"#,
        );
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
        let violations = check_render_order(
            r#"
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {
    let mut handed_off: u64 = 0;
    loop {
        gate.park_while_paused(|| { position.publish_consumed(handed_off, 0); true });
        let padding = unsafe { session.client.GetCurrentPadding() }.unwrap_or(0);
        let ptr = unsafe { session.render.GetBuffer(available as u32) };
        let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available) };
        match render_input.read_frames(dst) {
            PcmPull::Frames(n) => {
                if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                    break abort_msg(format!("ReleaseBuffer failed: {e}"));
                }
                handed_off += n as u64;
                position.publish_consumed(handed_off, u64::from(padding));
            }
        }
    }
}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {
    position.publish_consumed(handed_off, 0);
    LoopOutcome::Drained
}
"#,
        );
        assert!(
            violations.iter().any(|v| v.starts_with("P2")),
            "the loop's publication must be checked, not the park slice's \
             copy: {violations:?}"
        );
        assert!(
            violations.iter().any(|v| v.starts_with("P1")),
            "and it is also past the acquisition: {violations:?}"
        );
    }

    /// The credit must not appear before the submission succeeded: a
    /// credit placed above `ReleaseBuffer` counts a block the device
    /// never accepted.
    #[test]
    fn crediting_before_a_successful_submission_is_rejected() {
        let violations = check_render_order(
            r#"
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {
    let mut handed_off: u64 = 0;
    loop {
        gate.park_while_paused(|| { position.publish_consumed(handed_off, 0); true });
        let padding = unsafe { session.client.GetCurrentPadding() }.unwrap_or(0);
        position.publish_consumed(handed_off, u64::from(padding));
        let ptr = unsafe { session.render.GetBuffer(available as u32) };
        let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available) };
        match render_input.read_frames(dst) {
            PcmPull::Frames(n) => {
                handed_off += n as u64;
                if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                    break abort_msg(format!("ReleaseBuffer failed: {e}"));
                }
            }
        }
    }
}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {
    position.publish_consumed(handed_off, 0);
    LoopOutcome::Drained
}
"#,
        );
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
        let violations = check_render_order(
            r#"
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {
    let mut handed_off: u64 = 0;
    loop {
        gate.park_while_paused(|| unsafe { session.client.GetCurrentPadding() }.is_ok_and(|p| p == 0));
        let padding = unsafe { session.client.GetCurrentPadding() }.unwrap_or(0);
        position.publish_consumed(handed_off, u64::from(padding));
        let ptr = unsafe { session.render.GetBuffer(available as u32) };
        let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available) };
        match render_input.read_frames(dst) {
            PcmPull::Frames(n) => {
                if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                    break abort_msg(format!("ReleaseBuffer failed: {e}"));
                }
                handed_off += n as u64;
            }
        }
    }
}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {
    position.publish_consumed(handed_off, 0);
    LoopOutcome::Drained
}
"#,
        );
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
        let violations = check_render_order(
            r#"
fn steady_loop(session: &DeviceSession, position: &PositionEvidence) -> LoopOutcome {
    let mut handed_off: u64 = 0;
    loop {
        gate.park_while_paused(|| { position.publish_consumed(handed_off, 0); true });
        let padding = unsafe { session.client.GetCurrentPadding() }.unwrap_or(0);
        position.publish_consumed(handed_off, u64::from(padding));
        let ptr = unsafe { session.render.GetBuffer(available as u32) };
        let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available) };
        match render_input.read_frames(dst) {
            PcmPull::Frames(n) => {
                if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                    break abort_msg(format!("ReleaseBuffer failed: {e}"));
                }
                handed_off += n as u64;
            }
        }
    }
}

fn drain_to_zero(session: &DeviceSession, position: &PositionEvidence, handed_off: u64) -> LoopOutcome {
    LoopOutcome::Drained
}
"#,
        );
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
            &format!("{CREDIT}\n        {PUBLISH}handed_off, u64::from(padding));"),
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P2")),
            "a publication after the credit must be rejected, wherever it \
             sits relative to the first one: {violations:?}"
        );
    }

    /// A park slice that publishes only when the tail is NOT quiesced
    /// skips exactly the observation that matters most: quiescence is
    /// where a paused episode's sample is walked up to the frozen total,
    /// and `paused()` establishes at the same instant.
    #[test]
    fn a_park_slice_that_skips_its_quiescent_publication_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            &format!("            {PUBLISH}handed_off, u64::from(padding));\n"),
            &format!(
                "            if padding != 0 {{\n                {PUBLISH}handed_off, u64::from(padding));\n            }}\n"
            ),
        );
        let violations = check_render_order(&source);
        assert!(
            violations.iter().any(|v| v.starts_with("P8")),
            "a conditional park publication must be rejected: {violations:?}"
        );
    }

    /// A comment that merely mentions the call is not a call. The
    /// text-matching version of this check counted it, so deleting the
    /// publication degraded into renaming it.
    #[test]
    fn a_publication_moved_into_a_comment_is_rejected() {
        let source = include_str!("wasapi.rs").replace(
            &format!("        {PUBLISH}handed_off, u64::from(padding));\n"),
            &format!("        // {PUBLISH}handed_off, u64::from(padding));\n"),
        );
        let violations = check_render_order(&source);
        assert!(
            violations
                .iter()
                .any(|v| v.starts_with("P1") || v.starts_with("P2")),
            "a commented-out publication is not a publication: {violations:?}"
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
