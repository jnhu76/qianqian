//! Real-resource lifecycle witnesses for the composition root and kernel.
//!
//! **Executable evidence, test-only.** The lifecycle mechanism under test is
//! the generic kernel's effect/unwind/withdrawal machinery, exercised through
//! the runtime's public admission and disposal seams. Every lifecycle object
//! here is real — a spawned OS thread joined through its `JoinHandle`, and a
//! real `std::fs::File` in the platform temp directory. Drop sentinels and the
//! shared sequence trace are observation instrumentation around those real
//! resources, never substitutes for them, and a release observation is always
//! recorded strictly AFTER the real destruction it reports.
//!
//! These witnesses prove ownership ordering only: stop → join → release under
//! activation failure, provider withdrawal and explicit root disposal. The
//! worker threads are plain best-effort threads; nothing here claims realtime
//! or audio-device safety, which only real plugin work can earn.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use qianqian_app::QianqianApp;
use qianqian_composition::{
    ActivationError, Capability, ComponentSpec, DesiredEntry, Discharge, FiberState, Revision,
};

// ---------------------------------------------------------------------------
// Observation instrumentation (test-only; the resources themselves are real)
// ---------------------------------------------------------------------------

/// Append-only order witness for lifecycle events, shared across the test
/// thread, the kernel and the real worker threads.
#[derive(Clone)]
struct Sequence(Arc<Mutex<Vec<&'static str>>>);

impl Sequence {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn push(&self, label: &'static str) {
        self.0.lock().expect("sequence lock").push(label);
    }

    fn position(&self, label: &str) -> Option<usize> {
        self.0
            .lock()
            .expect("sequence lock")
            .iter()
            .position(|l| *l == label)
    }

    fn contains(&self, label: &str) -> bool {
        self.position(label).is_some()
    }

    /// True only when both events were observed and `first` preceded `second`.
    fn before(&self, first: &str, second: &str) -> bool {
        match (self.position(first), self.position(second)) {
            (Some(a), Some(b)) => a < b,
            _ => false,
        }
    }

    fn labels(&self) -> Vec<&'static str> {
        self.0.lock().expect("sequence lock").clone()
    }
}

/// A real OS thread with a real stop signal, observed through a real join.
/// The whole value is moved into the teardown inverse so the episode cannot
/// close while the worker is alive.
struct RealWorker {
    stop: Arc<AtomicBool>,
    heartbeat: Arc<AtomicU64>,
    handle: JoinHandle<()>,
}

impl RealWorker {
    /// Spawn a real thread that loops until the stop flag is observed.
    fn spawn(exit_label: &'static str, seq: Sequence) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let heartbeat = Arc::new(AtomicU64::new(0));
        let worker_stop = stop.clone();
        let worker_heartbeat = heartbeat.clone();
        let handle = thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                worker_heartbeat.fetch_add(1, Ordering::SeqCst);
                thread::yield_now();
            }
            seq.push(exit_label);
        });
        Self {
            stop,
            heartbeat,
            handle,
        }
    }

    /// Bounded wait for observable forward progress of the real thread.
    fn assert_running(&self) {
        for _ in 0..5_000 {
            if self.heartbeat.load(Ordering::SeqCst) > 0 {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("the spawned worker thread never made observable progress");
    }

    /// The real release: signal the real flag, then block on the real join.
    fn stop_and_join(self, seq: &Sequence) {
        seq.push("stop_signalled");
        self.stop.store(true, Ordering::SeqCst);
        self.handle.join().expect("worker joins cleanly");
        seq.push("worker_joined");
    }
}

/// A real OS file plus release observation. The file is the resource; the
/// flags and sequence entries are instrumentation. Ordering guarantee: the
/// real `File` value is explicitly destroyed first (the std drop glue closes
/// the real OS handle — the documented `File` destructor contract), then the
/// destruction is recorded, then the release observation.
struct ObservedFile {
    /// `Some` until the real file has been explicitly destroyed in `drop`.
    file: Option<File>,
    released: Arc<AtomicBool>,
    seq: Sequence,
}

impl ObservedFile {
    /// Create a real file with real bytes at `path`.
    fn create_at(path: &Path, released: Arc<AtomicBool>, seq: Sequence) -> Self {
        let mut file = File::create(path).expect("temp file creation");
        file.write_all(b"lifecycle-witness")
            .expect("temp file write");
        file.flush().expect("temp file flush");
        Self {
            file: Some(file),
            released,
            seq,
        }
    }
}

impl Drop for ObservedFile {
    fn drop(&mut self) {
        // Destroy the real File FIRST: `std::fs::File` owns the actual OS
        // handle and its destructor closes it. Only after that destruction
        // has happened may the release observations be recorded — they must
        // witness an already-true fact, never one still pending.
        drop(self.file.take());
        self.seq.push("real_file_destructed");
        self.released.store(true, Ordering::SeqCst);
        self.seq.push("file_handle_dropped");
    }
}

fn unique_temp_path(label: &'static str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "qianqian_lifecycle_{label}_{}_{}.tmp",
        std::process::id(),
        n
    ))
}

fn desired(id: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, id, Revision::new(1))
}

fn mount(runtime: &mut QianqianApp, id: &'static str, spec: ComponentSpec) {
    runtime
        .register_component(spec)
        .expect("legal registration");
    runtime
        .revise_desired(vec![desired(id)])
        .expect("legal desired composition");
}

// ---------------------------------------------------------------------------
// Component spec builders (test plugins; the resources they own are real)
// ---------------------------------------------------------------------------

/// Service published by the thread-owning provider. The Drop sentinel
/// observes the kernel releasing the provision (the service value drops with
/// its effect record); the thread is the real resource.
struct ObservedService {
    released: Arc<AtomicBool>,
    seq: Sequence,
}

impl Drop for ObservedService {
    fn drop(&mut self) {
        self.released.store(true, Ordering::SeqCst);
        self.seq.push("service_dropped");
    }
}

struct WorkerCapability;

impl Capability for WorkerCapability {
    const NAME: &'static str = "LifecycleWorker";
    type Service = ObservedService;
}

/// Provider spec owning a real worker thread. Provision first, stop-and-join
/// inverse second: LIFO unwind must run stop → join before the provision
/// service releases.
fn thread_owner_spec(seq: Sequence, released: Arc<AtomicBool>) -> ComponentSpec {
    ComponentSpec::new("thread_owner")
        .provides::<WorkerCapability>()
        .on_activate(move |ctx| {
            let worker = RealWorker::spawn("worker_body_exited", seq.clone());
            worker.assert_running();
            let service = Rc::new(ObservedService {
                released: released.clone(),
                seq: seq.clone(),
            });
            ctx.provide::<WorkerCapability>(service)
                .expect("provides declared");
            let seq_for_inverse = seq.clone();
            ctx.register_effect(move || {
                worker.stop_and_join(&seq_for_inverse);
                Discharge::Discharged
            });
            Ok(())
        })
}

struct SharedFileCapability;

impl Capability for SharedFileCapability {
    const NAME: &'static str = "SharedFile";
    type Service = ObservedFile;
}

/// Provider spec publishing a real file as the shared service.
fn file_provider_spec(path: PathBuf, seq: Sequence, released: Arc<AtomicBool>) -> ComponentSpec {
    ComponentSpec::new("file_provider")
        .provides::<SharedFileCapability>()
        .on_activate(move |ctx| {
            let file = ObservedFile::create_at(&path, released.clone(), seq.clone());
            ctx.provide::<SharedFileCapability>(Rc::new(file))
                .expect("provides declared");
            Ok(())
        })
}

/// Consumer spec: resolves the shared file, registers the relation, and holds
/// a real worker across the episode. Relation first, stop-and-join inverse
/// second: LIFO unwind must join the worker before the relation releases.
fn file_consumer_spec(seq: Sequence) -> ComponentSpec {
    ComponentSpec::new("file_consumer")
        .requires::<SharedFileCapability>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<SharedFileCapability>()
                .expect("an active provider exists");
            let seq_for_relation = seq.clone();
            ctx.register_relation(&binding, move || {
                seq_for_relation.push("relation_released");
                Discharge::Discharged
            });
            let service = binding.service();
            let worker = RealWorker::spawn("worker_body_exited", seq.clone());
            worker.assert_running();
            let seq_for_inverse = seq.clone();
            ctx.register_effect(move || {
                seq_for_inverse.push("consumer_quiescing");
                worker.stop_and_join(&seq_for_inverse);
                drop(service); // the episode's own service reference dies here
                Discharge::Discharged
            });
            Ok(())
        })
}

/// Owner-local spec: the registered inverse OWNS the real file, so the open
/// handle lives exactly as long as the episode.
fn file_owner_spec(
    name: &'static str,
    path: PathBuf,
    seq: Sequence,
    released: Arc<AtomicBool>,
) -> ComponentSpec {
    ComponentSpec::new(name).on_activate(move |ctx| {
        let observed = ObservedFile::create_at(&path, released.clone(), seq.clone());
        let seq_for_inverse = seq.clone();
        ctx.register_effect(move || {
            seq_for_inverse.push("releasing_file");
            drop(observed); // closes the real handle; Drop records it
            Discharge::Discharged
        });
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Witnesses
// ---------------------------------------------------------------------------

/// W1 (E1/E2/E3) — a real thread runs while its fiber is active; explicit
/// disposal signals stop and joins the thread BEFORE the kernel releases the
/// provision service.
#[test]
fn worker_thread_stops_and_joins_before_service_release() {
    let seq = Sequence::new();
    let released = Arc::new(AtomicBool::new(false));

    let mut runtime = QianqianApp::new();
    mount(
        &mut runtime,
        "thread_owner",
        thread_owner_spec(seq.clone(), released.clone()),
    );

    let snap = runtime.composition_snapshot();
    assert_eq!(
        snap.fibers.get("thread_owner").map(|f| f.state),
        Some(FiberState::Active)
    );
    assert!(
        !released.load(Ordering::SeqCst),
        "service held while active"
    );

    let snap = runtime.dispose();

    assert!(
        seq.contains("stop_signalled"),
        "the teardown inverse signalled the real stop flag"
    );
    assert!(
        seq.before("worker_body_exited", "worker_joined"),
        "the real thread body exited before join() returned; got {:?}",
        seq.labels()
    );
    assert!(
        seq.before("worker_joined", "service_dropped"),
        "the real join completed before the provision service was released; got {:?}",
        seq.labels()
    );
    assert!(released.load(Ordering::SeqCst), "service released");
    assert!(snap.fibers.is_empty(), "the composition drained");
    assert!(snap.quiet);
}

/// W2 (E4) — activation owns a real file; explicit disposal runs the inverse
/// that releases the real handle. The file's bytes stay OS-observable through
/// an independent open, before and after release.
#[test]
fn os_file_handle_released_by_registered_inverse() {
    let seq = Sequence::new();
    let path = unique_temp_path("owner");

    let mut runtime = QianqianApp::new();
    mount(
        &mut runtime,
        "file_owner",
        file_owner_spec(
            "file_owner",
            path.clone(),
            seq.clone(),
            Arc::new(AtomicBool::new(false)),
        ),
    );

    let snap = runtime.composition_snapshot();
    assert_eq!(
        snap.fibers.get("file_owner").map(|f| f.state),
        Some(FiberState::Active)
    );
    assert_eq!(
        std::fs::read(&path).expect("real file readable while the episode holds it"),
        b"lifecycle-witness",
        "the resource is a real file with real bytes, not a counter"
    );

    let snap = runtime.dispose();

    assert!(
        seq.before("releasing_file", "real_file_destructed"),
        "the registered inverse released the real file; got {:?}",
        seq.labels()
    );
    assert!(
        seq.before("real_file_destructed", "file_handle_dropped"),
        "the release observation is recorded only after the real File value was destroyed (std drop glue closed the OS handle); got {:?}",
        seq.labels()
    );
    assert!(snap.fibers.is_empty());
    assert!(snap.quiet);
    // Supplemental filesystem cleanup only: POSIX permits unlinking an open
    // file, so successful removal is NOT a cross-platform handle-close
    // oracle. The release evidence is the ordered trace above: the real File
    // is destroyed before the observation is recorded.
    std::fs::remove_file(&path).expect("temp file cleanup");
}

/// W3 (E5) — a raising activation unwinds every real resource it acquired,
/// strictly LIFO: the thread acquired last is stopped and joined first, the
/// file acquired first is released last; the fiber lands FAILED with no ghost
/// provisions and the root stays quiet.
#[test]
fn partial_activation_failure_unwinds_real_resources_lifo() {
    let seq = Sequence::new();
    let seq_for_spec = seq.clone();
    let file_released = Arc::new(AtomicBool::new(false));
    let file_released_for_spec = file_released.clone();
    let path = unique_temp_path("partial");
    let path_for_cleanup = path.clone();

    let mut runtime = QianqianApp::new();
    runtime
        .register_component(ComponentSpec::new("partial").on_activate(move |ctx| {
            // 1. Acquire real file A; register its cleanup.
            let file_a = ObservedFile::create_at(
                &path,
                file_released_for_spec.clone(),
                seq_for_spec.clone(),
            );
            let seq_for_cleanup_a = seq_for_spec.clone();
            ctx.register_effect(move || {
                seq_for_cleanup_a.push("cleanup_file_a");
                drop(file_a);
                Discharge::Discharged
            });
            // 2. Spawn real thread B; register its cleanup.
            let worker = RealWorker::spawn("worker_body_exited", seq_for_spec.clone());
            worker.assert_running();
            let seq_for_cleanup_b = seq_for_spec.clone();
            ctx.register_effect(move || {
                seq_for_cleanup_b.push("cleanup_thread_b");
                worker.stop_and_join(&seq_for_cleanup_b);
                Discharge::Discharged
            });
            // 3. Raise.
            Err(ActivationError::new(
                "the probe of the acquired resources failed",
            ))
        }))
        .expect("legal registration");
    runtime
        .revise_desired(vec![desired("partial")])
        .expect("legal desired composition");

    let snap = runtime.composition_snapshot();
    let fiber = snap.fibers.get("partial").expect("installed");
    assert_eq!(fiber.state, FiberState::Failed);
    assert!(fiber.failed_outcome, "the raise is recorded as FAILED");
    assert!(
        seq.before("cleanup_thread_b", "cleanup_file_a"),
        "LIFO unwind: the resource acquired last is cleaned up first; got {:?}",
        seq.labels()
    );
    assert!(
        seq.before("worker_joined", "real_file_destructed"),
        "the real thread is fully joined before the real file is destroyed; got {:?}",
        seq.labels()
    );
    assert!(
        seq.before("real_file_destructed", "file_handle_dropped"),
        "the release observation is recorded only after the real File value was destroyed; got {:?}",
        seq.labels()
    );
    assert!(
        file_released.load(Ordering::SeqCst),
        "the real file was released by the unwind"
    );
    assert!(
        snap.provisions.is_empty(),
        "a raised activation publishes no provision"
    );
    assert!(snap.quiet, "settled FAILED is quiet-legal");
    std::fs::remove_file(&path_for_cleanup).expect("temp file cleanup");
}

/// W4 (E6) — withdrawing the provider cannot release it below a live
/// dependent: the consumer's real worker is joined first, the consumer's
/// relation releases second, and only then does the provider's real file
/// handle drop.
#[test]
fn provider_withdrawal_quiesces_consumer_before_release() {
    let seq = Sequence::new();
    let file_released = Arc::new(AtomicBool::new(false));
    let path = unique_temp_path("withdrawal");

    let mut runtime = QianqianApp::new();
    runtime
        .register_component(file_provider_spec(
            path.clone(),
            seq.clone(),
            file_released.clone(),
        ))
        .expect("legal registration");
    runtime
        .register_component(file_consumer_spec(seq.clone()))
        .expect("legal registration");
    runtime
        .revise_desired(vec![desired("file_provider"), desired("file_consumer")])
        .expect("legal desired composition");

    assert_eq!(
        std::fs::read(&path).expect("real file readable while the dependency is live"),
        b"lifecycle-witness",
    );

    // Withdraw the provider; the consumer stays desired.
    runtime
        .revise_desired(vec![desired("file_consumer")])
        .expect("legal");

    let snap = runtime.composition_snapshot();
    assert!(
        seq.before("worker_joined", "relation_released"),
        "the consumer's real worker quiesces before its relation releases; got {:?}",
        seq.labels()
    );
    assert!(
        seq.before("relation_released", "real_file_destructed"),
        "the provider's real file is destroyed only after the consumer fully drained; got {:?}",
        seq.labels()
    );
    assert!(
        seq.before("real_file_destructed", "file_handle_dropped"),
        "the release observation is recorded only after the real File value was destroyed; got {:?}",
        seq.labels()
    );
    assert!(
        file_released.load(Ordering::SeqCst),
        "the provider resource was released last"
    );
    assert_eq!(
        snap.fibers.get("file_consumer").map(|f| f.state),
        Some(FiberState::Pending),
        "the still-desired consumer degraded over its vanished dependency"
    );
    assert!(
        !snap.fibers.contains_key("file_provider"),
        "the withdrawn provider is gone"
    );
    assert!(snap.quiet);
    std::fs::remove_file(&path).expect("temp file cleanup");
}

/// W5 (E7) — explicit root disposal drains every real resource at once:
/// the worker thread joins and the file handle drops, and the post-disposal
/// composition state is quiet and empty.
#[test]
fn root_disposal_drains_all_real_resources() {
    let seq = Sequence::new();
    let service_released = Arc::new(AtomicBool::new(false));
    let file_released = Arc::new(AtomicBool::new(false));
    let path = unique_temp_path("disposal");

    let mut runtime = QianqianApp::new();
    runtime
        .register_component(thread_owner_spec(seq.clone(), service_released.clone()))
        .expect("legal registration");
    runtime
        .register_component(file_owner_spec(
            "file_owner",
            path.clone(),
            seq.clone(),
            file_released.clone(),
        ))
        .expect("legal registration");
    runtime
        .revise_desired(vec![desired("thread_owner"), desired("file_owner")])
        .expect("legal desired composition");

    let snap = runtime.dispose();

    assert!(
        seq.contains("worker_joined"),
        "the real worker joined at root disposal"
    );
    assert!(
        seq.contains("file_handle_dropped"),
        "the real file handle dropped at root disposal"
    );
    assert!(seq.before("worker_joined", "service_dropped"));
    assert!(service_released.load(Ordering::SeqCst));
    assert!(file_released.load(Ordering::SeqCst));
    assert!(snap.fibers.is_empty(), "the composition drained");
    assert!(snap.quiet, "the post-disposal state is quiet");
    std::fs::remove_file(&path).expect("temp file cleanup");
}
