#!/usr/bin/env bash
# specs/composition-kernel-0/check.sh — FV-TEMP-0（#122）K0 控制面 TLA+/TLC 套件
#
# 语义来源：docs/architecture/composition-kernel-0-design.md（§F/§E.3/§E.4/§G/
# §G.6/§L.1/§L.5）。本套件是 formal evidence（#124 result vocabulary），
# 不是第二份 authority；BOUNDED-CLEAN 不得表述为 "architecture proven"。
#
# 规则（继承 specs/realtime-publication/check.sh 的 fail-closed 纪律，
# 吸收 PR #79/#82 学费：cfg model-value mismatch、-continue false-complete、
# exit-code 语义（12=invariant violated / 255=crash / 1=其他）、tmpfs 状态
# 目录坑、warning 处理）：
#   baseline（无 mutation）必须 TLC 探索完成（"Model checking completed"）、
#     全部 invariant PASS、temporal property PASS；
#   每个 mutation 必须违反其目标 invariant / 性质（counterexample 才算过）；
#   lfail 模式要求 temporal property 被违反（liveness counterexample）；
#   每个 probe（正向控制）必须违反其 "unreachable" 断言（witness 才算过）；
#   任何 TLC run 输出 Warning 即 FAIL（fail closed，无 whitelist）；
#   MUTANT/PROBE 另要求 TLC 自行收尾（log 含 "Finished in"）：被杀/崩溃
#   进程的部分输出不得作为反例证据。
#
# 工具链：tla2tools v1.7.4（Xenophones），sha256 校验，缺失时自动下载。
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
#
# TLC 1.7.4 已知行为（#82 exit-code 学费）：-continue 且有 violated invariant
# 时，JVM 可能在校验完成、打印统计后的收尾阶段随机崩溃（exit 75/255），
# 缓冲的日志尾部（含 "Finished in"）随崩溃丢失。完整探索过的 run 的日志
# 必然含 "Finished in"；被杀/崩溃丢尾的 run 不是有效 evidence。因此对
# fail:/lfail 模式做最多 3 次重试，直到拿到含 "Finished in" 的完整日志。
run_tlc() {
  local cfg="$1" expect="$2" label="$3"; shift 3
  local module="CompositionKernel0"
  local tmp log completed viol propviol finished stats warnings
  tmp="$(mktemp -d "$WORK_BASE/qianqian-specs-k0.XXXXXX")"
  cp "$SPEC_ROOT/$module.tla" "$cfg" "$tmp/"
  log="$tmp/out.log"
  local attempt
  for attempt in 1 2 3; do
    ( cd "$tmp" && timeout 3600 "$JAVA_BIN" -XX:+UseParallelGC -jar "$JAR" \
        -workers "$WORKERS" "$@" -config "$(basename "$cfg")" "$module.tla" > out.log 2>&1 )
    finished="$(grep -c 'Finished in' "$log" || true)"
    [[ "$finished" -ge 1 ]] && break
  done
  completed="$(grep -c 'Model checking completed' "$log" || true)"
  viol="$(grep -oE 'Invariant [A-Za-z0-9]+ is violated' "$log" | sed 's/Invariant \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  propviol="$(grep -c 'Temporal properties were violated' "$log" || true)"
  stats="$(grep -E '[0-9]+ states generated, [0-9]+ distinct states found' "$log" | tail -1)"
  # TLC Warning 检测必须先于 pass/mutation 判定（fail closed）：有 warning 的
  # run 不是有效 evidence。pattern 匹配行首/空白后的 "Warning:"。
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

echo "== baseline（必须全部 PASS：invariants + temporal properties）"
run_tlc "$SPEC_ROOT/CompositionKernel0.cfg"          pass "K0 / baseline safety"
run_tlc "$SPEC_ROOT/CompositionKernel0Liveness.cfg"  pass "K0 / baseline liveness（WF(KernelActions)）"

echo "== 负控制 mutations（必须产生 counterexample；-continue 取全部 violated 集）"
run_tlc "$SPEC_ROOT/mutations/M1DropReliedGuard.cfg"       fail:ReliedGuard   "Mutation / M1 DropReliedGuard" -continue
run_tlc "$SPEC_ROOT/mutations/M2RemoveBeforeDischarge.cfg" fail:NoRemovalOwing "Mutation / M2 RemoveBeforeDischarge" -continue
run_tlc "$SPEC_ROOT/mutations/M3EarlyReplacement.cfg"      fail:SingleSource  "Mutation / M3 EarlyReplacement" -continue
run_tlc "$SPEC_ROOT/mutations/M4DoubleInverse.cfg"         fail:InverseOnce   "Mutation / M4 DoubleInverse" -continue
run_tlc "$SPEC_ROOT/mutations/M5MountOverViolation.cfg"    fail:SingleSource  "Mutation / M5 MountOverViolation（pre-#126 Rust 行为；现作 overlap guard 负控制）" -continue

echo "== 可达性探针（正向控制：witness 必须找到 = 断言必须被违反）"
run_tlc "$SPEC_ROOT/probes/StagingWindow.cfg"        fail:ProbeStagingWindowUnreachable        "Probe / §E.4 staging 窗口"
run_tlc "$SPEC_ROOT/probes/ViolatedLatch.cfg"        fail:ProbeViolatedLatchUnreachable        "Probe / §G.6 违约 latch"
run_tlc "$SPEC_ROOT/probes/GuardLatched.cfg"         fail:ProbeGuardLatchedUnreachable         "Probe / §G.6 Scenario A guard latch"
run_tlc "$SPEC_ROOT/probes/FailedQuiet.cfg"          fail:ProbeFailedQuietUnreachable          "Probe / §L.1 条款3 FAILED quiet-legal"
run_tlc "$SPEC_ROOT/probes/ReplacementComplete.cfg"  fail:ProbeReplacementCompleteUnreachable  "Probe / §E.4 replacement 完成+re-commit"
run_tlc "$SPEC_ROOT/probes/DisposeConvergence.cfg"   fail:ProbeDisposeConvergenceUnreachable   "Probe / dispose_root 收敛到空 registry"

if [[ "$fail" -eq 0 ]]; then
  echo "== 全部门通过"
else
  echo "== 存在失败项"
fi
exit "$fail"
