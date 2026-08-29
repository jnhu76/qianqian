# Roadmap

```text
M0-A
Thin Native SongCore
│
├── MP3
├── FLAC
├── custom IO
├── PCM
├── seek
└── corpus

      ↓

M0-B
Extreme FFmpeg Reduction
│
├── component provenance
├── binary size
├── startup
└── decode benchmark

      ↓

M0-C
Audio Quality
│
├── lossless reference
├── bypass transparency
├── resample
└── DSP boundary

      ↓

M0-D
WASM Backend
│
├── same API
├── same corpus
└── native-vs-wasm

      ↓
      ├────────────────────┐
      ▼                    ▼
Audio research       Qianqian Modern
WASM/SIMD/etc.       actual player
```

播放器不等待 WASM 成熟。

只要 Native SongCore 稳定，Phase 1 就可以开始。
