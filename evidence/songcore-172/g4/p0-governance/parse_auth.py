#!/usr/bin/env python3
"""Summarize the authenticated GitHub reads for the #330 G4 P0 governance lane."""
import json, os, hashlib

B = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-governance/"
def load(n): return json.load(open(B + n, encoding="utf-8"))

u = load("rest-auth-user.json")
print("AUTH_USER login=%s id=%s type=%s" % (u.get("login"), u.get("id"), u.get("type")))

r = load("rest-auth-repo.json")
print("REPO full_name=%s private=%s default_branch=%s" % (r.get("full_name"), r.get("private"), r.get("default_branch")))
print("REPO permissions:", r.get("permissions"))

rel = load("rest-auth-releases-list.json")
print("\nAUTHENTICATED RELEASES total=%d" % len(rel))
for x in rel:
    print("  %-22s draft=%-5s pre=%-5s published=%s assets=%s" % (
        x.get("tag_name"), x.get("draft"), x.get("prerelease"), x.get("published_at"),
        [(a.get("name"), a.get("size"), a.get("state")) for a in x.get("assets", [])]))
drafts = [x.get("tag_name") for x in rel if x.get("draft")]
print("DRAFTS:", drafts if drafts else "none")
print("songcore releases:", [x.get("tag_name") for x in rel if str(x.get("tag_name","")).startswith("songcore-v")])

p2 = json.load(open(B + "rest-auth-releases-page2.json", encoding="utf-8"))
print("PAGE2 len=%d (0 proves the per_page=100 list is complete)" % len(p2))

a = load("rest-release-v0.1.0.json")          # anonymous
b = load("rest-auth-release-v0.1.0.json")     # authenticated
def canon(d):
    keep = ("id","tag_name","name","published_at","created_at","draft","prerelease","target_commitish")
    o = {k: d.get(k) for k in keep}
    o["assets"] = sorted([{k: x.get(k) for k in ("name","size","state","digest","created_at","updated_at","updated_at")} for x in d.get("assets", [])], key=lambda y: y["name"])
    o["body_sha256"] = hashlib.sha256((d.get("body") or "").encode()).hexdigest()
    return o
ca, cb = canon(a), canon(b)
print("\nANON vs AUTH release object identical:", ca == cb)
if ca != cb:
    for k in ca:
        if ca[k] != cb[k]:
            print("  differs:", k, ca[k], "!=", cb[k])

print("\nASSET:", [(x.get("name"), x.get("size"), x.get("digest"), x.get("created_at"), x.get("updated_at")) for x in b.get("assets", [])])
print("release updated_at=%s created_at=%s published_at=%s" % (b.get("updated_at"), b.get("created_at"), b.get("published_at")))
print("body_sha256=%s" % hashlib.sha256((b.get("body") or "").encode()).hexdigest())
print("target_commitish=%s" % b.get("target_commitish"))
