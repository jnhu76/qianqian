//! The Playback Session component (first-audible-slice design §6).
//!
//! Activation ordering (each step immediately followed by its inverse
//! registration so a raise unwinds exactly what the step acquired):
//!
//! ```text
//! resolve Decode capability once
//! resolve Output capability once
//! open decode endpoint            (RAII: rides with the decode worker)
//! build bounded edge
//! open render stream              -> inverse: stop_and_join stream
//! spawn decode worker             -> inverse: stop edge + join worker
//! ```
//!
//! Effects unwind strictly LIFO, so disposal runs stop-edge, join worker,
//! then stop-join-release the stream — the required stop -> join ->
//! release order.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use qianqian_audio_api::ports::{
    AudioOutputCapability, DecodeOutcome, DecodedPcmStream, PcmDecodeCapability,
};
use qianqian_composition::{ActivationError, ComponentSpec, Discharge};

use crate::completion::SessionCompletion;
use crate::edge::PcmEdge;

/// Frames of PCM the edge buffers (~185 ms at 44.1 kHz stereo). Chosen
/// from the measured decode tail (p99 ~0.2 ms per 1024-frame block,
/// decode-cost-model.md §5) plus scheduling margin — a latency bound, not
/// a throughput parameter.
const EDGE_CAPACITY_FRAMES: usize = 8192;

/// Frames per decode staging refill. Matches the block size the decode
/// baselines were measured at.
const STAGING_FRAMES: usize = 1024;

/// The Playback Session component definition. The App captures the file
/// and the completion handle it will wait on; desired entries need no
/// config payload for the first slice.
pub fn playback_session_spec(file: PathBuf, completion: SessionCompletion) -> ComponentSpec {
    ComponentSpec::new("playback_session")
        .requires::<PcmDecodeCapability>()
        .requires::<AudioOutputCapability>()
        .on_activate(move |ctx| activate(&file, &completion, ctx))
}

fn activate(
    file: &Path,
    completion: &SessionCompletion,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
) -> Result<(), ActivationError> {
    // The kernel's diagnostic surface carries the FAILED verdict but not
    // the domain message; the session publishes its own activation
    // failure so the App can show why an episode never started.
    let result = activate_inner(file, completion, ctx);
    if let Err(e) = &result {
        completion.activation_failed(&e.message);
    }
    result
}

fn activate_inner(
    file: &Path,
    completion: &SessionCompletion,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
) -> Result<(), ActivationError> {
    // Control plane: capability resolution happens exactly once, here.
    let decode = ctx.resolve::<PcmDecodeCapability>().map_err(|e| {
        ActivationError::new(format!(
            "decode capability unresolved: {}",
            resolve_error(e)
        ))
    })?;
    let output = ctx.resolve::<AudioOutputCapability>().map_err(|e| {
        ActivationError::new(format!(
            "output capability unresolved: {}",
            resolve_error(e)
        ))
    })?;

    // One playback-specific decode endpoint. Its RAII rides with the
    // worker closure: released when the worker exits (joined before any
    // stream release), dropped on spawn failure, dropped on earlier raises.
    let decode_stream = decode
        .service()
        .open_media(file)
        .map_err(|e| ActivationError::new(format!("decode open failed: {}", e.message)))?;
    let format = decode_stream.format();
    completion.set_source_format(format);

    // The one bounded PCM edge: session-owned, preallocated now.
    let edge = Arc::new(PcmEdge::new(format.channels, EDGE_CAPACITY_FRAMES));

    // Playback-specific render stream, pre-bound to the edge's consumer
    // half and the session's drain signal. A bounded open verdict keeps
    // device failures inside activation.
    let stream = output
        .service()
        .open_stream(qianqian_audio_api::ports::RenderRequest {
            format,
            input: edge.clone(),
            drain: completion.drain_signal(),
        })
        .map_err(|e| ActivationError::new(format!("render stream open failed: {}", e.message)))?;
    // Registered first, so it unwinds after the worker inverse:
    // stop+join the producer before the device is released. It is a
    // relation-bearing effect: the stream is a cross-fiber contribution
    // toward the output provider.
    ctx.register_relation::<AudioOutputCapability>(&output, move || {
        stream.stop_and_join();
        Discharge::Discharged
    });

    let worker_edge = edge.clone();
    let worker_completion = completion.clone();
    let worker = std::thread::Builder::new()
        .name("qianqian-decode".into())
        .spawn(move || {
            decode_worker(
                decode_stream,
                worker_edge,
                worker_completion,
                STAGING_FRAMES,
            )
        })
        .map_err(|e| ActivationError::new(format!("decode worker spawn failed: {e}")))?;
    // Registered last, so it unwinds first: stop the edge (unblocking
    // both legs), then join the producer. Relation-bearing toward the
    // decode provider (the data edge it feeds).
    ctx.register_relation::<PcmDecodeCapability>(&decode, move || {
        edge.stop();
        let _ = worker.join();
        Discharge::Discharged
    });

    Ok(())
}

/// The decode worker: SongCore/FFmpeg/filesystem work lives only here.
/// Refills a once-allocated staging buffer and feeds the bounded edge.
fn decode_worker(
    mut decode_stream: Box<dyn DecodedPcmStream>,
    edge: Arc<PcmEdge>,
    completion: SessionCompletion,
    staging_frames: usize,
) {
    let channels = usize::from(decode_stream.format().channels);
    let catch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Startup allocation only; the steady loop reuses this buffer.
        let mut staging = vec![0.0f32; staging_frames * channels];
        loop {
            match decode_stream.read_frames(&mut staging) {
                // A zero-frame response must still observe the data plane:
                // a decoder that never progresses cannot pin the worker
                // past a stop.
                Ok(DecodeOutcome::Frames(0)) => {
                    if edge.terminal() != crate::edge::EdgeTerminal::Open {
                        return;
                    }
                }
                Ok(DecodeOutcome::Frames(n)) => {
                    if edge.write(&staging[..n * channels]) == crate::edge::WriteOutcome::Stopped {
                        return;
                    }
                }
                Ok(DecodeOutcome::Eof) => {
                    edge.close_eof();
                    return;
                }
                Err(e) => {
                    completion.decode_failed(&e.message);
                    edge.fail();
                    return;
                }
            }
        }
    }));
    match catch {
        Ok(()) => {}
        Err(payload) => {
            let message = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("decode worker panicked: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("decode worker panicked: {s}")
            } else {
                "decode worker panicked".to_owned()
            };
            completion.decode_failed(&message);
            edge.fail();
        }
    }
    completion.worker_exited(edge.terminal());
}

/// Map a kernel resolution error to a human-readable diagnostic without
/// coupling session.rs to `ResolveError`'s Debug surface.
fn resolve_error(e: qianqian_composition::ResolveError) -> &'static str {
    match e {
        qianqian_composition::ResolveError::Undeclared => "capability not declared",
        qianqian_composition::ResolveError::InactiveAccess => "activation context inactive",
        qianqian_composition::ResolveError::Unresolved => "no active provider",
        qianqian_composition::ResolveError::Ambiguous => "multiple providers",
        qianqian_composition::ResolveError::AlreadyProvided => "already provided",
    }
}
