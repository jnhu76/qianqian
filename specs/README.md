# specs/ — 形式化模型注册表

> **STATUS: EXPERIMENTAL EVIDENCE（playback/ 历史套件）+ FORMAL EVIDENCE（realtime-publication/ 当前套件）**
>
> `specs/playback/` 的模型保留了早期 Playback 架构实验的 failure witnesses 与验证技术。
> 它们**不是当前 Playback architecture authority**，也**不是新 Playback 设计的 acceptance gate**。
> 当前播放架构 authority：`docs/adr/ADR-PBK-001.md`（**ACCEPTED**）。
> 可复用：mutation 技术、具体 counterexample、verifier runner；不得要求新架构镜像旧变量/状态/名词。
> `specs/realtime-publication/` 是重置后新建的 publication lifetime 语义级
> 证据：其针对的交错正是 ADR §13 点名的头号候选（旧视图引用 provider → 新
> 视图排除 → 旧 reader 仍在 → final release），TLC 穷举证明该 collision 在
> 模型空间真实可达（风险驱动成立）；实现层碰撞确认仍随 §12 Phase D 展开。
> 语义范围来自 ACCEPTED ADR §6；机制 representation 仍 OPEN。

`specs/` 保存 Qianqian 中值得进行状态空间验证的形式化模型。这里描述的是**长期系统语义**，不是开发阶段历史。

## 验证哲学

> **形式化验证优先用于发现高风险状态组合产生的反直觉错误，不用于为整个架构建立第二份完整实现。**

> **结构性架构边界优先通过类型系统、ownership、模块边界和普通测试约束；只有存在复杂状态交错风险时才升级为形式化模型。**

> **TLA+ 用来找撞车，不用来证明整个架构。**

### 什么时候值得形式化

优先把形式化验证用于**多个单独合法的状态、事件或所有权变化组合后，可能到达非法状态**的地方。典型信号包括：

- **状态交错**：同一事实会被多个异步事件推进，例如 `seek / stop / EOF / render evidence` 的交错；
- **并发 ownership / lifecycle**：资源退出、provider withdrawal、dependent teardown 之间存在先后约束，而且错误顺序可能产生悬挂资源或失效访问；
- **不可逆边界**：软件状态变化与外部世界之间存在 point-of-no-return，例如历史实验中的 Physical Fence、提交到设备、持久化提交；
- **合法事件组合可能产生非法结果**：每个动作单独看都正确，但组合后可能出现 stale re-entry、双 authority、提前终态化、死锁或不可恢复状态；
- **普通测试难覆盖所有排列**：问题的风险主要来自 action ordering / interleaving，而不是某个单一函数的输入输出。

这类问题适合使用 TLA+/TLC、针对性的并发模型检查或其他状态空间工具主动寻找 counterexample。

### 什么时候不应该形式化

以下问题默认**不升级为形式化模型**，除非后来出现了真实的状态交错风险：

- 命名与 vocabulary 选择；
- 普通模块、crate、component 边界；
- 可以直接由 Rust ownership / borrowing / 类型系统约束的简单所有权关系；
- 数据结构 representation，例如 `Box` / `Arc` / handle / token 的具体选择；
- 可以由普通单元测试、属性测试或静态检查充分覆盖的局部逻辑；
- 单纯为了让 ADR、Issue 或阶段 gate 获得“形式化证明”标签而建立的模型。

**不要采用“架构里有一个概念，就为它建立一个模型”的做法。**

### 风险驱动原则

形式化验证的入口应当是一个明确的问题：

> **这里有哪些独立合法的状态或事件，可能因为交错而撞出一个非法状态？**

如果回答不出这个问题，优先使用更便宜、更直接的约束手段。

推荐顺序：

```text
类型系统 / ownership / 模块边界
        ↓
普通测试 / 属性测试 / 静态检查
        ↓
确认存在高风险状态交错
        ↓
形式化模型 / 状态空间探索
```

模型得到的结论是**在其显式 abstraction 与 assumptions 下的证据**，不是架构本身的第二份 authority。模型为了闭合状态空间所做的选择，不得未经 ADR/设计 review 就自动升级为生产语义。

## 命名规则

- spec 文件、TLA+ module、operator、invariant、mutation 与长期注释必须使用**稳定领域 vocabulary**（如 `PlaybackTemporal`、`PromotionRequiresSuccessfulFence`、`PromoteWithoutFence`）。
- ADR 编号 / Issue 编号 / PR 编号 / Corrective / Phase / milestone / gate 编号只作为 README 中的 **traceability 信息**，不进入模型 vocabulary 与文件名。

## 当前模型

| 模型 | 定位 | 负责验证 | 方法 |
| --- | --- | --- | --- |
| `playback/PlaybackTemporal` | Core evidence set（历史 blocking 定位已退役） | 五组高风险 temporal 语义：Dual Window、Generation admission、Physical Fence、submitted/rendered 记账、EOF/drained/ENDED terminalization | TLA+ / TLC |
| `playback/PlaybackOwnership` | Extended exploration（supporting / non-blocking） | resource-lifecycle 假设：composition lifecycle root、TrackSession/DecodeSession immediate lifetime ownership、semantic authority 与 lifetime ownership 的区分、provider withdrawal 顺序 | TLA+ / TLC |
| `realtime-publication/RealtimePublication` | 当前挣得的 formal evidence（语义来源：ACCEPTED ADR-PBK-001 §6） | realtime view publication / reader quiescence / resource reclamation：coherent publication、retired 视图闭门、quiescence 先于释放、多代退休记账、回收可达性 | TLA+ / TLC（含 liveness 性质与可达性探针） |

每个模型配备**负控制（negative controls）**：故意注入错误，TLC 必须抓到（counterexample 才算通过），以证明模型不是 vacuous。其中 4 个 core mutation（`PromoteWithoutFence` / `AcceptUnadmittedDecode` / `SingleGlobalGenerationCheck` / `EndBeforeRenderDrain`）曾是 ADR ACCEPTED 的 blocking 集；播放架构重置后该 blocking 定位已退役，`playback/` 两模型现在统一是 experimental evidence，不是新 Playback 设计的 acceptance gate（`realtime-publication/` 套件定位见上文，不属于本段历史）。

## 运行入口

```bash
# 全量（playback 历史套件 + realtime-publication 当前套件；缺省模式）
specs/check.sh

# core 集（PlaybackTemporal 正常模型 + 4 个 core mutation + realtime-publication 套件）
specs/check.sh core

# 显式全量
specs/check.sh all

# 需要代理下载工具链时：
export https_proxy=http://127.0.0.1:7897
specs/check.sh
```

工具链固定为 `tla2tools v1.7.4 (Xenophanes)`，`check.sh` 按内嵌 sha256 校验、fail closed。jar 不入库（见 `.gitignore`），由脚本自动下载。

`playback/` 各模型的语义说明、状态空间数据与负控制结果见 `playback/README.md`；`realtime-publication/` 套件的对应信息见 `realtime-publication/README.md`。

## Traceability

- `PlaybackTemporal` 五组语义 + 4 个 core mutation 曾对应旧版 ADR 的 **Formal Acceptance** 章节（该章节已随重置移除）；当前 core temporal checks = PASS 只作为历史证据记录，不是当前 ADR 的 acceptance 项。
- `PlaybackOwnership` 与其余 mutation 对应同节**支持证据**：它们继续保留、继续运行，作为 experimental evidence；其结论不构成当前 ADR 的 acceptance 项。
- 模型 vocabulary 不使用 ADR/Issue/PR 编号；ADR 与模型的对应关系只在 README 层维护。
