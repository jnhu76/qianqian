#!/usr/bin/env bash
# specs/episode-terminal-settlement/check.sh — episode terminal settlement
# current-spec conformance runner
#
# 语义来源：ADR-PBK-001 §2（semantic commit / fact authority identity /
# projection 非权威）与 ADR-PBK-002 §17 D11（terminal settlement ownership +
# late-command stability + 三条外部命题）。本套件是 current formal gate 的
# 模型证据：它不定义语义，只机器检查 accepted D11 contract 的一致表达，
# 并用 mutation 证明每条约束有约束力。不是第二份 authority。
#
# 规则（继承 specs/check.sh 的 fail-closed 纪律）：
#   正常模型必须 TLC 探索完成（"Model checking completed"）、全部 invariant
#     PASS、temporal property PASS（"Temporal properties were violated"
#     不得出现）；
#   每个 mutation 必须违反其目标 invariant（counterexample 才算通过）；
#   每个 witness 探针必须违反其"不可达"断言（witness 才算通过）；
#   反向控制 / fairness 承重控制必须产生 liveness 反例；
#   任何 TLC run 输出 Warning 即 FAIL（fail closed，无 whitelist）；
#   MUST-FAIL 另要求 TLC 自行收尾（log 含 "Finished in"）：被杀/崩溃进程
#   的部分输出不得作为反例证据。
#
# 工具链与其它套件相同：tla2tools v1.7.4 (Xenophanes)，sha256 校验，
# 缺失时自动下载（需要代理请先 export http_proxy/https_proxy）。
set -uo pipefail

SPEC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL_DIR="$SPEC_ROOT/../tools"
JAR="$TOOL_DIR/tla2tools.jar"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar"
TLA_SHA256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
WORKERS="${TLA_WORKERS:-4}"
JAVA_BIN="${TLA_JAVA:-java}"
MODULE="EpisodeTerminalSettlement"

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
  local tmp; tmp="$(mktemp -d "$WORK_BASE/qianqian-episode-terminal.XXXXXX")"
  cp "$SPEC_ROOT/$MODULE.tla" "$cfg" "$tmp/"
  local log="$tmp/out.log"
  local attempt finished
  for attempt in 1 2 3; do
    ( cd "$tmp" && timeout 3600 "$JAVA_BIN" -XX:+UseParallelGC -jar "$JAR" \
        -workers "$WORKERS" "$@" -config "$(basename "$cfg")" "$MODULE.tla" > out.log 2>&1 )
    finished="$(grep -c 'Finished in' "$log" || true)"
    [[ "$finished" -ge 1 ]] && break
  done
  local completed viol propviol
  completed="$(grep -c 'Model checking completed' "$log" || true)"
  viol="$(grep -oE 'Invariant [A-Za-z0-9_]+ is violated' "$log" | sed 's/Invariant \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  propviol="$(grep -c 'Temporal properties were violated' "$log" || true)"
  finished="$(grep -c 'Finished in' "$log" || true)"
  local stats; stats="$(grep -E '[0-9]+ states generated, [0-9]+ distinct states found' "$log" | tail -1)"
  # TLC Warning 检测必须先于 pass/mutation 判定（fail closed）。
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
    if [[ "$completed" -ge 1 && -z "$viol" && "$propviol" -eq 0 ]]; then
      printf '%-64s PASS  %s\n' "$label" "$stats"
      rm -rf "$tmp"
    else
      printf '%-64s FAIL（期望 PASS）violated=%s propviol=%s\n' "$label" "${viol:-none}" "$propviol"; fail=1
      printf '    log：%s\n' "$log"
    fi
  elif [[ "$expect" == lfail:* ]]; then
    if [[ "$propviol" -ge 1 && "$finished" -ge 1 ]]; then
      printf '%-64s MUST-FAIL-OK（liveness 反例：%s）\n' "$label" "${expect#lfail:}"
      rm -rf "$tmp"
    else
      printf '%-64s FAIL（期望 liveness 反例；实际 violated=%s propviol=%s finished=%s）\n' "$label" "${viol:-none}" "$propviol" "$finished"; fail=1
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

echo "== 正常模型（必须全部 PASS：invariants + temporal properties）"
run_tlc "$SPEC_ROOT/EpisodeTerminalSettlement.cfg"           pass "Base / safety + conditional progress（WF settle）"
run_tlc "$SPEC_ROOT/EpisodeTerminalSettlementSafetyOnly.cfg" pass "Base / safety only（no fairness）"

echo "== 负控制（每个 mutation 必须被抓住）"
run_tlc "$SPEC_ROOT/mutations/ObserveCommits.cfg"              fail:OutcomeWrittenOnlyByAuthoritySettle "Mutation / M1 ObserveCommits（pure read commits）" -continue
run_tlc "$SPEC_ROOT/mutations/WaitCommits.cfg"                 fail:OutcomeWrittenOnlyByAuthoritySettle "Mutation / M2 WaitCommits（wait settles）" -continue
run_tlc "$SPEC_ROOT/mutations/TerminalRewritable.cfg"          fail:TerminalOutcomeImmutable            "Mutation / M3 TerminalRewritable（late stop rewrites）" -continue
run_tlc "$SPEC_ROOT/mutations/LateStopReadsCurrentIntent.cfg"  fail:CommittedOutcomeMatchesContract     "Mutation / M4 LateStopReadsCurrentIntent（relabel）" -continue
run_tlc "$SPEC_ROOT/mutations/TeardownBeforeSettlement.cfg"    fail:TeardownRequiresSettlement          "Mutation / M5 TeardownBeforeSettlement" -continue
run_tlc "$SPEC_ROOT/mutations/ActivationFailureBecomesFailed.cfg" fail:ActivationFailureIsNotTerminalFailed "Mutation / M6 ActivationFailureBecomesFailed" -continue
run_tlc "$SPEC_ROOT/mutations/FalseCompleted.cfg"              fail:NoFalseCompleted                    "Mutation / M7 FalseCompleted" -continue
run_tlc "$SPEC_ROOT/mutations/FalseStopped.cfg"                fail:NoFalseStopped                      "Mutation / M8 FalseStopped（stopRequestedNow）" -continue
run_tlc "$SPEC_ROOT/mutations/EvidenceProducerCommits.cfg"     fail:OutcomeWrittenOnlyByAuthoritySettle "Mutation / M9 EvidenceProducerCommits" -continue

echo "== 反向控制（对模型自己结论的负控制：必须被违反；带 WF 的最强让步下）"
run_tlc "$SPEC_ROOT/mutations/OverclaimTermination.cfg"       lfail:EveryEpisodeEventuallyTerminates      "Overclaim / unconditional termination"
run_tlc "$SPEC_ROOT/mutations/OverclaimActiveCompletes.cfg"   lfail:EveryActiveEpisodeEventuallyCompletes "Overclaim / active episode must complete"
run_tlc "$SPEC_ROOT/mutations/OverclaimStopStops.cfg"         lfail:EveryStopEventuallyStops              "Overclaim / stop must yield Stopped"
run_tlc "$SPEC_ROOT/mutations/OverclaimDecoderExits.cfg"      lfail:EveryDecoderEventuallyExits           "Overclaim / decoder must exit"
run_tlc "$SPEC_ROOT/mutations/OverclaimDeviceDrains.cfg"      lfail:EveryDeviceEventuallyDrains           "Overclaim / device must drain"

echo "== fairness 承重控制"
run_tlc "$SPEC_ROOT/probes/NoFairnessProgressFails.cfg" lfail:SettlementProgress "Fairness / load-bearing（WF(AuthoritySettle) removed）"

echo "== 可达性 witness（正向控制：断言必须被违反 = witness 找到）"
run_tlc "$SPEC_ROOT/probes/WitnessNaturalCompletion.cfg"       fail:Unreachable_NaturalCompletion                "Witness / W1 natural EOF -> Completed"
run_tlc "$SPEC_ROOT/probes/WitnessUserStop.cfg"                fail:Unreachable_UserStop                         "Witness / W2 user stop -> Stopped"
run_tlc "$SPEC_ROOT/probes/WitnessDeviceAbortFailed.cfg"       fail:Unreachable_DeviceAbortFailed                "Witness / W3 device abort（no stop）-> Failed"
run_tlc "$SPEC_ROOT/probes/WitnessLateStopCannotRelabel.cfg"   fail:Unreachable_LateStopDecisiveFailedStaysFailed "Witness / W4 late stop stays Failed"
run_tlc "$SPEC_ROOT/probes/WitnessLateStopAfterCompleted.cfg"  fail:Unreachable_CompletedWithLateStop            "Witness / W5 late stop after Completed"
run_tlc "$SPEC_ROOT/probes/WitnessNoConsumerCommit.cfg"        fail:Unreachable_CommitWithoutConsumer            "Witness / W6 commit with no consumer at all"

if [[ "$fail" -eq 0 ]]; then
  echo "== 全部门通过"
else
  echo "== 存在失败项"
fi
exit "$fail"
