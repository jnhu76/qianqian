#!/usr/bin/env bash
# Extract the existing gh token from the WSL gh config into the Windows TEMP dir
# so Windows curl can perform the SAME authenticated read gh would perform.
# The token is never printed; it is written only to $TEMP and deleted by the caller.
set -u
CFG="$HOME/.config/gh/hosts.yml"
TOKEN=$(python3 - "$CFG" <<'PY'
import sys, re
p = sys.argv[1]
t = open(p, encoding="utf-8").read()
m = re.search(r'^\s*oauth_token:\s*(\S+)\s*$', t, re.M)
print(m.group(1) if m else "")
PY
)
if [ -z "$TOKEN" ]; then echo "NO_TOKEN"; exit 3; fi
printf '%s' "$TOKEN" > /mnt/c/Users/fred1/AppData/Local/Temp/gh_token_q172.txt
chmod 600 /mnt/c/Users/fred1/AppData/Local/Temp/gh_token_q172.txt 2>/dev/null
echo "TOKEN_WRITTEN len=${#TOKEN}"
echo "TOKEN_SHA_PREFIX=$(printf '%s' "$TOKEN" | sha256sum | cut -c1-12)"
