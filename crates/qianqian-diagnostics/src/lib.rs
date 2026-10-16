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
//!   bounded publish into that thread's own two-record ownership-handoff pool
//!   (`pool.rs`). No lock, no queue, no CAS loop, no allocation and no wakeup
//!   from the producer side — the pool is designed so that a producer blocking
//!   on drainer-side synchronization is structurally impossible. The protocol
//!   assumes exactly one producer and one consumer per pool, and the types
//!   keep that assumption: the pool core is crate-private, the producer role
//!   is [`AlphaRegistration`] (thread-bound, not shareable, not clonable) and
//!   the consumer role is the facility's single drainer.
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
//! is stuck inside a blocking sink. Admission stops for BOTH producer classes
//! through ONE admission word (bit 0 = closed, the remaining bits = publications
//! in flight): the producer's single RMW carries both its admission decision and
//! its claim on the final sweep, so a presentation the facility admitted is
//! either swept — the drainer orders that sweep after the last in-flight
//! publication, bounded by [`PUBLISHER_QUIESCENCE_WINDOW`] — or counted as an
//! admission loss at its own class's boundary ([`Diagnostics::shutdown`] states
//! the protocol). When that bound cannot be met the sweep still runs, but the
//! report does NOT claim a drain: `drained` requires the quiescence observation.
//! An abandoned shutdown is reported as such — never dressed up as "drain
//! complete" — and lost records are counted. The abandonment report itself is
//! attempted off the exit path (a detached, never-joined writer): the sink is
//! stuck by the abandonment rule, so the exit boundary must not synchronously
//! enter it again.
//!
//! Pool lifetime: one pool per registering thread, retired when the
//! registration drops. The drainer walks the residual records, folds the
//! pool's counters into the facility totals and releases the pool, so the
//! registry tracks live producers rather than the process's episode history.

#![forbid(unsafe_op_in_unsafe_fn)]

mod pool;

use std::borrow::Cow;
use std::cell::Cell;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pool::PoolCore;

/// Maximum bytes of a record's message (fixed inline storage; no heap).
pub const MSG_CAP: usize = 192;
/// The class-β channel's capacity (preallocated at channel creation).
pub const BETA_CAPACITY: usize = 256;
/// The drainer's idle receive window: while idle it also polls every
/// registered class-α pool at least once per window.
pub const POLL_WINDOW: Duration = Duration::from_millis(50);
/// The drainer's bounded wait for in-flight publications before its final
/// sweep ([`Diagnostics::shutdown`]): the close orders that sweep after every
/// publication the facility admitted, so a record admitted before the closure
/// is either swept there or is a presentation the report refuses to call a
/// complete drain. A representation parameter, like the poll cadence — the
/// semantics is the ordering, the window is only how long a stalled producer
/// may delay the drainer's own close decision.
pub const PUBLISHER_QUIESCENCE_WINDOW: Duration = Duration::from_millis(250);

// The drainer flag is thread-local: the facility's reentry guard must cost
// one register read on the producer path, never a global lookup.
thread_local! {
    static IS_DRAINER: Cell<bool> = const { Cell::new(false) };
    /// The calling thread's own registered pool, as a non-owning pointer to a
    /// core kept alive by [`AlphaRegistration`]. Written and cleared only by
    /// that thread (registration / its `Drop`), so the pointer is valid for
    /// every read on this thread and never observed by another thread; a plain
    /// thread-local `Cell` is what keeps the fault path free of locks,
    /// refcount traffic and lazy-init guards (the DG-4b hook reads it too).
    static ALPHA_POOL: Cell<Option<NonNull<PoolCore>>> = const { Cell::new(None) };
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
    /// A pure read: it never initializes the clock and never waits. A record
    /// stamped before the clock's cold-path initialization carries `0`, which
    /// is this facility's "no timestamp available" value (the panic-hook
    /// record carries it by design).
    pub fn stamped(mut self) -> Self {
        self.ts_micros = monotonic_micros();
        self
    }

    /// Constant empty record — used to preallocate pool storage.
    pub const fn empty() -> Self {
        Record::new("empty", Level::Info)
    }
}

/// The facility's process clock. Initialized on COLD paths only
/// ([`Diagnostics::install`], [`AlphaRegistration::register`]): a producer that
/// initialized it itself would enter `OnceLock` initialization synchronization
/// on the very path that must not synchronize, so producers only ever read.
static PROCESS_CLOCK: OnceLock<Instant> = OnceLock::new();

/// Cold-path clock initialization. Idempotent; called from installation and
/// from every registration, so a producer is never the initializer.
fn ensure_process_clock() {
    let _ = PROCESS_CLOCK.get_or_init(Instant::now);
}

/// Producer-path read: a plain `OnceLock::get` with no initialization state and
/// no wait. `0` means "the process clock was never initialized", which the
/// record states honestly instead of stamping an arbitrary instant.
fn monotonic_micros() -> u64 {
    PROCESS_CLOCK
        .get()
        .map_or(0, |start| start.elapsed().as_micros() as u64)
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
    /// Shutdown has begun: dropped + counted, never published (a late producer
    /// never revives the drainer). Decided by the admission word's closed bit,
    /// taken in the same RMW that would have held this publication, so it always
    /// answers a presentation the facility decided NOT to admit: a class-α
    /// thread's presentation is counted at its own pool's admission boundary, a
    /// class-β presentation at the channel's. A presentation admitted BEFORE
    /// the closure is never answered this way — it is swept, or (if its producer
    /// outlasts the quiescence window) the drain is reported as incomplete.
    Shutdown,
    /// Called from the drainer thread itself: dropped (no reentry).
    Reentry,
    /// A later critical panic on a thread whose pool already reported one: the
    /// one-shot latch suppressed the record (DG-4b). Not a pool loss — the
    /// panic is still counted by [`critical_panics`].
    PanicAlreadyReported,
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

    /// Deliver one record. Delivery is per record and this result IS its
    /// verdict: one `Ok` = exactly one delivered record (`exported`), one `Err`
    /// = that record's loss (`export_failed`) and this exporter is disabled for
    /// the rest of the run.
    fn export(&mut self, record: &Record) -> Result<(), ExportError>;

    /// Called once when the drainer performs its final sweep (normal or
    /// abandoned shutdown boundary). Its failure is a sink-level
    /// self-diagnostic (`self_diagnostics`), NOT a record loss: a flush outcome
    /// belongs to no record count; each record's verdict was already stated by
    /// its own [`Exporter::export`] result.
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
    /// or any record left after the exporter was disabled. Never a flush
    /// outcome — a flush failure has no record to be charged against and is a
    /// self-diagnostic instead.
    pub export_failed: u64,
    /// Records still published-unconsumed in a pool at this instant.
    pub outstanding: u64,
    /// Self-diagnostic events (an exporter disable, a flush failure).
    pub self_diagnostics: u64,
    /// Panics observed on isolation-obligated threads (DG-4b). Counted on the
    /// panicking thread with one relaxed increment; rendered elsewhere.
    pub critical_panics: u64,
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
    /// A COMPLETE drain: the drainer exited inside the deadline AND its final
    /// sweep was ordered after every publication the facility admitted before
    /// the closure (publisher quiescence was observed). Only a drained report
    /// accounts for every admitted record — see [`Diagnostics::shutdown`] — and
    /// it is never set when that ordering could not be established.
    pub drained: bool,
    /// The drainer did not exit inside the deadline: it was left running and
    /// the records not yet delivered are NOT claimed as delivered. NEVER
    /// reported as drained. (A report can also be neither drained nor
    /// abandoned: the drainer exited, but its sweep could not be ordered after
    /// publisher quiescence — the accounting is then incomplete by
    /// construction, and says so.)
    pub abandoned: bool,
    /// Records admitted but undelivered at the moment of this report (a
    /// snapshot, not an accumulated loss — an abandoned drainer may still
    /// deliver some of them later). Zero whenever `drained`: a complete drain
    /// leaves nothing admitted and unevaluated.
    pub undelivered_at_exit: u64,
    pub counters: Counters,
}

struct Inner {
    beta_send: SyncSender<Record>,
    beta_recv: Mutex<Option<Receiver<Record>>>,
    /// The facility's admission word: bit 0 = CLOSED (the closure has been
    /// published), the remaining bits = publications in flight. The producer
    /// takes ONE RMW here and the closer takes ONE on the same word, which is
    /// what makes "decide admission" and "publish" one indivisible step as seen
    /// by the close: a producer whose `fetch_add` precedes the closer's
    /// `fetch_or` holds a counted publication the drainer must wait for, and one
    /// whose `fetch_add` follows it reads the closed word and never publishes.
    admission: AtomicUsize,
    drainer_exited: AtomicBool,
    /// Written by the drainer just before its final sweep (Release), read by
    /// the closer after the drainer exited (Acquire): whether that sweep was
    /// ordered after publisher quiescence. `drained` requires `true`.
    sweep_quiescent: AtomicBool,
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

/// Admission word bit: the closure has been published.
const CLOSED: usize = 1;
/// Admission word unit: one publication in flight.
const IN_FLIGHT: usize = 2;

/// Publications in flight, decoded from an admission word.
fn in_flight(word: usize) -> usize {
    word / IN_FLIGHT
}

/// Close admission: ONE RMW, the linearization point of the closure. Every
/// admission decision is an RMW on the same word, so this is ordered against
/// every publication attempt — see [`AdmissionSection`].
fn close_admission(inner: &Inner) {
    inner.admission.fetch_or(CLOSED, Ordering::AcqRel);
}

/// Whether the closure has been published.
fn is_closed(inner: &Inner) -> bool {
    inner.admission.load(Ordering::Acquire) & CLOSED != 0
}

/// The producer's admission section: entered by ONE RMW before the closure
/// decision, left by ONE `Release` RMW after the publication (or after the
/// rejection has been counted). `Drop` performs the leave on every path, so no
/// early return can strand the count.
struct AdmissionSection<'a>(&'a Inner);

impl Drop for AdmissionSection<'_> {
    fn drop(&mut self) {
        self.0.admission.fetch_sub(IN_FLIGHT, Ordering::Release);
    }
}

/// Enter the admission section. The flag is the admission decision: `true` =
/// admitted before the closure (publish inside the section — the closer's
/// quiescence observation cannot complete until this section leaves), `false` =
/// the closure was already published (do NOT publish; count the presentation at
/// its own class's admission boundary).
fn enter_section(inner: &Inner) -> (bool, AdmissionSection<'_>) {
    let word = inner.admission.fetch_add(IN_FLIGHT, Ordering::AcqRel);
    (word & CLOSED == 0, AdmissionSection(inner))
}

/// The drainer's bounded wait for in-flight publications, run before its final
/// sweep. The observation is an RMW on purpose: a plain load may read a stale 0
/// that is concurrent with a `fetch_add` already preceding the closure in the
/// word's modification order, while an RMW reads the value written by the
/// latest modification before it. That is what makes "sees 0" mean "every
/// counted publication has left its section", and — through the release
/// sequence the Acquire observation reads from — makes those publications
/// visible to the sweep that follows. `true` = quiescent; `false` = the window
/// expired, and the sweep that follows must not be reported as a complete
/// drain.
fn wait_for_publisher_quiescence(inner: &Inner, window: Duration) -> bool {
    let started = Instant::now();
    loop {
        if in_flight(inner.admission.fetch_add(0, Ordering::Acquire)) == 0 {
            return true;
        }
        if started.elapsed() >= window {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The registered class-α pools. Producing threads never touch this: they
/// write through their thread-local pointer only. Registration (cold path)
/// appends; the drainer is the only reader and the only remover.
///
/// Retired pools whose residual records have already been walked are removed
/// and their counters folded into the `retired_*` totals, so the registry
/// holds LIVE producers rather than every thread that has ever registered.
/// That is what keeps the drainer's per-poll cost bounded by the number of
/// live α producers instead of growing with the process's episode history.
#[derive(Default)]
struct Registry {
    pools: Vec<Arc<PoolCore>>,
    /// Counters folded in when a pool was retired and released.
    retired_rejected: u64,
    retired_copied: u64,
}

static ALPHA_REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn alpha_registry() -> &'static Mutex<Registry> {
    ALPHA_REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// A snapshot of the live pools (Arc clones), so the drainer holds no registry
/// lock while it drains or while an exporter blocks.
fn registered_pools() -> Vec<Arc<PoolCore>> {
    alpha_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pools
        .clone()
}

/// The calling thread's registered pool, when it has one. The non-owning
/// pointer is set and cleared only by this thread's own registration, so the
/// dereference cannot race a release.
fn with_current_pool<R>(f: impl FnOnce(&PoolCore) -> R) -> Option<R> {
    ALPHA_POOL
        .with(Cell::get)
        // SAFETY: only this thread writes the cell, and it does so while the
        // registration owning the core is alive (`register` sets it, the
        // registration's `Drop` clears it before any reference it holds can
        // drop), so the pointee outlives every read performed here.
        .map(|pool| f(unsafe { pool.as_ref() }))
}

/// A registered class-α producer: the calling thread's exclusive, thread-bound
/// handle to its own pool.
///
/// Not `Send` and not `Sync` (the raw-pointer marker): the handle's `Drop`
/// clears the REGISTERING thread's fast path, so it must be dropped on the
/// thread that created it, and no safe code can share the handle — or hand a
/// second producer one — because the pool itself is private. The pool
/// protocol's single-producer premise is enforced by ownership, not by
/// documentation, and one thread holds at most one live registration
/// ([`AlphaRegistration::register`] rejects a second).
pub struct AlphaRegistration {
    pool: Arc<PoolCore>,
    _thread_bound: PhantomData<*const ()>,
}

impl AlphaRegistration {
    /// Register the CALLING thread as an isolation-obligated producer.
    ///
    /// ONE registration per thread: a second call on a thread that already holds
    /// one panics. The role is a per-thread fact (the thread-local fast path),
    /// so two live registrations on one thread cannot both be valid — a second
    /// would overwrite the first's fast path, and the first handle's `Drop`
    /// would then clear the second's, silently returning a registered thread to
    /// the class-β path (and, for the DG-4b posture, to chaining a panic into an
    /// unknown synchronous sink). Rejecting the duplicate keeps the fact
    /// single-valued; registering again AFTER a drop is ordinary and legal.
    ///
    /// Cold path only (thread/site startup) — this takes the registry lock and
    /// initializes the process clock when it is not running yet.
    pub fn register() -> Self {
        assert!(
            ALPHA_POOL.with(Cell::get).is_none(),
            "qianqian-diagnostics: this thread already holds an AlphaRegistration \
             (one registration per thread)"
        );
        ensure_process_clock();
        let pool = Arc::new(PoolCore::new());
        alpha_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pools
            .push(Arc::clone(&pool));
        ALPHA_POOL.with(|cell| cell.set(Some(NonNull::from(pool.as_ref()))));
        Self {
            pool,
            _thread_bound: PhantomData,
        }
    }

    /// This pool's admission-boundary attempts (observation only — the pool
    /// itself stays private).
    pub fn attempted(&self) -> u64 {
        self.pool.attempted()
    }

    /// This pool's accepted publications.
    pub fn accepted(&self) -> u64 {
        self.pool.accepted()
    }

    /// This pool's rejected publications (the protocol's only loss site).
    pub fn rejected(&self) -> u64 {
        self.pool.rejected()
    }

    /// Generations copied out of this pool.
    pub fn copied_out(&self) -> u64 {
        self.pool.copied_out()
    }

    /// Published-unconsumed + drainer-held records right now (0..=2).
    pub fn outstanding(&self) -> u32 {
        self.pool.outstanding()
    }
}

impl Drop for AlphaRegistration {
    fn drop(&mut self) {
        // Order matters twice over. (1) The thread's fast path is closed
        // FIRST, so no publish can be in flight when the drainer observes the
        // retirement — the drainer's Acquire load then covers every publish
        // this producer ever made. (2) Retirement is PUBLISHED on the pool, not
        // expressed by releasing it: this registration's reference may go away
        // now, but the registry keeps the pool alive until the drainer has
        // walked the residual records, folded the counters and released it.
        ALPHA_POOL.with(|cell| cell.set(None));
        self.pool.retire();
    }
}

/// Whether the calling thread is a registered class-α producer, i.e. whether
/// [`emit`] on this thread is a bounded atomics-only publish.
///
/// This is the thread-isolation fact, and it has two consumers:
///
/// * the av_log bridge's admission predicate (DG-4c): an av_log callback runs
///   on whatever FFmpeg-calling thread FFmpeg happens to be on, and the design
///   admits only the class-α path there — a callback that cannot see a
///   registered pool drops the record and counts it instead of riding the
///   class-β channel, whose send path may take internal synchronization on a
///   decode-side thread;
/// * the host's panic-hook posture (DG-4b): a thread whose progress another
///   thread's `join`/completion depends on must not run an unknown synchronous
///   sink on its panic path, and "registered a pool" IS that obligation's fact
///   — the same fact the producer classification uses, so the two cannot drift
///   apart.
pub fn is_alpha_producer() -> bool {
    with_current_pool(|_| ()).is_some()
}

/// Process-wide count of panics the DG-4b hook observed on isolation-obligated
/// threads. Relaxed: it orders nothing. Read from non-critical threads (the
/// shutdown report), never rendered on the panicking thread.
static CRITICAL_PANICS: AtomicU64 = AtomicU64::new(0);

/// The panic record's static site tag (the site vocabulary for this facility).
const PANIC_SITE: &str = "panic-hook";
/// The panic record's fixed message text. It states what was observed and
/// nothing more: no thread name, no payload, no backtrace — the hook cannot
/// render and must not format.
const PANIC_MESSAGE: &str = "panic on an isolation-obligated thread";

/// The DG-4b panic-hook entry point: ATOMICS ONLY.
///
/// Per panic: one relaxed counter increment, always. Then, on a thread that
/// registered a class-α pool AND while the facility still admits (see
/// [`Diagnostics::shutdown`]), at most ONE bounded publication into that pool
/// ([`pool::AlphaPool::publish_panic_once`]); a thread with no registered pool
/// gets the count only — the hook never touches the class-β channel, whose
/// send path may take internal synchronization.
///
/// Not here, by contract: no lock, no allocation, no formatting, no `write(2)`,
/// no call into the previous hook, no call into [`emit`] (a panic inside the
/// hook must not recurse). The record is deliberately unstamped — `stamped()`
/// would touch the process clock's `OnceLock`, and a panic hook must not be the
/// thread that initializes it.
pub fn note_critical_panic() -> Disposition {
    CRITICAL_PANICS.fetch_add(1, Ordering::Relaxed);
    let Some(inner) = FACILITY.get() else {
        return publish_panic_record();
    };
    // The publication decision and the publication sit inside ONE admission
    // section, exactly as in `emit`: after the drainer's quiescence observation
    // there is no publication in flight, so the final sweep cannot miss this
    // one. The increment above — taken unconditionally, before any decision — is
    // the authoritative fact, and it is already recorded.
    let (admitted, _section) = enter_section(inner);
    if !admitted {
        // The closure was already published: no consumer is left for the
        // one-shot publication, and the counter above IS the record of the
        // panic.
        return Disposition::Shutdown;
    }
    publish_panic_record()
}

/// The one-shot panic publication into the calling thread's own pool (the hook
/// never touches the class-β channel, whose send path may take internal
/// synchronization).
fn publish_panic_record() -> Disposition {
    let record =
        Record::new(PANIC_SITE, Level::Error).with_message(Text::from_static(PANIC_MESSAGE));
    match with_current_pool(|pool| {
        if pool.publish_panic_once(record) {
            Disposition::AlphaRecorded
        } else {
            Disposition::PanicAlreadyReported
        }
    }) {
        Some(disposition) => disposition,
        None => Disposition::NoSink,
    }
}

/// Panics observed on isolation-obligated threads since process start.
pub fn critical_panics() -> u64 {
    CRITICAL_PANICS.load(Ordering::Relaxed)
}

/// Test-only scheduling seam: parks a producer at the position where the
/// admission section of a correct close protocol must still be held — after
/// the closure decision, before the publication — so a lifecycle oracle can
/// run the close and the final sweep against a publication that is in flight.
///
/// Compiled in test builds only (`cfg(all(test, not(loom)))`): production has
/// no seam, no rendezvous and no extra load on the producer path, so the
/// class-α "atomics only" property is untouched by this module. One-shot per
/// [`arm`]: the first publication after arming parks here, every later one is
/// untouched.
#[cfg(all(test, not(loom)))]
mod seam {
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    /// Which publication the parked producer was about to perform.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Transport {
        Alpha,
        Beta,
    }

    /// The liveness valve: a parked producer resumes on its own after this
    /// long, so a broken oracle fails its assertions instead of hanging the
    /// test binary forever. It is NOT a premise of any oracle — every oracle
    /// releases the producer only after observing a signal that the schedule
    /// it wants to test has been reached.
    const VALVE: Duration = Duration::from_secs(30);

    struct State {
        armed: bool,
        paused: Option<Transport>,
        released: bool,
    }

    static STATE: Mutex<State> = Mutex::new(State {
        armed: false,
        paused: None,
        released: false,
    });
    static RESUME: Condvar = Condvar::new();

    fn lock() -> std::sync::MutexGuard<'static, State> {
        STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Arm the seam for the next publication (the calling oracle's).
    pub(crate) fn arm() {
        let mut state = lock();
        state.armed = true;
        state.paused = None;
        state.released = false;
        RESUME.notify_all();
    }

    /// Disarm and resume (oracle cleanup; safe when nothing is parked).
    pub(crate) fn disarm() {
        let mut state = lock();
        state.armed = false;
        state.released = true;
        RESUME.notify_all();
    }

    /// The producer-path hook: park here until [`release`] (or the valve).
    pub(crate) fn pause(transport: Transport) {
        let mut state = lock();
        if !state.armed {
            return;
        }
        state.armed = false;
        state.paused = Some(transport);
        RESUME.notify_all();
        let (guard, _) = RESUME
            .wait_timeout_while(state, VALVE, |state| !state.released)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drop(guard);
    }

    /// Block until a producer is parked here, and report the transport it was
    /// about to publish through.
    pub(crate) fn wait_until_paused() -> Transport {
        let mut state = lock();
        let deadline = Instant::now() + Duration::from_secs(30);
        while state.paused.is_none() {
            let now = Instant::now();
            assert!(now < deadline, "no producer parked inside the seam");
            let (guard, _) = RESUME
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = guard;
        }
        state.paused.expect("just observed")
    }

    /// Resume the parked producer.
    pub(crate) fn release() {
        let mut state = lock();
        state.released = true;
        RESUME.notify_all();
    }
}

/// Emit one diagnostic record. Never blocks, never panics, never allocates on
/// the producer path, and never waits for a sink.
pub fn emit(record: Record) -> Disposition {
    match FACILITY.get() {
        Some(inner) => emit_through(inner, record),
        None => emit_without_facility(record),
    }
}

/// The class-α-only path when no facility is installed: a registered pool can
/// outlive the facility's absence (registration is an earlier, independent
/// act), and with no facility there is no closure to be ordered against, so
/// the calling thread's own pool is the only sink a presentation can reach.
fn emit_without_facility(record: Record) -> Disposition {
    match with_current_pool(|pool| {
        if pool.publish(record) {
            Disposition::AlphaRecorded
        } else {
            Disposition::AlphaRejected
        }
    }) {
        Some(disposition) => disposition,
        None => Disposition::NoSink,
    }
}

/// The production emission body, parameterized by the facility interior it
/// goes through. Split out of [`emit`] so the lifecycle oracles can drive the
/// REAL path against a detached interior: a process-wide installation is a
/// once-only, process-global act, and an oracle that installed it would decide
/// the fate of every other test in the binary.
fn emit_through(inner: &Inner, record: Record) -> Disposition {
    // ENTER the admission section BEFORE the closure decision: the one RMW
    // below is both the decision and — when admitted — this publication's claim
    // on the drainer's final sweep. A presentation the facility decides NOT to
    // admit is counted at its own class's boundary instead (the pool's, or the
    // channel's): never published, because the final sweep is the last consumer
    // there will be.
    let (admitted, _section) = enter_section(inner);
    if !admitted {
        let counted = with_current_pool(|pool| pool.reject_presentation()).is_some();
        // No pool on this thread: the class-β admission-loss counter is the
        // only home for the presentation.
        if !counted {
            inner.beta_rejected.fetch_add(1, Ordering::Relaxed);
        }
        return Disposition::Shutdown;
    }
    if let Some(disposition) = with_current_pool(|pool| {
        #[cfg(all(test, not(loom)))]
        seam::pause(seam::Transport::Alpha);
        if pool.publish(record) {
            Disposition::AlphaRecorded
        } else {
            Disposition::AlphaRejected
        }
    }) {
        return disposition;
    }
    if IS_DRAINER.with(Cell::get) {
        return Disposition::Reentry;
    }
    #[cfg(all(test, not(loom)))]
    seam::pause(seam::Transport::Beta);
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
            // Reachable only when an ABANDONED close let the drainer exit while
            // a publication was still in flight (the receiver drops with it).
            // The record is an admission loss at the channel's boundary, and the
            // report for that close did not claim a drain.
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
        // Cold path: the process clock starts here when nothing registered
        // before installation, so no producer ever initializes it.
        ensure_process_clock();
        let (beta_send, beta_recv) = std::sync::mpsc::sync_channel(config.beta_capacity);
        let inner = Arc::new(Inner {
            beta_send,
            beta_recv: Mutex::new(Some(beta_recv)),
            admission: AtomicUsize::new(0),
            drainer_exited: AtomicBool::new(false),
            sweep_quiescent: AtomicBool::new(false),
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
    ///
    /// The closing protocol. Admission and publication are ONE step as seen by
    /// this closure: [`emit`] and [`note_critical_panic`] take a single RMW on
    /// the facility's admission word whose closed bit is both their admission
    /// decision and — when admitted — their claim on the final sweep, and this
    /// method publishes the closure with one RMW on that same word. Exactly:
    ///
    /// * A presentation ADMITTED before the closure cannot be lost to it. Its
    ///   producer holds a counted publication, the drainer waits
    ///   ([`PUBLISHER_QUIESCENCE_WINDOW`]) for the last in-flight publication
    ///   before sweeping, and the sweep therefore either delivers the record or
    ///   charges it as an export loss.
    /// * A presentation DECIDED after the closure is never published: it is
    ///   dropped and counted at its own class's admission boundary (the
    ///   presenting pool's counters for α, `beta_rejected` for β).
    /// * Producer quiescence is not required of the host — the stdin reader has
    ///   a detached lifetime — and producers never wait: their whole
    ///   involvement is two RMWs. The side that waits is the DRAINER, bounded.
    /// * If that bound cannot be met (a producer stalled inside its section),
    ///   the sweep still runs — it is the last consumer there will be — but the
    ///   report is NOT `drained`. The accounting says "incomplete" instead of
    ///   presenting itself as a complete drain, and `abandoned` stays false
    ///   because the drainer did exit. What happens to such a presentation
    ///   afterwards is stated at its own admission boundary, never hidden: an α
    ///   publication lands in its pool (visible as `outstanding`, and a later
    ///   walk can still deliver it), while a β presentation finds the channel's
    ///   receiver gone with the drainer and is counted as a `beta_rejected`
    ///   admission loss.
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        // The closure: ONE RMW on the admission word, ordered against every
        // admission decision (which is an RMW on the same word).
        close_admission(&self.inner);
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
            // The drainer exited; whether this is a COMPLETE drain is its own
            // judgment, recorded when it saw (or failed to see) publisher
            // quiescence before its sweep.
            drained: self.inner.sweep_quiescent.load(Ordering::Acquire),
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
        critical_panics: critical_panics(),
    }
}

fn pool_totals() -> (u64, u64, u64) {
    let registry = alpha_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut rejected = registry.retired_rejected;
    let mut copied = registry.retired_copied;
    let mut outstanding = 0u64;
    for pool in registry.pools.iter() {
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
                // One bounded line, non-panicking, reported from the DRAINER
                // thread. Its own write result is deliberately dropped: the
                // sink just failed, and a failure *of the failure report* has
                // nowhere left to be counted.
                let _ = write_stderr_line(&format!(
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

/// Non-panicking stderr line writer. It returns the write result instead of
/// swallowing it: "did not panic" and "was delivered" are different facts, and
/// a caller that accounts delivery (an exporter, the host) needs the second
/// one. `writeln!`/`flush` return `Err` on a closed or broken descriptor; they
/// never panic. A blocked descriptor is not an error — it blocks this caller,
/// which is the declared residual of any blocking sink.
pub fn write_stderr_line(line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut stderr = std::io::stderr().lock();
    writeln!(stderr, "{line}")?;
    stderr.flush()
}

/// The stderr exporter's failure report (the exporter is disabled and the
/// record counted as lost when a write fails).
const STDERR_WRITE_FAILED: ExportError = ExportError {
    message: "stderr write failed",
};

/// Drain every live pool once, then release the ones whose producer is gone.
///
/// There is no per-pool cursor to look up: the walk position lives in the pool
/// core, so a poll is a single pass over the pools — and the release below is
/// what keeps that set from growing with the process's episode history.
fn poll_pools(inner: &Inner, exporter: &mut Option<Box<dyn Exporter>>, disabled: &mut bool) {
    let pools = registered_pools();
    for pool in &pools {
        pool.drain(&mut |record| {
            let _ = deliver(inner, exporter, disabled, record);
        });
        if pool.is_retired() {
            // The retirement was observed (Acquire) AFTER the walk above, so a
            // producer that published and then retired may have had its last
            // record missed by that walk. Walk once more: the Acquire load has
            // synchronized with the retirement, nothing can be published
            // afterwards (the registration is gone and the handle is
            // thread-bound), so this second walk IS the residual handoff and
            // leaves the pool empty.
            pool.drain(&mut |record| {
                let _ = deliver(inner, exporter, disabled, record);
            });
        }
    }
    reap_retired_pools();
}

/// Release every pool whose producer's registration is gone and whose records
/// are all delivered: fold its counters into the registry totals (so "what
/// happened" outlives "where it happened") and drop the pool. The snapshot in
/// `poll_pools` owns a reference until it goes out of scope, so a pool reaped
/// here is released only after the walk that used it has finished.
fn reap_retired_pools() {
    let mut registry = alpha_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut folded_rejected = 0u64;
    let mut folded_copied = 0u64;
    registry.pools.retain(|pool| {
        if pool.is_retired() && pool.outstanding() == 0 {
            folded_rejected += pool.rejected();
            folded_copied += pool.copied_out();
            false
        } else {
            true
        }
    });
    registry.retired_rejected += folded_rejected;
    registry.retired_copied += folded_copied;
}

/// The final sweep at the shutdown boundary: one last pool poll, every record
/// still queued on the channel, then the exporter's own flush.
fn final_sweep(
    inner: &Inner,
    receiver: &Receiver<Record>,
    exporter: &mut Option<Box<dyn Exporter>>,
    disabled: &mut bool,
) {
    poll_pools(inner, exporter, disabled);
    while let Ok(record) = receiver.try_recv() {
        inner.beta_consumed.fetch_add(1, Ordering::Relaxed);
        let _ = deliver(inner, exporter, disabled, &record);
    }
    let Some(sink) = exporter.as_mut() else {
        return;
    };
    if let Err(error) = sink.flush() {
        // A failed flush is a SINK-level outcome, not a record outcome: it can
        // be attributed to no particular record count, so it is charged a
        // self-diagnostic and never `export_failed`. The delivery contract
        // stays with `export`: one `Ok` there is exactly one delivered record,
        // one `Err` is that record's loss. (This branch runs only while the
        // exporter is still live — `deliver` drops and disables together, so
        // the `else` above already returned for a disabled one — and the
        // drainer exits right after, so no later record can observe it.)
        inner.self_diagnostics.fetch_add(1, Ordering::Relaxed);
        let _ = write_stderr_line(&format!(
            "[qianqian-diagnostics] exporter {} flush failed: {}",
            sink.name(),
            error.message
        ));
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

    // The drainer's poll obligation is a conditional FREQUENCY, not a latency
    // bound: every pool is polled after each batch and at least once per idle
    // receive window. While an exporter blocks there is no poll at all and no
    // delivery deadline — the declared residual.
    loop {
        poll_pools(&inner, &mut exporter, &mut disabled);
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
                final_sweep(&inner, &receiver, &mut exporter, &mut disabled);
                break;
            }
        }
        if is_closed(&inner) {
            // The final sweep is the last consumer there will be, so it must be
            // ordered after every publication the facility admitted: wait
            // (bounded) for the last in-flight one, then sweep. The outcome is
            // recorded for `shutdown` — a sweep that could not be ordered after
            // publisher quiescence is never reported as a complete drain.
            let quiescent = wait_for_publisher_quiescence(&inner, PUBLISHER_QUIESCENCE_WINDOW);
            inner.sweep_quiescent.store(quiescent, Ordering::Release);
            final_sweep(&inner, &receiver, &mut exporter, &mut disabled);
            break;
        }
    }
    inner.drainer_exited.store(true, Ordering::Release);
}

/// The built-in stderr exporter: one plain line per record, rendered on the
/// drainer, written with the non-panicking writer.
///
/// Exporter admission (T5-5 strategy A — every exporter states its progress
/// bound). This sink's bound: rendering is a bounded fixed-buffer format and the
/// write is a single `write`/`write_all` on a descriptor already open at
/// installation time — no open, no name resolution, no allocation growth. It is
/// therefore bounded for a descriptor that drains, and its residual is the
/// environment's: a full pipe with no reader (or a stalled filesystem behind
/// stderr) blocks the DRAINER inside the write for as long as the environment
/// keeps it blocked. That residual is not compensated anywhere: producers stay
/// isolated (class α) and the drainer is not, which is exactly why shutdown is
/// bounded-or-honest — the host abandons a stuck drainer at its deadline and
/// reports lost records instead of claiming a completed drain.
///
/// Blocked and FAILED are different outcomes, and this sink reports them
/// differently: a failed write (`EPIPE` on a closed reader, a bad descriptor, a
/// full disk) returns `Err`, so the record is counted as an export loss and
/// never as delivered. Only a write that is still in progress blocks.
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
        write_stderr_line(&line).map_err(|_| STDERR_WRITE_FAILED)
    }

    /// The final sweep's flush, propagated as its own outcome: an `Err` here is
    /// the SINK failing at the sweep boundary — the drainer reports it once as
    /// a self-diagnostic, and no record is charged to it (each record's verdict
    /// was its own `export` result).
    fn flush(&mut self) -> Result<(), ExportError> {
        use std::io::Write;
        std::io::stderr()
            .lock()
            .flush()
            .map_err(|_| STDERR_WRITE_FAILED)
    }
}

#[cfg(test)]
mod tests {
    // The lifecycle oracles below run on real threads and real time; under
    // `--cfg loom` this crate's atomics ARE loom's, so they are gated off the
    // schedule-exploration build exactly like the pool module's arithmetic
    // oracles.
    use super::*;

    /// Serializes this module's registration/poll oracles: the class-α registry
    /// is process-global, `poll_pools` walks every registered pool, and test
    /// functions run in parallel — without this guard one oracle's poll (or its
    /// live registration) leaks into another oracle's sink, counters or
    /// registry-length assertions.
    static SERIALIZE_POOL_TESTS: Mutex<()> = Mutex::new(());

    fn serialize_pool_tests() -> std::sync::MutexGuard<'static, ()> {
        SERIALIZE_POOL_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Drain and reap every registered pool through a throwaway facility
    /// interior, so a finished oracle leaves neither a live registration nor a
    /// retired pool with residual records in the process-global registry (the
    /// poll oracles in this module must find a clean registry).
    fn reap_this_threads_registration() {
        let inner = detached_inner();
        let mut exporter: Option<Box<dyn Exporter>> = None;
        let mut disabled = false;
        poll_pools(&inner, &mut exporter, &mut disabled);
    }

    fn record(marker: i64) -> Record {
        let mut message = Text::from_static("marker=");
        message.push_i64(marker);
        Record::new("facility-test", Level::Warn).with_message(message)
    }

    /// Collects the drained markers, for the poll oracles below.
    struct Collected {
        sink: Arc<Mutex<Vec<i64>>>,
    }

    impl Exporter for Collected {
        fn name(&self) -> &'static str {
            "collected"
        }

        fn export(&mut self, record: &Record) -> Result<(), ExportError> {
            if let Some(value) = record.message.render().strip_prefix("marker=") {
                self.sink
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(value.parse().expect("marker"));
            }
            Ok(())
        }
    }

    /// A facility interior with no drainer thread and no process-wide
    /// installation. The poll functions take an `&Inner`, so the pool
    /// lifecycle can be exercised without installing a facility (which is a
    /// process-global, once-only act).
    fn detached_inner() -> Inner {
        let (beta_send, beta_recv) = std::sync::mpsc::sync_channel(1);
        Inner {
            beta_send,
            beta_recv: Mutex::new(Some(beta_recv)),
            admission: AtomicUsize::new(0),
            drainer_exited: AtomicBool::new(false),
            sweep_quiescent: AtomicBool::new(false),
            beta_queued: AtomicU64::new(0),
            beta_rejected: AtomicU64::new(0),
            beta_consumed: AtomicU64::new(0),
            exported: AtomicU64::new(0),
            export_failed: AtomicU64::new(0),
            self_diagnostics: AtomicU64::new(0),
        }
    }

    /// Oracle (pool retirement, both directions): a LIVE producer's pool stays
    /// registered and is polled; a retired producer's pool has its residual
    /// records walked, its counters folded into the facility totals, and is
    /// then RELEASED — the registry tracks live producers, not the process's
    /// registration history.
    #[cfg(not(loom))]
    #[test]
    fn retired_pools_are_residual_drained_folded_and_released() {
        let _serial = serialize_pool_tests();
        let live_before = alpha_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pools
            .len();
        let inner = detached_inner();
        let sink = Arc::new(Mutex::new(Vec::new()));
        let mut exporter: Option<Box<dyn Exporter>> = Some(Box::new(Collected {
            sink: Arc::clone(&sink),
        }));
        let mut disabled = false;

        let registration = AlphaRegistration::register();
        let pool = registered_pools()
            .last()
            .cloned()
            .expect("the registration appended its pool");
        assert!(
            is_alpha_producer(),
            "this thread is class α while registered"
        );
        assert_eq!(emit(record(1)), Disposition::AlphaRecorded);
        assert_eq!(emit(record(2)), Disposition::AlphaRecorded);
        assert_eq!(pool.outstanding(), 2, "both records wait in the pool");
        assert_eq!(registration.accepted(), 2);
        assert!(!pool.is_retired(), "a live producer's pool is not retired");

        // A live pool is polled (its records are delivered) but NOT reaped.
        poll_pools(&inner, &mut exporter, &mut disabled);
        assert_eq!(
            *sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![1, 2],
            "a live pool's records reach the drainer's sink"
        );
        assert_eq!(pool.outstanding(), 0);
        assert_eq!(
            alpha_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pools
                .len(),
            live_before + 1,
            "a live producer's pool is never reaped"
        );

        // The producer leaves, and publishes nothing further: the thread's fast
        // path closes first, then retirement is published.
        drop(registration);
        assert!(
            !is_alpha_producer(),
            "the fast path is closed with the guard"
        );
        assert!(pool.is_retired());

        poll_pools(&inner, &mut exporter, &mut disabled);
        assert_eq!(
            alpha_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pools
                .len(),
            live_before,
            "the retired pool is released, not accumulated"
        );
        assert_eq!(
            Arc::strong_count(&pool),
            1,
            "the registry released the only reference it held"
        );
        let (_, copied, outstanding) = pool_totals();
        assert!(
            copied >= 2,
            "the released pool's counters were folded into the totals, not lost with it"
        );
        assert_eq!(outstanding, 0);
    }

    /// Oracle: a retired pool with records still in flight is NOT released
    /// before its residual walk — the release happens only after the drainer
    /// has emptied it, so no record is lost to the reclamation.
    #[cfg(not(loom))]
    #[test]
    fn a_retired_pool_with_residual_records_is_released_only_after_they_are_delivered() {
        let _serial = serialize_pool_tests();
        let live_before = alpha_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pools
            .len();
        let inner = detached_inner();
        let sink = Arc::new(Mutex::new(Vec::new()));
        let mut exporter: Option<Box<dyn Exporter>> = Some(Box::new(Collected {
            sink: Arc::clone(&sink),
        }));
        let mut disabled = false;

        let registration = AlphaRegistration::register();
        let pool = registered_pools().last().cloned().expect("registered");
        assert_eq!(emit(record(11)), Disposition::AlphaRecorded);
        assert_eq!(emit(record(12)), Disposition::AlphaRecorded);
        // Retire WITHOUT a poll in between: the pool is retired with two
        // records still unpublished to any sink.
        drop(registration);

        poll_pools(&inner, &mut exporter, &mut disabled);
        assert_eq!(
            *sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![11, 12],
            "the retirement handoff delivers the residual records"
        );
        assert_eq!(pool.outstanding(), 0, "and leaves nothing behind");
        assert_eq!(
            alpha_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pools
                .len(),
            live_before,
            "and only then is the pool released"
        );
    }

    /// Oracle (producer role, one per thread): a second registration on a thread
    /// that already holds one is REJECTED, and the rejection cannot disturb the
    /// live registration. Under the defect the second call overwrites the
    /// thread's fast path, and the first handle's `Drop` then clears it out from
    /// under the still-live second handle: a registered thread silently returns
    /// to the class-β path — and, for the DG-4b posture, back to chaining a
    /// panic into an unknown synchronous sink.
    #[cfg(not(loom))]
    #[test]
    fn a_second_registration_on_the_same_thread_is_rejected() {
        let _serial = serialize_pool_tests();
        let registration = AlphaRegistration::register();
        assert!(is_alpha_producer());
        assert_eq!(emit(record(1)), Disposition::AlphaRecorded);

        let duplicate = std::panic::catch_unwind(AlphaRegistration::register);
        let payload = match duplicate {
            Err(payload) => payload,
            Ok(_) => panic!("the duplicate registration must be rejected"),
        };
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .unwrap_or("<non-string panic payload>");
        assert!(
            message.contains("one registration per thread"),
            "the rejection states the contract: {message:?}"
        );

        // The live registration is untouched by the rejected attempt.
        assert!(
            is_alpha_producer(),
            "the fast path still belongs to the first handle"
        );
        assert_eq!(emit(record(2)), Disposition::AlphaRecorded);
        assert_eq!(
            registration.accepted(),
            2,
            "both publishes went to its pool"
        );
        assert_eq!(registration.rejected(), 0);

        // And that handle's own drop still owns the fact.
        drop(registration);
        assert!(
            !is_alpha_producer(),
            "the drop closes the thread's fast path"
        );
        reap_this_threads_registration();
    }

    /// Oracle: after a registration drops, the thread may register again — the
    /// one-per-thread rule is about LIVE registrations, not a thread's lifetime.
    #[cfg(not(loom))]
    #[test]
    fn a_thread_may_register_again_after_its_registration_drops() {
        let _serial = serialize_pool_tests();
        let first = AlphaRegistration::register();
        drop(first);
        assert!(!is_alpha_producer());

        let second = AlphaRegistration::register();
        assert!(is_alpha_producer(), "re-registration after a drop is legal");
        assert_eq!(emit(record(3)), Disposition::AlphaRecorded);
        assert_eq!(second.accepted(), 1, "the new registration starts fresh");
        drop(second);
        assert!(!is_alpha_producer());
        reap_this_threads_registration();
    }

    /// Oracle (non-LIFO drop order): registration lifetimes on DIFFERENT threads
    /// are independent — the later-created registration may drop first, and
    /// neither drop touches the other thread's role fact. Once same-thread
    /// duplicates are rejected this is the only legal order-variation (in-thread
    /// non-LIFO becomes unconstructible rather than merely handled).
    #[cfg(not(loom))]
    #[test]
    fn registration_lifetimes_are_per_thread_and_drop_order_is_irrelevant() {
        let _serial = serialize_pool_tests();
        let first_registration = AlphaRegistration::register();
        let (report, observations) = std::sync::mpsc::channel();
        let other = std::thread::spawn(move || {
            let registration = AlphaRegistration::register();
            assert!(is_alpha_producer(), "the second thread has its own fact");
            assert_eq!(emit(record(4)), Disposition::AlphaRecorded);
            drop(registration);
            report.send(is_alpha_producer()).expect("report home");
        });
        assert!(
            !observations.recv().expect("the second thread reports"),
            "its own drop closes only its own fast path"
        );
        other.join().expect("the registering thread");
        // The FIRST registration outlives the second (non-LIFO): its role fact
        // and its pool are untouched.
        assert!(is_alpha_producer(), "the first thread's fact survived");
        assert_eq!(emit(record(5)), Disposition::AlphaRecorded);
        assert_eq!(first_registration.accepted(), 1);
        drop(first_registration);
        assert!(!is_alpha_producer());
        reap_this_threads_registration();
    }

    /// An exporter that delivers every record and then fails its FLUSH: the
    /// legitimate `Ok(export)` / `Err(flush)` pairing the delivery contract
    /// must account correctly.
    struct FlushFailing {
        exports: Arc<Mutex<usize>>,
    }

    impl Exporter for FlushFailing {
        fn name(&self) -> &'static str {
            "flush-failing"
        }

        fn export(&mut self, _record: &Record) -> Result<(), ExportError> {
            *self
                .exports
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ExportError> {
            Err(ExportError {
                message: "the test sink cannot flush",
            })
        }
    }

    /// Oracle: a failed FLUSH is a sink-level self-diagnostic, never a record
    /// outcome. `Exporter::export` returning `Ok` IS the record's delivery —
    /// one delivered record, counted once — and a flush failure cannot change
    /// that count, because it has no record to be attributed to. Charging it to
    /// `export_failed` breaks the drain-boundary identity
    /// (`copied_out + beta_consumed == exported + export_failed`) at quiescence,
    /// and the resulting over-resolution is exactly what `undelivered_now`'s
    /// saturating subtraction would then hide.
    #[cfg(not(loom))]
    #[test]
    fn a_failed_flush_is_a_self_diagnostic_not_a_lost_record() {
        let _serial = serialize_pool_tests();
        let inner = detached_inner();
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        sender.try_send(record(1)).expect("the record is queued");
        let exports = Arc::new(Mutex::new(0usize));
        let mut exporter: Option<Box<dyn Exporter>> = Some(Box::new(FlushFailing {
            exports: Arc::clone(&exports),
        }));
        let mut disabled = false;

        final_sweep(&inner, &receiver, &mut exporter, &mut disabled);

        assert_eq!(
            *exports
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            1,
            "the record was handed to the sink exactly once"
        );
        let consumed = inner.beta_consumed.load(Ordering::Relaxed);
        let exported = inner.exported.load(Ordering::Relaxed);
        let export_failed = inner.export_failed.load(Ordering::Relaxed);
        let self_diagnostics = inner.self_diagnostics.load(Ordering::Relaxed);
        assert_eq!(consumed, 1, "one queued record was pulled");
        assert_eq!(
            exported, 1,
            "an Ok from export is that record's delivery: one record, one export"
        );
        assert_eq!(
            export_failed, 0,
            "a flush failure has no record to be charged against"
        );
        assert_eq!(
            self_diagnostics, 1,
            "the flush failure is reported once as a sink-level self-diagnostic"
        );
        assert_eq!(
            exported + export_failed,
            consumed,
            "the drain-boundary identity holds at quiescence"
        );
    }

    // --- The close protocol: publications in flight across the closure ------
    //
    // The oracle shape (see `seam`): a producer is parked inside the seam —
    // after the closure decision, before the publication — while the close and
    // the final sweep run. Both the premise and the release are established
    // through signals the oracle observes (the seam's Paused state, the sink's
    // final-sweep flush, and the ShutdownReport itself); the one bounded
    // quantity is the release valve, which is what lets the "delivered" oracle
    // distinguish "the drainer swept first" from "the drainer was waiting for
    // this producer".

    /// The close-race probe: delivers records to the oracle and signals every
    /// FINAL SWEEP that reaches it through [`Exporter::flush`] — the observable
    /// "the sweep has run" fact.
    #[cfg(not(loom))]
    #[derive(Clone)]
    struct CloseRaceProbe {
        delivered: Arc<Mutex<Vec<i64>>>,
        sweeps: Arc<AtomicU64>,
    }

    #[cfg(not(loom))]
    impl Exporter for CloseRaceProbe {
        fn name(&self) -> &'static str {
            "close-race-probe"
        }

        fn export(&mut self, record: &Record) -> Result<(), ExportError> {
            if let Some(value) = record.message.render().strip_prefix("marker=") {
                self.delivered
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(value.parse().expect("marker"));
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ExportError> {
            self.sweeps.fetch_add(1, Ordering::Release);
            Ok(())
        }
    }

    /// A detached facility interior with its OWN real drainer thread: the whole
    /// production close path — closure, quiescence wait, final sweep, report —
    /// without the process-wide installation that would decide the fate of
    /// every other oracle in the binary.
    #[cfg(not(loom))]
    struct CloseRace {
        inner: Arc<Inner>,
        diagnostics: Diagnostics,
        delivered: Arc<Mutex<Vec<i64>>>,
        sweeps: Arc<AtomicU64>,
    }

    #[cfg(not(loom))]
    impl CloseRace {
        fn new() -> Self {
            let inner = Arc::new(detached_inner());
            let receiver = inner
                .beta_recv
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
                .expect("a fresh interior owns its receiver");
            let delivered = Arc::new(Mutex::new(Vec::new()));
            let sweeps = Arc::new(AtomicU64::new(0));
            let probe = CloseRaceProbe {
                delivered: Arc::clone(&delivered),
                sweeps: Arc::clone(&sweeps),
            };
            let handle = std::thread::Builder::new()
                .name("close-race-drainer".into())
                .spawn({
                    let inner = Arc::clone(&inner);
                    move || drainer(inner, receiver, Box::new(probe), Duration::from_millis(5))
                })
                .expect("the probe drainer starts");
            Self {
                diagnostics: Diagnostics {
                    inner: Arc::clone(&inner),
                    drainer: Mutex::new(Some(handle)),
                },
                inner,
                delivered,
                sweeps,
            }
        }

        fn delivered_markers(&self) -> Vec<i64> {
            self.delivered
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        /// Whether a final sweep reached the sink inside `window`.
        fn wait_for_sweep(&self, window: Duration) -> bool {
            let deadline = Instant::now() + window;
            loop {
                if self.sweeps.load(Ordering::Acquire) > 0 {
                    return true;
                }
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// What the producing thread could observe on its OWN path, before its
    /// registration dropped. The `Counters` of a detached harness are NOT usable
    /// for class-α assertions: their pool half is summed from the process-global
    /// registry, which folds in (and never unfolds) every other oracle's pools
    /// for the whole life of the test binary.
    #[derive(Clone, Copy, Debug)]
    struct ProducerReport {
        disposition: Disposition,
        /// The producing thread's own pool: (accepted, copied_out, outstanding).
        pool: Option<(u64, u64, u32)>,
    }

    /// Spawn the parked producer: a registered class-α thread when the oracle
    /// arms the α transport (the registration lives for the whole emission), a
    /// plain host thread for β.
    #[cfg(not(loom))]
    fn spawn_close_race_producer(
        race: &CloseRace,
        transport: seam::Transport,
        marker: i64,
    ) -> std::thread::JoinHandle<ProducerReport> {
        let inner = Arc::clone(&race.inner);
        std::thread::spawn(move || match transport {
            seam::Transport::Alpha => {
                let registration = AlphaRegistration::register();
                let disposition = emit_through(&inner, record(marker));
                let pool = Some((
                    registration.accepted(),
                    registration.copied_out(),
                    registration.outstanding(),
                ));
                drop(registration);
                ProducerReport { disposition, pool }
            }
            seam::Transport::Beta => ProducerReport {
                disposition: emit_through(&inner, record(marker)),
                pool: None,
            },
        })
    }

    /// How the "delivered" oracle's release was decided.
    #[cfg(not(loom))]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ReleaseReason {
        /// A final sweep had already reached the sink: the drainer swept BEFORE
        /// the release, so a producer released now publishes into a facility
        /// whose last consumer is gone.
        SweptFirst,
        /// No sweep inside the valve: the drainer cannot have swept — it is the
        /// one waiting for this producer.
        Valved,
    }

    /// The release valve for the "delivered" oracles. It must stay well inside
    /// the drainer's quiescence window (`PUBLISHER_QUIESCENCE_WINDOW`), because
    /// the release has to precede that window's expiry for the protocol to
    /// deliver the in-flight record: 150 ms against 250 ms.
    #[cfg(not(loom))]
    const RELEASE_VALVE: Duration = Duration::from_millis(150);
    /// Generous: the closer only ever waits for the drainer's own bounded work.
    #[cfg(not(loom))]
    const CLOSE_RACE_DEADLINE: Duration = Duration::from_secs(30);

    /// Oracle (close protocol, both transports): a record admitted BEFORE the
    /// closure — the producer has already made its admission decision and is
    /// parked before the publication — is delivered by the final sweep when the
    /// producer completes inside the quiescence window. The release is decided
    /// by the sink's own flush signal: a release under `SweptFirst` means the
    /// schedule the protocol must survive was actually reached.
    #[cfg(not(loom))]
    fn an_in_flight_publication_is_delivered_by_the_final_sweep(
        transport: seam::Transport,
        marker: i64,
    ) {
        let _serial = serialize_pool_tests();
        let race = CloseRace::new();
        seam::arm();
        let producer = spawn_close_race_producer(&race, transport, marker);
        assert_eq!(
            seam::wait_until_paused(),
            transport,
            "the producer must park inside its own transport's section"
        );

        let report = Mutex::new(None);
        let release = Mutex::new(None);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let outcome = race.diagnostics.shutdown(CLOSE_RACE_DEADLINE);
                *report.lock().unwrap_or_else(|p| p.into_inner()) = Some(outcome);
            });
            scope.spawn(|| {
                let reason = if race.wait_for_sweep(RELEASE_VALVE) {
                    ReleaseReason::SweptFirst
                } else {
                    ReleaseReason::Valved
                };
                *release.lock().unwrap_or_else(|p| p.into_inner()) = Some(reason);
                seam::release();
            });
        });
        let producer = producer.join().expect("the parked producer");
        let release = release
            .into_inner()
            .unwrap_or_else(|p| p.into_inner())
            .expect("the releaser recorded its reason");
        let report = report
            .into_inner()
            .unwrap_or_else(|p| p.into_inner())
            .expect("the closer recorded its report");

        let hits = race
            .delivered_markers()
            .into_iter()
            .filter(|delivered| *delivered == marker)
            .count();
        assert_eq!(
            hits, 1,
            "a record admitted before the closure must be delivered by the final \
             sweep, exactly once (release decided by {release:?})"
        );
        assert!(
            release == ReleaseReason::Valved,
            "the release must be what let the sweep proceed: {release:?}"
        );
        assert!(
            matches!(
                producer.disposition,
                Disposition::AlphaRecorded | Disposition::BetaQueued
            ),
            "the presentation was admitted before the closure: {producer:?}"
        );
        assert_eq!(
            race.inner.exported.load(Ordering::Relaxed),
            1,
            "the sink's own delivery counter: the in-flight record was handed to it once"
        );
        assert!(
            report.drained,
            "the close completed a full drain: {report:?}"
        );
        assert!(!report.abandoned, "{report:?}");
        // `undelivered_at_exit` and the drain-boundary identity are deliberately
        // NOT asserted from this detached harness: their class-α half is summed
        // from the process-global registry, whose retained totals belong to
        // every other oracle in the binary as well. Those properties are
        // asserted where they are real — on an installed facility, by
        // `tests/shutdown_admission.rs` and the host's drained-report witness.
        seam::disarm();
    }

    /// Oracle (close protocol, both transports): a record admitted before the
    /// closure whose producer OUTLASTS the quiescence window makes the report
    /// say so. The report must not claim a complete drain while a presentation
    /// admitted before the closure is still in flight; the record itself is
    /// not lost to the accounting (its boundary counters stay consistent and a
    /// later walk still delivers it).
    #[cfg(not(loom))]
    fn a_publication_outlasting_the_quiescence_window_is_not_reported_drained(
        transport: seam::Transport,
        marker: i64,
    ) {
        let _serial = serialize_pool_tests();
        let race = CloseRace::new();
        seam::arm();
        let producer = spawn_close_race_producer(&race, transport, marker);
        assert_eq!(seam::wait_until_paused(), transport, "parked");

        // The producer stays parked past the drainer's quiescence window; the
        // report is built and returned while it is still in flight, so the
        // release below is strictly ordered AFTER the report (a signal, not a
        // guess).
        let report = race.diagnostics.shutdown(CLOSE_RACE_DEADLINE);
        assert!(
            !report.drained,
            "a drain must not be claimed while a presentation admitted before \
             the closure is still in flight: {report:?}"
        );
        assert!(
            !report.abandoned,
            "the drainer exited; what failed is the ordering, not the sink: {report:?}"
        );

        seam::release();
        let producer = producer.join().expect("the parked producer");
        match transport {
            seam::Transport::Alpha => {
                // Admitted before the closure: the record is published into its
                // own pool (the close does not retract an admission), where it
                // is visible at its own boundary — never invisible — and a later
                // walk still delivers it. The report simply did not claim to
                // have done so.
                assert_eq!(
                    producer.disposition,
                    Disposition::AlphaRecorded,
                    "the presentation was admitted before the closure: {producer:?}"
                );
                let (accepted, copied_out, outstanding) =
                    producer.pool.expect("an α producer reports its pool");
                assert_eq!(
                    accepted, 1,
                    "the presentation was accepted at its own pool boundary"
                );
                assert_eq!(
                    copied_out, 0,
                    "the last sweep had already run without it (that is what the report said)"
                );
                assert_eq!(
                    outstanding, 1,
                    "the unevaluated record is visible as outstanding, never invisible"
                );
                let mut exporter: Option<Box<dyn Exporter>> = Some(Box::new(CloseRaceProbe {
                    delivered: Arc::clone(&race.delivered),
                    sweeps: Arc::clone(&race.sweeps),
                }));
                let mut disabled = false;
                poll_pools(&race.inner, &mut exporter, &mut disabled);
                let hits = race
                    .delivered_markers()
                    .into_iter()
                    .filter(|delivered| *delivered == marker)
                    .count();
                assert_eq!(
                    hits, 1,
                    "the in-flight record was never lost: a later walk delivers it"
                );
            }
            seam::Transport::Beta => {
                // Admitted before the closure, but by the time its section left,
                // the incomplete close had already dropped the channel's
                // receiver: the presentation cannot be queued. It is therefore
                // an ADMISSION LOSS at the channel's own boundary — an
                // accountable rejection fact, never an invisible record.
                assert_eq!(
                    producer.disposition,
                    Disposition::Shutdown,
                    "with the receiver gone the presentation cannot be queued: {producer:?}"
                );
                assert_eq!(
                    race.inner.beta_queued.load(Ordering::Relaxed),
                    0,
                    "nothing was queued into a channel whose consumer is gone"
                );
                assert!(
                    race.inner.beta_rejected.load(Ordering::Relaxed) >= 1,
                    "the admission loss is counted at the channel's boundary"
                );
            }
        }
        seam::disarm();
    }

    /// Oracle (close protocol): a class-α record admitted before the closure
    /// and published only after the quiescence window is still delivered when
    /// its producer completes inside that window.
    #[cfg(not(loom))]
    #[test]
    fn an_in_flight_alpha_publication_is_delivered_by_the_final_sweep() {
        an_in_flight_publication_is_delivered_by_the_final_sweep(seam::Transport::Alpha, 21);
        reap_this_threads_registration();
    }

    /// Oracle (close protocol): the class-β half of the same property — the
    /// channel's own window is closed by the same admission section.
    #[cfg(not(loom))]
    #[test]
    fn an_in_flight_beta_publication_is_delivered_by_the_final_sweep() {
        an_in_flight_publication_is_delivered_by_the_final_sweep(seam::Transport::Beta, 22);
    }

    /// Oracle (no false completion, α): the report never claims a drained
    /// facility while a presentation admitted before the closure is in flight.
    #[cfg(not(loom))]
    #[test]
    fn an_alpha_publication_outlasting_the_quiescence_window_is_not_reported_drained() {
        a_publication_outlasting_the_quiescence_window_is_not_reported_drained(
            seam::Transport::Alpha,
            23,
        );
        reap_this_threads_registration();
    }

    /// Oracle (no false completion, β): the same property for the channel.
    #[cfg(not(loom))]
    #[test]
    fn a_beta_publication_outlasting_the_quiescence_window_is_not_reported_drained() {
        a_publication_outlasting_the_quiescence_window_is_not_reported_drained(
            seam::Transport::Beta,
            24,
        );
    }
}
