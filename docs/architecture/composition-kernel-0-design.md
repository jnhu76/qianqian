# Composition Kernel 0 — Semantic Design

Design-gate deliverable for **#67 COMPOSITION-KERNEL-0** (parent authority **#46**, boundary prerequisite **#53 / PR #66**, accepted audit `component-boundary-a0.md`).

Status: **merged semantic authority (PR #68); semantic design only — implementation NOT authorized.** Nothing here freezes a Rust API, a crate layout, or an async-runtime choice. Acceptance of the current pre-implementation review (Corrective-5, §Verdict) authorizes only opening a separate `COMPOSITION-KERNEL-0 IMPLEMENTATION` issue.

Revision 1 (2026-09-06): initial semantic design (PR #68).

Revision 2 (Corrective-1, 2026-09-07): applies human review round 1 on PR #68 — teardown contract-violation semantics frozen (infallible inverse contract, §G.6/§H.7; §G.2, §F.4, §L, §M, §O, §Q, §T.7); provider replacement frozen as staged orchestration preserving install-level single-source (§E.2/E.4, §G.3, §L.2, §O.1, §M.4); composition-owned Effect vs domain-resource ownership universes split, Option A frozen (§H.5/§H.5.1, §I.1, §J.4, §K.2); CLAIMED ≠ emission wording repaired (§B23, §H.5/§H.5.1); governance gate advanced to #67 in `AGENTS.md` / `composition-kernel.md` / `docs/README.md` / `CONTEXT.md` / `overview.md`; paper authority snapshot pinned (§A.4).

Revision 3 (Corrective-2, 2026-09-07): applies human review round 2 on PR #68 — activation-failure state machine repaired: a raise lands the fiber in Unloading first; FAILED is recorded only by a **fully discharged** unwind of a failed activation; a violated unwind stays latched in Unloading + `TEARDOWN_VIOLATED` and may not reach FAILED; pending activation error is episode metadata, not an eighth state (§F.1–F.5, §G.6, §O.1, §Q13); `TEARDOWN_VIOLATED` folded into the §I.1 fiber lifecycle diagnostic surface (§I.1, §R); the five effect classes re-frozen as a descriptive **system-boundary/action taxonomy** — the K0 Effect has exactly one shape (reversible composition-lifecycle mutation + total inverse), no Effect-class enum, no `Option<Disposer>` (§D.4, §H.5, §H.7, §R, §Q5); domain obligations fenced as neither a sixth primitive nor kernel data — the kernel's whole teardown knowledge is one verdict per fiber, `DISCHARGED` / `CONTRACT_VIOLATED` (§D.6, §G.6, §H.5.1, §J.4, §Q17); §G.4 AudioOutput switch trace now instantiates §E.4's staged old-remove → new-mount explicitly (§G.4).

Revision 4 (Corrective-3, 2026-09-07): applies human review round 3 on PR #68 (review `5127362067`, **PASS_WITH_TWO_CORRECTIVES**) — M4 reclassified from a confluence-history row to a **failure/recovery sanitation oracle**: histories containing an activation failure never widen Thm 80, even when a later revision succeeds; a theorem-backed claim is possible only for the failure-free suffix H′ cut after the failed generation fully discharges and is removed (§M.3, §M.4, §O.2, §Q14, PASS criteria); child-fiber instantiation removed from K0 scope — parent/child semantics are [PAPER] design context only, `child mount` removed from the K0 Effect examples, the no-children removal guard marked vacuous in K0, the §S trigger kept (§D.3, §F.2–F.3, §H.5, §S); verdict closure wording made review-number-neutral (§Verdict).

Revision 5 (Corrective-4, 2026-09-07): applied on `main` after PR #68 merged — the post-merge, pre-implementation adversarial review round. Three semantic repairs: (1) **desired revision identity** frozen — a desired entry conceptually carries an opaque revision-identity token (desired incarnation intent), so explicit fresh-generation intent is representable even when component identity, configuration semantics and enabled-state are unchanged; an unchanged desired incarnation can never retry FAILED, and nothing in the kernel may derive a revision trigger (R1–R8, §L.5, oracle D0–D4); (2) **Effect structural provenance** frozen — "one shape, no class" bans behavioral classification, not structural composition identity; every composition-visible binding has exactly one authoritative ownership/provenance record coupled to one owner fiber episode, and diagnostics are projections of it (§D.4, §K.4); (3) **quiescence** frozen in transition semantics rather than target-view equality — settled FAILED and Pending are quiet-legal, in-flight/staged orchestration and latched violations are not (§L.1). Governance: PR #68 is merged semantic authority; implementation remains blocked pending human acceptance of this corrective (§Verdict).
Revision 6 (Corrective-5, 2026-09-07): applies human review of Corrective-4 (review `5127750303`, **PASS_WITH_ONE_CORRECTIVE**) — **Effect structural provenance made conditional**: every K0 Effect carries the base triple (owner fiber episode, total inverse, LIFO position); structural composition provenance exists only on a **relation-bearing effect** (capability provision/binding, data-edge binding, cross-fiber keyed contribution — capability key, provider/peer fiber identity where applicable), so owner-local reversible effects (timer, watcher, local handle, buffer allocation) fabricate no capability key and no optional field; §D.4 "must not know other fibers" narrowed to "other fibers' internals/domain payload" so §K.4 provenance naming the provider fiber identity is contradiction-free (§D.4, §Q20, §R); the five-label taxonomy now classifies **actions** in every authority wording (§H.5 unchanged; `AGENTS.md`, `composition-kernel.md`, #46). Desired revision identity, quiescence, confluence and the Fiber state machine are untouched by this corrective.

**Playback examples erratum (2026-09):** playback examples in this document (e.g. Music-owned decode worker / PCM ring / open Decoder handle) were inherited from the #53 decomposition available when K0 was designed. They are examples used to instantiate generic K0 lifecycle semantics, not current Playback ownership authority. Playback architecture has since been reopened from first principles; this historical erratum no longer establishes Playback authority — the current Playback Foundations proposal is `ADR-PBK-001` (PROPOSED), and old playback code/specs are experimental evidence only. Generic K0 semantics in this document are unchanged.

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
#53  COMPONENT-BOUNDARY-A0                       PASS / CLOSED (two corrective rounds; PR #66 merged)
#67  COMPOSITION-KERNEL-0                        semantic design MERGED via PR #68
PR #68                                            merged semantic authority (this document, Revisions 1–4)
Corrective-4                                      reviewed by human review `5127750303`: PASS_WITH_ONE_CORRECTIVE
Corrective-5                                      current PRE-IMPLEMENTATION review gate (Revision 6)
future COMPOSITION-KERNEL-0 IMPLEMENTATION       opens only after Corrective-5 is accepted by human review
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

Corrective-2 role note on the effect-class line: in K0 proper these five are carried as A0's descriptive **system-boundary / action taxonomy** (§H.5); the only legal K0 Effect is the Reversible row's composition-lifecycle mutation with a total inverse (§D.4, §H.7). None of the five is a kernel Effect variant.

### A.3 Evidence actually inspected for this design

| Source | How inspected | Used for |
|---|---|---|
| arXiv:2608.25512v1 PDF (92 pp.) | downloaded, full text read (§1–§6, all definitions/theorems cited below located directly) | Evidence ledger §B, mapping §C, all semantic sections |
| Cordis docs mirror (`@deepseek-ai/cordis`) | context7 query results (fiber states, `ctx.effect`, `ctx.inject`, registry, loader, HMR, events) | [CORDIS] rows in §B/§C |
| Frozen playback evidence (`playback-reference-v1`) | **not re-inspected**; carried only as frozen facts inside `component-boundary-a0.md` §A.2 | withdrawal traces (§G), RT firewall (§N) |

Not inspected (and not relied upon): Cordis/`@deepseek-ai/cordis` source code beyond the docs mirror (web quota exhausted during this task); any DeepSeek harness internals beyond what the paper and docs state. Every conclusion that would depend on uninspected internals is marked [CORDIS]-doc or [OPEN] accordingly.

### A.4 Paper authority snapshot (Corrective-1)

The paper repository describes the preprint as under active revision, and different drafts use different theorem numbering (public summaries of older drafts cite Thm 63/66/73 for what this snapshot numbers 70/73/80). This document therefore pins the exact snapshot all §B citations refer to:

```text
Paper authority snapshot:

title:          A Programming Paradigm for Spatiotemporal Composability
arXiv:          2608.25512v1  (v1 is the pinned version)
retrieved:      2026-09-07
page count:     92
source:         https://arxiv.org/pdf/2608.25512v1
SHA256:         390775dbc9debdcf2ed1b076eed013387ca057630be3cb594617b2b742e48cf0
cross-check:    numbered items run to Def 81 (HMR, B29); Thm 70 (teardown-access
                window), Thm 73 (quiescence/progress), Thm 80 (confluence) and
                Cor 69 (departing fiber contributes nothing) verified present at
                this snapshot with exactly the meanings cited in §B
```

> **Snapshot rule (frozen).** Theorem/definition/algorithm numbers in this document are valid **only for the pinned snapshot above**. If the paper revises, do not silently migrate theorem numbers across revisions: re-run the §B evidence-ledger mapping against the new snapshot first, update the pin block, and only then update citations. A citation whose target moved or vanished under a new snapshot is an unverified claim until re-inspected.

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
| B12 | The coeffect context is **derived**: σ_γ = union of Active fibers' tables; each key has one possible provider; O-Insert refuses overlapping provisions | [PAPER] | §4.1 Def 50 + O-Insert premise (§4.2.1) | single-source resolution; a second provider of the same key is refused at insertion | what a *desired composition* containing two providers should do — that is an orchestrator/reconcile policy (§6.2 discusses brokers as one answer) | required-single cardinality: ambiguity = composition error at reconcile, never silent pick (§E.2). The O-Insert disjointness `∀m. pₙ ∩ pₘ = ∅` runs over **installed** fibers' declared provisions — so replacement must be staged (old removed before new inserted, §E.4) |
| B13 | Lifecycle has four states — Inactive / Reloading / Active / Unloading — driven by comparing a committed view ω against a target view; transitions: O-Insert/O-Retire/O-Remove + L-Begin/L-Iter/L-Finish/L-Divert/L-Leave/L-Unload | [PAPER] | §4.2 Fig 1, Def 53, rules §4.2.1–4.2.2, Table 1 | a complete transition system with quiescence (`quiet`) predicate; retirement is a request, removal waits for Inactive+empty table+no children | any particular state *naming* for an implementation; failed activations (separate extension, B19) | K0 state machine adopts this shape (§F); names below are Qianqian's |
| B14 | Committed view ω records **which fiber provided each key** (not the value); it is fixed for the whole episode; bindings stay readable through the dependent's own teardown | [PAPER] | §4.1 Def 49; Lemma 59(2); Thm 70 | teardown-access window: a consumer deactivated by a withdrawal still resolves its keys until its own unload completes | that the provider's *service* remains fully functional — only reachability/readability of the binding is claimed | teardown-time access ≠ new-resolution availability (§G.2) |
| B15 | Provider withdrawal ordering: L-Leave marks the provider non-providing first (its table leaves σ_γ); L-Unload is guarded by `¬relied` (no installed fiber resolves a key to it); guard always releases (Theorem 73); provider activates before dependent, releases after dependent | [PAPER] | §4.2.2 L-Leave/L-Unload + Def 54; Thm 70(2); Thm 73 | dependents deactivate and finish teardown **while the provider's bindings remain in place**; deterministic, deadlock-free (under acyclic ≺) | that provider *resource* release is deferred automatically — the model only orders table withdrawal vs dependents; physical resources are §6.1 boundary questions | K0 withdrawal protocol is exactly this guard sequence (§G); Music closes its Decoder handle / tears down SinkSession inside the window |
| B16 | Registry well-formedness is preserved by every rule; a fiber leaving an episode ends with an empty table; removal discards nothing | [PAPER] | §4.3.1 Def 63, Thm 64; Cor 69 | no-leak structural guarantee per deactivation; O-Remove safe once Inactive | absence of *external* resource leaks — outside-Γ locations are §6.1; the empty-table conclusion presupposes every inverse runs to completion (total-function setting — an inverse that violates its contract exits this guarantee, §G.6) | failed/deactivated fibers leave no ghost bindings in composition truth (§O, §M); K0 strengthens episode close with a discharge requirement (§G.6) |
| B17 | Recovery exactness: an accumulator applied at a state other fibers moved still withdraws exactly that fiber's contribution, provided pairwise independence (always supplied by the paradigm) or rule-imposed ordering of entangled pairs | [PAPER] | §4.3.2 Def 65, Lemma 66–67, Thm 68 | cross-fiber interleaved removal is sound up to ≃_K | restoration of emissions crossing the system boundary (§6.1) — Thm 68 compares tables only | independent removal contract (§H.2) rests on this, not on disposers alone |
| B18 | Progress + confluence: under acyclic dependency order ≺, bounded activation length, finite names, and components **total on their provision**, every maximal lifecycle sequence ends quiescent, and the quiescent state equals (up to ≃ and fiber renaming) a clean dependency-ordered load of the final composition; vestigial retired entries are observationally invisible | [PAPER] | §4.3.4 Thm 73; §4.3.5 Def 74–76, Lemma 75/77/78/79, Thm 80; Lemma 61/62 | history-independence of the settled composition — the paper's central oracle | (a) quiescence of *domain* state; (b) confluence of *failed* fibers (explicitly excluded, §4.4); (c) any timing/order guarantees during transitions | K0 confluence oracle (§M) inherits these exact preconditions; A0's bind-at-activation rule is what makes AudioOutput/Music total on provision |
| B19 | Failure extension: a raising activation routes into Unloading with the partial accumulator, installs nothing, writes an error **outcome** on the fiber; L-Begin then requires an error-free fiber (no auto-retry against an unchanged environment); retry = revision (reinsertion); confluence excludes failed fibers | [PAPER] | §4.4 Failure; Cor 69 | activation failure leaves no ghost effects; FAILED is a *recorded outcome*, not a retry loop; sibling fibers keep running | that retry policy is forbidden — a host *may* reinsert; the paper only forbids invisible auto-retry | §F.3/F.5: raise → Unloading (partial unwind, pending activation error kept as episode metadata) → FAILED only on full discharge; a violated unwind stays latched in Unloading (§G.6); reactivation only via revision/generation (§L) |
| B20 | Asynchrony/inertia: an async host takes the landing alternative of L-Divert only (an in-flight iteration completes); all metatheory still holds | [PAPER] | §4.4 Asynchrony; Alg 5 mutual chaining | K0 may serialize transitions on a single control plane without losing any guarantee | that async transitions are *required* — the synchronous schedule is one legal schedule | K0 control plane is synchronous/serialized (§N); async transport [OPEN] |
| B21 | Isolation realms + interception are *mechanisms* (derived-context realizations), not obligations; the base calculus reads every key at one shared realm | [PAPER] | §3.2.3 Def 24–27; §4.1 disjointness discussion; §4.4 Isolation | multi-realm/intercept exist in the model and implementation, orthogonal to the core guarantees | that K0 needs them — nothing in #53 requires child realms, overrides, or interception | defer realms/interception with explicit triggers (§S) |
| B22 | Service multiplexing (several providers of one interface): exclusive binding (orchestrator switches, consumers perturbed) or a **broker** fiber that providers register with | [PAPER] | §6.2 | broker is a *pattern on top of* single-source, not a kernel primitive | that K0 must ship a broker | broker deferred; required-single + replacement is the K0 story (§E.2, §S) |
| B23 | System boundary: inside = exclusively modifiable + restorable location (tracked, revertible); outside = acts as identity (untracked). Outside operations decompose into **acquisition** (revertible record inside) and **emission** (crosses boundary, irreversible); recovery = withholding (output commit) or **compensation** (coarser, application-supplied equivalence; commutation must be re-proved against it) | [PAPER] | §6.1 | the paper itself legitimizes irreversible/compensatable classes and refuses to pretend `undo` exists for emissions | that compensation participates in the core metatheory — it does not (commutation vs ≃ must be re-established) | A0's Reversible/Transactional/Compensatable/Irreversible/outside classes are paper-aligned **as a descriptive system-boundary/action taxonomy, not kernel Effect variants** (§H.5, Corrective-2); CLAIMED flush = **protocol point-of-no-return** (cancellation authority ends, I4) — **not** the external emission itself; physical render is the emission boundary (A0 corrective P1-3, §H.5.1) |
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
| FAILED | 6-state machine incl. FAILED | failed activation attempt outcome, recorded only via a discharged unwind; retry via revision only (§F.3/F.5) | B19 |
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
| Owned state | identity (name, opaque, never reused); component definition (d, p, e) fixed at mount; parent pointer ([PAPER] model context only — carries no K0 semantics; child mounting deferred, §S); own provision table σ (written only by its own effects); retirement flag; lifecycle state θ incl. committed view and error outcome (a raise records a *pending activation error* as episode metadata that becomes the FAILED outcome only at discharged cleanup, §F.5; a latched §G.6 violation is a diagnostic flag, not a state); owned-effect record (accumulator) |
| Legal operations | mount/retire/remove (orchestration); activate/deactivate (lifecycle); own effects. Child-fiber instantiation is **not** a K0 fiber operation: the paper's Def 52 mechanism is preserved as [PAPER] design context only, deferred with its §S trigger — the K0 graph is flat (Corrective-3) |
| Illegal operations | writing another fiber's table or control fields (confinement, B-paper Def 55–57); surviving removal with a non-empty table; being destroyed while relied upon; instantiating child fibers (out of K0 scope — [PAPER] context only, deferred §S; Corrective-3) |
| Observable facts | lifecycle state; provisions installed; committed view; activation-failure outcome if failed; teardown-contract-violation flag (§G.6 — §I.1 surface 2) |
| Lifecycle | §F |
| Relationships | provider/dependent (via capabilities). Parent/child and instantiator relations are [PAPER] context only, not K0 semantics (child mounting deferred, §S — Corrective-3) |
| Proof obligations | registry well-formedness (Thm 64); empty table at episode close (Cor 69) |
| Must not know | other fibers' internals; domain payload; that it is "music", "decoder", or "output" |

### D.4 Effect

| Field | Definition |
|---|---|
| Purpose | kernel-visible mutation/resource provenance owned by a fiber. **Every K0 Effect is a composition-lifecycle reversible mutation with a total inverse (§H.7)** — an action that is not reversible-with-total-inverse is not a K0 Effect at all (Corrective-2: no effect-class variants) |
| Owned state | the **base triple** every K0 Effect carries: its owner fiber episode; the total inverse (disposer, §H.7); its ordering position in the owning fiber's accumulator. **Structural composition provenance is conditional** (Corrective-5; Corrective-4 over-froze it as unconditional): it exists only on a **relation-bearing effect** — one that contributes to a composition-visible relation — and then records the capability key the effect acts on, plus the provider fiber identity when that relation is a data-edge binding (§K.4). An **owner-local effect** (timer registration, watcher, local handle, buffer/resource allocation inside the system boundary) contributes to no composition relation and carries no provenance beyond the base triple — no fabricated capability key, no silently-optional field. **No class field**: the five A0 classes are a descriptive system-boundary/action taxonomy (§H.5), never kernel Effect variants |
| Legal operations | registered by the owning fiber at composition/activation time; explicitly disposed early by the owner; unwound LIFO at deactivation; for same-key contribution effects: remove only the owner's contribution |
| Illegal operations | being executed after the owning fiber left its episode (except teardown of the effect itself); executing twice (idempotent no-op, B26); wrapping an emission and claiming rollback (B23) |
| Observable facts | existence/count per fiber; for a relation-bearing effect, the composition relation it contributes to (capability key; for a data-edge binding, the provider fiber — §K.4); an owner-local effect exposes no relation to observe. **Not** payload, and not a behavioral class/kind |
| Lifecycle | born at registration inside an episode; dies at dispose or episode close |
| Relationships | owned by exactly one fiber; provision effects create capability bindings; data-edge bindings are effects owned by the consumer |
| Proof obligations | the inverse is a **total semantic obligation** (§H.7): it actually reverts at the state of application and has no failure outcome — an author obligation the runtime does not verify (paper §5.1.1); violation latches §G.6. Same-key independence per D.2 witness |
| Must not know | other fibers' internals/domain payload (Corrective-5, aligning with D.3 — naming a peer/provider fiber *identity* in structural provenance is not knowledge of that fiber's internals); domain semantics of the mutated state |

Corrective-4 note — **provenance is not a class**. "No class/kind" bans
*behavioral* classification of Effects into reversible/transactional/domain
variants; it does **not** make an Effect structurally anonymous. A provision
effect cannot exist without naming its capability key (B4: provision is
`set(k,v)` on key k); a data-edge binding effect cannot carry §K.3 teardown
truth without naming its owner, provider, and relation. The one legal Effect
shape therefore includes minimal **structural provenance** — identity of the
composition relation, never payload, never behavioral taxonomy (§H.5, §K.4).

Corrective-5 note — **provenance is conditional on bearing a relation** (the
Corrective-4 note above, and the Owned-state row it froze, over-reached: they
read as if every Effect must name a capability key). Two distinctions, both
frozen:

```text
behavioral class       never exists on Effect (§H.5) — no enum, no kind
structural provenance  exists only on a relation-bearing Effect

relation-bearing Effect             owner-local reversible Effect
  capability provision                timer registration
  capability binding                  watcher
  data-edge binding (§K)              local handle
  cross-fiber keyed contribution      buffer/resource allocation inside
                                        the system boundary
```

The test is whether the effect contributes to a composition-visible relation
that must survive independent removal, provider withdrawal and ghost audits
(§G, §H.4, §I.1.6) — not the effect's mechanism. An owner-local effect that
fabricated a capability key would pollute same-key independence reasoning
(§H.4) with a key that denotes nothing; a silently-optional provenance field
would be the `Option<Disposer>` mistake (§H.5) in structural clothing. Two
non-implications close the loop: structural provenance does not imply
crossing fibers (the owner's own capability provision is relation-bearing
and intra-fiber), and carrying provenance does not imply a behavioral kind —
**behavioral class ≠ structural provenance** remains the one-shape rule
(§H.5, §H.7): no `EffectKind`, no `DataEdgeRegistry`, no sixth primitive
(§D.6).

### D.5 Reconcile

| Field | Definition |
|---|---|
| Purpose | move the running fiber graph toward the desired composition without a privileged imperative `boot()` |
| Owned state | desired composition (abstract entry tree; each entry carries an opaque desired revision identity, §L.5); running registry (shared with kernel); plan-in-progress |
| Legal operations | diff desired vs running; emit mount/retire/revise; detect ambiguity/cycles as **composition errors**; report quiescence |
| Illegal operations | setting a fiber's lifecycle state directly (only orchestration requests + lifecycle rules move fibers — B13); wiring data edges itself; choosing between ambiguous providers; retrying failed activations invisibly |
| Observable facts | desired tree; running composition snapshot; pending/failed entries |
| Lifecycle | perpetual; quiescence is a predicate, not an end state |
| Relationships | sole orchestrator (O-rule) issuer; never bypasses L-rules |
| Proof obligations | termination (Thm 73 preconditions: acyclic ≺, bounded activations, finite entries); confluence (Thm 80 preconditions incl. totality-on-provision) |
| Must not know | why the desired composition changed, domain payloads, device policy |

### D.6 Primitive-budget audit

- **Removing any one?** No. Without Context, declaration discipline (undeclared/inactive access) is unenforceable (D.1). Without Capability as a first-class concept, provider/consumer topology collapses into concrete types (violates #53 topology rule). Without Fiber, effects and bindings have no owner/lifetime. Without Effect, teardown is unattributed and revertibility is folklore. Without Reconcile, desired-state changes degenerate into imperative boot scripts (explicitly rejected, B13's separation of orchestration from lifecycle).
- **Missing a sixth?** Candidates tested: *EventBus* → rejected (kernel-internal notify suffices; product events are a service plugin — B29, C.2). *Session/DataEdge* → **not a primitive**: a composition-owned binding Effect with structural provenance, one authority per binding (§K.4). *Obligation* → **not a primitive** (Corrective-2): the kernel's entire teardown knowledge is one verdict per fiber — `DISCHARGED` / `CONTRACT_VIOLATED` (§G.6), which is Fiber lifecycle truth; the obligations themselves are component-contract content (§H.5.1), never kernel data. *Profile* → not a primitive: it is Reconcile's input datum. *Registry* → not a primitive: it is Fiber-set truth the kernel maintains. No requirement from #53 exceeds the budget of five.

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
| provider replaced | staged replacement (§E.4): the old provider's episode fully closes and its entry is removed **before** the replacement is mounted; consumers reactivate against the new provider identity (B30); the running registry never holds two providers of one capability |

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

### E.4 Replacement vs coexistence (frozen: staged orchestration)

K0 freezes two distinct notions:

```text
desired graph     MAY express replacement intent as a single logical edit
                  (entry A1's component becomes A2) — the desired tree is not
                  the running registry and carries no per-step lifecycle truth

running registry  NEVER simultaneously holds two installed providers of one
                  capability — not transitively, not transiently, not
                  "Pending-but-declared"
```

Replacement intent is realized as an explicitly **staged** orchestration, issued only by Reconcile on the serialized control plane:

```text
desired replacement A1 → A2

1. retire A1                    target(A1) → ⊥
2. A1 leaves new resolution     L-Leave: A1's provisions leave σ_γ
3. dependents invalidate        their target views recompute; L-Leave cascades
4. dependents teardown          each unwinds its obligations inside the
                                teardown-access window (B14); each reaches a
                                closed episode or latches §G.6
5. A1 completes unload          guard (¬relied) releases; L-Unload applies;
                                table provably empty (Cor 69 + §G.6 discharge)
6. A1 is removed                O-Remove: entry gone from the registry
7. mount A2                     O-Insert: now, and only now, is A2 inserted
8. A2 activates                 provisions enter σ_γ
9. dependents reactivate        against the new provider identity (B30)
```

**Invariant (frozen; demanded by the paper's own registry well-formedness):** at every step of the sequence — hence at every point of any legal K0 history — the set of installed fibers declaring provision for capability k has **at most one** member. This is not an extra K0 restriction: the paper's O-Insert premise `∀m. pₙ ∩ pₘ = ∅` (B12) quantifies over the **declared provision sets of all installed fibers**, not merely over the derived active context σ_γ. An Unloading old fiber is still installed, so inserting an overlapping new provider before the old is removed would violate the registry invariant outright — the review's reading is the correct one, and the simple staged sequence above is the only replacement shape the formal model licenses. K0 adopts it verbatim and gains a practical bonus: required-single becomes decidable by inspection at every step (not only at quiescence), and no question of which record owns the binding mid-flight can arise. The cost is one drain delay on the serialized control plane — and the honest Pending gap it produces is the same degraded state consumers sit in whenever a dependency is absent (A0 §K.5). A dual-provider intermediate state is therefore not an optimization available to K0; it is forbidden by the formal registry invariant, and frozen out.

Because the control plane is synchronous and serialized (§N), a mount request targeting a capability whose previous provider has not completed removal is simply **not issued early**: Reconcile stages it after the removal. Step 7 never races step 6.

Coexistence of two providers of one capability remains illegal in K0 (E.2). A provider mutating its own service value in place is **not** a replacement and is not observed as one (B30) — but the *service contract* may publish its own change events on its data plane; that is the service's business, not the kernel's.

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

A fiber may have many episodes over its life (deactivate → reactivate when target view returns non-⊥), but **one error outcome per episode**: a raise records a *pending activation error* as episode metadata, and the FAILED outcome is recorded on the fiber only once the partial unwind has fully discharged (§F.3/F.5, Corrective-2) — a violated unwind leaves the fiber latched in Unloading with no outcome recorded (§G.6). A failed fiber never silently starts a new episode (B19). Reactivation after failure, or reconfiguration, is a **revision**: retire → (lifecycle deactivation) → remove → re-mount as a fresh fiber/generation (paper §4.4 Configuration). The name may be reused only after removal; no stale committed view can name a removed fiber (Thm 64 corollary, §4.3.1).

### F.2 State diagram (artifact 1)

```text
                      mount (desired)
        ┌─────────┐ ────────────────────► ┌─────────┐◄────────────┐
        │ ABSENT  │                       │ PENDING │             │
        └─────────┘ ◄──────────────────┐ └────┬────┘◄────────────┘
            ▲     remove (retired,     │      │ all deps ACTIVE  │ stays PENDING:
            │     table empty, no      │      │ (target ≠ ⊥)     │ target view → ⊥
            │     children)            │      ▼                  │ (dep withdrawn/
            │                   ┌──────────────┐                 │  retired)
            │                   │  ACTIVATING  │──────┐
            │                   │ (one bounded │      │ raises error (B19):
            │                   │  run of e vs │      │ pending activation error
            │                   │  view ω)     │      │ kept as episode metadata;
            │                   └──────┬───────┘      │ nothing installs; partial
            │          completes,      │              │ unwind starts here
            │          target still=ω  │              │
            │                           ▼              │
            │                      ┌────────┐         │
            │                      │ ACTIVE │         │
            │                      └───┬────┘         │
            │                          │ target ≠ ω  │
            │                          │ (divert /   │
            │                          │  withdrawal/│
            │                          │  retire):   │
            │                          │  unwind now │
            │                          ▼              ▼
            │            ┌────────────────────────────────────┐
            └── remove ──│              UNLOADING             │
              (after     │  (LIFO unwind; committed view      │
               Inactive, │   stays readable to the end)       │
               drained)  └────────────┬───────────────────────┘
                                      │
   discharge fails anywhere (an owned inverse or a teardown
   obligation, §G.6): STAYS UNLOADING with TEARDOWN_VIOLATED
   latched — no exit; FAILED/Pending/Absent are unreachable
   until the violation is resolved (operator/revision)

   exit requires BOTH guard released (¬relied: no installed
   fiber resolves any key to this fiber) AND teardown verdict
   DISCHARGED (§G.6):
                                      │
           ┌──────────────────────────┼─────────────────────────┐
           ▼                          ▼                         ▼
   back to PENDING              Absent-track             FAILED — reached
   (target ≠ ⊥)                (retired) → ABSENT       only when this
   ──► ACTIVATING               on remove; revision     unload carried a
   (chained)                    of FAILED lands here    pending activation
                                too                     error: activation
                                                        failed AND the scene
                                                        cleaned (outcome
                                                        persists; no auto
                                                        retry, B19)
```

K0 observable lifecycle vocabulary (7): `Absent, Pending, Activating, Active, Unloading, Failed, (Disposed→Absent)`. Mapping to the paper's four (B13): Pending/Failed/Disposed are observational refinements of **Inactive**; Activating = **Reloading**; the core transition system is the paper's Fig 1 with K0 collapsing L-Iter/L-Finish into one bounded activation step (legal per B3/B20: a synchronous host takes whole-episode steps).

Corrective-2 note: there is **no eighth state** behind the repaired raise path. The pending activation error is episode metadata and the latched `TEARDOWN_VIOLATED` condition is a diagnostic flag (§G.6, §I.1 surface 2) — a raise first lands in Unloading, FAILED is the recorded outcome of a *fully discharged* unwind (§F.3), and a violated unwind simply never leaves Unloading. The diagram's `no children` removal guard is inherited from the paper's O-Remove and is **vacuous in K0**: child mounting is deferred (§S, Corrective-3), so no K0 fiber ever has children.

### F.3 Transition table

| From | Trigger | To | Effect/ownership behavior |
|---|---|---|---|
| Absent | reconcile mounts entry | Pending | O-Insert: entry created, empty table, τ=⊥ |
| Pending | all declared keys resolvable (target ≠ ⊥) | Activating | L-Begin: committed view ω frozen; run e |
| Activating | e completes; target still = ω | Active | effects owned; provisions installed; dependents may now commit |
| Activating | e raises | Unloading | pending activation error recorded as episode metadata; partial accumulator unwinds; nothing installed (Cor 69 — presupposing the unwind discharges; a failing inverse latches §G.6 in the row below). FAILED is **not** reached directly (Corrective-2) |
| Activating | target ≠ ω at completion (divert) | Unloading | unwind accumulated effects immediately; land-in-flight alternative is the only one in K0 (inertia, B20) |
| Active | target ≠ ω (provider withdrawal / retire / replacement) | Unloading | L-Leave: provisions leave σ_γ **first** (dependents invalidate against this), then unwind |
| Unloading | teardown verdict DISCHARGED (§G.6) **and** guard released (no relied-upon bindings); no pending activation error | Pending (target ≠ ⊥) or Absent-track (retired) | L-Unload: accumulator applied; table provably empty (Cor 69); committed view discarded last |
| Unloading | teardown verdict DISCHARGED (§G.6); pending activation error present (raise-derived unload; nothing installed, so no guard applies) | Failed | FAILED is **earned only by a fully discharged unwind** of a failed activation: activation failed *and* the failure scene is provably cleaned; outcome recorded; no auto retry (B19) |
| Unloading | any owned inverse or teardown obligation fails to discharge | Unloading (stays — no exit) | TEARDOWN_VIOLATED latches (§G.6); the episode never closes; FAILED/Pending/Absent-track are all unreachable; provider final-release guards stay latched |
| Failed | fresh desired incarnation (§L.5) | Absent-track → fresh fiber | retry = a new generation behind an explicitly revised desired entry, never in-place, never for an unchanged desired incarnation (B19, §L.5) |
| any | retire request (τ=⊤) | (flag only) | lifecycle rules carry it out; removal only from Inactive-family state with empty table and no children (the no-children guard is vacuous in K0 — child mounting deferred, §S) |

### F.4 Illegal transitions

```text
Pending/Failed  → Active          (no activation without passing Activating)
Active          → Activating      (must pass Unloading; no in-place reload)
Unloading       → Active          (guard must release; no resurrection mid-unload)
Failed          → Activating      (no silent retry against unchanged environment)
Activating      → Failed          (removed in Corrective-2: a raise first lands in
                                  Unloading, and FAILED requires a fully discharged
                                  unwind — §F.3; a violated unwind stays latched in
                                  Unloading and may not reach FAILED, §G.6)
any             → ABSENT with non-empty table or live dependents   (Thm 64 / Cor 69)
provider final release before all dependents finished             (B15 — kernel invariant)
Unloading → episode close while a teardown obligation is undischarged
                  (the episode may not close over a violated teardown contract;
                  §G.6 latches TEARDOWN_VIOLATED and withholds the transition)
```

### F.5 FAILED semantics (mandatory question answered)

**FAILED = a failed activation attempt's outcome, recorded on the fiber — and reached only through a fully discharged cleanup** (Corrective-2): a raise first lands the fiber in Unloading with a *pending activation error* kept as episode metadata; FAILED records the outcome only when that partial unwind discharges completely (§F.3). FAILED therefore certifies two things at once — the activation failed, *and* the failure scene has been safely collected. Neither a terminal product state nor a retryable kernel state:

- It is *not* terminal for the component: the desired composition still names it, so it stays a first-class entry that reconcile may **revise** (fresh generation).
- It is *not* auto-retried: L-Begin requires an error-free fiber (B19); an unchanged environment cannot silently relaunch it (this is what makes quiescence decidable).
- It does not propagate: siblings keep running (B19).
- A dependency that later becomes resolvable again does **not** clear FAILED by itself — and neither does reconcile running again: retry requires an explicitly fresh desired incarnation (§L.5); an unchanged desired revision identity can never retry a FAILED generation. (Confluence consequently excludes failed fibers — §M.3.)
- Domain "retry" policies (e.g., device retry inside AudioOutput) live **inside** the component behind an ACTIVE facade; they are invisible to lifecycle (A0 §G.2 bounded-retry degradation is intra-provider).
- FAILED (an activation outcome) is a **different class** from a teardown contract violation (§G.6): FAILED is a legal, recordable, revision-recoverable result of an episode that never installed **and whose unwind discharged cleanly**; a violation means an episode that could not prove it gave everything back — whether it had installed (ordinary Unloading) or not (unwinding a failed activation, §G.6's second failure site) — and it is a latched condition, not a lifecycle state, that no revision silently clears. A failed activation whose own unwind violates therefore **never reaches FAILED**: the fiber stays in Unloading with TEARDOWN_VIOLATED latched, and the pending activation error stays episode metadata (§G.6).

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
6. dependents finish teardown          each reaches a **discharged** episode close
                                       (empty table per Cor 69 + all teardown
                                       obligations discharged, §G.6); the guard on
                                       the provider (¬relied) releases fiber by
                                       fiber (Thm 73) — and stays latched against
                                       any dependent that cannot discharge (§G.6)
7. provider final release              only now the provider's L-Unload applies its own
                                       accumulator; entry removable
```

### G.2 Precise answers (required question set)

| Question | K0 answer |
|---|---|
| What remains reachable during teardown? | the withdrawing provider's capability bindings **as values** — readable through dependents' committed views until each dependent's own unload completes. Nothing else changes: no *new* resolution sees the provider (step 2). |
| Who may access it? | exactly the fibers whose committed views bind the provider (`relied` set); they are by construction all in Unloading/Pending themselves |
| Can new calls occur? | new *kernel* commitments: no. New *service calls on an existing handle*: the service contract decides — K0 requires contracts to define teardown-time behavior; the Decoder contract keeps `close` valid (A0 §G.1), other calls may fail-closed. This is service-contract semantics, not kernel policy. |
| How are existing service handles treated? | handles are domain resources owned by the consumer (Music owns its open Decoder handle; §J.4 ownership universes). The kernel never revokes them and never tracks them as kernel Effects; it guarantees the **window** in which the consumer must discharge its domain teardown obligations, and the consumer's episode may not close until discharge is complete (§G.6). |
| What prevents use-after-provider-destroy? | the guard: the provider's final release physically cannot run while any installed fiber resolves a key to it (L-Unload premise); plus removal requires an empty table and no children (Thm 64). A dependent whose teardown violated its contract keeps its committed view open, keeps `relied` true, and keeps this guard latched (§G.6) — the guarantee survives disposer failure instead of assuming it away. |
| What prevents a withdrawing provider from accepting new dependents? | σ_γ is the union over **ACTIVE** fibers only; an Unloading provider is invisible to target-view computation (B15). |
| What if dependent teardown fails (disposer errors)? | **not an ordinary lifecycle result.** A legal K0 inverse is a total semantic obligation (§H.7): failure to discharge it is a teardown contract violation. The dependent's episode does **not** close; a `TEARDOWN_VIOLATED` condition latches (composition truth); every provider whose safety depends on the undischarged obligations keeps its final-release guard **latched**; the run forfeits quiescence, confluence, and independent-removal claims; the violation is surfaced explicitly. Recovery is operator/revision business — "best-effort cleanup, then destroy the provider anyway" is rejected as a correctness model (§G.6). [QIANQIAN] on [PAPER]'s total-function setting |
| What if provider destruction fails? | same class (§G.6): the provider's episode does not close; its own providers' guards latch if it still relies on them; its (already drained) dependents are unaffected. Root disposal reports the violation and does not claim completion. |
| Chain A ← B ← C (C provides to B provides to A)? | invalidation cascades transitively (step 3); unwind orders *against* the dependency edges (A out, then B, then C); termination follows Thm 73 under the acyclic check of §L.4 — **provided every disposer discharges**; a violated teardown latches §G.6 and deliberately withholds further progress on that edge. K0 enforces acyclicity at reconcile, so a clean cascade always drains. |

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
      a. decode worker quiesce/join                                [Music composition effect]
      b. song_close(open handle) — still valid, D1 not yet released
                                                                   [Music domain teardown
                                                                    obligation, §J.4 — must
                                                                    discharge before episode
                                                                    close, §G.6]
      c. SinkSession binding quiesce/teardown (commit/flush)        [Music composition effect]
      d. remaining owned effects LIFO
t5  Music reaches a discharged episode close → Pending (deps currently unsatisfied)
t6  guard on D1 released (no committed view names D1) → D1 L-Unload:
      FFmpeg closure resources released — use-after-unload impossible by construction
t7  D1 entry removed (O-Remove) — **only now** D2 mounts (staged replacement, §E.4):
      D2 → ACTIVE → `Decoder` in σ_γ → Music target ≠ ⊥ → Music activates:
      fresh episode, commits ω(D2); binds PcmSink at ACTIVATION (A0 frozen rule);
      domain recovery (reopen same source, park-at-CONFIRMED policy) is Music's
      §H.b business, invisible here
```

At every step t0–t7 exactly one installed fiber declares provision for `Decoder` (D1 until t7's insert replaces the removed entry, D2 after) — the E.4 invariant holds pointwise, not only at quiescence.

### G.4 Withdrawal trace — AudioOutput / device switch

```text
t1  AudioOutput X withdrawal begins (planned switch X→Y or device-loss
    policy): X retires → X L-Leave: `PcmSink` leaves σ_γ → Music invalidated
t2  Music Unloading with teardown access:
      park pipeline at last CONFIRMED landing (domain) → quiesce RT edge via
      commit/flush handshake → tear down SinkSession binding (Music's effect) →
      device session released by AudioOutput only afterwards (A0 §G.2 order:
      renderer destroyed before engine — same invariant, derived not copied)
t3  X completes a discharged unload (guard ¬relied released once Music's
    episode closed) → X removed (O-Remove) — staged replacement, §E.4:
    the running registry never holds X and Y together
t4  only now Y mounts (O-Insert) → Y activates → `PcmSink` re-enters σ_γ
t5  Music re-binds by presenting its fill endpoint; format renegotiated at
    bind (SRC parameters may change)
t6  Music resumes per policy; if no replacement exists, Y never appears and
    Music stays Pending — the honest degraded state (A0 §K.5)
```

No special case hides here: t3/t4 are §E.4's steps 5–7 (old discharged unload → O-Remove → O-Insert) instantiated verbatim. G.4 exists to show the dependent-side domain choreography inside the teardown window (t2), not a different replacement mechanism; if the old-remove → new-mount steps are ever dropped from a trace like this, some reader will eventually mistake G.4 for an allowed coexistence special case (Corrective-2 restores them).

### G.5 Non-negotiables

No music-specific step, device policy, or track semantics may enter this protocol — G.3/G.4 are *instances*; the mechanism (steps 1–7) is fully generic. The two traces differ only in what the dependent's owned effects are.

### G.6 Teardown contract violations (frozen in Corrective-1)

The paper works in a total-function setting: every inverse, once invoked, completes (B16). K0 keeps that setting **as a contract on components**, not as an assumption about defective implementations:

```text
FROZEN — the infallible-inverse contract:
A legal K0 Effect inverse is a total semantic obligation. Once the owning
fiber's teardown (or explicit dispose) invokes it, the inverse must complete
its semantic teardown obligation. The inverse has no failure outcome in the
kernel's semantic model.

An implementation in which an inverse fails to discharge its obligation has
violated the component contract. This is NOT an ordinary recoverable
lifecycle result:

    ordinary activation failure   ≠   teardown invariant violation
    (B19: FAILED outcome,              (contract violated; see below)
     revision-owned retry)
```

An activation failure is a legal outcome a component may produce (§F.5): the environment was unsatisfiable, the component says no, the accumulator unwinds, nothing installs. A teardown violation is different in kind: the component already said yes, acquired state, and now cannot prove it gave the state back. Proceeding as if cleanup succeeded is exactly the use-after-provider-destroy the withdrawal window exists to prevent.

**What "discharge" means to the kernel (frozen in Corrective-2):** exactly one verdict per fiber teardown — `DISCHARGED` or `CONTRACT_VIOLATED`. "Declared domain teardown obligations" (§H.5.1/§J.4) is component-contract vocabulary, not kernel data: the kernel holds no obligation registry, no obligation list, count, or identity — whether a component closed one handle, stopped three threads, or cleared twenty domain objects is component-private (§J.3). A domain obligation is therefore not a sixth primitive under another name; the entire kernel-visible fact is the verdict.

**K0 semantics on violation:**

```text
teardown contract violation (any disposer fails to discharge)
        ↓
the fiber's episode does NOT close        (F.4: no close over undischarged
                                           obligations; committed view stays open)
        ↓
TEARDOWN_VIOLATED latches                 (composition-truth condition on the
                                           fiber — a diagnostic flag, NOT a new
                                           lifecycle state and NOT a sixth primitive;
                                           final name is an implementation-issue choice)
        ↓
providers whose safety depends on the undischarged obligations
MUST NOT final-release                    (their ¬relied guard stays latched: the
                                           open committed view keeps `relied` true —
                                           B14's window mechanism reused verbatim)
        ↓
the violation is surfaced explicitly      (composition-truth diagnostic; the kernel
                                           knows "teardown contract violated", never
                                           the domain why — §J.3)
```

**What a latched run may no longer claim:**

```text
clean quiescence               the quiet predicate requires no open episode and
                               no latched violation (§L.1)
successful independent removal any removal claim involving the violated fiber is void
composition confluence         §M excludes violation histories — Thm 80's
                               total-function precondition is what failed
safe provider final release    the guard stays latched; the provider stays alive
```

**What is deliberately traded:** progress. Thm 73's deadlock-freedom presupposes inverses complete; by violating that hypothesis the defective component forfeits automated progress on its dependency edges. K0 accepts this: liveness is never bought with a use-after-provider-destroy hazard. The kernel does not "wedge silently" — the violation is loud, diagnosable composition truth — but it also does not pretend the episode closed.

**Recovery** is not automatic and not fabricated:

```text
no invisible retry of the failed disposer          (same discipline as B19)
no automatic provider destruction past the latch   (rejected correctness model:
                                                    "best-effort cleanup, then
                                                    destroy anyway")
operator/revision options — full process restart, or a future explicit policy —
are [OPEN] at the implementation issue; the frozen part is: latch + surface +
block final release + forfeit quiescence/confluence claims
```

The violation class covers **both** failure sites: a disposer failing during ordinary Unloading, and a disposer failing while the partial accumulator of a failed activation unwinds (Cor 69's "installs nothing" also presupposes its inverses complete — §G.6 latches identically there). In the second site the fiber never reaches FAILED: FAILED requires a fully discharged unwind (§F.3/F.5, Corrective-2), and a violated one has no such discharge — the fiber stays in Unloading with the latch, and the pending activation error stays episode metadata.

**Scenario A — failed consumer teardown (required adversarial case):**

```text
Decoder D1 ACTIVE; Music ACTIVE, requires D1;
Music owns a domain Decoder handle (a track was opened — §J.4 domain universe)

D1 withdraws (retire or replacement)
  → D1 leaves σ_γ; Music invalidated; Music enters Unloading
  → Music runs its teardown inside the window (§G.3 t4):
      song_close(handle) FAILS
  → Music's teardown obligations are undischarged
  → Music's episode does NOT close; TEARDOWN_VIOLATED latches on Music
```

Answers required by review:

- **Can D1 final-release?** No. Music's committed view still names D1 (the episode never closed), so `relied` stays true and D1's L-Unload guard stays latched. The remaining domain resource can never reference dead provider state through this edge.
- **Can the system claim quiescence?** No. Music sits in an open Unloading episode with a latched violation — the quiet predicate (§L.1) is false, and stays false until an operator/revision resolution (e.g., process restart) clears the fiber.
- **What state is observable?** Composition truth: Music Unloading + `TEARDOWN_VIOLATED`; D1 still installed but non-providing (σ_γ excludes it — no new consumer can bind); siblings unaffected and still runnable; diagnostic surface shows the violation without domain detail (§J.3).
- **What must operator/revision do?** Treat it as a component-contract defect, not a runtime glitch: capture the diagnostic, restart or otherwise externally resolve the run, fix the defective disposer. The kernel offers no automatic "continue anyway" path — that path is frozen out.
- **Which theorem assumptions no longer apply?** Cor 69/Thm 64 (empty table, clean removal — the conclusion is withheld), Thm 73 (progress/deadlock-freedom — presupposed total inverses), Thm 80 (confluence — §M excludes the history), and the §6.1-style treatment of teardown as complete. The paper never models disposer failure (total-function setting), so none of its theorems licenses proceeding past a violated inverse; §G.6 is the [QIANQIAN] semantics for that case.

---

## H. Effect / Independence / Commutativity Model

### H.1 Local revertibility (intra-fiber)

One fiber's owned effects unwind strictly LIFO within its episode (B1–B3, B26). K0 obligations:

- every kernel-visible mutation performed by a fiber is registered as an owned effect at the moment of mutation;
- the inverse must actually revert at the state of application — an **author obligation** the runtime does not verify (paper §5.1.1) and review/tests must; it is additionally a **total semantic obligation**: failure to discharge is a teardown contract violation, not a lifecycle result (§H.7, §G.6);
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

### H.5 System-boundary action taxonomy (frozen in Corrective-2: NOT a Kernel Effect enum)

Corrective-2 freezes the *role* of the five A0 classes: they are a **descriptive system-boundary / action taxonomy** for reasoning about actions — component-internal actions, cross-boundary acquisitions and emissions (B23) — and for deciding *which actions may enter the kernel effect machinery*. They are **not** variants of a kernel `Effect` type, and the kernel never stores them per effect.

**K0 Effect has exactly one shape (frozen):**

```text
K0 Effect = composition-lifecycle reversible mutation + total inverse (§H.7)

consequences (frozen in Corrective-2):
- no EffectClass enum exists in the kernel — no Reversible/Transactional/
  Compensatable/Irreversible variants of Effect, and no `disposer: Option<…>`
  fields: a registered inverse is total, not optional
- an action that is not a reversible-with-total-inverse composition-lifecycle
  mutation is simply NOT registered as a K0 Effect: it either stays inside
  the component (domain universe, §H.5.1) or crosses the system boundary
  (described by the taxonomy rows below; never tracked by the kernel)
```

**Scope (frozen in Corrective-1, restated): only the Reversible row's composition-owned instances enter the kernel effect machinery** — kernel-visible mutations whose existence a fresh construction of the desired composition exhibits (§J.4 ownership universes). Domain-session mechanisms are a separate universe and live in §H.5.1; they never enter this table as kernel Effects and never enter the composition-confluence oracle.

| Taxonomy row | Definition | MVP composition-owned instances | Paper grounding |
|---|---|---|---|
| Reversible | tracked action with a true inverse — the **only** row that can ever produce K0 Effects | capability provision, commutative listener registration, SinkSession bind/unbind (frozen bind-at-activation rule), Music's activation-owned playback mechanisms (decode worker, ring — present in a fresh construction per A0 §B.1/§H.a). Child mount is **not** an instance: child mounting is out of K0 scope, [PAPER] context only (§D.3, §S — Corrective-3) | B4, §3.1 |
| Transactional | all-or-nothing with fail-closed outcome | none — **can never be a K0 Effect** (Corrective-2); K0's atomic-activation behavior (B19) is lifecycle semantics, not an effect class | consistent with B19's atomic activation |
| Compensatable | application-supplied coarser recovery | none — **can never be a K0 Effect**; compensation lives in domain/policy layers (§H.5.1) | B23 compensation (commutation must be re-proved against the coarser relation) |
| Irreversible (protocol point-of-no-return) | rollback/cancellation authority ends; outcome must be awaited | none — **can never be a K0 Effect** (CLAIMED flush is the RT island's protocol, §H.5.1) | B23 acquisition/emission split |
| Outside recoverable boundary | crosses into the external world | none — **can never be a K0 Effect** | B23 emission |

An Effect may wrap an *acquisition* (tracked); it must never claim to roll back an *emission*. `Everything is a Plugin` ≠ `Everything is rollbackable`.

### H.5.1 Domain-session mechanisms are NOT K0 Effects (frozen: Option A)

The taxonomy above is also used **descriptively** at the domain layer (A0 §F froze domain-mechanism classifications) — labels for reasoning and review, never kernel runtime categories — but domain-session resources are owned by domain/session semantics, not by the composition kernel. Corrective-1 freezes the ownership-universes split and, with it, the design question the review surfaced about the paper's episode-oriented effect model:

```text
DECISION (frozen) — Option A: the K0 Effect is composition-lifecycle only.

Domain resources use domain ownership mechanisms outside the generic
composition-confluence oracle. K0 does NOT track domain-time resources
through the Effect machinery. (Option B — fibers registering ACTIVE-time
runtime effects into the kernel accumulator — is deferred with an explicit
trigger; it would require ACTIVE-time registration rules, episode/generation
attachment, confluence-exclusion rules for domain-triggered effects, and
provider-withdrawal unwinding of runtime effects, and nothing in #53 or the
MVP requires paying that cost.)
```

| Domain-session mechanism | Descriptive class | Owner | Kernel-visible? |
|---|---|---|---|
| open Decoder handle after track open (open/close) | reversible at the domain layer, discharged inside the teardown-access window | Music's domain session (A0 §F) | existence/value: **no** (domain truth, §J.2); discharge completed or violated: **yes, as episode close vs §G.6 latch** |
| seek transaction (CONFIRMED landing or ERROR, never half-seek); track open (READY@0 or ERROR) | transactional | Music's domain session | no |
| stop-recovery reopen; device-switch park-at-CONFIRMED | compensatable | Music / provider policy | no |
| CLAIMED flush (commit/flush handshake, I3/I4/I5) | **irreversible protocol point-of-no-return** — from CLAIMED, cancellation/rollback authority ends and control must await the outcome (I4) | RT island mechanism, *driven by* composition teardown ordering (§K.2/§G.4) | handshake state: no; binding teardown state: yes (§K.3) |
| physically rendered audio | **outside recoverable boundary — the external emission boundary** | the physical world | no |

**CLAIMED ≠ emission (frozen, A0 corrective P1-3, restated because review found a regression):**

```text
CLAIMED flush  = the protocol point-of-no-return: cancellation/rollback
                 authority ends; the outcome must be awaited (I4);
                 protocol-level irreversibility
physical render = the external emission boundary: sound crossed the
                 recoverable system boundary; concrete external emission
```

A claimed flush is classified Irreversible **because rollback authority ends at CLAIMED** (and a flush may irreversibly discard device-buffer state) — it is *not* classified as, and must never be equated with, the external emission itself. A silent/null backend can pass CLAIMED with nothing audible emitted.

**Scenario D — CLAIMED without emission (required adversarial case):** a null/silent output backend performs a graph-swap flush. The handshake reaches CLAIMED: from this point the swap can no longer be cancelled, and the pipeline must await COMPLETED (I4). No sample is ever rendered; the external emission boundary is never crossed; there is nothing to compensate for in the world. Both facts hold simultaneously and neither subsumes the other — the point-of-no-return governs protocol authority, the emission boundary governs external-world effects. Any wording equating CLAIMED with emission is a defect (it was exactly the regression this corrective removed from §B23/§H.5).

The bridge between the universes is **obligation discharge, not effect tracking**: a fiber's teardown procedure must discharge its declared domain obligations (e.g., close the Decoder handle) inside the §G window; the kernel observes only *completion or violation* of that discharge (§G.6), never the resources themselves (§J.3).

**Corrective-2 fence — a domain obligation is not a sixth kernel primitive and not kernel data.** The kernel owns no `ObligationRegistry`, no `Vec<DomainObligation>`, no obligation count or identity; "declared" is a component-contract statement, not a kernel record. The complete kernel-visible fact is one teardown verdict per fiber: `DISCHARGED` or `CONTRACT_VIOLATED` (§G.6). Renaming a domain Effect to a domain Obligation and moving it back into the kernel is exactly the smuggling this fence forbids.

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

### H.7 The inverse is a total semantic obligation (frozen in Corrective-1)

H.1's "the inverse must actually revert at the state of application" is an *author obligation* — and in K0 it is also a **totality contract**, not a best-effort hope:

```text
registering an Effect asserts: its inverse, once invoked, can and will
discharge the effect's semantic teardown obligation.

A legal K0 Effect has no failure outcome. If an implementation's disposer
fails to discharge, the component contract is violated and §G.6 applies:

    episode does not close → TEARDOWN_VIOLATED latches → provider guards
    whose safety depends on the obligation stay latched → the run forfeits
    quiescence / independent-removal / confluence claims → surfaced loudly.
```

Consequences for effect authors and the kernel:

- **Effects must wrap only obligations whose discharge is actually achievable inside the episode.** An effect whose cleanup can legitimately fail (e.g., a device close with a wedged backend) must be modeled so that the *kernel-visible* inverse still completes — recording the domain-level failure in domain truth — or the author must accept that its failure is a contract violation with §G.6 consequences. Effects are not a place to hide uncertain cleanup.
- **One shape, no variants (Corrective-2)**: a K0 Effect is a reversible composition-lifecycle mutation carrying this total inverse — the kernel has no effect-class enum and no optional disposer (§H.5 frozen block); the five classes are descriptive taxonomy for actions, never runtime variants of Effect.
- **No fabricated rollback**: a disposer must never *report* success for an obligation it did not discharge (§O.2). A partial discharge is a violation, not a success with notes.
- **The kernel does not retry disposers** and does not offer "continue past violation" — the rejected correctness model is `best-effort cleanup → guard releases → provider may die`.
- The paper grounding stays honest: B1–B3/B16's guarantees (LIFO recovery, empty-table removal) are theorems *in the total-function setting*. §G.6/§H.7 do not extend those theorems to failing inverses — they define the Qianqian behavior when the setting is violated, and name exactly which guarantees are forfeited.

---

## I. Observational Equivalence

### I.1 The surfaces

Comparison of restored/settled state happens at exactly these surfaces (closed set — anything else is private):

1. **capability reachability**: for each capability — absent, or provided by fiber X (provider identity compared *up to generation-renaming*, B18/Lemma 61);
2. **fiber lifecycle truth**: each desired fiber's observable state (§F.2 vocabulary), activation-failure outcome present/absent, and its teardown-contract-violation flag (§G.6) — Corrective-2 folds the flag into this surface; it is not a seventh diagnostic concept;
3. **committed bindings**: which consumer binds which provider (semantic identity, not uid values);
4. **contribution sets**: per commutative key, the set of live contributions compared by semantic identity (listener *kinds*), never token values; dispatch count == registered count where observable;
5. **composition-owned data-edge bindings**: SinkSession/device-session existence per live Music↔PcmSink binding (idle ≠ absent — A0 frozen activation rule); read as a projection of the binding effects' structural provenance, per the §K.4 single-authority rule;
6. **ghost absence**: no composition-owned bindings, provisions, contributions, sessions, or effects beyond the desired set. Scope (frozen in Corrective-1): the ghost oracle checks the composition-owned universe only (§J.4/§H.5) — a legitimately open Decoder handle, track session, or other domain-session resource that exists because a user/domain action occurred is **never a ghost effect**, precisely because the clean baseline is defined as *no domain session* (A0 §H.b): comparing composition truth against it is well-formed only if domain resources are outside the comparison (§J.1, §M.2).

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

is an **architecture violation on sight** — redesign (that data lives behind `PlaybackControl`/`PlaybackSnapshot`/provider contracts; the kernel may know "Music failed activation", never "Music failed because the file was bad"). The same firewall applies to teardown: the kernel may know "Music's teardown contract was violated" (§G.6), never "song_close failed on file X".

### J.4 Ownership universes (frozen in Corrective-1)

There are exactly two ownership universes, and a fact belongs to exactly one:

```text
COMPOSITION-OWNED LIFECYCLE EFFECTS
    capability provision
    commutative listener registration (when part of plugin activation)
    SinkSession bind created by the frozen Music-activation rule
    fiber-owned composition bindings/data edges
    activation-owned component mechanisms a fresh build exhibits (worker, ring)

    → participate in: Composition Kernel effect calculus (§H),
      composition confluence (§M), ghost-effect oracle (§I.1.6)

DOMAIN / RUNTIME RESOURCES
    open Decoder handle after a user opens a track
    track session
    playback transaction resources (seek/flush transaction state)
    checkpoint / recovery state

    → owned and torn down under domain/session semantics (A0 §F);
      they do NOT automatically become Composition Kernel Effects,
      composition-confluence state, or ghosts (§I.1.6, §H.5.1, Option A)
```

The bridge between universes is **obligation discharge**: when a fiber tears down, its episode may close only after its composition effects are unwound **and** its declared domain teardown obligations are discharged (§G.6). The kernel sees exactly **one verdict per fiber teardown — `DISCHARGED` or `CONTRACT_VIOLATED`** (§G.6); it never sees the resources, their count, or their identity (§J.3, §H.5.1 fence). This keeps the Decoder handle safely torn down *inside* the provider-withdrawal window without making the handle a kernel fact.

**Scenario C — user opens a track (required adversarial case):**

```text
fresh composition: Music + Decoder + AudioOutput; no track (clean baseline)
user opens track A → a Decoder handle appears inside Music's domain session
```

The composition graph is unchanged: same fibers, same capabilities, same committed views, same SinkSession binding (which existed pre-open per the activation rule). Composition truth before == composition truth after; the handle is domain truth (§J.2). Therefore:

- **composition confluence does not fail**: §M compares the settled registry against `Fresh(D_H)` *constructed with no domain session* at the §I.1 surfaces — none of which contain the handle. Domain-resource existence is structurally outside the comparison (§J.1 classifier, §M.2).
- **the handle is not a ghost**: §I.1.6 scopes ghost absence to the composition-owned universe; a domain-session resource present because a domain action legitimately occurred is the classifier working as designed, not a leak. The ghost oracle would fire only if a composition-owned artifact (a stale binding, a second SinkSession, a provision from a dead generation) outlived its justification.
- Symmetrically, a *composition* mutation (e.g., decoder replacement) does not have to preserve or even observe the handle's *value* — it only has to preserve the window in which Music discharges it (§G), and Music's domain continuity is §H.b policy business, not confluence.

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

### K.4 Single authority for composition-visible bindings (frozen in Corrective-4)

§K.3's facts (binding exists; owner fiber; provider fiber; teardown state)
must have exactly one mutable authority. Frozen:

> **Every composition-visible binding has exactly one authoritative
> ownership/provenance record, and its lifetime is coupled to exactly one
> owner Fiber episode. Diagnostics are projections of that authority, not a
> second mutable registry.**

For the MVP data edge that authority is the **binding Effect owned by the
consumer fiber** (§K.1): its structural provenance (§D.4) names the relation
(capability key — `PcmSink`), the owner (Music's fiber episode), and the
provider fiber identity (AudioOutput). What the kernel can prove is derived
by reading that effect together with lifecycle truth:

```text
binding exists      ← the binding effect is live inside its owner's episode
owner / provider    ← the effect's structural provenance
relation remains?   ← effect live AND provider episode still consistent
ghost session?      ← live binding effect outside any justified episode
                      (§I.1.6 ghost oracle)
```

What the kernel still never learns: PCM, buffer addresses, sample values,
format payload, device retry policy, track identity (§K.3, §J.3). Frozen
anti-patterns, both directions:

```text
no EffectKind::SinkSession-style behavioral enum   (§H.5: one Effect shape,
                                                    no domain variants)
no standalone mutable DataEdgeRegistry             (a second authority and a
                                                    sixth-primitive risk, §D.6)
```

A derived, read-only projection index (for diagnostics/tests) is a
representation choice (§T), never a second truth.

---

## L. Reconcile Semantics

### L.1 Model

```text
Desired graph D    abstract entry tree: {id → component, configuration
                   semantics, enabled-state, revision identity (§L.5)} —
                   in-memory only; no YAML/files/config language (#67 G).
                   The revision identity is opaque equality-domain data:
                   its only semantic content is "same desired incarnation"
                   vs "fresh desired incarnation".
Running graph R    the fiber registry + lifecycle states
Difference         per-entry: absent | extra | same desired incarnation
                   (no revision — dependency-driven lifecycle only) |
                   fresh desired incarnation (revision: staged
                   retire/remove → mount a fresh generation, §E.4/§L.5)
Transition plan    a sequence of orchestration requests: mount / retire / revise
Settlement         quiescence — frozen in transition semantics (Corrective-4),
                   not in target-view equality alone (predicate below)
```

```text
FROZEN (Corrective-4) — the quiescence predicate:

A running composition is quiescent iff:

1. no lifecycle/orchestration transition is currently enabled or in
   flight (no pending orchestration request; no fiber mid-transition);
2. every non-failed fiber has reached the stable state implied by its
   current desired/target condition — ACTIVE where satisfied, PENDING
   where a required provider is absent or disabled, ABSENT where
   unmounted;
3. FAILED activation outcomes are settled states: FAILED inhibits
   automatic L-Begin/retry (B19), so a FAILED fiber — even with all
   dependencies satisfied, even with a non-⊥ target view — is
   quiet-legal and remains FAILED until an explicit fresh desired
   incarnation (§L.5);
4. no TEARDOWN_VIOLATED condition is latched and no teardown episode is
   open (§G.6) — a latched run can never satisfy the predicate;
5. Reconcile holds no outstanding staged orchestration step — a staged
   replacement is NOT quiescent between old-removal and the still-owed
   new-mount (§E.4).

Frozen distinctions: quiet ≠ healthy; quiet ≠ successful; quiet ≠
theorem-backed confluent (§M.1 conditions; failure histories excluded,
§M.3).

Examples (each an executable future oracle):

  Pending because a required provider is absent         → may be quiet
  FAILED waiting for an explicit fresh incarnation      → may be quiet
  Unloading                                             → not quiet
  Activating                                            → not quiet
  TEARDOWN_VIOLATED latched / open teardown episode     → not quiet
  staged replacement between old-remove and new-mount   → not quiet while
                                                          the mount is owed
```

A naive `quiet = committed_view == target_view` reading of B13 is
explicitly rejected: it would hang settlement forever on a legal,
quiet-visible FAILED fiber (clause 3) and would call a half-drained
staged replacement quiet (clause 5).

Reconcile is the **only** issuer of orchestration requests. It never sets lifecycle state directly; it never touches effects; it never wires data edges.

### L.2 Required operations

| Operation | Semantics |
|---|---|
| mount(entry) | insert fiber (Pending); lifecycle rules do the rest (B24's loader argument: no load order needed — providers first is *emergent*, not arranged) |
| unmount(id) | retire → wait for quiesced deactivation → remove (entry may re-mount later) |
| replace provider | **staged replacement (§E.4)**: retire(old) → dependents invalidate + teardown → old completes a discharged unload → old removed → *then* mount(new) → new ACTIVE → dependents reactivate. The mount(new) request is issued only after the removal completes (serialized control plane); the running registry never holds two providers of one capability at any step. Equal-value replacements still reactivate consumers (B30) |
| missing provider | not an error: dependants sit Pending; root never crashes (A0 §K.5) |
| ambiguous provider | **composition error**: desired graph itself is illegal (two enabled providers of one required-single capability) — reported, plan refused, no silent pick (E.2) |
| dependency cycle | **composition error**: detected from declarations alone (B24); refused at plan time; the runtime never sits on an undetactable deadlock |
| activation failure | raise records a pending activation error and routes the fiber into Unloading; FAILED lands only on a fully discharged unwind (§F.3); reconcile leaves it visible; no invisible retry (B19); a violating unwind instead latches §G.6 (row below) |
| teardown contract violation | **not an activation outcome**: the fiber's episode stays open, TEARDOWN_VIOLATED latches, dependent provider final-release guards stay latched (§G.6); reconcile surfaces it and issues no further requests through the affected edge; recovery is operator/revision business |
| revision — triggered only by a fresh desired incarnation (config change / retry / re-enable, §L.5) | retire → deactivate → remove → re-mount as a fresh fiber/generation (paper §4.4 Configuration composite); dependents follow unprompted; an unchanged desired incarnation never reaches this row |
| root disposal | retire all; dependents before providers emerge from the guard ordering; quiescence = empty registry; teardown contract violations latch §G.6 — root disposal reports them and does not claim completion while a violation is open |

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

### L.5 Desired revision identity (frozen in Corrective-4)

**Problem this closes.** The frozen semantics make retry a *visible
revision* (retire → deactivate → remove → mount a fresh fiber/generation)
and forbid invisible auto-retry (B19). But a desired graph carrying only
{component, configuration, enabled-state} cannot *express* "retry" when
nothing else changed: a FAILED `Decoder` whose desired entry is otherwise
unchanged produces an empty diff, so Reconcile cannot distinguish "keep
FAILED" from "the operator explicitly asked for a fresh generation". Left
unfrozen, an implementer must invent a trigger — imperative `revise(id)`,
revision counters, config hashing, or (forbidden) automatic retry.

**Frozen concept — desired revision identity.** Every desired entry
conceptually includes an opaque **revision identity** (alias: *desired
incarnation intent*). Its entire semantic content is an equality domain:

```text
same revision identity as the running generation  → same incarnation; no revision
different revision identity                       → fresh incarnation requested;
                                                    revision = staged retire/remove
                                                    (§E.4), then mount a fresh
                                                    generation
```

Invariants (frozen):

```text
R1  FAILED does not mutate the desired revision identity — automatically,
    ever. Failure settles the generation; it never rewrites the profile.
R2  dependency appearance/disappearance never creates or changes a desired
    revision identity (provider flapping cannot fabricate revisions).
R3  an explicit retry / re-enable / configuration revision requests a
    fresh incarnation by presenting a different revision identity.
    Authoring discipline: any semantic edit to an entry that must reach
    the running fiber presents a fresh identity; K0 semantics define no
    content-diff revision trigger.
R4  a fresh desired incarnation is history-visible: the revision appears
    as explicit orchestration (retire/remove/mount) in the reconcile
    history — never as an invisible in-place mutation.
R5  the revision identity itself is opaque and carries no application
    meaning: the kernel compares it, never interprets it (no timestamps,
    no "version N means X", no configuration semantics inside the token).
R6  private numeric identity is NOT part of observational equivalence
    (§I.2); confluence never compares generation counters or token values.
R7  the same desired incarnation must not remount merely because
    reconcile runs again (reconcile is idempotent over an unchanged
    desired graph).
R8  a fresh desired incarnation must not reuse the FAILED fiber's
    episode: after the visible staged retire/remove, it mounts a fresh
    fiber/generation (§E.4 sequence; the FAILED generation is removed,
    not resurrected).
```

The revision identity is **supplied with the desired composition** (by the
operator/profile layer). The kernel never derives it — not from component
identity, configuration content, enabled-state, dependency state, prior
failures, or the number of reconcile runs. A pure function of configuration
content is exactly the representation that made fresh-generation intent
unexpressible in the first place. (See also §S: content-hash revision
triggers are rejected.)

This is **not a sixth primitive**: the revision identity is a field of
Reconcile's input datum (the desired entry), in exactly the sense §D.6
already classifies Profile as Reconcile's input rather than a primitive.

**Representation stays open (§T.11).** Monotonic per-entry counters,
operator-supplied epoch tokens, or fresh UUIDs per edit are all admissible;
the frozen contract above is independent of which one an implementation
picks. No Rust representation is frozen here.

**Implementation oracle — revision identity (D0–D4).** Frozen as a required
future executable test (step labels D0–D4 are local to this trace — they
name oracle steps, not fibers; generations in the trace are G1/G2):

```text
D0  desired: Decoder@R1 enabled
    → mount G1 → activation fails → raise → Unloading (partial unwind)
    → fully discharged → G1 FAILED, outcome visible (§F.3/F.5)

D1  reconcile runs again with the unchanged Decoder@R1 entry
    → diff = same desired incarnation → no orchestration request
    → G1 remains FAILED; no new activation episode
    → the quiescence predicate (§L.1) is TRUE with G1 FAILED visible
      (the settled failure stays visible; silent retry is a defect)

D2  a capability unrelated to Decoder disappears and reappears
    → G1 remains FAILED; no desired revision identity changed anywhere (R2)

D3  the operator presents Decoder@R2 (fresh incarnation; component and
    configuration otherwise identical)
    → diff = fresh desired incarnation → revision:
      G1 retired → discharged unload → removed (staged, §E.4)
      → fresh G2 mounts → G2 may activate

D4  reconcile runs repeatedly with Decoder@R2 unchanged
    → no further requests; no G3/G4 churn (R7)
```

D1 doubles as the **FAILED-settlement oracle**: an implementation whose
settle/quiescence check hangs on — or silently retries — a FAILED fiber
with a satisfied target view violates §L.1 clause 3.

---

## M. Composition Confluence Oracle

### M.1 The property (K0 statement)

> For any legal orchestration history H (mounts/retires/revisions only, no activation failures, no teardown contract violations) reaching quiescence, the **composition truth** of the settled registry is observationally equivalent — at the §I.1 surfaces, up to fiber-name renaming and vestigial-entry invisibility — to `Fresh(D_H)`, a clean dependency-ordered load of the final desired composition D_H, **constructed with no domain session** (A0 §H.b clean-baseline rule).

This is [PAPER] Thm 80 (with Lemma 61/62 readings) narrowed to Qianqian's composition-truth classifier (A0 §H.a). Conditions: L.4's checks (acyclicity, totality, finiteness, boundedness) hold.

### M.2 Why the oracle excludes domain truth

`current source`, `PlaybackState`, open `Decoder` handle, `position` are functions of *domain session history* (which track was opened, whether play was pressed). A fresh build of D_H has no such history — nothing in the desired composition determines it. Demanding their equality would make confluence false by definition; A0's §H.b handles them under explicit checkpoint/apply or behavioral-probe policies. The kernel structurally cannot leak them: its diagnostic surface (§I.1) does not contain them (§J.3).

### M.3 Failure exclusion

Histories containing activation failures **or teardown contract violations** are **excluded from confluence claims** (B19: schedule-dependent failure breaks endpoint equality; §G.6: a violated inverse exits the total-function setting Thm 80 presumes — the empty-table and guard-release conclusions of Cor 69/Thm 73 are exactly what failed). Activation failures get their own assertions via §O: no ghost effects, FAILED visible, siblings unaffected. Teardown violations get theirs: episode open, violation latched, provider final release blocked, quiescence forfeited (§G.6) — the assertions are about the *honest poisoned state*, never about proceeding as if clean. A retry decision is a revision — itself just another history step; there is no revision that silently clears a latched §G.6 violation (that resolution is operator/revision business, visible in history).

```text
FROZEN (Corrective-3) — failure histories never widen Thm 80:
a history that contains an activation failure stays outside the confluence
claim even if a later revision succeeds — success does not wash the failure
out of the history. Fail-then-revise is judged by §M.4's failure/recovery
sanitation oracle. A theorem-backed confluence statement about the recovered
state may only be made about the suffix history H′ whose initial state is
taken after the failed generation has fully discharged and been removed by
the visible revision; H′ contains no failure and is compared to Fresh(D_H′)
(D_H′ = H′'s final desired composition) under §M.1. Nothing theorem-backed
is claimed about the pre-H′ failed prefix.
```

### M.4 Confluence history matrix (artifact 7)

Composition assertions are unconditional; continuity probes are policy-conditional (A0 §H.b) and out of kernel scope. Baselines are clean builds with **no domain session**. M4 is the one row that is not a confluence-history row at all: it is a **failure/recovery sanitation oracle** (Corrective-3).

| # | History (→ settle) | Clean baseline | Composition assertions (§I.1) |
|---|---|---|---|
| M0 | root without UiHost, null output (headless) | same root | baseline itself: full capability/fiber/binding truth without UI |
| M1 | provider X absent → present → absent → present (flap, N times) | fresh build with X present | bindings resolve to the *final* X generation; no ghost generations; dependents ACTIVE |
| M2 | A1 → A2 → A1 provider generations (same capability; each replacement staged per §E.4) | fresh build with final A1′ generation | consumers committed to final provider; no stale views; exactly one provider of the capability **at every step of the history, not only at quiescence** (E.4 invariant) |
| M3 | consumer mounted **before** provider vs **after** provider (two runs) | fresh build of both | identical settled truth — order of mounting is not observable at quiescence (B24 loader argument) |
| M4 | activation fails once (e.g., device init error) → revise/retry succeeds — **sanitation oracle, NOT a confluence history** (Corrective-3) | fresh build of the succeeded generation — comparison target for the settlement check only; the whole history is outside Thm 80 (§M.3) | failed attempt **fully discharged** (episode closed, verdict DISCHARGED); FAILED outcome **visible** on the failed generation; failed generation **removed by an explicit, visible revision** (no silent reuse); **no ghost composition state** from the failed generation (Cor 69, §I.1.6); fresh generation **settles normally** — its settled composition truth equals a clean build of it. A violated unwind latches §G.6 instead, and the oracle asserts the honest poisoned state (§G.6). Thm 80 applies at most to the failure-free suffix H′ (§M.3) |
| M5 | same-key contributions added/removed in opposite orders | clean set | final listener sets equal by semantic identity; dispatch count == registered count (H.4) |
| M6 | unrelated Y-side contribution survives X-provider churn | fresh build with Y + final X | Y's contribution set untouched through all X transitions (independence, H.2) |
| M7 | open+play → switch output X→Y → settle (A0 H1) | fresh build on Y, idle | capabilities equal; exactly one live SinkSession + device session on Y; lifecycle clean |
| M8 | output flapped N times, no track ever opened (A0 H3) | clean Y, idle | exactly one SinkSession total — the session-leak detector |
| M9 | decoder replaced while PLAYING / PAUSED / after ENDED (A0 H4) | clean build, no track | all three settle to identical composition truth |
| M10 | listeners in opposite orders (A0 H5) | clean set | M5 stated for the MVP listener set |
| M11 | UiHost removed mid-playback (A0 H6) | root without UiHost | playback capability/fiber truth unchanged by removing a pure consumer |
| M12 | (future DSP) insert EQ → switch output → remove EQ → replace decoder (A0 H7) | clean Music+Decoder+Output(+nodes) | node list == desired order; no ghost nodes/taps |
| M13 | root disposal from any quiescent state (A0 H8) | n/a | empty registry; all composition-owned resources released (device session closed, SinkSession torn down); a teardown contract violation instead latches §G.6 and root disposal reports non-completion |

M0–M3, M5–M11, M13 are confluence rows expressible with the MVP decomposition; M4 is the sanitation oracle and needs a fail-then-revise flow; M12 needs DSP (deferred with AudioRuntime).

**Why M4 is not a confluence row (frozen, Corrective-3).** §M.3 excludes any history containing an activation failure from Thm 80 claims, and a later successful revision does not erase the earlier failure from that history — so `fail once → revise → succeed` can never be cited as theorem-backed confluence over the whole history. What M4 asserts instead is the **failure/recovery sanitation oracle**: the failed attempt fully discharged, FAILED was visible, the failed generation was removed by an explicit visible revision, no ghost composition state survived it, and the fresh generation settles normally. If a theorem-backed confluence statement about the post-recovery state is wanted, it can only be made about the suffix H′ cut after the failed generation has fully discharged and been removed (§M.3's frozen rule); the pre-H′ failed prefix gets no Thm 80 claim. Future executable tests must not silently widen Thm 80 to failed histories — that widening is exactly what this row forbids. The revision-identity oracle D0–D4 (§L.5, Corrective-4) belongs to the same sanitation family: it pins that an unchanged desired incarnation never retries FAILED and that a fresh desired incarnation is the only path to a new generation.

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
| provider activation throws | entry + outcome; dependents stay Pending (raise → Unloading first; FAILED records only after the partial unwind discharges — a failing unwind stays Unloading + §G.6 latch, §F.3) | partial effects of the raising activation (Cor 69: none survive — presupposing their inverses discharge; a failing unwind is §G.6) | revision only | provider fiber | no (failed fibers are quiet-legal, B19) | no ghost provisions; siblings run |
| consumer activation fails after 2 effects, 3rd raises | entry + outcome (FAILED lands only after effects 1–2 unwind cleanly; a failing unwind stays Unloading + §G.6 latch, §F.3) | effects 1–2 unwound; installs nothing (unwind failure → §G.6) | revision only | consumer fiber | no | empty table at rest (Cor 69) |
| consumer teardown disposer fails to discharge (§G.6) | TEARDOWN_VIOLATED latched on the fiber; its committed view stays open; **no clean-exit claim** | the fiber's episode stays open by design — nothing further may claim discharge | no invisible retry; operator/revision only | no — distinct class: violation ≠ activation-FAILED; the fiber stays Unloading even when the violated unwind followed a raise (Corrective-2) | **yes — deliberately**: quiet predicate unsatisfiable while latched; provider final release blocked | no UAF path: providers the fiber relied on keep `relied` true (B14 window); violation surfaced (§J.3); siblings unaffected |
| provider withdrawal during consumer activation | consumer divert → Unloading (target moved) | partial activation unwound | automatic (re-activates when satisfiable) | no | no | resolution coherence (Thm 71): no effect survives against a stale view |
| root disposal during withdrawal | registry draining | same §G order, all providers | n/a | no | until drained (finite, Thm 73) | dependents-before-providers to the end |
| replacement (desired A1 → A2) | staged per §E.4: old drains fully and is removed before new mounts; intermediate state = A1 gone, A2 not yet present (consumers Pending) | old's release completes before A2 is inserted | dependents re-activate automatically against A2 at step 9 | no | until the staged sequence drains (finite, serialized) | **single-source at every step**: installed provision sets for the capability never exceed one (E.4 invariant); a mount request is never issued early |
| same-key disposer called twice | nothing | — | — | — | — | idempotent no-op (B26) |
| effect disposer fails | see §G.6 row (TEARDOWN_VIOLATED) | — | — | — | — | violation latched; no fabricated rollback; provider guards stay latched |
| two required-single providers appear simultaneously | plan refused before any mount | nothing mounted | fix desired graph | no (composition error, not fiber state) | plan rejected (system keeps previous composition) | ambiguity never becomes runtime state (E.2) |
| dependency chain invalidates transitively (C←B←A withdrawal) | cascade of L-Leave | each dependent unwinds own effects | automatic on re-satisfaction | no | until chain drains (finite, acyclic) | ordering Thm 70(2) |
| activation runs forever | fiber stuck Activating | — | — | no (it's a hang, not a failure) | **yes** — boundedness violated | L.4 boundedness is the guard; a hang is a defect of e, surfaced by diagnostics |
| kernel-external resource exhausted (e.g., no device) | provider Failed or degraded-in-provider | per provider policy | per policy | depends where it raised | no | domain policy firewall: degradation inside an ACTIVE provider is invisible to lifecycle |

### O.2 Reading rules

- No scenario leaves unexplained ghost state: every "must clean" cell is Cor 69 (empty table at episode close) or an explicitly latched/surfaced §G.6 violation — never a silent "assume it's clean".
- Retry is always either *automatic reactive re-activation* (dependency returned; the fiber was never failed) or *visible revision behind a fresh desired incarnation* (after FAILED; an unchanged desired incarnation can never retry — §L.5). Never an invisible loop. A teardown contract violation has no retry at all in K0 — it latches (§G.6).
- A fail-then-revise history never becomes a confluence history by eventually succeeding: it is judged by §M.4's failure/recovery sanitation oracle, and Thm 80 is available at most for the failure-free suffix H′ (§M.3, Corrective-3).
- Quiescence blockers reduce to: in-flight transitions (finite by Thm 73 under L.4), an unbounded activation (a component defect caught by the boundedness check), an outstanding staged orchestration step the plan still owes (§E.4/§L.1 clause 5), or a latched TEARDOWN_VIOLATED (§G.6 — deliberate, surfaced, operator-resolved).
- No disposer may report success for an obligation it did not discharge; partial discharge is a violation, not a success with notes.

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
| 5 | Is Effect becoming generic transaction machinery? | **PASS** | effects have exactly one shape — provenance + total inverse (§H.5 frozen block, §H.7); no class enum, no `Option<Disposer>`, no two-phase commit anywhere; the five classes are a descriptive system-boundary/action taxonomy, never kernel Effect variants (H.5) |
| 6 | Is Reconcile becoming Kubernetes? | **PASS** | no health probes, no desired-state polling loop over external reality; desired graph is in-memory declarations; quiescence is a predicate over the registry itself (L.1) |
| 7 | Are we accidentally implementing HMR? | **PASS** | no module loading, no caches, no files (C.2); revision is retire+remount of fibers, never code swapping |
| 8 | Are domain semantics entering lifecycle state? | **PASS** | lifecycle vocabulary is domain-free (F.2); degraded-device, PLAYING/PAUSED, ENDED, retry policy all live inside components (F.5, J.3) |
| 9 | Are ordered relations mislabeled commutative? | **PASS** | H.6 matrix puts DSP/commit-flush/control-ops explicitly outside the independent-effect claim; H.4 forbids tokenizing order |
| 10 | Are Rust type tricks hiding unresolved semantics? | **DEFERRED WITH EXPLICIT TRIGGER** | P.2 shows two genuine open shape questions (dual resolution modes; Decoder object safety). Semantics are decided (§E/§G); representation is §T-open, and the implementation issue must resolve them before API freeze |
| 11 | Can a provider be destroyed before dependents? | **PASS** | guard (`¬relied`) + removal preconditions make it structurally impossible (G.2, Thm 64/70/73) |
| 12 | Can foreign contributions be removed accidentally? | **PASS** | same-key removal goes through opaque tokens scoped to the registrant (H.4); independence claims require the commutativity witness (H.2) |
| 13 | Can a failed activation leave ghost state? | **PASS** | a raise routes into Unloading with the partial accumulator (F.2/F.3); FAILED is recorded only after that unwind fully discharges — a violating unwind stays latched in Unloading instead (§G.6); Cor 69 empties the table (B19, O.1) |
| 14 | Can two legal histories reach visibly different settled composition? | **PASS** (conditioned) | not under L.4 checks + no-failure histories (Thm 80, §M); failure histories are excluded by definition — even fail-then-succeed ones — and are covered by §O assertions plus §M.4's sanitation oracle (Corrective-3); the conditioning is explicit, not hand-waved |
| 15 | Does anything require kernel work on the realtime path? | **PASS** | §N.2 table: all kernel operations forbidden per block; zero exceptions proposed |
| 16 | Can a failing disposer hide a use-after-provider-destroy? | **PASS** (frozen in Corrective-1) | the old "anomaly + exit + guard releases" shape was a review-confirmed defect; §G.6/§H.7 freeze the infallible-inverse contract: violated teardown keeps the episode open, latches TEARDOWN_VIOLATED, blocks provider final release, and forfeits quiescence/confluence claims — liveness is traded, never safety |
| 17 | Can a domain obligation sneak back in as a sixth primitive? | **PASS** (frozen in Corrective-2) | the kernel's whole teardown knowledge is one verdict per fiber — `DISCHARGED` / `CONTRACT_VIOLATED` (§G.6); no obligation registry, list, count, or identity exists kernel-side; obligations are component-contract content (§H.5.1 fence, §J.4, §D.6) |
| 18 | Can Reconcile confuse "unchanged desired entry" with "the operator asked for a retry"? | **PASS** (frozen in Corrective-4) | desired revision identity makes fresh-incarnation intent expressible (§L.5); an unchanged identity never retries FAILED (R1/R7, oracle D0–D4); the kernel derives revision triggers from nothing — not config content, not dependency churn (R2/R3, §S) |
| 19 | Can a FAILED fiber hang settlement forever, or be retried by accident? | **PASS** (frozen in Corrective-4) | quiescence is transition semantics: settled FAILED (and Pending) are quiet-legal (§L.1 clauses 2–3, D1 oracle); nothing retries without a fresh desired incarnation (§L.5); staged plans still owing work and latched violations stay non-quiescent (§L.1 clauses 4–5) |
| 20 | Does "no EffectKind" make composition bindings unprovable, or force a DataEdge sixth primitive? | **PASS** (frozen in Corrective-4; provenance scope corrected in Corrective-5) | structural provenance (capability key + provider fiber identity) is part of the one Effect shape **for relation-bearing effects only** (§D.4): the binding effect is the single authority and §K.3/§I.1.5 diagnostics are projections of it (§K.4); owner-local effects carry only the base triple (owner episode + total inverse + LIFO position) and fabricate no key — no behavioral enum, no second registry, no payload exposure (A4–A6 class of attacks closed) |

Score: 19 PASS, 1 DEFERRED-WITH-TRIGGER (#10, routed to §T). No DESIGN DEFECT remaining. (Corrective-1 converted the review-confirmed defect in the teardown-failure story into #16's frozen defense; Corrective-2 added #17's obligation fence and repaired the raise-path state machine underlying #13; Corrective-4 added #18–#20.)

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
| diagnostic concepts | **≤ 6** (the §I.1 surfaces; the §G.6 violation flag lives **inside** the fiber lifecycle surface — not a seventh concept) |
| kernel Effect shapes | **1** — reversible composition-lifecycle mutation + total inverse (§H.5 frozen block, §H.7); no effect-class enum exists; structural provenance (§D.4/§K.4) is a conditional field of that one shape — present only on relation-bearing effects — not a second shape |
| system-boundary action classes (descriptive taxonomy, §H.5) | **5** — none is a kernel Effect variant |
| desired revision identity per desired entry | **1** opaque equality token — kernel compares, never interprets, never derives (§L.5) |
| kernel obligation concepts | **0** — no registry/list/count/identity; one teardown verdict (`DISCHARGED`/`CONTRACT_VIOLATED`) per fiber (§G.6, §H.5.1) |
| context realms | **1** (root only) |

Explicitly rejected framework-growth patterns (no present requirement proves them): plugin marketplace, version solver/semver machinery, runtime reflection DSL, generic middleware pipeline, distributed/remote discovery, arbitrary nested scopes/realms, macro-DI, runtime scripting, hot module replacement, generic public event bus.

---

## S. Rejected Alternatives

| Alternative | Verdict | Reason (evidence) |
|---|---|---|
| generic public EventBus as kernel primitive | rejected | paper core never requires it; Koishi's need is domain (B29); #67 non-scope |
| service broker / multi-provider coexistence in K0 | deferred (trigger: a real second concurrent provider requirement, e.g. multiple outputs) | broker is a pattern on single-source, not core (B22); MVP has profile-level competition only (A0 §D.3) |
| isolation realms / interception in K0 | deferred (trigger: multi-tenant/sandbox/override requirement) | B21; #67 B default bias; no #53 invariant |
| child-context hierarchy machinery | deferred (trigger: a component that actually instantiates children) | paper Def 52 mechanism is [PAPER] design context only — K0 exposes no child-instantiation fiber operation and lists no child-mount Effect example (§D.3, §H.5, Corrective-3); MVP graph is flat |
| effect-iterator / generator-style incremental activation | rejected for K0 | whole-episode steps are a legal inertial host (B3/B20); generators add machinery with no current requirement |
| async kernel transitions (per-fiber tasks) | deferred (trigger: a blocking activation that must not stall composition) | synchronous serialized control plane suffices (B20); RT firewall favors it |
| in-place provider value mutation as replacement | rejected | provider-identity resolution means equal values are not replacements (B30); withdraw-then-provide is the only observed replacement |
| silent auto-retry of failed activations | rejected | breaks quiescence decidability and confluence accounting (B19); retry = visible revision |
| revision trigger derived by the kernel (config-content hash, implicit change detection, reconcile-run counters) | rejected (Corrective-4) | the kernel derives revision triggers from nothing (§L.5, R1–R3): content-derived triggers make fresh-generation intent unexpressible at best and fabricate retries at worst; the desired revision identity is the sole revision trigger |
| globally transactional reconcile (all-or-nothing mount batches) | rejected | would need second-order recovery machinery; Thm 80 makes quiescent convergence sufficient (L.3) |
| FAILED as terminal product state / as retry loop | rejected (both extremes) | F.5: outcome-record semantics; revision-owned retry |
| "best-effort cleanup" teardown: failing disposer exits anyway, guard releases, provider may die | **rejected** (Corrective-1; review-confirmed defect of Revision 1) | the exact use-after-provider-destroy the withdrawal window exists to prevent (§G.6); teardown violations latch, block provider final release, and forfeit quiescence/confluence claims |
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
7. **Teardown-violation surface** — exact diagnostic schema and operator-recovery shape for the latched `TEARDOWN_VIOLATED` condition (§G.6): frozen are the semantic requirements (latch + explicit surface — the flag is part of the §I.1 fiber lifecycle surface, Corrective-2 — + blocked provider final release + forfeited quiescence/confluence claims, no invisible retry, no "continue anyway"); open are the diagnostic schema, the flag's final name, and which operator recovery paths (process restart vs future explicit policies) the implementation offers.
8. **Domain continuity policy (§K.3 of A0)** — pause-at-CONFIRMED default remains proposed, not frozen; it gates §H.b probes only, never §M confluence.
9. **Device-surprise MVP policy** — fail-closed into G.2 sequence with bounded-retry degradation remains the proposal (A0 §K.4); product decision at implementation time.
10. *(resolved in Corrective-1)* **Governance gate drift** — the review directed the gate advance to happen in this same docs-only PR; `AGENTS.md`, `composition-kernel.md`, `docs/README.md`, `CONTEXT.md`, and `overview.md` now name #67 as the current design gate (chain: #53 PASS/CLOSED → #67 current gate → PR #68 proposed semantic authority → implementation issue only after PASS + merge).
11. **Desired revision identity representation** (Corrective-4) — token form (per-entry counter, operator-supplied epoch, fresh UUID per edit), the authoring convention that keeps it in sync with semantic edits, and the surface for presenting a fresh incarnation (§L.5): frozen is the equality-only semantics (R1–R8) and the D0–D4 oracle; open is the representation and API shape.

---

## Verdict

**Status: merged semantic authority (PR #68, Revisions 1–4); Revision 6 / Corrective-5 is the current pre-implementation review gate.** (Initial draft proposed PASS; human review round 1 returned **PASS_WITH_CORRECTIVES** — six items, resolved by Corrective-1; human review round 2 (PR #68, review `5127266745`) returned **PASS_WITH_CORRECTIVES** — P0-1 plus P1-2/P1-3/P1-4 and P2, resolved by Corrective-2; human review round 3 (PR #68, review `5127362067`) returned **PASS_WITH_TWO_CORRECTIVES** — P0-1 confluence widening in M4 and P1-2 child-mount scope, plus P2 wording, resolved by Corrective-3. PR #68 then **merged**: Revisions 1–4 are the merged semantic authority for #67. The post-merge pre-implementation adversarial review produced Corrective-4 (Revision 5); human review of Corrective-4 (review `5127750303`) returned **PASS_WITH_ONE_CORRECTIVE** — P0 Effect structural provenance over-frozen plus P1 taxonomy wording residue, resolved by Corrective-5 (Revision 6, this revision), which is now subject to the next human review per delivery discipline.)

Corrective-5 resolution summary (human review of Corrective-4, `5127750303`):

```text
P0  Effect structural           Corrective-4 froze provenance as if every
    provenance over-frozen;      Effect owned a capability key, while D.4
    D.4 self-contradiction       also said Effect "must not know other
                                 fibers" yet required data-edge provenance
                                 to name the provider fiber. Frozen: every
                                 K0 Effect carries the base triple — owner
                                 fiber episode, total inverse, LIFO
                                 position; structural composition provenance
                                 exists only on a relation-bearing effect
                                 (capability key; provider/peer fiber
                                 identity for a data edge, §K.4); owner-local
                                 reversible effects (timer, watcher, local
                                 handle, buffer allocation) fabricate no
                                 key and no optional field; "must not know"
                                 narrowed to other fibers' internals/domain
                                 payload — naming a peer identity is not
                                 reading its internals. One behavioral shape
                                 unchanged; no EffectKind, no
                                 DataEdgeRegistry                  §D.4, §K.4, §H.5, §Q20, §R
P1  "Effects may be" wording     the five labels classify actions, not
    residue                      Effects: "Classify effects" → "Classify
                                 actions" in AGENTS.md/composition-kernel.md;
                                 #46 "Effects may be" → "Actions may be
                                 classified as"          AGENTS.md, composition-kernel.md, #46
```

Corrective-3 resolution summary (review round 3):

```text
P0-1 M4 widened Thm 80 to a    M4 is reclassified as a failure/recovery
    failed history            sanitation oracle, not a confluence-history
                               row: fully discharged failed attempt, FAILED
                               visible, failed generation removed by an
                               explicit visible revision, no ghost
                               composition state, fresh generation settles
                               normally. The whole history is NOT theorem-
                               backed confluence; Thm 80 applies at most to
                               the failure-free suffix H′ (§M.3 frozen
                               block)                                          §M.3, §M.4, §O.2, §Q14, PASS criteria
P1-2 child mount half in/out   parent/child semantics are [PAPER] design
    of K0 scope                context only: child instantiation removed
                               from Fiber legal operations and fenced as
                               out-of-scope, `child mount` removed from
                               K0 Effect examples, no-children removal
                               guard marked vacuous; §S trigger kept        §D.3, §F.2–F.3, §H.5, §S
P2 closure wording             review-number-neutral: the implementation
                               issue opens only after final human review
                               accepts this revision and PR #68 merges      Verdict
```

Corrective-2 resolution summary (review round 2):

```text
P0-1 activation-failure       a raise no longer edges Activating → FAILED
    state machine             directly: the fiber first lands in Unloading
                               (partial unwind, pending activation error kept
                               as episode metadata); FAILED is recorded only
                               by a fully discharged unwind — activation
                               failed AND the scene provably cleaned; a
                               violating unwind stays in Unloading +
                               TEARDOWN_VIOLATED and may not reach FAILED;
                               no eighth state introduced
                                                                  §F.1–F.5, §G.6, §B19, §L.2, §M.4, §O.1, §Q13
P1-2 violation joins the      TEARDOWN_VIOLATED is composition-truth
    diagnostic surface        diagnostic truth that falsifies quiescence, so
                               it is folded into §I.1 surface 2 (fiber
                               lifecycle truth) — not a seventh diagnostic
                               concept                                        §I.1, §R, §T.7
P1-3 taxonomy ≠ Effect enum   K0 Effect has exactly one shape: reversible
                               composition-lifecycle mutation + total
                               inverse; the five classes re-frozen as a
                               descriptive system-boundary/action taxonomy;
                               no EffectClass enum, no Option<Disposer>     §D.4, §H.5, §H.7, §A.2, §B23, §R, §Q5
P1-4 domain obligation is     not a sixth primitive and not kernel data: no
    not a kernel primitive    obligation registry/list/count/identity; one
                               teardown verdict per fiber (DISCHARGED /
                               CONTRACT_VIOLATED) is the whole kernel truth  §D.6, §G.6, §H.5.1, §J.4, §Q17
P2 AudioOutput switch trace   old provider's discharged unload + O-Remove now
                               explicit before new mounts (§E.4 steps 5–7
                               instantiated; G.4 no longer reads as a
                               coexistence special case)                      §G.4
```

Corrective-1 resolution summary:

```text
P0-1 teardown failure semantics   infallible-inverse contract frozen; ordinary
                                  activation failure ≠ teardown invariant violation;
                                  TEARDOWN_VIOLATED latches, provider final release
                                  blocked, quiescence/confluence forfeited; the old
                                  "anomaly + exit + guard releases" shape withdrawn   §G.6, §H.7, §F.4, §L.1, §M.3, §O, §Q16
P0-2 provider replacement         staged orchestration frozen (9 steps); running
                                  registry never holds two providers of one
                                  capability at any step; desired-graph intent vs
                                  running-registry truth explicitly separated         §E.4, §E.2, §G.3, §L.2, §O.1, §M.4
P0-3 Effect vs domain ownership   two ownership universes frozen; Option A (K0 Effect
                                  is composition-lifecycle only) chosen over Option B;
                                  ghost oracle scoped to composition-owned universe;
                                  domain resources bridge via obligation discharge    §J.4, §H.5/§H.5.1, §I.1.6, §G.2, §K
P1-4 CLAIMED ≠ emission           CLAIMED = protocol point-of-no-return (cancellation
                                  authority ends); physical render = external emission
                                  boundary; regression in §B23/§H.5 removed; null-
                                  backend scenario frozen                             §H.5.1, §B23
P1-5 authority drift              governance gate advanced to #67 across AGENTS.md,
                                  composition-kernel.md, docs/README.md, CONTEXT.md,
                                  overview.md                                         those files + §T.10
P2 paper snapshot                 arXiv:2608.25512v1 pinned (retrieval date, page count,
                                  SHA256, theorem cross-check); snapshot rule frozen   §A.4
```

All PASS criteria of #67 / the tasking are met within this document:

```text
five primitives: precise semantic responsibility          §D
provider withdrawal deterministic                        §G (guard sequence, trace)
teardown-access semantics explicit                       §E.3, §G.1–G.2
teardown-failure semantics precise and safe              §G.6, §H.7 (Corrective-1)
activation failure reaches FAILED only via discharged    §F (Corrective-2)
unwind; violated unwind stays latched in Unloading
effect classes are descriptive taxonomy, never kernel    §H.5, §R (Corrective-2)
Effect variants; one Effect shape (reversible + total inverse)
domain obligations fenced: one teardown verdict per      §H.5.1, §G.6 (Corrective-2)
fiber, no obligation data in the kernel
replacement preserves single-source at every step        §E.4 (Corrective-1)
composition Effect ≠ domain resource ownership            §J.4, §H.5.1 (Corrective-1)
local revertibility ≠ cross-fiber independence           §H.1 vs §H.2
same-key composability contract explicit                 §H.4, E.5
non-commutative order has an explicit home               §H.4/H.6, L.1 (desired topology)
composition truth vs domain truth cleanly separated      §J (frozen classifier + universes)
confluence testable without domain leakage               §I, §M
failure histories never widen Thm 80 — judged by the    §M.3/M.4 (Corrective-3)
sanitation oracle; confluence claimable only for the
failure-free suffix H′
realtime payload bypasses the kernel                     §K, §N
failure paths leave no unexplained ghost state           §O (Cor 69 grounding; violations latch, §G.6)
CLAIMED ≠ physical emission                              §H.5.1, §B23
paper snapshot identity pinned                           §A.4
Rust representation downstream of semantics              §P (survey, no freeze)
no unsupported claim presented as paper authority        §B provenance ledger + §A.4 pin
```

PR #68 merged on 2026-09-07: Revisions 1–4 are the merged semantic authority for #67. Revision 5 (Corrective-4) was reviewed by human review `5127750303` (**PASS_WITH_ONE_CORRECTIVE**); Revision 6 (Corrective-5) resolves its findings and is the **current pre-implementation review gate**; implementation remains unauthorized until a further human review accepts it. Acceptance authorizes exactly one next step: **opening a separate `COMPOSITION-KERNEL-0 IMPLEMENTATION` issue**. It does not authorize implementation itself, a Rust API freeze, FFmpeg/WASAPI/PocketJS integration, async-runtime selection, or any §S-deferred machinery. This document stops at the gate.



