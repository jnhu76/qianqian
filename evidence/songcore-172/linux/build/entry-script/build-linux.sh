#!/usr/bin/env bash
# ops/build-linux.sh — #172 S3 replayable entry: Linux x86_64 SongCore reference
# rebuild under the same release contract as the windows-x86_64 route.
#
# Runs entirely inside WSL (ext4). Contract, fail-closed end to end:
#   stage1  source tree: reuse $HOME/qianqian-172-linux when it is exactly the
#           pinned branch/commit with a clean tracked tree; else (re)clone it.
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
#           ops/build-evidence/<stamp>/.
#
# Exit code: 0 only if every stage passed. Failures emit a FAIL summary and
# propagate the real nonzero exit. Nothing is swallowed.
#
# Environment knobs:
#   QIANQIAN_S3_SOURCE   clone source path (default the campaign worktree)
#   QIANQIAN_S3_MAIN     owning-repository fallback path
#   QIANQIAN_FETCH_PROXY https proxy used by scripts/fetch-ffmpeg if needed
#
# Re-entrant: a rerun reuses the existing clone and the already-imported
# closure (validated, not trusted); xmake rebuilds incrementally.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"

REPO_SRC=${QIANQIAN_S3_SOURCE:-/mnt/c/Users/fred1/source/qianqian-172}
REPO_MAIN=${QIANQIAN_S3_MAIN:-/mnt/c/Users/fred1/source/qianqian}
BRANCH=songcore-172/pr-a-build-fixes
PINNED_HEAD=bf23516b5851174dc63fb5058997fa5478751713
DST=$HOME/qianqian-172-linux
EVROOT=$DST/ops/build-evidence
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
EV=$EVROOT/$STAMP
FACTS=$EV/facts.env
STAGE=init
mkdir -p "$EV"

fact() { printf '%s=%s\n' "$1" "$2" >> "$FACTS"; }
log()  { printf '[build-linux] %s\n' "$*"; }

on_err() {
  local rc=$?
  printf '{"verdict":"FAIL","failed_stage":"%s","exit":%d}\n' "$STAGE" "$rc"
  exit "$rc"
}
trap on_err ERR

# ---------------------------------------------------------------- stage 1
STAGE=clone
: > "$FACTS"
fact run_stamp "$STAMP"
fact branch "$BRANCH"
fact pinned_head "$PINNED_HEAD"

reuse_clone() {
  [ -d "$DST/.git" ] || return 1
  ( cd "$DST"
    [ "$(git rev-parse HEAD)" = "$PINNED_HEAD" ] || return 1
    [ "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" ] || return 1
    git diff-index --quiet HEAD -- ) || return 1
  return 0
}

do_clone() {
  log "stage1: cloning fresh"
  rm -rf "$DST"
  if git clone --no-local "file://$REPO_SRC" "$DST" >"$EV/clone-worktree-path.log" 2>&1; then
    { echo "clone-source-path=$REPO_SRC"
      echo "note=the worktree's .git gitfile carries a Windows-absolute gitdir; repo discovery from that path resolves to the owning repository (same object store and refs)."
    } > "$EV/clone-source.txt"
  else
    log "stage1: worktree-path clone failed, falling back to owning repository"
    cp -f "$EV/clone-worktree-path.log" "$EV/clone-worktree-path-failed.log"
    rm -rf "$DST"
    git clone --no-local "file://$REPO_MAIN" "$DST" >"$EV/clone.log" 2>&1
    { echo "clone-source-path=$REPO_MAIN"
      echo "note=clone from the worktree path failed on Linux git (Windows-absolute gitdir gitfile); owning repository used (same object store and refs). See clone-worktree-path-failed.log."
    } > "$EV/clone-source.txt"
  fi
  cd "$DST"
  git checkout -q -B "$BRANCH" "origin/$BRANCH"
  [ "$(git rev-parse HEAD)" = "$PINNED_HEAD" ] || {
    echo "FATAL: clone HEAD $(git rev-parse HEAD) != $PINNED_HEAD"; exit 1; }
  git diff-index --quiet HEAD -- || { echo "FATAL: fresh clone dirty"; exit 1; }
  git fsck --full --no-dangling >"$EV/clone-fsck.txt" 2>&1
  { git archive HEAD | sha256sum | awk '{print "git_archive_sha256="$1}'
    git rev-parse 'HEAD^{tree}'
  } > "$EV/clone-tree-verify.txt"
}

if reuse_clone; then
  cd "$DST"
  log "stage1: reusing clone at $(git rev-parse HEAD)"
  fact clone reuse
else
  do_clone
  log "stage1: cloned $PINNED_HEAD"
  fact clone fresh
fi
fact head "$(cd "$DST" && git rev-parse HEAD)"
fact tree "$(cd "$DST" && git rev-parse 'HEAD^{tree}')"

# ---------------------------------------------------------------- stage 2
STAGE=closure
cd "$DST/native"
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
  python3 - "$MAN" "$RECIPE" <<'PY' || return 1
import json, sys, hashlib, platform
man = json.load(open(sys.argv[1]))
rec = json.load(open(sys.argv[2]))
rec_sha = hashlib.sha256(open(sys.argv[2], 'rb').read()).hexdigest()
t, tt, tl = man.get("target", {}), rec.get("ffmpeg", {}), man.get("toolchain", {})
ok = (man.get("schema") == 2
      and t.get("id") == "linux-x86_64"
      and t.get("platform") == rec.get("platform")
      and t.get("arch") == rec.get("arch")
      and t.get("target_os") == tt.get("target_os")
      and t.get("recipe_sha256") == rec_sha
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
xmake f -p linux -a x86_64 -m release -y >"$EV/xmake-config.log" 2>&1
xmake build songcore >"$EV/xmake-build.log" 2>&1
xmake build songcore_static >"$EV/xmake-build-static-explicit.log" 2>&1
xmake build songcore_shared >"$EV/xmake-build-shared-explicit.log" 2>&1
log "stage3: xmake build songcore OK (static+shared per recipe)"
fact build ok

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
sha256sum "$ART_A" "$ART_SO" "$MAN" "$PIN" "$PROFILE" "$RECIPE" src/songcore_ffmpeg.c ../ops/build-linux.sh > "$EV/artifacts-sha256.txt"
{
  echo "# closure provenance $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "target=linux-x86_64"
  echo "head=$(cd "$DST" && git rev-parse HEAD)"
  echo "tree=$(cd "$DST" && git rev-parse 'HEAD^{tree}')"
  echo "os=$(uname -sr)"
  echo "gcc=$(gcc --version | head -1)"
  echo "xmake=$(xmake --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g')"
  echo "ffmpeg_tag=$(python3 -c "import json;print(json.load(open('$PIN'))['ffmpeg_tag'])")"
  echo "ffmpeg_commit=$PIN_COMMIT"
  echo "ffmpeg_source_sha256=$PIN_SHA"
  cat "$EV/fetch-provenance-lines.txt"
} > "$EV/closure-provenance.txt"

# machine-readable summary (< 64KB) built from the recorded evidence
python3 - "$EV" "$DST" > /tmp/s3-summary.json <<'PY'
import json, os, sys, re
ev, dst = sys.argv[1], sys.argv[2]
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
    "branch": d["branch"], "head": d["head"], "tree": d["tree"],
    "clone": d["clone"], "closure": d["closure"],
    "target_recipe": "native/ffmpeg/targets/linux-x86_64.json",
    "artifact_policy": {"static": True, "shared": True, "wasm": False},
    "ffmpeg": {"tag": "n9.0.1", "commit": d["ffmpeg_commit"],
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
}
out = json.dumps(summary, indent=2)
assert len(out) < 65536
print(out)
PY
cat /tmp/s3-summary.json
cp /tmp/s3-summary.json "$EV/summary.json"
log "stage5: PASS — evidence in $EV"
fact verdict PASS
exit 0
