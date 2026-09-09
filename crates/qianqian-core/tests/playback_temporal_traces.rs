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
use qianqian_core::transport::{MediaId, MediaSpan, TransportKernel, WindowRole};

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
