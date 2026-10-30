#!/usr/bin/env bash
set -u
export LC_ALL=C
E=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$E/raw"
OUT="$RAW/non-glibc-explicit-check.txt"
{
  echo "== explicit non-GLIBC namespace probe =="
  grep -E '^(GLIBCXX_|CXXABI_|GCC_|GOMP_|LIBATOMIC_|GLIBC_ABI_)' "$RAW/dynsym-version-tags.txt"
  rc=$?
  echo "grep_rc=$rc (1 = none of these namespaces present)"
  echo
  echo "== distinct version namespaces in dynsym tags =="
  sed 's/_.*//' "$RAW/dynsym-version-tags.txt" | sort -u
  echo
  echo "== versioned dynamic symbols: count =="
  grep -c '@' "$RAW/readelf-dyn-syms-so.txt"
  echo
  echo "== SONAME/RPATH bytes (0 = absent) =="
  wc -c < "$RAW/soname-rpath.txt"
  echo
  echo "== undefined dynamic symbols WITHOUT a version tag =="
  awk '$7=="UND" && $8 !~ /@/ {print $8}' "$RAW/readelf-dyn-syms-so.txt" | sort -u
  echo "(end)"
} > "$OUT" 2>&1
cat "$OUT"
