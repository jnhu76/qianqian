#!/usr/bin/env python3
"""Resolve a minimal FFmpeg build once, then freeze its compile closure for Xmake.

This is an *import/upgrade-time* tool. It is allowed to use FFmpeg's configure
and Makefiles as an upstream oracle. Normal Qianqian builds do not use them:
Xmake reads build/ffmpeg-xmake/manifest.json and recompiles only the recorded
translation units into one application-owned static archive.

The manifest is deliberately generated, never hand-maintained. Updating FFmpeg
means rerunning this importer with the same capability profile and reviewing the
resulting source/flag drift.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "build" / "ffmpeg-src"
OUT = ROOT / "build" / "ffmpeg-xmake"
ORACLE = OUT / "oracle"
MANIFEST = OUT / "manifest.json"
PIN = ROOT / "ffmpeg" / "pin.json"
PROFILE = ROOT / "ffmpeg" / "profiles" / "codec-base.json"
TARGETS = ROOT / "ffmpeg" / "targets"

# platform.system() -> Xmake platform name (recipe xmake.plat vocabulary)
HOST_PLAT = {"linux": "linux", "darwin": "macosx", "windows": "windows"}
# platform.machine() -> Xmake/recipe arch vocabulary
HOST_ARCH = {"x86_64": "x86_64", "amd64": "x86_64", "arm64": "arm64",
             "aarch64": "arm64"}

LIB_TARGETS = (
    "libavutil/libavutil.a",
    "libavcodec/libavcodec.a",
    "libavformat/libavformat.a",
)


def lib_targets_for(profile: dict) -> tuple[str, ...]:
    # The V=1 make log is the closure source, so every archive a profile needs
    # must actually be built. libswresample is only compiled when the profile
    # enables it (e.g. the Opus decoder path); without this the captured
    # manifest silently misses its translation units.
    enabled = profile.get("libraries", {}).get("enable", [])
    if "swresample" in enabled:
        return LIB_TARGETS + ("libswresample/libswresample.a",)
    return LIB_TARGETS
# Object roots accepted as closure members. libswresample is included so
# profiles that enable it (e.g. the Opus decoder's upstream dependency)
# capture its translation units; for profiles without it the root simply
# never matches. libavfilter likewise (DSP capability ladder).
OBJECT_ROOTS = ("libavutil/", "libavcodec/", "libavformat/", "libswresample/",
                "libavfilter/")
SOURCE_SUFFIXES = (".c", ".S", ".s", ".asm", ".cpp", ".m")
DEP_FLAGS_WITH_VALUE = {"-MF", "-MT", "-MQ"}
DEP_FLAGS = {"-MMD", "-MD", "-MP", "-MM", "-M"}
WINDOWS_ABS = re.compile(r"^[A-Za-z]:[\\/]")


def run(cmd: list[str], *, cwd: Path, capture: bool = False) -> str:
    print("+", shlex.join(cmd))
    p = subprocess.run(
        cmd,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
        env={**os.environ, "LC_ALL": "C"},
    )
    if p.returncode:
        tail = (p.stdout or "")[-8000:]
        raise SystemExit(f"command failed ({p.returncode}): {shlex.join(cmd)}\n{tail}")
    return p.stdout or ""


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def _assert_portable_value(value: str, where: str) -> None:
    if flag_contains_absolute_path(value):
        raise SystemExit(
            f"machine-local path leaked into durable intent ({where}): {value}")


def load_recipe(target_id: str) -> dict:
    """Load one machine-readable target recipe (ffmpeg/targets/<id>.json).

    Recipes carry only target facts: platform/arch identities, FFmpeg
    target_os/arch/cross facts, artifact capability, and honest status.
    SDK roots and machine-local paths are forbidden here — they belong to
    the environment at derive time."""
    path = TARGETS / f"{target_id}.json"
    if not path.is_file():
        known = sorted(p.stem for p in TARGETS.glob("*.json"))
        raise SystemExit(f"unknown target '{target_id}'; known targets: {known}")
    recipe = json.loads(path.read_text())
    if recipe.get("id") != target_id:
        raise SystemExit(f"recipe id mismatch: {path.name} declares {recipe.get('id')!r}")

    def walk(value: object, where: str) -> None:
        if isinstance(value, str):
            _assert_portable_value(value, where)
        elif isinstance(value, dict):
            for k, v in value.items():
                walk(v, f"{where}.{k}")
        elif isinstance(value, list):
            for i, v in enumerate(value):
                walk(v, f"{where}[{i}]")

    walk(recipe, target_id)
    return recipe


def find_native_recipe() -> dict:
    """The target recipe matching THIS host (canonical native import)."""
    plat = HOST_PLAT.get(platform.system().lower())
    arch = HOST_ARCH.get(platform.machine().lower())
    for path in sorted(TARGETS.glob("*.json")):
        recipe = json.loads(path.read_text())
        xm = recipe.get("xmake", {})
        plats = xm.get("plat", [])
        if isinstance(plats, str):
            plats = [plats]
        if plat in plats and xm.get("arch") == arch:
            return recipe
    raise SystemExit(
        f"no ffmpeg/targets recipe matches host "
        f"{platform.system()}/{platform.machine()} (plat={plat}, arch={arch}); "
        f"add one before importing")


def recipe_configure_args(recipe: dict) -> list[str]:
    """Deterministic configure arguments implied by the target facts."""
    ff = recipe.get("ffmpeg", {})
    args = []
    if ff.get("cross"):
        args.append("--enable-cross-compile")
    if ff.get("target_os"):
        args.append(f"--target-os={ff['target_os']}")
    if ff.get("arch"):
        args.append(f"--arch={ff['arch']}")
    if ff.get("cross_prefix"):
        args.append(f"--cross-prefix={ff['cross_prefix']}")
    return args


def target_identity(recipe: dict | None, recipe_sha256: str | None) -> dict:
    if recipe is None:
        return {"id": None, "platform": None, "arch": None,
                "recipe_sha256": None}
    return {
        "id": recipe["id"],
        "platform": recipe.get("platform"),
        "arch": recipe.get("arch"),
        "recipe_sha256": recipe_sha256,
        # Toolchain-family facts bound into the manifest identity so a
        # replay session can detect recipe drift — or a manifest from a
        # DIFFERENT toolchain family (MinGW vs MSVC) — by field comparison,
        # without hashing.
        "target_os": (recipe.get("ffmpeg") or {}).get("target_os"),
        "cross_prefix": (recipe.get("ffmpeg") or {}).get("cross_prefix"),
    }


def verified_source(pin: dict) -> None:
    stamp = SRC / ".qianqian-verified"
    expected = f"{pin['ffmpeg_commit_sha']}|{pin['source_sha256']}"
    if not stamp.is_file() or stamp.read_text().strip() != expected:
        fetch = ROOT / "scripts" / "fetch-ffmpeg"
        run([str(fetch)], cwd=ROOT)
    if not stamp.is_file() or stamp.read_text().strip() != expected:
        raise SystemExit("pinned FFmpeg source did not verify after fetch")


def configure_args(profile: dict) -> list[str]:
    # Deterministic ordering: profile JSON -> configure argument list.
    args = ["--prefix=install"]
    if profile.get("component_base") == "everything-disabled":
        args.append("--disable-everything")
    args.extend("--disable-" + item for item in profile.get("disable", []))

    libs = profile.get("libraries", {})
    for name in sorted(libs.get("disable", [])):
        args.append(f"--disable-{name}")
    for name in libs.get("enable", []):
        args.append(f"--enable-{name}")

    classes = ("demuxer", "decoder", "encoder", "muxer", "parser", "bsf",
               "protocol", "filter", "indev", "outdev")
    for cls in classes:
        for name in sorted(profile.get("components", {}).get(cls, [])):
            args.append(f"--enable-{cls}={name}")
    return args


def make_log() -> str:
    # A clean oracle tree gives generated config and the exact commands FFmpeg
    # itself considers necessary for this capability slice.
    shutil.rmtree(ORACLE, ignore_errors=True)
    ORACLE.mkdir(parents=True)

    profile = json.loads(PROFILE.read_text())
    args = configure_args(profile)
    run([str(SRC / "configure"), *args], cwd=ORACLE)

    jobs = str(max(1, os.cpu_count() or 4))
    # V=1 is the key: source closure is observed from real compiler invocations
    # rather than reimplementing FFmpeg's Make language.
    return run(["make", "-j", jobs, "V=1", *lib_targets_for(profile)], cwd=ORACLE, capture=True)


def config_value(name: str) -> str | None:
    config_mak = ORACLE / "ffbuild" / "config.mak"
    if not config_mak.is_file():
        return None
    text = config_mak.read_text(errors="replace")
    match = re.search(rf"^{re.escape(name)}=(.*)$", text, re.M)
    return match.group(1).strip() if match else None


def toolchain_identity() -> dict:
    """Record the oracle environment that generated this closure.

    Generated config headers and selected architecture sources are not portable
    across arbitrary compilers/targets. A changed toolchain therefore requires
    a fresh import rather than silently replaying an old manifest.
    """
    return {
        "system": platform.system().lower(),
        "machine": platform.machine(),
        "cc": config_value("CC"),
        "cc_ident": config_value("CC_IDENT"),
        "arch": config_value("ARCH"),
        "target_os": config_value("TARGET_OS"),
    }


def normalize_path(token: str) -> tuple[str, str] | None:
    p = Path(token)
    if not p.is_absolute():
        p = (ORACLE / p).resolve()
    else:
        p = p.resolve()
    try:
        return "source", p.relative_to(SRC.resolve()).as_posix()
    except ValueError:
        pass
    try:
        return "generated", p.relative_to(ORACLE.resolve()).as_posix()
    except ValueError:
        return None


def rewrite_flag(flag: str) -> str:
    # No checkout-specific absolute paths are allowed in the lock manifest.
    sroot = str(SRC.resolve())
    broot = str(ORACLE.resolve())
    flag = flag.replace(sroot, "@SRC@").replace(broot, "@BUILD@")
    if flag in ("-I.", "-I./"):
        return "-I@BUILD@"
    return flag


def parse_compile(line: str) -> dict | None:
    try:
        argv = shlex.split(line)
    except ValueError:
        return None
    if "-c" not in argv or "-o" not in argv:
        return None
    oi = argv.index("-o")
    if oi + 1 >= len(argv):
        return None
    obj = argv[oi + 1]
    obj_norm = Path(obj).as_posix().lstrip("./")
    if not obj_norm.startswith(OBJECT_ROOTS):
        return None

    source_i = None
    source_info = None
    for i, token in enumerate(argv):
        if token.endswith(SOURCE_SUFFIXES):
            info = normalize_path(token)
            if info is not None:
                source_i, source_info = i, info
    if source_i is None or source_info is None:
        return None

    # Skip compiler/wrapper words before the first option (e.g. ccache gcc),
    # plus output/dependency bookkeeping. Preserve semantic compile flags.
    first_option = next((i for i, token in enumerate(argv) if token.startswith("-")), 1)
    flags: list[str] = []
    skip_next = False
    for i, token in enumerate(argv[first_option:], first_option):
        if skip_next:
            skip_next = False
            continue
        if i == source_i or token == "-c":
            continue
        if token == "-o":
            skip_next = True
            continue
        if token in DEP_FLAGS:
            continue
        if token in DEP_FLAGS_WITH_VALUE:
            skip_next = True
            continue
        flags.append(rewrite_flag(token))

    origin, source = source_info
    suffix = Path(source).suffix
    flag_kind = "asflags" if suffix in (".S", ".s", ".asm") else "cflags"
    return {
        "object": obj_norm,
        "origin": origin,
        "path": source,
        "flag_kind": flag_kind,
        "flags": flags,
    }


def closure_from_log(log: str) -> list[dict]:
    by_object: dict[str, dict] = {}
    for line in log.splitlines():
        unit = parse_compile(line.strip())
        if unit:
            previous = by_object.get(unit["object"])
            if previous and previous != unit:
                raise SystemExit(f"compiler command drift for {unit['object']}")
            by_object[unit["object"]] = unit
    units = [by_object[k] for k in sorted(by_object)]
    if not units:
        raise SystemExit("no FFmpeg translation units captured; V=1 parsing failed")
    return units


def flag_contains_absolute_path(flag: str) -> bool:
    if "@SRC@" in flag or "@BUILD@" in flag:
        return False
    candidates = [flag]
    for prefix in ("-I", "-L", "-isystem", "--sysroot="):
        if flag.startswith(prefix) and len(flag) > len(prefix):
            candidates.append(flag[len(prefix):])
    return any(c.startswith("/") or WINDOWS_ABS.match(c) for c in candidates)


def assert_portable_manifest(manifest: dict) -> None:
    for unit in manifest["units"]:
        for flag in unit["flags"]:
            if flag_contains_absolute_path(flag):
                raise SystemExit(f"machine-local compile path leaked into manifest: {flag}")
    for arg in manifest.get("configure_args", []):
        _assert_portable_value(arg, "configure_args")
    for key, value in manifest.get("toolchain", {}).items():
        if isinstance(value, str):
            _assert_portable_value(value, f"toolchain.{key}")


def main() -> None:
    pin = json.loads(PIN.read_text())
    profile = json.loads(PROFILE.read_text())
    if profile.get("profile") != "codec-base":
        raise SystemExit("import profile identity changed unexpectedly")
    recipe = find_native_recipe()
    verified_source(pin)

    log = make_log()
    units = closure_from_log(log)
    args = configure_args(profile)
    toolchain = toolchain_identity()

    refs = {}
    for rel in lib_targets_for(profile):
        archive = ORACLE / rel
        if not archive.is_file():
            raise SystemExit(f"oracle archive missing: {archive}")
        refs[rel] = {"bytes": archive.stat().st_size, "sha256": sha256_file(archive)}

    source_units = sum(u["origin"] == "source" for u in units)
    generated_units = len(units) - source_units
    manifest = {
        "schema": 2,
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "profile": profile["profile"],
        "profile_variant": profile.get("variant"),
        "profile_sha256": sha256_file(PROFILE),
        "target": target_identity(recipe, sha256_file(TARGETS / f"{recipe['id']}.json")),
        "toolchain": toolchain,
        "source_root": "build/ffmpeg-src",
        "config_root": ORACLE.relative_to(ROOT).as_posix(),
        "configure_args": args,
        "closure": {
            "translation_units": len(units),
            "upstream_sources": source_units,
            "generated_sources": generated_units,
        },
        "reference_archives": refs,
        "units": units,
    }
    assert_portable_manifest(manifest)
    OUT.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    (OUT / "oracle-build.log").write_text(log)

    print(f"wrote {MANIFEST.relative_to(ROOT)}")
    print(f"closure: {len(units)} translation units ({generated_units} generated)")
    print(f"toolchain: {toolchain}")
    print("normal builds may now run: xmake build qianqian_av")


if __name__ == "__main__":
    main()
