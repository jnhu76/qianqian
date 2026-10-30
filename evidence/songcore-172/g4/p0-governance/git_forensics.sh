#!/usr/bin/env bash
# Read-only git forensics for the #330 G4 P0 governance lane.
set -u
cd /c/Users/fred1/source/qianqian || exit 90
OUT=evidence/songcore-172/g4/p0-governance/git-forensics.log
{
  echo "### HEAD"; git rev-parse HEAD; echo "exit=$?"
  echo "### status"; git status --porcelain | head -20; echo "exit=$?"
  echo
  echo "### [G1] local tag inventory"; git tag -l 'songcore-v*'; echo "exit=$?"
  echo "### [G1] count"; git tag -l 'songcore-v*' | wc -l
  echo "### [G2] cat-file -t songcore-v0.1.0"; git cat-file -t songcore-v0.1.0; echo "exit=$?"
  echo "### [G3] rev-parse songcore-v0.1.0^{commit}"; git rev-parse songcore-v0.1.0^{commit}; echo "exit=$?"
  echo "### [G4] rev-parse songcore-v0.1.0 (tag object)"; git rev-parse songcore-v0.1.0; echo "exit=$?"
  echo "### [G5] tag object body"; git cat-file -p songcore-v0.1.0; echo "exit=$?"
  echo
  echo "### [H1] header blob at tag == blob at HEAD"
  echo "tag_blob=$(git rev-parse songcore-v0.1.0:include/songcore.h)"
  echo "head_blob=$(git rev-parse HEAD:native/include/songcore.h)"
  git diff --stat songcore-v0.1.0:include/songcore.h HEAD:native/include/songcore.h; echo "diff_exit=$?"
  echo "### [H2] sha256 of working-tree header"; sha256sum native/include/songcore.h; echo "exit=$?"
  echo "### [H3] last commits touching the header"; git log --oneline -3 -- native/include/songcore.h
  echo "### [H4] last commits touching the C implementation"; git log --oneline -3 -- native/src/songcore_ffmpeg.c
  echo "### [H5] blame of D1/D3 cited lines"
  git blame -L 218,219 native/include/songcore.h
  git blame -L 357,358 native/include/songcore.h
  git blame -L 823,825 native/src/songcore_ffmpeg.c
  git blame -L 1111,1114 native/src/songcore_ffmpeg.c
  echo "### [H6] did #331 (0b7c5e3) touch native/ ?"
  git show --numstat --format="" 0b7c5e3 | grep -c '^[0-9-].*native/' ; echo "grep_exit=$? (1 = no match => no native/ file touched)"
  echo "### [H7] did #332 (69ba1ce) touch native/include or native/src ?"
  git show --numstat --format="" 69ba1ce | grep -E 'native/(include|src)/' ; echo "grep_exit=$? (1 = no match)"
  echo
  echo "### [M1] ancestry of issue commits in main"
  for c in 43d5e96 742abf8 b49b18c 0b7c5e3 69ba1ce; do
    if git merge-base --is-ancestor "$c" 69ba1ce05a2772f5aa077c6ece3365c40f6e0500; then
      echo "$c ANCESTOR_OF_HEAD=yes"
    else
      echo "$c ANCESTOR_OF_HEAD=NO"
    fi
  done
  echo "### [M2] commit subjects"; for c in 43d5e96 742abf8 b49b18c 0b7c5e3 69ba1ce; do git show -s --format='%H %ci %s' "$c"; done
  echo
  echo "### [L1] git log --oneline --no-merges 69ba1ce -40"
  git log --oneline --no-merges 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 -40; echo "exit=$?"
} > "$OUT" 2>&1
echo "wrote $OUT"
