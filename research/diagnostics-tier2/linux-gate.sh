#!/usr/bin/env bash
# Original full gate in a fresh LF checkout; no checker adaptations/skips.
set -euo pipefail
repository=$1
evidence_checkout=$2
destination=$3
subject=$4
raw="$evidence_checkout/evidence/diagnostics-tier2-296/raw"
for tool in git cargo rustc python3 node npm lefthook; do command -v "$tool" >/dev/null; done
if [[ -e "$destination" ]]; then
  echo "refusing to overwrite existing validation checkout: $destination" >&2
  exit 2
fi
git clone --shared --no-checkout "$repository" "$destination" >"$raw/linux-checkout.log" 2>&1
git -C "$destination" config core.autocrlf false
git -C "$destination" checkout --detach "$subject" >>"$raw/linux-checkout.log" 2>&1
cd "$destination"
# Overlay only the current report/router, not uncommitted Rust or checker code.
cp "$evidence_checkout/docs/architecture/error-system-decision-record.md" docs/architecture/
# Outer evidence runners can be newer than the frozen Rust subject. Preserve
# and hash those explicit script overlays; never overlay Rust/checker code.
for script in "$evidence_checkout"/research/diagnostics-tier2/*.ps1 "$evidence_checkout"/research/diagnostics-tier2/*.sh; do
  [[ -f "$script" ]] && cp "$script" research/diagnostics-tier2/
done
cp -a "$evidence_checkout/evidence/diagnostics-tier2-296" evidence/
git status --short >"$raw/linux-subject-status.log"
cargo fetch --locked --manifest-path tools/audio-api-surface/Cargo.toml >"$raw/linux-tooling-fetch.log" 2>&1
npm ci --prefix website >"$raw/linux-website-install.log" 2>&1
python3 - "$subject" "$destination" >"$raw/linux-gate-environment.json" <<'PY'
import hashlib, json, pathlib, platform, subprocess, sys
manifest=json.loads(pathlib.Path("evidence/diagnostics-tier2-296/raw/source-sha256.json").read_text())
def output(*cmd): return subprocess.check_output(cmd,text=True).strip()
print(json.dumps({"subject_commit":sys.argv[1],"checkout":sys.argv[2],
 "platform":platform.platform(),"rustc":output("rustc","-vV"),
 "node":output("node","--version"),"python":platform.python_version(),
 "outer_script_overlays":[r["path"] for r in manifest if r["path"].endswith((".ps1",".sh"))],
 "source_eol":"native Git LF checkout; Windows raw input hashes recorded separately",
 "source_sha256":[{"path":r["path"],"sha256":hashlib.sha256(pathlib.Path(r["path"]).read_bytes()).hexdigest()} for r in manifest]},indent=2))
PY
set +e
lefthook run pre-push --all-files >"$raw/pre-push-linux.log" 2>&1
code=$?
set -e
printf '\nGATE_EXIT=%s\n' "$code" >>"$raw/pre-push-linux.log"
exit "$code"
