#!/usr/bin/env bash
# Tree identity + deterministic tree digests for the two staged trees.
set -u
declare -A A
A[linux_wsl]=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
A[linux_win]=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64
A[win_wsl]=/home/hoo/g4-172/laneB-stage/windows/songcore-v0.0.0-preflight-windows-x86_64
A[win_win]=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64

echo "== diff -r linux (wsl stage vs windows-visible mirror) =="
diff -r "${A[linux_wsl]}" "${A[linux_win]}"; echo "diff_linux_exit=$?"
echo "== diff -r windows (wsl stage vs windows-visible mirror) =="
diff -r "${A[win_wsl]}" "${A[win_win]}"; echo "diff_win_exit=$?"

for k in linux_win win_win; do
  echo "== tree digest $k (${A[$k]}) =="
  ( cd "${A[$k]}" && find . -type f | sed 's#^\./##' | LC_ALL=C sort | while read -r f; do
      printf '%s  %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$f"
    done | tee /tmp/tree_$k.txt | sha256sum
  )
  echo "files=$(wc -l < /tmp/tree_$k.txt)"
  echo "tree_digest_exit=$?"
done

echo "== windows package: any .so / shared dir? =="
find "${A[win_win]}" -iname '*.so' -o -iname '*shared*'; echo "find_shared_exit=$? (no output above = none)"
echo "== file counts =="
for k in linux_win win_win; do printf '%s %s\n' "$k" "$(find "${A[$k]}" -type f | wc -l)"; done
