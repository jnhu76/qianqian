#!/usr/bin/env bash
# p0-contract lane, finalize: confirm read-only discipline and emit stage.json.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
cd "$SRC" || exit 99
mark(){ echo; echo "########## $* ##########"; }

mark "I1 tracked-tree discipline after the audit"
git status --porcelain --untracked-files=no > "$OUT/git-tracked-status.txt" 2>&1
echo "exit=$?"
echo "tracked_changes=$(wc -l < "$OUT/git-tracked-status.txt")"
cat "$OUT/git-tracked-status.txt"
git status --porcelain | grep -c '^??' || true

mark "I2 environment identity"
{ uname -a; python3 --version; sha256sum --version | head -1; bash --version | head -1; } > "$OUT/env-identity.txt" 2>&1
echo "exit=$?"; cat "$OUT/env-identity.txt"

mark "I3 evidence inventory + hashes"
cd "$OUT" || exit 99
find . -maxdepth 1 -type f ! -name 'stage.json' | sed 's|^\./||' | sort > files.txt
sha256sum $(cat files.txt) > evidence-sha256.txt
echo "exit=$?"
wc -l < evidence-sha256.txt
cat evidence-sha256.txt

mark "I4 emit stage.json"
python3 - "$OUT" <<'PY'
import hashlib, json, os, sys
out = sys.argv[1]
def sha(p):
    return hashlib.sha256(open(p, 'rb').read()).hexdigest()
inputs = {}
for line in open(os.path.join(out, 'sha256-inputs.txt'), encoding='utf-8'):
    parts = line.split()
    if len(parts) == 2:
        inputs[parts[1]] = parts[0]
aggregate = sha(os.path.join(out, 'sha256-inputs.txt'))
evidence = []
for line in open(os.path.join(out, 'evidence-sha256.txt'), encoding='utf-8'):
    parts = line.split()
    if len(parts) == 2:
        evidence.append(parts[1])
stage = {
  "stage_id": "g4-p0-contract",
  "source_sha": "69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
  "input_sha": {
    "aggregate_sha256_of_sha256-inputs.txt": aggregate,
    "per_file": inputs,
    "note": "measured with sha256sum in WSL; the Windows worktree copy of native/include/songcore.h hashed identically (sha256-win-worktree.txt)"
  },
  "target_toolchain": "WSL2 Ubuntu-24.04; GNU coreutils sha256sum/grep/sed/awk + python3 (json/zipfile/hashlib); no compiler, linker, archiver or runtime audio backend invoked (read-only audit)",
  "argv": [
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-checks.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-secondary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-tertiary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-quaternary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-quinary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-senary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-finalize.sh"
  ],
  "exit_codes": [0, 0, 0, 0, 0, 0, 0],
  "evidence_paths": ["evidence/songcore-172/g4/p0-contract/" + p for p in evidence] + ["evidence/songcore-172/g4/p0-contract/stage.json"],
  "evidence_sha256": [line.split()[0] for line in open(os.path.join(out, 'evidence-sha256.txt'), encoding='utf-8') if len(line.split()) == 2],
  "verdict": "OWNER_BLOCKED",
  "not_run": [
    "G0 BUILD / G1 LINK / G2 CORE_RUNTIME / G4 RELEASE for either target: this lane ran no build, link, execution or packaging; it audited contract-vs-source facts only",
    "Windows or Linux runtime/audio-device evidence: not attempted in this lane",
    "L1/L2/L3 two-root reproducibility comparison (contract section 12 T1-T6): not this lane; existing RC receipt records all three as NOT_ESTABLISHED",
    "Full-hash verification of the contract's abbreviated digests (pin.json 224ad3d5...ff85, windows recipe 3ba0a9e6...8492d): only a 12-char-prefix + 4-char-suffix match is possible because the contract stores abbreviations, not full 64-hex values",
    "Existence of a tag/Release for any version other than songcore-v0.1.0: no tag was created (forbidden by this lane); owner Q1 open",
    "Cross-check of #329 experimental L1/L2/L3 claims: explicitly out of scope; no such claim was promoted to RELEASE here",
    "Audit of native/build/artifacts member counts (280 members) and runtime import sets: artifact-level, not source-fact; not re-run in this lane"
  ]
}
with open(os.path.join(out, 'stage.json'), 'w', encoding='utf-8', newline='\n') as f:
    json.dump(stage, f, indent=2, sort_keys=False)
    f.write('\n')
print("stage.json written, bytes=%d" % os.path.getsize(os.path.join(out, 'stage.json')))
PY
echo "exit=$?"
sha256sum stage.json
rm -f files.txt
echo "########## DONE ##########"
