//! Qianqian Composition Kernel (K0): the generic control-plane kernel.
//!
//! > **Kernel controls reachability, ownership and lifetime; it should not
//! > own application payloads.**
//!
//! Primitive budget is exactly five (design §D, §R):
//!
//! ```text
//! Context  Capability  Fiber  Effect  Reconcile
//! ```
//!
//! This crate is generic. It must not — and structurally cannot — know any
//! product, media, platform, or UI payload semantics: it depends on no
//! qianqian-* crate (see `tests/dependency_firewall.rs` and the
//! implementation ADR, D1).
//!
//! Semantic authority: `docs/architecture/composition-kernel-0-design.md`
//! (PR #68 + Corrective-4/5), realized per
//! `docs/architecture/composition-kernel-0-implementation-adr.md`.
//!
//! Control plane is synchronous and serialized; the realtime firewall (§N)
//! forbids every kernel operation on the realtime hot path. Payload flows
//! through pre-bound data edges, never through the kernel.

mod capability;
mod component;
mod context;
mod desired;
mod diagnostic;
mod fiber;
mod kernel;

pub use capability::{Capability, CapabilityKey};
pub use component::{ActivationError, ComponentSpec, Discharge};
pub use context::{ActivationCtx, Binding, ResolveError, TeardownCtx};
pub use desired::{CompositionError, CompositionErrors, DesiredEntry, Revision};
pub use diagnostic::{CompositionSnapshot, FiberDiagnostic, RelationDiagnostic};
pub use fiber::FiberState;
pub use kernel::{EffectHandle, Kernel, StepOutcome};
