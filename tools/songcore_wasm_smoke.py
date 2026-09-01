#!/usr/bin/env python3
"""songcore_wasm_smoke.py — external WASI-host acceptance gate for SongCore.

An independent host (Python `wasmtime`, no Qianqian code involved) loads the
shipped guest module `build/artifacts/wasm/SongCore.wasm`, provides the
`qianqian_host` read/seek/size imports over a plain file, and drives the
frozen ABI through the song_wasm_* bridge: abi -> open -> probe -> metadata
-> decode -> seek -> close. This is exactly the work a browser/WASM
embedder must do; nothing here touches qn_pcm_dump or the bench harnesses.

Guest-memory scratch (output structs, PCM buffer) is placed 128 KiB above
the data segment, inside the module's 1 MiB stack region: dlmalloc's arena
starts at __heap_base (above the stack), so the region below the active
stack frames is never handed out by the guest allocator.

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

# --- wasm32 layouts of the ABI snapshot structs (pointer = 4 bytes) --------
INFO_FMT = "<iiQi i 32s 32s III3I"          # song_info, size 116 (pad to 120)
INFO_SIZE = 120
INFO_OFF = {"sample_rate": 0, "channels": 4, "channel_mask": 8,
            "duration_us": 16, "bits_per_sample": 24, "codec": 28,
            "container": 60, "selected": 92, "streams": 96}
# song_metadata: 8 string triples (ptr, len, has) then 4*2 track/disc then
# 4 gain groups (i32,u32,u32,u32) — 160 bytes total, no internal padding.
META_STRINGS = ["title", "artist", "album", "album_artist", "genre",
                "composer", "date", "comment"]
META_SIZE = 160
ERR_SIZE = 16

SCRATCH_OFF = 128 * 1024                     # deep stack region (see above)


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

    def i64(self, off: int) -> int:
        return struct.unpack_from("<q", self.read(off, 8))[0]

    def cstr(self, ptr: int, length: int) -> str:
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


class WasmSmoke:
    def __init__(self, instance, store, guest: GuestMemory, sources: HostSources,
                 song_path: Path, seconds: float | None):
        self.inst = instance
        self.store = store
        self.g = guest
        self.sources = sources
        self.song_path = song_path
        self.seconds = seconds
        self.checks: dict[str, str] = {}
        self.result: dict = {"song": song_path.name}

    def export(self, name: str):
        return self.inst.exports(self.store)[name]

    def check(self, name: str, ok: bool, detail: str = "") -> bool:
        self.checks[name] = "pass" if ok else "fail"
        print(f"    [{'PASS' if ok else 'FAIL'}] {name}"
              + (f" — {detail}" if detail else ""))
        return ok

    def scratch(self, size: int) -> int:
        return SCRATCH_OFF

    def run(self) -> bool:
        # The linker callbacks close over THIS sources instance, so the
        # handle passed to song_wasm_open must come from it.
        handle = self.sources.add(self.song_path)
        try:
            return self._run(handle)
        finally:
            self.sources.remove(handle)

    def _run(self, host_handle: int) -> bool:
        g, s = self.g, self.store
        abi = self.export("song_wasm_abi_version")(s)
        if not self.check("abi_version", abi == SONGCORE_ABI_VERSION,
                          f"song_wasm_abi_version()={abi}"):
            return False

        out = self.scratch(8)
        st = self.export("song_wasm_open")(s, host_handle, out)
        song = g.i64(out)
        if not self.check("open", st == SONG_OK and song != 0,
                          STATUS_NAMES.get(st, str(st))):
            return False
        try:
            return self._after_open(song)
        finally:
            self.export("song_wasm_close")(s, song)
            self.check("close", True, "song_wasm_close returned")

    def _after_open(self, song: int) -> bool:
        g, s = self.g, self.store
        info_ptr = self.scratch(INFO_SIZE)
        st = self.export("song_wasm_probe")(s, song, info_ptr)
        if st != SONG_OK:
            return self.check("probe", False, STATUS_NAMES.get(st, str(st)))
        raw = g.read(info_ptr, INFO_SIZE)
        sample_rate, channels = struct.unpack_from("<ii", raw)
        channel_mask, duration_us = struct.unpack_from("<Qq", raw, 8)
        bits = struct.unpack_from("<i", raw, 24)[0]
        codec = raw[28:60].split(b"\0")[0].decode()
        container = raw[60:92].split(b"\0")[0].decode()
        streams = struct.unpack_from("<I", raw, 96)[0]
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

        count_ptr = self.scratch(4)
        st = self.export("song_wasm_audio_stream_count")(s, song, count_ptr)
        self.check("stream_enumeration",
                   st == SONG_OK and g.u32(count_ptr) == streams,
                   f"count={g.u32(count_ptr)}")

        meta_ptr_out = self.scratch(4)
        st = self.export("song_wasm_get_metadata")(s, song, meta_ptr_out)
        meta_count_ptr = self.scratch(4)
        self.export("song_wasm_get_metadata_count")(s, song, meta_count_ptr)
        n_meta = g.u32(meta_count_ptr)
        canon = {}
        if st == SONG_OK:
            snapshot = g.u32(meta_ptr_out)
            for i, field in enumerate(META_STRINGS):
                base = snapshot + i * 12
                has = g.u32(base + 8)
                if has:
                    ptr, length = g.u32(base), g.u32(base + 4)
                    canon[field] = g.cstr(ptr, length)
        self.check("metadata", st == SONG_OK,
                   f"{n_meta} entries"
                   + (f"; title={canon['title']!r}" if "title" in canon else ""))
        self.result["metadata_count"] = n_meta
        self.result["canonical_metadata"] = canon

        # -- decode -----------------------------------------------------------
        cap = 4096
        dst = self.scratch(4) + 4096      # past the small outputs
        frames_ptr = self.scratch(4) + 8
        limit = int(self.seconds * sample_rate) if self.seconds else None
        total, hasher, peak, eof = 0, hashlib.sha256(), 0.0, False
        while limit is None or total < limit:
            st = self.export("song_wasm_read_pcm")(s, song, dst, cap, frames_ptr)
            if st == SONG_EOF:
                eof = True
                if g.i64(frames_ptr) != 0:
                    return self.check("decode", False, "EOF with frames")
                break
            if st != SONG_OK:
                return self.check("decode", False,
                                  STATUS_NAMES.get(st, str(st)))
            produced = g.i64(frames_ptr)
            if produced == 0 or produced > cap:
                return self.check("decode", False, f"produced={produced}")
            raw = g.read(dst, int(produced) * channels * 4)
            hasher.update(raw)
            floats = struct.unpack(f"<{produced * channels}f", raw)
            peak = max(peak, max(abs(v) for v in floats))
            total += produced
        if total <= 0:
            return self.check("decode", False, "no frames decoded")
        print(f"    [PASS] decode — {total} frames "
              f"({total / sample_rate:.3f}s) peak={peak:.4f} "
              f"sha256={hasher.hexdigest()[:16]}… eof={eof}")
        self.result["decoded_frames"] = total
        self.result["pcm_sha256_prefix"] = hasher.hexdigest()[:16]
        self.result["peak"] = round(peak, 6)

        # -- seek --------------------------------------------------------------
        target = min(duration_us // 2, 5_000_000) if duration_us > 0 else 1_000_000
        st = self.export("song_wasm_seek")(s, song, target, frames_ptr)
        if st == SONG_ERR_SEEK_UNSUPPORTED:
            print("    [PASS] seek — SONG_ERR_SEEK_UNSUPPORTED "
                  "(unseekable container, typed)")
            self.result["seek_unsupported"] = True
            return True
        if st != SONG_OK:
            return self.check("seek", False, STATUS_NAMES.get(st, str(st)))
        actual = g.i64(frames_ptr)
        post, eof2 = 0, False
        while post < 30 * sample_rate:
            st = self.export("song_wasm_read_pcm")(s, song, dst, cap, frames_ptr)
            if st == SONG_EOF:
                eof2 = True
                break
            if st != SONG_OK:
                return self.check("seek", False,
                                  f"post-seek {STATUS_NAMES.get(st, str(st))}")
            post += g.i64(frames_ptr)
        ok = post > 0
        detail = (f"target={target / 1e6:.3f}s "
                  f"actual={actual / 1e6:.3f}s" if actual >= 0
                  else f"target={target / 1e6:.3f}s actual=unknown(-1)")
        print(f"    [{'PASS' if ok else 'FAIL'}] seek — {detail}, "
              f"{post} post-seek frames, eof={eof2}")
        self.result["post_seek_frames"] = post
        return ok


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

    print(f"songcore_wasm_smoke: {args.wasm.resolve()}")
    print(f"  sha256={sha256_file(args.wasm)}")
    print(f"  wasmtime {getattr(wasmtime, '__version__', '?')} on "
          f"{platform.system()} {platform.machine()}")

    guest = sources.guest
    all_ok = True
    songs_out = []
    for song in args.songs:
        print(f"  -- {song}")
        smoke = WasmSmoke(instance, store, guest, sources,
                          song, args.seconds or None)
        ok = smoke.run()
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
