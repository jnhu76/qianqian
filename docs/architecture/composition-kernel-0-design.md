# Composition Kernel 0 — Semantic Design

Design-gate deliverable for **#67 COMPOSITION-KERNEL-0** (parent authority **#46**, boundary prerequisite **#53 / PR #66**, accepted audit `component-boundary-a0.md`).

Status: **semantic design only. Implementation NOT authorized by this document.** Nothing here freezes a Rust API, a crate layout, or an async-runtime choice. PASS (proposed, §Verdict) authorizes only opening a separate `COMPOSITION-KERNEL-0 IMPLEMENTATION` issue.

Primary external sources:

- **[PAPER]** *A Programming Paradigm for Spatiotemporal Composability*, Yifan Shi, Wei Zhang, Tianyi Cui (Peking University; DeepSeek-AI), arXiv:2608.25512v1, 92 pp. — the Cordis formal model. Section/definition/theorem numbers below cite this text directly.
- **[CORDIS]** the Cordis implementation as documented for `@deepseek-ai/cordis` (fiber/registry/loader/HMR docs; read via the context7 mirror of `acryldev/cordis-meta-framework-spatiotemporal-composability/cordis-docs`, e.g. `cordis-api/fiber.md`, `cordis-api/registry.md`, `cordis-tutorial/06-composition-and-hmr.md`). The paper's own §5 also documents the implementation (Algorithms 1–10, Table 2); where §5 and the docs mirror agree, both count as [CORDIS] evidence.
- **[QIANQIAN]** decisions made for this player, always downstream of #53 frozen facts.
- **[OPEN]** not yet justified.

Provenance discipline: no claim below is presented as paper authority unless its source was inspected. Paper statements are cited by exact location; Cordis-only behavior is never upgraded to [PAPER]; Qianqian choices are never presented as theorems.

---

## A. Authority / Reality Audit

### A.1 Base and authority chain

```text
BASE: dca7b18 (main, clean) — merge of PR #66 (component-boundary-a0.md, Revision 3 / Corrective-2)

AGENTS.md / CONTEXT.md / docs/README.md          governance + vocabulary
docs/architecture/overview.md                    architecture v2 overview
docs/architecture/composition-kernel.md          generic kernel preconditions + invariants
docs/architecture/component-boundary-a0.md       accepted #53 audit (Rev 3)
#46  PLAYER-PLUGIN-ARCH-1                         parent architecture authority (open)
#53  COMPONENT-BOUNDARY-A0                       PASS / CLOSED (two corrective rounds)
#67  COMPOSITION-KERNEL-0                        current DESIGN gate (this document)
```

Repository reality at BASE: `qianqian-core` (empty `base.rs`, `MusicKernel` state machine, three empty port traits, presentation mapping), `qianqian-runtime` (`AppRuntime` constructor composition), `apps/headless`. All R0 shapes (`AppRuntime::new()`, `with_audio_output()`, `audio_output()`) are bootstrap witnesses, not contracts; replacing them is out of scope here.

### A.2 Frozen inputs from #53 this design must honor

From `component-boundary-a0.md` (Rev 3) and #46, without re-litigation:

```text
MVP runtime components : Music, Decoder, AudioOutput (nothing else)
Capability graph       : Music requires {Decoder, PcmSink};
                         Decoder provides Decoder;
                         AudioOutput provides PcmSink + OutputDeviceDiscovery
Activation rule        : Music binds PcmSink at ACTIVATION (not at track open);
                         SinkSession/device-session existence is composition truth
Binding ownership      : Music owns the SinkSession binding effect; never root-wired
Classifier             : composition truth iff a fresh construction of the desired
                         composition, before any user/domain action, exhibits it
Withdrawal ordering    : dependents finish teardown before provider final release
Effect classes         : Reversible / Transactional / Compensatable / Irreversible /
                         outside recoverable boundary (CLAIMED flush = protocol
                         point-of-no-return; physical render = emission boundary)
Interaction rule       : commutative → independent effects; ordered → explicit structure
Confluence split       : §H.a composition confluence unconditional; §H.b domain
                         continuity policy-conditional only
Degraded rule          : unsatisfied dependency ⇒ dependent inactive/pending, never
                         a root crash (#53 §K.5)
```

### A.3 Evidence actually inspected for this design

| Source | How inspected | Used for |
|---|---|---|
| arXiv:2608.25512v1 PDF (92 pp.) | downloaded, full text read (§1–§6, all definitions/theorems cited below located directly) | Evidence ledger §B, mapping §C, all semantic sections |
| Cordis docs mirror (`@deepseek-ai/cordis`) | context7 query results (fiber states, `ctx.effect`, `ctx.inject`, registry, loader, HMR, events) | [CORDIS] rows in §B/§C |
| Frozen playback evidence (`playback-reference-v1`) | **not re-inspected**; carried only as frozen facts inside `component-boundary-a0.md` §A.2 | withdrawal traces (§G), RT firewall (§N) |

Not inspected (and not relied upon): Cordis/`@deepseek-ai/cordis` source code beyond the docs mirror (web quota exhausted during this task); any DeepSeek harness internals beyond what the paper and docs state. Every conclusion that would depend on uninspected internals is marked [CORDIS]-doc or [OPEN] accordingly.

---

## B. Paper Evidence Ledger

Legend: Prov = provenance. "What it does NOT prove" is the adversarial column — it exists so unsupported extrapolation is detectable.

| # | Claim | Prov | Exact source | What it proves | What it does NOT prove | K0 consequence |
|---|---|---|---|---|---|---|
| B1 | Every context transformation can carry an inverse; inverses compose in twisted (LIFO-accumulating) order | [PAPER] | §3.1 Def 1–3, Thm 5, Thm 7 | one sequence of effects can be accumulated and recovered in LIFO order | nothing about interleaving several components' effects (paper says so explicitly, §3.1.3 end) | intra-fiber teardown = owned effects unwind LIFO (§H.1) |
| B2 | A revertible effect's inverse is only obligated at the state where the effect was applied (`g(δ)=γ` per application) | [PAPER] | §3.1 Def 8, Def 12, Thm 15 | local revertibility is a per-application witness, not a global two-sided inverse | that the inverse still works at states foreign effects moved — that needs independence (B9) | a disposer proves local revert only; cross-fiber removal needs the independence contract (§H.2) |
| B3 | An effect iterator reifies a staged activation; boundaries between iterations are the only interruption points | [PAPER] | §3.1.3 Def 17–18, Thm 16 | loading = running one iterator; unloading = applying the accumulator; interruption granularity = iteration boundary | that a runtime must expose generator-style iteration — a host may run the whole activation as one step (§4.4 inertia quantifies over all schedules) | K0 activation is one bounded step (atomic transition); no generator machinery (§S) |
| B4 | Dependencies are a typed key table Σ; provide = `set` (a revertible effect), consume = `get`; satisfaction ⊨ d is decidable | [PAPER] | §3.2.1 Def 19–21 | capability provision is itself a tracked, reversible effect | anything about *which* key representation (string/symbol/TypeId) — K is abstract | Capability = key + value type + published operation set (§D, §E) |
| B5 | Every context change is classified against a component's declared specification as activating/deactivating/neutral | [PAPER] | §3.2.2 Def 22 | invalidation is reactive and local to declarations; no polling-and-fail | any specific transport (callback/channel/poll) — mechanism left open | kernel-internal invalidation push; transport is [OPEN] (§T) |
| B6 | A key's published operations act on that key's value alone (operation locality); distinct keys are independent outright | [PAPER] | §3.3.1 Def 29–31; §3.4.2 Thm 45 | different-key operations commute by construction; cross-key coupling must be explicit | that same-key operations commute — that needs the key's own witness (B10) | operation locality is a capability-definition obligation (§H.3) |
| B7 | Observational equivalence ≃ is generated per-key by the tests the key's published operations induce; it is the coarsest respected equivalence | [PAPER] | §3.3.2 Def 31–36, Lemma 32, 38 | restoration is judged through published operations only; withholding outcomes coarsens ≃ (design lever, §3.4.2 POSIX/allocator discussion) | that any two implementations agree on ≃ — each key defines its own | confluence compares composition-truth surfaces only (§I, §M); token values/generations invisible by construction |
| B8 | Whole-state equality is read up to ≃ at the declaring component's own keys; the part no key binds is forgotten | [PAPER] | §3.3.2 Def 33, 34; Lemma 39 | a component's correctness claims restrict to the keys it names | that private state may be compared for confluence | tests may not inspect private internals (§I) |
| B9 | Independence of two effects = every transformation (forward **and inverse**) of one commutes with every transformation of the other, plus non-disturbance of yielded inverses/outcomes | [PAPER] | §3.4.1 Def 40–42, Lemma 41; Thm 43 | under pairwise independence, inverses may run in **any permutation** and still return to the initial state | independence itself — it is a condition on the effects, supplied by key witnesses (B10) and disjoint declarations (Thm 47) | cross-fiber independent removal is only promised where §H's contract holds |
| B10 | A commutative key carries a witness that its operations are pairwise independent (entry-per-registration tables yes; ordered chains no; opaque-handle allocators yes, address-comparing ones no) | [PAPER] | §3.4.2 Def 44–46 + examples | same-key composability is a per-capability proof obligation owned by the **provider** | that ordered pipelines can be made commutative by clever encoding — the paper's middleware-chain example says the opposite | contribution-oriented `register → opaque token` contracts for listener-like capabilities; DSP order stays explicit (§H.4) |
| B11 | A component = (declared deps d, provided keys p, witnessed effect function e); a fiber = one instantiation with parent pointer, own table, retirement flag, lifecycle state | [PAPER] | §4.1 Def 48–49 | plugin-definition vs live-instance separation; provision confined to p; effects witnessed at d∪p | that fibers need child-context hierarchies for K0 — parent pointers exist but MVP graph is flat | Fiber owns identity/requirements/provisions/effects/lifecycle (§D, §F) |
| B12 | The coeffect context is **derived**: σ_γ = union of Active fibers' tables; each key has one possible provider; O-Insert refuses overlapping provisions | [PAPER] | §4.1 Def 50 + O-Insert premise (§4.2.1) | single-source resolution; a second provider of the same key is refused at insertion | what a *desired composition* containing two providers should do — that is an orchestrator/reconcile policy (§6.2 discusses brokers as one answer) | required-single cardinality: ambiguity = composition error at reconcile, never silent pick (§E.2) |
| B13 | Lifecycle has four states — Inactive / Reloading / Active / Unloading — driven by comparing a committed view ω against a target view; transitions: O-Insert/O-Retire/O-Remove + L-Begin/L-Iter/L-Finish/L-Divert/L-Leave/L-Unload | [PAPER] | §4.2 Fig 1, Def 53, rules §4.2.1–4.2.2, Table 1 | a complete transition system with quiescence (`quiet`) predicate; retirement is a request, removal waits for Inactive+empty table+no children | any particular state *naming* for an implementation; failed activations (separate extension, B19) | K0 state machine adopts this shape (§F); names below are Qianqian's |
| B14 | Committed view ω records **which fiber provided each key** (not the value); it is fixed for the whole episode; bindings stay readable through the dependent's own teardown | [PAPER] | §4.1 Def 49; Lemma 59(2); Thm 70 | teardown-access window: a consumer deactivated by a withdrawal still resolves its keys until its own unload completes | that the provider's *service* remains fully functional — only reachability/readability of the binding is claimed | teardown-time access ≠ new-resolution availability (§G.2) |
| B15 | Provider withdrawal ordering: L-Leave marks the provider non-providing first (its table leaves σ_γ); L-Unload is guarded by `¬relied` (no installed fiber resolves a key to it); guard always releases (Theorem 73); provider activates before dependent, releases after dependent | [PAPER] | §4.2.2 L-Leave/L-Unload + Def 54; Thm 70(2); Thm 73 | dependents deactivate and finish teardown **while the provider's bindings remain in place**; deterministic, deadlock-free (under acyclic ≺) | that provider *resource* release is deferred automatically — the model only orders table withdrawal vs dependents; physical resources are §6.1 boundary questions | K0 withdrawal protocol is exactly this guard sequence (§G); Music closes its Decoder handle / tears down SinkSession inside the window |
| B16 | Registry well-formedness is preserved by every rule; a fiber leaving an episode ends with an empty table; removal discards nothing | [PAPER] | §4.3.1 Def 63, Thm 64; Cor 69 | no-leak structural guarantee per deactivation; O-Remove safe once Inactive | absence of *external* resource leaks — outside-Γ locations are §6.1 | failed/deactivated fibers leave no ghost bindings in composition truth (§O, §M) |
| B17 | Recovery exactness: an accumulator applied at a state other fibers moved still withdraws exactly that fiber's contribution, provided pairwise independence (always supplied by the paradigm) or rule-imposed ordering of entangled pairs | [PAPER] | §4.3.2 Def 65, Lemma 66–67, Thm 68 | cross-fiber interleaved removal is sound up to ≃_K | restoration of emissions crossing the system boundary (§6.1) — Thm 68 compares tables only | independent removal contract (§H.2) rests on this, not on disposers alone |
| B18 | Progress + confluence: under acyclic dependency order ≺, bounded activation length, finite names, and components **total on their provision**, every maximal lifecycle sequence ends quiescent, and the quiescent state equals (up to ≃ and fiber renaming) a clean dependency-ordered load of the final composition; vestigial retired entries are observationally invisible | [PAPER] | §4.3.4 Thm 73; §4.3.5 Def 74–76, Lemma 75/77/78/79, Thm 80; Lemma 61/62 | history-independence of the settled composition — the paper's central oracle | (a) quiescence of *domain* state; (b) confluence of *failed* fibers (explicitly excluded, §4.4); (c) any timing/order guarantees during transitions | K0 confluence oracle (§M) inherits these exact preconditions; A0's bind-at-activation rule is what makes AudioOutput/Music total on provision |
| B19 | Failure extension: a raising activation routes into Unloading with the partial accumulator, installs nothing, writes an error **outcome** on the fiber; L-Begin then requires an error-free fiber (no auto-retry against an unchanged environment); retry = revision (reinsertion); confluence excludes failed fibers | [PAPER] | §4.4 Failure; Cor 69 | activation failure leaves no ghost effects; FAILED is a *recorded outcome*, not a retry loop; sibling fibers keep running | that retry policy is forbidden — a host *may* reinsert; the paper only forbids invisible auto-retry | §F.4: FAILED = failed activation attempt; reactivation only via revision/generation (§L) |
| B20 | Asynchrony/inertia: an async host takes the landing alternative of L-Divert only (an in-flight iteration completes); all metatheory still holds | [PAPER] | §4.4 Asynchrony; Alg 5 mutual chaining | K0 may serialize transitions on a single control plane without losing any guarantee | that async transitions are *required* — the synchronous schedule is one legal schedule | K0 control plane is synchronous/serialized (§N); async transport [OPEN] |
| B21 | Isolation realms + interception are *mechanisms* (derived-context realizations), not obligations; the base calculus reads every key at one shared realm | [PAPER] | §3.2.3 Def 24–27; §4.1 disjointness discussion; §4.4 Isolation | multi-realm/intercept exist in the model and implementation, orthogonal to the core guarantees | that K0 needs them — nothing in #53 requires child realms, overrides, or interception | defer realms/interception with explicit triggers (§S) |
| B22 | Service multiplexing (several providers of one interface): exclusive binding (orchestrator switches, consumers perturbed) or a **broker** fiber that providers register with | [PAPER] | §6.2 | broker is a *pattern on top of* single-source, not a kernel primitive | that K0 must ship a broker | broker deferred; required-single + replacement is the K0 story (§E.2, §S) |
| B23 | System boundary: inside = exclusively modifiable + restorable location (tracked, revertible); outside = acts as identity (untracked). Outside operations decompose into **acquisition** (revertible record inside) and **emission** (crosses boundary, irreversible); recovery = withholding (output commit) or **compensation** (coarser, application-supplied equivalence; commutation must be re-proved against it) | [PAPER] | §6.1 | the paper itself legitimizes irreversible/compensatable classes and refuses to pretend `undo` exists for emissions | that compensation participates in the core metatheory — it does not (commutation vs ≃ must be re-established) | A0's Reversible/Transactional/Compensatable/Irreversible/outside classes are paper-aligned (§H.5); CLAIMED flush = emission per I4 |
| B24 | Dependency cycles leave components permanently inactive; the condition is detectable from declarations alone (reportable at load); apparent mutual deps should decompose into integration components, at quadratic authoring cost | [PAPER] | §6.5 | cycle detection is static; mediation components are the sanctioned fix; granularity cost is real engineering, not theorem | that every cycle *must* be decomposed — cost/benefit judgment stays with the designer | A0 §D cycle audit is already this analysis; K0 keeps acyclicity as a reconcile-time check (§L.4) |
| B25 | Nominal key linking alone admits interface drift and key collision; versioned/structural linking is an open problem; Cordis currently uses host package-manager peer dependencies | [PAPER]+[CORDIS] | §6.6 | semver/solver/structural compatibility machinery is explicitly *not* solved by the paradigm | that K0 should defer it — confirmed: single compilation unit in K0 makes 𝒱_k typing sufficient | no semver/no solver/no marketplace (§R, §S); TypeId-era identity is intra-crate [QIANQIAN] |
| B26 | `ctx.effect` registers an effect whose disposers run LIFO on unload or on manual dispose (idempotent); double-dispose is a no-op; effects on a disposed fiber raise `INACTIVE_EFFECT`; disposal can be awaited | [CORDIS] | docs `cordis-api/fiber.md` (effect/disposer semantics); paper Alg 1 matches | the implementation really is the paper's accumulator discipline | any Rust-shaped API — TypeScript closures do not translate directly | Effect ownership semantics carried into K0; representation deferred (§P) |
| B27 | Implementation fiber states: PENDING, LOADING, ACTIVE, FAILED, UNLOADING, DISPOSED (paper's four + observable PENDING split + FAILED outcome + DISPOSED); registry enumerable (`ctx.registry` → runtime.fibers with name/state/uid) | [CORDIS] | docs `cordis-api/fiber.md`, `cordis-api/registry.md`, onboarding guide; paper Table 2 (LOADING=Reloading; FAILED=§4.4 outcome) | a practical 6-state vocabulary exists and is used in production (Koishi: 4000+ plugins over four years) | that PENDING/DISPOSED are theoretically distinct from Inactive — they are observational refinements of it | K0 adopts a 7-state vocabulary (six + Absent) with the paper's four as the core (§F) |
| B28 | `ctx.inject(deps, cb)` mounts a dependency-gated fiber that unloads/re-runs when a required service's provider changes; provider disappearance unloads dependents automatically, reappearance reloads them | [CORDIS] | docs `cordis-api/registry.md`; practical tutorial ("Dependency disappearance after startup") | reactive coeffect composition works end-to-end in the fielded implementation | that reload preserves any domain state — it does not (unload runs disposers first) | K0 target behavior for Music under Decoder/AudioOutput churn (§G.4–G.5, §M) |
| B29 | Cordis core includes an event emitter (`ctx.on('stats/report')`), a component loader over declarative entry trees (id/name/config/disabled; group/include plugins), and an HMR plugin with transactional module reload (cache backup + rollback) | [CORDIS] | docs `cordis-tutorial/04-events.md`, `06-composition-and-hmr.md`; paper §5.2 Def 81, Alg 8–10 | event bus, YAML configuration, and HMR are *implementation features*, absent from the formal core's requirements | that any of them is required by the theory — none is cited by any theorem | event bus ≠ kernel primitive (kernel-internal notify only); loader/HMR rejected for K0 (§S) |
| B30 | A provider overwriting its own binding **in place** is not observed as a change; replacement must withdraw-then-install so dependents re-resolve against the new provider identity | [CORDIS]+[PAPER] | paper §5.1.3 (uid-based target digest discussion) | binding identity is provider-fiber identity, not value equality | — | provider replacement = withdraw + re-provide; equal-value replacement still reactivates dependents (§E.4, §M H2) |

Distinctions this ledger freezes (negative results that constrain the design):

```text
LIFO inverse (B1)              ≠ cross-component independent removal (B9/B17)
forward commutativity (B9(1))  ≠ full independence (B9(2) non-disturbance)
bit-identical restoration (B7) ≠ observational equivalence
revertible effect (B2)         ≠ external-world rollback (B23)
decomposition possible (B24)   ≠ decomposition desirable (quadratic cost)
confluence (B18)               ≠ domain continuity (A0 §H.b) — separate oracles
```

---

## C. Paper vs Cordis vs Qianqian Mapping

### C.1 Concept mapping

| Paper concept | Cordis implementation choice | Qianqian K0 decision | Provenance of K0 decision |
|---|---|---|---|
| Context Γ∞ | first-class `ctx` object; Proxy property access; symbol-keyed slots (`@@store`/`@@isolate`/`@@intercept`) | Context = per-fiber capability/dependency view; single root realm; property-access sugar NOT designed yet (representation, §P) | [PAPER] §3.3 + #67 B bias |
| Capability key k | string/symbol keys, module-augmentation typing | typed capability identity (representation deferred; §P) | [PAPER] K abstract; [QIANQIAN] |
| provide `set(k,v)` | `ctx.set` / service property assignment | provision = tracked effect owned by provider fiber (§E) | [PAPER] Def 20 |
| `get`/consumption | `ctx.get`, proxy `ctx[key]` | resolution through committed view only (§E.3) | [PAPER] Alg 6 |
| Fiber | `fiber` with uid/state/committed/target/inertia; registry | Fiber = live component instance; 7-state lifecycle (§F) | [PAPER] Def 49; [CORDIS] B27 |
| Effect + disposer | `ctx.effect` disposers, LIFO, idempotent, awaitable | owned reversible mutation record; explicit dispose + LIFO unwind (§H) | [PAPER] §3.1; [CORDIS] B26 |
| Activation = run iterator | generator-driven `execute` with per-iteration guard | one bounded synchronous activation step per episode (§F.3) | [QIANQIAN] (legal per B3/B20) |
| Reactive notification `notify` | store-change → refresh affected fibers | kernel-internal invalidation push; transport [OPEN] (§T) | [PAPER] Def 22 |
| Provider withdrawal guard | unload drains notified dependents before disposing (Alg 5 line 25) | K0 withdrawal protocol verbatim (§G) | [PAPER] Thm 70/B15 |
| Events | `ctx.on` emitter in core | **no public kernel event bus**; invalidation is internal; product events = future service plugin | [CORDIS] B29 vs [PAPER] core |
| Loader / declarative config | entry trees (cordis.yml), group/include plugins | in-memory desired composition only; no files/YAML (§L) | #67 G |
| HMR | transactional module reload | rejected for K0 (no dynamic loading) | #67 non-scope |
| Isolation realms / interception | `ctx.isolate`, `ctx.intercept`, loader-managed realms | deferred with triggers (§S) | #67 B bias; B21 |
| Service broker / multiplexing | broker pattern (§6.2) | deferred; required-single + replace (§E.2) | B12/B22 |
| Dependency typing/versioning | host package manager peer deps | single compilation unit; TypeId-level identity suffices; no solver | B25 |
| FAILED | 6-state machine incl. FAILED | failed activation attempt outcome; retry via revision only (§F.4) | B19 |
| Registry diagnostics | `ctx.registry` enumeration | composition-truth diagnostic surface, closed set (§I.3, §R) | B27; A0 §H.a |

### C.2 Cordis features that must NOT enter K0 automatically

| Feature | Why it exists there | K0 disposition | Evidence-based reason |
|---|---|---|---|
| generic public event bus (`ctx.on`) | Koishi chatbot domain needs broadcast events | reject as kernel primitive; allowed later as a normal service plugin (listener registration still an owned effect) | paper core never requires it; A0 "Event Service" rule; #67 non-scope |
| HMR + module cache machinery | dev-time JS module swapping | reject | #67 non-scope (no dynamic loading); no requirement from #53 |
| declarative YAML/JSON files, group/include plugins | operator-authored config trees | reject files; keep abstract desired graph | #67 G ("small in-memory desired tree is sufficient") |
| deep child-context hierarchy, realms, interception | multi-tenant bots, sandboxing, scoped overrides | defer with explicit triggers | B21; #67 B default bias; no #53 invariant needs them |
| service broker / multi-provider coexistence | load balancing, rolling updates, RPC | defer; exclusive binding matches MVP | B22; A0 §D.3 (WASAPI vs null compete at profile level) |
| uid-based async `inertia` handles | JS async transitions | K0 synchronous serialized control plane | B20 legal; RT firewall favors serialization |
| proxy property access as *the* context surface | TS ergonomics | undecided representation | §P |

---

## D. Five-Primitive Semantic Model

Semantics only — Rust representation is §P. All five primitives are necessary (audit below); no sixth is added.

### D.1 Context

| Field | Definition |
|---|---|
| Purpose | the capability/dependency view **visible to one fiber**; the only door through which a fiber reaches anything it did not create itself |
| Owned state | none of its own; it is a *view* — resolution results are read from kernel registry truth via the fiber's committed view |
| Legal operations | resolve a **declared** capability (returns the binding from the committed view); register an owned effect; (providers) perform a provision effect; perform a coeffect operation published by a resolved capability |
| Illegal operations | resolving an undeclared capability (undeclared access); resolving a declared but currently unsatisfied capability outside an active episode (inactive access); carrying product payload (PCM/UI/state); acting as event bus; per-RT-block use |
| Observable facts | which capabilities this fiber's committed view binds, and to which provider fibers |
| Lifecycle | created with the fiber's activation (committed view written at transition start); immutable during the episode; discarded at unload completion — during the fiber's own teardown the view **remains readable** (teardown access, B14) |
| Relationships | derived from Registry+Capability; enforces declaration discipline (B26's `INACTIVE_EFFECT`/paper Alg 6 analogues) |
| Proof obligations | every reachability claim in Thm 70/71 rests on the committed view being episode-fixed |
| Must not know | any payload schema, media, UI, device policy; other fibers' private state; keys outside `d ∪ p` of its fiber |

### D.2 Capability

| Field | Definition |
|---|---|
| Purpose | identity of a service contract that can be required and provided |
| Owned state | identity (K0: one per contract); the value type 𝒱 (service definition); the published operation set 𝒜; the **commutativity witness** for same-key operations (B10); cardinality semantics (K0: required-single) |
| Legal operations | be declared required by a component; be provided by at most one ACTIVE fiber; be resolved through a committed view; have its published operations invoked by holders |
| Illegal operations | carrying per-block payload; being provided twice simultaneously in one realm (composition error, B12); undeclared resolution |
| Observable facts | reachable or not; which fiber provides it; the service value's public operations |
| Lifecycle | static definition; bindings follow provider episodes |
| Relationships | providers/consumers; provision is a tracked Effect of the provider |
| Proof obligations | the provider discharges the commutativity witness (entry-per-registration style) or the contract must be declared ordered and ordered explicitly elsewhere |
| Must not know | provider implementation, devices, files, PCM shape, playback semantics |

MVP capability set (from #53): `Decoder`, `PcmSink`, `OutputDeviceDiscovery` (providers) and `PlaybackControl`, `PlaybackSnapshot` (Music-provided). `PlayerView`/`PlayerAction` is payload vocabulary, not a capability.

### D.3 Fiber

| Field | Definition |
|---|---|
| Purpose | one live component instance; the unit of composition, ownership and lifetime |
| Owned state | identity (name, opaque, never reused); component definition (d, p, e) fixed at mount; parent pointer; own provision table σ (written only by its own effects); retirement flag; lifecycle state θ incl. committed view and error outcome; owned-effect record (accumulator) |
| Legal operations | mount/retire/remove (orchestration); activate/deactivate (lifecycle); own effects; instantiate child fibers (mechanism exists in the model; MVP uses none — flat graph) |
| Illegal operations | writing another fiber's table or control fields (confinement, B-paper Def 55–57); surviving removal with a non-empty table; being destroyed while relied upon |
| Observable facts | lifecycle state; provisions installed; committed view; error outcome if failed |
| Lifecycle | §F |
| Relationships | parent/child (structural), provider/dependent (via capabilities), instantiator (for child fibers) |
| Proof obligations | registry well-formedness (Thm 64); empty table at episode close (Cor 69) |
| Must not know | other fibers' internals; domain payload; that it is "music", "decoder", or "output" |

### D.4 Effect

| Field | Definition |
|---|---|
| Purpose | kernel-visible mutation/resource provenance owned by a fiber, with an inverse where reversal is valid |
| Owned state | the inverse (disposer); ordering position in the owning fiber's accumulator; classification (reversible / transactional / compensatable / irreversible-outside, B23) |
| Legal operations | registered by the owning fiber at composition/activation time; explicitly disposed early by the owner; unwound LIFO at deactivation; for same-key contribution effects: remove only the owner's contribution |
| Illegal operations | being executed after the owning fiber left its episode (except teardown of the effect itself); executing twice (idempotent no-op, B26); wrapping an emission and claiming rollback (B23) |
| Observable facts | existence/count/kind per fiber (composition truth); **not** payload |
| Lifecycle | born at registration inside an episode; dies at dispose or episode close |
| Relationships | owned by exactly one fiber; provision effects create capability bindings; data-edge bindings are effects owned by the consumer |
| Proof obligations | the inverse actually reverts at the state of application (author obligation, paper §5.1.1); same-key independence per D.2 witness |
| Must not know | other fibers; domain semantics of the mutated state |

### D.5 Reconcile

| Field | Definition |
|---|---|
| Purpose | move the running fiber graph toward the desired composition without a privileged imperative `boot()` |
| Owned state | desired composition (abstract entry tree); running registry (shared with kernel); plan-in-progress |
| Legal operations | diff desired vs running; emit mount/retire/revise; detect ambiguity/cycles as **composition errors**; report quiescence |
| Illegal operations | setting a fiber's lifecycle state directly (only orchestration requests + lifecycle rules move fibers — B13); wiring data edges itself; choosing between ambiguous providers; retrying failed activations invisibly |
| Observable facts | desired tree; running composition snapshot; pending/failed entries |
| Lifecycle | perpetual; quiescence is a predicate, not an end state |
| Relationships | sole orchestrator (O-rule) issuer; never bypasses L-rules |
| Proof obligations | termination (Thm 73 preconditions: acyclic ≺, bounded activations, finite entries); confluence (Thm 80 preconditions incl. totality-on-provision) |
| Must not know | why the desired composition changed, domain payloads, device policy |

### D.6 Primitive-budget audit

- **Removing any one?** No. Without Context, declaration discipline (undeclared/inactive access) is unenforceable (D.1). Without Capability as a first-class concept, provider/consumer topology collapses into concrete types (violates #53 topology rule). Without Fiber, effects and bindings have no owner/lifetime. Without Effect, teardown is unattributed and revertibility is folklore. Without Reconcile, desired-state changes degenerate into imperative boot scripts (explicitly rejected, B13's separation of orchestration from lifecycle).
- **Missing a sixth?** Candidates tested: *EventBus* → rejected (kernel-internal notify suffices; product events are a service plugin — B29, C.2). *Session/DataEdge* → **not a primitive**: an owned Effect of a specific kind (§K). *Profile* → not a primitive: it is Reconcile's input datum. *Registry* → not a primitive: it is Fiber-set truth the kernel maintains. No requirement from #53 exceeds the budget of five.

---

## E. Capability Algebra

### E.1 Identity

- Two capabilities are identical iff they are the same contract identity (K0: same definition site; Rust representation deferred to §P — `TypeId`-vs-typed-key is representation, not semantics). [QIANQIAN] on [PAPER]'s abstract K.
- A capability's identity is **not** its provider, value, or version (B30: equal values from different fibers are different resolutions).

### E.2 Cardinality — required-single (K0's only mode)

| Provider count | Semantics |
|---|---|
| 0 | consumers declaring it have target = ⊥ → they are Pending, never ACTIVE; the root never crashes (A0 §K.5; #53 frozen) |
| 1 | canonical case; consumers commit to that provider fiber |
| ≥2 simultaneously | **composition error** at reconcile/planning (B12's insertion disjointness, surfaced as an explicit plan failure), never a silent pick; the desired composition itself is illegal |
| provider replaced | withdraw-then-provide; consumers see provider-identity change and reactivate against the new provider (B30) |

Optional-requirement mode, many-provider mode, broker mode: deferred (§S) — no MVP component needs them.

### E.3 Resolution eligibility and access discipline

```text
resolvable-for-NEW-commitment(k)  ⇔  exactly one fiber f: f.state = ACTIVE ∧ k ∈ provided(f)
readable-through-committed-view(k) ⇔  consumer's committed view binds k  (episode-fixed, B14)
```

- **Undeclared access**: resolving a capability not in the consumer's `d` — rejected at the Context door (D.1); in Rust this should be unrepresentable or a hard error (representation question, §P).
- **Inactive access**: a declared capability resolved while the fiber is not in an episode — rejected (the capability may be absent precisely because the fiber is being invalidated). Paper Alg 6's `INACTIVE_ACCESS`/`UNDECLARED_ACCESS` split is adopted semantically.
- **Teardown access**: during the consumer's own Unloading, the committed view stays readable — this is the window in which Music closes its Decoder handle and tears down its SinkSession (§G).
- **Withdrawing provider**: once a provider enters Unloading it stops satisfying new resolution (`σ_γ` excludes it, B15) — new consumers cannot commit to it even though old ones still hold readable views.

### E.4 Replacement vs coexistence

Replacement = withdraw (old provider leaves ACTIVE; dependents invalidate, tear down, become Pending) → provide (new provider ACTIVE; dependents reactivate against new identity). Coexistence of two providers of one capability is illegal in K0 (E.2). A provider mutating its own service value in place is **not** a replacement and is not observed as one (B30) — but the *service contract* may publish its own change events on its data plane; that is the service's business, not the kernel's.

### E.5 Capability binding table (artifact 2)

| Capability | Provider (MVP) | Consumers | Cardinality | Same-key operations | Commutativity contract |
|---|---|---|---|---|---|
| `Decoder` | Decoder fiber | Music | required-single | `open/probe/decode/seek/EOF` on distinct handles | distinct handles independent by construction (A0 §B.2); same-handle ops are sequential — consumers must not share one handle across fibers |
| `PcmSink` | AudioOutput fiber | Music | required-single | `bind(endpoint) → SinkSession`; session ops single-owner | one session per live binding (activation rule); no same-key cross-fiber algebra on the RT path |
| `OutputDeviceDiscovery` | AudioOutput fiber | (future UI) | required-single | read-only enumeration | reads commute |
| `PlaybackControl` | Music fiber | UI/MediaKeys (future) | required-single | open/play/pause/seek/stop on one session | **ordered** (state machine) — order is owned by Music's domain semantics, NOT by kernel effects |
| `PlaybackSnapshot` | Music fiber | UI (future) | required-single | register listener → opaque token | commutative contribution set; removal of one token never touches others (A0 §E) |

---

## F. Fiber Lifecycle Model

### F.1 Separation of concerns (mandatory question)

```text
plugin/component definition   (d, p, e)   — static, named, reusable
fiber instance                 one (d,p,e) instantiation with identity — the unit of composition
activation attempt (episode)   one bounded run of e against one committed view — the unit of retry/accounting
```

A fiber may have many episodes over its life (deactivate → reactivate when target view returns non-⊥), but **one error outcome per episode, recorded on the fiber**; a failed fiber never silently starts a new episode (B19). Reactivation after failure, or reconfiguration, is a **revision**: retire → (lifecycle deactivation) → remove → re-mount as a fresh fiber/generation (paper §4.4 Configuration). The name may be reused only after removal; no stale committed view can name a removed fiber (Thm 64 corollary, §4.3.1).

### F.2 State diagram (artifact 1)

```text
                     mount (desired)
        ┌─────────┐ ──────────────────► ┌─────────┐
        │ ABSENT  │                     │ PENDING │◄────────────┐
        └─────────┘ ◄─────────────────┐ └────┬────┘             │
            ▲    remove (retired,     │      │ all deps ACTIVE  │ target view
            │    table empty, no      │      │ (target ≠ ⊥)     │ changed to ⊥
            │    children)            │      ▼                  │ (dep withdrawn/
            │                  ┌──────┴────────────┐            │  retired)
            │                  │   ACTIVATING      │            │
            │                  │ (one bounded run  │            │
            │                  │  of e vs view ω)  │            │
            │                  └────┬─────────┬────┘            │
            │             completes │         │ raises error
            │      target still = ω │         │ (partial unwind,
            │                       ▼         ▼  installs nothing)
            │                  ┌────────┐  ┌────────┐
            │   target ≠ ω ───►│ ACTIVE │  │ FAILED │
            │   (divert; unwind└───┬────┘  └────────┘
            │    immediately)     │ provider withdrawal / retire /
            │                     │ target view ≠ committed ω
            │                     ▼
            │            ┌─────────────┐
            └──remove────│  UNLOADING  │
              (after     │ (LIFO unwind;│
               Inactive, │  committed   │
               drained)  │  view readable│
                          │  to the end) │
                          └──────┬──────┘
                                 │ guard: no installed fiber
                                 │ resolves any key to this fiber
                                 │ (dependents drained)
                                 ▼
                          back to PENDING (target ≠ ⊥) ──► ACTIVATING (chained)
                          or   to DISPOSED-equivalent (retired) ──► ABSENT (removed)
                          or   stays recorded as FAILED if the unload
                               followed a failed activation (outcome persists)
```

K0 observable lifecycle vocabulary (7): `Absent, Pending, Activating, Active, Unloading, Failed, (Disposed→Absent)`. Mapping to the paper's four (B13): Pending/Failed/Disposed are observational refinements of **Inactive**; Activating = **Reloading**; the core transition system is the paper's Fig 1 with K0 collapsing L-Iter/L-Finish into one bounded activation step (legal per B3/B20: a synchronous host takes whole-episode steps).

### F.3 Transition table

| From | Trigger | To | Effect/ownership behavior |
|---|---|---|---|
| Absent | reconcile mounts entry | Pending | O-Insert: entry created, empty table, τ=⊥ |
| Pending | all declared keys resolvable (target ≠ ⊥) | Activating | L-Begin: committed view ω frozen; run e |
| Activating | e completes; target still = ω | Active | effects owned; provisions installed; dependents may now commit |
| Activating | e raises | Failed | partial accumulator unwinds; nothing installed (Cor 69); error outcome recorded; no auto retry (B19) |
| Activating | target ≠ ω at completion (divert) | Unloading | unwind accumulated effects immediately; land-in-flight alternative is the only one in K0 (inertia, B20) |
| Active | target ≠ ω (provider withdrawal / retire / replacement) | Unloading | L-Leave: provisions leave σ_γ **first** (dependents invalidate against this), then unwind |
| Unloading | guard released (no relied-upon bindings) | Pending (target ≠ ⊥) or Absent-track (retired) | L-Unload: accumulator applied; table provably empty (Cor 69); committed view discarded last |
| Failed | revision (reconcile replaces entry) | Absent-track → fresh fiber | retry = new generation, never in-place (B19) |
| any | retire request (τ=⊤) | (flag only) | lifecycle rules carry it out; removal only from Inactive-family state with empty table and no children |

### F.4 Illegal transitions

```text
Pending/Failed  → Active          (no activation without passing Activating)
Active          → Activating      (must pass Unloading; no in-place reload)
Unloading       → Active          (guard must release; no resurrection mid-unload)
Failed          → Activating      (no silent retry against unchanged environment)
any             → ABSENT with non-empty table or live dependents   (Thm 64 / Cor 69)
provider final release before all dependents finished             (B15 — kernel invariant)
```

### F.5 FAILED semantics (mandatory question answered)

**FAILED = a failed activation attempt's outcome, recorded on the fiber** — neither a terminal product state nor a retryable kernel state:

- It is *not* terminal for the component: the desired composition still names it, so it stays a first-class entry that reconcile may **revise** (fresh generation).
- It is *not* auto-retried: L-Begin requires an error-free fiber (B19); an unchanged environment cannot silently relaunch it (this is what makes quiescence decidable).
- It does not propagate: siblings keep running (B19).
- A dependency that later becomes resolvable again does **not** clear FAILED by itself — retry is a revision decision owned by reconcile/policy, visible as history. (Confluence consequently excludes failed fibers — §M.3.)
- Domain "retry" policies (e.g., device retry inside AudioOutput) live **inside** the component behind an ACTIVE facade; they are invisible to lifecycle (A0 §G.2 bounded-retry degradation is intra-provider).

---

## G. Provider Withdrawal Protocol (P0)

### G.1 The semantic sequence and its mechanism

The frozen #53/#46 sequence, with the paper mechanism that realizes each step:

```text
1. provider begins withdrawal          retire/replace request → target(provider) ≠ ω   [O-Retire / desired-graph change]
2. stops satisfying new resolution     L-Leave: provider leaves ACTIVE; its provisions
                                       leave σ_γ — new commitment impossible (B15)
3. dependents invalidated (push)       their target views recompute ≠ their ω → L-Leave
                                       cascades transitively along provider edges (B5)
4. dependents leave ACTIVE             each enters Unloading, provisions withdrawn first
5. dependents unwind owned bindings    LIFO accumulator; committed views remain READABLE
                                       (teardown access, B14) — Music closes its Decoder
                                       handle, quiesces + tears down its SinkSession here
6. dependents finish teardown          each reaches empty table (Cor 69); guard on the
                                       provider (¬relied) releases fiber by fiber (Thm 73)
7. provider final release              only now the provider's L-Unload applies its own
                                       accumulator; entry removable
```

### G.2 Precise answers (required question set)

| Question | K0 answer |
|---|---|
| What remains reachable during teardown? | the withdrawing provider's capability bindings **as values** — readable through dependents' committed views until each dependent's own unload completes. Nothing else changes: no *new* resolution sees the provider (step 2). |
| Who may access it? | exactly the fibers whose committed views bind the provider (`relied` set); they are by construction all in Unloading/Pending themselves |
| Can new calls occur? | new *kernel* commitments: no. New *service calls on an existing handle*: the service contract decides — K0 requires contracts to define teardown-time behavior; the Decoder contract keeps `close` valid (A0 §G.1), other calls may fail-closed. This is service-contract semantics, not kernel policy. |
| How are existing service handles treated? | handles are domain resources owned by the consumer (Music owns its open Decoder handle as an effect with a teardown-access window). The kernel never revokes them; it only guarantees the **window** in which the consumer must dispose of them. |
| What prevents use-after-provider-destroy? | the guard: the provider's final release physically cannot run while any installed fiber resolves a key to it (L-Unload premise); plus removal requires an empty table and no children (Thm 64). |
| What prevents a withdrawing provider from accepting new dependents? | σ_γ is the union over **ACTIVE** fibers only; an Unloading provider is invisible to target-view computation (B15). |
| What if dependent teardown fails (disposer errors)? | the effect's inverse has no error channel in K0 semantics — a failing disposer is a defect of the effect owner; the kernel records the failure as a diagnostic (composition truth: teardown anomaly), does not fabricate rollback, and must not wedge: the guard releases on the dependent leaving its episode, and the anomaly is surfaced, not swallowed. [QIANQIAN] on [PAPER]'s total-function setting |
| What if provider destruction fails? | same class: provider stays observable as teardown-anomalous; nothing below it in the ordering is affected (its dependents are already gone). Root disposal reports the anomaly. |
| Chain A ← B ← C (C provides to B provides to A)? | invalidation cascades transitively (step 3); unwind orders *against* the dependency edges (A out, then B, then C); termination follows Thm 73 under the acyclic check of §L.4. K0 enforces acyclicity at reconcile, so the cascade always drains. |

### G.3 Withdrawal trace — Decoder replacement mid-playback (artifact 3)

```text
t0  desired: replace Decoder fiber D1 with D2 (same capability `Decoder`)
t1  Reconcile: retire(D1); target(D1) becomes ⊥
t2  D1 L-Leave: D1 leaves ACTIVE → `Decoder` leaves σ_γ
t3  Music target view recomputes (push) ≠ its ω(D1) → Music L-Leave:
    `PlaybackControl`/`PlaybackSnapshot` leave σ_γ (listener contributions are owned by
    their *registrants*; the contribution registry vanishes with the capability, and
    foreign tokens dispose as idempotent no-ops — H.4)
t4  Music Unloading (teardown access to D1's binding still valid):
      a. decode worker quiesce/join                                [Music effect]
      b. song_close(open handle) — still valid, D1 not yet released [Music effect]
      c. SinkSession binding quiesce/teardown (commit/flush)        [Music effect]
      d. remaining owned effects LIFO
t5  Music reaches empty table → Pending (deps currently unsatisfied)
t6  guard on D1 released (no committed view names D1) → D1 L-Unload:
      FFmpeg closure resources released  — use-after-unload impossible by construction
t7  D2 mounts → ACTIVE → `Decoder` in σ_γ → Music target ≠ ⊥ → Music activates:
      fresh episode, commits ω(D2); binds PcmSink at ACTIVATION (A0 frozen rule);
      domain recovery (reopen same source, park-at-CONFIRMED policy) is Music's
      §H.b business, invisible here
```

### G.4 Withdrawal trace — AudioOutput / device switch

```text
t1  AudioOutput withdrawal (planned switch X→Y or device loss policy):
    PcmSink leaves σ_γ → Music invalidated
t2  Music Unloading with teardown access:
      park pipeline at last CONFIRMED landing (domain) → quiesce RT edge via
      commit/flush handshake → tear down SinkSession binding (Music's effect) →
      device session released by AudioOutput only afterwards (A0 §G.2 order:
      renderer destroyed before engine — same invariant, derived not copied)
t3  new provider Y ACTIVE → Music re-binds by presenting its fill endpoint;
    format renegotiated at bind (SRC parameters may change)
t4  Music resumes per policy; if no replacement exists, Y never appears and
    Music stays Pending — the honest degraded state (A0 §K.5)
```

### G.5 Non-negotiables

No music-specific step, device policy, or track semantics may enter this protocol — G.3/G.4 are *instances*; the mechanism (steps 1–7) is fully generic. The two traces differ only in what the dependent's owned effects are.

---

## H. Effect / Independence / Commutativity Model

### H.1 Local revertibility (intra-fiber)

One fiber's owned effects unwind strictly LIFO within its episode (B1–B3, B26). K0 obligations:

- every kernel-visible mutation performed by a fiber is registered as an owned effect at the moment of mutation;
- the inverse must actually revert at the state of application — an **author obligation** the runtime does not verify (paper §5.1.1) and review/tests must;
- dispose is idempotent; effects cannot fire after their episode ends;
- activation is one bounded step: either it completes, or it raises and unwinds **everything it did** (Cor 69 — a failed/aborted activation installs nothing).

### H.2 Cross-fiber independent removal (what a disposer does NOT prove)

Removing fiber A while B's effects interleave with A's is sound only under the paper's independence condition (B9): every transformation of A (forward **and** inverse) commutes with every transformation of B, and neither disturbs what the other yields. The paradigm supplies this only when (Thm 47 / Lemma 66):

1. provisions are disjoint from the other's declarations (guaranteed structurally — single-source, B12);
2. every key both touch is a **commutative** key (witnessed by the provider, B10).

Engineering translation for K0 — a cross-fiber removal claim requires all of:

```text
inverse correctness      each disposer reverts at the state of application
independence             the shared keys touched are commutative-keyed contracts
commutativity witness    the provider's contract documents entry-per-contribution semantics
operation locality       no operation secretly reads/writes other keys or hidden globals
observational target     "restored" means ≃ at the published-operation surface, not bit identity
```

### H.3 Different-key interactions

Operations at distinct keys are independent outright (Thm 45) **iff** operation locality holds: an operation on key k reads/writes only k's binding (Def 29's typing is what enforces it). Any hidden global, any undeclared cross-key write, voids the theorem's hypothesis — this is why hidden globals and parallel registries are constitutionally banned, not stylistically discouraged.

### H.4 Same-key contribution contract

For listener/observer/route-like capabilities (the `PlaybackSnapshot` listener set is the MVP case):

```text
register(value) → opaque token        token identity = the registrant's contribution only
unregister(token)                     removes exactly that contribution
foreign contributions survive         removing A's token cannot damage B's
token values are observationally meaningless   (no sequence numbers, no indices —
                                      A0 §E: they would leak insertion order and create
                                      artificial non-commutativity; paper's route-table
                                      example, B10)
```

This is the paper's entry-per-registration commutative-key pattern. It is **not** applicable to naturally ordered pipelines: an ordered chain (middleware; DSP) is non-commutative and no encoding changes that (B10's negative example). K0 rule (frozen from #53):

> **Commutative relation → may compose as independent effects. Non-commutative relation → explicit dependency/order/integration structure.**

For DSP (`EQ → Compressor ≠ Compressor → EQ`), order lives in: the **declared desired topology** (Reconcile input), the future graph-owner component (`AudioRuntime`, deferred with A0 §J triggers), or the domain kernel — never in registration timing, hash iteration, or mount order.

### H.5 Effect classification and the system boundary

| Class | Definition | MVP examples | Paper grounding |
|---|---|---|---|
| Reversible | tracked effect with a true inverse | capability provision, listener registration, child mount, decoder-handle open/close, SinkSession bind/teardown, ring/worker alloc | B4, §3.1 |
| Transactional | all-or-nothing with fail-closed outcome | seek (CONFIRMED landing or ERROR, never half-seek), track open (READY @0 or ERROR) | [QIANQIAN] semantics; consistent with B19's atomic activation |
| Compensatable | application-supplied coarser recovery | stop-recovery reopen; device switch park-at-CONFIRMED (silence is honest for the interim) | B23 compensation (commutation must be re-proved against the coarser relation) |
| Irreversible (protocol point-of-no-return) | rollback/cancellation authority ends; outcome must be awaited | CLAIMED flush (I4) | B23 emission + A0 corrective P1-3 |
| Outside recoverable boundary | crosses into the external world | physically rendered audio; host-IO side effects | B23 emission |

An Effect may wrap an acquisition (tracked); it must never claim to roll back an emission. `Everything is a Plugin` ≠ `Everything is rollbackable`.

### H.6 Effect-independence matrix (artifact 4)

MVP interactions classified; "independent removal" means: remove one side at quiescence, the other's observable contribution is unchanged.

| Interaction pair | Keys shared? | Class | Independent removal? | Where order lives |
|---|---|---|---|---|
| snapshot listener A ↔ listener B | `PlaybackSnapshot` contribution set | commutative (entry-per-token) | **yes** — by contract (H.4) | none (order-free by design) |
| capability provision D ↔ Music's consumption | `Decoder` (provider/consumer pair) | entangled provider/consumer | removal ordered by §G, **not** by commutativity | lifecycle ordering (guard) |
| Music SinkSession bind ↔ AudioOutput device session | data edge + provider resources | provider/consumer over data edge | teardown ordered: binding torn down before device release (§K) | §G protocol |
| distinct capability bindings at root | none | key-local (Thm 45) | yes | none needed |
| EQ node ↔ Compressor node (future DSP) | PCM graph position | **non-commutative** | **no — and must not be claimed** | explicit ordered topology in desired composition, owned by graph owner |
| visualizer taps (future) ↔ PCM path | read-only taps | commutative observers | yes (tap removal) | none (taps never mutate — the deliberate contrast) |
| two Decoders (WASAPI-vs-null analog) | same capability | same-key **provider competition** | not a composition operation — profile-level choice, second simultaneous provider = composition error | desired composition (E.2) |
| device enumeration reads | `OutputDeviceDiscovery` | read-only | yes | none |
| commit/flush swaps | RT handshake state | ordered single-flight (I3/I4/I5) | no (protocol) | the handshake itself (domain mechanism island) |
| host-IO callbacks ↔ Decoder | payload data edge | not kernel effects | n/a (host-owned) | data-edge ownership (§K) |

---

## I. Observational Equivalence

### I.1 The surfaces

Comparison of restored/settled state happens at exactly these surfaces (closed set — anything else is private):

1. **capability reachability**: for each capability — absent, or provided by fiber X (provider identity compared *up to generation-renaming*, B18/Lemma 61);
2. **fiber lifecycle truth**: each desired fiber's observable state (§F.2 vocabulary), failed outcomes present/absent;
3. **committed bindings**: which consumer binds which provider (semantic identity, not uid values);
4. **contribution sets**: per commutative key, the set of live contributions compared by semantic identity (listener *kinds*), never token values; dispatch count == registered count where observable;
5. **composition-owned data-edge bindings**: SinkSession/device-session existence per live Music↔PcmSink binding (idle ≠ absent — A0 frozen activation rule);
6. **ghost absence**: no bindings, provisions, contributions, sessions, or effects beyond the desired set.

### I.2 What is deliberately NOT compared

```text
allocation addresses / heap layout          opaque token numeric values
fiber internal generation counters          thread IDs
private insertion counters                  exact buffered_frames / underrun_count values
current track / open handle / PlaybackState / position / checkpoints / ENDED / recovery state
```

The last line is domain-session truth (§J). The first lines are invisible to ≃ by construction because no key publishes them (B7) — and Qianqian keeps it that way: no capability may publish incidental identity as observable outcome unless a consumer genuinely needs it (B10's POSIX `open` counter-example is the standing warning).

### I.3 Keeping tests honest

The kernel's diagnostic surface is the §I.1 list and nothing more (§R budget). A confluence test that needs a field outside this list is testing the wrong thing. Diagnostics expose composition truth read-only; they cannot mutate, and they cannot see payload.

---

## J. Composition State vs Domain State Firewall

### J.1 Classifier (frozen, carried from #53 Corrective-2)

> A fact is **composition truth** iff a fresh construction of the desired composition, **before any user/domain action**, deterministically exhibits it. If its existence or value depends on a domain session — which source is open, whether anything is playing, where the checkpoint is — it is **domain truth**.

### J.2 Classifier table (artifact 5)

| Fact | Composition | Domain | Reason |
|---|:---:|:---:|---|
| `Decoder` capability available | ✓ | | fresh build of desired graph has it |
| `PcmSink` / `OutputDeviceDiscovery` reachable | ✓ | | same |
| `PlaybackControl` / `PlaybackSnapshot` reachable (iff Music present) | ✓ | | follows from desired graph alone |
| Music fiber ACTIVE / PENDING / FAILED | ✓ | | lifecycle truth |
| SinkSession exists (idle ≠ absent) | ✓ | | bind-at-activation frozen rule — existence follows from live binding, not track state |
| device session exists under live binding | ✓ | | same |
| exactly one SinkSession per live binding (no ghosts) | ✓ | | leak detector H3 |
| current track / source | | ✓ | needs a domain session |
| open Decoder handle | | ✓ | track-open dependent |
| PlaybackState / position / CONFIRMED landing | | ✓ | domain session |
| ENDED / reopen / recovery state | | ✓ | domain session |
| resume-after-replacement policy outcome | | ✓ | §H.b continuity, policy-conditional |
| listener contribution set (semantic identity) | ✓ | | registration is a composition-plane effect; content payloads flow on the data plane |

### J.3 Adversarial audit rule for kernel diagnostics

Any proposed kernel API/diagnostic needing:

```text
track   position   playback intent   ENDED   device retry policy   reopen checkpoint
```

is an **architecture violation on sight** — redesign (that data lives behind `PlaybackControl`/`PlaybackSnapshot`/provider contracts; the kernel may know "Music failed activation", never "Music failed because the file was bad").

---

## K. Owned Data-Edge Model

### K.1 The pattern

```text
consumer resolves capability            (control plane, once per episode)
        ↓
consumer invokes a control-plane bind   PcmSink.bind(PcmSourceEndpoint) → SinkSession
        ↓
owned session binding created           an Effect owned by the CONSUMER (Music)
        ↓
payload flows directly through the bound endpoint   (data plane; RT island)
        ↓
kernel never sees payload — only that the binding exists, its owner, provider,
and teardown state
```

A data edge is not a capability edge (it carries payload, not reachability), but it has explicit ownership, provenance and teardown (A0 §C). No composition-root pointer wiring exists or is permitted (A0 §J).

### K.2 Binding lifecycle semantics

| Operation | Semantics |
|---|---|
| creation | consumer-only, at activation once the capability is resolvable in its committed view (frozen: at Music activation, not at track open) |
| owner | the consumer fiber — creation, quiesce, teardown are its owned effects |
| provider's role | holds only the endpoint handed to that session; can never reach the consumer as a capability (keeps the graph acyclic, A0 §B.3) |
| teardown (consumer withdrawal) | consumer unwinds the binding effect inside its own Unloading (quiesce via the commit/flush handshake, then release) |
| provider withdrawal | §G.4: provider leaves ACTIVE first; consumer quiesces + tears down the binding while teardown access holds; provider releases device resources only after dependents drain |
| replacement | old binding torn down (above), then a fresh bind against the new provider at re-activation; format renegotiation is part of the bind contract, not a kernel concern |
| root disposal | reverse of activation: Music deactivates fully (including the binding) before Decoder/AudioOutput release (A0 §G.5, proven `pe_destroy` order) |
| failure | a failed activation owns nothing (Cor 69) — no half-bound session can survive |

### K.3 What the kernel knows / must never know

```text
knows: binding exists; owner fiber; provider fiber; teardown state
never: PCM format payload per block, sample values, buffer addresses, media timestamps
```

The RT thread pulls blocks only through the endpoint handed to the session — zero kernel operations per block (§N).

---

## L. Reconcile Semantics

### L.1 Model

```text
Desired graph D    abstract entry tree: {id → component, desired-state (enabled/disabled)}
                  — in-memory only; no YAML/files/config language (#67 G)
Running graph R    the fiber registry + lifecycle states
Difference         per-entry: absent | extra | same-component-different-generation
Transition plan    a sequence of orchestration requests: mount / retire / revise
Settlement         quiescence = quiet predicate (B13): every fiber sits at its target view,
                  no transition in flight
```

Reconcile is the **only** issuer of orchestration requests. It never sets lifecycle state directly; it never touches effects; it never wires data edges.

### L.2 Required operations

| Operation | Semantics |
|---|---|
| mount(entry) | insert fiber (Pending); lifecycle rules do the rest (B24's loader argument: no load order needed — providers first is *emergent*, not arranged) |
| unmount(id) | retire → wait for quiesced deactivation → remove (entry may re-mount later) |
| replace provider | retire(old) + mount(new); consumers transition through Pending automatically; **equal-value replacements still reactivate consumers** (B30) |
| missing provider | not an error: dependants sit Pending; root never crashes (A0 §K.5) |
| ambiguous provider | **composition error**: desired graph itself is illegal (two enabled providers of one required-single capability) — reported, plan refused, no silent pick (E.2) |
| dependency cycle | **composition error**: detected from declarations alone (B24); refused at plan time; the runtime never sits on an undetactable deadlock |
| activation failure | fiber records FAILED outcome; reconcile leaves it visible; no invisible retry (B19) |
| revision (config change / retry / re-enable) | retire → deactivate → remove → re-mount as a fresh fiber/generation (paper §4.4 Configuration composite); dependents follow unprompted |
| root disposal | retire all; dependents before providers emerge from the guard ordering; quiescence = empty registry; teardown anomalies reported, not swallowed |

### L.3 Incremental + eventually convergent (chosen), not globally transactional

Decision [QIANQIAN], grounded in [PAPER] Thm 73/80:

- **Incremental**: revisions apply per-entry; untouched fibers never transition (Cor 69 + Thm 68 make this sound — a departing fiber's removal leaves neighbors exactly as they were).
- **Eventually convergent**: correctness is claimed **at quiescence** (Thm 80), not at each intermediate step. Intermediate states are legal, observable, and need not resemble any clean build.
- **Not transactional** (no all-or-nothing mount batches): rejected — it would require a second, stronger recovery machinery on top of the kernel (what would rolling back a half-mounted graph even undo?), and no #53 requirement needs it. Failure consequence: a failed multi-entry plan leaves partial composition with visible FAILED/Pending entries — honest, debuggable, and confluent-excluded (§M.3). Cordis's HMR *does* add a transactional layer (B29, Alg 10) — that is a dev-tooling choice on top, not core semantics; deferred with the rest of HMR.

### L.4 Plan-time checks (preconditions Qianqian enforces before issuing requests)

```text
single-source:  enabled provisions of each capability ≤ 1            (E.2)
acyclicity:     requirement graph ≺ over enabled entries is acyclic  (B24; Thm 73/80 precondition)
finiteness:     finite entries; components cannot self-instantiate unboundedly (MVP: none do)
boundedness:    activation must terminate (engineering obligation on e, B18)
totality:       providers install every declared key on a completed activation
                (MVP frozen: Decoder/PcmSink/Discovery at activation; bind-at-activation rule)
```

---

## M. Composition Confluence Oracle

### M.1 The property (K0 statement)

> For any legal orchestration history H (mounts/retires/revisions only, no activation failures) reaching quiescence, the **composition truth** of the settled registry is observationally equivalent — at the §I.1 surfaces, up to fiber-name renaming and vestigial-entry invisibility — to `Fresh(D_H)`, a clean dependency-ordered load of the final desired composition D_H, **constructed with no domain session** (A0 §H.b clean-baseline rule).

This is [PAPER] Thm 80 (with Lemma 61/62 readings) narrowed to Qianqian's composition-truth classifier (A0 §H.a). Conditions: L.4's checks (acyclicity, totality, finiteness, boundedness) hold.

### M.2 Why the oracle excludes domain truth

`current source`, `PlaybackState`, open `Decoder` handle, `position` are functions of *domain session history* (which track was opened, whether play was pressed). A fresh build of D_H has no such history — nothing in the desired composition determines it. Demanding their equality would make confluence false by definition; A0's §H.b handles them under explicit checkpoint/apply or behavioral-probe policies. The kernel structurally cannot leak them: its diagnostic surface (§I.1) does not contain them (§J.3).

### M.3 Failure exclusion

Histories containing activation failures are **excluded from confluence claims** (B19: schedule-dependent failure breaks endpoint equality). They get their own assertions via §O: no ghost effects, FAILED visible, siblings unaffected. A retry decision is a revision — itself just another history step.

### M.4 Confluence history matrix (artifact 7)

Composition assertions are unconditional; continuity probes are policy-conditional (A0 §H.b) and out of kernel scope. Baselines are clean builds with **no domain session**.

| # | History (→ settle) | Clean baseline | Composition assertions (§I.1) |
|---|---|---|---|
| M0 | root without UiHost, null output (headless) | same root | baseline itself: full capability/fiber/binding truth without UI |
| M1 | provider X absent → present → absent → present (flap, N times) | fresh build with X present | bindings resolve to the *final* X generation; no ghost generations; dependents ACTIVE |
| M2 | A1 → A2 → A1 provider generations (same capability) | fresh build with final A1′ generation | consumers committed to final provider; no stale views; exactly one provider of the capability |
| M3 | consumer mounted **before** provider vs **after** provider (two runs) | fresh build of both | identical settled truth — order of mounting is not observable at quiescence (B24 loader argument) |
| M4 | activation fails once (e.g., device init error) → revise/retry succeeds | fresh build of the succeeded generation | confluent *after* the retry revision; during failure: FAILED visible, no ghosts (§M.3) |
| M5 | same-key contributions added/removed in opposite orders | clean set | final listener sets equal by semantic identity; dispatch count == registered count (H.4) |
| M6 | unrelated Y-side contribution survives X-provider churn | fresh build with Y + final X | Y's contribution set untouched through all X transitions (independence, H.2) |
| M7 | open+play → switch output X→Y → settle (A0 H1) | fresh build on Y, idle | capabilities equal; exactly one live SinkSession + device session on Y; lifecycle clean |
| M8 | output flapped N times, no track ever opened (A0 H3) | clean Y, idle | exactly one SinkSession total — the session-leak detector |
| M9 | decoder replaced while PLAYING / PAUSED / after ENDED (A0 H4) | clean build, no track | all three settle to identical composition truth |
| M10 | listeners in opposite orders (A0 H5) | clean set | M5 stated for the MVP listener set |
| M11 | UiHost removed mid-playback (A0 H6) | root without UiHost | playback capability/fiber truth unchanged by removing a pure consumer |
| M12 | (future DSP) insert EQ → switch output → remove EQ → replace decoder (A0 H7) | clean Music+Decoder+Output(+nodes) | node list == desired order; no ghost nodes/taps |
| M13 | root disposal from any quiescent state (A0 H8) | n/a | empty registry; all composition-owned resources released (device session closed, SinkSession torn down); anomalies reported |

M0–M3, M5–M11, M13 are expressible with the MVP decomposition; M4 needs a fail-then-revise flow; M12 needs DSP (deferred with AudioRuntime).

---

## N. Realtime Firewall

### N.1 The cut

```text
CONTROL PLANE (kernel operations legal here):
composition time · activation · binding · invalidation · teardown · reconcile · diagnostics

RT PATH (per callback/block/sample — kernel machinery FORBIDDEN):
```

### N.2 Per-operation classification

| Operation | Composition/activation time | RT render path |
|---|---|---|
| Context lookup / capability resolution | ✓ (committed view, once per episode) | **forbidden** |
| Fiber state mutation / lifecycle rules | ✓ | **forbidden** |
| Effect registration | ✓ | **forbidden** |
| Reconcile / desired-graph diff | ✓ | **forbidden** |
| generic event dispatch | ✓ (if any) | **forbidden** |
| allocation | ✓ | bounded/pre-allocated only (RT island discipline) |
| locking | ✓ (control plane serialized) | zero-mutex hot path (frozen PlayerEngine corrective) |
| filesystem/network/UI round trips | ✓ | **forbidden** |
| PCM block transport | ✗ (never kernel) | pre-bound SinkSession endpoint only |

**No exceptions are proposed for K0.** If one is ever proposed, it must come with a proof-sized justification against this table (issue #67 J wording). Enforcement mechanism (types vs ownership shape vs audit vs tests) is an implementation question (§T); the semantic firewall is frozen here.

The pattern that makes this possible is §K: bind at activation → RT thread pulls through the pre-bound endpoint → graph changes prepared on the control plane, published at an RT-safe boundary (commit/flush single-flight, I3/I4/I5).

---

## O. Failure Semantics

### O.1 Failure matrix (artifact 6)

Columns: what remains observable · what must be cleaned · what can retry · what becomes FAILED · what blocks quiescence · invariants that must hold.

| Scenario | Observable remains | Must clean | Retry? | FAILED? | Blocks quiescence? | Invariants |
|---|---|---|---|---|---|---|
| provider activation throws | entry + outcome; dependents stay Pending | partial effects of the raising activation (Cor 69: none survive) | revision only | provider fiber | no (failed fibers are quiet-legal, B19) | no ghost provisions; siblings run |
| consumer activation fails after 2 effects, 3rd raises | entry + outcome | effects 1–2 unwound; installs nothing | revision only | consumer fiber | no | empty table at rest (Cor 69) |
| consumer teardown partially fails (disposer error) | teardown anomaly diagnostic; fiber still completes exit | best-effort; anomaly recorded, never swallowed | manual/revision | no (anomaly ≠ activation failure) | the anomalous fiber's own episode closes; others unaffected | guard still releases (dependent left episode); no silent wedge |
| provider withdrawal during consumer activation | consumer divert → Unloading (target moved) | partial activation unwound | automatic (re-activates when satisfiable) | no | no | resolution coherence (Thm 71): no effect survives against a stale view |
| root disposal during withdrawal | registry draining | same §G order, all providers | n/a | no | until drained (finite, Thm 73) | dependents-before-providers to the end |
| replacement appears before old provider fully drains | old: Unloading→gone; new: Pending→ACTIVE | old's release completes regardless | dependents re-activate against new | no | no | single-source never violated: old left σ_γ before new entered |
| same-key disposer called twice | nothing | — | — | — | — | idempotent no-op (B26) |
| effect disposer fails | see row 3 | — | — | — | — | anomaly surfaced; no fabricated rollback |
| two required-single providers appear simultaneously | plan refused before any mount | nothing mounted | fix desired graph | no (composition error, not fiber state) | plan rejected (system keeps previous composition) | ambiguity never becomes runtime state (E.2) |
| dependency chain invalidates transitively (C←B←A withdrawal) | cascade of L-Leave | each dependent unwinds own effects | automatic on re-satisfaction | no | until chain drains (finite, acyclic) | ordering Thm 70(2) |
| activation runs forever | fiber stuck Activating | — | — | no (it's a hang, not a failure) | **yes** — boundedness violated | L.4 boundedness is the guard; a hang is a defect of e, surfaced by diagnostics |
| kernel-external resource exhausted (e.g., no device) | provider Failed or degraded-in-provider | per provider policy | per policy | depends where it raised | no | domain policy firewall: degradation inside an ACTIVE provider is invisible to lifecycle |

### O.2 Reading rules

- No scenario leaves unexplained ghost state: every "must clean" cell is Cor 69 (empty table at episode close) or an explicitly surfaced anomaly.
- Retry is always either *automatic reactive re-activation* (dependency returned; the fiber was never failed) or *visible revision* (after FAILED). Never an invisible loop.
- Quiescence blockers reduce to: in-flight transitions (finite by Thm 73 under L.4) or an unbounded activation (a component defect caught by the boundedness check).

---

## P. Rust Representation Survey

Survey only — **no production API is frozen here.** Signatures are illustrative of semantic problems, not commitments.

### P.1 Candidate representations

| Option | Semantic fit | Type safety | Lifetime difficulty | Teardown behavior | Object safety | Thread safety | RT risk | Complexity | Child-scope future |
|---|---|---|---|---|---|---|---|---|---|
| `TypeId`-keyed capability identity (`TypeId::of::<T>()`) | high (identity is static) | medium (Any downcast at edges) | low | n/a (identity only) | n/a | `Send + Sync` free | none | low | extends poorly to realms (K×R needs a tuple key — paper §4.4) |
| typed capability keys (`struct DecoderKey; impl Capability for DecoderKey { type Service = dyn DecoderService; }`) | high — identity + contract + operations colocated | high (phantom-typed resolution) | low | n/a | trait-object service must be object-safe | free | none | low-mid | good (realms wrap the key) |
| trait-object service definitions (`dyn DecoderService`) behind keys | high (matches 𝒜 published-operation model) | medium-high | medium (lifetime of the binding vs provider) | provider must keep vtable valid through teardown window | constrains service traits (object safety: no generics, no `Self` returns) | requires `Send + Sync` discipline | none on control plane | mid | good |
| string-keyed registry | **rejected** | none | — | — | — | — | — | — | drift/collision territory (B25) |
| generational fiber ids (`FiberId(u32, Gen)` slab/arena) | high (rename-safety of Lemma 61 by construction; uid never reused — B28) | high | low (ids are Copy) | arena slot freed after remove | n/a | behind kernel lock | none | low | good |
| owned registration guards (`struct ListenerToken<'ctx>` with Drop → unregister) | high for same-key contributions | high | medium (borrow of context) | Drop as fallback; explicit dispose primary (ordering must not depend on drop order) | n/a | Send per contract | none | low | good |
| explicit effect stack per fiber (`Vec<Disposer>` unwound LIFO) | high (accumulator made concrete) | medium (boxed closures) | medium (closures capture) | exact LIFO by construction; idempotence per entry | closures fine | `Send` boxed | none | low | good |
| RAII-only teardown (no explicit dispose) | low — loses drain/await and explicit ordering | — | — | Drop cannot await or fail loudly | — | — | — | — | rejected as primary |
| full graph structures (petgraph-style DAG as registry) | medium — registry is derived truth, not a stored graph (B: σ_γ is a union) | — | — | — | — | — | — | mid | derived maps preferred |
| async-transition machinery (tokio tasks per fiber) | medium (B20 legal but unneeded) | — | — | inertia handles complicate teardown | — | — | control plane single-thread preferable | high | deferred |

### P.2 Representation-level problems the semantics forces us to notice

```rust
// teardown access vs new resolution — the type system must express BOTH:
fn resolve_now(&self, key: K) -> Option<Binding>;        // target-view use: ACTIVE providers only
fn resolve_teardown(&self, view: &CommittedView, k: K) -> &dyn Service; // episode-fixed view: readable during Unloading

// object safety bites immediately: a service trait with generics cannot be a dyn capability
trait PcmSink { fn bind(&self, ep: PcmSourceEndpoint) -> Result<SinkSession, BindError>; } // object-safe
// vs
trait Decoder { fn open(&self, io: impl SongIo) -> Handle; }                              // NOT object-safe — needs
//   either `&mut dyn SongIo` or an associated factory; a real §T question
```

These are exactly the "unresolved semantics surfaced by representation" cases the issue asks to expose: (1) two resolution modes must stay distinct in any API; (2) the frozen SongCore host-IO shape (`SongSource` callbacks as payload) constrains the Decoder capability's object-safe form (A0 §K.1 — ABI v1 is a preservation asset and candidate implementation, not necessarily the kernel seam).

### P.3 Provisional direction (not frozen)

Typed static capability keys + object-safe service traits + generational `FiberId` arena + explicit LIFO effect stack + explicit-dispose-primary/guard-Drop-fallback. Synchronous, single-threaded control plane (`!Send` context handles; whole-kernel behind one lock or event loop) — justified by §N, revisitable only with an async-requirement issue.

---

## Q. Adversarial Architecture Review

| # | Attack | Verdict | Evidence / defense |
|---|---|---|---|
| 1 | Is Context becoming Service Locator? | **PASS** | resolution restricted to declared keys through episode-fixed committed views (D.1, E.3); undeclared access is a door-closed error; no `get_anything` surface in the §I.1 budget |
| 2 | Is Context becoming a payload bus? | **PASS** | Context carries reachability only (D.1); PCM/UI/state travel on §K data edges; RT firewall (§N) forbids per-block kernel work outright |
| 3 | Is EventBus leaking into the kernel? | **PASS** | invalidation is kernel-internal (Def 22 analog); public event bus rejected (C.2); future product events are a service plugin |
| 4 | Is Fiber becoming an Actor? | **PASS** | fibers have no mailbox, no scheduling identity; transitions are kernel-driven comparisons of two views (B13); K0 control plane is synchronous |
| 5 | Is Effect becoming generic transaction machinery? | **PASS** | effects are provenance + inverse only; transactional/compensatable classes are declared, not mechanized (H.5); no two-phase commit anywhere |
| 6 | Is Reconcile becoming Kubernetes? | **PASS** | no health probes, no desired-state polling loop over external reality; desired graph is in-memory declarations; quiescence is a predicate over the registry itself (L.1) |
| 7 | Are we accidentally implementing HMR? | **PASS** | no module loading, no caches, no files (C.2); revision is retire+remount of fibers, never code swapping |
| 8 | Are domain semantics entering lifecycle state? | **PASS** | lifecycle vocabulary is domain-free (F.2); degraded-device, PLAYING/PAUSED, ENDED, retry policy all live inside components (F.5, J.3) |
| 9 | Are ordered relations mislabeled commutative? | **PASS** | H.6 matrix puts DSP/commit-flush/control-ops explicitly outside the independent-effect claim; H.4 forbids tokenizing order |
| 10 | Are Rust type tricks hiding unresolved semantics? | **DEFERRED WITH EXPLICIT TRIGGER** | P.2 shows two genuine open shape questions (dual resolution modes; Decoder object safety). Semantics are decided (§E/§G); representation is §T-open, and the implementation issue must resolve them before API freeze |
| 11 | Can a provider be destroyed before dependents? | **PASS** | guard (`¬relied`) + removal preconditions make it structurally impossible (G.2, Thm 64/70/73) |
| 12 | Can foreign contributions be removed accidentally? | **PASS** | same-key removal goes through opaque tokens scoped to the registrant (H.4); independence claims require the commutativity witness (H.2) |
| 13 | Can a failed activation leave ghost state? | **PASS** | raise routes into Unloading with the partial accumulator; Cor 69 empties the table; FAILED carries the outcome (B19, O.1) |
| 14 | Can two legal histories reach visibly different settled composition? | **PASS** (conditioned) | not under L.4 checks + no-failure histories (Thm 80, §M); failure histories are excluded by definition and covered by §O assertions; the conditioning is explicit, not hand-waved |
| 15 | Does anything require kernel work on the realtime path? | **PASS** | §N.2 table: all kernel operations forbidden per block; zero exceptions proposed |

Score: 14 PASS, 1 DEFERRED-WITH-TRIGGER (#10, routed to §T). No DESIGN DEFECT.

---

## R. Complexity Budget

Frozen upper bounds for the future implementation (any excess requires a new architecture issue with evidence):

| Dimension | Budget |
|---|---|
| kernel primitives | **5** (`Context Capability Fiber Effect Reconcile`) — no sixth without an architecture issue |
| fiber lifecycle states (observable vocabulary) | **≤ 7** (Absent Pending Activating Active Unloading Failed + Disposed-as-Absent) |
| orchestration + lifecycle rule kinds | **≤ 9** (paper's O-Insert/O-Retire/O-Remove/L-Begin/L-Leave/L-Unload/L-Divert-completion-check + K0's activation-step collapse of L-Iter/L-Finish) |
| public kernel semantic operations | **≤ 12** (mount retire remove resolve provide bind-data-edge register-effect dispose deactivate drain-unload revise observe-diagnostics) |
| capability cardinality modes | **1** (required-single; optional/many/broker deferred, §S) |
| reconcile concepts | **≤ 6** (desired diff plan revise settle compose-error) |
| diagnostic concepts | **≤ 6** (the §I.1 surfaces) |
| effect classes | **5** (H.5 table) |
| context realms | **1** (root only) |

Explicitly rejected framework-growth patterns (no present requirement proves them): plugin marketplace, version solver/semver machinery, runtime reflection DSL, generic middleware pipeline, distributed/remote discovery, arbitrary nested scopes/realms, macro-DI, runtime scripting, hot module replacement, generic public event bus.

---

## S. Rejected Alternatives

| Alternative | Verdict | Reason (evidence) |
|---|---|---|
| generic public EventBus as kernel primitive | rejected | paper core never requires it; Koishi's need is domain (B29); #67 non-scope |
| service broker / multi-provider coexistence in K0 | deferred (trigger: a real second concurrent provider requirement, e.g. multiple outputs) | broker is a pattern on single-source, not core (B22); MVP has profile-level competition only (A0 §D.3) |
| isolation realms / interception in K0 | deferred (trigger: multi-tenant/sandbox/override requirement) | B21; #67 B default bias; no #53 invariant |
| child-context hierarchy machinery | deferred (trigger: a component that actually instantiates children) | paper Def 52 mechanism is designed-in (D.3) but MVP graph is flat |
| effect-iterator / generator-style incremental activation | rejected for K0 | whole-episode steps are a legal inertial host (B3/B20); generators add machinery with no current requirement |
| async kernel transitions (per-fiber tasks) | deferred (trigger: a blocking activation that must not stall composition) | synchronous serialized control plane suffices (B20); RT firewall favors it |
| in-place provider value mutation as replacement | rejected | provider-identity resolution means equal values are not replacements (B30); withdraw-then-provide is the only observed replacement |
| silent auto-retry of failed activations | rejected | breaks quiescence decidability and confluence accounting (B19); retry = visible revision |
| globally transactional reconcile (all-or-nothing mount batches) | rejected | would need second-order recovery machinery; Thm 80 makes quiescent convergence sufficient (L.3) |
| FAILED as terminal product state / as retry loop | rejected (both extremes) | F.5: outcome-record semantics; revision-owned retry |
| string-keyed capability registry | rejected | B25 drift/collision; typed keys preferred (P) |
| semver/structural dependency solver | rejected for K0 | B25 open problem; single compilation unit makes key typing sufficient |
| YAML/config-file layer | rejected for K0 | #67 G: in-memory desired tree only |
| HMR / dynamic library loading | rejected for K0 | #67 non-scope; no dynamic loading anywhere |
| composition-root pointer wiring of data edges (`root: sink.source = music.endpoint`) | rejected (carried from A0 §J) | bypasses capability plane; no ownership/provenance/teardown (§K) |
| opaque sequence numbers as contribution tokens | rejected (carried from A0 §E) | leaks insertion order, fabricates non-commutativity (H.4) |
| kernel knowing "why" a fiber failed | rejected | J.3 firewall; outcome opaque beyond "failed activation (component-local detail)" |
| disposer-only teardown (no explicit dispose/drain) | rejected | cannot await draining, cannot order against the guard (P.1) |

---

## T. Unresolved Questions

Implementation-issue inputs, not design gaps — each has a frozen semantic answer above and an open representation/mechanism choice:

1. **Invalidation transport** — callback vs channel vs poll: push + teardown-window semantics are frozen (§G); transport is implementation (A0 §K.2).
2. **Realtime-firewall enforcement** — type system vs ownership shape vs audit vs tests: semantic firewall frozen (§N); enforcement mechanism open (A0 §K.7).
3. **Executable confluence surface** — how §I.1 diagnostics are exposed to tests (snapshot function vs event log vs both): frozen content, open shape (A0 §K.6).
4. **Capability-contract Rust shape** — typed keys + object-safe traits vs other; SongCore ABI v1 as *the* Decoder seam vs one implementation behind it (A0 §K.1; P.2 exposes the object-safety collision).
5. **Dual-mode resolution API shape** — `resolve_now` vs `resolve_teardown` (P.2) must remain visibly distinct in any API freeze.
6. **Activation boundedness enforcement** — how the kernel surfaces/disallows non-terminating activations (L.4) beyond review discipline.
7. **Teardown-anomaly surface** — exact diagnostic shape for failing disposers (O.1 row 3) without turning anomalies into lifecycle states.
8. **Domain continuity policy (§K.3 of A0)** — pause-at-CONFIRMED default remains proposed, not frozen; it gates §H.b probes only, never §M confluence.
9. **Device-surprise MVP policy** — fail-closed into G.2 sequence with bounded-retry degradation remains the proposal (A0 §K.4); product decision at implementation time.
10. **Docs drift note** — `AGENTS.md` and `composition-kernel.md` still name #53 as "current gate" (pre-close text); updating governance gate lines is a human call at review time, deliberately not done in this docs-only PR.

---

## Verdict

**PASS** (proposed; subject to human review per delivery discipline).

All PASS criteria of #67 / the tasking are met within this document:

```text
five primitives: precise semantic responsibility          §D
provider withdrawal deterministic                        §G (guard sequence, trace)
teardown-access semantics explicit                       §E.3, §G.1–G.2
local revertibility ≠ cross-fiber independence           §H.1 vs §H.2
same-key composability contract explicit                 §H.4, E.5
non-commutative order has an explicit home               §H.4/H.6, L.1 (desired topology)
composition truth vs domain truth cleanly separated      §J (frozen classifier)
confluence testable without domain leakage               §I, §M
realtime payload bypasses the kernel                     §K, §N
failure paths leave no unexplained ghost state           §O (Cor 69 grounding)
Rust representation downstream of semantics              §P (survey, no freeze)
no unsupported claim presented as paper authority        §B provenance ledger
```

PASS authorizes exactly one next step: **opening a separate `COMPOSITION-KERNEL-0 IMPLEMENTATION` issue.** It does not authorize implementation, Rust API freeze, FFmpeg/WASAPI/PocketJS integration, async-runtime selection, or any §S-deferred machinery. This document stops at the gate.



