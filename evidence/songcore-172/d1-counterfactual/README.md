# D1 counterfactual — SongCore build.rs provenance digest defect

Campaign #172 (SONGCORE-MULTI-TARGET-RELEASE-1), build-fixes branch
`songcore-172/pr-a-build-fixes` (base `05f5fef`). Defect confirmed by #324,
`evidence/windows-native-tier2-324/ENVIRONMENT.md` §"Out-of-scope build
observations" #1.

## Defect (pre-fix)

`crates/qianqian-songcore-sys/build.rs` hashed the header and the merged
static archive by shelling out to GNU coreutils `sha256sum` and taking the
**first whitespace token** of its output. For a file name containing a
backslash or newline, coreutils prints the digest line in escaped format:

```text
\<digest>  <escaped-file-name>
```

Every native Windows absolute path contains backslashes, so on Windows
`QN_SONGCORE_HEADER_SHA256` / `QN_SONGCORE_ARTIFACT_SHA256`
(`crates/qianqian-songcore-sys/src/lib.rs:177-178`, `env!()` constants baked
into the rlib) carried a leading `\` — 65 chars, not 64 hex. Additionally,
without `sha256sum` on PATH the whole build panicked
(`sha256sum failed: NotFound`, exit 101) — an external-tool dependency for a
build-provenance step.

## Fix (post-fix)

`build.rs` computes both digests **in-process**: std-only FIPS 180-4 SHA-256
with a fail-closed NIST test-vector self-check (`sha256_self_test()` runs on
every build-script invocation, before any file is hashed; the four vectors
were independently verified against coreutils `sha256sum` before being
embedded). No build-dependency is introduced:

- **Dependency audit conclusion**: zero new crates → no lockfile change
  (verified: a full `cargo build` of the crate leaves the tracked
  crate-local `Cargo.lock` byte-identical), no workspace-member resolution
  impact (the crate is its own `[workspace]` root, excluded from the root
  workspace; the resolver-fragility recorded in its Cargo.toml comment is
  untouched), no new license surface, no build/target toolchain isolation
  concerns (std-only host-compiled build script), and no
  `rerun-if-changed` behavior change (all emitted cargo directives are
  byte-identical to the pre-fix build script). The `sha2` build-dependency
  alternative was considered and rejected: it would add ~8 transitive crates
  plus a network fetch into the fragile excluded-workspace resolution area
  for no correctness gain over the self-checked std implementation.
- Digest semantics unchanged: lowercase hex of the file bytes. The hex
  payloads observed pre-fix and post-fix are identical; the fix removes only
  the escape prefix (and the external-tool dependency).

## Harness

```text
build_rs_prefix_snapshot.rs   pre-fix build.rs, byte-identical snapshot of
                              build.rs at base 05f5fef
                              (sha256 3cb898c6d25c2efdb26858c81c6b9e1d079a38d52c9e991dd11c734af6f9db3e)
run.sh                        replayable harness; phases red|green
results/                      one file per case per host per phase + full
                              WSL run logs (wsl-run-*.log)
```

Replay (per host):

```bash
# Windows (Git Bash):
bash run.sh red && bash run.sh green
# WSL (Ubuntu-24.04; scratch on the WSL-native fs — see "Environment caveats"):
HARNESS_WORK_ROOT=$HOME/d1-harness bash run.sh red
HARNESS_WORK_ROOT=$HOME/d1-harness bash run.sh green
```

Mechanics: the build script itself (snapshot for red, the live fixed build.rs
for green) is compiled standalone with `rustc --edition 2024` and driven via
`CARGO_MANIFEST_DIR` exactly as cargo drives it, against controlled native
trees under `scratch-<phase>/` (deterministic ~1 MiB valid GNU ar archive +
the real tracked `native/include/songcore.h`). Case e additionally runs real
`cargo build -v` — on a throwaway crate copy (red) and on the real crate
(green, scratch target dir) — and reads the build-script record cargo itself
writes (`target/debug/build/qianqian-songcore-sys-*/output`). Oracles:
coreutils `sha256sum` (invoked on a clean relative path) and Python hashlib,
both reading the same physical bytes; scratch is removed on success.

## Case matrix and results

Assertions: Windows red 35/35 PASS, Windows green 47/47 PASS, WSL red
35/35 PASS, WSL green 47/47 PASS (each phase exits 0 with
`SUMMARY: 0 failed assertion(s)`).

| case | pre-fix Windows (RED) | pre-fix WSL (contrast) | post-fix both hosts |
|---|---|---|---|
| a) absolute path as passed to `sha256sum` | `QN_SONGCORE_HEADER_SHA256=\cf509a5c…` (65 chars, escape prefix; artifact `\\322dbf2e…`); payload == oracle | clean 64-hex == oracle (POSIX paths never trigger escaping) | clean 64-hex == coreutils == hashlib |
| a-mechanism) raw `sha256sum` output | line begins `\` and the file name is printed `\\`-escaped; POSIX spelling of the same file is clean | n/a (Windows-specific) | n/a |
| b) path with spaces | same escape prefix (backslash, not spaces, is the trigger) | clean 64-hex == oracle | clean 64-hex == both oracles |
| c) non-ASCII path | same escape prefix | clean 64-hex == oracle | clean 64-hex == both oracles |
| d) no `sha256sum` on PATH | **panic** `sha256sum failed: NotFound` at build.rs:9:10, exit 101 → build failure | same panic | **completes without sha256sum**; digest correct |
| e) cargo rerun-if-changed | rerun semantics correct (no-change → fresh; untracked package file → fresh; header change → rerun with new digest; restore → original digest), **but every rerun records the `\-prefixed` value** in cargo's own output file | same, with clean values | same semantics with clean values; content-change and restore digests == oracles |

Cross-platform digest equality (same deterministic input bytes): the fixed
implementation produces **identical** digests from Windows probe (rustc
1.96.0 / msvc host), WSL probe (rustc 1.97.1 / linux-gnu), the real crate
under Windows cargo, and the real crate under WSL cargo:
`cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536` (header) /
`322dbf2e1023a74f8f9b14911b190e578a100acf3244cc423828340e397a1ea9` (artifact).

## Semantic change recorded for case d

Pre-fix, `sha256sum` had to be on PATH or the build failed. Post-fix the
build script depends only on std, so case d changes from RED (panic) to GREEN
(build completes without the tool). This is an intended consequence of the
fix, not a behavior regression: the digest contract (64 lowercase hex of the
file bytes) is unchanged and re-verified against two independent oracles.

## Environment caveats

- Windows: the default PATH's `python3` is the Store stub (exit 49); the
  harness probes usability and falls back to the real interpreter
  (`/c/Users/fred1/tools/bin`, WORKBRIEF §1).
- WSL on /mnt/c: file mtime readings can drift between stats, which makes
  cargo's own no-change fingerprint check spuriously `Dirty` (cargo message:
  "the dependency `qianqian-songcore-sys` was rebuilt (…463 ms after last
  build)"). This is a host-environment artifact, not build-script behavior —
  the same /mnt/c caveat that pushes native builds into the WSL local fs in
  #324. Set `HARNESS_WORK_ROOT` to a WSL-native directory for case e's
  fresh/rerun assertions to be meaningful.

## Not covered here

Integration linking of the real `libsongcore.a` into
`qianqian-decode-songcore` (D2: no native artifact exists in this worktree;
`native/build/artifacts/` is absent). The digest-level and cargo-integration
verification above uses controlled input trees; the D2 follow-up gate covers
the real artifact.
