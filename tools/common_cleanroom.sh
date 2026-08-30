#!/usr/bin/env bash
# Clean-room reproduction of the ENTIRE Common Formats ladder (issue #8).
#
# From a bare checkout: verify corpus integrity, re-derive the corpus seek
# calibration on a throwaway full-capability build, then run every stage
# from capability intent through the FFmpeg oracle, Xmake replay, link
# reachability, projection, and the full behavior gate:
#
#   prepare-c5    full-capability calibration build (seek pins --check)
#   c0            MP3+FLAC baseline (regression reference)
#   c1..c5        +AAC / +ALAC / +WAV / +Vorbis / +Opus (each with its own
#                 oracle closure, projection, gate, and size-minimal .so)
#   c6-os         full set at -Os + section flags (archive floor)
#   c6-os-lto     full set at -Os -flto + gc link (size-oriented exe)
#   c6-so-lto     size-minimal shared core (-Os+LTO+PIC+gc+version script)
#   summary       bench/results/common-formats/{summary.json,ladder.md}
#
# All numbers in bench/results/common-formats are derived from THESE runs.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n===== %s =====\n' "$*"; }

step "0. clean build tree and reset persisted xmake config"
rm -rf build
rm -f .xmake/xmake.conf

step "1. corpus integrity"
python3 - <<'EOF'
import sys
sys.path.insert(0, "tools")
from common_corpus import verify_fixtures
verify_fixtures()
print("fixtures verified against manifests")
EOF

step "2. full-capability calibration build (seek-pin check)"
python3 tools/common_import.py --stage prepare-c5 --profile bench/profiles/c5-opus.json
xmake f -m release --av_manifest=build/minimize/prepare-c5/manifest.json --gc_sections=n --lto=n -y
rm -rf build/xmake build/artifacts
xmake build qn_pcm_dump
python3 tools/common_calibrate.py --stage prepare-c5 --check

for s in c0 c1 c2 c3 c4 c5; do
    step "3. ladder stage $s"
    python3 tools/common_stage.py --stage "$s"
    if [ "$s" != "c0" ]; then
        python3 tools/common_compare.py c0 "$s"
    fi
    step "3b. size-minimal .so for $s"
    python3 tools/common_so.py --stage "$s-so" --from-stage "$s"
done

step "4. c6-os: full set at -Os + section flags (archive floor)"
python3 tools/minimize_flags.py --stage c6-os --from-stage c5 \
    --replace-opt Os --add-flag=-ffunction-sections --add-flag=-fdata-sections
xmake f -m release --av_manifest=build/minimize/c6-os/manifest-projected.json --gc_sections=y --lto=n -y
rm -rf build/xmake build/artifacts
xmake build qn_pcm_dump
python3 tools/common_gate.py --stage c6-os

step "5. c6-os-lto: size-oriented linked executable (-Os -flto + gc)"
python3 tools/minimize_flags.py --stage c6-os-lto --from-stage c5 \
    --replace-opt Os --add-flag=-flto --add-flag=-ffunction-sections --add-flag=-fdata-sections
xmake f -m release --av_manifest=build/minimize/c6-os-lto/manifest-projected.json --gc_sections=y --lto=y -y
rm -rf build/xmake build/artifacts
xmake build qn_pcm_dump
python3 tools/common_gate.py --stage c6-os-lto

step "6. c6-so-lto: size-minimal shared core"
python3 tools/common_so.py --stage c6-so-lto --from-stage c5 --lto

step "7. summary (all numbers from gate.json / so.json, zero hand-entry)"
python3 tools/common_summary.py

step "7b. capability attribution (C0 -> C1 live .so bytes, machine-derived)"
python3 tools/common_attribution.py --base c0-so --stage c1-so

step "8. markdown authority check (ladder.md / PR_BODY.md == summary.json derivation)"
python3 tools/common_summary.py --check
