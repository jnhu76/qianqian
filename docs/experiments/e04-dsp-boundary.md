# E04 — DSP Boundary

## 目标

证明音效可以独立于 FFmpeg，并且不破坏“极致减法”。

## 第一批 DSP

只考虑：

```text
Gain
ReplayGain
10-band EQ
Balance
Spectrum tap
```

## Pipeline

```text
SongCore
  ↓ Float32 PCM
ReplayGain (optional)
  ↓
EQ (optional)
  ↓
Balance (optional)
  ↓
Peak guard (optional)
  ↓
Spectrum tap (read-only)
  ↓
AudioSink
```

## 规则

1. DSP 全部可旁路。
2. Spectrum 不修改 PCM。
3. 不使用 libavfilter。
4. 新音效默认不进入 core。
5. EQ 实现需有 frequency-response golden tests。

## 10-band EQ 候选中心频率

```text
31
62
125
250
500
1000
2000
4000
8000
16000 Hz
```

## 实验

### Flat response

所有 band 0 dB：

```text
frequency response ≈ 0 dB
```

### Single-band boost

1kHz +6 dB：

验证中心附近响应与预期一致。

### Stability

极端 preset：

```text
all bands +12 dB
all bands -12 dB
alternating +12/-12
```

检查：

- NaN；
- Inf；
- clipping；
- instability；
- denormal performance。
