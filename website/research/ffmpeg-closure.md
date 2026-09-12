---
title: FFmpeg 闭包研究
status: HISTORICAL_EVIDENCE
---

# FFmpeg 闭包研究

<StatusBadge status="HISTORICAL_EVIDENCE" />

## Qianqian 从 FFmpeg 最小化实验中学到了什么工程方法?

---

## 来源

<ClaimBadge role="evidence" />

playback-reference-v1 实验期间建立的 FFmpeg 构建 profiles 与能力。相关行为事实记录于 component-boundary-a0.md（历史证据）。

| 来源 | 类型 |
|------|------|
| `native/ffmpeg/profiles/*.json` | 机器制品(`playback-reference-v1` tag 上) |
| `native/ffmpeg/capabilities/*.json` | 机器制品(`playback-reference-v1` tag 上) |
| `tools/ffmpeg_import.py` + Xmake 重放 | Qianqian 自有的导入/重放工具(tag 上) |
| `bench/results/common-formats/ladder.md` | 后续能力阶梯延续(tag 上) |
| `component-boundary-a0.md §B.2` | 历史证据（#48 行为事实仍有效） |
| Issue #2 / #3 / #48 / #49 | 实验记录与边界审计 |

---

## 经久的方法

<ClaimBadge role="evidence" />

实验建立了一种可重复、升级安全的方式,让依赖保持封闭且最小:

```text
能力意图
      ↓
上游 configure / Make 用作导入 / 升级 oracle
      ↓
机器推导的源码 + 语义旗标闭包
      ↓
manifest
      ↓
Qianqian 自有的正常构建重放
      ↓
行为 / PCM / seek / 符号 / 尺寸门槛
```

核心思想:

> 上游构建系统是一个 **oracle**,不必是产品的正常构建系统。

在参考实现中,Qianqian 的正常构建**不会**重跑 FFmpeg Makefile(`xmake ffmpeg-import` 运行一次 configure/Make,以捕获真实的 `V=1` 编译器调用;manifest 随后喂给 Qianqian 自有的 Xmake 重放,产出 `libqianqian_av.a`)。升级不变量是:提升 pin → 重跑 importer → diff 闭包 → 重建 → 重跑语料/PCM/尺寸/符号门槛。不维护长期存在的"删除源码"式 FFmpeg fork。

---

## Qianqian 借鉴了什么

<ClaimBadge role="interpretation" />

- **唯一闭包权威** — Decoder 与 Processing 共享单一 FFmpeg 构建,不各自持有一份（#53 历史边界约束；Decoder provider granularity 当前仍 OPEN，见 ADR-PBK-001 §10）
- **configure 即 oracle** — 机器推导闭包,不手改构建文件
- **manifest 重放** — 闭包组成被记录且可复现
- **闭包最小化** — 只构建需要的组件
- **机器推导的依赖知识** — 闭包随每次升级重算,永不手工维护

---

## Qianqian 不借鉴什么

<ClaimBadge role="interpretation" />

- **按 codec 拆分插件** — 已否决:它把闭包权威成倍拆开,却零可组合性收益
- **运行时 codec 检测** — 编解码覆盖是提供者配置,不是运行时层
- **完整 FFmpeg 特性集** — 只需要已声明的编解码集
- **FFmpeg 类型跨缝** — ABI 纪律是契约的一部分(SongCore ABI v1 已证明)

---

## Qianqian 改变了什么

<ClaimBadge role="evidence" />

链接完整 FFmpeg 库的传统做法被替换为**能力驱动的最小闭包**。组件边界审计(#53)将其冻结为架构约束:

> 当 FFmpeg 重新引入时,Decoder/Processing 必须继续共享唯一的 FFmpeg 闭包权威,而不是复制依赖。

---

## 仍然有效的证据约束

这些是 #48 实验挣得的**行为证据约束** —— 即使当前 main 上不存在任何 FFmpeg 代码,它们仍作为可复用证据存续于 `docs/architecture/component-boundary-a0.md` §B.2（历史文档;未来 Decoder 设计须重新引用并挣得其 normative 形式）:

```text
唯一闭包权威(Decoder + 未来 Processing 共享)
机器推导的依赖知识
FFmpeg 类型不跨组件缝
按 codec 的运行时插件拆分已否决
```

## 当前 main 中尚不存在的部分

<ClaimBadge role="evidence" />

这些**缺席于今天的 Rust main** —— 不要把历史证据读作当前实现:

```text
当前 Rust Decoder 实现
当前 Rust FFmpeg 构建 / 重放集成
已发布的 FFmpeg 制品
```

已于 2026-09-07 核验:当前 main 不含 FFmpeg crate、不含 FFmpeg 构建配方、不含解码器实现;仅有的出现是一处文档注释(`crates/qianqian-app/src/lib.rs:8`)和一处能力命名测试字符串(`crates/qianqian-composition/tests/adversarial_review.rs:372`)。

---

## 开放问题

- 首个真正的 Decoder 实现所需的精确 FFmpeg configure 旗标集是什么?
- 闭包能否在 CI 中自动验证?
- 共享闭包在未来的插件/组合系统中应如何表达?

---

<ProvenancePanel
  :authority="[]"
  :evidence="['docs/architecture/component-boundary-a0.md §B.2', 'research/playback-reference-v1']"
  :decisions="[{ issue: 2 }, { issue: 3 }, { issue: 48 }, { issue: 53 }]"
  last-verified="issue #2/#3/#48/#49 + playback-reference-v1 tag + current main scan, 2026-09-07"
/>
