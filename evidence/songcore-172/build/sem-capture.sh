#!/bin/bash
# songcore-172 D2: capture sem experiment outputs into files and copy all
# WSL-side evidence back to the worktree evidence dir.
set -u
export PATH=/home/hoo/.local/bin:$PATH
CAP=/home/hoo/q172d2/captured
mkdir -p "$CAP"

echo "== re-running sem matrix (capture) =="
bash /home/hoo/tmp/sem-matrix-lf.sh > "$CAP/sem-matrix-output.txt" 2>&1
echo "sem-matrix exit=$?"
bash /home/hoo/tmp/sem6-lf.sh > "$CAP/sem6-multi-dep-output.txt" 2>&1
echo "sem6 exit=$?"
bash /home/hoo/tmp/sem8-lf.sh > "$CAP/sem8-onconfig-output.txt" 2>&1
echo "sem8 exit=$?"
bash /home/hoo/tmp/sem9-lf.sh > "$CAP/sem9-norecipe-output.txt" 2>&1
echo "sem9 exit=$?"

echo "== copying evidence into worktree =="
EV=/mnt/c/Users/fred1/source/qianqian-172/evidence/songcore-172
mkdir -p "$EV/build" "$EV/audit" "$EV/semantics"
cp -f /home/hoo/q172d2/red-build.log "$EV/build/red-windows-songcore-build.log"
cp -f /home/hoo/q172d2/red-closure-provenance.txt "$EV/build/red-closure-provenance.txt"
cp -f /home/hoo/q172d2/green/*.log "$EV/build/" 2>/dev/null
cp -f /home/hoo/q172d2/green/objdump-f.txt "$EV/build/windows-objdump-f.txt" 2>/dev/null
cp -f /home/hoo/q172d2/linux-green/*.log "$EV/build/" 2>/dev/null
cp -f /home/hoo/q172d2/linux-green/prefix-artifacts.txt "$EV/build/" 2>/dev/null
cp -f /home/hoo/q172d2/linux-green/postfix-artifacts.txt "$EV/build/" 2>/dev/null
cp -f /home/hoo/q172d2/linux-green/prefix-so-songcount.txt "$EV/build/" 2>/dev/null
cp -f /home/hoo/q172d2/linux-green/postfix-so-songcount.txt "$EV/build/" 2>/dev/null
cp -f /home/hoo/q172d2/audit/* "$EV/audit/" 2>/dev/null
rm -f "$EV/audit/consumer/consumer.c" 2>/dev/null
cp -f "$CAP/"*.txt "$EV/semantics/" 2>/dev/null
cp -f /home/hoo/q172d2/audit/consumer/consumer.c "$EV/audit/abi-complete-consumer.c" 2>/dev/null || true
echo "== evidence dir contents =="
ls -la "$EV" "$EV/build" "$EV/audit" "$EV/semantics" | head -60
echo CAPTURE_DONE
