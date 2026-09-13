//! FV-CONC-0: Loom model checking of the REAL `PcmEdge` (campaign
//! QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B2).
//!
//! The whole file is `cfg(loom)`: run with
//! `RUSTFLAGS="--cfg loom" cargo test -p qianqian-playback --features loom`.
//! The tests spawn threads against the production edge — its
//! synchronization primitives are swapped for loom's drop-in equivalents
//! by `cfg(loom)` in `edge.rs`, preserving semantics — so every result is
//! about the production code, not a copy.
//!
//! Modeled slice (campaign B2.1: where concurrency actually begins): the
//! edge is the only genuinely interleaved shared state in the playback
//! session. Properties: L1 write/read/stop interleaving with FIFO ring
//! integrity; L3 first-terminal-wins; L4 blocked-endpoint wakeup. A clean
//! run is SCHEDULE-CLEAN within the stated thread/operation bounds
//! (vocabulary #124) — not a general proof.

#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;

use qianqian_audio_api::ports::{PcmPull, RenderPcmInput};
use qianqian_playback::{EdgeTerminal, PcmEdge, WriteOutcome};

/// L1 — concurrent write × read × stop.
///
/// The consumer must observe only a FIFO prefix of what was written (ring
/// integrity under every interleaving), the run must end `Stopped` (the
/// stopper always runs), EOF must be unreachable, and a producer blocked
/// behind a full edge must never lose its wakeup.
#[test]
fn loom_l1_write_read_stop_interleave() {
    loom::model(|| {
        let edge = Arc::new(PcmEdge::new(1, 2)); // 1 channel, 2 samples
        let producer = {
            let e = edge.clone();
            thread::spawn(move || e.write(&[7.0, 9.0]))
        };
        let stopper = {
            let e = edge.clone();
            thread::spawn(move || e.stop())
        };
        let consumer = {
            let e = edge.clone();
            thread::spawn(move || {
                let mut dst = [0.0f32; 2];
                let mut expect = 7.0f32;
                let mut consumed = 0usize;
                let mut stopped = false;
                loop {
                    match e.read_frames(&mut dst) {
                        PcmPull::Frames(k) => {
                            for v in &dst[..k] {
                                assert_eq!(*v, expect, "FIFO ring order violated");
                                if *v == 7.0 {
                                    expect = 9.0;
                                }
                            }
                            consumed += k;
                        }
                        PcmPull::Eof => panic!("EOF is unreachable without close_eof"),
                        PcmPull::Stopped => {
                            stopped = true;
                            break;
                        }
                    }
                }
                (consumed, stopped)
            })
        };

        let written = producer.join().unwrap();
        stopper.join().unwrap();
        let (consumed, stopped) = consumer.join().unwrap();

        // The stopper always runs and terminals are monotone, so the edge
        // must have ended Stopped. `Written` only means the whole slice
        // was accepted before the stop became visible — legal per the
        // write contract (started before stop); `Stopped` covers every
        // later race. A stop that lands mid-write abandons the buffered
        // remainder, so no conservation law holds across the race; what
        // must hold is FIFO integrity (asserted above) and clean exit.
        assert!(matches!(
            written,
            WriteOutcome::Written | WriteOutcome::Stopped
        ));
        assert_eq!(edge.terminal(), EdgeTerminal::Stopped);
        assert!(stopped, "consumer must exit through the terminal");
        assert!(consumed <= 2);
    })
}

/// L3 — first terminal wins: `close_eof` × `stop` race. The terminal is
/// one of the two, never downgraded or swapped, and the consumer outcome
/// matches the final terminal exactly.
#[test]
fn loom_l3_first_terminal_wins() {
    loom::model(|| {
        let edge = Arc::new(PcmEdge::new(1, 2));
        let eof = {
            let e = edge.clone();
            thread::spawn(move || e.close_eof())
        };
        let stop = {
            let e = edge.clone();
            thread::spawn(move || e.stop())
        };
        eof.join().unwrap();
        stop.join().unwrap();

        let term = edge.terminal();
        let mut dst = [0.0f32; 1];
        let pull = edge.read_frames(&mut dst);
        match term {
            EdgeTerminal::Eof => {
                assert!(matches!(pull, PcmPull::Eof), "EOF terminal must read Eof");
            }
            EdgeTerminal::Stopped => {
                assert!(
                    matches!(pull, PcmPull::Stopped),
                    "stopped terminal must read Stopped"
                );
            }
            other => panic!("terminal must be Eof or Stopped, got {other:?}"),
        }
    })
}

/// L4a — a producer blocked on a full edge must be woken by `stop` and
/// return `Stopped` under every schedule (no lost wakeup, no wedge).
#[test]
fn loom_l4a_stop_unblocks_full_producer() {
    loom::model(|| {
        let edge = Arc::new(PcmEdge::new(1, 1));
        // Fill the single-sample edge synchronously: the spawned write
        // below is genuinely blocked on `space_freed`.
        assert_eq!(edge.write(&[7.0]), WriteOutcome::Written);
        let producer = {
            let e = edge.clone();
            thread::spawn(move || e.write(&[9.0]))
        };
        let stopper = {
            let e = edge.clone();
            thread::spawn(move || e.stop())
        };
        let written = producer.join().unwrap();
        stopper.join().unwrap();
        assert_eq!(written, WriteOutcome::Stopped);
    })
}

/// L4b — a consumer blocked on an empty edge must be woken by a terminal:
/// once by committed EOF (reading `Eof`), once by stop (reading
/// `Stopped`), under every schedule.
#[test]
fn loom_l4b_terminal_unblocks_blocked_consumer() {
    for set_eof in [true, false] {
        loom::model(move || {
            let edge = Arc::new(PcmEdge::new(1, 2));
            let consumer = {
                let e = edge.clone();
                thread::spawn(move || {
                    let mut dst = [0.0f32; 1];
                    e.read_frames(&mut dst)
                })
            };
            let setter = {
                let e = edge.clone();
                thread::spawn(move || {
                    if set_eof {
                        e.close_eof();
                    } else {
                        e.stop();
                    }
                })
            };
            setter.join().unwrap();
            let pull = consumer.join().unwrap();
            if set_eof {
                assert!(matches!(pull, PcmPull::Eof));
            } else {
                assert!(matches!(pull, PcmPull::Stopped));
            }
        });
    }
}
