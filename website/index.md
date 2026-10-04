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
    details: "PBK-001/002/003 是已接受的播放基础、当前静态组合/最小播放语义与 Output/backend 边界 authority。执行模型 #198/#201 经 Stage 4 #207/#214 和 Stage 5 #208 明确表达；C1/C2 已合并，#209 已 ACCEPT D1–D6，#210 已 FROZEN（绑定 #212 验证契约）。旧 MusicKernel / TransportKernel 等模型是历史证据。"
    status: "CURRENT"
  - title: "Decoder"
    details: "Decode Plugin 提供 SongCore-backed Decoder；真实 decode → PCM 路径已实现。当前 ownership/lifetime 见 PBK-002 D6/D14；目标支持成熟度见 SongCore binding authority。"
    status: "IMPLEMENTED"
  - title: "AudioOutput"
    details: "Output Plugin 提供 backend-neutral AudioOutput，拥有可替换的具体 backend；当前实现含 WASAPI mechanism。边界见 PBK-003，存在实现不等于本次已验证设备运行。"
    status: "IMPLEMENTED"
---
