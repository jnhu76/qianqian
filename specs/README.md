# specs/ — 形式化模型注册表

`specs/` 保存 Qianqian 中值得进行状态空间验证的形式化模型。这里描述的是**长期系统语义**，不是开发阶段历史。

## 命名规则

- spec 文件、TLA+ module、operator、invariant、mutation 与长期注释必须使用**稳定领域 vocabulary**（如 `PlaybackTemporal`、`PromotionRequiresSuccessfulFence`、`PromoteWithoutFence`）。
- ADR 编号 / Issue 编号 / PR 编号 / Corrective / Phase / milestone 只作为 README 中的 **traceability 信息**，不进入模型 vocabulary 与文件名。

## 当前模型

| 模型 | 负责验证 | 方法 |
| --- | --- | --- |
| `playback/PlaybackTemporal` | Dual Window、Generation admission、seek/next/stop 骨架、Physical Fence、submitted/rendered 记账、EOF/drained/ENDED 层次、迟到 decode 拒绝、rapid supersede | TLA+ / TLC |
| `playback/PlaybackOwnership` | composition lifecycle root、TrackSession/DecodeSession immediate lifetime ownership、semantic authority 与 lifetime ownership 的区分、provider withdrawal 顺序 | TLA+ / TLC |

每个模型配备**负控制（negative controls）**：故意注入错误，TLC 必须抓到（counterexample 才算通过），以证明模型不是 vacuous。

## 运行入口

```bash
# 全量运行（正常模型 + 全部负控制）
specs/check.sh

# 需要代理下载工具链时：
export https_proxy=http://127.0.0.1:7897
specs/check.sh
```

工具链固定为 `tla2tools v1.7.4 (Xenophanes)`，`check.sh` 按内嵌 sha256 校验、fail closed。jar 不入库（见 `.gitignore`），由脚本自动下载。

各模型的语义说明、状态空间数据与负控制结果见 `playback/README.md`。

## Traceability

- `PlaybackTemporal` / `PlaybackOwnership` 验证 `docs/adr/ADR-PBK-001.md`（PROPOSED / Corrective-2 / Formal Gate Pending）§21 Formal Gate 的 F1（PlaybackTemporal）、F2（PlaybackOwnership）、F3（Negative Controls，含 BUG-A..E）。
- 模型 vocabulary 不使用 ADR/Issue/PR 编号；ADR 与模型的对应关系只在 README 层维护。
