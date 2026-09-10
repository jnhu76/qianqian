---
title: 实时音频研究
status: HISTORICAL_EVIDENCE
---

# 实时音频研究

<StatusBadge status="HISTORICAL_EVIDENCE" />

## 是什么让音频输出实时安全?

---

## 来源

<ClaimBadge role="evidence" />

来自 playback-reference-v1 冻结实验的历史证据。WASAPI renderer 与 PlayerEngine 架构。

| 来源 | 证据 |
|------|------|
| `native/src/player/wasapi_renderer.hpp` | 事件驱动渲染线程,无互斥引擎缝 |
| `native/src/player/pcm_ring.hpp` | 引擎自有的环形缓冲 |
| `native/src/player/playback_timeline.hpp` | 单一所有者时间线记账 |
| `native/src/player/wasapi_submit_accounting.hpp` | 提交 vs 渲染记账 |
| `native/src/player/null_audio_backend.*` | 无头正确性模式 |
| `native/src/player/commit_flush_handshake.hpp` | 单航次 commit/flush 协议 |

---

## 证据说了什么

<ClaimBadge role="evidence" />

### 渲染线程架构

WASAPI renderer 在无互斥的引擎缝上运行**事件驱动的渲染线程**:

- `fill_output` / `advance_render`,带 padding 证明的 playout
- renderer 在引擎**之后**创建、在引擎**之前**销毁
- 设备失败降级为有界重试 + 可听位置冻结

### 时间线所有权

单一所有者的时间线记账 —— 分散在 submit/render 两侧曾是真实的 bug 来源。修复方式:时间线所有权必须明确归属一侧。

### Commit/Flush 协议

单航次(single-flight)协议,四个状态:

```text
REQUESTED → CLAIMED → COMPLETED
                  ↘ CANCELLED
```

关键不变量:

- **I3:** claim 之前取消 = 从未开始
- **I4:** claimed 之后再也不能取消 —— 不伪造对可能已开始的物理 flush 的回滚
- **I5:** 无跨请求 ACK/ABA

### RT 数据边

音频数据流经预绑定端点,而不是逐块经过 Context/事件派发。

---

## Qianqian 借鉴了什么

<ClaimBadge role="interpretation" />

| 证据 | Qianqian 原则 |
|------|---------------|
| 无互斥渲染线程 | 实时路径零互斥 |
| 单一所有者时间线 | 时间线记账归属一侧 |
| renderer 生命周期顺序 | 提供者生命周期排序很重要 |
| 预绑定数据边 | 无逐块 Context 查找 |
| commit/flush 单航次 | 不可逆动作有显式协议 |

上表是对历史证据的解读。这些原则的当前 normative 形式以 `docs/adr/ADR-PBK-001.md` 为准（§2.4 realtime data plane、§6 P1–P5 publication/reclamation contract）；commit/flush 单航次是历史实验机制，**不是** P1–P5 的当前 implementation（机制 DEFERRED，Phase D 验证）。

---

## Qianqian 不借鉴什么

<ClaimBadge role="interpretation" />

- **WASAPI 特定实现** — 原则是平台无关的
- **具体缓冲尺寸** — 调优是实现相关的
- **设备枚举细节** — 平台事务,不是架构

---

## 架构后果

<ClaimBadge role="authority" />

normative 见 `docs/adr/ADR-PBK-001.md` §2.4；行为证据来源：overview.md 与 component-boundary-a0.md（历史证据，其 RT 行为事实仍有效）：

> 实时音频路径是数据平面孤岛。每个回调/数据块内不得执行 Context 查找、能力解析、Fiber 调和、任意事件派发、文件系统/网络 I/O 或 UI 往返。

未来的图变更必须在**控制平面**上准备,并在 **RT 安全边界**处发布。

---

## 开放问题

- AudioRuntime 插件应如何拥有 AudioGraph、时钟、缓冲池、格式协商、RT 调度与图发布?
- 发布控制平面图变更的正确 RT 安全边界是什么?
- commit/flush 协议能否推广到其他不可逆操作?

---

<ProvenancePanel
  :authority="['docs/adr/ADR-PBK-001.md']"
  :evidence="['docs/architecture/component-boundary-a0.md §A.2', 'research/playback-reference-v1']"
  :decisions="[{ issue: 53 }, { pr: 66 }]"
/>
