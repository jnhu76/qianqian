#!/usr/bin/env python3
"""Parse the anonymously-fetched GitHub REST JSON for the #330 G4 P0 audit.
Read-only: reads evidence JSON, prints a normalized summary, writes summary json."""
import json, sys, hashlib, os

BASE = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-governance"
out = {}

rel = json.load(open(os.path.join(BASE, "rest-release-v0.1.0.json"), encoding="utf-8"))
out["release"] = {
    "tag_name": rel.get("tag_name"),
    "name": rel.get("name"),
    "published_at": rel.get("published_at"),
    "created_at": rel.get("created_at"),
    "target_commitish": rel.get("target_commitish"),
    "draft": rel.get("draft"),
    "prerelease": rel.get("prerelease"),
    "id": rel.get("id"),
    "author": (rel.get("author") or {}).get("login"),
    "html_url": rel.get("html_url"),
    "assets": [
        {
            "name": a.get("name"),
            "size": a.get("size"),
            "state": a.get("state"),
            "digest": a.get("digest"),
            "content_type": a.get("content_type"),
            "created_at": a.get("created_at"),
            "updated_at": a.get("updated_at"),
            "download_count": a.get("download_count"),
        }
        for a in rel.get("assets", [])
    ],
}
print("=== RELEASE songcore-v0.1.0 ===")
print(json.dumps(out["release"], indent=2, ensure_ascii=False))

rl = json.load(open(os.path.join(BASE, "rest-releases-list.json"), encoding="utf-8"))
out["releases_total"] = len(rl)
out["releases"] = [
    {"tag": r.get("tag_name"), "name": r.get("name"), "draft": r.get("draft"),
     "prerelease": r.get("prerelease"), "published_at": r.get("published_at"),
     "assets": [a.get("name") for a in r.get("assets", [])]}
    for r in rl
]
print("\n=== RELEASES (total=%d) ===" % len(rl))
for r in out["releases"]:
    print("%-28s draft=%s pre=%s %s assets=%s" % (r["tag"], r["draft"], r["prerelease"], r["published_at"], r["assets"]))
out["songcore_releases"] = [r["tag"] for r in out["releases"] if r["tag"].startswith("songcore-v")]

mr = json.load(open(os.path.join(BASE, "rest-matching-refs-songcore-v.json"), encoding="utf-8"))
out["matching_refs"] = [{"ref": x.get("ref"), "type": x.get("object", {}).get("type"),
                         "sha": x.get("object", {}).get("sha")} for x in mr]
print("\n=== matching-refs tags/songcore-v (total=%d) ===" % len(mr))
for x in out["matching_refs"]:
    print("%-40s %-6s %s" % (x["ref"], x["type"], x["sha"]))
out["songcore_tags_total"] = len(mr)

tg = json.load(open(os.path.join(BASE, "rest-tags-page1.json"), encoding="utf-8"))
out["tags_page1_total"] = len(tg)
out["tags_page1_songcore"] = [t.get("name") for t in tg if str(t.get("name","")).startswith("songcore-v")]
print("\n=== /tags page1 (n=%d) ===" % len(tg))
print("songcore-v* on page:", out["tags_page1_songcore"])
print("all names:", [t.get("name") for t in tg])

dst = os.path.join(BASE, "release-summary.json")
with open(dst, "w", encoding="utf-8") as f:
    json.dump(out, f, indent=2, ensure_ascii=False)
print("\nwrote", dst)
