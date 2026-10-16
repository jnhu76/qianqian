//! Host-owned diagnostic outlet (error-system target contract §DG-1).
//!
//! Truth class: **disposable diagnostics** — Class C. Nothing here decides a
//! terminal outcome, performs recovery, or carries a reliable wakeup: no
//! operation result, lifecycle evidence, settle, join or Discharge may depend
//! on a record being delivered. The machine protocol stdout never passes
//! through this facility.
//!
//! Two producer classes, no third:
//!
//! * **class α — isolation-obligated threads** (the failure-publication cone,
//!   render-leg sites, the decode worker, cleanup paths, any thread that
//!   registered a pool). `emit` on such a thread performs ATOMICS ONLY: one
//!   bounded publish into that thread's own two-record pool
//!   ([`pool::AlphaPool`]). No lock, no queue, no CAS loop, no allocation and
//!   no wakeup from the producer side — the pool is designed so that a
//!   producer blocking on drainer-side synchronization is structurally
//!   impossible.
//! * **class β — ordinary host threads** (CLI report rows, the host shutdown
//!   report). `emit` uses the facility's bounded channel with `try_send`:
//!   capacity-non-waiting (a full queue is an immediate drop + count), which
//!   is all that is promised — the std send path may take internal
//!   synchronization after a successful write, and that is acceptable exactly
//!   because no isolation obligation attaches to these threads.
//!
//! One drainer thread performs all rendering and every exporter call. It is
//! owned by the host application (the App installs and shuts it down); it is
//! not a Plugin, not a K0 capability and not an event bus — one
//! process-lifetime resource with no composition identity to earn.
//!
//! Shutdown is bounded-or-honest: the host stops admission, waits a bounded
//! deadline for the drainer's final sweep, and then ABANDONS the thread if it
//! is stuck inside a blocking sink. An abandoned shutdown is reported as
//! such — never dressed up as "drain complete" — and lost records are counted.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod pool;

use std::borrow::Cow;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub use pool::AlphaPool;

/// Maximum bytes of a record's message (fixed inline storage; no heap).
pub const MSG_CAP: usize = 192;
/// The class-β channel's capacity (preallocated at channel creation).
pub const BETA_CAPACITY: usize = 256;
/// The drainer's idle receive window: while idle it also polls every
/// registered class-α pool at least once per window.
pub const POLL_WINDOW: Duration = Duration::from_millis(50);

// The drainer flag is thread-local: the facility's reentry guard must cost
// one register read on the producer path, never a global lookup.
thread_local! {
    static IS_DRAINER: Cell<bool> = const { Cell::new(false) };
    static ALPHA_POOL: Cell<Option<&'static AlphaPool>> = const { Cell::new(None) };
}

/// Severity. Independent of disposition: even `Error` records are best-effort.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    pub const fn tag(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

/// Owned, bounded, heap-free message text.
///
/// Producers may compose it ONLY from whitelisted segments: `&'static str`
/// tags, bounded byte copies from site-provided slices, and primitive
/// integers rendered by the internal digit loop. Arbitrary `Display` values
/// are banned on producer paths — a formatter may compute, allocate or block
/// before a fixed buffer can stop it, so "the output length is bounded" never
/// implies "the formatting is bounded". A site that needs richer rendering
/// sends structured fields (status / native code) and lets the drainer render
/// them.
#[derive(Clone, Copy)]
pub struct Text {
    bytes: [u8; MSG_CAP],
    len: u16,
}

impl Default for Text {
    fn default() -> Self {
        Self::empty()
    }
}

impl Text {
    pub const fn empty() -> Self {
        Self {
            bytes: [0u8; MSG_CAP],
            len: 0,
        }
    }

    /// Copy a static tag (bounded; truncation stays inside the buffer).
    pub fn from_static(tag: &'static str) -> Self {
        let mut text = Self::empty();
        text.push_str(tag);
        text
    }

    /// Copy a byte slice (bounded). Bytes are not assumed to be UTF-8;
    /// rendering is lossy and happens on the drainer.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut text = Self::empty();
        text.push_bytes(bytes);
        text
    }

    /// Append a `&str`, truncating on a character boundary at capacity.
    pub fn push_str(&mut self, tag: &str) {
        let room = MSG_CAP - self.len as usize;
        let mut take = tag.len().min(room);
        while take > 0 && !tag.is_char_boundary(take) {
            take -= 1;
        }
        self.push_bytes(&tag.as_bytes()[..take]);
    }

    /// Append raw bytes, truncating at capacity.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let room = MSG_CAP - self.len as usize;
        let take = bytes.len().min(room);
        self.bytes[self.len as usize..self.len as usize + take].copy_from_slice(&bytes[..take]);
        self.len += take as u16;
    }

    /// Append an integer with a plain digit loop: terminating, allocation-free
    /// and panic-free (the whitelisted integer rendering).
    pub fn push_i64(&mut self, value: i64) {
        let mut digits = [0u8; 20];
        let mut cursor = digits.len();
        let negative = value < 0;
        let mut magnitude = value.unsigned_abs();
        loop {
            cursor -= 1;
            digits[cursor] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
            if magnitude == 0 {
                break;
            }
        }
        if negative {
            cursor -= 1;
            digits[cursor] = b'-';
        }
        self.push_bytes(&digits[cursor..]);
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// Drainer-side rendering: lossy over arbitrary producer bytes.
    pub fn render(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(self.as_bytes())
    }

    /// Strict view (used by tests and by producers that only appended
    /// `&str`/integers, which is always valid UTF-8).
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).unwrap_or("")
    }
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\"")?;
        f.write_str(&self.render())?;
        f.write_str("\"")
    }
}

/// One owned, plain-data diagnostic record: no `String`, no heap, no handle.
/// Dropping one is a no-op and storage is preallocated, so the producer path
/// never pays construct-then-truncate costs.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    /// Static site tag (the design's "site: static enum — no runtime
    /// strings": a `&'static str` is a static tag, never an owned string).
    pub site: &'static str,
    pub level: Level,
    pub message: Text,
    /// Typed status (e.g. a SongCore `song_status`) — the DG-3 consumer.
    pub status: Option<i32>,
    /// Raw native/backend code where one exists (e.g. `song_error.native_code`).
    pub native_code: Option<i32>,
    /// Monotonic microseconds since the process started (never wall clock).
    pub ts_micros: u64,
}

impl Record {
    pub const fn new(site: &'static str, level: Level) -> Self {
        Self {
            site,
            level,
            message: Text::empty(),
            status: None,
            native_code: None,
            ts_micros: 0,
        }
    }

    pub fn with_message(mut self, message: Text) -> Self {
        self.message = message;
        self
    }

    pub fn with_status(mut self, status: i32) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_native_code(mut self, native_code: i32) -> Self {
        self.native_code = Some(native_code);
        self
    }

    /// Stamp with the monotonic process clock (producer-side, no allocation).
    pub fn stamped(mut self) -> Self {
        self.ts_micros = monotonic_micros();
        self
    }

    /// Constant empty record — used to preallocate pool storage.
    pub const fn empty() -> Self {
        Record::new("empty", Level::Info)
    }
}

fn monotonic_micros() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_micros() as u64
}

/// What happened to one emission attempt. `emit` never fails the caller and
/// never blocks; the return value exists for tests and for sites that want to
/// account locally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Recorded in the calling thread's own class-α pool.
    AlphaRecorded,
    /// Rejected at the class-α pool's admission check — the pool's only loss
    /// site (dropped + counted).
    AlphaRejected,
    /// Handed to the class-β bounded channel.
    BetaQueued,
    /// The class-β channel was full: dropped + counted, no retry.
    BetaRejected,
    /// Shutdown has begun: dropped + counted (a late producer never revives
    /// the drainer).
    Shutdown,
    /// Called from the drainer thread itself: dropped (no reentry).
    Reentry,
    /// No facility is installed and this thread registered no α pool.
    NoSink,
}

/// An exporter's failure report. The drainer never panics on a sink failure:
/// it disables the exporter, counts the loss and reports once through the
/// non-panicking stderr writer.
#[derive(Clone, Copy, Debug)]
pub struct ExportError {
    pub message: &'static str,
}

/// A sink, owned by the facility. `export` runs ONLY on the drainer thread,
/// never on a producer's call stack and never while a reliable-publication
/// lock is held.
pub trait Exporter: Send + 'static {
    /// Self-diagnostic name (used in the disable report).
    fn name(&self) -> &'static str;

    /// Deliver one record. An `Err` disables this exporter for the rest of the
    /// run; the record counts as lost and the failure is counted once.
    fn export(&mut self, record: &Record) -> Result<(), ExportError>;

    /// Called once when the drainer performs its final sweep (normal or
    /// abandoned shutdown boundary).
    fn flush(&mut self) -> Result<(), ExportError> {
        Ok(())
    }
}

/// Configuration of one installation. The defaults are the design-round
/// choices for the residual representation-level parameters (capacity, poll
/// cadence); they carry no semantics.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub beta_capacity: usize,
    pub poll_window: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            beta_capacity: BETA_CAPACITY,
            poll_window: POLL_WINDOW,
        }
    }
}

/// The facility's monotonic counters. Every one is a REAL boundary count, so
/// the two accounting identities below are falsifiable rather than derived:
///
/// ```text
/// admission (β):  beta_queued   = beta_consumed  + (records still queued)
/// drain boundary: copied_out + beta_consumed = exported + export_failed
/// ```
///
/// The second identity holds at quiescence (no record in flight inside the
/// drainer) and is what the tests assert.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Class-β records accepted by the channel.
    pub beta_queued: u64,
    /// Class-β records dropped at a full channel, and records dropped after
    /// shutdown began (admission losses).
    pub beta_rejected: u64,
    /// Records the drainer pulled off the channel.
    pub beta_consumed: u64,
    /// Class-α records rejected at a pool's admission check (summed over
    /// every registered pool — the pools' ONLY loss site).
    pub alpha_rejected: u64,
    /// Generations copied out of every registered pool.
    pub copied_out: u64,
    /// Records actually delivered to the exporter.
    pub exported: u64,
    /// Admitted records the exporter did not deliver: a failed export attempt,
    /// or any record left after the exporter was disabled.
    pub export_failed: u64,
    /// Records still published-unconsumed in a pool at this instant.
    pub outstanding: u64,
    /// Self-diagnostic events (an exporter disable, a flush failure).
    pub self_diagnostics: u64,
}

impl Counters {
    /// Admitted but not yet delivered at this instant: pool-outstanding
    /// records, records still queued on the β channel, and records the drainer
    /// has already taken but not yet handed to a sink (a blocked exporter's
    /// in-flight record included).
    pub fn undelivered_now(&self) -> u64 {
        let admitted = self.copied_out.saturating_add(self.beta_queued);
        let resolved = self.exported.saturating_add(self.export_failed);
        self.outstanding + admitted.saturating_sub(resolved)
    }
}

/// The outcome of a bounded shutdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShutdownReport {
    /// The drainer performed its final sweep and exited inside the deadline.
    pub drained: bool,
    /// The drainer did not exit inside the deadline: it was left running and
    /// the records not yet delivered are NOT claimed as delivered. NEVER
    /// reported as drained.
    pub abandoned: bool,
    /// Records admitted but undelivered at the moment of this report (a
    /// snapshot, not an accumulated loss — an abandoned drainer may still
    /// deliver some of them later).
    pub undelivered_at_exit: u64,
    pub counters: Counters,
}

struct Inner {
    beta_send: SyncSender<Record>,
    beta_recv: Mutex<Option<Receiver<Record>>>,
    shutdown: AtomicBool,
    drainer_exited: AtomicBool,
    /// Admission/delivery counters.
    beta_queued: AtomicU64,
    beta_rejected: AtomicU64,
    beta_consumed: AtomicU64,
    exported: AtomicU64,
    export_failed: AtomicU64,
    self_diagnostics: AtomicU64,
}

/// Process-wide facility slot: at most one host installation.
static FACILITY: OnceLock<Arc<Inner>> = OnceLock::new();

/// Every registered class-α pool, in registration order. Producing threads
/// never touch this: they write through their thread-local pointer only.
static ALPHA_REGISTRY: OnceLock<Mutex<Vec<&'static AlphaPool>>> = OnceLock::new();

fn alpha_registry() -> &'static Mutex<Vec<&'static AlphaPool>> {
    ALPHA_REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// A registered class-α producer. The pool is leaked on purpose: one
/// preallocated two-record pool per registered thread/site for the process
/// lifetime, with no lazy-init lock anywhere on the fault path.
pub struct AlphaRegistration {
    pool: &'static AlphaPool,
}

impl AlphaRegistration {
    /// Register the CALLING thread as an isolation-obligated producer.
    /// Cold path only (thread/site startup) — this takes the registry lock.
    pub fn register() -> Self {
        let pool: &'static AlphaPool = Box::leak(Box::new(AlphaPool::new()));
        alpha_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(pool);
        ALPHA_POOL.with(|cell| cell.set(Some(pool)));
        Self { pool }
    }

    pub fn pool(&self) -> &'static AlphaPool {
        self.pool
    }
}

impl Drop for AlphaRegistration {
    fn drop(&mut self) {
        // The thread is leaving: it must not keep publishing into a pool the
        // drainer still polls. Clear the thread-local pointer; the pool stays
        // registered so its unpolled records are still counted/exported.
        ALPHA_POOL.with(|cell| cell.set(None));
    }
}

/// Whether the calling thread is a registered class-α producer (the panic
/// hook's critical-thread test, DG-4b).
pub fn is_alpha_producer() -> bool {
    ALPHA_POOL.with(Cell::get).is_some()
}

/// Emit one diagnostic record. Never blocks, never panics, never allocates on
/// the producer path, and never waits for a sink.
pub fn emit(record: Record) -> Disposition {
    if let Some(pool) = ALPHA_POOL.with(Cell::get) {
        return if pool.publish(record) {
            Disposition::AlphaRecorded
        } else {
            Disposition::AlphaRejected
        };
    }
    let Some(inner) = FACILITY.get() else {
        return Disposition::NoSink;
    };
    if IS_DRAINER.with(Cell::get) {
        return Disposition::Reentry;
    }
    if inner.shutdown.load(Ordering::Acquire) {
        inner.beta_rejected.fetch_add(1, Ordering::Relaxed);
        return Disposition::Shutdown;
    }
    match inner.beta_send.try_send(record) {
        Ok(()) => {
            inner.beta_queued.fetch_add(1, Ordering::Relaxed);
            Disposition::BetaQueued
        }
        Err(TrySendError::Full(_)) => {
            inner.beta_rejected.fetch_add(1, Ordering::Relaxed);
            Disposition::BetaRejected
        }
        Err(TrySendError::Disconnected(_)) => {
            inner.beta_rejected.fetch_add(1, Ordering::Relaxed);
            Disposition::Shutdown
        }
    }
}

/// The installed facility.
pub struct Diagnostics {
    inner: Arc<Inner>,
    drainer: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Diagnostics {
    /// Install the process-wide facility with one exporter and start the
    /// drainer thread. Fails if a facility is already installed: the host owns
    /// exactly one.
    pub fn install(exporter: Box<dyn Exporter>, config: Config) -> Result<Self, InstallError> {
        let (beta_send, beta_recv) = std::sync::mpsc::sync_channel(config.beta_capacity);
        let inner = Arc::new(Inner {
            beta_send,
            beta_recv: Mutex::new(Some(beta_recv)),
            shutdown: AtomicBool::new(false),
            drainer_exited: AtomicBool::new(false),
            beta_queued: AtomicU64::new(0),
            beta_rejected: AtomicU64::new(0),
            beta_consumed: AtomicU64::new(0),
            exported: AtomicU64::new(0),
            export_failed: AtomicU64::new(0),
            self_diagnostics: AtomicU64::new(0),
        });
        FACILITY
            .set(Arc::clone(&inner))
            .map_err(|_| InstallError::AlreadyInstalled)?;
        let receiver = inner
            .beta_recv
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .expect("fresh facility owns its receiver");
        let drainer_thread = {
            let inner = Arc::clone(&inner);
            std::thread::Builder::new()
                .name("qianqian-diagnostics".into())
                .spawn(move || drainer(inner, receiver, exporter, config.poll_window))
                .map_err(|_| InstallError::DrainerSpawnFailed)?
        };
        Ok(Self {
            inner,
            drainer: Mutex::new(Some(drainer_thread)),
        })
    }

    /// Whether a host installed the facility in this process.
    pub fn is_installed() -> bool {
        FACILITY.get().is_some()
    }

    /// The installed facility's counters, when one is installed.
    pub fn global_counters() -> Option<Counters> {
        FACILITY.get().map(|inner| counters_of(inner))
    }

    /// Stop admitting records, wait a BOUNDED deadline for the drainer's final
    /// sweep, then abandon if it is stuck in a blocking sink. An abandoned
    /// shutdown is reported honestly and its records are counted as lost.
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        self.inner.shutdown.store(true, Ordering::Release);
        let started = Instant::now();
        while !self.inner.drainer_exited.load(Ordering::Acquire) {
            if started.elapsed() >= deadline {
                let counters = self.counters();
                return ShutdownReport {
                    drained: false,
                    abandoned: true,
                    undelivered_at_exit: counters.undelivered_now(),
                    counters,
                };
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(handle) = self
            .drainer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = handle.join();
        }
        let counters = self.counters();
        ShutdownReport {
            drained: true,
            abandoned: false,
            undelivered_at_exit: counters.undelivered_now(),
            counters,
        }
    }

    /// Monotonic counters, including the class-α pools' own accounting.
    pub fn counters(&self) -> Counters {
        counters_of(&self.inner)
    }
}

/// Why an installation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError {
    /// A facility is already installed: the host owns exactly one.
    AlreadyInstalled,
    /// The drainer thread could not be spawned.
    DrainerSpawnFailed,
}

fn counters_of(inner: &Inner) -> Counters {
    let (alpha_rejected, copied_out, outstanding) = pool_totals();
    Counters {
        beta_queued: inner.beta_queued.load(Ordering::Relaxed),
        beta_rejected: inner.beta_rejected.load(Ordering::Relaxed),
        beta_consumed: inner.beta_consumed.load(Ordering::Relaxed),
        alpha_rejected,
        copied_out,
        exported: inner.exported.load(Ordering::Relaxed),
        export_failed: inner.export_failed.load(Ordering::Relaxed),
        outstanding,
        self_diagnostics: inner.self_diagnostics.load(Ordering::Relaxed),
    }
}

fn pool_totals() -> (u64, u64, u64) {
    let registry = alpha_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut rejected = 0u64;
    let mut copied = 0u64;
    let mut outstanding = 0u64;
    for pool in registry.iter() {
        rejected += pool.rejected();
        copied += pool.copied_out();
        outstanding += u64::from(pool.outstanding());
    }
    (rejected, copied, outstanding)
}

/// The exporter's outcome vocabulary, kept explicit so accounting is visible.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Delivery {
    Exported,
    Failed,
}

/// Deliver one record, disabling a failing exporter for the rest of the run
/// and reporting that once through the non-panicking writer.
fn deliver(
    inner: &Inner,
    exporter: &mut Option<Box<dyn Exporter>>,
    disabled: &mut bool,
    record: &Record,
) -> Delivery {
    let Some(sink) = exporter.as_mut() else {
        // The exporter was disabled (or never existed): the record is admitted
        // but not delivered — counted as an export loss, never as delivered.
        inner.export_failed.fetch_add(1, Ordering::Relaxed);
        return Delivery::Failed;
    };
    match sink.export(record) {
        Ok(()) => {
            inner.exported.fetch_add(1, Ordering::Relaxed);
            Delivery::Exported
        }
        Err(error) => {
            inner.export_failed.fetch_add(1, Ordering::Relaxed);
            if !*disabled {
                *disabled = true;
                inner.self_diagnostics.fetch_add(1, Ordering::Relaxed);
                // One bounded line, non-panicking (a write error is ignored,
                // never unwrapped), and reported from the DRAINER thread.
                write_stderr_line(&format!(
                    "[qianqian-diagnostics] exporter {} disabled: {}",
                    sink.name(),
                    error.message
                ));
            }
            // Retirement: the exporter is dropped here, so no later record can
            // reach it (later records count as export losses).
            *exporter = None;
            Delivery::Failed
        }
    }
}

/// Non-panicking stderr line writer. `writeln!` returns `Err` on a broken or
/// blocked-descendant pipe; it never panics and the result is deliberately
/// ignored — a diagnostic write failure is a diagnostic loss, nothing else.
pub fn write_stderr_line(line: &str) {
    use std::io::Write;
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{line}");
    let _ = stderr.flush();
}

/// Drain every registered pool once. `cursors` holds the drainer's own
/// per-pool walk position (single-consumer by construction).
fn poll_pools(
    inner: &Inner,
    cursors: &mut Vec<(&'static AlphaPool, u32)>,
    exporter: &mut Option<Box<dyn Exporter>>,
    disabled: &mut bool,
) {
    let registered: Vec<&'static AlphaPool> = alpha_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    for pool in registered {
        if let Some((_, last)) = cursors
            .iter_mut()
            .find(|(known, _)| std::ptr::eq(*known, pool))
        {
            pool.drain(last, &mut |record| {
                let _ = deliver(inner, exporter, disabled, record);
            });
        } else {
            let mut last = 0u32;
            pool.drain(&mut last, &mut |record| {
                let _ = deliver(inner, exporter, disabled, record);
            });
            cursors.push((pool, last));
        }
    }
}

/// The final sweep at the shutdown boundary: one last pool poll, every record
/// still queued on the channel, then the exporter's own flush.
fn final_sweep(
    inner: &Inner,
    receiver: &Receiver<Record>,
    cursors: &mut Vec<(&'static AlphaPool, u32)>,
    exporter: &mut Option<Box<dyn Exporter>>,
    disabled: &mut bool,
) {
    poll_pools(inner, cursors, exporter, disabled);
    while let Ok(record) = receiver.try_recv() {
        inner.beta_consumed.fetch_add(1, Ordering::Relaxed);
        let _ = deliver(inner, exporter, disabled, &record);
    }
    let Some(sink) = exporter.as_mut() else {
        return;
    };
    if let Err(error) = sink.flush() {
        inner.export_failed.fetch_add(1, Ordering::Relaxed);
        if !*disabled {
            *disabled = true;
            inner.self_diagnostics.fetch_add(1, Ordering::Relaxed);
            write_stderr_line(&format!(
                "[qianqian-diagnostics] exporter {} flush failed: {}",
                sink.name(),
                error.message
            ));
        }
    }
}

fn drainer(
    inner: Arc<Inner>,
    receiver: Receiver<Record>,
    exporter: Box<dyn Exporter>,
    poll_window: Duration,
) {
    IS_DRAINER.with(|flag| flag.set(true));
    let mut exporter = Some(exporter);
    let mut disabled = false;
    let mut cursors: Vec<(&'static AlphaPool, u32)> = Vec::new();

    // The drainer's poll obligation is a conditional FREQUENCY, not a latency
    // bound: every pool is polled after each batch and at least once per idle
    // receive window. While an exporter blocks there is no poll at all and no
    // delivery deadline — the declared residual.
    loop {
        poll_pools(&inner, &mut cursors, &mut exporter, &mut disabled);
        match receiver.recv_timeout(poll_window) {
            Ok(record) => {
                inner.beta_consumed.fetch_add(1, Ordering::Relaxed);
                let _ = deliver(&inner, &mut exporter, &mut disabled, &record);
                while let Ok(record) = receiver.try_recv() {
                    inner.beta_consumed.fetch_add(1, Ordering::Relaxed);
                    let _ = deliver(&inner, &mut exporter, &mut disabled, &record);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                final_sweep(
                    &inner,
                    &receiver,
                    &mut cursors,
                    &mut exporter,
                    &mut disabled,
                );
                break;
            }
        }
        if inner.shutdown.load(Ordering::Acquire) {
            final_sweep(
                &inner,
                &receiver,
                &mut cursors,
                &mut exporter,
                &mut disabled,
            );
            break;
        }
    }
    inner.drainer_exited.store(true, Ordering::Release);
}

/// The built-in stderr exporter: one plain line per record, rendered on the
/// drainer, written with the non-panicking writer.
pub struct StderrExporter {
    /// Minimum level admitted by this sink (the authoritative filter).
    pub min_level: Level,
}

impl Default for StderrExporter {
    fn default() -> Self {
        Self {
            min_level: Level::Info,
        }
    }
}

impl Exporter for StderrExporter {
    fn name(&self) -> &'static str {
        "stderr"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        if record.level < self.min_level {
            return Ok(());
        }
        let mut line = String::with_capacity(MSG_CAP + 64);
        line.push('[');
        line.push_str(record.site);
        line.push_str("] ");
        line.push_str(record.level.tag());
        if let Some(status) = record.status {
            line.push_str(" status=");
            line.push_str(&status.to_string());
        }
        if let Some(native) = record.native_code {
            line.push_str(" native=");
            line.push_str(&native.to_string());
        }
        line.push_str(" ts_us=");
        line.push_str(&record.ts_micros.to_string());
        if !record.message.is_empty() {
            line.push(' ');
            line.push_str(&record.message.render());
        }
        write_stderr_line(&line);
        Ok(())
    }
}
