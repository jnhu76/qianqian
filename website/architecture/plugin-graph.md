---
title: 插件图
status: NEXT
---

# 插件图

<StatusBadge status="NEXT" />

插件图描述 **Composition topology**：哪些长期能力真的通过 Capability/Fiber 参与组合。它不是 PCM Processing Graph。

> **逻辑插件边界 != crate / 动态库边界。**

---

## 组合边界示例（历史实验映射）

```mermaid
flowchart LR
    MUSIC["MusicComponent<br/>composition lifecycle root"]
    DEC["Decoder provider<br/>encoded media → canonical PCM"]
    AOUT["AudioOutput / PcmSink provider<br/>PCM → physical + evidence"]

    MUSIC -->|"requires Decoder"| DEC
    MUSIC -->|"requires AudioOutput / PcmSink"| AOUT
```

> **Historical / experimental evidence:** 上图 `MusicComponent` 与下述 `MusicKernel` / `TransportKernel` / `TrackSession` / `DecodeSession` 结构来自旧 Playback 架构实验（#53 分解），重置后不再是 current authority（ADR-PBK-001 现为 ACCEPTED，不冻结这些名词；playback-specific component granularity 需由未来实验重新挣得）。旧代码中只保留为 experimental evidence：

```text
MusicKernel      music/product semantic authority
TransportKernel  playback temporal authority
TrackSession(s)
DecodeSession(s)
```

这些内部对象不因为有状态就自动成为独立 Fiber。

---

## Processing Graph 不是 Plugin Graph

```mermaid
flowchart LR
    GAIN["Gain"] --> EQ["EQ"] --> SRC["SRC"] --> LIM["Limiter"]
```

Gain / EQ / SRC / Limiter 是有顺序的 PCM transform。它们的普通插入、移除和参数更新属于 processing topology，不自动触发 Composition Reconcile。

只有当一个 Processing provider 真正获得独立组合身份、对外 Capability、replace/withdraw boundary 时，它才进入 Composition graph。

---

## 未来组件

```mermaid
flowchart TB
    MUSIC2["MusicComponent（历史示例锚点）"]
    PROC["independent Processing provider<br/>仅在边界被证明后"]
    UH["UiHost<br/>presentation mechanism"]

    MUSIC2 -.->|"future, if earned"| PROC
    UH -.->|"consumes domain contracts"| MUSIC2
```

> 图中的 `MusicComponent` 锚点只是历史示例；未来组合边界由实验挣得，不由本页决定。

---

## 组件依赖矩阵

| 组件 | 依赖 | 提供 / 角色 |
|---|---|---|
| MusicComponent *（历史示例，非 current authority）* | Decoder、AudioOutput/PcmSink | Playback/domain contracts；内部 lifecycle root |
| Decoder | 无 | Decoder capability |
| AudioOutput | 无 | PcmSink / output evidence / optional device control |
| independent Processing provider *(未来，若挣得边界)* | 由未来 contract 决定 | ordered PCM graph service |
| UiHost *(未来)* | domain/presentation contracts | UI mechanism |

---

## 边界判断

一个候选成为 plugin/capability 前，要回答：

- 是否具有独立组合身份？
- 外部 component 是否通过 Capability 绑定它？
- 是否可以独立 replace / withdraw？
- 是否存在 composition-level teardown boundary？
- 更细的拆分是否真的换来 composability，而不是只增加命名与配置成本？

不同特性名、不同 Rust struct、甚至独立线程，都不是独立 plugin 的充分证据。

---

<ProvenancePanel
  :authority="['docs/adr/ADR-PBK-001.md']"
  :decisions="[{ pr: 66 }, { pr: 78 }, { pr: 79 }]"
  :evidence="['crates/qianqian-audio-api/src/ports.rs', 'crates/qianqian-audio-api/tests/playback_temporal_traces/transport.rs']"
/>
