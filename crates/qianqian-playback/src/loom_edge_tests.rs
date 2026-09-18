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
//! integrity; L3 first-terminal-wins over {EOF, failure, stop}; L4
//! blocked-endpoint wakeup under every terminal; L5–L7 (F5/D14.5) the
//! non-terminal `invalidate` under every interleaving with the live
//! endpoints — it purges, wakes, and never touches terminal truth. A
//! clean run is SCHEDULE-CLEAN within the stated thread/operation
//! bounds (vocabulary #124) — not a general proof.

#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;

use qianqian_audio_api::ports::{PcmPull, RenderPcmInput};

// White-box: included into the crate by src/lib.rs (cfg(all(test, loom))),
// so the modeled edge is the production mechanism reached through the
// crate path, not a public export.
use crate::edge::{EdgeTerminal, PcmEdge};

/// The production write shape (the decode worker's bounded-slice loop):
/// `write_some` until the slice is taken, `wait_for_space` between full
/// attempts, `false` when a terminal ended the write. F5 removed the
/// blocking whole-slice write, so loom now explores exactly the write
/// primitives production runs.
fn write_all(e: &PcmEdge, mut src: &[f32]) -> bool {
    while !src.is_empty() {
        let n = e.write_some(src);
        src = &src[n..];
        if n == 0 {
            if e.terminal() != EdgeTerminal::Open {
                return false;
            }
            e.wait_for_space(std::time::Duration::from_millis(1));
        }
    }
    true
}

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
            thread::spawn(move || write_all(&e, &[7.0, 9.0]))
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
                // The only exit is the terminal: EOF is unreachable without
                // close_eof, so reaching the join below proves the consumer
                // left through `Stopped`.
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
                        PcmPull::Stopped => break,
                    }
                }
                consumed
            })
        };

        let written = producer.join().unwrap();
        stopper.join().unwrap();
        let consumed = consumer.join().unwrap();

        // The stopper always runs and terminals are monotone, so the edge
        // must have ended Stopped. A `true` write only means the whole
        // slice was accepted before the stop became visible — legal per
        // the write contract (started before stop); `false` covers every
        // later race. A stop that lands mid-write abandons the buffered
        // remainder, so no conservation law holds across the race; what
        // must hold is FIFO integrity (asserted above) and clean exit.
        let _ = written;
        assert_eq!(edge.terminal(), EdgeTerminal::Stopped);
        assert!(consumed <= 2);
    })
}

/// L3a — first terminal wins: `close_eof` × `stop` race. The terminal is
/// one of the two, never downgraded or swapped, and the consumer outcome
/// matches the final terminal exactly.
#[test]
fn loom_l3a_eof_stop_first_terminal_wins() {
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

/// L3b — first terminal wins: `fail` × `stop` race. Whichever terminal
/// commits first keeps its identity internally (`Failed` is never
/// downgraded to `Stopped` and vice versa — the edge holds a single
/// winner, not a merged state); the consumer outcome is `Stopped` for
/// both, which is the collapsed read-side view.
#[test]
fn loom_l3b_fail_stop_first_terminal_wins() {
    loom::model(|| {
        let edge = Arc::new(PcmEdge::new(1, 2));
        let fail = {
            let e = edge.clone();
            thread::spawn(move || e.fail())
        };
        let stop = {
            let e = edge.clone();
            thread::spawn(move || e.stop())
        };
        fail.join().unwrap();
        stop.join().unwrap();

        let term = edge.terminal();
        match term {
            EdgeTerminal::Failed | EdgeTerminal::Stopped => {}
            other => panic!("terminal must be Failed or Stopped, got {other:?}"),
        }
        let mut dst = [0.0f32; 1];
        assert!(
            matches!(edge.read_frames(&mut dst), PcmPull::Stopped),
            "fail × stop must read Stopped under every schedule"
        );
    })
}

/// L4a — a producer blocked on a full edge must be woken by a terminal
/// and return `Stopped` under every schedule (no lost wakeup, no wedge):
/// once by `stop`, once by `fail`. The failing edge keeps its `Failed`
/// identity while still unblocking the writer.
#[test]
fn loom_l4a_terminal_unblocks_full_producer() {
    /// Which terminal the closer commits against the blocked producer.
    #[derive(Clone, Copy)]
    enum Kill {
        Stop,
        Fail,
    }
    for kill in [Kill::Stop, Kill::Fail] {
        loom::model(move || {
            let edge = Arc::new(PcmEdge::new(1, 1));
            // Fill the single-sample edge synchronously: the spawned write
            // below is genuinely blocked on `space_freed`.
            assert_eq!(edge.write_some(&[7.0]), 1);
            let producer = {
                let e = edge.clone();
                thread::spawn(move || write_all(&e, &[9.0]))
            };
            let closer = {
                let e = edge.clone();
                thread::spawn(move || match kill {
                    Kill::Stop => e.stop(),
                    Kill::Fail => e.fail(),
                })
            };
            let written = producer.join().unwrap();
            closer.join().unwrap();
            // No consumer ever runs, so the slot can never free before
            // the terminal: the write can only leave through the
            // terminal route.
            assert!(!written);
            match kill {
                Kill::Stop => assert_eq!(edge.terminal(), EdgeTerminal::Stopped),
                Kill::Fail => assert_eq!(edge.terminal(), EdgeTerminal::Failed),
            }
        });
    }
}

/// L4b — a consumer blocked on an empty edge must be woken by a terminal
/// under every schedule (no lost wakeup, no wedge): by committed EOF
/// (reading `Eof`), by stop (reading `Stopped`), and by failure
/// (reading the collapsed `Stopped` while the edge itself keeps the
/// `Failed` identity).
#[test]
fn loom_l4b_terminal_unblocks_blocked_consumer() {
    /// Which terminal the setter commits against the blocked consumer.
    #[derive(Clone, Copy)]
    enum Kill {
        Eof,
        Stop,
        Fail,
    }
    for kill in [Kill::Eof, Kill::Stop, Kill::Fail] {
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
                thread::spawn(move || match kill {
                    Kill::Eof => e.close_eof(),
                    Kill::Stop => e.stop(),
                    Kill::Fail => e.fail(),
                })
            };
            setter.join().unwrap();
            let pull = consumer.join().unwrap();
            match kill {
                Kill::Eof => {
                    assert!(matches!(pull, PcmPull::Eof));
                    assert_eq!(edge.terminal(), EdgeTerminal::Eof);
                }
                Kill::Stop => {
                    assert!(matches!(pull, PcmPull::Stopped));
                    assert_eq!(edge.terminal(), EdgeTerminal::Stopped);
                }
                Kill::Fail => {
                    assert!(matches!(pull, PcmPull::Stopped));
                    assert_eq!(edge.terminal(), EdgeTerminal::Failed);
                }
            }
        });
    }
}

/// L5 — the F5 purge (D14.5): `invalidate` interleaved with the live
/// producer and consumer. Whatever the schedule, the consumer's read
/// sequence is a legal observation of {purge happened before/after this
/// or that frame}: one of `[]`, `[7]`, `[9]`, `[7,9]` — never a
/// reordered, duplicated or torn sequence (ring integrity survives the
/// cursor reset), and the run always terminates through the committed
/// EOF. `[9,7]` would be stale pre-cut content reaching the consumer
/// after post-cut content — the exclusion the frozen protocol owes.
#[test]
fn loom_l5_invalidate_interleave_keeps_fifo_and_terminates() {
    loom::model(|| {
        let edge = Arc::new(PcmEdge::new(1, 2));
        // One stale frame sits in the edge when the purge fires.
        assert_eq!(edge.write_some(&[7.0]), 1);
        let consumer = {
            let e = edge.clone();
            thread::spawn(move || {
                let mut dst = [0.0f32; 2];
                let mut seen = Vec::new();
                loop {
                    match e.read_frames(&mut dst) {
                        PcmPull::Frames(k) => seen.extend_from_slice(&dst[..k]),
                        PcmPull::Eof => break seen,
                        PcmPull::Stopped => panic!("no stop is ever issued"),
                    }
                }
            })
        };
        let invalidator = {
            let e = edge.clone();
            thread::spawn(move || e.invalidate())
        };
        let producer = {
            let e = edge.clone();
            thread::spawn(move || write_all(&e, &[9.0]))
        };
        invalidator.join().unwrap();
        producer.join().unwrap();
        edge.close_eof();
        let seen = consumer.join().unwrap();
        assert_eq!(edge.terminal(), EdgeTerminal::Eof);
        assert!(
            matches!(seen.as_slice(), [] | [7.0] | [9.0] | [7.0, 9.0]),
            "an illegal post-purge observation: {seen:?}"
        );
    })
}

/// L6 — `invalidate` is NOT a terminal: raced against a terminal
/// setter it never wins, never overwrites, and never resurrects a
/// closed data plane (a purge landing after the terminal cannot reopen
/// the edge or change its identity).
#[test]
fn loom_l6_invalidate_never_touches_terminal_truth() {
    /// Which terminal the setter commits against the purge.
    #[derive(Clone, Copy)]
    enum Kill {
        Eof,
        Stop,
    }
    for kill in [Kill::Eof, Kill::Stop] {
        loom::model(move || {
            let edge = Arc::new(PcmEdge::new(1, 2));
            let setter = {
                let e = edge.clone();
                thread::spawn(move || match kill {
                    Kill::Eof => e.close_eof(),
                    Kill::Stop => e.stop(),
                })
            };
            let invalidator = {
                let e = edge.clone();
                thread::spawn(move || e.invalidate())
            };
            setter.join().unwrap();
            // The purge lands after the setter under every explored
            // schedule of this join — the after-the-terminal face.
            edge.invalidate();
            invalidator.join().unwrap();
            match kill {
                Kill::Eof => assert_eq!(edge.terminal(), EdgeTerminal::Eof),
                Kill::Stop => assert_eq!(edge.terminal(), EdgeTerminal::Stopped),
            }
            let mut dst = [0.0f32; 1];
            match kill {
                Kill::Eof => assert!(matches!(edge.read_frames(&mut dst), PcmPull::Eof)),
                Kill::Stop => {
                    assert!(matches!(edge.read_frames(&mut dst), PcmPull::Stopped))
                }
            }
        });
    }
}

/// L7 — the purge unblocks a producer parked on a FULL edge (no lost
/// wakeup) and the parked-over frame never reaches the consumer: the
/// writer can only complete after the purge freed its slot, so the
/// consumer sees exactly the post-cut content.
#[test]
fn loom_l7_invalidate_unblocks_full_producer_and_purges_the_stale_frame() {
    loom::model(|| {
        let edge = Arc::new(PcmEdge::new(1, 1));
        // Fill the single-sample edge synchronously: the spawned write
        // below is genuinely blocked on `space_freed`.
        assert_eq!(edge.write_some(&[7.0]), 1);
        let producer = {
            let e = edge.clone();
            thread::spawn(move || write_all(&e, &[9.0]))
        };
        let invalidator = {
            let e = edge.clone();
            thread::spawn(move || e.invalidate())
        };
        invalidator.join().unwrap();
        let written = producer.join().unwrap();
        assert!(
            written,
            "the purge frees the slot; the write must complete without \
             any terminal"
        );
        assert_eq!(edge.terminal(), EdgeTerminal::Open);
        edge.close_eof();
        let mut dst = [0.0f32; 2];
        let mut seen = Vec::new();
        loop {
            match edge.read_frames(&mut dst) {
                PcmPull::Frames(k) => seen.extend_from_slice(&dst[..k]),
                PcmPull::Eof => break,
                PcmPull::Stopped => panic!("no stop is ever issued"),
            }
        }
        assert_eq!(
            seen,
            vec![9.0],
            "the pre-cut frame must never be delivered after the purge"
        );
    })
}
