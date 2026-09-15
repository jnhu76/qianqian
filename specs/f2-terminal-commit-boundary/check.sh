#!/usr/bin/env bash
# specs/f2-terminal-commit-boundary/check.sh — 运行 terminal Fact commit ownership 模型
#
# 语义来源：ADR-PBK-001 §2（Fact Plane / semantic commit / Projection 非权威）与
# ADR-PBK-002 §17 D11（episode terminal outcome authority）。本套件是 formal
# evidence / campaign artifact，不是第二份 authority，也不改变 D11 命题。
#
# 规则（继承 specs/check.sh 的 fail-closed 纪律）：
#   正常模型（无 mutation）必须 TLC 探索完成（"Model checking completed"）、
#     全部 invariant PASS、temporal property PASS（"Temporal properties were
#     violated" 不得出现）；
#   每个 mutation 必须违反其目标 invariant（counterexample 才算通过）；
#   lfail 模式要求 temporal property 被违反（liveness counterexample）；
#   每个 witness 探针必须违反其"不可达"断言（witness 才算通过）；
#   不可达性取证探针必须 PASS（= 已证不可达）；
#   任何 TLC run 输出 Warning 即 FAIL（fail closed，无 whitelist）；
#   MUST-FAIL 另要求 TLC 自行收尾（log 含 "Finished in"）：被杀/崩溃进程的
#   部分输出不得作为反例证据。
#
# 工具链与 specs/realtime-publication 相同：tla2tools v1.7.4 (Xenophanes)，
# sha256 校验，缺失时自动下载（需要代理请先 export http_proxy/https_proxy）。
set -uo pipefail

SPEC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL_DIR="$SPEC_ROOT/../tools"
JAR="$TOOL_DIR/tla2tools.jar"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar"
TLA_SHA256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
WORKERS="${TLA_WORKERS:-4}"
JAVA_BIN="${TLA_JAVA:-java}"
MODULE="F2TerminalCommitBoundary"

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
  local tmp; tmp="$(mktemp -d "$WORK_BASE/qianqian-f2.XXXXXX")"
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
  viol="$(grep -oE 'Invariant [A-Za-z0-9]+ is violated' "$log" | sed 's/Invariant \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  propviol="$(grep -c 'Temporal properties were violated' "$log" || true)"
  finished="$(grep -c 'Finished in' "$log" || true)"
  local stats; stats="$(grep -E '[0-9]+ states generated, [0-9]+ distinct states found' "$log" | tail -1)"
  # TLC Warning 检测必须先于 pass/mutation 判定（fail closed）。
  local warnings
  warnings="$(grep -E -A1 '(^|[[:space:]])Warning:' "$log" || true)"
  if [[ -n "$warnings" ]]; then
    printf '%-56s FAIL（TLC warning：evidence invalid）\n' "$label"
    printf '%s\n' "$warnings" | head -12 | sed 's/^/    /'
    printf '    完整 log（保留供诊断）：%s\n' "$log"
    fail=1
    return
  fi
  if [[ "$expect" == pass ]]; then
    if [[ "$completed" -ge 1 && -z "$viol" && "$propviol" -eq 0 ]]; then
      printf '%-56s PASS  %s\n' "$label" "$stats"
      rm -rf "$tmp"
    else
      printf '%-56s FAIL（期望 PASS）violated=%s propviol=%s\n' "$label" "${viol:-none}" "$propviol"; fail=1
      printf '    log：%s\n' "$log"
    fi
  elif [[ "$expect" == lfail:* ]]; then
    if [[ "$propviol" -ge 1 && "$finished" -ge 1 ]]; then
      printf '%-56s MUST-FAIL-OK（liveness 反例：%s）\n' "$label" "${expect#lfail:}"
      rm -rf "$tmp"
    else
      printf '%-56s FAIL（期望 liveness 反例；实际 violated=%s propviol=%s finished=%s）\n' "$label" "${viol:-none}" "$propviol" "$finished"; fail=1
      printf '    log：%s\n' "$log"
    fi
  else
    local target; target="${expect#fail:}"
    if [[ ",$viol," == *",$target,"* && "$propviol" -eq 0 && "$finished" -ge 1 ]]; then
      printf '%-56s MUST-FAIL-OK（违反 %s）\n' "$label" "$viol"
      rm -rf "$tmp"
    else
      printf '%-56s FAIL（期望违反 %s；实际 violated=%s propviol=%s finished=%s）\n' "$label" "$target" "${viol:-none}" "$propviol" "$finished"; fail=1
      printf '    log：%s\n' "$log"
    fi
  fi
}

echo "== 正常模型（必须全部 PASS：invariants + temporal properties）"
run_tlc "$SPEC_ROOT/F2TerminalCommitBoundary.cfg"          pass "Variant A / consumer-triggered commit"
run_tlc "$SPEC_ROOT/F2TerminalCommitBoundaryAuthority.cfg" pass "Variant B / authority-owned commit"
run_tlc "$SPEC_ROOT/F2TerminalCommitBoundaryAtomic.cfg"    pass "Variant B' / commit atomic with evidence"

echo "== 负控制（必须产生 counterexample）"
run_tlc "$SPEC_ROOT/mutations/ObserveCommits.cfg"           fail:OutcomeWrittenOnlyByContractCommitter "Mutation / M1 ObserveCommits（pure read commits）" -continue
run_tlc "$SPEC_ROOT/mutations/TerminalRewritable.cfg"       fail:TerminalOutcomeImmutable            "Mutation / M2 TerminalRewritable（late stop rewrites）" -continue
run_tlc "$SPEC_ROOT/mutations/WaitIsSoleResolver.cfg"       lfail:TerminalEvidenceCommitsEventually  "Mutation / M3 WaitIsSoleResolver（liveness）"
run_tlc "$SPEC_ROOT/mutations/AuthorityResolverRemoved.cfg" lfail:TerminalEvidenceCommitsEventually  "Mutation / M4 AuthorityResolverRemoved（liveness）"
run_tlc "$SPEC_ROOT/mutations/ActivationFailureIsFailed.cfg" fail:ActivationFailureIsNotTerminalFailed "Mutation / M5 ActivationFailureIsFailed" -continue
run_tlc "$SPEC_ROOT/mutations/ResolverIgnoresEvidence.cfg"  fail:NoFalseCompleted                    "Mutation / M6 ResolverIgnoresEvidence" -continue
run_tlc "$SPEC_ROOT/mutations/StopDiscriminatorRemoved.cfg"  fail:ScenarioUserStop                    "Mutation / M7 StopDiscriminatorRemoved（precedence）" -continue
run_tlc "$SPEC_ROOT/mutations/FailureDowngraded.cfg"         fail:ScenarioDecodeFailureNeverStopped   "Mutation / M9 FailureDowngraded（precedence）" -continue
run_tlc "$SPEC_ROOT/mutations/TeardownBeforeLegsJoined.cfg"  fail:TeardownImpliesDecisive             "Mutation / M8 TeardownBeforeLegsJoined（join 纪律）" -continue

echo "== 反向控制（对模型自己结论的负控制：必须被违反）"
run_tlc "$SPEC_ROOT/mutations/Overclaim_ConsumerTriggered.cfg"      lfail:EpisodesEventuallyTerminate      "Overclaim / unconditional termination（A）"
run_tlc "$SPEC_ROOT/mutations/Overclaim_AuthorityOwned.cfg"         lfail:EpisodesEventuallyTerminate      "Overclaim / unconditional termination（B）"
run_tlc "$SPEC_ROOT/mutations/Overclaim_AtomicWithEvidence.cfg"     lfail:EpisodesEventuallyTerminate      "Overclaim / unconditional termination（B'）"
run_tlc "$SPEC_ROOT/probes/NoFairnessProgressFails_ConsumerTriggered.cfg" lfail:TerminalEvidenceCommitsEventually "Fairness / load-bearing（A，去掉 fairness）"
run_tlc "$SPEC_ROOT/probes/NoFairnessProgressFails_AuthorityOwned.cfg"    lfail:TerminalEvidenceCommitsEventually "Fairness / load-bearing（B，去掉 fairness）"

echo "== 可达性 witness（正向控制：断言必须被违反 = witness 找到）"
run_tlc "$SPEC_ROOT/probes/DecisiveEvidencePendingWitness.cfg"    fail:DecisiveImpliesCommitted                "Witness / decisive evidence but pending（A）"
run_tlc "$SPEC_ROOT/probes/CommitWithoutConsumerWitness.cfg"      fail:BoundaryCommitRequiresConsumer          "Witness / commit without consumer（B）"
run_tlc "$SPEC_ROOT/probes/PendingDeviceAbortWindowWitness.cfg"   fail:DiagnosticPendingDeviceAbortUnreachable "Witness / uncommitted abort window（B）"
run_tlc "$SPEC_ROOT/probes/NoConsumerCommitWitness.cfg"           fail:NeverCommitted                          "Witness / commit with no consumer at all（B）"
run_tlc "$SPEC_ROOT/probes/TeardownWithoutFactWitness_OwnershipConsumerTriggered.cfg" fail:DiagnosticTeardownWithoutFactUnreachable "Witness / teardown without fact（A）"
run_tlc "$SPEC_ROOT/probes/TeardownWithoutFactWitness_OwnershipAuthorityOwned.cfg" fail:DiagnosticTeardownWithoutFactUnreachable "Witness / teardown without fact（B）"
run_tlc "$SPEC_ROOT/probes/LateStopAfterCommitWitness_OwnershipConsumerTriggered.cfg" fail:DiagnosticLateStopAfterCommitUnreachable "Witness / late stop after commit（A）"
run_tlc "$SPEC_ROOT/probes/LateStopAfterCommitWitness_OwnershipAuthorityOwned.cfg" fail:DiagnosticLateStopAfterCommitUnreachable "Witness / late stop after commit（B）"
run_tlc "$SPEC_ROOT/probes/LateStopAfterCommitWitness_OwnershipAtomicWithEvidence.cfg" fail:DiagnosticLateStopAfterCommitUnreachable "Witness / late stop after commit（B'）"

echo "== 不可达性取证（断言必须 PASS = 已证不可达）"
run_tlc "$SPEC_ROOT/probes/NoConsumerCommitImpossible.cfg" pass "Impossible / commit with no consumer（A）"

if [[ "$fail" -eq 0 ]]; then
  echo "== 全部门通过"
else
  echo "== 存在失败项"
fi
exit "$fail"
