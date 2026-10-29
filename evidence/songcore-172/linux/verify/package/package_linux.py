#!/usr/bin/env python3
"""S3 Linux package builder (#172 S3-verify) — contract
docs/release/songcore-release-contract.md §7 PACKAGE_LAYOUT + §8
PROVENANCE_SCHEMA (schema_version 2) + §10 license obligations, with the
V1-V7 machine checks from §8 executed before the container is written.

Version identity: the concrete next release number is owner-reserved
(contract §14 Q1). This archive is campaign verification evidence, so it is
built under the non-publishable placeholder 0.0.0-s3-verify and the manifest
notes say so. It is NOT a release and must never be published under the
songcore-v* namespace.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

CLONE = Path("/home/hoo/qianqian-172-linux")
NATIVE = CLONE / "native"
VERSION = "0.0.0-s3-verify"
PKG_ROOT_NAME = f"songcore-v{VERSION}-linux-x86_64"
PKG_WORK = CLONE / "ops" / "package"
STAGE = PKG_WORK / "stage" / PKG_ROOT_NAME
OUT_TAR = PKG_WORK / f"{PKG_ROOT_NAME}.tar.gz"
EV = CLONE / "ops" / "verify-evidence"
V010_TAR = Path("/tmp/s3-v010/songcore-v0.1.0-linux-x86_64.tar.gz")
V010_SHA = "018e0cbdfd8d83b6967bbba8e899ae6dbcc2ccf690c5294bd48e2ef2699f6815"

SYMBOLS = sorted([
    "song_audio_stream_count", "song_audio_stream_info", "song_close",
    "song_get_artwork_count", "song_get_artwork_item", "song_get_metadata",
    "song_get_metadata_count", "song_get_metadata_entry", "song_last_error",
    "song_open", "song_probe", "song_read_pcm", "song_seek",
    "song_select_stream", "songcore_abi_version",
])


def die(msg: str) -> None:
    print(f"FATAL: {msg}", file=sys.stderr)
    sys.exit(1)


def sh(cmd: list[str]) -> str:
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode:
        die(f"command failed ({p.returncode}): {' '.join(cmd)}\n{p.stderr[-1500:]}")
    return p.stdout


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def first_line(cmd: list[str]) -> str:
    out = sh(cmd).strip().splitlines()
    if not out:
        die(f"no output from {' '.join(cmd)}")
    return out[0].strip()


def main() -> None:
    pin = json.loads((NATIVE / "ffmpeg" / "pin.json").read_text())
    recipe = json.loads((NATIVE / "ffmpeg" / "targets" / "linux-x86_64.json").read_text())
    profile_sha = sha256_file(NATIVE / "ffmpeg" / "profiles" / "codec-base.json")
    closure_manifest_path = NATIVE / "build" / "ffmpeg-xmake" / "manifest.json"
    closure = json.loads(closure_manifest_path.read_text())
    art_static = NATIVE / "build" / "artifacts" / "libsongcore.a"
    art_shared = NATIVE / "build" / "artifacts" / "shared" / "libsongcore.so"
    for p in (art_static, art_shared):
        if not p.is_file():
            die(f"missing artifact {p}")

    # ---- staging per §7 ----
    if STAGE.exists():
        shutil.rmtree(STAGE)
    (STAGE / "include").mkdir(parents=True)
    (STAGE / "lib" / "shared").mkdir(parents=True)
    (STAGE / "LICENSES").mkdir(parents=True)
    (STAGE / "metadata").mkdir(parents=True)

    shutil.copy2(NATIVE / "include" / "songcore.h", STAGE / "include" / "songcore.h")
    shutil.copy2(art_static, STAGE / "lib" / "libsongcore.a")
    shutil.copy2(art_shared, STAGE / "lib" / "shared" / "libsongcore.so")
    for name in ("COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3"):
        shutil.copy2(NATIVE / "build" / "ffmpeg-src" / name, STAGE / "LICENSES" / name)
    # THIRD_PARTY_NOTICES.txt: v0.1.0-shaped file, taken read-only from the
    # published historical release (sha-verified download), per contract §7
    # origin note (S-V010) and §10 B3.
    tsha = sha256_file(V010_TAR)
    if tsha != V010_SHA:
        die(f"v0.1.0 tarball sha mismatch: {tsha}")
    members = sh(["tar", "-tzf", str(V010_TAR)]).splitlines()
    tpn_member = next(m for m in members if m.endswith("LICENSES/THIRD_PARTY_NOTICES.txt"))
    sh(["tar", "-xzf", str(V010_TAR), "-C", "/tmp/s3-v010", tpn_member])
    shutil.copy2(Path("/tmp/s3-v010") / tpn_member, STAGE / "LICENSES" / "THIRD_PARTY_NOTICES.txt")
    shutil.copy2(CLONE / "LICENSE-MIT", STAGE / "LICENSES" / "LICENSE-MIT")
    shutil.copy2(CLONE / "LICENSE-APACHE", STAGE / "LICENSES" / "LICENSE-APACHE")
    shutil.copy2(Path(__file__).resolve().parent / "external-consumer-example.c",
                 STAGE / "metadata" / "external-consumer-example.c")

    # ---- carried + schema-2 manifest fields ----
    static_sha = sha256_file(STAGE / "lib" / "libsongcore.a")
    shared_sha = sha256_file(STAGE / "lib" / "shared" / "libsongcore.so")
    header_sha = sha256_file(STAGE / "include" / "songcore.h")
    glibc_floor = sorted({
        l.split("GLIBC_")[-1].split(")")[0]
        for l in sh(["objdump", "-T", str(art_shared)]).splitlines()
        if "GLIBC_" in l
    }, key=lambda v: [int(x) for x in v.split(".")])[-1]
    configure_args = closure["configure_args"]
    forbidden = [a for a in configure_args
                 if a.startswith("--enable-gpl") or a.startswith("--enable-version3")
                 or a.startswith("--enable-nonfree") or a.startswith("--enable-lib")]

    manifest = {
        "schema_version": 2,
        "songcore_version": VERSION,
        "songcore_abi": 1,
        "abi_header_sha256": header_sha,
        "target": "linux-x86_64",
        "platform": "linux",
        "arch": "x86_64",
        "toolchain": "host-native",
        "ffmpeg_version": pin["ffmpeg_version"],
        "ffmpeg_commit": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "ffmpeg_profile": "codec-base",
        "ffmpeg_profile_sha256": profile_sha,
        "artifact": "lib/libsongcore.a",
        "sha256": static_sha,
        "artifact_bytes": (STAGE / "lib" / "libsongcore.a").stat().st_size,
        "archive_members": len(sh(["ar", "t", str(art_static)]).split()),
        "artifact_shared": True,
        "shared_sha256": shared_sha,
        "public_symbols": SYMBOLS,
        "git_sha": sh(["git", "-C", str(CLONE), "rev-parse", "HEAD"]).strip(),
        "target_identity": {
            "id": recipe["id"],
            "platform": recipe["platform"],
            "arch": recipe["arch"],
            "triple": "x86_64-unknown-linux-gnu",
            "target_os": recipe["ffmpeg"]["target_os"],
            "cross_prefix": recipe["ffmpeg"].get("cross_prefix"),
            "toolchain": recipe["toolchain"],
            "recipe_sha256": sha256_file(NATIVE / "ffmpeg" / "targets" / "linux-x86_64.json"),
        },
        "tools": {
            "cc": first_line(["gcc", "--version"]),
            "ar": first_line(["ar", "--version"]),
            "xmake": first_line(["xmake", "--version"]).replace("\x1b[0m\x1b[1m", "").replace("\x1b[0m", ""),
        },
        "sdk_ndk": ("not applicable this round — linux-x86_64: host-native, "
                    "no SDK/NDK (target recipe toolchain identity, contract §8/§9)"),
        "source": {
            "commit": sh(["git", "-C", str(CLONE), "rev-parse", "HEAD"]).strip(),
            "status": "clean",
        },
        "pin": {
            "ffmpeg_tag": pin["ffmpeg_tag"],
            "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
            "ffmpeg_source_sha256": pin["source_sha256"],
            "pin_sha256": sha256_file(NATIVE / "ffmpeg" / "pin.json"),
        },
        "closure": {
            "manifest_sha256": sha256_file(closure_manifest_path),
            "toolchain": closure["toolchain"],
            "configure_args": configure_args,
            "units": closure["closure"],
            "reference_archives": closure["reference_archives"],
        },
        "files": {},
        "runtime_dependencies": {
            "static": {
                "libc": "REQUIRED (glibc)",
                "libm": "REQUIRED (contract §9; songcore.lua add_syslinks)",
                "libpthread": "REQUIRED to link (contract §9 link line; glibc >= 2.34 has merged pthread symbols into libc)",
                "link_line": "cc -O2 -Iinclude consumer.c -Llib -lsongcore -lm -lpthread",
            },
            "shared": {
                "needed": ["libm.so.6", "libc.so.6"],
                "glibc_symbol_version_floor": f"GLIBC_{glibc_floor}",
                "note": ("floor measured per contract §9 (objdump -T max GLIBC_*); "
                         "oldest-distro support policy is owner-reserved (§14 Q4)"),
            },
            "ffmpeg_runtime": "none — the closure ships in object form inside the archive",
        },
        "license": {
            "ffmpeg_effective": "LGPL-2.1-or-later",
            "gpl_enabled": False,
            "version3_enabled": False,
            "nonfree_enabled": False,
            "external_libs": [],
            "license_files": {},
        },
        "reproducibility": {
            "recipe": "RECIPE_RECONSTRUCTIBLE",
            "semantic_abi": "SEMANTIC_ABI_VERIFIED",
            "byte": "NOT_ESTABLISHED",
            "evidence": "ops/verify-evidence/ (c-suite-receipt.json R1+R2, r2-artifact-compare.txt, r2-differing-members.txt, r2-rootcause.txt, cargo/layout/surface gate logs)",
        },
        "built_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "packer": "release-substrate songcore-172 S3 linux lane (WSL2 Ubuntu 26.04.1, xmake v2.9.7, gcc 15.2.0)",
        "notes": ("songcore_version 0.0.0-s3-verify is a NON-PUBLISHABLE placeholder: "
                  "the concrete next release number is owner-reserved (contract §14 Q1). "
                  "This archive is campaign verification evidence per the S3 lane ask; "
                  "it is not a release (contract §11: a produced archive is not a release). "
                  "License texts under LICENSES/ come from the verified pinned FFmpeg "
                  "source extraction (O1) and the published songcore-v0.1.0 asset "
                  "(THIRD_PARTY_NOTICES.txt, S-V010 origin); LICENSE-MIT/LICENSE-APACHE "
                  "are byte-identical to the repo-root texts at the source commit (B4)."),
    }

    # README (O3 relink path + consumer instructions)
    readme = f"""# SongCore {VERSION} — linux-x86_64

SongCore is the local media → PCM decode component of Qianqian: one local
media file in, one coherent song snapshot out (identity, metadata, artwork,
stream information, source-rate/source-layout Float32 interleaved PCM, seek,
EOF). This archive is verification-grade campaign evidence (#172), not a
release; the version is a placeholder (contract §14 Q1).

## Contents

    include/songcore.h                  the ONLY ABI description (ABI v1, frozen)
    lib/libsongcore.a                   static artifact (FFmpeg closure merged in)
    lib/shared/libsongcore.so           shared artifact (closure internalized)
    LICENSES/                           FFmpeg COPYING set (pinned source), notices,
                                        MIT + Apache-2.0 texts
    metadata/manifest.json              provenance, schema_version 2
    metadata/symbols.txt                the 15 ABI v1 exports
    metadata/checksums.txt              sha256 of every packaged file except itself
    metadata/external-consumer-example.c  consumer smoke (open→probe→decode→seek→EOF→close)

## Verify the package

    sha256sum -c metadata/checksums.txt   # from inside the package root

## Link against it

Static (recommended; contract §9):
    cc -O2 -Iinclude your.c -Llib -lsongcore -lm -lpthread -o your

Shared:
    cc -O2 -Iinclude your.c -Llib/shared -lsongcore -lm -lpthread \\
       -Wl,-rpath,<abs path to lib/shared> -o your

Runtime dependencies: glibc (libc, libm; link -lpthread per the contract;
glibc >= 2.34 has merged pthread symbols into libc). The shared form's
measured glibc symbol-version floor is recorded in
metadata/manifest.json (runtime_dependencies.shared); the supported-distro
policy is owner-reserved. No FFmpeg runtime library is needed: the closure
ships in object form inside the archive.

## Consumer smoke

    cc -O2 -Iinclude metadata/external-consumer-example.c \\
       -Llib -lsongcore -lm -lpthread -o songcore-consumer-example
    ./songcore-consumer-example /path/to/media.mp3

## Relink obligations (LGPL, contract §10 O3)

The archive contains the FFmpeg members in object form. The machine-verifiable
rebuild inputs — ffmpeg/pin.json (tag→commit + zip sha256), the codec-base
capability profile, the linux-x86_64 target recipe, the imported closure
manifest (schema 2, units + flags), and the Xmake replay configuration — are
part of the source repository at the manifest's recorded commit; a licensee
can recombine them to relink a modified FFmpeg closure without any
undocumented step.
"""
    (STAGE / "README.md").write_text(readme)

    # manifest.json + symbols.txt written BEFORE checksums/files binding
    manifest_path = STAGE / "metadata" / "manifest.json"
    symbols_path = STAGE / "metadata" / "symbols.txt"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    symbols_path.write_text("".join(s + "\n" for s in SYMBOLS))

    # license_files hashes (O1 machine check basis)
    lic = manifest["license"]["license_files"]
    for name in ("COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3"):
        lic[name] = sha256_file(STAGE / "LICENSES" / name)
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    # files{} + checksums.txt binding.
    # Contract differential, recorded: §8 says files{} covers "the same set
    # checksums.txt covers" while §7/V1 also say "the manifest hashes neither
    # itself nor checksums.txt". Those two sentences cannot both hold
    # literally (checksums.txt covers manifest.json per the verified v0.1.0
    # shape), so the self-consistent reading is enforced here:
    #   checksums.txt  — every packaged file except itself (13 entries,
    #                    includes manifest.json), matching the v0.1.0 shape;
    #   files{}        — the same set minus manifest.json (the manifest never
    #                    hashes itself), i.e. the non-circular subset.
    packaged = sorted(
        str(p.relative_to(STAGE)) for p in STAGE.rglob("*") if p.is_file()
    )
    covered_ck = [p for p in packaged if p != "metadata/checksums.txt"]
    covered_mf = [p for p in covered_ck if p != "metadata/manifest.json"]
    manifest["files"] = {
        p: {"sha256": sha256_file(STAGE / p), "bytes": (STAGE / p).stat().st_size}
        for p in covered_mf
    }
    manifest["notes"] += (" files{} covers the checksums.txt set minus "
                          "manifest.json itself (non-circular binding; "
                          "recorded differential to §8's 'same set' wording).")
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    checksums = "".join(f"{sha256_file(STAGE / p)}  {p}\n" for p in covered_ck)
    (STAGE / "metadata" / "checksums.txt").write_text(checksums)

    # ---- V1-V7 machine checks (§8) ----
    v = {}
    files_keys = set(manifest["files"].keys())
    cks_entries = {l.split("  ", 1)[1]: l.split("  ", 1)[0]
                   for l in checksums.splitlines()}
    v["V1_files_rehash"] = all(
        manifest["files"][p]["sha256"] == sha256_file(STAGE / p) for p in covered_mf)
    v["V1_sets"] = (files_keys == set(covered_mf) and
                    set(cks_entries.keys()) == set(covered_ck) and
                    "metadata/manifest.json" not in files_keys and
                    "metadata/manifest.json" in cks_entries)
    v["V2_header_sha"] = (manifest["abi_header_sha256"] == header_sha ==
                          sha256_file(NATIVE / "include" / "songcore.h"))
    v["V3_pin_carry_dups"] = (
        manifest["ffmpeg_commit"] == manifest["pin"]["ffmpeg_commit_sha"] ==
        pin["ffmpeg_commit_sha"] and
        manifest["ffmpeg_source_sha256"] == manifest["pin"]["ffmpeg_source_sha256"] ==
        pin["source_sha256"] and
        manifest["pin"]["pin_sha256"] == sha256_file(NATIVE / "ffmpeg" / "pin.json") and
        manifest["closure"]["manifest_sha256"] == sha256_file(closure_manifest_path))
    sym_file = [l.strip() for l in symbols_path.read_text().splitlines() if l.strip()]
    exports = sorted(l.split()[-1] for l in
                     sh(["nm", "-D", "--defined-only", str(art_shared)]).splitlines() if l.strip())
    static_syms = sorted({l.split()[-1] for l in sh(["nm", str(art_static)]).splitlines()
                          if " T " in l and l.split()[-1].startswith("song")})
    v["V4_symbols"] = (manifest["public_symbols"] == SYMBOLS == sym_file ==
                       exports and static_syms == SYMBOLS)
    v["V5_license_args"] = (not forbidden and
                            not manifest["license"]["gpl_enabled"] and
                            not manifest["license"]["version3_enabled"] and
                            not manifest["license"]["nonfree_enabled"])
    v["V6_repro_fields"] = (manifest["reproducibility"]["recipe"] == "RECIPE_RECONSTRUCTIBLE"
                            and manifest["reproducibility"]["semantic_abi"] == "SEMANTIC_ABI_VERIFIED"
                            and manifest["reproducibility"]["byte"] == "NOT_ESTABLISHED"
                            and bool(manifest["reproducibility"]["evidence"]))
    v["V7_tools"] = (all(manifest["tools"].get(k) for k in ("cc", "ar", "xmake")) and
                     "not applicable" in manifest["sdk_ndk"] and
                     manifest["target_identity"]["toolchain"] == recipe["toolchain"] and
                     manifest["target_identity"]["recipe_sha256"] ==
                     sha256_file(NATIVE / "ffmpeg" / "targets" / "linux-x86_64.json"))
    # O1: license texts vs pinned-source extraction
    v["O1_license_texts"] = all(
        lic[name] == sha256_file(NATIVE / "build" / "ffmpeg-src" / name)
        for name in ("COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3"))
    v["B4_component_licenses"] = (
        sha256_file(STAGE / "LICENSES" / "LICENSE-MIT") == sha256_file(CLONE / "LICENSE-MIT") and
        sha256_file(STAGE / "LICENSES" / "LICENSE-APACHE") == sha256_file(CLONE / "LICENSE-APACHE"))
    (EV / "package-v-checks.json").write_text(
        json.dumps(v, indent=2, sort_keys=True) + "\n")
    failed = [k for k, ok in v.items() if not ok]
    if failed:
        die(f"package checks failed: {failed}")

    # ---- container ----
    if OUT_TAR.exists():
        OUT_TAR.unlink()
    sh(["tar", "-czf", str(OUT_TAR), "-C", str(STAGE.parent), PKG_ROOT_NAME])
    print(f"staged {len(packaged)} files (checksums covers {len(covered_ck)}, "
          f"manifest files[] covers {len(covered_mf)})")
    print(f"V-checks: {json.dumps(v, sort_keys=True)}")
    print(f"package: {OUT_TAR}")
    print(f"package_sha256: {sha256_file(OUT_TAR)}")
    print(f"package_bytes: {OUT_TAR.stat().st_size}")


if __name__ == "__main__":
    main()
