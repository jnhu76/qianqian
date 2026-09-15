--------------------------- MODULE EpisodeTerminalSettlementTable ---------------------------
(***************************************************************************)
(* EpisodeTerminalSettlementTable — production ↔ formal 判决合同             *)
(*                                  refinement oracle（CORRECTIVE-2）        *)
(*                                                                         *)
(* STATUS: verifier-only refinement oracle（非 normative）。本模块不新增     *)
(*   任何语义：它把三件事钉在同一张有限真值表（CurrentDecisionTable.tla，    *)
(*   2 × 2 × 4 × 3 = 48 行，穷举）上：                                       *)
(*                                                                         *)
(*     1. production 当前判决合同：qianqian-playback                        *)
(*        SessionCompletion::resolve()——由 Rust 侧 exhaustive oracle        *)
(*        (crates/qianqian-playback/tests/completion_decision_table.rs)     *)
(*        经公开 seam 逐元组驱动并生成/比对表文件（Verification Rust        *)
(*        Gate 执行）；                                                      *)
(*     2. formal 判决域：CurrentDecisionDecisive；                           *)
(*     3. formal 判决值：CurrentDecisionVerdict。                            *)
(*                                                                         *)
(*   本模块的 TLC run（check.sh 接入，Formal Semantic Gate 执行）以          *)
(*   DecisionDomain 的全部 48 个元组为初始状态，逐行校验表 ↔ formal 判决     *)
(*   函数一致。任何一侧漂移（production 分支增删 / 表文件手改 / TLA 判决     *)
(*   函数改写）都会击穿对应 gate——堵住"两个 truth source 无 durable          *)
(*   binding、静默漂移"的缺陷类（CORRECTIVE-2 的 MAJOR）。                   *)
(*                                                                         *)
(* 边界声明（AGENTS.md verification authority boundary）：                    *)
(*   - 这里的不变式是 refinement 诊断，不定义 lifecycle / correctness        *)
(*     语义；W7/M10 证明的是"给定 CurrentDecision* 这份 formal ground        *)
(*     truth，C5 等性质对完整域承重"，而这份 ground truth 与 production       *)
(*     的相等性由本 oracle 机器维护，不靠人工逐支核对。                      *)
(*   - failed-decode / failed-device 在模型 verdict 中同为 "Failed"          *)
(*     （stage 文本是 realization）；OutcomeClassOf 的投影只用于比较。       *)
(*   - 表冻结的是**静态**判决合同（证据形状 → 判决类，intent 在 resolve      *)
(*     时刻固定）。production resolve() 读当前 stop intent 的 known           *)
(*     differential（late intent）不在表域内——由主模型 M4/W4 机器检查、      *)
(*     D11 记录、F2 修正。                                                   *)
(***************************************************************************)

EXTENDS EpisodeTerminalSettlement, CurrentDecisionTable, FiniteSets

VARIABLE probe
\* 下标必须是"模块（含 EXTENDS 链）声明的全部变量"的元组，否则 TLC
\* 发 subscript warning（runner fail closed）。
tableVars == <<stopSeen, decodeFailure, workerTerminal, drainVerdict,
              activationFailed, terminalOutcome, episodeLifecycle,
              decisionLatched, stopAtDecision,
              firstCommitted, observeRan, waitRan, probe>>

StopIntents == {FALSE, TRUE}

\* 完整证据元组域（与 Rust oracle 的枚举域同一有限域）。
DecisionDomain ==
    {<<sa, df, wt, dv>> :
        sa \in StopIntents, df \in BOOLEAN,
        wt \in WorkerTerminals, dv \in DrainVerdicts}

\* 表内 class → 模型 outcome class（stage 细分在 verdict 层折叠为 "Failed"）。
OutcomeClassOf(cls) ==
    IF cls = "undecided" THEN "None"
    ELSE IF cls = "completed" THEN "Completed"
    ELSE IF cls = "stopped" THEN "Stopped"
    ELSE "Failed"

\* 行 key（前四元组）：DecisionDomain 的一个证据元组。
RowKey(r) == <<r[1], r[2], r[3], r[4]>>

\* 行完整性 + **显式 bijection**：行数 = 域大小、每行形状合法、且
\* **每个域 key 恰好一行**。"48 行穷举、key 无缺无重"因此是本模块自身
\* 的明确 theorem，不依赖 RowFor 无解 CHOOSE 的运行时行为兜底
\* （key 重复两行不同 class、或 key 缺失而行数仍凑够，都被
\* Cardinality 子句直接击穿）。
TableRowsWellFormed ==
    /\ Cardinality(DecisionTableRows) = Cardinality(DecisionDomain)
    /\ \A r \in DecisionTableRows :
        /\ RowKey(r) \in DecisionDomain
        /\ r[5] \in DecisionClasses
    /\ \A t \in DecisionDomain :
        Cardinality({r \in DecisionTableRows : RowKey(r) = t}) = 1

\* key → 行查找。上面的 bijection 子句保证选择集非空且唯一，
\* CHOOSE 在此只是查找，不承担完整性检测。
RowFor(t) == CHOOSE r \in DecisionTableRows : RowKey(r) = t

\* 穷举枚举器：48 个初始状态、其后纯 stutter。不变式在每个初始状态上
\* 逐行比对（对全部域元组完整覆盖）。EXTENDS 继承主模块声明的全部
\* 变量，因此 Init 复用主模块初始谓词把它们钉在初始值（本 run 对它们
\* 无任何兴趣——[TableNext]_tableVars 之外它们恒 stutter）。
TableInit == Init /\ probe \in DecisionDomain
TableNext == UNCHANGED <<vars, probe>>
TableSpec == TableInit /\ [][TableNext]_tableVars

\* 比对一：decisive 域一致（表 undecided <=> formal 不可判决）。
TableDecisiveMatchesContract ==
    LET r == RowFor(probe)
    IN (r[5] = "undecided") <=> ~CurrentDecisionDecisive(probe[2], probe[3], probe[4])

\* 比对二：判决值一致（class 经 OutcomeClassOf 投影后等于 formal verdict）。
TableVerdictMatchesContract ==
    LET r == RowFor(probe)
    IN OutcomeClassOf(r[5]) =
       CurrentDecisionVerdict(probe[1], probe[2], probe[3], probe[4])

=============================================================================
\* #### EOF ####
