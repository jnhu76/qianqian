//! K0-ON composition of one scan session — for EITHER mechanism arm
//! (SPEC §4.6; Gate I established the H0 face, Gate II #308 generalizes
//! the wrapper over the executor surface so H1 composes identically).
//!
//! K0 is the orthogonal composition/lifecycle factor: the scan session
//! runs as ONE fiber in a [`CompositionKernel`], and K0 owns its
//! activation and teardown. The constraints are the frozen ones:
//!
//! 1. The Composition Owner (`!Send`/`!Sync` — K0 is `Rc`-based) stays on
//!    the composing (control) thread. The kernel and the session cell are
//!    never sent anywhere; mechanism workers only touch their own shared
//!    state (H0 `Arc<Shared>` / H1 `Arc<H1Shared>` + channel handles).
//! 2. K0 never sees items and never participates in per-item scheduling.
//!    It only adds/withdraws the fiber and its resource ownership.
//! 3. The inverse (teardown closure) carries the Stop/Settlement-Join
//!    responsibility: it takes the executor, publishes the fan-in
//!    aggregate + quiescence receipt after every worker join — or aborts
//!    (join everything, NO receipt) when settlement failed.
//! 4. `DisposeVerdict::TeardownViolated` is a FAILURE, never papered over
//!    by the harness.
//!
//! The executors themselves are unchanged between K0 OFF/ON — same
//! business path, same B/P(/Q) semantics, same oracle verdicts (SPEC
//! §4.6 rule 5). The generic surface below is the whole of what K0
//! touches; it deliberately exposes no queue/channel/condvar detail.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use qianqian_composition::{
    Capability, ComponentSpec, CompositionKernel, DesiredEntry, Discharge, DisposeVerdict, Revision,
};

use crate::h0::SettlementWatch as H0SettlementWatch;
use crate::h0::{H0Config, H0Executor};
use crate::h1::SettlementWatch as H1SettlementWatch;
use crate::h1::{H1Config, H1Executor, H1SessionStats};
use crate::observer::ObserverSink;
use crate::session_core::{SessionError, SessionStats};
use crate::workload::BatchSpec;

/// The settlement-wait face shared by both mechanisms' watch handles
/// (SPEC §5.5 action 2).
pub trait SettlementWatchFace {
    fn wait(&self) -> Result<(), SessionError>;
}

/// The minimal executor surface the K0 composition needs. Both H0 and H1
/// implement it unchanged; K0 never learns anything mechanism-shaped.
pub trait ScanSessionExecutor: Send + 'static {
    /// The K0 component identity (one mechanism arm = one component name).
    const COMPONENT_NAME: &'static str;
    type Config: Clone;
    type Stats;
    type Watch: SettlementWatchFace;
    fn start(cfg: Self::Config, spec: BatchSpec, sink: ObserverSink) -> Self;
    fn admit_all(&mut self);
    fn finish_admission(&mut self);
    fn settlement_watch(&self) -> Self::Watch;
    fn await_quiescence(&mut self) -> Result<Self::Stats, SessionError>;
    fn abort(&mut self) -> Self::Stats;
}

/// Capability carrying the session controls between K0 activation and
/// teardown. The service lives on the control thread only (`RefCell`),
/// so the capability type carries the executor identity as a phantom.
struct SessionControlsCap<E: 'static>(PhantomData<fn() -> E>);

impl<E: ScanSessionExecutor> Capability for SessionControlsCap<E> {
    const NAME: &'static str = "wrk-scan.session-controls";
    type Service = RefCell<SessionControls<E>>;
}

/// What activation hands to teardown. Control-thread-only (`RefCell`,
/// part of the `!Send` composition owner's world).
struct SessionControls<E: ScanSessionExecutor> {
    executor: Option<E>,
    settlement: Option<Result<(), SessionError>>,
    stats: Option<E::Stats>,
}

/// A K0-composed scan session for mechanism arm `E`. Call [`Self::dispose`]
/// exactly once; the kernel cannot be reused after root disposal.
pub struct K0ScanSession<E: ScanSessionExecutor> {
    kernel: CompositionKernel,
    cell: Rc<RefCell<SessionControls<E>>>,
}

impl<E: ScanSessionExecutor> K0ScanSession<E> {
    /// Register the session component, install the desired entry and
    /// settle — the K0 activation step spawns the mechanism's worker pool
    /// and mounts the controls. No admission has happened yet.
    pub fn compose(cfg: E::Config, spec: BatchSpec, sink: ObserverSink) -> Result<Self, String> {
        let cell: Rc<RefCell<SessionControls<E>>> = Rc::new(RefCell::new(SessionControls {
            executor: None,
            settlement: None,
            stats: None,
        }));
        let cell_for_activation = Rc::clone(&cell);
        let cell_for_teardown = Rc::clone(&cell);
        let spec_for_activation = spec;

        let component = ComponentSpec::new(E::COMPONENT_NAME)
            .provides::<SessionControlsCap<E>>()
            .on_activate(move |ctx| {
                let executor = E::start(cfg.clone(), spec_for_activation.clone(), sink.clone());
                cell_for_activation.borrow_mut().executor = Some(executor);
                ctx.provide::<SessionControlsCap<E>>(Rc::clone(&cell_for_activation))
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))
            })
            .on_teardown(move |_ctx| {
                // The inverse holds its own handle to the session controls
                // (the repository's provider-owned-resource pattern, cf.
                // output_plugin): K0 owns the LIFECYCLE decision (when
                // teardown runs, and the latched verdict), not a second
                // capability round-trip.
                let mut controls = cell_for_teardown.borrow_mut();
                let Some(mut executor) = controls.executor.take() else {
                    return Discharge::Violated;
                };
                match controls.settlement.take() {
                    // Settlement held: the inverse publishes the aggregate
                    // and the quiescence receipt AFTER joining every
                    // worker (all exits precede the receipt, O6).
                    Some(Ok(())) => match executor.await_quiescence() {
                        Ok(stats) => {
                            controls.stats = Some(stats);
                            Discharge::Discharged
                        }
                        Err(_) => Discharge::Violated,
                    },
                    // Owner death (or a missing settlement): abort — join
                    // everything, publish NO aggregate and NO receipt, and
                    // let the kernel latch the violation.
                    Some(Err(_)) | None => {
                        let stats = executor.abort();
                        controls.stats = Some(stats);
                        Discharge::Violated
                    }
                }
            });

        let mut kernel = CompositionKernel::new();
        kernel
            .register_component(component)
            .map_err(|e| e.to_string())?;
        kernel
            .set_desired(vec![DesiredEntry::enabled(
                "scan-session",
                E::COMPONENT_NAME,
                Revision::fresh(),
            )])
            .map_err(|e| e.to_string())?;
        kernel.settle();
        if cell.borrow().executor.is_none() {
            return Err("K0 activation did not mount the session executor".into());
        }
        Ok(K0ScanSession { kernel, cell })
    }

    /// Admission + offer close + settlement, all on the composing
    /// (control) thread. The settlement result is recorded in the cell
    /// for the teardown inverse.
    pub fn run_admission_and_settlement(&mut self) -> Result<(), SessionError> {
        let watch = {
            let mut controls = self.cell.borrow_mut();
            let executor = controls
                .executor
                .as_mut()
                .expect("K0 activation mounts the executor");
            executor.admit_all();
            executor.finish_admission();
            executor.settlement_watch()
        };
        let result = watch.wait();
        self.cell.borrow_mut().settlement = Some(result.clone());
        result
    }

    /// Root disposal: the teardown inverse joins every worker and
    /// publishes the aggregate + quiescence receipt (settlement held), or
    /// aborts without a receipt (owner death). Returns the kernel's
    /// authoritative verdict and the session stats if the executor ran.
    pub fn dispose(mut self) -> (DisposeVerdict, Option<E::Stats>) {
        let verdict = self.kernel.dispose_root();
        let stats = self.cell.borrow_mut().stats.take();
        (verdict, stats)
    }
}

impl SettlementWatchFace for H0SettlementWatch {
    fn wait(&self) -> Result<(), SessionError> {
        Self::wait(self)
    }
}

impl SettlementWatchFace for H1SettlementWatch {
    fn wait(&self) -> Result<(), SessionError> {
        Self::wait(self)
    }
}

impl ScanSessionExecutor for H0Executor {
    const COMPONENT_NAME: &'static str = "wrk-scan.h0-session";
    type Config = H0Config;
    type Stats = SessionStats;
    type Watch = H0SettlementWatch;

    fn start(cfg: Self::Config, spec: BatchSpec, sink: ObserverSink) -> Self {
        H0Executor::start(cfg, spec, sink)
    }
    fn admit_all(&mut self) {
        H0Executor::admit_all(self)
    }
    fn finish_admission(&mut self) {
        H0Executor::finish_admission(self)
    }
    fn settlement_watch(&self) -> Self::Watch {
        H0Executor::settlement_watch(self)
    }
    fn await_quiescence(&mut self) -> Result<Self::Stats, SessionError> {
        H0Executor::await_quiescence(self)
    }
    fn abort(&mut self) -> Self::Stats {
        H0Executor::abort(self)
    }
}

impl ScanSessionExecutor for H1Executor {
    const COMPONENT_NAME: &'static str = "wrk-scan.h1-session";
    type Config = H1Config;
    type Stats = H1SessionStats;
    type Watch = H1SettlementWatch;

    fn start(cfg: Self::Config, spec: BatchSpec, sink: ObserverSink) -> Self {
        H1Executor::start(cfg, spec, sink)
    }
    fn admit_all(&mut self) {
        H1Executor::admit_all(self)
    }
    fn finish_admission(&mut self) {
        H1Executor::finish_admission(self)
    }
    fn settlement_watch(&self) -> Self::Watch {
        H1Executor::settlement_watch(self)
    }
    fn await_quiescence(&mut self) -> Result<Self::Stats, SessionError> {
        H1Executor::await_quiescence(self)
    }
    fn abort(&mut self) -> Self::Stats {
        H1Executor::abort(self)
    }
}

#[cfg(test)]
mod tests {
    //! K0-ON arms of the frozen lifecycle, on the real composition kernel.

    use super::*;
    use crate::h0::tests::{probe_cfg, with_watchdog, SEED};
    use crate::h1::H1Config;
    use crate::oracle;
    use crate::scenario::Scenario;

    fn h1_cfg(p: usize, b: usize, cancel: Option<(u32, u32)>) -> H1Config {
        H1Config {
            p,
            b,
            cancel_milestone: cancel,
        }
    }

    #[test]
    fn k0_on_probe_batch_full_oracle_green_discharged() {
        let _g = crate::h0::session_test_lock();
        with_watchdog(30, move || {
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let mut session = K0ScanSession::<H0Executor>::compose(
                probe_cfg(4, 8, None),
                spec.clone(),
                sink.clone(),
            )
            .expect("compose");
            session
                .run_admission_and_settlement()
                .expect("settlement under K0");
            let (verdict, stats) = session.dispose();
            assert_eq!(verdict, DisposeVerdict::Discharged);
            let stats = stats.expect("stats recorded");
            assert_eq!(stats.admitted_count, 48);
            assert_eq!(stats.workers_joined, 4);
            assert!(stats.b_high_water <= 8);
            assert!(stats.active_execution_high_water <= 4);
            let v = oracle::adjudicate(&spec, sink.ledger().unwrap());
            assert!(v.green(), "oracle RED: {:?}", v.failed());
        });
    }

    #[test]
    fn k0_on_owner_death_teardown_violated_no_receipt() {
        let _g = crate::h0::session_test_lock();
        with_watchdog(30, move || {
            struct ResetInjection;
            impl Drop for ResetInjection {
                fn drop(&mut self) {
                    H0Executor::inject_owner_panic_at(u32::MAX);
                }
            }
            let _reset = ResetInjection;
            H0Executor::inject_owner_panic_at(3);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let mut session = K0ScanSession::<H0Executor>::compose(
                probe_cfg(4, 8, None),
                spec.clone(),
                sink.clone(),
            )
            .expect("compose");
            let err = session
                .run_admission_and_settlement()
                .expect_err("settlement must fail on owner death");
            assert_eq!(err.item, 3);
            let (verdict, stats) = session.dispose();
            // TeardownViolated is a FAILURE, and no receipt may exist.
            assert_eq!(verdict, DisposeVerdict::TeardownViolated);
            let stats = stats.expect("abort stats recorded");
            assert_eq!(stats.workers_joined, 4);
            assert!(
                !sink
                    .ledger()
                    .unwrap()
                    .snapshot()
                    .iter()
                    .any(|r| r.kind == crate::ledger::EffectKind::QuiescenceReceipt),
                "no receipt after owner-death teardown"
            );
            H0Executor::inject_owner_panic_at(u32::MAX);
        });
    }

    #[test]
    fn k0_on_s5_cancel_cell_oracle_green() {
        let _g = crate::h0::session_test_lock();
        with_watchdog(30, move || {
            let scenario = Scenario::S5;
            let spec = scenario.batch_spec(1, SEED);
            let sink = ObserverSink::on(spec.batch_id);
            let mut session = K0ScanSession::<H0Executor>::compose(
                probe_cfg(4, 8, scenario.cancel_milestone()),
                spec.clone(),
                sink.clone(),
            )
            .expect("compose");
            session.run_admission_and_settlement().expect("settlement");
            let (verdict, stats) = session.dispose();
            assert_eq!(verdict, DisposeVerdict::Discharged);
            let stats = stats.expect("stats");
            assert!(stats.admitted_count < spec.items.len(), "the cut must bite");
            let v = oracle::adjudicate(&spec, sink.ledger().unwrap());
            assert!(v.green(), "oracle RED: {:?}", v.failed());
        });
    }

    // ---- H1 through the same generic composition ----

    #[test]
    fn k0_on_h1_probe_batch_full_oracle_green_discharged() {
        let _g = crate::session_core::session_test_lock();
        with_watchdog(30, move || {
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let mut session = K0ScanSession::<H1Executor>::compose(
                h1_cfg(4, 8, None),
                spec.clone(),
                sink.clone(),
            )
            .expect("compose h1");
            session
                .run_admission_and_settlement()
                .expect("settlement under K0 (h1)");
            let (verdict, stats) = session.dispose();
            assert_eq!(verdict, DisposeVerdict::Discharged);
            let stats = stats.expect("stats recorded");
            assert_eq!(stats.stats.admitted_count, 48);
            assert_eq!(stats.stats.workers_joined, 4);
            assert!(stats.stats.b_high_water <= 8);
            assert!(stats.stats.active_execution_high_water <= 4);
            // The Q witness never exceeds B = Q.
            assert!(stats.q_high_water <= 8);
            assert_eq!(stats.channel_resident_final, 0, "clean drain");
            let v = oracle::adjudicate(&spec, sink.ledger().unwrap());
            assert!(v.green(), "oracle RED: {:?}", v.failed());
        });
    }

    #[test]
    fn k0_on_h1_owner_death_teardown_violated_no_receipt() {
        let _g = crate::session_core::session_test_lock();
        with_watchdog(30, move || {
            struct ResetInjection;
            impl Drop for ResetInjection {
                fn drop(&mut self) {
                    H1Executor::inject_owner_panic_at(u32::MAX);
                }
            }
            let _reset = ResetInjection;
            H1Executor::inject_owner_panic_at(3);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let mut session = K0ScanSession::<H1Executor>::compose(
                h1_cfg(4, 8, None),
                spec.clone(),
                sink.clone(),
            )
            .expect("compose h1");
            let err = session
                .run_admission_and_settlement()
                .expect_err("settlement must fail on owner death (h1)");
            assert_eq!(err.item, 3);
            let (verdict, stats) = session.dispose();
            assert_eq!(verdict, DisposeVerdict::TeardownViolated);
            let stats = stats.expect("abort stats recorded");
            assert_eq!(stats.stats.workers_joined, 4);
            assert!(
                !sink
                    .ledger()
                    .unwrap()
                    .snapshot()
                    .iter()
                    .any(|r| r.kind == crate::ledger::EffectKind::QuiescenceReceipt),
                "no receipt after owner-death teardown (h1)"
            );
            H1Executor::inject_owner_panic_at(u32::MAX);
        });
    }

    #[test]
    fn k0_on_h1_s5_cancel_cell_oracle_green() {
        let _g = crate::session_core::session_test_lock();
        with_watchdog(30, move || {
            let scenario = Scenario::S5;
            let spec = scenario.batch_spec(1, SEED);
            let sink = ObserverSink::on(spec.batch_id);
            let mut session = K0ScanSession::<H1Executor>::compose(
                h1_cfg(4, 8, scenario.cancel_milestone()),
                spec.clone(),
                sink.clone(),
            )
            .expect("compose h1");
            session.run_admission_and_settlement().expect("settlement");
            let (verdict, stats) = session.dispose();
            assert_eq!(verdict, DisposeVerdict::Discharged);
            let stats = stats.expect("stats");
            assert!(
                stats.stats.admitted_count < spec.items.len(),
                "the cut must bite"
            );
            let v = oracle::adjudicate(&spec, sink.ledger().unwrap());
            assert!(v.green(), "oracle RED: {:?}", v.failed());
        });
    }
}
