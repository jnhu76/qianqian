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
    details: "播放架构已从第一性原理重开（REOPENED）：旧 MusicKernel / TransportKernel 等模型只是 experimental evidence。新基础提案 ADR-PBK-001 为 PROPOSED，等待 fresh-context review 与 acceptance gates。"
    status: "NEXT"
  - title: "Decoder"
    details: "编码媒体 → Canonical PCM。真实 provider contract/实现仍待 Playback ADR 获得后续实现授权。"
    status: "PLANNED"
  - title: "AudioOutput"
    details: "Canonical PCM → 物理设备 + submitted/rendered/fence evidence。真实 backend 尚未获得本轮实现授权。"
    status: "PLANNED"
---
