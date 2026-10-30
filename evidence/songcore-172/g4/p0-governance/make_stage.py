#!/usr/bin/env python3
"""Emit stage.json for the #330 G4 P0 governance lane (hashes computed here)."""
import hashlib, json, os

D = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-governance"

def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()

evidence = [
    "AUDIT.md",
    "audit_raw.log",
    "wsl_release_audit.sh",
    "auth_read.sh",
    "auth-read.log",
    "auth-summary.log",
    "extract_token.sh",
    "parse_rest.py",
    "parse_auth.py",
    "release-summary.json",
    "git_forensics.sh",
    "git-forensics.log",
    "contract-330-sections.txt",
    "contract-extract-window.txt",
    "pr-merge-check.log",
    "rest-release-v0.1.0.json",
    "rest-auth-release-v0.1.0.json",
    "rest-releases-list.json",
    "rest-auth-releases-list.json",
    "rest-auth-releases-page2.json",
    "rest-matching-refs-songcore-v.json",
    "rest-auth-matching-refs.json",
    "rest-tags-page1.json",
    "rest-issue-330.json",
    "rest-330-is-pr.json",
    "rest-pr-326.json",
    "rest-pr-327.json",
    "rest-pr-328.json",
    "rest-pr-331.json",
    "rest-pr-332.json",
    "rest-auth-user.json",
    "rest-auth-repo.json",
    "make_stage.py",
]

stage = {
    "stage_id": "g4-p0-governance",
    "source_sha": "69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
    "input_sha": {
        "docs/release/songcore-release-contract.md": "c26611f72bdbe510fb0baf1f8dd331f71fb12e52962df456ef81dda26b9559a4",
        "native/include/songcore.h": "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536",
        "native/src/songcore_ffmpeg.c": "55eab2432bf2f1959fe13b3f97282895b5918983925f175cad521b0ae5c05ad6",
        "git:songcore-v0.1.0:tag-object": "a1b6c996fcb5d6c7dfea965a11bf3f05b94943fb",
        "git:songcore-v0.1.0:commit": "522c16d283f024c01d1ef56fa96e587b286ca3b8",
        "git:songcore-v0.1.0:include/songcore.h:blob": "3055ca5cdff7c4d40def9e99883cb3bead94c417",
        "release-asset:songcore-v0.1.0-linux-x86_64.tar.gz": "018e0cbdfd8d83b6967bbba8e899ae6dbcc2ccf690c5294bd48e2ef2699f6815",
    },
    "target_toolchain": {
        "host": "Windows 10.0.26300 x64 (Git Bash) + WSL2 Ubuntu-24.04",
        "tools_used": ["git 2.x", "sha256sum", "curl (Windows, network)", "python3 (WSL, offline parse)"],
        "not_used": "no compiler, no linker, no native build in this lane",
    },
    "argv": [
        "sha256sum docs/release/songcore-release-contract.md native/include/songcore.h native/src/songcore_ffmpeg.c",
        "git tag -l 'songcore-v*'",
        "git cat-file -t songcore-v0.1.0",
        "git rev-parse songcore-v0.1.0^{commit}",
        "git rev-parse songcore-v0.1.0",
        "git cat-file -p songcore-v0.1.0",
        "git rev-parse songcore-v0.1.0:include/songcore.h",
        "git rev-parse HEAD:native/include/songcore.h",
        "git diff --stat songcore-v0.1.0:include/songcore.h HEAD:native/include/songcore.h",
        "git blame -L 218,219 native/include/songcore.h",
        "git blame -L 357,358 native/include/songcore.h",
        "git blame -L 823,825 native/src/songcore_ffmpeg.c",
        "git blame -L 1111,1114 native/src/songcore_ffmpeg.c",
        "git show --numstat --format= 0b7c5e3",
        "git show --numstat --format= 69ba1ce",
        "git merge-base --is-ancestor <c> 69ba1ce05a2772f5aa077c6ece3365c40f6e0500  (c = 43d5e96,742abf8,b49b18c,0b7c5e3,69ba1ce)",
        "git log --oneline --no-merges 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 -40",
        "wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/.../g4/p0-governance/wsl_release_audit.sh   (gh route; gh calls exit 1, no network)",
        "curl -sS -H 'Accept: application/vnd.github+json' https://api.github.com/repos/jnhu76/qianqian/releases/tags/songcore-v0.1.0",
        "curl -sS 'https://api.github.com/repos/jnhu76/qianqian/releases?per_page=100'",
        "curl -sS 'https://api.github.com/repos/jnhu76/qianqian/git/matching-refs/tags/songcore-v'",
        "curl -sS 'https://api.github.com/repos/jnhu76/qianqian/tags?per_page=100'",
        "curl -sS 'https://api.github.com/repos/jnhu76/qianqian/issues/330'",
        "curl -sS 'https://api.github.com/repos/jnhu76/qianqian/pulls/330'",
        "curl -sS 'https://api.github.com/repos/jnhu76/qianqian/pulls/{326,327,328,331,332}'",
        "bash evidence/songcore-172/g4/p0-governance/auth_read.sh   (curl -K config; /user, repo, releases, releases/tags, page2, matching-refs)",
        "python3 parse_rest.py",
        "python3 parse_auth.py",
    ],
    "exit_codes": [
        0,  # sha256sum inputs
        0,  # git tag -l
        0,  # git cat-file -t
        0,  # rev-parse ^{commit}
        0,  # rev-parse tag object
        0,  # cat-file -p
        0, 0,  # blob rev-parse x2
        0,  # git diff tag..HEAD header (empty)
        0, 0, 0, 0,  # blame x4
        0,  # show 0b7c5e3
        0,  # show 69ba1ce
        0, 0, 0, 0, 0,  # merge-base --is-ancestor x5 (0 = yes)
        0,  # git log -40
        0,  # wsl audit script (gh inner calls: 1,1,1,1,1,1 and git ls-remote 128 => "unable to resolve host")
        0, 0, 0, 0,  # anonymous REST: http 200,200,200,200
        0, 0,  # issue 330 http 200 / pulls 330 http 404
        0, 0, 0, 0,  # pulls 326/327/328/332 http 200
        0,  # auth_read.sh (6 requests, all http 200)
        0, 0,  # parse_rest.py, parse_auth.py
    ],
    "evidence_paths": ["evidence/songcore-172/g4/p0-governance/" + e for e in evidence],
    "evidence_sha256": [sha(os.path.join(D, e)) for e in evidence],
    "verdict": "PASS",
    "not_run": [
        "gh release view / gh release list / gh api in WSL: NOT_RUN — WSL VM has no network interface (ip addr: only lo; ping 223.5.5.5 -> 'Network is unreachable'), every gh call exits 1 with 'error connecting to api.github.com'. Substitute: the same GitHub REST data read with the same jnhu76 token over Windows curl (authenticated and anonymous payloads byte-identical).",
        "re-download of songcore-v0.1.0-linux-x86_64.tar.gz to re-hash the served bytes: NOT_RUN by choice — the local historical copy already equals the published asset digest (size 2588376, sha256 018e0cbd...6815).",
        "any D1/D3 code or songcore.h fix: NOT_RUN by design — both remain needsOwnerDecision (contract section 16); no coding in this lane.",
        "lefthook run pre-push: NOT_RUN — this lane changes no tracked file (git status --porcelain --untracked-files=no -> 0 lines), so the gate has no subject.",
    ],
}

p = os.path.join(D, "stage.json")
with open(p, "w", encoding="utf-8") as f:
    json.dump(stage, f, indent=2, ensure_ascii=False)
print("wrote", p, "evidence_files=", len(evidence))
