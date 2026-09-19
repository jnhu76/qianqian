//! The Stage A scenario matrix (campaign §6) as ConPTY driver scripts.
//!
//! Oracle classes used here, honestly scoped:
//! - label witnesses: the shell's own truth-class-pinned renderings
//!   (`Terminal: Stopped`, `Paused: true`, `Volume: 80/100 (desired)`,
//!   status feedback lines) and process exit codes;
//! - position witnesses: NEW published samples (D14.8 Projection)
//!   after a mark — a liveness/consumption witness, never an
//!   audibility claim;
//! - resource witnesses: bounded thread/working-set checkpoints against
//!   the child process (explicit measurements, not crash inference);
//! - absence witnesses after a mark (`warning:`/`teardown violated`
//!   never appearing in the post-mark window).
//! Stale-audio ABSENCE after a seek cutover is NOT mechanically claimed
//! by this harness — the D14.5 cutover protocol's own oracles plus the
//! recorded human-ear item own that; this matrix verifies command
//! routing, truth classes, liveness and lifecycle cleanliness on the
//! real TUI path.

use std::time::Duration;

use crate::{BACKSPACE, ENTER, ESC, LEFT, RIGHT};

#[derive(Debug)]
pub enum Step {
    /// Snapshot the output-stream offset; later `AfterMark` steps only
    /// see text emitted after it.
    Mark,
    Keys(&'static str),
    /// `keys` sent `times` times, `gap_ms` apart.
    KeysEach(&'static str, usize, u64),
    SleepMs(u64),
    /// Search the whole transcript.
    Expect { text: &'static str, within_ms: u64 },
    /// Search only text emitted after the last Mark.
    ExpectAfterMark { text: &'static str, within_ms: u64 },
    /// A `Position: MM:SS` sample after the last Mark different from
    /// the last recorded one (liveness witness).
    ExpectNewPosition { within_ms: u64 },
    /// Record the newest post-mark sample without requiring change.
    RecordPosition,
    /// The text must NOT appear in the post-Mark window.
    AbsentAfterMark(&'static str),
    /// Bounded child resource checkpoint (thread delta and working-set
    /// bound against baseline).
    Resources {
        max_thread_delta: u32,
        max_ws_mb: u64,
    },
    ExpectExit { code: u32, within_ms: u64 },
}

const SHORT_WAIT: u64 = 5_000;
const OPEN_WAIT: u64 = 20_000;
const EOF_WAIT: u64 = 30_000;

fn expect(text: &'static str) -> Step {
    Step::Expect {
        text,
        within_ms: SHORT_WAIT,
    }
}

fn expect_within(text: &'static str, within_ms: u64) -> Step {
    Step::Expect { text, within_ms }
}

fn expect_mark(text: &'static str) -> Step {
    Step::ExpectAfterMark {
        text,
        within_ms: OPEN_WAIT,
    }
}

fn new_position() -> Step {
    Step::ExpectNewPosition { within_ms: SHORT_WAIT }
}

fn keys(k: &'static str) -> Step {
    Step::Keys(k)
}

fn quit_clean() -> Vec<Step> {
    vec![
        keys("q"),
        Step::ExpectExit {
            code: 0,
            within_ms: 15_000,
        },
        // The quit report on stdout, after terminal restore.
        expect("stopped before completion"),
        Step::AbsentAfterMark("teardown violated"),
        Step::AbsentAfterMark("warning: disposal"),
    ]
}

fn open_candidate(path: &'static str) -> Vec<Step> {
    vec![
        Step::Mark,
        keys("o"),
        keys(path),
        keys(ENTER),
    ]
}

/// The corpus. flac4/mp3cbr/alac4/alac6 are the repository's committed
/// fixtures (renamed only); synth45/synth30 are locally generated
/// synthetic media (ffmpeg sine), recorded by SHA256 in the run
/// environment files; garbage.bin is a deterministic invalid candidate.
pub fn scenario(name: &str) -> (Vec<&'static str>, Vec<Step>, Duration) {
    let m: &[&str] = match name {
        "A1-flac4" | "A16-drain-stop" => &["flac4.flac"],
        "A1-mp3cbr" => &["mp3cbr.mp3"],
        "A1-alac4" => &["alac4.m4a"],
        "A1-alac6" => &["alac6.m4a"],
        "A14" | "A15" => &["flac4.flac", "mp3cbr.mp3", "alac4.m4a"],
        "A20" => &["flac4.flac", "mp3cbr.mp3", "synth45.mp3"],
        _ => &["synth45.mp3"],
    };
    let watchdog = if name == "A20" {
        Duration::from_secs(300)
    } else if name == "A19" {
        Duration::from_secs(200)
    } else {
        Duration::from_secs(120)
    };
    let steps = match name {
        // A1 — baseline playback: open, natural EOF, quit, quiet exit.
        "A1-flac4" | "A1-mp3cbr" | "A1-alac4" | "A1-alac6" => {
            let mut v = vec![
                expect("Format: 44100 Hz"),
                new_position(),
            ];
            v.push(expect_within("Terminal: Completed", EOF_WAIT));
            v.push(expect("terminal outcome committed"));
            v.extend(quit_clean());
            v
        }

        // A2 — pause/resume incl. pause right after open and rapid
        // double presses (Space is a frozen toggle over fresh
        // observations; rapid presses land in defined intent states).
        "A2" => vec![
            expect("Format: 44100 Hz"),
            keys(" "),
            expect("Pause requested: true"),
            expect_within("Paused: true", 3_000),
            // Double pause: stays recorded intent, no wedge.
            Step::KeysEach(" ", 2, 120),
            expect("Pause requested: true"),
            expect_within("Paused: true", 3_000),
            // Double press again: resume then pause.
            Step::KeysEach(" ", 2, 120),
            expect("Pause requested: true"),
            keys(" "),
            expect("Pause requested: false"),
            new_position(),
            keys(" "),
            expect("Pause requested: true"),
            expect_within("Paused: true", 3_000),
            keys(" "),
            expect("Pause requested: false"),
            new_position(),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A3 — forward seeks: small/medium, then near-end, then natural
        // EOF validates terminal semantics after cutover.
        "A3" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(3_000),
            Step::Mark,
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
            // Near end: seven more steps land ~38 s into the 45 s track.
            Step::KeysEach(RIGHT, 7, 150),
            Step::ExpectNewPosition { within_ms: 8_000 },
            expect_within("Terminal: Completed", EOF_WAIT),
            expect("terminal outcome committed"),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("EOF: played out completely"),
        ],

        // A4 — backward seeks: legal backward discontinuity, then
        // monotone progression.
        "A4" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(6_000),
            Step::RecordPosition,
            Step::Mark,
            Step::KeysEach(LEFT, 1, 200),
            new_position(),
            Step::SleepMs(2_500),
            new_position(),
            Step::KeysEach(LEFT, 1, 200),
            new_position(),
            expect("Terminal: pending"),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A5 — rapid seek: faster than playback progression; one seek
        // in flight; the rest inert. No wedge, playback continues.
        "A5" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(2_000),
            Step::Mark,
            Step::KeysEach(RIGHT, 3, 120),
            Step::KeysEach(LEFT, 2, 120),
            Step::KeysEach(RIGHT, 1, 120),
            Step::ExpectNewPosition { within_ms: 8_000 },
            Step::SleepMs(2_000),
            new_position(),
            expect("Terminal: pending"),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A6 — pause × seek: pause intent survives the cut; paused
        // rebase mid-park; resume continues post-seek; and the reverse
        // timing (pause landing right after a seek).
        "A6" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(2_000),
            keys(" "),
            expect_within("Paused: true", 3_000),
            Step::Mark,
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
            expect_mark("Pause requested: true"),
            Step::KeysEach(LEFT, 1, 200),
            new_position(),
            expect("Paused: true"),
            keys(" "),
            expect("Pause requested: false"),
            Step::SleepMs(1_500),
            new_position(),
            // Reverse timing: seek, pause lands right after.
            Step::KeysEach(RIGHT, 1, 100),
            keys(" "),
            expect("Pause requested: true"),
            expect_within("Paused: true", 4_000),
            keys(" "),
            expect("Pause requested: false"),
            new_position(),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A7 — volume walk 100→80→50→20→0→70 while playing; 0 must not
        // stop consumption; the label means the desired stream factor.
        "A7" => vec![
            expect("Format: 44100 Hz"),
            expect("Volume: 100/100 (desired)"),
            Step::KeysEach("-", 4, 100),
            expect("Volume: 80/100 (desired)"),
            Step::KeysEach("-", 6, 100),
            expect("Volume: 50/100 (desired)"),
            Step::KeysEach("-", 6, 100),
            expect("Volume: 20/100 (desired)"),
            Step::KeysEach("-", 4, 100),
            expect("Volume: 0/100 (desired)"),
            Step::Mark,
            Step::SleepMs(2_500),
            new_position(),
            expect("Terminal: pending"),
            Step::KeysEach("+", 14, 100),
            expect("Volume: 70/100 (desired)"),
            expect("Terminal: pending"),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A8 — volume while paused, then resume.
        "A8" => vec![
            expect("Format: 44100 Hz"),
            keys(" "),
            expect_within("Paused: true", 3_000),
            Step::KeysEach("-", 4, 100),
            expect("Volume: 80/100 (desired)"),
            expect("Paused: true"),
            keys(" "),
            expect("Pause requested: false"),
            new_position(),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A9 — volume routed around seek cutovers; desired level never
        // lost, episode never reset.
        "A9" => vec![
            expect("Format: 44100 Hz"),
            keys("-"),
            expect("Volume: 95/100 (desired)"),
            Step::Mark,
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
            expect("Volume: 95/100 (desired)"),
            keys("-"),
            expect("Volume: 90/100 (desired)"),
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
            Step::KeysEach(LEFT, 1, 200),
            new_position(),
            keys("+"),
            expect("Volume: 95/100 (desired)"),
            expect("Terminal: pending"),
            keys("q"),
            Step::ExpectExit { code: 0, within_ms: 15_000 },
            expect("stopped before completion"),
        ],

        // A10 — Open valid while playing, repeatedly (A→B→A).
        "A10" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(1_500),
        ]
        .into_iter()
        .chain(open_round("synth30.flac"))
        .chain(open_round("synth45.mp3"))
        .chain(open_round("synth30.flac"))
        .chain(quit_clean())
        .collect(),

        // A11 — Open invalid while old episode plays: refusal, old
        // episode untouched and still consuming, no replacement.
        "A11" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(1_500),
        ]
        .into_iter()
        .chain(open_candidate("garbage.bin"))
        .chain(vec![
            expect_mark("open refused"),
            Step::AbsentAfterMark("opened "),
        ])
        .chain(vec![
            Step::Mark,
            Step::SleepMs(1_500),
            new_position(),
        ])
        .chain(open_candidate("missing-file.flac"))
        .chain(vec![
            expect_mark("open refused"),
            expect("Terminal: pending"),
        ])
        .chain(quit_clean())
        .collect(),

        // A12 — Open while paused: replacement commits; the fresh
        // episode starts unpaused (no episode-local pause carry).
        "A12" => vec![
            expect("Format: 44100 Hz"),
            keys(" "),
            expect_within("Paused: true", 3_000),
        ]
        .into_iter()
        .chain(open_candidate("synth30.flac"))
        .chain(vec![
            expect_mark("opened"),
            expect_mark("Pause requested: false"),
            expect_mark("Paused: false"),
            Step::Mark,
            new_position(),
        ])
        .chain(quit_clean())
        .collect(),

        // A13 — Open after seek: replacement right after a cutover; no
        // residue (clean quit, bounded resources, no warnings).
        "A13" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(2_000),
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
        ]
        .into_iter()
        .chain(open_candidate("synth30.flac"))
        .chain(vec![
            expect_mark("opened"),
            expect_mark("synth30.flac"),
            Step::Mark,
            new_position(),
            Step::Resources {
                max_thread_delta: 6,
                max_ws_mb: 300,
            },
        ])
        .chain(quit_clean())
        .collect(),

        // A14 — navigation forward/backward with inert boundaries.
        "A14" => vec![
            expect("Track: 1/3"),
            Step::Mark,
            keys("n"),
            expect_mark("next: opened"),
            expect_mark("Track: 2/3"),
            keys("n"),
            expect_mark("next: opened"),
            expect_mark("Track: 3/3"),
            keys("n"),
            expect_mark("no next track"),
            expect_mark("Track: 3/3"),
            keys("p"),
            expect_mark("previous: opened"),
            expect_mark("Track: 2/3"),
            keys("p"),
            expect_mark("Track: 1/3"),
            keys("p"),
            expect_mark("no previous track"),
            expect_mark("Track: 1/3"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A15 — navigation failure: a refused candidate moves nothing
        // and never auto-skips.
        "A15" => vec![
            expect("Track: 1/3"),
            Step::Mark,
            keys("n"),
            expect_mark("next refused"),
            expect_mark("Track: 1/3"),
            Step::SleepMs(1_200),
            new_position(),
            keys("n"),
            expect_mark("next refused"),
            expect_mark("Track: 1/3"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A16 — Stop while active, repeated; immutable terminal truth.
        "A16" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(2_000),
            keys("s"),
            expect("Terminal: Stopped"),
            keys("s"),
            expect("Terminal: Stopped"),
            expect("terminal outcome committed"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A16-drain-stop — Stop inside the post-decode-EOF drain
        // window: the frozen mapping settles Completed (plays out and
        // drains), never a forged Stopped.
        "A16-drain-stop" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(3_200),
            keys("s"),
            expect_within("Terminal: Completed", 8_000),
            expect("terminal outcome committed"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A16-paused — Stop while paused (mid-play): Stopped.
        "A16-paused" => vec![
            expect("Format: 44100 Hz"),
            keys(" "),
            expect_within("Paused: true", 3_000),
            keys("s"),
            expect("Terminal: Stopped"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A16-after-seek — Stop after a committed seek: Stopped.
        "A16-after-seek" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(1_500),
            Step::Mark,
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
            keys("s"),
            expect("Terminal: Stopped"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A17 — Stop → Open: clean old-side clearing, fresh episode.
        "A17" => vec![
            expect("Format: 44100 Hz"),
            keys("s"),
            expect("Terminal: Stopped"),
        ]
        .into_iter()
        .chain(open_candidate("synth30.flac"))
        .chain(vec![
            expect_mark("opened"),
            expect_mark("synth30.flac"),
            Step::Mark,
            new_position(),
        ])
        .chain(quit_clean())
        .collect(),

        // A18 — Open → Stop: stop routes to the CURRENT episode.
        "A18" => vec![
            expect("Format: 44100 Hz"),
            Step::SleepMs(1_500),
        ]
        .into_iter()
        .chain(open_candidate("synth30.flac"))
        .chain(vec![
            expect_mark("opened"),
            expect_mark("synth30.flac"),
            Step::Mark,
            new_position(),
            keys("s"),
            expect("Terminal: Stopped"),
        ])
        .chain(quit_clean())
        .collect(),

        // A19 — repeated replacement stress: dozens of Open
        // replacements, then bounded resource checkpoints.
        "A19" => {
            let mut v = vec![expect("Format: 44100 Hz"), Step::SleepMs(1_000)];
            for round in 0..12 {
                let target = if round % 2 == 0 {
                    "synth30.flac"
                } else {
                    "synth45.mp3"
                };
                v.extend(open_candidate(target));
                v.push(expect_mark("opened"));
            }
            v.push(Step::Resources {
                max_thread_delta: 6,
                max_ws_mb: 300,
            });
            v.extend(quit_clean());
            v
        }

        // A20 — bounded soak mixing every earned control on the real
        // path; resource checkpoints at the end.
        "A20" => {
            let mut v = vec![
                expect("Track: 1/3"),
                expect("Format: 44100 Hz"),
                Step::SleepMs(1_000),
                keys("n"),
                expect_mark("next: opened"),
                expect_mark("Track: 2/3"),
                keys(" "),
                expect_within("Paused: true", 3_000),
                Step::SleepMs(800),
                keys(" "),
                expect("Pause requested: false"),
                Step::KeysEach(RIGHT, 1, 200),
                new_position(),
                Step::KeysEach(LEFT, 1, 200),
                new_position(),
                keys("-"),
                expect("Volume: 95/100 (desired)"),
                keys("+"),
                expect("Volume: 100/100 (desired)"),
                keys("n"),
                expect_mark("next: opened"),
                expect_mark("Track: 3/3"),
            ];
            v.extend(open_candidate("garbage.bin"));
            v.push(expect_mark("open refused"));
            v.push(expect_mark("Track: 3/3"));
            v.push(keys("p"));
            v.push(expect_mark("previous: opened"));
            v.push(expect_mark("Track: 2/3"));
            v.push(keys(" "));
            v.push(expect_within("Paused: true", 3_000));
            v.push(Step::KeysEach(RIGHT, 1, 200));
            v.push(new_position());
            v.push(expect("Paused: true"));
            v.push(keys(" "));
            v.push(expect("Pause requested: false"));
            v.push(Step::SleepMs(1_000));
            v.push(new_position());
            v.extend(open_candidate("synth30.flac"));
            v.push(expect_mark("opened"));
            v.push(Step::SleepMs(1_000));
            v.push(Step::KeysEach(RIGHT, 2, 200));
            v.push(new_position());
            v.push(keys("s"));
            v.push(expect("Terminal: Stopped"));
            v.extend(open_candidate("synth45.mp3"));
            v.push(expect_mark("opened"));
            v.push(Step::SleepMs(1_500));
            v.push(Step::Resources {
                max_thread_delta: 6,
                max_ws_mb: 300,
            });
            v.extend(quit_clean());
            v
        }

        other => panic!("unknown scenario {other}"),
    };
    (m.to_vec(), steps, watchdog)
}

/// One committed Open round on the TUI: mark, O, type the path, Enter,
/// the `opened` feedback, the new Source line, and proof the old
/// episode's Source line is no longer rendered after the commit.
fn open_round(path: &'static str) -> Vec<Step> {
    let mut v = open_candidate(path);
    v.push(expect_mark("opened"));
    v.push(expect_mark(path));
    let old: &'static str = if path == "synth30.flac" {
        "synth45.mp3"
    } else {
        "synth30.flac"
    };
    v.push(Step::AbsentAfterMark(old));
    v
}

// Kept for symmetry with the runtime imports: the O-input Esc/Backspace
// affordances are exercised in Stage C quality passes, not A-matrix.
#[allow(dead_code)]
fn unused_keys() -> [&'static str; 2] {
    [BACKSPACE, ESC]
}
