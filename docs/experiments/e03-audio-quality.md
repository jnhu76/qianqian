# E03 — Audio Quality / Transparency

## 目标

把“音质”转化为可测试的工程属性。

## Q1：Lossless decode 是否正确？

测试：

```text
FLAC / ALAC / WAV
```

比较：

```text
reference decode
vs
SongCore decode
```

检查：

- sample rate；
- channels；
- frame count；
- PCM hash（可严格比较时）；
- float tolerance（需要格式转换时）。

## Q2：无 DSP 时是否透明？

当：

```text
DSP disabled
```

路径必须是：

```text
decode
→ only necessary format adaptation
→ sink
```

如果 source 与 sink 格式一致：

```text
不得 resample
不得 gain
不得 EQ
不得 limiter
```

## Q3：Flat DSP 是否真正 bypass？

不是：

```text
EQ enabled but all bands = 0 dB
```

而是：

```text
DSP node bypassed
```

目标：

```text
input PCM == output PCM
```

## Q4：必要 resample 的质量

测试：

- 44.1k → 48k；
- 48k → 44.1k；
- 96k → 48k。

信号：

- sine sweep；
- impulse；
- multi-tone；
- near-Nyquist tone。

检查：

- frequency response；
- aliasing；
- clipping；
- latency；
- CPU。

## Q5：整数输出的量化

若 Float32 最终转换到 S16/S24：

比较：

- no dither；
- FFmpeg/libswresample dither（如采用）。

仅在真实 AudioSink 需要整数输出时保留该路径。
