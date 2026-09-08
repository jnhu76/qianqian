#!/usr/bin/env bash
# specs/check.sh — 运行 specs/ 下全部形式化模型
#
# 规则：
#   正常模型（无 mutation）必须 TLC 探索完成且全部 invariant PASS；
#   每个 mutation 必须违反其目标 invariant（counterexample 才算通过——
#   mutation 的目的就是证明 checker 抓得住错误）。
#
# 工具链固定版本（fail closed）：
#   tla2tools v1.7.4 (Xenophanes)，sha256 见下；缺失时自动下载
#   （需要代理时请先 export http_proxy/https_proxy）。
set -uo pipefail

SPEC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAYBACK="$SPEC_ROOT/playback"
TOOL_DIR="$SPEC_ROOT/tools"
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

# TLC 的磁盘状态队列可能达到 GB 级：工作目录放在真实磁盘而非 tmpfs。
WORK_BASE="${TLA_TMPDIR:-/var/tmp}"

# run_tlc <module> <cfg_path> <期望模式 pass|fail:<目标invariant>,...> <显示名> [附加TLC参数...]
run_tlc() {
  local module="$1" cfg="$2" expect="$3" label="$4"; shift 4
  local tmp; tmp="$(mktemp -d "$WORK_BASE/qianqian-specs.XXXXXX")"
  cp "$PLAYBACK/$module.tla" "$cfg" "$tmp/"
  local log="$tmp/out.log"
  ( cd "$tmp" && timeout 3600 "$JAVA_BIN" -XX:+UseParallelGC -jar "$JAR" \
      -workers "$WORKERS" "$@" -config "$(basename "$cfg")" "$module.tla" > out.log 2>&1 )
  local completed viol
  completed="$(grep -c 'Model checking completed' "$log" || true)"
  viol="$(grep -oE 'Invariant [A-Za-z0-9]+ is violated' "$log" | sed 's/Invariant \(.*\) is violated/\1/' | sort -u | tr '\n' ',' | sed 's/,$//')"
  local stats; stats="$(grep -E '[0-9]+ states generated, [0-9]+ distinct states found' "$log" | tail -1)"
  if [[ "$expect" == pass ]]; then
    if [[ "$completed" -ge 1 && -z "$viol" ]]; then
      printf '%-42s PASS  %s\n' "$label" "$stats"
    else
      printf '%-42s FAIL（期望 PASS）violated=%s\n' "$label" "${viol:-none}"; fail=1
    fi
  else
    local target; target="${expect#fail:}"
    local propviol; propviol="$(grep -c 'Temporal properties were violated' "$log" || true)"
    if [[ ",$viol," == *",$target,"* && "$propviol" -eq 0 ]]; then
      printf '%-42s MUST-FAIL-OK（违反 %s）\n' "$label" "$viol"
    else
      printf '%-42s FAIL（期望违反 %s 且症状属性成立；实际 violated=%s propviol=%s）\n' "$label" "$target" "${viol:-none}" "$propviol"; fail=1
    fi
  fi
  rm -rf "$tmp"
}

echo "== 正常模型（必须全部 PASS）"
run_tlc PlaybackTemporal "$PLAYBACK/PlaybackTemporal.cfg" pass "PlaybackTemporal"
run_tlc PlaybackOwnership "$PLAYBACK/PlaybackOwnership.cfg" pass "PlaybackOwnership"

echo "== 负控制（必须产生 counterexample）"
run_tlc PlaybackTemporal "$PLAYBACK/mutations/PromoteWithoutFence.cfg"      fail:PromotionRequiresSuccessfulFence    "Temporal / PromoteWithoutFence"
run_tlc PlaybackTemporal "$PLAYBACK/mutations/AcceptUnadmittedDecode.cfg"  fail:DecodeResultRequiresAdmission       "Temporal / AcceptUnadmittedDecode"
run_tlc PlaybackTemporal "$PLAYBACK/mutations/SingleGlobalGenerationCheck.cfg" fail:DecodeResultRequiresAdmission  "Temporal / SingleGlobalGenerationCheck" -continue
run_tlc PlaybackTemporal "$PLAYBACK/mutations/RetiredGenerationStillAdmitted.cfg" fail:RetiredGenerationCannotReenter "Temporal / RetiredGenerationStillAdmitted"
run_tlc PlaybackTemporal "$PLAYBACK/mutations/EndBeforeRenderDrain.cfg"    fail:TransportDrainRequiresRenderedDrain "Temporal / EndBeforeRenderDrain"
run_tlc PlaybackOwnership "$PLAYBACK/mutations/ReleaseProviderEarly.cfg"   fail:ProviderFinalReleaseRequiresDependentExit "Ownership / ReleaseProviderEarly"
run_tlc PlaybackOwnership "$PLAYBACK/mutations/MultipleImmediateOwners.cfg" fail:UniqueImmediateLifetimeOwner      "Ownership / MultipleImmediateOwners"
run_tlc PlaybackOwnership "$PLAYBACK/mutations/OwnershipCycle.cfg"         fail:OwnershipReachesLifecycleRoot       "Ownership / OwnershipCycle"
run_tlc PlaybackOwnership "$PLAYBACK/mutations/KernelAdoptsLifetimeOwnership.cfg" fail:SemanticAuthoritiesHoldNoLifetimeOwnership "Ownership / KernelAdoptsLifetimeOwnership"

if [[ "$fail" -eq 0 ]]; then
  echo "== 全部门通过"
else
  echo "== 存在失败项"
fi
exit "$fail"
