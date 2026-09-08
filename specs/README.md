# specs/ — 形式化模型注册表

`specs/` 保存 Qianqian 中值得进行状态空间验证的形式化模型。这里描述的是**长期系统语义**，不是开发阶段历史。

## 验证哲学

> **形式化验证优先用于发现高风险状态组合产生的反直觉错误，不用于为整个架构建立第二份完整实现。**

> **结构性架构边界优先通过类型系统、ownership、模块边界和普通测试约束；只有存在复杂状态交错风险时才升级为形式化模型。**

> **TLA+ 用来找撞车，不用来证明整个架构。**

## 命名规则

- spec 文件、TLA+ module、operator、invariant、mutation 与长期注释必须使用**稳定领域 vocabulary**（如 `PlaybackTemporal`、`PromotionRequiresSuccessfulFence`、`PromoteWithoutFence`）。
- ADR 编号 / Issue 编号 / PR 编号 / Corrective / Phase / milestone / gate 编号只作为 README 中的 **traceability 信息**，不进入模型 vocabulary 与文件名。

## 当前模型

| 模型 | 定位 | 负责验证 | 方法 |
| --- | --- | --- | --- |
| `playback/PlaybackTemporal` | **Core acceptance（blocking）** | 五组高风险 temporal 语义：Dual Window、Generation admission、Physical Fence、submitted/rendered 记账、EOF/drained/ENDED terminalization | TLA+ / TLC |
| `playback/PlaybackOwnership` | Extended exploration（supporting / non-blocking） | resource-lifecycle 假设：composition lifecycle root、TrackSession/DecodeSession immediate lifetime ownership、semantic authority 与 lifetime ownership 的区分、provider withdrawal 顺序 | TLA+ / TLC |

每个模型配备**负控制（negative controls）**：故意注入错误，TLC 必须抓到（counterexample 才算通过），以证明模型不是 vacuous。其中 4 个 core mutation（`PromoteWithoutFence` / `AcceptUnadmittedDecode` / `SingleGlobalGenerationCheck` / `EndBeforeRenderDrain`）属于 ADR ACCEPTED blocking 集；其余 mutation 属于 extended exploration / supporting evidence，不阻塞 ACCEPTED。

## 运行入口

```bash
# 全量（正常模型 + 全部负控制；缺省模式）
specs/check.sh

# 仅 core acceptance 集（PlaybackTemporal 正常模型 + 4 个 core mutation）
specs/check.sh core

# 显式全量
specs/check.sh all

# 需要代理下载工具链时：
export https_proxy=http://127.0.0.1:7897
specs/check.sh
```

工具链固定为 `tla2tools v1.7.4 (Xenophanes)`，`check.sh` 按内嵌 sha256 校验、fail closed。jar 不入库（见 `.gitignore`），由脚本自动下载。

各模型的语义说明、状态空间数据与负控制结果见 `playback/README.md`。

## Traceability

- `PlaybackTemporal` 五组语义 + 4 个 core mutation 对应 `docs/adr/ADR-PBK-001.md` §21 **Formal Acceptance** 必须项（blocking）；当前 core temporal checks = PASS。
- `PlaybackOwnership` 与其余 mutation 对应同节**支持证据**（non-blocking）：它们继续保留、继续运行，其 FAIL 不自动推出该 ADR 不能 ACCEPTED（除非发现 ADR 本身明确语义矛盾）。
- 模型 vocabulary 不使用 ADR/Issue/PR 编号；ADR 与模型的对应关系只在 README 层维护。
