#!/usr/bin/env python3
"""Generate the Stage A corpus: deterministic synthetic MP3/FLAC fixtures.

All audio is synthesized from fixed lavfi expressions (no copyrighted
material). Encoding uses the host ffmpeg purely as a corpus *generator*;
the benchmark itself never touches it. Every fixture's bytes, structural
expectations and PCM checksums are recorded in corpus/manifest/stage-a.json.

For lossless FLAC the manifest pins:
  - exact sample count
  - canonical Float32-interleaved sha256 (the strict PCM gate)

For MP3 (lossy) the manifest records reference checksums as information
only; the gates are structural expectations and cross-profile PCM
consistency enforced by the harness.

Usage: python3 corpus/tools/gen_corpus.py [--out corpus]
"""
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIXTURES = os.path.join(ROOT, "corpus", "fixtures")
WORK = os.path.join(ROOT, "build", "corpus-work")


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"command failed: {' '.join(cmd)}\n{r.stderr[-2000:]}")


def ffmpeg_version():
    out = subprocess.run(["ffmpeg", "-version"], capture_output=True, text=True).stdout
    return out.splitlines()[0].strip()


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()


def decode_f32(path):
    """Full audio decode to canonical raw Float32-interleaved (f32le) bytes."""
    r = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-map", "0:a:0",
                        "-f", "f32le", "-c:a", "pcm_f32le", "-"], capture_output=True)
    if r.returncode != 0:
        raise SystemExit(f"decode failed: {path}\n{r.stderr[-2000:]}")
    return r.stdout


def canonical_f32_sha(path):
    """sha256 of the raw Float32-interleaved PCM (f32le) a decoder must emit."""
    data = decode_f32(path)
    return hashlib.sha256(data).hexdigest(), len(data)


def stream_info(path):
    out = subprocess.run([
        "ffprobe", "-v", "error", "-select_streams", "a:0",
        "-show_entries", "stream=duration,sample_rate,channels",
        "-of", "json", path], capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit(f"ffprobe failed: {path}")
    return json.loads(out.stdout)["streams"][0]


def sample_count(path):
    st = stream_info(path)
    ch = int(st["channels"])
    n = len(decode_f32(path)) // (4 * ch)
    return n, st


def read_metadata(path):
    out = subprocess.run([
        "ffprobe", "-v", "error", "-show_entries",
        "format_tags=title,artist,album,date", "-of", "json", path],
        capture_output=True, text=True)
    tags = json.loads(out.stdout).get("format", {}).get("tags", {})
    return {k.upper(): v for k, v in tags.items()}


def make_png(path, w, h):
    """Deterministic RGB gradient PNG, pure stdlib."""
    raw = b""
    for y in range(h):
        raw += b"\x00"
        for x in range(w):
            raw += bytes(((x * 255) // max(w - 1, 1),
                          (y * 255) // max(h - 1, 1),
                          ((x + y) * 255) // max(w + h - 2, 1)))

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
           + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
    open(path, "wb").write(png)


def probe_artwork(path):
    """First attached-picture packet: size + sha256, via ffmpeg stream copy."""
    out = subprocess.run([
        "ffprobe", "-v", "error", "-select_streams", "v",
        "-show_entries",
        "stream=codec_name:stream_disposition=attached_pic",
        "-of", "json", path], capture_output=True, text=True)
    streams = json.loads(out.stdout).get("streams", [])
    pic = None
    for st in streams:
        if st.get("disposition", {}).get("attached_pic") == 1:
            pic = st
            break
    if pic is None:
        return None
    # extract attached picture bytes by dumping the stream
    r = subprocess.run(["ffmpeg", "-v", "error", "-i", path,
                        "-map", "0:v:0", "-c", "copy", "-f", "data", "-"],
                       capture_output=True)
    if r.returncode != 0 or not r.stdout:
        raise SystemExit(f"artwork extraction failed: {path}")
    return {"sha256": hashlib.sha256(r.stdout).hexdigest(),
            "size": len(r.stdout),
            "codec": pic.get("codec_name", "unknown")}


# signal snippets (deterministic math, no randomness)
SIG_STEREO = "aevalsrc=0.55*sin(2*PI*440*t)|0.40*sin(2*PI*554.365*t):s=44100:d={dur}"
SIG_MONO = "aevalsrc=0.60*sin(2*PI*330*t):s=44100:d={dur}"
SIG_24 = "aevalsrc=0.50*sin(2*PI*523.251*t)|0.35*sin(2*PI*659.255*t):s=96000:d={dur}"

ID3_V3 = ["-id3v2_version", "3",
          "-metadata", "title=千千测试 CBR",
          "-metadata", "artist=Qianqian Lab",
          "-metadata", "album=Corpus Alpha"]
ID3_V4 = ["-id3v2_version", "4",
          "-metadata", "title=Qianqian VBR Test",
          "-metadata", "artist=Qianqian Lab",
          "-metadata", "album=Corpus Beta"]


def encode_mp3(out, sig, enc_args, extra_inputs=None, meta=True):
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact"]
    cmd += ["-f", "lavfi", "-i", sig]
    if extra_inputs:
        for inp in extra_inputs:
            cmd += ["-i", inp]
    cmd += ["-map", "0:a"]
    if extra_inputs:
        cmd += ["-map", "1:v"]
    if not meta:
        cmd += ["-map_metadata", "-1"]
    cmd += ["-c:a", "libmp3lame"] + enc_args
    if extra_inputs:
        cmd += ["-c:v", "png", "-disposition:v", "attached_pic",
                "-metadata:s:v", "title=Album cover",
                "-metadata:s:v", "comment=Cover (front)"]
    cmd += ["-f", "mp3", out]
    run(cmd)


def encode_flac(out, sig, flac_args=None, extra_inputs=None, meta=True):
    # route through a bitexact wav intermediate so the FLAC bit depth is explicit
    wav = out + ".tmp.wav"
    run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", sig,
         "-c:a", "pcm_s16le" if "96000" not in sig else "pcm_s24le", wav])
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact", "-i", wav]
    if extra_inputs:
        for inp in extra_inputs:
            cmd += ["-i", inp]
    cmd += ["-map", "0:a"]
    if extra_inputs:
        cmd += ["-map", "1:v", "-c:v", "png", "-disposition:v", "attached_pic"]
    if not meta:
        cmd += ["-map_metadata", "-1"]
    cmd += ["-c:a", "flac"] + (flac_args or []) + ["-f", "flac", out]
    run(cmd)
    os.remove(wav)


def truncate(src, dst, frac):
    data = open(src, "rb").read()
    open(dst, "wb").write(data[: int(len(data) * frac)])


def main():
    os.makedirs(FIXTURES, exist_ok=True)
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    tool = ffmpeg_version()
    cases = []

    png_small = os.path.join(WORK, "cover-small.png")
    png_big = os.path.join(WORK, "cover-large.png")
    make_png(png_small, 64, 64)
    make_png(png_big, 256, 256)

    def add(case_id, fname, fmt, expect, throughput=False, degraded=False,
            with_stream_info=True):
        p = os.path.join(FIXTURES, fname)
        if with_stream_info:
            n_samples, st = sample_count(p)
            expect.setdefault("sample_rate", int(st["sample_rate"]))
            expect.setdefault("channels", int(st["channels"]))
            expect.setdefault("samples", n_samples)
            expect.setdefault("duration_us", round(float(st.get("duration", 0)) * 1e6))
        else:
            n_samples = 0
        case = {
            "id": case_id,
            "file": fname,
            "format": fmt,
            "fixture_sha256": sha256_file(p),
            "fixture_bytes": os.path.getsize(p),
            "throughput": throughput,
            "degraded": degraded,
            "expect": expect,
        }
        cases.append(case)
        print(f"  {case_id}: {os.path.getsize(p)} bytes, {n_samples} samples")

    print("MP3 fixtures:")
    p = os.path.join(FIXTURES, "mp3-cbr-id3v23.mp3")
    encode_mp3(p, SIG_STEREO.format(dur=4), ["-b:a", "128k", "-write_xing", "1"] + ID3_V3)
    add("mp3-cbr-id3v23", "mp3-cbr-id3v23.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "metadata": read_metadata(p), "artwork": None, "seek": "record", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "mp3-cbr-id3v23-artwork.mp3")
    encode_mp3(p, SIG_STEREO.format(dur=4), ["-b:a", "128k"] + ID3_V3,
               extra_inputs=[png_small])
    add("mp3-cbr-id3v23-artwork", "mp3-cbr-id3v23-artwork.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "metadata": read_metadata(p), "artwork": probe_artwork(p),
         "seek": "record", "eof": "clean"})

    p = os.path.join(FIXTURES, "mp3-vbr-id3v24.mp3")
    encode_mp3(p, SIG_STEREO.format(dur=4), ["-q:a", "4", "-write_xing", "1"] + ID3_V4)
    add("mp3-vbr-id3v24", "mp3-vbr-id3v24.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "metadata": read_metadata(p), "artwork": None, "seek": "record", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "mp3-minimal.mp3")
    encode_mp3(p, SIG_STEREO.format(dur=2), ["-b:a", "128k"], meta=False)
    add("mp3-minimal", "mp3-minimal.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "metadata_absent_or_empty": True, "artwork": None,
         "seek": "record", "eof": "clean"})

    p = os.path.join(FIXTURES, "mp3-short.mp3")
    encode_mp3(p, SIG_STEREO.format(dur=0.3), ["-b:a", "128k"] + ID3_V3)
    add("mp3-short", "mp3-short.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "metadata": read_metadata(p), "artwork": None, "seek": "record", "eof": "clean"})

    p = os.path.join(FIXTURES, "mp3-long.mp3")
    encode_mp3(p, SIG_STEREO.format(dur=12), ["-b:a", "96k"] + ID3_V3)
    add("mp3-long", "mp3-long.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "metadata": read_metadata(p), "artwork": None, "seek": "record", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "mp3-corrupt-tail.mp3")
    truncate(os.path.join(FIXTURES, "mp3-cbr-id3v23.mp3"), p, 0.75)
    full = next(c for c in cases if c["id"] == "mp3-cbr-id3v23")
    add("mp3-corrupt-tail", "mp3-corrupt-tail.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "min_samples": int(full["expect"]["samples"] * 0.5), "artwork": None,
         "seek": "record", "eof": "error_or_eof"}, degraded=True)

    p = os.path.join(FIXTURES, "mp3-truncated-header.mp3")
    truncate(os.path.join(FIXTURES, "mp3-cbr-id3v23.mp3"), p, 0.012)
    add("mp3-truncated-header", "mp3-truncated-header.mp3", "mp3",
        {"container": "mp3", "codec": "mp3", "pcm": {"mode": "consistency"},
         "artwork": None, "seek": "none", "eof": "error_or_eof",
         "probe_may_fail": True}, degraded=True, with_stream_info=False)

    print("FLAC fixtures:")
    p = os.path.join(FIXTURES, "flac-16-44-stereo.flac")
    encode_flac(p, SIG_STEREO.format(dur=4),
                ["-metadata", "TITLE=Flac Sixteen Fortyfour",
                 "-metadata", "ARTIST=Qianqian Lab",
                 "-metadata", "ALBUM=Corpus Gamma"])
    sha, _ = canonical_f32_sha(p)
    add("flac-16-44-stereo", "flac-16-44-stereo.flac", "flac",
        {"container": "flac", "codec": "flac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": read_metadata(p), "artwork": None, "seek": "strict", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "flac-16-44-mono.flac")
    encode_flac(p, SIG_MONO.format(dur=3), ["-metadata", "TITLE=Flac Mono"])
    sha, _ = canonical_f32_sha(p)
    add("flac-16-44-mono", "flac-16-44-mono.flac", "flac",
        {"container": "flac", "codec": "flac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": read_metadata(p), "artwork": None, "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "flac-24-96.flac")
    encode_flac(p, SIG_24.format(dur=3), ["-sample_fmt", "s32",
                "-metadata", "TITLE=Flac Twentyfour Ninetysix"])
    sha, _ = canonical_f32_sha(p)
    add("flac-24-96", "flac-24-96.flac", "flac",
        {"container": "flac", "codec": "flac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": read_metadata(p), "artwork": None, "seek": "strict", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "flac-artwork.flac")
    encode_flac(p, SIG_STEREO.format(dur=3), ["-metadata", "TITLE=Flac With Art"],
                extra_inputs=[png_big])
    sha, _ = canonical_f32_sha(p)
    add("flac-artwork", "flac-artwork.flac", "flac",
        {"container": "flac", "codec": "flac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": read_metadata(p), "artwork": probe_artwork(p),
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "flac-replaygain.flac")
    encode_flac(p, SIG_STEREO.format(dur=2),
                ["-metadata", "TITLE=Flac Replaygain",
                 "-metadata", "REPLAYGAIN_TRACK_GAIN=-3.21 dB",
                 "-metadata", "REPLAYGAIN_TRACK_PEAK=0.778990",
                 "-metadata", "REPLAYGAIN_ALBUM_GAIN=-3.21 dB",
                 "-metadata", "REPLAYGAIN_ALBUM_PEAK=0.778990"])
    sha, _ = canonical_f32_sha(p)
    add("flac-replaygain", "flac-replaygain.flac", "flac",
        {"container": "flac", "codec": "flac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": read_metadata(p), "artwork": None, "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "flac-minimal.flac")
    encode_flac(p, SIG_STEREO.format(dur=2), meta=False)
    sha, _ = canonical_f32_sha(p)
    add("flac-minimal", "flac-minimal.flac", "flac",
        {"container": "flac", "codec": "flac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata_absent_or_empty": True, "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "flac-truncated.flac")
    truncate(os.path.join(FIXTURES, "flac-16-44-stereo.flac"), p, 0.6)
    add("flac-truncated", "flac-truncated.flac", "flac",
        {"container": "flac", "codec": "flac", "pcm": {"mode": "consistency"},
         "artwork": None, "seek": "record", "eof": "error_or_eof"}, degraded=True)

    manifest = {
        "corpus_id": "stage-a-v1",
        "canonical_pcm_definition": "Float32 interleaved, source rate/layout, "
            "no resample/rematrix; sha256 over raw f32le bytes",
        "generator": {"tool": tool, "script": "corpus/tools/gen_corpus.py",
                      "regenerate": "python3 corpus/tools/gen_corpus.py"},
        "cases": cases,
    }
    out = os.path.join(ROOT, "corpus", "manifest", "stage-a.json")
    with open(out, "w") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")
    total = sum(c["fixture_bytes"] for c in cases)
    print(f"\nwrote {out}: {len(cases)} cases, {total/1024:.0f} KiB fixtures")


if __name__ == "__main__":
    main()
