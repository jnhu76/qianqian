#!/usr/bin/env bash
# specs/realtime-publication/check.sh — 运行 realtime publication lifetime 模型
#
# 语义来源：docs/adr/ADR-PBK-001.md §6（ACCEPTED）。本套件是 formal evidence，
# 不是第二份 authority。
#
# 规则（继承 specs/check.sh 的 fail-closed 纪律）：
#   正常模型（无 mutation）必须 TLC 探索完成（"Model checking completed"）、
#     全部 invariant PASS、temporal property PASS（"Temporal properties were
#     violated" 不得出现）；
#   每个 mutation 必须违反其目标 invariant（counterexample 才算通过）；
#   lfail 模式要求 temporal property 被违反（liveness counterexample）；
#   每个 probe（正向控制）必须违反其“不可达”断言（witness 才算通过）；
#   任何 TLC run 输出 Warning 即 FAIL（fail closed，无 whitelist）；
#   MUST-FAIL 另要求 TLC 自行收尾（log 含 "Finished in"）：被杀/崩溃进程的
#   部分输出不得作为反例证据。
#
# 工具链与 specs/composition-kernel-0 相同：tla2tools v1.7.4 (Xenophanes)，sha256 校验，
# 缺失时自动下载（需要代理请先 export http_proxy/https_proxy）。
set -uo pipefail

SPEC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL_DIR="$SPEC_ROOT/../tools"
JAR="$TOOL_DIR/tla2tools.jar"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar"
TLA_SHA256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
WORKERS="${TLA_WORKERS:-4}"
JAVA_BIN="${TLA_JAVA:-java}"

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

# run_tlc <cfg_path> <期望模式 pass|fail:<目标invariant>|lfail:<性质名>> <显示名> [附加TLC参数...]
run_tlc() {
  local cfg="$1" expect="$2" label="$3"; shift 3
  local module="RealtimePublication"
  local tmp; tmp="$(mktemp -d "$WORK_BASE/qianqian-specs.XXXXXX")"
  cp "$SPEC_ROOT/$module.tla" "$cfg" "$tmp/"
  local log="$tmp/out.log"
  ( cd "$tmp" && timeout 3600 "$JAVA_BIN" -XX:+UseParallelGC -jar "$JAR" \
      -workers "$WORKERS" "$@" -config "$(basename "$cfg")" "$module.tla" > out.log 2>&1 )
  local completed viol propviol finished
  completed="$(grep -c 'Model checking completed' "$log" || true)"
  viol="$(grep -oE 'Invariant [A-Za-z0-9]+ is violated' "$log" | sed 's/Invariant \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  propviol="$(grep -c 'Temporal properties were violated' "$log" || true)"
  finished="$(grep -c 'Finished in' "$log" || true)"
  local stats; stats="$(grep -E '[0-9]+ states generated, [0-9]+ distinct states found' "$log" | tail -1)"
  # TLC Warning 检测必须先于 pass/mutation 判定（fail closed）：有 warning 的
  # run 不是有效 evidence。pattern 匹配行首/空白后的 "Warning:"（TLC 稳定
  # marker，大写；不匹配 JVM 小写 "warning:" 或路径等偶然字符串）。
  local warnings
  warnings="$(grep -E -A1 '(^|[[:space:]])Warning:' "$log" || true)"
  if [[ -n "$warnings" ]]; then
    printf '%-52s FAIL（TLC warning：evidence invalid）\n' "$label"
    printf '%s\n' "$warnings" | head -12 | sed 's/^/    /'
    printf '    完整 log（保留供诊断）：%s\n' "$log"
    fail=1
    return
  fi
  if [[ "$expect" == pass ]]; then
    if [[ "$completed" -ge 1 && -z "$viol" && "$propviol" -eq 0 ]]; then
      printf '%-52s PASS  %s\n' "$label" "$stats"
    else
      printf '%-52s FAIL（期望 PASS）violated=%s propviol=%s\n' "$label" "${viol:-none}" "$propviol"; fail=1
    fi
  elif [[ "$expect" == lfail:* ]]; then
    if [[ "$propviol" -ge 1 && "$finished" -ge 1 ]]; then
      printf '%-52s MUST-FAIL-OK（liveness 反例：性质被违反）\n' "$label"
    else
      printf '%-52s FAIL（期望 liveness 反例；实际 violated=%s propviol=%s finished=%s）\n' "$label" "${viol:-none}" "$propviol" "$finished"; fail=1
    fi
  else
    local target; target="${expect#fail:}"
    if [[ ",$viol," == *",$target,"* && "$propviol" -eq 0 && "$finished" -ge 1 ]]; then
      printf '%-52s MUST-FAIL-OK（违反 %s）\n' "$label" "$viol"
    else
      printf '%-52s FAIL（期望违反 %s；实际 violated=%s propviol=%s finished=%s）\n' "$label" "$target" "${viol:-none}" "$propviol" "$finished"; fail=1
    fi
  fi
  rm -rf "$tmp"
}

echo "== 正常模型（必须全部 PASS：invariants + temporal property）"
run_tlc "$SPEC_ROOT/RealtimePublication.cfg"      pass "Publication / normal（N->N+1，{A}）"
run_tlc "$SPEC_ROOT/RealtimePublicationChain.cfg" pass "Publication / normal-chain（N->N+1->N+2，{A,B}）"

echo "== 负控制（必须产生 counterexample；-continue 取全部 violated 集）"
run_tlc "$SPEC_ROOT/mutations/ReleaseBeforeQuiesce.cfg"  fail:NoReaderDereferencesReleasedResource "Mutation / M1 ReleaseBeforeQuiesce" -continue
run_tlc "$SPEC_ROOT/mutations/SplitPublication.cfg"      fail:ViewIsCoherent                   "Mutation / M2 SplitPublication" -continue
run_tlc "$SPEC_ROOT/mutations/StaleEntry.cfg"            fail:NoReaderAcquiresRetiredView      "Mutation / M3 StaleEntry（safety）" -continue
run_tlc "$SPEC_ROOT/mutations/StaleEntryLiveness.cfg"    lfail:ReplacementEventuallyReclaimable "Mutation / M3 StaleEntry（liveness）"
run_tlc "$SPEC_ROOT/mutations/ForgetsOlderRetirement.cfg" fail:NoReaderDereferencesReleasedResource "Mutation / M4 ForgetsOlderRetirement" -continue

echo "== 可达性探针（正向控制：witness 必须找到 = 不变式必须被违反）"
run_tlc "$SPEC_ROOT/probes/RetirementOverlapWitness.cfg"    fail:RetirementOverlapUnreachable    "Probe / RetirementOverlap（I5 witness）"
run_tlc "$SPEC_ROOT/probes/QuiescentUncertifiedWitness.cfg" fail:QuiescentUncertifiedUnreachable "Probe / QuiescentUncertified"
run_tlc "$SPEC_ROOT/probes/ReclaimableWitness.cfg"          fail:ReclaimableUnreachable          "Probe / Reclaimable reachable"
run_tlc "$SPEC_ROOT/probes/ReleasedWitness.cfg"             fail:ReleasedUnreachable             "Probe / Released reachable"

if [[ "$fail" -eq 0 ]]; then
  echo "== 全部门通过"
else
  echo "== 存在失败项"
fi
exit "$fail"
