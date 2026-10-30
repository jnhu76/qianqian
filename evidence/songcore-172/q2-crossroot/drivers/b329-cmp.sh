#!/usr/bin/env bash
# b329-cmp.sh <target:linux|windows> <root1> <root2> <outdir> <label1> <label2>
# Occurrence-indexed comparison of the shipped artifacts across two build roots.
# Prints a JSON verdict; exit 0 = all artifacts byte-identical, 1 = differences, 2 = tool error.
set -uo pipefail
TARGET=${1:?}; R1=${2:?}; R2=${3:?}; OUT=${4:?}; L1=${5:?}; L2=${6:?}
TOOL=/mnt/c/Users/fred1/AppData/Local/Temp/qq172wf/329/b329-arcmp.py
export R1 R2
mkdir -p "$OUT"
A1="$R1/src/native/build/artifacts/libsongcore.a"
A2="$R2/src/native/build/artifacts/libsongcore.a"
[ -f "$A1" ] && [ -f "$A2" ] || { echo '{"error":"static artifact missing"}'; exit 2; }

python3 - "$TARGET" "$OUT" "$L1" "$L2" <<'PY'
import hashlib, json, os, subprocess, sys
target, out, l1, l2 = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
tool = "/mnt/c/Users/fred1/AppData/Local/Temp/qq172wf/329/b329-arcmp.py"
r1, r2 = os.environ["R1"], os.environ["R2"]

def cmp_pair(x, y, name):
    if name == "shared":
        # A .so is an ELF file, not an ar archive: byte-compare by sha256.
        # (The ar member parser would exit 2 on it — a tool error, not a verdict.)
        h1, h2 = sha(x), sha(y)
        return {
            "artifact": name, "exit": 0 if h1 == h2 else 1, "identical": h1 == h2,
            "identical_occurrences": None, "differing_occurrences": None,
            "order_identical": "shared-sha256", "duplicate_names": None,
            "leak_r1": None, "leak_r2": None, "cmp_file": "",
        }
    p = os.path.join(out, name)
    r = subprocess.run(["python3", tool, x, y, p, r1, r2],
                       capture_output=True, text=True)
    cmp_txt = p + "-cmp.txt"
    identical_occ = differing_occ = dup = None
    order = None
    leak1 = leak2 = None
    if os.path.exists(cmp_txt):
        for line in open(cmp_txt, encoding="utf-8"):
            if line.startswith("IDENTICAL_OCCURRENCES="):
                identical_occ = int(line.split("=", 1)[1])
            elif line.startswith("DIFFERING_OCCURRENCES="):
                differing_occ = int(line.split("=", 1)[1])
            elif line.startswith("ORDER_IDENTICAL="):
                order = line.split("=", 1)[1].strip()
            elif line.startswith("DUPLICATE_NAMES="):
                dup = int(line.split("=", 1)[1])
            elif line.startswith("R1_LABEL="):
                leak1 = line.split("TOTAL_HITS=", 1)[1].strip()
            elif line.startswith("R2_LABEL="):
                leak2 = line.split("TOTAL_HITS=", 1)[1].strip()
    return {
        "artifact": name, "exit": r.returncode, "identical": r.returncode == 0,
        "identical_occurrences": identical_occ, "differing_occurrences": differing_occ,
        "order_identical": order, "duplicate_names": dup,
        "leak_r1": leak1, "leak_r2": leak2,
        "cmp_file": cmp_txt,
    }

def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()

a1 = os.path.join(r1, "src/native/build/artifacts/libsongcore.a")
a2 = os.path.join(r2, "src/native/build/artifacts/libsongcore.a")
res = {"target": target, "cmp_dir": out, "static": cmp_pair(a1, a2, "static"), "shared": None}
res["static_sha_r1"] = sha(a1)
res["static_sha_r2"] = sha(a2)
if target == "linux":
    s1 = os.path.join(r1, "src/native/build/artifacts/shared/libsongcore.so")
    s2 = os.path.join(r2, "src/native/build/artifacts/shared/libsongcore.so")
    res["shared"] = cmp_pair(s1, s2, "shared")
    res["shared_sha_r1"] = sha(s1)
    res["shared_sha_r2"] = sha(s2)

all_ident = res["static"]["identical"] and (res["shared"] is None or res["shared"]["identical"])
res["all_identical"] = all_ident
with open(os.path.join(out, "verdict.json"), "w", encoding="utf-8") as f:
    json.dump(res, f, indent=2)
print(json.dumps(res, indent=2))
sys.exit(0 if all_ident else 1)
PY
exit $?
