# Qianqian product

> Purpose: 产品的 stable truth——Qianqian 是什么、坚持什么原则、边界在哪里。
> Scope: 产品语义与产品边界。native implementation 细节不在本文
> （见 [docs/architecture/](../architecture/)）。

## What Qianqian is

「千千·现代」（Qianqian Modern）是一款简洁、轻量、纯粹的跨平台**本地音乐
播放器**。

- Local-first：只服务用户已经拥有的本地歌曲文件，不做网络服务。
- Playback-only：核心关注点是正确、快速、稳定地播放，不做转码、编辑、
  通用媒体处理。
- Explicit DSP：默认透明路径；任何处理必须被显式启用。
- Extreme subtraction：能力默认不存在，只有真实歌曲播放需求才能将它加入。

目标平台：Windows x86_64 / arm64 优先；macOS / Linux / Android / iOS
随后验证。

## Why the audio core comes first

在 UI、媒体库、歌词、皮肤、播放列表之前，项目必须先回答一个更基础的问题：

> 跨平台本地歌曲播放，真正需要多大的音频核心？

如果这一层没有稳定边界，播放器会被实现细节侵入：FFmpeg API、JNI、
Kotlin/Native cinterop、AVFoundation、Media3、JavaSound、WASM runtime、
resampler、codec-specific behavior。因此产品建立在稳定的核心边界之上：
上层只依赖自有 contract，永不依赖 FFmpeg 或平台解码 API。

## Product boundaries

Qianqian 不是：

- 通用媒体框架；
- 通用音频处理库；
- FFmpeg binding / FFmpeg CLI wrapper；
- 转码工具、编辑器、视频播放器。

它只服务于一个场景：

> 用户已经拥有一个本地歌曲文件，我们需要正确、快速、稳定地理解它并将其
> 解码为可播放 PCM。

能力负清单（默认不存在的能力）由
[architecture/negative-capability-manifest.md](../architecture/negative-capability-manifest.md)
拥有。

## Audio quality principle

用户没有启用任何音效时，路径必须透明：

```text
Song → Decode → PCM → 必要格式适配 → AudioSink
```

不得偷偷经过 EQ、ReplayGain、limiter、crossfade、reverb、volume
normalization：

> **No processing unless necessary.**

## Product philosophy

音频层：

> **Decode faithfully. Process explicitly. Output minimally.**

中文：忠实解码，显式处理，最短输出路径。

整个项目：

> 没有真实播放需求证明其必要性的能力，不进入核心。

## Related

- 当前播放产品范围：[player-mvp.md](player-mvp.md)
- 架构权威：[docs/architecture/](../architecture/)
- 契约权威：[docs/contracts/](../contracts/)
