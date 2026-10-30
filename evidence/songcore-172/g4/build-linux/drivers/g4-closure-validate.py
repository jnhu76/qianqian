#!/usr/bin/env python3
"""Revalidate an already-imported FFmpeg closure against pin.json + target recipe.

Usage: g4-closure-validate.py <src-root> <pinned-zip>

Reads (all inside <src-root>/native):
  ffmpeg/pin.json
  ffmpeg/targets/linux-x86_64.json
  ffmpeg/profiles/codec-base.json
  build/ffmpeg-xmake/manifest.json
  build/ffmpeg-src/.qianqian-verified
  build/ffmpeg-xmake/oracle/ffbuild/config.mak

Prints one JSON document on stdout. Exits 1 when any mandatory check fails.
The zip content check streams the pinned GitHub archive and compares every
regular file against the copied build/ffmpeg-src tree; it never downloads.
"""
from __future__ import annotations

import hashlib
import json
import platform
import sys
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main() -> int:
    src = Path(sys.argv[1]).resolve()
    zip_path = Path(sys.argv[2]).resolve()
    native = src / "native"

    pin_path = native / "ffmpeg" / "pin.json"
    recipe_path = native / "ffmpeg" / "targets" / "linux-x86_64.json"
    profile_path = native / "ffmpeg" / "profiles" / "codec-base.json"
    man_path = native / "build" / "ffmpeg-xmake" / "manifest.json"
    stamp_path = native / "build" / "ffmpeg-src" / ".qianqian-verified"
    oracle_cfg = native / "build" / "ffmpeg-xmake" / "oracle" / "ffbuild" / "config.mak"

    pin = json.loads(pin_path.read_text())
    recipe = json.loads(recipe_path.read_text())
    profile = json.loads(profile_path.read_text())
    man = json.loads(man_path.read_text())

    recipe_sha = sha256_file(recipe_path)
    profile_sha = sha256_file(profile_path)
    pin_sha = sha256_file(pin_path)
    man_sha = sha256_file(man_path)
    stamp = stamp_path.read_text().strip() if stamp_path.is_file() else None

    checks: list[dict] = []

    def ck(cid: str, ok: bool, detail: str) -> bool:
        checks.append({"id": cid, "status": "PASS" if ok else "FAIL",
                       "detail": detail})
        return ok

    t = man.get("target") or {}
    tc = man.get("toolchain") or {}
    ff = recipe.get("ffmpeg") or {}

    ck("manifest_schema_2", man.get("schema") == 2,
       f"schema={man.get('schema')}")
    ck("manifest_tag_matches_pin", man.get("ffmpeg_tag") == pin.get("ffmpeg_tag"),
       f"manifest={man.get('ffmpeg_tag')} pin={pin.get('ffmpeg_tag')}")
    ck("manifest_commit_matches_pin",
       man.get("ffmpeg_commit_sha") == pin.get("ffmpeg_commit_sha"),
       f"manifest={man.get('ffmpeg_commit_sha')}")
    ck("manifest_source_sha_matches_pin",
       man.get("ffmpeg_source_sha256") == pin.get("source_sha256"),
       f"manifest={man.get('ffmpeg_source_sha256')} pin={pin.get('source_sha256')}")
    expected_stamp = f"{pin['ffmpeg_commit_sha']}|{pin['source_sha256']}"
    ck("verified_stamp_matches_pin", stamp == expected_stamp,
       f"stamp={stamp} expected={expected_stamp}")
    ck("oracle_config_mak_present", oracle_cfg.is_file(),
       f"{oracle_cfg} exists={oracle_cfg.is_file()} "
       f"bytes={oracle_cfg.stat().st_size if oracle_cfg.is_file() else 0}")
    ck("manifest_target_id_matches_recipe", t.get("id") == recipe.get("id"),
       f"manifest={t.get('id')} recipe={recipe.get('id')}")
    ck("manifest_platform_arch_match_recipe",
       t.get("platform") == recipe.get("platform")
       and t.get("arch") == recipe.get("arch"),
       f"manifest={t.get('platform')}/{t.get('arch')} "
       f"recipe={recipe.get('platform')}/{recipe.get('arch')}")
    ck("manifest_target_os_matches_recipe",
       t.get("target_os") == ff.get("target_os"),
       f"manifest={t.get('target_os')} recipe={ff.get('target_os')}")
    ck("manifest_cross_prefix_matches_recipe",
       t.get("cross_prefix") == ff.get("cross_prefix"),
       f"manifest={t.get('cross_prefix')} recipe={ff.get('cross_prefix')}")
    ck("manifest_recipe_sha256_matches_current_recipe",
       t.get("recipe_sha256") == recipe_sha,
       f"manifest={t.get('recipe_sha256')} current_file={recipe_sha}")
    ck("manifest_profile_sha256_matches_current_profile",
       man.get("profile_sha256") == profile_sha,
       f"manifest={man.get('profile_sha256')} current_file={profile_sha}")
    ck("manifest_profile_identity_codec_base",
       man.get("profile") == profile.get("profile") == "codec-base",
       f"manifest={man.get('profile')} profile_file={profile.get('profile')}")
    ck("toolchain_host_matches_manifest",
       tc.get("system") == platform.system().lower()
       and tc.get("machine") == platform.machine(),
       f"manifest={tc.get('system')}/{tc.get('machine')} "
       f"host={platform.system().lower()}/{platform.machine()}")
    ck("toolchain_cc_identity_gcc",
       str(tc.get("cc", "")).startswith("gcc")
       and "gcc" in str(tc.get("cc_ident", "")),
       f"cc={tc.get('cc')} cc_ident={tc.get('cc_ident')}")

    units = man.get("units") or []
    declared = (man.get("closure") or {}).get("translation_units")
    ck("closure_unit_count_consistent", len(units) == declared == 279,
       f"units={len(units)} manifest.closure.translation_units={declared}")

    srcroot = (native / man.get("source_root", "")).resolve()
    buildroot = (native / man.get("config_root", "")).resolve()
    missing_units = []
    generated_units = 0
    for u in units:
        root = buildroot if u.get("origin") == "generated" else srcroot
        if u.get("origin") == "generated":
            generated_units += 1
        if not (root / u["path"]).is_file():
            missing_units.append(f"{u.get('origin')}:{u['path']}")
    ck("all_unit_sources_present", not missing_units,
       f"missing={len(missing_units)} generated_units={generated_units} "
       f"existing={len(units) - len(missing_units)}/{len(units)}")
    ck("source_root_and_config_root_present",
       srcroot.is_dir() and buildroot.is_dir(),
       f"source_root={srcroot} is_dir={srcroot.is_dir()} "
       f"config_root={buildroot} is_dir={buildroot.is_dir()}")

    # ---- pinned zip: hash + full content comparison against the copied tree
    zip_sha = sha256_file(zip_path) if zip_path.is_file() else None
    ck("pinned_zip_sha256_matches_pin", zip_sha == pin.get("source_sha256"),
       f"zip={zip_sha} pin={pin.get('source_sha256')} path={zip_path}")

    zip_result: dict = {"path": str(zip_path), "sha256": zip_sha}
    if zip_sha == pin.get("source_sha256"):
        with zipfile.ZipFile(zip_path) as z:
            infos = z.infolist()
            tops = sorted({i.filename.split("/", 1)[0] for i in infos})
            prefix = tops[0] + "/"
            files = [i for i in infos if not i.is_dir()]
            compared = 0
            missing = []
            mismatch = []
            names = set()
            for i in files:
                rel = i.filename[len(prefix):]
                names.add(rel)
                f = srcroot / rel
                if not f.is_file():
                    missing.append(rel)
                    continue
                entry_sha = hashlib.sha256(z.read(i)).hexdigest()
                if sha256_file(f) != entry_sha:
                    mismatch.append(rel)
                else:
                    compared += 1
            extras = []
            for p in sorted(srcroot.rglob("*")):
                if p.is_file():
                    rel = p.relative_to(srcroot).as_posix()
                    if rel not in names:
                        extras.append(rel)
            zip_result.update({
                "top_level": tops,
                "zip_file_entries": len(files),
                "files_identical": compared,
                "missing_in_src": len(missing),
                "missing_samples": missing[:5],
                "content_mismatch": len(mismatch),
                "mismatch_samples": mismatch[:5],
                "extras_in_src": len(extras),
                "extras_samples": extras[:5],
            })
            ck("zip_content_equals_copied_source_tree",
               not missing and not mismatch and extras == [".qianqian-verified"],
               f"identical={compared}/{len(files)} missing={len(missing)} "
               f"mismatch={len(mismatch)} extras={extras[:3]}")
    else:
        zip_result["note"] = "zip unusable; content comparison NOT RUN"
        ck("zip_content_equals_copied_source_tree", False,
           "NOT RUN: pinned zip missing or sha mismatch")

    mandatory_failures = [c["id"] for c in checks if c["status"] != "PASS"]
    out = {
        "tool": "g4-closure-validate.py",
        "src": str(src),
        "mandatory_failures": mandatory_failures,
        "verdict": "PASS" if not mandatory_failures else "FAIL",
        "identity": {
            "pin_json": {"path": str(pin_path), "sha256": pin_sha,
                         "ffmpeg_tag": pin.get("ffmpeg_tag"),
                         "ffmpeg_commit_sha": pin.get("ffmpeg_commit_sha"),
                         "source_sha256": pin.get("source_sha256")},
            "recipe": {"path": str(recipe_path), "sha256": recipe_sha,
                       "id": recipe.get("id"),
                       "platform": recipe.get("platform"),
                       "arch": recipe.get("arch"),
                       "ffmpeg": ff},
            "profile": {"path": str(profile_path), "sha256": profile_sha,
                        "profile": profile.get("profile")},
            "manifest": {"path": str(man_path), "sha256": man_sha,
                         "schema": man.get("schema"),
                         "target": t, "toolchain": tc,
                         "closure": man.get("closure"),
                         "source_root": man.get("source_root"),
                         "config_root": man.get("config_root")},
            "verified_stamp": stamp,
        },
        "zip": zip_result,
        "checks": checks,
    }
    print(json.dumps(out, indent=2, sort_keys=True))
    return 0 if not mandatory_failures else 1


if __name__ == "__main__":
    sys.exit(main())
