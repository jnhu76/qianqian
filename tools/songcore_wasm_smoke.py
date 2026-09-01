#!/usr/bin/env python3
"""songcore_wasm_smoke.py — external WASI-host acceptance gate for SongCore.

An independent host (Python `wasmtime`, no Qianqian code involved) loads the
shipped guest module `build/artifacts/wasm/SongCore.wasm`, provides the
`qianqian_host` read/seek/size imports over a plain file, and drives the
frozen ABI through the song_wasm_* bridge:

  abi -> open -> probe -> streams -> metadata (canonical + raw) ->
  artwork -> decode -> seek -> close

This is exactly the work a browser/WASM embedder must do; nothing here
touches qn_pcm_dump or the bench harnesses.

Memory contract (no guessed addresses): every host-side buffer — output
structs, count cells, the PCM buffer — is obtained from the GUEST through
the bridge's `song_wasm_alloc` export and returned via `song_wasm_free`.
Struct interpretation is machine-validated: `song_wasm_layout` returns the
sizeof/offsetof table compiled INTO the guest from songcore.h, so this host
never hardcodes a layout number. Known-metadata fixtures additionally
assert exact canonical values (the gate would catch any layout drift
semantically as well).

Exit codes: 0 all songs PASS, 1 smoke failure, 2 environment/usage error.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_WASM = ROOT / "build" / "artifacts" / "wasm" / "SongCore.wasm"

SONGCORE_ABI_VERSION = 1
SONG_OK = 0
SONG_EOF = 1
SONG_ERR_SEEK_UNSUPPORTED = 109

STATUS_NAMES = {
    0: "SONG_OK", 1: "SONG_EOF", 100: "SONG_ERR_INVALID_ARGUMENT",
    101: "SONG_ERR_STATE", 102: "SONG_ERR_NOT_OPEN", 103: "SONG_ERR_IO",
    104: "SONG_ERR_UNSUPPORTED_CONTAINER", 105: "SONG_ERR_NO_AUDIO_STREAM",
    106: "SONG_ERR_UNSUPPORTED_CODEC", 107: "SONG_ERR_CORRUPT_DATA",
    108: "SONG_ERR_DECODE_ERROR", 109: "SONG_ERR_SEEK_UNSUPPORTED",
    110: "SONG_ERR_SEEK_ERROR", 111: "SONG_ERR_STREAM_CHANGE",
    112: "SONG_ERR_OUT_OF_MEMORY", 113: "SONG_ERR_INTERNAL_ERROR",
}

# The 15 song_wasm_* exports must mirror the native 15-symbol ABI 1:1.
# Bridge infrastructure (alloc/free/layout) is deliberately NOT ABI.
CONTRACT_EXPORTS = [
    "song_wasm_abi_version", "song_wasm_open", "song_wasm_probe",
    "song_wasm_audio_stream_count", "song_wasm_audio_stream_info",
    "song_wasm_select_stream", "song_wasm_get_metadata",
    "song_wasm_get_metadata_count", "song_wasm_get_metadata_entry",
    "song_wasm_get_artwork_count", "song_wasm_get_artwork_item",
    "song_wasm_read_pcm", "song_wasm_seek", "song_wasm_last_error",
    "song_wasm_close",
]
BRIDGE_EXPORTS = ["song_wasm_alloc", "song_wasm_free", "song_wasm_layout"]
LAYOUT_WORDS = 37  # returned by song_wasm_layout; verified at runtime

META_STRINGS = ["title", "artist", "album", "album_artist", "genre",
                "composer", "date", "comment"]

# Cross-backend PCM comparison window (matches songcore_ffi_smoke.py).
PCM_DUMP_FRAMES = 8192

# Frozen acceptance values for known-metadata fixtures. The WASM consumer
# must read the SAME song meaning the native consumers see.
EXPECTED_METADATA = {
    "flac-16-44-stereo.flac": {
        "title": "Flac Sixteen Fortyfour",
        "artist": "Qianqian Lab",
        "album": "Corpus Gamma",
    },
    "mp3-cbr-id3v23.mp3": {
        "title": "千千测试 CBR",
        "artist": "Qianqian Lab",
        "album": "Corpus Alpha",
    },
}


class GuestMemory:
    def __init__(self, store, memory):
        self.store = store
        self.mem = memory

    def read(self, off: int, size: int) -> bytes:
        return bytes(self.mem.read(self.store, off, off + size))

    def write(self, data: bytes, off: int) -> None:
        self.mem.write(self.store, data, off)

    def u32(self, off: int) -> int:
        return struct.unpack_from("<I", self.read(off, 4))[0]

    def i32(self, off: int) -> int:
        return struct.unpack_from("<i", self.read(off, 4))[0]

    def i64(self, off: int) -> int:
        return struct.unpack_from("<q", self.read(off, 8))[0]

    def u64(self, off: int) -> int:
        return struct.unpack_from("<Q", self.read(off, 8))[0]

    def cstr(self, ptr: int, length: int) -> str:
        """ABI strings are (pointer, length) pairs — never NUL-scanned."""
        if not ptr or length == 0:
            return ""
        return self.read(ptr, length).decode("utf-8", "replace")


class HostSources:
    """The host side of the `qianqian_host` import module.

    The guest-memory view is injected after instantiation (the linker must
    be complete before the module instantiates, but the exported memory
    only exists afterwards); the callbacks never run before that.
    """

    def __init__(self):
        self.sources: dict[int, object] = {}
        self._next = 1
        self.guest: GuestMemory | None = None

    def add(self, path: Path) -> int:
        fh = open(path, "rb")
        fh.seek(0, os.SEEK_END)
        size = fh.tell()
        fh.seek(0)
        handle = self._next
        self._next += 1
        self.sources[handle] = (fh, size)
        return handle

    def remove(self, handle: int) -> None:
        fh, _ = self.sources.pop(handle)
        fh.close()

    def bind(self, linker):
        import wasmtime

        def read(handle: int, dst: int, length: int) -> int:
            entry = self.sources.get(handle)
            if entry is None or self.guest is None:
                return -1
            data = entry[0].read(length)
            if not data:
                return 0
            self.guest.write(data, dst)
            return len(data)

        def seek(handle: int, absolute: int) -> int:
            entry = self.sources.get(handle)
            if entry is None:
                return -1
            fh = entry[0]
            try:
                fh.seek(absolute, os.SEEK_SET)
            except OSError:
                return -1
            return fh.tell()

        def size(handle: int) -> int:
            entry = self.sources.get(handle)
            return entry[1] if entry else -1

        def func(params, results):
            return wasmtime.FuncType(
                [getattr(wasmtime.ValType, p)() for p in params],
                [getattr(wasmtime.ValType, r)() for r in results])

        linker.define_func("qianqian_host", "read",
                           func(["i64", "i32", "i32"], ["i64"]), read)
        linker.define_func("qianqian_host", "seek",
                           func(["i64", "i64"], ["i64"]), seek)
        linker.define_func("qianqian_host", "size",
                           func(["i64"], ["i64"]), size)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


class Alloc:
    """Guest-side allocator: every host-owned buffer comes from the guest's
    own malloc through the bridge exports. No fixed addresses anywhere."""

    def __init__(self, instance, store):
        ex = instance.exports(store)
        self.alloc = ex["song_wasm_alloc"]
        self.free = ex["song_wasm_free"]
        self.live = 0

    def take(self, store, size: int) -> int:
        ptr = self.alloc(store, size)
        if not ptr:
            raise RuntimeError(f"song_wasm_alloc({size}) failed in the guest")
        self.live += 1
        return ptr

    def give(self, store, ptr: int) -> None:
        if ptr:
            self.free(store, ptr)
            self.live -= 1


class WasmSmoke:
    def __init__(self, instance, store, guest: GuestMemory, sources: HostSources,
                 song_path: Path, seconds: float | None,
                 pcm_dump_dir: Path | None):
        self.inst = instance
        self.store = store
        self.g = guest
        self.sources = sources
        self.song_path = song_path
        self.seconds = seconds
        self.pcm_dump_dir = pcm_dump_dir
        self.checks: dict[str, str] = {}
        self.result: dict = {"song": song_path.name}

    def export(self, name: str):
        return self.inst.exports(self.store)[name]

    def check(self, name: str, ok: bool, detail: str = "") -> bool:
        self.checks[name] = "pass" if ok else "fail"
        print(f"    [{'PASS' if ok else 'FAIL'}] {name}"
              + (f" — {detail}" if detail else ""))
        return ok

    def run(self, expected_meta: dict | None) -> bool:
        # The linker callbacks close over THIS sources instance, so the
        # handle passed to song_wasm_open must come from it.
        handle = self.sources.add(self.song_path)
        try:
            return self._run(handle, expected_meta or {})
        finally:
            self.sources.remove(handle)

    def _run(self, host_handle: int, expected_meta: dict) -> bool:
        g, s = self.g, self.store
        alloc = Alloc(self.inst, s)
        ok = False
        try:
            ok = self._with_alloc(alloc, host_handle, expected_meta)
        finally:
            # Guest-memory ownership is part of the host contract: a leaked
            # song_wasm_alloc allocation fails the run.
            if alloc.live:
                print(f"    [FAIL] guest memory — "
                      f"{alloc.live} song_wasm_alloc allocation(s) leaked")
                self.checks["guest_memory"] = "fail"
                ok = False
            else:
                self.checks.setdefault("guest_memory", "pass")
        return ok

    def _with_alloc(self, alloc: Alloc, host_handle: int,
                    expected_meta: dict) -> bool:
        g, s = self.g, self.store

        # -- layout: machine-validated mirror of the guest's own structs ---
        words_ptr = alloc.take(s, 4)
        n_words = self.export("song_wasm_layout")(s, words_ptr, 0)
        if not self.check("layout", n_words == LAYOUT_WORDS,
                          f"song_wasm_layout reports {n_words} words"):
            alloc.give(s, words_ptr)
            return False
        words_buf = alloc.take(s, n_words * 4)
        ret = self.export("song_wasm_layout")(s, words_buf, n_words)
        raw = g.read(words_buf, ret * 4)
        L = struct.unpack(f"<{ret}I", raw)
        alloc.give(s, words_buf)
        alloc.give(s, words_ptr)
        (L_ver, info_size, off_rate, off_ch, off_mask, off_dur, off_bits,
         off_codec, off_container, off_sel, off_count, sinfo_size,
         off_srate, off_sch, meta_size, meta_first, meta_stride,
         meta_track, entry_size, e_scope, e_key, e_key_len, e_val,
         e_val_len, art_size, a_role, a_mime, a_mime_len, a_data,
         a_data_len, a_w, a_h, a_front, err_size, err_msg, err_msg_len,
         err_native) = L
        if L[0] != 1:
            return self.check("layout", False, f"unknown version {L_ver}")

        # -- ABI version ----------------------------------------------------
        abi = self.export("song_wasm_abi_version")(s)
        if not self.check("abi_version", abi == SONGCORE_ABI_VERSION,
                          f"song_wasm_abi_version()={abi}"):
            return False

        # -- open (guest-allocated out cell) --------------------------------
        out = alloc.take(s, 8)
        st = self.export("song_wasm_open")(s, host_handle, out)
        song = g.i64(out)
        alloc.give(s, out)
        if not self.check("open", st == SONG_OK and song != 0,
                          STATUS_NAMES.get(st, str(st))):
            return False
        try:
            return self._after_open(alloc, song, L, expected_meta)
        finally:
            self.export("song_wasm_close")(s, song)
            self.check("close", True, "song_wasm_close returned")

    def _after_open(self, alloc: Alloc, song: int, L: tuple,
                    expected_meta: dict) -> bool:
        (L_ver, info_size, off_rate, off_ch, off_mask, off_dur, off_bits,
         off_codec, off_container, off_sel, off_count, sinfo_size,
         off_srate, off_sch, meta_size, meta_first, meta_stride,
         meta_track, entry_size, e_scope, e_key, e_key_len, e_val,
         e_val_len, art_size, a_role, a_mime, a_mime_len, a_data,
         a_data_len, a_w, a_h, a_front, err_size, err_msg, err_msg_len,
         err_native) = L
        g, s = self.g, self.store

        # -- probe ------------------------------------------------------------
        info_ptr = alloc.take(s, info_size)
        st = self.export("song_wasm_probe")(s, song, info_ptr)
        if st != SONG_OK:
            return self.check("probe", False, STATUS_NAMES.get(st, str(st)))
        raw = g.read(info_ptr, info_size)
        sample_rate = struct.unpack_from("<i", raw, off_rate)[0]
        channels = struct.unpack_from("<i", raw, off_ch)[0]
        duration_us = struct.unpack_from("<q", raw, off_dur)[0]
        codec = raw[off_codec:off_codec + 32].split(b"\0")[0].decode()
        container = raw[off_container:off_container + 32].split(b"\0")[0].decode()
        streams = struct.unpack_from("<I", raw, off_count)[0]
        alloc.give(s, info_ptr)
        ok = sample_rate > 0 and channels > 0 and streams >= 1
        if not self.check("probe", ok,
                          f"container={container} codec={codec} "
                          f"{sample_rate} Hz / {channels} ch "
                          f"dur={duration_us / 1e6 if duration_us > 0 else -1:.3f}s "
                          f"streams={streams}"):
            return False
        self.result.update({"container": container, "codec": codec,
                            "sample_rate": sample_rate, "channels": channels,
                            "duration_us": duration_us,
                            "audio_stream_count": streams})

        # -- stream enumeration ---------------------------------------------
        count_ptr = alloc.take(s, 4)
        st = self.export("song_wasm_audio_stream_count")(s, song, count_ptr)
        n_streams = g.u32(count_ptr)
        sinfo_ptr = alloc.take(s, sinfo_size)
        st_s = self.export("song_wasm_audio_stream_info")(s, song, 0, sinfo_ptr)
        srate = g.i32(sinfo_ptr + off_srate) if st_s == SONG_OK else -1
        alloc.give(s, sinfo_ptr)
        alloc.give(s, count_ptr)
        if not self.check("stream_enumeration",
                          st == SONG_OK and st_s == SONG_OK
                          and n_streams == streams and srate > 0,
                          f"count={n_streams} stream0={srate} Hz"):
            return False

        # -- canonical metadata (pointer+length only) -------------------------
        meta_ptr_out = alloc.take(s, 4)
        st = self.export("song_wasm_get_metadata")(s, song, meta_ptr_out)
        snapshot = g.u32(meta_ptr_out)
        alloc.give(s, meta_ptr_out)
        meta_count_ptr = alloc.take(s, 4)
        self.export("song_wasm_get_metadata_count")(s, song, meta_count_ptr)
        n_meta = g.u32(meta_count_ptr)
        alloc.give(s, meta_count_ptr)
        canon = {}
        if st == SONG_OK and snapshot:
            for i, field in enumerate(META_STRINGS):
                base = snapshot + i * meta_stride
                if g.u32(base + 8):  # has_<field>
                    canon[field] = g.cstr(g.u32(base), g.u32(base + 4))
        meta_detail = f"{n_meta} entries"
        if "title" in canon:
            meta_detail += f"; title={canon['title']!r}"
        meta_ok = st == SONG_OK
        for field, want in expected_meta.items():
            got = canon.get(field)
            if got != want:
                meta_ok = False
                meta_detail += f"; MISMATCH {field}: got {got!r}, want {want!r}"
        missing = [f for f in expected_meta if f not in canon]
        if missing:
            meta_ok = False
            meta_detail += f"; missing canonical fields {missing}"
        if not self.check("metadata", meta_ok, meta_detail):
            return False
        self.result["metadata_count"] = n_meta
        self.result["canonical_metadata"] = canon

        # -- raw metadata: every entry via pointer+length ---------------------
        raw_entries = []
        raw_ok = True
        for i in range(n_meta):
            entry_ptr = alloc.take(s, entry_size)
            st_e = self.export("song_wasm_get_metadata_entry")(
                s, song, i, entry_ptr)
            er = g.read(entry_ptr, entry_size)
            alloc.give(s, entry_ptr)
            if st_e != SONG_OK:
                raw_ok = False
                break
            scope = struct.unpack_from("<I", er, e_scope)[0]
            kptr, klen = struct.unpack_from("<II", er, e_key)
            vptr = struct.unpack_from("<I", er, e_val)[0]
            vlen = struct.unpack_from("<I", er, e_val_len)[0]
            entry = {"scope": scope,
                     "key": g.cstr(kptr, klen),
                     "value": g.cstr(vptr, vlen)}
            raw_entries.append(entry)
        if not self.check("raw_metadata", raw_ok,
                          f"{len(raw_entries)} raw entries"
                          + (f"; first={raw_entries[0]['key']!r}"
                             if raw_entries else "")):
            return False
        self.result["raw_metadata"] = raw_entries[:8]

        # -- artwork ------------------------------------------------------------
        art_count_ptr = alloc.take(s, 4)
        st = self.export("song_wasm_get_artwork_count")(s, song, art_count_ptr)
        art_count = g.u32(art_count_ptr)
        alloc.give(s, art_count_ptr)
        art_ok = st == SONG_OK
        art_summary = None
        if art_ok and art_count > 0:
            item_ptr = alloc.take(s, art_size)
            st_i = self.export("song_wasm_get_artwork_item")(
                s, song, 0, item_ptr)
            ir = g.read(item_ptr, art_size)
            alloc.give(s, item_ptr)
            if st_i != SONG_OK:
                art_ok = False
            else:
                mime_ptr, mime_len = struct.unpack_from("<II", ir, a_mime)
                data_ptr = struct.unpack_from("<I", ir, a_data)[0]
                data_len = struct.unpack_from("<Q", ir, a_data_len)[0]
                width = struct.unpack_from("<i", ir, a_w)[0]
                height = struct.unpack_from("<i", ir, a_h)[0]
                front = struct.unpack_from("<I", ir, a_front)[0]
                if not data_ptr or data_len == 0 or mime_len == 0:
                    art_ok = False
                else:
                    art_bytes = g.read(data_ptr, data_len)
                    art_hasher = hashlib.sha256()
                    art_hasher.update(art_bytes)
                    art_summary = {
                        "mime": g.cstr(mime_ptr, mime_len),
                        "data_len": data_len,
                        "sha256": art_hasher.hexdigest(),
                        "width": width,
                        "height": height,
                        "is_front_cover": front,
                    }
                    self.result["artwork0"] = art_summary
        if not self.check("artwork", art_ok, f"{art_count} item(s)"
                          + (f", mime={art_summary['mime']}, "
                             f"{art_summary['data_len']} B, "
                             f"sha256={art_summary['sha256'][:16]}…"
                             if art_summary else "")):
            return False

        # -- decode -----------------------------------------------------------
        cap = 4096
        dst = alloc.take(s, cap * channels * 4)
        frames_ptr = alloc.take(s, 8)
        limit = int(self.seconds * sample_rate) if self.seconds else None
        total, hasher, peak, eof = 0, hashlib.sha256(), 0.0, False
        dump = bytearray()
        while limit is None or total < limit:
            st = self.export("song_wasm_read_pcm")(s, song, dst, cap,
                                                   frames_ptr)
            if st == SONG_EOF:
                eof = True
                if g.i64(frames_ptr) != 0:
                    alloc.give(s, dst)
                    alloc.give(s, frames_ptr)
                    return self.check("decode", False, "EOF with frames")
                break
            if st != SONG_OK:
                alloc.give(s, dst)
                alloc.give(s, frames_ptr)
                return self.check("decode", False,
                                  STATUS_NAMES.get(st, str(st)))
            produced = g.i64(frames_ptr)
            if produced == 0 or produced > cap:
                alloc.give(s, dst)
                alloc.give(s, frames_ptr)
                return self.check("decode", False, f"produced={produced}")
            raw_pcm = g.read(dst, int(produced) * channels * 4)
            hasher.update(raw_pcm)
            if len(dump) < PCM_DUMP_FRAMES * channels * 4:
                dump += raw_pcm[:PCM_DUMP_FRAMES * channels * 4 - len(dump)]
            floats = struct.unpack(f"<{produced * channels}f", raw_pcm)
            peak = max(peak, max(abs(v) for v in floats))
            total += produced
        alloc.give(s, dst)
        alloc.give(s, frames_ptr)
        if total <= 0:
            return self.check("decode", False, "no frames decoded")
        self.check("decode", True, f"{total} frames decoded, eof={eof}")
        print(f"    [PASS] decode — {total} frames "
              f"({total / sample_rate:.3f}s) peak={peak:.4f} "
              f"sha256={hasher.hexdigest()[:16]}… eof={eof}")
        self.result["decoded_frames"] = total
        self.result["pcm_sha256"] = hasher.hexdigest()
        self.result["pcm_sha256_prefix"] = hasher.hexdigest()[:16]
        self.result["peak"] = round(peak, 6)
        if self.pcm_dump_dir is not None and total >= 0:
            self.pcm_dump_dir.mkdir(parents=True, exist_ok=True)
            dump_path = self.pcm_dump_dir / f"{self.song_path.name}.f32.dump"
            dump_path.write_bytes(bytes(dump))
            self.result["pcm_dump"] = {
                "file": dump_path.name,
                "frames": len(dump) // (channels * 4),
                "sha256": hashlib.sha256(dump).hexdigest(),
            }

        # -- seek --------------------------------------------------------------
        target = min(duration_us // 2, 5_000_000) if duration_us > 0 else 1_000_000
        frames_ptr = alloc.take(s, 8)
        st = self.export("song_wasm_seek")(s, song, target, frames_ptr)
        if st == SONG_ERR_SEEK_UNSUPPORTED:
            alloc.give(s, frames_ptr)
            print("    [PASS] seek — SONG_ERR_SEEK_UNSUPPORTED "
                  "(unseekable container, typed)")
            self.check("seek", True, "SONG_ERR_SEEK_UNSUPPORTED (typed)")
            self.result["seek_unsupported"] = True
            return True
        if st != SONG_OK:
            alloc.give(s, frames_ptr)
            return self.check("seek", False, STATUS_NAMES.get(st, str(st)))
        actual = g.i64(frames_ptr)
        alloc.give(s, frames_ptr)
        post, eof2 = 0, False
        dst = alloc.take(s, cap * channels * 4)
        frames_ptr = alloc.take(s, 8)
        while post < 30 * sample_rate:
            st = self.export("song_wasm_read_pcm")(s, song, dst, cap,
                                                   frames_ptr)
            if st == SONG_EOF:
                eof2 = True
                break
            if st != SONG_OK:
                alloc.give(s, dst)
                alloc.give(s, frames_ptr)
                return self.check("seek", False,
                                  f"post-seek {STATUS_NAMES.get(st, str(st))}")
            post += g.i64(frames_ptr)
        alloc.give(s, dst)
        alloc.give(s, frames_ptr)
        ok = post > 0
        detail2 = (f"target={target / 1e6:.3f}s "
                   f"actual={actual / 1e6:.3f}s" if actual >= 0
                   else f"target={target / 1e6:.3f}s actual=unknown(-1)")
        self.check("seek", ok, detail2)
        print(f"    [{'PASS' if ok else 'FAIL'}] seek — {detail2}, "
              f"{post} post-seek frames, eof={eof2}")
        self.result["post_seek_frames"] = post
        return ok


def check_export_surface(instance, store) -> tuple[bool, str]:
    """The guest must expose exactly the 15 contract mirrors + the declared
    bridge infrastructure; any other song_* export is an ABI violation."""
    exports = sorted(instance.exports(store).keys())
    song_exports = [e for e in exports if e.startswith("song")]
    want = sorted(CONTRACT_EXPORTS + BRIDGE_EXPORTS)
    extra = [e for e in song_exports if e not in want]
    missing = [e for e in CONTRACT_EXPORTS if e not in exports]
    if missing:
        return False, f"missing contract exports: {missing}"
    if extra:
        return False, f"unexpected song_* exports: {extra}"
    return True, (f"{len(CONTRACT_EXPORTS)} contract + "
                  f"{len(BRIDGE_EXPORTS)} bridge exports")


def main() -> int:
    try:
        sys.stdout.reconfigure(errors="replace")
    except Exception:
        pass
    ap = argparse.ArgumentParser(
        description="Independent wasmtime host smoke for the SongCore WASM guest")
    ap.add_argument("songs", nargs="+", type=Path)
    ap.add_argument("--wasm", type=Path, default=DEFAULT_WASM)
    ap.add_argument("--seconds", type=float, default=3.0)
    ap.add_argument("--json", type=Path, default=None)
    ap.add_argument("--pcm-dump-dir", type=Path, default=None,
                    help="write per-song bounded PCM sample dumps (start of "
                         "the decode window) here, for the cross-backend "
                         "consistency gate")
    args = ap.parse_args()

    for song in args.songs:
        if not song.is_file():
            ap.error(f"song not found: {song}")
    if not args.wasm.is_file():
        print(f"songcore_wasm_smoke: module not found: {args.wasm}\n"
              "build it with: xmake f --wasm=wasi ... && xmake build songcore_wasm",
              file=sys.stderr)
        return 2

    try:
        import wasmtime
    except ImportError:
        print("songcore_wasm_smoke: missing `wasmtime`; install with:\n"
              "  python -m pip install wasmtime", file=sys.stderr)
        return 2

    engine = wasmtime.Engine()
    module = wasmtime.Module.from_file(engine, str(args.wasm))
    linker = wasmtime.Linker(engine)
    linker.define_wasi()
    store = wasmtime.Store(engine)
    store.set_wasi(wasmtime.WasiConfig())

    sources = HostSources()
    sources.bind(linker)

    instance = linker.instantiate(store, module)
    initialize = instance.exports(store).get("_initialize")
    if initialize is not None:
        initialize(store)
    sources.guest = GuestMemory(store, instance.exports(store)["memory"])

    surface_ok, surface_detail = check_export_surface(instance, store)
    print(f"songcore_wasm_smoke: {args.wasm.resolve()}")
    print(f"  sha256={sha256_file(args.wasm)}")
    print(f"  wasmtime {getattr(wasmtime, '__version__', '?')} on "
          f"{platform.system()} {platform.machine()}")
    print(f"  [{'PASS' if surface_ok else 'FAIL'}] export surface — "
          f"{surface_detail}")

    guest = sources.guest
    all_ok = surface_ok
    songs_out = []
    for song in args.songs:
        print(f"  -- {song}")
        smoke = WasmSmoke(instance, store, guest, sources,
                          song, args.seconds or None, args.pcm_dump_dir)
        ok = smoke.run(EXPECTED_METADATA.get(song.name))
        all_ok = all_ok and ok
        result = dict(smoke.result)
        result["checks"] = smoke.checks
        result["song_sha256"] = sha256_file(song)
        result["verdict"] = "pass" if ok else "fail"
        songs_out.append(result)
        print(f"  => {'PASS' if ok else 'FAIL'}: {song}")

    report = {
        "tool": "songcore_wasm_smoke",
        "wasm": str(args.wasm),
        "wasm_sha256": sha256_file(args.wasm),
        "abi_version_expected": SONGCORE_ABI_VERSION,
        "export_surface": {"verdict": "pass" if surface_ok else "fail",
                           "detail": surface_detail,
                           "contract": CONTRACT_EXPORTS,
                           "bridge": BRIDGE_EXPORTS},
        "platform": f"{platform.system()}-{platform.machine()}",
        "seconds_cap": args.seconds,
        "songs": songs_out,
        "verdict": "pass" if all_ok else "fail",
    }
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        print(f"  json -> {args.json}")

    print(f"WASM SMOKE {'PASS' if all_ok else 'FAIL'} ({len(args.songs)} song(s))")
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
