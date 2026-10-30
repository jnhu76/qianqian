#!/usr/bin/env bash
# probe-evidence-inventory.sh — read-only inventory of the lane receipts.
set -uo pipefail
export LC_ALL=C
EV=/home/hoo/g4-172/linux-root/receipts/g4-linux-build
echo "== tree =="
find "$EV" -maxdepth 2 -type f -printf '%10s  %p\n' | sort -k2
echo "== needed =="
cat "$EV/verify/needed-libs.txt"
echo "== abi expected =="
cat "$EV/verify/abi-expected.txt"
echo "== abi so =="
cat "$EV/verify/abi-exported-so.txt"
echo "== abi a =="
cat "$EV/verify/abi-exported-a.txt"
echo "== diff sizes =="
stat -c '%s %n' "$EV/verify/abi-diff-so.txt" "$EV/verify/abi-diff-a.txt"
echo "== file identity =="
cat "$EV/verify/file-identity.txt"
echo "== invocations =="
cat "$EV/invocations.tsv"
echo "== facts =="
cat "$EV/facts.env"
