#!/usr/bin/env bash
# Lane D driver: run every check script and persist its stdout+exit code.
set -u
D=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-d
mkdir -p "$D/logs"

run() {
  name=$1; script=$2
  out="$D/logs/$name.log"
  : > "$out"
  bash "$D/$script" >> "$out" 2>&1
  rc=$?
  {
    echo ""
    echo "----"
    echo "script=$script"
    echo "exit_code=$rc"
  } >> "$out"
  echo "$name exit=$rc"
}

run 01-extract-license-sources   extract-license-sources.sh
run 02-archive-audit            audit-archives.sh
run 03-notices-template         check-notices-template.sh
run 04-notices-template-v       render-v-version.sh
run 05-notices-diff             diff-notices.sh
run 06-closure-provenance       closure-provenance.sh
run 07-package-integrity        verify-package-integrity.sh
run 08-closure-locations        find-closure.sh

echo "== log sha256 =="
sha256sum "$D"/logs/*.log
