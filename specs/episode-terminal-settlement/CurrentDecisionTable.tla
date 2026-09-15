--------------------------- MODULE CurrentDecisionTable ---------------------------
(**************************************************************************)
(* 生成文件 —— 请勿手改。                                                 *)
(*                                                                        *)
(* episode terminal settlement 的 production ↔ formal 判决合同            *)
(* refinement oracle 共享真值表（CORRECTIVE-2）：对证据元组               *)
(*   <<stop_intent, decode_failure, worker_terminal, drain_verdict,       *)
(*     class>>                                                             *)
(* 在完整有限域（2 × 2 × 4 × 3 = 48 行）上穷举冻结**当前** production      *)
(* 判决器（qianqian-playback SessionCompletion::resolve，经 crate 内       *)
(* seam 逐元组驱动）的 decisive 域与判决类。                               *)
(*                                                                        *)
(* 生成/比对：crates/qianqian-playback/src/                                *)
(* decision_table_oracle.rs（Verification Rust Gate 执行；F2 起为          *)
(* crate-internal 白盒 oracle——evidence mutators 已收缩为 crate 私有）。  *)
(* 消费：EpisodeTerminalSettlementTable.tla 的 TLC run（Formal Semantic   *)
(* Gate 执行）逐行校验本表与 CurrentDecisionDecisive /                    *)
(* CurrentDecisionVerdict 一致。两侧共用这一个 artifact：改               *)
(* completion.rs 判决合同、手改本表、或改 TLA 判决函数，任何漂移都会击穿   *)
(* 对应 gate。                                                             *)
(*                                                                        *)
(* 再生成（仅限 authority 记录在案的判决合同变更，ADR-PBK-002 §17 D11）：  *)
(*   QIANQIAN_UPDATE_DECISION_TABLE=1 cargo test -p qianqian-playback \   *)
(*     --lib decision_table                                               *)
(* 之后必须重跑 specs/check.sh terminal（TLC refinement run 必须仍 PASS）。*)
(*                                                                        *)
(* verifier-only evidence（AGENTS.md verification authority boundary）：   *)
(* 本表是 refinement oracle，不是 authority；语义只在 ADR-PBK-001 §2 与    *)
(* ADR-PBK-002 §17 D11/D14。表冻结的是**静态**判决合同（证据形状 →         *)
(* 判决类，intent 在 settlement 边界固定）。动态差分闭合依据               *)
(* （CORRECTIVE-1）：每个 decisive evidence publication 路径在返回前同步    *)
(* 完成 authority settlement，request_stop 与 evidence publication 经同一  *)
(* completion 边界串行化，不存在异步 settlement 间隙。主模型 M4/W4 继续    *)
(* 机器检查该边界规则；Rust 侧 M4-RUST-A/B、W4-RUST 白盒 witness 直接      *)
(* 钉住同一规则。                                                          *)
(**************************************************************************)
DecisionTableRows ==
    {
      <<FALSE, FALSE, "None", "None", "undecided">>,
      <<FALSE, FALSE, "None", "Drained", "undecided">>,
      <<FALSE, FALSE, "None", "Aborted", "undecided">>,
      <<FALSE, FALSE, "Eof", "None", "undecided">>,
      <<FALSE, FALSE, "Eof", "Drained", "completed">>,
      <<FALSE, FALSE, "Eof", "Aborted", "failed-device">>,
      <<FALSE, FALSE, "Stopped", "None", "undecided">>,
      <<FALSE, FALSE, "Stopped", "Drained", "undecided">>,
      <<FALSE, FALSE, "Stopped", "Aborted", "failed-device">>,
      <<FALSE, FALSE, "Failed", "None", "failed-decode">>,
      <<FALSE, FALSE, "Failed", "Drained", "failed-decode">>,
      <<FALSE, FALSE, "Failed", "Aborted", "failed-decode">>,
      <<FALSE, TRUE, "None", "None", "failed-decode">>,
      <<FALSE, TRUE, "None", "Drained", "failed-decode">>,
      <<FALSE, TRUE, "None", "Aborted", "failed-decode">>,
      <<FALSE, TRUE, "Eof", "None", "failed-decode">>,
      <<FALSE, TRUE, "Eof", "Drained", "failed-decode">>,
      <<FALSE, TRUE, "Eof", "Aborted", "failed-decode">>,
      <<FALSE, TRUE, "Stopped", "None", "failed-decode">>,
      <<FALSE, TRUE, "Stopped", "Drained", "failed-decode">>,
      <<FALSE, TRUE, "Stopped", "Aborted", "failed-decode">>,
      <<FALSE, TRUE, "Failed", "None", "failed-decode">>,
      <<FALSE, TRUE, "Failed", "Drained", "failed-decode">>,
      <<FALSE, TRUE, "Failed", "Aborted", "failed-decode">>,
      <<TRUE, FALSE, "None", "None", "undecided">>,
      <<TRUE, FALSE, "None", "Drained", "undecided">>,
      <<TRUE, FALSE, "None", "Aborted", "undecided">>,
      <<TRUE, FALSE, "Eof", "None", "undecided">>,
      <<TRUE, FALSE, "Eof", "Drained", "completed">>,
      <<TRUE, FALSE, "Eof", "Aborted", "failed-device">>,
      <<TRUE, FALSE, "Stopped", "None", "undecided">>,
      <<TRUE, FALSE, "Stopped", "Drained", "undecided">>,
      <<TRUE, FALSE, "Stopped", "Aborted", "stopped">>,
      <<TRUE, FALSE, "Failed", "None", "failed-decode">>,
      <<TRUE, FALSE, "Failed", "Drained", "failed-decode">>,
      <<TRUE, FALSE, "Failed", "Aborted", "failed-decode">>,
      <<TRUE, TRUE, "None", "None", "failed-decode">>,
      <<TRUE, TRUE, "None", "Drained", "failed-decode">>,
      <<TRUE, TRUE, "None", "Aborted", "failed-decode">>,
      <<TRUE, TRUE, "Eof", "None", "failed-decode">>,
      <<TRUE, TRUE, "Eof", "Drained", "failed-decode">>,
      <<TRUE, TRUE, "Eof", "Aborted", "failed-decode">>,
      <<TRUE, TRUE, "Stopped", "None", "failed-decode">>,
      <<TRUE, TRUE, "Stopped", "Drained", "failed-decode">>,
      <<TRUE, TRUE, "Stopped", "Aborted", "failed-decode">>,
      <<TRUE, TRUE, "Failed", "None", "failed-decode">>,
      <<TRUE, TRUE, "Failed", "Drained", "failed-decode">>,
      <<TRUE, TRUE, "Failed", "Aborted", "failed-decode">>
    }

DecisionClasses == {"undecided", "completed", "stopped", "failed-decode", "failed-device"}
=============================================================================
\* #### EOF ####
