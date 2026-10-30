#!/usr/bin/env bash
# Lane F docs-verify evidence. AGENTS.md documents `cmd /c <tools>\docs-verify.cmd <root>`,
# but no such file exists at 69ba1ce; run the lefthook.yml docs-verify body instead
# (website/scripts/verify.mjs + npx vitepress build). node_modules already present.
set -uo pipefail
ROOT=/c/Users/fred1/source/qianqian
EVID="$ROOT/evidence/songcore-172/g4/release-policy"
cd "$ROOT/website" || exit 91
{
  echo "source_sha: $(git -C "$ROOT" rev-parse HEAD)"
  echo "node: $(node --version)  npm: $(npm --version)"
  echo "node_modules_present: $([ -d node_modules ] && echo yes || echo no)"
  echo "lockfile_package_entries: $(node -e "console.log(Object.keys(require('./package-lock.json').packages||{}).length)")"
  echo "node_modules_top_level_dirs: $(ls -d node_modules/*/ 2>/dev/null | wc -l)"
  echo "start_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$EVID/docs-verify.meta.txt"
node scripts/verify.mjs > "$EVID/docs-verify-verify.mjs.log" 2>&1
vrc=$?
npx vitepress build > "$EVID/docs-verify-vitepress-build.log" 2>&1
brc=$?
{
  echo "argv1: node scripts/verify.mjs   (cwd=$ROOT/website) exit_code=$vrc"
  echo "argv2: npx vitepress build       (cwd=$ROOT/website) exit_code=$brc"
  echo "end_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >> "$EVID/docs-verify.meta.txt"
echo "verify_exit=$vrc build_exit=$brc"
echo "--- verify tail ---"; tail -n 8 "$EVID/docs-verify-verify.mjs.log"
echo "--- build tail ---"; tail -n 8 "$EVID/docs-verify-vitepress-build.log"
