#!/usr/bin/env python3
"""Generate the SongCore ABI v1 corpus: deterministic fixtures covering

  metadata (none / simple / album / UTF-8 / track-disc / full /
            ReplayGain / raw enumeration / container+stream conflict),
  artwork (MP3 APIC jpeg+multi, M4A covr, no-artwork),
  multi-audio-stream selection (M4A and MKV),
  negative corpus (empty / non-media / no-audio-stream / unsupported codec /
            reused truncated+corrupt cases).

All audio is synthesized from fixed lavfi expressions; images are generated
PNGs (no copyrighted material). The host ffmpeg is only a corpus *generator*.
Every fixture's bytes and the expectations the SongCore gates assert on it are
recorded in corpus/manifest/songcore.json.

Usage: python3 corpus/tools/gen_corpus_songcore.py [--out corpus]
"""
import argparse
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_corpus as base  # deterministic helpers (make_png, ...)

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIXTURES = os.path.join(ROOT, "corpus", "fixtures")
WORK = os.path.join(ROOT, "build", "corpus-work")
MANIFEST = os.path.join(ROOT, "corpus", "manifest", "songcore.json")

SIG_STEREO = "aevalsrc=0.55*sin(2*PI*440*t)|0.40*sin(2*PI*554.365*t):s=44100:d={dur}"
SIG_STEREO_48 = "aevalsrc=0.50*sin(2*PI*493.883*t)|0.42*sin(2*PI*698.456*t):s=48000:d={dur}"


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    if r.returncode != 0:
        raise SystemExit(f"command failed: {subprocess.list2cmdline(cmd)}\n{r.stderr[-2000:]}")


def png_small():
    path = os.path.join(WORK, "img-small.png")
    base.make_png(path, 16, 16)
    return path


def png_big():
    path = os.path.join(WORK, "img-big.png")
    base.make_png(path, 64, 64)
    return path


def gen_flac(out, sig, meta_args):
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", sig]
    cmd += meta_args
    cmd += ["-c:a", "flac", out]
    run(cmd)


def gen_mp3(out, sig, meta_args):
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", sig]
    cmd += meta_args
    cmd += ["-c:a", "libmp3lame", "-b:a", "128k", out]
    run(cmd)


def gen_m4a(out, sig, meta_args, audio_codec="aac"):
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", sig]
    cmd += meta_args
    cmd += ["-c:a", audio_codec, out]
    run(cmd)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="corpus")
    args = ap.parse_args()
    out_root = os.path.join(ROOT, args.out)
    fixtures = os.path.join(out_root, "fixtures")

    if os.path.isdir(WORK):
        shutil.rmtree(WORK)
    os.makedirs(WORK, exist_ok=True)

    cases = []

    def add(cid, fname, mode, expect, notes=""):
        path = os.path.join(fixtures, fname)
        if not os.path.isfile(path):
            raise SystemExit(f"missing fixture {path}")
        cases.append({
            "id": cid, "file": fname, "mode": mode,
            "fixture_sha256": sha256_file(path),
            "fixture_bytes": os.path.getsize(path),
            "expect": expect, "notes": notes,
        })

    # ---------------------------------------------------------------- metadata
    dur = 3

    gen_flac(os.path.join(fixtures, "metadata-none.flac"), SIG_STEREO.format(dur=dur),
             ["-map_metadata", "-1"])
    add("metadata-none", "metadata-none.flac", "record",
        {"container": "flac", "codec": "flac", "sample_rate": 44100,
         "channels": 2, "seek_family": "strict",
         "metadata": {"has_title": False, "has_artist": False,
                      "has_album": False, "has_album_artist": False,
                      "has_genre": False, "has_composer": False,
                      "has_date": False, "has_comment": False,
                      "has_track_number": False, "has_disc_number": False,
                      "has_track_gain": False, "has_album_gain": False},
         "artwork_count": 0},
        "no metadata at all")

    gen_flac(os.path.join(fixtures, "metadata-simple.flac"),
             SIG_STEREO.format(dur=dur),
             ["-metadata", "title=Simple Title",
              "-metadata", "artist=Simple Artist"])
    add("metadata-simple", "metadata-simple.flac", "record",
        {"container": "flac", "codec": "flac", "sample_rate": 44100,
         "channels": 2, "seek_family": "strict",
         "metadata": {"title": "Simple Title", "artist": "Simple Artist",
                      "has_title": True, "has_artist": True,
                      "has_album": False, "has_album_artist": False,
                      "has_track_number": False, "has_disc_number": False},
         "artwork_count": 0})

    gen_flac(os.path.join(fixtures, "metadata-album.flac"),
             SIG_STEREO.format(dur=dur),
             ["-metadata", "title=Album Track",
              "-metadata", "artist=Album Artist",
              "-metadata", "album=The Album",
              "-metadata", "album_artist=Album Artist"])
    add("metadata-album", "metadata-album.flac", "record",
        {"container": "flac", "codec": "flac", "seek_family": "strict",
         "metadata": {"title": "Album Track", "artist": "Album Artist",
                      "album": "The Album", "album_artist": "Album Artist",
                      "has_album": True, "has_album_artist": True},
         "artwork_count": 0})

    gen_flac(os.path.join(fixtures, "metadata-utf8.flac"),
             SIG_STEREO.format(dur=dur),
             ["-metadata", "title=千千·现代 青花瓷",
              "-metadata", "artist=测试 Ünïcode",
              "-metadata", "album=夜曲 ♪ 山"])
    add("metadata-utf8", "metadata-utf8.flac", "record",
        {"container": "flac", "codec": "flac", "seek_family": "strict",
         "metadata": {"title": "千千·现代 青花瓷",
                      "artist": "测试 Ünïcode",
                      "album": "夜曲 ♪ 山",
                      "has_title": True, "has_artist": True,
                      "has_album": True},
         "artwork_count": 0},
        "UTF-8 / non-ASCII metadata round-trips byte-exact")

    gen_flac(os.path.join(fixtures, "metadata-track.flac"),
             SIG_STEREO.format(dur=dur),
             ["-metadata", "title=Track Field",
              "-metadata", "track=3/12",
              "-metadata", "disc=1/2"])
    add("metadata-track", "metadata-track.flac", "record",
        {"container": "flac", "codec": "flac", "seek_family": "strict",
         "metadata": {"track_number": 3, "has_track_number": True,
                      "track_total": 12, "has_track_total": True,
                      "disc_number": 1, "has_disc_number": True,
                      "disc_total": 2, "has_disc_total": True},
         "artwork_count": 0})

    gen_flac(os.path.join(fixtures, "metadata-full.flac"),
             SIG_STEREO.format(dur=dur),
             ["-metadata", "title=Full Field",
              "-metadata", "artist=Full Artist",
              "-metadata", "album=Full Album",
              "-metadata", "album_artist=Full Album Artist",
              "-metadata", "genre=Instrumental",
              "-metadata", "composer=Full Composer",
              "-metadata", "date=1999",
              "-metadata", "comment=Full comment text",
              "-metadata", "track=7/9",
              "-metadata", "disc=2/2"])
    add("metadata-full", "metadata-full.flac", "record",
        {"container": "flac", "codec": "flac", "seek_family": "strict",
         "metadata": {"title": "Full Field", "artist": "Full Artist",
                      "album": "Full Album",
                      "album_artist": "Full Album Artist",
                      "genre": "Instrumental", "composer": "Full Composer",
                      "date": "1999", "comment": "Full comment text",
                      "track_number": 7, "track_total": 9,
                      "disc_number": 2, "disc_total": 2,
                      "has_title": True, "has_artist": True, "has_album": True,
                      "has_album_artist": True, "has_genre": True,
                      "has_composer": True, "has_date": True,
                      "has_comment": True, "has_track_number": True,
                      "has_track_total": True, "has_disc_number": True,
                      "has_disc_total": True},
         "artwork_count": 0})

    gen_flac(os.path.join(fixtures, "metadata-replaygain.flac"),
             SIG_STEREO.format(dur=dur),
             ["-metadata", "title=ReplayGain Case",
              "-metadata", "REPLAYGAIN_TRACK_GAIN=-6.02 dB",
              "-metadata", "REPLAYGAIN_TRACK_PEAK=0.969009",
              "-metadata", "REPLAYGAIN_ALBUM_GAIN=-8.10 dB",
              "-metadata", "REPLAYGAIN_ALBUM_PEAK=0.971311"])
    add("metadata-replaygain", "metadata-replaygain.flac", "record",
        {"container": "flac", "codec": "flac", "seek_family": "strict",
         "metadata": {"has_track_gain": True, "has_track_peak": True,
                      "has_album_gain": True, "has_album_peak": True,
                      "title": "ReplayGain Case"},
         "artwork_count": 0},
        "ReplayGain parsed into microbels / 100000-full-scale")

    # MP3 raw-tag enumeration: many distinct TXXX keys stay visible raw.
    gen_mp3(os.path.join(fixtures, "metadata-mp3.mp3"),
            SIG_STEREO.format(dur=dur),
            ["-metadata", "title=Raw MP3",
             "-metadata", "artist=Raw Artist",
             "-metadata", "CUSTOM_XKEY=alpha",
             "-metadata", "CUSTOM_YKEY=beta",
             "-metadata", "CUSTOM_ZKEY=gamma"])
    add("metadata-mp3", "metadata-mp3.mp3", "record",
        {"container": "mp3", "codec": "mp3", "seek_family": "lapped",
         "metadata": {"title": "Raw MP3", "artist": "Raw Artist",
                      "has_title": True, "has_artist": True},
         "raw_keys": ["CUSTOM_XKEY", "CUSTOM_YKEY", "CUSTOM_ZKEY"],
         "artwork_count": 0},
        "raw metadata enumeration exposes unknown tags without ABI change")

    # Container vs stream metadata conflict (Matroska segment vs track tags).
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur),
           "-c:a", "flac",
           "-metadata", "title=ContainerTitle",
           "-metadata:s:a:0", "title=StreamTitle",
           "-metadata:s:a:0", "genre=StreamGenre",
           os.path.join(fixtures, "metadata-conflict.mka")]
    run(cmd)
    add("metadata-conflict", "metadata-conflict.mka", "record",
        {"container": "matroska", "codec": "flac", "seek_family": "strict",
         "metadata": {"title": "StreamTitle", "genre": "StreamGenre",
                      "has_title": True, "has_genre": True},
         "artwork_count": 0},
        "canonical precedence: selected stream overrides container")

    # ----------------------------------------------------------------- artwork
    small = png_small()
    big = png_big()

    # MP3 APIC with a JPEG image (MIME image/jpeg path).
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur), "-i", small,
           "-map", "0:a", "-map", "1:v", "-c:a", "libmp3lame", "-b:a", "128k",
           "-c:v", "png", "-disposition:v", "attached_pic",
           os.path.join(fixtures, "artwork-mp3-png.mp3")]
    run(cmd)
    jpeg = os.path.join(WORK, "img.jpg")
    run(["ffmpeg", "-v", "error", "-y", "-i", small, "-q:v", "2", jpeg])
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur), "-i", jpeg,
           "-map", "0:a", "-map", "1:v", "-c:a", "libmp3lame", "-b:a", "128k",
           "-c:v", "mjpeg", "-disposition:v", "attached_pic",
           os.path.join(fixtures, "artwork-mp3-jpeg.mp3")]
    run(cmd)
    add("artwork-mp3-jpeg", "artwork-mp3-jpeg.mp3", "record",
        {"container": "mp3", "codec": "mp3", "seek_family": "lapped",
         "artwork_count": 1,
         "artwork": [{"mime": "image/jpeg", "role": 1, "front": True}]},
        "MP3 APIC JPEG: single artwork is the front cover")

    # MP3 with two APIC pictures (multi-artwork policy).
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur), "-i", small,
           "-i", big, "-map", "0:a", "-map", "1:v", "-map", "2:v",
           "-c:a", "libmp3lame", "-b:a", "128k", "-c:v", "png",
           "-disposition:v:0", "attached_pic",
           "-disposition:v:1", "attached_pic",
           os.path.join(fixtures, "artwork-mp3-multi.mp3")]
    run(cmd)
    multi_sha = sha256_file(os.path.join(fixtures, "artwork-mp3-multi.mp3"))
    add("artwork-mp3-multi", "artwork-mp3-multi.mp3", "record",
        {"container": "mp3", "codec": "mp3", "seek_family": "lapped",
         "artwork_count": 2,
         "artwork": [{"mime": "image/png", "front": False},
                     {"mime": "image/png", "front": False}]},
        f"multiple artwork items; only explicit front covers are flagged; fixture_sha256={multi_sha}")

    # M4A covr.
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur), "-i", small,
           "-map", "0:a", "-map", "1:v", "-c:a", "aac", "-b:a", "96k",
           "-c:v", "png", "-disposition:v", "attached_pic",
           os.path.join(fixtures, "artwork-m4a.m4a")]
    run(cmd)
    add("artwork-m4a", "artwork-m4a.m4a", "record",
        {"container": "mov", "codec": "aac", "seek_family": "lapped",
         "artwork_count": 1,
         "artwork": [{"mime": "image/png", "front": True}]})

    # ----------------------------------------------------- multi-audio-stream
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur),
           "-f", "lavfi", "-i", SIG_STEREO_48.format(dur=dur),
           "-map", "0:a", "-map", "1:a",
           "-c:a:0", "aac", "-b:a:0", "64k",
           "-c:a:1", "aac", "-b:a:1", "96k",
           "-metadata:s:a:0", "title=Stream Zero",
           "-metadata:s:a:1", "title=Stream One",
           "-disposition:a:1", "default",
           os.path.join(fixtures, "multiaudio-default-stream.m4a")]
    run(cmd)
    add("multiaudio-default-stream", "multiaudio-default-stream.m4a", "record",
        {"container": "mov", "codec": "aac", "seek_family": "lapped",
         "audio_stream_count": 2,
         "default_selected": 1,
         "streams": [
             {"sample_rate": 44100, "is_default": 0, "codec": "aac"},
             {"sample_rate": 48000, "is_default": 1, "codec": "aac"},
         ],
         "select": {"index": 0, "sample_rate": 44100,
                    "metadata_title": "Stream Zero"},
         "artwork_count": 0},
        "two AAC streams (44.1k/48k); default disposition selects stream 1")

    # MKV with two flac audio streams + per-track tags + one default.
    cmd = ["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
           "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur),
           "-f", "lavfi", "-i", SIG_STEREO_48.format(dur=dur),
           "-map", "0:a", "-map", "1:a", "-c:a", "flac",
           "-metadata:s:a:0", "title=Mkv Zero",
           "-metadata:s:a:1", "title=Mkv One",
           "-disposition:a:1", "default",
           os.path.join(fixtures, "multiaudio-default-stream.mka")]
    run(cmd)
    add("multiaudio-default-stream-mka", "multiaudio-default-stream.mka", "record",
        {"container": "matroska", "codec": "flac", "seek_family": "strict",
         "audio_stream_count": 2,
         "default_selected": 1,
         "streams": [
             {"sample_rate": 44100, "is_default": 0, "codec": "flac"},
             {"sample_rate": 48000, "is_default": 1, "codec": "flac"},
         ],
         "select": {"index": 0, "sample_rate": 44100,
                    "metadata_title": "Mkv Zero"},
         "artwork_count": 0},
        "two flac streams in Matroska; selection swaps decoder + metadata")

    # ------------------------------------------------------------- negatives
    with open(os.path.join(fixtures, "invalid-empty.bin"), "wb"):
        pass
    add("invalid-empty", "invalid-empty.bin", "neg",
        {"open": "UNSUPPORTED_CONTAINER"})

    with open(os.path.join(fixtures, "invalid-text.bin"), "w") as f:
        f.write("this is not a media file\n")
    add("invalid-text", "invalid-text.bin", "neg",
        {"open": "UNSUPPORTED_CONTAINER"})

    shutil.copy(small, os.path.join(fixtures, "invalid-png.png"))
    add("invalid-png", "invalid-png.png", "neg",
        {"open": "UNSUPPORTED_CONTAINER"})

    run(["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
         "-f", "lavfi", "-i", "testsrc=duration=1:size=64x64:rate=10",
         "-c:v", "mpeg4",
         os.path.join(fixtures, "invalid-videoonly.m4a")])
    add("invalid-videoonly", "invalid-videoonly.m4a", "neg",
        {"open": "OK", "probe": "NO_AUDIO_STREAM"},
        "recognized container with no audio stream")

    run(["ffmpeg", "-v", "error", "-y", "-fflags", "+bitexact",
         "-f", "lavfi", "-i", SIG_STEREO.format(dur=dur),
         "-c:a", "ac3",
         os.path.join(fixtures, "invalid-ac3.m4a")])
    add("invalid-ac3", "invalid-ac3.m4a", "neg",
        {"open": "OK", "probe": "UNSUPPORTED_CODEC"},
        "M4A with AC-3: mov demuxer present, ac3 decoder absent in closure")

    # iofail: host I/O failure mid-read must be a typed IO error.
    add("iofail", "flac-16-44-stereo.flac", "iofail",
        {"open": "OK", "probe": "OK", "read": "IO"})

    manifest = {
        "corpus_id": "songcore-v1",
        "generated_by": "corpus/tools/gen_corpus_songcore.py",
        "cases": cases,
    }
    with open(MANIFEST, "w") as f:
        json.dump(manifest, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(f"wrote {len(cases)} cases to {MANIFEST}")


if __name__ == "__main__":
    main()
