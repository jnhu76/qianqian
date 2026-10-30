#!/usr/bin/env python3
"""Generate the Lane E evidence manifest and stage.json (read-only over evidence)."""
import hashlib
import json
import os
from pathlib import Path

LANE = Path("/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps")
REPO = Path("/mnt/c/Users/fred1/source/qianqian")
RAW = LANE / "raw"


def sha256(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# 1) manifest of everything currently in the lane dir (stage.json excluded)
files = sorted(p for p in LANE.rglob("*") if p.is_file() and p.name != "stage.json")
manifest = RAW / "evidence-manifest.sha256.txt"
with manifest.open("w", encoding="utf-8", newline="\n") as f:
    for p in files:
        f.write(f"{sha256(p)}  {p.relative_to(LANE).as_posix()}\n")

# 2) include the manifest in the final evidence set
files = sorted(p for p in LANE.rglob("*") if p.is_file() and p.name != "stage.json")

evidence_paths = [p.relative_to(REPO).as_posix() for p in files]
evidence_sha256 = [sha256(p) for p in files]

ART = "/home/hoo/g4-172/linux-root/src/native/build/artifacts"
SO = f"{ART}/shared/libsongcore.so"
AR = f"{ART}/libsongcore.a"
INC = "/home/hoo/g4-172/linux-root/src/native/include"
W = "$HOME/lane-e-runtime/work"
STAGE = "/mnt/c/Users/fred1/lane-e-container"
MEDIA = "/home/hoo/Source/qianqian/native/experiments/songcore-equivalence/fixtures/mp3-cbr-id3v23.mp3"

argv = [
    f"sha256sum {SO} {AR}",
    f"file {SO} {AR}",
    f"ar t {AR}",
    f"objdump -T {SO}",
    f"objdump -T {SO} | grep -o 'GLIBC_[0-9.]*' | sort -uV",
    f"readelf --version-info {SO}",
    f"readelf --dyn-syms --wide {SO}",
    f"grep -E '^(GLIBCXX_|CXXABI_|GCC_|GOMP_|LIBATOMIC_|GLIBC_ABI_)' {LANE}/raw/dynsym-version-tags.txt",
    f"readelf -d {SO}",
    f"objdump -p {SO}",
    f"nm -D --defined-only {SO}",
    f"nm -D --undefined-only {SO}",
    f"(cd $HOME/lane-e-runtime/isolate && ldd ./libsongcore.so)",
    f"nm -A --undefined-only {AR}",
    f"gcc -O0 -I{INC} src/abi-complete-consumer.c -L{ART} -lsongcore -o static/abi-O0-nolibs",
    f"gcc -O0 -I{INC} src/abi-complete-consumer.c -L{ART} -lsongcore -lpthread -o static/abi-O0-nomath",
    f"gcc -O0 -I{INC} src/abi-complete-consumer.c -L{ART} -lsongcore -lm -o static/abi-O0-nopthread",
    f"gcc -O0 -I{INC} src/abi-complete-consumer.c -L{ART} -lsongcore -lm -lpthread -o static/abi-O0-libs",
    f"gcc -O2 -I{INC} src/minimal-consumer.c -L{ART} -lsongcore -lm -o static/minimal-lm-only",
    f"gcc -O2 -I{INC} src/minimal-consumer.c -L{ART} -lsongcore -lpthread -o static/minimal-pthread-only",
    f"gcc -O2 -I{INC} src/external-consumer-example.c -L{ART} -lsongcore -lm -lpthread -o static/external-consumer-static",
    f"gcc -O2 -I{INC} src/external-consumer-example.c -L{ART}/shared -lsongcore -lm -lpthread -Wl,-rpath,'$ORIGIN' -o shared/external-consumer-shared",
    f"gcc -O2 -I{INC} src/abi-complete-consumer.c -L{ART} -lsongcore -lm -lpthread -o static/abi-complete-libs",
    f"(cd {W} && ./static/external-consumer-static {MEDIA})",
    f"(cd {W}/shared && ./external-consumer-shared {MEDIA})",
    f"(cd {W} && ./static/abi-complete-libs)",
    f"(cd {W} && ./static/abi-O0-libs)",
    f"(cd {W}/shared && ./abi-complete-shared)",
    f"(cd {W} && ./static/abi-sizeof-probe-static)",
    f"(cd {W}/shared && ./abi-sizeof-probe-shared)",
    "docker desktop start",
    "docker version",
    "docker run --rm --entrypoint ldd ubuntu:20.04 --version",
    "docker run --rm --entrypoint ldd postgres:18.4-bookworm --version",
    "docker run --rm --entrypoint ldd mcr.microsoft.com/playwright:v1.61.0-noble --version",
    "docker pull ubuntu:20.04",
    f"docker run --rm -v {STAGE}:/work -w /work ubuntu:20.04 ./external-consumer-shared /work/media.mp3",
    f"docker run --rm -v {STAGE}:/work -w /work ubuntu:20.04 ./minimal-libs",
    f"docker run --rm -v {STAGE}:/work -w /work postgres:18.4-bookworm ./external-consumer-shared /work/media.mp3",
    f"docker run --rm -v {STAGE}:/work -w /work postgres:18.4-bookworm ./minimal-libs",
    f"docker run --rm -v {STAGE}:/work -w /work mcr.microsoft.com/playwright:v1.61.0-noble ./external-consumer-shared /work/media.mp3",
    f"docker run --rm -v {STAGE}:/work -w /work mcr.microsoft.com/playwright:v1.61.0-noble ./minimal-libs",
    f"docker run --rm -v {STAGE}:/work -w /work alpine:latest ./external-consumer-shared /work/media.mp3",
    "git -C /home/hoo/Source/qianqian rev-parse HEAD",
    f"sha256sum {W}/shared/libsongcore.so {W}/src/songcore.h",
    "nm -D --defined-only /usr/lib/x86_64-linux-gnu/libpthread.so.0",
    "readelf -d /usr/lib/x86_64-linux-gnu/libpthread.so.0",
]
exit_codes = [
    0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
    1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 255, 0, 0,
    0, 0,
]
assert len(argv) == len(exit_codes), (len(argv), len(exit_codes))

stage = {
    "stage_id": "lane-e-runtime-deps-20261030",
    "source_sha": "69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
    "source_sha_note": "git -C /home/hoo/Source/qianqian rev-parse HEAD and the recreated /home/hoo/g4-172/linux-root both report 69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
    "input_sha": {
        "libsongcore.a": "0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308",
        "shared/libsongcore.so": "085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98",
        "match": "both measured sha256 values equal the task input hashes (raw/sha256-artifacts.txt)",
        "preserved_copy_check": "the .so copy at $HOME/lane-e-runtime/work/shared/libsongcore.so and the container stage copy both hash to 085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98",
        "build_time_header_sha256": "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536",
        "build_time_header_note": "the build tree /home/hoo/g4-172/linux-root was deleted by another process at ~20:51 after all artifact measurements; the header copy taken at compile time matches the repo and packaged headers",
    },
    "target_toolchain": {
        "target": "linux-x86_64",
        "build_host": "WSL2 Ubuntu-24.04, glibc 2.43-2ubuntu2.4",
        "compiler": "gcc (Ubuntu 15.2.0-16ubuntu1) 15.2.0",
        "binutils": "GNU objdump/readelf 2.46",
        "artifact_forms": ["static libsongcore.a", "shared libsongcore.so"],
    },
    "argv": argv,
    "exit_codes": exit_codes,
    "evidence_paths": evidence_paths,
    "evidence_sha256": evidence_sha256,
    "measured": {
        "shared_needed": ["libm.so.6", "libc.so.6"],
        "shared_glibc_versions": [
            "GLIBC_2.2.5", "GLIBC_2.3.2", "GLIBC_2.3.4", "GLIBC_2.4",
            "GLIBC_2.6", "GLIBC_2.7", "GLIBC_2.14", "GLIBC_2.17",
            "GLIBC_2.27", "GLIBC_2.28", "GLIBC_2.29", "GLIBC_2.34",
            "GLIBC_2.35", "GLIBC_2.43",
        ],
        "shared_glibc_floor": "GLIBC_2.43",
        "floor_forcing_symbols": {
            "GLIBC_2.43": ["atan2f (libm.so.6)"],
            "GLIBC_2.35": ["hypot (libm.so.6)"],
            "GLIBC_2.34": ["pthread_join", "pthread_once", "pthread_create (libc.so.6)"],
        },
        "non_glibc_version_namespaces": "none (grep for GLIBCXX_/CXXABI_/GCC_/GOMP_/LIBATOMIC_/GLIBC_ABI_ returned exit 1; the only namespace in dynsym tags is GLIBC)",
        "shared_exports": "15 song* symbols, matching the ABI list; no other defined dynamic symbols",
        "shared_soname_rpath": "no SONAME, no RPATH, no RUNPATH (raw/soname-rpath.txt is empty)",
        "shared_ffmpeg_dynamic_dep": "none: NEEDED contains no libav*/libsw*/libpostproc; nm -D --undefined-only has no av*/sw* symbol",
        "shared_ldd_isolated": ["linux-vdso.so.1", "libm.so.6", "libc.so.6", "/lib64/ld-linux-x86-64.so.2"],
        "archive_members": 280,
        "archive_link_requires": "-lm is required (without it: undefined cos/exp/atan from vorbisdec.c.o); -lpthread is not required at link time on glibc 2.43 because pthread_* resolve from libc.so.6 and the host's libpthread.so.0 defines only __libpthread_version_placeholder plus version ABS symbols (no real functions; raw/libpthread-stub-check.txt); the repository link contract keeps -lpthread for older glibc; no -ldl/-lrt needed",
        "archive_new_glibc_name_symbols": "no __isoc23_* names; only __isoc99_sscanf (glibc 2.7 era); pthread_*/clock_gettime present",
        "static_consumer_glibc_floor_when_linked_on_this_host": "GLIBC_2.43 (atan2f/hypot default versions from the host libm); measured for minimal-libs, external-consumer-static, abi-O0-libs",
        "abi_sizes": {
            "sizeof_song_status": 4,
            "sizeof_song_io": 32,
            "sizeof_song_info": 120,
            "sizeof_song_stream_info": 88,
            "sizeof_song_metadata": 192,
            "sizeof_song_handle_ptr": 8,
            "note": "identical for static-linked and shared-linked probes; both exit 0",
        },
    },
    "verdict": "PASS",
    "verdict_scope": "Measurement lane only: every requested measurement executed and produced a conclusive result. This PASS does not assert portability or a support range; the measured floor and old-distro results are facts for the Owner's Q4 decision.",
    "not_run": [
        "Link-time necessity of -lpthread on glibc < 2.34: NOT RUN - no older toolchain/sysroot was used to link; only the host glibc 2.43 link matrix was measured.",
        "Distros older than Ubuntu 20.04 (glibc 2.17/2.27 class): NOT RUN - no such image was pulled or used.",
        "Cross-version ABI consumer from the v0.1.0 package: NOT RUN - optional, outside this lane's scope.",
        "Any target other than linux-x86_64: NOT RUN - out of scope.",
        "A glibc 2.43 environment other than the build host: NOT RUN - none available.",
    ],
    "provenance_caveat": "The Linux build root /home/hoo/g4-172/linux-root was deleted and recreated by another process at ~20:51, after all artifact-dependent measurements completed (ELF measurement 20:45-20:46, link/consumer matrix 20:47-20:48, container staging 20:49). Artifact identity is pinned by the sha256 match recorded at measurement time and by preserved copies; the archive itself was not copied and no longer exists at its original path.",
}

out = LANE / "stage.json"
with out.open("w", encoding="utf-8", newline="\n") as f:
    json.dump(stage, f, indent=2, ensure_ascii=False)
    f.write("\n")

print(f"evidence files: {len(evidence_paths)}")
print(f"stage.json written: {out}")
print(f"manifest: {manifest}")
