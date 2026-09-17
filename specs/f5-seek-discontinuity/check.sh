#!/usr/bin/env bash
# specs/f5-seek-discontinuity/check.sh — F5 seek cutover safety runner
#
# 语义来源：ADR-PBK-002 §20 D14.5（seek 语义 spine：cutover commit 之后
# pre-seek PCM 不得再成为 post-seek output）与 §20 D14.8 的 seek 规则
# （publication 不得混合 pre/post-cutover handed-off 总量）。
# 本套件是 F5-GATE 的 gate-local formal evidence：它不定义语义，只机器
# 检查提议的 same-resource discontinuity protocol 在其显式抽象内满足
# 冻结不变式，并用 mutation 证明每条护栏承重。不是第二份 authority。
#
# 规则（fail-closed，与其它套件一致）：
#   正常模型必须 TLC 探索完成（"Model checking completed"）且全部
#     invariant PASS；
#   每个 mutation 必须违反其目标 invariant（counterexample 才算通过）；
#   每个 witness 探针必须违反其"不可达"断言（reachable 才算通过）；
#   任何 TLC Warning 即 FAIL（fail closed，无 whitelist）；
#   MUST-FAIL 另要求 TLC 自行收尾（log 含 "Finished in"）。
#
# 工具链：tla2tools v1.7.4（Xenophanes），sha256 校验，缺失时自动下载
# （需要代理请先 export http_proxy/https_proxy）。
set -uo pipefail

SPEC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL_DIR="$SPEC_ROOT/../tools"
JAR="$TOOL_DIR/tla2tools.jar"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar"
TLA_SHA256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
WORKERS="${TLA_WORKERS:-4}"
JAVA_BIN="${TLA_JAVA:-java}"
MODULE="SeekDiscontinuity"

fail=0

if [[ ! -f "$JAR" ]]; then
  echo "== 下载 tla2tools.jar v1.7.4（需要代理请先 export http_proxy/https_proxy）"
  mkdir -p "$TOOL_DIR"
  if ! curl -fSL -o "$JAR" "$TLA_URL"; then
    echo "FAIL: 下载失败：$TLA_URL"; exit 1
  fi
fi
actual_sha="$(sha256sum "$JAR" | cut -d' ' -f1)"
if [[ "$actual_sha" != "$TLA_SHA256" ]]; then
  echo "FAIL: tla2tools.jar sha256 不匹配（期望 $TLA_SHA256，实际 $actual_sha）"; exit 1
fi

# TLC 的磁盘状态队列可能很大：工作目录放在真实磁盘而非 tmpfs。
WORK_BASE="${TLA_TMPDIR:-/var/tmp}"

# run_tlc <cfg_path> <期望模式 pass|fail:<目标invariant>> <显示名> [附加TLC参数...]
run_tlc() {
  local cfg="$1" expect="$2" label="$3"; shift 3
  local tmp; tmp="$(mktemp -d "$WORK_BASE/qianqian-f5-seek.XXXXXX")"
  cp "$SPEC_ROOT"/*.tla "$cfg" "$tmp/"
  local log="$tmp/out.log"
  local attempt finished
  for attempt in 1 2 3; do
    ( cd "$tmp" && timeout 3600 "$JAVA_BIN" -XX:+UseParallelGC -jar "$JAR" \
        -workers "$WORKERS" "$@" -config "$(basename "$cfg")" "$MODULE.tla" > out.log 2>&1 )
    finished="$(grep -c 'Finished in' "$log" || true)"
    [[ "$finished" -ge 1 ]] && break
  done
  local completed viol propviol apviol
  completed="$(grep -c 'Model checking completed' "$log" || true)"
  viol="$(grep -oE 'Invariant [A-Za-z0-9_]+ is violated' "$log" | sed 's/Invariant \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  propviol="$(grep -c 'Temporal properties were violated' "$log" || true)"
  apviol="$(grep -oE 'Action property [A-Za-z0-9_]+ is violated' "$log" | sed 's/Action property \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  finished="$(grep -c 'Finished in' "$log" || true)"
  local stats; stats="$(grep -E '[0-9]+ states generated, [0-9]+ distinct states found' "$log" | tail -1)"
  # TLC Warning 检测先于 pass/mutation 判定（fail closed）。
  local warnings
  warnings="$(grep -E -A1 '(^|[[:space:]])Warning:' "$log" || true)"
  if [[ -n "$warnings" ]]; then
    printf '%-64s FAIL（TLC warning：evidence invalid）\n' "$label"
    printf '%s\n' "$warnings" | head -12 | sed 's/^/    /'
    printf '    完整 log（保留供诊断）：%s\n' "$log"
    fail=1
    return
  fi
  if [[ "$expect" == pass ]]; then
    if [[ "$completed" -ge 1 && -z "$viol" && "$propviol" -eq 0 && -z "$apviol" ]]; then
      printf '%-64s PASS  %s\n' "$label" "$stats"
      rm -rf "$tmp"
    else
      printf '%-64s FAIL（期望 PASS）violated=%s propviol=%s apviol=%s\n' "$label" "${viol:-none}" "$propviol" "${apviol:-none}"; fail=1
      printf '    log：%s\n' "$log"
    fi
  else
    local target; target="${expect#fail:}"
    if [[ ",$viol," == *",$target,"* && "$propviol" -eq 0 && "$finished" -ge 1 ]]; then
      printf '%-64s MUST-FAIL-OK（违反 %s）\n' "$label" "$viol"
      rm -rf "$tmp"
    else
      printf '%-64s FAIL（期望违反 %s；实际 violated=%s propviol=%s finished=%s）\n' "$label" "$target" "${viol:-none}" "$propviol" "$finished"; fail=1
      printf '    log：%s\n' "$log"
    fi
  fi
}

echo "== 正常模型（必须 PASS：TypeOK + 三条安全不变式，探索完成）"
run_tlc "$SPEC_ROOT/SeekDiscontinuity.cfg" pass "Base / safety（commit 后旧 PCM 不可能）"

echo "== witness 探针（必须 MUST-FAIL：可达性证明，防空洞不变式）"
run_tlc "$SPEC_ROOT/probes/ReachCommit.cfg"             fail:ProbeNeverCommitted  "Witness / commit 可达"                 -continue
run_tlc "$SPEC_ROOT/probes/WitnessOldDrainPreCommit.cfg" fail:ProbeNoOldDrainEver "Witness / pre-commit 旧输出可达（合法）" -continue
run_tlc "$SPEC_ROOT/probes/WitnessOldEdgePreCommit.cfg"  fail:ProbeNoOldEdgeEver  "Witness / pre-commit edge 旧库可达"     -continue
run_tlc "$SPEC_ROOT/probes/WitnessSeekRefused.cfg"       fail:ProbeNoRefusalEver  "Witness / seek 拒绝路径可达"            -continue

echo "== 负控制（每个 mutation 必须被抓住）"
run_tlc "$SPEC_ROOT/mutations/M1CommitBeforeTailPurge.cfg" fail:InvStaleOutput     "Mutation / M1 CommitBeforeTailPurge（不等尾排空）" -continue
run_tlc "$SPEC_ROOT/mutations/M2SeekMidWrite.cfg"           fail:InvStaleOutput     "Mutation / M2 SeekMidWrite（staging 未丢弃）"      -continue
run_tlc "$SPEC_ROOT/mutations/M3ParkWhileHeld.cfg"         fail:InvStaleOutput     "Mutation / M3 ParkWhileHeld（held block 跨 park）" -continue
run_tlc "$SPEC_ROOT/mutations/M4StalePositionWriter.cfg"   fail:InvPositionNoMixing "Mutation / M4 StalePositionWriter（旧 basis 复用）" -continue
run_tlc "$SPEC_ROOT/mutations/M5CommitBeforeLanding.cfg"   fail:InvStaleOutput     "Mutation / M5 CommitBeforeLanding（未 reposition）" -continue

if [[ "$fail" -ne 0 ]]; then
  echo "F5-SEEK-DISCONTINUITY: FAILED"
  exit 1
fi
echo "F5-SEEK-DISCONTINUITY: ALL PASS"
