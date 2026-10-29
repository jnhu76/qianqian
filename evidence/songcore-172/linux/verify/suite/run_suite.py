#!/usr/bin/env python3
"""S3 Linux lane C-consumer suite driver (#172 S3-verify).

Orchestrates, against the Linux artifacts in the WSL clone:
  1. the canonical equivalence gates (native/experiments/songcore-equivalence/
     run.py --skip-perf) — fixture sha preflight, ABI export gate, static/
     shared artifact gates, per-fixture correctness vs the frozen
     reference.json (frames / pcm_sha256 / terminal);
  2. the extension suite (suite_ext.c) — S-BIND §7.1 steps 04-13 not covered
     by the committed gates: stream enumeration, metadata canonical + raw,
     artwork, seek 25/50/75 with the §8.9 contract, EOF stability, read
     negatives, malformed/truncated/corrupted sources (typed fail-closed +
     no-fabrication prefix property), lifetime/reopen, open-failure status
     classes;
  3. assembles one machine-readable receipt; exit 0 only if everything passed.

Technique provenance: extension C code follows the canonical harness's
callback/sha256 patterns (harness.c:99-142, 19-97); the corruption-window
variant mirrors the committed Rust fault technique (e1_abi_tests.rs
"disturb", applied at 20% of the FLAC fixture like fault_config); no second
corpus, no second reference numbers (S-BIND §7.3).
"""
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

CLONE = Path(os.environ.get("S3_CLONE", "/home/hoo/qianqian-172-linux"))
NATIVE = CLONE / "native"
CORPUS = NATIVE / "experiments" / "songcore-equivalence"
EV = CLONE / "ops" / "verify-evidence"
SUITE_SRC = CLONE / "ops" / "verify-suite" / "suite_ext.c"
SUITE_BIN = EV / "suite_ext"
STATIC = NATIVE / "build" / "artifacts" / "libsongcore.a"
SHARED = NATIVE / "build" / "artifacts" / "shared" / "libsongcore.so"
REFERENCE = CORPUS / "reference.json"


def die(msg: str) -> None:
    print(f"FATAL: {msg}", file=sys.stderr)
    sys.exit(1)


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_json_lines(text: str) -> list[dict]:
    out = []
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("{"):
            out.append(json.loads(s))
    return out


def main() -> None:
    ref = json.loads(REFERENCE.read_text())
    EV.mkdir(parents=True, exist_ok=True)

    # ---- 1. canonical equivalence gates (fixture sha preflight inside) ----
    p = run([sys.executable, str(CORPUS / "run.py"), "--skip-perf",
             "--out", str(EV / "equivalence.json")])
    (EV / "run-canonical.stdout.txt").write_text(p.stdout)
    (EV / "run-canonical.stderr.txt").write_text(p.stderr)
    if p.returncode:
        die(f"canonical run.py failed rc={p.returncode}: {p.stdout[-800:]} {p.stderr[-800:]}")
    canonical = json.loads((EV / "equivalence.json").read_text())

    # ---- 2. compile + run the extension suite ----
    cc = run(["gcc", "-O2", "-std=c11", "-Wall", "-Wextra",
              f"-I{NATIVE / 'include'}", str(SUITE_SRC), str(STATIC),
              "-lm", "-lpthread", "-o", str(SUITE_BIN)])
    if cc.returncode:
        die(f"suite_ext compile failed: {cc.stderr[-2000:]}")

    fixtures = {fx["id"]: fx for fx in ref["fixtures"]}
    receipt_ext: dict[str, dict] = {}
    scratch_dir = EV / "scratch"
    scratch_dir.mkdir(exist_ok=True)

    def collect(title: str, proc: subprocess.CompletedProcess) -> list[dict]:
        rows = parse_json_lines(proc.stdout)
        bad = [r for r in rows if r.get("pass") == 0]
        receipt_ext[title] = {
            "exit": proc.returncode,
            "checks": rows,
            "failed_checks": [r["check"] for r in bad],
        }
        return rows

    # open-failure status classes
    collect("misc", run([str(SUITE_BIN), "misc"]))

    for fid, fx in fixtures.items():
        fpath = CORPUS / "fixtures" / fx["file"]
        scratch = str(scratch_dir / fid)
        # verify the corpus file itself against the frozen fixture sha
        got = sha256_file(fpath)
        if got != fx["fixture_sha256"]:
            die(f"fixture sha mismatch {fpath}: {got}")

        rows = collect(f"{fid}:scenario", run([str(SUITE_BIN), "scenario", str(fpath), scratch]))
        # frozen-field judgment vs reference.json (S-BIND §7.3: same manifest)
        probe = next((r for r in rows if r["check"] == "probe"), None)
        dec = next((r for r in rows if r["check"] == "full_decode"), None)
        frozen = {
            "rate_match": bool(probe) and probe["rate"] == fx["sample_rate"],
            "channels_match": bool(probe) and probe["channels"] == fx["channels"],
            "codec_match": bool(probe) and probe["codec"] == fx["codec"],
            "frames_match": bool(dec) and dec["frames"] == fx["pcm_frames"],
            "pcm_sha256_match": bool(dec) and dec["pcm_sha256"] == fx["pcm_sha256"],
        }
        frozen["pass"] = all(frozen.values())
        receipt_ext[f"{fid}:frozen_reference_match"] = frozen

        # truncated / malformed variants (derived negatives, §7.1 step 12)
        data = fpath.read_bytes()
        variants = {
            "trunc50": data[: len(data) // 2],
            "trunc4k": data[:4096],
            "empty": b"",
        }
        if fx["codec"] == "flac":
            # corruption-window technique (e1_abi_tests.rs disturb): zero a
            # 16-byte window at 20% of the file, inside decoded frame data
            off = len(data) // 5
            corrupted = bytearray(data)
            corrupted[off : off + 16] = b"\x00" * 16
            variants["corrupt16@20pct"] = bytes(corrupted)
        for name, blob in variants.items():
            vpath = scratch_dir / f"{fid}.{name}"
            vpath.write_bytes(blob)
            collect(f"{fid}:{name}",
                    run([str(SUITE_BIN), "trunc", str(vpath), scratch + ".pcm"]))

        # lifetime (§7.1 step 13)
        collect(f"{fid}:lifetime", run([str(SUITE_BIN), "lifetime", str(fpath)]))

    # ---- 3. verdict ----
    ext_all_ok = all(
        r["exit"] == 0 and not r["failed_checks"] for r in receipt_ext.values()
        if "exit" in r
    )
    frozen_all_ok = all(
        v["pass"] for k, v in receipt_ext.items() if k.endswith(":frozen_reference_match")
    )
    receipt = {
        "schema": "songcore-s3-verify-c-suite/1",
        "suite_sources": ["ops/verify-suite/suite_ext.c", "ops/verify-suite/run_suite.py"],
        "technique_provenance": [
            "native/experiments/songcore-equivalence/{harness.c,run.py,reference.json} (canonical corpus + frozen numbers, S-BIND §7.3)",
            "docs/architecture/songcore-binding-architecture.md §7.1 steps 01-13, §8.2-§8.11 (scenario + status/sentinel/seek/EOF/lifetime contracts)",
            "docs/release/songcore-release-contract.md §11 G1-G3 (gate definitions)",
            "crates/qianqian-decode-songcore/src/e1_abi_tests.rs disturb/fault technique (corruption window, applied to FLAC as there)",
        ],
        "canonical_gates": {
            "abi_gate_pass": canonical["abi_gate"]["pass"],
            "artifact_gate_pass": canonical["artifact_gate"]["pass"],
            "correctness_verdict": canonical["correctness_verdict"],
            "static_members": canonical["artifact_gate"]["static_members"],
            "shared_needed": canonical["abi_gate"]["shared_needed"],
        },
        "extension": receipt_ext,
        "extension_all_ok": ext_all_ok,
        "frozen_reference_all_ok": frozen_all_ok,
        "verdict": "PASS" if (ext_all_ok and frozen_all_ok) else "FAIL",
    }
    out = EV / "c-suite-receipt.json"
    out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"canonical: abi={canonical['abi_gate']['pass']} "
          f"artifact={canonical['artifact_gate']['pass']} "
          f"correctness={canonical['correctness_verdict']}")
    for k, v in receipt_ext.items():
        if "exit" in v:
            print(f"  {k}: exit={v['exit']} failed={v['failed_checks'] or 'none'}")
        else:
            print(f"  {k}: {v}")
    print(f"verdict: {receipt['verdict']}")
    print(f"wrote {out}")
    if receipt["verdict"] != "PASS":
        sys.exit(1)


if __name__ == "__main__":
    main()
