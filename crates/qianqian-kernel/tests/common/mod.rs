//! Shared synthetic fixtures for the executable semantic oracles.
//!
//! Everything here is generic: capability names like `Tag`/`Sink`/`Listeners`
//! are deliberately anonymous contracts with no product semantics (the kernel
//! must never learn more than these fixtures show).

// Each integration-test binary compiles this module and uses a subset.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use qianqian_kernel::{Capability, ComponentSpec, Discharge};

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
                qianqian_kernel::ActivationError::new(format!("{e:?}"))
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
                return Err(qianqian_kernel::ActivationError::new("fixture failure"));
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
        Err(qianqian_kernel::ActivationError::new("fixture failure"))
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

impl qianqian_kernel::Capability for Extra {
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
