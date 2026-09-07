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
```

<ProvenancePanel
  :authority="['docs/architecture/overview.md', 'docs/architecture/composition-kernel.md']"
  :decisions="[{ issue: 46 }, { issue: 53 }, { issue: 67 }, { issue: 70 }]"
  :pr="[{ pr: 66 }, { pr: 68 }, { pr: 69 }, { pr: 71 }]"
  lastVerified="743eb86"
/>
