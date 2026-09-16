# Plugin boundary conformance audit — headless playback path

> ## 审计最终结论
>
> - **判决：`PLUGIN_BOUNDARY_PASS_WITH_HARDENING_GAPS`（通过，存在待加固缺口）。**
> - **当前生产路径 0 处边界违规。** bootstrap/Plugin 准入区分、Composition 边界（结构 + 行为双证）、Capability 边界、PCM/K0 实时防火墙、D11 终局权威边界、D13 资源分类——全部 PASS。
> - **但 7 个 latent bypass 面（L1–L7）今天只靠 review 纪律维持**：后人写错不会变红。其中「headless 直接 import `SongcoreDecode` / `PcmEdge`」两个可弃置突变实验证明可以干净编译（EXIT=0），即 bypass 面是活的，不是理论风险。
> - **依赖卫生 2 项**：`qianqian-app` 声明但从不使用 `qianqian-audio-api`（应删除）；`qianqian-headless` 把 `qianqian-audio-api` 放在 normal 依赖（实际仅测试代码使用）。
> - **加固方案（§7）经两轮 review 修正后被接受**：
>   - **MAJOR-1 修正**：`DrainSignal` 是 audio-api 有意保留的公共共享契约，不能、也不应靠可见性隐藏；它的防线是 **Cargo 拓扑**——headless → audio-api 仅允许 dev 依赖，normal/build 一律拒绝。分工由此定死：机制实现边界（PcmEdge 等）走 rustc privacy；共享契约角色边界（DrainSignal 等）走 Cargo 依赖图。
>   - **MAJOR-2 修正**：H2 从「headless → 全部 crate 允许」改为 **allowlist-first**：headless 生产依赖白名单 = app / composition / playback + 显式准入的 decode-songcore / output-wasapi。未来新增 Provider 必须显式更新白名单——这个摩擦是刻意的：新 Plugin 进入 composition root 本来就应当是一次显式架构事件。
>   - **MAJOR-3 修正**：decode 链（decode-songcore / songcore-sys）被 workspace 排除且没有任何 CI 编译覆盖，因此加固**不得**在「coverage decision raised」时就宣布完成。完成条件是 `H1_DECODE_BOUNDARY_DURABLE = PASS`：必须存在一个持续运行的 gate，能证明 decode 具体机制 bypass 突变会红。最优解是 CI 真实构建 native SongCore 产物并编译整条链；暂时做不到时允许较弱的 architecture/export gate，但必须诚实标注 `SOURCE-GATE-ENFORCED` 而非 `COMPILER-CI-ENFORCED`。不得为 CI 方便引入 fake/stub SongCore 或 no-link 生产 feature。
>   - **MINOR 修正**：M5 拆分为 M5a（PcmEdge bypass → 期望 rustc privacy 失败）与 M5b（audio-api/DrainSignal bypass → 期望 Cargo 架构门 FAIL，**明确不是** privacy 失败，因为 DrainSignal 本来就是合法公共契约）。
> - **Review 最终判定：AUDIT ACCEPTED；审计报告质量 PASS_WITH_CORRECTIVES；按本文件（已含全部修正）的方案，HARDENING AUTHORIZED。** 加固施工（QIANQIAN-PLUGIN-BOUNDARY-HARDENING-1）是独立后续任务；本审计期间未做任何实现修改，worktree 保持 clean。
> - **本文件是审计证据记录（EVIDENCE），不是架构权威**。唯一规范性权威仍是 `docs/adr/ADR-PBK-001.md` 与 `docs/adr/ADR-PBK-002.md`；本报告不得被引用为新的 authority。

| Field | Value |
|---|---|
| Audit task | QIANQIAN-PLUGIN-BOUNDARY-CONFORMANCE-AUDIT-1 |
| Date | 2026-09-16 |
| BASE_SHA | `c92fd6449d5f8377c5dda14a2c434c4e64df5776` (merge commit of PR #145) |
| PR #145 state at audit | MERGED (branch tip `9b5f528` verified as ancestor of BASE) |
| Worktree during audit | clean; all disposable probes reverted; final `git status --porcelain` empty |
| Truth class | EVIDENCE — audit record, not authority |
| Review outcome | Round 1 (fresh-context adversarial): 0 MAJOR / 6 MINOR, all incorporated. Round 2 (final review): AUDIT ACCEPTED, report quality PASS_WITH_CORRECTIVES; hardening authorized after plan corrections — corrections incorporated in §7. |

---

## 1. Scope and questions

Code-first conformance audit of the current production architecture at BASE, answering:

1. Does the real headless playback path conform to ADR-PBK-002's Plugin / Capability / App / owned-resource boundaries?
2. Which boundaries are machine-enforced today, and which are maintained only by review/docs discipline?
3. What is the minimal hardening plan that makes the latter compiler/CI-enforced, with non-vacuous negative controls?

Per the audit task, the A-vs-B distinction of §2 governs the bootstrap question: naming a Plugin constructor that returns a `ComponentSpec` is legitimate admission; touching a concrete mechanism type is bypass.

## 2. Method and evidence base

- Every claim below was verified against production source and manifests at BASE_SHA. No historical spec, old noun, or prior design was inherited as authority (AGENTS.md start-here rules).
- Dependency edges extracted from `cargo metadata` plus a direct read of all nine manifests (Appendix A), including the workspace-excluded decode chain.
- Disposable demonstrations (all reverted, worktree verified clean afterwards):
  - A compile probe mutating `apps/headless` to import and use `SongcoreDecode` and `PcmEdge` directly — compiled clean (EXIT=0), proving surfaces L1/L4 are live.
  - An H2 prototype gate (kept outside the repo) validated against the current tree (PASS) and two synthetic mutations (M3, M4 — both RED).
- Review rounds: a fresh-context adversarial review attacking the findings and the plan (§8, round 1), then a final review that accepted the audit with plan correctives (§8, round 2). This document is the post-corrective record.

## 3. Conformance findings

All six boundary checks PASS. Zero current violations.

### B1 — Bootstrap vs mechanism (PASS)

`apps/headless/src/main.rs` names only ComponentSpec-returning constructors and the App layer:

```text
QianqianApp::new()
qianqian_decode_songcore::songcore_decode_plugin()
qianqian_output_wasapi::wasapi_output_plugin()
qianqian_playback::playback_session_spec(file, handle)
```

then `revise_desired`, `composition_snapshot` (with `FiberState::Active` used for activation diagnosis only), a stdin thread that reaches the episode exclusively through the `PlaybackSessionHandle` (`request_stop` / `observe`), `wait_terminal`, `dispose`. No mechanism type (`SongcoreDecode`, `WasapiOutput`, `PcmEdge`, `DrainSignal`) appears anywhere in bootstrap.

### B2 — Composition boundary of the Playback Session Plugin (PASS, with a stated evidence caveat)

- Structural: in `qianqian-playback` production src, only `session.rs` references `qianqian_composition`. The only capability resolutions are `ctx.resolve::<PcmDecodeCapability>()` (`session.rs:77`) and `ctx.resolve::<AudioOutputCapability>()` (`session.rs:83`), each exactly once at activation. The decode worker steady loop touches only `DecodedPcmStream`, `PcmEdge`, and completion evidence mutators.
- Behavioral: `tests/k0_firewall.rs` runs a `debug_op_count` oracle that counts kernel operations across activation and the per-quantum path, with a sensitivity control proving the oracle flips red when a K0 operation is injected into the data plane (non-vacuous).
- Caveat (recorded honestly): k0_firewall exercises provider doubles. For the real WASAPI provider, the zero-K0 property rests on the structural argument — `wasapi.rs` contains no composition imports and its steady loop is `WaitForSingleObject` / `GetCurrentPadding` / `GetBuffer` / `read_frames` / `ReleaseBuffer` per block.

### B3 — Capability boundary (PASS)

Capability keys and service traits live in `qianqian-audio-api/src/ports.rs` as pure contracts. Providers expose constructors plus capability trait impls; `SongcoreDecodeStream` is pub-named but its `open` is private, so it is unconstructable outside the decode crate. The session resolves each capability exactly once; no provider reaches into another provider's internals.

### B4 — PCM/K0 realtime firewall (PASS)

`edge.rs` has zero kernel references; `PcmEdge` is direct typed hot data, not dispatch payload. `RenderRequest` carries `Arc<dyn RenderPcmInput>` plus a `DrainSignal`; the per-block paths in both providers and the session worker contain no Context lookup, no Capability resolution, no Fiber Reconcile, no Fact fan-out.

### B5 — D11 terminal-outcome authority boundary (PASS)

`completion.rs` settles the episode synchronously under one lock on session-owned paths — no resolver thread (D14.3); resolver precedence is decode failure > drain×worker-terminal, with stop intent disambiguating Stopped vs Completed. `handle.rs` is the only application-facing seam (`request_stop` / `observe` / `wait_terminal`). `status.rs` is a pure projection carrying a `FORBIDDEN_STATUS_WORDS` negative-control oracle. No authority forgery, no inference of semantic truth from mechanism evidence, no projection-as-correctness was found. The #144 production↔formal decision-table oracle (48-tuple byte-compare against the shared `CurrentDecisionTable.tla`, gated from `completion.rs`) remains attached to this boundary.

### B6 — D13 Plugin vs owned-resource classification (PASS)

Decoder endpoint, decode worker, PCM edge, and render relation are owned resources/effects of the Playback Session Plugin, registered LIFO so teardown unwinds stop → join → release. No feature-shaped over-fragmentation; no unit was found that should be a Plugin but is an owned resource, or vice versa. `qianqian-app` stays outside composition (D3) as a thin generic admission layer.

## 4. Latent bypass surfaces (L1–L7)

These are not current violations. They are public surfaces or missing gates that would let a future change violate a boundary without anything turning red.

| ID | Surface | Today's exposure | Demonstration status |
|---|---|---|---|
| L1 | `pub struct SongcoreDecode` + `pub fn new()` in decode-songcore | headless can construct the decode mechanism directly, skipping admission | LIVE: M1 probe compiled EXIT=0 at BASE (reverted) |
| L2 | `pub struct SongcoreDecodeStream` (private `open`) | pub-named but unconstructable externally; zero consumers anywhere | latent, unused |
| L3 | `pub use wasapi::WasapiOutput` ("exported for direct-mechanism tests") | headless can touch the output mechanism directly | latent; stateless unit struct, pub today |
| L4 | `pub PcmEdge` + `pub use edge::{PcmEdge, ...}` in playback | headless can build its own PCM edge beside the session's | LIVE: M5 probe compiled EXIT=0 at BASE (reverted) |
| L5 | `pub type SharedEdge = Arc<PcmEdge>` | alias invites out-of-crate edge sharing; zero consumers anywhere | latent, unused |
| L6 | `pub EdgeTerminal`, `pub WriteOutcome` | consumers are only in-crate `#[cfg(test)]` modules (test-only today) | latent, test-only consumers |
| L7 | No machine crate-edge gate | vocabulary gate catches retired identifiers only; nothing denies a wrong crate edge | decode chain additionally has no CI compile coverage (see §7.5) |

## 5. Dependency hygiene findings

- **H-a**: `crates/qianqian-app/Cargo.toml` declares `qianqian-audio-api`; the crate has zero `audio_api` references in source. The app layer is generic K0 admission only. Delete the dependency (see §7.2 for the permanent rule this becomes).
- **H-b**: `apps/headless/Cargo.toml` lists `qianqian-audio-api` as a **normal** dependency; production src never uses it — only `apps/headless/tests/status.rs:10` (`use qianqian_audio_api::ports::PcmFormat;`). Per review round 2 this is promoted from mere hygiene to a production boundary rule of the H2 gate (MAJOR-1): normal/build DENY, dev ALLOW.

## 6. Enforcement status map

| Boundary | Maintained today by | Machine-enforced today? |
|---|---|---|
| Bootstrap vs mechanism (B1) | review discipline only | NO |
| Capability-only consumption (B3) | provider structure (private `open`) + session structure | partial (no gate) |
| PCM/K0 firewall (B4) | structure + k0_firewall oracle (CI-run, but over provider doubles) | partial |
| Mechanism hiding (L1–L6) | nothing — surfaces are `pub` | NO |
| Crate edge topology (L7) | nothing | NO (vocabulary gate ≠ edge gate) |
| D11 authority (B5) | settlement tests + #144 Rust↔TLC oracle (completion.rs triggers the formal gate) | partial→strong (semantic layer) |
| Architecture vocabulary | `tools/check_architecture_vocabulary.py` with built-in negative control | YES (stale identifiers only) |

## 7. Hardening plan (final, post-corrective)

Authorized as follow-up task QIANQIAN-PLUGIN-BOUNDARY-HARDENING-1. Not implemented in this audit. The plan enforces boundaries PBK-002 already defines; it introduces no new authority, noun, or lifecycle state (razor-compliant: Rust visibility + manifest topology are ordinary local mechanisms).

### 7.1 H1 — compiler visibility shrink

Final public API surface:

```text
qianqian-decode-songcore:  pub songcore_decode_plugin()
qianqian-output-wasapi:    pub wasapi_output_plugin()
qianqian-playback:         pub PlaybackSessionHandle
                           pub PlaybackSessionObservation
                           pub EpisodeTerminalOutcome
                           pub playback_session_spec
```

Everything else goes private/`pub(crate)`: `PcmEdge`, `SharedEdge`, `EdgeTerminal`, `WriteOutcome`, `SongcoreDecode`, `SongcoreDecodeStream`, `WasapiOutput`, completion internals. The `pub use edge::{...}` re-export block in `crates/qianqian-playback/src/lib.rs` is the bypass surface this removes.

Integration tests that exercise mechanisms (`edge_lifecycle.rs`, `loom_edge.rs`, `read_seam.rs`, `session_activation.rs`, `stop_seam.rs`, `test_oracles.rs`, `k0_firewall.rs`) move in-crate following the F2 white-box pattern (`#[cfg(test)]` + `#[path]`), which also retires the "exported for direct-mechanism tests" rationale for L3.

**`DrainSignal` and the rest of `qianqian-audio-api` ports stay public.** They are deliberately shared contracts, not mechanisms (MAJOR-1). Their protection is Cargo topology (§7.2), not visibility.

Loom recipe correction (round-1 MINOR 1): this is not a one-line `--cfg loom --lib` change — that would compile the thread-spawning settlement tests against loom primitives and panic. Required: `#[cfg(not(loom))]` partitioning of those tests, plus the two `--test loom_edge` runner updates in `specs/playback-concurrency/check.sh`.

### 7.2 H2 — allowlist-first Cargo graph gate

A committed (in-repo, CI-run) gate over `cargo metadata` edges: normal, dev, and build kinds; fail-closed on unknown edges; must also scan the workspace-excluded crates via `--manifest-path` so the decode chain is covered; ships with its own negative controls in CI. (The audit prototype proved feasibility: current tree PASS; M3/M4 mutations RED.)

**headless production dependency allowlist** (MAJOR-2 — allowlist-first, not denylist):

```text
headless production
    ├── qianqian-app
    ├── qianqian-composition
    ├── qianqian-playback            (semantic seam)
    ├── qianqian-decode-songcore     (Plugin constructor, explicitly admitted)
    └── qianqian-output-wasapi       (Plugin constructor, explicitly admitted)

    X qianqian-audio-api             normal/build DENY, dev ALLOW   (MAJOR-1)
    X qianqian-songcore-sys          DENY
    X any crate not explicitly admitted to the composition root
```

Per-crate normal-edge rules:

```text
qianqian-composition       ALLOW: (none)                                 DENY: any qianqian-*  (existing firewall test)
qianqian-audio-api         ALLOW: composition                            DENY: playback, providers, app
qianqian-app               ALLOW: composition                            DENY: audio-api (after H-a deletion), playback, providers, sys
qianqian-playback          ALLOW: audio-api, composition                 DENY: providers, sys, headless
qianqian-decode-songcore   ALLOW: audio-api, composition, songcore-sys   DENY: output-wasapi, playback, headless
qianqian-output-wasapi     ALLOW: audio-api, composition                 DENY: decode-songcore, playback, headless
qianqian-headless          ALLOW: app, composition, playback, decode-songcore, output-wasapi
```

Dev edges are gated too, with the status-quo allowlist preserved: playback / decode-songcore / output-wasapi may dev-depend on `qianqian-app` (admission tests); headless may dev-depend on `qianqian-audio-api` (after the H-b move; consumed today by `tests/status.rs`). Any new edge — normal or dev — requires an explicit rule update.

Adding a future provider (`output-coreaudio`, `output-alsa`, any new decoder) to the composition root means touching this allowlist on purpose. That friction is the design: a new Plugin entering the composition root is an explicit architecture event.

`qianqian-app` is thereby pinned permanently:

```text
Qianqian App abstraction
       ↓
generic K0 only
```

while the concrete executable `apps/headless` is the only place where concrete Plugin definitions get composed.

### 7.3 H3 — no general symbol/call gate

Not needed once H1+H2 land. If one is ever earned, it must be import/call-site aware: comment-only mentions (e.g. `status.rs` naming `DrainSignal` in a doc comment) must not trip it (round-1 MINOR 6).

### 7.4 H4 — negative controls (all must be demonstrated non-vacuous before "complete")

| Control | Mutation | Expected post-hardening | Status |
|---|---|---|---|
| M1 | headless imports/constructs `SongcoreDecode` | rustc privacy failure | demonstrated LIVE pre-H1 (compiled EXIT=0) |
| M2 | headless imports/uses `WasapiOutput` | rustc privacy failure | live surface today (L3) |
| M3 | playback depends on output-wasapi | H2 gate FAIL | demonstrated RED on prototype |
| M4 | decode-songcore depends on output-wasapi | H2 gate FAIL | demonstrated RED on prototype |
| M5a | headless uses `qianqian_playback::PcmEdge` | rustc privacy failure | demonstrated LIVE pre-H1 (compiled EXIT=0) |
| M5b | headless adds audio-api as normal dependency and uses `DrainSignal` | H2 gate FAIL — **not** a rustc privacy failure; `DrainSignal` is legitimately public | half-live today: the normal dependency already exists (H-b) |
| M6 | K0 operation injected into the per-quantum path | `debug_op_count` oracle RED (sensitivity control already proves non-vacuity) | oracle green with sensitivity assertion in place |

The loom-level mutation control M-L1 in `specs/playback-concurrency/check.sh` (patching `src/edge.rs`) remains the concurrency-property control and is unaffected.

The M5 split (round-2 MINOR) fixes the layer confusion of the original M5: mechanism implementation boundary → Rust visibility (M5a); shared-contract role boundary → Cargo graph (M5b).

### 7.5 Decode-boundary durability completion gate (MAJOR-3)

`SongcoreDecode → private` is compiler-enforced in the language, but the decode chain is workspace-excluded (structurally forced: `crates/qianqian-songcore-sys/build.rs` fails closed without the real native `libsongcore.a` + headers) and no CI workflow compiles it. If someone later re-publishes the type and starts calling it from headless, main CI may have no compile task that sees it.

Therefore hardening is not complete on "coverage decision raised". The completion condition is:

```text
H1_DECODE_BOUNDARY_DURABLE = PASS
    iff a continuously-running gate demonstrates that the decode
    concrete-mechanism bypass mutation (M1) turns red.
```

- Preferred: CI builds the real native SongCore artifact and compiles the decode chain, making the privacy boundary genuinely COMPILER-CI-ENFORCED.
- Interim acceptable: an architecture/export gate over the decode chain source, honestly labeled **SOURCE-GATE-ENFORCED** — not COMPILER-CI-ENFORCED.
- Forbidden for CI convenience: fake/stub SongCore, or a no-link production feature, unless separately earned with independent evidence.

### 7.6 Authority discipline

H1/H2 enforce existing PBK-002 boundaries; no ADR text changes are required or performed by the hardening task. One authority-queue item was found during the audit and is **not** actioned here: the "Current realization" note in ADR-PBK-002 §17 is stale relative to post-#145 code and must be resolved by an explicit authority amendment under review — not by a silent edit and not by this audit report.

## 8. Review history

### Round 1 — fresh-context adversarial review (§25 attack pass)

0 MAJOR, 6 MINOR, all incorporated into this record:

1. H1's loom migration is not a one-line change; needs `#[cfg(not(loom))]` partitioning plus two `check.sh` runner updates.
2. `SharedEdge` and `SongcoreDecodeStream` have zero consumers anywhere (stronger latent-surface claim).
3. `EdgeTerminal` is additionally consumed by in-crate `#[cfg(test)]` modules (still test-only).
4. k0_firewall runs on provider doubles; real-WASAPI zero-K0 rests on the structural argument — recorded as the B2 caveat.
5. Workspace exclusion of the decode chain is structurally forced by `songcore-sys/build.rs`; the decode provider has no CI compile coverage — escalated in round 2 to MAJOR-3.
6. Comment-only grep pollution (`status.rs` names `DrainSignal` in a doc comment); any future symbol gate must be import/call-site aware.

### Round 2 — final review

- MAJOR-1: DrainSignal cannot be protected by H1 privacy; enforce headless→audio-api through Cargo topology (normal/build DENY, dev ALLOW). Incorporated in §7.1/§7.2 and M5b.
- MAJOR-2: replace "headless → ALL crates" with the explicit production allowlist. Incorporated in §7.2.
- MAJOR-3: decode compiler privacy needs durable CI/negative-control coverage; "coverage decision raised" is not enough to claim hardening closed. Incorporated in §7.5.
- MINOR: split M5 into the PcmEdge/compiler control (M5a) and the DrainSignal/Cargo control (M5b). Incorporated in §7.4.

Final assessment as delivered:

```text
CURRENT PRODUCTION:
    violations                     0
    Plugin/bootstrap distinction   PASS
    Capability boundary            PASS
    PCM/K0 firewall                PASS
    D11 authority boundary         PASS
    D13 resource classification    PASS

AUDIT VERDICT:
    PLUGIN_BOUNDARY_PASS_WITH_HARDENING_GAPS
    CONFIRMED

AUDIT REPORT QUALITY:
    PASS_WITH_CORRECTIVES

VERDICT:
    AUDIT ACCEPTED
    HARDENING AUTHORIZED AFTER ABOVE PLAN CORRECTIONS
```

## 9. Verdict

```text
PLUGIN_BOUNDARY_PASS_WITH_HARDENING_GAPS
```

The real headless playback path conforms to ADR-PBK-002's Plugin / Capability / App / owned-resource boundaries with zero current violations. Seven latent bypass surfaces and the absence of any crate-edge gate mean those boundaries are currently maintained by review discipline; the corrected §7 plan (visibility shrink + allowlist-first Cargo gate + durable decode coverage + non-vacuous negative controls) is the authorized route to "future mistakes turn red".

## 10. Explicitly not done here

- No production code, visibility, manifest, CI, or ADR change was made during this audit; the worktree is clean at delivery. This document is an evidence record only.
- Implementation of §7 (QIANQIAN-PLUGIN-BOUNDARY-HARDENING-1) is a separate authorized follow-up task — not started.
- F3 is not started.
- The ADR-PBK-002 §17 staleness noted in §7.6 awaits an explicit authority-resolution amendment.

## Appendix A — dependency edge table (verified at BASE)

Workspace members per root `Cargo.toml`: headless, audio-api, composition, output-wasapi, playback, app.
Workspace-excluded: `crates/qianqian-decode-songcore`, `crates/qianqian-songcore-sys`, `experiments/songcore-call-comparison`.

| From | Edge | Kind | Class |
|---|---|---|---|
| headless | audio-api | normal | H-b → H2 rule: normal/build DENY, dev ALLOW |
| headless | composition | normal | allow |
| headless | playback | normal | allow |
| headless | app | normal | allow |
| headless | decode-songcore | normal, optional (`feature = playback`) | allow (admitted Plugin) |
| headless | output-wasapi | normal, optional (`feature = playback`) | allow (admitted Plugin) |
| audio-api | composition | normal | allow (capability key binding site) |
| composition | — | (no dependencies; in-crate firewall test enforces) | allow |
| playback | audio-api | normal | allow |
| playback | composition | normal | allow |
| playback | app | dev | allow (admission tests) |
| playback | loom | optional (`feature = loom`), `--cfg loom` only | external, verified constraint |
| output-wasapi | audio-api | normal | allow |
| output-wasapi | composition | normal | allow |
| output-wasapi | app | dev | allow |
| output-wasapi | windows 0.62 | normal, `cfg(windows)` | external |
| app | audio-api | normal, **zero source usage** | delete (H-a) |
| app | composition | normal | allow |
| decode-songcore | audio-api | normal | allow |
| decode-songcore | composition | normal | allow |
| decode-songcore | songcore-sys | normal | allow |
| decode-songcore | app | dev | allow |
| decode-songcore | serde_json | dev | external |
| songcore-sys | — | (no qianqian deps; `build.rs` fails closed without native `libsongcore.a`) | allow |

CI inventory (relevant to L7 / §7.5): `architecture-vocabulary` (vocabulary gate), `verification-rust-gate`, `windows-compile-gate` (`cargo check --workspace --all-targets`; `cargo test -p qianqian-playback -p qianqian-headless`), `formal-semantic-gate` (TLC). No workflow compiles the excluded decode chain.
