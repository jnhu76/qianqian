/*
 * tools/wasm_em_node_harness.mjs — E09 Emscripten variant harness (Node).
 *
 * Drives the same guest modules the C runtime ladder drives, through the
 * same three contracts, so Emscripten numbers are measured identically:
 *
 *   guest=bench     bench_bind / bench_correct / bench_bench /
 *                   bench_pcm_prepare / bench_pcm_ptr|len / bench_pcm_pull
 *                   (Modes A/B/C; JSON arrives on guest stdout)
 *   guest=songcore  song_wasm_open / probe / read_pcm / seek / close
 *                   (the shipping-shape contract, door wired from JS)
 *   guest=pb        pb_fill / pb_ptr / pb_pull (§14 PCM-copy microbench)
 *
 * Host-owned IO is preserved: the guest gets an opaque BigInt handle and
 * three functions; the fixture file never enters the guest heap.
 *
 * Usage: node tools/wasm_em_node_harness.mjs <guest> <mode> <fixture> [args]
 *   bench  correct  <fixture>
 *   bench  bench    <fixture> [iterations=3]
 *   bench  pcm      <fixture> [chunk_frames]   (Mode B if chunk_frames omitted)
 *   songcore probe  <fixture>
 *   songcore decode <fixture>
 *   pb     pull     <chunk_bytes> <iters>
 *
 * Requires the Emscripten artifacts next to the built .js glue:
 *   build/artifacts/wasm/{qn_guest_bench.js+qn_guest_bench.wasm,
 *                         SongCore.js+SongCore.wasm, qn_pb_guest.js+qn_pb_guest.wasm}
 * Run with cwd = repo root.
 */

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const require_ = createRequire(import.meta.url);
const ART = "build/artifacts/wasm";

const [guest, mode, ...rest] = process.argv.slice(2);
if (!guest || !mode) {
    console.error("usage: node tools/wasm_em_node_harness.mjs <guest> <mode> ...");
    process.exit(2);
}

function out(obj) { console.log(JSON.stringify(obj)); }

/* ------------------------------------------------------------------ */
/* the door                                                            */
/* ------------------------------------------------------------------ */

function makeDoor(mod) {
    let bound = null;                       // { buf: Buffer, pos: number }
    const host = {
        // i64 params/results are BigInt (modern emscripten = WASM_BIGINT)
        qn_host_read: (handle, dst, len) => {
            if (!bound) return -1n;
            if (bound.pos >= bound.buf.length) return 0n;
            const n = Math.min(bound.buf.length - bound.pos, len);
            bound.buf.copy(mod.HEAPU8, dst, bound.pos, bound.pos + n);
            bound.pos += n;
            return BigInt(n);
        },
        qn_host_seek: (handle, off) => {
            if (!bound) return -1n;
            if (off < 0n || off > BigInt(bound.buf.length)) return -1n;
            bound.pos = Number(off);
            return off;
        },
        qn_host_size: () => BigInt(bound ? bound.buf.length : 0),
    };
    return {
        host,
        bind(path) { bound = { buf: readFileSync(path), pos: 0 }; },
    };
}

async function loadModule(jsName, doorHost, stdoutLines, stderrLines) {
    const factory = require_(`${process.cwd()}/${ART}/${jsName}`);
    const mod = await factory({
        print: (s) => stdoutLines.push(s),
        printErr: (s) => stderrLines.push(s),
        noInitialRun: true,
        instantiateWasm: (imports, success) => {
            imports["qianqian_host"] = doorHost;
            WebAssembly.instantiateStreaming(
                Promise.resolve({
                    arrayBuffer: () =>
                        readFileSync(`${process.cwd()}/${ART}/${jsName.replace(/\.js$/, ".wasm")}`)
                            .buffer,
                }),
                imports
            ).then((r) => success(r.instance, r.module));
            return {};  // async path; emscripten awaits the success callback
        },
    });
    return mod;
}

function jsonLine(lines) {
    const l = lines.filter((s) => s.startsWith("{"));
    return l.length ? JSON.parse(l[l.length - 1]) : null;
}

function observable(j) {
    const seeks = j.seeks === undefined ? null : j.seeks.map((s) => ({
        pos_us: s.pos_us, actual_us: s.actual_us,
        delta_us: Math.round((s.actual_us - s.pos_us) * 10) / 10,
    }));
    return {
        status: j.status, container: j.container, codec: j.codec,
        rate: j.rate, channels: j.channels, duration_us: j.duration_us,
        metadata: j.metadata === undefined ? null : j.metadata,
        artwork_sha: j.artwork_sha === undefined ? null : j.artwork_sha,
        decode: j.decode === undefined ? null : j.decode,
        eof: j.eof === undefined ? null : j.eof,
        suffix: j.suffix === undefined ? null : j.suffix,
        seeks,
    };
}

/* ------------------------------------------------------------------ */
/* guests                                                              */
/* ------------------------------------------------------------------ */

/* single-load variant: door must exist before instantiation, so build the
 * module and the door together */
async function loadBench() {
    const stdout = [], stderr = [];
    let modRef = null;
    const boundRef = { fx: null };
    const heap = () => modRef.HEAPU8;
    const host = {
        qn_host_read: (handle, dst, len) => {
            const fx = boundRef.fx;
            if (!fx) return -1n;
            if (fx.pos >= fx.buf.length) return 0n;
            const n = Math.min(fx.buf.length - fx.pos, len);
            fx.buf.copy(heap(), dst, fx.pos, fx.pos + n);
            fx.pos += n;
            return BigInt(n);
        },
        qn_host_seek: (handle, off) => {
            const fx = boundRef.fx;
            if (!fx) return -1n;
            if (off < 0n || off > BigInt(fx.buf.length)) return -1n;
            fx.pos = Number(off);
            return off;
        },
        qn_host_size: () => BigInt(boundRef.fx ? boundRef.fx.buf.length : 0),
    };
    const factory = require_(`${process.cwd()}/${ART}/qn_guest_bench.js`);
    modRef = await factory({
        print: (s) => stdout.push(s),
        printErr: (s) => stderr.push(s),
        noInitialRun: true,
        instantiateWasm: (imports, success) => {
            imports["qianqian_host"] = host;
            const wasmPath = `${process.cwd()}/${ART}/qn_guest_bench.wasm`;
            const bytes = readFileSync(wasmPath);
            WebAssembly.instantiate(bytes, imports).then((r) =>
                success(r.instance, r.module));
            return {};
        },
    });
    return { mod: modRef, boundRef, stdout, stderr };
}

async function main() {
    if (guest === "bench") {
        const fixturePath = rest[0];
        if (!fixturePath) process.exit(2);
        const { mod, boundRef, stdout, stderr } = await loadBench();
        boundRef.fx = { buf: readFileSync(fixturePath), pos: 0 };
        const H = 1n;                       // opaque to the guest
        mod._bench_bind(H);

        if (sub === "correct") {
            const rc = mod._bench_correct();
            const j = jsonLine(stdout);
            if (!j) { out({ ok: false, gate: "no-json", stderr: stderr.slice(-3) }); process.exit(1); }
            out({ ok: rc === 0 && j.status === "ok", rc, json: j, observable: observable(j) });
            process.exit(rc === 0 && j.status === "ok" ? 0 : 1);
        }

        if (sub === "bench") {
            const iters = Number(rest[1] ?? 3);
            const rc = mod._bench_bench(iters);
            const j = jsonLine(stdout);
            if (!j) { out({ ok: false, gate: "no-json", stderr: stderr.slice(-3) }); process.exit(1); }
            out({ ok: rc === 0, json: j });
            process.exit(rc === 0 ? 0 : 1);
        }

        if (sub === "pcm") {
            const t0 = process.hrtime.bigint();
            const rc = mod._bench_pcm_prepare();
            const prepMs = Number(process.hrtime.bigint() - t0) / 1e6;
            const j = jsonLine(stdout);
            if (rc !== 0 || !j) { out({ ok: false, gate: "no-json-or-failed", prepMs, stderr: stderr.slice(-3) }); process.exit(1); }
            const len = mod._bench_pcm_len();
            const ptr = mod._bench_pcm_ptr();
            const pagesBefore = mod._bench_mem_pages();

            if (rest[1] === undefined) {
                // Mode B: one host-side bulk read of guest linear memory
                const t1 = process.hrtime.bigint();
                const pcm = Buffer.from(heap().buffer, ptr, len);
                const sha = createHash("sha256").update(pcm).digest("hex");
                const copyMs = Number(process.hrtime.bigint() - t1) / 1e6;
                out({ ok: true, mode: "B", prepMs, copyMs,
                      gbps: (len / 1e9) / (copyMs / 1e3), bytes: len, sha,
                      mem_pages_after: pagesBefore });
                process.exit(0);
            }
            // Mode C: chunked pull (chunk given in frames)
            const frames = Number(rest[1]);
            const ch = mod._bench_pcm_channels(), rate = mod._bench_pcm_rate();
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

    if (guest === "pb") {
        const chunkBytes = Number(rest[0] ?? 65536);
        const iters = Number(rest[1] ?? 200);
        const stdout = [], stderr = [];
        const factory = require_(`${process.cwd()}/${ART}/qn_pb_guest.js`);
        const mod = await factory({
            print: (s) => stdout.push(s), printErr: (s) => stderr.push(s),
            noInitialRun: true, instantiateWasm: undefined,
        });
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

    if (guest === "songcore") {
        const fixturePath = rest[0];
        if (!fixturePath) process.exit(2);
        const stdout = [], stderr = [];
        const boundRef = { fx: null };
        let modRef = null;
        const host = {
            qn_host_read: (h, dst, len) => {
                const fx = boundRef.fx; if (!fx) return -1n;
                if (fx.pos >= fx.buf.length) return 0n;
                const n = Math.min(fx.buf.length - fx.pos, len);
                fx.buf.copy(modRef.HEAPU8, dst, fx.pos, fx.pos + n);
                fx.pos += n; return BigInt(n);
            },
            qn_host_seek: (h, off) => {
                const fx = boundRef.fx; if (!fx) return -1n;
                if (off < 0n || off > BigInt(fx.buf.length)) return -1n;
                fx.pos = Number(off); return off;
            },
            qn_host_size: () => BigInt(boundRef.fx ? boundRef.fx.buf.length : 0),
        };
        const factory = require_(`${process.cwd()}/${ART}/SongCore.js`);
        modRef = await factory({
            print: (s) => stdout.push(s), printErr: (s) => stderr.push(s),
            noInitialRun: true,
            instantiateWasm: (imports, success) => {
                imports["qianqian_host"] = host;
                const bytes = readFileSync(`${process.cwd()}/${ART}/SongCore.wasm`);
                WebAssembly.instantiate(bytes, imports).then((r) => success(r.instance, r.module));
                return {};
            },
        });
        boundRef.fx = { buf: readFileSync(fixturePath), pos: 0 };
        const H = 1n;

        if (sub === "probe") {
            const infoPtr = modRef._malloc(256);
            const h = modRef._song_wasm_open(H);
            const rc = h ? modRef._song_wasm_probe(h, infoPtr) : -1;
            out({ ok: rc === 0, open_handle: h !== 0, rc });
            if (h) modRef._song_wasm_close(h);
            process.exit(rc === 0 ? 0 : 1);
        }
        if (sub === "decode") {
            // The song_info layout is contract-private to SongCore; decode
            // proof for E09 stays on the bench guest (same SongCore source).
            out({ ok: false, note: "decode via guest=bench (bench guest is the SongCore-linked harness)" });
            process.exit(2);
        }
        process.exit(2);
    }

    console.error(`unknown guest ${guest}`);
    process.exit(2);
}

main().catch((e) => { console.error(e); process.exit(1); });
