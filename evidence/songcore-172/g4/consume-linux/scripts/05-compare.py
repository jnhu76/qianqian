#!/usr/bin/env python3
"""Lane C (linux) - compare measured corpus results with the frozen reference.

Inputs (all read-only):
  <workdir>/reference.json              frozen SongCore equivalence corpus
  <workdir>/out/static/corpus-<id>.jsonl   static-link consumer output
  <workdir>/out/shared/corpus-<id>.jsonl   shared-link consumer output

Emits a JSON report to <out> and exits 1 on any mismatch.
"""
import json
import sys
import hashlib
from pathlib import Path

work = Path(sys.argv[1])
out_path = Path(sys.argv[2])
ref = json.loads((work / "reference.json").read_text())


def lines(p):
    recs = []
    for line in p.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            recs.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return recs


def pick(recs, **kv):
    for r in recs:
        if all(r.get(k) == v for k, v in kv.items()):
            return r
    return None


report = {
    "reference_json_sha256": hashlib.sha256((work / "reference.json").read_bytes()).hexdigest(),
    "reference_schema": ref.get("schema"),
    "fixtures": [],
    "ok": True,
}

for fx in ref["fixtures"]:
    fid = fx["id"]
    entry = {
        "id": fid,
        "fixture_file": fx["file"],
        "fixture_sha256_expected": fx["fixture_sha256"],
        "fixture_sha256_actual": hashlib.sha256((work / "fixtures" / fx["file"]).read_bytes()).hexdigest(),
        "frames_expected": fx["pcm_frames"],
        "pcm_sha256_expected": fx["pcm_sha256"],
        "codec_expected": fx["codec"],
        "sample_rate_expected": fx["sample_rate"],
        "channels_expected": fx["channels"],
        "links": {},
    }
    entry["fixture_sha256_match"] = entry["fixture_sha256_actual"] == entry["fixture_sha256_expected"]
    for link in ("static", "shared"):
        p = work / "out" / link / f"corpus-{fid}.jsonl"
        if not p.exists():
            entry["links"][link] = {"present": False}
            report["ok"] = False
            continue
        recs = lines(p)
        dec = pick(recs, stage="full_decode", status="done")
        probe = pick(recs, stage="probe")
        verdict = pick(recs, stage="verdict")
        seeks = [r for r in recs if r.get("stage") == "seek" and "pct" in r]
        next_reads = [r for r in recs if r.get("stage") == "seek_next_read"]
        land_checks = [r for r in recs if r.get("stage") == "seek_lossless_landing_check"]
        eof_repeat = pick(recs, stage="eof_repeat")
        drain = pick(recs, stage="drain")
        drain_seeks = [r for r in recs if r.get("stage") == "drain_seek"]
        loaded = next((r.get("loaded_songcore_paths") for r in recs if "loaded_songcore_paths" in r), None)
        L = {
            "present": True,
            "exit_verdict": verdict,
            "probe": probe,
            "frames_actual": dec.get("frames") if dec else None,
            "pcm_sha256_actual": dec.get("pcm_sha256") if dec else None,
            "frames_match": bool(dec) and dec.get("frames") == fx["pcm_frames"],
            "pcm_sha256_match": bool(dec) and dec.get("pcm_sha256") == fx["pcm_sha256"],
            "codec_match": bool(probe) and probe.get("codec") == fx["codec"],
            "sample_rate_match": bool(probe) and probe.get("sample_rate") == fx["sample_rate"],
            "channels_match": bool(probe) and probe.get("channels") == fx["channels"],
            "seek_results": seeks,
            "seek_next_reads": next_reads,
            "seek_lossless_landing_checks": land_checks,
            "eof_repeat": eof_repeat,
            "drain_seek_attempts": drain_seeks,
            "drain": drain,
            "loaded_songcore_paths": loaded,
        }
        L["all_seeks_ok"] = len(seeks) == 3 and all(s.get("status") == 0 for s in seeks)
        L["all_seeks_landing_known"] = len(seeks) == 3 and all(s.get("landing_us", -1) >= 0 for s in seeks)
        L["all_seek_next_reads_produced"] = len(next_reads) == 3 and all(
            n.get("status") == 0 and n.get("produced", 0) > 0 for n in next_reads)
        L["eof_repeat_ok"] = bool(eof_repeat) and eof_repeat.get("status") == 1 and eof_repeat.get("produced") == 0
        L["drain_eof_ok"] = bool(drain) and drain.get("terminal") == "SONG_EOF" and drain.get("produced_at_eof") == 0
        L["gate_ok"] = bool(verdict) and verdict.get("ok") == 1
        entry["links"][link] = L
        core = all([L["frames_match"], L["pcm_sha256_match"], L["codec_match"],
                    L["sample_rate_match"], L["channels_match"], L["all_seeks_ok"],
                    L["all_seeks_landing_known"], L["all_seek_next_reads_produced"],
                    L["eof_repeat_ok"], L["drain_eof_ok"], L["gate_ok"]])
        if not core:
            report["ok"] = False
    st, sh = entry["links"].get("static", {}), entry["links"].get("shared", {})
    entry["static_vs_shared_equal"] = (
        st.get("pcm_sha256_actual") == sh.get("pcm_sha256_actual") and
        st.get("frames_actual") == sh.get("frames_actual"))
    if st.get("present") and sh.get("present") and not entry["static_vs_shared_equal"]:
        report["ok"] = False
    if not entry["fixture_sha256_match"]:
        report["ok"] = False
    report["fixtures"].append(entry)

out_path.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({f["id"]: {
    "fixture_sha256_match": f["fixture_sha256_match"],
    "static": {k: f["links"]["static"].get(k) for k in
               ("frames_actual", "pcm_sha256_actual", "frames_match", "pcm_sha256_match",
                "all_seeks_ok", "all_seek_next_reads_produced", "eof_repeat_ok", "drain_eof_ok", "gate_ok",
                "loaded_songcore_paths")},
    "shared": {k: f["links"]["shared"].get(k) for k in
               ("frames_actual", "pcm_sha256_actual", "frames_match", "pcm_sha256_match",
                "all_seeks_ok", "all_seek_next_reads_produced", "eof_repeat_ok", "drain_eof_ok", "gate_ok",
                "loaded_songcore_paths")},
    "static_vs_shared_equal": f["static_vs_shared_equal"],
} for f in report["fixtures"]}, indent=2))
sys.exit(0 if report["ok"] else 1)
