# R-H H0 consolidated remediation — execution evidence index

Every run below is an immutable RUN_ID directory under
`/home/hoo/Source/qianqian-h0-evidence/runs/<RUN_ID>/` with `metadata.json`
(label, exact argv, cwd, subject sha, start/end UTC, rc, unfiltered log path,
log sha256, rustc/cargo versions, platform) and the full unfiltered
`stdout-stderr.log`. No gate/test output was ever piped through `tail`/`head`
for capture; nothing was truncated. A red was never washed by a rerun: every
failed attempt keeps its RUN_ID, and reruns got NEW RUN_IDs. Toolchain:
`rustc`/`cargo` versions recorded per run; platform `linux-wsl2-x86_64`.

Local executor logs are mirrored into this PR's comments (WRITE→FETCH→
BYTE-COMPARE) so the evidence does not depend on this machine.

## Lane coverage legend

`RUN_PASS` (executed, green) · `RUN_RED_AS_EXPECTED` (mutant/control red is the
correct outcome) · `NOT_RUN` (channel not executable here; never reported as
passing).

## G0 — baseline at the frozen subject `daad414` (before any edit)

| RUN_ID | Lane | Result |
|---|---|---|
| `20261012T134037-566016-12167` | `cargo test -p qianqian-audio-api --test direct_pcm_flow --test realtime_view_publication --test render_gate --test render_gate_seek` | RUN_PASS — 19+23+7+9 = 58/58 |
| `20261012T134037-566030-20463` | `cargo test -p qianqian-playback --lib` | RUN_PASS — 187/187 |
| `20261012T134037-566044-9147` | `cargo test -p qianqian-playback --test pause_seam --test seek_seam` | RUN_PASS — 10/10 + 18/18 |

Reference green for the subject (C5: "861/861 workspace at daad414") is
consistent with these focused lanes.

## Pre-fix probes / falsifiers (disposable worktrees, isolated)

| RUN_ID | Subject | Result |
|---|---|---|
| `20261012T134814-568156-31126` | P-P105 probe (payload unconsumed at the illegal slot free) | RUN_PASS of the probe assert ⇒ defect witnessed |
| `20261012T134908-568785-27163` | M-P201@base (request_resume deleted) under external `timeout 90` | rc=124, "running for over 60 seconds" ⇒ forever-park witnessed |
| `20261012T135034-570017-23909` | M-P106a (second-hold conjunct deleted only) | property GREEN ⇒ SURVIVES, classified (twin-conjunct masking) |
| `20261012T135134-570685-13005` | M-P106b (both holds) | RED at the new case's semantic assert ⇒ KILLED |

## Per-commit focused evidence (main tree; test-then-commit workflow)

Subject relation (round-1 scope-review correction): each run below executed the WORKING
TREE containing that group's repair BEFORE the commit was recorded, so a run's
`subject_sha` in metadata.json is the PARENT commit (e.g. commit1's run has subject
`daad414`). Additionally, the commit-header rewrite (see the gate table below) changed the
branch's commit SHAs after these runs were captured; the ledger's BRANCH_COMMIT column uses
the post-rewrite SHAs, while the PR-comment run mirror preserves the original subjects the
runs actually reported. `git diff d7ae68e..cb18af5` (pre-rewrite docs commit vs HEAD) shows
the rewrite touched only the three h0 docs' commit references — no RUN_ID changed and no
history was washed.

| Commit (post-rewrite) | pre-rewrite SHA | Group | RUN_ID | Result |
|---|---|---|---|---|
| `cde9529` | (unchanged) | R3A-P2 | `20261012T134916-569089-9099` | RUN_PASS 1/1 (pause_seam focused) |
| `7a16df3` | `4f9dbcb` | R3B-P1-05 | `20261012T134948-569320-9874` | RUN_PASS 1/1 |
| `bb70fcf` | `2145cdf` | R3B-P1-06 | `20261012T135011-569667-18889` | RUN_PASS 1/1 |
| `a739c0e` | `c4d0c56` | R3B-P2-01 | final focused run of the sequence: `20261012T135753-571385-24750` | RUN_PASS 20/20 (incl. new kill test; zero-allocation oracle restored). Failed attempts preserved: `…571022-8696` (compile E0599), `…571083-473` (semantic: witness Vec violated the zero-allocation oracle — measured 5 allocations), `…571330-4106` (compile E0433) |
| `1d11f57` | `7617d2e` | R3A-A1 | `20261012T135855-571637-6474` | RUN_PASS 9/9 (render_gate_seek) |
| `acb707a` | `91efa08` | R3A-A2 | `20261012T140031-571845-6492` | RUN_PASS 9/9 |
| `35917f6` | `ac1bde6` | R3A-A3 | `20261012T140236-572068-15482` | RUN_PASS 7/7 (render_gate) |
| `48f87b4` | `b705465` | R3A-A6 | `20261012T140352-572276-7070` | RUN_PASS 23/23 (realtime_view_publication) |
| `45edeac` | `c4a8a36` | R3B-P1-03 | `20261012T140422-572543-6897` | RUN_PASS 1/1 |

## Mutant falsifier at the fixed HEAD

| RUN_ID | Mutation | Result |
|---|---|---|
| `20261012T140447-572905-25150` | M-P201@HEAD (request_resume deleted) | RUN_RED_AS_EXPECTED — `operation exceeded 10s`, FAILED in 10.00s |

## Registered negative controls at the H0 HEAD (post-`45edeac` test tree)

| RUN_ID | Control | Result |
|---|---|---|
| `20261012T140950-573394-23426` | PR #257 M2 (first attempt) | INVALID RUN, retained: the capture wrapper invoked `cargo test … straddle straddling` (two positional filters — a cargo usage error, rc=1, no tests executed). Diagnosed from the preserved log; rerun with a single filter as `…573453-23204` below. No semantic claim rests on this run. |
| `20261012T141018-573453-23204` | PR #257 M2 (worker_gone, second hold) | RUN_RED_AS_EXPECTED — both straddle witnesses FAILED |
| `20261012T141033-573799-14367` | PR #257 M3 (both holds) | RUN_RED_AS_EXPECTED — both straddle witnesses FAILED; restore byte-verified (completion.rs sha256 `23240024…65c0`) |

## Final HEAD verification

| RUN_ID | Lane | Result |
|---|---|---|
| `20261012T141058-574150-17105` | `cargo test --workspace` (test tree of `45edeac` + fmt commit) | RUN_PASS — 862 passed / 0 failed across 47 binaries (+1 vs the 861 baseline = the new kill test) |
| `20261012T141309-579506-9423` | `RUSTFLAGS="--cfg loom" cargo test -p qianqian-playback --features loom --release --lib` | RUN_PASS — 12/12 (156.22s; loom-compiled set excludes the fmt commit's files; completion.rs untouched by fmt) |
| `cargo fmt --check` | gate lane | RUN_PASS (after the fmt follow-up commit) |
| `cargo clippy --workspace --all-targets -- -D warnings` | gate lane | RUN_PASS (no diagnostics) |
| `20261012T141838-580070-18461` | `lefthook run pre-push --all-files` (first attempt) | FAILED — commit-convention: 7 headers over the 72-char maximum + a `style` type outside the allowed set; downstream checks skipped fail-fast. Diagnosed from the preserved log (never washed): headers rewritten while the branch was still unpushed; the failed run is retained. |
| `20261012T142052-580436-20312` | `lefthook run pre-push --all-files` (after the header rewrite, at the branch HEAD) | RUN_PASS — VERDICT PASS, rc=0: vocabulary, commit-convention (11 commits ahead of `daad414`), plugin boundary, fmt, clippy `-D warnings`, full workspace suite (47 binaries, 862/0 — this IS the post-docs final full re-run; the docs delta touched no compiled file), docs-verify (verify.mjs + VitePress build), 103.83s |

## GitHub CI outcomes on the PR (recorded after the local campaign; round-2 docs commit)

All five PR CI lanes ran GREEN on the branch HEAD (checked via `gh pr checks` after the
reviews opened; fold-in requested by the round-1 scope review):

| CI lane | Result |
|---|---|
| `cfg(windows) compile + device-free tests` | PASS (4m36s) — the Windows device-free lane the local executor could not run |
| TLA+/TLC current suites (K0 + realtime publication + episode terminal settlement) | PASS (1m47s) — covers the TLC lane the local executor recorded NOT_RUN (no cached jar); the frozen models are unchanged by this PR |
| matrices + Miri + mutations + loom (specs/check.sh rust) | PASS (7m59s) |
| fmt + check + test + clippy (Linux CI) | PASS (2m35s) |
| Cargo topology allowlist + export surface + negative controls | PASS (9s) |

## NOT_RUN lanes (accurate recording, per #260 G3)

| Lane | Status | Reason |
|---|---|---|
| Windows device-free tests / `cfg(windows)` | locally NOT_RUN (no Windows host); CI lane on this PR: PASS (table above) | locally not executable; CI is the Windows evidence channel |
| specs/f5 M1, episode-terminal M8/M10 (TLA+/TLC) | locally NOT_RUN (no cached tla2tools.jar); CI TLA+/TLC lane on this PR: PASS (table above) | local jar absent (network download required); the frozen models are byte-identical to `daad414` in this PR's diff |
| specs/f5 M1, episode-terminal M8/M10 (TLA+/TLC) | NOT_RUN | `tla2tools.jar` not cached locally (`specs/tools/` absent; download needs network). Scope note: this PR changes no formal artifact, so the frozen models' subject is byte-identical to `daad414`; existing formal evidence remains applicable to the unchanged models. |
| FFI / excluded crates lane (`QIANQIAN_NATIVE_DIR` decode-songcore) | NOT_RUN | Outside this PR's touched surface (no SongCore/binding file touched); lane intact per R3-B round. |
| Miri | NOT_RUN | No Miri lane is registered for these test binaries in the repo's verification matrix. |
| Real hardware / WASAPI endpoint | NOT_RUN | No audio endpoint in this executor; AGENTS.md forbids reporting device claims without the real boundary. |
