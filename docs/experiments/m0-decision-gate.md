# M0 Decision Gate

M0 结束时必须回答以下问题。

## A. SongCore 是否值得继续？

必须满足：

- API 足够小；
- FFmpeg 类型没有泄漏；
- MP3/FLAC corpus 稳定；
- seek 可靠；
- decoder errors 可解释；
- benchmark 可重复。

## B. FFmpeg 极限裁剪是否成功？

必须给出：

- enable manifest；
- 每个 component 的 necessity evidence；
- binary size baseline；
- 与普通/完整 FFmpeg 的 size delta。

## C. 音频质量是否可证？

必须给出：

- lossless decode evidence；
- bypass transparency；
- resample tests；
- DSP flat response。

## D. WASM 是否继续？

WASM 至少满足：

- 同一 SongCore contract；
- 同一 corpus；
- realtime 余量足够；
- memory overhead 可接受；
- integration complexity 可接受。

若不满足：

```text
WASM remains experimental
```

不会阻塞播放器。

## E. 是否进入 Qianqian Modern Phase 1？

只有当 Native SongCore 达到稳定 baseline 后进入。

Phase 1 才开始：

- Compose；
- Media Library；
- PlayerStore；
- Playlist；
- LRC；
- Spectrum UI；
- EQ UI。
