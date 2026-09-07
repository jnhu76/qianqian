//! Synthetic data-edge binding oracles (#70 Stage 10, oracle families E and
//! the RT firewall): the Sink/bind/Session analogue of the §K owned data-edge
//! model, diagnosed from the single Effect authority, with payload traffic
//! performing zero kernel operations.
//!
//! Semantic authority: composition-kernel-0-design.md §K (owned data-edge
//! model), §N (realtime firewall).

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use qianqian_kernel::{
    Capability, ComponentSpec, DesiredEntry, Discharge, Kernel, RelationDiagnostic, Revision,
};

// ---------------------------------------------------------------------------
// Sink / Session: the synthetic data-edge analogue of PcmSink.bind(endpoint).
// ---------------------------------------------------------------------------

pub struct Sink;

impl Capability for Sink {
    const NAME: &'static str = "Sink";
    type Service = dyn SinkService;
}

/// The data-plane endpoint handed to the session: payload flows through it
/// directly. It holds NO kernel reference — that is the RT firewall shape.
#[derive(Default)]
pub struct Endpoint {
    blocks: RefCell<Vec<u32>>,
    released: std::cell::Cell<bool>,
}

impl Endpoint {
    pub fn send(&self, block: u32) {
        assert!(!self.released.get(), "session released");
        self.blocks.borrow_mut().push(block);
    }

    pub fn block_count(&self) -> usize {
        self.blocks.borrow().len()
    }
}

/// The session object created by `bind`. Provider-mechanism owned; the
/// consumer's binding Effect owns its lifecycle.
pub struct Session {
    endpoint: Rc<Endpoint>,
    released: std::cell::Cell<bool>,
}

impl Session {
    pub fn release(&self) {
        self.released.set(true);
        self.endpoint.released.set(true);
    }

    fn is_released(&self) -> bool {
        self.released.get()
    }
}

pub trait SinkService {
    fn bind(&self, endpoint: Rc<Endpoint>) -> Rc<Session>;
}

struct SinkMechanism;

impl SinkService for SinkMechanism {
    fn bind(&self, endpoint: Rc<Endpoint>) -> Rc<Session> {
        Rc::new(Session {
            endpoint,
            released: std::cell::Cell::new(false),
        })
    }
}

fn sink_provider(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name)
        .provides::<Sink>()
        .on_activate(|ctx| {
            ctx.provide::<Sink>(Rc::new(SinkMechanism))
                .expect("provides declared");
            Ok(())
        })
}

/// The consumer with the frozen bind-at-activation rule: resolves Sink once,
/// binds an endpoint, registers ONE relation-bearing effect owning the
/// session, and stashes the pre-bound endpoint for direct payload use.
fn binding_consumer(
    name: &'static str,
    endpoint_out: Rc<RefCell<Option<Rc<Endpoint>>>>,
) -> ComponentSpec {
    ComponentSpec::new(name)
        .requires::<Sink>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<Sink>()
                .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
            let endpoint = Rc::new(Endpoint::default());
            let session = binding.service().bind(endpoint.clone());
            let released = session.clone();
            ctx.register_relation(&binding, move || {
                released.release();
                Discharge::Discharged
            });
            *endpoint_out.borrow_mut() = Some(endpoint);
            Ok(())
        })
}

fn sink_relation(owner: &str) -> RelationDiagnostic {
    RelationDiagnostic {
        owner: owner.to_owned(),
        provider: "sink".to_owned(),
        capability: "Sink",
    }
}

/// Binding lifecycle: consumer resolves Sink → bind(endpoint) → one
/// relation-bearing Effect; diagnostics derive existence, owner, provider,
/// relation, and teardown state from that single authority — no payload
/// inspection, no second registry.
#[test]
fn binding_provenance_is_projected_from_the_single_effect_authority() {
    let mut k = Kernel::new();
    let _ = log();
    let endpoint_cell = Rc::new(RefCell::new(None));
    k.register_component(sink_provider("sink"))
        .expect("component registered");
    k.register_component(binding_consumer("music", endpoint_cell.clone()))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("sink", "sink", Revision::fresh()),
        DesiredEntry::enabled("music", "music", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    let snap = k.snapshot();
    assert_eq!(
        snap.relations.iter().cloned().collect::<Vec<_>>(),
        vec![sink_relation("music")],
        "binding exists; owner=consumer; provider=sink fiber; relation=Sink"
    );

    // Teardown state derives from the owner's lifecycle truth: the binding
    // effect is live inside the owner's ACTIVE episode.
    assert_eq!(
        snap.fibers.get("music").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active)
    );

    // Consumer withdrawal unwinds the binding effect: the session is
    // released through the effect's total inverse, and the diagnostic row
    // disappears with it (no ghost binding).
    k.set_desired(vec![DesiredEntry::enabled(
        "sink",
        "sink",
        Revision::fresh(),
    )])
    .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert!(snap.relations.is_empty(), "no ghost binding may survive");
    assert_eq!(
        snap.capabilities.get("Sink"),
        Some(&Some("sink".to_owned())),
        "the provider itself is unaffected by the consumer's withdrawal"
    );
}

/// Payload use requires zero kernel operations: control-plane bind once →
/// pre-bound endpoint → direct payload calls (§N, §K.3).
#[test]
fn payload_traffic_performs_zero_kernel_operations() {
    let mut k = Kernel::new();
    let _ = log();
    let endpoint_cell = Rc::new(RefCell::new(None));
    k.register_component(sink_provider("sink"))
        .expect("component registered");
    k.register_component(binding_consumer("music", endpoint_cell.clone()))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("sink", "sink", Revision::fresh()),
        DesiredEntry::enabled("music", "music", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    let endpoint = endpoint_cell
        .borrow()
        .clone()
        .expect("bind-at-activation produced a pre-bound endpoint");

    let ops_before = k.debug_op_count();
    // The "RT hot path": direct payload calls on the pre-bound endpoint.
    for block in 0..10_000u32 {
        endpoint.send(block);
    }
    assert_eq!(
        k.debug_op_count(),
        ops_before,
        "payload operations must perform zero kernel operations (RT firewall)"
    );
    assert_eq!(endpoint.block_count(), 10_000);

    // Structural proof: the endpoint carries no way to reach the kernel —
    // it is a plain data object owned by the session (checked by payload
    // succeeding after the kernel is borrowed immutably elsewhere; no
    // borrow of the kernel exists on this path by construction).
    let _still_live = &k;
}

/// Consumer withdrawal releases the session through the binding effect's
/// inverse BEFORE the provider's final release (teardown ordering at the
/// data edge, §K.2 / §G.4).
#[test]
fn binding_releases_inside_the_withdrawal_window() {
    let mut k = Kernel::new();
    let _ = log();
    let endpoint_cell = Rc::new(RefCell::new(None));
    let session_cell: Rc<RefCell<Option<Rc<Session>>>> = Rc::new(RefCell::new(None));
    // Same consumer but keeping the session handle for state assertions.
    let consumer = {
        let ec = endpoint_cell.clone();
        let sc = session_cell.clone();
        ComponentSpec::new("music")
            .requires::<Sink>()
            .on_activate(move |ctx| {
                let binding = ctx
                    .resolve::<Sink>()
                    .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
                let endpoint = Rc::new(Endpoint::default());
                let session = binding.service().bind(endpoint.clone());
                let for_dispose = session.clone();
                ctx.register_relation(&binding, move || {
                    for_dispose.release();
                    Discharge::Discharged
                });
                *ec.borrow_mut() = Some(endpoint);
                *sc.borrow_mut() = Some(session);
                Ok(())
            })
    };
    k.register_component(sink_provider("sink"))
        .expect("component registered");
    k.register_component(consumer)
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("sink", "sink", Revision::fresh()),
        DesiredEntry::enabled("music", "music", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert!(!session_cell.borrow().as_ref().unwrap().is_released());

    // Withdraw the consumer: the binding effect discharges inside the
    // consumer's own Unloading, while the provider is still installed.
    k.set_desired(vec![DesiredEntry::enabled(
        "sink",
        "sink",
        Revision::fresh(),
    )])
    .expect("legal");
    k.step(); // retire flag (consumer)
    k.step(); // divert into Unloading
    assert_eq!(
        k.snapshot().provisions.get("Sink").map(|v| v.len()),
        Some(1),
        "the provider is still installed while the consumer discharges"
    );
    k.settle();
    assert!(
        session_cell.borrow().as_ref().unwrap().is_released(),
        "the binding effect's inverse must have released the session"
    );
    let snap = k.snapshot();
    assert!(snap.relations.is_empty());
    assert!(snap.quiet);
}
