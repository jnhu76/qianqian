#!/usr/bin/env python3
"""Lane B independent verifier for the SongCore #172 G4 preflight packages.

Reads ONLY the containers, the source commit objects (git show) and the
imported closure manifests; it never trusts the packagers' own receipts or
variables. Emits JSON checks + a human log.
"""
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile

SRC_REPO = "/home/hoo/Source/qianqian"
SOURCE_COMMIT = "69ba1ce05a2772f5aa077c6ece3365c40f6e0500"
FROZEN_HEADER = "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536"
ABI_SYMBOLS = [
    "song_audio_stream_count", "song_audio_stream_info", "song_close",
    "song_get_artwork_count", "song_get_artwork_item", "song_get_metadata",
    "song_get_metadata_count", "song_get_metadata_entry", "song_last_error",
    "song_open", "song_probe", "song_read_pcm", "song_seek",
    "song_select_stream", "songcore_abi_version",
]

CFG = {
    "windows": {
        "container": "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/"
                     "songcore-v0.0.0-preflight-windows-x86_64.zip",
        "kind": "zip",
        "stage": "/home/hoo/g4-172/laneB-stage/windows",
        "root": "songcore-v0.0.0-preflight-windows-x86_64",
        "recipe": "windows-x86_64",
        "closure_manifest": "/mnt/c/Users/fred1/source/qianqian/native/build/ffmpeg-xmake/manifest.json",
        "closure_alt": "/home/hoo/q172rc/closure/ffmpeg-xmake/manifest.json",
        "nm": "x86_64-w64-mingw32-nm",
        "objdump": "x86_64-w64-mingw32-objdump",
        "ar": "x86_64-w64-mingw32-ar",
        "readelf": None,
        "member_format_re": r"file format pe-x86-64",
        "expect_static_sha": "89d0428db2152da60f233290c082ecfb33b5590628ab9c40b97e8354afc65e97",
        "expect_static_bytes": 3309294,
        "expect_members": 280,
        "has_shared": False,
    },
    "linux": {
        "container": "/home/hoo/g4-172/laneB-packages/linux/"
                     "songcore-v0.0.0-preflight-linux-x86_64.tar.gz",
        "kind": "tar",
        "stage": "/home/hoo/g4-172/laneB-stage/linux",
        "root": "songcore-v0.0.0-preflight-linux-x86_64",
        "recipe": "linux-x86_64",
        "closure_manifest": "/home/hoo/g4-172/linux-root/native/build/ffmpeg-xmake/manifest.json",
        "closure_alt": "/home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/manifest.json",
        "nm": "nm",
        "objdump": "objdump",
        "ar": "ar",
        "readelf": "readelf",
        "member_format_re": r"file format elf64-x86-64",
        "expect_static_sha": "0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308",
        "expect_static_bytes": 4066980,
        "expect_members": 280,
        "has_shared": True,
        "expect_shared_sha": "085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98",
        "expect_shared_bytes": 2288536,
        "source_work_root": "/home/hoo/g4-172/linux-root",
    },
}

results = []
loglines = []


def rec(cid, ok, detail, target):
    results.append({"id": cid, "target": target, "status": "PASS" if ok else "FAIL", "detail": detail})
    loglines.append("%-7s %-28s %s  %s" % (target, cid, "PASS" if ok else "FAIL", detail))


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def git_show(rel, target):
    out = subprocess.run(["git", "-C", SRC_REPO, "show", "%s:%s" % (SOURCE_COMMIT, rel)],
                         capture_output=True, check=True)
    return out.stdout


def git_blob_sha(rel, target):
    return hashlib.sha256(git_show(rel, target)).hexdigest()


def run(cmd, cwd=None):
    p = subprocess.run(cmd, capture_output=True, cwd=cwd)
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def html_unescape_none(s):
    return s


def main(target, cfg):
    sys.stdout.write("=== %s ===\n" % target)
    stage = cfg["stage"]
    if os.path.isdir(stage):
        shutil.rmtree(stage)
    os.makedirs(stage, exist_ok=True)

    # ---------------- container open + entry hygiene ----------------
    entries = []
    if cfg["kind"] == "zip":
        z = zipfile.ZipFile(cfg["container"])
        infos = z.infolist()
        names = [i.filename for i in infos]
        dup = sorted({n for n in names if names.count(n) > 1})
        bad_trav = [n for n in names if n.startswith("/") or ".." in n.split("/")]
        bad_bs = [n for n in names if "\\" in n]
        sym = [i.filename for i in infos if ((i.external_attr >> 16) & 0xF000) == 0xA000]
        for i in infos:
            # extract manually, byte-exact
            dst = os.path.join(stage, i.filename.replace("/", os.sep))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with z.open(i) as src, open(dst, "wb") as dstf:
                dstf.write(src.read())
            entries.append(i.filename)
        z.close()
    else:
        tf = tarfile.open(cfg["container"], "r:gz")
        members = tf.getmembers()
        names = [m.name for m in members]
        dup = sorted({n for n in names if names.count(n) > 1})
        bad_trav = [n for n in names if n.startswith("/") or ".." in n.split("/")]
        bad_bs = [n for n in names if "\\" in n]
        sym = [m.name for m in members if m.issym() or m.islnk()]
        loose = [n for n in names if "/" not in n]
        tf.extractall(stage, filter="data")
        entries = [m.name for m in members if m.isfile()]
        tf.close()
        loose_files = [m.name for m in members if m.isfile() and "/" not in m.name]
        rec("LAYOUT_no_loose_files", not loose_files,
            "file members at the container top level (outside the versioned root): %s; "
            "top-level directory members: %s" % (loose_files or "none", loose or "none"), target)

    rec("LAYOUT_member_hygiene",
        not dup and not bad_trav and not bad_bs and not sym,
        "duplicates=%s traversal=%s backslash=%s symlink=%s (%d members)"
        % (dup or "none", bad_trav or "none", bad_bs or "none", sym or "none", len(names)),
        target)

    # exactly one versioned root directory
    tops = sorted(os.listdir(stage))
    rec("LAYOUT_single_root", tops == [cfg["root"]],
        "top-level entries under the container: %s (expected exactly [%s])" % (tops, cfg["root"]), target)

    root = os.path.join(stage, cfg["root"])
    packaged = []
    for dirpath, _dirs, files in os.walk(root):
        for n in files:
            packaged.append(os.path.relpath(os.path.join(dirpath, n), root).replace(os.sep, "/"))
    packaged.sort()
    rec("LAYOUT_file_count", True, "packaged files: %d -> %s" % (len(packaged), packaged), target)

    # ---------------- V1: set arithmetic on the extracted bytes ----------------
    mpath = os.path.join(root, "metadata/manifest.json")
    cpath = os.path.join(root, "metadata/checksums.txt")
    manifest = json.load(open(mpath, encoding="utf-8"))
    cks = {}
    dup_ck = []
    for line in open(cpath, encoding="utf-8"):
        if not line.strip():
            continue
        h, p = line.rstrip("\n").split("  ", 1)
        if p in cks:
            dup_ck.append(p)
        cks[p] = h

    tree_set = set(packaged)
    ck_set = set(cks)
    files_set = set(manifest.get("files", {}))
    rec("V1_checksums_set", ck_set == (tree_set - {"metadata/checksums.txt"}),
        "checksums.txt entries (%d) == packaged files (%d) minus checksums.txt -> %s"
        % (len(ck_set), len(tree_set),
           "equal" if ck_set == (tree_set - {"metadata/checksums.txt"})
           else "diff=%s" % sorted(ck_set.symmetric_difference(tree_set - {"metadata/checksums.txt"}))),
        target)
    rec("V1_files_set", files_set == (ck_set - {"metadata/manifest.json"}),
        "files{} keys (%d) == checksums set (%d) minus manifest.json -> %s"
        % (len(files_set), len(ck_set),
           "equal" if files_set == (ck_set - {"metadata/manifest.json"})
           else "diff=%s" % sorted(files_set.symmetric_difference(ck_set - {"metadata/manifest.json"}))),
        target)
    rec("V1_no_self_hash",
        "metadata/checksums.txt" not in ck_set
        and "metadata/checksums.txt" not in files_set
        and "metadata/manifest.json" not in files_set,
        "checksums self-hash=%s, files{{}} has checksums=%s manifest=%s"
        % ("YES" if "metadata/checksums.txt" in ck_set else "no",
           "YES" if "metadata/checksums.txt" in files_set else "no",
           "YES" if "metadata/manifest.json" in files_set else "no"),
        target)
    bad = []
    for p, h in cks.items():
        full = os.path.join(root, p)
        if not os.path.isfile(full):
            bad.append(p + " missing")
        elif sha256_file(full) != h:
            bad.append(p + " sha")
    rec("V1_checksums_rehash", not bad, "every checksums.txt entry re-hashes to the packaged bytes (bad=%s)" % (bad or "none"), target)
    bad = []
    for p, ent in manifest["files"].items():
        full = os.path.join(root, p)
        if not os.path.isfile(full):
            bad.append(p + " missing")
            continue
        if sha256_file(full) != ent.get("sha256"):
            bad.append(p + " sha")
        if os.path.getsize(full) != ent.get("bytes"):
            bad.append(p + " bytes")
    rec("V1_files_rehash", not bad, "every files{} entry re-hashes to the packaged bytes with the recorded size (bad=%s)" % (bad or "none"), target)
    rec("V1_no_duplicate_checksums", not dup_ck, "duplicate checksums.txt entries: %s" % (dup_ck or "none"), target)

    # ---------------- V2: header identity ----------------
    staged_h = sha256_file(os.path.join(root, "include/songcore.h"))
    src_h = git_blob_sha("native/include/songcore.h", target)
    rec("V2_header_sha",
        staged_h == manifest.get("abi_header_sha256") == src_h == FROZEN_HEADER,
        "packaged=%s manifest=%s source@%s=%s frozen=%s"
        % (staged_h, manifest.get("abi_header_sha256"), SOURCE_COMMIT[:12], src_h, FROZEN_HEADER),
        target)

    # ---------------- V3: pin / closure binding ----------------
    pin = json.loads(git_show("native/ffmpeg/pin.json", target))
    recipe = json.loads(git_show("native/ffmpeg/targets/%s.json" % cfg["recipe"], target))
    profile = json.loads(git_show("native/ffmpeg/profiles/codec-base.json", target))
    pin_sha = git_blob_sha("native/ffmpeg/pin.json", target)
    recipe_sha = git_blob_sha("native/ffmpeg/targets/%s.json" % cfg["recipe"], target)
    profile_sha = git_blob_sha("native/ffmpeg/profiles/codec-base.json", target)
    closure = json.load(open(cfg["closure_manifest"], encoding="utf-8"))
    closure_sha = sha256_file(cfg["closure_manifest"])

    v3 = []
    if manifest.get("ffmpeg_commit") != manifest["pin"]["ffmpeg_commit_sha"]:
        v3.append("ffmpeg_commit carry")
    if manifest.get("ffmpeg_commit") != pin["ffmpeg_commit_sha"]:
        v3.append("ffmpeg_commit vs pin.json")
    if manifest["pin"].get("ffmpeg_tag") != pin["ffmpeg_tag"]:
        v3.append("pin tag vs pin.json")
    if manifest.get("ffmpeg_source_sha256") != manifest["pin"]["ffmpeg_source_sha256"]:
        v3.append("source sha carry")
    if manifest.get("ffmpeg_source_sha256") != pin["source_sha256"]:
        v3.append("source sha vs pin.json")
    if manifest.get("ffmpeg_version") != pin["ffmpeg_version"]:
        v3.append("ffmpeg_version vs pin.json")
    if manifest["pin"].get("pin_sha256") != pin_sha:
        v3.append("pin_sha256 vs committed pin.json")
    if manifest["closure"].get("manifest_sha256") != closure_sha:
        v3.append("closure manifest sha")
    if manifest["closure"].get("configure_args") != closure.get("configure_args"):
        v3.append("configure_args")
    for k in ("system", "machine", "arch"):
        if manifest["closure"]["toolchain"].get(k) != closure["toolchain"].get(k):
            v3.append("closure toolchain.%s" % k)
    if manifest["closure"]["toolchain"].get("cc_ident") != closure["toolchain"].get("cc_ident"):
        v3.append("closure cc_ident")
    for k_man, k_closure in (("translation_units", "translation_units"),
                             ("upstream_sources", "upstream_sources"),
                             ("generated_sources", "generated_sources")):
        if manifest["closure"]["units"].get(k_man) != closure["closure"].get(k_closure):
            v3.append("closure units.%s" % k_man)
    if manifest["closure"].get("reference_archives") != closure.get("reference_archives"):
        v3.append("reference_archives")
    if manifest.get("ffmpeg_profile_sha256") != profile_sha:
        v3.append("profile sha")
    if manifest["target_identity"].get("recipe_sha256") != recipe_sha:
        v3.append("recipe sha")
    if closure.get("target", {}).get("recipe_sha256") != recipe_sha:
        v3.append("closure target.recipe_sha256 vs recipe")
    if closure.get("target", {}).get("id") != cfg["recipe"]:
        v3.append("closure target.id")
    if manifest["target_identity"].get("id") != recipe["id"]:
        v3.append("target_identity.id vs recipe")
    rec("V3_pin_closure", not v3,
        "pin.*/closure.*/profile/recipe agree with the committed inputs and the imported closure; "
        "pin_sha=%s recipe_sha=%s profile_sha=%s closure_sha=%s mismatches=%s"
        % (pin_sha[:12], recipe_sha[:12], profile_sha[:12], closure_sha[:12], v3 or "none"),
        target)

    # pinned zip identity (the O1 basis) where the file is reachable
    zipcand = ["/home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip"]
    for zc in zipcand:
        if os.path.isfile(zc):
            zs = sha256_file(zc)
            rec("V3_pinned_zip", zs == pin["source_sha256"],
                "pinned zip %s sha256=%s vs pin.source_sha256=%s" % (zc, zs, pin["source_sha256"]), target)
            break

    # ---------------- artifact identity ----------------
    static_sha = sha256_file(os.path.join(root, "lib/libsongcore.a"))
    static_bytes = os.path.getsize(os.path.join(root, "lib/libsongcore.a"))
    ok = (static_sha == cfg["expect_static_sha"] == manifest.get("sha256")
          and static_bytes == cfg["expect_static_bytes"] == manifest.get("artifact_bytes"))
    rec("ARTIFACT_static_identity", ok,
        "packaged lib/libsongcore.a sha256=%s bytes=%d vs input=%s/%d and manifest=%s/%s"
        % (static_sha, static_bytes, cfg["expect_static_sha"], cfg["expect_static_bytes"],
           manifest.get("sha256"), manifest.get("artifact_bytes")), target)

    shared_path = os.path.join(root, "lib/shared/libsongcore.so")
    shared_exists = os.path.isfile(shared_path)
    rec("LAYOUT_shared_form", shared_exists == cfg["has_shared"],
        "lib/shared/libsongcore.so present=%s (expected %s)" % (shared_exists, cfg["has_shared"]), target)
    if not cfg["has_shared"]:
        ghosts = [p for p in packaged if p.startswith("lib/shared/") or p.endswith((".so", ".dll", ".dylib"))]
        dirghosts = [d for d in os.listdir(os.path.join(root, "lib")) if d == "shared"]
        rec("LAYOUT_no_fictitious_shared", not ghosts and not dirghosts,
            "paths under lib/shared/ or shared-library members in the windows package: %s; lib/shared dir: %s"
            % (ghosts or "none", dirghosts or "none"), target)
    if shared_exists:
        sh_sha = sha256_file(shared_path)
        rec("ARTIFACT_shared_identity",
            sh_sha == cfg["expect_shared_sha"] == manifest.get("shared_sha256"),
            "packaged lib/shared/libsongcore.so sha256=%s bytes=%d vs input=%s/%d manifest.shared_sha256=%s"
            % (sh_sha, os.path.getsize(shared_path), cfg["expect_shared_sha"],
               cfg["expect_shared_bytes"], manifest.get("shared_sha256")), target)

    # ---------------- member census ----------------
    rc, out, err = run([cfg["objdump"], "-f", os.path.join(root, "lib/libsongcore.a")])
    fmt_hits = len(re.findall(cfg["member_format_re"], out))
    rc2, out2, _ = run([cfg["ar"], "t", os.path.join(root, "lib/libsongcore.a")])
    nmembers = len([l for l in out2.splitlines() if l.strip()])
    rec("ARTIFACT_member_census",
        fmt_hits == nmembers == cfg["expect_members"] and manifest.get("archive_members") == nmembers,
        "objdump -f %d/%d members match '%s'; ar t count=%d; manifest.archive_members=%s"
        % (fmt_hits, nmembers, cfg["member_format_re"], nmembers, manifest.get("archive_members")), target)

    # ---------------- V4: symbols ----------------
    syms_txt = [l.strip() for l in open(os.path.join(root, "metadata/symbols.txt"), encoding="utf-8") if l.strip()]
    rc, out, _ = run([cfg["nm"], "--defined-only", os.path.join(root, "lib/libsongcore.a")])
    defined_global = set()
    defined_local = set()
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[-2] in ("T", "A", "W"):
            defined_global.add(parts[-1])
        elif len(parts) >= 3 and parts[-2] in ("t", "a", "w"):
            defined_local.add(parts[-1])
    static_song_global = sorted(s for s in defined_global if s.startswith("song"))
    static_song_local = sorted(s for s in defined_local if s.startswith("song"))
    missing = sorted(set(ABI_SYMBOLS) - set(static_song_global))
    rec("V4_symbols_static",
        sorted(syms_txt) == sorted(manifest.get("public_symbols", [])) == sorted(ABI_SYMBOLS)
        and not missing and static_song_global == sorted(ABI_SYMBOLS),
        "manifest.public_symbols(%d) == symbols.txt(%d) == contract §3 set(%d); "
        "static global defined song* = %d (missing=%s); static local/internal song* = %s "
        "(contract §3: a static archive necessarily also defines non-ABI internal symbols)"
        % (len(manifest.get("public_symbols", [])), len(syms_txt), len(ABI_SYMBOLS),
           len(static_song_global), missing or "none", static_song_local or "none"), target)

    if cfg["has_shared"]:
        rc, out, _ = run([cfg["nm"], "-D", "--defined-only", os.path.join(root, "lib/shared/libsongcore.so")])
        exported = set()
        for line in out.splitlines():
            parts = line.split()
            if len(parts) >= 3 and parts[-2].upper() in ("T", "W", "D", "B", "R", "V"):
                exported.add(parts[-1])
        non_song = sorted(s for s in exported if not s.startswith("song"))
        song_exp = sorted(s for s in exported if s.startswith("song"))
        rec("V4_symbols_shared",
            song_exp == sorted(ABI_SYMBOLS) and not non_song,
            "shared exported dynamic symbols: %d song* (== contract set: %s), non-song exports: %s"
            % (len(song_exp), song_exp == sorted(ABI_SYMBOLS), non_song or "none"), target)
        rc, out, _ = run([cfg["readelf"], "-d", os.path.join(root, "lib/shared/libsongcore.so")])
        needed = re.findall(r"NEEDED.*\[(.*?)\]", out)
        rec("V4_shared_needed", bool(needed) and not any(n.startswith(("libav", "libsw")) for n in needed),
            "NEEDED=%s (no FFmpeg closure library)" % needed, target)
        rc, out, _ = run([cfg["readelf"], "--dyn-syms", "--wide", os.path.join(root, "lib/shared/libsongcore.so")])
        floors = sorted(set(re.findall(r"GLIBC_[0-9]+(?:\.[0-9]+)*", out)),
                        key=lambda v: [int(x) for x in v.split("_")[1].split(".")])
        rec("V4_glibc_floor", bool(floors), "glibc symbol-version floor=%s" % (floors[-1] if floors else "none"), target)

    # ---------------- V5: license posture ----------------
    forb = [a for a in manifest["closure"]["configure_args"]
            if a.startswith(("--enable-gpl", "--enable-version3", "--enable-nonfree", "--enable-lib"))]
    lic = manifest["license"]
    lic_files = lic.get("license_files", {})
    lic_bad = [n for n, h in lic_files.items()
               if not os.path.isfile(os.path.join(root, "LICENSES", n))
               or sha256_file(os.path.join(root, "LICENSES", n)) != h]
    rec("V5_license",
        not forb and lic.get("gpl_enabled") is False and lic.get("version3_enabled") is False
        and lic.get("nonfree_enabled") is False and lic.get("external_libs") == []
        and len(lic_files) == 4 and not lic_bad,
        "forbidden configure args=%s; gpl/version3/nonfree=%s/%s/%s external_libs=%s; COPYING files bound=%d bad=%s"
        % (forb or "none", lic.get("gpl_enabled"), lic.get("version3_enabled"), lic.get("nonfree_enabled"),
           lic.get("external_libs"), len(lic_files), lic_bad or "none"), target)
    notices = os.path.join(root, "LICENSES/THIRD_PARTY_NOTICES.txt")
    rec("V5_notices_present", os.path.isfile(notices) and "LICENSES/THIRD_PARTY_NOTICES.txt" in cks,
        "THIRD_PARTY_NOTICES.txt present and covered by checksums.txt", target)

    # ---------------- V6: reproducibility claims (preflight) ----------------
    rep = manifest.get("reproducibility", {})
    positive = {"recipe": "RECIPE_RECONSTRUCTIBLE", "semantic_abi": "SEMANTIC_ABI_VERIFIED",
                "byte": "BYTE_REPRODUCIBLE"}
    v6bad = []
    for k, pv in positive.items():
        val = rep.get(k)
        if val == "NOT_ESTABLISHED":
            continue
        if val != pv:
            v6bad.append("%s=%r" % (k, val))
        elif not rep.get("evidence"):
            v6bad.append("%s claims %s without evidence" % (k, pv))
    claimed = [k for k in positive if rep.get(k) == positive[k]]
    rec("V6_reproducibility_preflight",
        not v6bad and not claimed,
        "levels recipe/semantic_abi/byte = %s/%s/%s; no positive level claimed -> preflight only, NOT a RELEASE claim; "
        "evidence field=%r" % (rep.get("recipe"), rep.get("semantic_abi"), rep.get("byte"),
                               (rep.get("evidence") or "")[:120]), target)

    # ---------------- V7: tools + target identity ----------------
    ti = manifest.get("target_identity", {})
    tools = manifest.get("tools", {})
    nonempty = all(isinstance(tools.get(k), str) and tools[k].strip() for k in ("cc", "ar", "xmake"))
    sdk = manifest.get("sdk_ndk", "")
    ok = (nonempty and "not applicable" in sdk
          and ti.get("toolchain", {}).get("family") == recipe["toolchain"]["family"]
          and ti.get("toolchain", {}).get("identity") == recipe["toolchain"]["identity"]
          and ti.get("recipe_sha256") == recipe_sha)
    rec("V7_tools_target_identity", ok,
        "tools={cc:%r, ar:%r, xmake:%r}; sdk_ndk states disposition=%s; target_identity.toolchain=%s/%s vs recipe=%s/%s"
        % (tools.get("cc"), tools.get("ar"), tools.get("xmake"), "not applicable" in sdk,
           ti.get("toolchain", {}).get("family"), ti.get("toolchain", {}).get("identity"),
           recipe["toolchain"]["family"], recipe["toolchain"]["identity"]), target)

    # ---------------- version identity (contract §2) ----------------
    xmake_lua = git_show("native/xmake.lua", target).decode("utf-8", "replace")
    m = re.search(r'set_version\("([^"]+)"\)', xmake_lua)
    xmake_ver = m.group(1) if m else None
    v = manifest.get("songcore_version")
    d = os.path.basename(root)
    rec("VERSION_identity",
        v == xmake_ver and d == "songcore-v%s-%s" % (v, cfg["recipe"]),
        "manifest.songcore_version=%r, package dir=%r, native/xmake.lua set_version=%r (contract §2 equality; "
        "tag leg has no local existence)" % (v, d, xmake_ver), target)

    return True


if __name__ == "__main__":
    which = sys.argv[1:] or ["windows", "linux"]
    for t in which:
        main(t, CFG[t])
    out = {"source_commit": SOURCE_COMMIT, "checks": results}
    op = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package-laneB/laneB-verify.json"
    with open(op, "w", encoding="utf-8", newline="\n") as f:
        json.dump(out, f, indent=2)
        f.write("\n")
    lp = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package-laneB/laneB-verify.log"
    with open(lp, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(loglines) + "\n")
    print("\n".join(loglines))
    fails = [r for r in results if r["status"] != "PASS"]
    print("\nTOTAL=%d PASS=%d FAIL=%d" % (len(results), len(results) - len(fails), len(fails)))
    sys.exit(1 if fails else 0)
