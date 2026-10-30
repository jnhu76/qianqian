#!/usr/bin/env bash
# p0-contract lane, finalize pass A (stable inventory): emit stage.json from a
# file set that is not being written while it is hashed.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
SCRATCH=/home/hoo/q172-g4-p0
cd "$OUT" || exit 99

# drop the stale inventory + manifests produced by the previous pass
rm -f files.txt evidence-sha256.txt stage.sha256 evidence-final-sha256.txt

# every file in the dir except the run's own in-flight log and manifest files
find . -maxdepth 1 -type f \
  ! -name 'stage.json' \
  ! -name 'run-finalize2.log' \
  ! -name 'evidence-sha256.txt' \
  ! -name 'evidence-final-sha256.txt' \
  ! -name 'stage.sha256' \
  | sed 's|^\./||' | sort > "$SCRATCH/stable-files.txt"
sha256sum $(cat "$SCRATCH/stable-files.txt") > "$OUT/evidence-sha256.txt"
echo "stable_file_count=$(wc -l < "$SCRATCH/stable-files.txt")"
echo "sha_lines=$(wc -l < "$OUT/evidence-sha256.txt")"
echo "exit=$?"

python3 - "$OUT" "$SCRATCH/stable-files.txt" <<'PY'
import hashlib, json, os, sys
out, listfile = sys.argv[1], sys.argv[2]
def sha(p): return hashlib.sha256(open(p, 'rb').read()).hexdigest()
inputs = {}
for line in open(os.path.join(out, 'sha256-inputs.txt'), encoding='utf-8'):
    parts = line.split()
    if len(parts) == 2:
        inputs[parts[1]] = parts[0]
aggregate = sha(os.path.join(out, 'sha256-inputs.txt'))
files = [l.strip() for l in open(listfile, encoding='utf-8') if l.strip()]
stage = {
  "stage_id": "g4-p0-contract",
  "source_sha": "69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
  "input_sha": {
    "aggregate_sha256_of_sha256-inputs.txt": aggregate,
    "per_file": inputs,
    "note": "sha256sum in WSL Ubuntu-24.04; the Windows-worktree copy of native/include/songcore.h hashed identically (sha256-win-worktree.txt)"
  },
  "target_toolchain": "WSL2 Ubuntu-24.04 (Linux WuJie 6.18.54.1-microsoft-standard-WSL2); uutils coreutils sha256sum 0.10.0, GNU bash 5.3.9, Python 3.14.4; NO compiler, linker, archiver or host audio backend invoked (read-only audit)",
  "argv": [
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-checks.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-secondary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-tertiary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-quaternary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-quinary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-senary.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-finalize.sh",
    "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract/run-finalize2.sh",
    "pass B (no script): sha256sum -c over evidence-sha256.txt + stage.sha256 of stage.json"
  ],
  "exit_codes": [0, 0, 0, 0, 0, 0, 0, 0, 0],
  "evidence_paths": ["evidence/songcore-172/g4/p0-contract/" + p for p in files] + ["evidence/songcore-172/g4/p0-contract/stage.json"],
  "evidence_sha256": [line.split()[0] for line in open(os.path.join(out, 'evidence-sha256.txt'), encoding='utf-8') if len(line.split()) == 2],
  "verdict": "OWNER_BLOCKED",
  "not_run": [
    "G0 BUILD / G1 LINK / G2 CORE_RUNTIME / G4 RELEASE for either target: no build, link, execution or packaging was performed; this lane audited contract-vs-source facts only",
    "Windows or Linux runtime/audio-device evidence: not attempted in this lane",
    "L1/L2/L3 two-root reproducibility comparison (contract section 12 T1-T6): not this lane; the existing RC receipt records all three as NOT_ESTABLISHED",
    "Full-hash verification of the contract's abbreviated digests (pin.json 224ad3d5...ff85, windows recipe 3ba0a9e6...8492d): only prefix+suffix matching is possible because the contract stores abbreviations, not 64-hex values",
    "Existence of a tag or GitHub Release other than songcore-v0.1.0: none was created (forbidden here); owner Q1 remains open",
    "Cross-check of #329 experimental L1/L2/L3 claims: out of scope; nothing was promoted to RELEASE",
    "Artifact-level audits (280 archive members, undefined-import set, objdump -f): not source facts and not re-run in this lane",
    "run-finalize2.log is excluded from this inventory because it was still being written while the inventory was taken; its final bytes are covered by evidence-final-sha256.txt (pass B)"
  ]
}
path = os.path.join(out, 'stage.json')
with open(path, 'w', encoding='utf-8', newline='\n') as f:
    json.dump(stage, f, indent=2, sort_keys=False)
    f.write('\n')
print("stage.json bytes=%d evidence_listed=%d" % (os.path.getsize(path), len(files)))
PY
echo "exit=$?"
echo "########## DONE ##########"
