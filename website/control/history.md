---
title: 历史
status: CURRENT
---

# 项目历史

Qianqian Architecture v2 的关键里程碑。

---

## 时间线

### R0 引导(设计前)

创建了基础 Rust crates:

- `qianqian-core` — 空基座、MusicKernel 状态机、三个空 port trait
- `qianqian-runtime` — AppRuntime 构造组合
- `apps/headless` — CLI 运行器

**引导见证**(`AppRuntime::new()`、`with_audio_output()`、`audio_output()`)不是兼容性契约。

### 组件边界审计 — Issue #53

**PASS / CLOSED**

组件边界审计为首个 Windows 切片论证了三个运行时组件:Music、Decoder、AudioOutput。冻结事实包括:

- MVP 运行时组件:Music、Decoder、AudioOutput
- 能力图:Music 依赖 Decoder + PcmSink
- 激活规则:Music 在激活时绑定 PcmSink
- 撤回排序:依赖方在提供者最终释放之前完成 teardown
- FFmpeg 闭包权威:单一共享权威,Decoder/Processing

> #53 中 playback-specific 的 ownership/granularity 结论(如 Music 同时拥有 worker/ring/timeline、拒绝 Transport 拆分)是**历史输入**;播放架构已于 2026-09 从第一性原理重开,旧 playback 模型(包括其后短暂 ACCEPTED 的 MusicKernel/TransportKernel 拆分)统一降级为 experimental evidence,均非 current authority。当前 authority 见 ADR-PBK-001(ACCEPTED)。Generic Composition Kernel 历史结论仍然有效。

[阅读审计 →](https://github.com/jnhu76/qianqian/issues/53)
[PR #66 已合并](https://github.com/jnhu76/qianqian/pull/66)

### Playback Reference v1 — 已冻结

已冻结的播放实验,证明本地文件 → 解码 → PCM → 物理输出。以 branch/tag 形式保存,不在 main 上。

**保留的证据:** SongCore ABI v1、PlayerEngine C ABI、commit-flush 握手、WASAPI renderer、null audio 后端、FFmpeg 构建 profiles。

### Composition Kernel K0 — 语义设计 — Issue #67

**MERGED via PR #68**

通用 Composition Kernel 的语义设计:五个原语(Context、Capability、Fiber、Effect、Reconcile),论文基础为 *A Programming Paradigm for Spatiotemporal Composability*(arXiv:2608.25512v1)。

[阅读设计 →](/architecture/base-kernel)

### Composition Kernel K0 — 实现 — Issue #70

**IMPLEMENTED via PR #71**

六个语义保证组共 70 项内核测试(75 项 workspace 测试)。领域无关内核不了解音乐、PCM、FFmpeg、WASAPI 或 UI。

合并于 commit `743eb86`。

### Playback Foundations reset — PR #87

**MERGED via PR #87**

播放基础从第一性原理重置:ADR-PBK-001(**ACCEPTED**)冻结四平面宪法、command/fact authority、fact-authority identity(每个 (fact kind, subject scope) 一个 designated authority)、projection read-side firewall 与 realtime 数据面边界;旧 playback 状态机名词(MusicKernel / TransportKernel / Active-Prepared / Generation / Physical Fence 等)全部重新开放,统一降级为 experimental evidence。

[ADR-PBK-001 →](https://github.com/jnhu76/qianqian/blob/main/docs/adr/ADR-PBK-001.md)
[PR #87 已合并](https://github.com/jnhu76/qianqian/pull/87)

### Realtime publication formal evidence — PR #91

`specs/realtime-publication/` 以 TLC 穷举 + mutation 负控制证明 publication/reclamation collision,其语义协议 P1–P5 冻结为 ADR §6 normative contract(机制仍 DEFERRED,由 §12 Phase D 验证候选机制);同轮完成 tree-wide authority surfaces 对齐。

[PR #91 — realtime publication formal evidence + ADR §6 P1–P5 semantic clarification](https://github.com/jnhu76/qianqian/pull/91)

---

## 权威链

```text
#53 COMPONENT-BOUNDARY-A0        PASS / CLOSED
        ↓
#67 COMPOSITION-KERNEL-0 DESIGN  MERGED via PR #68
        ↓
Corrective-4                     PASS_WITH_ONE_CORRECTIVE
        ↓
Corrective-5                     PRE-IMPLEMENTATION REVIEW
        ↓
#70 COMPOSITION-KERNEL-0 IMPL   MERGED via PR #71
        ↓
#87 PLAYBACK FOUNDATIONS RESET  MERGED (ADR-PBK-001 ACCEPTED)
        ↓
#91 REALTIME PUBLICATION        FORMAL EVIDENCE + ADR §6 P1–P5
```

<ProvenancePanel
  :authority="['docs/adr/ADR-PBK-001.md']"
  :decisions="[{ issue: 46 }, { issue: 53 }, { issue: 67 }, { issue: 70 }, { pr: 66 }, { pr: 68 }, { pr: 69 }, { pr: 71 }, { pr: 87 }, { pr: 91 }]"
  lastVerified="PR #91"
/>
