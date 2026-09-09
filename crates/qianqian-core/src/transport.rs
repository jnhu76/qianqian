//! Transport Kernel: playback temporal authority.
//!
//! `TransportKernel` owns the meaning of playback time: active/prepared
//! temporal roles, generation admission, discontinuity execution, physical
//! fence coordination, and interpretation of raw playback evidence
//! (decode results, decoder EOF, submitted/rendered media, fence verdicts).
//!
//! This is the deterministic, mechanism-independent temporal core of the
//! accepted Playback architecture (ADR-PBK-001): pure state plus
//! evidence-ingestion methods, no threads, no devices, no PCM. Mechanisms
//! produce evidence; this kernel is the single interpreter of raw playback
//! evidence. `MusicKernel` receives derived typed facts, never raw evidence.
//!
//! `TransportKernel` is a semantic authority role, not a Composition plugin
//! boundary. This module does not constitute production Playback
//! implementation authorization.

use crate::music::TransportFact;

/// Window-scoped temporal identity. Monotonic, never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenerationId(u64);

impl GenerationId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Identity of a `TrackSession` (media identity / source lifetime root).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackSessionId(u64);

impl TrackSessionId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Identity of a `DecodeSession` (one decoder cursor/handle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DecodeSessionId(u64);

impl DecodeSessionId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Media identity for opening a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MediaId(u64);

impl MediaId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Media-time span carried by decode evidence. Media time is the timeline
/// authority; buffer boundaries are not (buffer != MediaSpan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaSpan {
    pub generation: GenerationId,
    pub start: u64,
    pub end: u64,
}

impl MediaSpan {
    pub fn frames(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// Outcome of opening a playback episode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpisodeStarted {
    pub generation: GenerationId,
    pub track: TrackSessionId,
    pub decode_session: DecodeSessionId,
}

/// A superseded prepared contribution: its admission closed and its decode
/// session retired before promotion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupersededPrepared {
    pub generation: GenerationId,
}

/// Outcome of preparing an intra-track discontinuity or track replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedForCut {
    pub generation: GenerationId,
    pub track: TrackSessionId,
    pub decode_session: DecodeSessionId,
    /// Previous pending prepared contribution atomically superseded by
    /// this intent, if one existed.
    pub superseded: Option<SupersededPrepared>,
}

/// Outcome of accepting decode evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeAdmitted {
    /// Session cursor after accepting the span.
    pub advanced_to: u64,
}

/// Read-only projection of one temporal window role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowState {
    pub generation: GenerationId,
    pub track: TrackSessionId,
    pub decode_session: DecodeSessionId,
    /// Admission contract open: decode evidence for this generation is
    /// still receivable. Closing is one-way.
    pub admission_open: bool,
    /// Prepared-window readiness (primed); active windows carry no
    /// readiness meaning.
    pub ready: bool,
    pub decode_position: u64,
    pub accepted_frames: u64,
    pub submitted_frames: u64,
    pub rendered_frames: u64,
    pub queued_frames: u64,
}

/// One decode session inside the ownership tree of a track session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeSessionState {
    pub id: DecodeSessionId,
    pub generation: GenerationId,
    /// Window role this session feeds, when it still holds one.
    pub role: Option<WindowRole>,
    pub decode_position: u64,
    pub closed: bool,
}

/// Temporal role of a window slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowRole {
    Active,
    Prepared,
}

/// Read-only projection of a track session and its decode sessions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackSessionState {
    pub id: TrackSessionId,
    pub media: MediaId,
    pub decode_sessions: Vec<DecodeSessionState>,
}

/// Physical fence handshake state visible in the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceState {
    /// No fence in flight.
    Idle,
    /// Cut requested, device has not claimed the transaction yet.
    Requested { cut: GenerationId },
    /// Device claimed the physical transaction: point of no return, later
    /// intents cannot cancel or rewrite the claimed flush.
    Claimed { cut: GenerationId },
    /// Last verdict failed; awaiting retry or fail-closed abandon.
    Failed { cut: GenerationId },
}

/// Read-only temporal projection (single authority: TransportKernel).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportSnapshot {
    pub active: Option<WindowState>,
    pub prepared: Option<WindowState>,
    pub fence: FenceState,
    pub transport_drained: bool,
    pub track_sessions: Vec<TrackSessionState>,
}

#[derive(Debug)]
struct Window {
    generation: GenerationId,
    track: TrackSessionId,
    decode_session: DecodeSessionId,
    admission_open: bool,
    ready: bool,
}

#[derive(Debug)]
struct DecodeSession {
    id: DecodeSessionId,
    generation: GenerationId,
    role: Option<WindowRole>,
    position: u64,
    closed: bool,
    accepted_frames: u64,
    submitted_frames: u64,
    rendered_frames: u64,
    queued_frames: u64,
}

#[derive(Debug)]
struct TrackSession {
    id: TrackSessionId,
    media: MediaId,
    decode_sessions: Vec<DecodeSession>,
}

/// Authority for playback-temporal semantics.
#[derive(Debug, Default)]
pub struct TransportKernel {
    track_sessions: Vec<TrackSession>,
    active: Option<Window>,
    prepared: Option<Window>,
    next_generation: u64,
    next_track: u64,
    next_decode_session: u64,
    transport_drained: bool,
    derived_facts: Vec<TransportFact>,
}

impl TransportKernel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a new playback episode: fresh generation, one track session
    /// owning one decode session, active role, admission open.
    pub fn play(&mut self, media: MediaId) -> Result<EpisodeStarted, &'static str> {
        if self.active.is_some() {
            return Err("episode already active");
        }
        let generation = self.fresh_generation();
        let track = self.open_track(media);
        let decode_session = self.open_decode_session(track, generation, WindowRole::Active, 0);
        self.active = Some(Window {
            generation,
            track,
            decode_session,
            admission_open: true,
            ready: false,
        });
        self.transport_drained = false;
        Ok(EpisodeStarted {
            generation,
            track,
            decode_session,
        })
    }

    /// Prepare an intra-track discontinuity (seek): same TrackSession opens
    /// a second decode cursor at `target` under a fresh prepared
    /// generation. A pending prepared contribution is superseded
    /// atomically: TransportKernel is the only supersede authority.
    pub fn seek(&mut self, target: u64) -> Result<PreparedForCut, &'static str> {
        let active = self.active.as_ref().ok_or("no active episode")?;
        let track = active.track;
        let from_position = target;
        self.prepare_discontinuity(track, from_position)
    }

    /// Prepare a track replacement (next): a new TrackSession opens a
    /// decode cursor under a fresh prepared generation.
    pub fn next_track(&mut self, media: MediaId) -> Result<PreparedForCut, &'static str> {
        self.active.as_ref().ok_or("no active episode")?;
        let track = self.open_track(media);
        self.prepare_discontinuity(track, 0)
    }

    /// Shared execution skeleton for seek and next (ADR freezes one
    /// skeleton): create the prepared window/generation, superseding any
    /// previous prepared contribution.
    fn prepare_discontinuity(
        &mut self,
        track: TrackSessionId,
        start_position: u64,
    ) -> Result<PreparedForCut, &'static str> {
        let superseded = self.supersede_prepared();
        let generation = self.fresh_generation();
        let decode_session =
            self.open_decode_session(track, generation, WindowRole::Prepared, start_position);
        self.prepared = Some(Window {
            generation,
            track,
            decode_session,
            admission_open: true,
            ready: false,
        });
        Ok(PreparedForCut {
            generation,
            track,
            decode_session,
            superseded,
        })
    }

    /// Atomically retire the current prepared contribution (if any):
    /// window removed, admission closed for good, session closed.
    fn supersede_prepared(&mut self) -> Option<SupersededPrepared> {
        let prepared = self.prepared.take()?;
        self.close_admission_of(&prepared);
        Some(SupersededPrepared {
            generation: prepared.generation,
        })
    }

    /// Close a window's admission and detach its decode session.
    fn close_admission_of(&mut self, window: &Window) {
        if let Some(session) = self.decode_session_mut(window.decode_session) {
            session.role = None;
            session.closed = true;
        }
    }

    /// Ingest decode evidence. Admission contract: the evidence's
    /// generation must hold a window role whose admission is still open.
    pub fn decode_result(
        &mut self,
        session: DecodeSessionId,
        span: MediaSpan,
    ) -> Result<DecodeAdmitted, &'static str> {
        let generation = span.generation;
        let frames = span.frames();
        let admitted = self
            .window_of(generation)
            .map(|w| w.admission_open)
            .unwrap_or(false);
        if !admitted {
            return Err("decode evidence not admitted by owning temporal role");
        }
        let session = self
            .decode_session_mut(session)
            .ok_or("unknown decode session")?;
        if session.closed {
            return Err("decode session closed");
        }
        session.position = session.position.max(span.end);
        session.accepted_frames += frames;
        Ok(DecodeAdmitted {
            advanced_to: session.position,
        })
    }

    /// Ingest submitted-media evidence from the output path. Only the
    /// active generation with open admission may submit.
    pub fn media_submitted(
        &mut self,
        generation: GenerationId,
        frames: u64,
    ) -> Result<(), &'static str> {
        let is_admitted_active = self
            .active
            .as_ref()
            .is_some_and(|w| w.generation == generation && w.admission_open);
        if !is_admitted_active {
            return Err("only the admitted active generation may submit media");
        }
        let session = self.session_of_mut(generation);
        session.submitted_frames += frames;
        session.queued_frames += frames;
        self.transport_drained = false;
        Ok(())
    }

    /// Ingest rendered-media evidence from the device.
    pub fn media_rendered(
        &mut self,
        generation: GenerationId,
        frames: u64,
    ) -> Result<(), &'static str> {
        let session = self.session_of_mut(generation);
        if session.queued_frames < frames {
            return Err("cannot render media that was never submitted");
        }
        session.queued_frames -= frames;
        session.rendered_frames += frames;
        Ok(())
    }

    /// Transport-drained truth: the transport holds no media that could
    /// still become audible and the active producer is terminal.
    pub fn transport_drained(&self) -> bool {
        self.transport_drained
    }

    /// Take derived typed facts destined for `MusicKernel`.
    pub fn take_derived_facts(&mut self) -> Vec<TransportFact> {
        std::mem::take(&mut self.derived_facts)
    }

    /// Read-only temporal projection.
    pub fn snapshot(&self) -> TransportSnapshot {
        TransportSnapshot {
            active: self
                .active
                .as_ref()
                .map(|w| self.window_state(w, WindowRole::Active)),
            prepared: self
                .prepared
                .as_ref()
                .map(|w| self.window_state(w, WindowRole::Prepared)),
            fence: FenceState::Idle,
            transport_drained: self.transport_drained,
            track_sessions: self
                .track_sessions
                .iter()
                .map(|t| TrackSessionState {
                    id: t.id,
                    media: t.media,
                    decode_sessions: t
                        .decode_sessions
                        .iter()
                        .map(|d| DecodeSessionState {
                            id: d.id,
                            generation: d.generation,
                            role: d.role,
                            decode_position: d.position,
                            closed: d.closed,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn fresh_generation(&mut self) -> GenerationId {
        self.next_generation += 1;
        GenerationId(self.next_generation)
    }

    fn open_track(&mut self, media: MediaId) -> TrackSessionId {
        self.next_track += 1;
        let id = TrackSessionId(self.next_track);
        self.track_sessions.push(TrackSession {
            id,
            media,
            decode_sessions: Vec::new(),
        });
        id
    }

    fn open_decode_session(
        &mut self,
        track: TrackSessionId,
        generation: GenerationId,
        role: WindowRole,
        start_position: u64,
    ) -> DecodeSessionId {
        self.next_decode_session += 1;
        let id = DecodeSessionId(self.next_decode_session);
        let track_session = self.track_session_mut(track);
        track_session.decode_sessions.push(DecodeSession {
            id,
            generation,
            role: Some(role),
            position: start_position,
            closed: false,
            accepted_frames: 0,
            submitted_frames: 0,
            rendered_frames: 0,
            queued_frames: 0,
        });
        id
    }

    fn window_of(&self, generation: GenerationId) -> Option<&Window> {
        self.active
            .iter()
            .chain(self.prepared.iter())
            .find(|w| w.generation == generation)
    }

    fn track_session_mut(&mut self, track: TrackSessionId) -> &mut TrackSession {
        self.track_sessions
            .iter_mut()
            .find(|t| t.id == track)
            .expect("track session exists while referenced")
    }

    fn decode_session_mut(&mut self, session: DecodeSessionId) -> Option<&mut DecodeSession> {
        self.track_sessions
            .iter_mut()
            .flat_map(|t| t.decode_sessions.iter_mut())
            .find(|d| d.id == session)
    }

    fn session_of_mut(&mut self, generation: GenerationId) -> &mut DecodeSession {
        self.track_sessions
            .iter_mut()
            .flat_map(|t| t.decode_sessions.iter_mut())
            .find(|d| d.generation == generation)
            .expect("generation with a window role has a decode session")
    }

    fn window_state(&self, window: &Window, role: WindowRole) -> WindowState {
        let session = self
            .track_sessions
            .iter()
            .flat_map(|t| t.decode_sessions.iter())
            .find(|d| d.id == window.decode_session)
            .expect("window references an owned decode session");
        let _ = role;
        WindowState {
            generation: window.generation,
            track: window.track,
            decode_session: window.decode_session,
            admission_open: window.admission_open,
            ready: window.ready,
            decode_position: session.position,
            accepted_frames: session.accepted_frames,
            submitted_frames: session.submitted_frames,
            rendered_frames: session.rendered_frames,
            queued_frames: session.queued_frames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_kernel_shell_is_constructible() {
        let _kernel = TransportKernel::new();
    }
}
