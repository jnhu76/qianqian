#!/usr/bin/env python3
"""Fail-closed SongCore build-receipt verifier used by package gates."""
from __future__ import annotations
import argparse, hashlib, json, os, sys

def digest(path: str) -> tuple[str, int]:
    h=hashlib.sha256(); size=0
    with open(path,"rb") as stream:
        for block in iter(lambda:stream.read(1024*1024),b""): h.update(block); size+=len(block)
    return h.hexdigest(),size

def fail(message: str) -> None:
    print(f"FAIL: {message}",file=sys.stderr); raise SystemExit(1)

def field(obj, path: str):
    cur=obj
    for part in path.split("."):
        if not isinstance(cur,dict) or part not in cur: fail(f"receipt missing {path}")
        cur=cur[part]
    return cur

def main() -> int:
    p=argparse.ArgumentParser()
    p.add_argument("--receipt",required=True); p.add_argument("--source-commit",required=True)
    p.add_argument("--artifact",required=True); p.add_argument("--header",required=True)
    p.add_argument("--recipe",required=True); p.add_argument("--profile",required=True)
    p.add_argument("--pin",required=True); p.add_argument("--closure",required=True)
    p.add_argument("--target",required=True); p.add_argument("--toolchain",required=True)
    p.add_argument("--source-tree",required=True); p.add_argument("--artifact-shared")
    p.add_argument("--package-manifest")
    a=p.parse_args()
    if not os.path.isfile(a.receipt): fail(f"receipt missing: {a.receipt}")
    try: receipt=json.load(open(a.receipt,encoding="utf-8"))
    except (OSError,json.JSONDecodeError) as e: fail(f"receipt unreadable: {e}")
    if field(receipt,"schema_version") != 1: fail("receipt schema_version")
    if field(receipt,"qualification") != "RELEASE_QUALIFIED": fail("receipt is not RELEASE_QUALIFIED")
    if field(receipt,"source.commit") != a.source_commit: fail("source.commit mismatch")
    tree=field(receipt,"source.tree_oid")
    if tree != a.source_tree: fail("source tree identity mismatch")
    if field(receipt,"target.id") != a.target: fail("target identity mismatch")
    if field(receipt,"toolchain.identity") != a.toolchain: fail("toolchain identity mismatch")
    for name,path in (("header",a.header),("recipe",a.recipe),("profile",a.profile),("pin",a.pin),("closure",a.closure)):
        actual,_=digest(path); expected=field(receipt,f"inputs.{name}.sha256")
        if actual != expected: fail(f"{name} sha256 mismatch")
    actual_sha,actual_bytes=digest(a.artifact)
    if actual_sha != field(receipt,"artifacts.static.sha256"): fail("artifact sha256 mismatch")
    if actual_bytes != field(receipt,"artifacts.static.bytes"): fail("artifact bytes mismatch")
    if a.artifact_shared:
        shared_sha,shared_bytes=digest(a.artifact_shared)
        if shared_sha != field(receipt,"artifacts.shared.sha256"): fail("shared artifact sha256 mismatch")
        if shared_bytes != field(receipt,"artifacts.shared.bytes"): fail("shared artifact bytes mismatch")
    actions=field(receipt,"execution.compile_actions")
    cache=field(receipt,"execution.cache_reuse")
    if not isinstance(actions,int) or actions < 0 or not isinstance(cache,bool): fail("execution evidence malformed")
    if actions <= 0 or cache: fail("release-qualified receipt requires observed compile actions and no cache reuse")
    if a.package_manifest:
        try: manifest=json.load(open(a.package_manifest,encoding="utf-8"))
        except (OSError,json.JSONDecodeError) as e: fail(f"package manifest unreadable: {e}")
        if field(manifest,"source.commit") != a.source_commit: fail("manifest source.commit mismatch")
    print(f"VERIFIED source={a.source_commit} artifact_sha256={actual_sha} bytes={actual_bytes}")
    return 0
if __name__ == "__main__": raise SystemExit(main())
