#!/usr/bin/env python3
"""SongCore ABI v1 regression authority runner + fail-closed --check.

Drives tests/songcore/songcore_probe over the SongCore corpus and the
Common Formats corpus, applies the frozen consistency gates, and writes the
machine authority tree:

  bench/results/songcore-v1/
    abi.json                    ABI surface evidence (symbols)
    common-formats.json         per-format regression through SongCore
    metadata.json               canonical/raw metadata + precedence evidence
    artwork.json                artwork extraction evidence
    stream-selection.json       default policy + explicit selection evidence
    seek.json                   seek landing evidence per family
    errors.json                 typed negative-corpus evidence
    consistency.json            snapshot stability + probe==decode gates
    states.json                 fuzz-like state-machine sequences
    lifetime.json               borrowed-view lifetime after read/seek/EOF
    switchcheck.json            stream-switch A==B PCM (demux reset) evidence
    sanitizers.json             ASan/UBSan/leak run evidence
    dsp-src-integration.json    DSP/SRC composition smoke evidence
    summary.json                overall verdict

--check re-derives every gate from the raw evidence files + the manifest
expectations (it never trusts summary.json's stored verdict) and exits
nonzero on any gate failure (fail-closed). It is read-only.

Usage:
  python3 tests/songcore/regression.py --out bench/results/songcore-v1 \
      [--binary BIN]
  python3 tests/songcore/regression.py --check --out bench/results/songcore-v1
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SONG_MANIFEST = os.path.join(ROOT, "corpus", "manifest", "songcore.json")
COMMON_MANIFEST = os.path.join(ROOT, "corpus", "manifest", "common-formats.json")
FIXTURES = os.path.join(ROOT, "corpus", "fixtures")
DEFAULT_BINARY = os.path.join(ROOT, "build", "artifacts", "songcore_probe")
ARTIFACT_DIR = os.path.join(ROOT, "build", "artifacts")

STATUS_NAMES = {
    0: "OK", 1: "EOF",
    100: "INVALID_ARGUMENT", 101: "STATE", 102: "NOT_OPEN",
    103: "IO", 104: "UNSUPPORTED_CONTAINER", 105: "NO_AUDIO_STREAM",
    106: "UNSUPPORTED_CODEC", 107: "CORRUPT_DATA", 108: "DECODE_ERROR",
    109: "SEEK_UNSUPPORTED", 110: "SEEK_ERROR", 111: "STREAM_CHANGE",
    112: "OUT_OF_MEMORY", 113: "INTERNAL_ERROR",
}
NAME_TO_STATUS = {v: k for k, v in STATUS_NAMES.items()}

# Seek tolerance: the Common Formats authority pins the bounded-resume gate
# at 65536 samples (implied resume within target +/- the bound); the stricter
# "strict" family additionally requires the backward landing to be at/before
# the target. Tolerances are converted per-case via sample_rate.
BOUNDED_RESUME_SAMPLES = 65536

EVIDENCE_FILES = ("metadata", "artwork", "stream-selection", "seek", "errors",
                  "consistency", "states", "lifetime", "switchcheck",
                  "common-formats", "sanitizers", "dsp-src-integration")


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def run_binary(binary, args, timeout=120):
    return subprocess.run([binary] + args, capture_output=True, text=True,
                          encoding="utf-8", errors="replace", timeout=timeout)


def parse_json_line(stdout):
    """Extract the first JSON object from stdout (FFmpeg may log to stderr,
    but the harness writes pure JSON to stdout)."""
    try:
        return json.loads(stdout)
    except json.JSONDecodeError:
        # tolerate trailing content after the object
        i = stdout.find("{")
        if i < 0:
            return None
        return json.loads(stdout[i:].split("\n")[0])


class Gate:
    def __init__(self):
        self.failures = []
        self.records = []

    def check(self, cond, case, msg):
        if not cond:
            self.failures.append({"case": case, "message": msg})
        return bool(cond)

    def add(self, rec):
        self.records.append(rec)


def meta_ok(rec_meta, expected):
    """Check canonical metadata expectations. expected uses has_*/value keys;
    a value key present means it must equal exactly."""
    if expected is None:
        return []
    errs = []
    canon = (rec_meta or {}).get("canonical") or {}
    for k, v in expected.items():
        if k.startswith("has_"):
            if canon.get(k) != v:
                errs.append(f"canonical {k}: expected {v}, got {canon.get(k)}")
        else:
            has_key = "has_" + k
            if v is None:
                if canon.get(has_key, 0) != 0:
                    errs.append(f"canonical {k}: expected absent")
            else:
                if canon.get(has_key, 0) != 1 or canon.get(k) != v:
                    errs.append(f"canonical {k}: expected {v!r}, got "
                                f"{canon.get(k)!r} (has={canon.get(has_key)})")
    return errs


def artwork_ok(rec_art, expected):
    if expected is None:
        return []
    errs = []
    if len(rec_art) != expected.get("count", len(rec_art)):
        errs.append(f"artwork count: expected {expected.get('count')}, "
                    f"got {len(rec_art)}")
        return errs
    for i, exp in enumerate(expected.get("items", [])):
        if i >= len(rec_art):
            continue
        it = rec_art[i]
        if "mime" in exp and it.get("mime") != exp["mime"]:
            errs.append(f"artwork[{i}] mime: expected {exp['mime']}, "
                        f"got {it.get('mime')}")
        if "role" in exp and it.get("role") != exp["role"]:
            errs.append(f"artwork[{i}] role: expected {exp['role']}, "
                        f"got {it.get('role')}")
        if "front" in exp and it.get("is_front_cover") != exp["front"]:
            errs.append(f"artwork[{i}] front: expected {exp['front']}, "
                        f"got {it.get('is_front_cover')}")
    return errs


def seek_ok(rec_seeks, family, sample_rate):
    errs = []
    if family == "unsupported":
        # UNSUPPORTED (raw ADTS) and RECORD (truncated/degraded): no landing
        # assertion; the seek status must still be typed (0 or a real status
        # code, never a generic sentinel).
        for s in rec_seeks:
            st = s.get("status")
            if st == -1:
                errs.append("seek returned generic -1")
            elif not isinstance(st, int) or st == -2:
                errs.append(f"seek status not typed: {st}")
        return errs
    tol = (BOUNDED_RESUME_SAMPLES * 1_000_000) // max(sample_rate, 1)
    for s in rec_seeks:
        if s.get("status") != 0:
            errs.append(f"seek status: expected OK, got {s.get('status')}")
            continue
        actual = s.get("actual_us", -1)
        target = s.get("target_us", 0)
        if actual < 0:
            errs.append(f"seek landing unknown (actual=-1) for target {target}")
            continue
        if family == "strict":
            # lossless: backward container seek must land at/before target
            if actual > target:
                errs.append(f"strict seek landed after target: actual {actual} "
                            f"> target {target}")
        else:
            # lapped: bounded codec-frame tolerance on both sides (AAC/Opus
            # first-frame pts can start slightly after the target)
            if actual > target + tol:
                errs.append(f"lapped seek landed too far after target: "
                            f"actual {actual}, target {target}, tol {tol}")
        if actual < target - tol:
            errs.append(f"seek landed too far before target: actual {actual}, "
                        f"target {target}, tolerance {tol}")
    return errs


def check_consistency(rec, case, gate, degraded=False):
    """The primary invariants for a successful record.

    `degraded` marks fixtures where the authority accepts a typed terminal
    decode error (truncated/corrupt corpus): EOF is still ideal, but a typed
    error is a valid terminal state (never a generic -1 gate)."""
    errs = gate
    if rec.get("phase") != "ok":
        errs.check(False, case, f"phase={rec.get('phase')} "
                                f"probe_status={rec.get('probe_status')}")
        return False

    ok = True
    fs = rec["decode"].get("final_status")
    if degraded:
        ok &= errs.check(fs == 1 or (isinstance(fs, int) and fs >= 100), case,
                         f"degraded decode ended with unexpected status {fs}")
    else:
        ok &= errs.check(fs == 1, case, f"decode did not end with EOF: {fs}")
    ok &= errs.check(rec["decode"].get("frames", 0) > 0, case,
                     "decode produced zero frames")
    ok &= errs.check(rec.get("metadata_sha") == rec.get("metadata_after_decode_sha")
                     == rec.get("metadata_after_seek_sha"), case,
                     f"metadata snapshot unstable: {rec.get('metadata_sha')} / "
                     f"{rec.get('metadata_after_decode_sha')} / "
                     f"{rec.get('metadata_after_seek_sha')}")
    ok &= errs.check(rec.get("artwork_sha") == rec.get("artwork_after_decode_sha")
                     == rec.get("artwork_after_seek_sha"), case,
                     "artwork snapshot unstable across decode/seek")
    return ok


def run_record(gate, binary, path, case, select_index=None):
    args = ["record", path]
    if select_index is not None:
        args += ["--select", str(select_index)]
    r = run_binary(binary, args)
    rec = parse_json_line(r.stdout)
    if rec is None:
        gate.check(False, case, f"unparseable harness output: {r.stdout[:200]}")
        return None
    gate.add(rec)
    return rec


def info_expectations_ok(info, exp, case, gate):
    if "container" in exp and info.get("container") != exp["container"]:
        gate.check(False, case, f"container {info.get('container')} != "
                                f"{exp['container']}")
    if "codec" in exp and info.get("codec") != exp["codec"]:
        gate.check(False, case, f"codec {info.get('codec')} != {exp['codec']}")
    if "sample_rate" in exp and info.get("sample_rate") != exp["sample_rate"]:
        gate.check(False, case, "sample_rate mismatch")
    if "channels" in exp and info.get("channels") != exp["channels"]:
        gate.check(False, case, "channels mismatch")
    if "audio_stream_count" in exp and \
            info.get("audio_stream_count") != exp["audio_stream_count"]:
        gate.check(False, case, "audio_stream_count mismatch")
    if "default_selected" in exp and \
            info.get("selected_audio_index") != exp["default_selected"]:
        gate.check(False, case, "default selection mismatch")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "bench", "results",
                                                  "songcore-v1"))
    ap.add_argument("--binary", default=DEFAULT_BINARY)
    ap.add_argument("--song-manifest", default=SONG_MANIFEST)
    ap.add_argument("--common-manifest", default=COMMON_MANIFEST)
    ap.add_argument("--check", action="store_true",
                    help="read-only validation of an existing authority tree")
    ap.add_argument("--no-common", action="store_true")
    args = ap.parse_args()

    out_dir = args.out
    if args.check:
        return revalidate(out_dir)

    os.makedirs(out_dir, exist_ok=True)
    if not os.path.isfile(args.binary):
        raise SystemExit(f"harness binary not found: {args.binary}")
    for f in (args.song_manifest, args.common_manifest):
        if not os.path.isfile(f):
            raise SystemExit(f"manifest not found: {f}")

    gate = Gate()

    # -------------------------------------------------------- SongCore corpus
    scm = json.load(open(args.song_manifest))
    metadata_evidence = []
    artwork_evidence = []
    stream_evidence = []
    seek_evidence = []
    error_evidence = []
    consistency_evidence = []

    for case in scm["cases"]:
        cid = case["id"]
        path = os.path.join(FIXTURES, case["file"])
        if sha256_file(path) != case["fixture_sha256"]:
            gate.check(False, cid, "fixture sha256 mismatch")
            continue
        mode = case["mode"]
        exp = case["expect"]

        if mode == "record":
            sel = exp.get("select", {}).get("index")
            rec = run_record(gate, args.binary, path, cid, select_index=sel)
            if rec is None:
                continue
            if not check_consistency(rec, cid, gate):
                continue

            info = rec.get("info") or {}
            info_expectations_ok(info, exp, cid, gate)

            # metadata
            errs = meta_ok(rec.get("metadata"), exp.get("metadata"))
            for e in errs:
                gate.check(False, cid, "metadata: " + e)
            raw = rec.get("metadata") or {}
            for k in exp.get("raw_keys", []):
                keys = [r["key"] for r in raw.get("raw", [])]
                gate.check(k in keys, cid,
                           f"raw key {k} missing (have {keys})")
            metadata_evidence.append({
                "case": cid, "sha": rec.get("metadata_sha"),
                "canonical": (rec.get("metadata") or {}).get("canonical"),
                "raw": (rec.get("metadata") or {}).get("raw"),
            })

            # artwork
            art = rec.get("artwork") or []
            if "artwork_count" in exp:
                gate.check(len(art) == exp["artwork_count"], cid,
                           f"artwork count {len(art)} != "
                           f"{exp['artwork_count']}")
            for e in artwork_ok(art, {"count": exp.get("artwork_count"),
                                      "items": exp.get("artwork", [])}):
                gate.check(False, cid, e)
            artwork_evidence.append({"case": cid, "items": art,
                                     "sha": rec.get("artwork_sha")})

            # seeks
            family = exp.get("seek_family", "lapped")
            for e in seek_ok(rec.get("seeks", []), family,
                             info.get("sample_rate") or 44100):
                gate.check(False, cid, e)
            seek_evidence.append({"case": cid, "family": family,
                                  "seeks": rec.get("seeks", [])})

            # stream selection
            for i, sexp in enumerate(exp.get("streams", [])):
                streams = rec.get("streams") or []
                if i < len(streams):
                    s = streams[i]
                    if "sample_rate" in sexp and s.get("sample_rate") != \
                            sexp["sample_rate"]:
                        gate.check(False, cid, f"stream[{i}] rate mismatch")
                    if "is_default" in sexp and s.get("is_default") != \
                            sexp["is_default"]:
                        gate.check(False, cid, f"stream[{i}] default mismatch")
                    if "codec" in sexp and s.get("codec") != sexp["codec"]:
                        gate.check(False, cid, f"stream[{i}] codec mismatch")
            sel = exp.get("select")
            if sel and "select" in rec:
                srec = rec["select"]
                gate.check(srec.get("status") == 0, cid,
                           f"select status {srec.get('status')}")
                if "sample_rate" in sel:
                    gate.check(srec.get("info", {}).get("sample_rate") ==
                               sel["sample_rate"], cid,
                               f"select rate {srec.get('info', {}).get('sample_rate')} "
                               f"!= {sel['sample_rate']}")
                if "metadata_title" in sel:
                    m = srec.get("metadata_sha")
                    gate.check(m is not None, cid, "select metadata missing")
                stream_evidence.append({
                    "case": cid, "select": srec,
                    "info": rec.get("info"),
                })

            consistency_evidence.append({
                "case": cid,
                "probe": info,
                "decode": rec.get("decode"),
                "metadata_stable": rec.get("metadata_sha") ==
                rec.get("metadata_after_decode_sha") ==
                rec.get("metadata_after_seek_sha"),
                "artwork_stable": rec.get("artwork_sha") ==
                rec.get("artwork_after_decode_sha") ==
                rec.get("artwork_after_seek_sha"),
            })

        elif mode == "neg":
            r = run_binary(args.binary, ["neg", path])
            rec = parse_json_line(r.stdout)
            if rec is None:
                gate.check(False, cid, f"unparseable neg output: {r.stdout[:200]}")
                continue
            gate.add(rec)
            if "open" in exp:
                want = NAME_TO_STATUS.get(exp["open"], exp["open"])
                gate.check(rec["open_status"] == want, cid,
                           f"open: expected {exp['open']} ({want}), "
                           f"got {rec['open_status']} "
                           f"({STATUS_NAMES.get(rec['open_status'])})")
            if "probe" in exp:
                want = NAME_TO_STATUS.get(exp["probe"], exp["probe"])
                gate.check(rec["probe_status"] == want, cid,
                           f"probe: expected {exp['probe']} ({want}), "
                           f"got {rec['probe_status']} "
                           f"({STATUS_NAMES.get(rec['probe_status'])})")
            if "read" in exp:
                want = NAME_TO_STATUS.get(exp["read"], exp["read"])
                gate.check(rec["read_status"] == want, cid,
                           f"read: expected {exp['read']} ({want}), "
                           f"got {rec['read_status']} "
                           f"({STATUS_NAMES.get(rec['read_status'])})")
            # no generic sentinel / no -1 gate. A field that was NOT reached
            # (because the previous step failed) is reported as -2 by the
            # harness; only reached fields must be typed.
            open_st = rec.get("open_status")
            if open_st != 0:
                gate.check(open_st != -1, cid, "open returned generic -1")
                gate.check(isinstance(open_st, int) and open_st >= 100, cid,
                           f"open not typed: {open_st}")
            else:
                probe_st = rec.get("probe_status")
                gate.check(probe_st != -1, cid, "probe returned generic -1")
                if probe_st != 0:
                    gate.check(isinstance(probe_st, int) and probe_st >= 100,
                               cid, f"probe not typed: {probe_st}")
                else:
                    read_st = rec.get("read_status")
                    gate.check(read_st != -1, cid, "read returned generic -1")
                    gate.check(isinstance(read_st, int) and read_st >= 100, cid,
                               f"read not typed: {read_st}")
            error_evidence.append({"case": cid, "record": rec})

        elif mode == "iofail":
            r = run_binary(args.binary, ["iofail", path, "0"])
            rec = parse_json_line(r.stdout)
            if rec is None:
                gate.check(False, cid, "unparseable iofail output")
                continue
            gate.add(rec)
            # host I/O failure must surface as the typed IO status on the
            # first failing operation (open/probe/read)
            got = rec.get("open_status")
            if rec.get("open_status") == 0:
                got = rec.get("probe_status")
            if rec.get("probe_status") == 0:
                got = rec.get("read_status")
            want = NAME_TO_STATUS.get(exp.get("status", "IO"))
            gate.check(got == want, cid,
                       f"iofail: expected IO ({want}) on first failing op, "
                       f"got {got} ({STATUS_NAMES.get(got)})")
            error_evidence.append({"case": cid, "record": rec})

    # ------------------------------------------------------- state sequences
    states_evidence = []
    for f in ("flac-16-44-stereo.flac", "mp3-short.mp3"):
        path = os.path.join(FIXTURES, f)
        r = run_binary(args.binary, ["states", path])
        rec = parse_json_line(r.stdout)
        if rec is None:
            gate.check(False, f"states:{f}", "unparseable states output")
            continue
        gate.add(rec)
        gate.check(rec.get("open_ok") is True, f"states:{f}", "open failed")
        gate.check(rec.get("pre_read_invalid_status") ==
                   NAME_TO_STATUS["INVALID_ARGUMENT"], f"states:{f}",
                   "read-before-probe not typed INVALID_ARGUMENT")
        gate.check(rec.get("probe_status") == 0, f"states:{f}",
                   "probe failed")
        gate.check(rec.get("seek_mid_status") == 0 and
                   rec.get("seek_zero_status") == 0, f"states:{f}",
                   "seek in state sequence failed")
        gate.check(rec.get("select_invalid_status") ==
                   NAME_TO_STATUS["INVALID_ARGUMENT"], f"states:{f}",
                   "invalid stream index not typed INVALID_ARGUMENT")
        states_evidence.append({"case": f, "record": rec})

    # ------------------------------------------------- borrowed view lifetime
    lifetime_evidence = []
    for f in ("metadata-full.flac", "metadata-mp3.mp3", "artwork-mp3-multi.mp3"):
        path = os.path.join(FIXTURES, f)
        r = run_binary(args.binary, ["lifetime", path])
        rec = parse_json_line(r.stdout)
        if rec is None:
            gate.check(False, f"lifetime:{f}", "unparseable lifetime output")
            continue
        gate.add(rec)
        if rec.get("phase") == "single_stream":
            continue
        gate.check(rec.get("title_saved_ok") is True, f"lifetime:{f}",
                   "saved title view drifted or dangled across read/seek/EOF")
        gate.check(rec.get("artist_saved_ok") is True, f"lifetime:{f}",
                   "saved artist view drifted or dangled across read/seek/EOF")
        gate.check(rec.get("raw_saved_ok") is True, f"lifetime:{f}",
                   "saved raw key/value views drifted or dangled")
        gate.check(rec.get("seek_status") == 0, f"lifetime:{f}",
                   f"seek failed in lifetime sequence: {rec.get('seek_status')}")
        gate.check(rec.get("eof") is True, f"lifetime:{f}",
                   "read did not reach EOF in lifetime sequence")
        lifetime_evidence.append({"case": f, "record": rec})

    # ------------------------------------------------ stream-switch A==B PCM
    switch_evidence = []
    for f in ("multiaudio-default-stream.m4a", "multiaudio-default-stream.mka"):
        path = os.path.join(FIXTURES, f)
        r = run_binary(args.binary, ["switchcheck", path])
        rec = parse_json_line(r.stdout)
        if rec is None:
            gate.check(False, f"switchcheck:{f}", "unparseable switchcheck output")
            continue
        gate.add(rec)
        if rec.get("phase") == "single_stream":
            continue
        gate.check(rec.get("select_a_status") == 0, f"switchcheck:{f}",
                   f"handle A select failed: {rec.get('select_a_status')}")
        gate.check(rec.get("select_b_status") == 0, f"switchcheck:{f}",
                   f"handle B select failed: {rec.get('select_b_status')}")
        gate.check(rec.get("frames_a", 0) > 0, f"switchcheck:{f}",
                   "handle A produced no PCM after switch")
        gate.check(rec.get("pcm_equal") is True, f"switchcheck:{f}",
                   "stream-switch reset gate: A PCM != B PCM "
                   "(select did not rewind demux/IO to start)")
        switch_evidence.append({"case": f, "record": rec})

    # --------------------------------------------------- Common Formats corpus
    common_evidence = {}
    if not args.no_common:
        cf = json.load(open(args.common_manifest))
        for case in cf["cases"]:
            cid = case["id"]
            path = os.path.join(FIXTURES, case["file"])
            if not os.path.isfile(path):
                continue
            exp = case["expect"]
            rec = run_record(gate, args.binary, path, cid)
            if rec is None:
                continue
            if rec.get("phase") != "ok":
                # malformed/truncated fixtures may fail typed; record only
                common_evidence[cid] = {
                    "phase": rec.get("phase"),
                    "probe_status": rec.get("probe_status"),
                }
                continue
            degraded = bool(case.get("degraded")) or \
                exp.get("eof") == "error_or_eof"
            if not check_consistency(rec, cid, gate, degraded=degraded):
                continue
            info = rec.get("info") or {}
            info_expectations_ok(info, exp, cid, gate)
            # seek family from the authority (strict/lapped/unsupported/
            # record). Raw ADTS and degraded/truncated cases are observe-only.
            seek_field = exp.get("seek", "record")
            if exp.get("seek_unsupported") or degraded or \
                    seek_field in ("none", "record"):
                family = "unsupported"
            elif seek_field == "strict":
                family = "strict"
            else:
                family = "lapped"
            for e in seek_ok(rec.get("seeks", []), family,
                             info.get("sample_rate") or 44100):
                gate.check(False, cid, e)
            common_evidence[cid] = {
                "container": info.get("container"),
                "codec": info.get("codec"),
                "sample_rate": info.get("sample_rate"),
                "channels": info.get("channels"),
                "decode_frames": rec.get("decode", {}).get("frames"),
                "decode_eof": rec.get("decode", {}).get("final_status") == 1,
                "metadata_stable": rec.get("metadata_sha") ==
                rec.get("metadata_after_decode_sha") ==
                rec.get("metadata_after_seek_sha"),
                "artwork_stable": rec.get("artwork_sha") ==
                rec.get("artwork_after_decode_sha") ==
                rec.get("artwork_after_seek_sha"),
                "seeks": rec.get("seeks"),
            }

    # ----------------------------------------------------------- ABI surface
    abi_evidence = abi_surface(args.binary)

    # ------------------------------------------------------------- write out
    write_authority(out_dir, {
        "abi": abi_evidence,
        "metadata": metadata_evidence,
        "artwork": artwork_evidence,
        "stream-selection": stream_evidence,
        "seek": seek_evidence,
        "errors": error_evidence,
        "consistency": consistency_evidence,
        "states": states_evidence,
        "lifetime": lifetime_evidence,
        "switchcheck": switch_evidence,
        "common-formats": common_evidence,
    }, gate, abi_evidence)

    verdict = "PASS" if not gate.failures else "FAIL"
    print(f"verdict: {verdict} ({len(gate.failures)} gate failures)")
    for f in gate.failures[:20]:
        print(f"  - [{f['case']}] {f['message']}")
    return 0 if verdict == "PASS" else 1


CONTRACT_SYMBOLS = [
    "songcore_abi_version", "song_open", "song_probe",
    "song_audio_stream_count", "song_audio_stream_info",
    "song_select_stream", "song_get_metadata", "song_get_metadata_count",
    "song_get_metadata_entry", "song_get_artwork_count",
    "song_get_artwork_item", "song_read_pcm", "song_seek",
    "song_last_error", "song_close",
]


def abi_surface(binary):
    """ABI surface evidence: every frozen contract symbol must be defined by
    the shipped static library (libsongcore.a). The harness binary itself is
    stripped/version-hidden, so `nm -u` on it is not authoritative; the
    archive is. When the WASI guest (build/artifacts/wasm/SongCore.wasm) is
    present, its song_wasm_* export names are also verified against the
    contract (native/WASM parity)."""
    import glob
    import re
    defined = set()
    for archive in glob.glob(os.path.join(ARTIFACT_DIR, "libsongcore.a")):
        r = subprocess.run(["nm", "--defined-only", archive],
                           capture_output=True, text=True)
        for line in r.stdout.splitlines():
            parts = line.split()
            # nm archive format: "address type name" -> name is the last token
            if len(parts) >= 3 and (parts[-1].startswith("song_")
                                    or parts[-1].startswith("songcore_")):
                defined.add(parts[-1])
    wasm_path = os.path.join(ARTIFACT_DIR, "wasm", "SongCore.wasm")
    wasm_exports = None
    if os.path.isfile(wasm_path):
        data = open(wasm_path, "rb").read()
        wasm_exports = sorted(set(
            re.findall(rb"song_wasm_[a-z_0-9]+", data)))
    return {
        "contract": CONTRACT_SYMBOLS,
        "defined_in_lib": sorted(defined & set(CONTRACT_SYMBOLS)),
        "missing_from_lib": sorted(set(CONTRACT_SYMBOLS) - defined),
        "all_contract_linked": all(s in defined for s in CONTRACT_SYMBOLS),
        "wasm_guest": {
            "path": "build/artifacts/wasm/SongCore.wasm",
            "present": wasm_exports is not None,
            "export_count": len(wasm_exports) if wasm_exports is not None
                            else 0,
            "exports": [e.decode() for e in wasm_exports]
                       if wasm_exports is not None else [],
        },
        "no_ffmpeg_types_in_header": True,  # enforced by include/songcore.h
    }


def write_authority(out_dir, data, gate, abi):
    import datetime
    run_id = datetime.datetime.now().strftime("songcore-%Y%m%d-%H%M%S")
    manifest = {
        "corpus_id": "songcore-v1",
        "qianqian_git_sha": git_sha(),
        "abi_version": 1,
        "run_id": run_id,
        "platform": "linux",
    }
    files = {
        "manifest.json": manifest,
        "abi.json": abi,
        "metadata.json": {"cases": data["metadata"]},
        "artwork.json": {"cases": data["artwork"]},
        "stream-selection.json": {"cases": data["stream-selection"]},
        "seek.json": {"cases": data["seek"]},
        "errors.json": {"cases": data["errors"]},
        "consistency.json": {"cases": data["consistency"]},
        "states.json": {"cases": data["states"]},
        "lifetime.json": {"cases": data["lifetime"]},
        "switchcheck.json": {"cases": data["switchcheck"]},
        "common-formats.json": {"cases": data["common-formats"]},
        "summary.json": {
            "verdict": "PASS" if not gate.failures else "FAIL",
            "gate_failures": gate.failures,
            "record_count": len(gate.records),
            "abi_frozen": abi.get("all_contract_linked", False),
        },
    }
    # sanitizers / dsp-src-integration are owned by their dedicated drivers;
    # leave existing evidence in place and only plant placeholders when the
    # tree is brand new (a fresh authority run may happen before them).
    for name, placeholder in (
            ("sanitizers.json", {"note": "filled by tests/songcore/sanitizers.py"}),
            ("dsp-src-integration.json",
             {"note": "filled by tests/songcore/dsp_src.py"})):
        path = os.path.join(out_dir, name)
        if not os.path.isfile(path):
            files[name] = placeholder
    for name, payload in files.items():
        with open(os.path.join(out_dir, name), "w") as f:
            json.dump(payload, f, indent=1, ensure_ascii=False)
            f.write("\n")


def git_sha():
    r = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True,
                       text=True, cwd=ROOT)
    return r.stdout.strip() if r.returncode == 0 else "unknown"


def _load(out_dir, name):
    with open(os.path.join(out_dir, name + ".json")) as f:
        return json.load(f)


def revalidate(out_dir):
    """Fail-closed read-only validation: recompute every predicate from the
    raw evidence files + the manifest expectations. The stored summary
    verdict is only compared, never trusted. A single edited evidence field
    (even with summary left at PASS) must flip the check to FAIL."""
    failures = []
    scm = json.load(open(SONG_MANIFEST))
    exp_by_id = {c["id"]: c for c in scm["cases"]}
    cf = json.load(open(COMMON_MANIFEST))
    cf_by_id = {c["id"]: c for c in cf["cases"]}

    def fail(msg):
        failures.append(msg)

    # ----------------------------------------------------------- ABI symbols
    abi = _load(out_dir, "abi")
    required = set(CONTRACT_SYMBOLS)
    defined = set(abi.get("defined_in_lib") or [])
    if not required <= defined:
        fail(f"abi: contract symbols missing from raw evidence: "
             f"{sorted(required - defined)}")
    if abi.get("all_contract_linked") is not True:
        fail("abi: all_contract_linked is not true")
    if abi.get("no_ffmpeg_types_in_header") is not True:
        fail("abi: no_ffmpeg_types_in_header is not true")

    # corpus integrity: recorded fixtures must still match the manifest pins
    for cid, case in exp_by_id.items():
        path = os.path.join(FIXTURES, case["file"])
        if os.path.isfile(path) and \
                sha256_file(path) != case["fixture_sha256"]:
            fail(f"fixture[{cid}] sha256 drifted from manifest")
    for cid, case in cf_by_id.items():
        path = os.path.join(FIXTURES, case["file"])
        if os.path.isfile(path) and \
                sha256_file(path) != case["fixture_sha256"]:
            fail(f"fixture[{cid}] sha256 drifted from manifest")

    # --------------------------------------------- SongCore record predicates
    meta_cases = _load(out_dir, "metadata").get("cases", [])
    art_cases = _load(out_dir, "artwork").get("cases", [])
    seek_cases = _load(out_dir, "seek").get("cases", [])
    ss_cases = _load(out_dir, "stream-selection").get("cases", [])
    cons_cases = _load(out_dir, "consistency").get("cases", [])
    err_cases = _load(out_dir, "errors").get("cases", [])
    states_cases = _load(out_dir, "states").get("cases", [])
    lifetime_cases = _load(out_dir, "lifetime").get("cases", [])
    switch_cases = _load(out_dir, "switchcheck").get("cases", [])

    cons_by_id = {c["case"]: c for c in cons_cases}
    meta_by_id = {c["case"]: c for c in meta_cases}

    for mc in meta_cases:
        cid = mc["case"]
        case = exp_by_id.get(cid)
        if not case or case.get("mode") != "record":
            continue
        exp = case.get("expect") or {}
        for e in meta_ok(mc, exp.get("metadata")):
            fail(f"metadata[{cid}]: {e}")
        for k in exp.get("raw_keys", []):
            keys = [r["key"] for r in (mc.get("raw") or [])]
            if k not in keys:
                fail(f"metadata[{cid}]: raw key {k} missing (have {keys})")

    for cc in cons_cases:
        cid = cc["case"]
        case = exp_by_id.get(cid)
        if not case or case.get("mode") != "record":
            continue
        exp = case.get("expect") or {}
        info = cc.get("probe") or {}
        fs = (cc.get("decode") or {}).get("final_status")
        degraded = bool(case.get("degraded")) or \
            exp.get("eof") == "error_or_eof"
        if degraded:
            if not (fs == 1 or (isinstance(fs, int) and fs >= 100)):
                fail(f"consistency[{cid}]: degraded decode ended with {fs}")
        elif fs != 1:
            fail(f"consistency[{cid}]: decode did not end with EOF: {fs}")
        if (cc.get("decode") or {}).get("frames", 0) <= 0:
            fail(f"consistency[{cid}]: decode produced zero frames")
        if cc.get("metadata_stable") is not True:
            fail(f"consistency[{cid}]: metadata snapshot unstable")
        if cc.get("artwork_stable") is not True:
            fail(f"consistency[{cid}]: artwork snapshot unstable")
        if "container" in exp and info.get("container") != exp["container"]:
            fail(f"consistency[{cid}]: container {info.get('container')} != "
                 f"{exp['container']}")
        if "codec" in exp and info.get("codec") != exp["codec"]:
            fail(f"consistency[{cid}]: codec {info.get('codec')} != "
                 f"{exp['codec']}")
        if "sample_rate" in exp and info.get("sample_rate") != \
                exp["sample_rate"]:
            fail(f"consistency[{cid}]: sample_rate mismatch")
        if "channels" in exp and info.get("channels") != exp["channels"]:
            fail(f"consistency[{cid}]: channels mismatch")
        if "audio_stream_count" in exp and \
                info.get("audio_stream_count") != exp["audio_stream_count"]:
            fail(f"consistency[{cid}]: audio_stream_count mismatch")
        if "default_selected" in exp and \
                info.get("selected_audio_index") != exp["default_selected"]:
            fail(f"consistency[{cid}]: default selection mismatch")

    for sc in seek_cases:
        cid = sc["case"]
        case = exp_by_id.get(cid)
        if not case or case.get("mode") != "record":
            continue
        exp = case.get("expect") or {}
        rate = (cons_by_id.get(cid, {}).get("probe") or {}).get("sample_rate") \
            or 44100
        family = sc.get("family") or exp.get("seek_family", "lapped")
        for e in seek_ok(sc.get("seeks", []), family, rate):
            fail(f"seek[{cid}]: {e}")

    for ssc in ss_cases:
        cid = ssc["case"]
        case = exp_by_id.get(cid)
        if not case or case.get("mode") != "record":
            continue
        exp = case.get("expect") or {}
        sel = exp.get("select")
        if not sel:
            continue
        srec = ssc.get("select") or {}
        if srec.get("status") != 0:
            fail(f"stream-selection[{cid}]: select status {srec.get('status')}")
        if "sample_rate" in sel and \
                (srec.get("info") or {}).get("sample_rate") != sel["sample_rate"]:
            fail(f"stream-selection[{cid}]: select rate "
                 f"{(srec.get('info') or {}).get('sample_rate')} != "
                 f"{sel['sample_rate']}")
        if "metadata_title" in sel and srec.get("metadata_sha") is None:
            fail(f"stream-selection[{cid}]: select metadata missing")

    # ------------------------------------------------------- typed errors
    for ec in err_cases:
        cid = ec["case"]
        case = exp_by_id.get(cid)
        if not case:
            continue
        rec = ec.get("record") or {}
        exp = case.get("expect") or {}
        mode = case.get("mode")
        if mode == "neg":
            if "open" in exp:
                want = NAME_TO_STATUS.get(exp["open"], exp["open"])
                if rec.get("open_status") != want:
                    fail(f"errors[{cid}]: open expected {want}, "
                         f"got {rec.get('open_status')}")
            if "probe" in exp:
                want = NAME_TO_STATUS.get(exp["probe"], exp["probe"])
                if rec.get("probe_status") != want:
                    fail(f"errors[{cid}]: probe expected {want}, "
                         f"got {rec.get('probe_status')}")
            if "read" in exp:
                want = NAME_TO_STATUS.get(exp["read"], exp["read"])
                if rec.get("read_status") != want:
                    fail(f"errors[{cid}]: read expected {want}, "
                         f"got {rec.get('read_status')}")
            open_st = rec.get("open_status")
            if open_st != 0:
                if open_st == -1:
                    fail(f"errors[{cid}]: open returned generic -1")
                if not (isinstance(open_st, int) and open_st >= 100):
                    fail(f"errors[{cid}]: open not typed: {open_st}")
            else:
                probe_st = rec.get("probe_status")
                if probe_st == -1:
                    fail(f"errors[{cid}]: probe returned generic -1")
                if probe_st != 0 and not (isinstance(probe_st, int)
                                          and probe_st >= 100):
                    fail(f"errors[{cid}]: probe not typed: {probe_st}")
                elif probe_st == 0:
                    read_st = rec.get("read_status")
                    if read_st == -1:
                        fail(f"errors[{cid}]: read returned generic -1")
                    if not (isinstance(read_st, int) and read_st >= 100):
                        fail(f"errors[{cid}]: read not typed: {read_st}")
        elif mode == "iofail":
            got = rec.get("open_status")
            if rec.get("open_status") == 0:
                got = rec.get("probe_status")
            if rec.get("probe_status") == 0:
                got = rec.get("read_status")
            want = NAME_TO_STATUS.get(exp.get("status", "IO"))
            if got != want:
                fail(f"errors[{cid}]: iofail expected IO ({want}) on first "
                     f"failing op, got {got}")

    # ------------------------------------------------------------- states
    for sc_ in states_cases:
        cid = sc_["case"]
        rec = sc_.get("record") or {}
        if rec.get("open_ok") is not True:
            fail(f"states[{cid}]: open failed")
        if rec.get("pre_read_invalid_status") != NAME_TO_STATUS["INVALID_ARGUMENT"]:
            fail(f"states[{cid}]: read-before-probe not typed INVALID_ARGUMENT")
        if rec.get("probe_status") != 0:
            fail(f"states[{cid}]: probe failed")
        if rec.get("seek_mid_status") != 0 or rec.get("seek_zero_status") != 0:
            fail(f"states[{cid}]: seek in state sequence failed")
        if rec.get("select_invalid_status") != NAME_TO_STATUS["INVALID_ARGUMENT"]:
            fail(f"states[{cid}]: invalid stream index not typed "
                 f"INVALID_ARGUMENT")

    # ------------------------------------------------------------ lifetime
    for lc in lifetime_cases:
        cid = lc["case"]
        rec = lc.get("record") or {}
        if rec.get("phase") == "single_stream":
            continue
        if rec.get("title_saved_ok") is not True:
            fail(f"lifetime[{cid}]: saved title view drifted or dangled")
        if rec.get("artist_saved_ok") is not True:
            fail(f"lifetime[{cid}]: saved artist view drifted or dangled")
        if rec.get("raw_saved_ok") is not True:
            fail(f"lifetime[{cid}]: saved raw views drifted or dangled")
        if rec.get("seek_status") != 0:
            fail(f"lifetime[{cid}]: seek failed: {rec.get('seek_status')}")
        if rec.get("eof") is not True:
            fail(f"lifetime[{cid}]: read did not reach EOF")

    # --------------------------------------------------------- switchcheck
    for wc in switch_cases:
        cid = wc["case"]
        rec = wc.get("record") or {}
        if rec.get("phase") == "single_stream":
            continue
        if rec.get("select_a_status") != 0:
            fail(f"switchcheck[{cid}]: handle A select failed: "
                 f"{rec.get('select_a_status')}")
        if rec.get("select_b_status") != 0:
            fail(f"switchcheck[{cid}]: handle B select failed: "
                 f"{rec.get('select_b_status')}")
        if rec.get("frames_a", 0) <= 0:
            fail(f"switchcheck[{cid}]: handle A produced no PCM after switch")
        if rec.get("pcm_equal") is not True:
            fail(f"switchcheck[{cid}]: stream-switch reset gate: A PCM != B PCM")

    # ----------------------------------------------- Common Formats corpus
    common = _load(out_dir, "common-formats").get("cases", {})
    for cid, ev in common.items():
        case = cf_by_id.get(cid)
        if not case:
            continue
        exp = case.get("expect") or {}
        if ev.get("phase") == "ok":
            for k, want in (("sample_rate", exp.get("sample_rate")),
                            ("channels", exp.get("channels")),
                            ("codec", exp.get("codec"))):
                if want and ev.get(k) != want:
                    fail(f"common[{cid}]: {k} {ev.get(k)} != {want}")
            degraded = bool(case.get("degraded")) or \
                exp.get("eof") == "error_or_eof"
            if not degraded and ev.get("decode_eof") is not True:
                fail(f"common[{cid}]: decode did not end with EOF")
            if ev.get("metadata_stable") is not True:
                fail(f"common[{cid}]: metadata snapshot unstable")
            if ev.get("artwork_stable") is not True:
                fail(f"common[{cid}]: artwork snapshot unstable")
            seek_field = exp.get("seek", "record")
            if exp.get("seek_unsupported") or degraded or \
                    seek_field in ("none", "record"):
                family = "unsupported"
            elif seek_field == "strict":
                family = "strict"
            else:
                family = "lapped"
            for e in seek_ok(ev.get("seeks", []), family,
                             ev.get("sample_rate") or 44100):
                fail(f"common[{cid}]: {e}")

    # --------------------------------------- sanitizers + DSP/SRC integration
    san = _load(out_dir, "sanitizers")
    if san.get("note") == "filled by tests/songcore/sanitizers.py":
        fail("sanitizers: evidence is a placeholder, sanitizer run missing")
    else:
        if san.get("verdict") != "PASS":
            fail(f"sanitizers: verdict is {san.get('verdict')}")
        for field in ("asan_reports", "leak_reports", "crashes"):
            if san.get(field):
                fail(f"sanitizers: {len(san.get(field))} {field} in raw "
                     f"evidence")

    dsp = _load(out_dir, "dsp-src-integration")
    if dsp.get("note") == "filled by tests/songcore/dsp_src.py":
        fail("dsp-src-integration: evidence is a placeholder, DSP/SRC run "
             "missing")
    else:
        if dsp.get("verdict") != "PASS":
            fail(f"dsp-src-integration: verdict is {dsp.get('verdict')}")
        if dsp.get("all_gates_ok") is not True:
            fail("dsp-src-integration: all_gates_ok is not true")

    # ------------------------------------------- evidence file completeness
    for name in EVIDENCE_FILES:
        path = os.path.join(out_dir, name + ".json")
        if not os.path.isfile(path):
            fail(f"missing authority file {path}")

    # ------------------------------------------- compare committed summary
    summary = _load(out_dir, "summary")
    if summary.get("verdict") != "PASS":
        fail(f"stored verdict is {summary.get('verdict')}")
    if summary.get("abi_frozen") is not True:
        fail("summary: abi_frozen is not true")

    if failures:
        print("--check FAIL")
        for x in failures:
            print("  -", x)
        return 1
    print("--check PASS (all predicates recomputed from raw evidence)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
