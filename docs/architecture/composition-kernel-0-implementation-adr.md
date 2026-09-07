# COMPOSITION-KERNEL-0 Implementation ADR — Representation Decisions

Implementation-issue deliverable for **#70** (Stage 1, representation freeze).

This note records only the Rust representation decisions needed to express the
already-frozen semantics of `composition-kernel-0-design.md` (PR #68 + Corrective-4/5).
It reopens no architecture: every decision below cites the frozen semantic authority
it represents. Rust APIs remain free to evolve; the semantics do not.

Baseline facts recorded at branch point (`c0f818b`, main):

- Workspace: `apps/headless`, `crates/qianqian-core`, `crates/qianqian-runtime`
  (dependency direction headless → runtime → core). No tests beyond 4 R0 witness tests.
- R0 witnesses (`qianqian-core::base`, `qianqian-runtime::AppRuntime`, `AppRuntime::new()`,
  `with_audio_output()`, empty port traits) are bootstrap witnesses, **not compatibility
  contracts**. The authorized implementation may reshape them.
- Baseline gates green: `cargo fmt --check`, `cargo check --workspace`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`.

## D1 — Kernel home

`crates/qianqian-kernel/`, a new workspace member with an **empty `[dependencies]`
table** (std only). Dependency direction:

```text
qianqian-kernel        (generic; imports no qianqian-* crate)
        ↑
qianqian-runtime       (composes product components using the kernel)
        ↑
apps/headless
```

`qianqian-kernel` must not depend on `qianqian-core` (hard invariant, #70 §5.1).
A test reads the kernel manifest and fails if any workspace dependency appears,
so the firewall is executable, not aspirational. A separate crate is justified:
the kernel is the one component whose *entire point* is not knowing product
semantics; only a crate boundary enforces that against accidental imports.

## D2 — Capability identity and service storage

Typed capability keys (design §P.1 row 2, provisional direction confirmed):

```rust
pub trait Capability: 'static {
    const NAME: &'static str;          // diagnostic vocabulary only, never identity
    type Service: ?Sized + 'static;    // the service definition (object-safe trait)
}
```

- Identity = `TypeId::of::<K>()` (`CapabilityKey { id, name }`). Identity is the
  contract definition site — never the provider, the service object address, or
  any payload (§E.1).
- Service storage: the provision effect (the single authority, §K.4) owns
  `Rc<ServiceHandle<K>>` erased as `Rc<dyn Any>`; resolution downcasts
  `ServiceHandle<K>` (sized wrapper around `Rc<K::Service>`) back to
  `Rc<K::Service>`. `Rc`/single-threaded is legal because the K0 control plane
  is synchronous and serialized (§N, §P.3).
- Cardinality: required-single only. 0 providers → unresolved (consumer stays
  Pending); 1 → resolvable; ≥2 enabled desired providers → composition error at
  plan time, plan refused, no silent pick (§E.2). A runtime ambiguity (cannot
  arise through legal K0 orchestration) resolves to a hard error, never a pick.

## D3 — Fiber identity and generation

Generational slab: `FiberId { idx: u32, gen: u32 }`, private. The observable
diagnostic name of a fiber is its desired-entry id (string), unique among
installed fibers; a name may be reused only after removal (§F.1). Private
generation values never appear in any diagnostic snapshot and never participate
in equality (§I.2, R6): confluence compares component-level truth, up to
generation renaming (Lemma 61).

## D4 — Desired revision identity

`Revision` — opaque, `Copy`, equality-only token (`Revision(u64)`), with a
caller-side convenience factory. It is a field of the desired entry
(Reconcile's input datum, §L.5 / §D.6 — not a sixth primitive). The kernel
compares it, never interprets or derives it: no config hash, no dependency
state, no timestamps, no reconcile counters (R1–R8). D0–D4 is the executable
oracle. A pure-`u64` token is admissible per §T.11 (representation stays open);
equality is the only operation the kernel performs.

## D5 — Effect representation

One Effect shape (§H.5/§H.7): composition-lifecycle reversible mutation with a
total inverse.

```text
base triple (every Effect):
  owner fiber episode   — the effect lives in the owner's accumulator (§D.4)
  total inverse         — Box<dyn FnOnce() -> Discharge>; Discharge is the
                          frozen one-verdict teardown truth (§G.6):
                          Discharged | Violated. "No failure outcome" is
                          semantic: there is no partial-success/retry path —
                          a Violated verdict is not a result the runtime
                          continues past, it is the §G.6 latch.
  LIFO position         — structural: the owner's effect stack is a Vec;
                          unwind pops it. No semantic position number exists.
```

Structural provenance is **conditional on bearing a relation** (Corrective-5),
represented as an enum so the conditional field is never silently optional:

```rust
enum EffectProvenance {
    OwnerLocal,                                  // timer/watcher/handle/buffer class
    Relation { key: CapabilityKey, provider: Option<FiberId> },  // provision (provider = owner, None)
                                                 // binding / data-edge / cross-fiber contribution (Some)
}
```

This is a structural presence/absence encoding of the frozen conditional field
(§R: "a conditional field of that one shape"), **not** a behavioral class:
there is no `EffectKind`, no `EffectClass`, no Reversible/Transactional/…
variants, no `Option<Disposer>`. An owner-local effect cannot carry a key (the
variant has no such field); a relation-bearing effect cannot be constructed
without one. Domain teardown obligations are **not** kernel data: the kernel's
whole teardown knowledge is the one verdict per fiber (§G.6 fence) — the
component teardown closure returns `Discharge`, and nothing else about domain
obligations exists kernel-side.

Double dispose is an idempotent no-op (B26): explicit dispose removes the
effect from the accumulator by handle; unwinding skips absent effects. Effects
cannot fire after their episode ends: episode close empties the accumulator,
and stale handles are no-ops.

## D6 — Resolution modes (new vs teardown access)

Two visibly distinct operations (§E.3, T.5):

```rust
ActivationCtx::resolve::<K>()          // NEW commitment: only during activation,
                                       // provider must be ACTIVE; returns Rc<K::Service>
TeardownCtx::resolve_committed::<K>()  // teardown access: reads the episode-fixed
                                       // committed view, valid through Unloading
```

Undeclared access (key not in the component's `requires ∪ provides`) and
inactive access (resolve outside an episode) are hard errors — the paper's
`UNDECLARED_ACCESS` / `INACTIVE_ACCESS` split adopted semantically (§E.3).
A provider entering Unloading stops satisfying new resolution immediately
(L-Leave) while committed views stay readable (B14 teardown window).

## D7 — Invalidation transport

Synchronous push inside the serialized control plane. No channels, no
callbacks, no polling: each `step()` first compares every Active fiber's
committed view against freshly computed target resolvability; a mismatch is
the divert/invalidation trigger (B5/B13 semantics, B20's synchronous host).
Invalidation is kernel-internal — there is no public event bus (C.2).

## D8 — Lifecycle and reconcile engine

- 7-state vocabulary, exactly as frozen (§F.2): `Absent` (not in registry),
  `Pending`, `Activating`, `Active`, `Unloading`, `Failed`; `TEARDOWN_VIOLATED`
  is a latched diagnostic flag on an `Unloading` fiber, never an eighth state.
- `step()` performs at most one enabled transition: eligible unloads (guard
  `¬relied` respected — a provider whose open committed views still name it
  waits), divert checks, activations (one bounded step), removals, mounts,
  revision staging (retire → drain → remove → **then** mount, §E.4).
- `settle()` loops `step()` until quiet or blocked; `is_quiet()` is the frozen
  transition predicate (§L.1 clauses 1–5), not `committed_view == target_view`.
- Activation raise → `Unloading` with pending activation error as episode
  metadata → unwind partial effects LIFO → discharged ⇒ `FAILED`; violated ⇒
  latched in `Unloading`, `FAILED` unreachable, provider final-release guards
  stay latched (§F.3, §G.6). A raised activation does not run the component
  teardown closure (no activated episode existed); its effects still unwind.
- Ordering inside unwind: owner effects LIFO, then the component teardown
  closure (domain obligation verdict). Domain choreography (e.g. close handle
  before session teardown) is component code inside inverses/closures — the
  kernel sequences LIFO + closure + verdict, never domain steps (§G.5).
- Staged replacement and the owed-mount non-quiescence (§L.1 clause 5) are
  directly observable because `step()` exposes the staging boundary.

## D9 — Diagnostics

One closed snapshot type projecting exactly the §I.1 surfaces: per-fiber
lifecycle truth (state, failed-outcome presence, violation flag), capability
reachability (name → provider name or absent), relation set (owner, provider,
capability — the projection of relation-bearing provenance per §K.4), and the
quiescence flag. No track/position/PlaybackState/payload fields exist (§J.3).
A `#[doc(hidden)]` operation counter exists solely as the executable RT-firewall
witness (§N) and is not a diagnostic surface.

## Stage-1 stop gate

No stage-1 decision required: a sixth semantic primitive, an `EffectClass`,
a `DataEdgeRegistry` second truth, kernel-owned domain resources, or a child
Context hierarchy. The frozen semantics were expressible with the five
primitives and the representations above. Stage 2 is authorized.
