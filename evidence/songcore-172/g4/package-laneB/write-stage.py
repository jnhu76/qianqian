#!/usr/bin/env python3
"""Emit evidence/songcore-172/g4/package-laneB/stage.json with real hashes."""
import hashlib
import json
import os

REPO = "/mnt/c/Users/fred1/source/qianqian"
LANE = os.path.join(REPO, "evidence/songcore-172/g4/package-laneB")
PKG = os.path.join(REPO, "evidence/songcore-172/g4/package")


def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()


def entry(rel):
    p = os.path.join(LANE, rel)
    return {"path": "evidence/songcore-172/g4/package-laneB/" + rel, "sha256": sha(p), "bytes": os.path.getsize(p)}


files = [
    "package-linux.sh.branch-sha",
    "run-linux-package.sh",
    "linux-package.log",
    "verify.py",
    "verify-run.log",
    "laneB-verify.json",
    "laneB-verify.log",
    "collect-receipts.sh",
    "receipts.log",
    "extract-stages.sh",
    "extract-stages.log",
    "final-crosschecks.sh",
    "final-crosschecks.log",
    "win-package.stdout.log",
    "win-package.stderr.log",
]
evidence = [entry(f) for f in files]
extra = [
    "evidence/songcore-172/g4/package/win/songcore-v0.0.0-preflight-windows-x86_64.zip",
    "evidence/songcore-172/g4/package/win/package-summary.json",
    "evidence/songcore-172/g4/package/win/v-checks.json",
    "evidence/songcore-172/g4/package/win/SHA256SUMS",
    "evidence/songcore-172/g4/package/linux/songcore-v0.0.0-preflight-linux-x86_64.tar.gz",
    "evidence/songcore-172/g4/package/linux/package-summary.json",
    "evidence/songcore-172/g4/package/linux/v-checks.json",
    "evidence/songcore-172/g4/package/linux/SHA256SUMS",
]
for rel in extra:
    p = os.path.join(REPO, rel)
    evidence.append({"path": rel, "sha256": sha(p), "bytes": os.path.getsize(p)})

old_rc = os.path.join(PKG, "win", "songcore-v0.2.0-rc.1-windows-x86_64.zip")
old_rc_entry = None
prior = os.path.join(REPO, "evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip")
if os.path.isfile(prior):
    old_rc_entry = {"path": "evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip",
                    "sha256": sha(prior), "bytes": os.path.getsize(prior)}

stage = {
    "stage_id": "g4-package-laneB",
    "source_sha": "69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
    "input_sha": {
        "windows_static_artifact": {
            "path": "/home/hoo/q172rc/win/native/build/artifacts/libsongcore.a "
                    "(also C:/Users/fred1/source/qianqian/native/build/artifacts/libsongcore.a)",
            "sha256": "89d0428db2152da60f233290c082ecfb33b5590628ab9c40b97e8354afc65e97",
            "bytes": 3309294},
        "linux_static_artifact": {
            "path": "/home/hoo/g4-172/linux-root/native/build/artifacts/libsongcore.a "
                    "(ask named .../linux-root/src/native/...; that path does not exist, see notes)",
            "sha256": "0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308",
            "bytes": 4066980},
        "linux_shared_artifact": {
            "path": "/home/hoo/g4-172/linux-root/native/build/artifacts/shared/libsongcore.so",
            "sha256": "085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98",
            "bytes": 2288536},
        "windows_closure_manifest": {
            "path": "native/build/ffmpeg-xmake/manifest.json (== /home/hoo/q172rc/closure/ffmpeg-xmake/manifest.json)",
            "sha256": "0d0f7af9a6bf102be92cece254319f8b12261dd822feefa2f3125ca9ed0aed14"},
        "linux_closure_manifest": {
            "path": "/home/hoo/g4-172/linux-root/native/build/ffmpeg-xmake/manifest.json",
            "sha256": "d037e8b472d247cf457b1af204546c5feb66798f9f44aec3f96b54663cba5525"},
        "pin_json": "224ad3d5f70b5ecdebf2fe688d2eb2f976a499c6818d336cbf984bc9ff2aff85",
        "recipe_windows_x86_64": "3ba0a9e686d169f0e00ba46ade11a8f8658f204537713eabe200633ad5f8492d",
        "recipe_linux_x86_64": "79c307e731427bf1d94ab2ed43d7729c91d1d747a02392ee84170616467cdb6d",
        "profile_codec_base": "57fb70ed2c29a8464d939c611b3c5ed1bc99879438eb7f159530dfa52de554fa",
        "abi_header": "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536",
        "pinned_ffmpeg_zip": "d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746",
        "linux_packager_ops_package_linux_sh": {
            "path": "ops/package-linux.sh @ ops/songcore-linux-release-substrate 6df6c429cc6fb7b16cf566d8bb2e7d72e235ad74 (no PR opened: PR '-')",
            "sha256": "8d36f443da328c06ac7d291ecb41416e5351e0335bc67f0a998961cc8af527e0"},
        "windows_packager_ops_package_rc_ps1": sha(os.path.join(REPO, "ops/package-rc.ps1")),
        "linux_build_receipt": {
            "path": "/home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json",
            "sha256": sha("/home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json")},
    },
    "target_toolchain": {
        "windows-x86_64": {
            "recipe": "native/ffmpeg/targets/windows-x86_64.json",
            "recipe_sha256": "3ba0a9e686d169f0e00ba46ade11a8f8658f204537713eabe200633ad5f8492d",
            "cc": "x86_64-w64-mingw32-gcc (GCC) 13-win32",
            "ar": "GNU ar (GNU Binutils) 2.45.90.20260125",
            "xmake": "xmake v2.9.7+20250101, A cross-platform build utility based on Lua",
            "triple": "x86_64-pc-windows-gnu",
            "member_census": "280/280 pe-x86-64, 0 elf"},
        "linux-x86_64": {
            "recipe": "native/ffmpeg/targets/linux-x86_64.json",
            "recipe_sha256": "79c307e731427bf1d94ab2ed43d7729c91d1d747a02392ee84170616467cdb6d",
            "cc": "cc (Ubuntu 15.2.0-16ubuntu1) 15.2.0",
            "ar": "GNU ar (GNU Binutils for Ubuntu) 2.46",
            "xmake": "xmake v2.9.7+20250101, A cross-platform build utility based on Lua",
            "triple": "x86_64-unknown-linux-gnu",
            "member_census": "280/280 elf64-x86-64, 0 foreign"},
    },
    "argv": [
        "MSYS_NO_PATHCONV=1 powershell -ExecutionPolicy Bypass -File ops/package-rc.ps1 "
        "-ArtifactPath C:\\Users\\fred1\\source\\qianqian\\native\\build\\artifacts\\libsongcore.a "
        "-OutDir C:\\Users\\fred1\\source\\qianqian\\evidence\\songcore-172\\g4\\package\\win -Version 0.0.0-preflight",
        "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/.../package-laneB/run-linux-package.sh",
        "  -> /home/hoo/g4-172/laneB-tools/package-linux.sh --version 0.0.0-preflight "
        "--source-commit 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 --work-root /home/hoo/g4-172/linux-root "
        "--out-dir /home/hoo/g4-172/laneB-packages/linux "
        "--build-receipt /home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json",
        "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/.../package-laneB/verify.py",
        "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/.../package-laneB/collect-receipts.sh",
        "MSYS_NO_PATHCONV=1 powershell -ExecutionPolicy Bypass -Command Expand-Archive -LiteralPath <win zip> -DestinationPath <win/extract> -Force",
        "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/.../package-laneB/extract-stages.sh",
        "MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/.../package-laneB/final-crosschecks.sh",
    ],
    "exit_codes": [0, 0, 0, 0, 0, 0, 0, 0],
    "evidence_paths": [e["path"] for e in evidence],
    "evidence_sha256": [e["sha256"] for e in evidence],
    "evidence_file_entries": evidence,
    "packages": {
        "windows": {
            "container": "evidence/songcore-172/g4/package/win/songcore-v0.0.0-preflight-windows-x86_64.zip",
            "sha256": "5d92b21d7092b8d84d1db955ddbd38d3dc8b66f9f0ad29de35593f496e6397d3",
            "bytes": 1449803,
            "packaged_files": 14,
            "checksums_entries": 13,
            "files_entries": 12,
            "stage": "C:\\Users\\fred1\\source\\qianqian\\evidence\\songcore-172\\g4\\package\\win\\extract\\songcore-v0.0.0-preflight-windows-x86_64",
        },
        "linux": {
            "container": "/home/hoo/g4-172/laneB-packages/linux/songcore-v0.0.0-preflight-linux-x86_64.tar.gz",
            "sha256": "bfe780c3bf65b43f8007f3cd48fe3cef39de8d3a17af81f9767280aa7a43d790",
            "bytes": 2575536,
            "packaged_files": 15,
            "checksums_entries": 14,
            "files_entries": 13,
            "stage": "/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64",
        },
    },
    "prior_archive_reference": old_rc_entry,
    "verdict": "FIX_REQUIRED",
    "notes": [
        "All §8 V1-V7 gates PASS on both targets (independent re-verification, see laneB-verify.json).",
        "The ask's linux artifact paths (.../linux-root/src/native/build/artifacts/...) do not exist; the "
        "artifacts actually live at /home/hoo/g4-172/linux-root/native/build/artifacts/ and their sha256 "
        "match the ask's declared values exactly.",
        "§2 version identity is NOT machine-checked by either packager: manifest.songcore_version="
        "0.0.0-preflight, package dir matches that manifest value, but native/xmake.lua:2 pins 0.1.0 "
        "(and the tag leg has no local existence). Neither script refused the synthetic version.",
        "Reproducibility: both manifests report recipe/semantic_abi/byte = NOT_ESTABLISHED; no L1/L2/L3 "
        "claim is made anywhere in either package (preflight only, never RELEASE).",
    ],
    "not_run": [
        "runtime/device smoke of the packaged consumer example (no media input, no audio device in this lane)",
        "two-root reproducibility rebuild (contract §12 L1/L2/L3) — hence NOT_ESTABLISHED",
        "Windows-host linker smoke of metadata/external-consumer-example.c (no mingw gcc on the Windows host; not attempted)",
        "tag creation / GitHub Release / publication (forbidden by this lane)",
        "rebuild of the Windows artifact (consumed the build lane's artifact; provenance = matching sha256 + build-lane receipts)",
    ],
}

with open(os.path.join(LANE, "stage.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(stage, f, indent=2)
    f.write("\n")
print("wrote", os.path.join(LANE, "stage.json"))
print("evidence entries:", len(evidence))
