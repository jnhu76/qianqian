//! Shared synthetic fixtures for the executable semantic oracles.
//!
//! Everything here is generic: capability names like `Tag`/`Sink`/`Listeners`
//! are deliberately anonymous contracts with no product semantics (the kernel
//! must never learn more than these fixtures show).

// Each integration-test binary compiles this module and uses a subset.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use qianqian_composition::{Capability, ComponentSpec, Discharge};

pub type Log = Rc<RefCell<Vec<String>>>;

pub fn log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

pub fn entries(l: &Log) -> Vec<String> {
    l.borrow().clone()
}

// ---------------------------------------------------------------------------
// Tag: minimal required-single capability with a string payload for identity
// assertions (which provider got resolved).
// ---------------------------------------------------------------------------

pub struct Tag;

impl Capability for Tag {
    const NAME: &'static str = "Tag";
    type Service = dyn TagService;
}

pub trait TagService {
    fn tag(&self) -> &'static str;
}

pub struct FixedTag(pub &'static str);

impl TagService for FixedTag {
    fn tag(&self) -> &'static str {
        self.0
    }
}

/// A provider component installing `Tag` with a fixed service value.
pub fn tag_provider(name: &'static str, tag: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    let log_teardown = l.clone();
    ComponentSpec::new(name)
        .provides::<Tag>()
        .on_activate(move |ctx| {
            log.borrow_mut().push(format!("{name}:activated"));
            ctx.provide::<Tag>(Rc::new(FixedTag(tag)))
                .expect("provides declared");
            Ok(())
        })
        .on_teardown(move |_ctx| {
            log_teardown.borrow_mut().push(format!("{name}:released"));
            Discharge::Discharged
        })
}

/// A consumer component requiring `Tag`; records the resolved provider tag.
pub fn tag_consumer(name: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name)
        .requires::<Tag>()
        .on_activate(move |ctx| {
            let binding = ctx.resolve::<Tag>().map_err(|e| {
                log.borrow_mut().push(format!("{name}:resolve-error:{e:?}"));
                qianqian_composition::ActivationError::new(format!("{e:?}"))
            })?;
            log.borrow_mut()
                .push(format!("{name}:bound-to-{}", binding.service().tag()));
            Ok(())
        })
}

// ---------------------------------------------------------------------------
// Failing / violating components for failure-sanitation oracles.
// ---------------------------------------------------------------------------

/// Provider whose activation fails for the first `failures` attempts, then
/// succeeds (for the D0–D4 fail-then-revise oracle).
pub fn flaky_provider(name: &'static str, failures: u32, l: &Log) -> ComponentSpec {
    let attempts = Rc::new(Cell::new(0u32));
    let log = l.clone();
    ComponentSpec::new(name)
        .provides::<Tag>()
        .on_activate(move |ctx| {
            let n = attempts.get();
            attempts.set(n + 1);
            log.borrow_mut().push(format!("{name}:attempt{n}"));
            if n < failures {
                return Err(qianqian_composition::ActivationError::new(
                    "fixture failure",
                ));
            }
            ctx.provide::<Tag>(Rc::new(FixedTag(name)))
                .expect("provides declared");
            Ok(())
        })
}

/// A component registering `effects` owner-local effects, then raising.
pub fn failing_after_effects(name: &'static str, effects: usize, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name).on_activate(move |ctx| {
        for i in 0..effects {
            let lg = log.clone();
            ctx.register_effect(move || {
                lg.borrow_mut().push(format!("{name}:dispose{i}"));
                Discharge::Discharged
            });
        }
        Err(qianqian_composition::ActivationError::new(
            "fixture failure",
        ))
    })
}

/// A component whose first owned effect's inverse violates its contract.
pub fn violating_unwind_component(name: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name).on_activate(move |ctx| {
        let lg = log.clone();
        ctx.register_effect(move || {
            lg.borrow_mut().push(format!("{name}:inverse-violated"));
            Discharge::Violated
        });
        Ok(())
    })
}

/// A component whose declared domain teardown obligation fails to discharge.
pub fn violating_teardown_component(name: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name).on_teardown(move |_| {
        log.borrow_mut().push(format!("{name}:teardown-violated"));
        Discharge::Violated
    })
}

// ---------------------------------------------------------------------------
// Extra: a second, independent capability for unrelated-churn oracles.
// ---------------------------------------------------------------------------

pub struct Extra;

impl qianqian_composition::Capability for Extra {
    const NAME: &'static str = "Extra";
    type Service = dyn TagService;
}

/// Provider of the unrelated `Extra` capability.
pub fn extra_provider(name: &'static str, tag: &'static str, l: &Log) -> ComponentSpec {
    let log_teardown = l.clone();
    ComponentSpec::new(name)
        .provides::<Extra>()
        .on_activate(move |ctx| {
            ctx.provide::<Extra>(Rc::new(FixedTag(tag)))
                .expect("provides declared");
            Ok(())
        })
        .on_teardown(move |_ctx| {
            log_teardown.borrow_mut().push(format!("{name}:released"));
            Discharge::Discharged
        })
}

// ---------------------------------------------------------------------------
// Listeners: the commutative contribution-set contract (§H.4). Each provider
// activation creates one registry instance; consumers register through the
// service (data plane) and own a relation-bearing effect whose inverse
// removes exactly their own token.
// ---------------------------------------------------------------------------

pub struct Listeners;

impl qianqian_composition::Capability for Listeners {
    const NAME: &'static str = "Listeners";
    type Service = dyn ListenerRegistry;
}

pub trait ListenerRegistry {
    fn register(&self, who: &'static str) -> ListenerToken;
}

#[derive(Clone)]
pub struct ListenerToken {
    live: Rc<RefCell<Vec<&'static str>>>,
    name: &'static str,
}

impl ListenerToken {
    pub fn unregister(self) {
        self.live.borrow_mut().retain(|n| *n != self.name);
    }
}

struct ListenerSet {
    live: Rc<RefCell<Vec<&'static str>>>,
}

impl ListenerRegistry for ListenerSet {
    fn register(&self, who: &'static str) -> ListenerToken {
        self.live.borrow_mut().push(who);
        ListenerToken {
            live: self.live.clone(),
            name: who,
        }
    }
}

pub fn listeners_provider(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name)
        .provides::<Listeners>()
        .on_activate(|ctx| {
            let set = Rc::new(ListenerSet {
                live: Rc::new(RefCell::new(Vec::new())),
            });
            ctx.provide::<Listeners>(set).expect("provides declared");
            Ok(())
        })
}

/// A consumer registering one listener contribution named after itself.
pub fn listener_consumer(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name)
        .requires::<Listeners>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<Listeners>()
                .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
            let token = binding.service().register(name);
            ctx.register_relation(&binding, move || {
                token.unregister();
                qianqian_composition::Discharge::Discharged
            });
            Ok(())
        })
}
