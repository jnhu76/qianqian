//! Deterministic playback temporal traces (executable oracle for ADR-PBK-001).
//!
//! These tests drive the pure, mechanism-independent temporal core through
//! the trace families the accepted ADR and `specs/playback` identify as
//! high-risk. They use stable semantic names; internal representation may
//! change freely as long as these public-interface traces keep passing.
//!
//! Test vocabulary comes from ADR-PBK-001: TransportKernel is the playback
//! temporal authority (windows, generations, admission, fence, raw evidence
//! interpretation); MusicKernel interprets derived facts into product
//! semantics. Fakes do not exist here: the test itself plays the mechanism
//! (decoder/device) by feeding raw evidence, and interpretation stays in the
//! kernels.

use qianqian_core::music::{MusicKernel, PlaybackState};
use qianqian_core::transport::{
    DecodeSessionId, EofOutcome, FenceState, FenceVerdictOutcome, GenerationId, MediaId, MediaSpan,
    TransportKernel, WindowRole,
};

/// A track counter giving each opened media a distinct identity.
struct Media {
    next: u64,
}

impl Media {
    fn new() -> Self {
        Self { next: 0 }
    }

    fn open(&mut self) -> MediaId {
        let id = MediaId::new(self.next);
        self.next += 1;
        id
    }
}

#[test]
fn active_only_episode_admits_decodes_submissions_and_renders() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    // Play opens a fresh episode: one active window on a fresh generation,
    // one track session owning one decode session.
    let started = transport.play(media.open()).expect("first play starts");
    let gen_id = started.generation;

    let snapshot = transport.snapshot();
    let active = snapshot.active.expect("active window after play");
    assert_eq!(active.generation, gen_id);
    assert!(active.admission_open, "fresh active window is admitted");
    assert!(!active.ready, "active window is not a readiness concept");
    assert_eq!(snapshot.prepared, None);
    assert_eq!(snapshot.track_sessions.len(), 1);

    // Decode evidence is admitted while the owning window's admission is
    // open; the session cursor advances to the span end.
    let admitted = transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: gen_id,
                start: 0,
                end: 720,
            },
        )
        .expect("decode admitted while active admission open");
    assert_eq!(admitted.advanced_to, 720);

    // Only the active generation may submit media to the output path.
    transport
        .media_submitted(gen_id, 720)
        .expect("active admitted generation may submit");

    // Render evidence consumes submitted media; rendered stays behind
    // submitted while media is in flight.
    transport
        .media_rendered(gen_id, 300)
        .expect("rendered submitted media");

    let snapshot = transport.snapshot();
    let active = snapshot.active.expect("active window");
    assert_eq!(active.decode_position, 720);
    assert_eq!(active.accepted_frames, 720);
    assert_eq!(active.submitted_frames, 720);
    assert_eq!(active.rendered_frames, 300);
    assert_eq!(active.queued_frames, 420);
    assert!(
        !snapshot.transport_drained,
        "in-flight media is not drained"
    );
}

#[test]
fn active_and_prepared_windows_coexist_on_distinct_generations() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let prepared = transport.seek(1000).expect("seek prepares a cut");

    // Dual window: both roles live at once on different generations.
    assert_ne!(started.generation, prepared.generation);
    let snapshot = transport.snapshot();
    assert_eq!(
        snapshot.active.expect("active kept").generation,
        started.generation
    );
    assert_eq!(
        snapshot.prepared.expect("prepared exists").generation,
        prepared.generation
    );

    // Both generations hold open admission: each window role admits its own
    // decode evidence (generation is window-scoped, not a global current).
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 480,
            },
        )
        .expect("active generation still admitted");
    transport
        .decode_result(
            prepared.decode_session,
            MediaSpan {
                generation: prepared.generation,
                start: 1000,
                end: 1480,
            },
        )
        .expect("prepared generation admitted while priming");
}

#[test]
fn same_track_seek_keeps_two_decode_sessions_under_one_track_session() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 720,
            },
        )
        .expect("old cursor decodes to 72s-equivalent position");

    // Seek within the same track: the TrackSession stays the media identity
    // root and now owns two independently advancing decode cursors.
    let sought = transport.seek(1000).expect("same-track seek prepares");
    assert_eq!(sought.track, started.track, "seek keeps the track session");

    let snapshot = transport.snapshot();
    assert_eq!(snapshot.track_sessions.len(), 1);
    let track = &snapshot.track_sessions[0];
    assert_eq!(track.decode_sessions.len(), 2);

    let old = &track.decode_sessions[0];
    assert_eq!(old.id, started.decode_session);
    assert_eq!(old.generation, started.generation);
    assert_eq!(old.role, Some(WindowRole::Active));
    assert_eq!(old.decode_position, 720);

    let new = &track.decode_sessions[1];
    assert_eq!(new.id, sought.decode_session);
    assert_eq!(new.generation, sought.generation);
    assert_eq!(new.role, Some(WindowRole::Prepared));
    assert_eq!(
        new.decode_position, 1000,
        "prepared cursor starts at seek target"
    );
}

#[test]
fn next_track_prepares_under_a_second_track_session() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let first = transport.play(media.open()).expect("play starts");
    let second = transport
        .next_track(media.open())
        .expect("next prepares a replacement track");

    // Track replacement: two track sessions exist during preparation; the
    // prepared window belongs to the new track's decode session.
    assert_ne!(first.track, second.track);
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.track_sessions.len(), 2);
    let active = snapshot.active.expect("old track still active");
    let prepared = snapshot.prepared.expect("new track prepared");
    assert_eq!(active.track, first.track);
    assert_eq!(prepared.track, second.track);
    assert_eq!(prepared.decode_session, second.decode_session);
    assert_eq!(
        prepared.decode_position, 0,
        "new track cursor starts at zero"
    );
}

#[test]
fn late_old_generation_decode_rejected_after_promotion() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    promote(&mut transport);

    // The old generation no longer holds a window role: its late decode
    // result is stale ("not admitted by the owning temporal role"), not
    // merely unequal to some global current generation.
    let late = transport.decode_result(
        started.decode_session,
        MediaSpan {
            generation: started.generation,
            start: 720,
            end: 780,
        },
    );
    assert!(
        late.is_err(),
        "retired generation decode evidence must be rejected"
    );

    // The promoted generation is now the output authority and may submit.
    transport
        .media_submitted(sought.generation, 480)
        .expect("promoted generation may submit");
}

#[test]
fn retired_track_session_releases_after_track_replacement_promotion() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let first = transport.play(media.open()).expect("play starts");
    let second = transport.next_track(media.open()).expect("next prepares");
    prime(&mut transport, second.decode_session, second.generation, 0);
    promote(&mut transport);

    // Old TrackSession released once its ownership subtree fully drained;
    // the replacement track session remains with its promoted session.
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.track_sessions.len(), 1);
    let remaining = &snapshot.track_sessions[0];
    assert_eq!(remaining.id, second.track);
    assert_ne!(remaining.id, first.track);
}

#[test]
fn prepared_generation_primes_while_active_remains_output_authority() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 100,
            },
        )
        .expect("decode evidence backs the submission");
    transport
        .media_submitted(started.generation, 100)
        .expect("active generation submits");

    let prepared = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        prepared.decode_session,
        prepared.generation,
        1000,
    );

    let snapshot = transport.snapshot();
    assert!(
        snapshot.prepared.expect("prepared").ready,
        "primed prepared window"
    );
    assert!(
        snapshot.active.expect("active kept").admission_open,
        "active admission still open while prepared primes"
    );

    // Prepared is not the physical-output authority: it cannot submit or
    // render media before promotion.
    assert!(
        transport.media_submitted(prepared.generation, 10).is_err(),
        "prepared generation must not submit media"
    );
    assert!(
        transport.media_rendered(prepared.generation, 10).is_err(),
        "prepared generation has nothing renderable"
    );
    transport
        .media_rendered(started.generation, 100)
        .expect("only the active generation's media renders");
}

#[test]
fn promotion_requires_a_successful_fence_verdict() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );

    let cut = transport
        .begin_hard_cut()
        .expect("cut begins after priming");
    assert_eq!(cut.cut, started.generation);
    assert_eq!(cut.target, sought.generation);

    // Between cut begin and verdict nothing promotes: logical
    // invalidation alone is not physical stop and not promotion.
    let mid = transport.snapshot();
    assert_eq!(
        mid.active
            .expect("old active survives until verdict")
            .generation,
        started.generation
    );
    assert!(
        !mid.active.expect("active").admission_open,
        "cut closed old admission"
    );
    assert_eq!(
        mid.prepared
            .expect("prepared survives until verdict")
            .generation,
        sought.generation
    );
    assert_eq!(
        mid.fence,
        FenceState::Requested {
            cut: started.generation
        },
        "physical fence handshake requested"
    );

    transport
        .claim_fence()
        .expect("device claims the transaction");
    let snapshot = transport.snapshot();
    assert_eq!(
        snapshot.fence,
        FenceState::Claimed {
            cut: started.generation
        }
    );

    let verdict = transport.fence_succeeded().expect("successful verdict");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::Promoted {
            promoted: sought.generation,
            cut: started.generation,
        }
    );

    let snapshot = transport.snapshot();
    assert_eq!(snapshot.fence, FenceState::Idle);
    let active = snapshot.active.expect("promoted window becomes active");
    assert_eq!(active.generation, sought.generation);
    assert!(active.admission_open, "promoted generation admitted");
    assert_eq!(
        snapshot.prepared, None,
        "prepared slot cleared on promotion"
    );
}

#[test]
fn fence_failure_cannot_fake_promotion_and_can_retry() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    transport.begin_hard_cut().expect("cut begins");
    transport.claim_fence().expect("device claims");

    transport.fence_failed().expect("failure recorded");
    let failed = transport.snapshot();
    assert_eq!(
        failed.active.expect("no promotion on failure").generation,
        started.generation
    );
    assert_eq!(
        failed.prepared.expect("prepared kept").generation,
        sought.generation
    );
    assert_eq!(
        failed.fence,
        FenceState::Failed {
            cut: started.generation
        }
    );

    // Retry the same physical transaction; success then promotes.
    transport
        .retry_fence()
        .expect("retry re-requests the flush");
    transport.claim_fence().expect("device claims again");
    let verdict = transport.fence_succeeded().expect("retry succeeds");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::Promoted {
            promoted: sought.generation,
            cut: started.generation,
        }
    );
}

#[test]
fn abandoned_fence_fails_closed_without_promotion() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    transport.begin_hard_cut().expect("cut begins");
    transport.claim_fence().expect("device claims");
    transport.fence_failed().expect("failure recorded");

    transport.abandon_fence().expect("fail closed");
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.fence, FenceState::Idle);
    assert_eq!(
        snapshot
            .active
            .expect("active kept, admission closed")
            .generation,
        started.generation
    );
    assert!(!snapshot.active.expect("active").admission_open);
    assert_eq!(
        snapshot
            .prepared
            .expect("prepared kept for a later episode")
            .generation,
        sought.generation
    );
}

/// Prime a prepared window: one admitted decode result marks readiness.
fn prime(
    transport: &mut TransportKernel,
    session: DecodeSessionId,
    generation: GenerationId,
    from: u64,
) {
    transport
        .decode_result(
            session,
            MediaSpan {
                generation,
                start: from,
                end: from + 480,
            },
        )
        .expect("priming decode admitted");
}

/// Drive one full promotion episode: cut, claim, successful verdict.
fn promote(transport: &mut TransportKernel) {
    transport.begin_hard_cut().expect("cut begins");
    transport.claim_fence().expect("device claims");
    let verdict = transport.fence_succeeded().expect("verdict succeeds");
    assert!(
        matches!(verdict, FenceVerdictOutcome::Promoted { .. }),
        "helper expects a promotion, got {verdict:?}"
    );
}

#[test]
fn submitted_media_outruns_rendered_without_draining_or_ending() {
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode ten frames");
    transport
        .media_submitted(started.generation, 10)
        .expect("submit ten frames");
    transport
        .media_rendered(started.generation, 7)
        .expect("render seven frames");

    // submitted != rendered: three frames are still in flight, so the
    // transport is not drained and the product has not ENDED even though
    // the decoder already produced everything.
    transport
        .decoder_eof(started.decode_session)
        .expect("producer terminal recorded");
    let snapshot = transport.snapshot();
    let active = snapshot.active.expect("active window");
    assert_eq!(active.submitted_frames, 10);
    assert_eq!(active.rendered_frames, 7);
    assert_eq!(active.queued_frames, 3);
    assert!(
        !snapshot.transport_drained,
        "submitted-but-unrendered media blocks drain"
    );

    music.observe_all(transport.take_derived_facts());
    assert_ne!(
        music.state(),
        PlaybackState::Ended,
        "no ENDED before render drain"
    );
}

#[test]
fn decoder_eof_alone_never_ends_and_full_drain_then_ends() {
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode ten frames");
    transport
        .media_submitted(started.generation, 10)
        .expect("submit ten frames");

    // Producer terminal with media still queued: decoder EOF is not
    // transport drained and not ENDED.
    transport
        .decoder_eof(started.decode_session)
        .expect("producer terminal");
    music.observe_all(transport.take_derived_facts());
    assert!(!transport.transport_drained());
    assert_ne!(music.state(), PlaybackState::Ended);

    // Rendering the remainder drains the transport; MusicKernel interprets
    // the derived drained fact as ENDED; the episode then completes and a
    // new episode can start.
    transport
        .media_rendered(started.generation, 10)
        .expect("render the remainder");
    assert!(
        transport.transport_drained(),
        "no in-flight media and producer terminal"
    );

    music.observe_all(transport.take_derived_facts());
    assert_eq!(music.state(), PlaybackState::Ended);

    transport
        .complete_ended_episode()
        .expect("episode completes after ENDED");
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.active, None, "ended episode retires its window");
    assert!(snapshot.track_sessions.is_empty(), "drained track released");

    transport
        .play(media.open())
        .expect("new episode after ENDED");
}

#[test]
fn natural_drain_during_stop_fence_does_not_ended_or_destroy_the_cut() {
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode ten frames");
    transport
        .media_submitted(started.generation, 10)
        .expect("submit ten frames");
    transport
        .media_rendered(started.generation, 7)
        .expect("render seven frames");

    // Stop races natural end: the fence goes in flight while EOF/drain
    // evidence is arriving.
    let cut = transport.stop().expect("stop requests the terminal fence");
    assert_eq!(cut, started.generation);
    transport
        .claim_fence()
        .expect("device claims the stop transaction");

    transport
        .decoder_eof(started.decode_session)
        .expect("EOF recorded during fence");
    transport
        .media_rendered(started.generation, 3)
        .expect("final renders land during fence");

    // The in-flight fence keeps the physical state undecided: no drained
    // truth, no ENDED, and the active temporal state the fence still
    // needs survives until the verdict.
    assert!(!transport.transport_drained());
    music.observe_all(transport.take_derived_facts());
    assert_ne!(music.state(), PlaybackState::Ended);
    let snapshot = transport.snapshot();
    assert_eq!(
        snapshot
            .active
            .expect("active window survives for the fence")
            .generation,
        started.generation
    );

    let verdict = transport.fence_succeeded().expect("verdict completes");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::StopCompleted {
            cut: started.generation
        }
    );

    music.observe_all(transport.take_derived_facts());
    // Stopped is a product interpretation distinct from ENDED.
    assert_eq!(music.state(), PlaybackState::Idle);
    assert_ne!(music.state(), PlaybackState::Ended);
    assert!(transport.transport_drained());
    assert_eq!(transport.snapshot().active, None);
}

#[test]
fn stop_after_drained_withdraws_the_pending_drained_fact() {
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode");
    transport
        .media_submitted(started.generation, 10)
        .expect("submit");
    transport
        .media_rendered(started.generation, 10)
        .expect("render");
    transport.decoder_eof(started.decode_session).expect("EOF");
    assert!(transport.transport_drained(), "naturally drained");

    // Stop arrives after the transport published drain but before
    // MusicKernel consumed the fact: the stop negates the stale drained
    // truth and withdraws the pending fact — stop, not ENDED, wins.
    transport.stop().expect("stop after drain");
    assert!(
        !transport.transport_drained(),
        "stop negates stale drained truth"
    );

    transport.claim_fence().expect("claim");
    transport.fence_succeeded().expect("verdict");

    music.observe_all(transport.take_derived_facts());
    assert_eq!(music.state(), PlaybackState::Idle);
    assert_ne!(music.state(), PlaybackState::Ended);
}

#[test]
fn rapid_seek_seek_next_stop_supersede_chain_completes() {
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let first_seek = transport.seek(500).expect("first seek prepares");
    assert_eq!(first_seek.superseded, None);

    let second_seek = transport.seek(900).expect("second seek supersedes");
    assert_eq!(
        second_seek.superseded.map(|s| s.generation),
        Some(first_seek.generation),
        "pending prepared contribution atomically superseded"
    );

    let replacement = transport
        .next_track(media.open())
        .expect("next supersedes the pending seek");
    assert_eq!(
        replacement.superseded.map(|s| s.generation),
        Some(second_seek.generation)
    );
    assert_ne!(replacement.track, started.track);

    let cut = transport.stop().expect("stop wins the chain");
    assert_eq!(cut, started.generation);
    transport.claim_fence().expect("claim");
    let verdict = transport.fence_succeeded().expect("verdict");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::StopCompleted {
            cut: started.generation
        }
    );

    music.observe_all(transport.take_derived_facts());
    assert_eq!(music.state(), PlaybackState::Idle);
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.active, None);
    assert_eq!(snapshot.prepared, None);
    assert_eq!(snapshot.fence, FenceState::Idle);
    assert!(
        snapshot.track_sessions.is_empty(),
        "both tracks fully retired after stop"
    );
}

#[test]
fn next_intent_during_claimed_promote_fence_consumes_verdict_without_fake_promotion() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    transport.begin_hard_cut().expect("cut begins");
    transport.claim_fence().expect("device claims the flush");

    // A later intent during the claimed promote-fence cannot rewrite the
    // claimed physical transaction, but it may supersede the promotion
    // target: the pending prepared window is replaced.
    let replacement = transport
        .next_track(media.open())
        .expect("new intent allowed while promote fence is in flight");
    assert_eq!(
        replacement.superseded.map(|s| s.generation),
        Some(sought.generation)
    );

    // The claimed flush completes for the same cut generation; its verdict
    // is consumed without promoting the superseded target.
    let verdict = transport
        .fence_succeeded()
        .expect("claimed flush completes");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::VerdictConsumed {
            cut: started.generation
        }
    );
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.fence, FenceState::Idle);
    assert_eq!(
        snapshot.active.expect("old active unchanged").generation,
        started.generation
    );
    assert_eq!(
        snapshot
            .prepared
            .expect("replacement prepared intact")
            .generation,
        replacement.generation
    );

    // The replacement runs its own episode and promotes through its own
    // fence (cutting the same already-silent generation again is legal).
    prime(
        &mut transport,
        replacement.decode_session,
        replacement.generation,
        0,
    );
    let cut = transport.begin_hard_cut().expect("second cut begins");
    assert_eq!(cut.cut, started.generation);
    transport.claim_fence().expect("claim");
    let verdict = transport.fence_succeeded().expect("verdict");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::Promoted {
            promoted: replacement.generation,
            cut: started.generation,
        }
    );
}

#[test]
fn stop_during_claimed_promote_fence_reinterprets_the_same_flush_as_terminal() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    transport.begin_hard_cut().expect("cut begins");
    transport.claim_fence().expect("device claims");

    let cut = transport
        .stop()
        .expect("stop reinterprets the claimed flush");
    assert_eq!(cut, started.generation);

    let verdict = transport
        .fence_succeeded()
        .expect("same flush completes terminally");
    assert_eq!(
        verdict,
        FenceVerdictOutcome::StopCompleted {
            cut: started.generation
        }
    );
    let snapshot = transport.snapshot();
    assert_eq!(snapshot.active, None, "no promotion happened");
    assert_eq!(
        snapshot.prepared, None,
        "stop superseded the prepared window"
    );
}

#[test]
fn seek_and_next_are_refused_while_a_stop_fence_is_in_flight() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport.stop().expect("stop requested");
    transport.claim_fence().expect("claim");

    // A stop fence is a terminal cut: new seek/next intents wait for the
    // verdict instead of queueing onto an undecided physical state.
    assert!(
        transport.seek(100).is_err(),
        "seek refused during stop fence"
    );
    assert!(
        transport.next_track(media.open()).is_err(),
        "next refused during stop fence"
    );
    let snapshot = transport.snapshot();
    assert_eq!(
        snapshot.prepared, None,
        "refused intents leave no window behind"
    );
    assert_eq!(
        snapshot
            .active
            .expect("active still exists for the fence")
            .generation,
        started.generation
    );

    // After the verdict lands, a new episode may start.
    transport.fence_succeeded().expect("verdict");
    transport
        .play(media.open())
        .expect("new episode after stop");
}

#[test]
fn prepared_eof_before_readiness_abandons_the_discontinuity_explicitly() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport
        .seek(10_000)
        .expect("seek to the file tail prepares");

    // Seek lands exactly at EOF: the producer terminals before priming.
    // EOF evidence does not imply prepared readiness; the contribution is
    // dropped with an explicit outcome, never silently promoted.
    let outcome = transport
        .decoder_eof(sought.decode_session)
        .expect("EOF handled");
    assert_eq!(
        outcome,
        EofOutcome::PreparedAbandonedBeforeReadiness {
            generation: sought.generation,
        }
    );

    let snapshot = transport.snapshot();
    assert_eq!(snapshot.prepared, None, "unprimable prepared dropped");
    assert_eq!(
        snapshot.active.expect("active unaffected").generation,
        started.generation
    );
    assert!(snapshot.active.expect("active").admission_open);

    // A later seek still works and fresh generations are issued.
    let retry = transport.seek(500).expect("later seek after abandonment");
    assert_ne!(retry.generation, sought.generation);
}

#[test]
fn retired_generation_can_never_re_enter() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    promote(&mut transport);

    // After promotion the old generation holds no window role and its
    // admission never reopens: no decode evidence, no submissions.
    assert!(
        transport
            .decode_result(
                started.decode_session,
                MediaSpan {
                    generation: started.generation,
                    start: 720,
                    end: 780,
                }
            )
            .is_err()
    );
    assert!(transport.media_submitted(started.generation, 10).is_err());

    // Superseded prepared generations are equally unreachable.
    let first = transport.seek(2000).expect("first seek");
    let second = transport.seek(3000).expect("second seek supersedes first");
    assert!(
        transport
            .decode_result(
                first.decode_session,
                MediaSpan {
                    generation: first.generation,
                    start: 2000,
                    end: 2080,
                }
            )
            .is_err()
    );
    assert!(transport.media_submitted(first.generation, 10).is_err());
    let _ = second;
}

// --- Adversarial-review regressions (Reviewer B blocking findings) ---

#[test]
fn decode_evidence_must_carry_its_own_session_generation() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    let sought = transport.seek(1000).expect("seek prepares");

    // Fake priming: the active session's id with a span claiming the
    // prepared generation. The admission contract binds evidence to the
    // generation of the session that produced it — this must be rejected
    // without priming the prepared window or moving the active cursor.
    let forged = transport.decode_result(
        started.decode_session,
        MediaSpan {
            generation: sought.generation,
            start: 1000,
            end: 1480,
        },
    );
    assert!(
        forged.is_err(),
        "evidence generation must match its session"
    );

    let snapshot = transport.snapshot();
    assert!(
        !snapshot.prepared.expect("prepared kept").ready,
        "no fake priming"
    );
    let active = snapshot.active.expect("active kept");
    assert_eq!(
        active.decode_position, 0,
        "active cursor unmoved by forged span"
    );
    assert_eq!(active.accepted_frames, 0);

    // The reverse direction (prepared session id, active-generation span)
    // is equally rejected.
    let laundered = transport.decode_result(
        sought.decode_session,
        MediaSpan {
            generation: started.generation,
            start: 0,
            end: 100,
        },
    );
    assert!(laundered.is_err());

    // Empty spans carry no media and must not prime anything.
    assert!(
        transport
            .decode_result(
                sought.decode_session,
                MediaSpan {
                    generation: sought.generation,
                    start: 1000,
                    end: 1000,
                }
            )
            .is_err()
    );
}

#[test]
fn stale_drained_facts_are_withdrawn_by_promotion_and_new_episode() {
    // Promotion invalidates a published drained truth; the unconsumed
    // drained fact must be withdrawn so ENDED cannot fire while the
    // promoted generation is playing.
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    drain_naturally(&mut transport, started.decode_session, started.generation);
    assert!(transport.transport_drained());

    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    promote(&mut transport);
    assert!(
        !transport.transport_drained(),
        "promotion resets drained truth"
    );

    music.observe_all(transport.take_derived_facts());
    assert_ne!(
        music.state(),
        PlaybackState::Ended,
        "withdrawn drained fact must not END the promoted episode"
    );

    // The replay flow on a fresh transport: natural end completes, a new
    // episode opens, and no stale drained fact leaks into it either.
    let mut transport = TransportKernel::new();
    let mut music = MusicKernel::new();
    let second = transport.play(media.open()).expect("new episode");
    drain_naturally(&mut transport, second.decode_session, second.generation);
    transport
        .complete_ended_episode()
        .expect("episode completes");
    transport.play(media.open()).expect("replay");
    music.observe_all(transport.take_derived_facts());
    assert_ne!(
        music.state(),
        PlaybackState::Ended,
        "new episode is not ENDED"
    );
}

#[test]
fn submissions_must_be_backed_by_admitted_decode_evidence() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");

    // Media cannot appear on the output path without decode evidence
    // backing it.
    assert!(
        transport.media_submitted(started.generation, 10).is_err(),
        "submission without decode backing is rejected"
    );

    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode ten frames");
    transport
        .media_submitted(started.generation, 10)
        .expect("backed submission accepted");
    assert!(
        transport.media_submitted(started.generation, 1).is_err(),
        "submission beyond the decoded backing is rejected"
    );
}

#[test]
fn late_render_evidence_for_released_generation_is_rejected_not_panicking() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    // After a completed stop the old generation's subtree is released; a
    // trailing device render must be rejected, not panic.
    let started = transport.play(media.open()).expect("play starts");
    transport.stop().expect("stop");
    transport.claim_fence().expect("claim");
    transport.fence_succeeded().expect("verdict");
    assert!(transport.media_rendered(started.generation, 1).is_err());

    // Same after a naturally completed episode; empty renders are not
    // evidence at all.
    let second = transport.play(media.open()).expect("second episode");
    drain_naturally(&mut transport, second.decode_session, second.generation);
    transport.complete_ended_episode().expect("completes");
    assert!(transport.media_rendered(second.generation, 0).is_err());
}

#[test]
fn refused_next_intent_leaves_no_phantom_track_session() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    let started = transport.play(media.open()).expect("play starts");
    transport.stop().expect("stop");
    transport.claim_fence().expect("claim");

    assert!(transport.next_track(media.open()).is_err());
    let snapshot = transport.snapshot();
    assert_eq!(
        snapshot.track_sessions.len(),
        1,
        "refused intent must not leave an empty track session behind"
    );
    assert_eq!(snapshot.track_sessions[0].id, started.track);
}

#[test]
fn consumed_verdict_can_complete_natural_drain() {
    let mut transport = TransportKernel::new();
    let mut media = Media::new();

    // Active media terminal with frames still queued; a cut begins, its
    // target is superseded mid-flight, and the consumed verdict flushes
    // the queued remainder — the natural-drain predicate then holds and
    // must be derived without waiting for further evidence.
    let started = transport.play(media.open()).expect("play starts");
    transport
        .decode_result(
            started.decode_session,
            MediaSpan {
                generation: started.generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode");
    transport
        .media_submitted(started.generation, 10)
        .expect("submit");
    transport
        .media_rendered(started.generation, 5)
        .expect("render half");
    transport.decoder_eof(started.decode_session).expect("EOF");

    let sought = transport.seek(1000).expect("seek prepares");
    prime(
        &mut transport,
        sought.decode_session,
        sought.generation,
        1000,
    );
    transport.begin_hard_cut().expect("cut begins");
    transport.claim_fence().expect("claim");
    let replacement = transport
        .next_track(media.open())
        .expect("supersede target");
    let _ = replacement;

    let verdict = transport.fence_succeeded().expect("verdict");
    assert!(matches!(
        verdict,
        FenceVerdictOutcome::VerdictConsumed { .. }
    ));
    assert!(
        transport.transport_drained(),
        "flush-completed drain predicate must be derived at the verdict"
    );

    let mut music = MusicKernel::new();
    music.observe_all(transport.take_derived_facts());
    assert_eq!(music.state(), PlaybackState::Ended);
}

/// Drive an active session to natural drain: decode, submit, render all
/// frames, then producer EOF.
fn drain_naturally(
    transport: &mut TransportKernel,
    session: DecodeSessionId,
    generation: GenerationId,
) {
    transport
        .decode_result(
            session,
            MediaSpan {
                generation,
                start: 0,
                end: 10,
            },
        )
        .expect("decode");
    transport.media_submitted(generation, 10).expect("submit");
    transport.media_rendered(generation, 10).expect("render");
    transport.decoder_eof(session).expect("EOF");
}
