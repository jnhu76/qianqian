#!/usr/bin/env python3
"""Patch the wasi-libc init guard in Qianqian E09 guest modules.

WHY (E09 typed finding, WAMR-2.4.5 classic interpreter):
  wasi-sdk-34/LLVM-23 reactor/command modules inline a __tls_init guard into
  _initialize / _start:

      block
        global.get __memory_base
        i32.const <guard_addr>
        i32.add
        i32.load
        i32.eqz
        br_if 0          ; guard == 0 -> skip (first init)
        unreachable      ; guard != 0 -> double-init abort
      end
      ... i32.store guard=1 ...
      call <init fn>

  Under WAMR 2.4.5 the br_if lands ON the `unreachable` byte instead of the
  `end` (guard memory and the __memory_base global were both verified to be
  zero, so only a mis-targeted branch can reach the unreachable). The module
  then traps "unreachable" at startup. Node and Wasmtime execute the same
  bytes correctly.

WORKAROUND (guest-side, not runtime modification):
  Replace the single `unreachable` (0x00) with `nop` (0x01) inside the guard
  sequence, i.e. `45 0d 00 00 0b` -> `45 0d 00 01 0b`, but ONLY inside
  _initialize / _start bodies.

  - Every runtime: first init still stores guard=1 and runs the init call;
    the nopped byte is never executed on a spec-correct interpreter because
    the guard is zero at the one and only init call.
  - WAMR: the mis-targeted branch lands on the nop and falls through to the
    block end — the intended behavior.
  - Lost: the double-init abort. E09 runners call init exactly once.

The pattern `45 0d 00 00 0b` (eqz; br_if 0; unreachable; end) is anchored to
the guard by requiring the preceding `28 02 00 45` (i32.load align=2 off=0;
eqz) — a load-then-test that only the init guard exhibits in these modules.
Patching is limited to the named entry bodies so unrelated code is never
touched.
"""

import sys


def read_uleb(data: bytes, pos: int):
    result = 0
    shift = 0
    while True:
        b = data[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        shift += 7
        if b < 0x80:
            return result, pos


def sections(data: bytes):
    pos = 8
    while pos < len(data):
        sid = data[pos]
        size, pos = read_uleb(data, pos + 1)
        yield sid, pos, size
        pos += size


def find_entry_code_ranges(data: bytes):
    """Return {export_name: (code_body_start, code_body_end)} file offsets."""
    entries = {}          # defined func index -> name
    import_funcs = 0
    code_pos = None
    code_size = 0
    for sid, pos, size in sections(data):
        if sid == 2:  # import
            p = pos
            n, p = read_uleb(data, p)
            for _ in range(n):
                l, p = read_uleb(data, p)
                p += l                    # module name
                l, p = read_uleb(data, p)
                p += l                    # field name
                kind = data[p]
                p += 1
                if kind == 0:             # func import
                    _, p = read_uleb(data, p)
                    import_funcs += 1
                elif kind == 1:           # table
                    p += 1                # elemtype
                    flags, p = read_uleb(data, p)
                    _, p = read_uleb(data, p)      # min
                    if flags & 1:
                        _, p = read_uleb(data, p)  # max
                elif kind == 2:           # memory
                    flags, p = read_uleb(data, p)
                    _, p = read_uleb(data, p)      # min
                    if flags & 1:
                        _, p = read_uleb(data, p)  # max
                elif kind == 3:           # global
                    p += 2
                elif kind == 4:           # tag
                    p += 1
                    _, p = read_uleb(data, p)
        elif sid == 7:  # export
            p = pos
            n, p = read_uleb(data, p)
            for _ in range(n):
                l, p = read_uleb(data, p)
                name = data[p:p + l].decode()
                p += l
                kind = data[p]
                p += 1
                idx, p = read_uleb(data, p)
                if kind == 0 and name in ("_initialize", "_start"):
                    entries[idx - import_funcs] = name
        elif sid == 10:  # code
            code_pos, code_size = pos, size
    if not entries or code_pos is None:
        return {}
    # code section: count, then per-function: size, locals, body...
    p = code_pos
    n, p = read_uleb(data, p)
    ranges = {}
    for i in range(n):
        body_size, p = read_uleb(data, p)
        body_start = p  # includes locals vector; guard search across it is
        body_end = p + body_size  # harmless (pattern cannot match there)
        if i in entries:
            ranges[entries[i]] = (body_start, body_end)
        p = body_end
    return ranges


# i32.load(align=2,off=0); eqz; br_if 0; unreachable; end
GUARD = bytes.fromhex("28020045" "0d00" "00" "0b")
GUARD_PATCHED = bytes.fromhex("28020045" "0d00" "01" "0b")


def patch(path: str) -> int:
    data = bytearray(open(path, "rb").read())
    ranges = find_entry_code_ranges(data)
    total = 0
    for name, (start, end) in sorted(ranges.items()):
        count = 0
        pos = start
        while True:
            idx = data.find(GUARD, pos, end)
            if idx < 0:
                break
            data[idx:idx + len(GUARD)] = GUARD_PATCHED
            pos = idx + len(GUARD_PATCHED)
            count += 1
        total += count
        if count:
            print(f"{path}: patched {count} init guard(s) in {name}")
    if not total:
        print(f"{path}: no init guard found (nothing to patch)")
    open(path, "wb").write(bytes(data))
    return 0


if __name__ == "__main__":
    rc = 0
    for p in sys.argv[1:]:
        rc |= patch(p)
    sys.exit(rc)
