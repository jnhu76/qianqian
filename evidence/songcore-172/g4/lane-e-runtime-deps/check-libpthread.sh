#!/usr/bin/env bash
set -u
P=/usr/lib/x86_64-linux-gnu/libpthread.so.0
OUT=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps/raw/libpthread-stub-check.txt
{
  echo "== libpthread.so.0 on the build host =="
  ls -l "$P"
  echo
  echo "== defined dynamic symbols: total / non-ABS (real definitions) =="
  nm -D --defined-only "$P" | wc -l
  nm -D --defined-only "$P" | awk '$2 != "A"' | wc -l
  echo
  echo "== all defined dynamic symbols =="
  nm -D --defined-only "$P"
  echo
  echo "== readelf -d (first 12 entries) =="
  readelf -d "$P" | head -14
} > "$OUT" 2>&1
cat "$OUT"
