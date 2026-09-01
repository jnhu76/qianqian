#!/usr/bin/env python3
"""songcore_ffi_smoke.py — external-consumer acceptance gate for SongCore ABI v1.

Drives the shipped shared library exactly the way an outside integrator
would: Python ctypes -> libsongcore.so / libsongcore.dylib / songcore.dll ->
the statically embedded FFmpeg closure inside it. No Qianqian test binary
and no test-only C code participates; the only mirror of the ABI is this
file's ctypes declarations of include/songcore.h (15 frozen symbols, no
FFmpeg types).

Default run (decode gate) needs ONLY the Python standard library:

    python3 tools/songcore_ffi_smoke.py song.flac

``--play`` adds audible acceptance through a thin test-only PortAudio sink
(sounddevice). PCM is handed to the sink at the source rate/layout —
SongCore never resamples for the caller and this tool must not compensate
in Python. ``--seconds`` caps decode/playback per song (default 3 s).

Exit codes: 0 all songs PASS, 1 smoke failure, 2 environment/usage error.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
import platform
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SONGCORE_ABI_VERSION = 1

# Cross-backend PCM comparison window: the first N frames (bytes, f32
# interleaved) of the decode window, dumped when --pcm-dump-dir is given.
# Never committed — consumed in-place by the consistency gate.
PCM_DUMP_BYTES = 8192 * 2 * 4  # 8192 stereo frames

SONG_OK = 0
SONG_EOF = 1
SONG_ERR_INVALID_ARGUMENT = 100

STATUS_NAMES = {
    0: "SONG_OK", 1: "SONG_EOF",
    100: "SONG_ERR_INVALID_ARGUMENT", 101: "SONG_ERR_STATE",
    102: "SONG_ERR_NOT_OPEN", 103: "SONG_ERR_IO",
    104: "SONG_ERR_UNSUPPORTED_CONTAINER", 105: "SONG_ERR_NO_AUDIO_STREAM",
    106: "SONG_ERR_UNSUPPORTED_CODEC", 107: "SONG_ERR_CORRUPT_DATA",
    108: "SONG_ERR_DECODE_ERROR", 109: "SONG_ERR_SEEK_UNSUPPORTED",
    110: "SONG_ERR_SEEK_ERROR", 111: "SONG_ERR_STREAM_CHANGE",
    112: "SONG_ERR_OUT_OF_MEMORY", 113: "SONG_ERR_INTERNAL_ERROR",
}


def status_name(code: int) -> str:
    return STATUS_NAMES.get(code, f"SONG_STATUS_{code}")


# ---------------------------------------------------------------------------
# ctypes mirror of include/songcore.h (ABI v1 — field order is the contract)
# ---------------------------------------------------------------------------

READ_FN = ctypes.CFUNCTYPE(
    ctypes.c_int64, ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t)
SEEK_FN = ctypes.CFUNCTYPE(ctypes.c_int64, ctypes.c_void_p, ctypes.c_int64)
SIZE_FN = ctypes.CFUNCTYPE(ctypes.c_int64, ctypes.c_void_p)


class SongIo(ctypes.Structure):
    _fields_ = [
        ("userdata", ctypes.c_void_p),
        ("read", READ_FN),
        ("seek", SEEK_FN),
        ("size", SIZE_FN),
    ]


class SongError(ctypes.Structure):
    _fields_ = [
        ("message", ctypes.c_void_p),        # borrowed pointer, NOT c_char_p:
        ("message_len", ctypes.c_uint32),    # the ABI is pointer+length
        ("native_code", ctypes.c_int32),
        ("reserved", ctypes.c_uint32),
    ]


class SongInfo(ctypes.Structure):
    _fields_ = [
        ("sample_rate", ctypes.c_int32),
        ("channels", ctypes.c_int32),
        ("channel_mask", ctypes.c_uint64),
        ("duration_us", ctypes.c_int64),
        ("bits_per_sample", ctypes.c_int32),
        ("codec", ctypes.c_char * 32),
        ("container", ctypes.c_char * 32),
        ("selected_audio_index", ctypes.c_uint32),
        ("audio_stream_count", ctypes.c_uint32),
        ("flags", ctypes.c_uint32),
        ("reserved", ctypes.c_uint32 * 3),
    ]


class SongStreamInfo(ctypes.Structure):
    _fields_ = [
        ("audio_index", ctypes.c_uint32),
        ("stream_index", ctypes.c_uint32),
        ("sample_rate", ctypes.c_int32),
        ("channels", ctypes.c_int32),
        ("channel_mask", ctypes.c_uint64),
        ("duration_us", ctypes.c_int64),
        ("bits_per_sample", ctypes.c_int32),
        ("codec", ctypes.c_char * 32),
        ("is_default", ctypes.c_uint32),
        ("reserved", ctypes.c_uint32 * 3),
    ]


class SongMetadata(ctypes.Structure):
    # Every string is a borrowed (pointer, length) pair — modeled as c_void_p
    # so ctypes never auto-converts or NUL-terminates. Read with utf8_view().
    _fields_ = [
        ("title", ctypes.c_void_p), ("title_len", ctypes.c_uint32), ("has_title", ctypes.c_uint32),
        ("artist", ctypes.c_void_p), ("artist_len", ctypes.c_uint32), ("has_artist", ctypes.c_uint32),
        ("album", ctypes.c_void_p), ("album_len", ctypes.c_uint32), ("has_album", ctypes.c_uint32),
        ("album_artist", ctypes.c_void_p), ("album_artist_len", ctypes.c_uint32), ("has_album_artist", ctypes.c_uint32),
        ("genre", ctypes.c_void_p), ("genre_len", ctypes.c_uint32), ("has_genre", ctypes.c_uint32),
        ("composer", ctypes.c_void_p), ("composer_len", ctypes.c_uint32), ("has_composer", ctypes.c_uint32),
        ("date", ctypes.c_void_p), ("date_len", ctypes.c_uint32), ("has_date", ctypes.c_uint32),
        ("comment", ctypes.c_void_p), ("comment_len", ctypes.c_uint32), ("has_comment", ctypes.c_uint32),
        ("track_number", ctypes.c_int32), ("has_track_number", ctypes.c_uint32),
        ("track_total", ctypes.c_int32), ("has_track_total", ctypes.c_uint32),
        ("disc_number", ctypes.c_int32), ("has_disc_number", ctypes.c_uint32),
        ("disc_total", ctypes.c_int32), ("has_disc_total", ctypes.c_uint32),
        ("track_gain_mb", ctypes.c_int32), ("has_track_gain", ctypes.c_uint32),
        ("track_peak", ctypes.c_uint32), ("has_track_peak", ctypes.c_uint32),
        ("album_gain_mb", ctypes.c_int32), ("has_album_gain", ctypes.c_uint32),
        ("album_peak", ctypes.c_uint32), ("has_album_peak", ctypes.c_uint32),
    ]


class SongMetadataEntry(ctypes.Structure):
    _fields_ = [
        ("scope", ctypes.c_uint32),
        ("key", ctypes.c_void_p),
        ("key_len", ctypes.c_uint32),
        ("value", ctypes.c_void_p),
        ("value_len", ctypes.c_uint32),
        ("reserved", ctypes.c_uint32),
    ]


class SongArtworkItem(ctypes.Structure):
    _fields_ = [
        ("role", ctypes.c_uint32),
        ("mime", ctypes.c_void_p),
        ("mime_len", ctypes.c_uint32),
        ("data", ctypes.POINTER(ctypes.c_uint8)),
        ("data_len", ctypes.c_uint64),
        ("width", ctypes.c_int32),
        ("height", ctypes.c_int32),
        ("is_front_cover", ctypes.c_uint32),
        ("reserved", ctypes.c_uint32),
    ]


def utf8_view(ptr: int, length: int) -> str:
    """Decode a borrowed ABI string view (pointer + explicit length).

    The authority is the PAIR — NUL termination is never assumed, matching
    the frozen contract that future Kotlin/JNI/Swift bindings must copy.
    """
    if not ptr or length == 0:
        return ""
    return ctypes.string_at(ptr, length).decode("utf-8", "replace")


def bind_library(lib: ctypes.CDLL) -> None:
    """Declare argtypes/restype for all 15 frozen ABI symbols."""
    handle = ctypes.c_void_p
    lib.songcore_abi_version.argtypes = []
    lib.songcore_abi_version.restype = ctypes.c_uint32

    lib.song_open.argtypes = [ctypes.POINTER(SongIo),
                              ctypes.POINTER(handle)]
    lib.song_open.restype = ctypes.c_int

    lib.song_probe.argtypes = [handle, ctypes.POINTER(SongInfo)]
    lib.song_probe.restype = ctypes.c_int

    lib.song_audio_stream_count.argtypes = [handle, ctypes.POINTER(ctypes.c_uint32)]
    lib.song_audio_stream_count.restype = ctypes.c_int

    lib.song_audio_stream_info.argtypes = [handle, ctypes.c_uint32,
                                           ctypes.POINTER(SongStreamInfo)]
    lib.song_audio_stream_info.restype = ctypes.c_int

    lib.song_select_stream.argtypes = [handle, ctypes.c_uint32]
    lib.song_select_stream.restype = ctypes.c_int

    lib.song_get_metadata.argtypes = [handle,
                                      ctypes.POINTER(ctypes.POINTER(SongMetadata))]
    lib.song_get_metadata.restype = ctypes.c_int

    lib.song_get_metadata_count.argtypes = [handle, ctypes.POINTER(ctypes.c_uint32)]
    lib.song_get_metadata_count.restype = ctypes.c_int

    lib.song_get_metadata_entry.argtypes = [handle, ctypes.c_uint32,
                                            ctypes.POINTER(SongMetadataEntry)]
    lib.song_get_metadata_entry.restype = ctypes.c_int

    lib.song_get_artwork_count.argtypes = [handle, ctypes.POINTER(ctypes.c_uint32)]
    lib.song_get_artwork_count.restype = ctypes.c_int

    lib.song_get_artwork_item.argtypes = [handle, ctypes.c_uint32,
                                          ctypes.POINTER(SongArtworkItem)]
    lib.song_get_artwork_item.restype = ctypes.c_int

    lib.song_read_pcm.argtypes = [handle, ctypes.POINTER(ctypes.c_float),
                                  ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64)]
    lib.song_read_pcm.restype = ctypes.c_int

    lib.song_seek.argtypes = [handle, ctypes.c_int64,
                              ctypes.POINTER(ctypes.c_int64)]
    lib.song_seek.restype = ctypes.c_int

    lib.song_last_error.argtypes = [handle,
                                    ctypes.POINTER(ctypes.POINTER(SongError))]
    lib.song_last_error.restype = ctypes.c_int

    lib.song_close.argtypes = [handle]
    lib.song_close.restype = None


def default_library_path() -> Path:
    if sys.platform == "win32":
        return ROOT / "build" / "artifacts" / "shared" / "songcore.dll"
    if sys.platform == "darwin":
        return ROOT / "build" / "artifacts" / "shared" / "libsongcore.dylib"
    return ROOT / "build" / "artifacts" / "shared" / "libsongcore.so"


# ---------------------------------------------------------------------------
# Host I/O over a plain Python file object (the "host" side of song_io)
# ---------------------------------------------------------------------------

class FileSource:
    def __init__(self, path: Path):
        self._fh = open(path, "rb")
        self._fh.seek(0, os.SEEK_END)
        self._size = self._fh.tell()
        self._fh.seek(0)
        # Keep-alive: ctypes does not retain these references for us.
        self.read_fn = READ_FN(self._read)
        self.seek_fn = SEEK_FN(self._seek)
        self.size_fn = SIZE_FN(self._size_cb)

    def _read(self, _userdata, dst, size):
        try:
            data = self._fh.read(size)
        except OSError:
            return -1
        if not data:
            return 0
        ctypes.memmove(dst, data, len(data))
        return len(data)

    def _seek(self, _userdata, absolute_offset):
        try:
            self._fh.seek(absolute_offset, os.SEEK_SET)
            return self._fh.tell()
        except OSError:
            return -1

    def _size_cb(self, _userdata):
        return self._size

    def io(self) -> SongIo:
        return SongIo(None, self.read_fn, self.seek_fn, self.size_fn)

    def close(self):
        self._fh.close()


# ---------------------------------------------------------------------------
# Smoke driver
# ---------------------------------------------------------------------------

class Check:
    def __init__(self, name: str, ok: bool, detail: str = ""):
        self.name = name
        self.ok = ok
        self.detail = detail


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def channel_mask_summary(mask: int) -> str:
    if mask == 0:
        return "unknown"
    names = ["FL", "FR", "FC", "LFE", "BL", "BR", "FLC", "FRC", "BC",
             "SL", "SR", "TC", "TFL", "TFC", "TFR", "TBL", "TBC", "TBR"]
    return "+".join(names[b] for b in range(min(mask.bit_length(), len(names)))
                    if mask & (1 << b))


def decode_args(info: SongInfo) -> int:
    return max(1, int(info.channels))


class Smoke:
    """One full open->probe->decode->seek->close pass over one song."""

    def __init__(self, lib, song: Path, seconds: float, play: bool,
                 device, verbose: bool, pcm_dump_dir: Path | None = None):
        self.lib = lib
        self.song = song
        self.seconds = seconds
        self.play = play
        self.device = device
        self.verbose = verbose
        self.pcm_dump_dir = pcm_dump_dir
        self.checks: list[Check] = []
        self.result: dict = {"song": song.name}

    def check(self, name: str, ok: bool, detail: str = "") -> bool:
        c = Check(name, ok, detail)
        self.checks.append(c)
        mark = "PASS" if ok else "FAIL"
        print(f"    [{mark}] {name}" + (f" — {detail}" if detail else ""))
        return ok

    def last_error(self, handle) -> str:
        err_ptr = ctypes.POINTER(SongError)()
        if self.lib.song_last_error(handle, ctypes.byref(err_ptr)) != SONG_OK \
                or not err_ptr:
            return "<song_last_error unavailable>"
        err = err_ptr.contents
        if not err.message:
            return "<no diagnostic>"
        return utf8_view(err.message, err.message_len)

    def run(self) -> bool:
        src = FileSource(self.song)
        ok = True
        try:
            ok = self._run(src)
        finally:
            src.close()
        self.result["checks"] = {c.name: ("pass" if c.ok else "fail")
                                 for c in self.checks}
        self.result["verdict"] = "pass" if ok else "fail"
        return ok

    def _run(self, src: FileSource) -> bool:
        lib = self.lib

        # -- ABI version ---------------------------------------------------
        abi = lib.songcore_abi_version()
        if not self.check("abi_version", abi == SONGCORE_ABI_VERSION,
                          f"songcore_abi_version()={abi}"):
            return False

        # -- open over host IO ----------------------------------------------
        io = src.io()
        handle = ctypes.c_void_p()
        st = lib.song_open(ctypes.byref(io), ctypes.byref(handle))
        if not self.check("open", st == SONG_OK and handle,
                          status_name(st) if st != SONG_OK else "handle opened"):
            return False

        try:
            return self._after_open(handle)
        finally:
            lib.song_close(handle)
            self.check("close", True, "song_close returned")

    def _after_open(self, handle) -> bool:
        lib = self.lib

        # -- probe (idempotent snapshot) -------------------------------------
        info = SongInfo()
        st = lib.song_probe(handle, ctypes.byref(info))
        if st != SONG_OK:
            return self.check("probe", False,
                              f"{status_name(st)}: {self.last_error(handle)}")
        info2 = SongInfo()
        st2 = lib.song_probe(handle, ctypes.byref(info2))
        idempotent = st2 == SONG_OK and bytes(info2) == bytes(info)
        probe_ok = (info.sample_rate > 0 and info.channels > 0
                    and info.audio_stream_count >= 1
                    and bool(info.codec) and bool(info.container))
        if not self.check(
                "probe", probe_ok and idempotent,
                f"container={info.container.decode()} codec={info.codec.decode()} "
                f"{info.sample_rate} Hz / {info.channels} ch "
                f"({channel_mask_summary(info.channel_mask)}) "
                f"{info.bits_per_sample or '?'}-bit "
                f"dur={info.duration_us / 1e6 if info.duration_us > 0 else -1:.3f}s "
                f"streams={info.audio_stream_count} idempotent={idempotent}"):
            return False
        self.result.update({
            "container": info.container.decode(),
            "codec": info.codec.decode(),
            "sample_rate": info.sample_rate,
            "channels": info.channels,
            "channel_mask": info.channel_mask,
            "duration_us": info.duration_us,
            "bits_per_sample": info.bits_per_sample,
            "audio_stream_count": info.audio_stream_count,
        })

        # -- stream enumeration ----------------------------------------------
        count = ctypes.c_uint32()
        st = lib.song_audio_stream_count(handle, ctypes.byref(count))
        streams_ok = st == SONG_OK and count.value == info.audio_stream_count
        if streams_ok and count.value:
            sinfo = SongStreamInfo()
            st_s = lib.song_audio_stream_info(handle, 0, ctypes.byref(sinfo))
            streams_ok = streams_ok and st_s == SONG_OK and sinfo.sample_rate > 0
            if streams_ok:
                self.result["stream0"] = {
                    "stream_index": sinfo.stream_index,
                    "sample_rate": sinfo.sample_rate,
                    "channels": sinfo.channels,
                    "codec": sinfo.codec.decode(),
                    "is_default": sinfo.is_default,
                }
        self.check("stream_enumeration", streams_ok,
                   f"count={count.value}")

        # -- contract: invalid audio_index must be rejected -------------------
        st_bad = lib.song_select_stream(handle, count.value + 100)
        self.check("select_stream_rejects_invalid",
                   st_bad == SONG_ERR_INVALID_ARGUMENT,
                   f"{status_name(st_bad)}")

        # -- contract: zero frame capacity must be rejected --------------------
        st_zero = lib.song_read_pcm(handle, None, 0, ctypes.byref(ctypes.c_uint64(0)))
        self.check("read_pcm_rejects_zero_capacity",
                   st_zero == SONG_ERR_INVALID_ARGUMENT,
                   f"{status_name(st_zero)}")
        if st_zero == SONG_ERR_INVALID_ARGUMENT:
            err = self.last_error(handle)
            self.result["last_error_after_invalid"] = err
            if self.verbose:
                print(f"        last_error: {err}")

        # -- metadata ----------------------------------------------------------
        meta_ok = True
        meta_count = ctypes.c_uint32()
        st = lib.song_get_metadata_count(handle, ctypes.byref(meta_count))
        meta_ok = st == SONG_OK
        entries = []
        for i in range(meta_count.value if meta_ok else 0):
            entry = SongMetadataEntry()
            st_e = lib.song_get_metadata_entry(handle, i, ctypes.byref(entry))
            if st_e != SONG_OK or not entry.key:
                meta_ok = False
                break
            entries.append({
                "scope": entry.scope,
                "key": utf8_view(entry.key, entry.key_len),
                "value": utf8_view(entry.value, entry.value_len),
            })
        meta_ptr = ctypes.POINTER(SongMetadata)()
        st_m = lib.song_get_metadata(handle, ctypes.byref(meta_ptr))
        meta_ok = meta_ok and st_m == SONG_OK
        canon = {}
        if meta_ok:
            m = meta_ptr.contents
            for field in ("title", "artist", "album", "album_artist", "genre",
                          "composer", "date", "comment"):
                if getattr(m, f"has_{field}"):
                    canon[field] = utf8_view(getattr(m, field),
                                             getattr(m, f"{field}_len"))
            self.result["canonical_metadata"] = canon
        self.check("metadata", meta_ok,
                   f"{meta_count.value} entries"
                   + (f"; title={canon['title']!r}" if "title" in canon else ""))
        self.result["metadata_count"] = meta_count.value if meta_ok else None
        self.result["metadata_entries"] = entries

        # -- artwork -------------------------------------------------------------
        art_count = ctypes.c_uint32()
        st = lib.song_get_artwork_count(handle, ctypes.byref(art_count))
        art_ok = st == SONG_OK
        item = SongArtworkItem()
        art_summary = None
        if art_ok and art_count.value:
            st_i = lib.song_get_artwork_item(handle, 0, ctypes.byref(item))
            art_ok = (st_i == SONG_OK and item.data_len > 0
                      and item.mime_len > 0)
            if art_ok:
                art_hasher = hashlib.sha256()
                art_hasher.update(ctypes.string_at(item.data, item.data_len))
                art_summary = {
                    "mime": utf8_view(item.mime, item.mime_len),
                    "data_len": item.data_len,
                    "sha256": art_hasher.hexdigest(),
                    "width": item.width,
                    "height": item.height,
                    "is_front_cover": item.is_front_cover,
                }
                self.result["artwork0"] = art_summary
        self.check("artwork", art_ok,
                   f"{art_count.value} item(s)"
                   + (f", first mime={art_summary['mime']}, "
                      f"sha256={art_summary['sha256'][:16]}…"
                      if art_summary else ""))
        self.result["artwork_count"] = art_count.value if art_ok else None

        # -- decode ------------------------------------------------------------
        decode = self._decode(handle, info)
        if not decode.pop("_ok"):
            return False
        self.result.update(decode)

        # -- seek ----------------------------------------------------------------
        seek = self._seek_and_read(handle, info)
        if not seek.pop("_ok"):
            return False
        self.result.update(seek)
        return True

    def _decode(self, handle, info: SongInfo) -> dict:
        """Sequential decode (up to --seconds), hash + peak + optional play."""
        lib = self.lib
        limit_frames = int(self.seconds * info.sample_rate) if self.seconds else None
        chunk_frames = 4096
        buf = (ctypes.c_float * (chunk_frames * info.channels))()
        produced = ctypes.c_uint64()
        hasher = hashlib.sha256()
        peak = 0.0
        total_frames = 0
        eof_reached = False
        stream = None

        if self.play:
            try:
                import sounddevice as sd
            except ImportError:
                print(
                    "songcore_ffi_smoke: --play needs the test-only dependency "
                    "`sounddevice`; install with: python -m pip install sounddevice",
                    file=sys.stderr)
                raise SystemExit(2)
            stream = sd.RawOutputStream(
                samplerate=info.sample_rate,
                channels=info.channels,
                dtype="float32",
                device=self.device,
            )
            print(f"    [play ] {info.sample_rate} Hz / {info.channels} ch Float32 "
                  f"-> PortAudio (no SRC in Python)")

        out = {"_ok": True}
        dump = bytearray() if self.pcm_dump_dir is not None else None
        try:
            if stream is not None:
                stream.start()
            while limit_frames is None or total_frames < limit_frames:
                st = lib.song_read_pcm(handle, buf, chunk_frames,
                                       ctypes.byref(produced))
                if st == SONG_EOF:
                    eof_reached = True
                    if produced.value != 0:
                        return {"_ok": self.check(
                            "decode", False,
                            f"EOF with {produced.value} frames produced")}
                    break
                if st != SONG_OK:
                    return {"_ok": self.check(
                        "decode", False,
                        f"{status_name(st)}: {self.last_error(handle)}")}
                if produced.value == 0 or produced.value > chunk_frames:
                    return {"_ok": self.check(
                        "decode", False,
                        f"OK but produced={produced.value} frames")}
                n = produced.value * info.channels
                raw = ctypes.string_at(buf, n * 4)
                hasher.update(raw)
                if dump is not None and len(dump) < PCM_DUMP_BYTES:
                    dump += raw[:PCM_DUMP_BYTES - len(dump)]
                floats = struct.unpack(f"<{n}f", raw)
                peak = max(peak, max(abs(v) for v in floats))
                total_frames += produced.value
                if stream is not None:
                    stream.write(raw)
        finally:
            if stream is not None:
                stream.stop()
                stream.close()

        if total_frames <= 0:
            return {"_ok": self.check("decode", False, "no frames decoded")}
        self.check("decode", True,
                   f"{total_frames} frames decoded, eof={eof_reached}")

        print(f"    [PASS] decode — {total_frames} frames "
              f"({total_frames / info.sample_rate:.3f}s) "
              f"peak={peak:.4f} sha256={hasher.hexdigest()[:16]}… "
              f"eof={eof_reached}")
        result = {
            "_ok": True,
            "decoded_frames": total_frames,
            "decoded_seconds": round(total_frames / info.sample_rate, 6),
            # Full-window hash: the cross-platform PCM authority consumes
            # this (lossless formats must be identical across backends).
            "pcm_sha256": hasher.hexdigest(),
            "pcm_sha256_prefix": hasher.hexdigest()[:16],
            "peak": round(peak, 6),
            "eof_during_decode": eof_reached,
        }
        if dump is not None:
            self.pcm_dump_dir.mkdir(parents=True, exist_ok=True)
            dump_path = self.pcm_dump_dir / f"{self.song.name}.f32.dump"
            dump_path.write_bytes(bytes(dump))
            result["pcm_dump"] = {
                "file": dump_path.name,
                "frames": len(dump) // (info.channels * 4),
                "sha256": hashlib.sha256(dump).hexdigest(),
            }
        return result

    def _seek_and_read(self, handle, info: SongInfo) -> dict:
        """Seek to min(duration/2, 5 s) and prove PCM flows after landing."""
        lib = self.lib
        if info.duration_us > 0:
            target = min(info.duration_us // 2, 5_000_000)
        else:
            target = 1_000_000
        actual = ctypes.c_int64(-999)
        st = lib.song_seek(handle, target, ctypes.byref(actual))
        if st == 109:  # SONG_ERR_SEEK_UNSUPPORTED: the ABI's typed answer for
            # containers with no seek (e.g. raw ADTS). A typed refusal is a
            # contract PASS; only generic/wrong failures fail the gate.
            print(f"    [PASS] seek — SONG_ERR_SEEK_UNSUPPORTED "
                  f"(unseekable container, target={target / 1e6:.3f}s)")
            self.check("seek", True, "SONG_ERR_SEEK_UNSUPPORTED (typed)")
            return {
                "_ok": True,
                "seek_requested_us": target,
                "seek_unsupported": True,
            }
        if st != SONG_OK:
            return {"_ok": self.check(
                "seek", False,
                f"{status_name(st)}: {self.last_error(handle)}")}
        detail = f"target={target / 1e6:.3f}s actual={actual.value / 1e6:.3f}s" \
            if actual.value >= 0 else f"target={target / 1e6:.3f}s actual=unknown(-1)"

        # Drain: the landing must produce PCM before EOF (guard cap: 30 s).
        chunk_frames = 4096
        buf = (ctypes.c_float * (chunk_frames * info.channels))()
        produced = ctypes.c_uint64()
        post_frames = 0
        cap = int(30 * info.sample_rate)
        eof = False
        while post_frames < cap:
            st = lib.song_read_pcm(handle, buf, chunk_frames, ctypes.byref(produced))
            if st == SONG_EOF:
                eof = True
                break
            if st != SONG_OK:
                return {"_ok": self.check(
                    "seek", False,
                    f"post-seek {status_name(st)}: {self.last_error(handle)}")}
            post_frames += produced.value
        ok = post_frames > 0
        self.check("seek", ok, detail)
        print(f"    [{'PASS' if ok else 'FAIL'}] seek — {detail}, "
              f"{post_frames} post-seek frames, eof={eof}")
        return {
            "_ok": ok,
            "seek_requested_us": target,
            "seek_actual_us": actual.value,
            "post_seek_frames": post_frames,
            "post_seek_eof": eof,
        }


def main() -> int:
    try:
        sys.stdout.reconfigure(errors="replace")
        sys.stderr.reconfigure(errors="replace")
    except Exception:
        pass

    ap = argparse.ArgumentParser(
        description="External-consumer FFI smoke for SongCore ABI v1 "
                    "(ctypes -> shared library -> embedded FFmpeg closure)")
    ap.add_argument("songs", nargs="+", type=Path, help="audio files to smoke")
    ap.add_argument("--library", type=Path,
                    default=Path(os.environ.get("SONGCORE_LIBRARY",
                                                default_library_path())),
                    help="path to libsongcore.so/.dylib/songcore.dll "
                         "(default: build artifact; env SONGCORE_LIBRARY)")
    ap.add_argument("--seconds", type=float, default=3.0,
                    help="decode/play cap per song in seconds (default 3, 0=full)")
    ap.add_argument("--play", action="store_true",
                    help="audibly play decoded Float32 PCM via sounddevice "
                         "(test-only PortAudio sink; needs pip install sounddevice)")
    ap.add_argument("--device", default=None,
                    help="sounddevice output device id/name for --play")
    ap.add_argument("--json", type=Path, default=None,
                    help="write machine-readable results to this path")
    ap.add_argument("--pcm-dump-dir", type=Path, default=None,
                    help="write per-song bounded PCM sample dumps (start of "
                         "the decode window) here, for the cross-backend "
                         "consistency gate")
    ap.add_argument("-v", "--verbose", action="store_true")
    args = ap.parse_args()

    if args.seconds < 0:
        ap.error("--seconds must be >= 0")
    for song in args.songs:
        if not song.is_file():
            ap.error(f"song not found: {song}")

    lib_path = args.library
    if not lib_path.is_file():
        print(f"songcore_ffi_smoke: library not found: {lib_path}\n"
              f"build it with: xmake build songcore_shared", file=sys.stderr)
        return 2

    lib = ctypes.CDLL(str(lib_path.resolve()))
    bind_library(lib)

    print(f"songcore_ffi_smoke: {lib_path.resolve()}")
    print(f"  sha256={sha256_file(lib_path)}")
    print(f"  python {platform.python_version()} on {platform.system()} "
          f"{platform.machine()}")

    all_ok = True
    songs_out = []
    for song in args.songs:
        print(f"  -- {song}")
        smoke = Smoke(lib, song, args.seconds or None, args.play,
                      args.device, args.verbose, args.pcm_dump_dir)
        ok = smoke.run()
        all_ok = all_ok and ok
        result = dict(smoke.result)
        result["song_sha256"] = sha256_file(song)
        result["song_bytes"] = song.stat().st_size
        songs_out.append(result)
        print(f"  => {'PASS' if ok else 'FAIL'}: {song}")

    report = {
        "tool": "songcore_ffi_smoke",
        "library": str(lib_path),
        "library_sha256": sha256_file(lib_path),
        "abi_version_expected": SONGCORE_ABI_VERSION,
        "platform": f"{platform.system()}-{platform.machine()}",
        "python": platform.python_version(),
        "seconds_cap": args.seconds,
        "played": args.play,
        "songs": songs_out,
        "verdict": "pass" if all_ok else "fail",
    }
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        print(f"  json -> {args.json}")

    print(f"FFI SMOKE {'PASS' if all_ok else 'FAIL'} "
          f"({len(args.songs)} song(s) via {lib_path.name})")
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
