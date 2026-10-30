#!/usr/bin/env python3
"""Lane C (windows) — compare the PCM hashes produced by the Windows PE
consumer against native/experiments/songcore-equivalence/reference.json.

Input: the run/*.stdout.txt captured on the real Windows host.
Output: JSON verdict on stdout and to reference-comparison.json.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

REPO = Path("/mnt/c/Users/fred1/source/qianqian")
REF = REPO / "native/experiments/songcore-equivalence/reference.json"
RUN = Path("/mnt/c/Users/fred1/lane-c-win-pe/run")
OUT = REPO / "evidence/songcore-172/g4/lane-c-windows-pe/raw/reference-comparison.json"

# fixture id -> run case file stem
CASES = {
    "mp3-cbr-id3v23": "mp3-cbr-id3v23",
    "flac-16-44-stereo": "flac-16-44-stereo",
    "alac-16-44-stereo": "alac-16-44-stereo",
    "alac-long": "alac-long",
}

num = r"(-?\d+)"
strv = r"([^ ]*)"


def parse(out_text: str) -> dict:
    d = {}
    for line in out_text.splitlines():
        if line.startswith("stage=probe "):
            for k in ("codec", "container", "rate", "channels", "duration_us",
                      "streams", "selected"):
                m = re.search(r"\b%s=%s" % (k, strv), line)
                if m:
                    d["probe_" + k] = m.group(1)
        elif line.startswith("stage=decode "):
            m = re.search(r"frames=(\d+) pcm_sha256=([0-9a-f]{64}) terminal=(\S+)", line)
            if m:
                d["frames"] = int(m.group(1))
                d["pcm_sha256"] = m.group(2)
                d["terminal"] = m.group(3)
        elif line.startswith("bcrypt_genrandom_iat_calls_open_to_decode_eof="):
            d["bcrypt_calls_decode"] = int(line.split("=")[1])
        elif line.startswith("bcrypt_runtime_probe_rc="):
            d["bcrypt_probe_rc"] = int(line.split("=")[1])
        elif line.startswith("bcrypt_iat_counter:"):
            m = re.search(r"state=(-?\d+)", line)
            if m:
                d["bcrypt_iat_state"] = int(m.group(1))
        elif line.startswith("stage=seek "):
            m = re.search(r"status=(\d+) target_us=%s landing_us=%s" % (num, num), line)
            if m:
                d["seek_status"] = int(m.group(1))
                d["seek_target_us"] = int(m.group(2))
                d["seek_landing_us"] = int(m.group(3))
        elif line.startswith("stage=post_seek_decode "):
            m = re.search(r"frames=(\d+) pcm_sha256=([0-9a-f]{64})", line)
            if m:
                d["post_seek_frames"] = int(m.group(1))
                d["post_seek_pcm_sha256"] = m.group(2)
        elif line.startswith("RESULT="):
            d["result"] = line.split("=", 1)[1]
        elif line.startswith("abi_version="):
            d["abi_version"] = int(line.split("=")[1])
    return d


def main() -> int:
    ref = json.loads(REF.read_text())
    ref_by_id = {f["id"]: f for f in ref["fixtures"]}
    results = []
    all_ok = True
    for fid, stem in CASES.items():
        out_path = RUN / (stem + ".stdout.txt")
        exit_path = RUN / (stem + ".exit.txt")
        text = out_path.read_text(errors="replace")
        got = parse(text)
        want = ref_by_id[fid]
        exit_code = int(exit_path.read_text().strip())
        checks = {
            "exit_code_zero": exit_code == 0,
            "result_pass": got.get("result") == "PASS",
            "pcm_sha256_match": got.get("pcm_sha256") == want["pcm_sha256"],
            "frames_match": got.get("frames") == want["pcm_frames"],
            "sample_rate_match": got.get("probe_rate") == str(want["sample_rate"]),
            "channels_match": got.get("probe_channels") == str(want["channels"]),
            "codec_match": got.get("probe_codec") == want["codec"],
            "terminal_clean_eof": got.get("terminal") == "SONG_EOF",
            "seek_ok": got.get("seek_status") == 0,
            "post_seek_frames_positive": (got.get("post_seek_frames") or 0) > 0,
        }
        ok = all(checks.values())
        all_ok = all_ok and ok
        results.append({
            "fixture": fid,
            "fixture_sha256": want["fixture_sha256"],
            "run_case": out_path.name,
            "observed": got,
            "reference": {
                "codec": want["codec"],
                "sample_rate": want["sample_rate"],
                "channels": want["channels"],
                "pcm_frames": want["pcm_frames"],
                "pcm_sha256": want["pcm_sha256"],
                "terminal": want["terminal"],
            },
            "checks": checks,
            "pass": ok,
        })

    payload = {
        "schema": "lane-c-windows-pe/reference-comparison/1",
        "reference_json": str(REF),
        "reference_json_sha256": hashlib.sha256(REF.read_bytes()).hexdigest(),
        "hash_convention": "native/experiments/songcore-equivalence/harness.c run_correct(): "
                           "sha256 over sizeof(float)*frames*channels interleaved Float32 PCM bytes, "
                           "4096-frame blocks, read order",
        "execution_host": "real Windows host (Windows 11 build 26300.9818); see run/host.txt",
        "results": results,
        "all_pass": all_ok,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    print("COMPARE_RESULT=%s" % ("PASS" if all_ok else "FAIL"))
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
