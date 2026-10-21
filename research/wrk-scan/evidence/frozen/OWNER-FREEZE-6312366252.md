# Owner 最终冻结裁决 — `R2_0_FROZEN`（仅协议；R2 执行仍不批准）

## 1. 结论与唯一冻结身份

**正式裁决：`R2_0_FROZEN`，批准冻结 `WRK-SCAN-v0/R2-0-v3`。** 这是一项**研究实验协议的不可变版本冻结**，**不是** H0–H4 机制实现许可、Native Linux 先导性能许可或正式 Benchmark GO。

```text
BEGIN_MVCS_R2_0_OWNER_FREEZE
ISSUE                        = #304
FREEZE_VERDICT               = R2_0_FROZEN
SPEC_ID                      = WRK-SCAN-v0/R2-0-v3
SUITE_SHA256                 = 6a00d0a86fcf76ab0b2f31602943914e5a3a395262c5d6c00b91ba94a0859281
CANONICALIZATION             = UTF8_LF_STRIP_TRAILING_LF
BINDING_MANIFEST             = #304 comment 6312183726
SPEC                         = #304 comment 6312123104
APP_A                        = #304 comment 6312139946
APP_B                        = #304 comment 6312142857
APP_C                        = #304 comment 6312145882
BASE_MAIN_SHA                = d2a140bea0c9b29e243af32544899e29b20d748a
G0_G3_RESEARCH               = ACCEPTED_WITH_EVIDENCE_LIMITS
ORACLE_M1_M9                 = REPORTED_EXECUTED / EXTERNALLY_REVIEWED
P1_OPEN                      = 0 (AS REPORTED IN INDEPENDENT REVIEWS)
P2_OPEN                      = 0 (AS REPORTED IN INDEPENDENT REVIEWS)
R2_PILOT_GO                  = NO
R2_H0_H4_IMPLEMENTATION_GO   = NO
R2_TIMED_BENCHMARK_GO        = NO
PRODUCTION_K0_ADR_CHANGE     = NOT_AUTHORIZED
NEW_GENERIC_RUNTIME          = NOT_AUTHORIZED
LOOM_EXTENSION               = NOT_AUTHORIZED
ISSUE_298                    = LEAVE_CLOSED
ISSUE_300                    = LEAVE_OPEN
ISSUE_304                    = LEAVE_OPEN
NEXT_GATE                    = SEPARATE R2_EXECUTION_ADMISSION
END_MVCS_R2_0_OWNER_FREEZE
```

## 2. 独立可核查的哈希结果

我直接重新读取 GitHub 上的四份**真实评论正文**，按 manifest 指定规则，对 UTF-8 内容按 LF 表示并剔除末尾所有 LF 后重新 SHA256：

| 冻结组成 | GitHub 评论 ID | 独立复算 Canonical SHA256 | 结果 |
|---|---:|---|---|
| SPEC v3 | 6312123104 | `48eb21c09aad6c0571fa953a40d9af7421826036eb64ca1182e23a46cc3dd4b8` | MATCH |
| APP-A | 6312139946 | `11b934af8925ff13fb91726b71d16bab3f6f4a3428691f0e258b1cefb0698b98` | MATCH |
| APP-B | 6312142857 | `a2fbb1c026998c6b2cec16487314d4cae5491faeb67951412e388d056b64757d` | MATCH |
| APP-C | 6312145882 | `8e8174f06bad3f79a52cce46b8d8a8f9d9fb364db5e0d9a4ca2d17e90a745852` | MATCH |

依 `SUITE_PREIMAGE` 的固定前缀和四行 `SPEC / APP-A / APP-B / APP-C` 重新串联并 SHA256，得 `6a00d0a86fcf76ab0b2f31602943914e5a3a395262c5d6c00b91ba94a0859281`，与申请冻结的组合根**完全一致**。

**辅助哈希差异的诚实披露（不影响上述组合身份）**：这次通过连接接口读取到的 GitHub 正文，对四项分别计算 *exact body SHA256* 时，均与 manifest 标注的 exact 值不一致；四个 manifest exact 值**恰好对应当前读取正文再追加一个尾部 LF**。说明两种读取/保存路径对尾部 LF 的表现相差一字节，**不是规范内容被编辑的证据**。因此本轮冻结时，**只有经独立复算 MATCH 的四项 Canonical SHA256 + SUITE_SHA256 属于规范身份的权威比较**；manifest 所列 exact body 值仅留为非权威、带尾部换行差异的传输层辅助收据。未来验证者不得声称该辅助项在当前读取路径上也 MATCH；不得偷偷改 SPEC/APP 评论以“修正”此差异。若需要统一 exact 路径，另发非规范解释收据即可，不改变本 Suite。

## 3. 语义冻结边界

- **已冻结**：`ScanItem/REF` 的字节算法与 endian/常量，S0–S6 的精确 item-id/class/seed/等待与计算档位，H0–H4（H2a/H2b 分臂）的 14 字段适配矩阵，K0 正交组合限制，统一 `B=admitted-not-terminal` 与 `P=active item execution` 预算，Admission/Cancellation/Terminal Owner/Settlement/Quiescence 语义，O1–O10 Oracle、M1–M9 负控制，仪器 ON/OFF 角色，测量九计时段与统计/平台预注册。
- **两项归档规范化属于本版本的一部分**：S1/S2 残差 Trivial、S4 `item_id % 10 == 0` 长尾、以及每 seed 依固定 SHA256 对 arm 排序，都不再是实现者可自由替换的“示例”。这些是已审查的工作形状/随机化配对语义的明确实例化；若将来要换映射、seed、比例或排序方法，必须发布新版本和新组合 SHA。
- **实验验证的证据上限**：当前被执行、通过独立复核的是 **G2 reference probe** 的 GREEN×30 / GREEN_NC×10 和九类 Mutation 的 RED（本次 Owner 未在本地重新编译/重跑探针；完整原始日志没有上传）。这不能自动外推到未来 H0–H4 的业务实现，也不能声称所有 O1–O10 分支均已执行、所有平台正确或实现/性能公平性已经实测成立。
- **边界继续有效**：`SYNTHETIC_WAIT` 不是 NATIVE_IO；WSL2 探针不是 Native Linux/Windows 性能证据；PCM 实时路径不获本研究的调度结论；K0 的 `!Send/!Sync` 和产品生命周期契约不得被移往执行线程。

## 4. 变更控制与下一道 Gate

**自本评论起，指定 `SUITE_SHA256` 是 R2-0 的唯一正式规范身份。** 旧的 #304 主报告 `6311599601`、Issue 草案正文和本地 v1/v2 仅保留研究沿革/讨论价值；解释协议应从本已绑定的 SPEC/APP 出发。不得原位改写已绑定评论。任何改变 REF、工作量映射、Admission/Owner ACK、Oracle、候选机制资源预算、统计方法或平台前提的修改，都必须新发版本化 `R2_0_AMENDMENT`（包含差异、旧/新 SHA、影响场景与 Owner 裁决），而不是“实现时顺便纠正”。

**下一道是独立 `R2_EXECUTION_ADMISSION`**，不是自动实施：
1. 决定 Native Linux 实际机器、CPU/NUMA/SMT、编译器、核心/线程数、CFS/affinity、运行预算及仪器；先导前书面固定 SESOI 与先导样本规则。
2. 分开审查最小 **H0 参考候选** 的实现与 G2 探针区别，以及 H1/H2a/H2b/H3/H4 的 B/P、线程池/offload 计量和资源一致性；不得把 probe 当成 H0。
3. 正式比较前完成 pilot 的 MDE/仪器开销/计时器粒度校准、negative controls、环境失效门槛和 `ORACLE_VALID` Gate。
4. 对先导运行、正式 Benchmark、额外平台重复和可能新 PR **分别申请 Owner 执行许可**，不得由本次协议冻结推导许可。

**最终原则：我们冻结了“什么算同一项工作、什么算正确结果、怎样公平对照”的契约，而没有批准任何新的执行机制或性能实验。**
