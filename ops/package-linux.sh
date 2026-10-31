#!/usr/bin/env bash
# ops/package-linux.sh — Linux x86_64 SongCore package entry, per the frozen
# contract docs/release/songcore-release-contract.md:
#   section 2   VERSION_RULE (songcore-vMAJOR.MINOR.PATCH, decimal components
#               without leading zeros; songcore-v0.1.0 is immutable history and
#               is refused here)
#   section 6   ARTIFACT_KINDS (static lib/libsongcore.a mandatory, shared
#               lib/shared/libsongcore.so secondary — the linux-x86_64 recipe
#               declares artifacts.static=true and artifacts.shared=true)
#   section 7   PACKAGE_LAYOUT (v0.1.0-shaped tree; schema-2 additions only
#               under metadata/ and LICENSES/; exactly one versioned root
#               directory and no loose files beside it)
#   section 8   PROVENANCE_SCHEMA (metadata/manifest.json schema_version 2;
#               V1-V7 machine checks run on the WRITTEN files before any
#               container exists)
#   section 9   RUNTIME_DEPENDENCIES (libc/libm/libpthread consumer link
#               requirements; the shared form's glibc symbol-version floor is
#               recorded)
#   section 10  LICENSE_OBLIGATIONS (O1/O2/B4 pass gates; B1-B4
#               RELEASE_BLOCKED conditions fail the run)
#   section 16  D2 (checksums.txt covers every packaged file except itself;
#               files{} covers that set MINUS metadata/manifest.json — never a
#               self-hash)
#
# Replayable flow (fail-closed; no container is written until every pre-tar
# check passes):
#   0. preflight: --version form + immutability guard, work root is the pinned
#      commit with a clean tracked tree (R2 discipline), Q1 version identity,
#      ABI header identity (work-root file == git blob at the commit ==
#      contract section 3), the pinned FFmpeg zip re-verified against
#      pin.json source_sha256, and every input present.
#   1. binutils audits: ELF identity, per-member archive format, the ABI export
#      set in BOTH forms, NEEDED audit, non-song export leak audit, glibc
#      symbol-version floor, and the archive-level closure-resolution audit.
#   2. LICENSES/: the four FFmpeg COPYING texts extracted from the
#      sha256-verified pinned source zip (O1); repo-root LICENSE-MIT /
#      LICENSE-APACHE copied byte-identically (B4); THIRD_PARTY_NOTICES.txt
#      rendered from the verified pin (tools/render_songcore_notices.py).
#   3. stage the section 7 tree under <out-dir>/stage/<pkg-root>/ and write
#      metadata/manifest.json (schema 2, bound to the staged bytes),
#      metadata/symbols.txt and metadata/checksums.txt.
#   4. V1-V7 + O1/O2/B4 re-verification by an independent re-read of the
#      written files; any failure aborts with a nonzero exit and no container.
#   5. write a deterministic .tar.gz (members sorted, mtime fixed to
#      SOURCE_DATE_EPOCH or the source commit time, uid/gid 0, empty
#      uname/gname, no absolute path, no symlink, no pax header), re-open it
#      and verify entry set, order, modes, ownership, mtime and every payload
#      hash, then extract it and run `sha256sum -c metadata/checksums.txt`.
#   6. write the SHA256SUMS sidecar and a package-summary.json receipt.
#
# Usage:
#   ops/package-linux.sh --version <X.Y.Z> --source-commit <40-hex> \
#                        --work-root <build root> --out-dir <dir> \
#                        --build-receipt <path> \
#                        [--artifact-static <path>] [--artifact-shared <path>] \
#                        [--closure-manifest <path>] [--pinned-zip <path>] \
#                        [--consumer-example <path>]
#
#   --version          REQUIRED, no default. The concrete next release number
#                      is owner-reserved (contract section 14 Q1); this entry
#                      never picks one, creates no tag and publishes nothing.
#   --source-commit    REQUIRED full 40-hex commit all inputs must come from.
#   --work-root        REQUIRED build root that ops/build-linux.sh produced.
#   --out-dir          REQUIRED output directory (stage tree, container,
#                      sidecars, receipts).
#   --build-receipt    REQUIRED ops/build-linux.sh summary.json. Release-grade
#                      packaging never accepts unattested artifact bytes.
#   --artifact-static  default <work-root>/native/build/artifacts/libsongcore.a
#   --artifact-shared  default <work-root>/native/build/artifacts/shared/libsongcore.so
#   --closure-manifest default <work-root>/native/build/ffmpeg-xmake/manifest.json
#   --pinned-zip       default <work-root>/native/build/downloads/ffmpeg-n9.0.1.zip
#   --consumer-example default <work-root>/evidence/songcore-172/linux/verify/
#                      package/external-consumer-example.c (tracked S3 template)
#
# Version: caller-supplied only. No tag, no GitHub Release, no publication.
#
# Exit code: 0 only when the container was written and re-verified.
set -Eeuo pipefail
# Mirror the build entry's tool search path: the receipt's toolchain identity
# is produced there, so the packager must resolve the same tools.
export PATH="$HOME/.local/bin:$PATH"

ARGV=("$@")
VERSION=""
SOURCE_COMMIT=""
WORK_ROOT=""
OUT_DIR=""
ART_STATIC=""
ART_SHARED=""
CLOSURE_MANIFEST=""
PINNED_ZIP=""
CONSUMER_EXAMPLE=""
BUILD_RECEIPT=""

# Contract section 3 ABI v1 header identity. It moves only with an authority
# reference (an ABI amendment), never with a packaging change.
FROZEN_ABI_HEADER_SHA256=d43a9d47b767fe9671b9691011feb1d5201f4080609010be4d6da96f16c78d2a
IMMUTABLE_HISTORICAL_VERSION=0.1.0

log() { printf '[package-linux] %s\n' "$*"; }
die() { printf 'package-linux: FAIL: %s\n' "$*" >&2; exit 1; }
kv()  { grep -m1 "^$2=" "$1" | cut -d= -f2-; }
sha_of() { sha256sum "$1" | awk '{print $1}'; }
# Under `set -e -o pipefail` a `tool --version | head -1` pipeline fails when the
# tool is absent (127, hidden by 2>/dev/null) or SIGPIPEs the writer. A missing
# tool must degrade to an empty identity string, never a silent exit.
ident() {
  command -v "$1" >/dev/null 2>&1 || return 0
  { "$1" --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g'; } || true
}

usage() {
  sed -n '/^# Usage:/,/^# Version:/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version)          [ $# -ge 2 ] || die "--version needs a value";           VERSION=$2; shift 2 ;;
    --source-commit)    [ $# -ge 2 ] || die "--source-commit needs a value";     SOURCE_COMMIT=$2; shift 2 ;;
    --work-root)        [ $# -ge 2 ] || die "--work-root needs a value";         WORK_ROOT=$2; shift 2 ;;
    --out-dir)          [ $# -ge 2 ] || die "--out-dir needs a value";           OUT_DIR=$2; shift 2 ;;
    --build-receipt)    [ $# -ge 2 ] || die "--build-receipt needs a value";     BUILD_RECEIPT=$2; shift 2 ;;
    --artifact-static)  [ $# -ge 2 ] || die "--artifact-static needs a value";   ART_STATIC=$2; shift 2 ;;
    --artifact-shared)  [ $# -ge 2 ] || die "--artifact-shared needs a value";   ART_SHARED=$2; shift 2 ;;
    --closure-manifest) [ $# -ge 2 ] || die "--closure-manifest needs a value";  CLOSURE_MANIFEST=$2; shift 2 ;;
    --pinned-zip)       [ $# -ge 2 ] || die "--pinned-zip needs a value";        PINNED_ZIP=$2; shift 2 ;;
    --consumer-example) [ $# -ge 2 ] || die "--consumer-example needs a value";  CONSUMER_EXAMPLE=$2; shift 2 ;;
    -h|--help)          usage; exit 0 ;;
    *) printf 'package-linux: unknown argument: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

# ------------------------------------------------------------------ preflight
[ -n "$VERSION" ] || die "--version is required: this entry has no default version (the next release number is owner-reserved, contract section 14 Q1)"
[ "$VERSION" != "$IMMUTABLE_HISTORICAL_VERSION" ] \
  || die "refusing version '$VERSION': songcore-v0.1.0 is immutable history (contract section 2); a failed release releases under a new version, never a re-tag"
[[ "$VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || die "version '$VERSION' is not the authorized MAJOR.MINOR.PATCH identity (contract section 2; suffixes and leading zeros are not authorized)"

[ -n "$SOURCE_COMMIT" ] || die "--source-commit is required"
[[ "$SOURCE_COMMIT" =~ ^[0-9a-f]{40}$ ]] \
  || die "--source-commit must be a full lowercase 40-hex sha (got '$SOURCE_COMMIT')"
[ -n "$WORK_ROOT" ] || die "--work-root is required"
[ -n "$OUT_DIR" ]   || die "--out-dir is required"
[ -n "$BUILD_RECEIPT" ] || die "--build-receipt is required for release-qualified packaging"
[ -d "$WORK_ROOT" ] || die "work root $WORK_ROOT does not exist"
{ [ -d "$WORK_ROOT/.git" ] || [ -f "$WORK_ROOT/.git" ]; } || die "work root $WORK_ROOT is not a git checkout"

WORK_HEAD=$(git -C "$WORK_ROOT" rev-parse HEAD) || die "cannot read HEAD of $WORK_ROOT"
[ "$WORK_HEAD" = "$SOURCE_COMMIT" ] \
  || die "work root HEAD is $WORK_HEAD, --source-commit is $SOURCE_COMMIT; refusing to package a different tree"
DIRTY=$(git -C "$WORK_ROOT" status --porcelain --untracked-files=no) || die "git status failed in $WORK_ROOT"
[ -z "$DIRTY" ] || die "work root has uncommitted tracked changes, so source.status could not be 'clean': $DIRTY"

ART_STATIC=${ART_STATIC:-$WORK_ROOT/native/build/artifacts/libsongcore.a}
ART_SHARED=${ART_SHARED:-$WORK_ROOT/native/build/artifacts/shared/libsongcore.so}
CLOSURE_MANIFEST=${CLOSURE_MANIFEST:-$WORK_ROOT/native/build/ffmpeg-xmake/manifest.json}
PINNED_ZIP=${PINNED_ZIP:-$WORK_ROOT/native/build/downloads/ffmpeg-n9.0.1.zip}
CONSUMER_EXAMPLE=${CONSUMER_EXAMPLE:-$WORK_ROOT/evidence/songcore-172/linux/verify/package/external-consumer-example.c}

[ -f "$BUILD_RECEIPT" ] || die "missing build receipt: $BUILD_RECEIPT"

VERSION_DECISION=$WORK_ROOT/docs/release/songcore-version-decision.json
[ -f "$VERSION_DECISION" ] || die "version identity decision missing: $VERSION_DECISION"
python3 "$WORK_ROOT/tools/verify_songcore_version_identity.py" \
  --decision "$VERSION_DECISION" --version "$VERSION" --target linux-x86_64 \
  --xmake "$WORK_ROOT/native/xmake.lua" \
  || die "version identity is not approved for release-qualified packaging"

PIN=$WORK_ROOT/native/ffmpeg/pin.json
RECIPE=$WORK_ROOT/native/ffmpeg/targets/linux-x86_64.json
PROFILE=$WORK_ROOT/native/ffmpeg/profiles/codec-base.json
HEADER=$WORK_ROOT/native/include/songcore.h
LIC_MIT=$WORK_ROOT/LICENSE-MIT
LIC_APACHE=$WORK_ROOT/LICENSE-APACHE

for f in "$ART_STATIC" "$ART_SHARED" "$CLOSURE_MANIFEST" "$PINNED_ZIP" "$PIN" \
         "$RECIPE" "$PROFILE" "$HEADER" "$LIC_MIT" "$LIC_APACHE" "$CONSUMER_EXAMPLE"; do
  [ -f "$f" ] || die "missing input: $f"
done

for t in git python3 sha256sum ar nm objdump readelf file tar gzip stat cc gcc; do
  command -v "$t" >/dev/null 2>&1 || die "missing required tool: $t"
done

ARGV_JSON=$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1:]))' ${ARGV[@]+"${ARGV[@]}"})

PKG_ROOT="songcore-v$VERSION-linux-x86_64"
STAGE=$OUT_DIR/stage/$PKG_ROOT
TARBALL=$OUT_DIR/$PKG_ROOT.tar.gz
AUD=$OUT_DIR/audit
mkdir -p "$AUD"
rm -rf "$OUT_DIR/stage" "$OUT_DIR/verify-extract"
mkdir -p "$STAGE"

log "version $VERSION (caller-supplied; not a release)"
log "source  $SOURCE_COMMIT @ $WORK_ROOT"

# ABI header identity: work-root file == git blob at the commit == contract §3.
HEADER_SHA=$(sha_of "$HEADER")
GIT_HEADER_SHA=$(git -C "$WORK_ROOT" show "$SOURCE_COMMIT:native/include/songcore.h" | sha256sum | awk '{print $1}')
[ "$HEADER_SHA" = "$FROZEN_ABI_HEADER_SHA256" ] \
  || die "ABI header drift: $HEADER (sha256 $HEADER_SHA) != frozen contract section 3 identity $FROZEN_ABI_HEADER_SHA256"
[ "$GIT_HEADER_SHA" = "$HEADER_SHA" ] \
  || die "ABI header at the source commit ($GIT_HEADER_SHA) != the work-root file ($HEADER_SHA)"

PIN_SHA=$(sha_of "$PIN")
RECIPE_SHA=$(sha_of "$RECIPE")
PROFILE_SHA=$(sha_of "$PROFILE")
CLOSURE_SHA=$(sha_of "$CLOSURE_MANIFEST")
ART_STATIC_SHA=$(sha_of "$ART_STATIC")
ART_SHARED_SHA=$(sha_of "$ART_SHARED")
ART_STATIC_BYTES=$(stat -c %s "$ART_STATIC")
ZIP_SHA=$(sha_of "$PINNED_ZIP")

python3 - "$PIN" > "$AUD/pin-values.env" <<'PY' || die "cannot read $PIN"
import json, sys
p = json.load(open(sys.argv[1], encoding="utf-8"))
for k in ("ffmpeg_tag", "ffmpeg_commit_sha", "ffmpeg_version", "source_sha256", "source_url"):
    print("%s=%s" % (k.upper(), str(p[k]).replace("\n", " ")))
PY
FFMPEG_TAG=$(kv "$AUD/pin-values.env" FFMPEG_TAG)
FFMPEG_COMMIT=$(kv "$AUD/pin-values.env" FFMPEG_COMMIT_SHA)
FFMPEG_SOURCE_SHA=$(kv "$AUD/pin-values.env" SOURCE_SHA256)

[ "$ZIP_SHA" = "$FFMPEG_SOURCE_SHA" ] \
  || die "pinned FFmpeg zip $PINNED_ZIP sha256 $ZIP_SHA != pin.json source_sha256 $FFMPEG_SOURCE_SHA (the O1 basis)"

# --------------------------------------------------------- stage 1: audits
log "stage1: binutils audits"
file "$ART_STATIC" "$ART_SHARED" > "$AUD/elf-identity-file.txt"
grep -q "current ar archive" "$AUD/elf-identity-file.txt" \
  || die "static artifact is not an ar archive"
grep -q "ELF 64-bit LSB shared object, x86-64" "$AUD/elf-identity-file.txt" \
  || die "shared artifact is not an ELF64 x86-64 shared object"

ar t "$ART_STATIC" > "$AUD/archive-members.txt" || die "ar t failed"
MEMBER_COUNT=$(grep -c . "$AUD/archive-members.txt" || true)
[ "${MEMBER_COUNT:-0}" -gt 0 ] || die "static archive has no members"
objdump -f "$ART_STATIC" > "$AUD/archive-member-formats.txt" || die "objdump -f failed"
ELF_MEMBERS=$(grep -c "file format elf64-x86-64" "$AUD/archive-member-formats.txt" || true)
[ "$ELF_MEMBERS" = "$MEMBER_COUNT" ] \
  || die "archive member formats: $ELF_MEMBERS/$MEMBER_COUNT are elf64-x86-64"

# The header-derived set is the single expected ABI set (V4 leg 1).
grep 'SONGCORE_API' "$HEADER" | grep -oE '\bsong[a-z0-9_]*\(' | tr -d '(' | sort -u > "$AUD/header-symbols.txt"
HEADER_SYMBOLS=$(grep -c . "$AUD/header-symbols.txt" || true)
[ "$HEADER_SYMBOLS" = 15 ] \
  || die "native/include/songcore.h declares $HEADER_SYMBOLS SONGCORE_API song* entry points, expected the ABI v1 set of 15 (contract section 3)"

nm "$ART_STATIC" | awk '$2=="T"||$2=="A"{print $3}' | grep '^song' | sort -u > "$AUD/static-symbols.txt" || true
diff "$AUD/header-symbols.txt" "$AUD/static-symbols.txt" > "$AUD/symbols-static.diff" \
  || die "static symbol set != the songcore.h ABI set (see $AUD/symbols-static.diff)"

nm -D --defined-only "$ART_SHARED" | awk '$2=="T"||$2=="W"{print $3}' | grep '^song' | sort -u > "$AUD/shared-symbols.txt" || true
diff "$AUD/header-symbols.txt" "$AUD/shared-symbols.txt" > "$AUD/symbols-shared.diff" \
  || die "shared export set != the songcore.h ABI set (see $AUD/symbols-shared.diff)"

# No non-ABI dynamic export may leak out of the shared form.
nm -D --defined-only "$ART_SHARED" | awk '$2=="T"||$2=="W"{print $3}' | grep -v '^song' | sort -u > "$AUD/shared-foreign-exports.txt" || true
if grep -q . "$AUD/shared-foreign-exports.txt"; then
  die "shared artifact exports non-ABI symbols: $(paste -sd, "$AUD/shared-foreign-exports.txt")"
fi

readelf -d "$ART_SHARED" | grep NEEDED | awk '{print $5}' | tr -d '[]' > "$AUD/needed-libs.txt" || true
[ -s "$AUD/needed-libs.txt" ] || die "shared artifact reports no NEEDED entries"
if grep -qE '^lib(av|sw)' "$AUD/needed-libs.txt"; then
  die "shared artifact still NEEDs an FFmpeg closure library: $(paste -sd, "$AUD/needed-libs.txt")"
fi

readelf --dyn-syms --wide "$ART_SHARED" | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)*' | sort -Vu | tail -1 > "$AUD/glibc-floor.txt" || true
GLIBC_FLOOR=$(cat "$AUD/glibc-floor.txt")
[ -n "$GLIBC_FLOOR" ] || die "cannot determine the glibc symbol-version floor of $ART_SHARED (contract section 9)"

# Closure-resolution audit: the merged archive must resolve every FFmpeg symbol
# within itself, leaving only host (libc/libm/...) imports undefined.
nm --defined-only "$ART_STATIC" | awk '{print $3}' | grep -E '.' | sort -u > "$AUD/static-defined-all.txt" || true
nm -u "$ART_STATIC" | awk '{print $2}' | grep -E '.' | sort -u > "$AUD/static-undefined-all.txt" || true
comm -23 "$AUD/static-undefined-all.txt" "$AUD/static-defined-all.txt" > "$AUD/static-unresolved.txt" || true
UNDEFINED_COUNT=$(grep -c . "$AUD/static-unresolved.txt" || true)
if grep -qE '^(av|avcodec|avformat|avutil|swr|sws)_' "$AUD/static-unresolved.txt"; then
  die "static archive leaves FFmpeg closure symbols unresolved ($(grep -E '^(av|avcodec|avformat|avutil|swr|sws)_' "$AUD/static-unresolved.txt" | paste -sd, -)); the closure is not statically closed"
fi

# The imported closure must be the one this commit's pin/recipe/profile define.
# (V5 re-checks the copylib posture of the written manifest; this checks the
# build output against the source-commit inputs before anything is staged.)
python3 - "$CLOSURE_MANIFEST" "$RECIPE" "$PROFILE" "$FFMPEG_TAG" "$FFMPEG_COMMIT" "$FFMPEG_SOURCE_SHA" \
  "$RECIPE_SHA" "$PROFILE_SHA" \
  > "$AUD/closure-identity.env" <<'PY' || die "the imported FFmpeg closure identity does not match the pin/recipe/profile at the source commit"
import hashlib, json, platform, sys
man = json.load(open(sys.argv[1], encoding="utf-8"))
rec = json.load(open(sys.argv[2], encoding="utf-8"))
t = man.get("target", {})
errs = []
if man.get("schema") != 2: errs.append("closure manifest schema != 2")
if t.get("id") != "linux-x86_64": errs.append("closure target.id != linux-x86_64")
if t.get("platform") != rec.get("platform"): errs.append("closure platform != recipe platform")
if t.get("arch") != rec.get("arch"): errs.append("closure arch != recipe arch")
if t.get("target_os") != rec.get("ffmpeg", {}).get("target_os"): errs.append("closure target_os != recipe target_os")
if t.get("recipe_sha256") != sys.argv[7]: errs.append("closure recipe_sha256 != the recipe at the source commit")
if man.get("profile_sha256") != sys.argv[8]: errs.append("closure profile_sha256 != the codec-base profile at the source commit")
if man.get("ffmpeg_tag") != sys.argv[4]: errs.append("closure ffmpeg_tag != pin.json")
if man.get("ffmpeg_commit_sha") != sys.argv[5]: errs.append("closure ffmpeg_commit != pin.json")
if man.get("ffmpeg_source_sha256") != sys.argv[6]: errs.append("closure ffmpeg_source_sha256 != pin.json")
if man.get("toolchain", {}).get("system") != platform.system().lower(): errs.append("closure toolchain.system != this host")
if man.get("toolchain", {}).get("machine") != platform.machine().lower(): errs.append("closure toolchain.machine != this host")
if errs:
    print("FAIL: " + "; ".join(errs), file=sys.stderr)
    sys.exit(1)
tc = man["toolchain"]
print("CLOSURE_CC=%s" % tc.get("cc"))
print("CLOSURE_CC_IDENT=%s" % tc.get("cc_ident"))
print("CLOSURE_ARCH=%s" % tc.get("arch"))
print("CLOSURE_TARGET_OS=%s" % (tc.get("target_os") or ""))
print("CLOSURE_UNITS=%d" % man["closure"]["translation_units"])
print("CLOSURE_UPSTREAM=%d" % man["closure"]["upstream_sources"])
print("CLOSURE_GENERATED=%d" % man["closure"]["generated_sources"])
PY

CC_IDENT=$(ident cc)
GCC_IDENT=$(ident gcc)
AR_IDENT=$(ident ar)
XMAKE_VER=$(ident xmake)
XMAKE_IDENT=${XMAKE_VER:-not found on this packaging host (xmake is not a packaging input)}

# --------------------------------------- stage 2: LICENSES + notices + README
log "stage2: license texts from the pinned source zip (O1) + component texts (B4)"
mkdir -p "$STAGE/LICENSES"
python3 - "$PINNED_ZIP" "$STAGE/LICENSES" <<'PY' || die "extracting the FFmpeg COPYING texts from $PINNED_ZIP failed"
import os, sys, zipfile
zip_path, out = sys.argv[1], sys.argv[2]
names = ["COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3"]
z = zipfile.ZipFile(zip_path)
members = z.namelist()
resolved = {}
for n in names:
    hits = [m for m in members if m.split("/")[-1] == n]
    if len(hits) != 1:
        print("FAIL: pinned zip member resolution for %s: %d hits" % (n, len(hits)), file=sys.stderr)
        sys.exit(22)
    resolved[n] = hits[0]
for n in names:
    with z.open(resolved[n]) as src, open(os.path.join(out, n), "wb") as dst:
        dst.write(src.read())
PY

cp -f "$LIC_MIT" "$STAGE/LICENSES/LICENSE-MIT"
cp -f "$LIC_APACHE" "$STAGE/LICENSES/LICENSE-APACHE"
python3 "$WORK_ROOT/tools/render_songcore_notices.py" \
  --out "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt" --version "$VERSION" \
  --target linux-x86_64 --pin "$PIN" --profile-sha256 "$PROFILE_SHA" \
  > "$AUD/notices.env" || die "rendering THIRD_PARTY_NOTICES.txt failed"

# ------------------------------------------------------ stage 3: payload tree
log "stage3: staging the section 7 tree"
mkdir -p "$STAGE/include" "$STAGE/lib/shared" "$STAGE/metadata"
cp -f "$HEADER"           "$STAGE/include/songcore.h"
cp -f "$ART_STATIC"       "$STAGE/lib/libsongcore.a"
cp -f "$ART_SHARED"       "$STAGE/lib/shared/libsongcore.so"
cp -f "$CONSUMER_EXAMPLE" "$STAGE/metadata/external-consumer-example.c"
cp -f "$AUD/header-symbols.txt" "$STAGE/metadata/symbols.txt"

# Artifact provenance: a packager must not stamp a stale artifact with the
# current commit's name. The shared verifier binds these exact bytes and every
# provenance input to the source commit/tree; packaging never trusts the
# receipt's own self-description beyond that.
SOURCE_TREE=$(git -C "$WORK_ROOT" rev-parse 'HEAD^{tree}')
# Byte-identical to the identity ops/build-linux.sh writes into the receipt:
# "gcc=<ver>;xmake=<ver>", with an absent xmake contributing an empty segment.
TOOLCHAIN_ID="gcc=$GCC_IDENT;xmake=$XMAKE_VER"
python3 "$WORK_ROOT/tools/verify_songcore_build_receipt.py" \
  --receipt "$BUILD_RECEIPT" --source-commit "$SOURCE_COMMIT" --source-tree "$SOURCE_TREE" \
  --artifact "$ART_STATIC" --artifact-shared "$ART_SHARED" --header "$HEADER" \
  --recipe "$RECIPE" --profile "$PROFILE" --pin "$PIN" --closure "$CLOSURE_MANIFEST" \
  --target linux-x86_64 --toolchain "$TOOLCHAIN_ID" \
  || die "build receipt $BUILD_RECEIPT does not bind these artifact bytes and inputs to $SOURCE_COMMIT"
BUILD_RECEIPT_SHA=$(sha_of "$BUILD_RECEIPT")

NEEDED_CSV=$(paste -sd' ' "$AUD/needed-libs.txt")
python3 - "$STAGE/README.md" "$VERSION" "$SOURCE_COMMIT" "$MEMBER_COUNT" "$NEEDED_CSV" "$GLIBC_FLOOR" <<'PY' \
  || die "rendering README.md failed"
import sys
path, version, commit, members, needed, floor = sys.argv[1:7]
text = """# SongCore {version} — linux-x86_64

SongCore is the local media -> PCM decode component of Qianqian: one local
media file in, one coherent song snapshot out (identity, metadata, artwork,
stream information, source-rate/source-layout Float32 interleaved PCM, seek,
EOF). The ABI authority is `include/songcore.h` alone (ABI v1, frozen);
packaging never modifies it.

**This container is a release-substrate verification archive, not a release.**
The version string is the one the packaging caller supplied, and the concrete
next release number is owner-reserved (contract section 14 Q1). Packaging
creates no tag, no GitHub Release and no other published asset, and makes no
platform-support claim.

## Contents

    include/songcore.h                     the ONLY ABI description (ABI v1, frozen)
    lib/libsongcore.a                      static artifact (FFmpeg closure merged in,
                                           {members} elf64-x86-64 members; static-first policy)
    lib/shared/libsongcore.so              optional shared secondary (same closure)
    LICENSES/                              FFmpeg COPYING set (extracted from the
                                           sha256-verified pinned n9.0.1 source zip),
                                           THIRD_PARTY_NOTICES.txt, MIT + Apache-2.0
                                           component texts (repo-root, B4)
    metadata/manifest.json                 provenance, schema_version 2
    metadata/symbols.txt                   the 15 ABI v1 exports
    metadata/checksums.txt                 sha256 of every packaged file except itself
    metadata/external-consumer-example.c   consumer smoke (open->probe->decode->seek->EOF->close)

## Verify the package

    sha256sum -c metadata/checksums.txt    # from inside this package root

## Link against it (C consumer link contract, contract section 9)

Static (the supported consumption path):

    gcc -O2 -Iinclude your.c -Llib -lsongcore -lm -lpthread -o your

    libc (glibc), libm and libpthread are the consumer link-time
    requirements; the FFmpeg closure ships in object form inside the archive,
    so no standalone FFmpeg runtime library is needed or shipped.

Shared (secondary):

    gcc -O2 -Iinclude your.c -Llib/shared -lsongcore -lm -lpthread \\
        -Wl,-rpath,'$ORIGIN' -o your

    NEEDED of lib/shared/libsongcore.so: {needed}
    glibc symbol-version floor: {floor} (recorded in
    metadata/manifest.json runtime_dependencies.shared; which distro family is
    supported is an owner decision, contract section 14)

## Consumer smoke

    gcc -O2 -Iinclude metadata/external-consumer-example.c -Llib -lsongcore -lm -lpthread -o songcore-consumer-example
    ./songcore-consumer-example <media-file>

## What packing verified about these bytes

Packing audited, with GNU binutils, that `include/songcore.h` is byte-identical
to the source commit, that the archive is an ar archive whose {members} members
are all elf64-x86-64, that both forms expose exactly the 15 ABI v1 entry points
declared by the header, and that neither form depends on an FFmpeg runtime
library. Those are build-time facts about the packaged bytes. They are not a
runtime, device or distro test and they do not establish platform support
beyond the audit above.

## Relink obligations (LGPL-2.1-or-later, contract section 10 O3)

The archive contains the FFmpeg members in object form (effective license
LGPL-2.1-or-later: the codec-base profile keeps gpl/version3/nonfree off and
pulls no external lib* copylibs). The machine-verifiable rebuild inputs --
native/ffmpeg/pin.json (tag->commit + zip sha256), the codec-base capability
profile, the linux-x86_64 target recipe and the Xmake import/replay tools live
in the source repository at commit {commit}. The imported closure manifest is
generated build output and is not in that commit. Until a clean pinned-source
import plus relink recipe is independently executed, O3 remains FIX_REQUIRED;
this archive does not claim an executable relink path.
""".format(version=version, commit=commit, members=members, needed=needed, floor=floor)
open(path, "w", encoding="utf-8", newline="\n").write(text)
PY

# ------------------------------- stage 4: manifest + symbols + checksums
log "stage4: writing metadata/manifest.json (schema 2), symbols.txt, checksums.txt"
BUILT_UTC=$(python3 - <<'PY'
import datetime, os
epoch = os.environ.get("SOURCE_DATE_EPOCH")
if epoch:
    t = datetime.datetime.fromtimestamp(int(epoch), datetime.timezone.utc)
else:
    t = datetime.datetime.now(datetime.timezone.utc)
print(t.strftime("%Y-%m-%dT%H:%M:%SZ"))
PY
) || die "cannot derive built_utc"

python3 - "$STAGE" "$WORK_ROOT" "$CLOSURE_MANIFEST" "$RECIPE" "$PROFILE" "$PIN" "$AUD" \
  "$SOURCE_COMMIT" "$VERSION" "$BUILT_UTC" "$BUILD_RECEIPT_SHA" "$MEMBER_COUNT" \
  "$GLIBC_FLOOR" "$UNDEFINED_COUNT" "$PROFILE_SHA" "$RECIPE_SHA" "$PIN_SHA" "$CLOSURE_SHA" \
  "$CC_IDENT" "$AR_IDENT" "$XMAKE_IDENT" "$ART_STATIC_SHA" "$ART_STATIC_BYTES" \
  "$ART_SHARED_SHA" <<'PY' || die "manifest/checksums binding failed"
import hashlib, json, os, sys

(stage, work_root, closure_path, recipe_path, profile_path, pin_path, audit_dir, source_commit,
 version, built_utc, receipt_sha, member_count, glibc_floor, undefined_count,
 profile_sha, recipe_sha, pin_sha, closure_sha, cc_ident, ar_ident, xmake_ident,
 static_sha, static_bytes, shared_sha) = sys.argv[1:25]

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

# contract section 7 exact content set (schema-2 additions included)
EXPECTED = {
    "include/songcore.h", "lib/libsongcore.a", "lib/shared/libsongcore.so",
    "LICENSES/COPYING.GPLv2", "LICENSES/COPYING.GPLv3",
    "LICENSES/COPYING.LGPLv2.1", "LICENSES/COPYING.LGPLv3",
    "LICENSES/THIRD_PARTY_NOTICES.txt", "LICENSES/LICENSE-MIT",
    "LICENSES/LICENSE-APACHE", "README.md",
    "metadata/manifest.json", "metadata/symbols.txt", "metadata/checksums.txt",
    "metadata/external-consumer-example.c",
}
PENDING = {"metadata/manifest.json", "metadata/checksums.txt"}
packaged = []
for root, _dirs, files in os.walk(stage):
    for name in files:
        packaged.append(os.path.relpath(os.path.join(root, name), stage).replace(os.sep, "/"))
packaged.sort()
missing = EXPECTED - set(packaged) - PENDING
extra = set(packaged) - EXPECTED
if missing or extra:
    sys.stderr.write("FAIL: staged content set != contract section 7 set: missing=%s extra=%s\n"
                     % (sorted(missing), sorted(extra)))
    sys.exit(30)

pin = json.load(open(pin_path, encoding="utf-8"))
recipe = json.load(open(recipe_path, encoding="utf-8"))
closure = json.load(open(closure_path, encoding="utf-8"))
symbols = [l.strip() for l in open(os.path.join(stage, "metadata/symbols.txt"), encoding="utf-8") if l.strip()]
needed = [l.strip() for l in open(os.path.join(audit_dir, "needed-libs.txt"), encoding="utf-8") if l.strip()]
license_files = {n: sha256(os.path.join(stage, "LICENSES", n))
                 for n in ("COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3")}

manifest = {
    "schema_version": 2,
    "songcore_version": version,
    "songcore_abi": 1,
    "abi_header_sha256": sha256(os.path.join(stage, "include/songcore.h")),
    "target": "linux-x86_64",
    "platform": "linux",
    "arch": "x86_64",
    "toolchain": recipe["toolchain"]["family"],
    "ffmpeg_version": pin["ffmpeg_version"],
    "ffmpeg_commit": pin["ffmpeg_commit_sha"],
    "ffmpeg_source_sha256": pin["source_sha256"],
    "ffmpeg_profile": "codec-base",
    "ffmpeg_profile_sha256": profile_sha,
    "artifact": "lib/libsongcore.a",
    "sha256": static_sha,
    "artifact_bytes": int(static_bytes),
    "archive_members": int(member_count),
    # v0.1.0 carried artifact_shared as the shared artifact PATH; that meaning
    # is preserved (contract section 8: carried v0.1.0 field names keep their
    # meaning). windows-x86_64 ships no shared form and records a boolean.
    "artifact_shared": "lib/shared/libsongcore.so",
    "shared_sha256": shared_sha,
    "public_symbols": sorted(symbols),
    "git_sha": source_commit,
    "target_identity": {
        "id": recipe["id"],
        "platform": recipe["platform"],
        "arch": recipe["arch"],
        "triple": "x86_64-unknown-linux-gnu",
        "target_os": recipe["ffmpeg"]["target_os"],
        "cross_prefix": recipe["ffmpeg"].get("cross_prefix"),
        "toolchain": {"family": recipe["toolchain"]["family"],
                      "identity": recipe["toolchain"]["identity"]},
        "recipe_sha256": recipe_sha,
    },
    "tools": {"cc": cc_ident, "ar": ar_ident, "xmake": xmake_ident},
    "sdk_ndk": ("not applicable this round — linux-x86_64: host-native, no SDK/NDK "
                "(target recipe toolchain identity, contract section 8)"),
    "source": {"commit": source_commit, "status": "clean",
               "build_receipt_sha256": receipt_sha},
    "pin": {
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "pin_sha256": pin_sha,
    },
    "closure": {
        "manifest_sha256": closure_sha,
        "toolchain": {
            "system": closure["toolchain"].get("system"),
            "machine": closure["toolchain"].get("machine"),
            "cc": closure["toolchain"].get("cc"),
            "cc_ident": closure["toolchain"].get("cc_ident"),
            "arch": closure["toolchain"].get("arch"),
            "target_os": closure["toolchain"].get("target_os") or None,
        },
        "configure_args": closure["configure_args"],
        "units": {
            "translation_units": closure["closure"]["translation_units"],
            "upstream_sources": closure["closure"]["upstream_sources"],
            "generated_sources": closure["closure"]["generated_sources"],
        },
        "reference_archives": closure["reference_archives"],
    },
    "files": {},
    "runtime_dependencies": {
        "static": {
            "libc": "REQUIRED (glibc) — consumer link-time requirement of the shipped archive (contract section 9)",
            "libm_libpthread": 'REQUIRED — native/build/songcore.lua add_syslinks("m", "pthread"); the link line records it',
            "link_line": "gcc -O2 -Iinclude consumer.c -Llib -lsongcore -lm -lpthread -o consumer",
            "unresolved_symbols": ("%s host symbols remain undefined after the merged closure resolves "
                                   "itself; no av*/sw* FFmpeg symbol is left unresolved" % undefined_count),
        },
        "shared": {
            "needed_libs": needed,
            "glibc_symbol_version_floor": (glibc_floor + " (readelf --dyn-syms; the supported "
                                           "distro family is an owner decision, contract section 14)"),
            "link_line": "gcc -O2 -Iinclude consumer.c -Llib/shared -lsongcore -lm -lpthread -Wl,-rpath,$ORIGIN -o consumer",
        },
        "ffmpeg_runtime": "none — the closure ships in object form inside the archive (contract section 9)",
    },
    "license": {
        "ffmpeg_effective": "LGPL-2.1-or-later",
        "gpl_enabled": False,
        "version3_enabled": False,
        "nonfree_enabled": False,
        "external_libs": [],
        "license_files": license_files,
    },
    "reproducibility": {
        "recipe": "NOT_ESTABLISHED",
        "semantic_abi": "NOT_ESTABLISHED",
        "byte": "NOT_ESTABLISHED",
        "evidence": ("none this pack: ops/package-linux.sh performs no two-root rebuild, so no "
                     "reproducibility receipt exists and every level is reported NOT_ESTABLISHED "
                     "(contract section 8 V6 / section 12). A positive level must name an existing receipt."),
    },
    "built_utc": built_utc,
    "packer": "ops/package-linux.sh (deterministic tar container)",
    "notes": ("songcore_version %s was supplied by the packaging caller; the concrete next release number "
              "is owner-reserved (contract section 14 Q1), this is a release-substrate verification "
              "archive, not a release, and nothing here is published. Artifact provenance IS attested: "
              "source.build_receipt_sha256 binds the build receipt, and both artifact sha256 values were "
              "re-verified against these bytes at packing time. files{} covers the checksums.txt set minus "
              "manifest.json itself (non-circular binding; contract section 8 V1, Issue #330 D2). "
              "LICENSES/COPYING.* were extracted from the pinned n9.0.1 source zip re-verified against "
              "pin.json source_sha256 on this run (O1); LICENSES/LICENSE-MIT and LICENSES/LICENSE-APACHE are "
              "byte-identical to the repo-root texts at the source commit (B4). Machine license checks "
              "verify presence, hash identity and configure-arg provenance only; they cannot certify legal "
              "sufficiency (contract section 10 limit)." % version),
}

covered_mf = [p for p in packaged if p not in ("metadata/checksums.txt", "metadata/manifest.json")]
manifest["files"] = {p: {"sha256": sha256(os.path.join(stage, p)),
                         "bytes": os.path.getsize(os.path.join(stage, p))}
                     for p in covered_mf}

with open(os.path.join(stage, "metadata", "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(manifest, f, indent=2, ensure_ascii=False)
    f.write("\n")

covered_ck = sorted(packaged + ["metadata/manifest.json"])
hashes = {p: sha256(os.path.join(stage, p)) for p in covered_ck}
with open(os.path.join(stage, "metadata", "checksums.txt"), "w", encoding="utf-8", newline="\n") as f:
    for p in covered_ck:
        f.write("%s  %s\n" % (hashes[p], p))
print("BOUND files=%d checksums=%d" % (len(covered_mf), len(covered_ck)))
PY

# ------------------------------------ stage 5: V1-V7 + O1/O2/B4 verification
log "stage5: V1-V7 + O1/O2/B4 re-verification of the written files"
python3 - "$STAGE" "$AUD" "$CLOSURE_MANIFEST" "$RECIPE" "$WORK_ROOT" "$FROZEN_ABI_HEADER_SHA256" \
  "$GIT_HEADER_SHA" "$PIN_SHA" "$RECIPE_SHA" "$PROFILE_SHA" "$CLOSURE_SHA" <<'PY' \
  || die "contract section 8 machine checks failed (see $OUT_DIR/v-checks.json)"
import hashlib, json, os, sys

(stage, audit_dir, closure_path, recipe_path, work_root, frozen_header, commit_header,
 pin_sha, recipe_sha, profile_sha, closure_sha) = sys.argv[1:12]
out_dir = os.path.dirname(os.path.dirname(stage))
checks, details = {}, {}

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

def record(name, ok, detail):
    checks[name] = bool(ok)
    details[name] = detail

manifest = json.load(open(os.path.join(stage, "metadata/manifest.json"), encoding="utf-8"))
cksum, dupes = {}, []
for line in open(os.path.join(stage, "metadata/checksums.txt"), encoding="utf-8"):
    if line.strip():
        h, p = line.rstrip("\n").split("  ", 1)
        if p in cksum:
            dupes.append(p)
        cksum[p] = h

packaged = []
for root, _dirs, files in os.walk(stage):
    for name in files:
        packaged.append(os.path.relpath(os.path.join(root, name), stage).replace(os.sep, "/"))
packaged.sort()

# ---- V1: set arithmetic + one rehash pass against the packaged bytes (#330 D2)
cksum_expected = sorted(p for p in packaged if p != "metadata/checksums.txt")
files_expected = sorted(p for p in cksum if p != "metadata/manifest.json")
record("V1_sets",
       not dupes
       and sorted(cksum) == cksum_expected
       and sorted(manifest.get("files", {})) == files_expected
       and "metadata/checksums.txt" not in cksum
       and "metadata/checksums.txt" not in manifest.get("files", {})
       and "metadata/manifest.json" not in manifest.get("files", {}),
       "checksums.txt (%d) = packaged minus itself; files{} (%d) = checksums minus manifest.json; no self-hash; duplicates=%s"
       % (len(cksum), len(manifest.get("files", {})), dupes or "none"))

actual = {p: sha256(os.path.join(stage, p)) for p in cksum}
sizes = {p: os.path.getsize(os.path.join(stage, p)) for p in cksum}
bad = (["%s (checksums.txt)" % p for p, h in cksum.items() if actual[p] != h]
       + ["%s (files{} sha256)" % p for p, e in manifest.get("files", {}).items() if actual.get(p) != e.get("sha256")]
       + ["%s (files{} bytes)" % p for p, e in manifest.get("files", {}).items() if sizes.get(p) != e.get("bytes")])
record("V1_rehash", not bad, "checksums.txt + files{} re-hash to the packaged bytes; bad=%s" % (bad or "none"))

# ---- V2: header identity (packaged == source commit blob == contract section 3)
record("V2_header_sha",
       actual.get("include/songcore.h") == manifest.get("abi_header_sha256")
       == commit_header == frozen_header,
       "packaged header %s == manifest == the source commit blob == contract section 3"
       % actual.get("include/songcore.h"))

# ---- V3: pin.*/closure.* agree with the source-commit inputs
pin = json.load(open(os.path.join(work_root, "native/ffmpeg/pin.json"), encoding="utf-8"))
recipe = json.load(open(recipe_path, encoding="utf-8"))
closure = json.load(open(closure_path, encoding="utf-8"))
v3 = []
if manifest.get("ffmpeg_commit") != manifest["pin"]["ffmpeg_commit_sha"]: v3.append("ffmpeg_commit carry")
if manifest.get("ffmpeg_commit") != pin["ffmpeg_commit_sha"]: v3.append("ffmpeg_commit vs pin.json")
if manifest.get("ffmpeg_source_sha256") != manifest["pin"]["ffmpeg_source_sha256"]: v3.append("source sha carry")
if manifest.get("ffmpeg_source_sha256") != pin["source_sha256"]: v3.append("source sha vs pin.json")
if manifest["pin"].get("pin_sha256") != pin_sha: v3.append("pin_sha256")
if manifest["closure"].get("manifest_sha256") != closure_sha: v3.append("closure manifest sha")
if manifest.get("ffmpeg_profile_sha256") != profile_sha: v3.append("profile sha")
if manifest["target_identity"].get("recipe_sha256") != recipe_sha: v3.append("recipe sha")
if manifest["closure"].get("configure_args") != closure.get("configure_args"): v3.append("configure_args")
record("V3_pin_binding", not v3,
       "pin.*/closure.* agree with the source-commit inputs; mismatch=%s" % (v3 or "none"))

# ---- V4: the 15 ABI symbols, in both forms and in symbols.txt
symbols_txt = [l.strip() for l in open(os.path.join(stage, "metadata/symbols.txt"), encoding="utf-8") if l.strip()]
aud = lambda n: [l.strip() for l in open(os.path.join(audit_dir, n), encoding="utf-8") if l.strip()]
record("V4_symbols",
       manifest.get("public_symbols") == symbols_txt == aud("static-symbols.txt")
       == aud("shared-symbols.txt") == aud("header-symbols.txt") and len(symbols_txt) == 15,
       "public_symbols == symbols.txt == static defined == shared exported == header set (%d symbols)"
       % len(symbols_txt))

# ---- V5: license posture
forbidden = [a for a in manifest["closure"]["configure_args"]
             if a.startswith(("--enable-gpl", "--enable-version3", "--enable-nonfree", "--enable-lib"))]
record("V5_license_args",
       not forbidden
       and manifest["license"]["gpl_enabled"] is False
       and manifest["license"]["version3_enabled"] is False
       and manifest["license"]["nonfree_enabled"] is False
       and manifest["license"]["external_libs"] == [],
       "no gpl/version3/nonfree/lib* configure arg; license flags false; forbidden=%s" % (forbidden or "none"))

# ---- V6: no reproducibility level beyond its receipt
rep = manifest.get("reproducibility", {})
levels = {"recipe": "RECIPE_RECONSTRUCTIBLE", "semantic_abi": "SEMANTIC_ABI_VERIFIED", "byte": "BYTE_REPRODUCIBLE"}
v6_bad, positive = [], False
for field, positive_value in levels.items():
    value = rep.get(field)
    if value == "NOT_ESTABLISHED":
        continue
    if value != positive_value:
        v6_bad.append("%s=%r" % (field, value))
        continue
    positive = True
    # A positive claim must name an existing receipt (contract section 8 V6).
    named = [t for t in str(rep.get("evidence", "")).replace(",", " ").split()
             if "/" in t or t.endswith(".json")]
    if not any(os.path.exists(os.path.join(work_root, t)) for t in named):
        v6_bad.append("%s claims %s without an existing evidence receipt" % (field, positive_value))
record("V6_reproducibility", not v6_bad,
       ("positive claims resolve to existing receipts" if positive
        else "every level is NOT_ESTABLISHED") + "; values=%s"
       % {k: rep.get(k) for k in levels})

# ---- V7: tools{} and target identity
ti = manifest.get("target_identity", {})
record("V7_tools",
       all(isinstance(manifest["tools"].get(k), str) and manifest["tools"][k].strip()
           for k in ("cc", "ar", "xmake"))
       and "not applicable" in manifest.get("sdk_ndk", "")
       and ti.get("toolchain", {}).get("family") == recipe["toolchain"]["family"]
       and ti.get("toolchain", {}).get("identity") == recipe["toolchain"]["identity"]
       and ti.get("recipe_sha256") == recipe_sha,
       "tools{} carries exact cc/ar/xmake versions; sdk_ndk disposition stated; target_identity.toolchain == recipe")

# ---- O1 / O2 / B4 license obligations
license_files = manifest["license"]["license_files"]
o1_bad = [n for n, h in license_files.items() if actual.get("LICENSES/" + n) != h]
record("O1_license_texts", not o1_bad and len(license_files) == 4,
       "the four FFmpeg COPYING texts are staged, hashed and bound in license_files; bad=%s" % (o1_bad or "none"))

record("O2_B3_notices",
       os.path.isfile(os.path.join(stage, "LICENSES/THIRD_PARTY_NOTICES.txt"))
       and "LICENSES/THIRD_PARTY_NOTICES.txt" in cksum,
       "THIRD_PARTY_NOTICES.txt present and hashed in checksums.txt")

b4_bad = [pkg for repo, pkg in (("LICENSE-MIT", "LICENSES/LICENSE-MIT"),
                                ("LICENSE-APACHE", "LICENSES/LICENSE-APACHE"))
          if sha256(os.path.join(work_root, repo)) != sha256(os.path.join(stage, pkg))]
record("B4_component_licenses", not b4_bad,
       "LICENSE-MIT/LICENSE-APACHE byte-identical to the repo-root texts at the source commit; bad=%s"
       % (b4_bad or "none"))

failed = [k for k, v in checks.items() if not v]
out = {"checks": checks, "details": details, "package_root": os.path.basename(stage),
       "packaged_files": len(packaged), "checksums_entries": len(cksum),
       "files_entries": len(manifest.get("files", {})),
       "passed": len(checks) - len(failed), "failed": len(failed),
       "verdict": "PASS" if not failed else "FAIL"}
with open(os.path.join(out_dir, "v-checks.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(out, f, indent=2)
    f.write("\n")
if failed:
    for k in failed:
        sys.stderr.write("FAIL %s: %s\n" % (k, details[k]))
    sys.exit(1)
print("V-CHECKS PASS (%d/%d)" % (len(checks), len(checks)))
PY

# ------------------------------------------- stage 6: deterministic container
log "stage6: writing $PKG_ROOT.tar.gz (deterministic)"
COMMIT_EPOCH=$(git -C "$WORK_ROOT" show -s --format=%ct "$SOURCE_COMMIT")
MTIME=${SOURCE_DATE_EPOCH:-$COMMIT_EPOCH}
python3 - "$STAGE" "$TARBALL" "$MTIME" <<'PY' || die "container write failed"
import gzip, io, os, sys, tarfile

stage, out, mtime = sys.argv[1], sys.argv[2], int(sys.argv[3])
pkg_root = os.path.basename(stage)

files = []
for root, _dirs, names in os.walk(stage):
    for n in names:
        files.append(os.path.relpath(os.path.join(root, n), stage).replace(os.sep, "/"))
files.sort()
dirs = set()
for p in files:
    parts = p.split("/")[:-1]
    for i in range(1, len(parts) + 1):
        dirs.add("/".join(parts[:i]))
# The versioned root directory is an explicit member, as in the v0.1.0
# container (contract section 7: exactly one versioned root directory).
members = sorted([("", "dir")] + [(p, "dir") for p in dirs] + [(p, "file") for p in files])

def check_name(rel):
    name = pkg_root if rel == "" else pkg_root + "/" + rel
    if len(name.encode("utf-8")) > 100:
        sys.stderr.write("FAIL: USTAR member name exceeds 100 bytes: %s\n" % name)
        sys.exit(41)
    return name

buf = io.BytesIO()
with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tf:
    for rel, kind in members:
        ti = tarfile.TarInfo(name=check_name(rel))
        ti.uid = 0
        ti.gid = 0
        ti.uname = ""
        ti.gname = ""
        ti.mtime = mtime
        if kind == "dir":
            ti.type = tarfile.DIRTYPE
            ti.mode = 0o755
            ti.size = 0
            tf.addfile(ti)
        else:
            data = open(os.path.join(stage, rel), "rb").read()
            ti.type = tarfile.REGTYPE
            ti.mode = 0o644
            ti.size = len(data)
            tf.addfile(ti, io.BytesIO(data))
with open(out, "wb") as f:
    f.write(gzip.compress(buf.getvalue(), compresslevel=9, mtime=0))
print("WROTE %s members=%d bytes=%d" % (out, len(members), os.path.getsize(out)))
PY

log "stage7: re-opening the container and verifying every entry"
python3 - "$TARBALL" "$STAGE" "$MTIME" <<'PY' || die "container re-verification failed"
import gzip, hashlib, io, json, os, sys, tarfile

tarball, stage, mtime = sys.argv[1], sys.argv[2], int(sys.argv[3])
pkg_root = os.path.basename(stage)
manifest = json.load(open(os.path.join(stage, "metadata/manifest.json"), encoding="utf-8"))
cksum = {}
for line in open(os.path.join(stage, "metadata/checksums.txt"), encoding="utf-8"):
    if line.strip():
        h, p = line.rstrip("\n").split("  ", 1)
        cksum[p] = h

tf = tarfile.open(fileobj=io.BytesIO(gzip.open(tarball, "rb").read()), mode="r:")
names = [m.name for m in tf.getmembers()]
if len(names) != len(set(names)):
    sys.stderr.write("FAIL: duplicate tar members\n"); sys.exit(51)
if names != sorted(names):
    sys.stderr.write("FAIL: tar members are not in sorted order\n"); sys.exit(52)

files = {}
for m in tf.getmembers():
    name = m.name
    if name != pkg_root and not name.startswith(pkg_root + "/"):
        sys.stderr.write("FAIL: member outside the versioned root: %s\n" % name); sys.exit(53)
    rel = "" if name == pkg_root else name[len(pkg_root) + 1:]
    if rel.startswith("/") or ".." in rel.split("/") or "\\" in name:
        sys.stderr.write("FAIL: absolute or traversal member: %s\n" % name); sys.exit(54)
    if m.uid != 0 or m.gid != 0 or m.uname != "" or m.gname != "":
        sys.stderr.write("FAIL: member ownership not normalised: %s\n" % name); sys.exit(55)
    if m.mtime != mtime:
        sys.stderr.write("FAIL: member mtime not fixed: %s\n" % name); sys.exit(56)
    if m.mode not in ((0o755,) if m.isdir() else (0o644,)):
        sys.stderr.write("FAIL: unexpected mode %o on %s\n" % (m.mode, name)); sys.exit(57)
    if not (m.isdir() or m.isreg()):
        sys.stderr.write("FAIL: non-regular member (link/symlink/device): %s\n" % name); sys.exit(58)
    if m.isreg():
        data = tf.extractfile(m).read()
        files[rel] = (hashlib.sha256(data).hexdigest(), len(data))

staged = []
for root, _dirs, names_ in os.walk(stage):
    for n in names_:
        staged.append(os.path.relpath(os.path.join(root, n), stage).replace(os.sep, "/"))
if sorted(files) != sorted(staged):
    sys.stderr.write("FAIL: container payload set != staged set\n"); sys.exit(59)

# The container is what ships: re-hash every payload from the archive itself.
# checksums.txt cannot cover itself (contract section 8 V1), so it is compared
# against the staged file that shipped, not against an entry of its own.
bad = []
for rel, (h, size) in files.items():
    if rel == "metadata/checksums.txt":
        if h != hashlib.sha256(open(os.path.join(stage, rel), "rb").read()).hexdigest():
            bad.append(rel + " (staged bytes)")
        continue
    if rel in manifest["files"]:
        if manifest["files"][rel]["sha256"] != h or manifest["files"][rel]["bytes"] != size:
            bad.append(rel)
    elif cksum.get(rel) != h:
        bad.append(rel + " (checksums.txt)")
if bad:
    sys.stderr.write("FAIL: container payload hashes differ: %s\n" % sorted(set(bad))); sys.exit(60)

print("CONTAINER VERIFIED files=%d entries=%d" % (len(files), len(tf.getmembers())))
PY

VERIFY_DIR=$OUT_DIR/verify-extract
rm -rf "$VERIFY_DIR"
mkdir -p "$VERIFY_DIR"
tar -xzf "$TARBALL" -C "$VERIFY_DIR" || die "extracting $TARBALL failed"
( cd "$VERIFY_DIR/$PKG_ROOT" && sha256sum -c metadata/checksums.txt ) > "$AUD/checksums-verify.log" 2>&1 \
  || { tail -5 "$AUD/checksums-verify.log" >&2; die "sha256sum -c metadata/checksums.txt failed inside the extracted container"; }
CHECKSUM_OK=$(grep -c ': OK$' "$AUD/checksums-verify.log" || true)
[ "$CHECKSUM_OK" = "$(grep -c . "$STAGE/metadata/checksums.txt")" ] \
  || die "sha256sum -c verified $CHECKSUM_OK entries, expected $(grep -c . "$STAGE/metadata/checksums.txt")"
log "stage7: sha256sum -c OK ($CHECKSUM_OK entries) on the extracted container"

TAR_SHA=$(sha_of "$TARBALL")
TAR_BYTES=$(stat -c %s "$TARBALL")

# ------------------------------------------------------------ stage 8: sidecars
printf '%s  %s.tar.gz\n' "$TAR_SHA" "$PKG_ROOT" > "$OUT_DIR/SHA256SUMS"
python3 - "$OUT_DIR" "$TARBALL" "$TAR_SHA" "$TAR_BYTES" "$SOURCE_COMMIT" "$VERSION" "$PKG_ROOT" \
  "$ARGV_JSON" <<'PY' || die "writing package-summary.json failed"
import json, os, sys
out_dir, tarball, sha, size, commit, version, pkg_root, argv_json = sys.argv[1:9]
vc = json.load(open(os.path.join(out_dir, "v-checks.json"), encoding="utf-8"))
manifest = json.load(open(os.path.join(out_dir, "stage", pkg_root, "metadata/manifest.json"),
                          encoding="utf-8"))
summary = {
    "entry": "ops/package-linux.sh",
    "argv": json.loads(argv_json),
    "version": version,
    "version_status": "caller-supplied; the next release number is owner-reserved (contract section 14 Q1); not a release",
    "package_root": pkg_root,
    "source_commit": commit,
    "tar": tarball,
    "tar_sha256": sha,
    "tar_bytes": int(size),
    "packaged_files": vc["packaged_files"],
    "checksums_entries": vc["checksums_entries"],
    "files_entries": vc["files_entries"],
    "build_receipt_sha256": manifest["source"]["build_receipt_sha256"],
    "artifact_static_sha256": manifest["sha256"],
    "artifact_shared_sha256": manifest["shared_sha256"],
    "v_checks": {"verdict": vc["verdict"], "passed": vc["passed"], "failed": vc["failed"]},
    "not_run": ["runtime/device smoke of the packaged consumer example (no media input in this lane)",
                "two-root reproducibility rebuild (contract section 12 levels stay NOT_ESTABLISHED)"],
    "ok": vc["verdict"] == "PASS",
}
with open(os.path.join(out_dir, "package-summary.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(summary, f, indent=2)
    f.write("\n")
print("PACKAGE_OK tar=%s sha256=%s bytes=%s files=%s checksums=%s files_entries=%s"
      % (tarball, sha, size, vc["packaged_files"], vc["checksums_entries"], vc["files_entries"]))
PY

log "PASS: $TARBALL sha256=$TAR_SHA bytes=$TAR_BYTES"
exit 0
