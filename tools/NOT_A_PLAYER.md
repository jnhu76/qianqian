# Not a player

`qn_pcm_dump` + `play_smoke.py` is an acceptance rig, not a product playback architecture.

It exists to prove one boundary only:

```text
trimmed FFmpeg slice -> SongCore -> real Float32 PCM -> audible device
```

Do not grow it into playlist state, pause/resume UI, volume policy, device management, threads, buffering policy, or cross-platform AudioSink product code. Those belong to the next product-facing phase after the core build/decode boundary is proven.
