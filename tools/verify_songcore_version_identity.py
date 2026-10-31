#!/usr/bin/env python3
import argparse, hashlib, json, re, sys
from pathlib import Path

SEMVER = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
AUTHORITY = re.compile(r"https://github\.com/jnhu76/qianqian/issues/333#issuecomment-[1-9][0-9]*")

def fail(message):
    print(f"VERSION_IDENTITY_RED: {message}", file=sys.stderr); return 1

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--decision", required=True); p.add_argument("--version", required=True)
    p.add_argument("--target", choices=("linux-x86_64","windows-x86_64"), required=True)
    p.add_argument("--xmake", required=True)
    a=p.parse_args(); d=json.loads(Path(a.decision).read_text(encoding="utf-8"))
    if d.get("schema_version") != 1: return fail("decision schema is not 1")
    if d.get("status") != "APPROVED": return fail("Q1 version identity is OWNER_BLOCKED")
    ref=d.get("authority_ref")
    if not isinstance(ref,str) or not AUTHORITY.fullmatch(ref): return fail("authority_ref is not a #333 Owner decision comment")
    digest=d.get("authority_record_sha256")
    if not isinstance(digest,str) or not re.fullmatch(r"[0-9a-f]{64}",digest): return fail("authority_record_sha256 is missing or invalid")
    v=d.get("semantic_version")
    if v != a.version or not isinstance(v,str) or not SEMVER.fullmatch(v): return fail("requested version is not the approved semantic version")
    xtext=Path(a.xmake).read_text(encoding="utf-8")
    matches=re.findall(r'^\s*set_version\("([^\"]+)"\)',xtext,re.M)
    if len(matches) != 1: return fail("native/xmake.lua must contain exactly one literal set_version")
    actual_xmake=matches[0]
    if d.get("xmake_project_version") != actual_xmake: return fail("decision xmake_project_version does not match native/xmake.lua")
    expected={"component_tag":f"songcore-v{v}","manifest_version":v,
      "linux_package_root":f"songcore-v{v}-linux-x86_64",
      "windows_package_root":f"songcore-v{v}-windows-x86_64","component_version":v}
    for k,val in expected.items():
        if d.get(k) != val: return fail(f"normalized mapping mismatch: {k}")
    mode=d.get("component_version_mode")
    if mode not in ("independent","project-coupled"): return fail("component_version_mode is not approved")
    if mode == "project-coupled" and actual_xmake != v: return fail("project-coupled mode differs from native/xmake.lua")
    print("VERSION_IDENTITY_GREEN")
    return 0
if __name__ == "__main__": raise SystemExit(main())
