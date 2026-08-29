# E02 — Native FFmpeg vs WASM FFmpeg

## 前置条件

只有 E01 Native baseline 稳定后开始。

两条 backend 必须实现同一 SongCore contract。

## 对比对象

```text
A. Native trimmed FFmpeg
B. WASM trimmed FFmpeg
```

二者使用：

- 同一 codec allow-list；
- 同一 corpus；
- 同一 host benchmark；
- 同一 PCM output contract。

## 指标

| Metric | Native | WASM |
|---|---:|---:|
| runtime/package size | TODO | TODO |
| codec artifact size | TODO | TODO |
| cold open | TODO | TODO |
| MP3 decode x realtime | TODO | TODO |
| FLAC decode x realtime | TODO | TODO |
| seek p50 | TODO | TODO |
| seek p95 | TODO | TODO |
| RSS | TODO | TODO |
| peak memory | TODO | TODO |
| PCM copy overhead | TODO | TODO |
| CPU | TODO | TODO |
| integration LOC | TODO | TODO |
| crash isolation | assess | assess |

## Decision Questions

1. WASM 是否显著简化跨平台分发？
2. runtime 本身是否抵消 codec 裁剪收益？
3. 是否需要额外内存复制？
4. 自定义 IO 是否自然？
5. 移动端 runtime 是否增加明显维护负担？
6. sandbox 是否提供真实安全价值？
7. Web 复用是否值得保留 WASM backend？

## 默认先验

Production 默认 Native。

只有实验数据证明 WASM 有足够收益时，才提升其地位。
