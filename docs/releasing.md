# Releasing Qianqian

This process governs future product artifacts; it does not retroactively validate
or replace the published 0.1.0 asset. Playback semantics and SongCore ABI stay
unchanged. The [reality audit](audits/release-reality-audit.md) records the baseline.

## Frozen release contract (R1–R7)

1. **R1 Version:** `qianqian-vMAJOR.MINOR.PATCH` uses decimal SemVer components
   without leading zeros. Its version equals `[workspace.package].version`, an
   exact dated CHANGELOG release heading, and the package version. `songcore-v*`
   is never a product release. Tag objects and peeled commits are distinct.
2. **R2 Source:** no tracked/staged/nonignored untracked changes. HEAD must equal
   the expected 40-character source commit; formal mode also requires the tag's
   peeled commit to equal it. Check before and after building. Generated output
   belongs only in ignored paths or temporary directories. Do not edit source
   concurrently with a build.
3. **R3 Native:** prepare always extracts tracked native source into a fresh
   temporary directory, fetches/verifies the pinned FFmpeg ZIP, imports the
   Windows compile closure and builds SongCore with Xmake/MinGW. No personal
   prebuilt archive is accepted by prepare. Archive members must be x86_64 PE
   objects. The receipt binds source, pin/profile/recipe/header/closure/archive
   hashes and Rust/MinGW/Xmake versions. The staged archive is exactly the one
   Rust links through QIANQIAN_NATIVE_DIR. Windows recipe remains provisional;
   compiling is not device validation.
4. **R4 Contents:** exactly one versioned directory containing qianqian.exe,
   QUICKSTART.md, LICENSE, LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY_NOTICES.md,
   BUILD-MANIFEST.txt. No other executable, DLL, source, fixture, duplicate,
   symlink, traversal or alternate path is admitted. All real PE imports must
   be in the explicit Windows system/API-set allowlist. API-set prefixes alone
   cannot establish an OS dependency; only the two current product imports
   are approved, and any new import requires a reviewed contract update.
5. **R5 Integrity:** versioned ZIP, external SHA256 and provenance JSON are one
   delivery set. Manifest schema 2 is readable JSON in BUILD-MANIFEST.txt. It
   hashes the six payloads, never itself. Verification independently reads ZIP
   entries, hashes bytes, checks tracked documentation against the source
   commit, inspects PE imports and binds the external native receipt. The ZIP
   checksum protects the completed manifest. No checksum is a signature;
   delivery identity depends on the trusted source/workflow and sidecars.
6. **R6 Evidence:** RECIPE_RECONSTRUCTIBLE means a clean environment can rebuild
   the locked source/recipe. BUILD_VERIFIED requires actual build and checks.
   BYTE_REPRODUCIBLE requires two independent complete builds with equal bytes;
   it is not claimed here. Manifest time and unpinned host package patch levels
   can vary; actual tool versions are recorded.
7. **R7 Publication:** local/dispatch/PR runs cannot publish. Only tag push on
   this repository enters a separate contents:write job after all gates.
   Existing releases/tags/assets are never moved, edited or overwritten.
   The publisher verifies the remote peeled tag again and refuses historical
   0.1.0 and every existing Release (including drafts). API/tool errors stop.

Exit classifications: PASS=0, POLICY_RED=2 (valid input violates contract),
TOOLING_FAIL=3 (missing tool, malformed JSON/ZIP, execution exception),
BUILD_FAIL=4, EVIDENCE_MISSING=5. NOT_RUN is a reporting class, never a pass.

## Local preparation and verification

Use a clean Linux checkout with Python 3.11+, Git, curl, unzip/zip, binutils,
make, nasm, pkg-config, MinGW-w64 GCC/G++ and Rust windows-gnu target.
The workflow uses Ubuntu 24.04, Rust 1.97.1 and Xmake 2.9.7; install those for
matching build inputs. Fetch scripts validate the pinned FFmpeg source SHA.

```sh
rustup target add x86_64-pc-windows-gnu
# After committing all intended source changes:
source_sha=$(git rev-parse HEAD)
# Prospective version label: no tag required, no external writes.
bash tools/release-qianqian.sh prepare --mode dry-run \
  --tag qianqian-v0.1.0 --source "$source_sha"
# Existing ZIP verification does not compile FFmpeg:
python3 tools/check_release.py verify --mode dry-run \
  --tag qianqian-v0.1.0 --source "$source_sha" \
  --zip dist/qianqian-v0.1.0-windows-x86_64.zip
python3 tools/test_release.py
lefthook run pre-push --all-files
```

`prepare --mode formal` additionally requires an existing local product tag at
HEAD. Prepare and verify never create tags or releases. `publish` is not a
local wrapper operation. The native bootstrap it performs is:

```sh
# In a fresh temporary native tree extracted from the exact source commit:
python3 tools/ffmpeg_import.py --recipe windows-x86_64
xmake f -c -p mingw -a x86_64 -m release --mingw=/usr -y
xmake build songcore_static
```

Keep ZIP, `.zip.sha256`, and `.zip.provenance.json` together. Native receipt
checks against an available build tree can also be run using `native --help`.
Old schema-1 ZIPs can be inspected via zipfile/objdump as in the audit; the new
verifier intentionally refuses to certify them under the new contract.

## Review, tag and CI

Before tagging: update workspace version and dated CHANGELOG section; review
release notes, licenses/notices, local gate, clean-checkout native build, package
checks, all fault controls and applicable hosted CI. Obtain the relevant Windows
runtime/device evidence separately; label absent checks NOT_RUN. PATCH is a
reliability/UX corrective, MINOR a substantial user-facing capability/backend/UI,
MAJOR an incompatible contract. Do not invent a date for Unreleased.

After owner review, an authorized maintainer may create and push a **new**
annotated product tag. Never re-tag 0.1.0. Tag push checks out exact github.sha,
checks local peeled tag, rebuilds native dependencies, runs shared prepare/verify,
retains workflow artifact, and only then publishes a new Release using matching
CHANGELOG notes plus checksum/provenance/limitations. Build jobs have read-only
contents permissions; publish has write permission. No dispatch boolean grants it.

PR staging runs the full native/product/package path in a clean hosted checkout;
workflow_dispatch provides the same nonpublishing path after the workflow exists
on the default branch. PR staging success is not evidence that tag publication
executed. Existing Linux Rust, Windows Compile and Plugin Boundary workflows
retain their own trigger contracts; release staging also runs Linux regression.

## Failure and recovery

A failure stops before publication. Retain logs and artifacts, classify policy,
tooling, build or missing evidence, correct source in a new reviewed commit and
repeat. If a new tag was already pushed and its build failed, do not move it:
use a new version/tag after review. If release creation partially succeeded,
stop and report the state for owner recovery; this workflow refuses a second
attempt against the existing Release. Never automatically delete/edit published
versions or retry uploads with overwrite flags. Local dry-run can be repeated
in ignored dist/; its artifacts confer no publication authority.

No hosted build proves audible WASAPI playback. Windows device/audio, other
platform runtimes and byte-for-byte reproducibility need separate evidence.
