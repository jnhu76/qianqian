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
//!
//! Witness precision: label needles are path-qualified where a stale
//! screen could otherwise satisfy them (the frame history is
//! append-only, so ANY post-mark frame carries the whole screen —
//! a bare `opened` would match an older `previous: opened C:\...`
//! feedback still rendered). Refusal needles use the stable semantic
//! prefix `open refused` (or the OS-error wording only where the error
//! kind is itself deterministic, e.g. a missing path), never a
//! diagnostic we do not own.

use std::time::Duration;

use crate::{BACKSPACE, ENTER, ESC, LEFT, RIGHT};

#[derive(Debug)]
pub enum Step {
    /// Snapshot the output-stream offset; later `AfterMark` steps only
    /// see text emitted after it.
    Mark,
    Keys(&'static str),
    /// Runtime-string key text (typed O-dialog candidate paths).
    Typed(String),
    /// `keys` sent `times` times, `gap_ms` apart.
    KeysEach(&'static str, usize, u64),
    SleepMs(u64),
    /// Search the whole transcript.
    Expect {
        text: String,
        within_ms: u64,
    },
    /// Either exact label satisfies the wait (used where two terminal
    /// outcomes are both legal for the pressed timing — never to blur
    /// an authority-owned mapping).
    ExpectEither {
        a: String,
        b: String,
        within_ms: u64,
    },
    /// Search only text emitted after the last Mark.
    ExpectAfterMark {
        text: String,
        within_ms: u64,
    },
    /// A `Position: MM:SS` sample after the last Mark different from
    /// the last recorded one (liveness witness).
    ExpectNewPosition {
        within_ms: u64,
    },
    /// Record the newest post-mark sample without requiring change.
    RecordPosition,
    /// The text must NOT appear in the post-Mark window.
    AbsentAfterMark(String),
    /// Bounded child resource checkpoint (thread delta and working-set
    /// bound against baseline).
    Resources {
        max_thread_delta: u32,
        max_ws_mb: u64,
    },
    ExpectExit {
        code: u32,
        within_ms: u64,
    },
}

const SHORT_WAIT: u64 = 5_000;
const OPEN_WAIT: u64 = 20_000;
const EOF_WAIT: u64 = 30_000;

fn expect(text: impl Into<String>) -> Step {
    Step::Expect {
        text: text.into(),
        within_ms: SHORT_WAIT,
    }
}

/// The device-open/format window: a cold WASAPI open can exceed the
/// ordinary label wait, so the FIRST format line gets a wider bound.
fn expect_format() -> Step {
    expect_within("Format: 44100 Hz", 10_000)
}

fn expect_within(text: impl Into<String>, within_ms: u64) -> Step {
    Step::Expect {
        text: text.into(),
        within_ms,
    }
}

fn expect_mark(text: impl Into<String>) -> Step {
    Step::ExpectAfterMark {
        text: text.into(),
        within_ms: OPEN_WAIT,
    }
}

fn new_position() -> Step {
    Step::ExpectNewPosition {
        within_ms: SHORT_WAIT,
    }
}

fn keys(k: &'static str) -> Step {
    Step::Keys(k)
}

fn quit_clean() -> Vec<Step> {
    quit_clean_reporting("stopped before completion")
}

/// The quit report line depends on how the live episode settles: a
/// stop-key quit settles `Stopped` ("stopped before completion"); a
/// naturally-settled episode keeps its own terminal ("EOF: played out
/// completely" for Completed).
fn quit_clean_reporting(line: &'static str) -> Vec<Step> {
    vec![
        keys("q"),
        Step::ExpectExit {
            code: 0,
            within_ms: 15_000,
        },
        // The quit report on stdout, after terminal restore.
        expect(line),
        Step::AbsentAfterMark("teardown violated".to_owned()),
        Step::AbsentAfterMark("warning: disposal".to_owned()),
    ]
}

/// Quit after an episode whose terminal may legitimately be either
/// Completed or Stopped: the report line matches whichever the
/// authority committed.
fn quit_clean_either_report() -> Vec<Step> {
    vec![
        keys("q"),
        Step::ExpectExit {
            code: 0,
            within_ms: 15_000,
        },
        Step::ExpectEither {
            a: "EOF: played out completely".to_owned(),
            b: "stopped before completion".to_owned(),
            within_ms: SHORT_WAIT,
        },
        Step::AbsentAfterMark("teardown violated".to_owned()),
        Step::AbsentAfterMark("warning: disposal".to_owned()),
    ]
}

fn open_candidate(path: &str) -> Vec<Step> {
    vec![
        Step::Mark,
        keys("o"),
        // Step::Keys carries &'static tokens; typed candidate paths are
        // runtime strings only in the driver runner, so re-type as a
        // leak-free owned send via KeysEach(1).
        Step::Typed(path.to_owned()),
        keys(ENTER),
    ]
}

/// One committed Open round on the TUI: mark, O, type the path, Enter,
/// the `opened <path>` feedback, the new Source line, and proof the old
/// episode's Source line is no longer rendered after the commit. The
/// caller supplies the old Source line exactly as it is currently
/// rendered (typed form if that episode was itself O-opened, absolute
/// form if it was seeded from argv at startup).
fn open_round(path: &str, old_source_line: String) -> Vec<Step> {
    let mut v = open_candidate(path);
    v.push(expect_mark(opened_feedback(path)));
    v.push(expect_mark(source_typed(path)));
    // Re-mark AFTER the commit is witnessed: the pre-commit frames of
    // the first post-mark window legitimately still show the old
    // episode's Source line, and only post-commit frames may be
    // scanned for its absence.
    v.push(Step::Mark);
    v.push(Step::AbsentAfterMark(old_source_line));
    v
}

// ------------------------------------------------------- needle builders

/// The success feedback line for an O-open. The shell echoes the TYPED
/// path here (what the user entered), so the witness is the typed
/// relative form — verified against run-E transcripts. Navigation
/// feedback (`next:`/`previous:`) renders the absolute seeded path, so
/// the substring `opened <file>` cannot match it.
fn opened_feedback(file: &str) -> String {
    format!("opened {file}")
}

/// The Source panel line after an O-open of a relative path: the shell
/// renders the typed path, not a canonicalized one (run-E transcript:
/// the old absolute tail is erased).
fn source_typed(file: &str) -> String {
    format!("Source: {file}")
}

/// The Source panel line for an argv-seeded startup episode (absolute).
fn source_abs(media: &str, file: &str) -> String {
    format!("Source: {media}\\{file}")
}

fn next_opened(media: &str, file: &str) -> String {
    format!("next: opened {media}\\{file}")
}

fn prev_opened(media: &str, file: &str) -> String {
    format!("previous: opened {media}\\{file}")
}

/// The corpus. flac4/mp3cbr/alac4/alac6 are the repository's committed
/// fixtures (renamed only); synth45/synth30 are locally generated
/// synthetic media (ffmpeg sine), recorded by SHA256 in the run
/// environment files; garbage.bin is a deterministic invalid candidate.
/// Fixture LENGTHS are load-bearing: multi-step scripts must finish
/// inside the occupied episode's duration (run-D lesson: 4–5 s fixtures
/// under 20–60 s scripts EOF mid-scenario and falsify every later
/// expectation).
pub fn scenario(name: &str, media: &str) -> (Vec<&'static str>, Vec<Step>, Duration) {
    let m: &[&str] = match name {
        "A1-flac4" | "A16-drain-stop" => &["flac4.flac"],
        "A1-mp3cbr" => &["mp3cbr.mp3"],
        "A1-alac4" => &["alac4.m4a"],
        "A1-alac6" => &["alac6.m4a"],
        // Track 2 is the deliberately-invalid navigation candidate.
        "A15" => &["synth45.mp3", "garbage.bin", "flac4.flac"],
        // Track durations cover the full navigation walk (~10 s).
        "A14" => &["synth30.flac", "flac4.flac", "synth45.mp3"],
        // Soak: every occupied episode outlasts its script segment.
        "A20" => &["synth45.mp3", "synth30.flac", "mp3cbr.mp3"],
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
            let mut v = vec![expect_format(), new_position()];
            v.push(expect_within("Terminal: Completed", EOF_WAIT));
            v.push(expect("terminal outcome committed"));
            v.extend(quit_clean_reporting("EOF: played out completely"));
            v
        }

        // A2 — pause/resume incl. pause right after open and rapid
        // double presses (Space is a frozen toggle over fresh
        // observations; rapid presses land in defined intent states).
        "A2" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A3 — forward seeks: small/medium, then near-end, then natural
        // EOF validates terminal semantics after cutover.
        "A3" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("EOF: played out completely"),
        ],

        // A4 — backward seeks: legal backward discontinuity, then
        // monotone progression.
        "A4" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A5 — rapid seek: faster than playback progression; one seek
        // in flight; the rest inert. No wedge, playback continues.
        "A5" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A6 — pause × seek: pause intent survives the cut; paused
        // rebase mid-park; resume continues post-seek; and the reverse
        // timing (pause landing right after a seek).
        "A6" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A7 — volume walk 100→80→50→20→0→70 while playing; 0 must not
        // stop consumption; the label means the desired stream factor.
        "A7" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A8 — volume while paused, then resume.
        "A8" => vec![
            expect_format(),
            keys(" "),
            expect_within("Paused: true", 3_000),
            Step::KeysEach("-", 4, 100),
            expect("Volume: 80/100 (desired)"),
            expect("Paused: true"),
            keys(" "),
            expect("Pause requested: false"),
            new_position(),
            keys("q"),
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A9 — volume routed around seek cutovers; desired level never
        // lost, episode never reset.
        "A9" => vec![
            expect_format(),
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
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
        ],

        // A10 — Open valid while playing, repeatedly (A→B→A).
        "A10" => vec![expect_format(), Step::SleepMs(1_500)]
            .into_iter()
            .chain(open_round("synth30.flac", source_abs(media, "synth45.mp3")))
            .chain(open_round("synth45.mp3", source_typed("synth30.flac")))
            .chain(open_round("synth30.flac", source_typed("synth45.mp3")))
            .chain(quit_clean())
            .collect(),

        // A11 — Open invalid while old episode plays: refusal, old
        // episode untouched and still consuming, no replacement.
        "A11" => vec![expect_format(), Step::SleepMs(1_500)]
            .into_iter()
            .chain(open_candidate("garbage.bin"))
            .chain(vec![
                expect_mark("open refused"),
                Step::AbsentAfterMark(format!("opened {media}\\")),
            ])
            .chain(vec![Step::Mark, Step::SleepMs(1_500), new_position()])
            .chain(open_candidate("missing-file.flac"))
            .chain(vec![
                // The missing-path refusal wording is deterministic (os
                // error 2), so the path-qualified diagnostic is a stable
                // witness — and it cannot match the earlier garbage refusal
                // still rendered on screen.
                expect_mark("cannot open 'missing-file.flac'"),
                expect("Terminal: pending"),
            ])
            .chain(quit_clean())
            .collect(),

        // A12 — Open while paused: replacement commits; the fresh
        // episode starts unpaused (no episode-local pause carry).
        "A12" => vec![
            expect_format(),
            keys(" "),
            expect_within("Paused: true", 3_000),
        ]
        .into_iter()
        .chain(open_candidate("synth30.flac"))
        .chain(vec![
            expect_mark(opened_feedback("synth30.flac")),
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
            expect_format(),
            Step::SleepMs(2_000),
            Step::KeysEach(RIGHT, 1, 200),
            new_position(),
        ]
        .into_iter()
        .chain(open_candidate("synth30.flac"))
        .chain(vec![
            expect_mark(opened_feedback("synth30.flac")),
            expect_mark(source_typed("synth30.flac")),
            Step::Mark,
            new_position(),
            Step::Resources {
                max_thread_delta: 6,
                max_ws_mb: 300,
            },
        ])
        .chain(quit_clean())
        .collect(),

        // A14 — navigation forward/backward with inert boundaries. The
        // walk (six single-key steps) completes well inside track 1's
        // 30 s; the 4 s track 2 is occupied only ~1 s per visit.
        "A14" => vec![
            expect("Track: 1/3"),
            expect_format(),
            Step::Mark,
            keys("n"),
            expect_mark(next_opened(media, "flac4.flac")),
            expect_mark("Track: 2/3"),
            keys("n"),
            expect_mark(next_opened(media, "synth45.mp3")),
            expect_mark("Track: 3/3"),
            keys("n"),
            expect_mark("no next track"),
            expect_mark("Track: 3/3"),
            keys("p"),
            expect_mark(prev_opened(media, "flac4.flac")),
            expect_mark("Track: 2/3"),
            keys("p"),
            expect_mark(prev_opened(media, "synth30.flac")),
            expect_mark("Track: 1/3"),
            keys("p"),
            expect_mark("no previous track"),
            expect_mark("Track: 1/3"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // A15 — navigation failure: a refused candidate (garbage.bin as
        // track 2) moves nothing and never auto-skips; the playing
        // episode keeps consuming. The navigation refusal's stable
        // prefix is `next refused:` (run-E transcript: `next refused:
        // SongCore refused '<abs path>': status 104`).
        "A15" => vec![
            expect("Track: 1/3"),
            expect_format(),
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
            expect_format(),
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

        // A16-drain-stop — Stop late in the episode. The decode-EOF
        // drain window itself is sub-second and unreachable
        // deterministically from the TUI (1 Hz position granularity);
        // WHICH terminal the authority commits for a stop inside that
        // window is pinned by the D11 conformance suite (shared truth
        // table, exhaustive Rust/TLC byte-compare), not re-derived
        // here. This scenario witnesses what the UI owes the user: a
        // late stop settles EXACTLY ONE truthful terminal Fact and
        // quits cleanly.
        "A16-drain-stop" => vec![
            expect_format(),
            Step::SleepMs(3_200),
            keys("s"),
            Step::ExpectEither {
                a: "Terminal: Completed".to_owned(),
                b: "Terminal: Stopped".to_owned(),
                within_ms: 8_000,
            },
            expect("terminal outcome committed"),
        ]
        .into_iter()
        .chain(quit_clean_either_report())
        .collect(),

        // A16-paused — Stop while paused (mid-play): Stopped.
        "A16-paused" => vec![
            expect_format(),
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
            expect_format(),
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
        "A17" => vec![expect_format(), keys("s"), expect("Terminal: Stopped")]
            .into_iter()
            .chain(open_candidate("synth30.flac"))
            .chain(vec![
                expect_mark(opened_feedback("synth30.flac")),
                expect_mark(source_typed("synth30.flac")),
                Step::Mark,
                new_position(),
            ])
            .chain(quit_clean())
            .collect(),

        // A18 — Open → Stop: stop routes to the CURRENT episode.
        "A18" => vec![expect_format(), Step::SleepMs(1_500)]
            .into_iter()
            .chain(open_candidate("synth30.flac"))
            .chain(vec![
                expect_mark(opened_feedback("synth30.flac")),
                expect_mark(source_typed("synth30.flac")),
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
                v.push(expect_mark(opened_feedback(target)));
            }
            v.push(Step::Resources {
                max_thread_delta: 6,
                max_ws_mb: 300,
            });
            v.extend(quit_clean());
            v
        }

        // A20 — bounded soak mixing every earned control on the real
        // path; resource checkpoints at the end. Playlist durations are
        // chosen so every occupied episode outlasts its script segment
        // (45 s / 30 s / 4 s; the 4 s track is occupied only briefly).
        "A20" => {
            let mut v = vec![
                expect("Track: 1/3"),
                expect_format(),
                Step::SleepMs(1_000),
                keys("n"),
                expect_mark(next_opened(media, "synth30.flac")),
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
                expect_mark(next_opened(media, "mp3cbr.mp3")),
                expect_mark("Track: 3/3"),
            ];
            v.extend(open_candidate("garbage.bin"));
            v.push(expect_mark("open refused"));
            v.push(expect_mark("Track: 3/3"));
            v.push(keys("p"));
            v.push(expect_mark(prev_opened(media, "synth30.flac")));
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
            v.push(expect_mark(opened_feedback("synth30.flac")));
            v.push(Step::SleepMs(1_000));
            v.push(Step::KeysEach(RIGHT, 2, 200));
            v.push(new_position());
            v.push(keys("s"));
            v.push(expect("Terminal: Stopped"));
            v.extend(open_candidate("synth45.mp3"));
            v.push(expect_mark(opened_feedback("synth45.mp3")));
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

// Kept for symmetry with the runtime imports: the O-input Esc/Backspace
// affordances are exercised in Stage C quality passes, not A-matrix.
#[allow(dead_code)]
fn unused_keys() -> [&'static str; 2] {
    [BACKSPACE, ESC]
}
