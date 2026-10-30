#!/usr/bin/env bash
# Lane F: direct publish-policy probes (read-only; no repo/tag/release mutation).
set -uo pipefail
cd /home/hoo/Source/qianqian || exit 91
run() {
  local label="$1"; shift
  local out rc
  out=$(python3 tools/check_release.py "$@" 2>&1)
  rc=$?
  printf '%s | argv: check_release.py %s | exit=%s | output: %s\n' "$label" "$*" "$rc" "$out"
}
run A_songcore_tag publish-policy --event push --ref refs/tags/songcore-v0.1.0 --tag songcore-v0.1.0
run B_existing_010 publish-policy --event push --ref refs/tags/qianqian-v0.1.0 --tag qianqian-v0.1.0 --existing
run C_dispatch publish-policy --event workflow_dispatch --ref refs/tags/qianqian-v9.9.9 --tag qianqian-v9.9.9
run D_new_tag_ok publish-policy --event push --ref refs/tags/qianqian-v9.9.9 --tag qianqian-v9.9.9
run E_partial_tag publish-policy --event push --ref refs/tags/songcore-v0.2.0-rc.1 --tag songcore-v0.2.0-rc.1
