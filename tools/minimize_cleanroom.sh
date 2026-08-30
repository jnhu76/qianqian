#!/usr/bin/env bash
# Clean-room reproduction of the source-minimization ladder finals.
#
# From a bare checkout: fetch pinned FFmpeg, derive every accepted stage's
# closure from the upstream oracle, replay through Xmake, and run the full
# correctness gate per stage. No prior build state is consulted; the script
# deletes build/ before starting.
#
# Stages reproduced:
#   s0           canonical n3-min-noswr baseline (205 TUs)
#   s3-pthreads  minimal-archive candidate (iconv+pthreads removed, projected)
#   s5-Os-LTO    minimal shipped-linked candidate (-Os -flto, linked with
#                --gc-sections; archive itself is LTO bytecode, not minimal)
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n===== %s =====\n' "$*"; }

step "0. clean build tree"
rm -rf build

step "1. canonical import (S0 baseline) + oracle archives for verify"
xmake ffmpeg-import
# reset all mutable options explicitly: .xmake/xmake.conf survives rm -rf build
xmake f -m release --av_manifest=build/ffmpeg-xmake/manifest.json --gc_sections=n --lto=n -y
xmake build qn_pcm_dump
python3 tools/minimize_gate.py --stage s0

step "2. minimal-archive candidate: iconv+pthreads variant closure, projected"
python3 tools/config_experiment.py --stage s3-pthreads \
    --add-disable iconv --add-disable pthreads \
    --description "clean-room composition of accepted dimensions: --disable-iconv + --disable-pthreads"
python3 tools/minimize_run_stage.py --stage s3-pthreads --base s0

step "3. minimal shipped-linked candidate: -Os -flto + --gc-sections link"
python3 tools/minimize_flags.py --stage s5-Os-LTO --from-stage s3-pthreads \
    --replace-opt Os --add-flag=-flto
xmake f -m release --av_manifest=build/minimize/s5-Os-LTO/manifest-projected.json \
    --gc_sections=y --lto=y -y
rm -rf build/xmake build/artifacts
xmake build qn_pcm_dump
python3 tools/minimize_gate.py --stage s5-Os-LTO
python3 tools/minimize_compare.py s0 s5-Os-LTO

step "4. summary"
python3 - <<'EOF'
import json
for stage in ("s0", "s3-pthreads", "s5-Os-LTO"):
    g = json.load(open(f"build/minimize/{stage}/gate.json"))
    s = g["sizes"]
    print(f"{stage:12s} archive={s['libqianqian_av_a_bytes']:>9,} members={s['libqianqian_av_members']:>3} "
          f"linked={s['qn_pcm_dump_bytes']:>7,} corpus={g['verify']['verdict']} "
          f"strict_failures={len(g['strict_failures'])}")
EOF
