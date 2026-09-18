#!/usr/bin/env bash
# specs/check.sh — 运行当前（current）形式化验证
#
# Post-139 spec reset 语义：本 runner 只编排当前幸存套件。
# 缺省模式 = 全部当前 TLA+ 验证（K0 控制面 + realtime publication）。
# 没有历史模式：pre-reset playback 模型已从 main 删除（Git 历史存档），
# 见 specs/README.md。
#
# 用法：specs/check.sh [current|k0|realtime|terminal|rust]（缺省 current）
#   current  — composition-kernel-0 + realtime-publication +
#              episode-terminal-settlement（TLA+/TLC）
#   k0       — K0 控制面套件
#   realtime — realtime publication 套件
#   terminal — episode terminal settlement 套件（D11 current-spec conformance）
#   rust     — Rust 侧当前验证：composition-kernel-0-rust（cargo 矩阵 +
#              Miri + production mutation 负控制）与 playback-concurrency
#              （native + loom + mutation）。需要 nightly miri / loom
#              feature；任一依赖缺失即 fail closed。
#
# 每个套件 runner 自身 fail closed（baseline 探索完成 + 全 PASS；
# mutation 必须被抓住；任何 TLC Warning 即 FAIL）。本脚本只聚合
# 各套件退出码，不复制判定逻辑。工具链（tla2tools v1.7.4，sha256
# 校验，缺失自动下载——需要代理请先 export http_proxy/https_proxy）
# 由各套件 runner 自行处理，共用 specs/tools/ 缓存。
set -uo pipefail

mode="${1:-current}"
case "$mode" in
  current|k0|realtime|terminal|rust|f5) ;;
  *) echo "usage: specs/check.sh [current|k0|realtime|terminal|rust|f5]（缺省 current）" >&2; exit 2 ;;
esac

SPEC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

fail=0
run_suite() {
  echo
  echo "===== $1 ====="
  if "$SPEC_ROOT/$1/check.sh"; then :; else
    echo "SUITE FAILED: $1" >&2
    fail=1
  fi
}

case "$mode" in
  current)
    run_suite composition-kernel-0
    run_suite realtime-publication
    run_suite episode-terminal-settlement
    run_suite f5-seek-discontinuity
    ;;
  k0)
    run_suite composition-kernel-0
    ;;
  realtime)
    run_suite realtime-publication
    ;;
  terminal)
    run_suite episode-terminal-settlement
    ;;
  f5)
    run_suite f5-seek-discontinuity
    ;;
  rust)
    run_suite composition-kernel-0-rust
    run_suite playback-concurrency
    ;;
esac

case "$mode" in
  current)    what="全部当前 TLA+ 验证（K0 + realtime publication + episode terminal settlement + f5 seek discontinuity）" ;;
  k0)         what="K0 套件" ;;
  realtime)   what="realtime publication 套件" ;;
  terminal)   what="episode terminal settlement 套件" ;;
  f5)         what="f5 seek discontinuity 套件" ;;
  rust)       what="全部当前 Rust 侧验证（matrices + Miri + loom + 负控制）" ;;
esac

if [[ "$fail" -eq 0 ]]; then
  echo
  echo "== ${what}通过（bounds 与结果类见 specs/README.md）"
else
  echo
  echo "== 存在失败项"
fi
exit "$fail"
