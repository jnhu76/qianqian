---
layout: home
hero:
  name: "Qianqian 工程观测站"
  text: "工程控制台"
  tagline: 一个轻量级音乐播放器,也是一座系统架构实验室。
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
    details: "五个原语 —— Context、Capability、Fiber、Effect、Reconcile —— 70 项内核 oracle 测试(75 项 workspace 测试)全部通过。与领域无关的控制平面。"
    status: "IMPLEMENTED"
  - title: "Playback Kernel"
    details: "音乐领域语义权威:曲目/会话/状态、播放/暂停/停止/seek、队列、缓冲/恢复、ENDED/时间线。"
    status: "NEXT"
  - title: "Decoder"
    details: "编码媒体 → 规范化 PCM。唯一共享的 FFmpeg 闭包权威。"
    status: "PLANNED"
  - title: "AudioOutput"
    details: "规范化 PCM → 物理设备。预绑定的实时数据边。"
    status: "PLANNED"
---
