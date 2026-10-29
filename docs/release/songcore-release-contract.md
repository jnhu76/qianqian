# SongCore Release Contract

```text
TRUTH CLASS   OWNER-RATIFIED repository authority for the SongCore
              component release domain (Issue #172 S1 Release Contract
              Freeze). Finalized 2026-10-29 on branch
              songcore-172/pr-b-windows-release-lane from the S1 draft
              (docs/release/songcore-contract-draft.md): the content below
              is the draft as amended by reviewer rounds 1–2 — the state
              the campaign's contract review returned APPROVE on
              (REVISION 1/2 below and the §15 appendix record the applied
              deltas). Committed here, it is the written contract S2/S3
              campaign work implements and verifies against. Ratified
              2026-10-30 as repository authority by the explicit approval
              PR reviewed under Issue #330 — that PR is the owner adoption
              act under AGENTS.md routing; an issue comment does not
              change a truth class. The frozen clauses (§1–§13) are
              adopted as written; §14 (Owner 待决) still holds the
              decisions only the owner may take, and Q1 (next release
              version), Q4 (linux glibc symbol floor) and Q5 (publication
              mechanism) remain owner-reserved — ratification does NOT
              answer them.
FINALIZED     2026-10-29 (this file). Body identical to the S1 draft at
              REVISION 2 except: this header block, the §14 heading (now
              "Owner 待决 (openQuestions)"), and the §15 finalization
              record. No contract clause was changed in finalization.
REVISION 1    reviewer round 1 (2026-10-29) applied: `target`-key collision
              resolved via `target_identity`; checksums/files coverage
              corrected to "everything except checksums.txt itself";
              tools{}/sdk_ndk added for the #172 §5 toolchain+exact-version
              and SDK/NDK checklist items; B4 made machine-checkable;
              windows current-truth restated (full G2 scenario not yet
              executed there); symbol audit split per artifact form;
              binding ABI-check authority fixed (S-BIND §4.2 is a MAY; the
              MUST is imposed by this contract in G3); V3 extended with
              carried-duplicate byte-equality.
REVISION 2    reviewer round 2 (2026-10-29) applied: G4 now enumerates
              V1–V7 so the tools{}/sdk_ndk/target_identity.toolchain check
              is actually enforced at the release gate; §3 shared-form
              rule reworded to "…and nothing else is exported…" and the
              stray closing paren removed.
SUBJECT       the SongCore component release contract: component identity,
              versioning, ABI version, supported targets, capability
              profile, artifact kinds, package layout, provenance schema,
              runtime dependencies, license obligations, correctness
              gates, and the three reproducibility layers.
RED LINES     songcore.h stays the only ABI authority; packaging never
              modifies the API; songcore-v0.1.0 stays immutable; items
              still OPEN are listed in §14, not decided here.
```

Sources used while drafting (every claim below cites one of these or an
explicitly named command run on 2026-10-29):

```text
S-HDR    native/include/songcore.h                  (read in full)
S-BIND   docs/architecture/songcore-binding-architecture.md (read in full)
S-172    issue #172 body (gh issue view 172, full text)
S-173    issue #173 body (gh issue view 173, full text)
S-REL    docs/releasing.md                          (product contract, method source)
S-CHK    tools/check_release.py:100–178             (receipt/manifest binding method)
S-IMP    native/tools/ffmpeg_import.py:216–256, 355–415 (closure manifest schema 2)
S-LUA    native/build/songcore.lua, native/build/ffmpeg.lua, native/xmake.lua
S-TGT    native/ffmpeg/targets/{linux,windows}-x86_64.json
S-PRF    native/ffmpeg/profiles/codec-base.json
S-PIN    native/ffmpeg/pin.json
S-V010   published tarball songcore-v0.1.0-linux-x86_64.tar.gz, downloaded and
         extracted this session (sha256 018e0cbdfd8d83b6967bbba8e899ae6dbcc2ccf
         690c5294bd48e2ef2699f6815 — matches the release notes hash); its
         metadata/manifest.json, metadata/checksums.txt, metadata/symbols.txt read
S-325    evidence/windows-native-tier2-324/{README,ARTIFACT,ENVIRONMENT}.md
S-323    evidence/native-tier2-296/README.md
S-LIVE   symbol/format/path audit of the local verification-grade artifact
         native/build/artifacts/libsongcore.a, executed this session via WSL
         binutils (details inline where cited)
```

---

## 1. COMPONENT_NAME — `songcore` (frozen)

```text
component name        songcore
tag namespace         songcore-vX.Y.Z        (never qianqian-v*, never shared)
```

Grounding: the canonical header is `songcore.h` (S-HDR:1–3); the Xmake
basename is `songcore` → `libsongcore.a` / `libsongcore.so` / `songcore.dll`
(S-LUA songcore.lua `set_basename("songcore")`); the historical release tag
is `songcore-v0.1.0` (S-V010). `songcore-v*` is never a product release and
product tags are never SongCore releases (S-REL R1; S-172 §10).

## 2. VERSION_RULE

```text
tag form            songcore-vMAJOR.MINOR.PATCH — decimal components,
                    no leading zeros (convention reused from S-REL R1;
                    proposed here for the component namespace)
version identity    manifest.songcore_version == tag == package directory
                    name == Xmake project version (native/xmake.lua:2 pins
                    0.1.0 today; the release build must prove equality)
```

Four versions tracked separately, never collapsed (normative in S-BIND §10):

```text
SongCore semantic/API version      (header contract + binding-architecture doc)
SongCore ABI version               (SONGCORE_ABI_VERSION = 1; frozen)
component version                  (songcore-vX.Y.Z — this contract)
binding/package version            (crate / AAR / XCFramework — #173 issues)
```

Bump policy (from S-172 §10, frozen as a rule; the concrete next number is
NOT decided here — see §14):

```text
release-infrastructure / target-matrix additions with ABI v1 intact and the
    accepted capability profile unchanged   → 0.1.x patch may suffice
supported capability profile or component contract materially changed
                                            → MINOR bump
ABI v1 reopened                             → forbidden without evidence the
    C ABI cannot express the decode contract (S-BIND §10); that is ABI v2,
    not a version-rule matter
```

Immutability: `songcore-v0.1.0` (tag, release, assets) is the historical
Linux baseline and stays untouched (S-172 mission/§10). Published releases,
tags and assets are never moved, edited or overwritten; a failed release
releases under a new version, never a re-tag (method from S-REL R7).

## 3. ABI_VERSION — 1

```text
SONGCORE_ABI_VERSION   1u            (S-HDR:62)
header sha256          cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536
                       (sha256sum run this session on the working-tree file;
                       byte-identical to the hash shipped inside the v0.1.0
                       tarball manifest and to evidence S-325 ARTIFACT.md §2 —
                       the ABI header has not changed since v0.1.0)
export surface         exactly 15 symbols: songcore_abi_version,
                       song_open, song_probe, song_audio_stream_count,
                       song_audio_stream_info, song_select_stream,
                       song_get_metadata, song_get_metadata_count,
                       song_get_metadata_entry, song_get_artwork_count,
                       song_get_artwork_item, song_last_error,
                       song_read_pcm, song_seek, song_close
```

Rules frozen here:

```text
native/include/songcore.h is the ONLY ABI description (S-BIND §3). No
    generated Kotlin/Swift/Rust/JNI declaration becomes a second authority.
Packaging MUST NOT modify the header. Release verification re-hashes the
    packaged include/songcore.h against the source-commit copy (method:
    S-CHK check_release.py:120–121 "R3 native header mismatch").
Symbol audit is per artifact form:
    static — all 15 ABI v1 entry points are DEFINED and resolvable in the
        archive (wording of S-325 ARTIFACT.md §5; a static archive
        necessarily also defines non-ABI internal symbols, so an
        "exports exactly 15" check is meaningless here);
    shared — the EXPORTED set is exactly the 15 ABI symbols and nothing
        else is exported (S-LUA songcore.lua sets hidden visibility +
        -Wl,--exclude-libs,ALL for the shared build);
songcore_abi_version() == 1 is asserted at runtime by every gate (G1/G2).
    A binding MAY also check it at construction — an allowed adapter
    transformation (S-BIND §4.2 states MAY, not MUST); this contract
    itself imposes the construction-time check on bindings it ships (G3).
```

## 4. SUPPORTED_TARGETS

S1–S3 scope of this contract round (frozen):

```text
TARGET-ID          STATUS VOCABULARY      ROLE THIS ROUND
linux-x86_64       BUILD…RELEASE          S3: reference rebuild under the new
                                          release machinery (v0.1.0 remains
                                          untouched as the historical baseline)
windows-x86_64     BUILD…RELEASE          S2: first-class release target
```

Explicitly OUT of this round (each earns its own bounded child issue and
enters the matrix only after its maturity states are earned; S-172 §3/§11):

```text
linux-aarch64        (S4 — ARM64 native control before any mobile target)
android-arm64-v8a    (S5 native core / S6 JNI+Kotlin binding)
macos-arm64, ios-arm64 (S7, coordinated with #5; XCFramework = packaging)
windows-arm64        (S8)
wasm32-*             (Tier D; coupled to #5's production-role verdict)
```

Status is reported per `(target, binding)` pair with exactly the five states
`BUILD / LINK / CORE_RUNTIME / BINDING_RUNTIME / RELEASE` defined in
S-BIND §5, including its gatekeeping rules: cross-compile PASS is at most
BUILD; an archive is at most LINK; `CORE_RUNTIME` precedes any
`BINDING_RUNTIME`; `RELEASE` (this contract, §11 G4) is the release-matrix
entry. Current truth at draft time: linux-x86_64 has a published v0.1.0
release; windows-x86_64 has real-Windows consumption evidence but NOT
CORE_RUNTIME: what exists is a single-fixture C consumer smoke
(open/probe/metadata/full-decode/EOF/one seek/post-seek — S-325 ARTIFACT.md
§6), not the full S-BIND §7.1 13-step parity scenario (no stream
enumeration, no raw metadata enumeration, no artwork, no 25/50/75 seek
matrix, no malformed/truncated arm). Executing G2 on Windows, plus
packaging/provenance, is the S2 gap.

## 5. CAPABILITY_PROFILE — `codec-base` (frozen name, frozen semantics)

```text
profile name        codec-base
profile identity    sha256(native/ffmpeg/profiles/codec-base.json)
                    = 57fb70ed2c29a8464d939c611b3c5ed1bc99879438eb7f159530dfa52de554fa
                    (run this session; byte-identical to the profile hash
                    the v0.1.0 manifest shipped — the profile has not moved
                    since v0.1.0)
```

Frozen content (S-PRF): libraries avformat + avcodec + swresample enabled;
avdevice/avfilter/swscale disabled; programs/doc/debug/autodetect/x86asm/
network/protocols/devices disabled; demuxers aac, flac, mov, mp3, ogg, wav;
decoders aac, alac, flac, mp3float, opus, vorbis, pcm_u8, pcm_s16le,
pcm_s24le, pcm_s32le, pcm_f32le, pcm_f64le; parser mpegaudio. swresample is
a hard configure dependency of FFmpeg n9.0.1's native opus decoder (S-PRF
provenance note); SongCore itself never resamples and performs the
planar→interleaved Float32 conversion itself.

Frozen rules:

```text
Every supported target accepts THE SAME capability profile — targets differ
    in toolchain/closure/codegen, never in accepted profile, PCM meaning,
    seek/EOF/error contract (S-172 §2).
Every target derives its OWN FFmpeg closure from the pin + profile; a Linux
    object/source manifest is never replayed onto another target (S-172 §2;
    enforced at build time by the fail-closed manifest identity gate,
    S-LUA ffmpeg.lua manifest_target_problem: a mingw32 manifest can never
    satisfy a linux session and vice versa).
The profile sha256 is recorded in every target provenance (§8) and is one
    of the hashes release verification re-checks against the source commit.
```

## 6. ARTIFACT_KINDS

```text
KIND                 REQUIREMENT      FORM
static archive       MANDATORY        lib/libsongcore.a
                     (static-first)   ELF64 (linux) / COFF pe-x86-64 (windows)
shared library       OPTIONAL         lib/shared/libsongcore.so (linux) /
                                      songcore.dll (windows) — shipped ONLY
                                      where independently verified with the
                                      full gate set; never for symmetry
package container    one per target   songcore-vX.Y.Z-linux-x86_64.tar.gz
                                      songcore-vX.Y.Z-windows-x86_64.zip
                                      (naming per S-172 §4)
```

Static-first stays the policy (S-172 §8; v0.1.0 release notes: "Static
artifact is the primary supported consumption path"). Grounding for the
optional shared form: linux target json `shared: true`, windows target json
`shared: false, status: provisional` (S-TGT) — so this round windows ships
static-only unless the owner amends (§14). The merged-closure shape is
frozen as built: one self-contained archive (`build.merge_archive` policy,
S-LUA songcore.lua) whose members are the FFmpeg closure + songcore_ffmpeg.c
— 280 members on both targets so far (S-V010 manifest `archive_members: 280`;
S-325 ARTIFACT.md §1; S-LIVE `objdump -f` = 280 pe-x86-64, 0 elf). When the
form is statically closed, no standalone FFmpeg runtime library is shipped
or required (S-172 §3).

Non-goals for artifact kinds this round: fat/universal binaries, AAR,
XCFramework, wasm modules (S-172 §11/§12).

## 7. PACKAGE_LAYOUT

#172 §4 sketches `include/lib/BUILD-MANIFEST/SHA256SUMS/LICENSE/
THIRD-PARTY-NOTICES/README`. A more accurate existing specification exists —
the published v0.1.0 tarball — and this draft reuses it verbatim as the base
(authorized by the task: reuse the more accurate existing spec and state its
origin). Origin: S-V010, extracted and listed from the published asset this
session; its layout is:

```text
songcore-vX.Y.Z-<target>/                     (single versioned root directory)
├── include/
│   └── songcore.h                            (byte-identical to source commit)
├── lib/
│   ├── libsongcore.a                         (static primary — mandatory)
│   └── shared/
│       └── libsongcore.so                    (optional shared secondary)
├── LICENSES/
│   ├── COPYING.GPLv2                         (FFmpeg texts, from pinned source)
│   ├── COPYING.GPLv3
│   ├── COPYING.LGPLv2.1
│   ├── COPYING.LGPLv3
│   ├── THIRD_PARTY_NOTICES.txt
│   ├── LICENSE-MIT                           (schema-2 addition, §10 B4)
│   └── LICENSE-APACHE                        (schema-2 addition, §10 B4)
├── README.md
└── metadata/
    ├── manifest.json                         (provenance schema §8)
    ├── symbols.txt                           (the 15 ABI exports, sorted)
    ├── checksums.txt                         (sha256 of every packaged file)
    └── external-consumer-example.c           (the consumer-smoke example)
```

Mapping onto #172's sketch names (aliases, not a second layout):

```text
BUILD-MANIFEST      → metadata/manifest.json
SHA256SUMS          → metadata/checksums.txt
LICENSE / notices   → LICENSES/ (FFmpeg COPYING set + THIRD_PARTY_NOTICES.txt)
public header       → include/songcore.h
consumer smoke      → metadata/external-consumer-example.c
```

Frozen layout rules:

```text
Exactly one versioned root directory inside the container (S-V010 shape;
    no loose files beside it). Adapted from S-REL R4: no extra executable,
    DLL, source tree, fixture, duplicate, symlink, traversal or alternate
    path is admitted; the exact content set is the one above (plus schema-2
    additions may only ADD files under metadata/ and LICENSES/ — B4 adds
    the two component-license texts under LICENSES/ — never remove or
    rename the base set, so a v0.1.0-shaped consumer keeps working).
checksums.txt binds every packaged file EXCEPT itself (verified: the
    v0.1.0 package holds 13 files and its checksums.txt lists exactly the
    other 12, including manifest.json and symbols.txt — S-V010, recounted
    this session); the manifest in turn never hashes itself or
    checksums.txt (either would be circular).
Windows .zip container follows the same internal layout.
```

## 8. PROVENANCE_SCHEMA — metadata/manifest.json, schema_version 2

Base: the published v0.1.0 manifest is `schema_version: 1` (S-V010) — the
real SongCore provenance precedent, reused as the substrate. The product
release machinery independently validates the same binding ideas
(source/target/archive/header/pin/recipe/profile/closure hashes, S-CHK
:122–160), and the FFmpeg closure manifest is `schema: 2` (S-IMP :386–406).
Schema_version 2 for SongCore = v0.1.0's field set plus the additions below;
all existing v0.1.0 field names keep their meaning.

Carried from v0.1.0 schema_version 1 (verified field list, S-V010):

```text
schema_version, songcore_version, songcore_abi, abi_header_sha256,
target, platform, arch, toolchain, ffmpeg_version, ffmpeg_commit,
ffmpeg_source_sha256, ffmpeg_profile, ffmpeg_profile_sha256,
artifact, sha256, artifact_bytes, archive_members, artifact_shared,
shared_sha256, public_symbols[], license{...}, reproducibility{...}, git_sha
```

Key-collision rule (reviewer round 1): the carried string `target`
(v0.1.0: `"linux-x86_64"`) and the carried string `toolchain` (v0.1.0:
`"host-native"`) keep their v0.1.0 type and meaning unchanged; the
structured blocks use DIFFERENT names (`target_identity`, `tools`), so the
statement "all existing v0.1.0 field names keep their meaning" is literally
true. The closure manifest's own `target` object (built by
native/tools/ffmpeg_import.py:394, validated as an object by
tools/check_release.py:158–160) is a different document and is untouched.

Schema 2 additions (each bound to reality at packaging time):

```text
target_identity {
  id, platform, arch,
  triple              e.g. x86_64-unknown-linux-gnu / x86_64-pc-windows-gnu
  target_os, cross_prefix        (from the target recipe; fail-closed
                                  identity, S-LUA ffmpeg.lua)
  toolchain { family, identity } (from the target recipe, S-TGT — the
                                  #172 §5 "toolchain family" checklist
                                  item; EXACT tool versions live in tools{})
  recipe_sha256                  sha256 of native/ffmpeg/targets/<id>.json
}
tools { cc, ar, xmake: "<exact first line of --version>" }
                                    #172 §5 "toolchain family + exact
                                    version" for the RELEASE artifact's own
                                    build tools, mirroring the receipt
                                    precedent tools/check_release.py:131–133
sdk_ndk "not applicable this round — windows-x86_64: mingw-w64 cross, no
                                    MSVC/SDK (S-TGT windows toolchain
                                    identity); linux-x86_64: host-native,
                                    no SDK/NDK (S-TGT)" — the #172 §5
                                    "SDK/NDK version where applicable"
                                    item is visibly dispositioned, not
                                    silently omitted
source { commit, status: "clean" }          (S-REL R2 discipline)
pin { ffmpeg_tag, ffmpeg_commit_sha, ffmpeg_source_sha256,
      pin_sha256 }                          (S-PIN; binding method S-CHK :153–156)
closure {
  manifest_sha256     sha256 of build/ffmpeg-xmake/manifest.json as imported
  toolchain { system, machine, cc, cc_ident, arch, target_os }   (S-IMP :216–230)
  configure_args[]    the exact FFmpeg configure argument list (S-IMP :170–195,
                      :398) — machine-checkable license gate, §10
  units { translation_units, upstream_sources, generated_sources }
  reference_archives { <lib>: {bytes, sha256} }                  (S-IMP :377–382)
}
files { <packaged-path>: {sha256, bytes} }  every packaged file EXCEPT
                                            checksums.txt itself (the same
                                            set checksums.txt covers) — this
                                            is the binding to real
                                            release-file SHA256s; must equal
                                            checksums.txt line for line
runtime_dependencies { per §9, per target }
license { ffmpeg_effective, gpl_enabled, version3_enabled, nonfree_enabled,
          external_libs[], license_files { COPYING.*: sha256 } }
reproducibility {
  recipe:         "RECIPE_RECONSTRUCTIBLE"      (level vocabulary, §12)
  semantic_abi:   "SEMANTIC_ABI_VERIFIED" | "NOT_ESTABLISHED"
  byte:           "BYTE_REPRODUCIBLE" | "NOT_ESTABLISHED"
  evidence:       <path/hash of the two-build comparison receipt, §12>
}
built_utc, packer ("release-substrate <identity>"), notes
```

Verification contract for the manifest (all machine-checkable at release
time, adapted from S-CHK):

```text
V1  every files[] entry re-hashes to the actual packaged bytes; the set of
    files[] keys == the set of checksums.txt entries == the §7 content set
    MINUS checksums.txt itself (checksums.txt hashes manifest.json and the
    other payloads but not itself; the manifest hashes neither itself nor
    checksums.txt — verified shape: 13 package files, 12 checksum entries)
V2  abi_header_sha256 == sha256(packaged include/songcore.h) == sha256 of
    the header at the source commit
V3  pin.* and closure.* agree with native/ffmpeg/pin.json and the imported
    closure manifest at the source commit (tag→commit + zip sha256
    identity); carried duplicates are byte-equal to pin.* —
    manifest.ffmpeg_commit == manifest.pin.ffmpeg_commit_sha ==
    pin.json.ffmpeg_commit_sha and manifest.ffmpeg_source_sha256 ==
    manifest.pin.ffmpeg_source_sha256 == pin.json.source_sha256 (drift
    between the two spellings is a failure)
V4  public_symbols[] == metadata/symbols.txt == the 15 ABI symbols per the
    §3 per-form audit (static: 15/15 resolve; shared: exported set == 15)
V5  license.gpl/version3/nonfree all false and configure_args contain no
    --enable-gpl/--enable-version3/--enable-nonfree/--enable-lib* (§10)
V6  reproducibility claims point at existing evidence receipts; an absent
    receipt forces the field to NOT_ESTABLISHED (NOT_RUN is a reporting
    class, never a pass — S-REL exit-class discipline)
V7  tools{} present with non-empty exact version strings for cc, ar, xmake
    and sdk_ndk stating its disposition; target_identity.toolchain equals
    the target recipe's toolchain block at the source commit
```

## 9. RUNTIME_DEPENDENCIES

Per-target, recorded in manifest.runtime_dependencies and verified by named
commands. Static-first means these are CONSUMER LINK-TIME requirements of
the shipped archive, not shipped runtime blobs.

### windows-x86_64 (static libsongcore.a, mingw-w64 closure)

```text
bcrypt.lib           REQUIRED. Audited this session (S-LIVE): nm on the
                     280-member pe-x86-64 verification artifact shows the
                     undefined imports BCryptOpenAlgorithmProvider,
                     BCryptGenRandom, BCryptCloseAlgorithmProvider
                     (FFmpeg's Win32 entropy path). The real C consumer
                     smoke linked with -lbcrypt (S-325 ARTIFACT.md §6).
mingw-w64 runtime    msvcrt-family CRT + Win32 system imports (observed
                     undefined set includes SRW-lock/condition-variable,
                     CloseHandle, CreateFileMappingA, GetStdHandle, … —
                     S-LIVE sample). System-DLL allowlist discipline is
                     already codified for the product exe (S-CHK :25–26
                     SYSTEM_DLLS incl. bcrypt, bcryptprimitives, msvcrt);
                     the same list is the audit base for any SongCore
                     shared/DLL form.
link line            gcc … libsongcore.a -lbcrypt (nothing else observed
                     required for the static archive)
```

Audit method (frozen): `nm <archive>` undefined-symbol listing for static;
`objdump -x <dll>` import walk against the explicit system allowlist for any
shared form; member-format audit `objdump -f` (280/280 pe-x86-64, 0 elf).
A new non-system import requires a reviewed contract update (S-REL R4 rule,
adopted here). rustc's windows-gnu runtime separately references BCrypt*
(see tools/package-windows.sh:67–72) — that is a Qianqian-consumer concern,
listed here because it is the audited precedent for the same dependency.

### linux-x86_64 (static libsongcore.a / shared libsongcore.so)

```text
libc (glibc)         REQUIRED (both forms)
libm, libpthread     REQUIRED (S-LUA songcore.lua: add_syslinks("m","pthread")
                     for linux/macosx/android/iphoneos)
link line            gcc … libsongcore.a -lm -lpthread
shared form extra    record the glibc symbol-version floor in schema 2:
                     objdump -T libsongcore.so | GLIBC_* max version.
                     v0.1.0 did not record it; the floor POLICY (which
                     distro family is supported) is an owner decision — §14.
```

Not shipped / not required when statically closed: any FFmpeg runtime
library (S-172 §3). No target may add a runtime dependency that changes ABI
semantics; new dependencies enter through a reviewed contract update.

## 10. LICENSE_OBLIGATIONS

```text
SongCore component code   the repository's own dual license (MIT OR
                          Apache-2.0 — repo-root LICENSE; product packages
                          ship LICENSE-MIT + LICENSE-APACHE, S-REL R4).
FFmpeg closure            effective LGPL-2.1-or-later, because the frozen
                          codec-base profile keeps gpl/version3/nonfree off
                          and pulls no external lib* copylibs (v0.1.0
                          manifest license block, S-V010; profile content
                          §5). v0.1.0 shipped exactly the four FFmpeg
                          COPYING texts + THIRD_PARTY_NOTICES.txt (S-V010).
```

Obligations frozen for every future release (LGPL static-link duties):

```text
O1  LICENSES/ carries the FFmpeg COPYING.GPLv2/GPLv3/LGPLv2.1/LGPLv3 texts
    extracted from the VERIFIED pinned FFmpeg source zip (the pin binds
    tag→commit + zip sha256, S-PIN), not hand-maintained copies.
    Machine check: sha256 each shipped text against the pinned-source
    extraction; record in manifest.license.license_files.
O2  THIRD_PARTY_NOTICES.txt present and hashed in checksums.txt.
O3  Relink obligation: the shipped archive contains the FFmpeg members in
    object form, and the repository provides the machine-verifiable rebuild
    inputs (pin.json + profile + target recipe + closure manifest + xmake
    config) that a licensee can recombine — the README must state this
    relink path explicitly (v0.1.0 release notes already advertise the
    same facts).
O4  Profile stability gate: manifest.configure_args must not contain
    --enable-gpl / --enable-version3 / --enable-nonfree / --enable-lib*
    (schema check V5). Enabling any of them, or any new external library,
    invalidates the effective-license designation and requires a NEW
    reviewed license analysis before release.
```

RELEASE_BLOCKED conditions (machine-detectable):

```text
B1  any license text missing or its sha256 ≠ pinned-source extraction
B2  V5 fails (gpl/version3/nonfree/lib* enabled without a new analysis)
B3  THIRD_PARTY_NOTICES.txt absent from the package or unhashed
B4  LICENSES/LICENSE-MIT or LICENSES/LICENSE-APACHE missing, or either
    file's sha256 ≠ the repo-root text at the source commit — machine-
    checkable exactly like O1/O2: the two repo-root texts ship under
    LICENSES/ (schema-2 addition to the §7 content set) and are hashed in
    checksums.txt/files{}. Background, verified this session: the v0.1.0
    THIRD_PARTY_NOTICES.txt states SongCore's sources carry "no open-source
    license yet; distribution terms are owned by the project" — B4 closes
    that gap for every future release with a checkable pass/fail instead
    of an aspirational notice statement.
```

Limit stated honestly: machine checks verify presence, hash identity and
configure-arg provenance; they cannot certify legal sufficiency. Each
release's license block is owner-reviewed (standing process note, not an
open decision).

## 11. CORRECTNESS_GATES

One-to-one with the #173 maturity ladder (S-BIND §5). A target enters the
release matrix only when every gate below has executed evidence for that
target; states lower on the ladder never masquerade as higher ones.

```text
G0  BUILD (= BUILD)
    The target recipe produced the artifact through the pinned import:
    ffmpeg_import.py verifies tag→commit + zip sha256 (S-PIN verification
    block) then freezes the closure; xmake replays it behind the fail-closed
    identity gate (a wrong-target manifest cannot satisfy the session,
    S-LUA ffmpeg.lua). Evidence: artifact sha256 + bytes; member audit
    objdump -f (linux → all members elf64-x86-64, windows → all
    pe-x86-64, 0 foreign); provenance recorded per §8.
G1  LINK (= LINK)
    A minimal C consumer (metadata/external-consumer-example.c class)
    compiles against include/songcore.h and links the artifact;
    songcore_abi_version()==1 at runtime. Symbol audit per form (§3):
    static — 15/15 ABI v1 entry points resolve in the archive; shared —
    the exported set is exactly the 15 ABI symbols. Windows link line
    includes -lbcrypt (§9). Executed commands are recorded (precedent:
    S-325 ARTIFACT.md §6).
G2  CORE_RUNTIME (= CORE_RUNTIME)
    The direct C consumer executes the binding-parity scenario (S-BIND
    §7.1 steps 01–13) on the target's real runtime against the canonical
    corpus native/experiments/songcore-equivalence/{fixtures,reference.json}
    — no second corpus, no second reference numbers (S-BIND §7.3): open →
    probe → stream enumeration → metadata (canonical + raw enumeration) →
    artwork where provided → full-decode PCM (exact SHA-256 for lossless/
    PCM fixtures; frozen deterministic tolerance/hash for lossy) → seek
    25/50/75% (status class, landing or -1 sentinel, next-read-lands,
    metadata/artwork/format unchanged) → EOF/drain (SONG_EOF terminal,
    produced == 0, distinct from error) → malformed/truncated (typed
    fail-closed statuses, no fabricated data) → close/lifetime cleanup.
    Cross-compilation can never produce this state (S-BIND §5).
G3  BINDING_RUNTIME (= BINDING_RUNTIME)
    For each binding shipped for the target: the SAME §7 scenario through
    the binding, matching the direct-C reference on the frozen comparison
    set (status/error class, units, sentinels, source facts, PCM format +
    reference policy, seek landing contract, EOF, ownership/lifetime).
    This contract additionally requires each binding it ships to check
    songcore_abi_version()==1 at construction — imposed HERE by this
    contract's release-gate authority; S-BIND §4.2 lists the check as an
    allowed adapter transformation (MAY), and #173 does not mandate it.
    Current bindings: qianqian-songcore-sys (raw) + qianqian-decode-songcore
    (adapter) are the reference path (S-BIND §6). Not required when no
    binding ships for the target.
G4  RELEASE (= RELEASE)
    Package gates: exact content set (§7); manifest schema-2 verification
    V1–V7 (§8); checksums.txt ↔ files[] equality; symbols.txt audit; license
    gates O1–O4 / B1–B4 (§10); runtime-dependency audit fresh for this
    artifact (§9); every artifact independently downloadable; historical
    releases untouched; per-target status reported in the five-state
    vocabulary with target/toolchain/bounds/result-class (AGENTS.md
    Verification section: never report an unrun check PASS).
```

Gate-to-issue mapping: G0–G2 + provenance are #172's release-matrix
evidence (this contract); G3's oracle is defined by S-BIND §7; G4 is
#172's RELEASE state. A produced archive is not a release (S-BIND §5).

## 12. Reproducibility — three layers, definitions and judgment

Vocabulary origin: S-REL R6 uses RECIPE_RECONSTRUCTIBLE / BUILD_VERIFIED /
BYTE_REPRODUCIBLE for the product; this contract defines the SongCore
component's three layers and their judgment methods. The ask's rule is
followed: root causes are verified before any flag is proposed.

### L1 recipe reproducible (RECIPE_RECONSTRUCTIBLE)

```text
Definition   a clean environment (fresh checkout/extract at the locked
             source commit, pinned FFmpeg zip, the target recipe) can
             rebuild the artifact and pass G0–G2 using only documented
             commands — no CI-only steps, no personal prebuilts (S-REL R3).
Judgment     execute the documented sequence end-to-end in a fresh tree;
             evidence = full gate receipts + environment identity. Actual
             tool versions recorded (S-REL R6: unpinned patch levels may
             vary; they are recorded, not hidden).
```

### L2 semantic-ABI reproducible (SEMANTIC_ABI_VERIFIED)

```text
Definition   two independent build directories (different absolute roots,
             same source commit / pin / profile / recipe) yield artifacts
             that are observably the same component:
               a) same ABI symbol audit result per form (§3: static —
                  15/15 entry points resolve; shared — exported set is
                  exactly the 15 ABI symbols)
               b) same closure identity from the same manifest (TU count,
                  member origins; reference_archives may legitimately
                  differ per build)
               c) both pass G2 with IDENTICAL observable results — per-
                  fixture PCM hashes, seek landings per policy, status
                  classes, metadata/artwork facts
               d) same runtime-dependency surface (§9 audits)
             Bytes may differ.
Judgment     run the full G0–G2 set on BOTH artifacts and diff the
             observable outputs; the comparison receipt records both
             artifact hashes and the per-check parity table.
```

### L3 byte reproducible (BYTE_REPRODUCIBLE)

```text
Definition   two independent complete builds (different absolute roots)
             produce byte-identical artifacts (equal sha256 for every
             shipped library).
Judgment     sha256 equality across the two roots, recorded member-wise.
```

### Verified root cause (evidence, this session)

```text
S-LIVE-1  the local verification-grade windows artifact
          native/build/artifacts/libsongcore.a
          (sha256 408d7d6ea4c9d1707283104366725c3deda47d5a4cebee5102c170fdf86d404b,
          3,309,982 bytes, objdump -f: 280 pe-x86-64 members, 0 elf) embeds
          builder-side absolute paths. grep -ao over the archive found:
            /home/hoo/Source/qianqian/native/build/ffmpeg-src/libavcodec/bytestream.h   5 lines
            /home/hoo/Source/qianqian/native/build/ffmpeg-src/libavutil/iamf.h          2 lines
            /home/hoo/Source/qianqian/native/build/ffmpeg-src/libavcodec/put_bits.h     2 lines
          — i.e. paths under the build-tree FFmpeg source root
          (source_root: "build/ffmpeg-src" in the closure manifest, S-IMP),
          rooted at the checkout location.
S-LIVE-2  the same artifact's sha256 differs from the #325-recorded build
          (90bde1ca…, 3,309,678 bytes — S-325 ARTIFACT.md §1) while the
          member count/format family match: a rebuild was not byte-stable.
S-323     evidence/native-tier2-296/README.md records the same phenomenon
          and its boundary: "A fresh rebuild in a different checkout can
          produce a different libsongcore.a archive hash because the merged
          FFmpeg objects embed absolute source paths… songcore_ffmpeg.c.o
          is byte-identical between builds."
S-325     evidence/windows-native-tier2-324/ARTIFACT.md §7: "absolute build
          paths are embedded in debug info; the C source hash is the stable
          identity" — reproducibility explicitly not claimed for that build.
```

Conclusion: the byte-instability is real, attributable, and located in the
merged FFmpeg objects (path-bearing debug/assert data), not in SongCore's
own C gate. v0.1.0's manifest claims `byte_identical: true`, but its
release notes scope that claim to "byte-identical across forced rebuilds"
— same-environment rebuilds; cross-root byte stability was never
established historically and must be re-earned under the new machinery.

### Test design for S2/S3 (two independent build directories)

```text
T1  Build the same source commit twice into two roots R1 ≠ R2 (fresh
    extract each; same pin/profile/recipe).
T2  Member-wise diff: ar x each member of both archives into scratch
    directories and sha256 each extracted member. (Method note from
    S-325 ARTIFACT.md §1: `ar p <archive> <member> | sha256sum` is NOT
    binary-safe in this Git Bash — use ar x.)
T3  Classify each differing member; for each, locate the differing bytes
    (strings/objdump --debugging) and attribute a root cause (embedded
    path, timestamp, nondeterministic ordering, …). Record the table.
T4  Only then, one candidate at a time, re-run T1–T3 and keep a change
    only if it demonstrably eliminates a measured difference class:
      - compiler path remap for the replayed FFmpeg compiles and the
        SongCore C compile: -ffile-prefix-map=<root>=. (or
        -fdebug-prefix-map) — note the import manifest already normalizes
        its OWN recorded paths to @SRC@/@BUILD@ (S-IMP :249–256); the
        replayed compiles do not remap what they embed.
      - deterministic archive mode (GNU ar D / deterministic-by-default
        binutils; verify which ar invocation xmake actually uses) for
        member timestamps/uid/gid/mode.
      - SOURCE_DATE_EPOCH only if generated-file timestamps show up as a
        measured difference class.
    A flag that changes no measured differing byte is DROPPED — no
    decorative parameter pile-up.
T5  Every accepted change must leave G0–G2 green on both roots (the
    gates, not the diff, decide semantic identity).
T6  Record the L1/L2/L3 verdicts into manifest.reproducibility (§8) with
    the comparison receipt path; NOT_ESTABLISHED until T1–T5 pass for
    that layer on that target.
```

Honest scope note: this contract freezes the three definitions, their
judgment methods and the T1–T6 design. The two-root comparison itself has
NOT been executed in this drafting session and is S2/S3 execution work.

## 13. Release versioning & immutability (summary of §2)

`songcore-v0.1.0` is frozen history. The next component release is produced
by the substrate defined here (target recipes → gates → package →
manifest/checksums → publication), its tag is decided by the owner after
the S2+S3 matrix is frozen (§14 Q1), and publication never touches existing
releases/tags/assets.

## 14. Owner 待决 (openQuestions — OPEN is not agent-choice)

```text
Q1  Next release tag/version number. #172 §10 reserves this decision until
    the supported matrix is frozen; this contract freezes only the RULE.
Q2  Debug-info policy for shipped artifacts: keep debug info and pursue
    cross-root byte stability via -ffile-prefix-map (T4), or ship
    stripped/-g0 artifacts (simpler byte stability, loses debuggability).
    Changes shipped bytes either way; needs owner sign-off.
Q3  windows-x86_64 shared-DLL form this round: target json says
    shared=false (provisional); a DLL needs the -lbcrypt link + the
    R4-style import audit (§9). Ship static-only (proposed default) or
    add the shared kind?
Q4  linux glibc symbol-version floor / oldest supported distro family for
    the shared .so (measurement method frozen in §9; the support policy
    itself is not).
Q5  songcore-v* publication mechanism: v0.1.0 was published manually; the
    product side has a tag-push publisher with R7 protections. Does
    SongCore get a workflow with equivalent no-overwrite/refuse-history
    protections, or stay manual with G4 as the written gate?
```

## 15. Drafting-session evidence appendix (what was actually run/read)

```text
Read:  issue #172, issue #173 (gh issue view via WSL gh route, full bodies)
Read:  native/include/songcore.h; docs/architecture/songcore-binding-architecture.md
Read:  docs/releasing.md; tools/check_release.py:100–178;
       native/tools/ffmpeg_import.py (manifest/toolchain/rewrite sections);
       native/build/songcore.lua; native/build/ffmpeg.lua (identity gate);
       native/xmake.lua; native/ffmpeg/{pin.json, profiles/codec-base.json,
       targets/linux-x86_64.json, targets/windows-x86_64.json}
Read:  evidence/windows-native-tier2-324/{README,ARTIFACT}.md;
       evidence/native-tier2-296/README.md
Ran:   gh release view songcore-v0.1.0            → release notes (asset, hashes)
Ran:   gh release download songcore-v0.1.0 …      → tarball fetched; sha256
       018e0cbd…6815 matches the notes; layout + manifest.json + checksums.txt
       + symbols.txt read after extraction (S-V010)
Ran:   sha256sum native/include/songcore.h        → cf509a5c…78536
Ran:   sha256sum native/ffmpeg/{profiles,pin,targets} inputs (§5, §8 values)
Ran:   WSL nm on local libsongcore.a              → BCrypt* undefined set (§9)
Ran:   WSL objdump -f / sha256sum / grep -ao on local libsongcore.a →
       280 pe-x86-64 / 408d7d6e… / embedded /home/... paths (S-LIVE-1/2)
Ran:   revision round 1 (2026-10-29): re-downloaded songcore-v0.1.0 and
       grep -in license on LICENSES/THIRD_PARTY_NOTICES.txt → line 24
       "no open-source license yet; distribution terms are owned by the
       project" (B4 background); recounted the package: 13 packaged files
       (tar -tzf | grep -vc /$) vs 12 checksums.txt entries (awk NF|wc -l)
       — the V1/§7 coverage wording rests on these counts
Ran:   revision round 2 (2026-10-29): grep sweep of this draft — the
       release-gate manifest-verification range reads V1–V7 (G4) and no
       stale six-rule range reference remains anywhere; the §3 shared-
       form rule reads "nothing else is exported" with balanced parens
Not run (reported as such): the L1–L3 two-root reproduction comparison
       (T1–T6) — that is S2/S3 execution, not drafting.
```

### Finalization record (2026-10-29, S2 build-lane session)

```text
Finalized as docs/release/songcore-release-contract.md on branch
       songcore-172/pr-b-windows-release-lane (base: PR-A HEAD bf23516)
       from this draft's REVISION 2 state, per the campaign contract
       review round that returned APPROVE. Header block restated (TRUTH
       CLASS / FINALIZED), §14 heading restated as the Owner-待决 section
       (Q1–Q5 remain owner-reserved); no clause changed.
Independently re-run during finalization (same session, Git Bash on the
       worktree): sha256sum native/include/songcore.h → cf509a5c…78536
       (§3 value re-confirmed); sha256sum
       native/ffmpeg/profiles/codec-base.json → 57fb70ed…54fa (§5
       re-confirmed); sha256sum native/ffmpeg/targets/windows-x86_64.json
       → 3ba0a9e6…8492d (== the closure manifest's
       target.recipe_sha256, §8 re-confirmed); sha256sum
       native/ffmpeg/pin.json → 224ad3d5…ff85; sha256sum
       native/src/songcore_ffmpeg.c → 55eab243…05ad6 (the S-325 stable
       C-source identity).
Still NOT_RUN: the L1–L3 two-root comparison (T1–T6). S2's build lane
       (ops/build-rc.ps1, -BuildDir) produces the second independent root
       for that comparison; the diff/classification itself (T2–T5) and
       the L1/L2/L3 verdicts remain S2/S3 execution work.
```
