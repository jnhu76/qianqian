#!/usr/bin/env bash
# ops/package-linux.sh — Linux x86_64 SongCore package entry, per the frozen
# contract docs/release/songcore-release-contract.md:
#   section 2   VERSION_RULE (songcore-vMAJOR.MINOR.PATCH, decimal components
#               without leading zeros; songcore-v0.1.0 is immutable history and
#               is refused here)
#   section 6   ARTIFACT_KINDS (static lib/libsongcore.a mandatory, shared
#               lib/shared/libsongcore.so secondary — the linux-x86_64 recipe
#               declares artifacts.static=true and artifacts.shared=true, so
#               this entry requires both forms)
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
#      commit with a clean tracked tree (R2 discipline), ABI header identity
#      (work-root file == git blob at the commit == contract section 3), the
#      pinned FFmpeg zip re-verified against pin.json source_sha256, closure
#      identity re-verified against the pin/recipe/profile at the commit, and
#      every input present.
#   1. binutils audits: ELF identity, per-member archive format, the ABI export
#      set in BOTH forms, NEEDED audit, non-song export leak audit, glibc
#      symbol-version floor, and the archive-level closure-resolution audit
#      (defined minus undefined must leave no av*/sw* symbol unresolved).
#   2. LICENSES/: the four FFmpeg COPYING texts extracted from the
#      sha256-verified pinned source zip (O1); repo-root LICENSE-MIT /
#      LICENSE-APACHE copied byte-identically (B4); THIRD_PARTY_NOTICES.txt
#      rendered in the Issue #330 P3 wording (identity header for this
#      package, FFmpeg facts bound to this run's verified pin/closure, and the
#      dual-license component paragraph) instead of a stale hand-maintained
#      copy.
#   3. stage the section 7 tree under <out-dir>/stage/<pkg-root>/ and write
#      metadata/manifest.json (schema 2, bound to the staged bytes),
#      metadata/symbols.txt and metadata/checksums.txt.
#   4. V1-V7 + O1/O2/B4 re-verification by an independent re-read of the
#      written files (not the builder's own variables); any failure aborts with
#      a nonzero exit and no container.
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
#                        [--artifact-static <path>] [--artifact-shared <path>] \
#                        [--closure-manifest <path>] [--pinned-zip <path>] \
#                        [--consumer-example <path>] [--build-receipt <path>]
#
#   --version          REQUIRED, no default. The concrete next release number
#                      is owner-reserved (contract section 14 Q1); this entry
#                      never picks one, creates no tag and publishes nothing.
#   --source-commit    REQUIRED full 40-hex commit all inputs must come from.
#   --work-root        REQUIRED build root that ops/build-linux.sh produced (a
#                      checkout at --source-commit holding
#                      native/build/artifacts/{libsongcore.a,shared/libsongcore.so}
#                      and native/build/ffmpeg-xmake/manifest.json).
#   --out-dir          REQUIRED output directory (stage tree, container,
#                      sidecars, receipts).
#   --artifact-static  default <work-root>/native/build/artifacts/libsongcore.a
#   --artifact-shared  default <work-root>/native/build/artifacts/shared/libsongcore.so
#   --closure-manifest default <work-root>/native/build/ffmpeg-xmake/manifest.json
#   --pinned-zip       default <work-root>/native/build/downloads/ffmpeg-n9.0.1.zip
#                      (where scripts/fetch-ffmpeg writes the pinned zip)
#   --consumer-example default <work-root>/evidence/songcore-172/linux/verify/
#                      package/external-consumer-example.c (the tracked S3
#                      template)
#   --build-receipt    REQUIRED ops/build-linux.sh summary.json. Release-grade
#                      packaging never accepts unattested artifact bytes.
#
# Version: caller-supplied only. No tag, no GitHub Release, no publication.
#
# Exit code: 0 only when the container was written and re-verified.
set -Eeuo pipefail

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
FROZEN_ABI_HEADER_SHA256=cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536
IMMUTABLE_HISTORICAL_VERSION=0.1.0

log() { printf '[package-linux] %s\n' "$*"; }
die() { printf 'package-linux: FAIL: %s\n' "$*" >&2; exit 1; }
kv()  { grep -m1 "^$2=" "$1" | cut -d= -f2-; }

usage() {
  sed -n '/^# Usage:/,/^# Version:/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version)          [ $# -ge 2 ] || die "--version needs a value";           VERSION=$2; shift 2 ;;
    --source-commit)    [ $# -ge 2 ] || die "--source-commit needs a value";     SOURCE_COMMIT=$2; shift 2 ;;
    --work-root)        [ $# -ge 2 ] || die "--work-root needs a value";         WORK_ROOT=$2; shift 2 ;;
    --out-dir)          [ $# -ge 2 ] || die "--out-dir needs a value";           OUT_DIR=$2; shift 2 ;;
    --artifact-static)  [ $# -ge 2 ] || die "--artifact-static needs a value";   ART_STATIC=$2; shift 2 ;;
    --artifact-shared)  [ $# -ge 2 ] || die "--artifact-shared needs a value";   ART_SHARED=$2; shift 2 ;;
    --closure-manifest) [ $# -ge 2 ] || die "--closure-manifest needs a value";  CLOSURE_MANIFEST=$2; shift 2 ;;
    --pinned-zip)       [ $# -ge 2 ] || die "--pinned-zip needs a value";        PINNED_ZIP=$2; shift 2 ;;
    --consumer-example) [ $# -ge 2 ] || die "--consumer-example needs a value";  CONSUMER_EXAMPLE=$2; shift 2 ;;
    --build-receipt)    [ $# -ge 2 ] || die "--build-receipt needs a value";     BUILD_RECEIPT=$2; shift 2 ;;
    -h|--help)          usage; exit 0 ;;
    *) printf 'package-linux: unknown argument: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

# ------------------------------------------------------------------ preflight
[ -n "$VERSION" ] || die "--version is required: this entry has no default version (the next release number is owner-reserved, contract section 14 Q1)"
[ "$VERSION" != "$IMMUTABLE_HISTORICAL_VERSION" ] \
  || die "refusing version '$VERSION': songcore-v0.1.0 is immutable history (contract section 2); a failed release releases under a new version, never a re-tag"
[[ "$VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || die "version '$VERSION' is not the authorized MAJOR.MINOR.PATCH identity (contract section 2; suffixes are not authorized)"
IFS=. read -r V_MAJOR V_MINOR V_PATCH <<<"$VERSION"
for c in "$V_MAJOR" "$V_MINOR" "$V_PATCH"; do
  [[ "$c" =~ ^(0|[1-9][0-9]*)$ ]] \
    || die "version '$VERSION' has a leading-zero or non-decimal component '$c' (contract section 2)"
done

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
python3 - "$VERSION_DECISION" "$VERSION" <<'PY' || die "version identity is not approved for release-qualified packaging"
import json, re, sys
p, requested = sys.argv[1:]
d = json.load(open(p, encoding="utf-8"))
if d.get("schema_version") != 1 or d.get("status") != "APPROVED":
    raise SystemExit("Q1 version identity is OWNER_BLOCKED")
if not isinstance(d.get("authority_ref"), str) or not d["authority_ref"].strip():
    raise SystemExit("approved version decision lacks authority_ref")
v = d.get("semantic_version")
if v != requested or not isinstance(v, str) or not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", v):
    raise SystemExit("requested version does not equal the approved semantic_version")
expected = {
    "component_tag": f"songcore-v{v}", "manifest_version": v,
    "linux_package_root": f"songcore-v{v}-linux-x86_64",
    "windows_package_root": f"songcore-v{v}-windows-x86_64",
    "component_version": v,
}
for key, value in expected.items():
    if d.get(key) != value: raise SystemExit(f"version mapping mismatch: {key}")
mode = d.get("component_version_mode")
if mode not in ("independent", "project-coupled"):
    raise SystemExit("component_version_mode is not approved")
if mode == "project-coupled" and d.get("xmake_project_version") != v:
    raise SystemExit("project-coupled version does not match xmake_project_version")
PY

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

for t in git python3 sha256sum ar nm objdump readelf file tar gzip comm; do
  command -v "$t" >/dev/null 2>&1 || die "missing required tool: $t"
done

PKG_ROOT="songcore-v$VERSION-linux-x86_64"
STAGE=$OUT_DIR/stage/$PKG_ROOT
TARBALL=$OUT_DIR/$PKG_ROOT.tar.gz
mkdir -p "$OUT_DIR/audit"
rm -rf "$OUT_DIR/stage" "$OUT_DIR/verify-extract"
mkdir -p "$STAGE"

AUD=$OUT_DIR/audit
FACTS=$AUD/facts.env
: > "$FACTS"
fact() { printf '%s=%s\n' "$1" "$2" >> "$FACTS"; }

log "version $VERSION (caller-supplied; not a release)"
log "source  $SOURCE_COMMIT @ $WORK_ROOT"

# ABI header identity: work-root file == git blob at the commit == contract §3.
HEADER_SHA=$(sha256sum "$HEADER" | awk '{print $1}')
GIT_HEADER_SHA=$(git -C "$WORK_ROOT" show "$SOURCE_COMMIT:native/include/songcore.h" | sha256sum | awk '{print $1}')
[ "$HEADER_SHA" = "$FROZEN_ABI_HEADER_SHA256" ] \
  || die "ABI header drift: $HEADER (sha256 $HEADER_SHA) != frozen contract section 3 identity $FROZEN_ABI_HEADER_SHA256"
[ "$GIT_HEADER_SHA" = "$HEADER_SHA" ] \
  || die "ABI header at the source commit ($GIT_HEADER_SHA) != the work-root file ($HEADER_SHA)"

PIN_SHA=$(sha256sum "$PIN" | awk '{print $1}')
RECIPE_SHA=$(sha256sum "$RECIPE" | awk '{print $1}')
PROFILE_SHA=$(sha256sum "$PROFILE" | awk '{print $1}')
CLOSURE_SHA=$(sha256sum "$CLOSURE_MANIFEST" | awk '{print $1}')
ART_STATIC_SHA=$(sha256sum "$ART_STATIC" | awk '{print $1}')
ART_SHARED_SHA=$(sha256sum "$ART_SHARED" | awk '{print $1}')
ART_STATIC_BYTES=$(stat -c %s "$ART_STATIC")
ART_SHARED_BYTES=$(stat -c %s "$ART_SHARED")
ZIP_SHA=$(sha256sum "$PINNED_ZIP" | awk '{print $1}')

python3 - "$PIN" > "$AUD/pin-values.env" <<'PY' || die "cannot read $PIN"
import json, sys
p = json.load(open(sys.argv[1], encoding="utf-8"))
for k in ("ffmpeg_tag", "ffmpeg_commit_sha", "ffmpeg_version", "source_sha256", "source_url"):
    print("%s=%s" % (k.upper(), str(p[k]).replace("\n", " ")))
PY
FFMPEG_TAG=$(kv "$AUD/pin-values.env" FFMPEG_TAG)
FFMPEG_COMMIT=$(kv "$AUD/pin-values.env" FFMPEG_COMMIT_SHA)
FFMPEG_VERSION=$(kv "$AUD/pin-values.env" FFMPEG_VERSION)
FFMPEG_SOURCE_SHA=$(kv "$AUD/pin-values.env" SOURCE_SHA256)
FFMPEG_SOURCE_URL=$(kv "$AUD/pin-values.env" SOURCE_URL)

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
STATIC_SYMBOLS=$(grep -c . "$AUD/static-symbols.txt" || true)
[ "$STATIC_SYMBOLS" = "$HEADER_SYMBOLS" ] \
  || die "static artifact defines $STATIC_SYMBOLS song* T symbols, header declares $HEADER_SYMBOLS"
diff "$AUD/header-symbols.txt" "$AUD/static-symbols.txt" > "$AUD/symbols-static.diff" \
  || die "static symbol set != the songcore.h ABI set (see $AUD/symbols-static.diff)"

nm -D --defined-only "$ART_SHARED" | awk '$2=="T"||$2=="W"{print $3}' | grep '^song' | sort -u > "$AUD/shared-symbols.txt" || true
SHARED_SYMBOLS=$(grep -c . "$AUD/shared-symbols.txt" || true)
[ "$SHARED_SYMBOLS" = "$HEADER_SYMBOLS" ] \
  || die "shared artifact exports $SHARED_SYMBOLS song* symbols, header declares $HEADER_SYMBOLS"
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

# Contract section 10 O4 / V5: the closure must stay copylib-free.
FORBIDDEN_ARGS=$(python3 - "$CLOSURE_MANIFEST" <<'PY'
import json, sys
m = json.load(open(sys.argv[1], encoding="utf-8"))
print(" ".join(a for a in m.get("configure_args", [])
               if a.startswith(("--enable-gpl", "--enable-version3", "--enable-nonfree", "--enable-lib"))))
PY
) || die "cannot read configure_args from $CLOSURE_MANIFEST"
[ -z "$FORBIDDEN_ARGS" ] \
  || die "closure configure_args enable gpl/version3/nonfree/lib*: $FORBIDDEN_ARGS (contract section 10 O4)"

python3 - "$CLOSURE_MANIFEST" "$RECIPE" "$PROFILE" "$FFMPEG_TAG" "$FFMPEG_COMMIT" "$FFMPEG_SOURCE_SHA" \
  > "$AUD/closure-identity.env" <<'PY' || die "the imported FFmpeg closure identity does not match the pin/recipe/profile at the source commit"
import hashlib, json, platform, sys
man = json.load(open(sys.argv[1], encoding="utf-8"))
rec = json.load(open(sys.argv[2], encoding="utf-8"))
rec_sha = hashlib.sha256(open(sys.argv[2], 'rb').read()).hexdigest()
prof_sha = hashlib.sha256(open(sys.argv[3], 'rb').read()).hexdigest()
t = man.get("target", {})
errs = []
if man.get("schema") != 2: errs.append("closure manifest schema != 2")
if t.get("id") != "linux-x86_64": errs.append("closure target.id != linux-x86_64")
if t.get("platform") != rec.get("platform"): errs.append("closure platform != recipe platform")
if t.get("arch") != rec.get("arch"): errs.append("closure arch != recipe arch")
if t.get("target_os") != rec.get("ffmpeg", {}).get("target_os"): errs.append("closure target_os != recipe target_os")
if t.get("recipe_sha256") != rec_sha: errs.append("closure recipe_sha256 != the recipe at the source commit")
if man.get("profile_sha256") != prof_sha: errs.append("closure profile_sha256 != the codec-base profile at the source commit")
if man.get("ffmpeg_tag") != sys.argv[4]: errs.append("closure ffmpeg_tag != pin.json")
if man.get("ffmpeg_commit_sha") != sys.argv[5]: errs.append("closure ffmpeg_commit != pin.json")
if man.get("ffmpeg_source_sha256") != sys.argv[6]: errs.append("closure ffmpeg_source_sha256 != pin.json")
if man.get("toolchain", {}).get("system") != platform.system().lower(): errs.append("closure toolchain.system != this host")
if man.get("toolchain", {}).get("machine") != platform.machine().lower(): errs.append("closure toolchain.machine != this host")
if errs:
    print("FAIL: " + "; ".join(errs), file=sys.stderr)
    sys.exit(1)
tc = man["toolchain"]
print("CLOSURE_MAN_SHA=%s" % hashlib.sha256(open(sys.argv[1], 'rb').read()).hexdigest())
print("CLOSURE_CC=%s" % tc.get("cc"))
print("CLOSURE_CC_IDENT=%s" % tc.get("cc_ident"))
print("CLOSURE_ARCH=%s" % tc.get("arch"))
print("CLOSURE_TARGET_OS=%s" % (tc.get("target_os") or ""))
print("CLOSURE_UNITS=%d" % man["closure"]["translation_units"])
print("CLOSURE_UPSTREAM=%d" % man["closure"]["upstream_sources"])
print("CLOSURE_GENERATED=%d" % man["closure"]["generated_sources"])
print("CLOSURE_NARGS=%d" % len(man.get("configure_args", [])))
PY
CL_CLOSURE_MAN_SHA=$(kv "$AUD/closure-identity.env" CLOSURE_MAN_SHA)
[ -n "$CL_CLOSURE_MAN_SHA" ] || die "closure identity file is missing CLOSURE_MAN_SHA"

CC_IDENT=$(cc --version | head -1)
AR_IDENT=$(ar --version | head -1)
if command -v xmake >/dev/null 2>&1; then
  XMAKE_IDENT=$(xmake --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g')
else
  XMAKE_IDENT="not found on this packaging host (xmake is not a packaging input)"
fi

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

# THIRD_PARTY_NOTICES.txt — Issue #330 P3 wording: an identity header for THIS
# package, FFmpeg facts bound to this run's verified pin/closure/profile, and
# the dual-license component paragraph (verbatim from ops/package-rc.ps1's P3
# disposition). No license text is hand-maintained here.
NOTICES_TITLE="Third-party notices — SongCore v$VERSION (linux-x86_64)"
python3 - "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt" "$NOTICES_TITLE" \
  "$FFMPEG_VERSION" "$FFMPEG_TAG" "$FFMPEG_COMMIT" "$FFMPEG_SOURCE_URL" \
  "$FFMPEG_SOURCE_SHA" "$PROFILE_SHA" <<'PY' || die "rendering THIRD_PARTY_NOTICES.txt failed"
import sys
path, title, version, tag, commit, url, sha, profile_sha = sys.argv[1:9]
lines = [
    title,
    "=" * len(title),
    "",
    "FFmpeg",
    "  version : %s (tag %s)" % (version, tag),
    "  commit  : %s" % commit,
    "  source  : %s" % url,
    "  sha256  : %s" % sha,
    "  profile : codec-base (machine-derived minimal closure; sha256",
    "            %s)" % profile_sha,
    "  effective license: LGPL-2.1-or-later (FFmpeg default, all copylibs disabled)",
    "  external libraries that alter licensing: none",
    "",
    "  The FFmpeg closure is statically linked inside libsongcore.a /",
    "  libsongcore.so. Under LGPL-2.1-or-later static distribution, the object",
    "  files of the application may be provided together with the means",
    "  (source or relink instructions) to re-link a modified FFmpeg. This",
    "  package contains the complete FFmpeg license texts (LICENSES/COPYING.*)",
    "  and the closure identity above; the exact source tree is reconstructible",
    "  from the pinned URL + sha256.",
    "",
    "Qianqian",
    "  The Qianqian sources (songcore.h and the SongCore implementation) are",
    "  dual-licensed: MIT or Apache-2.0, at the recipient's option. The",
    "  complete license texts ship in this package as LICENSES/LICENSE-MIT",
    "  and LICENSES/LICENSE-APACHE and are sha256-bound in",
    "  metadata/checksums.txt. This is an engineering statement, not legal",
    "  advice; each release's license block is owner-reviewed before",
    "  publication (contract section 10).",
    "",
]
open(path, "w", encoding="utf-8", newline="\n").write("\n".join(lines))
PY

# ------------------------------------------------------ stage 3: payload tree
log "stage3: staging the section 7 tree"
mkdir -p "$STAGE/include" "$STAGE/lib/shared" "$STAGE/metadata"
cp -f "$HEADER"           "$STAGE/include/songcore.h"
cp -f "$ART_STATIC"       "$STAGE/lib/libsongcore.a"
cp -f "$ART_SHARED"       "$STAGE/lib/shared/libsongcore.so"
cp -f "$CONSUMER_EXAMPLE" "$STAGE/metadata/external-consumer-example.c"
cp -f "$AUD/header-symbols.txt" "$STAGE/metadata/symbols.txt"

[ "$(sha256sum "$STAGE/include/songcore.h" | awk '{print $1}')" = "$HEADER_SHA" ] \
  || die "staged include/songcore.h sha changed in staging"
[ "$(sha256sum "$STAGE/lib/libsongcore.a" | awk '{print $1}')" = "$ART_STATIC_SHA" ] \
  || die "staged lib/libsongcore.a sha changed in staging"
[ "$(sha256sum "$STAGE/lib/shared/libsongcore.so" | awk '{print $1}')" = "$ART_SHARED_SHA" ] \
  || die "staged lib/shared/libsongcore.so sha changed in staging"

# Artifact provenance (the audit's artifact_attribution finding: a packager
# must not stamp a stale artifact with the current commit's name).
PROVENANCE_ATTESTED=false
BUILD_RECEIPT_SHA=""
SOURCE_TREE=$(git -C "$WORK_ROOT" rev-parse 'HEAD^{tree}')
TOOLCHAIN_ID="gcc=$(gcc --version | head -1);xmake=$(xmake --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g')"
python3 "$WORK_ROOT/tools/verify_songcore_build_receipt.py" \
  --receipt "$BUILD_RECEIPT" --source-commit "$SOURCE_COMMIT" --source-tree "$SOURCE_TREE" \
  --artifact "$ART_STATIC" --artifact-shared "$ART_SHARED" --header "$HEADER" \
  --recipe "$RECIPE" --profile "$PROFILE" --pin "$PIN" --closure "$CLOSURE_MANIFEST" \
  --target linux-x86_64 --toolchain "$TOOLCHAIN_ID" \
  || die "build receipt $BUILD_RECEIPT does not bind these artifact bytes and inputs to $SOURCE_COMMIT"
PROVENANCE_ATTESTED=true
BUILD_RECEIPT_SHA=$(sha256sum "$BUILD_RECEIPT" | awk '{print $1}')

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
The version string is the one the packaging caller supplied. The concrete next
release number is owner-reserved (contract section 14 Q1); packaging creates no
tag, no GitHub Release and no other published asset.

This archive makes no platform-support or release-readiness claim. G4 is not
approved; RC build is not authorized; no tag or Release was created.

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

# -------------------------------------------------------- facts for stage 4/5
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

fact version "$VERSION"
fact source_commit "$SOURCE_COMMIT"
fact pkg_root "$PKG_ROOT"
fact header_sha256 "$HEADER_SHA"
fact static_sha256 "$ART_STATIC_SHA"
fact static_bytes "$ART_STATIC_BYTES"
fact shared_sha256 "$ART_SHARED_SHA"
fact shared_bytes "$ART_SHARED_BYTES"
fact archive_members "$MEMBER_COUNT"
fact elf_members "$ELF_MEMBERS"
fact recipe_sha256 "$RECIPE_SHA"
fact profile_sha256 "$PROFILE_SHA"
fact pin_sha256 "$PIN_SHA"
fact closure_manifest_sha256 "$CLOSURE_SHA"
fact ffmpeg_tag "$FFMPEG_TAG"
fact ffmpeg_commit "$FFMPEG_COMMIT"
fact ffmpeg_version "$FFMPEG_VERSION"
fact ffmpeg_source_sha256 "$FFMPEG_SOURCE_SHA"
fact cc_ident "$CC_IDENT"
fact ar_ident "$AR_IDENT"
fact xmake_ident "$XMAKE_IDENT"
fact glibc_floor "$GLIBC_FLOOR"
fact undefined_count "$UNDEFINED_COUNT"
fact built_utc "$BUILT_UTC"
fact provenance_attested "$PROVENANCE_ATTESTED"
fact build_receipt_sha256 "$BUILD_RECEIPT_SHA"
fact closure_cc "$(kv "$AUD/closure-identity.env" CLOSURE_CC)"
fact closure_cc_ident "$(kv "$AUD/closure-identity.env" CLOSURE_CC_IDENT)"
fact closure_arch "$(kv "$AUD/closure-identity.env" CLOSURE_ARCH)"
fact closure_target_os "$(kv "$AUD/closure-identity.env" CLOSURE_TARGET_OS)"
fact closure_units "$(kv "$AUD/closure-identity.env" CLOSURE_UNITS)"
fact closure_upstream "$(kv "$AUD/closure-identity.env" CLOSURE_UPSTREAM)"
fact closure_generated "$(kv "$AUD/closure-identity.env" CLOSURE_GENERATED)"
fact closure_nargs "$(kv "$AUD/closure-identity.env" CLOSURE_NARGS)"
fact argv_json "$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1:]))' "${ARGV[@]}")"

# ------------------------------- stage 4: manifest + symbols + checksums
log "stage4: writing metadata/manifest.json (schema 2), symbols.txt, checksums.txt"
python3 - "$STAGE" "$OUT_DIR" "$WORK_ROOT" <<'PY' || die "manifest/checksums binding failed"
import hashlib, json, os, sys

stage, out_dir, work_root = sys.argv[1:4]
facts = {}
for line in open(os.path.join(out_dir, "audit", "facts.env"), encoding="utf-8"):
    k, _, v = line.rstrip("\n").partition("=")
    facts[k] = v

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

EXPECTED = {
    "include/songcore.h", "lib/libsongcore.a", "lib/shared/libsongcore.so",
    "LICENSES/COPYING.GPLv2", "LICENSES/COPYING.GPLv3",
    "LICENSES/COPYING.LGPLv2.1", "LICENSES/COPYING.LGPLv3",
    "LICENSES/THIRD_PARTY_NOTICES.txt", "LICENSES/LICENSE-MIT",
    "LICENSES/LICENSE-APACHE", "README.md",
    "metadata/manifest.json", "metadata/symbols.txt", "metadata/checksums.txt",
    "metadata/external-consumer-example.c",
}
assert len(EXPECTED) == 15, len(EXPECTED)

packaged = []
for root, _dirs, files in os.walk(stage):
    for name in files:
        packaged.append(os.path.relpath(os.path.join(root, name), stage).replace(os.sep, "/"))
packaged.sort()
# manifest.json and checksums.txt are written by this step, so they are still
# missing from the walked set; every other member must already be present.
PENDING = {"metadata/manifest.json", "metadata/checksums.txt"}
missing = EXPECTED - set(packaged) - PENDING
extra = set(packaged) - EXPECTED
if missing or extra:
    sys.stderr.write("FAIL: staged content set != contract section 7 set: missing=%s extra=%s\n"
                     % (sorted(missing), sorted(extra)))
    sys.exit(30)

pin = json.load(open(os.path.join(work_root, "native/ffmpeg/pin.json"), encoding="utf-8"))
recipe = json.load(open(os.path.join(work_root, "native/ffmpeg/targets/linux-x86_64.json"), encoding="utf-8"))
closure = json.load(open(os.path.join(work_root, "native/build/ffmpeg-xmake/manifest.json"), encoding="utf-8"))
symbols = [l.strip() for l in open(os.path.join(stage, "metadata/symbols.txt"), encoding="utf-8") if l.strip()]
needed = [l.strip() for l in open(os.path.join(out_dir, "audit/needed-libs.txt"), encoding="utf-8") if l.strip()]
license_files = {n: sha256(os.path.join(stage, "LICENSES", n))
                 for n in ("COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3")}

attested = facts.get("provenance_attested") == "true"
if attested:
    provenance_note = ("Artifact provenance IS attested: --build-receipt (sha256 %s) reports head %s "
                       "and both artifact sha256 values were re-verified against the packaged bytes at "
                       "packing time." % (facts.get("build_receipt_sha256"), facts["source_commit"]))
else:
    provenance_note = ("Artifact provenance is NOT attested: --build-receipt was not supplied, so git_sha "
                       "records the source tree this package was staged from, not a verified rebuild of "
                       "these artifact bytes (package-summary.json records "
                       "artifact_provenance_attested=false).")

manifest = {
    "schema_version": 2,
    "songcore_version": facts["version"],
    "songcore_abi": 1,
    "abi_header_sha256": facts["header_sha256"],
    "target": "linux-x86_64",
    "platform": "linux",
    "arch": "x86_64",
    "toolchain": recipe["toolchain"]["family"],
    "ffmpeg_version": pin["ffmpeg_version"],
    "ffmpeg_commit": pin["ffmpeg_commit_sha"],
    "ffmpeg_source_sha256": pin["source_sha256"],
    "ffmpeg_profile": "codec-base",
    "ffmpeg_profile_sha256": facts["profile_sha256"],
    "artifact": "lib/libsongcore.a",
    "sha256": facts["static_sha256"],
    "artifact_bytes": int(facts["static_bytes"]),
    "archive_members": int(facts["archive_members"]),
    # v0.1.0 carried artifact_shared as the shared artifact PATH; that meaning
    # is preserved (contract section 8: carried v0.1.0 field names keep their
    # meaning). windows-x86_64 ships no shared form and records a boolean.
    "artifact_shared": "lib/shared/libsongcore.so",
    "shared_sha256": facts["shared_sha256"],
    "public_symbols": sorted(symbols),
    "git_sha": facts["source_commit"],
    "target_identity": {
        "id": recipe["id"],
        "platform": recipe["platform"],
        "arch": recipe["arch"],
        "triple": "x86_64-unknown-linux-gnu",
        "target_os": recipe["ffmpeg"]["target_os"],
        "cross_prefix": recipe["ffmpeg"].get("cross_prefix"),
        "toolchain": {"family": recipe["toolchain"]["family"],
                      "identity": recipe["toolchain"]["identity"]},
        "recipe_sha256": facts["recipe_sha256"],
    },
    "tools": {"cc": facts["cc_ident"], "ar": facts["ar_ident"], "xmake": facts["xmake_ident"]},
    "sdk_ndk": ("not applicable this round — linux-x86_64: host-native, no SDK/NDK "
                "(target recipe toolchain identity, contract section 8)"),
    "source": {"commit": facts["source_commit"], "status": "clean"},
    "pin": {
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "pin_sha256": facts["pin_sha256"],
    },
    "closure": {
        "manifest_sha256": facts["closure_manifest_sha256"],
        "toolchain": {
            "system": closure["toolchain"].get("system"),
            "machine": closure["toolchain"].get("machine"),
            "cc": facts["closure_cc"],
            "cc_ident": facts["closure_cc_ident"],
            "arch": facts["closure_arch"],
            "target_os": facts["closure_target_os"] or None,
        },
        "configure_args": closure["configure_args"],
        "units": {
            "translation_units": int(facts["closure_units"]),
            "upstream_sources": int(facts["closure_upstream"]),
            "generated_sources": int(facts["closure_generated"]),
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
                                   "itself; no av*/sw* FFmpeg symbol is left unresolved" % facts["undefined_count"]),
        },
        "shared": {
            "needed_libs": needed,
            "glibc_symbol_version_floor": (facts["glibc_floor"] + " (readelf --dyn-syms; the supported "
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
    "built_utc": facts["built_utc"],
    "packer": "release-substrate songcore-172 G4 linux package lane (ops/package-linux.sh, deterministic tar container)",
    "notes": ("songcore_version %s was supplied by the packaging caller; the concrete next release number "
              "is owner-reserved (contract section 14 Q1), this is a release-substrate verification "
              "archive, not a release, and nothing here is published. %s files{} covers the checksums.txt "
              "set minus manifest.json itself (non-circular binding; the set arithmetic contract section 8 "
              "V1 states after the Issue #330 D2 disposition). LICENSES/COPYING.* were extracted from the "
              "pinned n9.0.1 source zip re-verified against pin.json source_sha256 on this run (O1); "
              "LICENSES/LICENSE-MIT and LICENSES/LICENSE-APACHE are byte-identical to the repo-root texts at "
              "the source commit (B4); THIRD_PARTY_NOTICES.txt uses the Issue #330 P3 wording. Machine "
              "license checks verify presence, hash identity and configure-arg provenance only; they cannot "
              "certify legal sufficiency (contract section 10 limit)."
              % (facts["version"], provenance_note)),
}

covered_mf = [p for p in packaged if p not in ("metadata/checksums.txt", "metadata/manifest.json")]
manifest["files"] = {p: {"sha256": sha256(os.path.join(stage, p)),
                         "bytes": os.path.getsize(os.path.join(stage, p))}
                     for p in covered_mf}

manifest_path = os.path.join(stage, "metadata", "manifest.json")
with open(manifest_path, "w", encoding="utf-8", newline="\n") as f:
    json.dump(manifest, f, indent=2, ensure_ascii=False)
    f.write("\n")

covered_ck = sorted(list(packaged) + ["metadata/manifest.json"])
assert "metadata/checksums.txt" not in covered_ck
hashes = {p: sha256(os.path.join(stage, p)) for p in covered_ck}
with open(os.path.join(stage, "metadata", "checksums.txt"), "w", encoding="utf-8", newline="\n") as f:
    for p in covered_ck:
        f.write("%s  %s\n" % (hashes[p], p))
print("BOUND files=%d checksums=%d" % (len(covered_mf), len(covered_ck)))
PY

# ------------------------------------ stage 5: V1-V7 + O1/O2/B4 verification
log "stage5: V1-V7 + O1/O2/B4 re-verification of the written files"
python3 - "$STAGE" "$OUT_DIR" "$WORK_ROOT" "$SOURCE_COMMIT" "$FROZEN_ABI_HEADER_SHA256" <<'PY' \
  || die "contract section 8 machine checks failed (see $OUT_DIR/v-checks.json)"
import hashlib, json, os, sys

stage, out_dir, work_root, source_commit, frozen_header = sys.argv[1:6]
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

facts = {}
for line in open(os.path.join(out_dir, "audit", "facts.env"), encoding="utf-8"):
    k, _, v = line.rstrip("\n").partition("=")
    facts[k] = v

manifest = json.load(open(os.path.join(stage, "metadata/manifest.json"), encoding="utf-8"))
cksum = {}
for line in open(os.path.join(stage, "metadata/checksums.txt"), encoding="utf-8"):
    if line.strip():
        h, p = line.rstrip("\n").split("  ", 1)
        if p in cksum:
            record("V1_sets", False, "duplicate checksums.txt entry: %s" % p)
        cksum[p] = h

packaged = []
for root, _dirs, files in os.walk(stage):
    for name in files:
        packaged.append(os.path.relpath(os.path.join(root, name), stage).replace(os.sep, "/"))
packaged.sort()

# ---- V1: files{}/checksums.txt vs the real bytes; no self-hash (#330 D2)
rehash_bad = []
for p, ent in manifest.get("files", {}).items():
    full = os.path.join(stage, p)
    if not os.path.isfile(full):
        rehash_bad.append("%s missing" % p); continue
    if sha256(full) != ent.get("sha256"):
        rehash_bad.append("%s sha256" % p)
    if os.path.getsize(full) != ent.get("bytes"):
        rehash_bad.append("%s bytes" % p)
record("V1_files_rehash", not rehash_bad,
       "files{} entries re-hash to the packaged bytes; bad=%s" % (rehash_bad or "none"))

cksum_expected = sorted(p for p in packaged if p != "metadata/checksums.txt")
record("V1_checksums_set", sorted(cksum) == cksum_expected,
       "checksums.txt entries (%d) == packaged files minus itself (%d)" % (len(cksum), len(cksum_expected)))

files_expected = sorted(p for p in cksum if p != "metadata/manifest.json")
record("V1_files_set", sorted(manifest.get("files", {})) == files_expected,
       "files{} keys (%d) == checksums.txt entries minus manifest.json (%d)"
       % (len(manifest.get("files", {})), len(files_expected)))

record("V1_no_self_hash",
       "metadata/checksums.txt" not in cksum
       and "metadata/checksums.txt" not in manifest.get("files", {})
       and "metadata/manifest.json" not in manifest.get("files", {}),
       "checksums.txt does not hash itself; files{} hashes neither checksums.txt nor manifest.json")

record("V1_checksums_match_bytes",
       all(sha256(os.path.join(stage, p)) == h for p, h in cksum.items()),
       "every checksums.txt entry equals the sha256 of the packaged bytes")

# ---- V2: header identity (packaged == source commit blob == contract section 3)
staged_header = sha256(os.path.join(stage, "include/songcore.h"))
record("V2_header_sha",
       staged_header == manifest.get("abi_header_sha256")
       and staged_header == facts["header_sha256"]
       and staged_header == frozen_header,
       "packaged header %s == manifest == work root == contract section 3" % staged_header)

# ---- V3: pin.*/closure.* agree with the source-commit inputs
pin = json.load(open(os.path.join(work_root, "native/ffmpeg/pin.json"), encoding="utf-8"))
recipe = json.load(open(os.path.join(work_root, "native/ffmpeg/targets/linux-x86_64.json"), encoding="utf-8"))
closure = json.load(open(os.path.join(work_root, "native/build/ffmpeg-xmake/manifest.json"), encoding="utf-8"))
v3 = []
if manifest.get("ffmpeg_commit") != manifest["pin"]["ffmpeg_commit_sha"]: v3.append("ffmpeg_commit carry")
if manifest.get("ffmpeg_commit") != pin["ffmpeg_commit_sha"]: v3.append("ffmpeg_commit vs pin.json")
if manifest.get("ffmpeg_source_sha256") != manifest["pin"]["ffmpeg_source_sha256"]: v3.append("source sha carry")
if manifest.get("ffmpeg_source_sha256") != pin["source_sha256"]: v3.append("source sha vs pin.json")
if manifest["pin"].get("pin_sha256") != facts["pin_sha256"]: v3.append("pin_sha256")
if manifest["closure"].get("manifest_sha256") != facts["closure_manifest_sha256"]: v3.append("closure manifest sha")
if manifest.get("ffmpeg_profile_sha256") != facts["profile_sha256"]: v3.append("profile sha")
if manifest["target_identity"].get("recipe_sha256") != facts["recipe_sha256"]: v3.append("recipe sha")
if manifest["closure"].get("configure_args") != closure.get("configure_args"): v3.append("configure_args")
record("V3_pin_binding", not v3,
       "pin.*/closure.* agree with the source-commit inputs; mismatch=%s" % (v3 or "none"))

# ---- V4: the 15 ABI symbols, in both forms and in symbols.txt
symbols_txt = [l.strip() for l in open(os.path.join(stage, "metadata/symbols.txt"), encoding="utf-8") if l.strip()]
aud = lambda n: [l.strip() for l in open(os.path.join(out_dir, "audit", n), encoding="utf-8") if l.strip()]
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
v6_bad = []
for field, positive in levels.items():
    value = rep.get(field)
    if value == "NOT_ESTABLISHED":
        continue
    if value != positive:
        v6_bad.append("%s=%r" % (field, value))
    elif not rep.get("evidence"):
        v6_bad.append("%s claims %s without an evidence receipt" % (field, positive))
record("V6_reproducibility", not v6_bad and bool(rep.get("evidence")),
       "no reproducibility level is claimed beyond its receipt; values=%s" % {k: rep.get(k) for k in levels})

# ---- V7: tools{} and target identity
ti = manifest.get("target_identity", {})
record("V7_tools",
       all(isinstance(manifest["tools"].get(k), str) and manifest["tools"][k].strip()
           for k in ("cc", "ar", "xmake"))
       and "not applicable" in manifest.get("sdk_ndk", "")
       and ti.get("toolchain", {}).get("family") == recipe["toolchain"]["family"]
       and ti.get("toolchain", {}).get("identity") == recipe["toolchain"]["identity"]
       and ti.get("recipe_sha256") == facts["recipe_sha256"],
       "tools{} carries exact cc/ar/xmake versions; sdk_ndk disposition stated; target_identity.toolchain == recipe")

# ---- O1 / O2 / B4 license obligations
license_files = manifest["license"]["license_files"]
o1_bad = [n for n, h in license_files.items()
          if not os.path.isfile(os.path.join(stage, "LICENSES", n))
          or sha256(os.path.join(stage, "LICENSES", n)) != h]
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

# ---- container readiness: relative, traversal-free member paths
record("L_relative_paths",
       all(not p.startswith("/") and ".." not in p.split("/") for p in packaged),
       "every staged path is relative and traversal-free")

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

check_name("")
for rel, _kind in members:
    check_name(rel)

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

log "stage7: re-opening the container, verifying every entry, then sha256sum -c"
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

raw = gzip.open(tarball, "rb").read()
tf = tarfile.open(fileobj=io.BytesIO(raw), mode="r:")

names = [m.name for m in tf.getmembers()]
if len(names) != len(set(names)):
    sys.stderr.write("FAIL: duplicate tar members\n"); sys.exit(51)
if names != sorted(names):
    sys.stderr.write("FAIL: tar members are not in sorted order\n"); sys.exit(52)

members = {m.name: m for m in tf.getmembers()}
files = {}
for name, m in members.items():
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
    if m.isdir():
        continue
    data = tf.extractfile(m).read()
    files[rel] = (hashlib.sha256(data).hexdigest(), len(data))

staged = []
for root, _dirs, names_ in os.walk(stage):
    for n in names_:
        staged.append(os.path.relpath(os.path.join(root, n), stage).replace(os.sep, "/"))
staged.sort()
if sorted(files) != staged:
    sys.stderr.write("FAIL: container payload set != staged set\n"); sys.exit(59)

bad = []
for rel, (h, size) in files.items():
    if hashlib.sha256(open(os.path.join(stage, rel), "rb").read()).hexdigest() != h:
        bad.append(rel)
    if rel in manifest["files"]:
        if manifest["files"][rel]["sha256"] != h or manifest["files"][rel]["bytes"] != size:
            bad.append(rel + " (manifest.files{})")
    elif rel == "metadata/manifest.json":
        if cksum.get(rel) != h:
            bad.append(rel + " (checksums.txt)")
if bad:
    sys.stderr.write("FAIL: container payload hashes differ: %s\n" % sorted(set(bad))); sys.exit(60)

print("CONTAINER VERIFIED files=%d entries=%d" % (len(files), len(members)))
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

TAR_SHA=$(sha256sum "$TARBALL" | awk '{print $1}')
TAR_BYTES=$(stat -c %s "$TARBALL")

# ------------------------------------------------------------ stage 8: sidecars
printf '%s  %s.tar.gz\n' "$TAR_SHA" "$PKG_ROOT" > "$OUT_DIR/SHA256SUMS"
python3 - "$OUT_DIR" "$TARBALL" "$TAR_SHA" "$TAR_BYTES" "$SOURCE_COMMIT" "$VERSION" "$PKG_ROOT" <<'PY' \
  || die "writing package-summary.json failed"
import json, os, sys
out_dir, tarball, sha, size, commit, version, pkg_root = sys.argv[1:8]
vc = json.load(open(os.path.join(out_dir, "v-checks.json"), encoding="utf-8"))
facts = {}
for line in open(os.path.join(out_dir, "audit", "facts.env"), encoding="utf-8"):
    k, _, v = line.rstrip("\n").partition("=")
    facts[k] = v
summary = {
    "entry": "ops/package-linux.sh",
    "argv": json.loads(facts["argv_json"]),
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
    "artifact_provenance_attested": facts.get("provenance_attested") == "true",
    "build_receipt_sha256": facts.get("build_receipt_sha256") or None,
    "artifact_static_sha256": facts["static_sha256"],
    "artifact_shared_sha256": facts["shared_sha256"],
    "v_checks": {"verdict": vc["verdict"], "passed": vc["passed"], "failed": vc["failed"]},
    "not_run": ["runtime/device smoke of the packaged consumer example (no media input in this lane)",
                "two-root reproducibility rebuild (contract section 12 levels stay NOT_ESTABLISHED)"],
    "ok": vc["verdict"] == "PASS",
}
with open(os.path.join(out_dir, "package-summary.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(summary, f, indent=2)
    f.write("\n")
PY

log "PASS: $TARBALL sha256=$TAR_SHA bytes=$TAR_BYTES"
printf 'PACKAGE_OK tar=%s sha256=%s bytes=%s files=%s checksums=%s files_entries=%s provenance_attested=%s\n' \
  "$TARBALL" "$TAR_SHA" "$TAR_BYTES" \
  "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["packaged_files"])' "$OUT_DIR/v-checks.json")" \
  "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["checksums_entries"])' "$OUT_DIR/v-checks.json")" \
  "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["files_entries"])' "$OUT_DIR/v-checks.json")" \
  "$PROVENANCE_ATTESTED"
exit 0
