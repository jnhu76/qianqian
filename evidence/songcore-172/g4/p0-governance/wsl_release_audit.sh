#!/usr/bin/env bash
# Read-only governance/release audit for #330 G4 P0. No writes to the repo,
# no tag/release creation, no asset mutation.
set -u
REPO=jnhu76/qianqian
TAG=songcore-v0.1.0
OUT=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-governance
cd /home/hoo/Source/qianqian || exit 90

exec > >(tee "$OUT/audit_raw.log") 2>&1
echo "### HOST"; uname -a
echo "### DATE"; date -u +%Y-%m-%dT%H:%M:%SZ
echo "### GH VERSION"; gh --version | head -2
echo "### WSL REPO HEAD"; git -C /home/hoo/Source/qianqian rev-parse HEAD

echo "### A1 gh release view (json)"
gh release view "$TAG" --repo "$REPO" --json tagName,name,publishedAt,assets,isDraft,isPrerelease > "$OUT/gh-release-view.json"
echo "A1_EXIT=$?"
cat "$OUT/gh-release-view.json"

echo "### A1b asset table (name size digest)"
gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[] | [.name, (.size|tostring), .state, .digest] | @tsv'
echo "A1b_EXIT=$?"

echo "### A2 gh api release tag commit"
gh api "repos/$REPO/releases/tags/$TAG" --jq '[.tag_name, .target_commitish, .author.login, .published_at] | @tsv'
echo "A2_EXIT=$?"

echo "### A3 git tag object / commit (WSL clone)"
git -C /home/hoo/Source/qianqian cat-file -t "$TAG"; echo "A3a_EXIT=$?"
git -C /home/hoo/Source/qianqian rev-parse "$TAG^{commit}"; echo "A3b_EXIT=$?"
git -C /home/hoo/Source/qianqian rev-parse "$TAG"; echo "A3c_EXIT=$?"

echo "### A4 git tag -l 'songcore-v*'"
git -C /home/hoo/Source/qianqian tag -l 'songcore-v*'; echo "A4_EXIT=$?"
echo "COUNT="; git -C /home/hoo/Source/qianqian tag -l 'songcore-v*' | wc -l

echo "### A5 gh release list"
gh release list --repo "$REPO" --limit 30; echo "A5_EXIT=$?"
echo "### A5b release list json"
gh release list --repo "$REPO" --limit 30 --json tagName,isDraft,isPrerelease,publishedAt --jq '.[] | [.tagName, (.isDraft|tostring), (.isPrerelease|tostring), .publishedAt] | @tsv'
echo "A5b_EXIT=$?"

echo "### A6 remote songcore-v* refs"
gh api "repos/$REPO/git/matching-refs/tags/songcore-v" --jq '.[] | [.ref, .object.type, .object.sha] | @tsv'; echo "A6_EXIT=$?"

echo "### A7 remote tags list (git ls-remote, filtered)"
git ls-remote --tags https://github.com/$REPO.git 'refs/tags/songcore-v*'; echo "A7_EXIT=$?"

echo "### DONE"
