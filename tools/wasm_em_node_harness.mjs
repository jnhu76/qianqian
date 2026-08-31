/*
 * tools/wasm_em_node_harness.mjs — E09 Emscripten variant harness (Node).
 *
 * Drives the same guest modules the C runtime ladder drives, through the
 * same contracts, so Emscripten numbers are measured identically:
 *
 *   guest=bench     bench_bind / bench_correct / bench_bench /
 *                   bench_pcm_prepare / bench_pcm_ptr|len / bench_pcm_pull
 *                   (Modes A/B/C; guest JSON arrives on stdout)
 *   guest=pb        pb_fill / pb_ptr / pb_len / pb_pull / pb_stage_alloc
 *                   (§14 PCM-copy microbenchmark)
 *
 * The SongCore.wasm contract (song_wasm_*) is exercised through the bench
 * guest for decode proofs; the shipping-shape harness wires the same door.
 *
 * Host-owned IO is preserved: the guest gets an opaque BigInt handle and
 * three functions; the fixture file never enters the guest heap.
 *
 * Import wiring: Emscripten 6 minifies import module/function names and
 * leaves the door slots empty at hook time, so we enumerate the module's
 * imports in declaration order and fill every non-callable slot with our
 * door functions (the guest declares read/seek/size in the same order).
 *
 * Usage: node tools/wasm_em_node_harness.mjs <guest> <mode> <args...>
 *   bench  correct  <fixture>
 *   bench  bench    <fixture> [iterations=3]
 *   bench  lifecycle <fixture>
 *   bench  pcm      <fixture> [chunk_frames]   (Mode B if chunk_frames omitted)
 *   pb     pull     [chunk_bytes=65536] [iters=200]
 */

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import * as path from "node:path";
import { createHash } from "node:crypto";

const require_ = createRequire(import.meta.url);
/* em artifacts live apart from the canonical dir once WASI is restored */
const ART = path.resolve(process.env.QN_EM_ART_DIR || "build/artifacts/wasm");

const [guest, mode, ...rest] = process.argv.slice(2);
if (!guest || !mode) {
    console.error("usage: node tools/wasm_em_node_harness.mjs <guest> <mode> ...");
    process.exit(2);
}

function out(obj) { console.log(JSON.stringify(obj)); }

/* ------------------------------------------------------------------ */
/* the door                                                            */
/* ------------------------------------------------------------------ */

function makeDoor(boundRef, heapRef) {
    return {
        // i64 params/results are BigInt (emscripten default = WASM_BIGINT)
        qn_host_read: (handle, dst, len) => {
            const fx = boundRef.fx;
            if (!fx) return -1n;
            dst = Number(dst); len = Number(len);   // i64/i32 arrive as BigInt
            if (fx.pos >= fx.buf.length) return 0n;
            const n = Math.min(fx.buf.length - fx.pos, len);
            fx.buf.copy(heapRef.mod.HEAPU8, dst, fx.pos, fx.pos + n);
            fx.pos += n;
            return BigInt(n);
        },
        qn_host_seek: (handle, off) => {
            const fx = boundRef.fx;
            if (!fx) return -1n;
            if (off < 0n || off > BigInt(fx.buf.length)) return -1n;
            void 0;
            fx.pos = Number(off);
            return off;
        },
        qn_host_size: () => BigInt(boundRef.fx ? boundRef.fx.buf.length : 0),
    };
}

function uleb(b, o) {
    let r = 0, s = 0;
    for (;;) {
        const x = b[o]; o += 1;
        r += (x & 0x7f) * Math.pow(2, s);
        if (!(x & 0x80)) return [r, o];
        s += 7;
    }
}

/* parse import func signatures; the linker reorders imports, so slots are
 * matched by type pattern, not declaration order */
function importSignatures(bytes) {
    let o = 8;
    const types = [];
    const sigs = [];
    const name = (t) => t === 0x7f ? "i32" : t === 0x7e ? "i64" : `0x${t.toString(16)}`;
    while (o < bytes.length) {
        const id = bytes[o]; o += 1;
        let size; [size, o] = uleb(bytes, o);
        if (id === 1) {
            const end = o + size;
            let p = o;
            let n; [n, p] = uleb(bytes, p);
            for (let ti = 0; ti < n; ti++) {
                p += 1;                                  // 0x60 functype
                let np; [np, p] = uleb(bytes, p);
                const params = [...bytes.slice(p, p + np)].map(name); p += np;
                let nr; [nr, p] = uleb(bytes, p);
                const results = [...bytes.slice(p, p + nr)].map(name); p += nr;
                types[ti] = { params, results };
            }
            o = end;
        } else if (id === 2) {
            const end = o + size;
            let p = o;
            let n; [n, p] = uleb(bytes, p);
            for (let i = 0; i < n; i++) {
                let ml; [ml, p] = uleb(bytes, p);
                const mod = bytes.slice(p, p + ml).toString(); p += ml;
                let nl; [nl, p] = uleb(bytes, p);
                const nm = bytes.slice(p, p + nl).toString(); p += nl;
                const kind = bytes[p]; p += 1;
                if (kind === 0) {
                    let ti; [ti, p] = uleb(bytes, p);
                    sigs.push({ mod, nm, ...types[ti] });
                } else if (kind === 2) {
                    const flags = bytes[p]; p += 1;
                    if (flags & 1) p += 4;
                    p += 4;
                } else if (kind === 1) {
                    p += 1;
                } else if (kind === 3) {
                    p += 2;
                } else if (kind === 4) {
                    p += 5;
                }
            }
            o = end;
            return sigs;
        } else {
            o += size;
        }
    }
    return sigs;
}

const DOOR_SIGS = {
    read: { params: ["i64", "i32", "i32"], results: ["i64"] },
    seek: { params: ["i64", "i64"], results: ["i64"] },
    size: { params: ["i64"], results: ["i64"] },
};

function wireImports(wasmPath, imports, suppliers) {
    const bytes = readFileSync(wasmPath);
    if (!suppliers) return bytes;               // guest without a door (pb)
    const pending = { ...suppliers };           // name -> fn
    for (const sig of importSignatures(bytes)) {
        const table = imports[sig.mod] || (imports[sig.mod] = {});
        if (typeof table[sig.nm] === "function") continue;
        const match = Object.keys(DOOR_SIGS).find((k) =>
            pending[k] &&
            DOOR_SIGS[k].params.join() === sig.params.join() &&
            DOOR_SIGS[k].results.join() === sig.results.join());
        if (match) {
            table[sig.nm] = pending[match];
            delete pending[match];
        } else {
            throw new Error(`unwired import ${sig.mod}.${sig.nm} (${sig.params}->${sig.results})`);
        }
    }
    const left = Object.keys(pending);
    if (left.length) {
        throw new Error(`door slots not found in module: ${left.join(",")}`);
    }
    return bytes;
}

/* load an emscripten MODULARIZE glue + its .wasm with the door wired */
async function loadGuest(jsName, boundRef, withDoor = true) {
    const heapRef = { mod: null };
    const hostRef = { load_ms: null, instantiate_ms: null };
    const door = makeDoor(boundRef, heapRef);
    const stdout = [], stderr = [];
    const factory = require_(path.join(ART, jsName));
    const wasmPath = path.join(ART, jsName.replace(/\.js$/, ".wasm"));
    const t_load = process.hrtime.bigint();
    const bytes = readFileSync(wasmPath);
    hostRef.load_ms = Number(process.hrtime.bigint() - t_load) / 1e6;
    const mod = await factory({
        print: (s) => stdout.push(s),
        printErr: (s) => stderr.push(s),
        noInitialRun: true,
        instantiateWasm: (imports, success) => {
            const wired = wireImports(wasmPath, imports, withDoor && {
                read: door.qn_host_read,
                seek: door.qn_host_seek,
                size: door.qn_host_size,
            });
            const t_inst = process.hrtime.bigint();
            WebAssembly.instantiate(wired, imports).then((r) => {
                hostRef.instantiate_ms =
                    Number(process.hrtime.bigint() - t_inst) / 1e6;
                success(r.instance, r.module);
            });
            return {};
        },
    });
    heapRef.mod = mod;
    return { mod, stdout, stderr, hostRef };
}

function jsonLine(lines) {
    const l = lines.filter((s) => s.startsWith("{"));
    return l.length ? JSON.parse(l[l.length - 1]) : null;
}

/* ------------------------------------------------------------------ */
/* modes                                                               */
/* ------------------------------------------------------------------ */

async function benchGuest() {
    const fixturePath = rest[0];
    if (!fixturePath) process.exit(2);
    const boundRef = { fx: null };
    const { mod, stdout, stderr, hostRef } = await loadGuest("qn_guest_bench.js", boundRef);
    boundRef.fx = { buf: readFileSync(fixturePath), pos: 0 };
    mod._bench_bind(1n);                       // opaque to the guest

    if (mode === "correct") {
        const rc = mod._bench_correct();
        const j = jsonLine(stdout);
        if (!j) { out({ ok: false, gate: "no-json", stderr: stderr.slice(-3) }); process.exit(1); }
        out({ ok: rc === 0 && j.status === "ok", rc, json: j });
        process.exit(rc === 0 && j.status === "ok" ? 0 : 1);
    }

    if (mode === "bench") {                     // Mode A
        const iters = Number(rest[1] ?? 3);
        const rc = mod._bench_bench(iters);
        const j = jsonLine(stdout);
        if (!j) { out({ ok: false, gate: "no-json", stderr: stderr.slice(-3) }); process.exit(1); }
        out({ ok: rc === 0, json: j });
        process.exit(rc === 0 ? 0 : 1);
    }

    if (mode === "lifecycle") {
        const rc = mod._bench_lifecycle();
        const j = jsonLine(stdout);
        if (!j || rc !== 0) { out({ ok: false, gate: "no-json-or-failed", stderr: stderr.slice(-3) }); process.exit(1); }
        out({ ok: true, mode: "lifecycle", guest: j, host: hostRef });
        process.exit(0);
    }

    if (mode === "pcm") {                       // Modes B / C
        const t0 = process.hrtime.bigint();
        const rc = mod._bench_pcm_prepare();
        const prepMs = Number(process.hrtime.bigint() - t0) / 1e6;
        const j = jsonLine(stdout);
        if (rc !== 0 || !j) { out({ ok: false, gate: "no-json-or-failed", prepMs, stderr: stderr.slice(-3) }); process.exit(1); }
        const len = mod._bench_pcm_len();
        const ptr = mod._bench_pcm_ptr();
        const pagesAfter = mod._bench_mem_pages();

        if (rest[1] === undefined) {            // Mode B: one bulk host read
            // Split the boundary cost honestly: Buffer.from(ArrayBuffer, off,
            // len) is a shared VIEW (no copy); the explicit copy and the
            // SHA-256 consumer are timed separately so EM's numbers compare
            // with the C runners' memcpy-based Mode B.
            const t_v = process.hrtime.bigint();
            const pcm = Buffer.from(mod.HEAPU8.buffer, ptr, len);
            const viewMs = Number(process.hrtime.bigint() - t_v) / 1e6;
            const t_c = process.hrtime.bigint();
            const pcmCopy = Buffer.from(pcm);   // explicit copy (new buffer)
            const copyMs = Number(process.hrtime.bigint() - t_c) / 1e6;
            const t_h = process.hrtime.bigint();
            const sha = createHash("sha256").update(pcmCopy).digest("hex");
            const hashMs = Number(process.hrtime.bigint() - t_h) / 1e6;
            out({ ok: true, mode: "B", prepMs, viewMs, copyMs, hashMs,
                  gbps: (len / 1e9) / (copyMs / 1e3), bytes: len, sha,
                  mem_pages_after: pagesAfter });
            process.exit(0);
        }
        // Mode C: chunked pull (chunk given in frames)
        const frames = Number(rest[1]);
        const ch = mod._bench_pcm_channels();
        const chunkBytes = frames * ch * 4;
        const stage = mod._bench_stage_alloc(chunkBytes);
        const calls = Math.ceil(len / chunkBytes);
        let total = 0;
        const t1 = process.hrtime.bigint();
        const callT = [];
        for (let i = 0; i < calls; i++) {
            const c0 = process.hrtime.bigint();
            const n = mod._bench_pcm_pull(stage, chunkBytes);
            callT.push(Number(process.hrtime.bigint() - c0) / 1e6);
            if (n <= 0) break;
            total += n;
        }
        const pullMs = Number(process.hrtime.bigint() - t1) / 1e6;
        const sorted = callT.slice().sort((a, b) => a - b);
        out({ ok: total === len, mode: "C", chunk_frames: frames,
              calls, total, bytes: len,
              pullMs, gbps: (total / 1e9) / (pullMs / 1e3),
              call_ms_median: sorted[Math.floor(sorted.length / 2)],
              call_ms_p999: sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.999))] });
        process.exit(total === len ? 0 : 1);
    }
    process.exit(2);
}

async function pbGuest() {
    const chunkBytes = Number(rest[0] ?? 65536);
    const iters = Number(rest[1] ?? 200);
    const boundRef = { fx: null };              // pb guest does not use the door
    const { mod } = await loadGuest("qn_pb_guest.js", boundRef, false);
    mod._pb_fill(0x5a);
    const src = mod._pb_ptr(), len = mod._pb_len();
    const stage = mod._pb_stage_alloc(chunkBytes);
    let bytes = 0;
    const t0 = process.hrtime.bigint();
    for (let i = 0; i < iters; i++) {
        let off = 0;
        while (off < len) {
            const n = mod._pb_pull(stage + off, Math.min(chunkBytes, len - off));
            if (n <= 0) break;
            off += n;
        }
        bytes += off;
    }
    const ms = Number(process.hrtime.bigint() - t0) / 1e6;
    const digest = createHash("sha256")
        .update(Buffer.from(mod.HEAPU8.buffer, src, len)).digest("hex");
    out({ ok: true, chunkBytes, iters, total_bytes: bytes, ms,
          gbps: (bytes / 1e9) / (ms / 1e3), pattern_sha: digest });
    process.exit(0);
}

if (guest === "bench") await benchGuest();
else if (guest === "pb") await pbGuest();
else { console.error(`unknown guest ${guest}`); process.exit(2); }
