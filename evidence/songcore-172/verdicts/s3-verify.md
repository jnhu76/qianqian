# S3 verdict — Linux lane verification, packaging, reproducibility (#172)

```text
TRUTH CLASS   Execution evidence for the songcore-172 S3 verify lane. Every
              claim below was executed during this session (2026-10-29, WSL
              Ubuntu 26.04.1 / registered Ubuntu-24.04, kernel
              6.18.54.1-microsoft-standard-WSL2, gcc 15.2.0, xmake v2.9.7,
              cargo/rustc 1.97.1) against the S3 build artifacts
              (libsongcore.a 409ef3da…, libsongcore.so 5e9c5697…,
              libqianqian_av.a ff6cacec…, closure manifest d037e8b4…) built
              from songcore-172/pr-a-build-fixes @ bf23516.
VERDICT       PASS for the executed S3-verify scope (C consumer suite,
              Rust binding parity, contract-shaped packaging with V1-V7,
              two-root reproducibility T1-T3, external unpack consumer).
              NOT_RUN / NOT_ESTABLISHED items are listed explicitly in §7.
```

## 1. C consumer suite (G1/G2 evidence)

Technique provenance (no second corpus, no invented semantics): the canonical
corpus `native/experiments/songcore-equivalence/{fixtures,reference.json,
harness.c,run.py}` (S-BIND §7.3) plus the extension scenarios from S-BIND
§7.1 steps 01-13 and §8.2-§8.11; the corruption-window variant mirrors the
committed Rust fault technique (`e1_abi_tests.rs` disturb, applied to FLAC as
there). Suite sources: `ops/verify-suite/{suite_ext.c,run_suite.py}` (synced
to `evidence/songcore-172/linux/verify/suite/`).

```text
canonical gates   python3 native/experiments/songcore-equivalence/run.py
                  --skip-perf → exit 0:
                  ABI export gate PASS (exactly 15 exports, zero av_/ff_/swr_
                  leaks in exports and header, shared NEEDED ⊆ {libc,libm}),
                  artifact gate PASS (280 members), correctness PASS 4/4 —
                  frames + pcm_sha256 + terminal bit-exact vs the frozen
                  reference.json for mp3-cbr-id3v23, flac-16-44-stereo,
                  alac-16-44-stereo, alac-long (the frozen policy is an exact
                  deterministic hash for lossy as well; it matched exactly).
extension suite   4/4 fixtures: open, probe (+idempotent), stream enumeration
                  + default-stream identity + invalid-index negatives,
                  metadata canonical presence rules + raw enumeration (scope
                  domain, duplicate preservation recorded), artwork
                  invariants, read_capacity_zero (valid dst, cap 0 →
                  SONG_ERR_INVALID_ARGUMENT, produced==0), full decode (PCM
                  sha == reference), EOF stability (repeated SONG_EOF,
                  produced==0, distinct from error), seek 25/50/75% per
                  §8.9 (status SONG_OK; landings at/before target, e.g.
                  alac-16: 928798/1950476/2972154 vs targets
                  1000000/2000000/3000000; lossless byte-accurate
                  continuation vs the sequential-decode PCM; frame
                  accounting; metadata/artwork/info invariance; mp3
                  contract-class only), truncated 50%/4KB/empty/corrupted
                  variants (typed fail-closed statuses + no-fabrication
                  prefix property), lifetime (mid-stream close → reopen →
                  byte-identical decode; song_close(NULL) no-op),
                  open-failure status classes (typed status, no handle
                  published). Receipt: c-suite-receipt.json (verdict PASS,
                  frozen_reference_match 4/4).
```

## 2. Rust binding parity (G3 evidence)

Isolation (no pollution): cargo ran with `CARGO_HOME=/home/hoo/.cargo-s3`
(fresh registry cache) from a cwd whose ancestry is entirely under /home/hoo —
verified: zero "/mnt/c" occurrences in cargo output (the /mnt/c
~/.cargo/config warning only fires when the cwd chain crosses
/mnt/c/Users/fred1). Proxy env used for the crates.io downloads the empty
registry needed; both crates built `--locked` from their committed
Cargo.locks.

```text
qianqian-songcore-sys     cargo test --locked → exit 0; build+link evidence
(raw, layer 4)            (0 tests by design — its gate is build.rs's
                          fail-closed provenance hashing + link against the
                          Linux artifact; lib.rs carries the ABI constants
                          and extern block).
qianqian-decode-songcore  cargo test --locked → exit 0; 36 passed + 2 passed
(adapter, layer 5)        (0 failed), including the S-BIND §7 scenario tests
                          (healthy_control_reproduces_the_reference_pcm_identity,
                          partial-success-then-latched-fatal,
                          applied_seek_erases_the_latch…, refused-seek,
                          destructive-seek-failure, close-after-latched-fatal,
                          open-failure, typed-fault-to-outlet) — the SAME
                          canonical corpus and the SAME reference.json frozen
                          fields as the C suite (tests/common/mod.rs loads
                          reference.json directly).
layout gate               experiments/songcore-call-comparison
                          `cargo run --release -- layout` → "layout PASS":
                          the C probe and Rust probe (every crossing
                          struct's size/align/offset, every constant)
                          compared line-for-line — identical.
surface gate              `cargo run --release -- surface` → PASS 4/4: all
                          15 symbols executed from C and Rust per fixture;
                          statuses, select results, seek landings
                          (e.g. 496327/496327 µs) and first-frame counts
                          (1024/1024) identical C vs Rust.
Parity basis              same media batch (4 canonical fixtures), same
                          frozen comparison fields (rate/channels/codec/
                          frames/pcm_sha256/terminal/status classes/seek
                          landing), C suite green + Rust suite green +
                          committed C↔Rust equality gates green.
```

## 3. Packaging (contract §7 PACKAGE_LAYOUT + §8 PROVENANCE_SCHEMA)

```text
package          ops/package/songcore-v0.0.0-s3-verify-linux-x86_64.tar.gz
sha256           d33f6762739d1fd5220f367dbe11022ced927c345572fa25328454784e97fcbe
bytes            2,593,935
content set      15 files, exactly the §7 shape (include/songcore.h;
                 lib/libsongcore.a; lib/shared/libsongcore.so; LICENSES/ with
                 the 4 FFmpeg COPYING texts extracted from the VERIFIED
                 pinned source + THIRD_PARTY_NOTICES.txt taken read-only from
                 the published songcore-v0.1.0 asset [download sha256
                 018e0cbd…6815 == the recorded historical hash] + LICENSE-MIT
                 + LICENSE-APACHE byte-identical to repo-root texts [B4];
                 README.md; metadata/manifest.json, symbols.txt,
                 checksums.txt, external-consumer-example.c)
manifest         schema_version 2 with the full carried v0.1.0 field set +
                 schema-2 additions (target_identity with recipe_sha256,
                 tools{cc,ar,xmake} exact versions, sdk_ndk disposition,
                 source{commit,clean}, pin{…,pin_sha256}, closure{manifest_
                 sha256 d037e8b4…, toolchain, configure_args, units,
                 reference_archives}, files{}, runtime_dependencies (static
                 link line + shared NEEDED + measured GLIBC_2.43 symbol-
                 version floor, support policy owner-reserved §14 Q4),
                 license block (LGPL-2.1-or-later, gpl/version3/nonfree
                 false, license_files hashes), reproducibility (recipe
                 RECIPE_RECONSTRUCTIBLE, semantic_abi SEMANTIC_ABI_VERIFIED,
                 byte NOT_ESTABLISHED + evidence paths), built_utc, packer.
V1-V7 + O1/B4    ALL PASS (package-v-checks.json): files re-hash; checksums
                 covers the 14 non-checksums files (v0.1.0 rule shape:
                 everything except itself); manifest files[] covers the same
                 set minus manifest.json itself (non-circular; recorded
                 differential to §8's "same set" wording); header sha ==
                 packaged == source-commit copy (cf509a5c… value class);
                 pin/carry duplicates byte-equal; public_symbols ==
                 symbols.txt == 15 == shared export set == static defined
                 set; no --enable-gpl/version3/nonfree/lib* in
                 configure_args; repro fields point at existing receipts;
                 tools non-empty, sdk_ndk dispositioned, target_identity ==
                 recipe toolchain block; license texts hash-equal to the
                 pinned extraction and repo-root texts.
VERSION NOTE     songcore-v0.0.0-s3-verify is a NON-PUBLISHABLE placeholder:
                 the concrete next release number is owner-reserved
                 (contract §14 Q1). This archive is campaign evidence, NOT a
                 release (contract §11); it must never be published under
                 songcore-v*.
```

## 4. External unpack consumer (/tmp, fresh)

```text
unpack           /tmp/s3-external/… (fresh dir; tar -xzf)
content set      15 files exact
self-verify      sha256sum -c metadata/checksums.txt → 14/14 OK (exit 0)
static build     gcc -O2 -Iinclude metadata/external-consumer-example.c
                 -Llib -lsongcore -lm -lpthread → exit 0; consumer NEEDED =
                 libm.so.6, libc.so.6 only (no libsongcore — truly static)
shared build     gcc … -Llib/shared -lsongcore … + RUNPATH → exit 0;
                 NEEDED libsongcore.so + libc.so.6
run              BOTH consumers executed open→probe→stream enumeration→
                 metadata→decode (176400 frames)→seek 50% (landing 1985306 ≤
                 target 2000000)→post-seek read→drain→clean SONG_EOF→close:
                 "external-consumer-example: PASS", exit 0 both.
                 Media: the canonical flac fixture (consumers supply their
                 own media; the package ships none) — only package
                 include/lib + libc/libm used.
```

## 5. Reproducibility (contract §12 T1-T3, two independent build roots)

```text
T1  R1 = ~/qianqian-172-linux (S3 build lane, closure derived from the
    pinned zip), R2 = ~/qianqian-172-linux-r2 (fresh clone @ bf23516, R1's
    imported closure copied in VERBATIM per the ask — closure manifest
    byte-identical, cmp-verified). Both built with the same xmake command
    line. R2 is therefore NOT an L1 test; L1 rests on R1's own fresh
    derive (fetch → import oracle → build → gates, all via documented
    commands / ops/build-linux.sh replays).
T2  artifact shas differ across roots: libsongcore.a 409ef3da… vs
    1883052d…; libsongcore.so 5e9c5697… vs fa86489b…; libqianqian_av.a
    ff6cacec… vs 3f07e888…. Member-wise (ar x, the frozen binary-safe
    method): 267 unique member names, 259 byte-identical, 8 differing
    (flac_picture.c.o, hdr_dynamic_metadata.c.o, iamf_parse.c.o,
    iamf_reader.c.o, lcevc.c.o, oggparseflac.c.o, oggparseogm.c.o,
    oggparsevorbis.c.o). 9 duplicate-basename names re-checked via binary-
    safe `ar p` on Linux ar: 9/9 first instances SAME. 13 non-first
    duplicate-name instances (version.c.o ×4, utils.c.o ×3, options.c.o ×3,
    and four ×2 names) were NOT individually compared (ar x/ar p extract
    only the first instance of a repeated name) — recorded limitation.
T3  root cause: every one of the 8 differing members embeds its OWN build
    root's absolute path (r1 members contain /home/hoo/qianqian-172-linux,
    r2 members contain /home/hoo/qianqian-172-linux-r2; 0 differing
    members without root hits) — the same measured phenomenon the contract
    records as S-LIVE-1/S-323 (absolute path data inside merged FFmpeg
    objects, e.g. __FILE__-bearing assert/diagnostic data). No timestamp
    or ordering difference class was observed. Consequently:
    L1 RECIPE_RECONSTRUCTIBLE (R1 fresh derive + documented replay),
    L2 SEMANTIC_ABI_VERIFIED — R2 passes the full G2 C-suite with the SAME
       frozen fields (receipt verdict PASS, frozen_reference_match 4/4),
       same ABI audits (15/15 both forms, 280/280 elf64 members, NEEDED
       libm+libc), byte-identical closure manifest, same runtime-dependency
       surface;
    L3 BYTE_REPRODUCIBLE = NOT_ESTABLISHED (differences measured and
       attributed; T4 flag experiments deliberately NOT run — owner
       decision Q2 pending; per contract T4 a flag that changes no measured
       differing byte must be dropped, and introducing one is outside this
       lane's ask).
```

## 6. Authority differentials recorded (not silently resolved)

```text
D1  §8 files{} wording ("the same set checksums.txt covers") vs §7/V1
    ("the manifest hashes neither itself nor checksums.txt") cannot both
    hold literally; the self-consistent reading was enforced (files[] =
    checksums set minus manifest.json) and is documented in
    manifest.notes. Owner may amend the contract text.
D2  §8.8's "any other status → produced == 0 for that call" vs observed:
    on the argument-validation path (dst == NULL AND capacity == 0) SongCore
    returns SONG_ERR_INVALID_ARGUMENT WITHOUT writing *out_frames_produced
    (observed: pre-set 1 stays 1; recorded as
    observation_nulldst_produced_untouched in the receipt). The documented
    capacity-0 case with a valid dst DOES zero produced. Recorded for
    authority review; not treated as a suite failure because the probe
    itself passed two illegal arguments at once.
```

## 7. NOT_RUN / NOT_ESTABLISHED (fail-closed reporting)

```text
- Performance gates: run.py perf section (historical #112 regression guard)
  and call-comparison `all` (A/B FFI-cost) — not run; outside the S3-verify
  ask (which names semantics/parity/packaging/repro).
- ogg/opus/wav/PCM-WAV scenario coverage: the canonical corpus contains
  exactly 4 committed fixtures (mp3/flac/alac×2); S-BIND §7.3 forbids a
  second corpus, so no additional-format scenarios were invented. Profile
  codecs without a canonical fixture are NOT covered by this evidence.
- Multi-audio-stream select_stream re-enumeration scenario: all canonical
  fixtures are single-stream; select_stream was exercised only via the
  invalid-index negative and the committed surface gate (select=0/0). A
  real multi-stream selection scenario is NOT_RUN for this target.
- L3 byte reproducibility: NOT_ESTABLISHED (see §5; T4 flag experiments
  deliberately not run — Q2 owner-reserved).
- G4 RELEASE state / publication: not performed — a produced archive is not
  a release (contract §11); version number (Q1) and publication mechanism
  (Q5) are owner-reserved.
- 13 non-first duplicate-name archive members: not individually
  byte-compared (method limitation, §5 T2).
- Windows-lane cross-execution of this lane's artifacts: not applicable
  (different target; no claim transferred in either direction).
- lefthook pre-push: not run (no commits created).
```

## 8. Commands of record

```text
python3 ops/verify-suite/run_suite.py                      # C suite + canonical gates
CARGO_HOME=/home/hoo/.cargo-s3 cargo test --locked --manifest-path …  # x2
CARGO_HOME=/home/hoo/.cargo-s3 cargo run --locked --release -- layout|surface
python3 ops/package/package_linux.py                       # §7/§8 + V1-V7 + tar
bash drivers for R2 clone/closure-copy/xmake build/T2-T3 + L2 suite rerun
tar -xzf …/songcore-v0.0.0-s3-verify-…tar.gz && sha256sum -c metadata/checksums.txt
gcc (README link lines) + ./consumer-{static,shared} <canonical fixture>
```
