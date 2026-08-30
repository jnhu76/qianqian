#!/usr/bin/env bash
# Clean-room reproduction of the ENTIRE source-minimization ladder.
#
# From a bare checkout: fetch pinned FFmpeg, derive every accepted stage's
# closure from the upstream oracle, replay through Xmake, and run the full
# correctness gate per stage. No prior build state is consulted; the script
# deletes build/ before starting and explicitly resets all mutable xmake
# options (.xmake/xmake.conf survives rm -rf build).
#
# Stages reproduced (all numbers in bench/results/source-minimization are
# derived from THESE runs, never hand-entered):
#   s0           canonical n3-min-noswr baseline (205 TUs)
#   s3-pthreads  minimal-archive candidate (iconv+pthreads removed, projected
#                by the S1 link-reachability audit with the strict global-only
#                symbol resolver)
#   s4-full-gc   CONTROL: full 205-TU closure + section flags + --gc-sections
#                (same -ffunction-sections as the minimal stage — gc needs
#                them to have anything to collect; isolates what source
#                trimming alone contributes on a GC'd link)
#   s4-gc        minimal closure + section flags + --gc-sections
#   s5-Os        minimal closure at -Os, no GC/LTO (conventional archive floor)
#   s5-Os-LTO    minimal shipped-linked candidate (-Os -flto, --gc-sections)
#   s6-shipped-so  libqianqian_songcore.so: PIC closure + version script
#                  exporting only the five SongCore entry points
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n===== %s =====\n' "$*"; }

configure_and_build() { # <manifest> <gc> <lto>
    xmake f -m release --av_manifest="$1" --gc_sections="$2" --lto="$3" -y
    rm -rf build/xmake build/artifacts
    xmake build qn_pcm_dump
}

step "0. clean build tree"
rm -rf build

step "1. canonical import (S0 baseline)"
xmake ffmpeg-import
configure_and_build build/ffmpeg-xmake/manifest.json n n
python3 tools/minimize_gate.py --stage s0

step "2. minimal source closure: iconv+pthreads variant, projected by S1 audit"
python3 tools/config_experiment.py --stage s3-pthreads \
    --add-disable iconv --add-disable pthreads \
    --description "clean-room composition of accepted dimensions: --disable-iconv + --disable-pthreads"
python3 tools/minimize_run_stage.py --stage s3-pthreads --base s0

step "3. CONTROL: full 205-TU closure + section flags + --gc-sections
# (must carry the same -ffunction-sections as the minimal stage, otherwise
#  gc-sections has nothing to collect and the control is meaningless)"
python3 tools/minimize_flags.py --stage s4-full-gc \
    --from-manifest build/ffmpeg-xmake/manifest.json \
    --add-flag=-ffunction-sections --add-flag=-fdata-sections
configure_and_build build/minimize/s4-full-gc/manifest-projected.json y n
python3 tools/minimize_gate.py --stage s4-full-gc
python3 tools/minimize_compare.py s0 s4-full-gc

step "4. minimal closure + section flags + --gc-sections"
python3 tools/minimize_flags.py --stage s4-gc --from-stage s3-pthreads \
    --add-flag=-ffunction-sections --add-flag=-fdata-sections
configure_and_build build/minimize/s4-gc/manifest-projected.json y n
python3 tools/minimize_gate.py --stage s4-gc
python3 tools/minimize_compare.py s0 s4-gc

step "5. minimal closure at -Os (conventional archive floor, no GC/LTO)"
python3 tools/minimize_flags.py --stage s5-Os --from-stage s3-pthreads --replace-opt Os
configure_and_build build/minimize/s5-Os/manifest-projected.json n n
python3 tools/minimize_gate.py --stage s5-Os
python3 tools/minimize_compare.py s0 s5-Os

step "6. minimal shipped-linked candidate: -Os -flto + --gc-sections link"
python3 tools/minimize_flags.py --stage s5-Os-LTO --from-stage s3-pthreads \
    --replace-opt Os --add-flag=-flto
configure_and_build build/minimize/s5-Os-LTO/manifest-projected.json y y
python3 tools/minimize_gate.py --stage s5-Os-LTO
python3 tools/minimize_compare.py s0 s5-Os-LTO

step "7. shipped shared core: libqianqian_songcore.so (PIC + version script)"
python3 tools/minimize_so.py --stage s6-shipped-so --from-stage s3-pthreads

step "8. summary (all numbers from gate.json / so.json, zero hand-entry)"
python3 tools/minimize_summary.py
