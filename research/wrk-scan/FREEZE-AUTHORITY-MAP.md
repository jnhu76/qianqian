# FREEZE_AUTHORITY_MAP — R2 Gate I（#306）

> SPEC_ID = `WRK-SCAN-v0/R2-0-v3`
> SUITE_SHA256 = `6a00d0a86fcf76ab0b2f31602943914e5a3a395262c5d6c00b91ba94a0859281`
> BASE_MAIN_SHA = `d2a140bea0c9b29e243af32544899e29b20d748a`（Owner Gate 6313541052 记录值；本 worktree HEAD 与之一致）
> 本文件是 Gate I 实现的权威恢复记录：冻结语义只认 #304 冻结评论；本文件路由，不复述规范。

## 1. 冻结身份复算（本轮 EXECUTED，2026-10-21）

```text
SPEC   canon_sha256 = 48eb21c09aad6c0571fa953a40d9af7421826036eb64ca1182e23a46cc3dd4b8   MATCH
APP-A  canon_sha256 = 11b934af8925ff13fb91726b71d16bab3f6f4a3428691f0e258b1cefb0698b98   MATCH
APP-B  canon_sha256 = a2fbb1c026998c6b2cec16487314d4cae5491faeb67951412e388d056b64757d   MATCH
APP-C  canon_sha256 = 8e8174f06bad3f79a52cce46b8d8a8f9d9fb364db5e0d9a4ca2d17e90a745852   MATCH
SUITE_PREIMAGE = "MVCS-R2-0-v3-SUITE\nSPEC <..>\nAPP-A <..>\nAPP-B <..>\nAPP-C <..>\n"
SUITE_SHA256   = 6a00d0a86fcf76ab0b2f31602943914e5a3a395262c5d6c00b91ba94a0859281        MATCH
CANON = UTF-8, LF, strip trailing LF only（MANIFEST §2 定义）
```

恢复副本：`evidence/frozen/`（GitHub body 原样下载，未做任何编辑）。

## 2. 冻结源码逐文件身份（APP 文本 ↔ 本机 G2 探针树）

本机存在归档轮的真实探针树 `/home/hoo/Source/qianqian-r20-wt/r20-probe/`（untracked、
standalone crate）。已逐文件核对 APP 内嵌源码文本与探针文件**字节一致**（6/6 BYTE_MATCH），
且探针文件 SHA256 与 MANIFEST §4 记录一致：

| 文件 | MANIFEST SHA256 | APP 文本 ↔ 探针树 |
|---|---|---|
| Cargo.toml | `cf2bbd48…` | BYTE_MATCH |
| src/workload.rs | `558a27dc…` | BYTE_MATCH |
| src/ledger.rs | `33d154de…` | BYTE_MATCH |
| src/oracle.rs | `3cfe25b2…` | BYTE_MATCH |
| src/reference_executor.rs | `78a54f2b…` | BYTE_MATCH |
| src/main.rs | `552b3844…` | BYTE_MATCH |
| run-cycle.sh | `35992a60…` | 探针树缺失；原始副本在 `/home/hoo/Source/qianqian-r20-evidence/run-cycle.sh`（未参与本轮，仅记录） |

结论：冻结源码身份链完整（SUITE → APP 文本 → 探针源文件三层一致）。
探针树中 `reference_executor.rs` 是 G2 Reference Executor：其**业务语义与纯函数**可复用，
其**调度逻辑**禁止充当 H0（Gate 授权 §2；SPEC §4.5）。

## 3. 权威映射

```text
REFERENCE_FUNCTION
  = workload::ref_result（APP-A workload.rs，冻结）
  语义权威：SPEC §2.2「REF(seed, spec)」。
  角色边界：REF 独立于被测执行器，仅供 Oracle/测试/参考结果生成；
  执行器路径禁止调用 ref_result（SPEC §2.5(b)）。

CONTENT_PRIMITIVES
  = SplitMix64 / generate_input / transform / content_digest /
    contribution / fold_meta / aggregate（APP-A workload.rs + APP-B oracle.rs::fold_meta，冻结）
  语义权威：SPEC §2.2 字节级算法定义；64 位目标；32 位 = UNSUPPORTED_TARGET_32BIT。
  WorkloadCore 必须逐字节复用这些原语；黄金向量见 §4。

ADMISSION_AUTHORITY
  = SPEC §4.2 ADMISSION_RECIPE（A–E 步，冻结）+ N1 三条实现安全裁定。
  线性化点 = 取消序临界区内追加 ItemAdmitted 的时刻（SPEC §5.2）。
  B 计数集合 = 队列 resident + running + completed-not-ACK（§4.1）；
  B ≠ 队列容量；cut 命中即释放 permit；受理满统一阻塞（N1(c)）。
  S5 取消里程碑 = workload owner 单线程受理计数第 164 次（§5.6，冻结）。

TERMINAL_AUTHORITY
  = SPEC §5.4。CompletedOk / CompletedErr(owner=完成 item 的真实执行者)；
  Abandoned(owner=观察 cancel 时真实持有 item 的执行者)。
  每 admitted item 恰一条 terminal ACK；Harness 永不写 terminal。
  PD 前提：每条 item 执行路径 panic-contained，owner 存活到 terminal 发布；
  无法安全清偿 => 会话显式失败，不得伪造 Quiescence（Gate 授权 §5）。

EFFECT_BOUNDARY
  = SPEC §6.1 双账本。RAW_EFFECT_LOG 在真实效果边界由发生线程同步追加；
  OWNER_TERMINAL_ACK_LOG 由真实 terminal owner 追加。
  唯一序列化边界 = 账本互斥锁分配的全局 seq；因果判断只用 seq
  （不用 wall-clock / 队列空 / 超时 / sleep / 完成计数）。
  LedgerRecord schema 冻结于 APP-A ledger.rs。

CANCELLATION_AUTHORITY
  = SPEC §5.3。request_cancel 只承诺「意图已入账 + 组受理已切断」；
  已受理 item 可完成或由 owner Abandon；非法：无 cancel 的 Abandoned（O5）、
  cancel 后同组受理（O7）、Abandoned 后 ResultPublished。
  CancelRequested 账本追加必须与 cut 检查同处取消序临界区
  （保证 abandon ACK seq > cancel seq，探针已证此形态）。

QUIESCENCE_AUTHORITY
  = SPEC §5.8。QuiescenceReceipt 由会话 shutdown owner 在
  （1）聚合已发布（2）每个 ExecutorSpawned 有 seq 更小的 ExecutorExited
  （3）settlement 完成（4）收据是账本最后一条 —— 全部满足后追加。
  O6/O10 是裁定谓词；三动作 request_cancel / await_settlement /
  await_quiescence 分离（§5.5）。

K0_COMPOSITION_OWNER
  = crates/qianqian-composition（仓库 HEAD 的 CompositionKernel；
  SPEC §1 G0）。K0 ON 时：scan session 作为 K0 组合中的 Fiber 运行；
  Composition Owner（!Send/!Sync）留在控制线程；K0 不参与每 item 调度；
  inverse 必须含 Stop/Settlement/Join 责任且 total；
  DisposeVerdict::TeardownViolated 记失败，不得用 harness 代 join 补救
  （SPEC §4.6 强制约束 1–5）。
```

## 4. 黄金向量计划（Phase 1 验证权威）

```text
GV-1  SplitMix64 独立向量：以 SPEC §2.2 伪代码的 python 独立实现计算，
      与 Rust 实现比对（独立来源，非自证）。
GV-2  探针批次 AGG_VALUE：本轮重算必须等于
      0x980a9dc0d8d8cee6（SPEC §2.3 冻结的 2026-10-21 WSL2 EXECUTED 收据）。
GV-3  逐 item REF 身份：新 WorkloadCore 的 (metadata_digest,
      aggregation_contribution) 逐 item 等于冻结 workload.rs 原文
      （evidence/frozen 提取的冻结模块，仅测试编译）。
GV-4  输入身份：generate_input 长度域 4096..=65536 步长 1024；
      content_digest = SHA256(LE4(item_id) || bytes)。
GV-5  Err 车道：fault item 完成全部共同工作后 CompletedErr(0x31)，
      contribution 与 Ok 车道同规则钉定（O3(c)）。
GV-6  Fan-in：aggregate 对输入对序列 rotate_left(7) 不变性（REFCHECK 语义）。
GV-7  输入顺序不变性：fold 按 item_id 升序，与完成顺序无关。
```

## 5. ARCHIVE_NORMALIZATION 两处确认（不得擅改）

1. **item_id 分布**：S0–S6 的类别映射按 SPEC §3.1 的 `item_id` 函数逐场景冻结
   （common_base5 / mod 5 == 3 残差 / mod 10 == 0 LongTail / 全 Trivial 等）。
   本轮不重新设计分布、不改随机分配、不改 Round Robin、不换哈希。
2. **每 seed 的 arm 顺序**：SPEC §7.3 冻结规则
   `SHA256(LE8(seed) || 0x00 || ASCII(arm_id))` 前 8 字节 LE 解释 u64 升序。
   本轮不执行 arm 排名，但 arm identity/顺序规则必须在 92-cell 记录中保留
   （记录 `arm_id` 命名与该 seed 的冻结顺序值），供未来 Gate 使用。

## 6. 目标机 46-cell 展开（P_phys=10，本轮 EXECUTED 核实）

本机 lscpu：i7-12700H，10 物理核 / 20 逻辑核（SMT×2）=> `P_phys = 10`。
按 SPEC §3.4 派生规则（场景档位 ∩ {1..P_phys} ∪ {floor(P_phys/2), P_phys}）：

| Scenario | N | P | B | Cells |
|---|---:|---|---|---:|
| S0 | 1024 | 1,4,5,10 | P,2P | 8 |
| S1 | 256 | 4,5,8,10 | P,2P,4P | 12 |
| S2 | 128 | 5,10 | P | 2 |
| S3 | 256 | 4,5,10 | 2P,4P | 6 |
| S4 | 256 | 4,5,10 | 2P | 3 |
| S5 | 256 | 4,5,10 | 2P | 3 |
| S6 | 4096 | 4,5,8,10 | P,2P,4P | 12 |
| 合计 | | | | **46** |

× K0 {OFF, ON} = **92 correctness cases**（非计时；至少固定公开 seed
`0x00C0FFEE03040001`）。等待档位唯一枚举（SPEC §3.6）：S0/S2=0；S1=5ms；
S3/S5=2ms；S4 仅 LongTail=2ms 其余 0；S6=2ms。fault 仅 S3/S5（%16==13→0x31）；
cancel 仅 S5（第 164 次受理后，group 7）。

## 7. 本轮实现边界（对冻结文本的三处最小扩展，全部留痕）

1. **场景参数显式化**：冻结 ScanItemSpec 用类默认 (cpu_blocks, wait_us)；
   SPEC §3.1 场景值是覆盖值。新 WorkloadCore 在 item spec 上显式携带生效
   `(cpu_blocks, wait_us)`；探针批次取类默认 => 与冻结行为逐字节一致（GV-2/GV-3 守护）。
2. **ObserverSink OFF**：冻结账本即 ON 模式；OFF 模式为最小回执
   （SPEC §7.5「三时间戳最小仪器」），同一执行器同一调用点，仅记录行为不同。
3. **K0 ON**：冻结附录不含 K0 组合；按 SPEC §4.6 与仓库 K0 API 新增
   session 组合模块；K0 不进每 item 路径。

依赖声明：`Cargo.lock` 将 sha2 固定到 0.11.1（冻结探针工件为 0.11.0）。
语义同一性不由版本号承担，而由黄金向量（GV-2/GV-3，AGG =
0x980a9dc0d8d8cee6）与独立 python 实现复核守护；此处仅声明锁级差异。

回执落位：§5.2 承诺的「arm_id 命名与该 seed 的冻结顺序值」已在
92-cell 回执中兑现（`cells.jsonl` 的 `ARM` 与 `ARM_ORDER_VALUE` 字段，
顺序值 = `SHA256(LE8(seed) || 0x00 || ASCII(arm_id))` 前 8 字节 LE u64，
本轮不做排名）。

除此之外任何与冻结文本的冲突 => `PROTOCOL_AMENDMENT_REQUIRED`，停止并上报，
不静默改规范。

## 8. Gate II 追加记录（#308，2026-10-22，EXECUTED）

本轮（R2 Gate II H1）不改动任何冻结语义；以下为 H1 机制轮的权威路由与身份记录。

### 8.1 冻结身份复算（Gate II 执行基线核验）

```text
执行基线   = main@1375d4853ef595ffb57180b43d65f655ed8cf5f5（PR #307 merge = Gate I 收口）
Owner GO   = GATE_II_H1_GO（#308 comment 6330781617）：
             CHANNEL_LIBRARY = crossbeam-channel 0.5（research-only）
             QUEUE_Q = B；SHARED_CODE = A_NARROW
SPEC/APP-A/APP-B/APP-C 四正文 canonical SHA256 与 GitHub live body 逐项复算：
             全部 MATCH；SUITE_SHA256 = 6a00d0a8…9281 MATCH（2026-10-22 复算）
```

### 8.2 H1 对冻结文本的扩展（全部为机制实现，不触语义）

```text
新依赖     crossbeam-channel 0.5（Cargo.lock pin 0.5.17；仅本 research crate；
           SPEC §1 G0 的"生产无 crossbeam"指生产 crate，研究 crate 依 Owner GO 引入）
H1 机制    = SPEC §4.4 表 H1 列（bounded channel 容量 C = Q = B，Owner GO §1.B 的
           C>=B 分支）；Q 与 B 分开计量（b_high_water / q_high_water / 
           channel_resident 三 witness，绝不同一计数器）
受理配方   = §4.2 ADMISSION_RECIPE 原样实现：B permit（condvar）→ 取消序临界区
           （义务登记 → 非阻塞 try_send → ItemAdmitted 入账）；
           ADMISSION_LINEARIZATION = 临界区内 ItemAdmitted 追加时刻
共享业务   = session_core::execute_item（A_NARROW；原 h0.rs 函数逐字迁移，
           唯一例外：cfg(test) 注入 panic 消息字面量改为机制中立措辞；
           patch 迁移映射见 GATE-II-MECHANISM-MAP.md §3）
REF 边界   不变：执行路径禁止 ref_result（§2.5(b)），oracle 独立
```

### 8.3 Gate II 证据归档路由

```text
evidence/gate-ii-motion/     A_NARROW 搬移后 H0 回归（92-cell receipt、
                             M1–M9+LW/PB round-2 收据 + round-1 的 M2
                             PATCH 编译失败诚实存档、逐 arm source-sha）
evidence/gate-ii-final/      最终 HEAD 的 H0/H1 双 92-cell receipt + hermeticity witness
evidence/gate-ii-mutations-h1/  H1 M1–M9 campaign 收据（逐 arm log + source-sha）
evidence/gate-ii-orthogonality/ K0×Observer 2×2 smoke 收据
H0 历史证据    evidence/gate-i-*/（原样保留，绝不覆盖）
```
