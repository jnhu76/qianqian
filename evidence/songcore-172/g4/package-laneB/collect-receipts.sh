#!/usr/bin/env bash
# Lane B: collect receipts and cross-checks for both preflight packages.
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"
LDIR=/home/hoo/g4-172/laneB-packages/linux
WSTAGE=/home/hoo/g4-172/laneB-stage/windows/songcore-v0.0.0-preflight-windows-x86_64
LSTAGE=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
WZIP=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/songcore-v0.0.0-preflight-windows-x86_64.zip

echo "===== containers ====="
sha256sum "$WZIP" "$LDIR/songcore-v0.0.0-preflight-linux-x86_64.tar.gz"
stat -c '%n %s bytes' "$WZIP" "$LDIR/songcore-v0.0.0-preflight-linux-x86_64.tar.gz"
echo "----- linux SHA256SUMS sidecar -----"
cat "$LDIR/SHA256SUMS"
echo "----- windows SHA256SUMS sidecar -----"
cat /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/SHA256SUMS

echo "===== linux v-checks.json ====="
cat "$LDIR/v-checks.json"
echo "===== linux package-summary.json (head) ====="
python3 - "$LDIR/package-summary.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(json.dumps({k: d[k] for k in d if k not in ("checks",)}, indent=1)[:2500])
print("--- checks ---")
print(json.dumps(d.get("checks"), indent=1)[:2500])
PY

echo "===== sha256sum -c (contract README verify recipe) ====="
echo "--- windows tree ---"
( cd "$WSTAGE" && sha256sum -c metadata/checksums.txt ); echo "win_rc=$?"
echo "--- linux tree ---"
( cd "$LSTAGE" && sha256sum -c metadata/checksums.txt ); echo "linux_rc=$?"

echo "===== tool versions recorded vs actual ====="
echo "-- windows manifest tools{} --"
python3 - "$WSTAGE/metadata/manifest.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
print(json.dumps({"manifest_songcore_version": m["songcore_version"], "tools": m["tools"],
                  "target_identity.toolchain": m["target_identity"]["toolchain"],
                  "triple": m["target_identity"]["triple"], "sdk_ndk": m["sdk_ndk"],
                  "reproducibility": m["reproducibility"], "artifact_shared": m["artifact_shared"],
                  "notes_head": m["notes"][:300]}, indent=1))
PY
echo "-- linux manifest tools{} --"
python3 - "$LSTAGE/metadata/manifest.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
print(json.dumps({"manifest_songcore_version": m["songcore_version"], "tools": m["tools"],
                  "target_identity.toolchain": m["target_identity"]["toolchain"],
                  "triple": m["target_identity"]["triple"], "sdk_ndk": m["sdk_ndk"],
                  "reproducibility": m["reproducibility"], "artifact_shared": m["artifact_shared"],
                  "notes_head": m["notes"][:400]}, indent=1))
PY
echo "-- actual host tool versions --"
x86_64-w64-mingw32-gcc --version | head -1
x86_64-w64-mingw32-ar --version | head -1
gcc --version | head -1
cc --version | head -1
ar --version | head -1
xmake --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g'

echo "===== README release-claim scan ====="
grep -n -i "release\|not a release\|provisional" "$WSTAGE/README.md" | head -8
echo "---"
grep -n -i "release\|not a release\|provisional" "$LSTAGE/README.md" | head -8
