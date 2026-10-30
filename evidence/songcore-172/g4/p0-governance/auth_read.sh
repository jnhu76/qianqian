#!/usr/bin/env bash
# Authenticated read of the same GitHub endpoints gh would call, using the
# existing gh token via a curl config file (never on the command line).
# Read-only GETs. Temp credential material is deleted at the end.
set -u
TOKF="/c/Users/fred1/AppData/Local/Temp/gh_token_q172.txt"
CFGF="/c/Users/fred1/AppData/Local/Temp/gh_curl_q172.cfg"
OUT="C:/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-governance"
API="https://api.github.com/repos/jnhu76/qianqian"

if [ ! -s "$TOKF" ]; then echo "NO_TOKEN_FILE"; exit 3; fi
TOKEN=$(cat "$TOKF")
{
  printf 'header = "Authorization: token %s"\n' "$TOKEN"
  printf 'header = "Accept: application/vnd.github+json"\n'
  printf 'silent\nshow-error\n'
} > "$CFGF"
unset TOKEN
echo "cfg_written=$(test -s "$CFGF" && echo yes)"

run() { # name url
  local name=$1 url=$2
  curl -K "$CFGF" -m 40 -o "$OUT/$name" -w "%{http_code}" "$url" > "/tmp/code_$name"
  echo "$name http=$(cat /tmp/code_$name) bytes=$(wc -c < "$OUT/$name")"
}

echo "### B1 /user (identity of the authenticated account)"
run "rest-auth-user.json" "https://api.github.com/user"
echo "### B2 repo permissions"
run "rest-auth-repo.json" "$API"
echo "### B3 releases?per_page=100 (includes drafts when authenticated)"
run "rest-auth-releases-list.json" "$API/releases?per_page=100"
echo "### B4 release by tag (authenticated, compare with anonymous)"
run "rest-auth-release-v0.1.0.json" "$API/releases/tags/songcore-v0.1.0"
echo "### B5 repos/{}/releases?per_page=100&page=2 (prove the list is complete)"
run "rest-auth-releases-page2.json" "$API/releases?per_page=100&page=2"
echo "### B6 matching-refs songcore-v* (authenticated)"
run "rest-auth-matching-refs.json" "$API/git/matching-refs/tags/songcore-v"

shred -u "$CFGF" 2>/dev/null || rm -f "$CFGF"
rm -f "$TOKF"
echo "cleanup cfg_exists=$(test -e "$CFGF" && echo STILL_THERE || echo removed) token_exists=$(test -e "$TOKF" && echo STILL_THERE || echo removed)"
