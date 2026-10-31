#!/usr/bin/env bash
# ops/build-linux.sh — replayable Linux x86_64 SongCore build entry under the
# release contract docs/release/songcore-release-contract.md (§6 artifact
# kinds, §11 gates).
#
# THE SOURCE PIN IS EXPLICIT. This entry has no built-in branch or commit
# default. The pre-G4 version hardcoded BRANCH=songcore-172/pr-a-build-fixes
# and PINNED_HEAD=bf23516b5851174dc63fb5058997fa5478751713 — a pin that is not
# an ancestor of main — and every run re-derived that stale tree, so a "PASS"
# could come from a cache no-op that built nothing at the requested commit.
# Both constants are gone; a run without --source-commit/--source-branch/
# --work-root refuses to start.
#
# Fail-closed end to end:
#   stage1  source tree: reuse $WORK_ROOT when it is exactly the pinned
#           branch/commit with a clean tracked tree; else clone $SOURCE_REPO
#           and REFUSE (never silently check out something else) when the
#           branch tip differs from --source-commit. An existing work root
#           that is not at the pin is never deleted without --force-reclone.
#   stage2  FFmpeg closure: reuse the imported closure only when the manifest,
#           the pinned verified source stamp and the oracle config all exist
#           AND the manifest identity matches this host and the current
#           linux-x86_64 recipe; otherwise re-derive (fetch + ffmpeg_import).
#           The Xmake-side fail-closed target gate (native/build/ffmpeg.lua)
#           re-validates the manifest at every build regardless.
#   stage3  xmake f -p linux -a x86_64 -m release; xmake build songcore
#           (the linux-x86_64 recipe declares artifacts.static=true AND
#           artifacts.shared=true, so both forms are built); explicit
#           songcore_static / songcore_shared gate-acceptance builds.
#   stage4  verification: ELF identity (file/readelf/objdump), per-member
#           format audit of the merged archive, dynamic-export set equality
#           against the SONGCORE_API set derived from native/include/songcore.h,
#           NEEDED audit of the shared form, sha256 of every artifact and
#           provenance input.
#   stage5  machine-readable JSON summary on stdout (< 64KB); detailed logs in
#           <evidence-root>/build-linux/<UTC stamp>/.
#
# Usage:
#   ops/build-linux.sh --source-commit <40-hex sha> \
#                      --source-branch <branch> \
#                      --work-root <directory> \
#                      [--source-repo <git repository>] \
#                      [--evidence-root <directory>] \
#                      [--force-reclone]
#
#   --source-commit  full 40-hex commit to build (abbreviated shas refused).
#   --source-branch  branch the work root must be on. The pin is the pair;
#                    a branch tip that is not --source-commit aborts.
#   --work-root      clone + artifact destination
#                    (artifacts land in <work-root>/native/build/artifacts).
#   --source-repo    repository to clone from. Default: the repository that
#                    owns this script.
#   --evidence-root  evidence destination. Default $HOME/qianqian-s3-evidence.
#                    It MUST be outside --work-root (a fresh clone deletes and
#                    re-creates the work root; the pre-G4 entry wrote evidence
#                    inside the clone and lost it on exactly that path).
#   --force-reclone  replace an existing work root that is not at the pin.
#
# Environment knobs:
#   QIANQIAN_FETCH_PROXY https proxy used by scripts/fetch-ffmpeg if needed
#
# Exit code: 0 only if every stage passed. Failures emit a FAIL summary and
# propagate the real nonzero exit. Nothing is swallowed.
#
# Re-entrant: a rerun reuses the existing clone and the already-imported
# closure (validated, not trusted); xmake rebuilds incrementally.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"

usage() {
  sed -n '/^# Usage:/,/^# Exit code:/p' "$0" | sed 's/^# \{0,1\}//'
}

SOURCE_COMMIT=""
SOURCE_BRANCH=""
WORK_ROOT=""
SOURCE_REPO=""
EVIDENCE_ROOT=""
FORCE_RECLONE=0

die() { printf 'FATAL: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --source-commit) [ $# -ge 2 ] || die "--source-commit needs a value"; SOURCE_COMMIT=$2; shift 2 ;;
    --source-branch) [ $# -ge 2 ] || die "--source-branch needs a value"; SOURCE_BRANCH=$2; shift 2 ;;
    --work-root)     [ $# -ge 2 ] || die "--work-root needs a value";     WORK_ROOT=$2; shift 2 ;;
    --source-repo)   [ $# -ge 2 ] || die "--source-repo needs a value";   SOURCE_REPO=$2; shift 2 ;;
    --evidence-root) [ $# -ge 2 ] || die "--evidence-root needs a value"; EVIDENCE_ROOT=$2; shift 2 ;;
    --force-reclone) FORCE_RECLONE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'FATAL: unknown argument: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

[ -n "$SOURCE_COMMIT" ] || die "missing --source-commit (there is no built-in pin)"
[ -n "$SOURCE_BRANCH" ] || die "missing --source-branch (there is no built-in branch)"
[ -n "$WORK_ROOT" ]     || die "missing --work-root (there is no built-in build destination)"
case "$SOURCE_COMMIT" in
  *[!0-9a-f]*|'') die "--source-commit '$SOURCE_COMMIT' is not a lowercase hex sha" ;;
esac
[ ${#SOURCE_COMMIT} -eq 40 ] || die "--source-commit must be the full 40-hex sha (got '$SOURCE_COMMIT'); an abbreviated pin is refused"
[ "$SOURCE_BRANCH" != "HEAD" ] || die "--source-branch must name a branch"

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
if [ -z "$SOURCE_REPO" ]; then
  SOURCE_REPO=$(git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel 2>/dev/null) \
    || die "cannot derive --source-repo from $SCRIPT_DIR; pass --source-repo explicitly"
fi
[ -d "$SOURCE_REPO/.git" ] || [ -f "$SOURCE_REPO/.git" ] || die "--source-repo $SOURCE_REPO is not a git repository"
git -C "$SOURCE_REPO" cat-file -e "${SOURCE_COMMIT}^{commit}" 2>/dev/null \
  || die "--source-commit $SOURCE_COMMIT does not exist in $SOURCE_REPO"

EVIDENCE_ROOT=${EVIDENCE_ROOT:-$HOME/qianqian-s3-evidence}
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
EV=$EVIDENCE_ROOT/build-linux/$STAMP
case "$(realpath -m "$EV")/" in
  "$(realpath -m "$WORK_ROOT")"/*)
    die "evidence dir $EV is inside the work root $WORK_ROOT; a fresh clone would delete it (pass --evidence-root outside the work root)" ;;
esac
mkdir -p "$EV"
FACTS=$EV/facts.env
STAGE=init

fact() { printf '%s=%s\n' "$1" "$2" >> "$FACTS"; }
log()  { printf '[build-linux] %s\n' "$*"; }

on_err() {
  local rc=$?
  printf '{"verdict":"FAIL","failed_stage":"%s","exit":%d}\n' "$STAGE" "$rc"
  exit "$rc"
}
trap on_err ERR

log "pin: branch=$SOURCE_BRANCH commit=$SOURCE_COMMIT work-root=$WORK_ROOT"
log "source repo: $SOURCE_REPO"
log "evidence:    $EV"

# ---------------------------------------------------------------- stage 1
STAGE=clone
: > "$FACTS"
fact run_stamp "$STAMP"
fact branch "$SOURCE_BRANCH"
fact pinned_head "$SOURCE_COMMIT"
fact work_root "$WORK_ROOT"
fact source_repo "$SOURCE_REPO"

reuse_clone() {
  [ -d "$WORK_ROOT/.git" ] || return 1
  ( cd "$WORK_ROOT"
    [ "$(git rev-parse HEAD)" = "$SOURCE_COMMIT" ] || return 1
    [ "$(git rev-parse --abbrev-ref HEAD)" = "$SOURCE_BRANCH" ] || return 1
    git diff-index --quiet HEAD -- ) || return 1
  return 0
}

do_clone() {
  log "stage1: cloning $SOURCE_REPO branch $SOURCE_BRANCH (expect $SOURCE_COMMIT)"
  if [ -d "$WORK_ROOT" ] && [ -n "$(ls -A "$WORK_ROOT" 2>/dev/null)" ]; then
    [ "$FORCE_RECLONE" = 1 ] || die "work root $WORK_ROOT exists but is not at $SOURCE_BRANCH@$SOURCE_COMMIT; refusing to delete it (pass --force-reclone to replace it)"
  fi
  rm -rf "$WORK_ROOT"
  mkdir -p "$(dirname "$WORK_ROOT")"
  git clone --no-local "file://$SOURCE_REPO" "$WORK_ROOT" >"$EV/clone.log" 2>&1 || {
    tail -5 "$EV/clone.log" >&2; die "git clone from $SOURCE_REPO failed (see $EV/clone.log)"; }
  cd "$WORK_ROOT"
  git rev-parse --verify --quiet "origin/$SOURCE_BRANCH^{commit}" >/dev/null \
    || die "branch $SOURCE_BRANCH does not exist in $SOURCE_REPO"
  git checkout -q -B "$SOURCE_BRANCH" "origin/$SOURCE_BRANCH"
  local head
  head=$(git rev-parse HEAD)
  [ "$head" = "$SOURCE_COMMIT" ] \
    || die "pin mismatch: origin/$SOURCE_BRANCH is at $head, --source-commit is $SOURCE_COMMIT; refusing to build a different commit"
  git diff-index --quiet HEAD -- || die "fresh clone has tracked modifications"
  git fsck --full --no-dangling >"$EV/clone-fsck.txt" 2>&1 || die "git fsck failed"
  { git archive HEAD | sha256sum | awk '{print "git_archive_sha256="$1}'
    git rev-parse 'HEAD^{tree}'
  } > "$EV/clone-tree-verify.txt"
}

if reuse_clone; then
  cd "$WORK_ROOT"
  log "stage1: reusing work root at $(git rev-parse HEAD)"
  fact clone reuse
else
  do_clone
  log "stage1: cloned $SOURCE_COMMIT"
  fact clone fresh
fi
fact head "$(cd "$WORK_ROOT" && git rev-parse HEAD)"
fact tree "$(cd "$WORK_ROOT" && git rev-parse 'HEAD^{tree}')"
[ "$(cd "$WORK_ROOT" && git rev-parse HEAD)" = "$SOURCE_COMMIT" ] \
  || die "work root HEAD is not --source-commit"
[ "$(cd "$WORK_ROOT" && git rev-parse --abbrev-ref HEAD)" = "$SOURCE_BRANCH" ] \
  || die "work root branch is not --source-branch"

# ---------------------------------------------------------------- stage 2
STAGE=closure
cd "$WORK_ROOT/native"
PIN=ffmpeg/pin.json
MAN=build/ffmpeg-xmake/manifest.json
PROFILE=ffmpeg/profiles/codec-base.json
RECIPE=ffmpeg/targets/linux-x86_64.json
PIN_COMMIT=$(python3 -c "import json;print(json.load(open('$PIN'))['ffmpeg_commit_sha'])")
PIN_SHA=$(python3 -c "import json;print(json.load(open('$PIN'))['source_sha256'])")

closure_reusable() {
  [ -f "$MAN" ] || return 1
  [ -f build/ffmpeg-src/.qianqian-verified ] || return 1
  grep -q "^${PIN_COMMIT}|${PIN_SHA}$" build/ffmpeg-src/.qianqian-verified || return 1
  [ -f build/ffmpeg-xmake/oracle/ffbuild/config.mak ] || return 1
  python3 - "$MAN" "$RECIPE" "$PROFILE" "$PIN" <<'PY' || return 1
import json, sys, hashlib, platform
man = json.load(open(sys.argv[1]))
rec = json.load(open(sys.argv[2]))
rec_sha = hashlib.sha256(open(sys.argv[2], 'rb').read()).hexdigest()
profile_sha = hashlib.sha256(open(sys.argv[3], 'rb').read()).hexdigest()
pin = json.load(open(sys.argv[4]))
t, tt, tl = man.get("target", {}), rec.get("ffmpeg", {}), man.get("toolchain", {})
ok = (man.get("schema") == 2
      and t.get("id") == "linux-x86_64"
      and t.get("platform") == rec.get("platform")
      and t.get("arch") == rec.get("arch")
      and t.get("target_os") == tt.get("target_os")
      and t.get("recipe_sha256") == rec_sha
      and man.get("profile_sha256") == profile_sha
      and man.get("ffmpeg_tag") == pin.get("ffmpeg_tag")
      and man.get("ffmpeg_commit_sha") == pin.get("ffmpeg_commit_sha")
      and man.get("ffmpeg_source_sha256") == pin.get("source_sha256")
      and tl.get("system") == platform.system().lower()
      and tl.get("machine") == platform.machine().lower())
sys.exit(0 if ok else 1)
PY
}

if closure_reusable; then
  log "stage2: reusing imported FFmpeg closure (identity revalidated)"
  fact closure reuse
  echo "fetch=not-run-this-replay (closure reused; original fetch log preserved in the deriving evidence run)" > "$EV/fetch-ffmpeg.log"
else
  log "stage2: deriving FFmpeg closure (fetch + import oracle)"
  fact closure fresh
  if bash scripts/fetch-ffmpeg >"$EV/fetch-ffmpeg.log" 2>&1; then
    :
  elif [ -z "${QIANQIAN_FETCH_PROXY:-}" ] && (exec 3<>/dev/tcp/127.0.0.1/7897) 2>/dev/null; then
    log "stage2: direct fetch failed; retrying via local proxy 127.0.0.1:7897"
    export QIANQIAN_FETCH_PROXY=http://127.0.0.1:7897
    bash scripts/fetch-ffmpeg >>"$EV/fetch-ffmpeg.log" 2>&1 || {
      echo "FATAL: fetch-ffmpeg failed via proxy retry"; tail -5 "$EV/fetch-ffmpeg.log"; exit 1; }
  else
    echo "FATAL: fetch-ffmpeg failed"; tail -5 "$EV/fetch-ffmpeg.log"; exit 1
  fi
  python3 tools/ffmpeg_import.py --recipe linux-x86_64 >"$EV/ffmpeg-import.log" 2>&1 || {
    echo "FATAL: ffmpeg_import failed"; tail -5 "$EV/ffmpeg-import.log"; exit 1; }
fi
grep -E 'tag recheck|zip sha256' "$EV/fetch-ffmpeg.log" > "$EV/fetch-provenance-lines.txt" || {
  echo "no fetch lines this run (closure reused)" > "$EV/fetch-provenance-lines.txt"; }
fact manifest_sha256 "$(sha256sum "$MAN" | awk '{print $1}')"
fact recipe_sha256 "$(sha256sum "$RECIPE" | awk '{print $1}')"
fact profile_sha256 "$(sha256sum "$PROFILE" | awk '{print $1}')"
fact pin_json_sha256 "$(sha256sum "$PIN" | awk '{print $1}')"
fact ffmpeg_commit "$PIN_COMMIT"
fact ffmpeg_source_sha256 "$PIN_SHA"
python3 -c "
import json; m=json.load(open('$MAN'))
print('ffmpeg_tag', m['ffmpeg_tag']); print('closure_units', m['closure']['translation_units'])
print('target', json.dumps(m['target'], sort_keys=True)); print('toolchain', json.dumps(m['toolchain'], sort_keys=True))
" > "$EV/manifest-identity.txt"

# ---------------------------------------------------------------- stage 3
STAGE=build
export PATH="$HOME/.local/bin:$PATH"
xmake f -p linux -a x86_64 -m release -y >"$EV/xmake-config.log" 2>&1
xmake clean -a >"$EV/xmake-clean.log" 2>&1
xmake build songcore >"$EV/xmake-build.log" 2>&1
xmake build songcore_static >"$EV/xmake-build-static-explicit.log" 2>&1
xmake build songcore_shared >"$EV/xmake-build-shared-explicit.log" 2>&1
log "stage3: xmake build songcore OK (static+shared per recipe)"
fact build ok
COMPILE_ACTIONS=$(cat "$EV"/xmake-build*.log | grep -Eic 'compiling|linking|archiving' || true)
fact compile_actions "$COMPILE_ACTIONS"
[ "$COMPILE_ACTIONS" -gt 0 ] || die "xmake reported zero compile/link/archive actions after xmake clean -a"
fact build_cache_reuse false

# ---------------------------------------------------------------- stage 4
STAGE=verify
ART_A=build/artifacts/libsongcore.a
ART_SO=build/artifacts/shared/libsongcore.so
[ -f "$ART_A" ] || { echo "FATAL: $ART_A missing"; exit 1; }
[ -f "$ART_SO" ] || { echo "FATAL: $ART_SO missing"; exit 1; }

file "$ART_A" "$ART_SO" > "$EV/elf-identity-file.txt"
readelf -h "$ART_SO" > "$EV/elf-identity-readelf-so.txt"
readelf -d "$ART_SO" > "$EV/elf-dynamic-so.txt"

grep -q "ELF 64-bit LSB shared object, x86-64" "$EV/elf-identity-file.txt" || {
  echo "FATAL: .so is not ELF64 x86-64 shared object"; exit 1; }
grep -q "current ar archive" "$EV/elf-identity-file.txt" || {
  echo "FATAL: static artifact is not an ar archive"; exit 1; }
grep -q "Advanced Micro Devices X86-64" "$EV/elf-identity-readelf-so.txt" || {
  echo "FATAL: .so machine is not X86-64"; exit 1; }

# every archive member must be elf64-x86-64 (merged FFmpeg closure + songcore)
MEMBERS=$(ar t "$ART_A" | wc -l)
ELF64_MEMBERS=$(objdump -f "$ART_A" | grep -c "file format elf64-x86-64")
[ "$MEMBERS" -gt 0 ] || { echo "FATAL: empty archive"; exit 1; }
[ "$MEMBERS" = "$ELF64_MEMBERS" ] || {
  echo "FATAL: archive member formats: $ELF64_MEMBERS/$MEMBERS are elf64-x86-64"; exit 1; }
fact static_members "$MEMBERS"
fact static_elf64_members "$ELF64_MEMBERS"
objdump -f "$ART_A" > "$EV/elf-identity-objdump-a.txt"

# dynamic NEEDED audit: closure must be internalized, no libav* dependency
readelf -d "$ART_SO" | grep NEEDED | awk '{print $5}' | tr -d '[]' > "$EV/needed-libs.txt"
if grep -qE '^libav' "$EV/needed-libs.txt"; then
  echo "FATAL: shared artifact still NEEDs an FFmpeg closure library"; exit 1
fi

# ABI export set == SONGCORE_API set derived from the ABI authority (songcore.h)
grep 'SONGCORE_API' include/songcore.h | grep -oE '\bsong[a-z0-9_]*\(' | tr -d '(' | sort -u > "$EV/abi-expected.txt"
nm -D --defined-only "$ART_SO" | awk '$2=="T"||$2=="W"{print $3}' | grep '^song' | sort -u > "$EV/abi-exported-so.txt"
nm "$ART_A" | awk '$2=="T"||$2=="A"{print $3}' | grep '^song' | sort -u > "$EV/abi-exported-a.txt"
diff "$EV/abi-expected.txt" "$EV/abi-exported-so.txt" > "$EV/abi-diff-so.txt" || {
  echo "FATAL: .so exported song* set differs from songcore.h ABI set (see abi-diff-so.txt)"; exit 1; }
diff "$EV/abi-expected.txt" "$EV/abi-exported-a.txt" > "$EV/abi-diff-a.txt" || {
  echo "FATAL: .a exported song* set differs from songcore.h ABI set (see abi-diff-a.txt)"; exit 1; }
fact abi_symbols "$(wc -l < "$EV/abi-expected.txt")"

# sha256 identity of artifacts and provenance inputs
sha256sum "$ART_A" "$ART_SO" "$MAN" "$PIN" "$PROFILE" "$RECIPE" include/songcore.h src/songcore_ffmpeg.c ../ops/build-linux.sh > "$EV/artifacts-sha256.txt"
{
  echo "# closure provenance $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "target=linux-x86_64"
  echo "head=$(cd "$WORK_ROOT" && git rev-parse HEAD)"
  echo "tree=$(cd "$WORK_ROOT" && git rev-parse 'HEAD^{tree}')"
  echo "os=$(uname -sr)"
  echo "gcc=$(gcc --version | head -1)"
  echo "xmake=$(xmake --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g')"
  echo "ffmpeg_tag=$(python3 -c "import json;print(json.load(open('$PIN'))['ffmpeg_tag'])")"
  echo "ffmpeg_commit=$PIN_COMMIT"
  echo "ffmpeg_source_sha256=$PIN_SHA"
  cat "$EV/fetch-provenance-lines.txt"
} > "$EV/closure-provenance.txt"

# machine-readable summary (< 64KB) built from the recorded evidence
python3 - "$EV" "$WORK_ROOT" "$SOURCE_BRANCH" "$SOURCE_COMMIT" > "$EV/summary.raw.json" <<'PY'
import json, os, sys, re
ev, dst, branch, commit = sys.argv[1:5]
d = {}
for line in open(os.path.join(ev, "facts.env")):
    k, _, v = line.strip().partition("=")
    d[k] = v
sha = {}
for line in open(os.path.join(ev, "artifacts-sha256.txt")):
    h, p = line.split()
    sha[p] = h
def art(rel):
    full = os.path.join(dst, "native", rel)
    return {"path": full, "bytes": os.path.getsize(full), "sha256": sha.get(rel)}
needed = [l.strip() for l in open(os.path.join(ev, "needed-libs.txt")) if l.strip()]
units = int(re.findall(r"closure_units (\d+)", open(os.path.join(ev, "manifest-identity.txt")).read())[0])
summary = {
    "entry": "ops/build-linux.sh",
    "campaign": "songcore-172/S3 linux-x86_64 reference rebuild",
    "run_stamp": d["run_stamp"],
    "branch": branch, "head": commit, "tree": d["tree"],
    "source_repo": d["source_repo"], "work_root": d["work_root"],
    "clone": d["clone"], "closure": d["closure"],
    "target_recipe": "native/ffmpeg/targets/linux-x86_64.json",
    "artifact_policy": {"static": True, "shared": True, "wasm": False},
    "ffmpeg": {"tag": re.findall(r"ffmpeg_tag (\S+)", open(os.path.join(ev, "manifest-identity.txt")).read())[0], "commit": d["ffmpeg_commit"],
               "source_sha256": d["ffmpeg_source_sha256"],
               "manifest_sha256": d["manifest_sha256"],
               "closure_translation_units": units},
    "artifacts": {"static": art("build/artifacts/libsongcore.a"),
                  "shared": art("build/artifacts/shared/libsongcore.so")},
    "static_members": int(d["static_members"]),
    "static_elf64_members": int(d["static_elf64_members"]),
    "shared_needed": needed,
    "abi_symbols": int(d["abi_symbols"]),
    "checks": {"clone_head_pinned": "PASS", "closure_identity": "PASS",
               "build_static": "PASS", "build_shared": "PASS",
               "elf_identity": "PASS", "archive_members_all_elf64_x86_64": "PASS",
               "abi_export_set_equals_songcore_h": "PASS",
               "shared_no_libav_needed": "PASS", "sha256_recorded": "PASS"},
    "verdict": "PASS",
    "schema_version": 1,
    "qualification": "RELEASE_QUALIFIED",
    "source": {"commit": commit, "tree_oid": d["tree"]},
    "target": {"id": "linux-x86_64"},
    "toolchain": {"identity": "gcc=" + os.popen("gcc --version").readline().strip() + ";xmake=" + re.sub(r"\x1b\[[0-9;]*m", "", os.popen("xmake --version 2>/dev/null").readline().strip())},
    "inputs": {
        "header": {"sha256": sha["include/songcore.h"]},
        "recipe": {"sha256": d["recipe_sha256"]},
        "profile": {"sha256": d["profile_sha256"]},
        "pin": {"sha256": d["pin_json_sha256"]},
        "closure": {"sha256": d["manifest_sha256"]},
    },
    "execution": {"compile_actions": int(d["compile_actions"]),
                  "cache_reuse": d["build_cache_reuse"] == "true"},
}
out = json.dumps(summary, indent=2)
assert len(out) < 65536
print(out)
PY
cat "$EV/summary.raw.json"
mv -f "$EV/summary.raw.json" "$EV/summary.json"
log "stage5: PASS — evidence in $EV"
fact verdict PASS
exit 0
