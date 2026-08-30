#!/usr/bin/env python3
"""Generate the Common Formats corpus: deterministic synthetic AAC/ALAC/WAV/
Vorbis/Opus fixtures (issue #8).

Same contract as the Stage A corpus (corpus/tools/gen_corpus.py):

- all audio synthesized from fixed lavfi expressions / fixed math (no
  copyrighted material); the host ffmpeg is only a corpus *generator*;
- every fixture's bytes, structural expectations and PCM checksums are
  recorded in corpus/manifest/common-formats.json;
- lossless / PCM fixtures (ALAC, WAV) pin a strict canonical
  Float32-interleaved sha256; lossy fixtures (AAC, Vorbis, Opus) record
  reference checksums as information only and rely on structural
  expectations + cross-stage PCM consistency;
- every case carries a `capability` tag (aac/alac/wav/vorbis/opus) so each
  capability-ladder stage can gate on exactly its own applicable corpus.

Seek semantics are NOT guessed: new cases are written with seek="strict"
PROVISIONAL and tools/common_calibrate.py later pins the observed contract
(strict iff oracle suffix-matches at 25/50/75%) with recorded evidence.

Usage: python3 corpus/tools/gen_corpus_common.py [--out corpus]
"""
import argparse
import json
import os
import shutil
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_corpus as base  # reuse deterministic helpers (make_png, probe_artwork, ...)

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIXTURES = os.path.join(ROOT, "corpus", "fixtures")
WORK = os.path.join(ROOT, "build", "corpus-work-common")
MANIFEST = os.path.join(ROOT, "corpus", "manifest", "common-formats.json")

PNG_SMALL = "cover-small.png"
PNG_BIG = "cover-large.png"

# deterministic signal snippets (same family as the stage-a corpus)
SIG_STEREO_44 = ("aevalsrc=0.55*sin(2*PI*440*t)|0.40*sin(2*PI*554.365*t)"
                 ":s=44100:d={dur}")
SIG_MONO_44 = "aevalsrc=0.60*sin(2*PI*330*t):s=44100:d={dur}"
SIG_STEREO_48 = ("aevalsrc=0.50*sin(2*PI*493.883*t)|0.42*sin(2*PI*698.456*t)"
                 ":s=48000:d={dur}")
SIG_STEREO_96 = ("aevalsrc=0.45*sin(2*PI*523.251*t)|0.38*sin(2*PI*783.991*t)"
                 ":s=96000:d={dur}")

META_MAIN = ["-metadata", "title=千千通用测试",
             "-metadata", "artist=Qianqian Lab",
             "-metadata", "album=Common Formats"]


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"command failed: {subprocess.list2cmdline(cmd)}\n{r.stderr[-2000:]}")


def encode(out, sig, enc_args, extra_inputs=None, meta=META_MAIN,
           muxer=None, mapping_audio_only=True):
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact"]
    cmd += ["-f", "lavfi", "-i", sig]
    if extra_inputs:
        for inp in extra_inputs:
            cmd += ["-i", inp]
    if mapping_audio_only:
        cmd += ["-map", "0:a"]
        if extra_inputs:
            cmd += ["-map", "1:v", "-c:v", "png",
                    "-disposition:v", "attached_pic"]
    if meta is None:
        cmd += ["-map_metadata", "-1"]
    cmd += enc_args
    if muxer:
        cmd += ["-f", muxer]
    cmd += [out]
    run(cmd)


def make_wav(out, sig, pcm_codec, meta=META_MAIN):
    """PCM WAV with explicit sample format; wav muxer writes fmt+data (+LIST)."""
    encode(out, sig, ["-c:a", pcm_codec, "-bitexact"], meta=meta, muxer="wav")


def truncate(src, dst, frac):
    data = open(src, "rb").read()
    open(dst, "wb").write(data[: int(len(data) * frac)])


def corrupt_byte(src, dst, offset):
    data = bytearray(open(src, "rb").read())
    data[offset] ^= 0xFF
    open(dst, "wb").write(bytes(data))


def wav_insert_odd_list(src, dst):
    """Insert an odd-sized (9-byte payload) LIST chunk + pad byte after 'fmt '.
    Exercises odd-chunk/padding handling in the wav demuxer; result stays a
    fully valid RIFF file."""
    data = open(src, "rb").read()
    riff_size = struct.unpack("<I", data[4:8])[0]
    # locate the end of the fmt chunk: 'fmt ' header at 12, size at 16
    assert data[12:16] == b"fmt "
    fmt_size = struct.unpack("<I", data[16:20])[0]
    fmt_end = 20 + fmt_size + (fmt_size & 1)
    payload = b"INFO" + b"IAS9\x00\x00\x00\x00\x01"  # 9 bytes -> odd
    assert len(payload) % 2 == 1
    chunk = b"LIST" + struct.pack("<I", len(payload)) + payload + b"\x00"
    out = data[:fmt_end] + chunk + data[fmt_end:]
    out = out[:4] + struct.pack("<I", riff_size + len(chunk)) + out[8:]
    open(dst, "wb").write(out)


def wav_corrupt_fmt(src, dst):
    """Destroy the fmt chunk payload (keep RIFF/fmt headers) -> open must fail."""
    data = bytearray(open(src, "rb").read())
    assert data[12:16] == b"fmt "
    for i in range(20, 28):
        data[i] ^= 0xFF
    open(dst, "wb").write(bytes(data))


def ogg_corrupt_header(src, dst):
    """Flip bytes inside the codec identification header page (page 0 body),
    so demux/decode setup fails in a typed way rather than truncation."""
    data = bytearray(open(src, "rb").read())
    # first page: 27-byte segment header + segment table; corrupt mid-page
    off = 27 + 1 + 8
    for i in range(off, off + 8):
        data[i] ^= 0xA5
    open(dst, "wb").write(bytes(data))


def nominal_samples(rate, dur):
    return int(round(rate * dur))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="corpus")
    args = ap.parse_args()

    os.makedirs(FIXTURES, exist_ok=True)
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    tool = base.ffmpeg_version()
    cases = []
    work = lambda name: os.path.join(WORK, name)

    png_small = work(PNG_SMALL)
    png_big = work(PNG_BIG)
    base.make_png(png_small, 64, 64)
    base.make_png(png_big, 256, 256)

    def add(case_id, fname, capability, fmt, expect, throughput=False,
            degraded=False, with_stream_info=True, notes=None):
        p = os.path.join(FIXTURES, fname)
        if with_stream_info:
            n_samples, st = base.sample_count(p)
            expect.setdefault("sample_rate", int(st["sample_rate"]))
            expect.setdefault("channels", int(st["channels"]))
            expect.setdefault("samples", n_samples)
            expect.setdefault("duration_us", round(float(st.get("duration", 0)) * 1e6))
        else:
            n_samples = 0
        case = {
            "id": case_id,
            "capability": capability,
            "file": fname,
            "format": fmt,
            "fixture_sha256": base.sha256_file(p),
            "fixture_bytes": os.path.getsize(p),
            "throughput": throughput,
            "degraded": degraded,
            "expect": expect,
        }
        if notes:
            case["notes"] = notes
        cases.append(case)
        print(f"  [{capability}] {case_id}: {os.path.getsize(p)} bytes, {n_samples} samples")

    # ------------------------------------------------------------------
    print("AAC / M4A fixtures:")
    p = os.path.join(FIXTURES, "aac-lc-44-stereo.m4a")
    encode(p, SIG_STEREO_44.format(dur=12), ["-c:a", "aac", "-b:a", "128k"],
           meta=META_MAIN, muxer="ipod")
    add("aac-lc-44-stereo", "aac-lc-44-stereo.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"},
        throughput=True,
        notes="main AAC-LC CBR case; also the AAC throughput sample")

    p = os.path.join(FIXTURES, "aac-lc-48-stereo.m4a")
    encode(p, SIG_STEREO_48.format(dur=4), ["-c:a", "aac", "-b:a", "128k"],
           meta=META_MAIN, muxer="ipod")
    add("aac-lc-48-stereo", "aac-lc-48-stereo.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "aac-lc-44-mono.m4a")
    encode(p, SIG_MONO_44.format(dur=4),
           ["-c:a", "aac", "-b:a", "96k", "-ac", "1"],
           meta=META_MAIN, muxer="ipod")
    add("aac-lc-44-mono", "aac-lc-44-mono.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "aac-vbr.m4a")
    encode(p, SIG_STEREO_44.format(dur=4), ["-c:a", "aac", "-q:a", "2"],
           meta=META_MAIN, muxer="ipod")
    add("aac-vbr", "aac-vbr.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"},
        notes="native aac encoder VBR mode (-q:a); avg bitrate recorded by generator")

    p = os.path.join(FIXTURES, "aac-adts-44-stereo.aac")
    encode(p, SIG_STEREO_44.format(dur=4), ["-c:a", "aac", "-b:a", "128k"],
           meta=None, muxer="adts")
    add("aac-adts-44-stereo", "aac-adts-44-stereo.aac", "aac", "aac",
        {"container": "aac", "codec": "aac", "pcm": {"mode": "consistency"},
         "metadata_absent_or_empty": True, "artwork": None,
         "seek": "strict", "eof": "clean"},
        notes="raw ADTS stream; no container metadata by construction")

    p = os.path.join(FIXTURES, "aac-artwork.m4a")
    encode(p, SIG_STEREO_44.format(dur=4), ["-c:a", "aac", "-b:a", "128k"],
           extra_inputs=[png_small], meta=META_MAIN, muxer="ipod")
    add("aac-artwork", "aac-artwork.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": base.probe_artwork(p), "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "aac-short.m4a")
    encode(p, SIG_STEREO_44.format(dur=0.3), ["-c:a", "aac", "-b:a", "128k"],
           meta=META_MAIN, muxer="ipod")
    add("aac-short", "aac-short.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"})

    # Truncated M4A cases need the moov atom UP FRONT (faststart): a plain
    # m4a keeps moov at EOF, so truncation would destroy the container header
    # instead of cutting the audio stream mid-way. Same signal/codec args as
    # the base case, so packets are identical up to the cut.
    def faststart_src(path, sig, codec_args, work_name):
        tmp = work(work_name)
        encode(tmp, sig, codec_args, meta=META_MAIN, muxer="ipod")
        run(["ffmpeg", "-v", "error", "-y", "-i", tmp, "-c", "copy",
             "-movflags", "+faststart", path])
        os.remove(tmp)

    p = os.path.join(FIXTURES, "aac-truncated.m4a")
    faststart_src(p, SIG_STEREO_44.format(dur=12), ["-c:a", "aac", "-b:a", "128k"],
                  "aac-trunc-src.m4a")
    truncate(p, p + ".cut", 0.6)
    os.replace(p + ".cut", p)
    full = next(c for c in cases if c["id"] == "aac-lc-44-stereo")
    add("aac-truncated", "aac-truncated.m4a", "aac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "aac",
         "pcm": {"mode": "consistency"},
         "min_samples": int(full["expect"]["samples"] * 0.5),
         "artwork": None, "seek": "record", "eof": "error_or_eof"},
        degraded=True)

    truncate(os.path.join(FIXTURES, "aac-adts-44-stereo.aac"),
             os.path.join(FIXTURES, "aac-malformed-header.aac"), 0.015)
    add("aac-malformed-header", "aac-malformed-header.aac", "aac", "aac",
        {"container": "aac", "codec": "aac", "pcm": {"mode": "consistency"},
         "artwork": None, "seek": "none", "eof": "error_or_eof",
         "probe_may_fail": True}, degraded=True, with_stream_info=False)

    # ------------------------------------------------------------------
    print("ALAC / M4A fixtures:")
    p = os.path.join(FIXTURES, "alac-16-44-stereo.m4a")
    encode(p, SIG_STEREO_44.format(dur=4), ["-c:a", "alac"],
           meta=META_MAIN, muxer="ipod")
    sha, _ = base.canonical_f32_sha(p)
    add("alac-16-44-stereo", "alac-16-44-stereo.m4a", "alac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "alac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "alac-24-96-stereo.m4a")
    encode(p, SIG_STEREO_96.format(dur=2), ["-c:a", "alac", "-sample_fmt", "s32p"],
           meta=META_MAIN, muxer="ipod")
    sha, _ = base.canonical_f32_sha(p)
    add("alac-24-96-stereo", "alac-24-96-stereo.m4a", "alac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "alac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"},
        notes="24-bit via s32 input; observed bits recorded by SongCore probe")

    p = os.path.join(FIXTURES, "alac-16-44-mono.m4a")
    encode(p, SIG_MONO_44.format(dur=2), ["-c:a", "alac", "-ac", "1"],
           meta=META_MAIN, muxer="ipod")
    sha, _ = base.canonical_f32_sha(p)
    add("alac-16-44-mono", "alac-16-44-mono.m4a", "alac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "alac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "alac-artwork.m4a")
    encode(p, SIG_STEREO_44.format(dur=2), ["-c:a", "alac"],
           extra_inputs=[png_big], meta=META_MAIN, muxer="ipod")
    sha, _ = base.canonical_f32_sha(p)
    add("alac-artwork", "alac-artwork.m4a", "alac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "alac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": base.probe_artwork(p),
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "alac-long.m4a")
    encode(p, SIG_STEREO_44.format(dur=6), ["-c:a", "alac"],
           meta=META_MAIN, muxer="ipod")
    sha, _ = base.canonical_f32_sha(p)
    add("alac-long", "alac-long.m4a", "alac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "alac",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "alac-truncated.m4a")
    faststart_src(p, SIG_STEREO_44.format(dur=4), ["-c:a", "alac"],
                  "alac-trunc-src.m4a")
    truncate(p, p + ".cut", 0.6)
    os.replace(p + ".cut", p)
    full = next(c for c in cases if c["id"] == "alac-16-44-stereo")
    add("alac-truncated", "alac-truncated.m4a", "alac", "mov,mp4,m4a,3gp,3g2,mj2",
        {"container": "mov,mp4,m4a,3gp,3g2,mj2", "codec": "alac",
         "pcm": {"mode": "consistency"},
         "min_samples": int(full["expect"]["samples"] * 0.5),
         "artwork": None, "seek": "record", "eof": "error_or_eof"},
        degraded=True)

    # ------------------------------------------------------------------
    print("PCM WAV fixtures:")
    p = os.path.join(FIXTURES, "wav-u8-44-mono.wav")
    make_wav(p, SIG_MONO_44.format(dur=2), "pcm_u8")
    sha, _ = base.canonical_f32_sha(p)
    add("wav-u8-44-mono", "wav-u8-44-mono.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_u8",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "wav-s16le-44-stereo.wav")
    make_wav(p, SIG_STEREO_44.format(dur=6), "pcm_s16le")
    sha, _ = base.canonical_f32_sha(p)
    add("wav-s16le-44-stereo", "wav-s16le-44-stereo.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_s16le",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "wav-s24le-96-stereo.wav")
    make_wav(p, SIG_STEREO_96.format(dur=1.5), "pcm_s24le")
    sha, _ = base.canonical_f32_sha(p)
    add("wav-s24le-96-stereo", "wav-s24le-96-stereo.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_s24le",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "wav-s32le-48-stereo.wav")
    make_wav(p, SIG_STEREO_48.format(dur=2), "pcm_s32le")
    sha, _ = base.canonical_f32_sha(p)
    add("wav-s32le-48-stereo", "wav-s32le-48-stereo.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_s32le",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "wav-f32le-44-stereo.wav")
    make_wav(p, SIG_STEREO_44.format(dur=2), "pcm_f32le")
    sha, _ = base.canonical_f32_sha(p)
    add("wav-f32le-44-stereo", "wav-f32le-44-stereo.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_f32le",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "wav-f64le-44-mono.wav")
    make_wav(p, SIG_MONO_44.format(dur=2), "pcm_f64le")
    sha, _ = base.canonical_f32_sha(p)
    add("wav-f64le-44-mono", "wav-f64le-44-mono.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_f64le",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata": base.read_metadata(p), "artwork": None,
         "seek": "strict", "eof": "clean"})

    p = os.path.join(FIXTURES, "wav-odd-chunk.wav")
    wav_insert_odd_list(os.path.join(FIXTURES, "wav-s16le-44-stereo.wav"), p)
    sha, _ = base.canonical_f32_sha(p)
    add("wav-odd-chunk", "wav-odd-chunk.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_s16le",
         "pcm": {"mode": "strict", "canonical_f32_sha256": sha},
         "metadata_absent_or_empty": True, "artwork": None,
         "seek": "strict", "eof": "clean"},
        notes="valid RIFF with an odd-sized LIST chunk + pad byte after 'fmt '")

    p = os.path.join(FIXTURES, "wav-truncated.wav")
    truncate(os.path.join(FIXTURES, "wav-s16le-44-stereo.wav"), p, 0.6)
    full = next(c for c in cases if c["id"] == "wav-s16le-44-stereo")
    add("wav-truncated", "wav-truncated.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_s16le", "pcm": {"mode": "consistency"},
         "min_samples": int(full["expect"]["samples"] * 0.5),
         "artwork": None, "seek": "record", "eof": "error_or_eof"},
        degraded=True)

    p = os.path.join(FIXTURES, "wav-malformed-header.wav")
    wav_corrupt_fmt(os.path.join(FIXTURES, "wav-s16le-44-stereo.wav"), p)
    add("wav-malformed-header", "wav-malformed-header.wav", "wav", "wav",
        {"container": "wav", "codec": "pcm_s16le", "pcm": {"mode": "consistency"},
         "artwork": None, "seek": "none", "eof": "error_or_eof",
         "probe_may_fail": True}, degraded=True, with_stream_info=False)

    # ------------------------------------------------------------------
    print("Ogg Vorbis fixtures:")
    p = os.path.join(FIXTURES, "vorbis-44-stereo.ogg")
    encode(p, SIG_STEREO_44.format(dur=12), ["-c:a", "libvorbis", "-q:a", "4"],
           meta=META_MAIN, muxer="ogg")
    add("vorbis-44-stereo", "vorbis-44-stereo.ogg", "vorbis", "ogg",
        {"container": "ogg", "codec": "vorbis",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"},
        throughput=True)

    p = os.path.join(FIXTURES, "vorbis-truncated.ogg")
    truncate(os.path.join(FIXTURES, "vorbis-44-stereo.ogg"), p, 0.75)
    full = next(c for c in cases if c["id"] == "vorbis-44-stereo")
    add("vorbis-truncated", "vorbis-truncated.ogg", "vorbis", "ogg",
        {"container": "ogg", "codec": "vorbis", "pcm": {"mode": "consistency"},
         "min_samples": int(full["expect"]["samples"] * 0.5),
         "artwork": None, "seek": "record", "eof": "error_or_eof"},
        degraded=True)

    # ------------------------------------------------------------------
    print("Ogg Opus fixtures:")
    p = os.path.join(FIXTURES, "opus-48-stereo.opus")
    encode(p, SIG_STEREO_48.format(dur=12), ["-c:a", "libopus", "-b:a", "64k"],
           meta=META_MAIN, muxer="opus")
    add("opus-48-stereo", "opus-48-stereo.opus", "opus", "ogg",
        {"container": "ogg", "codec": "opus",
         "pcm": {"mode": "consistency"}, "metadata": base.read_metadata(p),
         "artwork": None, "seek": "strict", "eof": "clean"},
        throughput=True,
        notes="preskip/end-trim correctness asserted below against nominal duration")

    p = os.path.join(FIXTURES, "opus-truncated.opus")
    truncate(os.path.join(FIXTURES, "opus-48-stereo.opus"), p, 0.75)
    full = next(c for c in cases if c["id"] == "opus-48-stereo")
    add("opus-truncated", "opus-truncated.opus", "opus", "ogg",
        {"container": "ogg", "codec": "opus", "pcm": {"mode": "consistency"},
         "min_samples": int(full["expect"]["samples"] * 0.5),
         "artwork": None, "seek": "record", "eof": "error_or_eof"},
        degraded=True)

    p = os.path.join(FIXTURES, "opus-malformed-header.opus")
    ogg_corrupt_header(os.path.join(FIXTURES, "opus-48-stereo.opus"), p)
    add("opus-malformed-header", "opus-malformed-header.opus", "opus", "ogg",
        {"container": "ogg", "codec": "opus", "pcm": {"mode": "consistency"},
         "artwork": None, "seek": "none", "eof": "error_or_eof",
         "probe_may_fail": True}, degraded=True, with_stream_info=False)

    # ------------------------------------------------------------------
    # independent sanity: decoded sample counts vs nominal encode durations
    # (catches codec-delay/preskip/end-trim accounting drift at pin time)
    nominal = {
        "aac-lc-44-stereo": (44100, 12), "aac-lc-48-stereo": (48000, 4),
        "aac-lc-44-mono": (44100, 4), "aac-vbr": (44100, 4),
        "aac-adts-44-stereo": (44100, 4), "aac-artwork": (44100, 4),
        "aac-short": (44100, 0.3),
        "alac-16-44-stereo": (44100, 4), "alac-24-96-stereo": (96000, 2),
        "alac-16-44-mono": (44100, 2), "alac-artwork": (44100, 2),
        "alac-long": (44100, 6),
        "wav-u8-44-mono": (44100, 2), "wav-s16le-44-stereo": (44100, 6),
        "wav-s24le-96-stereo": (96000, 1.5), "wav-s32le-48-stereo": (48000, 2),
        "wav-f32le-44-stereo": (44100, 2), "wav-f64le-44-mono": (44100, 2),
        "wav-odd-chunk": (44100, 6),
        "vorbis-44-stereo": (44100, 12), "opus-48-stereo": (48000, 12),
    }
    for c in cases:
        if c["id"] not in nominal or c["degraded"]:
            continue
        rate, dur = nominal[c["id"]]
        want = nominal_samples(rate, dur)
        got = c["expect"]["samples"]
        tol = 2 * 1152 if c["capability"] != "opus" else 2 * 960
        if abs(got - want) > tol:
            raise SystemExit(
                f"{c['id']}: decoded {got} samples vs nominal {want} "
                f"(tol {tol}) — codec delay/end-trim drift?")
        c["notes"] = (c.get("notes", "") +
                      f" [sample-count vs nominal {want}: delta {got - want}]").strip()

    manifest = {
        "corpus_id": "common-formats-v1",
        "canonical_pcm_definition": "Float32 interleaved, source rate/layout, "
            "no resample/rematrix; sha256 over raw f32le bytes",
        "generator": {"tool": tool, "script": "corpus/tools/gen_corpus_common.py",
                      "regenerate": "python3 corpus/tools/gen_corpus_common.py"},
        "cases": cases,
    }
    with open(MANIFEST, "w") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")
    total = sum(c["fixture_bytes"] for c in cases)
    print(f"\nwrote {MANIFEST}: {len(cases)} cases, {total/1024:.0f} KiB fixtures")


if __name__ == "__main__":
    main()
