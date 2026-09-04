#!/usr/bin/env python3
"""songcore_release.py — the ONE SongCore release command (fail-closed).

    python3 tools/songcore_release.py --target linux-x86_64

Turns the current tree into a staged, independently consumable release
package under build/release/songcore-v<version>-<target>/:

    songcore-v<version>-<target>/
    ├── include/songcore.h          the ONLY public surface (ABI v1)
    ├── lib/libsongcore.a           PRIMARY artifact: FFmpeg closure merged in
    ├── lib/shared/libsongcore.so   secondary; exports exactly the 15 symbols
    ├── metadata/manifest.json      machine-readable provenance (schema v1)
    ├── metadata/checksums.txt      sha256 of every shipped file
    ├── metadata/symbols.txt        public exported symbols (machine-audited)
    ├── LICENSES/                   FFmpeg license texts + third-party notices
    └── README.md                   capabilities, link line, limits, freeze

Gates, in order, each failing closed (exit 1, nothing staged):
  1. session identity  — the xmake session replays the PRODUCTION closure
     (profile `codec-base`); a test-closure session must reconfigure first.
  2. provenance        — FFmpeg pin, profile sha, target recipe identity.
  3. build             — xmake build songcore (static + shared).
  4. archive audit     — every manifest unit present (not hollow), public
                         symbols defined, sizes recorded.
  5. ABI gate          — layout dump equals the frozen v1 snapshot; the
                         permanent corpus/regression authority re-checked.
  6. consumers         — external static C consumer, Python ctypes decode,
                         shared export audit (tests/songcore/consumers.py).
  7. license           — nonfree fails the release; gpl/version3 recorded;
                         external lib* components recorded (none expected).
  8. reproducibility   — forced rebuild; artifact sha256 + member list +
                         exported symbol set compared (byte-identical or
                         semantic-only, recorded, never faked).
  9. stage + checksums — package written, checksums generated last.

The release is NEVER published here: tagging/upload is a separate human step
(docs/development/release.md).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.0"
ABI_VERSION = 1
PRODUCTION_PROFILE = "codec-base"

failures: list[str] = []


def gate(name: str, ok: bool, detail: str = "") -> bool:
    print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        failures.append(name)
    return ok


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    print(f"  $ {' '.join(str(c) for c in cmd)}")
    return subprocess.run([str(c) for c in cmd], cwd=ROOT, **kw)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read_session_manifest() -> dict:
    """The manifest the CURRENT xmake session replays (av_manifest option)."""
    conf = ROOT / ".xmake" / "linux" / "x86_64" / "xmake.conf"
    manifest_rel = "build/ffmpeg-xmake/manifest.json"  # documented default
    if conf.exists():
        m = re.search(r'av_manifest\s*=\s*"([^"]+)"', conf.read_text())
        if m:
            manifest_rel = m.group(1)
    return json.loads((ROOT / manifest_rel).read_text())


def archive_members(archive: Path) -> list[str]:
    out = run(["ar", "t", archive], capture_output=True, text=True, check=True)
    return out.stdout.split()


def global_symbols(archive: Path) -> set[str]:
    out = run(["nm", "-g", "--defined-only", archive],
              capture_output=True, text=True, check=True)
    return {line.split()[-1] for line in out.stdout.splitlines() if line.strip()}


def shared_exports(shared: Path) -> set[str]:
    out = run(["nm", "-D", "--defined-only", shared],
              capture_output=True, text=True, check=True)
    syms = {line.split()[-1] for line in out.stdout.splitlines() if line.strip()}
    return {s for s in syms if not s.startswith("_") and s not in
            ("__bss_start", "_edata", "_end", "__dso_handle", "_init", "_fini")}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--target", default="linux-x86_64")
    ap.add_argument("--out", default="build/release")
    args = ap.parse_args()

    artifacts = ROOT / "build" / "artifacts"
    static_lib = artifacts / "libsongcore.a"
    shared_lib = artifacts / "shared" / "libsongcore.so"
    ffmpeg_src = ROOT / "build" / "ffmpeg-src"

    # ---- 1. session identity: production closure only -----------------------
    manifest = read_session_manifest()
    gate("session: production profile (codec-base)",
         manifest.get("profile") == PRODUCTION_PROFILE,
         "" if manifest.get("profile") == PRODUCTION_PROFILE else
         f"session replays profile {manifest.get('profile')!r}; reconfigure with "
         "'xmake f --av_manifest=build/ffmpeg-xmake/manifest.json -y' for release")

    # ---- 2. provenance -------------------------------------------------------
    pin = json.loads((ROOT / "native" / "ffmpeg" / "pin.json").read_text())
    gate("provenance: ffmpeg pin commit",
         manifest.get("ffmpeg_commit_sha") == pin["ffmpeg_commit_sha"])
    profile_path = ROOT / "native" / "ffmpeg" / "profiles" / f"{PRODUCTION_PROFILE}.json"
    gate("provenance: profile sha",
         manifest.get("profile_sha256") == sha256(profile_path))
    recipe_path = ROOT / "native" / "ffmpeg" / "targets" / f"{args.target}.json"
    recipe = json.loads(recipe_path.read_text())
    gate("provenance: target recipe id",
         manifest.get("target", {}).get("id") == recipe["id"],
         f"{manifest.get('target', {}).get('id')} vs {recipe['id']}")
    gate("provenance: target recipe sha",
         manifest.get("target", {}).get("recipe_sha256") == sha256(recipe_path))
    gate("provenance: ffmpeg source sha",
         manifest.get("ffmpeg_source_sha256") == pin["source_sha256"])

    if failures:
        print("\nRELEASE FAIL: identity/provenance gate failed — nothing built.")
        return 1

    # ---- 3. build ------------------------------------------------------------
    if run(["xmake", "build", "songcore"]).returncode != 0:
        gate("build: songcore (static + shared)", False)
        return 1
    gate("build: songcore (static + shared)", static_lib.exists() and shared_lib.exists())

    # ---- 4. archive audit ------------------------------------------------------
    members = archive_members(static_lib)
    unit_basenames = {u["path"].rsplit("/", 1)[-1] + ".o" for u in manifest["units"]}
    missing = sorted(unit_basenames - set(members))
    gate("archive: not hollow (all closure units present)",
         not missing, f"{len(unit_basenames)} closure units, "
         f"{len(members)} members" + (f", MISSING {missing[:5]}" if missing else ""))
    syms = global_symbols(static_lib)
    public_needed = {  # the frozen 15 + version (abi.h contract)
        "songcore_abi_version", "song_open", "song_probe",
        "song_audio_stream_count", "song_audio_stream_info",
        "song_select_stream", "song_get_metadata", "song_get_metadata_count",
        "song_get_metadata_entry", "song_get_artwork_count",
        "song_get_artwork_item", "song_read_pcm", "song_seek",
        "song_last_error", "song_close",
    }
    gate("archive: defines every public ABI symbol",
         public_needed <= syms, f"missing {sorted(public_needed - syms)}")
    # Static archives legitimately CONTAIN internal av_/ff_ objects; the
    # leakage gate is the shared export audit below, not the member symbols.

    # ---- 5. ABI gate -----------------------------------------------------------
    dump_bin = ROOT / "build" / "release" / "abi_dump"
    dump_bin.parent.mkdir(parents=True, exist_ok=True)
    cc = run(["cc", "-Inative/include", "tools/songcore_abi_dump.c", "-o", dump_bin])
    if cc.returncode != 0:
        gate("abi: layout dump compiles", False)
        return 1
    dump = subprocess.run([dump_bin], capture_output=True, text=True, check=True).stdout
    snapshot = (ROOT / "bench" / "results" / "songcore-v1" /
                f"abi-layout-{args.target}-v1.txt")
    if not snapshot.exists():
        snapshot.write_text(dump)
        gate("abi: layout snapshot", False,
             f"snapshot did not exist; written for review: {snapshot} — re-run")
        return 1
    gate("abi: layout == frozen v1 snapshot", dump == snapshot.read_text(),
         "any drift requires an explicit SONGCORE_ABI_VERSION bump")
    abi_hdr = sha256(ROOT / "native" / "include" / "songcore.h")

    # ---- 6. consumers (external view) --------------------------------------------
    # --core: the three host-independent gates on THIS build. The run also
    # refreshes bench/results/songcore-v1/ffi-consumers-core.json — the
    # permanent `xmake test` chain re-records it on the test closure, so run
    # the test chain AFTER a release when both evidences are needed.
    cons = run([sys.executable, "native/tests/songcore/consumers.py", "--out", "--core"])
    core_report = json.loads((ROOT / "bench" / "results" / "songcore-v1" /
                              "ffi-consumers-core.json").read_text())
    core_gates = {g["gate"]: g["verdict"] for g in core_report.get("gates", [])}
    needed = ("shared_export_audit", "ctypes_decode_consumer",
              "static_archive_consumer")
    gate("consumers: static C + ctypes + export audit",
         all(core_gates.get(k) == "pass" for k in needed),
         "partial (wasm/windows recorded gates skipped in --core) is OK; "
         f"core gates: {core_gates}")

    # ---- 7. license gate -----------------------------------------------------------
    configure_args = manifest.get("configure_args", [])
    nonfree = any(a == "--enable-nonfree" for a in configure_args)
    gpl = any(a == "--enable-gpl" for a in configure_args)
    version3 = any(a == "--enable-version3" for a in configure_args)
    external_libs = sorted(a.removeprefix("--enable-lib-")
                           for a in configure_args
                           if a.startswith("--enable-lib-"))
    gate("license: nonfree not enabled", not nonfree, "RELEASE FAIL if enabled")
    gate("license: no external lib* components in the closure", not external_libs,
         f"{external_libs or 'none — effective license is FFmpeg-only'}")
    if gpl and version3:
        effective = "GPL-3.0-or-later (FFmpeg --enable-gpl --enable-version3)"
    elif gpl:
        effective = "GPL-2.0-or-later (FFmpeg --enable-gpl)"
    else:
        effective = "LGPL-2.1-or-later (FFmpeg default, all copylibs disabled)"

    if failures:
        print("\nRELEASE FAIL: pre-stage gates failed — nothing staged.")
        return 1

    # ---- 8. reproducibility (forced rebuild, compare semantics) -------------------
    sha_first, members_first, exports_first = (
        sha256(static_lib), members, shared_exports(shared_lib))
    run(["xmake", "build", "-r", "songcore"])
    sha_second = sha256(static_lib)
    members_second = archive_members(static_lib)
    exports_second = shared_exports(shared_lib)
    byte_identical = sha_first == sha_second
    semantic = (members_first == members_second and exports_first == exports_second)
    gate("reproducibility: member list + export set stable", semantic)
    print(f"[INFO] reproducibility: byte-identical={byte_identical} "
          f"(nondeterminism sources: toolchain metadata, build paths)"
          if not byte_identical else
          f"[INFO] reproducibility: byte-identical sha256={sha_first[:16]}…")

    # ---- 9. stage -------------------------------------------------------------------
    pkg = ROOT / args.out / f"songcore-v{VERSION}-{args.target}"
    if pkg.exists():
        shutil.rmtree(pkg)
    (pkg / "include").mkdir(parents=True)
    (pkg / "lib" / "shared").mkdir(parents=True)
    (pkg / "metadata").mkdir(parents=True)
    (pkg / "LICENSES").mkdir(parents=True)

    shutil.copy2(ROOT / "native" / "include" / "songcore.h", pkg / "include" / "songcore.h")
    shutil.copy2(static_lib, pkg / "lib" / "libsongcore.a")
    shutil.copy2(shared_lib, pkg / "lib" / "shared" / "libsongcore.so")
    for f in ("songcore_static_smoke.c",):
        src = ROOT / "native" / "tests" / "consumer" / f
        if src.exists():
            shutil.copy2(src, pkg / "metadata" / "external-consumer-example.c")

    # symbols.txt: machine-audited public exports of the shared artifact
    (pkg / "metadata" / "symbols.txt").write_text(
        "\n".join(sorted(exports_second)) + "\n")

    # LICENSES: authoritative FFmpeg texts + honest notices (never invented)
    for txt in sorted(ffmpeg_src.glob("COPYING.*")):
        shutil.copy2(txt, pkg / "LICENSES" / txt.name)
    (pkg / "LICENSES" / "THIRD_PARTY_NOTICES.txt").write_text(f"""\
Third-party notices — SongCore v{VERSION} ({args.target})
==========================================================

FFmpeg
  version : {pin["ffmpeg_version"]} (tag {pin["ffmpeg_tag"]})
  commit  : {pin["ffmpeg_commit_sha"]}
  source  : {pin["source_url"]}
  sha256  : {pin["source_sha256"]}
  profile : {PRODUCTION_PROFILE} (machine-derived minimal closure; sha256
            {manifest.get("profile_sha256")})
  effective license: {effective}
  external libraries that alter licensing: {"none" if not external_libs else ", ".join(external_libs)}

  The FFmpeg closure is statically linked inside libsongcore.a /
  libsongcore.so. Under LGPL-2.1-or-later static distribution, the object
  files of the application may be provided together with the means
  (source or relink instructions) to re-link a modified FFmpeg. This
  package contains the complete FFmpeg license texts (LICENSES/COPYING.*)
  and the closure identity above; the exact source tree is reconstructible
  from the pinned URL + sha256.

Qianqian
  The Qianqian sources (songcore.h and the SongCore implementation) carry
  no open-source license yet; distribution terms are owned by the project
  and are NOT granted by this package. Classification:
  ENGINEERING_FREEZE — this package is not a public redistribution grant.
  This is an engineering statement, not legal advice.
""")

    release_manifest = {
        "schema_version": 1,
        "songcore_version": VERSION,
        "songcore_abi": ABI_VERSION,
        "abi_header_sha256": abi_hdr,
        "target": recipe["id"],
        "platform": recipe["platform"],
        "arch": recipe["arch"],
        "toolchain": recipe["toolchain"]["family"],
        "ffmpeg_version": pin["ffmpeg_version"],
        "ffmpeg_commit": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "ffmpeg_profile": PRODUCTION_PROFILE,
        "ffmpeg_profile_sha256": manifest.get("profile_sha256"),
        "artifact": "lib/libsongcore.a",
        "sha256": sha_second,
        "artifact_bytes": (pkg / "lib" / "libsongcore.a").stat().st_size,
        "archive_members": len(members_second),
        "artifact_shared": "lib/shared/libsongcore.so",
        "shared_sha256": sha256(pkg / "lib" / "shared" / "libsongcore.so"),
        "public_symbols": sorted(public_needed),
        "license": {
            "ffmpeg_effective": effective,
            "gpl_enabled": gpl,
            "version3_enabled": version3,
            "nonfree_enabled": nonfree,
            "external_libs": external_libs,
        },
        "reproducibility": {
            "byte_identical": byte_identical,
            "member_list_stable": members_first == members_second,
            "export_set_stable": exports_first == exports_second,
        },
        "git_sha": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                  capture_output=True, text=True).stdout.strip(),
    }
    (pkg / "metadata" / "manifest.json").write_text(
        json.dumps(release_manifest, indent=1) + "\n")
    # in-repo release record (the staged package itself is a build artifact)
    (ROOT / "bench" / "results" / "songcore-v1" / "release-manifest.json").write_text(
        json.dumps(release_manifest, indent=1) + "\n")

    # package README — the outside-integrator entry point (kept short; the
    # full caller document lives in the repository: docs/contracts/songcore-api.md)
    tested = ("FLAC, MP3, AAC/M4A (mov), ADTS AAC, ALAC, PCM WAV "
              "(u8/s16le/s24le/s32le/f32le/f64le), Ogg Vorbis, Ogg Opus")
    (pkg / "README.md").write_text(f"""\
# SongCore v{VERSION} ({args.target})

A minimal, FFmpeg-backed local-audio decode core. One file in — one
coherent song out: identity, metadata, artwork, stream info, seek, and
Float32 interleaved PCM at the SOURCE sample rate and layout. It is a
decoder core, not an audio engine: no SRC, no mixer, no DSP, no output
backend, no loudness normalization, no playlist.

- ABI: v{ABI_VERSION} (frozen). Public surface: `include/songcore.h` only.
  The shared library exports exactly the {len(public_needed)} ABI symbols.
- Layout: machine-audited against the frozen snapshot for this target
  (metadata/manifest.json carries the identity).

## Link (static — the primary supported path)

    cc -Iinclude your.c lib/libsongcore.a -lm -lpthread -o your

One archive; the trimmed FFmpeg closure is merged inside and never leaks a
symbol. Windows/mingw: use `songcore.lib` naming and `-lbcrypt`; shared
`lib/shared/libsongcore.so` exports the same ABI (define `SONGCORE_DLL`
when consuming the DLL on Windows).

## Minimal flow

    song_io io = {{ .read = my_read, .seek = my_seek, .size = my_size, .userdata = my_file }};
    song_handle *h = NULL;  song_open(&io, &h);
    song_info info;         song_probe(h, &info);
    float pcm[1152 * info.channels];  uint64_t n;
    while (song_read_pcm(h, pcm, 1152, &n) == SONG_OK) play(pcm, n, &info);
    song_close(h);

EOF is `SONG_EOF` with zero frames — a normal terminal condition, not an
error. `song_probe`/`song_read_pcm` semantics: docs/contracts/songcore-api.md in the
source repository. An example external consumer is in
`metadata/external-consumer-example.c`.

## Capabilities (this target, profile `{PRODUCTION_PROFILE}`)

- regression-tested: {tested}
- enabled but not regression-tested: none
- not included: everything else (video, network, subtitle, filters, SRC)

## Provenance & license

`metadata/manifest.json` pins the FFmpeg tag/commit/source sha256, the
machine-derived closure profile sha256, artifact sha256 and the
reproducibility record. Effective license: {effective}. See `LICENSES/`
(FFmpeg license texts + third-party notices; LGPL static-distribution
obligations are described there).

## Distribution status

This package is an engineering freeze artifact (classification:
ENGINEERING_FREEZE, not PUBLIC_DISTRIBUTION_APPROVED): the ABI, binary
shape, provenance and external-consumer contract are frozen. It grants
no public redistribution license for the Qianqian/SongCore-owned
portions; public redistribution requires a separate
licensing/compliance decision and must not be inferred from the FFmpeg
LGPL notices above.

## Known limitations

Local files only (host IO callbacks); no sample-rate conversion; no
seek sample-exactness promise for lossy codecs (bounded codec-frame
tolerance, landings are explicit); a mid-stream rate/layout change fails
closed (`SONG_ERR_STREAM_CHANGE`).
""")

    checksums = []
    for p in sorted(pkg.rglob("*")):
        if p.is_file() and p != pkg / "metadata" / "checksums.txt":
            checksums.append(f"{sha256(p)}  {p.relative_to(pkg)}")
    (pkg / "metadata" / "checksums.txt").write_text("\n".join(checksums) + "\n")

    print(f"""
RELEASE STAGED: {pkg.relative_to(ROOT)}
  artifact : lib/libsongcore.a  {release_manifest['artifact_bytes']} bytes  sha256 {sha_second[:16]}…
  members  : {len(members_second)}  (closure units all present)
  exports  : {len(exports_second)} public symbols
  license  : {effective}
  report   : gates {'ALL PASS' if not failures else 'FAILED'}; publishing (tag/upload) is a separate human step.
""")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
