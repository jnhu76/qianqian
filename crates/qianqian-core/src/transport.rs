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
//!
//! # Implementation policy choices (ADR-open space)
//!
//! The ADR leaves some interaction policies open; this core selects the
//! same choices the temporal spec model closed its state space with
//! (`specs/playback` model decisions), so spec and executable oracle stay
//! in one alignment:
//!
//! * While a terminal stop fence is in flight, new seek/next intents are
//!   refused until the verdict lands (model decision 3).
//! * A stop arriving while a promote fence is in flight reinterprets the
//!   same physical flush as the terminal cut — the claimed transaction is
//!   never rewritten, only the promotion target is cleared (decision 1).
//! * A successful verdict whose promotion target was superseded is
//!   consumed without promotion; the superseding window runs its own
//!   episode and may fence the same already-silent cut generation again
//!   (decision 6).
//! * A prepared contribution that reaches producer EOF before priming is
//!   abandoned with an explicit outcome (decision 4; ADR §18 Prepared EOF).
//! * A failed fence exposes retry and fail-closed abandon as control-plane
//!   decisions (decision 5).
//! * `stopped` and natural `ENDED` are distinct product interpretations of
//!   transport truth: a completed stop publishes `Stopped`, never ENDED.
//! * Evidence integrity: decode evidence must carry its own session's
//!   generation; submissions must be backed by admitted decode evidence;
//!   empty decode/submit/render evidence is rejected (the frame-count
//!   translation of the model's block-count guards).
//! * A consumed verdict flushes queued media and derives drained truth at
//!   the verdict itself — the predicate is re-checked after every fence
//!   resolution, not only after decode/render evidence.
//! * Episode completion during an in-flight promote fence is permitted,
//!   mirroring the temporal spec (its `PublishEnded` has no fence-idle
//!   guard; only the stop fence is guarded, via ADR §18). The stale-drain
//!   exposure of that interleaving is closed by draining-truth
//!   invalidation, not by blocking the completion.
//!
//! If a future implementation pressure moves any of these choices, the
//! temporal spec must move with it (boundary change = ADR + specs + tests
//! as one synchronized transaction; policy-only drift still updates
//! `specs/playback` and reruns its core checks).

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

/// Outcome of producer-terminal (decoder EOF) evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EofOutcome {
    /// The producer for a live window reached terminal state.
    ProducerTerminal { generation: GenerationId },
    /// A prepared contribution terminated before priming and was dropped
    /// with an explicit outcome.
    PreparedAbandonedBeforeReadiness { generation: GenerationId },
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

/// A physical fence transaction in flight.
#[derive(Debug)]
struct FenceTransaction {
    /// Generation whose queued media the flush cuts.
    cut: GenerationId,
    /// Promotion target (`None` for a terminal stop fence).
    target: Option<GenerationId>,
    claimed: bool,
    failed: bool,
}

/// Outcome of starting the hard-cut handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HardCutStarted {
    pub cut: GenerationId,
    pub target: GenerationId,
}

/// Outcome of a fence verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceVerdictOutcome {
    /// The flush succeeded and the prepared window was promoted to active.
    Promoted {
        promoted: GenerationId,
        cut: GenerationId,
    },
    /// The flush physically happened, but its promotion target had been
    /// superseded: the verdict is consumed without promotion.
    VerdictConsumed { cut: GenerationId },
    /// A terminal stop fence completed; the transport holds no window.
    StopCompleted { cut: GenerationId },
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
    producer_terminal: bool,
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
    fence: Option<FenceTransaction>,
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
        self.invalidate_drained_truth();
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
        self.ensure_intent_admissible()?;
        let track = self.active.as_ref().ok_or("no active episode")?.track;
        self.prepare_discontinuity(track, target)
    }

    /// Prepare a track replacement (next): a new TrackSession opens a
    /// decode cursor under a fresh prepared generation.
    pub fn next_track(&mut self, media: MediaId) -> Result<PreparedForCut, &'static str> {
        self.ensure_intent_admissible()?;
        self.active.as_ref().ok_or("no active episode")?;
        let track = self.open_track(media);
        self.prepare_discontinuity(track, 0)
    }

    /// A stop fence in flight is a terminal cut with the physical state
    /// undecided: new seek/next intents are refused until its verdict
    /// lands (implementation choice inside the ADR's open policy space,
    /// matching the temporal spec's model decision). Checked before any
    /// mutation so a refused intent leaves nothing behind.
    fn ensure_intent_admissible(&self) -> Result<(), &'static str> {
        if let Some(fence) = &self.fence
            && fence.target.is_none()
        {
            return Err("stop fence in flight; intents wait for the verdict");
        }
        Ok(())
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

    /// Begin the hard-cut handshake over a primed prepared window: close
    /// the old admission (no new old-generation submission), then request
    /// the physical fence. Promotion happens only on a successful verdict.
    pub fn begin_hard_cut(&mut self) -> Result<HardCutStarted, &'static str> {
        if self.fence.is_some() {
            return Err("fence already in flight");
        }
        let prepared = self.prepared.as_ref().ok_or("no prepared window")?;
        if !prepared.ready {
            return Err("prepared window not ready");
        }
        let cut = self.active.as_ref().ok_or("no active episode")?.generation;
        let target = prepared.generation;
        if let Some(active) = self.active.as_mut() {
            active.admission_open = false;
        }
        self.fence = Some(FenceTransaction {
            cut,
            target: Some(target),
            claimed: false,
            failed: false,
        });
        Ok(HardCutStarted { cut, target })
    }

    /// Device claims the requested physical transaction. Claiming is the
    /// point of no return: later intents cannot cancel or rewrite the
    /// claimed flush.
    pub fn claim_fence(&mut self) -> Result<GenerationId, &'static str> {
        let fence = self.fence.as_mut().ok_or("no fence in flight")?;
        if fence.failed {
            return Err("fence failed; retry or abandon");
        }
        if fence.claimed {
            return Err("fence already claimed");
        }
        fence.claimed = true;
        Ok(fence.cut)
    }

    /// Successful fence verdict: the device completed the physical flush.
    /// The queued media of the cut generation is discarded (unrendered),
    /// and the definitive verdict resolves promotion / stop / consumption.
    pub fn fence_succeeded(&mut self) -> Result<FenceVerdictOutcome, &'static str> {
        let fence = self.fence.as_ref().ok_or("no fence in flight")?;
        if fence.failed {
            return Err("fence failed; retry or abandon");
        }
        if !fence.claimed {
            return Err("fence not claimed");
        }
        let fence = self.fence.take().expect("fence in flight checked above");
        self.flush_queued_media(fence.cut);
        let outcome = match fence.target {
            Some(target) => {
                let promotable = self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.generation == fence.cut)
                    && self
                        .prepared
                        .as_ref()
                        .is_some_and(|p| p.generation == target);
                if promotable {
                    self.promote_prepared();
                    FenceVerdictOutcome::Promoted {
                        promoted: target,
                        cut: fence.cut,
                    }
                } else {
                    // The flush physically happened; its promotion target
                    // was superseded. Consume the verdict without promotion.
                    FenceVerdictOutcome::VerdictConsumed { cut: fence.cut }
                }
            }
            None => {
                let cut = fence.cut;
                self.complete_stop(cut)
            }
        };
        // The flush can complete the natural-drain predicate (queued media
        // of the cut generation is gone); derive drained truth now instead
        // of waiting for further evidence that may never arrive.
        self.publish_drained_if_reached();
        Ok(outcome)
    }

    /// Failed fence verdict: the physical flush did not complete. Nothing
    /// promotes; the transaction waits for retry or fail-closed abandon.
    pub fn fence_failed(&mut self) -> Result<GenerationId, &'static str> {
        let fence = self.fence.as_mut().ok_or("no fence in flight")?;
        if !fence.claimed || fence.failed {
            return Err("fence not in claimed phase");
        }
        fence.failed = true;
        fence.claimed = false;
        Ok(fence.cut)
    }

    /// Retry the failed physical transaction (same cut generation).
    pub fn retry_fence(&mut self) -> Result<GenerationId, &'static str> {
        let fence = self.fence.as_mut().ok_or("no fence in flight")?;
        if !fence.failed {
            return Err("fence has not failed");
        }
        fence.failed = false;
        Ok(fence.cut)
    }

    /// Abandon the failed transaction and fail closed: no promotion, fence
    /// returns to idle, the admission-closed active window keeps draining
    /// naturally, and the prepared window stays for a later episode.
    pub fn abandon_fence(&mut self) -> Result<GenerationId, &'static str> {
        let fence = self.fence.as_ref().ok_or("no fence in flight")?;
        if !fence.failed {
            return Err("fence has not failed");
        }
        let cut = fence.cut;
        self.fence = None;
        Ok(cut)
    }

    /// Promote the prepared window to active over the retired old active.
    fn promote_prepared(&mut self) {
        let old_active = self.active.take().expect("promotable checked active");
        let mut promoted = self.prepared.take().expect("promotable checked prepared");
        if let Some(session) = self.decode_session_mut(old_active.decode_session) {
            session.role = None;
            session.closed = true;
        }
        if let Some(session) = self.decode_session_mut(promoted.decode_session) {
            session.role = Some(WindowRole::Active);
        }
        // Readiness is a prepared-role concept; the promoted active window
        // is simply the output authority.
        promoted.ready = false;
        self.active = Some(promoted);
        self.invalidate_drained_truth();
        self.release_drained_tracks();
    }

    /// Terminal stop completion: retire the cut active window, publish
    /// drained truth and the `Stopped` derived fact.
    fn complete_stop(&mut self, cut: GenerationId) -> FenceVerdictOutcome {
        match self.active.take() {
            Some(active) if active.generation == cut => {
                self.close_admission_of(&active);
                self.transport_drained = true;
                self.derived_facts.push(TransportFact::Stopped);
                self.release_drained_tracks();
                FenceVerdictOutcome::StopCompleted { cut }
            }
            other => {
                self.active = other;
                FenceVerdictOutcome::VerdictConsumed { cut }
            }
        }
    }

    /// Discard the cut generation's queued-but-unrendered media: after a
    /// successful flush the truncated generation stays silent.
    fn flush_queued_media(&mut self, cut: GenerationId) {
        if let Some(session) = self.find_session_mut(cut) {
            session.queued_frames = 0;
        }
    }

    /// Release track sessions whose decode sessions have all retired.
    fn release_drained_tracks(&mut self) {
        let active_track = self.active.as_ref().map(|w| w.track);
        let prepared_track = self.prepared.as_ref().map(|w| w.track);
        self.track_sessions.retain(|t| {
            Some(t.id) == active_track
                || Some(t.id) == prepared_track
                || t.decode_sessions.iter().any(|d| !d.closed)
        });
    }

    fn find_session_mut(&mut self, generation: GenerationId) -> Option<&mut DecodeSession> {
        self.track_sessions
            .iter_mut()
            .flat_map(|t| t.decode_sessions.iter_mut())
            .find(|d| d.generation == generation)
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
        if frames == 0 {
            return Err("empty decode span carries no media");
        }
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
        if session.generation != generation {
            return Err("decode evidence generation does not match its session");
        }
        if session.closed {
            return Err("decode session closed");
        }
        if session.producer_terminal {
            return Err("producer already terminal");
        }
        session.position = session.position.max(span.end);
        session.accepted_frames = session
            .accepted_frames
            .checked_add(frames)
            .ok_or("decode accounting overflow")?;
        let advanced_to = session.position;
        // First admitted decode evidence completes prepared-window priming.
        if let Some(prepared) = self.prepared.as_mut()
            && prepared.generation == generation
        {
            prepared.ready = true;
        }
        Ok(DecodeAdmitted { advanced_to })
    }

    /// Ingest submitted-media evidence from the output path. Only the
    /// active generation with open admission may submit, and every
    /// submission must be backed by decode evidence accepted under that
    /// admission.
    pub fn media_submitted(
        &mut self,
        generation: GenerationId,
        frames: u64,
    ) -> Result<(), &'static str> {
        if frames == 0 {
            return Err("empty submission carries no media");
        }
        let is_admitted_active = self
            .active
            .as_ref()
            .is_some_and(|w| w.generation == generation && w.admission_open);
        if !is_admitted_active {
            return Err("only the admitted active generation may submit media");
        }
        let session = self
            .find_session_mut(generation)
            .ok_or("admitted active generation lost its decode session")?;
        if session.accepted_frames < session.submitted_frames.saturating_add(frames) {
            return Err("submission exceeds its admitted decode backing");
        }
        session.submitted_frames = session
            .submitted_frames
            .checked_add(frames)
            .ok_or("submission accounting overflow")?;
        session.queued_frames = session
            .queued_frames
            .checked_add(frames)
            .ok_or("submission accounting overflow")?;
        self.invalidate_drained_truth();
        Ok(())
    }

    /// Ingest rendered-media evidence from the device. Rendering media of a
    /// released or unknown generation is rejected evidence, not a panic.
    pub fn media_rendered(
        &mut self,
        generation: GenerationId,
        frames: u64,
    ) -> Result<(), &'static str> {
        if frames == 0 {
            return Err("empty render carries no evidence");
        }
        let Some(session) = self.find_session_mut(generation) else {
            return Err("render evidence for a generation without a session");
        };
        if session.queued_frames < frames {
            return Err("cannot render media that was never submitted");
        }
        session.queued_frames -= frames;
        session.rendered_frames = session
            .rendered_frames
            .checked_add(frames)
            .ok_or("render accounting overflow")?;
        self.publish_drained_if_reached();
        Ok(())
    }

    /// Ingest producer-terminal (decoder EOF) evidence. EOF is raw
    /// evidence: it never directly means ENDED, and it does not imply a
    /// prepared window's readiness.
    pub fn decoder_eof(&mut self, session: DecodeSessionId) -> Result<EofOutcome, &'static str> {
        let session = self
            .decode_session_mut(session)
            .ok_or("unknown decode session")?;
        if session.closed {
            return Err("decode session closed");
        }
        if session.producer_terminal {
            return Err("producer already terminal");
        }
        session.producer_terminal = true;
        let generation = session.generation;

        // A prepared contribution that terminates before priming gets an
        // explicit outcome: the discontinuity is abandoned; it never
        // silently becomes ready.
        if let Some(prepared) = &self.prepared
            && prepared.generation == generation
            && !prepared.ready
        {
            self.supersede_prepared();
            self.derived_facts
                .push(TransportFact::PreparedAbandonedBeforeReadiness);
            return Ok(EofOutcome::PreparedAbandonedBeforeReadiness { generation });
        }
        self.publish_drained_if_reached();
        Ok(EofOutcome::ProducerTerminal { generation })
    }

    /// Stop: supersede any pending prepared contribution, close the active
    /// admission, and request (or reinterpret) the terminal physical fence.
    /// An in-flight promote fence is reinterpreted as the same terminal
    /// cut — the claimed physical flush itself is never rewritten.
    pub fn stop(&mut self) -> Result<GenerationId, &'static str> {
        let active = self.active.as_ref().ok_or("no active episode")?;
        let cut = active.generation;
        self.supersede_prepared();
        if let Some(active) = self.active.as_mut() {
            active.admission_open = false;
        }
        // The stop negates stale drained truth and withdraws any drained
        // fact MusicKernel has not consumed yet: stop, not ENDED, wins.
        self.invalidate_drained_truth();
        match &mut self.fence {
            None => {
                self.fence = Some(FenceTransaction {
                    cut,
                    target: None,
                    claimed: false,
                    failed: false,
                });
            }
            Some(fence) => {
                // Promote-fence in flight: keep the handshake phase and the
                // claimed transaction, clear only the promotion target.
                fence.target = None;
            }
        }
        Ok(cut)
    }

    /// Complete a naturally ended episode after MusicKernel interpreted
    /// the drained fact as ENDED: retire the active window, close its
    /// decode session, release the drained track.
    pub fn complete_ended_episode(&mut self) -> Result<(), &'static str> {
        if !self.transport_drained {
            return Err("transport not drained");
        }
        if self.prepared.is_some() {
            return Err("pending prepared contribution must be resolved first");
        }
        if let Some(active) = self.active.take() {
            self.close_admission_of(&active);
        }
        self.release_drained_tracks();
        Ok(())
    }

    /// Negate the drained truth and withdraw any unconsumed drained fact.
    /// Every mutation that invalidates drain (new episode, new submission,
    /// promotion, stop) must call this: a delivered-but-stale drained fact
    /// would let MusicKernel END an episode that is actually playing.
    fn invalidate_drained_truth(&mut self) {
        self.transport_drained = false;
        self.derived_facts
            .retain(|f| !matches!(f, TransportFact::NaturallyDrained));
    }

    /// Publish transport-drained truth when the natural-drain predicate is
    /// newly reached with no fence in flight. The predicate: the active
    /// producer is terminal, no generation holds queued media, and every
    /// admitted decode result has been submitted.
    fn publish_drained_if_reached(&mut self) {
        if self.transport_drained || self.fence.is_some() {
            return;
        }
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let Some(session) = self
            .track_sessions
            .iter()
            .flat_map(|t| t.decode_sessions.iter())
            .find(|d| d.id == active.decode_session)
        else {
            return;
        };
        let media_clear = session.producer_terminal
            && session.accepted_frames == session.submitted_frames
            && self
                .track_sessions
                .iter()
                .flat_map(|t| t.decode_sessions.iter())
                .all(|d| d.queued_frames == 0);
        if media_clear {
            self.transport_drained = true;
            self.derived_facts.push(TransportFact::NaturallyDrained);
        }
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
            fence: match &self.fence {
                None => FenceState::Idle,
                Some(f) if f.failed => FenceState::Failed { cut: f.cut },
                Some(f) if f.claimed => FenceState::Claimed { cut: f.cut },
                Some(f) => FenceState::Requested { cut: f.cut },
            },
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
            producer_terminal: false,
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
