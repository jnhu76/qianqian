//! Invocation-local machine input responsibility (execution-model D3/D5).
//! OS reads are outside effect admission: seal closes admission, drains the
//! active effect acknowledgement, then fixes the host-failure result. It never
//! waits for stdin or joins the reader; an active output effect can delay seal.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::thread::JoinHandle;

use qianqian_playback::PlaybackSessionHandle;

use crate::{cli, machine::ReportStream};

/// Fixed-size cause classification, separate from D11 and diagnostic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostFailure {
    Spawn,
    Read,
    Panic,
}

impl HostFailure {
    pub(crate) fn report(self) -> &'static str {
        match self {
            Self::Spawn => "machine input failed: could not spawn stdin reader",
            Self::Read => "machine input failed: stdin read error",
            Self::Panic => "machine input failed: stdin reader panicked",
        }
    }
}

#[derive(Default)]
struct State {
    admission_closed: bool,
    operation_active: bool,
    first_failure: Option<HostFailure>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    completed: Condvar,
}

#[derive(Clone, Default)]
pub(crate) struct HostInput {
    shared: Arc<Shared>,
}

impl HostInput {
    /// Linearization: setting operation_active under the bookkeeping lock.
    /// There is only one reader; synchronous spawn failure has no reader.
    fn admit(&self) -> Option<Operation<'_>> {
        let mut state = self.shared.state.lock().expect("machine input lock");
        if state.admission_closed {
            return None;
        }
        assert!(
            !state.operation_active,
            "one machine input operation at a time"
        );
        state.operation_active = true;
        Some(Operation { input: self })
    }

    fn is_open(&self) -> bool {
        !self
            .shared
            .state
            .lock()
            .expect("machine input lock")
            .admission_closed
    }

    /// Called by the invocation owner AFTER terminal wait and root disposal.
    /// Closing admission prevents fresh work from overtaking the drain; it is
    /// not the final seal. An already-admitted operation may still record its
    /// failure and complete its effects. The wait RELEASES the bookkeeping
    /// mutex. Final seal linearizes at the read of first_failure after the
    /// last completion acknowledgement, under that same mutex.
    pub(crate) fn seal(&self) -> Option<HostFailure> {
        let mut state = self.shared.state.lock().expect("machine input lock");
        state.admission_closed = true;
        while state.operation_active {
            state = self
                .shared
                .completed
                .wait(state)
                .expect("machine input lock");
        }
        state.first_failure
    }

    fn failure(&self, failure: HostFailure, handle: &PlaybackSessionHandle) {
        if let Some(operation) = self.admit() {
            operation.fail(failure, handle);
        }
    }

    fn line(
        &self,
        line: &str,
        handle: &PlaybackSessionHandle,
        output: &mut impl FnMut(ReportStream, &str),
    ) -> bool {
        let Some(operation) = self.admit() else {
            return false;
        };
        // Catch INSIDE the admitted operation. A dispatch/output unwind must
        // record its failure/Stop response before Drop acknowledges completion.
        let result = catch_unwind(AssertUnwindSafe(|| {
            match cli::parse_interactive_line(line) {
                Ok(cli::InteractiveCommand::Stop) => handle.request_stop(),
                Ok(cli::InteractiveCommand::Pause) => handle.request_pause(),
                Ok(cli::InteractiveCommand::Resume) => handle.request_resume(),
                Ok(cli::InteractiveCommand::Seek { time }) => match cli::parse_seek_time(&time) {
                    Some(target) => handle.request_seek(target),
                    None => output(
                        ReportStream::Stderr,
                        &format!("ignored input: cannot read seek time {time:?}"),
                    ),
                },
                Ok(cli::InteractiveCommand::Status) => output(
                    ReportStream::Stdout,
                    &crate::status::format_status(&handle.observe()),
                ),
                Ok(_) => output(
                    ReportStream::Stderr,
                    "not wired yet: only 'stop', 'pause', 'resume', 'seek' and 'status' control playback",
                ),
                Err(error) => output(ReportStream::Stderr, &format!("ignored input: {error}")),
            }
        }));
        if result.is_err() {
            operation.fail(HostFailure::Panic, handle);
            return false;
        }
        true
    }
}

/// This guard carries an acknowledgement obligation, NEVER a mutex guard.
struct Operation<'a> {
    input: &'a HostInput,
}

impl Operation<'_> {
    fn fail(&self, failure: HostFailure, handle: &PlaybackSessionHandle) {
        self.fail_with_response(failure, || {
            // Fact read only; no projection-derived lifecycle decision. A terminal
            // committed between this read and Stop makes the existing command inert.
            if handle.observe().terminal_outcome.is_none() {
                handle.request_stop();
            }
        });
    }

    fn fail_with_response(&self, failure: HostFailure, response: impl FnOnce()) {
        {
            let mut state = self.input.shared.state.lock().expect("machine input lock");
            // Failure-record linearization. An admitted operation retains this
            // right through admission closure until completion acknowledgement.
            state.first_failure.get_or_insert(failure);
        }
        response();
    }
}

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        let mut state = self.input.shared.state.lock().expect("machine input lock");
        state.operation_active = false; // operation completion acknowledgement
        self.input.shared.completed.notify_one();
    }
}

// This private module is the only production creator of this named thread.
// The process host owns the name; it is a diagnostic filter, never an identity
// used to route commands or grant semantic authority. Name plus process-wide
// panic-hook wrapper is a replaceable diagnostic mechanism, not the D3 identity
// model. Catchers classify admitted host failures; they never commit D11.
const READER_THREAD_NAME: &str = "qianqian-stdin";

fn install_reader_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Hooks run BEFORE catch_unwind. The explicit reader catchers own
            // its invocation report, including post-seal silence. The hook has
            // no handle, ledger, admission check or bookkeeping synchronization.
            if std::thread::current().name() != Some(READER_THREAD_NAME) {
                previous(info);
            }
        }));
    });
    // Keep this process diagnostic configuration after host return: restoring
    // the hook could let a detached reader print a late panic after seal.
}

pub(crate) fn spawn_reader(input: HostInput, handle: PlaybackSessionHandle) {
    install_reader_panic_hook();
    start_reader(
        input.clone(),
        handle.clone(),
        |task| {
            std::thread::Builder::new()
                .name(READER_THREAD_NAME.into())
                .spawn(task)
        },
        move || {
            use std::io::BufRead;
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            read_input(
                &input,
                &handle,
                |line| reader.read_line(line),
                |stream, line| {
                    use std::io::Write;
                    match stream {
                        ReportStream::Stdout => {
                            print!("{line}");
                            let _ = io::stdout().flush();
                        }
                        ReportStream::Stderr => eprintln!("{line}"),
                    }
                },
            );
        },
    );
}

/// Narrow OS spawn seam; production always uses Builder::spawn. Dropping the
/// handle deliberately detaches the reader, not its result responsibility.
fn start_reader(
    input: HostInput,
    handle: PlaybackSessionHandle,
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> io::Result<JoinHandle<()>>,
    reader: impl FnOnce() + Send + 'static,
) {
    let boundary_input = input.clone();
    let boundary_handle = handle.clone();
    if spawn(Box::new(move || {
        if catch_unwind(AssertUnwindSafe(reader)).is_err() {
            boundary_input.failure(HostFailure::Panic, &boundary_handle);
        }
    }))
    .is_err()
    {
        input.failure(HostFailure::Spawn, &handle);
    }
}

/// One physical read at a time; read errors/panics acquire admission only when
/// reported. A blocked read owns no acknowledgement debt. A late wake exits
/// without command parsing, status formatting, output, failure or another read.
/// An unlocked open check before closure may still precede a physical OS read
/// starting after seal; its result cannot gain postclosure effect admission.
fn read_input(
    input: &HostInput,
    handle: &PlaybackSessionHandle,
    mut read: impl FnMut(&mut String) -> io::Result<usize>,
    mut output: impl FnMut(ReportStream, &str),
) {
    let mut line = String::new();
    while input.is_open() {
        line.clear();
        match catch_unwind(AssertUnwindSafe(|| read(&mut line))) {
            Ok(Ok(0)) => return, // EOF closes normally, never Stop/Failed.
            Ok(Ok(_)) => {
                if !input.line(&line, handle, &mut output) {
                    return;
                }
            }
            Ok(Err(_)) => {
                input.failure(HostFailure::Read, handle);
                return;
            }
            Err(_) => {
                input.failure(HostFailure::Panic, handle);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
