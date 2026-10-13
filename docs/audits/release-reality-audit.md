# Release reality audit — #171

Evidence snapshot: 2026-10-13. This is evidence, not playback authority.
BASE_SHA = c1df5cf074f0873f68bb8b7c16f4de2da9d5d7c5.

## FACT

- `git fetch origin`; origin/main and initial HEAD equal BASE_SHA; initial
  `git status --porcelain` is empty. Workspace version is 0.1.0.
- Product annotated tag object: `3df1c27a83584a2631c715d21ce3c55a3500397c`;
  peeled source: `b7a526c77173f69ba8efd14897a3c57e1a806bd4`.
- Release published 2026-10-06T17:01:12Z. Asset:
  `qianqian-v0.1.0-windows-x86_64.zip`, 2742216 bytes, downloaded SHA-256
  `7918c3672a02075f7c01c38262116b1cdfaa776ea8caffb104695b9fd699ecb8`,
  equal to API digest and published notes.
- ZIP root is `qianqian-windows-x86_64/`; exactly canonical exe, QUICKSTART,
  LICENSE, LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY_NOTICES and BUILD-MANIFEST.
  Six payload hashes/sizes match. Source is clean and equals peeled tag.
  Recorded archive hash `408d7d6ea4c9d1707283104366725c3deda47d5a4cebee5102c170fdf86d404b`;
  header hash `cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536`.
  Rust 1.96.0; FFmpeg n9.0.1; target windows-gnu. Independently inspected
  real PE imports agree with the original manifest's system-only DLL list.
- HISTORICAL_RELEASE_DEBT: manifest claims its own size 1891 and hash
  `b7d598dfbf89ac750f0456fecb77facddee5e18b4d033670a2d8c6940a3938e3`;
  actual size 2658 and hash differs. Native compile-closure/toolchain receipt
  is absent. No archive binary is present to independently verify its hash.
- Before implementation, real old-script execution in an isolated worktree
  with modified tracked QUICKSTART succeeded (exit 0). No versioned name or
  external checksum file was generated. Source restored afterward.
- No open PR was returned by `gh pr list --state open`. No CHANGELOG or
  release-qianqian workflow exists at BASE_SHA.

## STALE_PREMISE

Issue #171's pre-0.1.0 and HOSTED_CI_UNAVAILABLE premises are obsolete.
At BASE_SHA Rust Regression run 38407936143, Plugin Boundary Gate 38407935713
and Windows Compile Gate 38407935684 all completed successfully. Publication
already happened; new infrastructure must never claim it built that asset.

## DERIVED — original DAG and stopping points

| Edge / input identity | Generator → consumer | Check authority / hidden state / clean rebuild / stop |
|---|---|---|
| Git commit/tag → version | Git/Cargo → package | No old tag/version gate; new source policy must stop before build |
| Version → history | Release notes → human | No CHANGELOG; tag source and notes anchor historical claims |
| FFmpeg pin → source ZIP/tree | fetch-ffmpeg → importer | ZIP SHA pin; cached stamp alone does not rehash extracted tree; use fresh tree |
| Source + codec profile + Windows recipe → closure | configure/Make importer → Xmake | schema 2 target/toolchain/config/units; derivable, generated ignored state |
| Closure → SongCore archive | Xmake merge_archive → Rust sys build | Xmake target gate; old package consumes unreceipted artifacts-mingw; stop on identity drift |
| Header/archive + Cargo.lock → exe | Rust windows-gnu/MinGW → packager | sys build stages selected archive; old native local state not bootstrapped |
| Exe → DLL closure | objdump → packager | real PE system allowlist; stop before staging on non-system DLL |
| Exe + tracked docs → stage/manifest | package-windows → ZIP | old dirty status only recorded; self-hash impossible; stop on dirty source/content drift |
| Stage → ZIP/hash | zip/sha256sum → maintainer | old unversioned name; stdout-only digest; no independent existing-ZIP checker |
| Verified ZIP → workflow artifact | absent → absent | new read-only clean-checkout build needed |
| Artifact → Release | historical manual action → GitHub | existing historical asset remains read-only; future publication isolated |

## UNKNOWN

Original native archive hash cannot be verified from ZIP alone. No new Windows
physical-device/audio test is performed by this task. Byte reproducibility is
not established (UTC manifest time and toolchain-dependent binaries). Full new
CI bootstrap and tag execution require their own exact-HEAD execution records.

Audit commands: `gh issue view 171`, `gh release view qianqian-v0.1.0`,
`gh api repos/jnhu76/qianqian/releases/tags/qianqian-v0.1.0`,
`gh release download qianqian-v0.1.0 --pattern '*.zip'`, `git cat-file -t`,
`git rev-parse tag tag^{}`, Python zipfile/hashlib payload inspection and
`objdump -p` on the extracted historical PE. No historical external writes.
