---
layout: home
hero:
  name: "Qianqian 工程观测站"
  text: "工程控制台"
  tagline: 一个轻量级音乐播放器，也是一座系统架构实验室。
  actions:
    - theme: brand
      text: 项目控制
      link: /control/
    - theme: alt
      text: 架构
      link: /architecture/
    - theme: alt
      text: 实验
      link: /experiments/
    - theme: alt
      text: 研究
      link: /research/

features:
  - title: "Composition Kernel K0"
    details: "Context、Capability、Fiber、Effect、Reconcile 五个原语已实现。它只拥有 composition truth，不拥有音乐/PCM/playback timeline。"
    status: "IMPLEMENTED"
  - title: "Playback Foundations"
    details: "播放基础已被接受（ADR-PBK-001 ACCEPTED）：平面边界与契约已冻结，publication/reclamation 语义协议 P1–P5 已 normative（ADR §6），Realtime Runtime 责任已由机制证据挣得（Issue #94 closed）；旧 MusicKernel / TransportKernel 等模型仍只是 experimental evidence。production realtime seam 已按收口后的 authority 重审并收缩（PR #98、#101–#103）；下一步：Phase D 机制最终裁决与真实 decoder/output（Phase E）实验。"
    status: "CURRENT"
  - title: "Decoder"
    details: "编码媒体 → Canonical PCM。真实 provider contract/实现仍待 Playback ADR 获得后续实现授权。"
    status: "PLANNED"
  - title: "AudioOutput"
    details: "Canonical PCM → 物理设备 + submitted/rendered/fence evidence。真实 backend 尚未获得本轮实现授权。"
    status: "PLANNED"
---
