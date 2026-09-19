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

use crate::{
    BACKSPACE, DOWN, ENTER, ESC, LEFT, RIGHT, SHIFT_LEFT, SHIFT_RIGHT, UP,
};

/// The pane rows the U2 scenarios witness, in the renderer's own shape:
/// the committed row carries `▶`, the selected row carries `>`, a row may
/// carry both, then the 1-based traversal position and the file name
/// (Issue #166 §20 — shapes, not colours; the pane is WYSIWYG, so a row
/// sits at the position `N` would play next).
///
/// The markers are U+25B6 / ASCII `>` — one grid cell each, so the
/// emulated capture reproduces them verbatim (U2 smoke transcript).
fn pane_row(playing: bool, selected: bool, position: usize, label: &str) -> String {
    let playing = if playing { "▶" } else { " " };
    let selected = if selected { ">" } else { " " };
    format!("{playing} {selected} {position:>3}  {label}")
}

/// The committed row when it is ALSO the selection — the state startup
/// reaches and the state every selection-following navigation reaches.
fn playing_selected_row(position: usize, label: &str) -> String {
    pane_row(true, true, position, label)
}

/// The committed row with the selection browsing elsewhere.
fn playing_only_row(position: usize, label: &str) -> String {
    pane_row(true, false, position, label)
}

/// The browsed row with the committed cursor elsewhere (possibly
/// scrolled out of the viewport).
fn selected_only_row(position: usize, label: &str) -> String {
    pane_row(false, true, position, label)
}

/// One pane row WITHOUT any marker — the needle for "this row left the
/// viewport". Deliberately built from the position field, not the bare
/// file name: the Source line carries the same file name, so a bare
/// label would match a frame in which the row is long gone.
fn pane_row_label(position: usize, label: &str) -> String {
    format!("{position:>3}  {label}")
}


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
    /// Like [`Step::ExpectExit`], but either code satisfies: for a
    /// damaged-input scenario whose terminal the authority may settle
    /// EITHER way (an early-EOF Completed or a decode Failed), the
    /// honest gate is "committed truthfully and reported", not one
    /// specific outcome.
    ExpectExitEither {
        a: u32,
        b: u32,
        within_ms: u64,
    },
    /// Resize the pseudoconsole (Stage-C closure C9): the runtime has
    /// no resize-specific code — the next draw picks up the new size —
    /// so the scenario resizes, then presses a key (which triggers the
    /// harness's full-repaint jiggle) before any expect. Post-shrink
    /// expects must stay LEFT-ANCHORED: the captured grid keeps the
    /// spawn width, so columns beyond a shrunk frame legitimately hold
    /// stale content until the frame grows back.
    Resize {
        cols: i16,
        rows: i16,
    },
    /// Like [`Step::ExpectAfterMark`], but while waiting it forces a
    /// full repaint every ~500 ms.
    ///
    /// This exists for ASYNCHRONOUS transitions (an automatic EOF
    /// advance, which no key press triggers): the app's stderr
    /// (ffmpeg/wasapi mechanism lines) interleaves with the TUI on the
    /// pseudoconsole and can overwrite cells of the very row that just
    /// changed — and because the application's own view of those cells
    /// is still correct, its diff renderer never rewrites them, so the
    /// pollution persists until a repaint happens. The witness is
    /// UNCHANGED (the text must really have been rendered after the
    /// mark); only the capture-side repaint the harness already applies
    /// after every key write is applied here too.
    ExpectAfterMarkRepaint {
        text: String,
        within_ms: u64,
    },
    /// Force one fresh full frame at the CURRENT size (the same
    /// width-jiggle the harness already performs after every key write),
    /// without sending a key or changing any application state.
    ///
    /// This exists for the honest negative oracle: an absence witness is
    /// only meaningful over a window in which the screen was actually
    /// re-rendered, and an idle episode's diff renderer legitimately
    /// emits no new frame while nothing changes. Without a repaint the
    /// window would be vacuous, so U2's "no auto-advance" claims repaint
    /// first and then assert absence over real frames.
    Repaint,
}

const SHORT_WAIT: u64 = 5_000;
const OPEN_WAIT: u64 = 20_000;
const EOF_WAIT: u64 = 30_000;

/// The 24-entry viewport list (staged by tools/run-tui.sh as renamed
/// copies of the 45 s synthetic sine, so every entry is long enough that
/// the scenario's browsing and its single Enter stay inside one
/// episode).
const VVIEWPORT: [&str; 24] = [
    "vtest01.mp3", "vtest02.mp3", "vtest03.mp3", "vtest04.mp3", "vtest05.mp3", "vtest06.mp3",
    "vtest07.mp3", "vtest08.mp3", "vtest09.mp3", "vtest10.mp3", "vtest11.mp3", "vtest12.mp3",
    "vtest13.mp3", "vtest14.mp3", "vtest15.mp3", "vtest16.mp3", "vtest17.mp3", "vtest18.mp3",
    "vtest19.mp3", "vtest20.mp3", "vtest21.mp3", "vtest22.mp3", "vtest23.mp3", "vtest24.mp3",
];

/// The 20-entry soak list (staged by tools/run-tui.sh as locally
/// generated 100 s synthetic sine tracks under `u2soak\`).
const SOAK_LIST: [&str; 20] = [
    "u2soak\\soak01.mp3", "u2soak\\soak02.mp3", "u2soak\\soak03.mp3", "u2soak\\soak04.mp3",
    "u2soak\\soak05.mp3", "u2soak\\soak06.mp3", "u2soak\\soak07.mp3", "u2soak\\soak08.mp3",
    "u2soak\\soak09.mp3", "u2soak\\soak10.mp3", "u2soak\\soak11.mp3", "u2soak\\soak12.mp3",
    "u2soak\\soak13.mp3", "u2soak\\soak14.mp3", "u2soak\\soak15.mp3", "u2soak\\soak16.mp3",
    "u2soak\\soak17.mp3", "u2soak\\soak18.mp3", "u2soak\\soak19.mp3", "u2soak\\soak20.mp3",
];

/// One 100 s soak track plus the replacement's own overhead.
const SOAK_WAIT: u64 = 150_000;

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


/// One forced full frame — see [`Step::Repaint`].
fn repaint() -> Step {
    Step::Repaint
}

/// A post-mark presence witness for an ASYNCHRONOUS transition, with
/// capture-side repainting while it waits (see
/// [`Step::ExpectAfterMarkRepaint`]).
fn expect_after_mark_async(text: impl Into<String>, within_ms: u64) -> Step {
    Step::ExpectAfterMarkRepaint {
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

fn expect_mark_within(text: impl Into<String>, within_ms: u64) -> Step {
    Step::ExpectAfterMark {
        text: text.into(),
        within_ms,
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

/// The absolute staged path as the player renders it in OPERATION
/// FEEDBACK (`play:` / `next:` / `auto-next:` echo the committed source
/// path, unlike the O-line, which echoes what the user typed).
fn abs_path(media: &str, file: &str) -> String {
    format!("{media}\\{file}")
}

fn next_opened(media: &str, file: &str) -> String {
    format!("next: opened {media}\\{file}")
}

/// The captured grid renders a wide glyph followed by its skip cell
/// (one blank); ASCII glyphs occupy one cell with no skip cell. So the
/// typed CJK source `千曲.flac` appears on the grid as
/// `千 曲 .flac` — blanks only after the wide glyphs, `.flac`
/// contiguous. Needles below use that exact rendered form (calibrated
/// against the run-PROBE3 transcript; H-11 grid model).

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
        // U1 (Issue #166) idle-launch scenarios: NO argv at all — the
        // no-argument product startup is itself the scenario (the
        // driver spawns the child with zero arguments for an empty
        // list).
        "U1-idle" | "U1-folder-open" => &[],
        // U2 (Issue #166 §51) playlist matrix. Three 4 s fixtures keep
        // the cursor/navigation scenarios inside the watchdog; the
        // EOF scenarios pick their own order because the LAST entry's
        // completion is what "Repeat Off stays" observes.
        "U2-pane" | "U2-select" | "U2-enter" | "U2-nav" | "U2-order"
        | "U2-repeat-labels" => &["flac4.flac", "mp3cbr.mp3", "alac4.m4a"],
        // The EOF scenarios keep their TRANSITION targets on non-MP3
        // sources: an MP3 open streams ffmpeg decoder warnings onto the
        // pseudoconsole and can hold the feedback row polluted for
        // longer than a 4 s episode lasts (the harness repairs by
        // repainting, but a stable episode gives it a quiet window).
        "U2-eof-advance" => &["flac4.flac", "synth30.flac", "alac4.m4a"],
        "U2-eof-all-wrap" | "U2-eof-one-replays" => &["flac4.flac", "alac4.m4a"],
        // Entry 2 completes LAST here, which is where "Repeat Off
        // stays" must be observed.
        "U2-eof-stays" => &["alac4.m4a", "flac4.flac"],
        "U2-stop-no-advance" => &["flac4.flac", "alac4.m4a"],
        "U2-seek-30" | "U2-goto" | "U2-help" => &["synth45.mp3"],
        // The ONE shuffle grammar (flag immediately after the
        // subcommand).
        "U2-shuffle-start" => &["--shuffle", "flac4.flac", "mp3cbr.mp3", "alac4.m4a"],
        "U2-cjk" => &["千曲.flac", "flac4.flac"],
        "U2-viewport" => &VVIEWPORT,
        "U2-soak" => &SOAK_LIST,

        "A1-flac4" | "A16-drain-stop" => &["flac4.flac"],
        "A1-mp3cbr" => &["mp3cbr.mp3"],
        "A1-alac4" => &["alac4.m4a"],
        "A1-alac6" => &["alac6.m4a"],
        // Startup includes one deliberately-invalid explicit file; the
        // Listening-Release scan rejects it before the playlist seeds.
        "A15" => &["synth45.mp3", "garbage.bin", "flac4.flac"],
        // Track durations cover the full navigation walk (~10 s).
        "A14" => &["synth30.flac", "flac4.flac", "synth45.mp3"],
        // Listening-Release physical gates (LR1): the F-matrix /
        // dedup / large-list folders staged by the campaign's runner
        // under the media root (fixtures + declared synthetic files,
        // shapes recorded in the run ENV file).
        "LR1-folder-mixed" => &["fmatrix"],
        "LR1-all-corrupt" => &["corrupt"],
        "LR1-duplicate-roots" => &["dup", "dup"],
        "LR1-truncated-next" => &["trunc"],
        "LR1-large" => &["big1000"],
        "LR1-huge" => &["big5000"],
        // Soak: every occupied episode outlasts its script segment.
        "A20" => &["synth45.mp3", "synth30.flac", "mp3cbr.mp3"],
        // Stage-C closure scenarios: a >112-char filename (the Source
        // line clips at the 120-col grid) and a CJK filename.
        "C11-longpath" => &[
            "qianqian-longpath-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.flac",
        ],
        "C12-cjk" => &["千曲.flac"],
        _ => &["synth45.mp3"],
    };
    let watchdog = if name == "U2-soak" {
        // 20 x 100 s of real playback plus transition overhead.
        Duration::from_secs(2_400)
    } else if name == "A20" {
        Duration::from_secs(300)
    } else if name == "A19" {
        Duration::from_secs(200)
    } else if name == "LR1-huge" {
        // 5,000-file scan (enumerate + probe each) plus playback start.
        Duration::from_secs(300)
    } else if name == "LR1-large" {
        Duration::from_secs(180)
    } else {
        Duration::from_secs(120)
    };
    let steps = match name {
        // U1-idle — the no-argument launch (Windows gate W1): the idle
        // page renders truthfully (no fabricated Position/Paused/
        // Terminal labels for an episode that does not exist) and Q
        // exits the session cleanly with the idle exit contract
        // (code 0, no outcome line, quiet disposal). U1 corrective
        // REQUIRED-1: the frame also carries NO operation feedback —
        // the false-refusal vocabulary for an Open that was never
        // attempted is explicitly rejected.
        "U1-idle" => vec![
            expect_within("No music loaded.", 10_000),
            expect("Press O to open a file or folder"),
            Step::Mark,
            Step::AbsentAfterMark("Position:".to_owned()),
            Step::AbsentAfterMark("Paused:".to_owned()),
            Step::AbsentAfterMark("Terminal:".to_owned()),
            Step::AbsentAfterMark("open refused".to_owned()),
            Step::AbsentAfterMark("no audio candidates".to_owned()),
            keys("q"),
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            Step::AbsentAfterMark("teardown violated".to_owned()),
            Step::AbsentAfterMark("warning: disposal".to_owned()),
        ],

        // U1-folder-open — folder expansion from the idle page (Windows
        // gate W3/W4 mechanics on real media, position witnesses not
        // audibility): type a FOLDER path into the O line, the
        // expansion commits the first candidate and seeds both, N walks
        // the seeded list through the same replacement.
        "U1-folder-open" => {
            let folder = format!("{media}\\u1music");
            let mut v = vec![
                expect_within("No music loaded.", 10_000),
                keys("o"),
                Step::Typed(folder.clone()),
                keys(ENTER),
                // The expansion commits: first candidate opened, both
                // candidates seeded, format published, PCM consumed.
                expect_mark_within(
                    format!("opened {folder}\\flac4.flac (2 candidates)"),
                    10_000,
                ),
                expect("Track: 1/2"),
                expect_format(),
                new_position(),
                // N walks the seeded list through the SAME replacement.
                keys("n"),
                expect_mark_within(format!("{folder}\\synth45.mp3"), 10_000),
                expect("Track: 2/2"),
                expect_format(),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

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
                // still rendered on screen. (Listening-Release note: the
                // O-key expansion now refuses a missing path at the
                // ENUMERATION step, so the needle reads `cannot read`,
                // not the decode layer's `cannot open`.)
                expect_mark("cannot read missing-file.flac"),
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

        // A15 — startup hardening under the Listening-Release scan
        // contract: an explicitly named invalid file is PROBE-REJECTED
        // at scan time (never enters the playlist, reported boundedly
        // under `not playable:`), the folder still opens its real
        // tracks, and navigation walks the two surviving entries with
        // inert boundaries. (The pre-campaign A15 pinned a refused
        // candidate INSIDE the playlist; scan-time rejection now keeps
        // it out — the runtime-failed-track-no-skip policy is pinned
        // by the machine transport's failure scenarios instead.)
        "A15" => vec![
            expect("Track: 1/2"),
            expect_format(),
            expect("not playable: garbage.bin"),
            Step::Mark,
            new_position(),
            keys("n"),
            expect_mark(next_opened(media, "flac4.flac")),
            expect_mark("Track: 2/2"),
            keys("n"),
            expect_mark("no next track"),
            expect_mark("Track: 2/2"),
            keys("p"),
            expect_mark(prev_opened(media, "synth45.mp3")),
            expect_mark("Track: 1/2"),
            keys("p"),
            expect_mark("no previous track"),
            expect_mark("Track: 1/2"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // LR1-folder-mixed — the F-matrix folder: real tracks (flat,
        // nested, CJK, long-name), quiet non-audio noise, one renamed
        // garbage "track", one zero-byte audio file, and one
        // access-denied subfolder. The scan keeps the five real
        // candidates, counts the noise, reports the two corrupt files
        // boundedly, and the partial scan never presents as complete.
        "LR1-folder-mixed" => vec![
            expect("scanning"),
            expect_format(),
            expect("5 candidates, 5 skipped, 2 unplayable"),
            expect("not playable: broken.flac"),
            expect("not playable: zero.mp3"),
            expect("scan warning: cannot read"),
            expect("Track: 1/5"),
            expect(playing_selected_row(1, "01-track.flac")),
            Step::Mark,
            new_position(),
            keys(DOWN),
            expect_mark("sel 2/5"),
            expect_mark(selected_only_row(2, "02-track.m4a")),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // LR1-all-corrupt — every audio-looking file in the folder
        // fails the probe: the honest refusal, an idle-but-alive
        // shell, and the argv-driven exit contract (visible refusal,
        // exit 1).
        "LR1-all-corrupt" => vec![
            expect("open refused"),
            expect("no playable audio files found; 2 unplayable"),
            expect("No music loaded."),
            keys("q"),
            Step::ExpectExit {
                code: 1,
                within_ms: 15_000,
            },
            Step::AbsentAfterMark("teardown violated".to_owned()),
        ],

        // LR1-duplicate-roots — the same folder named twice on argv:
        // every accepted path once (first occurrence order), the
        // duplicates counted, playback and navigation normal.
        "LR1-duplicate-roots" => vec![
            expect_format(),
            expect("2 candidates, 2 duplicates removed"),
            expect("Track: 1/2"),
            Step::Mark,
            new_position(),
            keys("n"),
            expect_mark("Track: 2/2"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        // LR1-truncated-next — a track whose container header parses
        // (probe passes at scan time) but whose stream is cut roughly
        // in half: navigation reaches it, the episode commits SOME
        // truthful terminal (an early-EOF Completed or a decode
        // Failed — both legal for damaged input), there is no panic,
        // no auto-skip and no teardown violation. Which terminal the
        // authority settles is recorded by the run, not assumed.
        "LR1-truncated-next" => vec![
            expect_format(),
            expect("2 candidates"),
            expect("Track: 1/2"),
            Step::Mark,
            new_position(),
            keys("n"),
            expect_mark("Track: 2/2"),
            Step::ExpectEither {
                a: "Terminal: Completed".to_owned(),
                b: "Terminal: Failed".to_owned(),
                within_ms: 30_000,
            },
            keys("q"),
            Step::ExpectExitEither {
                a: 0,
                b: 1,
                within_ms: 15_000,
            },
            Step::ExpectEither {
                a: "EOF: played out completely".to_owned(),
                b: "playback failed".to_owned(),
                within_ms: SHORT_WAIT,
            },
            Step::AbsentAfterMark("teardown violated".to_owned()),
        ],

        // LR1-large / LR1-huge — 1,000- and 5,000-entry playlists:
        // the scan completes in bounded practical time, the Track
        // line carries the real count, the viewport windows around
        // the selection, and browsing works (no 5,000-row redraw per
        // frame is pinned by the unit suite; this is the physical
        // usability witness). Wall-clock timing is recorded by the
        // campaign runner around the whole run.
        "LR1-large" => vec![
            expect("1000 candidates"),
            expect_format(),
            expect("Track: 1/1000"),
            Step::Mark,
            new_position(),
            Step::KeysEach(DOWN, 5, 100),
            expect_mark("sel 6/1000"),
            keys(UP),
            expect_mark("sel 5/1000"),
        ]
        .into_iter()
        .chain(quit_clean())
        .collect(),

        "LR1-huge" => vec![
            expect_mark_within("5000 candidates", 240_000),
            expect_format(),
            expect("Track: 1/5000"),
            Step::Mark,
            new_position(),
            Step::KeysEach(DOWN, 3, 100),
            expect_mark("sel 4/5000"),
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

        // C6 — Open input UX (Stage-C closure): editing (push +
        // backspace), Esc cancel, and the empty-line Enter cancel are
        // all INERT — no Open fires, the seeded episode keeps
        // consuming (F6: cancel preserves the old episode) — and a
        // real open still completes afterwards through the same line.
        "C6-open-cancel" => {
            let mut v = vec![
                expect_format(),
                Step::Mark,
                keys("o"),
                Step::Typed("synth30".to_owned()),
                keys(BACKSPACE),              // synth3
                Step::Typed("99".to_owned()), // synth399
                keys(BACKSPACE),              // synth39
                keys(BACKSPACE),              // synth3
                keys(ESC),                    // cancel: inert, nothing opens
                Step::SleepMs(500),
                Step::AbsentAfterMark("opened ".to_owned()),
                keys("o"),
                keys(ENTER), // empty line confirms nothing
                Step::SleepMs(500),
                Step::AbsentAfterMark("opened ".to_owned()),
                // Frozen-episode witness: baseline the newest position
                // sample right here, then re-scope the window with a
                // fresh mark and require a DIFFERENT sample inside it
                // — a live episode's label advances; an episode frozen
                // at its first sample only ever re-appends the stale
                // value and cannot pass (F2, review of Stage C; the
                // run-PROBE5/6 shapes showed both failure modes: a
                // too-early baseline records nothing, and an unscoped
                // wait matches the EARLY post-mark samples).
                Step::RecordPosition,
                Step::Mark,
                new_position(),
            ];
            v.extend(open_candidate("synth30.flac"));
            v.push(expect_mark(opened_feedback("synth30.flac")));
            v.push(expect_mark(source_typed("synth30.flac")));
            v.extend(quit_clean());
            v
        }

        // C9 — terminal resize (Stage-C closure): shrink, interact,
        // grow back. The runtime has no resize-specific code (the next
        // draw picks up the new size); the scenario proves the shell
        // stays interactive and truthful at 80x24 — every label still
        // physically fits there — and after growing back. The extreme
        // shrink sizes (down to 4x3) are pinned no-panic/no-fabrication
        // by the view unit tests; at 40x12 the main panel clips to one
        // row and the episode labels legitimately cannot render, so
        // they are NOT valid oracles there (run-PROBE layout lesson).
        "C9-resize" => {
            let mut v = vec![
                expect_format(),
                Step::Resize { cols: 80, rows: 24 },
                keys(" "), // pause — and the key press forces a full repaint at 80x24
                expect_within("Pause requested: true", 8_000),
                keys(" "), // resume
                expect_within("Pause requested: false", 8_000),
                Step::Resize {
                    cols: 120,
                    rows: 40,
                },
                // Mark AFTER the grow-back: the Volume needle must be
                // witnessed by a post-grow-back frame, not a spawn-time
                // frame (F3, review of Stage C).
                Step::Mark,
                keys("x"), // unbound noise key → full repaint at full width
                expect_mark_within("Volume: 100/100 (desired)", 8_000),
                Step::AbsentAfterMark("teardown violated".to_owned()),
            ];
            v.extend(quit_clean());
            v
        }

        // C11 — very long source path (Stage-C closure): the open
        // succeeds end-to-end and the Source/feedback lines clip at
        // the panel edge without corrupting the rows below or the
        // controls panel. Needles are the STABLE PREFIXES of the
        // clipped lines (the internal path identity is never truncated
        // — pinned by the view unit tests); after the open, controls
        // and episode commands still work.
        "C11-longpath" => {
            let mut v = vec![
                expect_format(),
                Step::Mark,
                keys("o"),
                Step::Typed(
                    "qianqian-longpath-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.flac".to_owned(),
                ),
                keys(ENTER),
                Step::ExpectAfterMark {
                    text: "opened qianqian-longpath".to_owned(),
                    within_ms: OPEN_WAIT,
                },
                Step::ExpectAfterMark {
                    text: "Source: qianqian-longpath".to_owned(),
                    within_ms: OPEN_WAIT,
                },
                // Mark BEFORE the stop: both post-stop checks must be
                // witnessed by post-stop frames, not spawn-time ones
                // ("N/P  Next/Prev" renders in the spawn controls panel
                // too) (F3, review of Stage C). The controls line is the
                // U2 frozen keymap (Issue #166 §30).
                Step::Mark,
                keys("s"),
                expect_mark_within("Terminal: Stopped", SHORT_WAIT),
                expect_mark_within("N/P  Next/Prev", SHORT_WAIT),
                Step::AbsentAfterMark("teardown violated".to_owned()),
            ];
            v.extend(quit_clean());
            v
        }

        // C12 — CJK filename (Stage-C closure): a real CJK-named
        // fixture opens, renders, pauses/resumes, and quits cleanly.
        // The needles are the H-11-calibrated captured form (wide
        // glyphs interleaved with their skip blanks); the property
        // under test is structural correctness, not typography.
        "C12-cjk" => {
            let mut v = vec![
                expect_format(),
                Step::Mark,
                keys("o"),
                Step::Typed("千曲.flac".to_owned()),
                keys(ENTER),
                expect_mark("opened 千 曲 .flac"),
                keys(" "),
                expect("Pause requested: true"),
                keys(" "),
                expect("Pause requested: false"),
                expect_mark("Source: 千 曲 .flac"),
                Step::AbsentAfterMark("teardown violated".to_owned()),
            ];
            v.extend(quit_clean());
            v
        }

        // C17 — Ctrl+C keeps its conventional quit meaning on the real
        // terminal (raw mode delivers it as a key event, routed to the
        // same Quit action as Q): bounded exit, honest quit report,
        // quiet teardown.
        "C17-ctrlc" => vec![
            expect_format(),
            Step::SleepMs(1_000),
            keys("\x03"),
            Step::ExpectExit {
                code: 0,
                within_ms: 15_000,
            },
            expect("stopped before completion"),
            Step::AbsentAfterMark("teardown violated".to_owned()),
            Step::AbsentAfterMark("warning: disposal".to_owned()),
        ],

        // U2 (Issue #166 §51) — the playlist / order / repeat / EOF /
        // seek matrix on the real Windows host. Oracles are the shell's
        // own truth-class-pinned projections (the App's navigation
        // cursor, the order/repeat labels, the operation feedback), the
        // pane's rendered rows, NEW published Position samples and
        // clean-exit witnesses. Audibility is NOT claimed by any of
        // them; the acoustic witness stays UNAVAILABLE.
        // -----------------------------------------------------------

        // U2-pane — the playlist pane renders the startup list with
        // file-name labels, and the committed cursor is the FIRST
        // accepted candidate (the startup discipline is untouched).
        "U2-pane" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/3"),
                expect("Order: Sequential"),
                expect("Repeat: Off"),
                // The pane lists all three staged sources by file name.
                expect("flac4.flac"),
                expect("mp3cbr.mp3"),
                expect("alac4.m4a"),
                expect(playing_selected_row(1, "flac4.flac")),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-select — browsing the pane NEVER changes playback: the
        // committed cursor, the Source line and the playing row stay on
        // entry 1 while the selection walks down and back up.
        "U2-select" => {
            let committed = source_abs(media, "flac4.flac");
            let mut v = vec![
                expect_format(),
                expect("Track: 1/3"),
                Step::Mark,
                keys(DOWN),
                // Two INDEPENDENT markers on the real screen: the
                // committed row keeps `▶` with no selection, the browsed
                // row carries `>` with no play marker.
                expect_after_mark_async(playing_only_row(1, "flac4.flac"), SHORT_WAIT),
                expect_after_mark_async(selected_only_row(2, "mp3cbr.mp3"), SHORT_WAIT),
                keys(DOWN),
                keys(DOWN),
                keys(UP),
                keys(UP),
                keys(UP),
                // The episode and the committed cursor are untouched.
                expect_after_mark_async(committed, SHORT_WAIT),
                expect_after_mark_async("Track: 1/3".to_owned(), SHORT_WAIT),
                expect_after_mark_async(playing_selected_row(1, "flac4.flac"), SHORT_WAIT),
                // Browsing produced no playback transition of any kind:
                // neither an automatic one nor a navigation one (both
                // would name the source they opened).
                Step::AbsentAfterMark(format!("auto-next: opened {}", abs_path(media, "flac4.flac"))),
                Step::AbsentAfterMark(next_opened(media, "mp3cbr.mp3")),
                Step::AbsentAfterMark(prev_opened(media, "alac4.m4a")),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-enter — Enter plays the SELECTED row through the same Open
        // replacement (mp3cbr.mp3 is entry 2) and the committed cursor
        // follows it.
        "U2-enter" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/3"),
                Step::Mark,
                keys(DOWN),
                keys(ENTER),
                expect_after_mark_async(format!("play: opened {}", abs_path(media, "mp3cbr.mp3")), OPEN_WAIT),
                expect_after_mark_async("Track: 2/3", OPEN_WAIT),
                expect_after_mark_async(playing_selected_row(2, "mp3cbr.mp3"), OPEN_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-nav — N/P walk the traversal, and at the boundary with
        // Repeat Off the navigation is INERT (no probe, no command, no
        // status change): the key reports nothing and nothing moves.
        "U2-nav" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/3"),
                Step::Mark,
                keys("n"),
                expect_after_mark_async("Track: 2/3", OPEN_WAIT),
                keys("n"),
                expect_after_mark_async("Track: 3/3", OPEN_WAIT),
                expect_after_mark_async(playing_selected_row(3, "alac4.m4a"), OPEN_WAIT),
                // At the last entry under Repeat Off: inert. The
                // witness is a wrap-shaped transition's own feedback,
                // which only a boundary bug could produce, over frames
                // the key press and the repaint actually emitted.
                Step::Mark,
                keys("n"),
                repaint(),
                expect_after_mark_async("Track: 3/3".to_owned(), SHORT_WAIT),
                expect_after_mark_async(playing_selected_row(3, "alac4.m4a"), SHORT_WAIT),
                expect_after_mark_async(source_abs(media, "alac4.m4a"), SHORT_WAIT),
                Step::AbsentAfterMark(next_opened(media, "flac4.flac")),
                Step::AbsentAfterMark(next_opened(media, "mp3cbr.mp3")),
                // P walks back one traversal position at a time.
                keys("p"),
                expect_after_mark_async("Track: 2/3", OPEN_WAIT),
                expect_after_mark_async(playing_selected_row(2, "mp3cbr.mp3"), OPEN_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-order — R toggles Sequential ↔ Shuffle: the committed entry
        // never changes, the label says "Shuffle" (never "Random"), and
        // toggling back restores the canonical order with the cursor
        // still on the same entry.
        "U2-order" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/3"),
                Step::Mark,
                keys("r"),
                expect_after_mark_async("Order: Shuffle".to_owned(), SHORT_WAIT),
                // The committed entry is unchanged by the reorder.
                expect_after_mark_async("Track: 1/3".to_owned(), SHORT_WAIT),
                expect_after_mark_async(source_abs(media, "flac4.flac"), SHORT_WAIT),
                Step::AbsentAfterMark("Random".to_owned()),
                keys("r"),
                expect_after_mark_async("Order: Sequential", OPEN_WAIT),
                expect_after_mark_async("Track: 1/3".to_owned(), SHORT_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-repeat-labels — L cycles Off → All → One → Off and the
        // traversal never moves.
        "U2-repeat-labels" => {
            let mut v = vec![
                expect_format(),
                expect("Repeat: Off"),
                Step::Mark,
                keys("l"),
                expect_after_mark_async("Repeat: All".to_owned(), SHORT_WAIT),
                keys("l"),
                expect_after_mark_async("Repeat: One".to_owned(), SHORT_WAIT),
                keys("l"),
                expect_after_mark_async("Repeat: Off".to_owned(), SHORT_WAIT),
                expect_after_mark_async("Track: 1/3".to_owned(), SHORT_WAIT),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-eof-advance — the natural-EOF policy on real media: with
        // Repeat Off a completed entry advances the traversal exactly one
        // position through the SAME Open replacement, and the committed
        // cursor follows the opened source.
        "U2-eof-advance" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/3"),
                Step::Mark,
                // The transition ITSELF is the witness (only the EOF
                // policy prints `auto-next:`, and it names the entry it
                // opened). The episode's own transient `Terminal:
                // Completed` label is deliberately NOT awaited here: the
                // replacement commits within a frame or two of it, so
                // waiting for it would race the very thing under test.
                expect_after_mark_async(
                    format!("auto-next: opened {}", abs_path(media, "synth30.flac")),
                    EOF_WAIT,
                ),
                expect_after_mark_async("Track: 2/3".to_owned(), SHORT_WAIT),
                expect_after_mark_async(playing_selected_row(2, "synth30.flac"), SHORT_WAIT),
                new_position(),
            ];
            v.extend(quit_clean_either_report());
            v
        }

        // U2-eof-stays — at the END of the traversal with Repeat Off the
        // completed entry STAYS completed: no further transition and no
        // skip cascade. The absence window is proven non-vacuous by the
        // forced repaint over it.
        "U2-eof-stays" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/2"),
                // Entry 1 completes and advances (the ordinary policy).
                expect_after_mark_async(
                    format!("auto-next: opened {}", abs_path(media, "flac4.flac")),
                    EOF_WAIT,
                ),
                expect_after_mark_async("Track: 2/2".to_owned(), SHORT_WAIT),
                // Entry 2 is the END of the traversal: its completion
                // must stay put.
                expect_within("Terminal: Completed", EOF_WAIT),
                Step::Mark,
                repaint(),
                Step::SleepMs(6_000),
                repaint(),
                // A wrap (or any further transition) would have opened
                // entry 1 and named it here.
                Step::AbsentAfterMark(format!("auto-next: opened {}", abs_path(media, "alac4.m4a"))),
                expect_after_mark_async("Track: 2/2".to_owned(), SHORT_WAIT),
                expect_after_mark_async(source_abs(media, "flac4.flac"), SHORT_WAIT),
                expect_after_mark_async(playing_selected_row(2, "flac4.flac"), SHORT_WAIT),
            ];
            v.extend(quit_clean_reporting("EOF: played out completely"));
            v
        }

        // U2-eof-all-wrap — Repeat All wraps at the end of the traversal:
        // each entry re-opens through the same replacement and the cursor
        // returns to 1/2. (Two 4 s fixtures keep the wrap inside the
        // watchdog.)
        "U2-eof-all-wrap" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/2"),
                keys("l"),
                expect("Repeat: All"),
                Step::Mark,
                expect_after_mark_async(
                    format!("auto-next: opened {}", abs_path(media, "alac4.m4a")),
                    EOF_WAIT,
                ),
                expect_after_mark_async("Track: 2/2".to_owned(), SHORT_WAIT),
                expect_after_mark_async(
                    format!("auto-next: opened {}", abs_path(media, "flac4.flac")),
                    EOF_WAIT,
                ),
                expect_after_mark_async("Track: 1/2".to_owned(), SHORT_WAIT),
                expect_after_mark_async(playing_selected_row(1, "flac4.flac"), SHORT_WAIT),
                new_position(),
            ];
            v.extend(quit_clean_either_report());
            v
        }

        // U2-eof-one-replays — Repeat One re-opens the completed entry
        // through the same replacement (the session is never taught to
        // loop), and manual N still navigates: Repeat One never traps the
        // user.
        "U2-eof-one-replays" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/2"),
                keys("l"),
                keys("l"),
                expect("Repeat: One"),
                Step::Mark,
                expect_after_mark_async(
                    format!("auto-next: opened {}", abs_path(media, "flac4.flac")),
                    EOF_WAIT,
                ),
                expect_after_mark_async("Track: 1/2".to_owned(), SHORT_WAIT),
                // Manual N under Repeat One is an ordinary traversal step.
                keys("n"),
                expect_after_mark_async("Track: 2/2", OPEN_WAIT),
                expect_after_mark_async(playing_selected_row(2, "alac4.m4a"), OPEN_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-stop-no-advance — a Stopped episode NEVER auto-advances:
        // S settles the episode, the cursor stays put, and no automatic
        // transition appears in a window a cascade would have used (the
        // 4 s fixtures complete well inside it, so a naive
        // terminal-driven advance would be caught here).
        "U2-stop-no-advance" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/2"),
                Step::Mark,
                keys("s"),
                expect_after_mark_async("Terminal: Stopped".to_owned(), 10_000),
                repaint(),
                Step::SleepMs(6_000),
                repaint(),
                // A terminal-driven advance would have opened entry 2
                // and named it; the 4 s fixtures complete well inside
                // this window, so the absence is not vacuous.
                Step::AbsentAfterMark(format!("auto-next: opened {}", abs_path(media, "alac4.m4a"))),
                expect_after_mark_async("Track: 1/2".to_owned(), SHORT_WAIT),
                expect_after_mark_async(source_abs(media, "flac4.flac"), SHORT_WAIT),
                expect_after_mark_async(playing_selected_row(1, "flac4.flac"), SHORT_WAIT),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-seek-30 — Shift+Right/Shift+Left are the ∓30 s steps of the
        // SAME seek command: the published Position jumps far past what
        // the 5 s step could reach and comes back, with liveness sampled
        // at each stop (never an audibility claim).
        "U2-seek-30" => {
            let mut v = vec![
                expect_format(),
                Step::SleepMs(3_000),
                Step::Mark,
                Step::KeysEach(SHIFT_RIGHT, 1, 300),
                // ~3 s in + 30 s forward: the published sample must be
                // past 00:30.
                expect_after_mark_async("Position: 00:3", OPEN_WAIT),
                new_position(),
                Step::KeysEach(SHIFT_LEFT, 1, 300),
                expect_after_mark_async("Position: 00:0", OPEN_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-goto — the exact-seek adapter: G opens the line, a typed
        // time is parsed by the shared reader and issued as the same seek
        // command; an unreadable token sends nothing and keeps the line
        // open; Esc leaves without a command.
        "U2-goto" => {
            let mut v = vec![
                expect_format(),
                expect("Go to"),
                keys("g"),
                expect("Go to:"),
                Step::Typed("0:20".to_owned()),
                keys(ENTER),
                expect_after_mark_async("seek requested: 00:20", OPEN_WAIT),
                expect_after_mark_async("Position: 00:2", OPEN_WAIT),
                // A malformed target: the line stays open, nothing is
                // sent, and the diagnostic is the shell's bounded one.
                keys("g"),
                Step::Typed("nonsense".to_owned()),
                keys(ENTER),
                expect_after_mark_async("Go to:".to_owned(), SHORT_WAIT),
                keys(ESC),
                Step::Mark,
                repaint(),
                Step::AbsentAfterMark("Go to:".to_owned()),
                // Esc left the episode alone: playback continues.
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-help — the help overlay lists exactly the shipped keymap and
        // closes again, leaving the episode untouched.
        "U2-help" => {
            let mut v = vec![
                expect_format(),
                expect("Shift+←/→"),
                Step::Mark,
                keys("?"),
                expect_after_mark_async("Qianqian keys", OPEN_WAIT),
                expect_after_mark_async("select the previous / next row".to_owned(), SHORT_WAIT),
                expect_after_mark_async("play the selected row".to_owned(), SHORT_WAIT),
                expect_after_mark_async("repeat: off / all / one".to_owned(), SHORT_WAIT),
                expect_after_mark_async("go to an exact position".to_owned(), SHORT_WAIT),
                // Not shipped, not advertised.
                Step::AbsentAfterMark("M3U".to_owned()),
                Step::AbsentAfterMark("mouse".to_owned()),
                Step::AbsentAfterMark("library".to_owned()),
                keys(ESC),
                Step::Mark,
                repaint(),
                Step::AbsentAfterMark("Qianqian keys".to_owned()),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-shuffle-start — `qianqian play --shuffle …` starts the list
        // in shuffle order: the FIRST accepted candidate is still what
        // opens (the safe start is untouched) and the shell says so.
        "U2-shuffle-start" => {
            let mut v = vec![
                expect_format(),
                expect("Order: Shuffle"),
                expect("Track: 1/3"),
                expect(playing_selected_row(1, "flac4.flac")),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-cjk — a CJK file name renders structurally in the pane and
        // the Source line (the label is the file name; no path is
        // rewritten and no metadata is read).
        "U2-cjk" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/2"),
                // The captured grid renders a wide glyph followed by its
                // skip cell (lesson 6/limitations: width-2 placements may
                // be off by one column), so the CJK label is witnessed in
                // its rendered form — the same convention C12-cjk uses.
                expect(playing_selected_row(1, "千 曲 .flac")),
                Step::Mark,
                keys("n"),
                expect_after_mark_async("Track: 2/2", OPEN_WAIT),
                expect_after_mark_async(source_abs(media, "flac4.flac"), OPEN_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-viewport — a 24-entry list is windowed: the pane keeps the
        // SELECTED row visible while the user walks the whole list, the
        // committed row is free to scroll away, and browsing stays inert
        // (the committed episode does not move). Enter then commits the
        // browsed row through the same Open path.
        "U2-viewport" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/24"),
                expect("vtest01.mp3"),
                Step::Mark,
                // 22 steps take the selection to the 24th row: the
                // window follows it, so the first rows (and the
                // committed row's own marker) scroll out of the pane.
                Step::KeysEach(DOWN, 22, 40),
                expect_after_mark_async(selected_only_row(23, "vtest23.mp3"), SHORT_WAIT),
                // Re-mark AFTER the scroll settled: the intermediate
                // frames of the walk legitimately still showed the first
                // rows, so only the settled window may be scanned for
                // their absence.
                Step::Mark,
                repaint(),
                Step::AbsentAfterMark(pane_row_label(1, "vtest01.mp3")),
                // Nothing played: the committed episode is still entry 1,
                // whose marker row the viewport has scrolled away.
                expect_after_mark_async("Track: 1/24".to_owned(), SHORT_WAIT),
                expect_after_mark_async(source_abs(media, "vtest01.mp3"), SHORT_WAIT),
                // Enter commits the browsed row through the Open path.
                keys(ENTER),
                expect_after_mark_async("Track: 23/24", OPEN_WAIT),
                expect_after_mark_async(playing_selected_row(23, "vtest23.mp3"), OPEN_WAIT),
                new_position(),
            ];
            v.extend(quit_clean());
            v
        }

        // U2-soak — the light soak (Issue #166 §52): 20 synthetic 100 s
        // tracks played END TO END through the natural-EOF policy with
        // Repeat Off, i.e. ~33 minutes of continuous real playback on the
        // real endpoint, with a published-Position liveness sample and a
        // bounded child-resource checkpoint at every transition. The
        // witness set is the shell's own: each transition names the
        // source it opened and the cursor advance, and the run ends at
        // the traversal end (Repeat Off stays) with a clean quit.
        "U2-soak" => {
            let mut v = vec![
                expect_format(),
                expect("Track: 1/20"),
                Step::Mark,
            ];
            for n in 2..=20u32 {
                v.push(expect_after_mark_async(
                    format!("auto-next: opened {media}\\u2soak\\soak{n:02}.mp3"),
                    SOAK_WAIT,
                ));
                v.push(expect_after_mark_async(format!("Track: {n}/20"), SHORT_WAIT));
                v.push(Step::ExpectNewPosition {
                    within_ms: SHORT_WAIT,
                });
                // A bounded resource checkpoint per transition: threads
                // and working set may not grow without bound across 20
                // replacement cycles (a tripwire, not a leak oracle).
                v.push(Step::Resources {
                    max_thread_delta: 6,
                    max_ws_mb: 400,
                });
            }
            // The traversal is over and Repeat Off leaves it over: mark,
            // force a frame, and verify nothing advances.
            v.push(expect_within("Terminal: Completed", SOAK_WAIT));
            v.push(Step::Mark);
            v.push(repaint());
            v.push(Step::SleepMs(8_000));
            v.push(repaint());
            // Only a wrap could print another transition line.
            v.push(Step::AbsentAfterMark(format!(
                "auto-next: opened {media}\\u2soak\\soak01.mp3"
            )));
            v.push(expect_after_mark_async("Track: 20/20".to_owned(), SHORT_WAIT));
            v.extend(quit_clean_reporting("EOF: played out completely"));
            v
        }

        other => panic!("unknown scenario {other}"),
    };
    (m.to_vec(), steps, watchdog)
}
